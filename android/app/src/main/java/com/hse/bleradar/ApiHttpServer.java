package com.hse.bleradar;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;
import java.util.function.LongSupplier;
import java.util.logging.Level;
import java.util.logging.Logger;

/**
 * Loopback-only HTTP server exposing the live device snapshot as JSON and
 * the web dashboard that renders it, for a browser or Termux tooling running
 * on the same device.
 *
 * <p>Android ships no HTTP server class ({@code com.sun.net.httpserver} is a
 * JDK-only package absent from {@code android.jar}), so this is a deliberately
 * minimal HTTP/1.1 responder over {@link ServerSocket}: every request is
 * answered with a complete {@code Content-Length}-framed body and the
 * connection is closed.
 *
 * <p>The class references nothing in {@code android.*}: its collaborators
 * arrive as {@link SnapshotSource}, {@link UpdateStatusSource},
 * {@link AssetSource} and two clocks, JSON is written by {@link Json}, and
 * logging goes through {@code java.util.logging} (which Android routes to
 * logcat). That is what lets {@code cargo xtask verify-api-live} run this
 * exact class on a host JVM with fixture sources, compare its documents byte
 * for byte with the fixtures the browser proofs use, and render the real
 * dashboard from it in headless Chromium. {@link RadarScanService} wires the
 * device implementations in.
 *
 * <p>Endpoints (the documents are {@code GET}, scan control is {@code POST};
 * JSON unless noted):
 * <ul>
 *   <li>{@code /api/devices} — every live device from
 *       {@link SnapshotSource#snapshot()}, in its ranked order: {@code address},
 *       {@code name} ({@code null} when the advertiser has none),
 *       {@code distance_m} / {@code distance_lower_m} /
 *       {@code distance_upper_m} (metres, {@code null} when unavailable),
 *       {@code rssi_dbm} ({@code null} when unavailable), {@code proximity}
 *       ({@code IMMEDIATE|NEAR|MID|FAR|UNKNOWN}), {@code trend}
 *       ({@code STRONGER|WEAKER|STABLE|UNKNOWN}), {@code freshness}
 *       ({@code LIVE|RECENT|STALE|UNKNOWN}), {@code confidence_percent},
 *       {@code last_seen_ago_ms}; plus the top-level {@code scanning},
 *       {@code native_available}, {@code timestamp_ms}.</li>
 *   <li>{@code /api/status} — {@code scanning}, {@code device_count},
 *       {@code native_available}, {@code uptime_ms}.</li>
 *   <li>{@code /api/updates} — {@code last_check_ms}, {@code next_check_ms},
 *       {@code retry_count}.</li>
 *   <li>{@code /} — the web dashboard ({@code text/html}): the packaged
 *       {@code assets/dashboard.html}, a self-contained page that polls the
 *       three JSON endpoints and renders the radar and device table. Java
 *       serves its bytes unchanged.</li>
 *   <li>{@code POST /api/scan/start} and {@code POST /api/scan/stop} — scan
 *       control through {@link ScanControl}: {@code 200} with
 *       {@code {"scanning": <live state>}}, or {@code 409} with an
 *       {@code error} naming why a start was refused (permissions not
 *       granted, Bluetooth off, scanner unavailable, background start
 *       refused by Android). A stop pauses the scan and keeps the service
 *       (and this API) alive.</li>
 * </ul>
 * Unknown paths answer {@code 404}; a known path with the wrong method
 * answers {@code 405}; a malformed request line answers {@code 400};
 * {@code /} answers {@code 500} if the packaged page could not be read, and
 * any route answers {@code 500} (the exception's class in {@code error}) if
 * a collaborator throws, so no request can end the app process. A request
 * body is consumed and discarded (no route reads one), so a client that
 * sent one is answered rather than reset; a body declared larger than
 * {@link #MAX_REQUEST_BODY_BYTES} answers {@code 413} at once instead of
 * tying a handler thread to a client that may never send it.
 *
 * <p>Binding to {@code 127.0.0.1} is the whole access-control model: nothing
 * off-device can reach the port, and remote use goes through an SSH tunnel.
 */
public final class ApiHttpServer {

    /** The loopback port the service binds; the host harness passes 0 for an ephemeral one. */
    public static final int DEFAULT_PORT = 8080;
    /** The dashboard page, packaged by {@code cargo xtask build-apk} from {@code src/main/assets/}. */
    static final String DASHBOARD_ASSET = "dashboard.html";
    private static final Logger LOG = Logger.getLogger("ApiHttpServer");
    private static final int BACKLOG = 8;
    private static final int HANDLER_THREADS = 2;
    /** Frees a handler thread from a client that connects but never sends its request. */
    private static final int REQUEST_TIMEOUT_MS = 5_000;
    /**
     * The largest declared request body a handler will consume; no route
     * reads one, so anything larger is refused ({@code 413}) before the
     * handler thread would wait on it.
     */
    static final long MAX_REQUEST_BODY_BYTES = 64 * 1024;
    private static final String JSON = "application/json";
    private static final String HTML = "text/html; charset=utf-8";

    private final SnapshotSource engine;
    private final UpdateStatusSource updates;
    private final AssetSource assets;
    private final ScanControl control;
    /** The {@code SystemClock.uptimeMillis()} timeline {@link Blip#lastSeenUptimeMillis} lives on. */
    private final LongSupplier uptimeMillis;
    /** Wall-clock epoch milliseconds for {@code timestamp_ms}. */
    private final LongSupplier epochMillis;
    private final int port;
    /** The dashboard bytes, read once per {@link #start()}; {@code null} when the asset is unreadable. */
    private volatile byte[] dashboard;
    private ServerSocket listener;
    private ExecutorService handlers;

    public ApiHttpServer(
            SnapshotSource engine,
            UpdateStatusSource updates,
            AssetSource assets,
            LongSupplier uptimeMillis,
            LongSupplier epochMillis,
            ScanControl control,
            int port) {
        this.engine = engine;
        this.updates = updates;
        this.assets = assets;
        this.uptimeMillis = uptimeMillis;
        this.epochMillis = epochMillis;
        this.control = control;
        this.port = port;
    }

    /**
     * Binds the loopback port and starts accepting connections on a daemon
     * thread. Idempotent; a bind failure is logged and leaves the server
     * stopped rather than throwing into the service lifecycle.
     */
    public synchronized void start() {
        if (listener != null) {
            return;
        }
        dashboard = loadDashboard();
        ServerSocket socket;
        try {
            socket = new ServerSocket();
            socket.setReuseAddress(true);
            socket.bind(new InetSocketAddress(InetAddress.getLoopbackAddress(), port), BACKLOG);
        } catch (IOException error) {
            LOG.log(Level.SEVERE, "Failed to bind http://127.0.0.1:" + port, error);
            return;
        }
        ExecutorService pool = Executors.newFixedThreadPool(HANDLER_THREADS);
        listener = socket;
        handlers = pool;
        Thread acceptThread = new Thread(() -> acceptLoop(socket, pool), "ApiHttpServer-accept");
        acceptThread.setDaemon(true);
        acceptThread.start();
        LOG.info("HTTP server listening on http://127.0.0.1:" + socket.getLocalPort());
    }

    /** The port the listener is bound to, or {@code -1} while stopped (what a port-0 start received). */
    public synchronized int boundPort() {
        return listener == null ? -1 : listener.getLocalPort();
    }

    /** Closes the listener (which ends the accept loop) and the handler threads. Idempotent. */
    public synchronized void stop() {
        if (listener == null) {
            return;
        }
        try {
            listener.close();
        } catch (IOException ignored) {
            // The port is released either way; nothing further to do.
        }
        listener = null;
        handlers.shutdownNow();
        handlers = null;
        LOG.info("HTTP server stopped");
    }

    /** Reads the packaged page; a missing or unreadable asset is logged and makes {@code /} answer 500. */
    private byte[] loadDashboard() {
        try (InputStream in = assets.open(DASHBOARD_ASSET)) {
            return Streams.readAllBytes(in);
        } catch (IOException error) {
            LOG.log(Level.SEVERE, "Dashboard asset " + DASHBOARD_ASSET + " unreadable; / will answer 500", error);
            return null;
        }
    }

    private void acceptLoop(ServerSocket socket, ExecutorService pool) {
        while (!socket.isClosed()) {
            Socket connection;
            try {
                connection = socket.accept();
            } catch (IOException closed) {
                break;
            }
            try {
                pool.execute(() -> handle(connection));
            } catch (RejectedExecutionException stopping) {
                closeQuietly(connection);
            }
        }
    }

    private void handle(Socket connection) {
        try (Socket socket = connection) {
            socket.setSoTimeout(REQUEST_TIMEOUT_MS);
            BufferedReader reader = new BufferedReader(
                    new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
            String requestLine = reader.readLine();
            if (requestLine == null) {
                return;
            }
            // No route reads a body: drain the headers to reach the end of the
            // request, then consume and discard a declared body so a client
            // that sent one is answered rather than reset.
            long bodyLength = 0;
            for (String header = reader.readLine(); header != null && !header.isEmpty(); header = reader.readLine()) {
                int colon = header.indexOf(':');
                if (colon > 0 && "content-length".equalsIgnoreCase(header.substring(0, colon).trim())) {
                    try {
                        bodyLength = Long.parseLong(header.substring(colon + 1).trim());
                    } catch (NumberFormatException malformed) {
                        bodyLength = 0;
                    }
                }
            }
            OutputStream out = socket.getOutputStream();
            if (bodyLength > MAX_REQUEST_BODY_BYTES) {
                writeResponse(out, 413, "Payload Too Large", JSON, errorJson("Request body too large"));
                return;
            }
            for (long skipped = 0; skipped < bodyLength; skipped++) {
                if (reader.read() < 0) {
                    break;
                }
            }
            String[] parts = requestLine.split(" ");
            if (parts.length != 3) {
                writeResponse(out, 400, "Bad Request", JSON, errorJson("Bad request"));
                return;
            }
            String path = parts[1];
            int query = path.indexOf('?');
            if (query >= 0) {
                path = path.substring(0, query);
            }
            try {
                respond(out, parts[0], path);
            } catch (RuntimeException error) {
                // A collaborator failed (scan control, the snapshot). On Android
                // an uncaught exception on any thread ends the whole process, so
                // a request must never take the app down: log it and answer 500.
                // Every route does its work before writing its response in one
                // call, so nothing has reached the client yet.
                LOG.log(Level.SEVERE, "HTTP " + parts[0] + " " + path + " failed", error);
                writeResponse(out, 500, "Internal Server Error", JSON,
                        errorJson("Internal error: " + error.getClass().getSimpleName()));
            }
        } catch (IOException error) {
            LOG.warning("HTTP request failed: " + error);
        }
    }

    private void respond(OutputStream out, String method, String path) throws IOException {
        if ("/api/scan/start".equals(path) || "/api/scan/stop".equals(path)) {
            if (!"POST".equals(method)) {
                writeResponse(out, 405, "Method Not Allowed", JSON, errorJson("Method not allowed"));
                return;
            }
            respondScan(out, path.endsWith("/start"));
            return;
        }
        boolean routed = "/api/devices".equals(path)
                || "/api/status".equals(path)
                || "/api/updates".equals(path)
                || "/".equals(path);
        if (!routed) {
            writeResponse(out, 404, "Not Found", JSON, errorJson("Not found"));
            return;
        }
        if (!"GET".equals(method)) {
            writeResponse(out, 405, "Method Not Allowed", JSON, errorJson("Method not allowed"));
            return;
        }
        switch (path) {
            case "/api/devices":
                writeResponse(out, 200, "OK", JSON, devicesJson());
                break;
            case "/api/status":
                writeResponse(out, 200, "OK", JSON, statusJson());
                break;
            case "/api/updates":
                writeResponse(out, 200, "OK", JSON, updatesJson());
                break;
            default:
                byte[] page = dashboard;
                if (page == null) {
                    writeResponse(out, 500, "Internal Server Error", JSON,
                            errorJson("Dashboard asset " + DASHBOARD_ASSET + " unreadable"));
                } else {
                    writeResponse(out, 200, "OK", HTML, page);
                }
                break;
        }
    }

    private void respondScan(OutputStream out, boolean start) throws IOException {
        if (start) {
            int outcome = control.requestStart();
            if (outcome != ScanControl.START_ACCEPTED) {
                writeResponse(out, 409, "Conflict", JSON, errorJson(startRefusalLabel(outcome)));
                return;
            }
        } else {
            control.requestStop();
        }
        writeResponse(out, 200, "OK", JSON, scanJson());
    }

    private String scanJson() {
        return new Json().beginObject().name("scanning").value(engine.isScanning()).endObject().toString();
    }

    private static String startRefusalLabel(int outcome) {
        switch (outcome) {
            case ScanControl.START_PERMISSIONS_MISSING:
                return "Bluetooth permissions not granted; open the app once to grant them";
            case ScanControl.START_BLUETOOTH_OFF:
                return "Bluetooth is off";
            case ScanControl.START_UNAVAILABLE:
                return "Bluetooth LE scanner unavailable";
            case ScanControl.START_BACKGROUND_RESTRICTED:
                return "Android refused a background start; open the app once";
            default:
                return "Scan could not start (outcome " + outcome + ")";
        }
    }

    private String devicesJson() {
        List<Blip> devices = engine.snapshot();
        // Blip.lastSeenUptimeMillis is on the SystemClock.uptimeMillis() timeline,
        // so the age must be measured on that same clock, never wall-clock epoch.
        long nowUptimeMs = uptimeMillis.getAsLong();
        Json json = new Json();
        json.beginObject();
        json.name("devices").beginArray();
        for (Blip device : devices) {
            json.beginObject();
            json.name("address").value(device.address);
            json.name("name").value(device.name);
            json.name("distance_m").value(device.distanceMetres);
            json.name("distance_lower_m").value(device.distanceLowerBoundMetres);
            json.name("distance_upper_m").value(device.distanceUpperBoundMetres);
            json.name("rssi_dbm").value(device.lastRssiDbm);
            json.name("proximity").value(proximityLabel(device.proximity));
            json.name("trend").value(trendLabel(device.trend));
            json.name("freshness").value(freshnessLabel(device.freshness));
            json.name("confidence_percent").value(device.confidencePercent);
            json.name("last_seen_ago_ms").value(Math.max(0L, nowUptimeMs - device.lastSeenUptimeMillis));
            json.endObject();
        }
        json.endArray();
        json.name("scanning").value(engine.isScanning());
        json.name("native_available").value(NativeRadar.isAvailable());
        json.name("timestamp_ms").value(epochMillis.getAsLong());
        json.endObject();
        return json.toString();
    }

    private String statusJson() {
        Json json = new Json();
        json.beginObject();
        json.name("scanning").value(engine.isScanning());
        json.name("device_count").value(engine.snapshot().size());
        json.name("native_available").value(NativeRadar.isAvailable());
        json.name("uptime_ms").value(engine.getUptimeMillis());
        json.endObject();
        return json.toString();
    }

    private String updatesJson() {
        Json json = new Json();
        json.beginObject();
        json.name("last_check_ms").value(updates.getLastCheckTimeMs());
        json.name("next_check_ms").value(updates.getNextCheckTimeMs());
        json.name("retry_count").value(updates.getRetryCount());
        json.endObject();
        return json.toString();
    }

    private static void writeResponse(OutputStream out, int status, String reason, String contentType, String body)
            throws IOException {
        writeResponse(out, status, reason, contentType, body.getBytes(StandardCharsets.UTF_8));
    }

    private static void writeResponse(OutputStream out, int status, String reason, String contentType, byte[] payload)
            throws IOException {
        String head = "HTTP/1.1 " + status + ' ' + reason + "\r\n"
                + "Content-Type: " + contentType + "\r\n"
                + "Content-Length: " + payload.length + "\r\n"
                + "Cache-Control: no-store\r\n"
                + "Connection: close\r\n"
                + "\r\n";
        out.write(head.getBytes(StandardCharsets.US_ASCII));
        out.write(payload);
        out.flush();
    }

    private static String errorJson(String message) {
        return new Json().beginObject().name("error").value(message).endObject().toString();
    }

    private static void closeQuietly(Socket socket) {
        try {
            socket.close();
        } catch (IOException ignored) {
            // Already gone.
        }
    }

    private static String proximityLabel(int proximity) {
        switch (proximity) {
            case NativeRadar.PROXIMITY_IMMEDIATE:
                return "IMMEDIATE";
            case NativeRadar.PROXIMITY_NEAR:
                return "NEAR";
            case NativeRadar.PROXIMITY_MID:
                return "MID";
            case NativeRadar.PROXIMITY_FAR:
                return "FAR";
            default:
                return "UNKNOWN";
        }
    }

    private static String trendLabel(int trend) {
        switch (trend) {
            case NativeRadar.TREND_STRONGER:
                return "STRONGER";
            case NativeRadar.TREND_WEAKER:
                return "WEAKER";
            case NativeRadar.TREND_STABLE:
                return "STABLE";
            default:
                return "UNKNOWN";
        }
    }

    private static String freshnessLabel(int freshness) {
        switch (freshness) {
            case NativeRadar.FRESHNESS_LIVE:
                return "LIVE";
            case NativeRadar.FRESHNESS_RECENT:
                return "RECENT";
            case NativeRadar.FRESHNESS_STALE:
                return "STALE";
            default:
                return "UNKNOWN";
        }
    }
}

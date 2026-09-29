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
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Locale;
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
 *       ({@code IMMEDIATE|NEAR|MID|FAR|UNKNOWN}), {@code trackability}
 *       ({@code TRACKABLE|RANDOMIZED|UNKNOWN}), {@code company_id} (the
 *       advertisement's first manufacturer company identifier as four hex
 *       digits, {@code null} when absent), {@code manufacturer} (its Bluetooth
 *       SIG assignee name, {@code null} when the id is unknown), {@code beacon}
 *       ({@code iBeacon|Eddystone-UID|Eddystone-URL|Eddystone-TLM}, {@code null}
 *       when not a beacon), {@code services} (comma-separated well-known
 *       service names, {@code null} when none), {@code identity_key} (the stable cross-rotation
 *       grouping key, {@code null} when the device cannot be grouped),
 *       {@code first_seen_ms} (wall-clock epoch milliseconds of the first
 *       sighting the persistent device history remembers, across sessions) and
 *       {@code visits} (separate visits it remembers) — both {@code null} when
 *       the device is not remembered (only public addresses are),
 *       {@code trend}
 *       ({@code STRONGER|WEAKER|STABLE|UNKNOWN}), {@code freshness}
 *       ({@code LIVE|RECENT|STALE|UNKNOWN}), {@code confidence_percent},
 *       {@code last_seen_ago_ms}; plus the top-level {@code scanning},
 *       {@code native_available}, {@code timestamp_ms}.</li>
 *   <li>{@code /api/wifi} — the passive Wi-Fi survey from
 *       {@link WifiSurveySource}: {@code access_points} (each with {@code bssid},
 *       {@code ssid}, {@code frequency_mhz}, {@code channel}, {@code rssi_dbm},
 *       {@code reliability}, {@code proximity}, {@code security},
 *       {@code enterprise}, {@code trackability}, {@code last_seen_ms}, and
 *       {@code first_seen_ms} / {@code visits} from the persistent history, both
 *       {@code null} for an access point that is not remembered (only a
 *       trackable BSSID is); every reading rule decided by the Rust core), {@code state} (why the list may be
 *       empty), {@code dropped}, {@code native_available}, {@code timestamp_ms}.</li>
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
 * <p>Access control is two things and no more. Binding to {@code 127.0.0.1}
 * keeps the network out (remote use goes through an SSH tunnel). Every route
 * then answers only a request whose {@code Host} is this loopback server
 * ({@link #hostAllowed}) and whose {@code Origin}, if it has one, is the same
 * ({@link #sameOrigin}) with {@code 403}, which is what keeps a web page in the
 * user's browser out: DNS rebinding and a cross-site POST both fail. A request
 * line or header longer than {@link #MAX_HEADER_LINE_CHARS}, more than
 * {@link #MAX_HEADERS} headers, or a request that takes more than ten seconds
 * to arrive is refused ({@code 431}, {@code 408}). What is <em>not</em> stopped
 * is another app installed on the same device: any app with the {@code INTERNET}
 * permission can connect to loopback and read every route, including the Wi-Fi
 * survey's SSIDs and BSSIDs. That is the design (the dashboard is for a browser
 * or Termux on the same device, which cannot be told apart from another app),
 * and it is stated in {@code docs/ANDROID_APP.md}.
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
    /** The longest request or header line read; a longer one is refused ({@code 431}) rather than buffered. */
    static final int MAX_HEADER_LINE_CHARS = 8 * 1024;
    /** The most header lines read; more is refused ({@code 431}). */
    static final int MAX_HEADERS = 64;
    /**
     * The most time a client gets to send its request line and headers. The socket
     * timeout is per read, so on its own a client that sends one byte every few
     * seconds could hold a handler thread (there are two) indefinitely.
     */
    private static final long REQUEST_DEADLINE_NANOS = 10_000_000_000L;
    private static final String JSON = "application/json";
    private static final String HTML = "text/html; charset=utf-8";

    private final SnapshotSource engine;
    private final WifiSurveySource wifi;
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
            WifiSurveySource wifi,
            UpdateStatusSource updates,
            AssetSource assets,
            LongSupplier uptimeMillis,
            LongSupplier epochMillis,
            ScanControl control,
            int port) {
        this.engine = engine;
        this.wifi = wifi;
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

    /** A request refused before any route ran, with the status it is answered with. */
    private static final class Refusal extends IOException {
        final int status;
        final String reason;
        final String message;

        Refusal(int status, String reason, String message) {
            super(message);
            this.status = status;
            this.reason = reason;
            this.message = message;
        }
    }

    /**
     * One line, at most {@link #MAX_HEADER_LINE_CHARS} long ({@code \r\n} or
     * {@code \n} ended), or {@code null} at the end of the stream. Unlike
     * {@code BufferedReader.readLine} it never buffers more than the cap, so a
     * client cannot make a handler allocate without limit.
     */
    private static String readBoundedLine(BufferedReader reader, long deadlineNanos) throws IOException {
        StringBuilder line = new StringBuilder();
        for (;;) {
            if (System.nanoTime() > deadlineNanos) {
                throw new Refusal(408, "Request Timeout", "Request took too long");
            }
            int c = reader.read();
            // The read can block for up to the socket timeout, so the deadline is
            // checked again once it returns: a byte (the terminating newline
            // especially) that arrives after the deadline is not accepted.
            if (System.nanoTime() > deadlineNanos) {
                throw new Refusal(408, "Request Timeout", "Request took too long");
            }
            if (c < 0) {
                return line.length() == 0 ? null : line.toString();
            }
            if (c == '\n') {
                int end = line.length();
                if (end > 0 && line.charAt(end - 1) == '\r') {
                    line.setLength(end - 1);
                }
                return line.toString();
            }
            if (line.length() >= MAX_HEADER_LINE_CHARS) {
                throw new Refusal(431, "Request Header Fields Too Large", "Request header too large");
            }
            line.append((char) c);
        }
    }

    /**
     * Whether a {@code Host} header names this loopback server: {@code 127.0.0.1},
     * {@code localhost} or {@code [::1]}, each with an optional numeric port. A page
     * whose name an attacker points at 127.0.0.1 (DNS rebinding) is served as
     * its own name, not one of these, so it is refused; the port is not compared
     * because a forwarded or tunnelled port legitimately differs from the bound one.
     */
    static boolean hostAllowed(String hostHeader) {
        if (hostHeader == null) {
            return false;
        }
        String host = hostHeader.trim().toLowerCase(Locale.ROOT);
        String name;
        String port;
        if (host.startsWith("[")) {
            int close = host.indexOf(']');
            if (close < 0) {
                return false;
            }
            name = host.substring(0, close + 1);
            port = host.substring(close + 1);
        } else {
            int colon = host.indexOf(':');
            name = colon < 0 ? host : host.substring(0, colon);
            port = colon < 0 ? "" : host.substring(colon);
        }
        if (!port.isEmpty() && !port.matches(":[0-9]{1,5}")) {
            return false;
        }
        return name.equals("127.0.0.1") || name.equals("localhost") || name.equals("[::1]");
    }

    /**
     * Whether an {@code Origin} header is this server's own: the page it came from
     * was served by the same {@code Host}. A browser sends {@code Origin} on a
     * cross-site request (and on a same-site POST), so a page on another site,
     * or a sandboxed one ({@code null}), is refused, while the dashboard's own
     * Start and Stop buttons, whose origin is {@code http://<Host>}, are not.
     */
    static boolean sameOrigin(String originHeader, String hostHeader) {
        return originHeader != null
                && hostHeader != null
                && originHeader.trim().equalsIgnoreCase("http://" + hostHeader.trim());
    }

    private void handle(Socket connection) {
        try (Socket socket = connection) {
            socket.setSoTimeout(REQUEST_TIMEOUT_MS);
            BufferedReader reader = new BufferedReader(
                    new InputStreamReader(socket.getInputStream(), StandardCharsets.US_ASCII));
            long deadline = System.nanoTime() + REQUEST_DEADLINE_NANOS;
            String requestLine;
            long bodyLength = 0;
            String host = null;
            String origin = null;
            try {
                requestLine = readBoundedLine(reader, deadline);
                if (requestLine == null) {
                    return;
                }
                // No route reads a body: drain the headers to reach the end of the
                // request, then consume and discard a declared body so a client
                // that sent one is answered rather than reset.
                int headers = 0;
                for (String header = readBoundedLine(reader, deadline);
                        header != null && !header.isEmpty();
                        header = readBoundedLine(reader, deadline)) {
                    if (++headers > MAX_HEADERS) {
                        throw new Refusal(431, "Request Header Fields Too Large", "Request header too large");
                    }
                    int colon = header.indexOf(':');
                    if (colon <= 0) {
                        continue;
                    }
                    String name = header.substring(0, colon).trim();
                    String value = header.substring(colon + 1).trim();
                    if ("content-length".equalsIgnoreCase(name)) {
                        try {
                            bodyLength = Long.parseLong(value);
                        } catch (NumberFormatException malformed) {
                            bodyLength = 0;
                        }
                    } else if ("host".equalsIgnoreCase(name)) {
                        host = value;
                    } else if ("origin".equalsIgnoreCase(name)) {
                        origin = value;
                    }
                }
            } catch (Refusal refused) {
                writeResponse(socket.getOutputStream(), refused.status, refused.reason, JSON,
                        errorJson(refused.message));
                return;
            } catch (SocketTimeoutException stalled) {
                // A client that sent nothing (or stopped) for the socket timeout is
                // answered like one that took too long overall, not just dropped.
                writeResponse(socket.getOutputStream(), 408, "Request Timeout", JSON,
                        errorJson("Request took too long"));
                return;
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
            // Every route, the page included, answers only a request that names this
            // server as its Host and, when it carries an Origin, is same-origin.
            // The socket is bound to loopback, which keeps the network out but not a
            // web page in the user's browser: DNS rebinding makes such a page reach
            // 127.0.0.1 under its own name, and a cross-site form can POST here.
            if (!hostAllowed(host) || (origin != null && !sameOrigin(origin, host))) {
                writeResponse(out, 403, "Forbidden", JSON, errorJson("Forbidden"));
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
                || "/api/wifi".equals(path)
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
            case "/api/wifi":
                writeResponse(out, 200, "OK", JSON, wifiJson());
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
            json.name("trackability").value(trackabilityLabel(device.trackability));
            Blip.AdvSummary advertisement = device.advertisement;
            json.name("company_id").value(advertisement == null ? null : advertisement.companyId);
            json.name("manufacturer").value(advertisement == null ? null : advertisement.manufacturer);
            json.name("beacon").value(advertisement == null ? null : advertisement.beacon);
            json.name("services").value(advertisement == null ? null : advertisement.services);
            json.name("identity_key").value(advertisement == null ? null : advertisement.identityKey);
            DeviceHistory.Record remembered = device.history;
            if (remembered != null) {
                json.name("first_seen_ms").value(remembered.firstSeenEpochMillis);
                json.name("visits").value((long) remembered.visits);
            } else {
                json.name("first_seen_ms").value((String) null);
                json.name("visits").value((String) null);
            }
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

    private String wifiJson() {
        // One generation: the list, its state and its dropped count are read together.
        WifiSurvey survey = wifi.survey();
        Json json = new Json();
        json.beginObject();
        json.name("access_points").beginArray();
        for (WifiAp ap : survey.accessPoints) {
            json.beginObject();
            json.name("bssid").value(ap.bssid);
            json.name("ssid").value(ap.ssid);
            json.name("frequency_mhz").value((long) ap.frequencyMhz);
            if (ap.channel > 0) {
                json.name("channel").value((long) ap.channel);
            } else {
                json.name("channel").value((String) null);
            }
            json.name("rssi_dbm").value((long) ap.rssiDbm);
            json.name("reliability").value(ap.reliability);
            json.name("proximity").value(ap.proximity);
            json.name("security").value(ap.security);
            json.name("enterprise").value(ap.enterprise);
            json.name("trackability").value(ap.trackability);
            json.name("last_seen_ms").value(ap.lastSeenEpochMillis);
            DeviceHistory.Record remembered = ap.history;
            if (remembered != null) {
                json.name("first_seen_ms").value(remembered.firstSeenEpochMillis);
                json.name("visits").value((long) remembered.visits);
            } else {
                json.name("first_seen_ms").value((String) null);
                json.name("visits").value((String) null);
            }
            json.endObject();
        }
        json.endArray();
        json.name("state").value(survey.state);
        json.name("dropped").value((long) survey.dropped);
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

    private static String trackabilityLabel(int trackability) {
        switch (trackability) {
            case NativeRadar.TRACKABILITY_TRACKABLE:
                return "TRACKABLE";
            case NativeRadar.TRACKABILITY_RANDOMIZED:
                return "RANDOMIZED";
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

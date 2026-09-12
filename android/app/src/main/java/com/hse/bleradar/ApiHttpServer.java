package com.hse.bleradar;

import android.os.SystemClock;
import android.util.JsonWriter;
import android.util.Log;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.StringWriter;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;

/**
 * Loopback-only HTTP server exposing the live device snapshot as JSON, for a
 * web UI or Termux tooling running on the same device.
 *
 * <p>Android ships no HTTP server class ({@code com.sun.net.httpserver} is a
 * JDK-only package absent from {@code android.jar}), so this is a deliberately
 * minimal HTTP/1.1 responder over {@link ServerSocket}: every request is
 * answered with a complete {@code Content-Length}-framed body and the
 * connection is closed.
 *
 * <p>Endpoints (all {@code GET}; JSON unless noted):
 * <ul>
 *   <li>{@code /api/devices} — every live device from
 *       {@link BleScanEngine#snapshot()}, in its ranked order: {@code address},
 *       {@code name}, {@code distance_m} / {@code distance_lower_m} /
 *       {@code distance_upper_m} (metres, {@code null} when unavailable),
 *       {@code rssi_dbm}, {@code proximity}, {@code trend}, {@code freshness},
 *       {@code confidence_percent}, {@code last_seen_ago_ms}; plus the top-level
 *       {@code scanning}, {@code native_available}, {@code timestamp_ms}.</li>
 *   <li>{@code /api/status} — {@code scanning}, {@code device_count},
 *       {@code native_available}, {@code uptime_ms}.</li>
 *   <li>{@code /api/updates} — {@code last_check_ms}, {@code next_check_ms},
 *       {@code retry_count}.</li>
 *   <li>{@code /} — an HTML index of the endpoints ({@code text/html}).</li>
 * </ul>
 * Unknown paths answer {@code 404}; known paths with any method but
 * {@code GET} answer {@code 405}; a malformed request line answers {@code 400}.
 *
 * <p>Binding to {@code 127.0.0.1} is the whole access-control model: nothing
 * off-device can reach the port, and remote use goes through an SSH tunnel.
 */
public final class ApiHttpServer {

    private static final String TAG = "ApiHttpServer";
    static final int PORT = 8080;
    private static final int BACKLOG = 8;
    private static final int HANDLER_THREADS = 2;
    /** Frees a handler thread from a client that connects but never sends its request. */
    private static final int REQUEST_TIMEOUT_MS = 5_000;
    private static final String JSON = "application/json";
    private static final String HTML = "text/html; charset=utf-8";
    private static final String INDEX_HTML = "<!DOCTYPE html>\n"
            + "<html>\n"
            + "<head><title>HSE BLE Radar API</title></head>\n"
            + "<body>\n"
            + "<h1>HSE BLE Radar REST API</h1>\n"
            + "<ul>\n"
            + "<li><a href=\"/api/devices\">/api/devices</a> - Live device snapshot</li>\n"
            + "<li><a href=\"/api/status\">/api/status</a> - Service status</li>\n"
            + "<li><a href=\"/api/updates\">/api/updates</a> - Update check status</li>\n"
            + "</ul>\n"
            + "</body>\n"
            + "</html>\n";

    private final BleScanEngine engine;
    private final UpdateManager updateManager;
    private ServerSocket listener;
    private ExecutorService handlers;

    public ApiHttpServer(BleScanEngine engine, UpdateManager updateManager) {
        this.engine = engine;
        this.updateManager = updateManager;
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
        ServerSocket socket;
        try {
            socket = new ServerSocket();
            socket.setReuseAddress(true);
            socket.bind(new InetSocketAddress(InetAddress.getLoopbackAddress(), PORT), BACKLOG);
        } catch (IOException error) {
            Log.e(TAG, "Failed to bind http://127.0.0.1:" + PORT, error);
            return;
        }
        ExecutorService pool = Executors.newFixedThreadPool(HANDLER_THREADS);
        listener = socket;
        handlers = pool;
        Thread acceptThread = new Thread(() -> acceptLoop(socket, pool), TAG + "-accept");
        acceptThread.setDaemon(true);
        acceptThread.start();
        Log.d(TAG, "HTTP server listening on http://127.0.0.1:" + PORT);
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
        Log.d(TAG, "HTTP server stopped");
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
            // Headers are irrelevant to every route and no route reads a body,
            // so drain them only to reach the end of the request.
            for (String header = reader.readLine(); header != null && !header.isEmpty(); header = reader.readLine()) {
                // drained
            }
            OutputStream out = socket.getOutputStream();
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
            respond(out, parts[0], path);
        } catch (IOException error) {
            Log.w(TAG, "HTTP request failed: " + error);
        }
    }

    private void respond(OutputStream out, String method, String path) throws IOException {
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
                writeResponse(out, 200, "OK", HTML, INDEX_HTML);
                break;
        }
    }

    private String devicesJson() throws IOException {
        List<Blip> devices = engine.snapshot();
        // Blip.lastSeenUptimeMillis is on the SystemClock.uptimeMillis() timeline,
        // so the age must be measured on that same clock, never wall-clock epoch.
        long nowUptimeMs = SystemClock.uptimeMillis();
        StringWriter buffer = new StringWriter();
        try (JsonWriter writer = new JsonWriter(buffer)) {
            writer.beginObject();
            writer.name("devices").beginArray();
            for (Blip device : devices) {
                writer.beginObject();
                writer.name("address").value(device.address);
                writer.name("name").value(device.name);
                writeFinite(writer, "distance_m", device.distanceMetres);
                writeFinite(writer, "distance_lower_m", device.distanceLowerBoundMetres);
                writeFinite(writer, "distance_upper_m", device.distanceUpperBoundMetres);
                writeFinite(writer, "rssi_dbm", device.lastRssiDbm);
                writer.name("proximity").value(proximityLabel(device.proximity));
                writer.name("trend").value(trendLabel(device.trend));
                writer.name("freshness").value(freshnessLabel(device.freshness));
                writer.name("confidence_percent").value(device.confidencePercent);
                writer.name("last_seen_ago_ms").value(Math.max(0L, nowUptimeMs - device.lastSeenUptimeMillis));
                writer.endObject();
            }
            writer.endArray();
            writer.name("scanning").value(engine.isScanning());
            writer.name("native_available").value(NativeRadar.isAvailable());
            writer.name("timestamp_ms").value(System.currentTimeMillis());
            writer.endObject();
        }
        return buffer.toString();
    }

    private String statusJson() throws IOException {
        StringWriter buffer = new StringWriter();
        try (JsonWriter writer = new JsonWriter(buffer)) {
            writer.beginObject();
            writer.name("scanning").value(engine.isScanning());
            writer.name("device_count").value(engine.snapshot().size());
            writer.name("native_available").value(NativeRadar.isAvailable());
            writer.name("uptime_ms").value(engine.getUptimeMillis());
            writer.endObject();
        }
        return buffer.toString();
    }

    private String updatesJson() throws IOException {
        StringWriter buffer = new StringWriter();
        try (JsonWriter writer = new JsonWriter(buffer)) {
            writer.beginObject();
            writer.name("last_check_ms").value(updateManager.getLastCheckTimeMs());
            writer.name("next_check_ms").value(updateManager.getNextCheckTimeMs());
            writer.name("retry_count").value(updateManager.getRetryCount());
            writer.endObject();
        }
        return buffer.toString();
    }

    /** Writes {@code value}, or JSON {@code null} for NaN/infinite (which JsonWriter would reject). */
    private static void writeFinite(JsonWriter writer, String name, double value) throws IOException {
        writer.name(name);
        if (Double.isFinite(value)) {
            writer.value(value);
        } else {
            writer.nullValue();
        }
    }

    private static void writeResponse(OutputStream out, int status, String reason, String contentType, String body)
            throws IOException {
        byte[] payload = body.getBytes(StandardCharsets.UTF_8);
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
        return "{\"error\":\"" + message + "\"}";
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

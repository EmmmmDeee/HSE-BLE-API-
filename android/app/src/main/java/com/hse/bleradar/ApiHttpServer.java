package com.hse.bleradar;

import android.util.JsonWriter;
import android.util.Log;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpHandler;
import com.sun.net.httpserver.HttpServer;

import java.io.IOException;
import java.io.OutputStreamWriter;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.Executors;

/**
 * Lightweight HTTP server exposing REST API for real-time Bluetooth device snapshot.
 *
 * <p>Binds to 127.0.0.1:8080 (localhost only) and provides:
 * <ul>
 *   <li>GET /api/devices - Current device snapshot with distance, RSSI, freshness
 *   <li>GET /api/status - Service status and native availability
 *   <li>GET /api/updates - Update check status and retry state
 * </ul>
 *
 * <p>Intended for web UI access from the same device (Termux, web browser).
 * No authentication required for localhost binding (network isolation is the security model).
 */
public final class ApiHttpServer {

    private static final String TAG = "ApiHttpServer";
    private static final int PORT = 8080;
    private static final String BIND_ADDRESS = "127.0.0.1";

    private HttpServer httpServer;
    private final BleScanEngine engine;
    private final UpdateManager updateManager;

    public ApiHttpServer(BleScanEngine engine, UpdateManager updateManager) {
        this.engine = engine;
        this.updateManager = updateManager;
    }

    /**
     * Starts the HTTP server on a background thread.
     * Safe to call multiple times (idempotent).
     */
    public void start() {
        if (httpServer != null) {
            return; // Already running
        }

        try {
            httpServer = HttpServer.create(new InetSocketAddress(BIND_ADDRESS, PORT), 0);
            httpServer.setExecutor(Executors.newFixedThreadPool(2));

            httpServer.createContext("/api/devices", new DevicesHandler());
            httpServer.createContext("/api/status", new StatusHandler());
            httpServer.createContext("/api/updates", new UpdatesHandler());
            httpServer.createContext("/", new RootHandler());

            httpServer.start();
            Log.d(TAG, "HTTP server started on http://" + BIND_ADDRESS + ":" + PORT);
        } catch (IOException e) {
            Log.e(TAG, "Failed to start HTTP server", e);
        }
    }

    /**
     * Stops the HTTP server immediately.
     * Safe to call multiple times (idempotent).
     */
    public void stop() {
        if (httpServer != null) {
            httpServer.stop(0);
            httpServer = null;
            Log.d(TAG, "HTTP server stopped");
        }
    }

    // ============ HTTP Handlers ============

    private class DevicesHandler implements HttpHandler {
        @Override
        public void handle(HttpExchange exchange) throws IOException {
            if (!"GET".equals(exchange.getRequestMethod())) {
                sendError(exchange, 405, "Method not allowed");
                return;
            }

            List<Blip> devices = engine.snapshot();
            long timestampMs = System.currentTimeMillis();

            exchange.getResponseHeaders().set("Content-Type", "application/json");
            exchange.sendResponseHeaders(200, 0);

            try (OutputStreamWriter osw = new OutputStreamWriter(exchange.getResponseBody(), StandardCharsets.UTF_8);
                 JsonWriter writer = new JsonWriter(osw)) {

                writer.beginObject();
                writer.name("devices").beginArray();

                for (Blip device : devices) {
                    writer.beginObject();
                    writer.name("address").value(device.address);
                    writer.name("name").value(device.name);
                    writer.name("distance_m").value(Double.isFinite(device.distanceMetres) ? device.distanceMetres : null);
                    writer.name("distance_lower_m").value(Double.isFinite(device.distanceLowerBoundMetres) ? device.distanceLowerBoundMetres : null);
                    writer.name("distance_upper_m").value(Double.isFinite(device.distanceUpperBoundMetres) ? device.distanceUpperBoundMetres : null);
                    writer.name("rssi_dbm").value(device.lastRssiDbm);
                    writer.name("proximity").value(proximityLabel(device.proximity));
                    writer.name("trend").value(trendLabel(device.trend));
                    writer.name("freshness").value(freshnessLabel(device.freshness));
                    writer.name("confidence_percent").value(device.confidencePercent);
                    writer.name("last_seen_ago_ms").value(System.currentTimeMillis() - device.lastSeenUptimeMillis);
                    writer.endObject();
                }

                writer.endArray();
                writer.name("scanning").value(engine.isScanning());
                writer.name("native_available").value(NativeRadar.isAvailable());
                writer.name("timestamp_ms").value(timestampMs);
                writer.endObject();
            }
        }
    }

    private class StatusHandler implements HttpHandler {
        @Override
        public void handle(HttpExchange exchange) throws IOException {
            if (!"GET".equals(exchange.getRequestMethod())) {
                sendError(exchange, 405, "Method not allowed");
                return;
            }

            exchange.getResponseHeaders().set("Content-Type", "application/json");
            exchange.sendResponseHeaders(200, 0);

            try (OutputStreamWriter osw = new OutputStreamWriter(exchange.getResponseBody(), StandardCharsets.UTF_8);
                 JsonWriter writer = new JsonWriter(osw)) {

                writer.beginObject();
                writer.name("scanning").value(engine.isScanning());
                writer.name("device_count").value(engine.snapshot().size());
                writer.name("native_available").value(NativeRadar.isAvailable());
                writer.name("uptime_ms").value(engine.getUptimeMillis());
                writer.endObject();
            }
        }
    }

    private class UpdatesHandler implements HttpHandler {
        @Override
        public void handle(HttpExchange exchange) throws IOException {
            if (!"GET".equals(exchange.getRequestMethod())) {
                sendError(exchange, 405, "Method not allowed");
                return;
            }

            exchange.getResponseHeaders().set("Content-Type", "application/json");
            exchange.sendResponseHeaders(200, 0);

            try (OutputStreamWriter osw = new OutputStreamWriter(exchange.getResponseBody(), StandardCharsets.UTF_8);
                 JsonWriter writer = new JsonWriter(osw)) {

                writer.beginObject();
                writer.name("last_check_ms").value(updateManager.getLastCheckTimeMs());
                writer.name("next_check_ms").value(updateManager.getNextCheckTimeMs());
                writer.name("retry_count").value(updateManager.getRetryCount());
                writer.endObject();
            }
        }
    }

    private class RootHandler implements HttpHandler {
        @Override
        public void handle(HttpExchange exchange) throws IOException {
            if (exchange.getRequestURI().getPath().equals("/")) {
                String html = "<!DOCTYPE html>\n" +
                        "<html>\n" +
                        "<head><title>HSE BLE Radar API</title></head>\n" +
                        "<body>\n" +
                        "<h1>HSE BLE Radar REST API</h1>\n" +
                        "<ul>\n" +
                        "<li><a href=\"/api/devices\">/api/devices</a> - Live device snapshot</li>\n" +
                        "<li><a href=\"/api/status\">/api/status</a> - Service status</li>\n" +
                        "<li><a href=\"/api/updates\">/api/updates</a> - Update check status</li>\n" +
                        "</ul>\n" +
                        "</body>\n" +
                        "</html>";

                exchange.getResponseHeaders().set("Content-Type", "text/html");
                exchange.sendResponseHeaders(200, html.getBytes(StandardCharsets.UTF_8).length);
                exchange.getResponseBody().write(html.getBytes(StandardCharsets.UTF_8));
                exchange.close();
            } else {
                sendError(exchange, 404, "Not found");
            }
        }
    }

    // ============ Utilities ============

    private void sendError(HttpExchange exchange, int code, String message) throws IOException {
        String errorJson = "{\"error\": \"" + message + "\"}";
        exchange.getResponseHeaders().set("Content-Type", "application/json");
        exchange.sendResponseHeaders(code, errorJson.getBytes(StandardCharsets.UTF_8).length);
        exchange.getResponseBody().write(errorJson.getBytes(StandardCharsets.UTF_8));
        exchange.close();
    }

    private String proximityLabel(int proximity) {
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

    private String trendLabel(int trend) {
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

    private String freshnessLabel(int freshness) {
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

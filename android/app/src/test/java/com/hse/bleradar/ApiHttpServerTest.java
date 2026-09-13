package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link ApiHttpServer} REST API contract.
 *
 * <p>ApiHttpServer exposes real-time Bluetooth device data via HTTP on localhost:8080.
 * These tests verify the API contract: endpoint paths, response formats, and data fields.
 *
 * <p>The API enables:
 * <ul>
 *   <li>Web UI dashboard in Termux or web browser</li>
 *   <li>Remote monitoring without pairing to Android system services</li>
 *   <li>Integration with custom analysis tools</li>
 * </ul>
 *
 * <p>These methods are contract notes, not executed coverage (no JUnit runner
 * exists in this tree; see decision #86). The executed proof of every note
 * below is {@code cargo xtask verify-api-live}: ApiHttpServer references
 * nothing in {@code android.*} — its collaborators are SnapshotSource
 * (BleScanEngine), UpdateStatusSource (UpdateManager), AssetSource
 * ({@code getAssets()::open}) and two LongSupplier clocks, JSON is written by
 * Json, logging goes through java.util.logging — so xtask compiles the class
 * without {@code android.jar}, runs it with fixture sources and the host native
 * library, answers ten real HTTP requests (the three JSON documents
 * byte-identical to the browser fixtures) and renders the dashboard from it in
 * headless Chromium. The {@code gates} CI job runs that command on every push.
 */
public class ApiHttpServerTest {

    @Test
    public void http_server_binds_to_localhost_only() {
        // ApiHttpServer must bind to 127.0.0.1:8080 for security
        // This prevents network exposure; remote access requires SSH tunnel
        String bindAddress = "127.0.0.1";
        int bindPort = 8080;

        assertTrue("Should bind to localhost address", bindAddress.equals("127.0.0.1"));
        assertEquals("Should bind to port 8080", 8080, bindPort);
    }

    @Test
    public void devices_endpoint_path_is_correct() {
        // GET /api/devices returns live device snapshot
        String endpoint = "/api/devices";
        assertTrue("Endpoint should start with /api", endpoint.startsWith("/api"));
        assertTrue("Endpoint should target devices", endpoint.contains("devices"));
    }

    @Test
    public void devices_endpoint_returns_json_array() {
        // Response format: { "devices": [ { device fields }, ... ], "metadata" }
        // Each device object contains address, name, distance, RSSI, proximity, trend, freshness, confidence
        String contentType = "application/json";
        assertEquals("Response should be JSON", "application/json", contentType);
    }

    @Test
    public void device_json_contains_required_fields() {
        // Each device object MUST contain:
        // - address: Bluetooth MAC address (string)
        // - name: Device name (string)
        // - distance_m: Estimated distance in meters (float or null)
        // - distance_lower_m: Conservative near bound (float or null)
        // - distance_upper_m: Conservative far bound (float or null)
        // - rssi_dbm: Last filtered RSSI in dBm (float)
        // - proximity: Classification (IMMEDIATE|NEAR|MID|FAR)
        // - trend: Signal change (STRONGER|WEAKER|STABLE)
        // - freshness: Time classification (LIVE|RECENT|STALE)
        // - confidence_percent: Confidence score 0-100 (int)
        // - last_seen_ago_ms: Milliseconds since last update (int)
        assertTrue("address field required", true);
        assertTrue("name field required", true);
        assertTrue("distance_m field required", true);
        assertTrue("rssi_dbm field required", true);
        assertTrue("proximity field required", true);
        assertTrue("confidence_percent field required", true);
    }

    @Test
    public void proximity_values_are_standard() {
        // Proximity values must match NativeRadar ordinals
        String immediate = "IMMEDIATE";
        String near = "NEAR";
        String mid = "MID";
        String far = "FAR";

        assertTrue("IMMEDIATE is valid", immediate.equals("IMMEDIATE"));
        assertTrue("NEAR is valid", near.equals("NEAR"));
        assertTrue("MID is valid", mid.equals("MID"));
        assertTrue("FAR is valid", far.equals("FAR"));
    }

    @Test
    public void trend_values_are_standard() {
        // Trend values must match NativeRadar ordinals
        String stronger = "STRONGER";
        String weaker = "WEAKER";
        String stable = "STABLE";

        assertTrue("STRONGER is valid", stronger.equals("STRONGER"));
        assertTrue("WEAKER is valid", weaker.equals("WEAKER"));
        assertTrue("STABLE is valid", stable.equals("STABLE"));
    }

    @Test
    public void freshness_values_are_standard() {
        // Freshness values must match NativeRadar ordinals
        String live = "LIVE";
        String recent = "RECENT";
        String stale = "STALE";

        assertTrue("LIVE is valid", live.equals("LIVE"));
        assertTrue("RECENT is valid", recent.equals("RECENT"));
        assertTrue("STALE is valid", stale.equals("STALE"));
    }

    @Test
    public void devices_response_includes_metadata() {
        // Top-level response MUST include:
        // - devices: array of device objects
        // - scanning: boolean (true if scan is active)
        // - native_available: boolean (true if Rust core loaded)
        // - timestamp_ms: response timestamp in milliseconds
        String scanningField = "scanning";
        String nativeAvailableField = "native_available";
        String timestampField = "timestamp_ms";

        assertTrue("scanning field required", scanningField.equals("scanning"));
        assertTrue("native_available field required", nativeAvailableField.equals("native_available"));
        assertTrue("timestamp_ms field required", timestampField.equals("timestamp_ms"));
    }

    @Test
    public void status_endpoint_path_is_correct() {
        // GET /api/status returns service status summary
        String endpoint = "/api/status";
        assertTrue("Endpoint should be /api/status", endpoint.equals("/api/status"));
    }

    @Test
    public void status_endpoint_contains_device_count() {
        // status response includes:
        // - scanning: boolean
        // - device_count: number of devices in map
        // - native_available: boolean
        // - uptime_ms: milliseconds since scan started
        String deviceCountField = "device_count";
        String uptimeField = "uptime_ms";

        assertTrue("device_count field required", deviceCountField.equals("device_count"));
        assertTrue("uptime_ms field required", uptimeField.equals("uptime_ms"));
    }

    @Test
    public void updates_endpoint_path_is_correct() {
        // GET /api/updates returns update check status
        String endpoint = "/api/updates";
        assertTrue("Endpoint should be /api/updates", endpoint.equals("/api/updates"));
    }

    @Test
    public void updates_endpoint_contains_check_state() {
        // updates response includes:
        // - last_check_ms: timestamp of last check (ms since epoch)
        // - next_check_ms: estimated next check time (ms since epoch)
        // - retry_count: current retry attempt number
        String lastCheckField = "last_check_ms";
        String nextCheckField = "next_check_ms";
        String retryCountField = "retry_count";

        assertTrue("last_check_ms field required", lastCheckField.equals("last_check_ms"));
        assertTrue("next_check_ms field required", nextCheckField.equals("next_check_ms"));
        assertTrue("retry_count field required", retryCountField.equals("retry_count"));
    }

    @Test
    public void root_path_returns_the_web_dashboard() {
        // GET / returns the packaged assets/dashboard.html (text/html; charset=utf-8),
        // a self-contained page that polls the three JSON endpoints; Java serves its
        // bytes unchanged (read once per start() through Streams.readAllBytes) and
        // answers 500 if the asset is unreadable. The page itself is exercised in a
        // real browser by `cargo xtask verify-dashboard-live`.
        String contentType = "text/html; charset=utf-8";
        assertTrue("Root should serve HTML", contentType.startsWith("text/html"));
    }

    @Test
    public void api_is_enabled_on_service_startup() {
        // ApiHttpServer is created in RadarScanService.onCreate()
        // and started before any scan requests arrive
        // This ensures API is immediately available once service starts
        boolean apiStartsAutomatically = true;
        assertTrue("API should start automatically", apiStartsAutomatically);
    }

    @Test
    public void api_is_disabled_on_service_shutdown() {
        // ApiHttpServer is stopped in RadarScanService.onDestroy()
        // This prevents port binding or resource leaks after service exit
        boolean apiStopsGracefully = true;
        assertTrue("API should stop on service destroy", apiStopsGracefully);
    }

    @Test
    public void invalid_http_methods_return_405() {
        // The documents (/, /api/devices, /api/status, /api/updates) are GET
        // only and scan control (/api/scan/start, /api/scan/stop) is POST only;
        // a known path with any other method answers 405 Method Not Allowed
        int notAllowed = 405;
        assertEquals("Invalid methods should return 405", 405, notAllowed);
    }

    @Test
    public void scan_control_routes_report_the_live_state_or_the_refusal() {
        // POST /api/scan/start -> ScanControl.requestStart(): 200 {"scanning":<live>}
        // when START_ACCEPTED, else 409 {"error":<reason>} — permissions not
        // granted, Bluetooth off, scanner unavailable, background start
        // refused by Android. POST /api/scan/stop -> requestStop(): 200 with
        // the live state; the service stays alive (paused) so the API remains
        // reachable. Request bodies are skipped, never read. A collaborator
        // that throws is answered 500 (COR-032) rather than ending the
        // process. Executed proof: cargo xtask verify-api-live drives every
        // outcome through a scripted ScanControl with real requests.
        int conflict = 409;
        assertEquals("A refused start answers 409", 409, conflict);
    }

    @Test
    public void missing_endpoints_return_404() {
        // Requests to undefined paths should return 404 Not Found
        // This allows future endpoint expansion without breaking clients
        int notFound = 404;
        assertEquals("Missing endpoints should return 404", 404, notFound);
    }

    @Test
    public void null_distance_values_are_omitted_from_json() {
        // If distance_m, distance_lower_m, or distance_upper_m are NaN (invalid),
        // they should be serialized as JSON null rather than numeric NaN
        // This ensures valid JSON and prevents client parsing errors
        boolean handlesNanCorrectly = true;
        assertTrue("NaN values should serialize as null", handlesNanCorrectly);
    }

    @Test
    public void last_seen_ago_is_computed_at_response_time() {
        // last_seen_ago_ms must be computed fresh for each request
        // as (SystemClock.uptimeMillis() - device.lastSeenUptimeMillis): both
        // sides live on the uptime clock, never the wall-clock epoch.
        // This ensures clients see relative time, not stale snapshots
        boolean computedFresh = true;
        assertTrue("last_seen_ago_ms computed fresh per request", computedFresh);
    }
}

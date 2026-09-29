package com.hse.bleradar;

import java.util.List;

/**
 * What {@link ApiHttpServer} reads to answer {@code /api/wifi}: the surveyed
 * access points and why the list may be empty. Free of {@code android.*}, like
 * {@link SnapshotSource}, so the host JVM can serve the route against fixtures.
 */
interface WifiSurveySource {

    /** The survey is not running (never started, or stopped with the scan). */
    String STATE_IDLE = "idle";
    /** Scanning, and results are being read. */
    String STATE_ACTIVE = "active";
    /** The Wi-Fi permission was not granted; the survey is empty by refusal, not by absence. */
    String STATE_PERMISSION_DENIED = "permission_denied";
    /** Wi-Fi is switched off, so there is nothing to scan. */
    String STATE_WIFI_OFF = "wifi_off";
    /** Below Android 13 only: location services are off, which hides scan results. */
    String STATE_LOCATION_OFF = "location_off";
    /** No Wi-Fi radio, the platform refused the service, or the Rust core is not loaded to classify results. */
    String STATE_UNAVAILABLE = "unavailable";

    /** The latest survey, one row per BSSID (a defensive copy). */
    List<WifiAp> accessPoints();

    /** One of the {@code STATE_*} constants. */
    String state();

    /** Scan results in the latest read that named no access point: a placeholder BSSID or an undecodable answer. */
    int dropped();
}

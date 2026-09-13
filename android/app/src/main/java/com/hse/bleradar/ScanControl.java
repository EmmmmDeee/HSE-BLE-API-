package com.hse.bleradar;

/**
 * Scan control as the HTTP API exposes it ({@code POST /api/scan/start},
 * {@code POST /api/scan/stop}), so a Termux shell or the web dashboard can
 * drive the radar without the screen.
 *
 * <p>{@link RadarScanService} implements it on the device; the host harness
 * behind {@code cargo xtask verify-api-live} implements it with a scripted
 * fake, which is how the routes are exercised by real requests.
 */
interface ScanControl {

    /** Scanning started (or was already running). */
    int START_ACCEPTED = 0;
    /** The Bluetooth runtime permissions have not been granted; the app must be opened once. */
    int START_PERMISSIONS_MISSING = 1;
    /** The Bluetooth adapter is off or absent. */
    int START_BLUETOOTH_OFF = 2;
    /** The LE scanner refused to start. */
    int START_UNAVAILABLE = 3;
    /**
     * Android refused to start the foreground service because the app counts
     * as being in the background (API 31+); opening the app once lifts it.
     */
    int START_BACKGROUND_RESTRICTED = 4;

    /** Requests a scan; one of the {@code START_*} outcomes. */
    int requestStart();

    /**
     * Pauses scanning while keeping the service — and therefore this API —
     * alive; ending the service is the app's own Stop action.
     */
    void requestStop();
}

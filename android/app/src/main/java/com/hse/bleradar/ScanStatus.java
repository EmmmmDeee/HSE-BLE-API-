package com.hse.bleradar;

/**
 * What the scan is doing and why, published as one immutable value (like
 * {@link WifiSurvey}) so a reader never pairs one moment's state with another's
 * reason. Free of {@code android.*} so the host proofs can build it.
 *
 * <p>{@link #state} is one of {@link #IDLE} (no scan wanted), {@link #SCANNING},
 * {@link #RECOVERING} (a scan is wanted but is not running right now: the
 * adapter is off, or the platform refused a registration and a retry is
 * scheduled) and {@link #FAILED} (a scan is wanted but the platform will not
 * run it; a new start request tries again). {@link #error} says why for the
 * last two, and is {@code null} otherwise.
 */
final class ScanStatus {

    static final String IDLE = "idle";
    static final String SCANNING = "scanning";
    static final String RECOVERING = "recovering";
    static final String FAILED = "failed";

    static final ScanStatus IDLE_STATUS = new ScanStatus(IDLE, null);
    static final ScanStatus SCANNING_STATUS = new ScanStatus(SCANNING, null);

    final String state;
    final String error;

    ScanStatus(String state, String error) {
        this.state = state;
        this.error = error;
    }

    /** Whether the platform scan is registered and running. */
    boolean isScanning() {
        return SCANNING.equals(state);
    }
}

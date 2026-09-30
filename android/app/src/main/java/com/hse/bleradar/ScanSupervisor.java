package com.hse.bleradar;

/**
 * The scan's state machine: keeps a scan that was asked for running through
 * what the platform does to it. {@link BleScanEngine} used to set a flag when
 * {@code startScan} returned and log {@code onScanFailed}; the emulator proof
 * showed the result — after Bluetooth was turned off and on, or after the
 * stack refused a registration, the app reported {@code scanning: true} while
 * nothing was scanned, for good.
 *
 * <p>It separates what was <em>requested</em> from what the platform is
 * <em>doing</em>:
 * <ul>
 *   <li>{@link #request()} records the request and starts the platform scan;
 *       {@link #cancel()} withdraws it;</li>
 *   <li>the adapter going off ({@link #onAdapter}) leaves the request standing
 *       in {@link ScanStatus#RECOVERING}; its return starts the scan again;</li>
 *   <li>a failure the platform reports ({@link #onScanFailed}) is judged by the
 *       Rust rule ({@link NativeRadar#scanFailureAction}): the scan is already
 *       running, is retried after a backoff, or is given up on
 *       ({@link ScanStatus#FAILED}), and the reason is published either way;</li>
 *   <li>a result arriving ({@link #onResult}) proves the scan works and ends
 *       the incident, so its retries are counted afresh.</li>
 * </ul>
 *
 * <p>Free of {@code android.*}: the platform is a {@link Platform} the engine
 * implements, so the host proofs drive every transition with a fake and the
 * real Rust rule. One monitor guards the state; callbacks may arrive on the
 * main thread, the HTTP handlers and the scheduler.
 */
final class ScanSupervisor {

    /** The Android side of the scan. */
    interface Platform {
        /** Registers the platform scan; {@code false} when it cannot be (no scanner, permission gone). Never throws. */
        boolean startScanner();

        /** Ends the registration, best effort. Never throws. */
        void stopScanner();

        /** Whether the Bluetooth adapter is on. */
        boolean adapterOn();

        /** Runs {@code action} once after {@code delayMillis}, on the thread the platform's callbacks arrive on. */
        void schedule(long delayMillis, Runnable action);

        /** Drops a scheduled action, if any. */
        void cancelScheduled();
    }

    /** {@link NativeRadar#scanFailureAction}, or a stand-in where the core is missing. */
    interface FailureRule {
        long action(int errorCode, int retriesSoFar);
    }

    /** {@code ScanCallback.SCAN_FAILED_INTERNAL_ERROR}: what a scanner that cannot be started counts as. */
    static final int INTERNAL_ERROR = 3;

    private final Platform platform;
    private final FailureRule rule;
    private volatile ScanStatus status = ScanStatus.IDLE_STATUS;
    private boolean requested;
    private boolean closed;
    private boolean registered;
    /** Failures of the current incident that were retried; reset by a result, a request or the adapter's return. */
    private volatile int retries;

    ScanSupervisor(Platform platform, FailureRule rule) {
        this.platform = platform;
        this.rule = rule;
    }

    ScanStatus status() {
        return status;
    }

    /**
     * Asks for a scan and starts it. {@code true} when the platform scan was
     * registered (or already was); {@code false} — leaving the state as it was —
     * when the adapter is off or the scan cannot be registered, which is the
     * caller's refusal to report.
     */
    synchronized boolean request() {
        if (closed) {
            return false;
        }
        if (status.isScanning()) {
            return true;
        }
        if (!platform.adapterOn()) {
            return false;
        }
        platform.cancelScheduled();
        if (registered) {
            platform.stopScanner();
            registered = false;
        }
        if (!platform.startScanner()) {
            return false;
        }
        requested = true;
        registered = true;
        retries = 0;
        status = ScanStatus.SCANNING_STATUS;
        return true;
    }

    /** Withdraws the request: no scan, no retry pending, {@link ScanStatus#IDLE}. */
    synchronized void cancel() {
        requested = false;
        retries = 0;
        platform.cancelScheduled();
        if (registered) {
            platform.stopScanner();
            registered = false;
        }
        status = ScanStatus.IDLE_STATUS;
    }

    /** {@link #cancel()}, and every later {@link #request()} is refused. */
    synchronized void close() {
        cancel();
        closed = true;
    }

    /** The adapter went off ({@code false}) or came back on ({@code true}). */
    synchronized void onAdapter(boolean on) {
        if (!requested || closed) {
            return;
        }
        platform.cancelScheduled();
        if (!on) {
            // The platform's scanner went with the adapter: nothing to stop.
            registered = false;
            status = new ScanStatus(ScanStatus.RECOVERING, "Bluetooth is off");
            return;
        }
        // Always register afresh: a missed off event must not leave a dead
        // registration standing.
        retries = 0;
        if (registered) {
            platform.stopScanner();
            registered = false;
        }
        attempt();
    }

    /** {@code ScanCallback.onScanFailed}. */
    synchronized void onScanFailed(int errorCode) {
        if (!requested || closed || status.state.equals(ScanStatus.IDLE)) {
            return;
        }
        fail(errorCode);
    }

    /** A scan result arrived: the scan works, so the incident is over. */
    void onResult() {
        if (retries != 0) {
            synchronized (this) {
                retries = 0;
            }
        }
    }

    /** Starts the platform scan; a scanner that cannot be started counts as an internal error. */
    private void attempt() {
        if (platform.startScanner()) {
            registered = true;
            status = ScanStatus.SCANNING_STATUS;
        } else {
            fail(INTERNAL_ERROR);
        }
    }

    private void fail(int errorCode) {
        long action = rule.action(errorCode, retries);
        if (action == NativeRadar.SCAN_FAILURE_ALREADY_RUNNING) {
            // The platform says it is scanning: the registration stands.
            return;
        }
        registered = false;
        platform.cancelScheduled();
        String cause = describe(errorCode);
        if (action == NativeRadar.SCAN_FAILURE_GIVE_UP || action < 0) {
            status = new ScanStatus(ScanStatus.FAILED,
                    "The Bluetooth scan failed: " + cause + "; not retrying"
                            + (retries > 0 ? " after " + retries + " retries" : ""));
            return;
        }
        retries++;
        status = new ScanStatus(ScanStatus.RECOVERING,
                "The Bluetooth scan failed: " + cause + "; retrying in " + seconds(action));
        platform.schedule(action, this::retry);
    }

    /** The scheduled retry. */
    private synchronized void retry() {
        if (!requested || closed || !status.state.equals(ScanStatus.RECOVERING)) {
            return;
        }
        if (!platform.adapterOn()) {
            // The adapter's return will resume the scan.
            status = new ScanStatus(ScanStatus.RECOVERING, "Bluetooth is off");
            return;
        }
        attempt();
    }

    private static String seconds(long millis) {
        return millis % 1000 == 0 ? (millis / 1000) + " s" : millis + " ms";
    }

    /** The platform's own name for a {@code SCAN_FAILED_*} code. */
    static String describe(int errorCode) {
        switch (errorCode) {
            case 1:
                return "already started (code 1)";
            case 2:
                return "the platform refused the registration (code 2)";
            case 3:
                return "internal error (code 3)";
            case 4:
                return "this device cannot scan (code 4)";
            case 5:
                return "out of hardware resources (code 5)";
            case 6:
                return "scanning too frequently (code 6)";
            default:
                return "unknown error (code " + errorCode + ")";
        }
    }
}

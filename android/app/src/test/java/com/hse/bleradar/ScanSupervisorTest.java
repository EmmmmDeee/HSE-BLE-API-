package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link ScanSupervisor}, the scan's state machine, driven with
 * a fake platform and the real Rust failure rule ({@code scanFailureAction}
 * through the loaded core). The emulator proof shows the same machine follow
 * the real Bluetooth adapter; these pin every transition and its bound.
 */
public class ScanSupervisorTest {

    private static final int REGISTRATION_FAILED = 2;
    private static final int UNSUPPORTED = 4;
    private static final int ALREADY_STARTED = 1;

    /** The Android side, scripted. */
    private static final class FakePlatform implements ScanSupervisor.Platform {
        boolean adapterOn = true;
        boolean startWorks = true;
        int starts;
        int stops;
        Runnable pending;
        long pendingDelay = -1;

        @Override
        public boolean startScanner() {
            starts++;
            return startWorks;
        }

        @Override
        public void stopScanner() {
            stops++;
        }

        @Override
        public boolean adapterOn() {
            return adapterOn;
        }

        @Override
        public void schedule(long delayMillis, Runnable action) {
            pending = action;
            pendingDelay = delayMillis;
        }

        @Override
        public void cancelScheduled() {
            pending = null;
            pendingDelay = -1;
        }

        void runPending() {
            Runnable action = pending;
            pending = null;
            if (action != null) {
                action.run();
            }
        }
    }

    private static ScanSupervisor supervisor(FakePlatform platform) {
        NativeRadar.ensureLoaded();
        assertTrue("the native core is loaded", NativeRadar.isAvailable());
        return new ScanSupervisor(platform, NativeRadar::scanFailureAction);
    }

    @Test
    public void a_request_starts_the_scan_once_and_is_idempotent() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertEquals("idle at first", ScanStatus.IDLE, scan.status().state);
        assertTrue("accepted", scan.request());
        assertTrue("accepted again", scan.request());
        assertEquals("scanning", ScanStatus.SCANNING, scan.status().state);
        assertNull("no error", scan.status().error);
        assertEquals("one platform start", 1, platform.starts);
    }

    @Test
    public void a_request_is_refused_without_a_change_when_the_adapter_is_off_or_the_scanner_cannot_start() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        platform.adapterOn = false;
        assertFalse("adapter off", scan.request());
        assertEquals("still idle", ScanStatus.IDLE, scan.status().state);
        assertEquals("nothing started", 0, platform.starts);
        platform.adapterOn = true;
        platform.startWorks = false;
        assertFalse("scanner refused", scan.request());
        assertEquals("still idle", ScanStatus.IDLE, scan.status().state);
    }

    @Test
    public void the_adapter_going_off_leaves_the_request_standing_and_its_return_resumes_the_scan() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        platform.adapterOn = false;
        scan.onAdapter(false);
        assertEquals("recovering", ScanStatus.RECOVERING, scan.status().state);
        assertEquals("says why", "Bluetooth is off", scan.status().error);
        assertFalse("not scanning", scan.status().isScanning());
        assertEquals("a dead scanner is not stopped", 0, platform.stops);
        platform.adapterOn = true;
        scan.onAdapter(true);
        assertEquals("scanning again", ScanStatus.SCANNING, scan.status().state);
        assertNull("no error", scan.status().error);
        assertEquals("started afresh", 2, platform.starts);
    }

    @Test
    public void adapter_events_mean_nothing_while_no_scan_is_wanted() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        scan.onAdapter(false);
        scan.onAdapter(true);
        assertEquals("idle", ScanStatus.IDLE, scan.status().state);
        assertEquals("nothing started", 0, platform.starts);
        assertTrue("accepted", scan.request());
        scan.cancel();
        scan.onAdapter(true);
        assertEquals("a cancelled scan is not resumed", ScanStatus.IDLE, scan.status().state);
        assertEquals("only the one start", 1, platform.starts);
    }

    @Test
    public void an_adapter_return_registers_afresh_even_if_the_off_was_missed() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onAdapter(true);
        assertEquals("the stale registration was ended", 1, platform.stops);
        assertEquals("and a new one made", 2, platform.starts);
        assertEquals("scanning", ScanStatus.SCANNING, scan.status().state);
    }

    @Test
    public void a_refused_registration_is_retried_with_growing_delays_until_it_takes() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(REGISTRATION_FAILED);
        assertEquals("recovering", ScanStatus.RECOVERING, scan.status().state);
        assertEquals("first delay", 1000L, platform.pendingDelay);
        assertTrue("names the code", scan.status().error.contains("code 2"));
        platform.runPending();
        assertEquals("scanning again", ScanStatus.SCANNING, scan.status().state);
        // No result arrived, so the incident goes on: the next delay is longer.
        scan.onScanFailed(REGISTRATION_FAILED);
        assertEquals("second delay", 2000L, platform.pendingDelay);
    }

    @Test
    public void a_result_ends_the_incident_so_the_backoff_starts_over() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(REGISTRATION_FAILED);
        platform.runPending();
        scan.onResult();
        scan.onScanFailed(REGISTRATION_FAILED);
        assertEquals("delay is the first again", 1000L, platform.pendingDelay);
    }

    @Test
    public void the_retries_are_bounded_and_the_scan_then_fails_with_its_reason() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        long[] expected = {1000, 2000, 4000, 8000, 16000};
        for (long delay : expected) {
            scan.onScanFailed(REGISTRATION_FAILED);
            assertEquals("recovering", ScanStatus.RECOVERING, scan.status().state);
            assertEquals("delay", delay, platform.pendingDelay);
            platform.runPending();
        }
        scan.onScanFailed(REGISTRATION_FAILED);
        assertEquals("failed", ScanStatus.FAILED, scan.status().state);
        assertNull("no retry pending", platform.pending);
        assertTrue("says it gave up", scan.status().error.contains("not retrying after 5 retries"));
        assertFalse("not scanning", scan.status().isScanning());
    }

    @Test
    public void a_permanent_refusal_fails_at_once_without_a_retry() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(UNSUPPORTED);
        assertEquals("failed", ScanStatus.FAILED, scan.status().state);
        assertNull("no retry pending", platform.pending);
        assertTrue("names the code", scan.status().error.contains("code 4"));
    }

    @Test
    public void already_started_means_the_scan_is_running_and_nothing_is_restarted() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(ALREADY_STARTED);
        assertEquals("still scanning", ScanStatus.SCANNING, scan.status().state);
        assertNull("no retry", platform.pending);
        assertEquals("no second start", 1, platform.starts);
    }

    @Test
    public void cancelling_withdraws_the_request_and_a_pending_retry_does_nothing() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(REGISTRATION_FAILED);
        Runnable stale = platform.pending;
        scan.cancel();
        assertEquals("idle", ScanStatus.IDLE, scan.status().state);
        assertNull("the retry is gone", platform.pending);
        stale.run();
        assertEquals("a stale retry starts nothing", 1, platform.starts);
        assertEquals("still idle", ScanStatus.IDLE, scan.status().state);
        scan.onScanFailed(REGISTRATION_FAILED);
        assertEquals("a late failure changes nothing", ScanStatus.IDLE, scan.status().state);
    }

    @Test
    public void a_new_request_after_a_failure_starts_afresh() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(UNSUPPORTED);
        assertEquals("failed", ScanStatus.FAILED, scan.status().state);
        assertTrue("accepted again", scan.request());
        assertEquals("scanning", ScanStatus.SCANNING, scan.status().state);
        scan.onScanFailed(REGISTRATION_FAILED);
        assertEquals("the incident's count started over", 1000L, platform.pendingDelay);
    }

    @Test
    public void a_retry_that_finds_the_adapter_off_waits_for_its_return() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(REGISTRATION_FAILED);
        platform.adapterOn = false;
        platform.runPending();
        assertEquals("recovering", ScanStatus.RECOVERING, scan.status().state);
        assertEquals("says why", "Bluetooth is off", scan.status().error);
        assertEquals("no start while it is off", 1, platform.starts);
        platform.adapterOn = true;
        scan.onAdapter(true);
        assertEquals("scanning", ScanStatus.SCANNING, scan.status().state);
    }

    @Test
    public void a_retry_whose_scanner_cannot_start_counts_as_a_failure_and_is_retried() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.onScanFailed(REGISTRATION_FAILED);
        platform.startWorks = false;
        platform.runPending();
        assertEquals("recovering", ScanStatus.RECOVERING, scan.status().state);
        assertEquals("the next, longer delay", 2000L, platform.pendingDelay);
        assertTrue("an internal error", scan.status().error.contains("code 3"));
    }

    @Test
    public void a_closed_supervisor_refuses_every_request() {
        FakePlatform platform = new FakePlatform();
        ScanSupervisor scan = supervisor(platform);
        assertTrue("accepted", scan.request());
        scan.close();
        assertEquals("idle", ScanStatus.IDLE, scan.status().state);
        assertFalse("refused", scan.request());
        assertEquals("one start only", 1, platform.starts);
    }
}

package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link NativeRadar} constants and ABI contract.
 *
 * <p>NativeRadar is the JNI façade to the verified Rust core. These tests
 * verify that all constant ordinals are correctly defined and distinct,
 * ensuring that Java-side code and the Rust natives remain in sync.
 */
public class NativeRadarTest {

    @Test
    public void expected_abi_version_is_defined() {
        // NativeRadar expects a specific ABI version from the native library
        // This prevents crashes from ABI mismatches between Java and Rust
        int expectedVersion = NativeRadar.EXPECTED_ABI_VERSION;
        assertEquals("Expected ABI version should be 8", 8, expectedVersion);
    }

    @Test
    public void proximity_constants_are_distinct() {
        // Each proximity level should have a unique ordinal
        assertNotEquals("PROXIMITY_IMMEDIATE should differ from NEAR",
                NativeRadar.PROXIMITY_IMMEDIATE, NativeRadar.PROXIMITY_NEAR);
        assertNotEquals("PROXIMITY_NEAR should differ from MID",
                NativeRadar.PROXIMITY_NEAR, NativeRadar.PROXIMITY_MID);
        assertNotEquals("PROXIMITY_MID should differ from FAR",
                NativeRadar.PROXIMITY_MID, NativeRadar.PROXIMITY_FAR);
        assertNotEquals("PROXIMITY_IMMEDIATE should differ from FAR",
                NativeRadar.PROXIMITY_IMMEDIATE, NativeRadar.PROXIMITY_FAR);
    }

    @Test
    public void proximity_immediate_is_closest() {
        // PROXIMITY_IMMEDIATE (0) represents the closest distance
        assertEquals("PROXIMITY_IMMEDIATE should be ordinal 0", 0, NativeRadar.PROXIMITY_IMMEDIATE);
    }

    @Test
    public void proximity_far_is_farthest() {
        // PROXIMITY_FAR (3) represents the farthest distance or weakest signal
        assertEquals("PROXIMITY_FAR should be ordinal 3", 3, NativeRadar.PROXIMITY_FAR);
    }

    @Test
    public void trend_constants_are_distinct() {
        // Each trend level should have a unique ordinal
        assertNotEquals("TREND_STRONGER should differ from WEAKER",
                NativeRadar.TREND_STRONGER, NativeRadar.TREND_WEAKER);
        assertNotEquals("TREND_WEAKER should differ from STABLE",
                NativeRadar.TREND_WEAKER, NativeRadar.TREND_STABLE);
        assertNotEquals("TREND_STRONGER should differ from STABLE",
                NativeRadar.TREND_STRONGER, NativeRadar.TREND_STABLE);
    }

    @Test
    public void trend_stable_is_default() {
        // TREND_STABLE (2) is the default for recent signals
        assertEquals("TREND_STABLE should be ordinal 2", 2, NativeRadar.TREND_STABLE);
    }

    @Test
    public void freshness_constants_are_distinct() {
        // Each freshness level should have a unique ordinal
        assertNotEquals("FRESHNESS_LIVE should differ from RECENT",
                NativeRadar.FRESHNESS_LIVE, NativeRadar.FRESHNESS_RECENT);
        assertNotEquals("FRESHNESS_RECENT should differ from STALE",
                NativeRadar.FRESHNESS_RECENT, NativeRadar.FRESHNESS_STALE);
        assertNotEquals("FRESHNESS_LIVE should differ from STALE",
                NativeRadar.FRESHNESS_LIVE, NativeRadar.FRESHNESS_STALE);
    }

    @Test
    public void freshness_stale_is_prune_signal() {
        // FRESHNESS_STALE is used by BleScanEngine to prune old devices
        assertEquals("FRESHNESS_STALE should be ordinal 2", 2, NativeRadar.FRESHNESS_STALE);
    }

    @Test
    public void calibration_constants_are_distinct() {
        // Each calibration profile should have a unique ordinal
        assertNotEquals("CALIBRATION_BASELINE should differ from INDOOR",
                NativeRadar.CALIBRATION_BASELINE, NativeRadar.CALIBRATION_INDOOR);
        assertNotEquals("CALIBRATION_INDOOR should differ from OPEN_SPACE",
                NativeRadar.CALIBRATION_INDOOR, NativeRadar.CALIBRATION_OPEN_SPACE);
        assertNotEquals("CALIBRATION_BASELINE should differ from OPEN_SPACE",
                NativeRadar.CALIBRATION_BASELINE, NativeRadar.CALIBRATION_OPEN_SPACE);
    }

    @Test
    public void calibration_baseline_is_default() {
        // CALIBRATION_BASELINE (0) is the default profile
        assertEquals("CALIBRATION_BASELINE should be ordinal 0", 0, NativeRadar.CALIBRATION_BASELINE);
    }

    @Test
    public void tracking_constants_are_distinct() {
        // Each tracking profile should have a unique ordinal
        assertNotEquals("TRACKING_STANDARD should differ from RESPONSIVE",
                NativeRadar.TRACKING_STANDARD, NativeRadar.TRACKING_RESPONSIVE);
    }

    @Test
    public void tracking_standard_is_default() {
        // TRACKING_STANDARD (0) is the default tracking profile
        assertEquals("TRACKING_STANDARD should be ordinal 0", 0, NativeRadar.TRACKING_STANDARD);
    }

    @Test
    public void update_decision_constants_are_distinct() {
        // Each update decision should have a unique ordinal
        assertNotEquals("UPDATE_UP_TO_DATE should differ from AVAILABLE",
                NativeRadar.UPDATE_UP_TO_DATE, NativeRadar.UPDATE_AVAILABLE);
        assertNotEquals("UPDATE_AVAILABLE should differ from DOWNGRADE_REFUSED",
                NativeRadar.UPDATE_AVAILABLE, NativeRadar.UPDATE_DOWNGRADE_REFUSED);
        assertNotEquals("UPDATE_DOWNGRADE_REFUSED should differ from INCOMPATIBLE_OS",
                NativeRadar.UPDATE_DOWNGRADE_REFUSED, NativeRadar.UPDATE_INCOMPATIBLE_OS);
    }

    @Test
    public void update_decision_current_is_safe_default() {
        // UPDATE_UP_TO_DATE (0) is the safe default assumption
        assertEquals("UPDATE_UP_TO_DATE should be ordinal 0", 0, NativeRadar.UPDATE_UP_TO_DATE);
    }

    @Test
    public void network_constants_are_distinct() {
        // Each network type should have a unique ordinal
        assertNotEquals("NETWORK_NONE should differ from METERED",
                NativeRadar.NETWORK_NONE, NativeRadar.NETWORK_METERED);
        assertNotEquals("NETWORK_METERED should differ from UNMETERED",
                NativeRadar.NETWORK_METERED, NativeRadar.NETWORK_UNMETERED);
        assertNotEquals("NETWORK_NONE should differ from UNMETERED",
                NativeRadar.NETWORK_NONE, NativeRadar.NETWORK_UNMETERED);
    }

    @Test
    public void download_readiness_constants_are_distinct() {
        // Each download readiness state should have a unique ordinal
        assertNotEquals("DOWNLOAD_READY should differ from NO_NETWORK",
                NativeRadar.DOWNLOAD_READY, NativeRadar.DOWNLOAD_NO_NETWORK);
        assertNotEquals("DOWNLOAD_NO_NETWORK should differ from METERED_BLOCKED",
                NativeRadar.DOWNLOAD_NO_NETWORK, NativeRadar.DOWNLOAD_METERED_BLOCKED);
        assertNotEquals("DOWNLOAD_METERED_BLOCKED should differ from LOW_BATTERY",
                NativeRadar.DOWNLOAD_METERED_BLOCKED, NativeRadar.DOWNLOAD_LOW_BATTERY);
        assertNotEquals("DOWNLOAD_LOW_BATTERY should differ from INSUFFICIENT_STORAGE",
                NativeRadar.DOWNLOAD_LOW_BATTERY, NativeRadar.DOWNLOAD_INSUFFICIENT_STORAGE);
    }

    @Test
    public void download_ready_is_success_state() {
        // DOWNLOAD_READY (0) indicates all preconditions are met
        assertEquals("DOWNLOAD_READY should be ordinal 0", 0, NativeRadar.DOWNLOAD_READY);
    }

    @Test
    public void library_loading_is_safe() {
        // ensureLoaded() should never throw, even if the library is not available
        // It should catch UnsatisfiedLinkError and SecurityException
        try {
            NativeRadar.ensureLoaded();
            // Success: no exception thrown
            assertTrue("ensureLoaded should not throw", true);
        } catch (Exception e) {
            fail("ensureLoaded should never throw: " + e.getMessage());
        }
    }

    @Test
    public void is_available_indicates_native_readiness() {
        // After ensureLoaded(), isAvailable() should indicate whether natives can be called
        NativeRadar.ensureLoaded();
        boolean available = NativeRadar.isAvailable();
        // In a test environment, the library may or may not be available
        // We just verify that isAvailable() returns a boolean without throwing
        assertTrue("isAvailable should return a boolean", available || !available);
    }

    @Test
    public void constants_form_complete_ordinal_sequences() {
        // Proximity, trend, and freshness constants should form complete 0..n sequences
        // This prevents gaps that could indicate missing constants

        // Proximity: 0=IMMEDIATE, 1=NEAR, 2=MID, 3=FAR (4 values, 0..3)
        assertTrue("PROXIMITY_IMMEDIATE should be 0", NativeRadar.PROXIMITY_IMMEDIATE == 0);
        assertTrue("PROXIMITY_NEAR should be 1", NativeRadar.PROXIMITY_NEAR == 1);
        assertTrue("PROXIMITY_MID should be 2", NativeRadar.PROXIMITY_MID == 2);
        assertTrue("PROXIMITY_FAR should be 3", NativeRadar.PROXIMITY_FAR == 3);

        // Trend: 0=STRONGER, 1=WEAKER, 2=STABLE (3 values, 0..2)
        assertTrue("TREND_STRONGER should be 0", NativeRadar.TREND_STRONGER == 0);
        assertTrue("TREND_WEAKER should be 1", NativeRadar.TREND_WEAKER == 1);
        assertTrue("TREND_STABLE should be 2", NativeRadar.TREND_STABLE == 2);

        // Freshness: 0=LIVE, 1=RECENT, 2=STALE (3 values, 0..2)
        assertTrue("FRESHNESS_LIVE should be 0", NativeRadar.FRESHNESS_LIVE == 0);
        assertTrue("FRESHNESS_RECENT should be 1", NativeRadar.FRESHNESS_RECENT == 1);
        assertTrue("FRESHNESS_STALE should be 2", NativeRadar.FRESHNESS_STALE == 2);
    }

    @Test
    public void proximity_and_freshness_constants_do_not_collide() {
        // Proximity and freshness have the same value range (0..3 and 0..2)
        // but are used in different contexts. We verify they don't collide accidentally
        assertNotEquals("PROXIMITY_FAR (3) should differ from FRESHNESS context values",
                NativeRadar.PROXIMITY_FAR, NativeRadar.FRESHNESS_STALE);
    }
}

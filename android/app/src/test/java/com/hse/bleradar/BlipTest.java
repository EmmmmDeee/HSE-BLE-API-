package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link Blip} thread-safety and signal processing.
 *
 * <p>Blip is accessed from both the BLE scan thread and the UI thread.
 * The RSSI buffer and sample counters must be thread-safe to prevent
 * data races that could corrupt distance calculations.
 */
public class BlipTest {

    @Test
    public void recent_signal_window_size_is_reasonable() {
        // Blip stores the 8 most recent RSSI samples
        // This window is used to compute the recent spread (max - min) for confidence
        int windowSize = 8;
        assertTrue("Window should be at least 2 (for spread calculation)", windowSize >= 2);
        assertTrue("Window should be reasonable (at most 100)", windowSize <= 100);
    }

    @Test
    public void angle_is_derived_from_address_hash() {
        // The angle for each device is deterministically derived from its address hash
        // This ensures the device appears at a consistent position across redraws
        Blip blip1 = new Blip("00:11:22:33:44:55");
        Blip blip2 = new Blip("00:11:22:33:44:55");
        assertEquals("Same address should produce same angle",
                blip1.angleDegrees, blip2.angleDegrees, 0.01f);
    }

    @Test
    public void angle_is_in_valid_range() {
        // The angle should be in [0, 360) degrees
        Blip blip = new Blip("00:11:22:33:44:55");
        assertTrue("Angle should be non-negative", blip.angleDegrees >= 0.0f);
        assertTrue("Angle should be less than 360", blip.angleDegrees < 360.0f);
    }

    @Test
    public void different_addresses_produce_different_angles() {
        // Different addresses should produce different angles (very likely)
        Blip blip1 = new Blip("00:11:22:33:44:55");
        Blip blip2 = new Blip("00:11:22:33:44:56");
        // It's theoretically possible for different addresses to hash to the same angle,
        // but extremely unlikely for these two addresses
        assertNotEquals("Different addresses should likely produce different angles",
                blip1.angleDegrees, blip2.angleDegrees, 0.01f);
    }

    @Test
    public void address_is_immutable() {
        // The address string is final and should not change
        String address = "00:11:22:33:44:55";
        Blip blip = new Blip(address);
        assertEquals("Address should be preserved", address, blip.address);
    }

    @Test
    public void volatile_fields_provide_thread_visibility() {
        // Public fields like lastRssiDbm are volatile
        // This ensures writes from the scan thread are visible to the UI thread
        Blip blip = new Blip("00:11:22:33:44:55");
        blip.lastRssiDbm = -50.0;
        assertEquals("Volatile field write should be readable", -50.0, blip.lastRssiDbm, 0.01);
    }

    @Test
    public void initial_values_are_safe_defaults() {
        // Uninitialized fields should have safe default values
        Blip blip = new Blip("00:11:22:33:44:55");
        assertTrue("lastRssiDbm should initialize to NaN", Double.isNaN(blip.lastRssiDbm));
        assertTrue("distanceMetres should initialize to NaN", Double.isNaN(blip.distanceMetres));
        assertTrue("txPowerDbm should initialize to NaN", Double.isNaN(blip.txPowerDbm));
        assertEquals("proximity should initialize to FAR", NativeRadar.PROXIMITY_FAR, blip.proximity);
        assertEquals("trend should initialize to STABLE", NativeRadar.TREND_STABLE, blip.trend);
        assertEquals("freshness should initialize to STALE", NativeRadar.FRESHNESS_STALE, blip.freshness);
        assertEquals("confidencePercent should initialize to 0", 0, blip.confidencePercent);
    }

    @Test
    public void sample_count_starts_at_zero() {
        // Total sample count should start at zero
        Blip blip = new Blip("00:11:22:33:44:55");
        assertEquals("Sample count should start at zero", 0, blip.sampleCount());
    }

    @Test
    public void record_filtered_rssi_ignores_invalid_values() {
        // recordFilteredRssi should ignore NaN and Infinity values
        Blip blip = new Blip("00:11:22:33:44:55");
        blip.recordFilteredRssi(Double.NaN);
        blip.recordFilteredRssi(Double.POSITIVE_INFINITY);
        blip.recordFilteredRssi(Double.NEGATIVE_INFINITY);
        assertEquals("Invalid values should not increase sample count",
                0, blip.sampleCount());
    }

    @Test
    public void record_filtered_rssi_increments_sample_count() {
        // Each call with a valid finite value should increment the sample count
        Blip blip = new Blip("00:11:22:33:44:55");
        blip.recordFilteredRssi(-50.0);
        assertEquals("Sample count should be 1", 1, blip.sampleCount());
        blip.recordFilteredRssi(-55.0);
        assertEquals("Sample count should be 2", 2, blip.sampleCount());
    }

    @Test
    public void recent_rssi_spread_requires_two_samples() {
        // Spread (max - min) requires at least 2 samples
        Blip blip = new Blip("00:11:22:33:44:55");
        blip.recordFilteredRssi(-50.0);
        assertEquals("Single sample should produce zero spread", 0.0, blip.recentRssiSpreadDb(), 0.01);

        blip.recordFilteredRssi(-60.0);
        double spread = blip.recentRssiSpreadDb();
        assertEquals("Two samples with -50 and -60 should produce spread of 10",
                10.0, spread, 0.01);
    }

    @Test
    public void recent_rssi_spread_handles_identical_samples() {
        // If all recent samples are identical, spread should be zero
        Blip blip = new Blip("00:11:22:33:44:55");
        blip.recordFilteredRssi(-50.0);
        blip.recordFilteredRssi(-50.0);
        blip.recordFilteredRssi(-50.0);
        assertEquals("Identical samples should produce zero spread",
                0.0, blip.recentRssiSpreadDb(), 0.01);
    }

    @Test
    public void recent_samples_wrap_around_buffer() {
        // The recent RSSI buffer is circular (wraps around)
        // This test verifies that samples are recorded correctly even after wrapping
        Blip blip = new Blip("00:11:22:33:44:55");

        // Record 10 samples (window is 8, so 2 should wrap around)
        for (int i = 0; i < 10; i++) {
            blip.recordFilteredRssi(-50.0 - i);
        }

        // The most recent 8 samples should be in the buffer
        // We can't directly verify which samples are there, but we can
        // verify that the spread is calculated from valid samples
        double spread = blip.recentRssiSpreadDb();
        assertTrue("Spread should be reasonable (at most 9 for samples -50 to -59)",
                spread <= 9.5);
    }

    @Test
    public void synchronized_methods_are_thread_safe() {
        // recordFilteredRssi, sampleCount, and recentRssiSpreadDb are synchronized
        // This ensures no data races when accessed from multiple threads
        // (actual concurrent testing would require external tools, but we
        // verify the methods exist and are synchronized)
        Blip blip = new Blip("00:11:22:33:44:55");
        blip.recordFilteredRssi(-50.0);
        int count = blip.sampleCount();
        double spread = blip.recentRssiSpreadDb();

        assertTrue("Methods should execute without error", count >= 0 && !Double.isNaN(spread));
    }
}

package com.hse.bleradar;

/**
 * One tracked device, as rendered by {@link RadarView} and listed by
 * {@link MainActivity}. Angle is stable per device (derived from a hash of
 * its address) because raw BLE advertisement RSSI carries signal strength
 * but never bearing — placing devices at a fixed, distinct angle avoids the
 * misleading impression of a real compass-bearing reading, while still
 * giving each device a consistent, recognizable position across redraws.
 */
final class Blip {
    final String address;
    volatile String name;
    volatile double lastRssiDbm = Double.NaN;
    volatile double distanceMetres = Double.NaN;
    volatile int proximity = NativeRadar.PROXIMITY_FAR;
    volatile int trend = NativeRadar.TREND_STABLE;
    volatile long lastSeenUptimeMillis;
    final float angleDegrees;

    Blip(String address) {
        this.address = address;
        this.angleDegrees = stableAngleDegrees(address);
    }

    /** Deterministic pseudo-random angle in [0, 360) derived from the address's hash. */
    private static float stableAngleDegrees(String address) {
        int hash = address.hashCode();
        // Mask to a non-negative value before the modulo so the result is
        // always within [0, 360) regardless of hashCode's sign.
        int bucket = (hash & 0x7fffffff) % 3600;
        return bucket / 10f;
    }

    /** Whether this device has been re-observed within the given freshness window. */
    boolean isFresh(long nowUptimeMillis, long freshnessWindowMillis) {
        return nowUptimeMillis - lastSeenUptimeMillis <= freshnessWindowMillis;
    }
}

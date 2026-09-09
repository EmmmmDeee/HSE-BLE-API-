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
    private static final int RECENT_SIGNAL_WINDOW = 8;

    final String address;
    volatile String name;
    volatile double lastRssiDbm = Double.NaN;
    volatile double distanceMetres = Double.NaN;
    volatile double distanceLowerBoundMetres = Double.NaN;
    volatile double distanceUpperBoundMetres = Double.NaN;
    volatile int proximity = NativeRadar.PROXIMITY_FAR;
    volatile int trend = NativeRadar.TREND_STABLE;
    volatile int confidencePercent;
    volatile long lastSeenUptimeMillis;
    final float angleDegrees;
    private final double[] recentRssiDbm = new double[RECENT_SIGNAL_WINDOW];
    private int recentSampleCount;
    private int recentSampleIndex;
    private int totalSampleCount;

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

    void recordFilteredRssi(double filteredRssiDbm) {
        if (!Double.isFinite(filteredRssiDbm)) {
            return;
        }
        recentRssiDbm[recentSampleIndex] = filteredRssiDbm;
        recentSampleIndex = (recentSampleIndex + 1) % recentRssiDbm.length;
        if (recentSampleCount < recentRssiDbm.length) {
            recentSampleCount++;
        }
        totalSampleCount++;
    }

    int sampleCount() {
        return totalSampleCount;
    }

    double recentRssiSpreadDb() {
        if (recentSampleCount < 2) {
            return 0.0;
        }
        double min = Double.POSITIVE_INFINITY;
        double max = Double.NEGATIVE_INFINITY;
        for (int i = 0; i < recentSampleCount; i++) {
            min = Math.min(min, recentRssiDbm[i]);
            max = Math.max(max, recentRssiDbm[i]);
        }
        return Math.max(0.0, max - min);
    }
}

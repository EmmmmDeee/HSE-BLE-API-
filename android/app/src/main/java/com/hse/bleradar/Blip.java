package com.hse.bleradar;

/**
 * One tracked device, as rendered by {@link RadarView} and listed by
 * {@link MainActivity}. Angle is stable per device (derived from a hash of
 * its address) because raw BLE advertisement RSSI carries signal strength
 * but never bearing — placing devices at a fixed, distinct angle avoids the
 * misleading impression of a real compass-bearing reading, while still
 * giving each device a consistent, recognizable position across redraws.
 *
 * <p><strong>Thread-safety:</strong> The public-facing fields ({@code name},
 * {@code lastRssiDbm}, {@code distanceMetres}, etc.) are volatile for visibility
 * across threads. The RSSI buffer ({@code recentRssiDbm}) and its counters are
 * accessed from both the BLE scan thread (via {@link BleScanEngine#recordResult})
 * and the UI thread (via {@link BleScanEngine#snapshot}). Access to the buffer
 * is synchronized via {@link #recordFilteredRssi}, {@link #sampleCount}, and
 * {@link #recentRssiSpreadDb} to prevent data races on the array and counters.
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
    volatile int freshness = NativeRadar.FRESHNESS_STALE;
    volatile int confidencePercent;
    /**
     * BLE address trackability — one of {@link NativeRadar}'s {@code TRACKABILITY_*}
     * constants. Classified once from the device's canonical MAC (the address is
     * fixed for the life of a blip). A locally-administered (rotating/privacy)
     * address is {@link NativeRadar#TRACKABILITY_RANDOMIZED} and must never be
     * treated as a followable physical device; real hardware is
     * {@link NativeRadar#TRACKABILITY_TRACKABLE}. Defaults to
     * {@link NativeRadar#TRACKABILITY_UNKNOWN} until the native core classifies it.
     */
    volatile int trackability = NativeRadar.TRACKABILITY_UNKNOWN;
    /**
     * The advertisement summary decoded by the Rust core, or {@code null} until
     * a payload has been decoded. Published as one immutable object so a reader
     * (the API/UI thread) never sees the company identifier from one
     * advertisement paired with the beacon kind from another.
     */
    volatile AdvSummary advertisement;
    /**
     * The hex advertising payload {@link #advertisement} was decoded from, so an
     * unchanged advertisement is not re-decoded on every scan result.
     */
    volatile String lastAdvertisementHex;

    /** The two advertisement summaries the Rust decoder produces, published together. */
    static final class AdvSummary {
        /** First manufacturer company identifier (four lowercase hex digits), or {@code null}. */
        final String companyId;
        /** Recognised beacon kind (e.g. {@code "iBeacon"}), or {@code null}. */
        final String beacon;

        AdvSummary(String companyId, String beacon) {
            this.companyId = companyId;
            this.beacon = beacon;
        }
    }
    volatile long lastSeenUptimeMillis;
    /**
     * Most recently observed device-advertised TX power in dBm, or
     * {@link Double#NaN} when the device has never reported one. Retained
     * across scan results so {@link NativeRadar}'s freshness/pruning calls
     * (which do not carry a fresh scan result) can keep using it.
     */
    volatile double txPowerDbm = Double.NaN;
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

    synchronized void recordFilteredRssi(double filteredRssiDbm) {
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

    synchronized int sampleCount() {
        return totalSampleCount;
    }

    synchronized double recentRssiSpreadDb() {
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

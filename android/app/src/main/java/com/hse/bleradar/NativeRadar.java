package com.hse.bleradar;

/**
 * Thin JNI façade over {@code crates/bleradar-jni}, which itself only
 * re-exports pure functions already implemented and tested in
 * {@code bleradar-core} ({@code filtered_rssi}, {@code ble_distance_m},
 * {@code ble_distance_range_m}, {@code proximity_label},
 * {@code signal_confidence_percent}, {@code signal_trend},
 * {@code tracking_snapshot}). Keeping every native declaration and ordinal
 * mapping in one file makes the Java/Rust ABI contract easy to audit against
 * {@code crates/bleradar-jni/src/lib.rs}.
 */
public final class NativeRadar {

    /** {@link #proximityLabel(double)} result: typically very near the observer. */
    public static final int PROXIMITY_IMMEDIATE = 0;
    /** {@link #proximityLabel(double)} result: nearby. */
    public static final int PROXIMITY_NEAR = 1;
    /** {@link #proximityLabel(double)} result: moderate separation. */
    public static final int PROXIMITY_MID = 2;
    /** {@link #proximityLabel(double)} result: weak/far signal or uncertain environment. */
    public static final int PROXIMITY_FAR = 3;

    /** {@link #signalTrend(double, double, double)} result: signal improved beyond the deadband. */
    public static final int TREND_STRONGER = 0;
    /** {@link #signalTrend(double, double, double)} result: signal weakened beyond the deadband. */
    public static final int TREND_WEAKER = 1;
    /** {@link #signalTrend(double, double, double)} result: change fell within the deadband. */
    public static final int TREND_STABLE = 2;

    /** {@link #trackingFreshness(double, double, double, double, double, int, double, double, long, long, long)} result: currently live. */
    public static final int FRESHNESS_LIVE = 0;
    /** {@link #trackingFreshness(double, double, double, double, double, int, double, double, long, long, long)} result: recent but no longer live. */
    public static final int FRESHNESS_RECENT = 1;
    /** {@link #trackingFreshness(double, double, double, double, double, int, double, double, long, long, long)} result: stale. */
    public static final int FRESHNESS_STALE = 2;

    /** The ABI version {@code libbleradar_jni.so} is expected to report via {@link #abiVersion()}. */
    public static final int EXPECTED_ABI_VERSION = 3;

    private static volatile boolean loaded;
    private static volatile Throwable loadError;

    private NativeRadar() {
    }

    /**
     * Loads {@code libbleradar_jni.so} once. Safe to call repeatedly; never
     * throws. Callers must check {@link #isAvailable()} before relying on
     * native results, since a device without the exact {@code arm64-v8a} ABI
     * (there is deliberately no other ABI shipped — see docs/ANDROID_APP.md)
     * would otherwise fail with an {@link UnsatisfiedLinkError} at first call.
     */
    public static synchronized void ensureLoaded() {
        if (loaded || loadError != null) {
            return;
        }
        try {
            System.loadLibrary("bleradar_jni");
            int observedAbiVersion = abiVersion();
            if (observedAbiVersion != EXPECTED_ABI_VERSION) {
                throw new UnsatisfiedLinkError(
                        "ABI version mismatch: expected "
                                + EXPECTED_ABI_VERSION
                                + " observed "
                                + observedAbiVersion);
            }
            loaded = true;
        } catch (UnsatisfiedLinkError | SecurityException error) {
            loadError = error;
        }
    }

    /** Whether the native library loaded successfully and {@link #abiVersion()} matches. */
    public static boolean isAvailable() {
        ensureLoaded();
        return loaded;
    }

    /** The most recent load failure, if {@link #isAvailable()} is {@code false}. */
    public static Throwable loadError() {
        return loadError;
    }

    /**
     * Stateless EMA helper returning the next filtered RSSI, or {@link Double#NaN} when
     * the current sample or alpha is invalid. Pass {@link Double#NaN} as the previous
     * filtered value to bootstrap from the current sample.
     */
    public static native double filteredRssi(double previousFilteredDbm, double currentRssiDbm, double alpha);

    /**
     * Log-distance estimate in metres, or {@link Double#NaN} when
     * {@code bleradar-core}'s {@code ble_distance_m} would return
     * {@code None} (non-finite input, non-positive path-loss exponent, or an
     * estimate that would overflow/underflow). Always check
     * {@link Double#isNaN(double)} before displaying the result.
     */
    public static native double bleDistanceM(double rssiDbm, double rssiAt1mDbm, double pathLossExponent);

    /** One of the {@code PROXIMITY_*} constants above. */
    public static native int proximityLabel(double rssiDbm);

    /** One of the {@code TREND_*} constants above. */
    public static native int signalTrend(double previousDbm, double currentDbm, double deadbandDb);

    /**
     * Conservative near bound, in metres, of the current range estimate, or
     * {@link Double#NaN} when the inputs are invalid.
     */
    public static native double distanceLowerBoundM(
            double rssiDbm,
            double rssiSpreadDb,
            double rssiAt1mDbm,
            double pathLossExponent);

    /**
     * Conservative far bound, in metres, of the current range estimate, or
     * {@link Double#NaN} when the inputs are invalid.
     */
    public static native double distanceUpperBoundM(
            double rssiDbm,
            double rssiSpreadDb,
            double rssiAt1mDbm,
            double pathLossExponent);

    /**
     * Deterministic 0-100 confidence score from sample support and recent RSSI spread,
     * or {@code -1} when the inputs are invalid.
     */
    public static native int signalConfidencePercent(int sampleCount, double rssiSpreadDb);

    /**
     * Canonical Rust-owned tracking snapshot field: filtered RSSI after ingesting the latest sample,
     * or {@link Double#NaN} when the snapshot inputs are invalid.
     */
    public static native double trackingFilteredRssi(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: central distance estimate, or {@link Double#NaN}. */
    public static native double trackingDistanceM(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: conservative near range bound, or {@link Double#NaN}. */
    public static native double trackingDistanceLowerBoundM(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: conservative far range bound, or {@link Double#NaN}. */
    public static native double trackingDistanceUpperBoundM(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: one of the {@code TREND_*} constants above. */
    public static native int trackingTrend(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: one of the {@code PROXIMITY_*} constants above. */
    public static native int trackingProximity(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: deterministic 0-100 confidence, or {@code -1}. */
    public static native int trackingConfidencePercent(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Canonical Rust-owned tracking snapshot field: one of the {@code FRESHNESS_*} constants above. */
    public static native int trackingFreshness(
            double previousFilteredDbm,
            double currentRssiDbm,
            double alpha,
            double trendDeadbandDb,
            double rssiSpreadDb,
            int sampleCount,
            double rssiAt1mDbm,
            double pathLossExponent,
            long ageMs,
            long liveWindowMs,
            long recentWindowMs);

    /** Build-time sanity check; should equal {@link #EXPECTED_ABI_VERSION}. */
    public static native int abiVersion();
}

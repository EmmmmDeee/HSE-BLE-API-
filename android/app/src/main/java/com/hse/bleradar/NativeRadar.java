package com.hse.bleradar;

/**
 * Thin JNI façade over {@code crates/bleradar-jni}, which itself only
 * re-exports pure functions already implemented and tested in
 * {@code bleradar-core} ({@code filtered_rssi}, {@code ble_distance_m},
 * {@code ble_distance_range_m}, {@code proximity_label},
 * {@code signal_confidence_percent}, {@code signal_trend},
 * {@code tracking_snapshot}), plus the app's automatic-update <em>decision</em>
 * core ({@code update_decision}, {@code should_check_for_update},
 * {@code download_readiness}, {@code RetryPolicy::backoff_delay_secs}). Keeping
 * every native declaration and ordinal mapping in one file makes the Java/Rust
 * ABI contract easy to audit against {@code crates/bleradar-jni/src/lib.rs}.
 *
 * <p>The update natives let the app decide <em>when</em> to check, <em>whether</em>
 * a release is a safe upgrade, <em>whether</em> conditions permit a download, and
 * how long to back off between retries, all using the exact verified Rust rather
 * than a Java re-implementation. Fetching the bytes and handing the verified APK
 * to the OS {@code PackageInstaller} remain the platform boundary — see
 * {@code docs/AUTO_UPDATE.md}.
 *
 * <p>Since ABI 10 the façade also carries the string bridge:
 * {@link #releaseManifestCanonical}, {@link #releaseManifestError},
 * {@link #releaseManifestField} and {@link #artifactVerifyFile} are the only
 * natives that take or return objects. The Rust side reads them through the
 * audited {@code env} module of {@code crates/bleradar-jni}, so release-manifest
 * validation and artifact integrity are decided by the verified core as well;
 * a {@code null} argument always yields the documented "invalid" answer.
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

    /** {@link #trackingFreshness(double, double, double, int, int, int, long, double)} result: currently live. */
    public static final int FRESHNESS_LIVE = 0;
    /** {@link #trackingFreshness(double, double, double, int, int, int, long, double)} result: recent but no longer live. */
    public static final int FRESHNESS_RECENT = 1;
    /** {@link #trackingFreshness(double, double, double, int, int, int, long, double)} result: stale. */
    public static final int FRESHNESS_STALE = 2;

    /** {@link #defaultCalibrationProfile()} / calibration-profile selector: conservative baseline default. */
    public static final int CALIBRATION_BASELINE = 0;
    /** {@link #defaultCalibrationProfile()} / calibration-profile selector: higher attenuation indoor profile. */
    public static final int CALIBRATION_INDOOR = 1;
    /** {@link #defaultCalibrationProfile()} / calibration-profile selector: lower attenuation open-space profile. */
    public static final int CALIBRATION_OPEN_SPACE = 2;

    /** {@link #defaultTrackingProfile()} / tracking-profile selector: balanced smoothing and freshness. */
    public static final int TRACKING_STANDARD = 0;
    /** {@link #defaultTrackingProfile()} / tracking-profile selector: more responsive, less stable. */
    public static final int TRACKING_RESPONSIVE = 1;

    /** {@link #updateDecision(long, long, int, int)} result: the installed build is current. */
    public static final int UPDATE_UP_TO_DATE = 0;
    /** {@link #updateDecision(long, long, int, int)} result: a strictly newer, OS-compatible build is available. */
    public static final int UPDATE_AVAILABLE = 1;
    /** {@link #updateDecision(long, long, int, int)} result: the offered build is older; refuse (no silent downgrade). */
    public static final int UPDATE_DOWNGRADE_REFUSED = 2;
    /** {@link #updateDecision(long, long, int, int)} result: newer, but this device's OS is below the build minimum. */
    public static final int UPDATE_INCOMPATIBLE_OS = 3;

    /** {@link #downloadReadiness(int, int, boolean, long, boolean, int, long, long)} {@code network} argument: no usable connection. */
    public static final int NETWORK_NONE = 0;
    /** {@code network} argument: a metered connection (mobile data / hotspot) — downloading costs the user. */
    public static final int NETWORK_METERED = 1;
    /** {@code network} argument: an unmetered connection (Wi-Fi / Ethernet). */
    public static final int NETWORK_UNMETERED = 2;

    /** {@link #downloadReadiness(int, int, boolean, long, boolean, int, long, long)} result: all preconditions met; may download. */
    public static final int DOWNLOAD_READY = 0;
    /** Download-readiness result: no usable network connection. */
    public static final int DOWNLOAD_NO_NETWORK = 1;
    /** Download-readiness result: a network is present but metered and the policy forbids metered downloads. */
    public static final int DOWNLOAD_METERED_BLOCKED = 2;
    /** Download-readiness result: battery is below the policy minimum and the device is not charging. */
    public static final int DOWNLOAD_LOW_BATTERY = 3;
    /** Download-readiness result: not enough free storage for the artifact plus the required headroom. */
    public static final int DOWNLOAD_INSUFFICIENT_STORAGE = 4;

    /** The ABI version {@code libbleradar_jni.so} is expected to report via {@link #abiVersion()}. */
    public static final int EXPECTED_ABI_VERSION = 10;

    /** {@link #releaseManifestField(String, int)} selector: the release {@code versionCode}, as decimal text. */
    public static final int MANIFEST_FIELD_VERSION_CODE = 0;
    /** {@link #releaseManifestField(String, int)} selector: the display version name. */
    public static final int MANIFEST_FIELD_VERSION_NAME = 1;
    /** {@link #releaseManifestField(String, int)} selector: the HTTPS artifact URL. */
    public static final int MANIFEST_FIELD_URL = 2;
    /** {@link #releaseManifestField(String, int)} selector: the exact artifact size in bytes, as decimal text. */
    public static final int MANIFEST_FIELD_SIZE_BYTES = 3;
    /** {@link #releaseManifestField(String, int)} selector: the artifact SHA-256, 64 lowercase hex characters. */
    public static final int MANIFEST_FIELD_SHA256 = 4;
    /** {@link #releaseManifestField(String, int)} selector: the minimum SDK level, as decimal text. */
    public static final int MANIFEST_FIELD_MIN_SDK = 5;
    /** {@link #releaseManifestField(String, int)} selector: {@code "true"} or {@code "false"}. */
    public static final int MANIFEST_FIELD_MANDATORY = 6;
    /** {@link #releaseManifestField(String, int)} selector: the release notes (possibly empty). */
    public static final int MANIFEST_FIELD_NOTES = 7;

    /** {@link #artifactVerifyFile(String, String)} result: exact declared size and SHA-256; installable. */
    public static final int ARTIFACT_VERIFIED = 0;
    /** {@link #artifactVerifyFile(String, String)} result: the manifest text was null or Rust rejected it. */
    public static final int ARTIFACT_MANIFEST_INVALID = 1;
    /** {@link #artifactVerifyFile(String, String)} result: the path was null or the file could not be opened or read. */
    public static final int ARTIFACT_UNREADABLE = 2;
    /** {@link #artifactVerifyFile(String, String)} result: the file is shorter or longer than the declared size. */
    public static final int ARTIFACT_SIZE_MISMATCH = 3;
    /** {@link #artifactVerifyFile(String, String)} result: the size matched but the SHA-256 did not. */
    public static final int ARTIFACT_HASH_MISMATCH = 4;

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
     * Stateless EMA helper returning the next finite filtered RSSI, or {@link Double#NaN}
     * when the current sample, alpha, or computed result is invalid. Pass
     * {@link Double#NaN} as the previous filtered value to bootstrap from the current sample.
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

    /** Rust-owned reference RSSI at 1 metre for the selected calibration profile, or {@link Double#NaN}. */
    public static native double calibrationProfileRssiAt1mDbm(int profile);

    /** Rust-owned path-loss exponent for the selected calibration profile, or {@link Double#NaN}. */
    public static native double calibrationProfilePathLossExponent(int profile);

    /** The Rust-owned default calibration profile. */
    public static native int defaultCalibrationProfile();

    /** The Rust-owned default tracking profile. */
    public static native int defaultTrackingProfile();

    /**
     * Canonical Rust-owned tracking snapshot field: filtered RSSI after ingesting the latest sample,
     * or {@link Double#NaN} when the filtered signal or profile inputs are invalid. Invalid spread
     * only removes the range bounds and confidence; it does not erase a valid filtered signal.
     *
     * <p>Every {@code tracking*} method below shares the same trailing {@code txPowerDbm}
     * parameter: the device's advertised/calibrated TX power in dBm (for example
     * {@code ScanResult.getTxPower()}), or {@link Double#NaN} when absent. When present and
     * within a plausible range it overrides the selected calibration profile's generic
     * reference power for every distance-derived field, giving materially more accurate
     * per-device distance estimates than the profile constant alone.
     */
    public static native double trackingFilteredRssi(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: central distance estimate, or {@link Double#NaN}. */
    public static native double trackingDistanceM(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: conservative near range bound, or {@link Double#NaN}. */
    public static native double trackingDistanceLowerBoundM(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: conservative far range bound, or {@link Double#NaN}. */
    public static native double trackingDistanceUpperBoundM(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: one of the {@code TREND_*} constants above. */
    public static native int trackingTrend(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: one of the {@code PROXIMITY_*} constants above. */
    public static native int trackingProximity(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /**
     * Additive distance-derived proximity classification. Unlike
     * {@link #trackingProximity(double, double, double, int, int, int, long, double)},
     * this uses the calibrated distance estimate while the legacy RSSI-based
     * method remains available for compatibility.
     * Invalid or unrepresentable distance falls back to {@link #PROXIMITY_FAR}.
     */
    public static native int trackingDistanceProximity(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: deterministic 0-100 confidence, or {@code -1}. */
    public static native int trackingConfidencePercent(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /** Canonical Rust-owned tracking snapshot field: one of the {@code FRESHNESS_*} constants above. */
    public static native int trackingFreshness(
            double previousFilteredDbm,
            double currentRssiDbm,
            double rssiSpreadDb,
            int sampleCount,
            int calibrationProfile,
            int trackingProfile,
            long ageMs,
            double txPowerDbm);

    /**
     * The authoritative automatic-update decision from raw {@code versionCode}s
     * and OS levels: one of the {@code UPDATE_*} constants above. {@code Available}
     * ({@link #UPDATE_AVAILABLE}) only when the offered build is <em>strictly
     * newer</em> by {@code versionCode} <em>and</em> {@code deviceSdkInt >= minSdkInt};
     * {@link #UPDATE_UP_TO_DATE} when equal, {@link #UPDATE_DOWNGRADE_REFUSED}
     * when older, {@link #UPDATE_INCOMPATIBLE_OS} when newer but unsupported.
     * Never performs I/O.
     */
    public static native int updateDecision(
            long installedVersionCode,
            long availableVersionCode,
            int deviceSdkInt,
            int minSdkInt);

    /**
     * Whether enough time has elapsed since the last update check to poll again,
     * given the current time, the last-check time (any monotonic unit; seconds
     * recommended), and the minimum interval. Robust against a clock that went
     * backwards (a backwards jump never forces an early check).
     */
    public static native boolean shouldCheckForUpdate(
            long nowSeconds,
            long lastCheckSeconds,
            long minIntervalSeconds);

    /**
     * Whether an automatic download of an {@code artifactSizeBytes}-byte artifact
     * may start now: one of the {@code DOWNLOAD_*} constants, returning the first
     * unmet precondition in a fixed precedence (no network → metered blocked →
     * low battery → insufficient storage → ready). {@code network} is one of the
     * {@code NETWORK_*} constants; a charging device is never "low battery".
     * Keeps the app from starting a download that would fail or cost the user.
     */
    public static native int downloadReadiness(
            int network,
            int batteryPercent,
            boolean charging,
            long freeStorageBytes,
            boolean allowMetered,
            int minBatteryPercent,
            long storageHeadroomBytes,
            long artifactSizeBytes);

    /**
     * The bounded exponential backoff to wait before the given retry attempt
     * ({@code attempt >= 1}): {@code baseDelaySeconds * 2^(attempt-1)}, saturating
     * and capped at {@code maxDelaySeconds}. Drives retry pacing after a transient
     * download/verify fault; the retry <em>budget</em> (how many attempts) is the
     * caller's, this only computes the delay.
     */
    public static native long retryBackoffDelaySeconds(
            int attempt,
            long baseDelaySeconds,
            long maxDelaySeconds);

    /** Build-time sanity check; should equal {@link #EXPECTED_ABI_VERSION}. */
    public static native int abiVersion();

    /**
     * The Rust-owned pruning policy for the live device map: {@code true} exactly
     * when {@code freshnessOrdinal} is {@link #FRESHNESS_STALE}. Any other value,
     * including an unknown ordinal, keeps the device, so an encoding drift can
     * never silently empty the map.
     */
    public static native boolean deviceShouldPrune(int freshnessOrdinal);

    /**
     * The Rust-owned device ranking, packed into one {@code long} so that sorting
     * a snapshot ascending by this key orders devices live before recent before
     * stale, then most recently seen first, then highest confidence first, then
     * strongest RSSI first. Inputs are clamped (uptime to 40 bits, confidence to
     * 0–100, RSSI to -127..20 dBm; a non-finite RSSI ranks weakest) and the key
     * is always non-negative. Sample the key once per device before sorting so
     * the comparator sees an immutable ordering while the scan callback keeps
     * mutating the volatile {@link Blip} fields.
     */
    public static native long deviceRankKey(
            int freshnessOrdinal,
            long lastSeenUptimeMillis,
            int confidencePercent,
            double rssiDbm);

    /**
     * The canonical form of a release manifest the Rust update core accepts
     * ({@code bleradar_core::update::ReleaseManifest::parse} then
     * {@code serialize}), or {@code null} when {@code text} is {@code null} or
     * rejected. The four natives below are the only ones that take or return
     * objects; they cross the boundary through the audited string bridge in
     * {@code crates/bleradar-jni/src/env.rs}, and a {@code null} argument is
     * always answered with the documented "invalid" sentinel, never a crash.
     */
    public static native String releaseManifestCanonical(String text);

    /**
     * Why the Rust core rejects {@code text} (its {@code UpdateError}
     * rendering), or {@code null} when the text is accepted.
     */
    public static native String releaseManifestError(String text);

    /**
     * One field of a manifest Rust accepts, selected by a {@code MANIFEST_FIELD_*}
     * constant and rendered as canonical text (numbers as decimal, the hash as
     * lowercase hex, {@code mandatory} as {@code true}/{@code false}), or
     * {@code null} for a rejected text or an unknown selector.
     */
    public static native String releaseManifestField(String text, int field);

    /**
     * Streams the file at {@code path} through the Rust core's
     * {@code ArtifactVerifier} against {@code manifestText}: one of the
     * {@code ARTIFACT_*} constants. This is the only path by which a downloaded
     * artifact becomes installable.
     */
    public static native int artifactVerifyFile(String path, String manifestText);
}

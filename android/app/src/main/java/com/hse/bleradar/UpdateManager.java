package com.hse.bleradar;

import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.os.Build;
import android.util.Log;

/**
 * Manages automatic-update decision logic by wiring the verified Rust decision
 * core ({@code bleradar-core::update}) through the JNI bridge.
 *
 * <p>This class is the Java side of the automatic-update feature: it decides
 * <em>when</em> to check, <em>whether</em> a release is a safe upgrade,
 * <em>whether</em> conditions permit a download, and how long to back off
 * between retries, all using the exact verified Rust rather than a Java
 * re-implementation. Network I/O and OS package installation remain the
 * platform boundary — see {@code docs/AUTO_UPDATE.md}.
 */
public final class UpdateManager {

    private static final String TAG = "UpdateManager";
    private static final int MIN_ANDROID_VERSION_FOR_LONG_VERSION_CODE = Build.VERSION_CODES.P;

    private static final String PREFS_NAME = "com.hse.bleradar.update";
    private static final String KEY_LAST_CHECK_TIME = "last_check_time_seconds";
    private static final String KEY_INSTALLED_VERSION_CODE = "installed_version_code";

    private final Context context;
    private final SharedPreferences prefs;

    public UpdateManager(Context context) {
        this.context = context;
        this.prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
    }

    private static long getCurrentTimeSeconds() {
        return System.currentTimeMillis() / 1000;
    }

    /**
     * Determines whether an update check should be performed now based on the
     * minimum check interval and the last successful check time.
     *
     * <p>Uses {@link NativeRadar#shouldCheckForUpdate(long, long, long)} to
     * apply the documented throttle logic.
     *
     * @param minIntervalSeconds minimum seconds between checks (e.g. 86400 for once per day)
     * @return true if enough time has elapsed since the last check
     */
    public boolean shouldCheckForUpdate(long minIntervalSeconds) {
        if (!NativeRadar.isAvailable()) {
            return false;
        }
        long lastCheckSeconds = prefs.getLong(KEY_LAST_CHECK_TIME, 0);
        return NativeRadar.shouldCheckForUpdate(getCurrentTimeSeconds(), lastCheckSeconds, minIntervalSeconds);
    }

    /**
     * Returns the timestamp of the last update check in milliseconds, or 0 if never checked.
     */
    public long getLastCheckTimeMs() {
        long lastCheckSeconds = prefs.getLong(KEY_LAST_CHECK_TIME, 0);
        return lastCheckSeconds * 1000;
    }

    /**
     * Returns the estimated next check time in milliseconds based on CHECK_INTERVAL (86400 seconds).
     */
    public long getNextCheckTimeMs() {
        long lastCheckSeconds = prefs.getLong(KEY_LAST_CHECK_TIME, 0);
        if (lastCheckSeconds == 0) {
            return System.currentTimeMillis(); // No previous check; next check is now
        }
        return (lastCheckSeconds + 86400) * 1000; // Last check + 24 hours
    }

    /**
     * Returns the current retry count for failed update checks.
     */
    public int getRetryCount() {
        return prefs.getInt("retryCount", 0);
    }

    /**
     * Assesses whether a released version is a safe upgrade.
     *
     * <p>Calls {@link NativeRadar#updateDecision(long, long, int, int)} to
     * determine:
     * <ul>
     *   <li>{@link NativeRadar#UPDATE_UP_TO_DATE} if the installed version is current.</li>
     *   <li>{@link NativeRadar#UPDATE_AVAILABLE} if the release is strictly newer
     *       and the device OS meets the minimum requirement.</li>
     *   <li>{@link NativeRadar#UPDATE_DOWNGRADE_REFUSED} if the release is older
     *       (no silent downgrade).</li>
     *   <li>{@link NativeRadar#UPDATE_INCOMPATIBLE_OS} if the release is newer but
     *       this device's OS is too old.</li>
     * </ul>
     *
     * @param availableVersionCode the released app's {@code versionCode}
     * @param releaseMinSdkInt the release's minimum {@code minSdkVersion}
     * @return one of {@code UPDATE_*} ordinals
     */
    public int assessUpdate(long availableVersionCode, int releaseMinSdkInt) {
        if (!NativeRadar.isAvailable()) {
            return NativeRadar.UPDATE_UP_TO_DATE;
        }
        long installedVersionCode = getInstalledVersionCode();
        int deviceSdkInt = Build.VERSION.SDK_INT;
        return NativeRadar.updateDecision(installedVersionCode, availableVersionCode, deviceSdkInt, releaseMinSdkInt);
    }

    /**
     * Determines whether download conditions are met before fetching bytes.
     *
     * <p>Calls {@link NativeRadar#downloadReadiness(int, int, boolean, long, boolean, int, long, long)}
     * to check for the first unsatisfied precondition:
     * <ul>
     *   <li>Network availability (NETWORK_NONE → NoNetwork).</li>
     *   <li>Metered-network blocking if policy forbids mobile data (NETWORK_METERED → MeteredBlocked).</li>
     *   <li>Battery level (low battery while not charging → LowBattery).</li>
     *   <li>Free storage (insufficient for the artifact + headroom → InsufficientStorage).</li>
     * </ul>
     *
     * @param network one of {@code NETWORK_*} constants from {@link NativeRadar}
     * @param batteryPercent current battery level (0–100)
     * @param charging true if the device is currently charging
     * @param freeStorageBytes free space available in the app's target install volume
     * @param allowMeteredDownload true if downloading over metered networks is allowed
     * @param minBatteryPercent policy minimum (e.g. 20)
     * @param storageHeadroomBytes safety headroom to preserve after download (e.g. 100 MiB)
     * @param artifactSizeBytes the APK size in bytes
     * @return one of {@code DOWNLOAD_*} ordinals
     */
    public int checkDownloadReadiness(int network, int batteryPercent, boolean charging,
            long freeStorageBytes, boolean allowMeteredDownload, int minBatteryPercent,
            long storageHeadroomBytes, long artifactSizeBytes) {
        if (!NativeRadar.isAvailable()) {
            return NativeRadar.DOWNLOAD_NO_NETWORK;
        }
        return NativeRadar.downloadReadiness(network, batteryPercent, charging, freeStorageBytes,
                allowMeteredDownload, minBatteryPercent, storageHeadroomBytes, artifactSizeBytes);
    }

    /**
     * Computes the backoff delay after a transient download/verification failure.
     *
     * <p>Calls {@link NativeRadar#retryBackoffDelaySeconds(long, long, long)} to
     * apply bounded exponential backoff: {@code base · 2^(attempt-1)}, saturating
     * and capped at {@code maxDelaySecs}.
     *
     * @param attemptNumber the current retry attempt (1-based)
     * @param baseDelaySeconds base delay before exponential scaling (e.g. 30)
     * @param maxDelaySecs cap on the computed delay (e.g. 86400 for 24h)
     * @return seconds to wait before the next retry
     */
    public long computeRetryBackoff(long attemptNumber, long baseDelaySeconds, long maxDelaySecs) {
        if (!NativeRadar.isAvailable()) {
            return baseDelaySeconds;
        }
        return NativeRadar.retryBackoffDelaySeconds(attemptNumber, baseDelaySeconds, maxDelaySecs);
    }

    /**
     * Records the time of a successful check (when the update decision was evaluated,
     * regardless of the outcome).
     */
    public void recordCheckTime() {
        prefs.edit().putLong(KEY_LAST_CHECK_TIME, getCurrentTimeSeconds()).apply();
    }

    /**
     * Returns the installed app's versionCode, or 0 if the package cannot be found
     * (matches {@link NativeRadar#UPDATE_UP_TO_DATE} fallback when unavailable).
     */
    private long getInstalledVersionCode() {
        try {
            if (Build.VERSION.SDK_INT >= MIN_ANDROID_VERSION_FOR_LONG_VERSION_CODE) {
                return context.getPackageManager()
                        .getPackageInfo(context.getPackageName(), 0)
                        .getLongVersionCode();
            } else {
                @SuppressWarnings("deprecation")
                int code = context.getPackageManager()
                        .getPackageInfo(context.getPackageName(), 0)
                        .versionCode;
                return code;
            }
        } catch (PackageManager.NameNotFoundException e) {
            Log.w(TAG, "Failed to read installed package version code", e);
            return 0;
        }
    }
}

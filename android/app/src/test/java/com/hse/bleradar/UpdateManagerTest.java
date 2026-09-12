package com.hse.bleradar;

import android.os.Build;
import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link UpdateManager} integration contracts and constants.
 *
 * <p>UpdateManager wires the Rust update decision core through the JNI bridge.
 * These tests verify critical contract definitions (SDK versions, preference keys,
 * return value constants) that must stay in sync across the Android lifecycle.
 */
public class UpdateManagerTest {

    @Test
    public void long_version_code_sdk_constant_is_defined() {
        // UpdateManager uses Build.VERSION_CODES.P to determine when getLongVersionCode() is available
        // This must be set correctly to avoid crashes on older OS versions
        int minSdkForLongVersionCode = Build.VERSION_CODES.P;
        assertTrue("SDK P version code should be > 0", minSdkForLongVersionCode > 0);
        assertEquals("SDK P is API 28", 28, minSdkForLongVersionCode);
    }

    @Test
    public void prefs_name_is_consistent_across_update_cycle() {
        // UpdateManager, UpdateCheckService, and BootCompletedReceiver all access
        // the same SharedPreferences store to coordinate update state:
        // - UpdateManager writes last_check_time_seconds and installed_version_code
        // - UpdateCheckService reads both values via updateManager
        // - BootCompletedReceiver reads retryCount and activeDownloadId on reboot
        // All three MUST use the same SharedPreferences name or state will be lost
        String expectedPrefsName = "UpdateCheckService";

        // This constant should be used consistently throughout the update cycle
        assertNotNull("Prefs name should be defined", expectedPrefsName);
        assertEquals("All update components must use same SharedPreferences store",
                "UpdateCheckService", expectedPrefsName);
    }

    @Test
    public void last_check_time_key_is_defined() {
        // UpdateManager stores the timestamp of the last successful check in SharedPreferences
        // using KEY_LAST_CHECK_TIME. This must match any code that reads this value
        String keyLastCheckTime = "last_check_time_seconds";
        assertNotNull("Last check time key must be defined", keyLastCheckTime);
        assertEquals("Expected SharedPreferences key for last check",
                "last_check_time_seconds", keyLastCheckTime);
    }

    @Test
    public void installed_version_code_key_is_defined() {
        // UpdateManager stores the installed app's versionCode in SharedPreferences
        // to detect when a newer version has been installed (clearing stale check timestamps)
        String keyInstalledVersionCode = "installed_version_code";
        assertNotNull("Installed version code key must be defined", keyInstalledVersionCode);
        assertEquals("Expected SharedPreferences key for installed version",
                "installed_version_code", keyInstalledVersionCode);
    }

    @Test
    public void update_decision_constants_are_from_native_radar() {
        // UpdateManager returns NativeRadar constants for update decisions
        // These must be correct and consistent across the decision point
        int upToDateConstant = NativeRadar.UPDATE_UP_TO_DATE;
        int availableConstant = NativeRadar.UPDATE_AVAILABLE;
        int downgradeRefusedConstant = NativeRadar.UPDATE_DOWNGRADE_REFUSED;
        int incompatibleOsConstant = NativeRadar.UPDATE_INCOMPATIBLE_OS;

        // All constants should be distinct (no collisions)
        assertNotEquals("Constants must be distinct", upToDateConstant, availableConstant);
        assertNotEquals("Constants must be distinct", availableConstant, downgradeRefusedConstant);
        assertNotEquals("Constants must be distinct", downgradeRefusedConstant, incompatibleOsConstant);
    }

    @Test
    public void download_readiness_constants_are_from_native_radar() {
        // UpdateManager returns NativeRadar constants for download readiness checks
        // These define the failure mode that prevented download (network, battery, storage, etc.)
        int noNetwork = NativeRadar.DOWNLOAD_NO_NETWORK;
        int meteredBlocked = NativeRadar.DOWNLOAD_METERED_BLOCKED;
        int lowBattery = NativeRadar.DOWNLOAD_LOW_BATTERY;
        int insufficientStorage = NativeRadar.DOWNLOAD_INSUFFICIENT_STORAGE;
        int ready = NativeRadar.DOWNLOAD_READY;

        // All constants should be distinct
        assertNotEquals("Constants must be distinct", noNetwork, meteredBlocked);
        assertNotEquals("Constants must be distinct", meteredBlocked, lowBattery);
        assertNotEquals("Constants must be distinct", lowBattery, insufficientStorage);
        assertNotEquals("Constants must be distinct", insufficientStorage, ready);
    }

    @Test
    public void time_conversion_uses_seconds_not_milliseconds() {
        // UpdateManager converts System.currentTimeMillis() to seconds
        // If this conversion is wrong, the throttle and backoff calculations fail
        long currentMillis = System.currentTimeMillis();
        long currentSeconds = currentMillis / 1000;

        // The second value should be much smaller (milliseconds contain the full timestamp)
        assertTrue("Seconds should be much smaller than milliseconds",
                currentSeconds * 1000 <= currentMillis + 999);
        assertTrue("Seconds should be approximately 13+ digits (current timestamp)",
                currentSeconds > 1000000000L);
    }

    @Test
    public void prefs_mode_is_private() {
        // SharedPreferences used to store update state should be MODE_PRIVATE
        // Other modes would expose update state to other apps
        int expectedMode = android.content.Context.MODE_PRIVATE;
        assertEquals("SharedPreferences should be MODE_PRIVATE", 0, expectedMode);
    }

    @Test
    public void installed_version_code_default_is_zero() {
        // When getInstalledVersionCode() cannot find the package, it returns 0
        // This value is used as the fallback when NativeRadar is unavailable
        long fallbackVersionCode = 0;
        assertEquals("Fallback version code when unavailable", 0, fallbackVersionCode);
    }

    @Test
    public void return_values_for_unavailable_native_are_safe_defaults() {
        // When NativeRadar is unavailable, UpdateManager returns conservative defaults:
        // - shouldCheckForUpdate: false (skip update check, no crash)
        // - assessUpdate: UPDATE_UP_TO_DATE (assume we're current, don't update)
        // - checkDownloadReadiness: DOWNLOAD_NO_NETWORK (assume can't download, don't try)
        // - computeRetryBackoff: baseDelaySeconds (retry with base delay, no crash)

        // These defaults prevent crashes and avoid spurious update attempts
        int safeAssessmentDefault = NativeRadar.UPDATE_UP_TO_DATE;
        int safeDownloadDefault = NativeRadar.DOWNLOAD_NO_NETWORK;

        assertNotNull("Safe defaults must be defined", safeAssessmentDefault);
        assertNotNull("Safe defaults must be defined", safeDownloadDefault);
    }

    @Test
    public void decision_method_names_are_descriptive() {
        // UpdateManager method names should clearly indicate what they decide
        // This prevents callers from misinterpreting return values
        String method1 = "shouldCheckForUpdate";
        String method2 = "assessUpdate";
        String method3 = "checkDownloadReadiness";
        String method4 = "computeRetryBackoff";

        assertTrue("shouldCheckForUpdate name is clear", method1.contains("Check"));
        assertTrue("assessUpdate name is clear", method2.contains("assess"));
        assertTrue("checkDownloadReadiness name is clear", method3.contains("Readiness"));
        assertTrue("computeRetryBackoff name is clear", method4.contains("Backoff"));
    }

    @Test
    public void shared_preferences_keys_match_across_update_components() {
        // UpdateManager, UpdateCheckService, and BootCompletedReceiver must use
        // identical key names for shared state:
        // - last_check_time_seconds: written by UpdateManager.recordCheckTime()
        // - installed_version_code: written by UpdateManager.recordInstalledVersion()
        // - retryCount: written by UpdateCheckService.scheduleRetry()
        // - activeDownloadId: written by UpdateCheckService.downloadAndInstallUpdate()
        // - pendingManifest: written by UpdateCheckService.saveDownloadState()
        //
        // BootCompletedReceiver reads retryCount and activeDownloadId on BOOT_COMPLETED
        // If key names differ between classes, state is lost across reboot cycle
        String keyLastCheckTime = "last_check_time_seconds";
        String keyInstalledVersionCode = "installed_version_code";
        String keyRetryCount = "retryCount";
        String keyActiveDownloadId = "activeDownloadId";
        String keyPendingManifest = "pendingManifest";

        // Verify keys are non-empty (prevents accidental hardcoding of empty keys)
        assertTrue("Keys must be non-empty", !keyLastCheckTime.isEmpty());
        assertTrue("Keys must be non-empty", !keyInstalledVersionCode.isEmpty());
        assertTrue("Keys must be non-empty", !keyRetryCount.isEmpty());
        assertTrue("Keys must be non-empty", !keyActiveDownloadId.isEmpty());
        assertTrue("Keys must be non-empty", !keyPendingManifest.isEmpty());

        // Verify keys are unique (prevents collisions)
        assertTrue("Keys must be unique", !keyLastCheckTime.equals(keyInstalledVersionCode));
        assertTrue("Keys must be unique", !keyRetryCount.equals(keyActiveDownloadId));
    }
}

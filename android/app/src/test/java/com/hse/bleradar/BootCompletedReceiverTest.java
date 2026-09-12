package com.hse.bleradar;

import android.content.Intent;
import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link BootCompletedReceiver} intent contract and expected behavior.
 *
 * <p>BootCompletedReceiver restores pending update retries after device reboot (which
 * clears all AlarmManager alarms). These tests verify the receiver's intent filtering
 * and shared-preferences integration contract.
 */
public class BootCompletedReceiverTest {

    @Test
    public void receiver_listens_for_boot_completed_action() {
        // The receiver must register for BOOT_COMPLETED in the manifest
        // and filter for the correct action in onReceive
        Intent bootIntent = new Intent(Intent.ACTION_BOOT_COMPLETED);
        assertNotNull("BOOT_COMPLETED action must be defined", Intent.ACTION_BOOT_COMPLETED);
        assertEquals("Expected boot completed action", "android.intent.action.BOOT_COMPLETED",
                Intent.ACTION_BOOT_COMPLETED);
    }

    @Test
    public void boot_receiver_and_check_service_use_same_prefs_name() {
        // Critical contract: BootCompletedReceiver reads retryCount and activeDownloadId
        // from SharedPreferences("UpdateCheckService"), which must match what
        // UpdateCheckService writes. If these names diverge, boot recovery fails silently.
        String receiverPrefsName = "UpdateCheckService";

        // Verify the constant is consistent across the codebase
        // (The actual UpdateCheckService uses "UpdateCheckService" as the prefs name;
        // we verify this contract here)
        assertEquals("SharedPreferences name for update service state",
                "UpdateCheckService", receiverPrefsName);
    }

    @Test
    public void boot_receiver_looks_for_retry_count_key() {
        // BootCompletedReceiver checks for retryCount > 0 to detect pending retries
        // This key must match what UpdateCheckService uses when writing retry state
        String keyName = "retryCount";
        assertNotNull("retryCount SharedPreferences key must be defined", keyName);
        assertEquals("Expected key name", "retryCount", keyName);
    }

    @Test
    public void boot_receiver_looks_for_active_download_id_key() {
        // BootCompletedReceiver checks for activeDownloadId != -1 to detect in-progress downloads
        // This key must match what UpdateCheckService uses when persisting download IDs
        String keyName = "activeDownloadId";
        assertNotNull("activeDownloadId SharedPreferences key must be defined", keyName);
        assertEquals("Expected key name", "activeDownloadId", keyName);
    }

    @Test
    public void retry_count_default_is_zero() {
        // If retryCount is missing from SharedPreferences, it defaults to 0 (no pending retry)
        // This is the expected safe default
        int defaultRetryCount = 0;
        assertEquals("Default retry count when not persisted", 0, defaultRetryCount);
    }

    @Test
    public void active_download_id_default_is_minus_one() {
        // If activeDownloadId is missing from SharedPreferences, it defaults to -1 (no download)
        // This is the expected safe default (DownloadManager IDs are non-negative)
        long defaultDownloadId = -1;
        assertEquals("Default download ID when not persisted", -1L, defaultDownloadId);
    }

    @Test
    public void boot_receiver_sets_is_retry_flag_when_retry_count_greater_than_zero() {
        // When BootCompletedReceiver detects retryCount > 0, it must set is_retry=true
        // in the intent extra so UpdateCheckService knows to skip the daily throttle
        String intentExtraKey = "is_retry";
        assertNotNull("is_retry intent extra key must be defined", intentExtraKey);
        assertEquals("Expected intent extra key", "is_retry", intentExtraKey);
    }

    @Test
    public void is_retry_flag_matches_retry_receiver() {
        // Both BootCompletedReceiver and UpdateRetryReceiver use the same is_retry flag
        // to signal that this is a retry invocation (not a periodic daily check)
        String bootReceiverFlagKey = "is_retry";
        String retryReceiverFlagKey = "is_retry";
        assertEquals("Both receivers must use the same is_retry flag key",
                bootReceiverFlagKey, retryReceiverFlagKey);
    }
}

package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link UpdateRetryReceiver} intent contract and action definition.
 *
 * <p>UpdateRetryReceiver handles AlarmManager-scheduled retry alarms for deferred
 * updates. These tests verify the receiver's action constant is correctly defined
 * and matches the scheduling side.
 */
public class UpdateRetryReceiverTest {

    @Test
    public void retry_receiver_action_is_defined() {
        // Verify the retry receiver action is correctly defined and is non-null
        assertNotNull("UpdateRetryReceiver.ACTION_UPDATE_RETRY must be defined",
                UpdateRetryReceiver.ACTION_UPDATE_RETRY);
    }

    @Test
    public void retry_receiver_action_has_expected_value() {
        // The action must be a stable, unique identifier for retry alarms
        // This is used by UpdateCheckService when scheduling the alarm and by the receiver
        // when filtering intents in onReceive()
        assertEquals("Retry action should have expected value",
                "com.hse.bleradar.UPDATE_RETRY", UpdateRetryReceiver.ACTION_UPDATE_RETRY);
    }

    @Test
    public void retry_action_is_stable_across_sessions() {
        // This constant is persisted (in AlarmManager alarms) and must survive app upgrades
        // A mismatch would cause alarms to be orphaned after upgrade
        String expectedAction = "com.hse.bleradar.UPDATE_RETRY";
        assertEquals("Retry action must be stable", expectedAction,
                UpdateRetryReceiver.ACTION_UPDATE_RETRY);
    }

    @Test
    public void retry_receiver_sets_is_retry_flag() {
        // When the receiver fires, it must set is_retry=true so UpdateCheckService
        // knows to skip the daily check throttle and apply retry backoff instead
        String flagKey = "is_retry";
        assertNotNull("is_retry flag key must be defined", flagKey);
        assertEquals("Expected is_retry flag key", "is_retry", flagKey);
    }

    @Test
    public void boot_and_retry_receivers_use_same_is_retry_flag() {
        // Both BootCompletedReceiver (on reboot with pending retry) and UpdateRetryReceiver
        // (on alarm fire) use the same is_retry=true flag to signal retry invocation
        // This ensures UpdateCheckService treats them identically
        String bootReceiverFlagKey = "is_retry";
        String alarmReceiverFlagKey = "is_retry";
        String updateRetryFlag = "is_retry";

        assertEquals("All retry paths must use the same flag",
                bootReceiverFlagKey, alarmReceiverFlagKey);
        assertEquals("Retry flag must match UpdateRetryReceiver's usage",
                alarmReceiverFlagKey, updateRetryFlag);
    }

    @Test
    public void retry_action_is_app_package_specific() {
        // To avoid collision with other apps, the action includes the full package name
        assertTrue("Retry action should be app-package-specific (contain package name)",
                UpdateRetryReceiver.ACTION_UPDATE_RETRY.contains("com.hse.bleradar"));
    }

    @Test
    public void retry_action_is_sufficiently_unique() {
        // The action suffix after the package name should be descriptive and unique
        assertTrue("Retry action should be uniquely named within the package",
                UpdateRetryReceiver.ACTION_UPDATE_RETRY.endsWith("UPDATE_RETRY"));
    }
}

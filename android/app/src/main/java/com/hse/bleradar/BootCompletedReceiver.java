package com.hse.bleradar;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.util.Log;

/**
 * Broadcast receiver that runs after device boot to restore any pending update retry alarms.
 *
 * <p>When the device reboots, all alarms are cleared by the system. This receiver detects
 * that reboot has completed and reschedules any pending retry alarm that was active before
 * the reboot. This ensures update retries survive a reboot cycle.
 */
public final class BootCompletedReceiver extends BroadcastReceiver {
    private static final String TAG = "BootCompletedReceiver";

    @Override
    public void onReceive(Context context, Intent intent) {
        if (!Intent.ACTION_BOOT_COMPLETED.equals(intent.getAction())) {
            return;
        }
        Log.d(TAG, "Device boot completed; restoring pending update retry alarms");

        // Check if there was a pending retry (retry count > 0)
        SharedPreferences prefs = context.getSharedPreferences("UpdateCheckService", Context.MODE_PRIVATE);
        int retryCount = prefs.getInt("retryCount", 0);
        long lastRetryTime = prefs.getLong("lastRetryTime", 0);

        if (retryCount > 0 && lastRetryTime > 0) {
            Log.d(TAG, "Found pending retry count=" + retryCount + "; rescheduling");
            // Trigger an update check to restart the retry cycle
            // This will re-evaluate the retry backoff and reschedule appropriately
            Intent checkIntent = new Intent(context, UpdateCheckService.class);
            checkIntent.putExtra("is_retry", true);
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
                context.startForegroundService(checkIntent);
            } else {
                context.startService(checkIntent);
            }
        }
    }
}

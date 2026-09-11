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
        Log.d(TAG, "Device boot completed; restoring pending updates");

        SharedPreferences prefs = context.getSharedPreferences("UpdateCheckService", Context.MODE_PRIVATE);
        int retryCount = prefs.getInt("retryCount", 0);
        long activeDownloadId = prefs.getLong("activeDownloadId", -1);

        // Always start the service to restore in-progress downloads
        // If there's a pending retry, mark it as a retry invocation so it skips the daily throttle
        Intent serviceIntent = new Intent(context, UpdateCheckService.class);
        if (retryCount > 0) {
            serviceIntent.putExtra("is_retry", true);
            Log.d(TAG, "Found pending retry count=" + retryCount + "; triggering as retry");
        }
        if (activeDownloadId != -1) {
            Log.d(TAG, "Found in-progress download " + activeDownloadId + "; restoring receiver");
        }

        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
            context.startForegroundService(serviceIntent);
        } else {
            context.startService(serviceIntent);
        }
    }
}

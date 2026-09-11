package com.hse.bleradar;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.util.Log;

/**
 * Broadcast receiver that handles scheduled update retry alarms.
 *
 * <p>When the UpdateCheckService defers a download due to transient conditions
 * (low battery, no network, insufficient storage), it schedules an alarm via
 * AlarmManager. This receiver wakes on that alarm and triggers the update check
 * to run again.
 *
 * <p>On Android 8+, background startService() is restricted. This receiver uses
 * startForegroundService() to ensure the service can start from a background
 * broadcast; the service must then call startForeground() within 5 seconds.
 */
public final class UpdateRetryReceiver extends BroadcastReceiver {
    private static final String TAG = "UpdateRetryReceiver";
    static final String ACTION_UPDATE_RETRY = "com.hse.bleradar.UPDATE_RETRY";

    @Override
    public void onReceive(Context context, Intent intent) {
        if (!ACTION_UPDATE_RETRY.equals(intent.getAction())) {
            return;
        }
        Log.d(TAG, "Update retry alarm fired; triggering check");
        Intent serviceIntent = new Intent(context, UpdateCheckService.class);
        serviceIntent.putExtra("is_retry", true);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(serviceIntent);
        } else {
            context.startService(serviceIntent);
        }
    }
}

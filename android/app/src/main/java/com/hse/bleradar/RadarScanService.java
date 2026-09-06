package com.hse.bleradar;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Binder;
import android.os.Build;
import android.os.IBinder;

import java.util.Collections;
import java.util.List;

/**
 * Foreground service that owns the single {@link BleScanEngine} instance so
 * scanning survives activity recreation (rotation, backgrounding). Mirrors
 * the oracle APK's own always-on "BLE Radar is scanning" notification
 * behaviour, using the exact channel/title copy extracted from its
 * resources (see {@code res/values/strings.xml}).
 */
public final class RadarScanService extends Service {

    private static final String CHANNEL_ID = "ble_radar_scanning";
    private static final int NOTIFICATION_ID = 1;

    private final IBinder binder = new LocalBinder();
    private BleScanEngine engine;

    /** Binder handed to {@link MainActivity} to reach this service's live state. */
    public final class LocalBinder extends Binder {
        RadarScanService getService() {
            return RadarScanService.this;
        }
    }

    @Override
    public void onCreate() {
        super.onCreate();
        engine = new BleScanEngine(this);
        createNotificationChannel();
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        Notification notification = buildNotification(false);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
        return START_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override
    public void onDestroy() {
        if (engine != null) {
            engine.stop();
        }
        super.onDestroy();
    }

    boolean startScanning() {
        boolean started = engine.start();
        updateNotification(started);
        return started;
    }

    void stopScanning() {
        engine.stop();
        updateNotification(false);
    }

    boolean isScanning() {
        return engine != null && engine.isScanning();
    }

    List<Blip> snapshot() {
        return engine == null ? Collections.emptyList() : engine.snapshot();
    }

    private void createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return;
        }
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notif_channel),
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(getString(R.string.notif_channel_description));
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (manager != null) {
            manager.createNotificationChannel(channel);
        }
    }

    private Notification buildNotification(boolean scanning) {
        Intent launchIntent = new Intent(this, MainActivity.class);
        int flags = PendingIntent.FLAG_UPDATE_CURRENT
                | (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M ? PendingIntent.FLAG_IMMUTABLE : 0);
        PendingIntent contentIntent = PendingIntent.getActivity(this, 0, launchIntent, flags);

        String text = getString(scanning ? R.string.notif_title : R.string.notif_title_idle);
        return new Notification.Builder(this, CHANNEL_ID)
                .setContentTitle(getString(R.string.app_name))
                .setContentText(text)
                .setSmallIcon(R.mipmap.ic_launcher)
                .setContentIntent(contentIntent)
                .setOngoing(true)
                .build();
    }

    private void updateNotification(boolean scanning) {
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (manager != null) {
            manager.notify(NOTIFICATION_ID, buildNotification(scanning));
        }
    }
}

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
import android.util.Log;

import java.util.Collections;
import java.util.List;

/**
 * Foreground service that owns the single {@link BleScanEngine} instance so
 * scanning survives activity recreation (rotation, backgrounding) and process
 * death. Mirrors the oracle APK's own always-on "BLE Radar is scanning"
 * notification behaviour, using the exact channel/title copy extracted from
 * its resources (see {@code res/values/strings.xml}).
 *
 * <p>Lifecycle contract (see docs/ANDROID_APP.md):
 * <ul>
 *   <li>{@link MainActivity} only <em>binds</em> while it is visible. Binding
 *       alone never promotes the service to the foreground, so nothing is
 *       shown in the notification shade while the app is idle.</li>
 *   <li>A scan request arrives as {@link #onStartCommand}: either an explicit
 *       {@code startForegroundService} from the activity after the runtime
 *       permissions were granted, or a {@code START_STICKY} restart after the
 *       process died while scanning. Both mean "scanning was requested", so
 *       the service promotes itself to the foreground and starts (or resumes)
 *       the scan. This is what makes a scan survive process death.</li>
 *   <li>{@link #stopScanning()} leaves the foreground, removes the
 *       notification, and drops the started state, so a user-initiated stop is
 *       never resurrected by a sticky restart.</li>
 * </ul>
 *
 * <p>On API 34+ the {@code connectedDevice} foreground-service type requires
 * one of the Bluetooth runtime permissions to be granted <em>before</em>
 * {@code startForeground}, otherwise the platform throws. Promoting only from
 * {@link #onStartCommand}, which is reached only after the grant (or after a
 * restart that implies it), keeps that ordering; a restart that finds the
 * permissions revoked stops the service instead of throwing.
 */
public final class RadarScanService extends Service {

    private static final String TAG = "RadarScanService";
    private static final int MIN_ANDROID_VERSION_FOR_FOREGROUND_SERVICE_TYPE = Build.VERSION_CODES.UPSIDE_DOWN_CAKE;

    private static final String CHANNEL_ID = "ble_radar_scanning";
    private static final int NOTIFICATION_ID = 1;

    private final IBinder binder = new LocalBinder();
    private BleScanEngine engine;
    private ApiHttpServer httpServer;

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
        httpServer = new ApiHttpServer(engine, new UpdateManager(this));
        httpServer.start();
    }

    /**
     * Handles every scan request: the activity's explicit start after the
     * permissions were granted, and the system's sticky restart after the
     * process died while scanning ({@code intent} is {@code null} then).
     */
    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (!BleScanEngine.hasRequiredPermissions(this)) {
            Log.w(TAG, "Bluetooth permissions revoked on sticky restart; stopping service");
            stopSelf(startId);
            return START_NOT_STICKY;
        }
        boolean started = engine.start();
        promoteToForeground(started);
        return START_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    @Override
    public void onDestroy() {
        if (httpServer != null) {
            httpServer.stop();
        }
        if (engine != null) {
            engine.stop();
        }
        super.onDestroy();
    }

    /**
     * Starts scanning immediately for the bound activity. The activity pairs
     * this with {@code startForegroundService}, whose {@link #onStartCommand}
     * promotes the service and pins the notification; {@link BleScanEngine#start()}
     * is idempotent, so the two paths never double-start the scanner.
     *
     * @return {@code true} if scanning is now active.
     */
    boolean startScanning() {
        return engine.start();
    }

    /** Stops scanning, removes the notification, and leaves the started/foreground state. */
    void stopScanning() {
        engine.stop();
        stopForeground(Service.STOP_FOREGROUND_REMOVE);
        stopSelf();
    }

    boolean isScanning() {
        return engine != null && engine.isScanning();
    }

    List<Blip> snapshot() {
        return engine == null ? Collections.emptyList() : engine.snapshot();
    }

    private void promoteToForeground(boolean scanning) {
        Notification notification = buildNotification(scanning);
        if (Build.VERSION.SDK_INT >= MIN_ANDROID_VERSION_FOR_FOREGROUND_SERVICE_TYPE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
        if (!scanning) {
            // The scan could not start (adapter off, scanner unavailable). The
            // promotion above honours the startForegroundService contract;
            // leave the foreground again so no misleading notification
            // lingers, and drop the started state.
            stopForeground(Service.STOP_FOREGROUND_REMOVE);
            stopSelf();
        }
    }

    private void createNotificationChannel() {
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notif_channel),
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(getString(R.string.notif_channel_description));
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (manager != null) {
            manager.createNotificationChannel(channel);
        } else {
            Log.w(TAG, "NotificationManager unavailable; notification channel creation failed");
        }
    }

    private Notification buildNotification(boolean scanning) {
        Intent launchIntent = new Intent(this, MainActivity.class);
        PendingIntent contentIntent = PendingIntent.getActivity(
                this, 0, launchIntent, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        String text = getString(scanning ? R.string.notif_title : R.string.notif_title_idle);
        return new Notification.Builder(this, CHANNEL_ID)
                .setContentTitle(getString(R.string.app_name))
                .setContentText(text)
                .setSmallIcon(R.mipmap.ic_launcher)
                .setContentIntent(contentIntent)
                .setOngoing(true)
                .build();
    }
}

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
import android.os.SystemClock;
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
public final class RadarScanService extends Service implements ScanControl {

    private static final String TAG = "RadarScanService";
    private static final int MIN_ANDROID_VERSION_FOR_FOREGROUND_SERVICE_TYPE = Build.VERSION_CODES.UPSIDE_DOWN_CAKE;

    private static final String CHANNEL_ID = "ble_radar_scanning";
    private static final int NOTIFICATION_ID = 1;

    private final IBinder binder = new LocalBinder();
    private BleScanEngine engine;
    private ApiHttpServer httpServer;
    /** Whether {@link #promoteToForeground} is in effect (so a pause can refresh the notification). */
    private volatile boolean foreground;
    /**
     * Set by a stop, cleared by a start request: {@link #onStartCommand}
     * skips the engine start only when a stop overtook the
     * {@code startForegroundService} it follows (both can arrive from the
     * HTTP handler thread within milliseconds). A fresh process starts with
     * it clear, so a sticky restart resumes the scan the process died with
     * whether its start command carries a redelivered intent or none — the
     * emulator proof caught the earlier "start still wanted" flag treating a
     * redelivered start intent as an overtaken one and ending the service.
     */
    private volatile boolean stopOvertookStart;

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
        httpServer = new ApiHttpServer(
                engine,
                new UpdateManager(this),
                getAssets()::open,
                SystemClock::uptimeMillis,
                System::currentTimeMillis,
                this,
                ApiHttpServer.DEFAULT_PORT);
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
        // Every start command resumes or starts the scan — an explicit start,
        // or the sticky restart's, whose intent may be null or a redelivered
        // start — unless a stop overtook it, in which case the promotion below
        // leaves the foreground again at once.
        boolean wanted = !stopOvertookStart;
        stopOvertookStart = false;
        boolean started = wanted && engine.start();
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
            // Close, not stop: a request the HTTP handler is still serving
            // (the activity's unbind and an API start can be milliseconds
            // apart) must not leave a scan running in this dead instance.
            engine.close();
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
        stopOvertookStart = false;
        return engine.start();
    }

    /** Stops scanning, removes the notification, and leaves the started/foreground state. */
    void stopScanning() {
        stopOvertookStart = true;
        engine.stop();
        foreground = false;
        stopForeground(Service.STOP_FOREGROUND_REMOVE);
        stopSelf();
    }

    /**
     * {@code POST /api/scan/start}: the same gate the activity's Start action
     * applies (permissions, adapter), then the same start request — the
     * started state promotes the service and lets a sticky restart resume the
     * scan — with the engine started here as well so the answer reports the
     * live state ({@link BleScanEngine#start()} is idempotent and serialized).
     */
    @Override
    public int requestStart() {
        if (engine.isScanning()) {
            // Already running: accepted without re-running the gates, which a
            // toggled adapter could otherwise turn into a refusal.
            stopOvertookStart = false;
            return ScanControl.START_ACCEPTED;
        }
        if (!BleScanEngine.hasRequiredPermissions(this)) {
            return ScanControl.START_PERMISSIONS_MISSING;
        }
        if (!BleScanEngine.isBluetoothEnabled(this)) {
            return ScanControl.START_BLUETOOTH_OFF;
        }
        stopOvertookStart = false;
        try {
            startForegroundService(new Intent(this, RadarScanService.class));
        } catch (IllegalStateException restricted) {
            // API 31+ ForegroundServiceStartNotAllowedException: the app counts
            // as background (no visible activity, not yet in the foreground).
            // Report it instead of letting it end the process from the HTTP
            // handler thread.
            Log.w(TAG, "Background start refused; open the app once", restricted);
            return ScanControl.START_BACKGROUND_RESTRICTED;
        }
        return engine.start() ? ScanControl.START_ACCEPTED : ScanControl.START_UNAVAILABLE;
    }

    /**
     * {@code POST /api/scan/stop}: pauses the scan but keeps the service
     * started and in the foreground (with the idle notification) so the API
     * stays reachable for a headless client; the app's Stop button is what
     * ends the service.
     */
    @Override
    public void requestStop() {
        stopOvertookStart = true;
        engine.stop();
        if (foreground) {
            promoteToForeground(false, false);
        }
    }

    /** The loopback URL of the web dashboard while the API is bound, else {@code null}. */
    String apiUrl() {
        int port = httpServer == null ? -1 : httpServer.boundPort();
        return port > 0 ? "http://127.0.0.1:" + port + "/" : null;
    }

    boolean isScanning() {
        return engine != null && engine.isScanning();
    }

    List<Blip> snapshot() {
        return engine == null ? Collections.emptyList() : engine.snapshot();
    }

    private void promoteToForeground(boolean scanning) {
        promoteToForeground(scanning, true);
    }

    /**
     * Enters (or refreshes) the foreground with the scanning or idle
     * notification. With {@code stopWhenIdle}, a scan that could not start
     * leaves the foreground again; an API pause passes {@code false} to stay.
     */
    private void promoteToForeground(boolean scanning, boolean stopWhenIdle) {
        Notification notification = buildNotification(scanning);
        if (Build.VERSION.SDK_INT >= MIN_ANDROID_VERSION_FOR_FOREGROUND_SERVICE_TYPE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
        foreground = true;
        if (!scanning && stopWhenIdle) {
            // The scan could not start (adapter off, scanner unavailable). The
            // promotion above honours the startForegroundService contract;
            // leave the foreground again so no misleading notification
            // lingers, and drop the started state.
            foreground = false;
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

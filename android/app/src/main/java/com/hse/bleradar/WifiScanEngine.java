package com.hse.bleradar;

import android.Manifest;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.location.LocationManager;
import android.net.wifi.ScanResult;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.provider.Settings;
import android.util.Log;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Passive Wi-Fi survey: reads the platform's scan results on a timer and hands
 * each to the Rust core ({@code bleradar_core::wifi_observation}, through
 * {@link NativeRadar#wifiObservation}) for every reading rule, keeping only
 * plumbing and bookkeeping here — the same split as {@link BleScanEngine}.
 *
 * <p>It is passive and best-effort: it never connects to or associates with a
 * network, asks the platform for a fresh scan no more than Android's foreground
 * throttle allows (four per two minutes, so one per 35 s), and reads whatever
 * the platform has cached in between. It rides the BLE scan's lifecycle
 * ({@link RadarScanService} starts and stops both) but never affects it: when
 * Wi-Fi cannot be read, {@link #state()} says why and the BLE scan carries on.
 *
 * <p>Reading scan results needs {@code ACCESS_FINE_LOCATION}, which the BLE scan
 * already holds, so the survey adds no permission prompt; on Android 12 and
 * below (and 13+ without {@code NEARBY_WIFI_DEVICES}) the platform also hides
 * results while location services are off, reported as
 * {@link #STATE_LOCATION_OFF} rather than as an empty network list.
 */
final class WifiScanEngine implements WifiSurveySource {

    private static final String TAG = "WifiScanEngine";
    /** One platform scan request per interval: inside Android's four-per-two-minutes foreground throttle. */
    static final long SCAN_INTERVAL_MILLIS = 35_000L;

    private final Context appContext;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private volatile List<WifiAp> accessPoints = Collections.emptyList();
    private volatile String state = STATE_IDLE;
    private volatile int dropped;
    private volatile boolean running;
    private boolean closed;
    private BroadcastReceiver receiver;

    private final Runnable tick = new Runnable() {
        @Override
        public void run() {
            if (!running) {
                return;
            }
            refresh(true);
            handler.postDelayed(this, SCAN_INTERVAL_MILLIS);
        }
    };

    WifiScanEngine(Context context) {
        this.appContext = context.getApplicationContext() == null
                ? context
                : context.getApplicationContext();
    }

    /** Whether the permission the survey reads scan results under is granted. */
    static boolean hasPermission(Context context) {
        return context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION)
                == PackageManager.PERMISSION_GRANTED;
    }

    /**
     * Starts the survey. Idempotent; never throws and never fails the BLE scan:
     * a survey that cannot run leaves its reason in {@link #state()}.
     */
    synchronized void start() {
        if (closed || running) {
            return;
        }
        dropped = 0;
        if (!hasPermission(appContext)) {
            state = STATE_PERMISSION_DENIED;
            return;
        }
        if (!NativeRadar.isAvailable() || appContext.getSystemService(WifiManager.class) == null) {
            state = STATE_UNAVAILABLE;
            return;
        }
        running = true;
        state = STATE_ACTIVE;
        receiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {
                refresh(false);
            }
        };
        IntentFilter filter = new IntentFilter(WifiManager.SCAN_RESULTS_AVAILABLE_ACTION);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            appContext.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            appContext.registerReceiver(receiver, filter);
        }
        handler.post(tick);
    }

    synchronized void stop() {
        if (!running) {
            return;
        }
        running = false;
        handler.removeCallbacks(tick);
        try {
            appContext.unregisterReceiver(receiver);
        } catch (IllegalArgumentException alreadyGone) {
            // The receiver is no longer registered; nothing further to release.
        }
        receiver = null;
        accessPoints = Collections.emptyList();
        state = STATE_IDLE;
    }

    /** Stops the survey and refuses every later {@link #start()}: the service is being destroyed. */
    synchronized void close() {
        stop();
        closed = true;
    }

    @Override
    public List<WifiAp> accessPoints() {
        return accessPoints;
    }

    @Override
    public String state() {
        return state;
    }

    @Override
    public int dropped() {
        return dropped;
    }

    /** Reads the platform's cached results, first asking for a fresh scan when {@code requestScan}. */
    @SuppressWarnings("deprecation")
    private void refresh(boolean requestScan) {
        WifiManager wifi = appContext.getSystemService(WifiManager.class);
        if (!running || wifi == null) {
            return;
        }
        if (!hasPermission(appContext)) {
            publish(Collections.emptyList(), STATE_PERMISSION_DENIED);
            return;
        }
        if (!wifi.isWifiEnabled()) {
            publish(Collections.emptyList(), STATE_WIFI_OFF);
            return;
        }
        if (!locationEnabled()) {
            publish(Collections.emptyList(), STATE_LOCATION_OFF);
            return;
        }
        try {
            if (requestScan && !wifi.startScan()) {
                Log.d(TAG, "Platform throttled the scan request; reading cached results");
            }
            publish(decode(wifi.getScanResults()), STATE_ACTIVE);
        } catch (SecurityException error) {
            Log.w(TAG, "Wi-Fi scan permission revoked at call time", error);
            publish(Collections.emptyList(), STATE_PERMISSION_DENIED);
        }
    }

    private void publish(List<WifiAp> rows, String newState) {
        accessPoints = rows;
        state = newState;
    }

    @SuppressWarnings("deprecation")
    private List<WifiAp> decode(List<ScanResult> results) {
        long nowEpochMillis = System.currentTimeMillis();
        long nowElapsedMillis = SystemClock.elapsedRealtime();
        Map<String, WifiAp> byBssid = new LinkedHashMap<>();
        int refused = 0;
        for (ScanResult result : results) {
            // ScanResult.timestamp is microseconds on the elapsed-realtime clock.
            long seenAgoMillis = Math.max(0L, nowElapsedMillis - result.timestamp / 1000L);
            WifiAp ap = WifiAp.observe(
                    result.BSSID, result.SSID, result.capabilities, result.level,
                    result.frequency, nowEpochMillis - seenAgoMillis);
            if (ap == null) {
                refused++;
            } else {
                byBssid.put(ap.bssid, ap);
            }
        }
        dropped = refused;
        List<WifiAp> rows = new ArrayList<>(byBssid.values());
        rows.sort(WifiAp.STRONGEST_FIRST);
        return Collections.unmodifiableList(rows);
    }

    @SuppressWarnings("deprecation")
    private boolean locationEnabled() {
        LocationManager location = appContext.getSystemService(LocationManager.class);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            return location != null && location.isLocationEnabled();
        }
        try {
            return Settings.Secure.getInt(
                    appContext.getContentResolver(), Settings.Secure.LOCATION_MODE)
                    != Settings.Secure.LOCATION_MODE_OFF;
        } catch (Settings.SettingNotFoundException error) {
            return false;
        }
    }
}

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
    /**
     * The survey's cross-session memory: the same Rust store and rules as the
     * BLE device history (only a trackable BSSID is remembered), in its own file.
     */
    private final DeviceHistory history;
    private final Handler handler = new Handler(Looper.getMainLooper());
    /** The one published generation; replaced whole, never edited in place. */
    private volatile WifiSurvey survey = WifiSurvey.empty(STATE_IDLE);
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
        this.history = new DeviceHistory(appContext.getFilesDir(), DeviceHistory.WIFI_FILE_NAME);
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
        if (!hasPermission(appContext)) {
            survey = WifiSurvey.empty(STATE_PERMISSION_DENIED);
            return;
        }
        if (!NativeRadar.isAvailable() || appContext.getSystemService(WifiManager.class) == null) {
            survey = WifiSurvey.empty(STATE_UNAVAILABLE);
            return;
        }
        running = true;
        survey = WifiSurvey.empty(STATE_ACTIVE);
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
        if (running) {
            running = false;
            handler.removeCallbacks(tick);
            try {
                appContext.unregisterReceiver(receiver);
            } catch (IllegalArgumentException alreadyGone) {
                // The receiver is no longer registered; nothing further to release.
            }
            receiver = null;
        }
        // Whether or not the survey ever ran, a stopped scan leaves an idle,
        // empty survey: a start that failed (no radio, no permission) must not
        // keep reporting its refusal, or its last count, after the scan ends.
        survey = WifiSurvey.empty(STATE_IDLE);
        history.flush(SystemClock.uptimeMillis());
    }

    /** Stops the survey and refuses every later {@link #start()}: the service is being destroyed. */
    synchronized void close() {
        stop();
        closed = true;
    }

    @Override
    public WifiSurvey survey() {
        return survey;
    }

    /** Reads the platform's cached results, first asking for a fresh scan when {@code requestScan}. */
    @SuppressWarnings("deprecation")
    private void refresh(boolean requestScan) {
        WifiManager wifi = appContext.getSystemService(WifiManager.class);
        if (!running || wifi == null) {
            return;
        }
        if (!hasPermission(appContext)) {
            publish(WifiSurvey.empty(STATE_PERMISSION_DENIED));
            return;
        }
        if (!wifi.isWifiEnabled()) {
            publish(WifiSurvey.empty(STATE_WIFI_OFF));
            return;
        }
        if (!locationEnabled()) {
            publish(WifiSurvey.empty(STATE_LOCATION_OFF));
            return;
        }
        try {
            if (requestScan && !wifi.startScan()) {
                Log.d(TAG, "Platform throttled the scan request; reading cached results");
            }
            publish(decode(wifi.getScanResults()));
        } catch (SecurityException error) {
            Log.w(TAG, "Wi-Fi scan permission revoked at call time", error);
            publish(WifiSurvey.empty(STATE_PERMISSION_DENIED));
        }
    }

    /**
     * Publishes a generation, unless the survey was stopped while it was being
     * read: a late read must not resurrect a stopped survey.
     */
    private synchronized void publish(WifiSurvey next) {
        if (running) {
            survey = next;
        }
    }

    @SuppressWarnings("deprecation")
    private WifiSurvey decode(List<ScanResult> results) {
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
        // One write per read: every trackable BSSID is recorded at the time the
        // platform actually saw it (not the time it was read, so a cached result
        // is not a new sighting), then the batch is flushed once, the Rust store
        // decides what is remembered, and each row carries it.
        for (WifiAp ap : byBssid.values()) {
            if ("TRACKABLE".equals(ap.trackability)) {
                history.record(ap.bssid, ap.lastSeenEpochMillis);
            }
        }
        history.flush(SystemClock.uptimeMillis());
        List<WifiAp> rows = new ArrayList<>(byBssid.size());
        for (WifiAp ap : byBssid.values()) {
            rows.add(ap.withHistory(history.lookup(ap.bssid)));
        }
        rows.sort(WifiAp.STRONGEST_FIRST);
        return new WifiSurvey(rows, STATE_ACTIVE, refused);
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

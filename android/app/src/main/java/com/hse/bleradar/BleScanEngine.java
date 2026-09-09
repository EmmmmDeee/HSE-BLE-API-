package com.hse.bleradar;

import android.Manifest;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothManager;
import android.bluetooth.le.BluetoothLeScanner;
import android.bluetooth.le.ScanCallback;
import android.bluetooth.le.ScanResult;
import android.bluetooth.le.ScanSettings;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.SystemClock;
import android.util.Log;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Owns the live {@link BluetoothLeScanner} session and the canonical
 * address-keyed device map. Every distance/proximity/trend value comes from
 * {@link NativeRadar}, i.e. from {@code bleradar-core}'s tested
 * implementation — this class only does BLE plumbing and bookkeeping (never
 * reimplements the signal/tracking math itself).
 */
final class BleScanEngine {

    private static final String TAG = "BleScanEngine";

    /** Deadband used for the strengthening/weakening trend classification. */
    private static final double TREND_DEADBAND_DB = 3.0;
    /** RSSI EMA alpha applied by the Rust core via {@link NativeRadar#trackingFilteredRssi(double, double, double, double, double, int, int, long, long, long)}. */
    private static final double RSSI_SMOOTHING_ALPHA = 0.35;
    /** Observations within this window are considered live. */
    private static final long LIVE_FRESHNESS_WINDOW_MILLIS = 5_000L;
    /** Long-idle devices are pruned to keep the simple UI focused on live signals. */
    private static final long STALE_RETENTION_WINDOW_MILLIS = 30_000L;
    private final Context appContext;
    private final Map<String, Blip> blipsByAddress = new ConcurrentHashMap<>();
    private final int calibrationProfile;
    private BluetoothLeScanner scanner;
    private volatile boolean scanning;

    private final ScanCallback scanCallback = new ScanCallback() {
        @Override
        public void onScanResult(int callbackType, ScanResult result) {
            recordResult(result);
        }

        @Override
        public void onBatchScanResults(List<ScanResult> results) {
            for (ScanResult result : results) {
                recordResult(result);
            }
        }

        @Override
        public void onScanFailed(int errorCode) {
            Log.w(TAG, "BLE scan failed with code " + errorCode);
            scanning = false;
        }
    };

    BleScanEngine(Context context) {
        this.appContext = context.getApplicationContext();
        this.calibrationProfile = NativeRadar.isAvailable()
                ? NativeRadar.defaultCalibrationProfile()
                : NativeRadar.CALIBRATION_BASELINE;
    }

    static boolean hasRequiredPermissions(Context context) {
        String[] required = requiredPermissions();
        for (String permission : required) {
            if (context.checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
                return false;
            }
        }
        return true;
    }

    static String[] requiredPermissions() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            return new String[] {
                    Manifest.permission.BLUETOOTH_SCAN,
                    Manifest.permission.BLUETOOTH_CONNECT,
                    Manifest.permission.ACCESS_FINE_LOCATION
            };
        }
        return new String[] {
                Manifest.permission.ACCESS_FINE_LOCATION
        };
    }

    /** @return {@code true} if scanning actually started. */
    boolean start() {
        if (scanning) {
            return true;
        }
        if (!hasRequiredPermissions(appContext)) {
            Log.w(TAG, "Missing required permissions; not starting scan");
            return false;
        }
        BluetoothManager manager = appContext.getSystemService(BluetoothManager.class);
        BluetoothAdapter adapter = manager == null ? null : manager.getAdapter();
        if (adapter == null || !adapter.isEnabled()) {
            Log.w(TAG, "Bluetooth adapter unavailable or disabled");
            return false;
        }
        scanner = adapter.getBluetoothLeScanner();
        if (scanner == null) {
            return false;
        }
        ScanSettings settings = new ScanSettings.Builder()
                .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
                .build();
        try {
            scanner.startScan(null, settings, scanCallback);
            scanning = true;
            return true;
        } catch (SecurityException error) {
            Log.w(TAG, "Scan permission revoked at call time", error);
            return false;
        }
    }

    void stop() {
        if (!scanning || scanner == null) {
            return;
        }
        try {
            scanner.stopScan(scanCallback);
        } catch (SecurityException ignored) {
            // Permission may already have been revoked; nothing further to release.
        } finally {
            scanning = false;
        }
    }

    boolean isScanning() {
        return scanning;
    }

    /** A defensive copy of every device observed within the current session. */
    List<Blip> snapshot() {
        long now = SystemClock.uptimeMillis();
        refreshFreshness(now);
        pruneStale(now);
        List<Blip> snapshot = new ArrayList<>(blipsByAddress.values());
        snapshot.sort((left, right) -> {
            int byFreshnessClass = Integer.compare(left.freshness, right.freshness);
            if (byFreshnessClass != 0) {
                return byFreshnessClass;
            }
            int byFreshness = Long.compare(right.lastSeenUptimeMillis, left.lastSeenUptimeMillis);
            if (byFreshness != 0) {
                return byFreshness;
            }
            int byConfidence = Integer.compare(right.confidencePercent, left.confidencePercent);
            if (byConfidence != 0) {
                return byConfidence;
            }
            return Double.compare(right.lastRssiDbm, left.lastRssiDbm);
        });
        return snapshot;
    }

    Collection<Blip> blips() {
        return blipsByAddress.values();
    }

    private void recordResult(ScanResult result) {
        long now = SystemClock.uptimeMillis();
        String address = result.getDevice().getAddress();
        if (address == null) {
            return;
        }
        Blip blip = blipsByAddress.computeIfAbsent(address, Blip::new);
        double rawRssi = result.getRssi();
        double previous = blip.lastRssiDbm;

        if (NativeRadar.isAvailable()) {
            double smoothed = NativeRadar.trackingFilteredRssi(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    0.0,
                    Math.max(1, blip.sampleCount()),
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            if (!Double.isFinite(smoothed)) {
                smoothed = rawRssi;
            }
            blip.recordFilteredRssi(smoothed);
            blip.lastRssiDbm = smoothed;
            double spreadDb = blip.recentRssiSpreadDb();
            int sampleCount = blip.sampleCount();
            blip.trend = NativeRadar.trackingTrend(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            blip.distanceMetres = NativeRadar.trackingDistanceM(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            blip.distanceLowerBoundMetres = NativeRadar.trackingDistanceLowerBoundM(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            blip.distanceUpperBoundMetres = NativeRadar.trackingDistanceUpperBoundM(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            blip.proximity = NativeRadar.trackingProximity(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            int confidencePercent = NativeRadar.trackingConfidencePercent(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
            blip.confidencePercent = Math.max(0, confidencePercent);
            blip.freshness = NativeRadar.trackingFreshness(
                    previous,
                    rawRssi,
                    RSSI_SMOOTHING_ALPHA,
                    TREND_DEADBAND_DB,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    0L,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
        } else {
            blip.lastRssiDbm = rawRssi;
            blip.distanceMetres = Double.NaN;
            blip.distanceLowerBoundMetres = Double.NaN;
            blip.distanceUpperBoundMetres = Double.NaN;
            blip.proximity = NativeRadar.PROXIMITY_FAR;
            blip.confidencePercent = 0;
            blip.freshness = NativeRadar.FRESHNESS_LIVE;
        }
        blip.lastSeenUptimeMillis = now;

        String name = safeDeviceName(result);
        if (name != null) {
            blip.name = name;
        }
        pruneStale(now);
    }

    private String safeDeviceName(ScanResult result) {
        try {
            if (result.getScanRecord() != null && result.getScanRecord().getDeviceName() != null) {
                return result.getScanRecord().getDeviceName();
            }
            return result.getDevice().getName();
        } catch (SecurityException denied) {
            return null;
        }
    }

    private void refreshFreshness(long nowUptimeMillis) {
        if (!NativeRadar.isAvailable()) {
            return;
        }
        for (Blip blip : blipsByAddress.values()) {
            long ageMs = Math.max(0L, nowUptimeMillis - blip.lastSeenUptimeMillis);
            blip.freshness = NativeRadar.trackingFreshness(
                    blip.lastRssiDbm,
                    blip.lastRssiDbm,
                    1.0,
                    TREND_DEADBAND_DB,
                    blip.recentRssiSpreadDb(),
                    Math.max(1, blip.sampleCount()),
                    calibrationProfile,
                    ageMs,
                    LIVE_FRESHNESS_WINDOW_MILLIS,
                    STALE_RETENTION_WINDOW_MILLIS);
        }
    }

    private void pruneStale(long nowUptimeMillis) {
        blipsByAddress.entrySet().removeIf(entry -> {
            Blip blip = entry.getValue();
            long ageMs = Math.max(0L, nowUptimeMillis - blip.lastSeenUptimeMillis);
            if (NativeRadar.isAvailable()) {
                return blip.freshness == NativeRadar.FRESHNESS_STALE
                        || NativeRadar.trackingFreshness(
                        blip.lastRssiDbm,
                        blip.lastRssiDbm,
                        1.0,
                        TREND_DEADBAND_DB,
                        blip.recentRssiSpreadDb(),
                        Math.max(1, blip.sampleCount()),
                        calibrationProfile,
                        ageMs,
                        LIVE_FRESHNESS_WINDOW_MILLIS,
                        STALE_RETENTION_WINDOW_MILLIS) == NativeRadar.FRESHNESS_STALE;
            }
            return ageMs > STALE_RETENTION_WINDOW_MILLIS;
        });
    }
}

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
 * implementation — this class only does BLE plumbing, RSSI smoothing, and
 * bookkeeping (never reimplements the signal math itself).
 */
final class BleScanEngine {

    private static final String TAG = "BleScanEngine";

    /** Reference RSSI at 1 metre. A conservative, commonly used default; real hardware varies. */
    private static final double DEFAULT_RSSI_AT_1M_DBM = -59.0;
    /** Free-space-ish default path-loss exponent; environments with walls/obstructions run higher. */
    private static final double DEFAULT_PATH_LOSS_EXPONENT = 2.0;
    /** Deadband used for the strengthening/weakening trend classification. */
    private static final double TREND_DEADBAND_DB = 3.0;
    /** RSSI EMA alpha applied by the Rust core via {@link NativeRadar#filteredRssi(double, double, double)}. */
    private static final double RSSI_SMOOTHING_ALPHA = 0.35;
    /** Long-idle devices are pruned to keep the simple UI focused on live signals. */
    private static final long STALE_RETENTION_WINDOW_MILLIS = 30_000L;

    private final Context appContext;
    private final Map<String, Blip> blipsByAddress = new ConcurrentHashMap<>();
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
        pruneStale(now);
        List<Blip> snapshot = new ArrayList<>(blipsByAddress.values());
        snapshot.sort((left, right) -> {
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
        boolean hasPrevious = Double.isFinite(previous);

        if (NativeRadar.isAvailable()) {
            double smoothed = NativeRadar.filteredRssi(previous, rawRssi, RSSI_SMOOTHING_ALPHA);
            if (!Double.isFinite(smoothed)) {
                smoothed = rawRssi;
            }
            blip.lastRssiDbm = smoothed;
            blip.recordFilteredRssi(smoothed);
            double spreadDb = blip.recentRssiSpreadDb();
            blip.trend = NativeRadar.signalTrend(hasPrevious ? previous : smoothed, smoothed, TREND_DEADBAND_DB);
            blip.distanceMetres = NativeRadar.bleDistanceM(smoothed, DEFAULT_RSSI_AT_1M_DBM, DEFAULT_PATH_LOSS_EXPONENT);
            blip.distanceLowerBoundMetres = NativeRadar.distanceLowerBoundM(
                    smoothed, spreadDb, DEFAULT_RSSI_AT_1M_DBM, DEFAULT_PATH_LOSS_EXPONENT);
            blip.distanceUpperBoundMetres = NativeRadar.distanceUpperBoundM(
                    smoothed, spreadDb, DEFAULT_RSSI_AT_1M_DBM, DEFAULT_PATH_LOSS_EXPONENT);
            blip.proximity = NativeRadar.proximityLabel(smoothed);
            int confidencePercent = NativeRadar.signalConfidencePercent(blip.sampleCount(), spreadDb);
            blip.confidencePercent = Math.max(0, confidencePercent);
        } else {
            blip.lastRssiDbm = rawRssi;
            blip.distanceMetres = Double.NaN;
            blip.distanceLowerBoundMetres = Double.NaN;
            blip.distanceUpperBoundMetres = Double.NaN;
            blip.proximity = NativeRadar.PROXIMITY_FAR;
            blip.confidencePercent = 0;
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

    private void pruneStale(long nowUptimeMillis) {
        blipsByAddress.entrySet().removeIf(entry ->
                nowUptimeMillis - entry.getValue().lastSeenUptimeMillis > STALE_RETENTION_WINDOW_MILLIS);
    }
}

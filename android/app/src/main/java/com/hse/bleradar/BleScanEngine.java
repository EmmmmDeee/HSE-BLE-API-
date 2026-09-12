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
import java.util.Comparator;
import java.util.IdentityHashMap;
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
    private static final int MIN_ANDROID_VERSION_FOR_BLUETOOTH_PERMISSIONS = Build.VERSION_CODES.S;

    /** Long-idle devices are pruned to keep the simple UI focused on live signals. */
    private static final long STALE_RETENTION_WINDOW_MILLIS = 30_000L;
    private final Context appContext;
    private final Map<String, Blip> blipsByAddress = new ConcurrentHashMap<>();
    private final int calibrationProfile;
    private final int trackingProfile;
    private BluetoothLeScanner scanner;
    private volatile boolean scanning;
    private volatile long scanStartUptimeMillis = 0;

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
        this.trackingProfile = NativeRadar.isAvailable()
                ? NativeRadar.defaultTrackingProfile()
                : NativeRadar.TRACKING_STANDARD;
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
        if (Build.VERSION.SDK_INT >= MIN_ANDROID_VERSION_FOR_BLUETOOTH_PERMISSIONS) {
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
            scanStartUptimeMillis = SystemClock.uptimeMillis();
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

    /**
     * Returns milliseconds elapsed since scanning started, or 0 if not currently scanning.
     * Used for UI status display and API reporting.
     */
    long getUptimeMillis() {
        if (!scanning || scanStartUptimeMillis == 0) {
            return 0;
        }
        return SystemClock.uptimeMillis() - scanStartUptimeMillis;
    }

    /**
     * A defensive copy of every live device, ranked by the Rust-owned
     * {@link NativeRadar#deviceRankKey} policy (live before recent before stale,
     * then most recent, then most confident, then strongest).
     */
    List<Blip> snapshot() {
        pruneStale(SystemClock.uptimeMillis());
        List<Blip> snapshot = new ArrayList<>(blipsByAddress.values());
        // Keys are sampled once so the sort sees an immutable ordering while the
        // scan callback keeps mutating the volatile Blip fields concurrently
        // (a comparator reading them live can violate TimSort's contract and
        // throw). This holds for the no-native fallback too, which ranks on
        // recency alone because without the core every device is reported
        // LIVE with zero confidence.
        boolean nativeAvailable = NativeRadar.isAvailable();
        Map<Blip, Long> rankKeys = new IdentityHashMap<>();
        for (Blip blip : snapshot) {
            long lastSeen = blip.lastSeenUptimeMillis;
            rankKeys.put(blip, nativeAvailable
                    ? NativeRadar.deviceRankKey(blip.freshness, lastSeen, blip.confidencePercent, blip.lastRssiDbm)
                    : -lastSeen);
        }
        snapshot.sort(Comparator.comparingLong(rankKeys::get));
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
        double txPowerDbm = readTxPowerDbm(result);
        blip.txPowerDbm = txPowerDbm;

        if (NativeRadar.isAvailable()) {
            double smoothed = NativeRadar.trackingFilteredRssi(
                    previous,
                    rawRssi,
                    0.0,
                    Math.max(1, blip.sampleCount()),
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
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
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
            blip.distanceMetres = NativeRadar.trackingDistanceM(
                    previous,
                    rawRssi,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
            blip.distanceLowerBoundMetres = NativeRadar.trackingDistanceLowerBoundM(
                    previous,
                    rawRssi,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
            blip.distanceUpperBoundMetres = NativeRadar.trackingDistanceUpperBoundM(
                    previous,
                    rawRssi,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
            blip.proximity = NativeRadar.trackingDistanceProximity(
                    previous,
                    rawRssi,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
            int confidencePercent = NativeRadar.trackingConfidencePercent(
                    previous,
                    rawRssi,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
            blip.confidencePercent = Math.max(0, confidencePercent);
            blip.freshness = NativeRadar.trackingFreshness(
                    previous,
                    rawRssi,
                    spreadDb,
                    sampleCount,
                    calibrationProfile,
                    trackingProfile,
                    0L,
                    txPowerDbm);
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

    /**
     * The device's advertised/calibrated TX power in dBm, or {@link Double#NaN}
     * when the platform did not report one (including the
     * {@code ScanResult.TX_POWER_NOT_PRESENT} sentinel). {@link NativeRadar}
     * independently validates plausibility before using this as a per-device
     * calibration override, so no range filtering happens here.
     */
    private static double readTxPowerDbm(ScanResult result) {
        int txPower = result.getTxPower();
        return txPower == ScanResult.TX_POWER_NOT_PRESENT ? Double.NaN : txPower;
    }

    private String safeDeviceName(ScanResult result) {
        try {
            if (result.getScanRecord() != null && result.getScanRecord().getDeviceName() != null) {
                return result.getScanRecord().getDeviceName();
            }
            return result.getDevice().getName();
        } catch (SecurityException denied) {
            Log.w(TAG, "Bluetooth name permission revoked at query time");
            return null;
        }
    }

    /**
     * Re-classifies every device's freshness as of {@code nowUptimeMillis} and
     * drops the ones the Rust pruning policy rejects, in one pass over the map.
     */
    private void pruneStale(long nowUptimeMillis) {
        boolean nativeAvailable = NativeRadar.isAvailable();
        blipsByAddress.entrySet().removeIf(entry -> {
            Blip blip = entry.getValue();
            long ageMs = Math.max(0L, nowUptimeMillis - blip.lastSeenUptimeMillis);
            if (!nativeAvailable) {
                return ageMs > STALE_RETENTION_WINDOW_MILLIS;
            }
            blip.freshness = computeFreshness(blip, ageMs);
            return NativeRadar.deviceShouldPrune(blip.freshness);
        });
    }

    private int computeFreshness(Blip blip, long ageMs) {
        return NativeRadar.trackingFreshness(
                blip.lastRssiDbm,
                blip.lastRssiDbm,
                blip.recentRssiSpreadDb(),
                Math.max(1, blip.sampleCount()),
                calibrationProfile,
                trackingProfile,
                ageMs,
                blip.txPowerDbm);
    }
}

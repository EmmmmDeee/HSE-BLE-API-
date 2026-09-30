package com.hse.bleradar;

import android.Manifest;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothManager;
import android.bluetooth.le.BluetoothLeScanner;
import android.bluetooth.le.ScanCallback;
import android.bluetooth.le.ScanRecord;
import android.bluetooth.le.ScanResult;
import android.bluetooth.le.ScanSettings;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Owns the live {@link BluetoothLeScanner} session and the canonical
 * address-keyed device map. Every distance/proximity/trend value comes from
 * {@link NativeRadar}, i.e. from {@code bleradar-core}'s tested
 * implementation — this class only does BLE plumbing and bookkeeping (never
 * reimplements the signal/tracking math itself).
 */
final class BleScanEngine implements SnapshotSource {

    private static final String TAG = "BleScanEngine";
    private static final int MIN_ANDROID_VERSION_FOR_BLUETOOTH_PERMISSIONS = Build.VERSION_CODES.S;

    /** Long-idle devices are pruned to keep the simple UI focused on live signals. */
    private static final long STALE_RETENTION_WINDOW_MILLIS = 30_000L;
    private final Context appContext;
    private final Map<String, Blip> blipsByAddress = new ConcurrentHashMap<>();
    private final int calibrationProfile;
    private final int trackingProfile;
    private final DeviceHistory history;
    private BluetoothLeScanner scanner;
    /** Set by {@link #close()}: the owning service is gone, every later start is refused. */
    private boolean closed;
    private volatile long scanStartUptimeMillis = 0;
    private final Handler mainHandler = new Handler(Looper.getMainLooper());
    private BroadcastReceiver adapterReceiver;
    /** What the scan is asked to be and what the platform makes of it (see {@link ScanSupervisor}). */
    private final ScanSupervisor supervisor;

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
            supervisor.onScanFailed(errorCode);
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
        this.history = new DeviceHistory(appContext.getFilesDir());
        this.supervisor = new ScanSupervisor(
                new AndroidPlatform(),
                NativeRadar.isAvailable()
                        ? NativeRadar::scanFailureAction
                        : (code, retries) -> NativeRadar.SCAN_FAILURE_GIVE_UP);
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

    /** Whether the Bluetooth adapter exists and is enabled. */
    static boolean isBluetoothEnabled(Context context) {
        BluetoothManager manager = context.getSystemService(BluetoothManager.class);
        BluetoothAdapter adapter = manager == null ? null : manager.getAdapter();
        return adapter != null && adapter.isEnabled();
    }

    /**
     * @return {@code true} if scanning actually started. Synchronized because
     *     the HTTP handler thread ({@code POST /api/scan/start}) and the main
     *     thread ({@code onStartCommand}) may both request it.
     */
    synchronized boolean start() {
        if (closed) {
            Log.w(TAG, "Engine closed with its service; not starting scan");
            return false;
        }
        if (!hasRequiredPermissions(appContext)) {
            Log.w(TAG, "Missing required permissions; not starting scan");
            return false;
        }
        boolean started = supervisor.request();
        if (started) {
            watchAdapter();
        }
        return started;
    }

    /**
     * Stops scanning and refuses every later {@link #start()}: the service is
     * being destroyed, and a request its HTTP handler is still serving must
     * not leave a scan running in a dead instance (observed on the emulator
     * when the activity relaunched while the API was asked to start).
     */
    synchronized void close() {
        stop();
        closed = true;
        supervisor.close();
    }

    synchronized void stop() {
        supervisor.cancel();
        unwatchAdapter();
        history.flush(SystemClock.uptimeMillis());
    }

    @Override
    public ScanStatus scanStatus() {
        return supervisor.status();
    }

    /**
     * Milliseconds elapsed since scanning started, or 0 if not currently scanning.
     * Used for UI status display and API reporting.
     */
    @Override
    public long getUptimeMillis() {
        long started = scanStartUptimeMillis;
        if (!supervisor.status().isScanning() || started == 0) {
            return 0;
        }
        return SystemClock.uptimeMillis() - started;
    }

    /**
     * Follows the Bluetooth adapter for as long as a scan is wanted: the
     * platform's scanner does not survive the adapter cycling, and nothing
     * else restarts it.
     */
    private void watchAdapter() {
        if (adapterReceiver != null) {
            return;
        }
        BroadcastReceiver receiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {
                int state = intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR);
                if (state == BluetoothAdapter.STATE_OFF || state == BluetoothAdapter.STATE_TURNING_OFF) {
                    supervisor.onAdapter(false);
                } else if (state == BluetoothAdapter.STATE_ON) {
                    supervisor.onAdapter(true);
                }
            }
        };
        IntentFilter filter = new IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED);
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                appContext.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
            } else {
                appContext.registerReceiver(receiver, filter);
            }
            adapterReceiver = receiver;
        } catch (RuntimeException refused) {
            Log.w(TAG, "Bluetooth state receiver refused; the scan will not follow the adapter", refused);
        }
    }

    private void unwatchAdapter() {
        if (adapterReceiver == null) {
            return;
        }
        try {
            appContext.unregisterReceiver(adapterReceiver);
        } catch (IllegalArgumentException alreadyGone) {
            // Nothing further to release.
        }
        adapterReceiver = null;
    }

    /** The Android side of {@link ScanSupervisor}. */
    private final class AndroidPlatform implements ScanSupervisor.Platform {
        @Override
        public boolean startScanner() {
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
                scanStartUptimeMillis = SystemClock.uptimeMillis();
                return true;
            } catch (SecurityException error) {
                Log.w(TAG, "Scan permission revoked at call time", error);
                return false;
            } catch (RuntimeException error) {
                Log.w(TAG, "The platform refused to start the scan", error);
                return false;
            }
        }

        @Override
        public void stopScanner() {
            BluetoothLeScanner current = scanner;
            scanner = null;
            if (current == null) {
                return;
            }
            try {
                current.stopScan(scanCallback);
            } catch (RuntimeException error) {
                // Permission revoked, or the adapter already gone: nothing to release.
                Log.w(TAG, "stopScan failed; the registration is gone with its adapter", error);
            }
        }

        @Override
        public boolean adapterOn() {
            return isBluetoothEnabled(appContext);
        }

        @Override
        public void schedule(long delayMillis, Runnable action) {
            mainHandler.postDelayed(action, delayMillis);
        }

        @Override
        public void cancelScheduled() {
            mainHandler.removeCallbacksAndMessages(null);
        }
    }

    /**
     * A defensive copy of every live device, ranked by the Rust-owned
     * {@link NativeRadar#deviceRankKey} policy (live before recent before stale,
     * then most recent, then most confident, then strongest).
     */
    @Override
    public List<Blip> snapshot() {
        pruneStale(SystemClock.uptimeMillis());
        List<Blip> snapshot = new ArrayList<>(blipsByAddress.values());
        for (Blip blip : snapshot) {
            applyHistory(blip);
        }
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
        supervisor.onResult();
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

        // Classify the address once (it is fixed for the blip's life): a
        // rotating private address is a throwaway, not a followable physical
        // device. The platform's address type (API 35+) decides it; owned by
        // Rust (bleradar_core::ble_address_trackability) through the façade.
        if (NativeRadar.isAvailable() && blip.trackability == NativeRadar.TRACKABILITY_UNKNOWN) {
            blip.trackability = NativeRadar.deviceAddressTrackability(address, addressType(result));
        }

        // Decode the advertising payload in Rust (bleradar_core::adv): who made
        // the device and whether it is a beacon. Re-decoded only when the
        // advertisement changed (Eddystone, for one, rotates frames).
        ScanRecord record = result.getScanRecord();
        byte[] advertisement = record == null ? null : record.getBytes();
        if (NativeRadar.isAvailable() && advertisement != null) {
            String advertisementHex = hex(advertisement);
            if (!advertisementHex.equals(blip.lastAdvertisementHex)) {
                blip.advertisement = Blip.AdvSummary.decode(address, addressType(result), advertisementHex);
                blip.lastAdvertisementHex = advertisementHex;
            }
        }

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

        // Cross-session memory (bleradar_core::history): only a public address
        // is a stable key worth remembering; the Rust side enforces that too.
        if (blip.trackability == NativeRadar.TRACKABILITY_TRACKABLE) {
            history.observe(address.toLowerCase(Locale.ROOT), now);
        }

        String name = safeDeviceName(result);
        if (name != null) {
            blip.name = name;
        }
        pruneStale(now);
    }

    /** Copies what the persistent history remembers onto {@code blip}. */
    private void applyHistory(Blip blip) {
        blip.history = blip.trackability == NativeRadar.TRACKABILITY_TRACKABLE
                ? history.lookup(blip.address.toLowerCase(Locale.ROOT))
                : null;
    }

    /**
     * The scanned device's {@code BluetoothDevice.ADDRESS_TYPE_*} (public or
     * random), which only API 35+ reports; {@link NativeRadar#ADDRESS_TYPE_UNKNOWN}
     * before that.
     */
    private static int addressType(ScanResult result) {
        return Build.VERSION.SDK_INT >= 35
                ? result.getDevice().getAddressType()
                : NativeRadar.ADDRESS_TYPE_UNKNOWN;
    }

    private static final char[] HEX_DIGITS = "0123456789abcdef".toCharArray();

    /** Lowercase hex of {@code bytes}: the form the Rust advertisement decoder takes across JNI. */
    static String hex(byte[] bytes) {
        char[] out = new char[bytes.length * 2];
        for (int i = 0; i < bytes.length; i++) {
            int b = bytes[i] & 0xff;
            out[2 * i] = HEX_DIGITS[b >>> 4];
            out[2 * i + 1] = HEX_DIGITS[b & 0x0f];
        }
        return new String(out);
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

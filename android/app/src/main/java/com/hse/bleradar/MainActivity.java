package com.hse.bleradar;

import android.Manifest;
import android.app.AlertDialog;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.provider.Settings;
import android.text.SpannableStringBuilder;
import android.text.style.ForegroundColorSpan;
import android.text.style.RelativeSizeSpan;
import android.text.style.StyleSpan;
import android.view.View;
import android.view.ViewGroup;
import android.widget.BaseAdapter;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ListView;
import android.widget.TextView;

import java.util.ArrayList;
import java.util.List;

/**
 * Launcher activity. UI is built entirely with plain {@code android.widget}
 * views constructed in code (no XML layouts, no AndroidX) — see
 * docs/ANDROID_APP.md for why. Scanning itself is owned by
 * {@link RadarScanService}; this activity only binds to it and renders
 * whatever snapshot it reports, on a lightweight periodic timer.
 */
public final class MainActivity extends android.app.Activity {

    private static final int PERMISSION_REQUEST_CODE = 42;
    private static final long UI_REFRESH_INTERVAL_MILLIS = 400L;
    private static final long CHECK_INTERVAL_SECONDS = 86400L; // 1 day

    private RadarView radarView;
    private TextView statusText;
    private Button toggleButton;
    private ListView deviceListView;
    private DeviceListAdapter deviceListAdapter;

    private RadarScanService boundService;
    private boolean serviceBound;
    private final Handler uiHandler = new Handler(Looper.getMainLooper());

    private final ServiceConnection connection = new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder service) {
            boundService = ((RadarScanService.LocalBinder) service).getService();
            serviceBound = true;
            // The API exists from this moment: re-render the status line so
            // its dashboard URL appears even while the app stays idle.
            refreshStatus();
            refreshUiLoop();
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
            boundService = null;
            serviceBound = false;
            refreshStatus();
        }
    };

    /** The last status text asked for, without the lines {@link #setStatus} appends. */
    private CharSequence baseStatus = "";

    private final Runnable refreshTicker = this::refreshUiLoop;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        NativeRadar.ensureLoaded();
        setContentView(buildRootView());
        applyIdleStatus();
        requestNotificationPermissionIfNeeded();
    }

    private void requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, PERMISSION_REQUEST_CODE + 1);
        }
    }

    @Override
    protected void onStart() {
        super.onStart();
        // Bind only. The service is promoted to the foreground solely by a
        // scan request (see onToggleClicked / RadarScanService.onStartCommand),
        // which is the only point where the Bluetooth runtime permissions the
        // connectedDevice foreground type requires on API 34+ are known to be
        // granted; promoting here, before any grant, would throw there.
        bindService(serviceIntent(), connection, Context.BIND_AUTO_CREATE);

        // Check for app updates on each activity start, respecting the daily throttle.
        // UpdateManager uses shouldCheckForUpdate to gate the check (returns quickly
        // if not yet time), and UpdateCheckService uses its own START_NOT_STICKY logic
        // to self-exit when not needed, so this is a safe call on every resume.
        tryCheckForUpdates();
    }

    private Intent serviceIntent() {
        return new Intent(this, RadarScanService.class);
    }

    /**
     * Initiates an update check if enough time has passed since the last check.
     *
     * <p>This is called on every app resume, but UpdateManager.shouldCheckForUpdate respects
     * the configured throttle interval (daily by default), so the check service only
     * runs once per day. Any failures or deferred checks are logged but do not interrupt
     * the app's normal operation.
     */
    private void tryCheckForUpdates() {
        UpdateManager updateManager = new UpdateManager(this);
        // The check is guarded by shouldCheckForUpdate, which respects the daily throttle,
        // so this call returns quickly on most app launches.
        if (updateManager.shouldCheckForUpdate(CHECK_INTERVAL_SECONDS)) {
            // Use startForegroundService on Android 8+ to ensure reliable service startup
            Intent intent = new Intent(this, UpdateCheckService.class);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(intent);
            } else {
                startService(intent);
            }
        }
    }

    @Override
    protected void onStop() {
        uiHandler.removeCallbacks(refreshTicker);
        if (serviceBound) {
            unbindService(connection);
            serviceBound = false;
        }
        super.onStop();
    }

    private View buildRootView() {
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.parseColor("#0F1419"));
        root.setLayoutParams(new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        TextView title = new TextView(this);
        title.setText(R.string.app_name);
        title.setTextColor(Color.parseColor("#E7F3ED"));
        title.setTextSize(20f);
        title.setPadding(dp(16), dp(16), dp(16), dp(4));
        root.addView(title);

        statusText = new TextView(this);
        statusText.setTextColor(Color.parseColor("#7E9C90"));
        statusText.setTextSize(14f);
        statusText.setPadding(dp(16), 0, dp(16), dp(8));
        root.addView(statusText);

        radarView = new RadarView(this);
        LinearLayout.LayoutParams radarParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f);
        radarView.setLayoutParams(radarParams);
        root.addView(radarView);

        toggleButton = new Button(this);
        toggleButton.setText(R.string.action_start);
        toggleButton.setAllCaps(false);
        toggleButton.setTextSize(16f);
        toggleButton.setTextColor(Color.parseColor("#0F1419"));
        GradientDrawable buttonBackground = new GradientDrawable();
        buttonBackground.setColor(Color.parseColor("#39D98A"));
        buttonBackground.setCornerRadius(dp(8));
        toggleButton.setBackground(buttonBackground);
        LinearLayout.LayoutParams buttonParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        buttonParams.setMargins(dp(16), dp(8), dp(16), dp(8));
        toggleButton.setLayoutParams(buttonParams);
        toggleButton.setOnClickListener(this::onToggleClicked);
        root.addView(toggleButton);

        deviceListAdapter = new DeviceListAdapter();
        deviceListView = new ListView(this);
        deviceListView.setAdapter(deviceListAdapter);
        GradientDrawable listBackground = new GradientDrawable();
        listBackground.setColor(Color.parseColor("#162026"));
        listBackground.setCornerRadius(dp(8));
        deviceListView.setBackground(listBackground);
        deviceListView.setCacheColorHint(Color.TRANSPARENT);
        deviceListView.setSelector(new ColorDrawable(Color.TRANSPARENT));
        deviceListView.setDivider(new ColorDrawable(Color.parseColor("#337E9C90")));
        deviceListView.setDividerHeight(1);
        LinearLayout.LayoutParams listParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, dp(180));
        listParams.setMargins(dp(16), 0, dp(16), dp(16));
        deviceListView.setLayoutParams(listParams);
        root.addView(deviceListView);

        return root;
    }

    private int dp(int value) {
        float density = getResources().getDisplayMetrics().density;
        return Math.round(value * density);
    }

    private void onToggleClicked(View view) {
        if (boundService == null) {
            return;
        }
        if (boundService.isScanning()) {
            boundService.stopScanning();
            toggleButton.setText(R.string.action_start);
            applyIdleStatus();
            return;
        }
        if (!BleScanEngine.hasRequiredPermissions(this)) {
            requestPermissions(BleScanEngine.requiredPermissions(), PERMISSION_REQUEST_CODE);
            return;
        }
        if (!BleScanEngine.isBluetoothEnabled(this)) {
            setStatus(getString(R.string.status_bluetooth_off));
            return;
        }
        // Permissions are granted here, so the service may now be promoted to
        // a connectedDevice foreground service; the started state is what lets
        // a sticky restart resume the scan after process death.
        startForegroundService(serviceIntent());
        if (boundService.startScanning()) {
            toggleButton.setText(R.string.action_stop);
        } else {
            setStatus(getString(R.string.status_permission_required));
        }
    }

    private void applyIdleStatus() {
        setStatus(getString(R.string.status_idle));
    }

    /**
     * Every status line goes through here so a failed native-library load is
     * always visible: without {@code libbleradar_jni.so} the engine still
     * records raw RSSI, but every distance, bound, and confidence is
     * unavailable, and silently presenting that as a normal scan would turn a
     * failure into apparent success.
     */
    private void setStatus(CharSequence status) {
        baseStatus = status;
        StringBuilder text = new StringBuilder(status);
        String apiUrl = boundService == null ? null : boundService.apiUrl();
        if (apiUrl != null) {
            // The loopback dashboard and API exist only while the service is
            // alive; say where they are so a Termux user can find them.
            text.append('\n').append(getString(R.string.status_web_ui_fmt, apiUrl));
        }
        if (!NativeRadar.isAvailable()) {
            Throwable error = NativeRadar.loadError();
            String cause = error == null ? "unknown" : error.getClass().getSimpleName();
            text.append('\n').append(getString(R.string.status_native_unavailable, cause));
        }
        statusText.setText(text);
    }

    /** Re-renders the current status: the appended lines depend on the bound service. */
    private void refreshStatus() {
        setStatus(baseStatus);
    }

    private void refreshUiLoop() {
        if (boundService != null) {
            List<Blip> blips = boundService.snapshot();
            radarView.setBlips(blips);
            deviceListAdapter.replaceAll(blips);
            if (boundService.isScanning()) {
                setStatus(getString(R.string.status_scanning_fmt, blips.size()));
                toggleButton.setText(R.string.action_stop);
            } else if (getString(R.string.action_stop).contentEquals(toggleButton.getText())) {
                // The scan ended without a toggle (scan failure, or the sticky
                // service could not resume): reflect the real state.
                toggleButton.setText(R.string.action_start);
                applyIdleStatus();
            }
        }
        uiHandler.postDelayed(refreshTicker, UI_REFRESH_INTERVAL_MILLIS);
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode != PERMISSION_REQUEST_CODE) {
            return;
        }
        boolean allGranted = grantResults.length > 0;
        for (int result : grantResults) {
            allGranted &= result == PackageManager.PERMISSION_GRANTED;
        }
        if (allGranted) {
            onToggleClicked(toggleButton);
        } else {
            showPermissionRationale();
        }
    }

    private void showPermissionRationale() {
        new AlertDialog.Builder(this)
                .setTitle(R.string.permission_rationale_title)
                .setMessage(R.string.permission_rationale_message)
                .setPositiveButton(R.string.permission_open_settings, (dialog, which) -> {
                    Intent intent = new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS);
                    intent.setData(Uri.fromParts("package", getPackageName(), null));
                    startActivity(intent);
                })
                .setNegativeButton(android.R.string.cancel, null)
                .show();
    }

    /** Minimal {@link BaseAdapter} rendering each {@link Blip} as a simple two-line row. */
    private final class DeviceListAdapter extends BaseAdapter {
        private final List<Blip> items = new ArrayList<>();

        void replaceAll(List<Blip> newItems) {
            items.clear();
            items.addAll(newItems);
            notifyDataSetChanged();
        }

        @Override
        public int getCount() {
            return items.size();
        }

        @Override
        public Blip getItem(int position) {
            return items.get(position);
        }

        @Override
        public long getItemId(int position) {
            return items.get(position).address.hashCode();
        }

        @Override
        public View getView(int position, View convertView, ViewGroup parent) {
            TextView row = convertView instanceof TextView
                    ? (TextView) convertView
                    : new TextView(MainActivity.this);
            row.setPadding(dp(16), dp(10), dp(16), dp(10));
            row.setLineSpacing(dp(2), 1f);
            Blip blip = items.get(position);
            String name = blip.name != null && !blip.name.isEmpty()
                    ? blip.name
                    : getString(R.string.device_unnamed);
            String distance = describeRange(blip);
            String identityLine = name + "  ·  " + blip.address;
            String metricsLine = distance
                    + "   "
                    + freshnessLabel(blip)
                    + "   "
                    + confidenceLabel(blip)
                    + "   "
                    + trendLabel(blip.trend)
                    + " "
                    + Math.round(blip.lastRssiDbm)
                    + " dBm";

            SpannableStringBuilder text = new SpannableStringBuilder(identityLine);
            text.setSpan(new StyleSpan(Typeface.BOLD), 0, identityLine.length(), 0);
            text.setSpan(new ForegroundColorSpan(Color.parseColor("#E7F3ED")), 0, identityLine.length(), 0);

            text.append("\n");
            int metricsStart = text.length();
            text.append(metricsLine);
            text.setSpan(new ForegroundColorSpan(RadarView.colourForProximity(blip.proximity)),
                    metricsStart, text.length(), 0);
            text.setSpan(new RelativeSizeSpan(0.88f), metricsStart, text.length(), 0);

            row.setText(text);
            return row;
        }

        private String describeRange(Blip blip) {
            boolean hasLower = Double.isFinite(blip.distanceLowerBoundMetres);
            boolean hasUpper = Double.isFinite(blip.distanceUpperBoundMetres);
            if (hasLower && hasUpper) {
                return Math.round(blip.distanceLowerBoundMetres)
                        + "–"
                        + Math.round(blip.distanceUpperBoundMetres)
                        + "m";
            }
            if (Double.isNaN(blip.distanceMetres)) {
                return getString(R.string.format_distance_unknown);
            }
            return getString(R.string.format_distance_meters, Math.round(blip.distanceMetres) + "");
        }

        private String confidenceLabel(Blip blip) {
            return blip.confidencePercent > 0 ? blip.confidencePercent + "% conf" : "low conf";
        }

        private String freshnessLabel(Blip blip) {
            switch (blip.freshness) {
                case NativeRadar.FRESHNESS_LIVE:
                    return "live";
                case NativeRadar.FRESHNESS_RECENT:
                    return "recent";
                default:
                    return "stale";
            }
        }

        private String trendLabel(int trend) {
            switch (trend) {
                case NativeRadar.TREND_STRONGER:
                    return "↗";
                case NativeRadar.TREND_WEAKER:
                    return "↘";
                default:
                    return "→";
            }
        }
    }
}

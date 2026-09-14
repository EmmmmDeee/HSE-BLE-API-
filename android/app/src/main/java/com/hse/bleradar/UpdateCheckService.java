package com.hse.bleradar;

import android.app.AlarmManager;
import android.app.DownloadManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.SharedPreferences;
import android.content.pm.ServiceInfo;
import android.net.ConnectivityManager;
import android.net.NetworkCapabilities;
import android.net.Uri;
import android.os.BatteryManager;
import android.os.Build;
import android.os.IBinder;
import android.os.StatFs;
import android.util.Log;

import java.io.File;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;

/**
 * Background service that periodically checks for app updates using the
 * automatic-update decision core from {@code bleradar-core}.
 *
 * <p>This service orchestrates the update check lifecycle:
 * <ul>
 *   <li>On each check cycle, evaluates whether enough time has passed since the
 *       last check (using {@link UpdateManager#shouldCheckForUpdate}).</li>
 *   <li>If a check is due, fetches the release manifest from the repository's
 *       latest release ({@link #RELEASE_MANIFEST_URL}) on a worker thread and,
 *       when that fails, assesses the bundled manifest instead — retrying the
 *       fetch with backoff only for the faults {@link NativeRadar#remoteManifestDisposition}
 *       calls transient.</li>
 *   <li>Calls {@link UpdateManager#assessUpdate} to determine if the release is
 *       a safe upgrade.</li>
 *   <li>If an update is available, gates the download on network/battery/storage
 *       using {@link UpdateManager#checkDownloadReadiness}.</li>
 *   <li>On transient failures, uses {@link UpdateManager#computeRetryBackoff} to
 *       schedule the next retry with exponential backoff.</li>
 * </ul>
 *
 * <p>Network fetch and OS package installation are the platform boundary —
 * see {@code docs/AUTO_UPDATE.md}.
 */
public final class UpdateCheckService extends Service {

    private static final String TAG = "UpdateCheckService";
    private static final long BASE_BACKOFF_SECONDS = 300; // 5 minutes
    private static final long MAX_BACKOFF_SECONDS = 86400; // 24 hours
    private static final int MIN_BATTERY_PERCENT = 20;
    private static final long STORAGE_HEADROOM_BYTES = 100 * 1024 * 1024; // 100 MiB

    /**
     * Where a release announces itself: the {@code release_manifest.txt} asset
     * of the repository's latest GitHub release (a stable URL that redirects to
     * the asset). A repository without a release answers {@code 404}, which
     * the Rust disposition reads as "the bundled manifest, no retry" — so a
     * fresh install checks daily and never retries against nothing.
     */
    static final String RELEASE_MANIFEST_URL =
            "https://github.com/EmmmmDeee/HSE-BLE-API-/releases/latest/download/release_manifest.txt";
    private static final int MANIFEST_CONNECT_TIMEOUT_MS = 10_000;
    private static final int MANIFEST_READ_TIMEOUT_MS = 10_000;
    /** A manifest is a few hundred bytes; anything past this is not one. */
    private static final int MANIFEST_MAX_BYTES = 16 * 1024;

    private static final String PREFS_NAME = "UpdateCheckService";
    private static final String PREFS_DOWNLOAD_ID = "activeDownloadId";
    private static final String PREFS_RETRY_COUNT = "retryCount";
    private static final String PREFS_LAST_RETRY_TIME = "lastRetryTime";
    private static final String PREFS_PENDING_MANIFEST = "pendingManifest";

    private static final String CHANNEL_ID = "update_check";
    private static final int NOTIFICATION_ID = 2;

    private UpdateManager updateManager;
    private DownloadManager downloadManager;
    private SharedPreferences prefs;
    private volatile long activeDownloadId = -1;
    /** The one worker the check runs on: network I/O may not run on the main thread. */
    private final ExecutorService checkExecutor = Executors.newSingleThreadExecutor();
    /**
     * Guards {@link #checkInFlight} and {@link #latestStartId} together, so a
     * start absorbed into a running check is always the id that check's
     * finish() answers: the claim (start side) and the release (worker side)
     * each read and write both under this lock.
     */
    private final Object checkLock = new Object();
    /** Whether a check is running on the worker; a start that arrives meanwhile is served by it. */
    private boolean checkInFlight;
    /** The most recent start id, which the check's finish() answers. */
    private int latestStartId;

    @Override
    public void onCreate() {
        super.onCreate();
        updateManager = new UpdateManager(this);
        downloadManager = getSystemService(DownloadManager.class);
        prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
        createNotificationChannel();

        // Restore any pending download from a prior session
        restorePendingDownload();

        Log.d(TAG, "UpdateCheckService created");
    }

    /**
     * Restores a pending download from SharedPreferences if one was interrupted.
     * Re-registers the broadcast receiver to monitor for completion, using the stored manifest.
     */
    private void restorePendingDownload() {
        activeDownloadId = prefs.getLong(PREFS_DOWNLOAD_ID, -1);
        if (activeDownloadId != -1 && downloadManager != null) {
            Log.d(TAG, "Restoring pending download " + activeDownloadId);
            // Check if the download still exists
            DownloadManager.Query query = new DownloadManager.Query().setFilterById(activeDownloadId);
            android.database.Cursor cursor = downloadManager.query(query);
            if (cursor != null && cursor.moveToFirst()) {
                int status = cursor.getInt(cursor.getColumnIndex(DownloadManager.COLUMN_STATUS));
                if (status == DownloadManager.STATUS_SUCCESSFUL || status == DownloadManager.STATUS_FAILED) {
                    // Download is already complete; clear the saved state
                    clearDownloadId();
                    activeDownloadId = -1;
                } else {
                    // Download is still in progress; restore manifest and re-register the receiver
                    ReleaseManifest manifest = restoreDownloadManifest();
                    if (manifest != null) {
                        try {
                            // The start that follows this restore promotes the service
                            // (every start does); the receiver leaves that foreground and
                            // stops the service unconditionally when the download ends.
                            BroadcastReceiver receiver = new DownloadCompletionReceiver(manifest);
                            IntentFilter filter = new IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE);
                            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                                registerReceiver(receiver, filter, Context.RECEIVER_EXPORTED);
                            } else {
                                registerReceiver(receiver, filter);
                            }
                            Log.d(TAG, "Re-registered broadcast receiver for pending download with restored manifest");
                        } catch (Exception e) {
                            Log.w(TAG, "Failed to re-register download receiver; cancelling the orphaned download", e);
                            // Nothing can verify or install this job any more: cancel it
                            // so no orphan lingers and no duplicate is enqueued beside it,
                            // and reset the in-flight guard so later checks proceed.
                            abandonActiveDownload();
                        }
                    } else {
                        Log.w(TAG, "Could not restore manifest for pending download; clearing");
                        clearDownloadId();
                        activeDownloadId = -1;
                    }
                }
                cursor.close();
            } else if (cursor != null) {
                cursor.close();
                clearDownloadId();
                activeDownloadId = -1;
            }
        }
    }

    /**
     * Persists the download ID and manifest to SharedPreferences so they survive process termination.
     */
    private void saveDownloadState(long downloadId, ReleaseManifest manifest) {
        prefs.edit()
                .putLong(PREFS_DOWNLOAD_ID, downloadId)
                .putString(PREFS_PENDING_MANIFEST, manifest.serialize())
                .apply();
    }

    /**
     * Clears the persisted download ID and manifest from SharedPreferences.
     */
    private void clearDownloadId() {
        prefs.edit()
                .remove(PREFS_DOWNLOAD_ID)
                .remove(PREFS_PENDING_MANIFEST)
                .apply();
    }

    /**
     * Forgets the active download after it completed: clears the persisted
     * state and resets the in-flight guard, leaving the downloaded file to
     * the installer.
     */
    private void releaseActiveDownload() {
        clearDownloadId();
        activeDownloadId = -1;
    }

    /**
     * Gives up on the active download: cancels the DownloadManager job (which
     * also deletes its file) so it is neither orphaned without a receiver nor
     * duplicated by a later check, then forgets it like
     * {@link #releaseActiveDownload()}.
     */
    private void abandonActiveDownload() {
        if (activeDownloadId != -1 && downloadManager != null) {
            downloadManager.remove(activeDownloadId);
        }
        releaseActiveDownload();
    }

    /**
     * Restores the manifest for a pending download from SharedPreferences.
     */
    private ReleaseManifest restoreDownloadManifest() {
        String manifestText = prefs.getString(PREFS_PENDING_MANIFEST, null);
        if (manifestText == null) {
            return null;
        }
        return ReleaseManifest.parse(manifestText);
    }

    /**
     * Clears the retry count when a download succeeds or a check completes.
     */
    private void clearRetryCount() {
        prefs.edit().remove(PREFS_RETRY_COUNT).remove(PREFS_LAST_RETRY_TIME).apply();
    }

    /**
     * Triggered by an explicit action or alarm to perform an update check.
     * Handles both normal periodic checks and retry invocations via AlarmManager.
     * Every start promotes the service to the foreground first (all callers use
     * startForegroundService()); a retry additionally bypasses the daily throttle.
     */
    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (updateManager == null) {
            return START_NOT_STICKY;
        }

        boolean isRetry = intent != null && intent.getBooleanExtra("is_retry", false);
        // Every start of this service arrives through startForegroundService()
        // (MainActivity, BootCompletedReceiver, UpdateRetryReceiver; minSdk is
        // 26), and a service started that way must call startForeground()
        // within seconds or the platform kills the process
        // (ForegroundServiceDidNotStartInTimeException). Promoting only for
        // retries left the restored-download and download-in-flight paths
        // running un-promoted (COR-030): promote first, unconditionally, and
        // leave the foreground through finish() on every exit.
        promoteToForeground();

        if (activeDownloadId != -1) {
            // A download restored in onCreate() is still in flight; its receiver
            // verifies, installs, and finishes the service when it completes,
            // so the foreground entered above is kept until then.
            Log.d(TAG, "Download " + activeDownloadId + " still pending; not starting another check");
            return START_NOT_STICKY;
        }

        // One check at a time: a start that arrives while the worker is still
        // fetching or assessing is served by that check, whose finish() uses
        // the latest start id, so the service ends once and never mid-check.
        if (!claimCheck(startId)) {
            Log.d(TAG, "An update check is in flight; this start is served by it");
            return START_NOT_STICKY;
        }

        // Check if enough time has passed since the last check (skip for retries)
        if (!isRetry && !updateManager.shouldCheckForUpdate(UpdateManager.CHECK_INTERVAL_SECONDS)) {
            Log.d(TAG, "Not yet time for an update check; skipping");
            finish(releaseCheck());
            return START_NOT_STICKY;
        }

        // Record the check time (regardless of outcome)
        updateManager.recordCheckTime();

        // The manifest fetch is network I/O, which the main thread may not
        // perform (NetworkOnMainThreadException): the rest of the check runs
        // on the worker and ends the service itself.
        try {
            checkExecutor.execute(this::runCheck);
        } catch (RejectedExecutionException destroyed) {
            Log.w(TAG, "The check worker is gone; skipping this check", destroyed);
            finish(releaseCheck());
        }
        return START_NOT_STICKY;
    }

    /**
     * Records {@code startId} as the latest start and claims the check for it,
     * or — when a check is already in flight — leaves the start absorbed into
     * that check, which will finish with this id.
     */
    private boolean claimCheck(int startId) {
        synchronized (checkLock) {
            latestStartId = startId;
            if (checkInFlight) {
                return false;
            }
            checkInFlight = true;
            return true;
        }
    }

    /**
     * Releases the check and returns the start id its finish() must answer:
     * the latest start, absorbed ones included. Under the same lock as the
     * claim, so no start can be absorbed between the release and the read.
     */
    private int releaseCheck() {
        synchronized (checkLock) {
            checkInFlight = false;
            return latestStartId;
        }
    }

    /** The check past its throttle, on the worker thread; every exit ends the service. */
    private void runCheck() {
        try {
            ManifestSource source = resolveReleaseManifest();
            if (source.manifest == null) {
                Log.w(TAG, "Could not load a release manifest; skipping update check");
                endCheck(true);
                return;
            }

            // Assess whether the release is a safe upgrade
            int decision = updateManager.assessUpdate(source.manifest.getVersionCode(), source.manifest.getMinSdk());
            Log.d(TAG, "Update decision: " + decision + " (available=" + NativeRadar.UPDATE_AVAILABLE + ")");

            if (decision != NativeRadar.UPDATE_AVAILABLE) {
                Log.d(TAG, "No safe update available (decision=" + decision + ")");
                // A retry scheduled for the fetch keeps its count: the backoff
                // must keep growing until a fetch succeeds.
                if (!source.retryScheduled) {
                    clearRetryCount();
                }
                endCheck(true);
                return;
            }

            // Check download readiness: network, battery, storage
            // Detect actual device state for download gating
            int network = detectNetworkType();
            int battery = detectBatteryLevel();
            boolean charging = isCharging();
            long freeStorage = detectFreeStorage();
            boolean allowMetered = false;

            int readiness = updateManager.checkDownloadReadiness(
                    network, battery, charging, freeStorage, allowMetered,
                    MIN_BATTERY_PERCENT, STORAGE_HEADROOM_BYTES, source.manifest.getSizeBytes());

            if (readiness != NativeRadar.DOWNLOAD_READY) {
                Log.d(TAG, "Download not ready (readiness=" + readiness + "); will retry later");
                scheduleRetry();
                endCheck(true);
                return;
            }

            // Download is ready; do NOT clear retry count yet (only clear on successful verification)
            // Download and verify the APK
            Log.d(TAG, "Downloading update from: " + source.manifest.getUrl());
            // An enqueued download's completion receiver ends the service.
            endCheck(!downloadAndInstallUpdate(source.manifest));
        } catch (RuntimeException e) {
            Log.e(TAG, "Update check failed", e);
            endCheck(true);
        }
    }

    /**
     * Releases the in-flight guard and, unless a download now owns the
     * service, ends it with the latest start id: a start absorbed while the
     * check ran had no work of its own, so the service stops once the check
     * that served it is over — and a start that arrives after the release
     * claims its own check, which the older id cannot stop.
     */
    private void endCheck(boolean finishService) {
        int startId = releaseCheck();
        if (finishService) {
            finish(startId);
        }
    }

    /** The manifest a check assesses, and whether a fetch retry was scheduled for it. */
    private static final class ManifestSource {
        final ReleaseManifest manifest;
        final boolean retryScheduled;

        ManifestSource(ReleaseManifest manifest, boolean retryScheduled) {
            this.manifest = manifest;
            this.retryScheduled = retryScheduled;
        }
    }

    /**
     * The remote manifest when the fetch succeeds and the Rust core accepts
     * the text, else the bundled one — with a retry scheduled only when the
     * Rust disposition calls the failure transient. The one log line names
     * the source, what the fetch got and what was decided.
     */
    private ManifestSource resolveReleaseManifest() {
        if (!NativeRadar.isAvailable()) {
            // No manifest can be validated without the core; the fetch would
            // be wasted bytes.
            Log.w(TAG, "Native core unavailable; not fetching the remote manifest");
            return new ManifestSource(loadBundledManifest(), false);
        }
        ReleaseManifestSource.Fetch fetch = ReleaseManifestSource.fetch(
                RELEASE_MANIFEST_URL, MANIFEST_CONNECT_TIMEOUT_MS, MANIFEST_READ_TIMEOUT_MS, MANIFEST_MAX_BYTES);
        int failureKind = fetch.failureKind;
        String detail = fetch.detail;
        ReleaseManifest remote = null;
        if (failureKind == NativeRadar.MANIFEST_FETCH_ANSWERED && fetch.text != null) {
            remote = ReleaseManifest.parse(fetch.text);
            if (remote == null) {
                failureKind = NativeRadar.MANIFEST_FETCH_REJECTED;
                // parse() also yields null for a text Rust accepts whose numeric
                // field does not fit the app's signed types; that has no Rust
                // reason and is named as its own cause.
                String reason = NativeRadar.releaseManifestError(fetch.text);
                detail = detail + (reason != null
                        ? ", rejected by the Rust core: " + reason
                        : ", accepted by the Rust core but a numeric field does not fit the app's signed types");
            }
        }
        int disposition = NativeRadar.remoteManifestDisposition(fetch.httpStatus, failureKind);
        Log.i(TAG, "Remote manifest " + RELEASE_MANIFEST_URL + ": " + detail
                + " -> " + dispositionLabel(disposition));
        if (disposition == NativeRadar.MANIFEST_SOURCE_USE_REMOTE && remote != null) {
            return new ManifestSource(remote, false);
        }
        boolean retry = disposition == NativeRadar.MANIFEST_SOURCE_FALLBACK_RETRY;
        if (retry) {
            scheduleRetry();
        }
        return new ManifestSource(loadBundledManifest(), retry);
    }

    private static String dispositionLabel(int disposition) {
        switch (disposition) {
            case NativeRadar.MANIFEST_SOURCE_USE_REMOTE:
                return "using the remote manifest";
            case NativeRadar.MANIFEST_SOURCE_FALLBACK_RETRY:
                return "using the bundled manifest; retry scheduled";
            case NativeRadar.MANIFEST_SOURCE_FALLBACK_NO_RETRY:
                return "using the bundled manifest; no retry";
            default:
                return "unknown disposition " + disposition + "; using the bundled manifest";
        }
    }

    /**
     * Loads the bundled release manifest, {@code release_manifest.txt} in the
     * app's assets: the offline fallback that describes the shipped build, so
     * a check without the remote source concludes "up to date".
     *
     * <p>The stream is drained by {@link Streams#readAllBytes}, not
     * {@code InputStream.readAllBytes()}: that method exists only from API 33
     * and threw {@code NoSuchMethodError} — past this {@code catch (Exception)}
     * — on every API 26–32 device, on first launch.
     *
     * @return the parsed manifest, or null if loading or parsing fails
     */
    private ReleaseManifest loadBundledManifest() {
        try (InputStream in = getAssets().open("release_manifest.txt")) {
            String manifestText = new String(Streams.readAllBytes(in), StandardCharsets.UTF_8);
            return ReleaseManifest.parse(manifestText);
        } catch (Exception e) {
            Log.w(TAG, "Failed to load release manifest from assets", e);
            return null;
        }
    }

    @Override
    public IBinder onBind(Intent intent) {
        // Not bound; this is a started service only
        return null;
    }

    /**
     * Initiates a download of the update APK via DownloadManager, then verifies
     * and installs it when complete. Returns whether the download is under way
     * with its completion receiver registered — that receiver then ends the
     * service; on any failure the job is abandoned, a retry scheduled, and
     * {@code false} returned so the caller ends the service.
     */
    private boolean downloadAndInstallUpdate(ReleaseManifest manifest) {
        if (downloadManager == null) {
            Log.w(TAG, "DownloadManager unavailable");
            return false;
        }

        try {
            DownloadManager.Request request = new DownloadManager.Request(Uri.parse(manifest.getUrl()));
            request.setTitle("BLE Radar Update");
            request.setDescription("Downloading update...");
            request.setDestinationInExternalFilesDir(this, null, "update.apk");
            request.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED);

            activeDownloadId = downloadManager.enqueue(request);
            saveDownloadState(activeDownloadId, manifest);
            Log.d(TAG, "Enqueued download with ID " + activeDownloadId);

            // Register broadcast receiver for download completion
            BroadcastReceiver receiver = new DownloadCompletionReceiver(manifest);
            IntentFilter filter = new IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                registerReceiver(receiver, filter, Context.RECEIVER_EXPORTED);
            } else {
                registerReceiver(receiver, filter);
            }
            return true;
        } catch (Exception e) {
            Log.e(TAG, "Failed to start download", e);
            // enqueue() and saveDownloadState() may have succeeded before
            // registerReceiver() threw: cancel the receiverless job so it is
            // neither orphaned nor guarding out every later check, and retry.
            abandonActiveDownload();
            scheduleRetry();
            return false;
        }
    }

    private static String artifactVerdictLabel(int verdict) {
        switch (verdict) {
            case NativeRadar.ARTIFACT_VERIFIED:
                return "verified";
            case NativeRadar.ARTIFACT_MANIFEST_INVALID:
                return "manifest rejected by the Rust core";
            case NativeRadar.ARTIFACT_UNREADABLE:
                return "downloaded file missing or unreadable";
            case NativeRadar.ARTIFACT_SIZE_MISMATCH:
                return "size differs from the manifest";
            case NativeRadar.ARTIFACT_HASH_MISMATCH:
                return "SHA-256 differs from the manifest";
            default:
                return "unknown verdict " + verdict;
        }
    }

    /**
     * Hands the verified APK to the system package installer through the
     * {@code content://} URI {@link DownloadManager} serves for the download.
     * A {@code file://} URI throws {@code FileUriExposedException} on API 24+
     * (this app targets 34) and a FileProvider would need AndroidX, which this
     * build deliberately does not ship; the DownloadManager URI needs neither
     * and the installer reads it through the granted permission.
     */
    private boolean installApk(long downloadId) {
        try {
            Uri apkUri = downloadManager.getUriForDownloadedFile(downloadId);
            if (apkUri == null) {
                Log.e(TAG, "DownloadManager has no URI for download " + downloadId);
                return false;
            }
            Intent install = new Intent(Intent.ACTION_VIEW);
            install.setDataAndType(apkUri, "application/vnd.android.package-archive");
            install.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_GRANT_READ_URI_PERMISSION);
            startActivity(install);
            Log.d(TAG, "Handed APK to system installer");
            return true;
        } catch (Exception e) {
            Log.e(TAG, "Failed to install APK", e);
            return false;
        }
    }

    /**
     * Leaves the foreground entered by {@link #onStartCommand} and stops the
     * service. A start's own exit paths pass their {@code startId}; the
     * download-completion receiver passes {@code -1}, which
     * {@link #stopSelf(int)} treats as unconditional: the single active
     * download was the service's last work, and any newer start that met the
     * in-flight guard did nothing that a start-id check should protect.
     */
    private void finish(int startId) {
        stopForeground(Service.STOP_FOREGROUND_REMOVE);
        stopSelf(startId);
    }

    /**
     * Broadcast receiver that handles the completion of the single active
     * download and always ends the service afterwards.
     */
    private class DownloadCompletionReceiver extends BroadcastReceiver {
        private final ReleaseManifest manifest;

        DownloadCompletionReceiver(ReleaseManifest manifest) {
            this.manifest = manifest;
        }

        @Override
        public void onReceive(Context context, Intent intent) {
            long downloadId = intent.getLongExtra(DownloadManager.EXTRA_DOWNLOAD_ID, -1);
            if (downloadId != activeDownloadId) {
                return; // Not our download
            }

            try {
                DownloadManager.Query query = new DownloadManager.Query()
                        .setFilterById(downloadId);
                android.database.Cursor cursor = downloadManager.query(query);
                if (!cursor.moveToFirst()) {
                    // The row vanished (cleared by the user or the system); the
                    // check time was already recorded, so without a retry the
                    // update would wait a whole interval.
                    Log.e(TAG, "Download " + downloadId + " vanished from DownloadManager; retrying later");
                    cursor.close();
                    abandonActiveDownload();
                    scheduleRetry();
                    unregisterReceiver(this);
                    finish(-1);
                    return;
                }

                int status = cursor.getInt(cursor.getColumnIndex(DownloadManager.COLUMN_STATUS));
                if (status != DownloadManager.STATUS_SUCCESSFUL) {
                    int reason = cursor.getInt(cursor.getColumnIndex(DownloadManager.COLUMN_REASON));
                    Log.w(TAG, "Download failed with reason " + reason);
                    cursor.close();
                    abandonActiveDownload();
                    scheduleRetry();
                    unregisterReceiver(this);
                    finish(-1);
                    return;
                }

                String path = cursor.getString(cursor.getColumnIndex(DownloadManager.COLUMN_LOCAL_URI));
                cursor.close();

                // Extract file path from content URI
                Uri fileUri = Uri.parse(path);
                File apkFile = new File(fileUri.getPath());

                // Exact size and SHA-256 are checked by the Rust core's
                // ArtifactVerifier: the only path by which an artifact
                // becomes installable.
                boolean retryNeeded;
                int verdict = NativeRadar.artifactVerifyFile(apkFile.getAbsolutePath(), manifest.serialize());
                if (verdict == NativeRadar.ARTIFACT_VERIFIED) {
                    Log.d(TAG, "Artifact verification passed");
                    // A verified artifact that could not be handed to the
                    // installer (no DownloadManager URI, no installer activity)
                    // is retried like a failed download, never silently dropped.
                    retryNeeded = !installApk(downloadId);
                    if (!retryNeeded) {
                        clearRetryCount();
                    }
                } else {
                    Log.e(TAG, "Artifact verification failed: " + artifactVerdictLabel(verdict));
                    retryNeeded = true;
                }

                if (retryNeeded) {
                    scheduleRetry();
                }

                // The file stays for the installer; only the bookkeeping ends.
                releaseActiveDownload();
                unregisterReceiver(this);
                finish(-1);
            } catch (Exception e) {
                Log.e(TAG, "Error handling download completion", e);
                releaseActiveDownload();
                try {
                    unregisterReceiver(this);
                } catch (IllegalArgumentException ignored) {
                    // Already unregistered
                }
                finish(-1);
            }
        }
    }

    @Override
    public void onDestroy() {
        // A check still on the worker is interrupted mid-fetch; its endCheck()
        // then runs against a stopped service, where finish() is a no-op.
        checkExecutor.shutdownNow();
        Log.d(TAG, "UpdateCheckService destroyed");
        super.onDestroy();
    }

    /**
     * Promotes the service to foreground when started via startForegroundService().
     * This satisfies the Android 8+ requirement to call startForeground() within 5 seconds.
     * On API 34+ the {@code dataSync} type is required here and must match the
     * service's manifest declaration and the {@code FOREGROUND_SERVICE_DATA_SYNC}
     * permission; it is the public type for a download that outlives a short task.
     */
    private void promoteToForeground() {
        Notification notification = buildUpdateNotification();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
    }

    /**
     * Creates the notification channel for update check notifications.
     * Called once during onCreate().
     */
    private void createNotificationChannel() {
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                "App Updates",
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription("Notifications about app update checks and downloads");
        NotificationManager manager = getSystemService(NotificationManager.class);
        if (manager != null) {
            manager.createNotificationChannel(channel);
        }
    }

    /**
     * Builds a minimal notification for the update check foreground service.
     */
    private Notification buildUpdateNotification() {
        return new Notification.Builder(this, CHANNEL_ID)
                .setContentTitle("Checking for updates...")
                .setContentText("")
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setProgress(0, 0, true)
                .build();
    }

    /**
     * Detects the current network type (unmetered, metered, or none).
     *
     * @return one of {@link NativeRadar#NETWORK_NONE}, {@link NativeRadar#NETWORK_METERED},
     *         or {@link NativeRadar#NETWORK_UNMETERED}
     */
    private int detectNetworkType() {
        ConnectivityManager cm = getSystemService(ConnectivityManager.class);
        if (cm == null) {
            return NativeRadar.NETWORK_NONE;
        }
        android.net.Network network = cm.getActiveNetwork();
        if (network == null) {
            return NativeRadar.NETWORK_NONE;
        }
        NetworkCapabilities caps = cm.getNetworkCapabilities(network);
        if (caps == null) {
            return NativeRadar.NETWORK_NONE;
        }
        boolean isMetered = !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        return isMetered ? NativeRadar.NETWORK_METERED : NativeRadar.NETWORK_UNMETERED;
    }

    /**
     * Detects the current battery level as a percentage (0–100).
     *
     * @return battery level in percent, or 0 on error
     */
    private int detectBatteryLevel() {
        IntentFilter filter = new IntentFilter(Intent.ACTION_BATTERY_CHANGED);
        Intent batteryStatus = registerReceiver(null, filter);
        if (batteryStatus == null) {
            return 0;
        }
        int level = batteryStatus.getIntExtra(BatteryManager.EXTRA_LEVEL, 0);
        int scale = batteryStatus.getIntExtra(BatteryManager.EXTRA_SCALE, 100);
        return (level * 100) / Math.max(1, scale);
    }

    /**
     * Detects whether the device is currently charging.
     *
     * @return true if charging, false otherwise
     */
    private boolean isCharging() {
        IntentFilter filter = new IntentFilter(Intent.ACTION_BATTERY_CHANGED);
        Intent batteryStatus = registerReceiver(null, filter);
        if (batteryStatus == null) {
            return false;
        }
        int status = batteryStatus.getIntExtra(BatteryManager.EXTRA_STATUS, -1);
        return status == BatteryManager.BATTERY_STATUS_CHARGING
                || status == BatteryManager.BATTERY_STATUS_FULL;
    }

    /**
     * Detects the free storage space in the app's cache directory.
     *
     * @return free space in bytes, or 0 on error
     */
    private long detectFreeStorage() {
        try {
            StatFs stat = new StatFs(getCacheDir().getAbsolutePath());
            return stat.getAvailableBytes();
        } catch (Exception e) {
            Log.w(TAG, "Failed to detect free storage", e);
            return 0;
        }
    }

    /**
     * Schedules a retry of the update check using exponential backoff.
     * Increments the retry count and computes the next backoff interval.
     */
    private void scheduleRetry() {
        int retryCount = prefs.getInt(PREFS_RETRY_COUNT, 0);
        retryCount++;

        long backoffSeconds = updateManager.computeRetryBackoff(
                retryCount, BASE_BACKOFF_SECONDS, MAX_BACKOFF_SECONDS);

        Log.d(TAG, "Scheduling retry #" + retryCount + " in " + backoffSeconds + " seconds");

        prefs.edit()
                .putInt(PREFS_RETRY_COUNT, retryCount)
                .putLong(PREFS_LAST_RETRY_TIME, System.currentTimeMillis())
                .apply();

        AlarmManager alarmManager = getSystemService(AlarmManager.class);
        if (alarmManager == null) {
            Log.w(TAG, "AlarmManager unavailable; cannot schedule retry");
            return;
        }

        Intent retryIntent = new Intent(this, UpdateRetryReceiver.class);
        retryIntent.setAction(UpdateRetryReceiver.ACTION_UPDATE_RETRY);
        PendingIntent pendingIntent = PendingIntent.getBroadcast(
                this, 0, retryIntent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        long triggerAtMillis = System.currentTimeMillis() + (backoffSeconds * 1000);

        // Use setExactAndAllowWhileIdle for more reliable wake-up; falls back to set() if
        // SCHEDULE_EXACT_ALARM permission is not granted or device restrictions prevent it.
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, triggerAtMillis, pendingIntent);
            } else {
                alarmManager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, triggerAtMillis, pendingIntent);
            }
            Log.d(TAG, "Scheduled update retry at " + triggerAtMillis);
        } catch (SecurityException e) {
            Log.w(TAG, "setExactAndAllowWhileIdle failed due to permission; using set() instead", e);
            alarmManager.set(AlarmManager.RTC_WAKEUP, triggerAtMillis, pendingIntent);
        }
    }
}

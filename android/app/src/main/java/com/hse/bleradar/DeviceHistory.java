package com.hse.bleradar;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.logging.Level;
import java.util.logging.Logger;

/**
 * The persistent, cross-session memory of which devices the radar has seen:
 * when each was first seen and on how many separate visits. Every rule — which
 * keys are remembered (public addresses only), what counts as a new visit, the
 * size bound, how a damaged file is read — is owned by Rust
 * ({@code bleradar_core::history}) through {@link NativeRadar#historyMerge} and
 * {@link NativeRadar#historyLookup}; this class only loads, batches and
 * persists the text document.
 *
 * <p>The document lives in the app's private files directory, so it survives a
 * process kill, a reboot and an app upgrade, and needs no permission or
 * configuration. Writes go to a temporary file renamed over the old one, so an
 * interrupted write leaves the previous history intact.
 */
final class DeviceHistory {

    /** {@code java.util.logging}, which Android forwards to logcat, so this class also runs on a plain JVM. */
    private static final Logger LOG = Logger.getLogger("DeviceHistory");
    static final String FILE_NAME = "device_history.txt";
    /** Sightings of devices already known this session are merged at most this often. */
    private static final long FLUSH_INTERVAL_MILLIS = 10_000L;
    /**
     * A device not yet known this session is merged within about this long.
     * Scan results arrive on the main thread, so a crowd of new devices is
     * batched into one merge and one write rather than one per device.
     */
    private static final long NEW_DEVICE_FLUSH_INTERVAL_MILLIS = 1_000L;
    /** A history larger than this on disk is not read (the Rust bound keeps it far smaller). */
    private static final int MAX_FILE_BYTES = 1024 * 1024;

    /** What the history remembers about one device. */
    static final class Record {
        final long firstSeenEpochMillis;
        final int visits;

        Record(long firstSeenEpochMillis, int visits) {
            this.firstSeenEpochMillis = firstSeenEpochMillis;
            this.visits = visits;
        }
    }

    private final File file;
    private final Map<String, Record> known = new ConcurrentHashMap<>();
    private final Set<String> pending = new LinkedHashSet<>();
    private String state;
    /** Whether {@link #pending} holds a device not yet known this session. */
    private boolean pendingHasNewDevice;
    /** So the very first sighting merges at once. */
    private long lastFlushUptimeMillis = -FLUSH_INTERVAL_MILLIS;

    DeviceHistory(File directory) {
        this.file = new File(directory, FILE_NAME);
        this.state = load(file);
    }

    /**
     * Notes a sighting of {@code key} (a canonical public address) at
     * {@code nowUptimeMillis}. Sightings are batched: a pending device not yet
     * known this session is merged within {@link #NEW_DEVICE_FLUSH_INTERVAL_MILLIS},
     * known ones within {@link #FLUSH_INTERVAL_MILLIS}.
     */
    synchronized void observe(String key, long nowUptimeMillis) {
        if (key == null || !NativeRadar.isAvailable()) {
            return;
        }
        pending.add(key);
        if (!known.containsKey(key)) {
            pendingHasNewDevice = true;
        }
        long sinceFlush = nowUptimeMillis - lastFlushUptimeMillis;
        if (sinceFlush >= FLUSH_INTERVAL_MILLIS
                || (pendingHasNewDevice && sinceFlush >= NEW_DEVICE_FLUSH_INTERVAL_MILLIS)) {
            flush(nowUptimeMillis);
        }
    }

    /** What is remembered about {@code key}, or {@code null}. */
    Record lookup(String key) {
        return key == null ? null : known.get(key);
    }

    /** Merges every pending sighting and persists the result. */
    synchronized void flush(long nowUptimeMillis) {
        lastFlushUptimeMillis = nowUptimeMillis;
        if (pending.isEmpty() || !NativeRadar.isAvailable()) {
            return;
        }
        String keys = String.join("\n", pending);
        String merged = NativeRadar.historyMerge(state, keys, System.currentTimeMillis());
        if (merged == null) {
            return;
        }
        state = merged;
        String lines = NativeRadar.historyLookup(state, keys);
        if (lines != null) {
            String[] rows = lines.split("\n", -1);
            int i = 0;
            for (String key : pending) {
                Record record = i < rows.length ? parseRow(rows[i]) : null;
                if (record != null) {
                    known.put(key, record);
                }
                i++;
            }
        }
        pending.clear();
        pendingHasNewDevice = false;
        save(file, state);
    }

    /** {@code first_seen_ms\tvisits} → a record; an empty or malformed row → {@code null}. */
    static Record parseRow(String row) {
        int tab = row.indexOf('\t');
        if (tab <= 0) {
            return null;
        }
        try {
            long first = Long.parseLong(row.substring(0, tab));
            int visits = Integer.parseInt(row.substring(tab + 1));
            return first >= 0 && visits > 0 ? new Record(first, visits) : null;
        } catch (NumberFormatException malformed) {
            return null;
        }
    }

    private static String load(File file) {
        if (!file.isFile() || file.length() > MAX_FILE_BYTES) {
            return "";
        }
        try (InputStream in = new FileInputStream(file)) {
            byte[] bytes = Streams.readUpTo(in, MAX_FILE_BYTES);
            return bytes == null ? "" : new String(bytes, StandardCharsets.UTF_8);
        } catch (IOException unreadable) {
            LOG.log(Level.WARNING, "Device history unreadable; starting empty", unreadable);
            return "";
        }
    }

    private static void save(File file, String text) {
        File temporary = new File(file.getPath() + ".tmp");
        try (FileOutputStream out = new FileOutputStream(temporary)) {
            out.write(text.getBytes(StandardCharsets.UTF_8));
            out.getFD().sync();
        } catch (IOException failed) {
            LOG.log(Level.WARNING, "Device history not saved", failed);
            return;
        }
        if (!temporary.renameTo(file)) {
            LOG.warning("Device history not saved: rename failed");
        }
    }
}

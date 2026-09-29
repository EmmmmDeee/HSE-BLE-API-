package com.hse.bleradar;

import java.util.Arrays;
import java.util.Comparator;
import java.util.HashSet;
import java.util.Set;

/**
 * One surveyed Wi-Fi access point: what the platform reported ({@code ssid},
 * {@code bssid}, {@code frequencyMhz}, {@code rssiDbm}) and what the Rust core
 * concluded about it ({@code bleradar_core::wifi_observation}, through
 * {@link NativeRadar#wifiObservation}), decoded once by {@link #decode}.
 *
 * <p>Immutable, and free of {@code android.*}, so the host JVM can test it.
 * The labels are the API's vocabulary ({@code /api/wifi}): upper case, like
 * {@code /api/devices}. An unrecognised label from the core is a decode failure,
 * never a guess, so a core/app mismatch drops the row (and is counted) instead
 * of surveying something the app cannot describe.
 */
final class WifiAp {

    private static final Set<String> TRACKABILITY =
            new HashSet<>(Arrays.asList("TRACKABLE", "RANDOMIZED", "UNKNOWN"));
    private static final Set<String> RELIABILITY = new HashSet<>(
            Arrays.asList("VERY_HIGH_PLUS", "VERY_HIGH", "MEDIUM_PLUS", "LOW_MEDIUM"));
    private static final Set<String> PROXIMITY =
            new HashSet<>(Arrays.asList("IMMEDIATE", "NEAR", "MID", "FAR"));
    /** The core's {@code WifiSecurity::label} values, and the API label each becomes. */
    private static final String[][] SECURITY = {
        {"?", "UNKNOWN"}, {"Open", "OPEN"}, {"WEP", "WEP"}, {"WPA", "WPA"},
        {"WPA2", "WPA2"}, {"WPA3", "WPA3"}, {"OWE", "OWE"},
    };

    /** The number of {@code |}-separated fields {@code wifiObservation} answers. */
    static final int FIELD_COUNT = 6;

    /** The platform's BSSID, canonical lower case as reported. */
    final String bssid;
    /** The network name; empty for a hidden network. */
    final String ssid;
    final int frequencyMhz;
    final int rssiDbm;
    /** {@code TRACKABLE}, {@code RANDOMIZED} or {@code UNKNOWN}: the BSSID's U/L rule. */
    final String trackability;
    /** {@code VERY_HIGH_PLUS}, {@code VERY_HIGH}, {@code MEDIUM_PLUS} or {@code LOW_MEDIUM}. */
    final String reliability;
    /** The 802.11 channel, or {@code -1} when the frequency is outside the plan. */
    final int channel;
    /** {@code IMMEDIATE}, {@code NEAR}, {@code MID} or {@code FAR}; {@code null} when unknown. */
    final String proximity;
    /** {@code OPEN}, {@code OWE}, {@code WEP}, {@code WPA}, {@code WPA2}, {@code WPA3} or {@code UNKNOWN}. */
    final String security;
    final boolean enterprise;
    /** Wall-clock epoch milliseconds the platform last saw this access point. */
    final long lastSeenEpochMillis;
    /**
     * What the persistent history remembers about this access point (first seen,
     * visits), or {@code null}: only a trackable BSSID is remembered, and only
     * once the survey has merged it.
     */
    final DeviceHistory.Record history;

    private WifiAp(
            String bssid,
            String ssid,
            int frequencyMhz,
            int rssiDbm,
            String[] f,
            int channel,
            long lastSeenEpochMillis,
            DeviceHistory.Record history) {
        this.bssid = bssid;
        this.ssid = ssid;
        this.frequencyMhz = frequencyMhz;
        this.rssiDbm = rssiDbm;
        this.trackability = f[0];
        this.reliability = f[1];
        this.channel = channel;
        this.proximity = f[3].isEmpty() ? null : f[3];
        this.security = f[4];
        this.enterprise = "1".equals(f[5]);
        this.lastSeenEpochMillis = lastSeenEpochMillis;
        this.history = history;
    }

    /** This access point with {@code remembered} (possibly {@code null}) as its history. */
    WifiAp withHistory(DeviceHistory.Record remembered) {
        return new WifiAp(
                bssid, ssid, frequencyMhz, rssiDbm,
                new String[] {trackability, reliability, "", proximity == null ? "" : proximity,
                    security, enterprise ? "1" : "0"},
                channel, lastSeenEpochMillis, remembered);
    }

    /**
     * The survey's order: rows with a usable reading first, strongest signal
     * first, then by BSSID. A reading the core could not band (a corrupt positive
     * RSSI) has no proximity and ranks last, so a bad value never tops the list.
     */
    static final Comparator<WifiAp> STRONGEST_FIRST = (a, b) -> {
        boolean aUsable = a.proximity != null;
        boolean bUsable = b.proximity != null;
        if (aUsable != bUsable) {
            return aUsable ? -1 : 1;
        }
        if (a.rssiDbm != b.rssiDbm) {
            return Integer.compare(b.rssiDbm, a.rssiDbm);
        }
        return a.bssid.compareTo(b.bssid);
    };

    /**
     * The epoch time the platform saw an access point, from its scan timestamp
     * ({@code ScanResult.timestamp}: microseconds on the elapsed-realtime clock).
     * A missing or non-positive timestamp is not "seen at boot": it is read as
     * seen now, so a driver that reports 0 cannot date a first sighting to the
     * device's boot. A timestamp in the future, or older than the epoch itself,
     * is clamped rather than producing a negative or future time.
     */
    static long seenEpochMillis(long nowEpochMillis, long nowElapsedMillis, long scanTimestampMicros) {
        if (scanTimestampMicros <= 0L) {
            return nowEpochMillis;
        }
        long agoMillis = Math.max(0L, nowElapsedMillis - scanTimestampMicros / 1000L);
        return Math.max(0L, nowEpochMillis - agoMillis);
    }

    /**
     * One platform scan result through the Rust core
     * ({@link NativeRadar#wifiObservation}) into an access point, or {@code null}
     * when it names none. The single path the live engine and the API harness share.
     */
    static WifiAp observe(
            String bssid,
            String ssid,
            String capabilities,
            int rssiDbm,
            int frequencyMhz,
            long lastSeenEpochMillis) {
        return decode(
                bssid, ssid, frequencyMhz, rssiDbm, lastSeenEpochMillis,
                NativeRadar.wifiObservation(bssid, capabilities, rssiDbm, frequencyMhz));
    }

    /**
     * Builds an access point from the platform's reading and the core's answer
     * ({@code encoded}, six {@code |}-separated fields), or {@code null} when the
     * core answered {@code null} (not an access point) or anything malformed.
     */
    static WifiAp decode(
            String bssid,
            String ssid,
            int frequencyMhz,
            int rssiDbm,
            long lastSeenEpochMillis,
            String encoded) {
        if (bssid == null || encoded == null) {
            return null;
        }
        String[] f = encoded.split("\\|", -1);
        if (f.length != FIELD_COUNT) {
            return null;
        }
        String[] label = new String[FIELD_COUNT];
        label[0] = f[0].toUpperCase(java.util.Locale.ROOT);
        label[1] = f[1].toUpperCase(java.util.Locale.ROOT);
        label[3] = f[3].toUpperCase(java.util.Locale.ROOT);
        label[5] = f[5];
        label[4] = null;
        for (String[] pair : SECURITY) {
            if (pair[0].equals(f[4])) {
                label[4] = pair[1];
            }
        }
        int channel = -1;
        if (!f[2].isEmpty()) {
            try {
                channel = Integer.parseInt(f[2]);
            } catch (NumberFormatException error) {
                return null;
            }
            if (channel <= 0) {
                return null;
            }
        }
        boolean known = TRACKABILITY.contains(label[0])
                && RELIABILITY.contains(label[1])
                && (label[3].isEmpty() || PROXIMITY.contains(label[3]))
                && label[4] != null
                && ("0".equals(label[5]) || "1".equals(label[5]));
        if (!known) {
            return null;
        }
        return new WifiAp(
                bssid, ssid == null ? "" : ssid, frequencyMhz, rssiDbm, label, channel,
                lastSeenEpochMillis, null);
    }
}

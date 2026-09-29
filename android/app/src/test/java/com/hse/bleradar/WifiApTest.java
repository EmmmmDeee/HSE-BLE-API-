package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

import java.util.ArrayList;
import java.util.List;

/**
 * Unit tests for {@link WifiAp}: the Java end of the Wi-Fi survey. Every reading
 * rule is decided by the Rust core, so these run against the real native core
 * (the runner refuses to pass without it) and pin what the app does with its
 * answer: decode it strictly, and rank it so a bad reading never leads.
 */
public class WifiApTest {

    private static final long SEEN = 1_757_699_998_800L;

    private static WifiAp observe(String bssid, String ssid, String capabilities, int rssi, int mhz) {
        return WifiAp.observe(bssid, ssid, capabilities, rssi, mhz, SEEN);
    }

    @Test
    public void a_real_scan_result_carries_every_rule_the_core_applied() {
        WifiAp ap = observe("3c:5a:b4:11:22:04", "CorpNet", "[WPA2-EAP-CCMP][ESS]", -60, 5745);
        assertNotNull("a real BSSID is an access point", ap);
        assertEquals("bssid", "3c:5a:b4:11:22:04", ap.bssid);
        assertEquals("ssid", "CorpNet", ap.ssid);
        assertEquals("rssi", -60, ap.rssiDbm);
        assertEquals("frequency", 5745, ap.frequencyMhz);
        assertEquals("channel", 149, ap.channel);
        assertEquals("trackability", "TRACKABLE", ap.trackability);
        assertEquals("reliability", "VERY_HIGH", ap.reliability);
        assertEquals("proximity", "NEAR", ap.proximity);
        assertEquals("security", "WPA2", ap.security);
        assertTrue("802.1X is flagged", ap.enterprise);
        assertEquals("last seen", SEEN, ap.lastSeenEpochMillis);
    }

    @Test
    public void a_rotating_bssid_is_randomized_not_trackable() {
        WifiAp ap = observe("aa:bb:cc:dd:ee:02", "Guest", "[RSN-SAE-CCMP][ESS]", -78, 5180);
        assertNotNull("a randomized BSSID is still an access point", ap);
        assertEquals("trackability", "RANDOMIZED", ap.trackability);
        assertEquals("security", "WPA3", ap.security);
        assertFalse("not enterprise", ap.enterprise);
    }

    @Test
    public void a_permission_masked_or_malformed_bssid_names_no_access_point() {
        assertNull("permission-masked BSSID", observe("02:00:00:00:00:00", "masked", "[ESS]", -40, 2412));
        assertNull("all-zero BSSID", observe("00:00:00:00:00:00", "zero", "[ESS]", -40, 2412));
        assertNull("not a MAC", observe("nope", "junk", "[ESS]", -40, 2412));
        assertNull("null BSSID", observe(null, "null", "[ESS]", -40, 2412));
    }

    @Test
    public void silence_about_security_is_unknown_and_never_open() {
        assertEquals("null capabilities", "UNKNOWN",
                observe("3c:5a:b4:11:22:07", "x", null, -70, 2437).security);
        assertEquals("blank capabilities", "UNKNOWN",
                observe("3c:5a:b4:11:22:07", "x", "", -70, 2437).security);
        assertEquals("an explicit ESS marker is open", "OPEN",
                observe("3c:5a:b4:11:22:07", "x", "[ESS]", -70, 2437).security);
    }

    @Test
    public void a_frequency_outside_the_plan_has_no_channel_and_a_corrupt_rssi_no_proximity() {
        WifiAp odd = observe("3c:5a:b4:11:22:07", "x", "[ESS]", -70, 2400);
        assertEquals("no channel", -1, odd.channel);
        WifiAp corrupt = observe("3c:5a:b4:11:22:05", "x", "[WEP][ESS]", 5, 2412);
        assertNull("a positive RSSI is not banded", corrupt.proximity);
        assertEquals("and is the worst reliability", "LOW_MEDIUM", corrupt.reliability);
        assertEquals("security still read", "WEP", corrupt.security);
    }

    @Test
    public void a_hidden_network_has_an_empty_ssid_never_null() {
        assertEquals("empty SSID", "", observe("3c:5a:b4:11:22:03", "", "[ESS]", -90, 2462).ssid);
        assertEquals("null SSID", "", observe("3c:5a:b4:11:22:03", null, "[ESS]", -90, 2462).ssid);
    }

    @Test
    public void the_core_answers_exactly_the_fields_the_decoder_expects() {
        String answer = NativeRadar.wifiObservation("3c:5a:b4:11:22:01", "[ESS]", -48, 2437);
        assertNotNull("the core answers a real BSSID", answer);
        assertEquals("field count", WifiAp.FIELD_COUNT, answer.split("\\|", -1).length);
        assertNull("and nothing for a masked one",
                NativeRadar.wifiObservation("02:00:00:00:00:00", "[ESS]", -48, 2437));
    }

    @Test
    public void an_answer_the_app_cannot_describe_is_refused_never_guessed() {
        String good = "trackable|very_high|6|near|WPA2|0";
        assertNotNull("the well-formed answer decodes", WifiAp.decode("b", "s", 2437, -60, SEEN, good));
        String[] bad = {
            null,
            "",
            "trackable|very_high|6|near|WPA2",
            "trackable|very_high|6|near|WPA2|0|extra",
            "sideways|very_high|6|near|WPA2|0",
            "trackable|excellent|6|near|WPA2|0",
            "trackable|very_high|six|near|WPA2|0",
            "trackable|very_high|0|near|WPA2|0",
            "trackable|very_high|6|adjacent|WPA2|0",
            "trackable|very_high|6|near|WPA9|0",
            "trackable|very_high|6|near|WPA2|2",
        };
        for (String answer : bad) {
            assertNull("decoded " + answer, WifiAp.decode("b", "s", 2437, -60, SEEN, answer));
        }
        assertNull("no BSSID", WifiAp.decode(null, "s", 2437, -60, SEEN, good));
    }

    @Test
    public void carrying_a_history_changes_nothing_else_about_an_access_point() {
        DeviceHistory.Record remembered = new DeviceHistory.Record(1_757_000_000_000L, 3);
        WifiAp[] originals = {
            observe("3c:5a:b4:11:22:04", "CorpNet", "[WPA2-EAP-CCMP][ESS]", -60, 5745),
            observe("3c:5a:b4:11:22:05", "Corrupt", "[WEP][ESS]", 5, 2412),
            observe("3c:5a:b4:11:22:07", null, null, -70, 2400),
        };
        for (WifiAp original : originals) {
            assertNull("a fresh row has no history", original.history);
            WifiAp carried = original.withHistory(remembered);
            assertTrue("the history is attached", remembered == carried.history);
            assertEquals("bssid", original.bssid, carried.bssid);
            assertEquals("ssid", original.ssid, carried.ssid);
            assertEquals("frequency", original.frequencyMhz, carried.frequencyMhz);
            assertEquals("rssi", original.rssiDbm, carried.rssiDbm);
            assertEquals("channel", original.channel, carried.channel);
            assertEquals("trackability", original.trackability, carried.trackability);
            assertEquals("reliability", original.reliability, carried.reliability);
            assertEquals("proximity (null stays null)", original.proximity, carried.proximity);
            assertEquals("security", original.security, carried.security);
            assertEquals("enterprise", original.enterprise, carried.enterprise);
            assertEquals("last seen", original.lastSeenEpochMillis, carried.lastSeenEpochMillis);
            assertNull("and it can be taken away again", carried.withHistory(null).history);
        }
    }

    @Test
    public void the_survey_ranks_usable_readings_strongest_first_and_a_corrupt_one_last() {
        List<WifiAp> rows = new ArrayList<>();
        rows.add(observe("3c:5a:b4:11:22:05", "corrupt", "[WEP][ESS]", 5, 2412));
        rows.add(observe("3c:5a:b4:11:22:03", "weak", "[ESS]", -90, 2462));
        rows.add(observe("3c:5a:b4:11:22:01", "strong", "[ESS]", -48, 2437));
        rows.add(observe("3c:5a:b4:11:22:02", "strong-tie", "[ESS]", -48, 2437));
        rows.sort(WifiAp.STRONGEST_FIRST);
        assertEquals("strongest first", "3c:5a:b4:11:22:01", rows.get(0).bssid);
        assertEquals("ties by BSSID", "3c:5a:b4:11:22:02", rows.get(1).bssid);
        assertEquals("weaker next", "3c:5a:b4:11:22:03", rows.get(2).bssid);
        assertEquals("a corrupt reading last, not first", "3c:5a:b4:11:22:05", rows.get(3).bssid);
    }
}

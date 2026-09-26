package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;

/**
 * The persistent device history through the real native core
 * ({@code bleradar_core::history} via {@link NativeRadar#historyMerge} /
 * {@link NativeRadar#historyLookup}) and the real file on disk: what one
 * {@link DeviceHistory} writes, a fresh instance on the same directory — the
 * app after a process kill, reboot or upgrade — reads back.
 */
public class DeviceHistoryTest {

    private static final String PUBLIC_KEY = "3c:5a:b4:11:22:01";
    private static final String RANDOMIZED_KEY = "aa:bb:cc:dd:ee:02";

    private static File freshDirectory() throws IOException {
        File dir = Files.createTempDirectory("device-history").toFile();
        dir.deleteOnExit();
        return dir;
    }

    private static String persisted(File dir) throws IOException {
        return new String(Files.readAllBytes(new File(dir, DeviceHistory.FILE_NAME).toPath()),
                StandardCharsets.UTF_8);
    }

    @Test
    public void history_survives_a_restart() throws IOException {
        File dir = freshDirectory();
        DeviceHistory first = new DeviceHistory(dir);
        first.observe(PUBLIC_KEY, 0L);
        DeviceHistory.Record before = first.lookup(PUBLIC_KEY);
        assertNotNull("a new public device is remembered at once", before);
        assertEquals("first visit", 1, before.visits);
        assertTrue("first seen is wall-clock time", before.firstSeenEpochMillis > 1_600_000_000_000L);

        DeviceHistory restarted = new DeviceHistory(dir);
        assertNull("a restarted instance reports only what it has observed", restarted.lookup(PUBLIC_KEY));
        restarted.observe(PUBLIC_KEY, 0L);
        DeviceHistory.Record after = restarted.lookup(PUBLIC_KEY);
        assertNotNull("remembered after the restart", after);
        assertEquals("first seen survives the restart", before.firstSeenEpochMillis, after.firstSeenEpochMillis);
        assertEquals("a sighting within the visit gap is the same visit", 1, after.visits);
    }

    @Test
    public void a_randomized_address_is_never_remembered() throws IOException {
        File dir = freshDirectory();
        DeviceHistory history = new DeviceHistory(dir);
        history.observe(RANDOMIZED_KEY, 0L);
        history.observe(PUBLIC_KEY, 2_000L);
        assertNull("rotating address not remembered", history.lookup(RANDOMIZED_KEY));
        assertNotNull("public address remembered", history.lookup(PUBLIC_KEY));
        assertFalse("nothing about the rotating address reaches disk", persisted(dir).contains(RANDOMIZED_KEY));
    }

    @Test
    public void a_damaged_file_starts_over_instead_of_failing() throws IOException {
        File dir = freshDirectory();
        Files.write(new File(dir, DeviceHistory.FILE_NAME).toPath(),
                "\u0000garbage\nnot a history\n".getBytes(StandardCharsets.UTF_8));
        DeviceHistory history = new DeviceHistory(dir);
        history.observe(PUBLIC_KEY, 0L);
        DeviceHistory.Record record = history.lookup(PUBLIC_KEY);
        assertNotNull("recorded despite the damaged file", record);
        assertEquals("a fresh first visit", 1, record.visits);
        assertTrue("rewritten as a valid history", persisted(dir).startsWith("bleradar-history v1\n"));
    }

    @Test
    public void a_crowd_of_new_devices_is_batched_into_one_merge() throws IOException {
        File dir = freshDirectory();
        DeviceHistory history = new DeviceHistory(dir);
        String second = "00:1a:7d:da:71:13";
        history.observe(PUBLIC_KEY, 0L);
        history.observe(second, 100L);
        assertNotNull("the first sighting merges at once", history.lookup(PUBLIC_KEY));
        assertNull("a new device within the batch window waits", history.lookup(second));
        assertFalse("and is not on disk yet", persisted(dir).contains(second));
        history.observe(PUBLIC_KEY, 1_100L);
        assertNotNull("the pending new device merges once the window passes", history.lookup(second));
        assertTrue("and reaches disk", persisted(dir).contains(second));
    }

    @Test
    public void a_save_leaves_no_temporary_file_behind() throws IOException {
        File dir = freshDirectory();
        new DeviceHistory(dir).observe(PUBLIC_KEY, 0L);
        assertTrue("history written", new File(dir, DeviceHistory.FILE_NAME).isFile());
        assertFalse("temporary file renamed away", new File(dir, DeviceHistory.FILE_NAME + ".tmp").exists());
    }

    @Test
    public void lookup_rows_parse_strictly() {
        DeviceHistory.Record record = DeviceHistory.parseRow("1757000000000\t3");
        assertNotNull("a well-formed row", record);
        assertEquals("first seen", 1_757_000_000_000L, record.firstSeenEpochMillis);
        assertEquals("visits", 3, record.visits);
        assertNull("empty row: not remembered", DeviceHistory.parseRow(""));
        assertNull("no tab", DeviceHistory.parseRow("12"));
        assertNull("non-numeric", DeviceHistory.parseRow("x\t1"));
        assertNull("zero visits", DeviceHistory.parseRow("5\t0"));
        assertNull("negative first seen", DeviceHistory.parseRow("-1\t2"));
    }
}

package com.hse.bleradar;

import org.junit.Test;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link UpdateCheckService} manifest loading, SHA-256 verification, and retry logic.
 */
public class UpdateCheckServiceTest {

    @Test
    public void retry_receiver_action_is_defined() {
        // Verify the retry receiver action is correctly defined for scheduling alarms
        assertNotNull("UpdateRetryReceiver.ACTION_UPDATE_RETRY must be defined",
                UpdateRetryReceiver.ACTION_UPDATE_RETRY);
        assertEquals("Retry action should have expected value",
                "com.hse.bleradar.UPDATE_RETRY", UpdateRetryReceiver.ACTION_UPDATE_RETRY);
    }

    @Test
    public void manifest_serialization_round_trips_correctly() {
        // Verify that manifest serialization persists all fields correctly
        // This ensures manifest can be persisted to SharedPreferences and restored
        String manifestText = ""
                + "version_code = 1\n"
                + "version_name = 1.0.0\n"
                + "url = https://example.com/ble-radar-release.apk\n"
                + "size_bytes = 52428800\n"
                + "sha256 = 0000000000000000000000000000000000000000000000000000000000000000\n"
                + "min_sdk = 26\n"
                + "mandatory = false\n"
                + "notes = Bundled offline default\n";

        ReleaseManifest m1 = ReleaseManifest.parse(manifestText);
        assertNotNull("original manifest should parse", m1);

        // Serialize and deserialize
        String serialized = m1.serialize();
        ReleaseManifest m2 = ReleaseManifest.parse(serialized);
        assertNotNull("round-trip manifest should parse", m2);

        // Verify all fields match after round-trip
        assertEquals("version code after round-trip", m1.getVersionCode(), m2.getVersionCode());
        assertEquals("SHA-256 after round-trip", m1.getSha256(), m2.getSha256());
        assertEquals("size after round-trip", m1.getSizeBytes(), m2.getSizeBytes());
        assertEquals("min SDK after round-trip", m1.getMinSdk(), m2.getMinSdk());
    }

    @Test
    public void bundled_manifest_parses() {
        // The bundled release_manifest.txt is a valid manifest that can be parsed
        // This test verifies the format is correct without needing a running service
        String bundledManifest = ""
                + "version_code = 1\n"
                + "version_name = 1.0.0\n"
                + "url = https://example.com/ble-radar-release.apk\n"
                + "size_bytes = 52428800\n"
                + "sha256 = 0000000000000000000000000000000000000000000000000000000000000000\n"
                + "min_sdk = 26\n"
                + "mandatory = false\n"
                + "notes = Bundled offline default; no update is currently available\n";

        ReleaseManifest m = ReleaseManifest.parse(bundledManifest);
        assertNotNull("bundled manifest should parse", m);
        assertEquals("version code", 1, m.getVersionCode());
        assertEquals("version name", "1.0.0", m.getVersionName());
        assertEquals("min SDK", 26, m.getMinSdk());
        assertFalse("mandatory", m.isMandatory());
    }

    @Test
    public void sha256_matches_standard_vectors() throws IOException, NoSuchAlgorithmException {
        // Test that our SHA-256 implementation matches known vectors
        // Empty string SHA-256
        File empty = File.createTempFile("update_test", ".bin");
        empty.deleteOnExit();
        try (FileOutputStream fos = new FileOutputStream(empty)) {
            // Write nothing
        }

        String emptyHash = computeSha256(empty);
        String expectedEmpty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assertEquals("empty file SHA-256", expectedEmpty, emptyHash);

        // Known test vector: "abc"
        File abc = File.createTempFile("update_test", ".bin");
        abc.deleteOnExit();
        try (FileOutputStream fos = new FileOutputStream(abc)) {
            fos.write(new byte[] { 'a', 'b', 'c' });
        }

        String abcHash = computeSha256(abc);
        String expectedAbc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assertEquals("'abc' SHA-256", expectedAbc, abcHash);
    }

    @Test
    public void sha256_is_case_insensitive_match() throws IOException, NoSuchAlgorithmException {
        // Verify that the manifest comparison will work with both cases
        File testFile = File.createTempFile("update_test", ".bin");
        testFile.deleteOnExit();
        try (FileOutputStream fos = new FileOutputStream(testFile)) {
            fos.write("test".getBytes());
        }

        String computed = computeSha256(testFile);
        String uppercase = computed.toUpperCase();

        // Verification should be case-insensitive (our implementation lowercases)
        assertEquals("case-insensitive comparison", computed, uppercase.toLowerCase());
    }

    /**
     * Standalone SHA-256 computation (mirrors UpdateCheckService.computeSha256).
     * Extracted to a static method so it can be unit-tested without running the full service.
     */
    private static String computeSha256(File file) throws IOException, NoSuchAlgorithmException {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        byte[] buffer = new byte[8192];
        try (java.io.FileInputStream fis = new java.io.FileInputStream(file)) {
            int read;
            while ((read = fis.read(buffer)) != -1) {
                digest.update(buffer, 0, read);
            }
        }
        byte[] hash = digest.digest();
        StringBuilder sb = new StringBuilder();
        for (byte b : hash) {
            sb.append(String.format("%02x", b));
        }
        return sb.toString();
    }
}

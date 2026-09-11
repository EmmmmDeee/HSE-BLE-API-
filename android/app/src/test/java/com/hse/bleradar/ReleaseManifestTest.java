package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link ReleaseManifest} parsing and serialization.
 */
public class ReleaseManifestTest {

    private static final String VALID_MANIFEST = ""
            + "version_code = 2\n"
            + "version_name = 1.0.1\n"
            + "url = https://example.com/release.apk\n"
            + "size_bytes = 1048576\n"
            + "sha256 = 0000000000000000000000000000000000000000000000000000000000000000\n"
            + "min_sdk = 26\n"
            + "mandatory = false\n"
            + "notes = Bug fixes and stability improvements\n";

    @Test
    public void parses_valid_manifest() {
        ReleaseManifest m = ReleaseManifest.parse(VALID_MANIFEST);
        assertNotNull("manifest should parse", m);
        assertEquals("version code", 2, m.getVersionCode());
        assertEquals("version name", "1.0.1", m.getVersionName());
        assertEquals("URL", "https://example.com/release.apk", m.getUrl());
        assertEquals("size", 1048576, m.getSizeBytes());
        assertEquals("SHA256", "0000000000000000000000000000000000000000000000000000000000000000", m.getSha256());
        assertEquals("minSdk", 26, m.getMinSdk());
        assertFalse("mandatory", m.isMandatory());
        assertEquals("notes", "Bug fixes and stability improvements", m.getNotes());
    }

    @Test
    public void rejects_null_input() {
        ReleaseManifest m = ReleaseManifest.parse(null);
        assertNull("null input should return null", m);
    }

    @Test
    public void rejects_empty_input() {
        ReleaseManifest m = ReleaseManifest.parse("");
        assertNull("empty input should return null", m);
    }

    @Test
    public void rejects_missing_required_field() {
        String noVersionCode = VALID_MANIFEST.replaceAll("version_code = 2\n", "");
        ReleaseManifest m = ReleaseManifest.parse(noVersionCode);
        assertNull("missing version_code should return null", m);
    }

    @Test
    public void rejects_non_https_url() {
        String badUrl = VALID_MANIFEST.replace("https://", "http://");
        ReleaseManifest m = ReleaseManifest.parse(badUrl);
        assertNull("non-HTTPS URL should return null", m);
    }

    @Test
    public void rejects_invalid_sha256() {
        String badSha = VALID_MANIFEST.replace(
                "sha256 = 0000000000000000000000000000000000000000000000000000000000000000",
                "sha256 = not_hex");
        ReleaseManifest m = ReleaseManifest.parse(badSha);
        assertNull("invalid SHA256 should return null", m);
    }

    @Test
    public void rejects_sha256_wrong_length() {
        String badSha = VALID_MANIFEST.replace(
                "sha256 = 0000000000000000000000000000000000000000000000000000000000000000",
                "sha256 = 000000000000000000000000000000000000000000000000000000000000000");
        ReleaseManifest m = ReleaseManifest.parse(badSha);
        assertNull("SHA256 with wrong length should return null", m);
    }

    @Test
    public void rejects_zero_size() {
        String zeroSize = VALID_MANIFEST.replace("size_bytes = 1048576", "size_bytes = 0");
        ReleaseManifest m = ReleaseManifest.parse(zeroSize);
        assertNull("zero size should return null", m);
    }

    @Test
    public void rejects_negative_size() {
        String negSize = VALID_MANIFEST.replace("size_bytes = 1048576", "size_bytes = -1");
        ReleaseManifest m = ReleaseManifest.parse(negSize);
        assertNull("negative size should return null", m);
    }

    @Test
    public void ignores_comments() {
        String withComments = "# This is a comment\n" + VALID_MANIFEST + "# Another comment\n";
        ReleaseManifest m = ReleaseManifest.parse(withComments);
        assertNotNull("manifest with comments should parse", m);
        assertEquals("version code", 2, m.getVersionCode());
    }

    @Test
    public void ignores_blank_lines() {
        String withBlanks = "\n\n" + VALID_MANIFEST + "\n\n";
        ReleaseManifest m = ReleaseManifest.parse(withBlanks);
        assertNotNull("manifest with blank lines should parse", m);
    }

    @Test
    public void normalizes_sha256_to_lowercase() {
        String uppercase = VALID_MANIFEST.replace(
                "sha256 = 0000000000000000000000000000000000000000000000000000000000000000",
                "sha256 = ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD");
        ReleaseManifest m = ReleaseManifest.parse(uppercase);
        assertNotNull("uppercase SHA256 should parse", m);
        assertEquals("SHA256 should be lowercase", "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd", m.getSha256());
    }

    @Test
    public void defaults_mandatory_to_false() {
        String noMandatory = VALID_MANIFEST.replaceAll("mandatory = false\n", "");
        ReleaseManifest m = ReleaseManifest.parse(noMandatory);
        assertNotNull("manifest without mandatory field should parse", m);
        assertFalse("mandatory should default to false", m.isMandatory());
    }

    @Test
    public void handles_mandatory_true() {
        String mandatory = VALID_MANIFEST.replace("mandatory = false", "mandatory = true");
        ReleaseManifest m = ReleaseManifest.parse(mandatory);
        assertNotNull("manifest with mandatory=true should parse", m);
        assertTrue("mandatory should be true", m.isMandatory());
    }

    @Test
    public void serializes_and_parses_round_trip() {
        ReleaseManifest m1 = ReleaseManifest.parse(VALID_MANIFEST);
        assertNotNull("original should parse", m1);
        String serialized = m1.serialize();
        ReleaseManifest m2 = ReleaseManifest.parse(serialized);
        assertNotNull("round-trip should parse", m2);
        assertEquals("version code", m1.getVersionCode(), m2.getVersionCode());
        assertEquals("version name", m1.getVersionName(), m2.getVersionName());
        assertEquals("URL", m1.getUrl(), m2.getUrl());
        assertEquals("size", m1.getSizeBytes(), m2.getSizeBytes());
        assertEquals("SHA256", m1.getSha256(), m2.getSha256());
        assertEquals("minSdk", m1.getMinSdk(), m2.getMinSdk());
        assertEquals("mandatory", m1.isMandatory(), m2.isMandatory());
        assertEquals("notes", m1.getNotes(), m2.getNotes());
    }

    @Test
    public void handles_extra_whitespace() {
        String withSpaces = VALID_MANIFEST.replace("version_code = 2", "version_code   =   2  ");
        ReleaseManifest m = ReleaseManifest.parse(withSpaces);
        assertNotNull("manifest with extra whitespace should parse", m);
        assertEquals("version code", 2, m.getVersionCode());
    }
}

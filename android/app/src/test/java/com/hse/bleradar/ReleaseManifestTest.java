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

    @Test
    public void boundary_version_code_one() {
        // versionCode = 1 is the minimum valid version
        String minVersion = VALID_MANIFEST.replace("version_code = 2", "version_code = 1");
        ReleaseManifest m = ReleaseManifest.parse(minVersion);
        assertNotNull("versionCode=1 should parse", m);
        assertEquals("version code", 1, m.getVersionCode());
    }

    @Test
    public void boundary_size_bytes_one() {
        // sizeBytes = 1 is the minimum valid size (non-zero)
        String minSize = VALID_MANIFEST.replace("size_bytes = 1048576", "size_bytes = 1");
        ReleaseManifest m = ReleaseManifest.parse(minSize);
        assertNotNull("sizeBytes=1 should parse", m);
        assertEquals("size", 1, m.getSizeBytes());
    }

    @Test
    public void boundary_min_sdk_one() {
        // minSdk = 1 is the minimum valid SDK level
        String minSdk = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = 1");
        ReleaseManifest m = ReleaseManifest.parse(minSdk);
        assertNotNull("minSdk=1 should parse", m);
        assertEquals("minSdk", 1, m.getMinSdk());
    }

    @Test
    public void rejects_min_sdk_zero() {
        // minSdk = 0 is invalid (must be >= 1)
        String zeroSdk = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = 0");
        ReleaseManifest m = ReleaseManifest.parse(zeroSdk);
        assertNull("minSdk=0 should be rejected", m);
    }

    @Test
    public void rejects_negative_min_sdk() {
        // minSdk = -1 is invalid
        String negativeSdk = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = -1");
        ReleaseManifest m = ReleaseManifest.parse(negativeSdk);
        assertNull("negative minSdk should be rejected", m);
    }

    @Test
    public void large_version_code_parses() {
        // versionCode can be very large (monotonically increasing)
        String largeVersion = VALID_MANIFEST.replace("version_code = 2", "version_code = 999999999");
        ReleaseManifest m = ReleaseManifest.parse(largeVersion);
        assertNotNull("large versionCode should parse", m);
        assertEquals("version code", 999999999, m.getVersionCode());
    }

    @Test
    public void large_size_bytes_parses() {
        // sizeBytes can be very large (gigabytes)
        String largeSize = VALID_MANIFEST.replace("size_bytes = 1048576", "size_bytes = 1073741824");
        ReleaseManifest m = ReleaseManifest.parse(largeSize);
        assertNotNull("large sizeBytes should parse", m);
        assertEquals("size", 1073741824, m.getSizeBytes());
    }

    @Test
    public void large_min_sdk_parses() {
        // minSdk can be large (future Android versions)
        String largeSdk = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = 99");
        ReleaseManifest m = ReleaseManifest.parse(largeSdk);
        assertNotNull("large minSdk should parse", m);
        assertEquals("minSdk", 99, m.getMinSdk());
    }

    @Test
    public void url_with_query_parameters_parses() {
        // URLs with query parameters should be accepted (HTTPS)
        String urlWithParams = VALID_MANIFEST.replace(
                "url = https://example.com/release.apk",
                "url = https://example.com/release.apk?version=1&token=abc123");
        ReleaseManifest m = ReleaseManifest.parse(urlWithParams);
        assertNotNull("URL with query parameters should parse", m);
        assertTrue("URL should be preserved", m.getUrl().contains("?"));
    }

    @Test
    public void url_with_port_parses() {
        // URLs with explicit ports should be accepted
        String urlWithPort = VALID_MANIFEST.replace(
                "url = https://example.com/release.apk",
                "url = https://example.com:443/release.apk");
        ReleaseManifest m = ReleaseManifest.parse(urlWithPort);
        assertNotNull("URL with port should parse", m);
        assertTrue("Port should be preserved", m.getUrl().contains(":443"));
    }

    @Test
    public void url_with_fragment_parses() {
        // URLs with fragments are unusual but should parse
        String urlWithFragment = VALID_MANIFEST.replace(
                "url = https://example.com/release.apk",
                "url = https://example.com/release.apk#section");
        ReleaseManifest m = ReleaseManifest.parse(urlWithFragment);
        assertNotNull("URL with fragment should parse", m);
        assertTrue("Fragment should be preserved", m.getUrl().contains("#"));
    }

    @Test
    public void sha256_all_zeros_parses() {
        // SHA-256 of all zeros is valid hex
        String allZeros = VALID_MANIFEST.replace(
                "sha256 = 0000000000000000000000000000000000000000000000000000000000000000",
                "sha256 = 0000000000000000000000000000000000000000000000000000000000000000");
        ReleaseManifest m = ReleaseManifest.parse(allZeros);
        assertNotNull("all-zero SHA-256 should parse", m);
        assertEquals("SHA256", "0000000000000000000000000000000000000000000000000000000000000000", m.getSha256());
    }

    @Test
    public void sha256_all_f_parses() {
        // SHA-256 of all F's is valid hex
        String allFs = VALID_MANIFEST.replace(
                "sha256 = 0000000000000000000000000000000000000000000000000000000000000000",
                "sha256 = ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
        ReleaseManifest m = ReleaseManifest.parse(allFs);
        assertNotNull("all-F SHA-256 should parse", m);
        assertEquals("SHA256 should be lowercase", "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", m.getSha256());
    }

    @Test
    public void version_name_empty_string_parses() {
        // version_name can be empty (unusual but valid)
        String emptyName = VALID_MANIFEST.replace("version_name = 1.0.1", "version_name = ");
        ReleaseManifest m = ReleaseManifest.parse(emptyName);
        assertNotNull("empty version_name should parse", m);
        assertEquals("version_name", "", m.getVersionName());
    }

    @Test
    public void version_name_with_special_chars_parses() {
        // version_name can contain special characters
        String specialName = VALID_MANIFEST.replace("version_name = 1.0.1", "version_name = 1.0.1-beta+build.123");
        ReleaseManifest m = ReleaseManifest.parse(specialName);
        assertNotNull("version_name with special chars should parse", m);
        assertTrue("special chars should be preserved", m.getVersionName().contains("-beta"));
    }

    @Test
    public void notes_with_special_chars_parses() {
        // notes can contain punctuation, special characters
        String specialNotes = VALID_MANIFEST.replace(
                "notes = Bug fixes and stability improvements",
                "notes = Fixed #123, #456 (critical); also: security & UI");
        ReleaseManifest m = ReleaseManifest.parse(specialNotes);
        assertNotNull("notes with special chars should parse", m);
        assertTrue("special chars should be preserved", m.getNotes().contains("#123"));
    }

    @Test
    public void notes_empty_string_parses() {
        // notes field is optional; empty string should parse as empty notes
        String emptyNotes = VALID_MANIFEST.replace("notes = Bug fixes and stability improvements", "notes = ");
        ReleaseManifest m = ReleaseManifest.parse(emptyNotes);
        assertNotNull("empty notes should parse", m);
        assertEquals("notes", "", m.getNotes());
    }

    @Test
    public void missing_optional_notes_defaults_to_empty() {
        // If notes field is completely absent, it defaults to ""
        String noNotes = VALID_MANIFEST.replaceAll("notes = Bug fixes and stability improvements\n", "");
        ReleaseManifest m = ReleaseManifest.parse(noNotes);
        assertNotNull("missing notes field should parse", m);
        assertEquals("notes", "", m.getNotes());
    }

    @Test
    public void mandatory_false_explicit_parses() {
        // Explicitly setting mandatory = false should parse
        String explicitFalse = VALID_MANIFEST.replace("mandatory = false", "mandatory = false");
        ReleaseManifest m = ReleaseManifest.parse(explicitFalse);
        assertNotNull("explicit mandatory=false should parse", m);
        assertFalse("mandatory", m.isMandatory());
    }

    @Test
    public void mandatory_true_parses() {
        // Explicitly setting mandatory = true should parse
        String explicitTrue = VALID_MANIFEST.replace("mandatory = false", "mandatory = true");
        ReleaseManifest m = ReleaseManifest.parse(explicitTrue);
        assertNotNull("explicit mandatory=true should parse", m);
        assertTrue("mandatory", m.isMandatory());
    }

    @Test
    public void mandatory_invalid_value_parses_as_false() {
        // Boolean.parseBoolean("invalid") returns false (not an error)
        String invalidBool = VALID_MANIFEST.replace("mandatory = false", "mandatory = maybe");
        ReleaseManifest m = ReleaseManifest.parse(invalidBool);
        assertNotNull("invalid mandatory value should parse (default to false)", m);
        assertFalse("mandatory", m.isMandatory());
    }

    @Test
    public void field_with_multiple_equals_uses_first() {
        // Only the first '=' is used to split key from value
        // Additional '=' in the value are preserved
        String multiEquals = VALID_MANIFEST.replace(
                "url = https://example.com/release.apk",
                "url = https://example.com/release.apk?key=value&other=data");
        ReleaseManifest m = ReleaseManifest.parse(multiEquals);
        assertNotNull("multiple equals in value should parse", m);
        assertTrue("all equals should be preserved", m.getUrl().contains("?key=value&other=data"));
    }

    @Test
    public void line_with_no_equals_is_skipped() {
        // Lines without an '=' are skipped as malformed (logged as warning)
        String malformedLine = VALID_MANIFEST + "garbage line without equals\n";
        ReleaseManifest m = ReleaseManifest.parse(malformedLine);
        assertNotNull("manifest with malformed line should still parse", m);
        assertEquals("version code", 2, m.getVersionCode());
    }

    @Test
    public void unknown_field_is_ignored() {
        // Unknown fields like "author = someone" are silently ignored
        String unknownField = VALID_MANIFEST + "author = John Doe\n";
        ReleaseManifest m = ReleaseManifest.parse(unknownField);
        assertNotNull("unknown field should be ignored", m);
        assertEquals("version code", 2, m.getVersionCode());
    }

    @Test
    public void key_whitespace_trimmed() {
        // Key names are trimmed of leading/trailing whitespace
        String trimmedKey = VALID_MANIFEST.replace("version_code = 2", "  version_code  = 2");
        ReleaseManifest m = ReleaseManifest.parse(trimmedKey);
        assertNotNull("key with extra whitespace should parse", m);
        assertEquals("version code", 2, m.getVersionCode());
    }

    @Test
    public void value_whitespace_trimmed() {
        // Values are trimmed of leading/trailing whitespace
        String trimmedValue = VALID_MANIFEST.replace("version_code = 2", "version_code =   2   ");
        ReleaseManifest m = ReleaseManifest.parse(trimmedValue);
        assertNotNull("value with extra whitespace should parse", m);
        assertEquals("version code", 2, m.getVersionCode());
    }

    @Test
    public void carriage_return_line_feed_handled() {
        // Manifests might have CRLF line endings
        String crlfManifest = VALID_MANIFEST.replace("\n", "\r\n");
        ReleaseManifest m = ReleaseManifest.parse(crlfManifest);
        // split("\n") leaves \r in place, but trim() removes it
        assertNotNull("CRLF line endings should parse", m);
        assertEquals("version code", 2, m.getVersionCode());
    }

    @Test
    public void numeric_overflow_version_code_rejected() {
        // If version_code overflows Long, NumberFormatException is caught
        String overflowVersion = VALID_MANIFEST.replace("version_code = 2", "version_code = 99999999999999999999999999");
        ReleaseManifest m = ReleaseManifest.parse(overflowVersion);
        assertNull("overflow version_code should be rejected", m);
    }

    @Test
    public void numeric_overflow_size_bytes_rejected() {
        // If size_bytes overflows Long, NumberFormatException is caught
        String overflowSize = VALID_MANIFEST.replace("size_bytes = 1048576", "size_bytes = 99999999999999999999999999");
        ReleaseManifest m = ReleaseManifest.parse(overflowSize);
        assertNull("overflow size_bytes should be rejected", m);
    }

    @Test
    public void numeric_overflow_min_sdk_rejected() {
        // If min_sdk overflows Integer, NumberFormatException is caught
        String overflowSdk = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = 99999999999");
        ReleaseManifest m = ReleaseManifest.parse(overflowSdk);
        assertNull("overflow min_sdk should be rejected", m);
    }

    @Test
    public void non_numeric_version_code_rejected() {
        String nonNumeric = VALID_MANIFEST.replace("version_code = 2", "version_code = abc");
        ReleaseManifest m = ReleaseManifest.parse(nonNumeric);
        assertNull("non-numeric version_code should be rejected", m);
    }

    @Test
    public void non_numeric_size_bytes_rejected() {
        String nonNumeric = VALID_MANIFEST.replace("size_bytes = 1048576", "size_bytes = huge");
        ReleaseManifest m = ReleaseManifest.parse(nonNumeric);
        assertNull("non-numeric size_bytes should be rejected", m);
    }

    @Test
    public void non_numeric_min_sdk_rejected() {
        String nonNumeric = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = latest");
        ReleaseManifest m = ReleaseManifest.parse(nonNumeric);
        assertNull("non-numeric min_sdk should be rejected", m);
    }
}

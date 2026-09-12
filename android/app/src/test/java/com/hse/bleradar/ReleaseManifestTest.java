package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Contract notes for {@link ReleaseManifest}: since decision #87 the manifest
 * is validated and canonicalized by the Rust update core
 * ({@code bleradar_core::update::ReleaseManifest::parse}) through
 * {@code NativeRadar.releaseManifestCanonical}, so every acceptance or
 * rejection below is the Rust contract (strict: required fields, HTTPS URL,
 * non-zero size, 64-hex hash, no unknown, duplicate or malformed lines) and
 * needs the native library to run. The executable lock for the same contract
 * is {@code crates/bleradar-jni/tests/jni_bridge.rs}, the export campaign, and
 * the JVM smoke harness of {@code cargo xtask verify-jni-live}.
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
    public void rejects_missing_mandatory() {
        // mandatory is required by the Rust contract; the old Java parser defaulted it to false
        String noMandatory = VALID_MANIFEST.replaceAll("mandatory = false\n", "");
        ReleaseManifest m = ReleaseManifest.parse(noMandatory);
        assertNull("manifest without mandatory field must be rejected", m);
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
        // a small versionCode parses (0 is accepted too; updateDecision compares codes)
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
    public void min_sdk_zero_parses() {
        // min_sdk is any non-negative integer; the OS comparison in updateDecision decides compatibility
        String zeroSdk = VALID_MANIFEST.replace("min_sdk = 26", "min_sdk = 0");
        ReleaseManifest m = ReleaseManifest.parse(zeroSdk);
        assertNotNull("minSdk=0 is accepted", m);
        assertEquals("minSdk", 0, m.getMinSdk());
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
    public void rejects_empty_version_name() {
        // version_name must not be empty
        String emptyName = VALID_MANIFEST.replace("version_name = 1.0.1", "version_name = ");
        ReleaseManifest m = ReleaseManifest.parse(emptyName);
        assertNull("empty version_name must be rejected", m);
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
    public void rejects_invalid_mandatory_value() {
        // Only "true" or "false" are accepted; the old Java parser read anything else as false
        String invalidBool = VALID_MANIFEST.replace("mandatory = false", "mandatory = maybe");
        ReleaseManifest m = ReleaseManifest.parse(invalidBool);
        assertNull("invalid mandatory value must be rejected", m);
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
    public void rejects_line_without_equals() {
        // A line that is not `key = value` rejects the whole manifest (the old Java parser skipped it)
        String malformedLine = VALID_MANIFEST + "garbage line without equals\n";
        ReleaseManifest m = ReleaseManifest.parse(malformedLine);
        assertNull("manifest with a malformed line must be rejected", m);
    }

    @Test
    public void rejects_unknown_field() {
        // An unknown key rejects the manifest (the old Java parser ignored it)
        String unknownField = VALID_MANIFEST + "author = John Doe\n";
        ReleaseManifest m = ReleaseManifest.parse(unknownField);
        assertNull("unknown field must be rejected", m);
    }

    @Test
    public void rejects_duplicate_field() {
        // A repeated key rejects the manifest rather than letting the last value win
        String duplicate = VALID_MANIFEST + "min_sdk = 26\n";
        ReleaseManifest m = ReleaseManifest.parse(duplicate);
        assertNull("duplicate field must be rejected", m);
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

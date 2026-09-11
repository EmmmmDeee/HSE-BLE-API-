package com.hse.bleradar;

import org.junit.Test;
import static org.junit.Assert.*;

/**
 * Unit tests for {@link UpdateCheckService} manifest loading.
 */
public class UpdateCheckServiceTest {

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
}

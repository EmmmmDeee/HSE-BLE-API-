package com.hse.bleradar;

import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * Parses a strictly-formatted {@code ReleaseManifest} as specified in
 * {@code docs/AUTO_UPDATE.md}.
 *
 * <p>Format: line-oriented {@code key = value} (one field per line; first
 * {@code =} splits key from value; {@code #} and blank lines ignored).
 *
 * <p>Required fields:
 * <ul>
 *   <li>{@code version_code}: monotonic {@code versionCode}.</li>
 *   <li>{@code version_name}: display name (e.g. "1.0.0").</li>
 *   <li>{@code url}: must be HTTPS.</li>
 *   <li>{@code size_bytes}: non-zero.</li>
 *   <li>{@code sha256}: exactly 64 hex characters.</li>
 *   <li>{@code min_sdk}: minimum {@code minSdkVersion}.</li>
 *   <li>{@code mandatory}: "true" or "false".</li>
 * </ul>
 *
 * <p>Optional:
 * <ul>
 *   <li>{@code notes}: release notes (prose).</li>
 * </ul>
 */
public final class ReleaseManifest {

    private static final Pattern HEX_SHA256 = Pattern.compile("[0-9a-fA-F]{64}");

    private final long versionCode;
    private final String versionName;
    private final String url;
    private final long sizeBytes;
    private final String sha256;
    private final int minSdk;
    private final boolean mandatory;
    private final String notes;

    private ReleaseManifest(long versionCode, String versionName, String url,
            long sizeBytes, String sha256, int minSdk, boolean mandatory, String notes) {
        this.versionCode = versionCode;
        this.versionName = versionName;
        this.url = url;
        this.sizeBytes = sizeBytes;
        this.sha256 = sha256.toLowerCase(Locale.US);
        this.minSdk = minSdk;
        this.mandatory = mandatory;
        this.notes = notes;
    }

    /**
     * Parses a manifest from a {@code key=value} line-oriented string.
     *
     * @return the parsed manifest, or null if parsing fails or a required field is missing/invalid
     */
    public static ReleaseManifest parse(String text) {
        if (text == null || text.isEmpty()) {
            return null;
        }
        Map<String, String> fields = new HashMap<>();
        for (String line : text.split("\n")) {
            line = line.trim();
            if (line.isEmpty() || line.startsWith("#")) {
                continue;
            }
            int eqIdx = line.indexOf('=');
            if (eqIdx < 0) {
                continue;
            }
            String key = line.substring(0, eqIdx).trim();
            String value = line.substring(eqIdx + 1).trim();
            fields.put(key, value);
        }

        try {
            long versionCode = Long.parseLong(fields.getOrDefault("version_code", "0"));
            String versionName = fields.get("version_name");
            String url = fields.get("url");
            long sizeBytes = Long.parseLong(fields.getOrDefault("size_bytes", "0"));
            String sha256 = fields.get("sha256");
            int minSdk = Integer.parseInt(fields.getOrDefault("min_sdk", "26"));
            boolean mandatory = Boolean.parseBoolean(fields.getOrDefault("mandatory", "false"));
            String notes = fields.getOrDefault("notes", "");

            if (versionCode <= 0 || versionName == null || url == null || sizeBytes <= 0 || sha256 == null || minSdk < 1) {
                return null;
            }
            if (!url.startsWith("https://")) {
                return null;
            }
            if (!HEX_SHA256.matcher(sha256).matches()) {
                return null;
            }

            return new ReleaseManifest(versionCode, versionName, url, sizeBytes, sha256, minSdk, mandatory, notes);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    public long getVersionCode() {
        return versionCode;
    }

    public String getVersionName() {
        return versionName;
    }

    public String getUrl() {
        return url;
    }

    public long getSizeBytes() {
        return sizeBytes;
    }

    public String getSha256() {
        return sha256;
    }

    public int getMinSdk() {
        return minSdk;
    }

    public boolean isMandatory() {
        return mandatory;
    }

    public String getNotes() {
        return notes;
    }

    /**
     * Serializes this manifest back to the line-oriented format (deterministic,
     * round-trips with {@link #parse}).
     */
    public String serialize() {
        StringBuilder sb = new StringBuilder();
        sb.append("version_code = ").append(versionCode).append("\n");
        sb.append("version_name = ").append(versionName).append("\n");
        sb.append("url = ").append(url).append("\n");
        sb.append("size_bytes = ").append(sizeBytes).append("\n");
        sb.append("sha256 = ").append(sha256).append("\n");
        sb.append("min_sdk = ").append(minSdk).append("\n");
        sb.append("mandatory = ").append(mandatory).append("\n");
        if (!notes.isEmpty()) {
            sb.append("notes = ").append(notes).append("\n");
        }
        return sb.toString();
    }
}

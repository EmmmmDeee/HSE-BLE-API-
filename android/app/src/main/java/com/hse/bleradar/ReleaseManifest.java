package com.hse.bleradar;

import android.util.Log;

/**
 * A release manifest as validated and canonicalized by the Rust update core
 * ({@code bleradar_core::update::ReleaseManifest}) through
 * {@link NativeRadar#releaseManifestCanonical}.
 *
 * <p>Java parses nothing: a text is accepted exactly when Rust accepts it
 * (required {@code version_code}, {@code version_name}, HTTPS {@code url},
 * non-zero {@code size_bytes}, 64-hex {@code sha256}, {@code min_sdk},
 * {@code mandatory}; optional {@code notes}; no unknown, duplicate, or
 * malformed lines — see {@code docs/AUTO_UPDATE.md}), every field is read
 * back through {@link NativeRadar#releaseManifestField} from the canonical
 * form, and {@link #serialize()} returns that canonical form, which Rust
 * round-trips. Without the native core no manifest can be validated, so
 * {@link #parse} yields {@code null} and the update check does not proceed.
 */
public final class ReleaseManifest {

    private static final String TAG = "ReleaseManifest";

    private final String canonical;
    private final long versionCode;
    private final String versionName;
    private final String url;
    private final long sizeBytes;
    private final String sha256;
    private final int minSdk;
    private final boolean mandatory;
    private final String notes;

    private ReleaseManifest(String canonical) {
        this.canonical = canonical;
        this.versionCode = Long.parseLong(field(canonical, NativeRadar.MANIFEST_FIELD_VERSION_CODE));
        this.versionName = field(canonical, NativeRadar.MANIFEST_FIELD_VERSION_NAME);
        this.url = field(canonical, NativeRadar.MANIFEST_FIELD_URL);
        this.sizeBytes = Long.parseLong(field(canonical, NativeRadar.MANIFEST_FIELD_SIZE_BYTES));
        this.sha256 = field(canonical, NativeRadar.MANIFEST_FIELD_SHA256);
        this.minSdk = Integer.parseInt(field(canonical, NativeRadar.MANIFEST_FIELD_MIN_SDK));
        this.mandatory = Boolean.parseBoolean(field(canonical, NativeRadar.MANIFEST_FIELD_MANDATORY));
        this.notes = field(canonical, NativeRadar.MANIFEST_FIELD_NOTES);
    }

    private static String field(String canonical, int field) {
        String value = NativeRadar.releaseManifestField(canonical, field);
        if (value == null) {
            // Unreachable for a text Rust has just canonicalized; fail loudly
            // rather than continue with a half-built manifest.
            throw new IllegalStateException("Rust rejected field " + field + " of a canonical manifest");
        }
        return value;
    }

    /**
     * Validates {@code text} in the Rust core.
     *
     * @return the manifest, or {@code null} when the native core is unavailable,
     *     Rust rejects the text (the reason is logged), or a numeric field does
     *     not fit the app's signed Java types
     */
    public static ReleaseManifest parse(String text) {
        if (!NativeRadar.isAvailable()) {
            Log.w(TAG, "Native core unavailable; cannot validate a release manifest");
            return null;
        }
        if (text == null) {
            Log.w(TAG, "Manifest text is null");
            return null;
        }
        String canonical = NativeRadar.releaseManifestCanonical(text);
        if (canonical == null) {
            Log.w(TAG, "Rejected release manifest: " + NativeRadar.releaseManifestError(text));
            return null;
        }
        try {
            return new ReleaseManifest(canonical);
        } catch (NumberFormatException e) {
            Log.w(TAG, "Release manifest field does not fit a Java signed integer", e);
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

    /** Lowercase hex, exactly 64 characters. */
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

    /** The canonical line-oriented form Rust emitted; {@link #parse} of it yields an equal manifest. */
    public String serialize() {
        return canonical;
    }
}

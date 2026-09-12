//! Regression tests for the JNI-facing ordinal/sentinel encodings that
//! `android/app/src/main/java/com/hse/bleradar/NativeRadar.java` depends on,
//! and for the string bridge (driven through the mock JNI function table in
//! `common/mod.rs`; `cargo xtask verify-jni-live` drives it through a real JVM).

mod common;

use std::io::Cursor;

use bleradar_core::ReleaseManifest;
use bleradar_jni::{
    ARTIFACT_HASH_MISMATCH, ARTIFACT_MANIFEST_INVALID, ARTIFACT_SIZE_MISMATCH, ARTIFACT_UNREADABLE,
    ARTIFACT_VERIFIED, Java_com_hse_bleradar_NativeRadar_artifactVerifyFile,
    Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical,
    Java_com_hse_bleradar_NativeRadar_releaseManifestError,
    Java_com_hse_bleradar_NativeRadar_releaseManifestField, MANIFEST_FIELD_MANDATORY,
    MANIFEST_FIELD_MIN_SDK, MANIFEST_FIELD_NOTES, MANIFEST_FIELD_SHA256, MANIFEST_FIELD_SIZE_BYTES,
    MANIFEST_FIELD_URL, MANIFEST_FIELD_VERSION_CODE, MANIFEST_FIELD_VERSION_NAME,
    TrackingSnapshotJniInput, artifact_verify_file, artifact_verify_reader, ble_distance_m_or_nan,
    calibration_profile_path_loss_exponent_or_nan, calibration_profile_rssi_at_1m_dbm_or_nan,
    default_calibration_profile_ordinal, default_tracking_profile_ordinal, device_rank_key,
    device_should_prune, distance_lower_bound_m_or_nan, distance_upper_bound_m_or_nan,
    download_readiness_ordinal, filtered_rssi_or_nan, proximity_label_ordinal,
    release_manifest_canonical, release_manifest_error, release_manifest_field,
    retry_backoff_delay_secs, should_check_for_update_flag, signal_confidence_percent_or_negative,
    signal_trend_ordinal, tracking_confidence_percent_or_negative,
    tracking_distance_lower_bound_m_or_nan, tracking_distance_m_or_nan,
    tracking_distance_proximity_ordinal, tracking_distance_upper_bound_m_or_nan,
    tracking_filtered_rssi_or_nan, tracking_freshness_ordinal, tracking_proximity_ordinal,
    tracking_trend_ordinal, update_decision_ordinal,
};

use common::MockEnv;

/// SHA-256 of the five bytes `hello` (the core's own documented example).
const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
const NOTES: &str = "Ünïcødé ✓ 😀 #123";

fn hello_manifest() -> String {
    format!(
        "version_code = 2\nversion_name = 0.2\nurl = https://e/x.apk\nsize_bytes = 5\n\
         sha256 = {}\nmin_sdk = 21\nmandatory = false\nnotes = {NOTES}\n",
        HELLO_SHA256.to_uppercase()
    )
}

/// A temporary file holding `bytes`, removed when dropped.
struct TempArtifact(std::path::PathBuf);

impl TempArtifact {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("bleradar-jni-{}-{name}.bin", std::process::id()));
        std::fs::write(&path, bytes).expect("write temp artifact");
        Self(path)
    }

    fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for TempArtifact {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn release_manifest_canonical_is_the_core_serialization_and_a_fixed_point() {
    let text = hello_manifest();
    let canonical = release_manifest_canonical(&text).expect("valid manifest");
    let core = ReleaseManifest::parse(&text).expect("core accepts it");
    assert_eq!(canonical, core.serialize());
    // The uppercase hash is normalized, and the canonical form re-canonicalizes to itself.
    assert!(canonical.contains(&format!("sha256 = {HELLO_SHA256}\n")));
    assert_eq!(
        release_manifest_canonical(&canonical).as_deref(),
        Some(canonical.as_str())
    );
    assert_eq!(
        ReleaseManifest::parse(&canonical).expect("round trip"),
        core
    );
    assert_eq!(release_manifest_error(&text), None);
}

#[test]
fn release_manifest_error_is_the_complement_of_canonical_and_names_the_cause() {
    let insecure = hello_manifest().replace("https://", "http://");
    assert_eq!(release_manifest_canonical(&insecure), None);
    let error = release_manifest_error(&insecure).expect("rejected");
    assert!(error.contains("https"), "{error}");
    let missing = hello_manifest().replace("min_sdk = 21\n", "");
    assert_eq!(release_manifest_canonical(&missing), None);
    assert!(
        release_manifest_error(&missing)
            .expect("rejected")
            .contains("min_sdk")
    );
    // The Java parser this replaced tolerated all of these; the core does not.
    for (label, text) in [
        ("unknown field", hello_manifest() + "author = someone\n"),
        (
            "malformed line",
            hello_manifest() + "garbage without equals\n",
        ),
        ("duplicate field", hello_manifest() + "min_sdk = 21\n"),
        (
            "empty version name",
            hello_manifest().replace("version_name = 0.2", "version_name ="),
        ),
        (
            "invalid mandatory",
            hello_manifest().replace("mandatory = false", "mandatory = maybe"),
        ),
        (
            "zero size",
            hello_manifest().replace("size_bytes = 5", "size_bytes = 0"),
        ),
        ("empty text", String::new()),
    ] {
        assert_eq!(release_manifest_canonical(&text), None, "{label}");
        assert!(release_manifest_error(&text).is_some(), "{label}");
    }
}

#[test]
fn release_manifest_field_renders_every_field_and_rejects_unknown_selectors() {
    let text = hello_manifest();
    let field = |selector| release_manifest_field(&text, selector);
    assert_eq!(field(MANIFEST_FIELD_VERSION_CODE).as_deref(), Some("2"));
    assert_eq!(field(MANIFEST_FIELD_VERSION_NAME).as_deref(), Some("0.2"));
    assert_eq!(
        field(MANIFEST_FIELD_URL).as_deref(),
        Some("https://e/x.apk")
    );
    assert_eq!(field(MANIFEST_FIELD_SIZE_BYTES).as_deref(), Some("5"));
    assert_eq!(field(MANIFEST_FIELD_SHA256).as_deref(), Some(HELLO_SHA256));
    assert_eq!(field(MANIFEST_FIELD_MIN_SDK).as_deref(), Some("21"));
    assert_eq!(field(MANIFEST_FIELD_MANDATORY).as_deref(), Some("false"));
    assert_eq!(field(MANIFEST_FIELD_NOTES).as_deref(), Some(NOTES));
    assert_eq!(field(-1), None);
    assert_eq!(field(8), None);
    assert_eq!(field(i32::MAX), None);
    assert_eq!(
        release_manifest_field("not a manifest", MANIFEST_FIELD_URL),
        None
    );
    // Every rendered field is exactly what the canonical form carries.
    let canonical = release_manifest_canonical(&text).unwrap();
    for (selector, key) in [
        (MANIFEST_FIELD_VERSION_CODE, "version_code"),
        (MANIFEST_FIELD_VERSION_NAME, "version_name"),
        (MANIFEST_FIELD_URL, "url"),
        (MANIFEST_FIELD_SIZE_BYTES, "size_bytes"),
        (MANIFEST_FIELD_SHA256, "sha256"),
        (MANIFEST_FIELD_MIN_SDK, "min_sdk"),
        (MANIFEST_FIELD_MANDATORY, "mandatory"),
        (MANIFEST_FIELD_NOTES, "notes"),
    ] {
        assert!(canonical.contains(&format!("{key} = {}\n", field(selector).unwrap())));
    }
}

#[test]
fn artifact_verify_reader_maps_every_outcome_to_its_ordinal() {
    let manifest = ReleaseManifest::parse(&hello_manifest()).unwrap();
    let verify = |bytes: &[u8]| artifact_verify_reader(Cursor::new(bytes.to_vec()), &manifest);
    assert_eq!(verify(b"hello"), ARTIFACT_VERIFIED);
    assert_eq!(verify(b"hellp"), ARTIFACT_HASH_MISMATCH);
    assert_eq!(verify(b"hell"), ARTIFACT_SIZE_MISMATCH);
    assert_eq!(verify(b"hello!"), ARTIFACT_SIZE_MISMATCH);
    assert_eq!(verify(b""), ARTIFACT_SIZE_MISMATCH);
    // A stream that fails mid-way is unreadable, not a mismatch.
    struct Failing;
    impl std::io::Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk gone"))
        }
    }
    assert_eq!(
        artifact_verify_reader(Failing, &manifest),
        ARTIFACT_UNREADABLE
    );
}

#[test]
fn artifact_verify_file_reads_the_file_and_checks_the_manifest_first() {
    let manifest = hello_manifest();
    let good = TempArtifact::new("good", b"hello");
    assert_eq!(
        artifact_verify_file(&good.path(), &manifest),
        ARTIFACT_VERIFIED
    );
    let corrupt = TempArtifact::new("corrupt", b"hellp");
    assert_eq!(
        artifact_verify_file(&corrupt.path(), &manifest),
        ARTIFACT_HASH_MISMATCH
    );
    let missing = format!("{}.missing", good.path());
    assert_eq!(
        artifact_verify_file(&missing, &manifest),
        ARTIFACT_UNREADABLE
    );
    let insecure = manifest.replace("https://", "http://");
    assert_eq!(
        artifact_verify_file(&good.path(), &insecure),
        ARTIFACT_MANIFEST_INVALID
    );
    assert_eq!(
        artifact_verify_file(&missing, &insecure),
        ARTIFACT_MANIFEST_INVALID
    );
}

#[test]
fn string_exports_round_trip_through_a_jni_function_table() {
    let mock = MockEnv::new();
    let env = mock.env();
    let text = hello_manifest();
    let jtext = mock.string(&text);
    let canonical = mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(
        env,
        core::ptr::null_mut(),
        jtext,
    ));
    assert_eq!(canonical, release_manifest_canonical(&text));
    assert!(
        Java_com_hse_bleradar_NativeRadar_releaseManifestError(env, core::ptr::null_mut(), jtext)
            .is_null()
    );
    // A supplementary-plane character survives both UTF-16 crossings intact.
    let notes = mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestField(
        env,
        core::ptr::null_mut(),
        jtext,
        MANIFEST_FIELD_NOTES,
    ));
    assert_eq!(notes.as_deref(), Some(NOTES));
    assert!(
        Java_com_hse_bleradar_NativeRadar_releaseManifestField(
            env,
            core::ptr::null_mut(),
            jtext,
            8
        )
        .is_null()
    );
    let insecure = mock.string(&text.replace("https://", "http://"));
    assert!(
        Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(
            env,
            core::ptr::null_mut(),
            insecure
        )
        .is_null()
    );
    let error = mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestError(
        env,
        core::ptr::null_mut(),
        insecure,
    ));
    assert!(error.unwrap().contains("https"));
    // An unpaired surrogate is not Unicode: the bridge treats it as invalid input.
    let lone = mock.string_units(&[0xD83Du16]);
    assert!(
        Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(
            env,
            core::ptr::null_mut(),
            lone
        )
        .is_null()
    );
    let empty = mock.string("");
    assert!(
        Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(
            env,
            core::ptr::null_mut(),
            empty
        )
        .is_null()
    );
    assert!(
        mock.read(Java_com_hse_bleradar_NativeRadar_releaseManifestError(
            env,
            core::ptr::null_mut(),
            empty
        ))
        .is_some()
    );
}

#[test]
fn string_exports_answer_null_inputs_with_sentinels() {
    let mock = MockEnv::new();
    let env = mock.env();
    let null = core::ptr::null_mut();
    let jtext = mock.string(&hello_manifest());
    assert!(
        Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(null, null, jtext).is_null()
    );
    assert!(Java_com_hse_bleradar_NativeRadar_releaseManifestCanonical(env, null, null).is_null());
    assert!(Java_com_hse_bleradar_NativeRadar_releaseManifestError(null, null, jtext).is_null());
    assert!(Java_com_hse_bleradar_NativeRadar_releaseManifestError(env, null, null).is_null());
    assert!(Java_com_hse_bleradar_NativeRadar_releaseManifestField(null, null, jtext, 0).is_null());
    assert!(Java_com_hse_bleradar_NativeRadar_releaseManifestField(env, null, null, 0).is_null());
    assert_eq!(
        Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(null, null, jtext, jtext),
        ARTIFACT_MANIFEST_INVALID
    );
    assert_eq!(
        Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(env, null, jtext, null),
        ARTIFACT_MANIFEST_INVALID
    );
    assert_eq!(
        Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(env, null, null, jtext),
        ARTIFACT_UNREADABLE
    );
}

#[test]
fn artifact_verify_file_export_streams_a_real_file_through_the_verifier() {
    let mock = MockEnv::new();
    let env = mock.env();
    let null = core::ptr::null_mut();
    let manifest = mock.string(&hello_manifest());
    let good = TempArtifact::new("export-good", b"hello");
    let path = mock.string(&good.path());
    assert_eq!(
        Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(env, null, path, manifest),
        ARTIFACT_VERIFIED
    );
    std::fs::write(&good.0, b"hellp").unwrap();
    assert_eq!(
        Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(env, null, path, manifest),
        ARTIFACT_HASH_MISMATCH
    );
    std::fs::write(&good.0, b"hello, world").unwrap();
    assert_eq!(
        Java_com_hse_bleradar_NativeRadar_artifactVerifyFile(env, null, path, manifest),
        ARTIFACT_SIZE_MISMATCH
    );
}

#[test]
fn distance_matches_reference_at_one_metre() {
    let distance = ble_distance_m_or_nan(-59.0, -59.0, 2.0);
    assert!((distance - 1.0).abs() < 1e-9);
}

#[test]
fn filtered_rssi_matches_ema_and_bootstraps_from_nan() {
    assert_eq!(filtered_rssi_or_nan(f64::NAN, -80.0, 0.35), -80.0);
    let filtered = filtered_rssi_or_nan(-80.0, -60.0, 0.5);
    assert!((filtered + 70.0).abs() < 1e-9);
}

#[test]
fn distance_is_nan_sentinel_when_core_returns_none() {
    // Non-positive path-loss exponent: `bleradar_core::ble_distance_m` returns `None`.
    assert!(ble_distance_m_or_nan(-70.0, -59.0, 0.0).is_nan());
    // Non-finite RSSI input.
    assert!(ble_distance_m_or_nan(f64::NAN, -59.0, 2.0).is_nan());
}

#[test]
fn distance_is_never_negative_or_infinite_for_finite_valid_input() {
    for rssi in [-100.0, -80.0, -59.0, -40.0, -10.0] {
        let distance = ble_distance_m_or_nan(rssi, -59.0, 2.0);
        assert!(distance.is_finite());
        assert!(distance > 0.0);
    }
}

#[test]
fn distance_bounds_expand_around_the_central_estimate() {
    let lower = distance_lower_bound_m_or_nan(-59.0, 6.0, -59.0, 2.0);
    let upper = distance_upper_bound_m_or_nan(-59.0, 6.0, -59.0, 2.0);
    assert!(lower.is_finite());
    assert!(upper.is_finite());
    assert!(lower < 1.0);
    assert!(upper > 1.0);
    assert!(lower < upper);
}

#[test]
fn proximity_ordinals_match_documented_mapping() {
    assert_eq!(proximity_label_ordinal(-40.0), 0); // Immediate
    assert_eq!(proximity_label_ordinal(-60.0), 1); // Near
    assert_eq!(proximity_label_ordinal(-75.0), 2); // Mid
    assert_eq!(proximity_label_ordinal(-95.0), 3); // Far
}

#[test]
fn signal_trend_ordinals_match_documented_mapping() {
    assert_eq!(signal_trend_ordinal(-80.0, -60.0, 3.0), 0); // Stronger
    assert_eq!(signal_trend_ordinal(-60.0, -80.0, 3.0), 1); // Weaker
    assert_eq!(signal_trend_ordinal(-70.0, -71.0, 3.0), 2); // Stable
}

#[test]
fn confidence_percent_uses_negative_one_as_invalid_sentinel() {
    assert_eq!(signal_confidence_percent_or_negative(-1, 2.0), -1);
    assert_eq!(signal_confidence_percent_or_negative(8, f64::NAN), -1);
    assert!(signal_confidence_percent_or_negative(8, 1.0) > 0);
}

#[test]
fn calibration_profiles_expose_rust_owned_defaults() {
    assert_eq!(default_calibration_profile_ordinal(), 0);
    assert_eq!(default_tracking_profile_ordinal(), 0);
    assert_eq!(calibration_profile_rssi_at_1m_dbm_or_nan(0), -59.0);
    assert_eq!(calibration_profile_path_loss_exponent_or_nan(0), 2.0);
    assert!(calibration_profile_path_loss_exponent_or_nan(1) > 2.0);
    assert!(calibration_profile_path_loss_exponent_or_nan(2) < 2.0);
    assert!(calibration_profile_rssi_at_1m_dbm_or_nan(99).is_nan());
}

#[test]
fn tracking_snapshot_exports_a_coherent_bundle() {
    let input = TrackingSnapshotJniInput {
        previous_filtered_dbm: -80.0,
        current_rssi_dbm: -60.0,
        rssi_spread_db: 4.0,
        sample_count: 6,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 1,
        age_ms: 250,
        tx_power_dbm: f64::NAN,
    };
    let filtered = tracking_filtered_rssi_or_nan(input);
    let distance = tracking_distance_m_or_nan(input);
    let lower = tracking_distance_lower_bound_m_or_nan(input);
    let upper = tracking_distance_upper_bound_m_or_nan(input);
    assert!((filtered - (-69.0)).abs() < 1e-9);
    assert!(distance.is_finite());
    assert!(lower < distance);
    assert!(upper > distance);
    assert_eq!(tracking_trend_ordinal(input), 0);
    assert_eq!(tracking_proximity_ordinal(input), 2);
    assert_eq!(tracking_distance_proximity_ordinal(input), 2);
    assert!(tracking_confidence_percent_or_negative(input) > 0);
    assert_eq!(tracking_freshness_ordinal(input), 0);
}

#[test]
fn tracking_distance_prefers_plausible_device_tx_power_over_profile_calibration() {
    let base = TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -70.0,
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 0,
        tx_power_dbm: f64::NAN,
    };
    let without_tx_power = tracking_distance_m_or_nan(base);
    let with_tx_power = tracking_distance_m_or_nan(TrackingSnapshotJniInput {
        tx_power_dbm: -70.0,
        ..base
    });
    assert!((with_tx_power - 1.0).abs() < 1e-9);
    assert!(with_tx_power < without_tx_power);
    // Android's `ScanResult.TX_POWER_NOT_PRESENT` sentinel must fall back to
    // the profile calibration, not corrupt the estimate.
    let not_present = tracking_distance_m_or_nan(TrackingSnapshotJniInput {
        tx_power_dbm: 127.0,
        ..base
    });
    assert_eq!(not_present, without_tx_power);
}

#[test]
fn distance_proximity_ordinal_uses_calibrated_boundaries() {
    let input = |distance_m: f64| TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -59.0 - 20.0 * distance_m.log10(),
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 0,
        tx_power_dbm: f64::NAN,
    };
    assert_eq!(tracking_distance_proximity_ordinal(input(0.999)), 0);
    assert_eq!(tracking_distance_proximity_ordinal(input(1.001)), 1);
    assert_eq!(tracking_distance_proximity_ordinal(input(1.999)), 1);
    assert_eq!(tracking_distance_proximity_ordinal(input(2.001)), 2);
    assert_eq!(tracking_distance_proximity_ordinal(input(4.999)), 2);
    assert_eq!(tracking_distance_proximity_ordinal(input(5.001)), 3);
    assert_eq!(tracking_distance_proximity_ordinal(input(24.999)), 3);
}

#[test]
fn tracking_snapshot_uses_invalid_sentinels() {
    let invalid_profile = TrackingSnapshotJniInput {
        previous_filtered_dbm: f64::NAN,
        current_rssi_dbm: -70.0,
        rssi_spread_db: 0.0,
        sample_count: 1,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 99,
        age_ms: 0,
        tx_power_dbm: f64::NAN,
    };
    let invalid_count = TrackingSnapshotJniInput {
        sample_count: -1,
        tracking_profile_ordinal: 0,
        ..invalid_profile
    };
    assert!(tracking_filtered_rssi_or_nan(invalid_profile).is_nan());
    assert!(tracking_distance_m_or_nan(invalid_count).is_nan());
    assert_eq!(tracking_distance_proximity_ordinal(invalid_count), 3);
    assert_eq!(tracking_confidence_percent_or_negative(invalid_count), -1);
    assert_eq!(tracking_freshness_ordinal(invalid_count), 2);
}

#[test]
fn tracking_snapshot_survives_invalid_spread() {
    // Live adverse-condition proof: invalid spread must not erase filtered RSSI.
    let input = TrackingSnapshotJniInput {
        previous_filtered_dbm: -70.0,
        current_rssi_dbm: -68.0,
        rssi_spread_db: f64::NAN,
        sample_count: 5,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 1_000,
        tx_power_dbm: f64::NAN,
    };
    let filtered = tracking_filtered_rssi_or_nan(input);
    assert!(
        filtered.is_finite(),
        "filtered RSSI must survive invalid spread"
    );
    assert!(tracking_distance_m_or_nan(input).is_finite());
    assert!(tracking_distance_lower_bound_m_or_nan(input).is_nan());
    assert!(tracking_distance_upper_bound_m_or_nan(input).is_nan());
    assert_eq!(tracking_confidence_percent_or_negative(input), -1);
    assert_eq!(tracking_freshness_ordinal(input), 0);
}

#[test]
fn proximity_and_trend_ordinals_use_conservative_invalid_sentinels() {
    assert_eq!(proximity_label_ordinal(f64::NAN), 3);
    assert_eq!(proximity_label_ordinal(f64::INFINITY), 3);
    assert_eq!(signal_trend_ordinal(f64::NAN, -70.0, 2.0), 2);
    assert_eq!(signal_trend_ordinal(-80.0, f64::INFINITY, 2.0), 2);
}

// ===== Automatic-update decision bridges =====
//
// These lock the JNI encodings that `NativeRadar.java`'s `UPDATE_*`,
// `NETWORK_*`, and `DOWNLOAD_*` constants depend on, and cross-check each
// bridge against the authoritative `bleradar_core::update` core so the two can
// never silently diverge.

#[test]
fn update_decision_ordinal_matches_the_four_documented_outcomes() {
    // Strictly newer and OS-compatible -> Available (1).
    assert_eq!(update_decision_ordinal(41, 42, 34, 26), 1);
    // Same versionCode -> UpToDate (0).
    assert_eq!(update_decision_ordinal(42, 42, 34, 26), 0);
    // Older build -> DowngradeRefused (2).
    assert_eq!(update_decision_ordinal(42, 41, 34, 26), 2);
    // Newer but device SDK below the build minimum -> IncompatibleOs (3).
    assert_eq!(update_decision_ordinal(41, 42, 24, 26), 3);
    // A downgrade is refused before the OS check even runs.
    assert_eq!(update_decision_ordinal(42, 41, 10, 26), 2);
}

#[test]
fn update_decision_ordinal_clamps_negative_inputs_to_zero() {
    // Negative "installed" clamps to 0, so a non-negative available > 0 is newer.
    assert_eq!(update_decision_ordinal(-5, 1, 34, 0), 1);
    // Negative device SDK clamps to 0; a positive minimum then blocks it.
    assert_eq!(update_decision_ordinal(1, 2, -1, 1), 3);
    // All-zero (the live harness's default-argument invocation) is UpToDate.
    assert_eq!(update_decision_ordinal(0, 0, 0, 0), 0);
}

#[test]
fn update_decision_ordinal_agrees_with_the_core_across_a_grid() {
    for installed in 0..8u64 {
        for available in 0..8u64 {
            for device_sdk in [0u32, 24, 26, 34] {
                for min_sdk in [0u32, 24, 26, 34] {
                    let expected =
                        bleradar_core::update_decision(installed, available, device_sdk, min_sdk)
                            .ordinal();
                    let actual = update_decision_ordinal(
                        installed as i64,
                        available as i64,
                        device_sdk as i32,
                        min_sdk as i32,
                    );
                    assert_eq!(actual, expected, "grid mismatch at {installed}/{available}");
                }
            }
        }
    }
}

#[test]
fn should_check_for_update_flag_respects_interval_and_backwards_clock() {
    assert!(should_check_for_update_flag(1_000, 0, 900)); // 1000s elapsed >= 900s
    assert!(!should_check_for_update_flag(1_000, 500, 900)); // only 500s elapsed
    assert!(!should_check_for_update_flag(400, 1_000, 900)); // clock went backwards
    assert!(should_check_for_update_flag(900, 0, 900)); // exactly the interval
    // Negative inputs clamp to 0; (0, 0, 0) is the default-argument invocation.
    assert!(should_check_for_update_flag(0, 0, 0));
    assert!(should_check_for_update_flag(-1, -1, 0));
}

#[test]
fn download_readiness_ordinal_follows_precedence_and_mapping() {
    let big = 100_000_000i64;
    // Ready: Wi-Fi (unmetered = 2), enough battery, ample storage.
    assert_eq!(
        download_readiness_ordinal(2, 80, false, big, false, 20, 0, 40_000_000),
        0
    );
    // No network (0) takes precedence over everything else.
    assert_eq!(
        download_readiness_ordinal(0, 80, true, big, true, 20, 0, 40_000_000),
        1
    );
    // Metered (1) is blocked when the policy forbids it.
    assert_eq!(
        download_readiness_ordinal(1, 80, false, big, false, 20, 0, 40_000_000),
        2
    );
    // Metered allowed proceeds (subject to the later gates).
    assert_eq!(
        download_readiness_ordinal(1, 80, false, big, true, 20, 0, 40_000_000),
        0
    );
    // Low battery when below the minimum and not charging.
    assert_eq!(
        download_readiness_ordinal(2, 10, false, big, false, 20, 0, 40_000_000),
        3
    );
    // Charging overrides low battery.
    assert_eq!(
        download_readiness_ordinal(2, 10, true, big, false, 20, 0, 40_000_000),
        0
    );
    // Insufficient storage: the artifact plus the required headroom exceeds free.
    assert_eq!(
        download_readiness_ordinal(2, 80, false, 30_000_000, false, 20, 20_000_000, 40_000_000),
        4
    );
    // An unknown network ordinal is treated as "no network" (conservative).
    assert_eq!(
        download_readiness_ordinal(99, 80, false, big, true, 20, 0, 40_000_000),
        1
    );
    // All-zero default-argument invocation: network 0 -> NoNetwork (1), never a panic.
    assert_eq!(
        download_readiness_ordinal(0, 0, false, 0, false, 0, 0, 0),
        1
    );
}

#[test]
fn retry_backoff_delay_secs_is_exponential_and_capped() {
    // base = 10, cap = 60: 10, 20, 40, 60 (clamped), 60 (clamped, no overflow).
    assert_eq!(retry_backoff_delay_secs(1, 10, 60), 10);
    assert_eq!(retry_backoff_delay_secs(2, 10, 60), 20);
    assert_eq!(retry_backoff_delay_secs(3, 10, 60), 40);
    assert_eq!(retry_backoff_delay_secs(4, 10, 60), 60);
    assert_eq!(retry_backoff_delay_secs(9, 10, 60), 60);
    // A huge attempt cannot overflow; it saturates to the cap.
    assert_eq!(retry_backoff_delay_secs(1_000_000, 30, 3600), 3600);
    // Negative inputs clamp to 0; (0, 0, 0) is the default-argument invocation.
    assert_eq!(retry_backoff_delay_secs(0, 0, 0), 0);
    assert_eq!(retry_backoff_delay_secs(-5, -5, -5), 0);
}

#[test]
fn retry_backoff_delay_secs_agrees_with_the_core_policy() {
    let policy = bleradar_core::RetryPolicy {
        max_attempts: 1,
        base_delay_secs: 30,
        max_delay_secs: 3600,
    };
    for attempt in 1..=12u32 {
        let expected = policy.backoff_delay_secs(attempt) as i64;
        let actual = retry_backoff_delay_secs(attempt as i32, 30, 3600);
        assert_eq!(actual, expected, "attempt {attempt}");
    }
}

#[test]
fn device_should_prune_only_the_stale_ordinal() {
    assert!(!device_should_prune(0)); // Live
    assert!(!device_should_prune(1)); // Recent
    assert!(device_should_prune(2)); // Stale
    // Unknown ordinals keep the device: an encoding drift can never empty the map.
    assert!(!device_should_prune(-1));
    assert!(!device_should_prune(3));
    assert!(!device_should_prune(i32::MIN));
    assert!(!device_should_prune(i32::MAX));
}

#[test]
fn device_should_prune_matches_the_tracking_freshness_encoding() {
    // The only ordinal that prunes is the one `tracking_freshness_ordinal`
    // emits for `FreshnessClass::Stale`, so the two exports cannot drift apart.
    let stale = TrackingSnapshotJniInput {
        previous_filtered_dbm: -70.0,
        current_rssi_dbm: -70.0,
        rssi_spread_db: 1.0,
        sample_count: 4,
        calibration_profile_ordinal: 0,
        tracking_profile_ordinal: 0,
        age_ms: 3_600_000,
        tx_power_dbm: f64::NAN,
    };
    let live = TrackingSnapshotJniInput { age_ms: 0, ..stale };
    assert!(device_should_prune(tracking_freshness_ordinal(stale)));
    assert!(!device_should_prune(tracking_freshness_ordinal(live)));
}

#[test]
fn device_rank_key_orders_live_recent_confident_strong_first() {
    let live = device_rank_key(0, 1_000, 50, -70.0);
    let recent = device_rank_key(1, 1_000, 50, -70.0);
    let stale = device_rank_key(2, 1_000, 50, -70.0);
    assert!(live < recent && recent < stale);
    // Freshness dominates every other field.
    assert!(device_rank_key(0, 0, 0, -127.0) < device_rank_key(1, i64::MAX, 100, 20.0));
    // Within a class the most recently seen device ranks first, and recency
    // outranks both confidence and signal strength.
    assert!(device_rank_key(0, 2_000, 50, -70.0) < live);
    assert!(device_rank_key(0, 1_001, 0, -127.0) < device_rank_key(0, 1_000, 100, 20.0));
    // At equal recency higher confidence ranks first and outranks strength.
    assert!(device_rank_key(0, 1_000, 51, -127.0) < device_rank_key(0, 1_000, 50, 20.0));
    // At equal recency and confidence the stronger signal ranks first.
    assert!(device_rank_key(0, 1_000, 50, -60.0) < device_rank_key(0, 1_000, 50, -61.0));
    assert!(device_rank_key(0, 1_000, 50, -60.0) < live);
    // Sub-dBm differences are below the radio's resolution and tie.
    assert_eq!(
        device_rank_key(0, 1_000, 50, -60.2),
        device_rank_key(0, 1_000, 50, -60.7)
    );
}

#[test]
fn device_rank_key_is_non_negative_and_clamps_every_field() {
    for key in [
        device_rank_key(i32::MIN, i64::MIN, i32::MIN, f64::NAN),
        device_rank_key(i32::MAX, i64::MAX, i32::MAX, f64::INFINITY),
        device_rank_key(0, 0, 0, 0.0),
        device_rank_key(2, (1 << 40) - 1, 100, 20.0),
        device_rank_key(2, (1 << 40) - 1, 0, -127.0),
        device_rank_key(0, 0, 0, f64::NEG_INFINITY),
    ] {
        assert!(key >= 0, "{key}");
    }
    // Out-of-range fields collapse onto the boundary value, never past it.
    assert_eq!(
        device_rank_key(9, 1, 1, -70.0),
        device_rank_key(2, 1, 1, -70.0)
    );
    assert_eq!(
        device_rank_key(-3, 1, 1, -70.0),
        device_rank_key(0, 1, 1, -70.0)
    );
    assert_eq!(
        device_rank_key(0, i64::MAX, 1, -70.0),
        device_rank_key(0, (1 << 40) - 1, 1, -70.0)
    );
    assert_eq!(
        device_rank_key(0, -5, 1, -70.0),
        device_rank_key(0, 0, 1, -70.0)
    );
    assert_eq!(
        device_rank_key(0, 1, 250, -70.0),
        device_rank_key(0, 1, 100, -70.0)
    );
    assert_eq!(
        device_rank_key(0, 1, -7, -70.0),
        device_rank_key(0, 1, 0, -70.0)
    );
    assert_eq!(
        device_rank_key(0, 1, 1, 55.0),
        device_rank_key(0, 1, 1, 20.0)
    );
    assert_eq!(
        device_rank_key(0, 1, 1, -200.0),
        device_rank_key(0, 1, 1, -127.0)
    );
    // A non-finite RSSI ranks as the weakest possible signal.
    for rssi in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY] {
        assert_eq!(
            device_rank_key(0, 1, 1, rssi),
            device_rank_key(0, 1, 1, -127.0)
        );
    }
    // Adjacent in-range values in every field produce distinct keys.
    assert_ne!(
        device_rank_key(0, 1, 1, -70.0),
        device_rank_key(0, 1, 1, -71.0)
    );
    assert_ne!(
        device_rank_key(0, 1, 1, -70.0),
        device_rank_key(0, 1, 2, -70.0)
    );
    assert_ne!(
        device_rank_key(0, 1, 1, -70.0),
        device_rank_key(0, 2, 1, -70.0)
    );
    assert_ne!(
        device_rank_key(0, 1, 1, -70.0),
        device_rank_key(1, 1, 1, -70.0)
    );
}

/// The ranking `BleScanEngine.snapshot()` implemented as a four-level Java
/// comparator before it moved into Rust: freshness ascending, then last-seen
/// descending, then confidence descending, then RSSI descending — restated
/// over the key's documented clamped, whole-dBm domain.
fn reference_order(left: (i32, i64, i32, f64), right: (i32, i64, i32, f64)) -> std::cmp::Ordering {
    use std::cmp::Reverse;
    fn bucket(field: (i32, i64, i32, f64)) -> (i32, Reverse<i64>, Reverse<i32>, Reverse<i64>) {
        let rssi = if field.3.is_finite() {
            (field.3 as i64).clamp(-127, 20)
        } else {
            -127
        };
        (
            field.0.clamp(0, 2),
            Reverse(field.1.clamp(0, (1 << 40) - 1)),
            Reverse(field.2.clamp(0, 100)),
            Reverse(rssi),
        )
    }
    bucket(left).cmp(&bucket(right))
}

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn sample_device(state: &mut u64) -> (i32, i64, i32, f64) {
    let freshness = match xorshift(state) % 8 {
        0 => -1,
        1 => 3,
        n => (n % 3) as i32,
    };
    let last_seen = match xorshift(state) % 8 {
        0 => -1,
        1 => i64::MAX,
        2 => (1 << 40) - 1,
        3 => 1 << 40,
        _ => (xorshift(state) % 100_000) as i64,
    };
    let confidence = match xorshift(state) % 8 {
        0 => -1,
        1 => 101,
        _ => (xorshift(state) % 101) as i32,
    };
    let rssi = match xorshift(state) % 10 {
        0 => f64::NAN,
        1 => 30.0,
        2 => -150.0,
        3 => f64::INFINITY,
        _ => -127.0 + (xorshift(state) % 1_470) as f64 / 10.0,
    };
    (freshness, last_seen, confidence, rssi)
}

#[test]
fn device_rank_key_agrees_with_the_reference_comparator_on_random_pairs() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..100_000 {
        let left = sample_device(&mut state);
        let mut right = sample_device(&mut state);
        // Share fields often so every tie-break tier is exercised.
        if xorshift(&mut state).is_multiple_of(2) {
            right.0 = left.0;
        }
        if xorshift(&mut state).is_multiple_of(2) {
            right.1 = left.1;
        }
        if xorshift(&mut state).is_multiple_of(2) {
            right.2 = left.2;
        }
        let left_key = device_rank_key(left.0, left.1, left.2, left.3);
        let right_key = device_rank_key(right.0, right.1, right.2, right.3);
        assert_eq!(
            left_key.cmp(&right_key),
            reference_order(left, right),
            "{left:?} (key {left_key}) vs {right:?} (key {right_key})"
        );
    }
}

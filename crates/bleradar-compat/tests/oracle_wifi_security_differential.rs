//! Executed-oracle differential for the previously-unmapped `wifi_is_enterprise`
//! and `wifi_security` contracts (`docs/ORACLE_DIFFERENTIAL.md`,
//! `docs/AUTONOMOUS_DECISIONS.md` decision 77).
//!
//! Both are in the runtime census but had no reconstruction analogue. They are
//! pure, deterministic capability-string classifiers (case-sensitive substring
//! tests, no floating point), so unlike the geodesy / `wifi_distance`
//! transcendental differentials they are **bit-exact**: `cargo xtask
//! oracle-differential` executes the immutable oracle under `qemu-aarch64` and
//! records `oracle/wifi_security_executed_vectors.tsv`, and this test replays it
//! asserting the reconstruction reproduces every executed-oracle output exactly
//! over the full `String` domain (there is no domain divergence -- the
//! reconstruction accepts any `&str`). Both are therefore
//! `DifferentiallyVerified`.

use bleradar_compat::{ParityStatus, parity_status};
use bleradar_core::{WifiSecurity, wifi_is_enterprise, wifi_security};

const VECTORS: &str = include_str!("oracle/wifi_security_executed_vectors.tsv");
const ORACLE_SO_SHA256: &str = "d14022cd113332312fb1719aafa107155a4c046c056cb9b2bcd3c94eb980b12d";

#[derive(Default)]
struct Coverage {
    rows: usize,
    none_rows: usize,
    enterprise_true: usize,
    enterprise_false: usize,
    open: usize,
    wep: usize,
    wpa: usize,
    wpa2: usize,
    wpa3: usize,
    owe: usize,
    unknown: usize,
}

#[test]
fn reconstruction_reproduces_the_executed_oracle_over_the_full_domain() {
    let mut cov = Coverage::default();

    for (index, raw) in VECTORS.lines().enumerate() {
        let line = raw.trim_end_matches(['\n', '\r']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = index + 1;
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 5, "line {lineno}: WS expects 5 columns, got {f:?}");
        assert_eq!(f[0], "WS", "line {lineno}: unexpected tag {:?}", f[0]);

        let present = match f[1] {
            "1" => true,
            "0" => false,
            other => panic!("line {lineno}: bad presence flag {other:?}"),
        };
        let caps: Option<&str> = if present { Some(f[2]) } else { None };
        let oracle_ent = match f[3] {
            "1" => true,
            "0" => false,
            other => panic!("line {lineno}: oracle enterprise value {other:?} (status fault?)"),
        };
        let oracle_sec = f[4];

        // wifi_is_enterprise: exact bool match.
        assert_eq!(
            wifi_is_enterprise(caps),
            oracle_ent,
            "line {lineno}: wifi_is_enterprise({caps:?}) != executed oracle {oracle_ent}"
        );

        // wifi_security: exact label match.
        let recon_sec = wifi_security(caps).label();
        assert_eq!(
            recon_sec, oracle_sec,
            "line {lineno}: wifi_security({caps:?}) = {recon_sec} != executed oracle {oracle_sec}"
        );

        cov.rows += 1;
        if !present {
            cov.none_rows += 1;
        }
        if oracle_ent {
            cov.enterprise_true += 1;
        } else {
            cov.enterprise_false += 1;
        }
        match wifi_security(caps) {
            WifiSecurity::Open => cov.open += 1,
            WifiSecurity::Wep => cov.wep += 1,
            WifiSecurity::Wpa => cov.wpa += 1,
            WifiSecurity::Wpa2 => cov.wpa2 += 1,
            WifiSecurity::Wpa3 => cov.wpa3 += 1,
            WifiSecurity::Owe => cov.owe += 1,
            WifiSecurity::Unknown => cov.unknown += 1,
        }
    }

    println!(
        "wifi_security/enterprise: rows={} none={} ent(t/f)={}/{} open={} wep={} wpa={} wpa2={} wpa3={} owe={} unknown={}",
        cov.rows,
        cov.none_rows,
        cov.enterprise_true,
        cov.enterprise_false,
        cov.open,
        cov.wep,
        cov.wpa,
        cov.wpa2,
        cov.wpa3,
        cov.owe,
        cov.unknown
    );

    // Every security category and both enterprise outcomes must be exercised so
    // a regression in any precedence branch is caught.
    assert!(
        cov.rows >= 40,
        "too few wifi_security vectors: {}",
        cov.rows
    );
    assert!(cov.enterprise_true > 0, "no enterprise=true rows");
    assert!(cov.enterprise_false > 0, "no enterprise=false rows");
    assert!(cov.open > 0, "Open category not exercised");
    assert!(cov.wep > 0, "WEP category not exercised");
    assert!(cov.wpa > 0, "WPA category not exercised");
    assert!(cov.wpa2 > 0, "WPA2 category not exercised");
    assert!(cov.wpa3 > 0, "WPA3 category not exercised");
    assert!(cov.owe > 0, "OWE category not exercised");
    assert!(cov.unknown > 0, "Unknown (None) category not exercised");
}

/// Locks the recovered precedence rules directly on the reconstruction, so a
/// reordering that still happened to match this vector set (e.g. swapping the
/// WPA2 and WPA3 tests) fails here with a readable message.
#[test]
fn reconstruction_locks_the_recovered_precedence() {
    // SAE / WPA3 beat WPA2, which beats OWE, which beats WPA, which beats WEP.
    assert_eq!(
        wifi_security(Some("[WPA2-PSK-CCMP][RSN-SAE-CCMP][ESS]")),
        WifiSecurity::Wpa3
    );
    assert_eq!(wifi_security(Some("[WPA3-XYZ][ESS]")), WifiSecurity::Wpa3);
    assert_eq!(
        wifi_security(Some("[RSN-OWE-CCMP][ESS]")),
        WifiSecurity::Wpa2
    );
    assert_eq!(
        wifi_security(Some("[WPA-PSK][WPA2-PSK][ESS]")),
        WifiSecurity::Wpa2
    );
    assert_eq!(wifi_security(Some("[OWE-CCMP][ESS]")), WifiSecurity::Owe);
    assert_eq!(
        wifi_security(Some("[WEP][WPA-PSK][ESS]")),
        WifiSecurity::Wpa
    );
    assert_eq!(wifi_security(Some("[WEP][ESS]")), WifiSecurity::Wep);
    assert_eq!(wifi_security(Some("[ESS]")), WifiSecurity::Open);
    assert_eq!(wifi_security(None), WifiSecurity::Unknown);
    // Case-sensitivity: lowercase tokens do not match.
    assert_eq!(wifi_security(Some("sae")), WifiSecurity::Open);
    assert!(!wifi_is_enterprise(Some("wpa2-eap")));
    assert!(wifi_is_enterprise(Some("MYEAPX")));
}

#[test]
fn wifi_capability_contracts_are_differentially_verified() {
    for name in ["wifi_is_enterprise", "wifi_security"] {
        assert_eq!(
            parity_status(name),
            Some(ParityStatus::DifferentiallyVerified),
            "{name} is a bit-exact executed-oracle differential"
        );
    }
}

#[test]
fn executed_wifi_security_vectors_are_pinned_to_the_immutable_oracle() {
    assert!(
        VECTORS.contains(ORACLE_SO_SHA256),
        "the committed vectors must record the immutable oracle .so SHA-256"
    );
    assert!(
        VECTORS.contains("qemu-aarch64"),
        "the committed vectors must record the executed-oracle provenance"
    );
}

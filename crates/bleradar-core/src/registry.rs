//! The machine-readable capability registry and the supersession matrix it
//! renders — the single, honest source of truth for what this project supersedes
//! and what it does not.
//!
//! Each `Capability` names the objective, the strongest reference tool for it,
//! the Rust module and live entry point that own it, the tests that verify it,
//! its `Status`, and the residual gap. The registry is data, so it is queried
//! and tested rather than narrated: `invariants` enforces the project's
//! evidence rule — a capability may be marked `Status::Superior` or
//! `Status::Parity` only if it names test evidence; **no evidence means
//! `Status::Unverified`**, never a superiority claim — and `render_matrix`
//! emits `docs/CAPABILITY_MATRIX.md`, which a snapshot test keeps in sync.
//!
//! The statuses are deliberately conservative. Without a head-to-head benchmark
//! against a competitor build and without physical-device validation, a
//! capability that works, is live and is tested is recorded as
//! `Status::Partial` with its limit named, not `Status::Superior` — so the
//! matrix honestly shows the app is not at final competitive closure.

use core::fmt::Write as _;

/// A capability's verification state, from the project directive's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Demonstrated material advantage over the strongest reference, with evidence.
    Superior,
    /// Matches the strongest reference on the objective, with evidence.
    Parity,
    /// Works and is verified, but with a named limit short of full parity.
    Partial,
    /// Not implemented.
    Absent,
    /// Not achievable through ordinary Android APIs (a platform constraint).
    Blocked,
    /// May exist but has not been demonstrated.
    Unverified,
    /// Does not apply to this project's scope.
    NotApplicable,
}

impl Status {
    /// A stable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Superior => "SUPERIOR",
            Self::Parity => "PARITY",
            Self::Partial => "PARTIAL",
            Self::Absent => "ABSENT",
            Self::Blocked => "BLOCKED_BY_PLATFORM",
            Self::Unverified => "UNVERIFIED",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }

    /// Whether this status asserts a competitive claim that the evidence rule
    /// requires test evidence to back.
    const fn is_a_claim(self) -> bool {
        matches!(self, Self::Superior | Self::Parity)
    }
}

/// One material capability and its supersession evidence.
#[derive(Debug, Clone, Copy)]
pub struct Capability {
    /// Stable identifier.
    pub id: &'static str,
    /// The objective the capability solves.
    pub objective: &'static str,
    /// The strongest reference tool for this objective.
    pub strongest_reference: &'static str,
    /// The Rust module that owns the substantive logic (or `""` when absent).
    pub rust_module: &'static str,
    /// The live runtime entry point (or `""` when there is no live path yet).
    pub runtime_entrypoint: &'static str,
    /// The tests and proofs that verify it (empty only for a non-claim status).
    pub tests: &'static str,
    /// The verification state.
    pub status: Status,
    /// The residual gap that keeps it short of superior, or `""`.
    pub residual_gap: &'static str,
}

/// The registry. Kept sorted by `id`; `invariants` asserts the ordering and
/// the evidence rule.
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: "address-trackability",
        objective: "Distinguish a rotating/randomized BLE address from trackable hardware (identifier is not a device)",
        strongest_reference: "BlueHydra",
        rust_module: "bleradar_core::sweep::address_trackability",
        runtime_entrypoint: "NativeRadar.deviceAddressTrackability",
        tests: "sweep unit tests; verify-api-live; verify-android-emulator (trackability on the runtime)",
        status: Status::Partial,
        residual_gap: "Classification uses the 802 U/L bit, an approximation of BLE address types",
    },
    Capability {
        id: "adv-decoding",
        objective: "Decode the BLE advertising payload (AD structures, service UUIDs, manufacturer/service data, iBeacon/Eddystone)",
        strongest_reference: "nRF Connect / Beacon Scanner",
        rust_module: "bleradar_core::adv",
        runtime_entrypoint: "NativeRadar.advertisementCompanyId/advertisementBeacon",
        tests: "adv 20 unit tests + adv_campaign (200k-input differential); verify-api-live; verify-android-emulator (company_id/beacon on the runtime)",
        status: Status::Partial,
        residual_gap: "No GATT-level detail, not every beacon format, no competitor head-to-head benchmark",
    },
    Capability {
        id: "auto-update",
        objective: "Decide and safely apply an app self-update (version, integrity, lifecycle, recovery)",
        strongest_reference: "Play Store in-app updates",
        rust_module: "bleradar_core::update",
        runtime_entrypoint: "NativeRadar.updateDecision/downloadReadiness/artifactVerifyFile",
        tests: "update.rs + update_campaign (200k-op); verify-android-emulator (self-upgrade against a stand-in release host)",
        status: Status::Parity,
        residual_gap: "The network fetch and OS installer are the documented platform boundary",
    },
    Capability {
        id: "gatt-inspection",
        objective: "Connect to a selected device and browse its GATT services, characteristics and descriptors",
        strongest_reference: "nRF Connect",
        rust_module: "",
        runtime_entrypoint: "",
        tests: "",
        status: Status::Absent,
        residual_gap: "Not implemented; must stay user-initiated and separate from passive scanning",
    },
    Capability {
        id: "history-persistence",
        objective: "A per-device timeline (first/last seen, recurrence) persisted across sessions",
        strongest_reference: "BlueHydra / WiGLE",
        rust_module: "bleradar_core::history",
        runtime_entrypoint: "NativeRadar.historyMerge / NativeRadar.historyLookup",
        tests: "history 10 unit tests + history_campaign (100k-step differential with persist/reload and corruption); DeviceHistoryTest (restart on the host JVM); verify-api-live; verify-android-emulator (first_seen survives kill -9)",
        status: Status::Partial,
        residual_gap: "Public addresses only (rotating addresses are not remembered); first seen + visit count, not a full sighting timeline or location trail; no history view or export; no competitor benchmark",
    },
    Capability {
        id: "identity-correlation",
        objective: "Correlate one physical device across BLE address rotation with explicit uncertainty",
        strongest_reference: "BlueHydra",
        rust_module: "bleradar_core::identity",
        runtime_entrypoint: "NativeRadar.deviceGroupKey",
        tests: "identity 11 unit tests + identity_campaign (300k-pair property); verify-api-live; verify-android-emulator (identity_key on the runtime)",
        status: Status::Partial,
        residual_gap: "Cross-session history covers public addresses only; U/L-bit address-type approximation; app does not yet visually merge rows",
    },
    Capability {
        id: "manufacturer-name",
        objective: "Name the Bluetooth SIG assignee behind a company identifier",
        strongest_reference: "nRF Connect",
        rust_module: "bleradar_core::adv::company_name",
        runtime_entrypoint: "NativeRadar.advertisementManufacturerName",
        tests: "adv unit tests (table sorted, unknown->None); verify-api-live; verify-android-emulator",
        status: Status::Partial,
        residual_gap: "A curated ~40-entry subset, not the full SIG registry (a data-only follow-up)",
    },
    Capability {
        id: "rssi-signal",
        objective: "Filter RSSI and report proximity/distance with uncertainty (never exact distance without calibration)",
        strongest_reference: "BLE Radar-class trackers",
        rust_module: "bleradar_core::signal",
        runtime_entrypoint: "NativeRadar.trackingFilteredRssi/trackingProximity",
        tests: "signal unit tests; oracle-differential vs the original APK under qemu-aarch64; verify-android-emulator (a finite distance on the runtime)",
        status: Status::Parity,
        residual_gap: "No empirical comparison of alternative filters (median/Hampel/Kalman) under real load",
    },
    Capability {
        id: "sensor-rules",
        objective: "One authority for the multi-sensor radar's reading-interpretation rules (Wi-Fi/BT/cell)",
        strongest_reference: "Huntsman Search Engine signal_radar",
        rust_module: "bleradar_core::sweep",
        runtime_entrypoint: "NativeRadar.deviceAddressTrackability (BLE path only)",
        tests: "sweep 10 unit tests",
        status: Status::Partial,
        residual_gap: "The app scans BLE only, so the Wi-Fi and cell rules have no in-app caller yet",
    },
    Capability {
        id: "wifi-survey",
        objective: "A wireless observation map/history (SSID/BSSID/channel/security/location) — WiGLE-class collection",
        strongest_reference: "WiGLE",
        rust_module: "",
        runtime_entrypoint: "",
        tests: "",
        status: Status::Blocked,
        residual_gap: "The Android app captures BLE only; Wi-Fi scanning would need a separate platform surface",
    },
];

/// Checks every registry invariant, returning the first violation. Run by a
/// unit test so a drifting or over-claiming registry fails the build.
///
/// The load-bearing rule (the directive's): a capability may claim
/// `Status::Superior` or `Status::Parity` only if it names test evidence.
pub fn invariants() -> Result<(), String> {
    check(CAPABILITIES)
}

/// The invariant check over an arbitrary slice, so a test can falsify it on a
/// deliberately bad entry.
fn check(caps: &[Capability]) -> Result<(), String> {
    let mut prev: Option<&str> = None;
    for c in caps {
        if let Some(p) = prev
            && c.id <= p
        {
            return Err(format!("registry not sorted / unique at id {:?}", c.id));
        }
        prev = Some(c.id);

        if c.objective.is_empty() || c.strongest_reference.is_empty() {
            return Err(format!("{}: objective and reference are required", c.id));
        }
        if c.status.is_a_claim() && c.tests.trim().is_empty() {
            return Err(format!(
                "{}: status {} claims parity/superiority but names no test evidence (no evidence = UNVERIFIED)",
                c.id,
                c.status.label()
            ));
        }
        // A live capability must name where it runs; an absent/blocked one must not.
        let has_module = !c.rust_module.is_empty();
        match c.status {
            Status::Absent | Status::Blocked => {
                if has_module || !c.runtime_entrypoint.is_empty() {
                    return Err(format!(
                        "{}: an absent/blocked capability names an implementation",
                        c.id
                    ));
                }
            }
            Status::NotApplicable | Status::Unverified => {}
            _ => {
                if !has_module {
                    return Err(format!(
                        "{}: a verified capability names no Rust module",
                        c.id
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Renders the supersession matrix as Markdown — the content of
/// `docs/CAPABILITY_MATRIX.md`, kept in sync by a snapshot test.
#[must_use]
pub fn render_matrix() -> String {
    let mut s = String::new();
    s.push_str("# Capability supersession matrix\n\n");
    s.push_str(
        "Generated from `bleradar_core::registry` by its snapshot test — edit the registry, not this file.\n\n",
    );
    s.push_str(
        "Statuses are conservative: without a head-to-head competitor benchmark or physical-device \
         validation, a capability that works, is live and is tested is `PARTIAL` with its limit named, \
         not `SUPERIOR`. `SUPERIOR`/`PARITY` require test evidence (no evidence = `UNVERIFIED`). An \
         `ABSENT`, `UNVERIFIED` or unexplained `PARTIAL` capability blocks final competitive closure.\n\n",
    );
    s.push_str("| Capability | Objective | Strongest reference | Rust owner | Live entry point | Status | Residual gap |\n");
    s.push_str("|---|---|---|---|---|---|---|\n");
    for c in CAPABILITIES {
        let dash = |v: &'static str| if v.is_empty() { "—" } else { v };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} |",
            c.id,
            c.objective,
            c.strongest_reference,
            dash(c.rust_module),
            dash(c.runtime_entrypoint),
            c.status.label(),
            dash(c.residual_gap),
        );
    }
    s.push('\n');
    let count = |st: Status| CAPABILITIES.iter().filter(|c| c.status == st).count();
    let _ = writeln!(
        s,
        "Totals: {} SUPERIOR, {} PARITY, {} PARTIAL, {} ABSENT, {} BLOCKED_BY_PLATFORM, {} UNVERIFIED.",
        count(Status::Superior),
        count(Status::Parity),
        count(Status::Partial),
        count(Status::Absent),
        count(Status::Blocked),
        count(Status::Unverified),
    );
    s.push_str(
        "\nNo capability is marked `SUPERIOR`: none has a head-to-head competitive benchmark yet, so \
         the project is not at final competitive closure (§52/§53). Each `PARTIAL`/`ABSENT`/`BLOCKED` \
         row names what remains.\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_invariants_hold() {
        invariants().expect("capability registry invariants");
    }

    #[test]
    fn no_capability_is_superior_without_a_benchmark() {
        // The evidence rule's sharpest form for this project: nothing claims
        // SUPERIOR, because no head-to-head competitor benchmark exists yet.
        for c in CAPABILITIES {
            assert_ne!(
                c.status,
                Status::Superior,
                "{}: SUPERIOR needs a competitive benchmark this project does not yet have",
                c.id
            );
        }
    }

    #[test]
    fn a_parity_or_superior_status_without_evidence_is_rejected() {
        // Falsification: a PARITY/SUPERIOR claim with no test evidence must fail
        // the check, and a valid entry must pass it.
        let bad = Capability {
            id: "x",
            objective: "o",
            strongest_reference: "r",
            rust_module: "m",
            runtime_entrypoint: "e",
            tests: "",
            status: Status::Parity,
            residual_gap: "",
        };
        let err = check(&[bad]).unwrap_err();
        assert!(err.contains("no test evidence"), "{err}");

        let good = Capability {
            tests: "some test",
            ..bad
        };
        assert!(check(&[good]).is_ok());

        // An absent capability that names an implementation is also rejected.
        let contradictory = Capability {
            status: Status::Absent,
            tests: "",
            ..bad
        };
        assert!(
            check(&[contradictory])
                .unwrap_err()
                .contains("names an implementation")
        );
    }

    #[test]
    fn matrix_snapshot_is_in_sync() {
        // The committed matrix must equal what the registry renders. To update
        // it, run: UPDATE_CAPABILITY_MATRIX=1 cargo test -p bleradar-core matrix_snapshot
        let rendered = render_matrix();
        let committed = include_str!("../../../docs/CAPABILITY_MATRIX.md");
        if std::env::var("UPDATE_CAPABILITY_MATRIX").is_ok() {
            let path = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/CAPABILITY_MATRIX.md"
            );
            std::fs::write(path, &rendered).expect("write matrix");
            return;
        }
        assert_eq!(
            rendered, committed,
            "docs/CAPABILITY_MATRIX.md is stale; run UPDATE_CAPABILITY_MATRIX=1 cargo test -p bleradar-core matrix_snapshot"
        );
    }
}

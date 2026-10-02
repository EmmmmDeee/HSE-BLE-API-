//! Defensive ATT&CK *coverage claims* ledger for BLE Radar.
//!
//! This module records **claims** that the product can defensively observe,
//! verify integrity of, or otherwise evidence — keyed by MITRE ATT&CK
//! technique IDs. It is **not** an attack toolkit: it does not implement
//! offensive techniques, payloads, exploits, or detection rules that encode
//! attack steps.
//!
//! # Status derivation (mandatory)
//!
//! [`CapabilityStatus::Verified`] **requires a full evidence chain** and is
//! **never** accepted from static seed data alone. Status is always computed
//! by [`derive_status`] from [`ClaimScope`] + [`CapabilityEvidenceLinks`].
//! There is no API to manually set a row to `Verified`.
//!
//! Evidence chain (mandate):
//! `TECHNIQUE → RUST COMPONENT → SOURCE → INPUT → EXECUTION → OUTPUT →
//! PROVENANCE → INDEPENDENT CORROBORATION → TEST → (optional BENCHMARK) →
//! REGRESSION → VERIFIED STATUS`
//!
//! # Navigator export
//!
//! [`CapabilityLedger::navigator_layer`] emits ATT&CK Navigator layer 4.x
//! compatible JSON that is **derived-only** from `derive_status`. There is
//! no manual promotion / status-override API for export.
//!
//! ## Color map (derived status → Navigator)
//!
//! | Status | Score | Color |
//! |---|---:|---|
//! | Verified | 100 | `#31a354` |
//! | Partial | 50 | `#fec44f` |
//! | Unverified | 10 | `#de2d26` |
//! | NotApplicable | 0 | `#bdbdbd` |

use std::collections::BTreeMap;
use std::fmt;

/// Derived coverage status for a defensive ATT&CK claim.
///
/// Order is for sorting/display only; **never** assign `Verified` from static
/// data — use [`derive_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapabilityStatus {
    /// Full mandatory evidence chain present and healthy.
    Verified,
    /// Some but not all mandatory evidence links are present.
    Partial,
    /// In-scope claim with no (or insufficient) evidence, or hard failure.
    Unverified,
    /// Curated out of product scope; never counted as verified.
    NotApplicable,
}

impl CapabilityStatus {
    /// Navigator layer score for this status.
    #[must_use]
    pub const fn navigator_score(self) -> u8 {
        match self {
            Self::Verified => 100,
            Self::Partial => 50,
            Self::Unverified => 10,
            Self::NotApplicable => 0,
        }
    }

    /// Navigator layer color (hex) for this status.
    #[must_use]
    pub const fn navigator_color(self) -> &'static str {
        match self {
            Self::Verified => "#31a354",
            Self::Partial => "#fec44f",
            Self::Unverified => "#de2d26",
            Self::NotApplicable => "#bdbdbd",
        }
    }

    /// Stable label for comments / Display.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "Verified",
            Self::Partial => "Partial",
            Self::Unverified => "Unverified",
            Self::NotApplicable => "NotApplicable",
        }
    }
}

impl fmt::Display for CapabilityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether a technique is in product scope for a future defensive claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClaimScope {
    /// Product may eventually evidence a defensive claim for this technique.
    InScope,
    /// Explicitly out of product scope (omission ≠ N/A).
    NotApplicable,
}

/// Links from a capability row into the defensive evidence chain.
///
/// Status is **not** stored here; callers always pass these links through
/// [`derive_status`]. Benchmarks are optional for v0 `Verified`.
///
/// An identifier that is empty or only whitespace is **missing**: it never
/// satisfies a mandatory link, and [`CapabilityLedger::set_links`] /
/// [`CapabilityLedger::insert_row`] drop it when links are stored.
///
/// There are no stored health flags. Whether corroboration and regression are
/// satisfied is derived from the linked records by
/// [`CapabilityEvidenceLinks::corroboration_ok`] and
/// [`CapabilityEvidenceLinks::regression_ok`], so neither can be set by hand:
///
/// ```compile_fail,E0560
/// use bleradar_core::CapabilityEvidenceLinks;
/// let _ = CapabilityEvidenceLinks {
///     corroboration_ok: true,
///     ..CapabilityEvidenceLinks::empty()
/// };
/// ```
///
/// ```compile_fail,E0560
/// use bleradar_core::CapabilityEvidenceLinks;
/// let _ = CapabilityEvidenceLinks {
///     regression_ok: true,
///     ..CapabilityEvidenceLinks::empty()
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapabilityEvidenceLinks {
    /// Authoritative source identifiers for the claim.
    pub source_ids: Vec<String>,
    /// Declared inputs for the claim verification procedure.
    pub input_ids: Vec<String>,
    /// Record id of the executed verification procedure.
    pub execution_record_id: Option<String>,
    /// Structured outputs bound to the claim.
    pub output_ids: Vec<String>,
    /// Provenance claim id (e.g. EvidenceStore ClaimId).
    pub provenance_claim_id: Option<String>,
    /// Independent corroboration channel ids.
    pub corroboration_ids: Vec<String>,
    /// Linked test ids that must Pass for Verified.
    pub test_ids: Vec<String>,
    /// Optional benchmark ids (not required for v0 Verified).
    pub benchmark_ids: Vec<String>,
    /// Regression lock ids tied to this claim.
    pub regression_lock_ids: Vec<String>,
    /// Linked tests that are currently known Passed (ids subset of `test_ids`).
    pub passed_test_ids: Vec<String>,
    /// Linked tests currently Failed — forces downgrade.
    pub failed_test_ids: Vec<String>,
}

/// True when `id` names a record: blank or whitespace-only ids are missing.
fn id_present(id: &str) -> bool {
    !id.trim().is_empty()
}

/// True when at least one id in `ids` is present (non-blank).
fn any_id_present(ids: &[String]) -> bool {
    ids.iter().any(|id| id_present(id))
}

/// True when the optional record id is present (non-blank).
fn opt_id_present(id: Option<&String>) -> bool {
    id.is_some_and(|id| id_present(id))
}

/// Trim every id and drop the blank ones.
fn normalize_ids(ids: Vec<String>) -> Vec<String> {
    ids.into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
}

/// Trim an optional id; a blank one becomes `None`.
fn normalize_opt_id(id: Option<String>) -> Option<String> {
    id.map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

impl CapabilityEvidenceLinks {
    /// Empty links (InScope seed rows start here → Unverified).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Derived: the independent corroboration channel is satisfied, i.e. at
    /// least one non-blank `corroboration_ids` entry is linked.
    #[must_use]
    pub fn corroboration_ok(&self) -> bool {
        any_id_present(&self.corroboration_ids)
    }

    /// True when at least one non-blank `failed_test_ids` entry is linked.
    fn has_failed_tests(&self) -> bool {
        any_id_present(&self.failed_test_ids)
    }

    /// Derived: regression locks are intact, i.e. at least one non-blank
    /// `regression_lock_ids` entry is linked and no linked test has failed.
    #[must_use]
    pub fn regression_ok(&self) -> bool {
        any_id_present(&self.regression_lock_ids) && !self.has_failed_tests()
    }

    /// True when every mandatory field for `Verified` is satisfied.
    ///
    /// Blank or whitespace-only ids count as missing. Benchmarks are optional
    /// for v0. Requires:
    /// - a present id in `source_ids`, `input_ids`, `output_ids`, `corroboration_ids`
    /// - `execution_record_id` and `provenance_claim_id` present
    /// - a present id in `test_ids`
    /// - a present id in `regression_lock_ids` (no regression lock, no `Verified`)
    /// - derived [`Self::corroboration_ok`] and [`Self::regression_ok`]
    /// - no present id in `failed_test_ids`
    /// - every present `test_id` appears (trimmed) in `passed_test_ids`
    #[must_use]
    pub fn mandatory_complete(&self) -> bool {
        if !any_id_present(&self.source_ids)
            || !any_id_present(&self.input_ids)
            || !any_id_present(&self.output_ids)
            || !any_id_present(&self.test_ids)
        {
            return false;
        }
        if !opt_id_present(self.execution_record_id.as_ref())
            || !opt_id_present(self.provenance_claim_id.as_ref())
        {
            return false;
        }
        if !self.corroboration_ok() || !self.regression_ok() {
            return false;
        }
        self.test_ids
            .iter()
            .map(|tid| tid.trim())
            .filter(|tid| !tid.is_empty())
            .all(|tid| self.passed_test_ids.iter().any(|p| p.trim() == tid))
    }

    /// True if any mandatory evidence field has at least one present
    /// (non-blank) value (used to distinguish Partial vs Unverified when
    /// incomplete).
    #[must_use]
    pub fn any_mandatory_partial(&self) -> bool {
        any_id_present(&self.source_ids)
            || any_id_present(&self.input_ids)
            || opt_id_present(self.execution_record_id.as_ref())
            || any_id_present(&self.output_ids)
            || opt_id_present(self.provenance_claim_id.as_ref())
            || any_id_present(&self.corroboration_ids)
            || any_id_present(&self.test_ids)
            || any_id_present(&self.passed_test_ids)
            || any_id_present(&self.failed_test_ids)
            || any_id_present(&self.regression_lock_ids)
    }

    /// Trim every id and drop blank ones (applied whenever links are stored).
    fn normalized(self) -> Self {
        Self {
            source_ids: normalize_ids(self.source_ids),
            input_ids: normalize_ids(self.input_ids),
            execution_record_id: normalize_opt_id(self.execution_record_id),
            output_ids: normalize_ids(self.output_ids),
            provenance_claim_id: normalize_opt_id(self.provenance_claim_id),
            corroboration_ids: normalize_ids(self.corroboration_ids),
            test_ids: normalize_ids(self.test_ids),
            benchmark_ids: normalize_ids(self.benchmark_ids),
            regression_lock_ids: normalize_ids(self.regression_lock_ids),
            passed_test_ids: normalize_ids(self.passed_test_ids),
            failed_test_ids: normalize_ids(self.failed_test_ids),
        }
    }
}

/// Static claim specification used when seeding the ledger.
///
/// Seed rows always start with **empty** links. Comments in [`CapabilityLedger::seed_v0`]
/// mark these as claim placeholders, not attack implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityClaimSpec {
    /// ATT&CK technique id (`Txxxx` or `Txxxx.xxx`).
    pub technique_id: &'static str,
    /// Human-readable defensive claim label.
    pub name: &'static str,
    /// Rust component path owning the claim surface.
    pub rust_component: &'static str,
    /// In-scope vs explicit NotApplicable.
    pub scope: ClaimScope,
}

/// One ledger row: technique + component + evidence links.
///
/// **No stored status field** — status is always derived via [`CapabilityLedger::status_of`]
/// / [`derive_status`]. This makes manual `Verified` promotion impossible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRow {
    /// ATT&CK technique id.
    pub technique_id: String,
    /// Human-readable defensive claim label.
    pub name: String,
    /// Rust component path owning the claim surface.
    pub rust_component: String,
    /// In-scope vs explicit NotApplicable.
    pub scope: ClaimScope,
    /// Evidence links (status is never stored here).
    pub links: CapabilityEvidenceLinks,
}

/// Errors from ledger mutation APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    /// Technique id is not present in the ledger.
    UnknownTechnique(String),
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTechnique(id) => write!(f, "unknown technique id: {id}"),
        }
    }
}

impl std::error::Error for CapabilityError {}

/// Derive coverage status from scope + evidence links.
///
/// Rules (blank or whitespace-only ids count as missing throughout):
/// - `NotApplicable` scope → [`CapabilityStatus::NotApplicable`] (even if links present)
/// - any linked `failed_test_ids` (a broken regression) → [`CapabilityStatus::Unverified`]
/// - `mandatory_complete` (incl. derived corroboration/regression ok) → [`CapabilityStatus::Verified`]
/// - any mandatory field partially filled → [`CapabilityStatus::Partial`]
/// - else [`CapabilityStatus::Unverified`]
#[must_use]
pub fn derive_status(scope: ClaimScope, links: &CapabilityEvidenceLinks) -> CapabilityStatus {
    if scope == ClaimScope::NotApplicable {
        return CapabilityStatus::NotApplicable;
    }
    // Hard failure: a failed linked test breaks the regression → Unverified,
    // even if every other link exists (auto-downgrade).
    if links.has_failed_tests() {
        return CapabilityStatus::Unverified;
    }
    if links.mandatory_complete() {
        // mandatory_complete already requires the derived corroboration_ok and
        // regression_ok, and every linked test_id in passed_test_ids.
        return CapabilityStatus::Verified;
    }
    if links.any_mandatory_partial() {
        return CapabilityStatus::Partial;
    }
    CapabilityStatus::Unverified
}

/// Self-verifying ATT&CK capability ledger (technique_id → row).
///
/// Internally a [`BTreeMap`] for stable techniqueID sort order in exports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapabilityLedger {
    rows: BTreeMap<String, CapabilityRow>,
}

impl CapabilityLedger {
    /// Empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: BTreeMap::new(),
        }
    }

    /// Curated v0 seed: InScope claim placeholders (empty links → Unverified)
    /// plus explicit NotApplicable stubs.
    ///
    /// These are **CLAIM PLACEHOLDERS for future defensive evidence**, not
    /// implementations of attacks. `verified_count()` on the seed is **0**.
    #[must_use]
    pub fn seed_v0() -> Self {
        // Claim placeholders only — no attack procedures.
        const SPECS: &[CapabilityClaimSpec] = &[
            // InScope defensive sensing / integrity claim placeholders
            CapabilityClaimSpec {
                technique_id: "T1040",
                name: "BLE/Wi-Fi observation surface (defensive sensing claim)",
                rust_component: "bleradar-core::signal",
                scope: ClaimScope::InScope,
            },
            CapabilityClaimSpec {
                technique_id: "T1016",
                name: "local network/radio environment awareness claim",
                rust_component: "bleradar-core::runtime",
                scope: ClaimScope::InScope,
            },
            CapabilityClaimSpec {
                technique_id: "T1200",
                name: "BLE device proximity/hardware presence observation claim",
                rust_component: "bleradar-core::tracking",
                scope: ClaimScope::InScope,
            },
            CapabilityClaimSpec {
                technique_id: "T1195",
                name: "update artifact integrity verification claim",
                rust_component: "bleradar-core::update",
                scope: ClaimScope::InScope,
            },
            CapabilityClaimSpec {
                technique_id: "T1565",
                name: "evidence-store integrity / anti-tamper invariants claim",
                rust_component: "bleradar-core::evidence",
                scope: ClaimScope::InScope,
            },
            // Explicit NotApplicable stubs (omission ≠ N/A)
            CapabilityClaimSpec {
                technique_id: "T1566",
                name: "Phishing — n/a for BLE radar product scope",
                rust_component: "n/a",
                scope: ClaimScope::NotApplicable,
            },
            CapabilityClaimSpec {
                technique_id: "T1059",
                name: "Command and Scripting Interpreter — n/a",
                rust_component: "n/a",
                scope: ClaimScope::NotApplicable,
            },
            CapabilityClaimSpec {
                technique_id: "T1003",
                name: "OS Credential Dumping — n/a",
                rust_component: "n/a",
                scope: ClaimScope::NotApplicable,
            },
            CapabilityClaimSpec {
                technique_id: "T1021",
                name: "Remote Services — n/a",
                rust_component: "n/a",
                scope: ClaimScope::NotApplicable,
            },
            CapabilityClaimSpec {
                technique_id: "T1486",
                name: "Data Encrypted for Impact — n/a",
                rust_component: "n/a",
                scope: ClaimScope::NotApplicable,
            },
        ];

        let mut ledger = Self::new();
        for spec in SPECS {
            ledger.insert_row(CapabilityRow {
                technique_id: spec.technique_id.to_string(),
                name: spec.name.to_string(),
                rust_component: spec.rust_component.to_string(),
                scope: spec.scope,
                links: CapabilityEvidenceLinks::empty(),
            });
        }
        ledger
    }

    /// Insert or replace a row. Status is never stored — always derived.
    /// Link ids are trimmed and blank ones dropped.
    pub fn insert_row(&mut self, mut row: CapabilityRow) {
        row.links = row.links.normalized();
        self.rows.insert(row.technique_id.clone(), row);
    }

    /// Borrow a row by technique id.
    #[must_use]
    pub fn get(&self, technique_id: &str) -> Option<&CapabilityRow> {
        self.rows.get(technique_id)
    }

    /// Iterate rows sorted by technique_id (BTreeMap order).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &CapabilityRow)> {
        self.rows.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Update evidence links only; status remains derived. Link ids are
    /// trimmed and blank ones dropped, so a whitespace id is never stored.
    pub fn set_links(
        &mut self,
        technique_id: &str,
        links: CapabilityEvidenceLinks,
    ) -> Result<(), CapabilityError> {
        let row = self
            .rows
            .get_mut(technique_id)
            .ok_or_else(|| CapabilityError::UnknownTechnique(technique_id.to_string()))?;
        row.links = links.normalized();
        Ok(())
    }

    /// Derived status for a technique, if present.
    #[must_use]
    pub fn status_of(&self, technique_id: &str) -> Option<CapabilityStatus> {
        self.rows
            .get(technique_id)
            .map(|r| derive_status(r.scope, &r.links))
    }

    /// Count of rows whose derived status is [`CapabilityStatus::Verified`].
    /// NotApplicable never contributes.
    #[must_use]
    pub fn verified_count(&self) -> usize {
        self.rows
            .values()
            .filter(|r| derive_status(r.scope, &r.links) == CapabilityStatus::Verified)
            .count()
    }

    /// Counts per derived status.
    #[must_use]
    pub fn counts_by_status(&self) -> BTreeMap<CapabilityStatus, usize> {
        let mut counts = BTreeMap::new();
        for status in [
            CapabilityStatus::Verified,
            CapabilityStatus::Partial,
            CapabilityStatus::Unverified,
            CapabilityStatus::NotApplicable,
        ] {
            counts.insert(status, 0);
        }
        for row in self.rows.values() {
            let s = derive_status(row.scope, &row.links);
            *counts.entry(s).or_insert(0) += 1;
        }
        counts
    }

    /// Mark a linked test as failed: move/add to `failed_test_ids`, remove from
    /// `passed_test_ids`. Used to demonstrate auto-downgrade of derived status.
    /// The id is trimmed; a blank id names no test and changes nothing.
    pub fn invalidate_test(
        &mut self,
        technique_id: &str,
        test_id: &str,
    ) -> Result<(), CapabilityError> {
        let row = self
            .rows
            .get_mut(technique_id)
            .ok_or_else(|| CapabilityError::UnknownTechnique(technique_id.to_string()))?;
        // Same id rules as `set_links`: trimmed, and a blank id names no test.
        let test_id = test_id.trim();
        if test_id.is_empty() {
            return Ok(());
        }
        row.links.passed_test_ids.retain(|id| id != test_id);
        if !row.links.failed_test_ids.iter().any(|id| id == test_id) {
            row.links.failed_test_ids.push(test_id.to_string());
        }
        Ok(())
    }

    /// ATT&CK Navigator layer 4.x compatible JSON (std-only, deterministic).
    ///
    /// Scores/colors come **only** from [`derive_status`]. No manual status
    /// override parameter exists.
    #[must_use]
    pub fn navigator_layer(&self, name: &str, domain: &str, version: &str) -> String {
        let mut out = String::new();
        out.push('{');
        json_kv_str(&mut out, "name", name);
        out.push(',');
        out.push_str("\"versions\":{");
        json_kv_str(&mut out, "attack", version);
        out.push(',');
        json_kv_str(&mut out, "navigator", "4.9.1");
        out.push(',');
        json_kv_str(&mut out, "layer", "4.5");
        out.push('}');
        out.push(',');
        json_kv_str(&mut out, "domain", domain);
        out.push(',');
        json_kv_str(
            &mut out,
            "description",
            "Derived-only defensive ATT&CK coverage claims ledger export. \
             Status colors reflect derive_status; Verified requires full evidence chain. \
             Not an attack toolkit.",
        );
        out.push(',');
        out.push_str("\"techniques\":[");
        let mut first = true;
        for (tid, row) in &self.rows {
            if !first {
                out.push(',');
            }
            first = false;
            let status = derive_status(row.scope, &row.links);
            out.push('{');
            json_kv_str(&mut out, "techniqueID", tid);
            out.push(',');
            out.push_str("\"score\":");
            out.push_str(&status.navigator_score().to_string());
            out.push(',');
            json_kv_str(&mut out, "color", status.navigator_color());
            out.push(',');
            let comment = format!(
                "{} | {} | status={}",
                row.name,
                row.rust_component,
                status.as_str()
            );
            json_kv_str(&mut out, "comment", &comment);
            out.push('}');
        }
        out.push(']');
        out.push('}');
        out
    }
}

fn json_kv_str(out: &mut String, key: &str, value: &str) {
    out.push('"');
    out.push_str(key);
    out.push('"');
    out.push(':');
    json_string(out, value);
}

fn json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn empty_links_in_scope_are_unverified() {
        assert_eq!(
            derive_status(ClaimScope::InScope, &CapabilityEvidenceLinks::empty()),
            CapabilityStatus::Unverified
        );
    }

    #[test]
    fn na_scope_ignores_links() {
        let mut links = CapabilityEvidenceLinks::empty();
        links.source_ids.push("s1".into());
        assert_eq!(
            derive_status(ClaimScope::NotApplicable, &links),
            CapabilityStatus::NotApplicable
        );
    }
}

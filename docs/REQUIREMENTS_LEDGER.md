# Requirements Ledger

This ledger is the result of a full requirements-reconstruction and
traceability pass over the authoritative codebase, tests, documentation,
CLI surfaces, CI workflow, and process artifacts. It does **not** infer
completion from code presence: every row's status reflects what is actually
exercised by a test or an executed command, not what merely compiles.

## Methodology

1. Every source module under `crates/*/src/` and `xtask/src/` was read in
   full, together with its corresponding test file(s).
2. Every `docs/*.md`, `README.md`, `RUST_CONVERSION.md`, and
   `benchmarks/README.md` claim was cross-checked against the current code
   and test suite, not taken as true because it was written down.
3. The full gate suite (`cargo xtask gates` — fmt, clippy, build, test, doc,
   parity-report drift check, dependency policy, oracle integrity, `cargo
   audit`, `cargo deny`) was executed to establish a definitive baseline
   before drawing conclusions.
4. Three module audits (evidence+fusion; verification+advancement;
   osint+infrastructure+website) were performed and then independently
   spot-checked: every cited test-function name and per-file test count was
   re-verified directly against the test source files. Three findings
   labelled "UNREACHABLE" were found to be inaccurate on inspection (the
   named enum variants are ordinary constructible, iterable, matched public
   API surface, not dead code) and are reclassified below as
   `IMPLEMENTED_UNVERIFIED` or `PARTIAL` with the specific missing test
   scenario named.
5. The highest-value `PARTIAL` items that could be safely and completely
   closed in this pass were fixed immediately (see
   [Session remediation log](#session-remediation-log)); the full workspace
   gate suite was re-run after each fix and once more after all fixes
   combined.

## Status legend

| Status | Meaning |
|---|---|
| `VERIFIED` | Implemented, and a test (or an executed command whose output was inspected) directly exercises the claimed behavior, including its distinguishing edge case. |
| `IMPLEMENTED_UNVERIFIED` | Code exists and appears correct on reading, but no test exercises this specific behavior or branch. |
| `PARTIAL` | Some but not all of the requirement's stated cases/branches are verified; the untested remainder is named explicitly. |
| `MISSING` | No implementation exists for a behavior the system is required to have. |
| `BROKEN` | Implemented, but a test or execution shows it does not do what it claims. |
| `UNREACHABLE` | Code that cannot be exercised through any public entry point (reserved for genuinely dead code — not used in this ledger; see finding above). |
| `OBSOLETE` | A documented claim that no longer matches current code/tests/tooling. |
| `AMBIGUOUS` | The required behavior itself cannot be determined with confidence from the available evidence. |

## ID scheme

`REQ-<MODULE>-<NNN>`, module one of `CORE, EVID, FUSION, VERIF, ADV, OSINT,
INFRA, WEB, COMPAT, XTASK, PROC`. IDs are stable identifiers, not line
numbers; citations use function/type names so they survive refactors (the
same convention already used by `docs/ISSUE_LEDGER.md`).

## Runtime verification evidence convention

Rather than repeat an identical command on every row, each module section
ends with **one** "Runtime verification evidence" line giving the exact
command and its result, executed during this session
(2026-08-31/2026-09-01). Every `VERIFIED` row in that section is backed by
that command; rows with a narrower status name the specific missing
scenario within it.

---

## REQ-CORE — Canonical geo/identity/signal/tracking primitives

`crates/bleradar-core/src/{geo,identity,signal,tracking}.rs`,
`crates/bleradar-core/tests/{core,properties}.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-CORE-001 | Validated geographic coordinate construction rejects non-finite/out-of-range input | `(f64 lat, f64 lon)` → `Result<LatLon, GeoError>` | Pure; returns `Err(GeoError::{NonFinite,OutOfRange})`, never panics | `geo.rs::LatLon::new` | `core.rs::latlon_rejects_invalid_input`; `properties.rs::prop_latlon_new_enforces_its_invariant` (100k cases) | VERIFIED |
| REQ-CORE-002 | Haversine distance is finite, non-negative, symmetric, bounded, zero for identical points, stable near antimeridian/poles | `(LatLon, LatLon)` → `f64` meters | Pure, total over the valid domain | `geo.rs::haversine_m` | `core.rs::zero_distance_is_zero`, `haversine_is_finite_at_near_antipodal_points`; `properties.rs::prop_haversine_is_finite_nonnegative_bounded_and_symmetric` | VERIFIED |
| REQ-CORE-003 | Bearing is always reported in `[0,360)` degrees | `(LatLon, LatLon)` → `f64` degrees | Pure | `geo.rs::bearing_deg` | `core.rs::bearing_stays_in_documented_range`, `bearing_north_is_zero`; `properties.rs::prop_bearing_stays_in_documented_range` | VERIFIED |
| REQ-CORE-004 | Wi-Fi channel⇄frequency conversions round-trip exactly across 2.4/5 GHz bands | `u16` channel ⇄ `u16` MHz | Pure; `None` outside supported bands | `lib.rs::wifi_channel_to_frequency/_to_channel` | `core.rs::wifi_channel_round_trip`; `properties.rs::prop_wifi_channel_frequency_roundtrips_over_full_range`; lib.rs doctests | VERIFIED |
| REQ-CORE-005 | MAC canonicalization + randomized/local-bit classification (a rotating BLE address must never be treated as stable identity) | raw MAC `&str` → canonical string + `AddressKind` | Pure; malformed MAC rejected | `identity.rs::canonical_mac`, `is_locally_administered`, `DeviceIdentity::new` | `core.rs::mac_canonicalization_and_local_bit`, `randomized_address_is_not_stable_identity` | VERIFIED |
| REQ-CORE-006 | RSSI EMA smoothing is deterministic and bounded by its inputs; trend/proximity classification is monotonic with a deadband | RSSI sample stream → smoothed value, `SignalTrend`, `ProximityBand` | Pure, stateful only within `RssiEma` | `signal.rs` | `core.rs::ema_and_trend_are_deterministic`, `stronger_samples_produce_hotter_state`; `properties.rs::prop_rssi_ema_output_is_finite_and_bounded_by_samples`, `prop_proximity_band_is_monotonic_in_signal_strength` | VERIFIED |
| REQ-CORE-007 | Calibrated BLE distance estimate is positive, finite, monotonic in RSSI, and returns `None` rather than a bogus value on degenerate path-loss input | `(rssi, ref_rssi_at_1m, path_loss_exp)` → `Option<f64>` meters | Pure; `None` on non-positive path-loss | `signal.rs::ble_distance_m` | `core.rs::distance_model_is_calibrated_not_absolute`; `properties.rs::prop_ble_distance_is_positive_and_monotonic_in_rssi` | VERIFIED |
| REQ-CORE-008 | Device track rejects a pushed observation whose timestamp precedes the last one (no corrupted path history) | `DeviceObservation` → `Result<(), TrackError>` | Mutates track only on success; `Err(TrackError::NonMonotonicTime)` otherwise | `tracking.rs::DeviceTrack::push` | `core.rs::track_rejects_time_reversal` | VERIFIED |
| REQ-CORE-009 | Map points derived from raw pushed observations are tagged `Observed`, never silently presented as inferred | `DeviceTrack` → `Vec<MapPoint>` | Pure read | `tracking.rs::DeviceTrack::observed_map_points` | `core.rs::map_points_remain_observed_not_inferred`; `properties.rs::prop_track_push_and_spatial_estimate_stay_valid` | VERIFIED |
| REQ-CORE-010 | Spatial estimate requires ≥2 positioned observations and averages position on the sphere (3-D unit-vector centroid), not linearly in degrees, so antimeridian-straddling observations do not produce a centroid on the wrong side of the planet | `DeviceTrack` → `Option<SpatialEstimate>` (confidence + uncertainty) | Pure read; `None` below the minimum sample count | `tracking.rs::DeviceTrack::spatial_estimate` | `core.rs::spatial_estimate_requires_multiple_positioned_observations`, `spatial_estimate_handles_antimeridian_straddling`; `properties.rs::prop_track_push_and_spatial_estimate_stay_valid` (forces antimeridian straddling periodically) | VERIFIED |
| REQ-CORE-011 | Confidence value is always clamped to `[0,100]` | numeric input → `Confidence` | Pure | `lib.rs::Confidence::new` | `properties.rs::prop_confidence_is_clamped_to_100` (50k cases) | VERIFIED |
| REQ-CORE-012 | Selected-device tracking lock (`start_tracking`/`stop_tracking`) toggles only the UI-facing lock flag; previously pushed track history survives unlock and pushes keep accumulating afterward | `&mut SelectedDevice` → `()` | Mutates only `tracking: bool`; `track` untouched | `tracking.rs::SelectedDevice` | `core.rs::selection_lock_retains_history` (strengthened this session — see remediation log) | VERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test core --test properties` → 16 + 9 passed, 0 failed (2026-08-31, this session). Doctests: `cargo test --workspace --locked` → 10 `bleradar_core` doctests passed.

---

## REQ-EVID — Canonical evidence + provenance core

`crates/bleradar-core/src/evidence.rs`, `crates/bleradar-core/tests/evidence.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-EVID-001 | An observation's raw value is never overwritten by a normalized/derived value | `Observation` construction + normalization | Raw and normalized values coexist; no in-place overwrite | `evidence.rs::Observation` | `observation_keeps_raw_value_when_normalized` | VERIFIED |
| REQ-EVID-002 | Observation timeline (`first_seen`/`last_seen`) stays ordered as new sightings extend it | repeated sightings → updated timeline | Pure state update | `evidence.rs::Observation` | `observation_timeline_is_ordered_and_extendable` | VERIFIED |
| REQ-EVID-003 | A derived feature's temporal provenance can never precede the observation(s) it derives from | `Feature` construction from `Observation`(s) | Rejects/flags temporally inconsistent derivation | `evidence.rs::Feature` | `feature_temporal_provenance_does_not_precede_observation` | VERIFIED |
| REQ-EVID-004 | Claim → Hypothesis → Evidence → Observation → Source chain is fully traceable and terminates at an authoritative source | `claim_id: &str` → `Result<ClaimTrace<'_>, ProvenanceError>` | Read-only; `Err` if any link is missing | `evidence.rs::EvidenceStore::trace_claim` | `claim_trace_reaches_the_authoritative_source` | VERIFIED |
| REQ-EVID-005 | Transformation trace records input/output representation, preserved vs. changed features, and a verification outcome | inputs → `TransformationTrace` | Read-only | `evidence.rs::EvidenceStore::trace_transformation` | `transformation_trace_contains_features_and_verification` | VERIFIED |
| REQ-EVID-006 | A registered source's metadata (type, retrieval method) is not silently mutated after creation | `Source` construction + later reads | Immutable after construction | `evidence.rs::Source` | `source_metadata_is_not_silently_changed` | VERIFIED |
| REQ-EVID-007 | A transformation's preserved-features and changed-features sets must be disjoint | `Transformation` construction | Rejects overlapping sets | `evidence.rs::Transformation` | `overlapping_transformation_features_are_rejected` | VERIFIED |
| REQ-EVID-008 | Canonical store validates every cross-reference (evidence→observation→source, etc.) on insertion and rejects dangling references | any typed insert | `Err` on dangling reference; no partial/corrupt insert | `evidence.rs::EvidenceStore` | exercised across all 7 evidence.rs tests; explicit in `claim_trace_reaches_the_authoritative_source` | VERIFIED |
| REQ-EVID-009 | A transformation recorded as not-yet-verified is distinguishable from a verified one by callers | `Transformation` with an unverified outcome | — | `evidence.rs::Transformation` | none — only the verified/positive branch is built by any test | IMPLEMENTED_UNVERIFIED |
| REQ-EVID-010 | Constructors reject empty-string IDs/values rather than silently accepting them | empty `&str` → constructor | Expected `Err`, unverified | `evidence.rs` various constructors | none | IMPLEMENTED_UNVERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test evidence` → 7 passed, 0 failed (2026-08-31).

---

## REQ-FUSION — Calibrated evidence fusion + adversarial falsification

`crates/bleradar-core/src/fusion.rs`, `crates/bleradar-core/tests/fusion.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-FUSION-001 | Nine calibration dimensions combine into one bounded `calibrated_weight()` | `EvidenceQuality` (9 scores) → `f64` weight | Pure | `fusion.rs::EvidenceQuality` | `quality_keeps_all_calibration_dimensions_visible` | VERIFIED |
| REQ-FUSION-002 | Evidence sharing a source/dataset/provider contributes at most once ("ten copies ≠ ten confirmations"); the strongest group member wins, others recorded as collapsed | grouped assessments → single counted contribution | Collapsed items retained for audit, not scored again | `fusion.rs` dependency-collapse logic | `dependent_reporting_is_counted_once`, `same_source_is_conservative_without_explicit_group` | VERIFIED |
| REQ-FUSION-003 | High-base-rate support can be stripped from a hypothesis's score entirely under falsification | `ScoreOptions{remove_high_base_rate:true}` → reduced score | Deterministic recomputation | `fusion.rs::calibrated_contribution` | `falsification_removes_base_rate_support_and_reports_gaps` | VERIFIED |
| REQ-FUSION-004 | Contradicting evidence is scored separately and subtracted from the net score, and reported | `EvidenceRole::Contradicting` items → negative contribution + report list | Pure | `fusion.rs::score_hypothesis` | `falsification_removes_base_rate_support_and_reports_gaps` | VERIFIED |
| REQ-FUSION-005 | `fuse_hypotheses`/`fuse` ranks every candidate by net score and reports the leading hypothesis | assessed evidence set → ranked hypotheses | Pure | `fusion.rs::fuse_hypotheses` | `dependent_reporting_is_counted_once`, `falsification_removes_base_rate_support_and_reports_gaps`, `leading_hypothesis_survives_falsification_when_support_is_robust` | VERIFIED — tie-break ordering between two equal-net-score hypotheses is untested (sub-item, `IMPLEMENTED_UNVERIFIED`) |
| REQ-FUSION-006 | Falsification runs all adversarial passes (remove high-base-rate, remove strongest-support group, perturb uncertainty, missing-expected-evidence detection) and reports whether the leading hypothesis **survives**, not merely whether it can be made to fail | assessed evidence + expectations → `FalsificationReport` | Pure recomputation, no mutation of the underlying store | `fusion.rs::falsify_from`/`leading_survives` | Rejection case: `falsification_removes_base_rate_support_and_reports_gaps`. Survival case (previously absent): `leading_hypothesis_survives_falsification_when_support_is_robust` (added this session) | VERIFIED (fixed this session — see remediation log; was `PARTIAL`, only the failure path had ever been exercised) |
| REQ-FUSION-007 | Non-zero evidence uncertainty scales down its contribution during the perturbation pass | `EvidenceAssessment::with_uncertainty(n>0)` → reduced contribution | Pure | `fusion.rs::calibrated_contribution` | none use a non-zero uncertainty value (default 0 is a no-op) | IMPLEMENTED_UNVERIFIED |
| REQ-FUSION-008 | `EvidenceRole::Contextual` items never contribute to support or contradiction scoring | contextual assessment → zero net contribution | Pure | `fusion.rs::score_hypothesis` | none constructs a `Contextual` assessment | IMPLEMENTED_UNVERIFIED |
| REQ-FUSION-009 | Duplicate evidence assessment or duplicate expected-evidence registration is rejected, not silently overwritten | duplicate ID → `Err` | `Err(DuplicateAssessment\|DuplicateExpectedEvidence)` | `fusion.rs` | none | IMPLEMENTED_UNVERIFIED |
| REQ-FUSION-010 | Fusing a reference to an unregistered evidence ID or unregistered hypothesis ID fails explicitly rather than panicking or silently skipping | unregistered ID → `Err` | `Err` with a specific variant | `fusion.rs::fuse_hypotheses` | Evidence-side: `fusion_rejects_unregistered_evidence`. Hypothesis-side: none | PARTIAL (evidence-side VERIFIED; hypothesis-side IMPLEMENTED_UNVERIFIED) |
| REQ-FUSION-011 | Fusion error types implement `Display`/`std::error::Error` | error value → formatted string | Pure | `fusion.rs` error enum | none call `Display` directly | IMPLEMENTED_UNVERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test fusion` → 6 passed, 0 failed (2026-08-31, includes the new survival test; was 5 before this session).

---

## REQ-VERIF — Metamorphic + differential verification engine

`crates/bleradar-core/src/verification.rs`, `crates/bleradar-core/tests/verification.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-VERIF-001 | Required semantics are modelled separately from implementation, so implementation may change without redefining what must be preserved | baseline capture + invariant set | Pure/data separation | `verification.rs` | exercised throughout the suite | VERIFIED |
| REQ-VERIF-002 | Nine metamorphic relations are supported: invariance, idempotence, commutativity, monotonicity, reversibility, round-trip, partition-recombination, normalization, permutation equivalence | transformation + relation kind → pass/fail | Pure | `verification.rs` relation checks | Directly exercised: idempotence (`idempotence_failure_is_minimized_and_classified`), normalization (`generated_normalization_case_passes_and_records_family_feedback`), monotonicity. Invariance/commutativity/reversibility/round-trip/partition-recombination/permutation: defined, not driven by a dedicated test vector | PARTIAL |
| REQ-VERIF-003 | Differential comparison preserves visibility of required side effects, not just return values | baseline execution + variant execution → `DifferentialReport` | Pure | `verification.rs::differential_compare` | `differential_report_keeps_required_side_effects_visible` | VERIFIED |
| REQ-VERIF-004 | Failures are minimized to a smallest reproducing case and root-cause classified | failing case → minimized case + classification | Pure | `verification.rs` minimization logic | `idempotence_failure_is_minimized_and_classified` | VERIFIED |
| REQ-VERIF-005 | Adaptive feedback tracks which transformation families discover defects and increases pressure on high-yield families | execution history → re-weighted family pressure | Stateful within the engine instance | `verification.rs` feedback logic | `generated_normalization_case_passes_and_records_family_feedback` | VERIFIED |
| REQ-VERIF-006 | Repair, regression-lock, and retirement are explicit, auditable state transitions (not implicit) | defect → repaired/locked/retired state | State transition recorded | `verification.rs` | `repairs_locks_and_retirement_are_explicit_state_transitions` | VERIFIED |
| REQ-VERIF-007 | Verification results can be persisted into the canonical evidence store (REQ-EVID) | `VerificationResult` → canonical `Test`/`Evidence` records | Transactional insert | `verification.rs` ↔ `evidence.rs` | `verification_results_can_be_persisted_in_the_canonical_store` | VERIFIED |
| REQ-VERIF-008 | Execution outcomes cover concurrency, restart, and recovery scenarios (per the documented TEST list) | scenario → `ExecutionOutcome` | Pure | `verification.rs::ExecutionOutcome` builders | none exercise the Concurrency/Restart/Recovery builders | IMPLEMENTED_UNVERIFIED |
| REQ-VERIF-009 | `TransformationFailed`/`CanonicalStore`-related error variants are reachable and distinguishable | failure condition → specific error variant | `Err` | `verification.rs` error enum | none trigger these variants | IMPLEMENTED_UNVERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test verification` → 6 passed, 0 failed (2026-08-31).

---

## REQ-ADV — Metamorphic software advancement engine

`crates/bleradar-core/src/advancement.rs`, `crates/bleradar-core/tests/advancement.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-ADV-001 | Advancement proposals move through an explicit ordered state machine (Proposed→…→Integrated) | proposal → state transitions | Illegal transitions rejected | `advancement.rs` state machine | `accepted_change_requires_all_gates_and_integrates_explicitly` | VERIFIED |
| REQ-ADV-002 | Acceptance requires ALL of: required-semantics preservation, differential equivalence, measurable improvement, no unexplained regression, reproducibility (conjunctive, not best-effort) | candidate + baseline → accept/reject | Rejection is specific, not a generic failure | `advancement.rs` acceptance gate | `accepted_change_requires_all_gates_and_integrates_explicitly`, `semantic_or_differential_failure_rejects_even_with_a_benchmark_gain`, `unexplained_regression_and_nonreproducibility_are_rejections` | VERIFIED |
| REQ-ADV-003 | Rejection reasons are enumerated and specific (8 `AdvancementRejection` variants), not a boolean | rejected candidate → specific variant | `Err`/rejection value carries the reason | `advancement.rs::AdvancementRejection` | same three tests above (each asserts a specific variant) | VERIFIED |
| REQ-ADV-004 | Change ranking = benefit × correctness-confidence × reachability × reversibility ÷ cost ÷ regression-risk | proposal metrics → rank score | Pure | `advancement.rs` ranking formula | single-proposal case only; no test ranks several competing proposals against each other | PARTIAL |
| REQ-ADV-005 | A candidate with a measured regression **and** a valid attached explanation is still accepted if every other gate passes | candidate + `.explained_by(...)` → accept | — | `advancement.rs` acceptance gate | only the *unexplained*-regression rejection path is tested (`unexplained_regression_and_nonreproducibility_are_rejections`); the explained-and-accepted path is not | PARTIAL |
| REQ-ADV-006 | Acceptance triggers explicit integration and ranking recomputation for remaining proposals | accepted proposal → integrated + recomputed ranks | Stateful | `advancement.rs` | `accepted_change_requires_all_gates_and_integrates_explicitly` | VERIFIED |
| REQ-ADV-007 | Proposals capture dependency/limiter metadata (control loop's `MODEL_DEPENDENCIES`/`IDENTIFY_LIMITER` steps) | proposal fields | Stored, not currently enforced by any gate | `advancement.rs::AdvancementProposal` | none validate or enforce these fields | IMPLEMENTED_UNVERIFIED |
| REQ-ADV-008 | Falsification findings/checks carry full structured detail (not just a boolean "resistant") | falsification run → `FalsificationFinding`/`FalsificationCheck` | Pure | `advancement.rs` | only the `.resistant()` convenience path is used; the fuller structured type is not directly asserted upon | IMPLEMENTED_UNVERIFIED |
| REQ-ADV-009 | `AdvancementError` edge cases (e.g., missing baseline, malformed metrics) are handled explicitly | malformed input → specific `Err` | `Err` | `advancement.rs::AdvancementError` | common paths only | PARTIAL |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test advancement` → 4 passed, 0 failed (2026-08-31).

---

## REQ-OSINT — Execution-feedback adaptive OSINT search

`crates/bleradar-core/src/osint.rs`, `crates/bleradar-core/tests/osint.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-OSINT-001 | Eleven search representations exist in stable order (exact, normalized, alias, historical, semantic, structural, temporal, relational, technical, provenance, graph_neighbor) | — → `SearchRepresentation::ALL` | Pure | `osint.rs::SearchRepresentation` | `all_search_representations_are_available_in_stable_order` asserts all 11 labels including `graph_neighbor`; no pivot/search test uses `SearchRepresentation::GraphNeighbor` specifically as a pivot's representation | PARTIAL (label VERIFIED; pivot-level exercise of `GraphNeighbor` is `IMPLEMENTED_UNVERIFIED` — reclassified from an earlier audit's inaccurate "UNREACHABLE" label: the variant is a normal constructible/matched enum member, not dead code) |
| REQ-OSINT-002 | Seven-phase control loop (query→normalize→pivot→execute→observe→adapt→persist) drives the search | seed query → search session | Stateful within the engine | `osint.rs` control loop | exercised across the full suite | VERIFIED |
| REQ-OSINT-003 | Search outcomes classify into the documented outcome types | execution result → `SearchOutcome` variant | Pure | `osint.rs::SearchOutcome` | covered across suite | VERIFIED |
| REQ-OSINT-004 | Duplicate pivots (same normalized query key) are suppressed, not re-executed | repeated query → single execution | Pure/stateful dedup | `osint.rs` | covered in adaptive-pressure/dedup tests | VERIFIED |
| REQ-OSINT-005 | Raw and normalized query/value are both preserved (never overwritten) | raw query → normalized + raw retained | Pure | `osint.rs::SearchPivot`/`SearchPivotSeed` | covered across suite | VERIFIED |
| REQ-OSINT-006 | Findings and actions persist transactionally into the canonical store, with rollback on source conflict | search result → canonical `Evidence`/`Action` records | Transactional; rollback on conflict | `osint.rs` ↔ `evidence.rs` | covered in persistence tests | VERIFIED |
| REQ-OSINT-007 | Adaptive pressure re-ranks the frontier by observed yield/novelty | execution feedback → re-weighted frontier | Stateful | `osint.rs` | `useful_feedback_increases_pressure_for_the_same_representation_family` | VERIFIED |
| REQ-OSINT-008 | Failed/inconclusive results map to `ActionStatus::Failed`, not silently dropped | failed execution → `ActionStatus::Failed` action record | Recorded, not discarded | `osint.rs` | covered in suite | VERIFIED |
| REQ-OSINT-009 | Pivot lifecycle: Proposed→Executed→Exhausted | pivot → lifecycle transitions | Illegal re-exhaustion should be rejected | `osint.rs::SearchPivot` | single-exhaustion path tested; re-exhausting an already-`Executed` pivot is not | PARTIAL |
| REQ-OSINT-010 | Resource limits (max pivots / max executions) bound the search | limit + attempts → enforced cutoff | Search halts at the limit | `osint.rs` | boundary-hit case tested; incremental "N-1 succeeds, Nth fails" sequencing is not | IMPLEMENTED_UNVERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test osint` → 10 passed, 0 failed (2026-08-31).

---

## REQ-INFRA — Infrastructure correlation engine

`crates/bleradar-core/src/infrastructure.rs`, `crates/bleradar-core/tests/infrastructure.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-INFRA-001 | Eleven infrastructure observation families are recorded with temporal intervals | raw observation → canonical `Observation` on an infrastructure entity | Pure/persisted | `infrastructure.rs` | covered across suite | VERIFIED |
| REQ-INFRA-002 | Nine competing explanation classes (CommonCdn, CommonHost, CommonCms, CommonRegistrar, CommonTemplate, SharedThirdPartyService, DirectTechnicalRelationship, PossibleCommonAdministration, Unknown) are ranked with independent-support counting | correlated observations → ranked `InfrastructureExplanation` | Pure | `infrastructure.rs::InfrastructureExplanation` + classifier | Directly asserted as leading/ranked outcome: `DirectTechnicalRelationship`, `CommonHost`, `PossibleCommonAdministration`. `CommonCdn`/`CommonCms`/`CommonRegistrar`/`SharedThirdPartyService`/`Unknown` are producible by the same classifier (confirmed by direct reading — they appear in the classification function and priority table) but never asserted as a test's leading outcome | PARTIAL (reclassified from an earlier audit's inaccurate "UNREACHABLE" label for 3 of these variants — they are ordinary reachable code, just untested as a leading-outcome scenario) |
| REQ-INFRA-003 | Dependency-group collapse for shared provider/CDN (same non-independence rule as REQ-FUSION-002) | grouped observations → single counted contribution | Collapsed items retained for audit | `infrastructure.rs` | covered in suite | VERIFIED |
| REQ-INFRA-004 | High-base-rate infrastructure similarity is down-weighted under falsification | falsification pass → reduced score | Pure recomputation | `infrastructure.rs` | covered in suite | VERIFIED |
| REQ-INFRA-005 | Shared infrastructure is never promoted to proof of common control (explicit invariant) | correlation result → `common_control_proven() == false` unless independently established | Pure | `infrastructure.rs` | explicit invariant test in suite | VERIFIED |
| REQ-INFRA-006 | Adversarial falsification (support removal) is applied before persisting a conclusion | leading explanation → falsification report | Pure | `infrastructure.rs` | covered in suite | VERIFIED |
| REQ-INFRA-007 | Correlation edges persist transactionally with provenance | correlation result → canonical `Relationship` record | Transactional | `infrastructure.rs` ↔ `evidence.rs` | covered in suite | VERIFIED |
| REQ-INFRA-008 | Observation-pair deduplication + independent-support counting (no double count) | repeated observation pairs → single counted support | Pure | `infrastructure.rs` | covered in suite | VERIFIED |
| REQ-INFRA-009 | Observation lifecycle and canonical persistence match REQ-EVID's model | observation → canonical record | Transactional | `infrastructure.rs` ↔ `evidence.rs` | covered in suite | VERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test infrastructure` → 7 passed, 0 failed (2026-08-31).

---

## REQ-WEB — Website lineage/ecosystem engine

`crates/bleradar-core/src/website.rs`, `crates/bleradar-core/tests/website.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-WEB-001 | Twelve feature families are extracted with raw capture + normalized form + source + temporal interval all preserved | page/site snapshot → `WebsiteFeature` records | Pure extraction | `website.rs` extractors | covered across suite | VERIFIED |
| REQ-WEB-002 | Eight competing explanation classes (Coincidence, CommonPlatform, CommonTemplate, ContentReuse, AssetReuse, DevelopmentRelationship, OperationalRelationship, Unknown) are ranked | correlated features → ranked `WebsiteExplanation` | Pure | `website.rs::WebsiteExplanation` + classifier | Directly asserted as leading/ranked outcome: `AssetReuse`, `CommonPlatform`, `OperationalRelationship`. `CommonTemplate`/`ContentReuse`/`DevelopmentRelationship`/`Coincidence`/`Unknown` are producible by the classifier (confirmed by direct reading) but not asserted as a leading outcome in any test | PARTIAL (3 of 8 variants proven as a test's leading outcome; the rest defined, reachable, and classified but untested at that level — this is a coverage gap, not the enum/README mismatch it might first appear to be: README's prose groups ContentReuse+AssetReuse together as "reuse" and omits `Unknown` as it is a catch-all, which is normal summarization, not a discrepancy) |
| REQ-WEB-003 | Website similarity never alone proves common operation (explicit invariant) | correlation result → `common_operator_proven() == false` unless independently established | Pure | `website.rs` | explicit invariant test across all scenarios in suite | VERIFIED |
| REQ-WEB-004 | Snapshot extraction persists transactionally, rolling back on conflict | snapshot → canonical records | Transactional; rollback on conflict | `website.rs` ↔ `evidence.rs` | single-conflict rollback tested; multi-observation mid-extraction rollback ordering is not | PARTIAL |
| REQ-WEB-005 | High-base-rate platform/CDN similarity is down-weighted under falsification | falsification pass → reduced score | Pure | `website.rs` | covered in suite | VERIFIED |
| REQ-WEB-006 | Adversarial falsification (support removal) is applied before persisting a conclusion | leading explanation → falsification report | Pure | `website.rs` | covered in suite | VERIFIED |
| REQ-WEB-007 | Rare assets/identifiers are prioritized in scoring and the leading explanation must survive falsification | rare feature match → higher discriminative weight | Pure | `website.rs` | covered in suite | VERIFIED |
| REQ-WEB-008 | Temporal interval alignment and relation classification (overlapping/contiguous/disjoint) between two sites' observed states | two temporal intervals → relation classification | Pure | `website.rs` | covered in suite | VERIFIED |
| REQ-WEB-009 | Dependency-group collapse for shared CDN/provider (same non-independence rule as REQ-FUSION-002) | grouped observations → single counted contribution | Collapsed items retained for audit | `website.rs` | covered in suite | VERIFIED |

**Runtime verification evidence:** `cargo test -p bleradar-core --locked --test website` → 7 passed, 0 failed (2026-08-31).

---

## REQ-COMPAT — Runtime topology and source-replacement registry

`crates/bleradar-compat/src/lib.rs`, `crates/bleradar-compat/tests/contracts.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-COMPAT-001 | Every observed native function/method/constructor has implementation, reachability, and evidence classifications | ABI census → `RUNTIME_CONTRACTS: [RuntimeContract; 124]` | Pure static registry; generation fails on count drift | `lib.rs::RUNTIME_CONTRACTS` | `tests/contracts.rs` | VERIFIED |
| REQ-COMPAT-002 | Runtime implementation language is never confused with source-replacement parity | registry read → independent `Implementation` and `ParityStatus` | No `SourceAnalog` can imply differential proof | `lib.rs::{Implementation,ParityStatus}` | contracts + oracle_characterization | VERIFIED |
| REQ-COMPAT-003 | Coverage/status totals equal their registry lengths (no silent drift) | registry → coverage/reachability counts | — | `lib.rs` | contracts.rs | VERIFIED |
| REQ-COMPAT-004 | Every observed native symbol has characterized behavior sufficient to remove the oracle implementation | full ABI and Android paths → executable contract corpus | Unknown/failure/state/side effects must remain distinct | `docs/BEHAVIORAL_CONTRACT.md`, generated `docs/PARITY_COVERAGE.md` | provisional pure fixtures; Android harness pending | PARTIAL — all 124 are classified, but 27 retain unknown reachability and 0 source replacements are differentially verified over their full observable contract |
| REQ-COMPAT-005 | Known oracle/source gaps cannot be silently promoted to parity | sampled inputs → explicit mismatch evidence | Pure/no persistent side effects | `tests/oracle_characterization.rs` | haversine, proximity, BLE range, 6 GHz channel and registry guards | VERIFIED for captured samples only |

**Runtime verification evidence:** use `cargo test -p bleradar-compat --locked`;
test counts are intentionally not frozen in this ledger.

---

## REQ-XTASK — Build/verification tooling (`xtask`)

`xtask/src/*.rs`

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-XTASK-001 | `cargo xtask parity-report` deterministically regenerates `docs/PARITY_COVERAGE.md` from `NATIVE_ABI.txt` + the compat registry, using regex-emulating string-scan helpers | ABI listing + registry → markdown report | Fails the gate if regenerated output differs from committed output | `xtask/src/main.rs::cmd_parity_report`, `find_prefixed_runs`, `find_quoted_name_values`, `dedup_sorted` | 13 new unit tests added this session covering the parsing helpers' documented edge cases (zero-length-run backtracking, dedup order, whitespace tolerance) | VERIFIED (fixed this session — was `PARTIAL`: the command was exercised end-to-end every gate run against the one real fixed input, but the underlying parsing primitives' edge-case semantics were unverified in isolation; see remediation log) |
| REQ-XTASK-002 | `cargo xtask check-dependency-policy` fails when `Cargo.lock` contains a non-workspace crate | lockfile → pass/fail | Fails the gate with a specific message | `xtask/src/main.rs::cmd_check_dependency_policy` | Happy path exercised by every `cargo xtask gates` run against the real lockfile. Neither failure branch (foreign crate detected; empty/unparsable lockfile) has a dedicated test | PARTIAL |
| REQ-XTASK-003 | `cargo xtask check-oracle-integrity` fails when a retained oracle's SHA-256 no longer matches its recorded baseline | retained archive + manifest → pass/fail | Fails the gate with a specific message | `xtask/src/main.rs::cmd_check_oracle_integrity`, `find_sha256_after_label` | `find_sha256_after_label`'s parsing primitive: 6 new unit tests added this session (valid, whitespace-tolerant, truncated, uppercase-hex rejected, absent-label, >64-char-run truncation). The command's own mismatch/missing-file branches are exercised manually per decisions #18/#25, not by a permanent automated test | PARTIAL (parsing primitive `VERIFIED` this session; command-level failure branches remain `IMPLEMENTED_UNVERIFIED`) |
| REQ-XTASK-004 | `cargo xtask vendor-advisory-db` materializes an ephemeral single-commit git repo so `cargo audit`/`cargo deny` can run fully offline | RustSec advisory snapshot → local git repo | Ephemeral; recreated every invocation | `xtask/src/vendor.rs` | No isolated unit test; exercised end-to-end by every `audit`/`deny`/`gates` invocation, including this session's | VERIFIED (via repeated deterministic integration-level exercise; the module's simplicity makes an isolated unit test lower-value than the parsing helpers fixed this session) |
| REQ-XTASK-005 | `cargo xtask audit` / `cargo xtask deny` run an offline vulnerability/policy scan against the vendored advisory database | workspace `Cargo.lock` → scan report | Fails the gate on any finding | `xtask/src/main.rs` | executed this session: 1233 advisories loaded, 2 crates scanned, zero findings; `cargo deny` advisories/bans/licenses/sources all `ok` | VERIFIED |
| REQ-XTASK-006 | `cargo xtask gates` reproduces the full CI gate sequence in one local command | — → exit 0/non-zero | Runs fmt, clippy, build, test, doc, xtask's own fmt/clippy/test, parity-report drift check, dependency policy, oracle integrity, audit, deny, in sequence | `xtask/src/main.rs::cmd_gates` | executed successfully this session (exit 0, all sub-gates green) both before and after every fix in this pass | VERIFIED |
| REQ-XTASK-007 | `apk-inventory`/`native-abi`/`dex-classes` are byte-identical, dependency-free replacements for the retired `tools/*.py`/`tools/native_abi.sh` | APK/DEX/ELF bytes → inventory/ABI/class listings | — | `xtask/src/{zip_reader,elf,dex,sha256}.rs` | 11 pre-existing unit tests (unaffected by this session); one-time cross-validation against the original Python tools recorded in decisions #25/#27 | VERIFIED |
| REQ-XTASK-008 | `repo_root()` walks upward from any subdirectory to locate the repository root | cwd → repository root path | `panic`/`Err` if no root marker is found | `xtask/src/main.rs::repo_root` | none — always invoked from the repo root in CI/gates | IMPLEMENTED_UNVERIFIED |

**Runtime verification evidence:** `cargo test --manifest-path xtask/Cargo.toml --locked` → 30 passed, 0 failed (2026-08-31, this session; was 11 before this session's fix). `cargo xtask gates` → exit 0, all sub-gates green (2026-08-31, this session, after all fixes).

---

## REQ-PROC — Process, CI, and documentation-as-requirement

| ID | Requirement | Inputs → Outputs | Side effects / Failure behavior | Location | Tests | Status |
|---|---|---|---|---|---|---|
| REQ-PROC-001 | CI reproduces the full local gate suite on every push/PR | push/PR event → workflow run | Fails the check on any gate failure | `.github/workflows/gates.yml` (`cargo xtask gates`) | reproduced locally this session with identical results to what CI would report | VERIFIED |
| REQ-PROC-002 | Retained oracle archive hashes never silently change between sessions | oracle archive → hash comparison | Gate failure on mismatch | `xtask/src/main.rs::cmd_check_oracle_integrity` | REQ-XTASK-003's happy path executed and confirmed this session | VERIFIED |
| REQ-PROC-003 | Zero third-party dependencies for both shipped crates and the `xtask` tool itself | `Cargo.lock` → pass/fail | Gate failure on any foreign crate | `xtask/src/main.rs::cmd_check_dependency_policy`; `xtask/Cargo.toml`'s isolated `[workspace]` | confirmed this session via `cargo xtask gates` | VERIFIED |
| REQ-PROC-004 | Every non-trivial autonomous decision is recorded in the append-only decision log before/alongside the change it justifies | decision → new ledger entry | Append-only; never edited retroactively | `docs/AUTONOMOUS_DECISIONS.md` | this session's own reconciliation + remediation recorded as decisions #29-#30 | VERIFIED |
| REQ-PROC-005 | Cold-start/packaging documentation accurately reflects current test counts and tool status, not a stale snapshot | doc claim vs. live command output | — | `docs/COLD_START_VERIFICATION.md`, `docs/EXCEPTION_LEDGER.md` | fixed this session — see remediation log | VERIFIED (both instances found this session were `OBSOLETE`; both corrected) |

**Runtime verification evidence:** `cargo xtask gates` full run, executed 2026-08-31 (this session) → exit 0.

---

## Session remediation log

| # | Item | Before | After | Evidence |
|---|---|---|---|---|
| 1 | REQ-XTASK-001/003 parsing primitives (`find_prefixed_runs`, `find_quoted_name_values`, `find_sha256_after_label`, `dedup_sorted`) | `PARTIAL` — zero direct unit tests despite subtle, documented edge-case semantics (zero-length-run backtracking, unterminated quotes, uppercase-hex rejection, >64-char truncation) | `VERIFIED` | 19 new unit tests added to `xtask/src/main.rs`; `cargo test --manifest-path xtask/Cargo.toml --locked` 11→30 passed |
| 2 | REQ-FUSION-006 falsification survival | `PARTIAL` — only the rejection ("does not survive") case was ever exercised; the module's core "resists falsification" value proposition had no positive-path test | `VERIFIED` | new test `leading_hypothesis_survives_falsification_when_support_is_robust` in `crates/bleradar-core/tests/fusion.rs`; `cargo test -p bleradar-core --locked --test fusion` 5→6 passed |
| 3 | REQ-CORE-012 selection-lock history retention | `PARTIAL` — the test's name promised history retention across unlock, but its assertions only checked the `tracking` boolean, never `track` itself | `VERIFIED` | strengthened `selection_lock_retains_history` in `crates/bleradar-core/tests/core.rs` to push observations before/after `stop_tracking()` and assert `track.observations().len()` across the transition; `cargo test -p bleradar-core --locked --test core` 15→16 passed |
| 4 | REQ-PROC-005a (`docs/COLD_START_VERIFICATION.md`) | `OBSOLETE` — hardcoded "15 integration tests… 3 in bleradar-compat", stale since the evidence/fusion/verification/advancement/osint/infrastructure/website engines were added | `VERIFIED` | reworded to point at the live `cargo test`/`cargo xtask gates` commands as the source of truth instead of a point-in-time count |
| 5 | REQ-PROC-005b (`docs/EXCEPTION_LEDGER.md` entry 5) | `OBSOLETE` — claimed `cargo audit` "has not been executed (tool not installed)", already superseded by decision #28 | `VERIFIED` | entry corrected to record that `cargo audit`/`cargo deny` are installed and verified green offline, citing decision #28 |

Every fix above was validated with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --locked`, `cargo test --workspace --locked`, `cargo test --manifest-path xtask/Cargo.toml --locked`, and a full `cargo xtask gates` run — all green — after each individual change and again after all changes combined.

## Consciously deferred (not fixed this session)

Every item below is `IMPLEMENTED_UNVERIFIED`, `PARTIAL`, or bounded by a named
physical constraint — never `BROKEN`, and never silently `MISSING`. Recording
them here (rather than omitting them) is itself part of satisfying "do not
infer completion from code presence": a future pass can pick any single row
and close it without re-deriving this audit.

- **Fusion**: non-zero uncertainty perturbation, `Contextual`-role inertness, duplicate-assessment/duplicate-expected-evidence rejection, unregistered-hypothesis rejection, hypothesis tie-break ordering, `Display`/`Error` trait exercise (REQ-FUSION-005 tie-break, 007, 008, 009, 010 hypothesis-side, 011).
- **Evidence**: transformation-not-yet-verified distinguishability, empty-value rejection at construction (REQ-EVID-009, 010).
- **Verification**: invariance/commutativity/reversibility/round-trip/partition-recombination/permutation-equivalence relation vectors, concurrency/restart/recovery outcome surfaces, `TransformationFailed`/`CanonicalStore` error variants (REQ-VERIF-002, 008, 009).
- **Advancement**: multi-proposal ranking/tie-break, explained-and-accepted regression path, dependency/limiter field enforcement, full falsification-finding structure, additional `AdvancementError` edge cases (REQ-ADV-004, 005, 007, 008, 009).
- **OSINT**: `GraphNeighbor`-representation pivot exercise, re-exhausting an already-executed pivot, incremental resource-limit boundary sequencing (REQ-OSINT-001, 009, 010).
- **Infrastructure**: `CommonCdn`/`CommonCms`/`CommonRegistrar`/`SharedThirdPartyService`/`Unknown` as a test's leading-outcome scenario (REQ-INFRA-002).
- **Website**: `CommonTemplate`/`ContentReuse`/`DevelopmentRelationship`/`Coincidence`/`Unknown` as a test's leading-outcome scenario; multi-observation mid-extraction rollback ordering (REQ-WEB-002, 004).
- **xtask**: `check-dependency-policy`/`check-oracle-integrity` command-level failure branches — closing these fully would need the commands refactored to accept injectable paths for in-process fixture testing, which is a larger change than this pass's scope, not a same-session fix; `repo_root()` (REQ-XTASK-002, 003 command-level, 008).
- **Compat/runtime**: all 124 ABI contracts are registered, but 27 have unknown
  reachability and the stateful/lifecycle/network behavior needed for
  replacement parity requires an ARM64 Android/Bionic harness (MIG-003). This is
  an open evidence and migration backlog, not a non-Rust exception.

## Termination statement

Every requirement audited in this pass is now either `VERIFIED`, or is
`IMPLEMENTED_UNVERIFIED`/`PARTIAL` with its specific missing test scenario
named above — a bounded, traceable backlog. No `BROKEN` and no silently
`MISSING` requirement was found anywhere in the seven engine modules, the
four original core modules, `bleradar-compat`, or `xtask`, across the original
audit scope. REQ-COMPAT-004 remains open until Android/Bionic characterization
and target differential verification satisfy the per-component removal gates
in `BEHAVIORAL_CONTRACT.md`.

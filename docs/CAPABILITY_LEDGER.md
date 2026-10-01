# ATT&CK Capability Ledger (v0)

Defensive **coverage claims** ledger for Huntsman's Radar. This is **not** an
attack toolkit and does not implement offensive ATT&CK techniques.

## Mandate

Status for each technique is derived from an evidence chain:

`TECHNIQUE → RUST COMPONENT → SOURCE → INPUT → EXECUTION → OUTPUT →
PROVENANCE → INDEPENDENT CORROBORATION → TEST → (optional BENCHMARK) →
REGRESSION → VERIFIED STATUS`

**No sufficient evidence = no Verified capability.**

`CapabilityStatus::Verified` cannot be set from static seed data. There is no
manual Navigator promotion API. Status is always computed by `derive_status`.

Do **not** map `docs/REQUIREMENTS_LEDGER.md` `VERIFIED` rows or
`ParityStatus` into ATT&CK `Verified`.

## Statuses

| Status | Meaning |
|---|---|
| Verified | Full mandatory evidence links present and healthy |
| Partial | Some but not all mandatory links |
| Unverified | In-scope claim without enough evidence, or hard failure (failed test / broken regression) |
| NotApplicable | Explicitly out of product scope (omission ≠ N/A) |

## Seed v0

`CapabilityLedger::seed_v0()` registers a small curated set of enterprise
technique IDs as **claim placeholders** with **empty** links:

- InScope rows start **Unverified** (`verified_count() == 0`)
- NotApplicable stubs stay N/A even if links are later attached

Extend the seed only with defensive claim placeholders and corresponding Rust
component paths. Adding a technique never ships attack steps.

## Navigator

`CapabilityLedger::navigator_layer(name, domain, version)` emits ATT&CK
Navigator layer 4.x compatible JSON (std-only, deterministic). Colors/scores
come only from derived status:

| Status | Score | Color |
|---|---:|---|
| Verified | 100 | `#31a354` |
| Partial | 50 | `#fec44f` |
| Unverified | 10 | `#de2d26` |
| NotApplicable | 0 | `#bdbdbd` |

## How to extend

1. Add a `CapabilityClaimSpec` to `seed_v0` (or `insert_row` at runtime).
2. Attach evidence via `set_links` when a real defensive evidence chain exists.
3. Use `invalidate_test` / failed links to demonstrate auto-downgrade.
4. Re-export Navigator JSON from the ledger — never hand-edit scores/colors.

Until a full evidence chain exists for a technique, **Verified count stays 0**.

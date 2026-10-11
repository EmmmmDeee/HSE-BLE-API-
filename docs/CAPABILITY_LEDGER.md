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

## Derived evidence health (no manual flags)

`CapabilityEvidenceLinks` has no stored health flags. Corroboration and
regression health are **derived from the linked records** and cannot be set
by hand (a struct literal naming either flag does not compile):

| Derived check | True when |
|---|---|
| `corroboration_ok()` | at least one `corroboration_ids` entry is linked |
| `regression_ok()` | at least one `regression_lock_ids` entry is linked **and** no `failed_test_ids` entry is linked |

**Blank ids are missing.** An id that is empty or only whitespace never
satisfies a link: `mandatory_complete`, `any_mandatory_partial`,
`corroboration_ok`, `regression_ok` and `derive_status` all ignore it, and
`set_links` / `insert_row` / `invalidate_test` trim ids and drop blank ones
before storing. A blank `failed_test_ids` entry is not a failure.

The ledger is not serialized: there is no stored flag to load, and the
Navigator export carries only the derived status.

Removing the two public `corroboration_ok` / `regression_ok` fields is a
source-breaking change (any struct literal or field access naming them stops
compiling), so `bleradar-core` went from 0.6.10 to 0.7.0: for a 0.x crate a
breaking change bumps the minor version (`docs/PACKAGING_ASSISTANT.md`,
`docs/AUTONOMOUS_DECISIONS.md` decision 117).

## Statuses

| Status | Meaning |
|---|---|
| Verified | Full mandatory evidence links present and healthy, including at least one regression lock (`regression_lock_ids`) |
| Partial | Some but not all mandatory links |
| Unverified | In-scope claim without enough evidence, or hard failure (a linked failed test, which breaks the regression) |
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
2. Attach evidence via `set_links` when a real defensive evidence chain exists
   (record ids only; health is derived from them).
3. Use `invalidate_test` / failed links to demonstrate auto-downgrade.
4. Re-export Navigator JSON from the ledger — never hand-edit scores/colors.

## Status command

`cargo xtask capability` prints each technique's derived status, its derived
`corroboration_ok` / `regression_ok`, the counts per status and the ledger's
`verified_count()`. It is read-only: it borrows the ledger, writes only to
stdout and creates or changes no file.

Until a full evidence chain exists for a technique, **Verified count stays 0**.

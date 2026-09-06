# Huntsman Search Engine (HSE) import

This workspace imports the most radar-applicable functionality from the
Huntsman Search Engine repository
(`EmmmmDeee/Huntsman-Search-Engine-HSE-Termux-Android-Aarch64-Rust-`),
specifically its `hse-core` crate (the engine's self-contained entity model,
extracted there so it can be shared unmodified across HSE builds).

## What was ported, and where it lives

| HSE source (`hse-core/src/…`) | Here (`crates/bleradar-core/src/…`) | Contents |
| --- | --- | --- |
| `coords.rs` + `coords/` | `coords.rs` | Universal coordinate parser: decimal pairs, DMS/DDM with every glyph and hemisphere placement, RFC 5870 `geo:` URIs, full Open Location Codes (Plus Codes), and 4/6/8-character Maidenhead locators, all funnelling through one validity gate. |
| `tags.rs` | `tags.rs` | Canonical entity tag vocabulary (provenance, geolocation, device/local, reputation/threat, sanctions, identity quarantine, lifecycle gates). |
| `lib.rs` (entity model) | `entity.rs` | SHA-256 deterministic UIDs, per-kind normalisation (email, username incl. the KSUID case-preservation shape, domain, phone, IP, MAC, coordinates, URL tracking-param stripping), the `C_eff` corroboration model with classification tiers, GREATEST-semantics merge with the candidate/derived tag lifecycle, evidence accumulation, and scan/evidence helpers. |
| `tests.rs` | `../tests/entity.rs` | The entity/coords test suite, adapted (see below). |

## Port adaptations (all deliberate, behaviour-preserving for explicit inputs)

- **Zero third-party dependencies.** HSE's `hse-core` depends on
  `serde`/`sha2`/`hex`; this workspace is intentionally third-party-free
  (`AUTONOMOUS_DECISIONS.md`, decision 9), so the port uses an in-crate
  FIPS 180-4 SHA-256 plus hex encoder (`entity::Sha256`, `entity::hex_encode`,
  verified against published vectors) and drops serde serialization
  (presentation/storage glue, not engine semantics).
- **Explicit timestamps.** `unix_now()` is not ported; `recorded_at` /
  `observed_at` are constructor parameters and gamma decay is computed
  against a caller-supplied `now` (`decayed_confidence_at` / `apply_decay_at`),
  so every value is deterministic and reproducible.
- **Renamed types.** `Entity`/`Evidence`/`EntityKind`/`Classification`/
  `VerificationMethod`/`EntityBuilder`/`EntityRef` are exported as
  `HseEntity`/`HseEvidence`/`HseEntityKind`/`HseClassification`/
  `HseVerificationMethod`/`HseEntityBuilder`/`HseEntityRef`, because the
  canonical evidence engine (`evidence.rs`) already owns the unprefixed names.
- **One geo type.** Parsed coordinates resolve to the existing validated
  `geo::LatLon` (via `coords::ParsedCoordinate`) rather than introducing a
  second coordinate type.
- **Tests.** Serde round-trip tests are dropped; the proptest property block
  is replaced by deterministic grid/property sweeps so the suite stays
  dependency-free, matching this workspace's `properties.rs` precedent.

## What was deliberately not imported

HSE's Tokio/Axum server, database storage layer, CLI, WASM UI, and
network-bound OSINT modules are server/application concerns outside this
crate's dependency-free radar-logic boundary, and would each violate the
zero-dependency gate (`cargo xtask check-dependency-policy`). The entity model
above is the complete dependency-free core of the search engine and the only
part applicable here.

The `HseEntity` confidence/corroboration model complements — and does not
replace — the crate's canonical `EvidenceStore`: `HseEntity` is a lightweight
working-graph finding, while `EvidenceStore` remains the authoritative
provenance record.

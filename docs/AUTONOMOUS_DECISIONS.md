# Autonomous Decision Log

## Recovery points

- `recovery/apk-oracle` — original binary/oracle preservation point.
- `recovery/rust-core` — first reconstructed Rust core.
- `migration/binary-grounded-v0.3.0` — initial packaged migration state.
- `recovery/critical-enhancement` — critically enhanced tracking/parity architecture.

## Critical enhancement decisions

1. **Do not fabricate Android source parity.** R8-obfuscated DEX is retained as oracle rather than decompiled into guessed source semantics.
2. **Separate observation from inference.** Map records carry `Observed`, `Inferred`, or `Predicted` state so estimates cannot masquerade as measurements.
3. **Prefer coarse proximity over fake precision.** Exact RSSI ranging is represented only as a calibrated estimate; coarse bands remain usable without calibration.
4. **Reject randomized MAC as stable identity evidence.** The U/L bit is interpreted conservatively and supplemental evidence is modeled separately.
5. **Enforce monotonic observation time.** A track cannot silently accept backwards timestamps.
6. **Represent GNSS accuracy explicitly.** Positioned observations retain horizontal uncertainty and confidence.
7. **Use a conservative weighted region estimate.** The implementation estimates strongest observed region rather than claiming transmitter multilateration parity.
8. **Mechanize parity status.** Observed ABI, oracle-retained implementation and source-reconstructed behavior are distinct registry states.
9. **Preserve zero-dependency Rust core.** No external crate was introduced solely for convenience, keeping the workspace deterministic and audit-light.
10. **Never report unavailable Cargo/Android gates as green.** Package integrity is verified locally; compile/runtime gates remain explicit blocked items.

## Autonomous advancement decisions (2026-08-28)

11. **Execute formerly blocked gates the moment a capable host exists.** All four cargo gates plus the parity drift check were run and observed green, converting MIG-004 from blocked to remediated with dated evidence.
12. **Falsify before and after fixing.** The haversine NaN (COR-010) and bearing `360.0` (COR-011) defects were demonstrated with concrete reproducers, pinned as failing regression tests first, and only then fixed — clamping to the mathematical domain rather than masking symptoms.
13. **Enforce claimed invariants at type boundaries.** `LatLon` validation moved from convention to compile-time enforcement by privatizing fields (COR-012); a claim of validation that can be bypassed is not a remediation.
14. **Lock verification into CI.** Green-once is not green; `.github/workflows/gates.yml` re-proves every gate and the parity report's determinism on each push and pull request.
15. **Operate future autonomous sessions under `docs/AUTONOMOUS_ENGINE.md`.** The engine document is versioned in-repo so its acceptance gate and recomputation obligations are auditable alongside the work they govern.

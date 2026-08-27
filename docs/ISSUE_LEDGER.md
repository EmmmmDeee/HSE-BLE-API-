# Issue Ledger

| ID | Severity | Evidence | Root cause / assessment | Remediation / disposition |
|---|---|---|---|---|
| MIG-001 | Critical | Only R8-obfuscated DEX and stripped native library supplied | original source semantics are not fully recoverable from binary | **Exception/open**: strict source-level whole-app parity cannot be proven without source or exhaustive black-box Android characterization |
| MIG-002 | Critical | Original APK is signed; signing private key not supplied | rebuilt Android package cannot retain update identity | **Exception/open**: do not forge or replace signing identity |
| MIG-003 | High | Android D8/AAPT2/apksigner and emulator/device absent | no rebuilt APK or Android cold-start can be verified here | **Exception/open** |
| MIG-004 | High | Rust compiler/cargo absent from execution host | requested cargo gates cannot be executed locally | **Exception/open**; workspace is dependency-free and pinned for external deterministic verification |
| MIG-005 | High | Native library exports >100 contracts, private implementation stripped | exact error/serialization/edge behavior unknown for many interfaces | **Exception/open**; complete ABI retained as oracle inventory; no speculative implementations |
| SEC-001 | Medium | Original Android dependency metadata exists but no original Gradle/Cargo lockfiles | full transitive advisory resolution cannot be reproduced from APK metadata alone | **Exception/open** for legacy binary; reconstructed workspace has zero third-party Rust dependencies |
| DEBT-001 | Low | string census for TODO/FIXME/HACK/XXX is contaminated by bundled library/debug text | binary strings do not prove project debt markers | **Intended/insufficient evidence**; no debt marker added to reconstructed Rust source |
| COR-001 | Medium | MAC privacy/randomization behavior visible in native strings/API | tracking stable identity by randomized MAC would be incorrect | **Remediated in reconstructed pure core** with locally-administered-bit classifier and regression test |
| COR-002 | Medium | map/location functions exposed and prior product requirement requires defensible geometry | geographic calculations need deterministic validated inputs | **Remediated in reconstructed pure core** with validated `LatLon`, haversine and bearing tests |
| COR-003 | Medium | live RSSI is noisy; product requires hot/cold guidance | raw sample comparison is unstable | **Remediated as new isolated Rust utility** with deterministic EMA + trend tests; not claimed as legacy parity |

No issue is marked as a legacy bug fix unless there is reproducible evidence. Entries that cannot be reproduced from the supplied binary are left explicit rather than falsely closed.

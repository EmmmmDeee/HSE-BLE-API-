# Issue Ledger

| ID | Severity | Evidence | Root cause / assessment | Remediation / disposition |
|---|---|---|---|---|
| MIG-001 | Critical | Only R8-obfuscated DEX and stripped native library supplied | original source semantics are not fully recoverable from binary | **Exception/open**: strict whole-app source parity cannot be proven without exhaustive differential Android characterization |
| MIG-002 | Critical | Original APK is signed; signing private key not supplied | rebuilt Android package cannot retain update identity | **Exception/open**: do not forge or replace signing identity |
| MIG-003 | High | Android D8/AAPT2/apksigner and emulator/device absent | no rebuilt APK or Android cold-start can be verified here | **Exception/open** |
| MIG-004 | High | Rust compiler/Cargo absent from execution host | requested cargo gates cannot be executed locally | **Exception/open**; exact commands documented and workspace remains dependency-free |
| MIG-005 | High | Native library exports a broad UniFFI surface, private implementation stripped | exact errors/serialization/edge behavior unknown for many interfaces | **Improved/open**: semantic parity registry + generated coverage report now expose the migration frontier |
| SEC-001 | Medium | Original Android dependency metadata exists but no original Gradle/Cargo lockfiles | complete legacy transitive advisory state cannot be reproduced from APK alone | **Exception/open**; reconstructed Rust workspace adds zero third-party crates |
| DEBT-001 | Low | binary TODO/FIXME/HACK/XXX strings are contaminated by bundled libraries/debug data | binary strings do not prove project debt markers | **Intended/insufficient evidence**; reconstructed Rust contains no unresolved debt markers |
| COR-001 | Medium | privacy/randomization behavior visible in native API | stable identity by randomized MAC is incorrect | **Remediated**: canonical MAC + locally-administered-bit classifier + identity test |
| COR-002 | Medium | map/location contracts exposed | geographic calculations require deterministic validated inputs | **Remediated**: validated `LatLon`, haversine and bearing tests |
| COR-003 | Medium | raw RSSI is noisy | raw comparison creates unstable hot/cold guidance | **Remediated**: deterministic EMA + deadband trend tests |
| COR-004 | High | prior reconstruction treated map coordinates as point facts | GNSS position has explicit horizontal uncertainty | **Remediated**: map observations now carry uncertainty and confidence |
| COR-005 | High | selected-device workflow required but absent from reconstructed source | no persistent UI-facing target state | **Remediated**: `SelectedDevice` lock/unlock state retains track history |
| COR-006 | High | path histories can be corrupted by timestamp reversal | no temporal invariant in first-pass model | **Remediated**: monotonic-time gate + regression test |
| COR-007 | Medium | precise BLE range claims would outrun RSSI evidence | RSSI-to-distance depends on calibration/environment | **Remediated**: calibrated estimator returns only estimate; coarse proximity remains first-class |
| COR-008 | Medium | original compatibility crate flattened all symbols into “observed” | observed ABI could be mistaken for completed migration | **Remediated**: `Reconstructed / OracleOnly / Blocked` semantic registry and tests |
| COR-009 | Medium | spatial inference was absent | map could not synthesize repeated positioned observations | **Remediated as enhancement**: conservative GPS/signal weighted region estimator with support count/confidence |

No legacy defect is marked fixed without reproducible evidence. Enhancement entries are explicitly labeled and are not represented as exact legacy behavior.

# Exception Ledger

Only hard external boundaries qualify here. Missing source, effort, missing
fixtures, and unknown behavior are migration risks, not permission to retain
core logic outside Rust.

| ID | Non-Rust remainder | Precise external blocker | Thinnest permitted boundary |
|---|---|---|---|
| EXT-001 | Android `Application`, `Activity`, `Service`, receiver/listener classes | Android's manifest, class loader, Binder/lifecycle dispatcher, and public SDK instantiate and invoke JVM classes/interfaces | Component/callback methods copy fields into generated FFI events and execute Rust-returned platform commands; no policy, scheduling, validation, or state |
| EXT-002 | Android radio, location, GATT, permission, intent, content-URI, notification, and wake-lock calls | These capabilities expose framework-owned Java objects, callbacks, tokens, and thread-affine APIs | Adapter retains opaque handles and executes typed operations selected by Rust; all normalization, retry, resource, and success policy remains Rust |
| EXT-003 | Compose/view rendering, accessibility, navigation, and Android resources | The shipped Android rendering/runtime and generated `R` resources execute as JVM/platform APIs | Render immutable Rust view models and relay user events; filtering, sorting, labels, action eligibility, and error state remain Rust |
| EXT-004 | Manifest/resources/Gradle/package/signing declarations | Android packaging consumes declarative XML/resources, build-system syntax, and cryptographic signing material rather than Rust application code | Metadata/build glue only; no application behavior |
| EXT-005 | Generated UniFFI/JNI/JNA platform binding code | Cross-language ABI marshalling and JVM-visible wrapper classes are required by the Android toolchain/calling convention | Generated wrappers only, with version/ownership/error tests; no handwritten domain decisions |
| EXT-006 | Original update signing identity | The original private signing key is absent; Android rejects an update signed by another identity | Never modify or re-sign the oracle; a future package must explicitly use a new distribution identity |
| EXT-007 | Complete legacy dependency audit | An APK does not preserve the original Gradle lockfiles and all source dependency declarations | Report only evidenced metadata; audit every dependency in a future reproducible source build |

The absent original source and stripped/private native implementation are
evidentiary limitations, not runtime exceptions. Exact lifecycle, callback,
record-layout, state, persistence, and network semantics remain
`UNKNOWN`/blocked until characterized; the target ownership remains Rust.

The former Cargo-host exception is closed: the pinned Rust toolchain and
offline `cargo audit`/`cargo deny` gates are available and CI-enforced. Android
SDK build tools are available in the current sandbox, but no original Android
source/Gradle project, emulator, physical device, or original signing key is
present.

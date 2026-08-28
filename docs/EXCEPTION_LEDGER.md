# Exception Ledger

1. **Android UI migration** — original Compose/Kotlin source is absent and R8-obfuscated. Recreating it from names/strings would be a new implementation, not strict parity.
2. **Android BLE service migration** — callback timing, permission behavior and lifecycle side effects require characterization on Android hardware/emulator.
3. **Exact UniFFI record layouts/private logic** — public record names are observable, but the stripped Rust source and all original type definitions are not.
4. **Signing identity** — no private signing key is present. Any modified APK signed with another key is a different application identity for update purposes.
5. **Cargo cold-start gate on this host** — no Rust toolchain is installed and outbound DNS from the execution container is unavailable.
6. **Legacy dependency audit** — APK metadata records some AndroidX versions but does not contain complete source package-manager lock data needed to reproduce every transitive dependency/advisory state.

These are physical/evidentiary constraints, not deferred claims of completion.

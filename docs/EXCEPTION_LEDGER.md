# Exception Ledger

1. **Android UI migration** — original Compose/Kotlin source is absent and R8-obfuscated. Recreating it from names/strings would be a new implementation, not strict parity.
2. **Android BLE service migration** — callback timing, permission behavior and lifecycle side effects require characterization on Android hardware/emulator.
3. **Exact UniFFI record layouts/private logic** — public record names are observable, but the stripped Rust source and all original type definitions are not.
4. **Signing identity** — no private signing key is present. Any modified APK signed with another key is a different application identity for update purposes.
5. **Cargo cold-start gate** — *superseded 2026-08-28*: a Rust-capable execution host became available; all four cargo gates were executed green with the pinned 1.98.0 toolchain and are now CI-enforced (`.github/workflows/gates.yml`). *Fully closed 2026-08-31*: `cargo audit`/`cargo deny` were installed and, per decision 28, converted to a fully offline posture against a vendored copy of the RustSec advisory database (`vendor/rustsec-advisory-db/`); both run green in CI via `cargo xtask audit`/`cargo xtask deny` (part of `cargo xtask gates`) against the zero-third-party-crate lockfile. No remaining gap in this area.
6. **Legacy dependency audit** — APK metadata records some AndroidX versions but does not contain complete source package-manager lock data needed to reproduce every transitive dependency/advisory state.

These are physical/evidentiary constraints, not deferred claims of completion.

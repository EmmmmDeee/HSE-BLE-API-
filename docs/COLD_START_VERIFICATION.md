# Cold-Start Verification

The deliverable is verified in two layers.

## Layer 1 — package integrity (executed here)

1. Create the final ZIP.
2. Extract it to a fresh directory.
3. Verify every `SHA256SUMS` entry.
4. Run `cargo xtask parity-report` from the extracted copy.
5. Confirm the Git history bundle lists the recovery tags and enhancement commit.

## Layer 2 — Rust execution gates (executed 2026-08-28)

Run from the clean extraction:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo xtask gates
```

Observed on Linux x86_64 with the pinned toolchain (rustc/cargo 1.98.0, per
`rust-toolchain.toml`): the four cargo gates exit 0. Test counts grow with the
workspace and are intentionally not re-asserted as a fixed number here (that
would itself drift silently); the authoritative count on any given commit is
whatever `cargo test --workspace --locked` and
`cargo test --manifest-path xtask/Cargo.toml --locked` report, re-proven on
every push and pull request by `.github/workflows/gates.yml`'s
`cargo xtask gates`, which runs both plus the parity-report drift check.

`cargo audit` and `cargo deny` now run fully offline against the vendored
RustSec advisory database (`vendor/rustsec-advisory-db/`, materialized into a
throwaway git commit by `cargo xtask vendor-advisory-db`) and are green: the
workspace's `Cargo.lock` contains zero third-party crates, so the advisory
surface is empty, and both tools confirm it rather than leaving the command
unrun. See `docs/AUTONOMOUS_DECISIONS.md` #28.

Android SDK packaging tools are available in the current environment, but the
original Android source/Gradle project, ARM64 emulator or device, and signing
key are not. Android/Bionic characterization therefore remains open under
MIG-003. This document continues to distinguish archive reproducibility from
compiler/runtime verification.

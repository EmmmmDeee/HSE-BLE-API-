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
`rust-toolchain.toml`): the four cargo gates exit 0, with 15 integration tests
passing in `bleradar-core` and 3 in `bleradar-compat`. The same gates plus a
parity-report drift check now run continuously in CI
(`.github/workflows/gates.yml`), so Layer 2 is re-proven on every push and
pull request rather than asserted once.

`cargo audit` and `cargo deny` now run fully offline against the vendored
RustSec advisory database (`vendor/rustsec-advisory-db/`, materialized into a
throwaway git commit by `cargo xtask vendor-advisory-db`) and are green: the
workspace's `Cargo.lock` contains zero third-party crates, so the advisory
surface is empty, and both tools confirm it rather than leaving the command
unrun. See `docs/AUTONOMOUS_DECISIONS.md` #28.

Android-target execution (device/emulator characterization) remains outside
this environment; see MIG-003. This document continues to distinguish archive
reproducibility from compiler/runtime verification.

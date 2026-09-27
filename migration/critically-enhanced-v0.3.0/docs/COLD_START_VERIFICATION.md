# Cold-Start Verification

The deliverable is verified in two layers.

## Layer 1 — package integrity (executed here)

1. Create the final ZIP.
2. Extract it to a fresh directory.
3. Verify every `SHA256SUMS` entry.
4. Run `python tools/parity_report.py` from the extracted copy.
5. Confirm the Git history bundle lists the recovery tags and enhancement commit.

## Layer 2 — Rust execution gates (required, unavailable on this host)

Run from the clean extraction:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
cargo audit
```

The execution host used to prepare this package does not have `cargo`/`rustc`; therefore Layer 2 is **BLOCKED**, not passed. This document deliberately distinguishes archive reproducibility from compiler/runtime verification.

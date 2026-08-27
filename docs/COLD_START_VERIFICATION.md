# Cold-start Verification

## Requested gate

The requested terminal gate is: extract the final ZIP into a fresh directory, then run `cargo build`, the full test suite, and `cargo audit` from that extracted copy alone.

## Host result

- Archive extraction: **PASS**
- Required source/manifests/oracles/docs present: **PASS**
- Python inventory tool syntax: **PASS**
- Shell ABI tool syntax: **PASS**
- Git bundle structural verification: **PASS**
- `cargo build --workspace --locked`: **BLOCKED — Cargo/Rust toolchain is not installed on the execution host**
- `cargo test --workspace --locked`: **BLOCKED — same physical host constraint**
- `cargo audit`: **BLOCKED — Cargo and cargo-audit are not installed**

Outbound DNS from the execution container is unavailable, so the pinned Rust toolchain cannot be bootstrapped into this host during the run. This is recorded as MIG-004 and is not represented as green.

The reconstructed workspace has zero third-party Rust dependencies. That materially reduces advisory exposure but does not substitute for executing the requested commands.

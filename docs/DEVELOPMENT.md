# Development setup

Concrete host setup for building and testing this workspace. Read this before
debugging an MSRV failure.

## Rust toolchain (required)

MSRV is **1.98**. It is pinned in two places:

- `rust-toolchain.toml` — channel `1.98.0`, profile `minimal`, components
  `clippy` and `rustfmt`
- root `Cargo.toml` — `rust-version = "1.98"`, edition `2024`

Do not lower either. Do not rely on a distro `rustc` package.

### rustup and PATH

Install [rustup](https://rustup.rs) if you do not already have it. Then put
Cargo's bin directory **first** on `PATH`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

Add that line to your shell profile so it survives new terminals.

Many Linux hosts also ship an older system toolchain (commonly
`/usr/bin/rustc` 1.85). If that wins on `PATH`, every `cargo` invocation fails
before compile with exit 101:

```text
error: rustc 1.85.0 is not supported by the following packages:
  bleradar-core@… requires rustc 1.98
```

That is a PATH / rustup problem, not a broken tree.

Inside this repo, `rust-toolchain.toml` makes rustup select 1.98.0
automatically once `~/.cargo/bin` is ahead of `/usr/bin`.

### Verify the active compiler

```sh
which rustc          # expect …/.cargo/bin/rustc (not /usr/bin/rustc)
rustc --version      # must show 1.98.x
cargo check --workspace
```

If `rustc --version` is below 1.98, fix PATH / install the pin before chasing
compile errors.

## Build and test

From the repository root, with the PATH above:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
```

Standard gates and the one-command runner (`cargo xtask gates`) are documented
in the README. Android / JNI live proofs need a JDK and SDK/NDK; see
`docs/ANDROID_APP.md`.

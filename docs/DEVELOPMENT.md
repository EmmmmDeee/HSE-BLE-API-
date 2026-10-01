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

## Full toolchain on a fresh Linux host (verified 2026-10-02)

Everything the CI `gates` and `android-apk` jobs need, set up once and
checked from fresh shells on a Debian 13 (trixie) x86_64 host. The versions
are the pins, not suggestions:

| Tool | Pin and where it comes from | Installed at |
|---|---|---|
| Rust | `1.98.0`, components `clippy` and `rustfmt` (`rust-toolchain.toml`) | rustup, `~/.rustup`, proxies in `~/.cargo/bin` |
| Rust target | `aarch64-linux-android` (CI's `android-apk` job; `xtask` installs it on demand too) | rustup |
| cargo-audit and cargo-deny | `0.22.2` and `0.20.2` (`CARGO_AUDIT_VERSION`/`CARGO_DENY_VERSION` in `.github/workflows/gates.yml`) | `~/.cargo/bin` |
| JDK | Temurin 21 (`actions/setup-java`, `distribution: temurin`, `java-version: '21'`); verified with `21.0.12.1+1` | `/opt/java/temurin-21` |
| Android platform | `platforms;android-36` (`PINNED_PLATFORM_API` in `xtask/src/main.rs`) | `$ANDROID_HOME/platforms/android-36` |
| Android build-tools | `build-tools;37.0.0` (`PINNED_BUILD_TOOLS_VERSION`) | `$ANDROID_HOME/build-tools/37.0.0` |
| Android NDK | `ndk;27.3.13750724` (`PINNED_NDK_VERSION`) | `$ANDROID_HOME/ndk/27.3.13750724` |
| Android cmdline-tools | not pinned by CI; `commandlinetools-linux-11076708` (cmdline-tools 12.0), as in `docs/COLD_START_VERIFICATION.md`. It carries `sdkmanager` and the `lint` that `verify-android-live` runs | `$ANDROID_HOME/cmdline-tools/latest` |
| Android platform-tools | latest (`adb`; the emulator proof installs it too); verified with `37.0.1` | `$ANDROID_HOME/platform-tools` |
| `zip` | Info-ZIP 3.0, which `cargo xtask build-apk` runs to assemble the package (the runner image ships it) | `apt install zip` |

`cargo xtask android-sdk-packages` prints the pinned SDK set. Read versions
from that command and from `rust-toolchain.toml`, not from this table.

### Install commands

```sh
# Rust: the pin as the default, so it also applies outside the repo.
rustup default 1.98.0
rustup component add clippy rustfmt --toolchain 1.98.0
rustup target add aarch64-linux-android --toolchain 1.98.0
cargo install --locked cargo-audit@0.22.2 cargo-deny@0.20.2

# Temurin 21 (CI's JDK distribution), checksum-verified.
curl -fsSL 'https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=x64&image_type=jdk&os=linux&vendor=eclipse'
#   gives the tarball link and its sha256; for 21.0.12.1+1:
curl -fsSLo temurin21.tar.gz 'https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.12.1%2B1/OpenJDK21U-jdk_x64_linux_hotspot_21.0.12.1_1.tar.gz'
echo 'ce79869e1307ed8ee1e2baa86a412b1eb5b75d10a01006d788a6f968bcfaee94  temurin21.tar.gz' | sha256sum -c
sudo mkdir -p /opt/java && sudo tar -xzf temurin21.tar.gz -C /opt/java
sudo ln -sfn /opt/java/jdk-21.0.12.1+1 /opt/java/temurin-21

# Android cmdline-tools into ~/Android/Sdk (sha256 2d2d5085…5e258).
curl -fsSLo cmdline-tools.zip https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
mkdir -p ~/Android/Sdk/cmdline-tools && unzip -q cmdline-tools.zip -d /tmp/cltx
mv /tmp/cltx/cmdline-tools ~/Android/Sdk/cmdline-tools/latest

# The pinned platform, build-tools and NDK, installed the way CI installs them.
# This accepts the licenses, retries, and checks that discovery finds them.
yes | sdkmanager --licenses >/dev/null
cargo xtask android-sdk-install
sdkmanager --install platform-tools

sudo apt-get install -y zip
```

### Environment (every shell)

The same idempotent block is in `/etc/profile.d/hse-toolchain.sh` (login
shells, including `env -i HOME=$HOME bash -lc`), at the end of `~/.profile`,
and at the top of `~/.bashrc`, ahead of its non-interactive `return`. Each
directory is removed from `PATH` and then prepended, so `~/.cargo/bin` always
ends up ahead of `/usr/bin` and its distro `rustc` 1.85.1, which stays
installed:

```sh
_hse_prepend() {
  [ -d "$1" ] || return 0
  case ":$PATH:" in *":$1:"*) PATH=$(printf '%s' ":$PATH:" | sed "s|:$1:|:|g; s|^:||; s|:\$||") ;; esac
  PATH="$1${PATH:+:$PATH}"
}
if [ -d /opt/java/temurin-21 ]; then export JAVA_HOME=/opt/java/temurin-21; fi
if [ -d "$HOME/Android/Sdk" ]; then
  export ANDROID_HOME="$HOME/Android/Sdk"
  export ANDROID_SDK_ROOT="$ANDROID_HOME"
  if [ -d "$ANDROID_HOME/ndk/27.3.13750724" ]; then
    export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.3.13750724"
    export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
  fi
  _hse_prepend "$ANDROID_HOME/platform-tools"
  _hse_prepend "$ANDROID_HOME/cmdline-tools/latest/bin"
fi
if [ -n "${JAVA_HOME:-}" ]; then _hse_prepend "$JAVA_HOME/bin"; fi
_hse_prepend "$HOME/.cargo/bin"
unset -f _hse_prepend
export PATH
```

`xtask` reads `ANDROID_HOME`, then `ANDROID_SDK_ROOT`, for the SDK. It reads
`ANDROID_NDK_HOME`, then `ANDROID_NDK_ROOT`, for the NDK, and otherwise
prefers `<sdk>/ndk/27.3.13750724`. For the JDK it reads `JAVA_HOME`, or
otherwise resolves the `javac` on `PATH` through its symlinks.

A non-login, non-interactive `bash -c` started by a tool reads none of
those files and keeps its parent's `PATH`, for example
`/usr/local/bin:/usr/bin:/bin`. These symlinks cover that case:

- `/usr/local/bin/{rustup,rustc,cargo,rustdoc,rustfmt,cargo-fmt,cargo-clippy,clippy-driver,rust-gdb,rust-gdbgui,rust-lldb}` → `~/.cargo/bin/…`
- `/usr/local/bin/{java,javac,jar,jarsigner,javadoc,javap,jshell,keytool,jlink,jdeps}` → `/opt/java/temurin-21/bin/…`
- `/usr/local/bin/{sdkmanager,avdmanager}` → `~/Android/Sdk/cmdline-tools/latest/bin/…`
- `/usr/local/bin/adb` → `~/Android/Sdk/platform-tools/adb`
- `/opt/android-sdk` → `~/Android/Sdk`, a fallback path `xtask` checks when `ANDROID_HOME` is unset

`/usr/local/bin` comes before `/usr/bin` in Debian's default `PATH`, so the
rustup proxy wins there as well.

### Verified

The following gave the same result under `bash -lc`, under `bash -c`, and
under `env -i HOME=$HOME bash -lc`, both in `/tmp` and in the repository:

```text
rustc 1.98.0 (88d9e12ae 2026-08-18)
cargo 1.98.0 (797e8a9bc 2026-08-05)
openjdk version "21.0.12.1" 2026-08-18 LTS   (Temurin-21.0.12.1+1)
sdkmanager --list_installed:
  build-tools;37.0.0   | 37.0.0
  ndk;27.3.13750724    | 27.3.13750724
  platform-tools       | 37.0.1
  platforms;android-36 | 2
```

Results on that host, run on PR #43's tree (`feat/capability-ledger-v0`):

- `cargo check`, `build` and `test --locked --workspace` passed, with 580 tests passed and 0 failed.
- `cargo xtask gates` passed. It ran 719 tests, and fmt, clippy, doc, audit and deny were all green.
- `cargo xtask verify-android-live` passed from a fresh clone once the committed APK had been rebuilt. It cross-compiled, ran `aapt2`, `javac`, `d8`, `zipalign` and `apksigner`, ran lint `NewApi` ("No issues found."), and checked the JNI contract (42 ↔ 42). The committed-APK reproduce check matched all 10 entries.
- `cargo xtask build-update-proof` passed.

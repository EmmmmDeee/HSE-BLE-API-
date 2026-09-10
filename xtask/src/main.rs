//! `cargo xtask` — dependency-free, Rust-native developer tooling for the
//! `bleradar-*` workspace.
//!
//! Replaces the `tools/*.py` and `tools/native_abi.sh` scripts with a single
//! binary that needs nothing beyond the pinned Rust toolchain: no Python,
//! no `readelf`, no `unzip`. `gates` is the "one command" local gate runner
//! (mirrors `.github/workflows/gates.yml` plus the offline `cargo audit`/
//! `cargo deny` checks CI does not yet run).
//!
//! Every subcommand is designed to be run from the repository root (exactly
//! how the `cargo xtask` alias in `.cargo/config.toml` and CI invoke it);
//! [`repo_root`] additionally walks upward from the current directory so
//! invoking it from a subdirectory still works.

mod dex;
mod elf;
mod sha256;
mod vendor;
mod zip_reader;

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Path (relative to the repo root) of the vendored, plain-file advisory
/// database `cargo audit` reads directly (no materialization needed: unlike
/// `cargo deny`, `cargo audit` tolerates a plain, non-git directory).
const AUDIT_DB_PATH: &str = "vendor/rustsec-advisory-db/advisory-db-3157b0e258782691";

/// The committed migration archive that retains the immutable native oracles.
const MIGRATION_ZIP_NAME: &str = "BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip";
/// Zip entry of the immutable v0.3.0 native oracle inside [`MIGRATION_ZIP_NAME`].
const ORACLE_SO_ZIP_ENTRY: &str = "oracle/libbleradar_core.so";
/// SHA-256 of that oracle `.so`, pinned so `oracle-differential` refuses to run
/// against anything but the recorded immutable binary.
const ORACLE_SO_SHA256: &str = "d14022cd113332312fb1719aafa107155a4c046c056cb9b2bcd3c94eb980b12d";
/// Committed executed-oracle ground truth `oracle-differential` regenerates and
/// drift-checks (relative to the repo root).
const EXECUTED_VECTORS_PATH: &str = "crates/bleradar-compat/tests/oracle/wifi_executed_vectors.tsv";
/// The differential harness source, compiled for aarch64 and run under qemu.
const ORACLE_HARNESS_C: &str = include_str!("oracle_harness.c");

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.collect();

    let result = match command.as_str() {
        "parity-report" => cmd_parity_report(),
        "check-dependency-policy" => cmd_check_dependency_policy(),
        "check-oracle-integrity" => cmd_check_oracle_integrity(),
        "apk-inventory" => cmd_apk_inventory(&rest),
        "native-abi" => cmd_native_abi(&rest),
        "dex-classes" => cmd_dex_classes(&rest),
        "vendor-advisory-db" => cmd_vendor_advisory_db(),
        "build-apk" => cmd_build_apk(),
        "check-jni-contract" => cmd_check_jni_contract(&rest),
        "verify-jni-live" => cmd_verify_jni_live(),
        "verify-android-live" => cmd_verify_android_live(),
        "oracle-differential" => cmd_oracle_differential(),
        "audit" => cmd_audit(),
        "deny" => cmd_deny(),
        "gates" => cmd_gates(),
        "help" | "-h" | "--help" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("xtask: unknown subcommand '{other}'\n");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        "usage: cargo xtask <command> [args]\n\
         \n\
         commands:\n\
         \x20 parity-report              regenerate docs/PARITY_COVERAGE.md\n\
         \x20 check-dependency-policy    fail if Cargo.lock has non-workspace crates\n\
         \x20 check-oracle-integrity     verify retained oracle SHA-256 hashes\n\
         \x20 apk-inventory <apk>        print sha256 + every zip entry name\n\
         \x20 native-abi <lib.so>        print sorted defined FUNC/OBJECT symbols\n\
         \x20 dex-classes <classes.dex>  print sorted class descriptors\n\
         \x20 vendor-advisory-db         materialize the offline cargo-deny advisory db\n\
         \x20 build-apk                  cross-compile + package + sign the Android radar APK\n\
         \x20 check-jni-contract [lib]   fail unless NativeRadar.java's static natives and the library's Java_* exports match 1:1\n\
         \x20 verify-jni-live            run a live Java→JNI→Rust verification against NativeRadar.java\n\
         \x20 verify-android-live        run the strongest current end-to-end Android proof available in this sandbox\n\
         \x20 oracle-differential        execute the immutable oracle under qemu-aarch64 and check the committed executed-oracle vectors (see docs/ORACLE_DIFFERENTIAL.md)\n\
         \x20 audit                      cargo audit against the vendored advisory db\n\
         \x20 deny                       cargo deny check against the vendored advisory db\n\
         \x20 gates                      run every gate (fmt/clippy/build/jni-contract/test/doc/checks/audit/deny)"
    );
}

/// Walks upward from the current directory looking for the repository root
/// (identified by the presence of both `Cargo.toml` and `docs/NATIVE_ABI.txt`,
/// which together are specific enough to avoid false positives from an
/// unrelated ancestor `Cargo.toml`).
fn repo_root() -> Result<PathBuf, String> {
    let start = env::current_dir().map_err(|e| format!("current_dir: {e}"))?;
    repo_root_from(start)
}

/// Pure upward-walk core of [`repo_root`], parameterized on a starting
/// directory instead of reading process-global current-directory state, so
/// both the "found at or above `start`" and "no ancestor qualifies" branches
/// are directly unit-testable against a disposable fixture tree instead of
/// only ever being exercised against this process's real working directory.
fn repo_root_from(start: PathBuf) -> Result<PathBuf, String> {
    let mut dir = start;
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("docs/NATIVE_ABI.txt").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(
                "could not locate repository root (no ancestor has both Cargo.toml and docs/NATIVE_ABI.txt)"
                    .to_string(),
            );
        }
    }
}

fn read_to_string(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

/// Emulates `re.findall(prefix + "([A-Z0-9_]+)", haystack)`: every
/// non-overlapping, left-to-right occurrence of the literal `prefix`
/// immediately followed by one or more ASCII uppercase/digit/underscore
/// characters, capturing that run. Mirrors Python `re`'s backtracking: an
/// occurrence of `prefix` with zero run characters after it is not a match,
/// and the search resumes one byte later (not past the whole prefix).
fn find_prefixed_runs(haystack: &str, prefix: &str) -> Vec<String> {
    fn is_run_char(b: u8) -> bool {
        b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
    }

    let bytes = haystack.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while search_from + prefix_bytes.len() <= bytes.len() {
        let Some(rel) = bytes[search_from..]
            .windows(prefix_bytes.len())
            .position(|w| w == prefix_bytes)
        else {
            break;
        };
        let match_start = search_from + rel;
        let run_start = match_start + prefix_bytes.len();
        let mut run_end = run_start;
        while run_end < bytes.len() && is_run_char(bytes[run_end]) {
            run_end += 1;
        }
        if run_end > run_start {
            out.push(String::from_utf8_lossy(&bytes[run_start..run_end]).into_owned());
            search_from = run_end;
        } else {
            search_from = match_start + 1;
        }
    }
    out
}

/// Emulates `re.findall(r'name: "([^"]+)"', haystack)`: every
/// non-overlapping occurrence of `name: "` followed by one or more
/// non-quote characters and a closing `"`, capturing the inner content, in
/// original order (no dedup, no sort).
fn find_quoted_name_values(haystack: &str) -> Vec<String> {
    let bytes = haystack.as_bytes();
    let prefix = b"name: \"";
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while search_from + prefix.len() <= bytes.len() {
        let Some(rel) = bytes[search_from..]
            .windows(prefix.len())
            .position(|w| w == prefix)
        else {
            break;
        };
        let match_start = search_from + rel;
        let content_start = match_start + prefix.len();
        let Some(quote_rel) = bytes[content_start..].iter().position(|&b| b == b'"') else {
            break;
        };
        let content_end = content_start + quote_rel;
        if content_end > content_start {
            out.push(String::from_utf8_lossy(&bytes[content_start..content_end]).into_owned());
            search_from = content_end + 1;
        } else {
            search_from = match_start + 1;
        }
    }
    out
}

fn dedup_sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

/// Returns the initializer body of a public array constant in Rust source.
fn array_const_body<'a>(source: &'a str, declaration: &str) -> Result<&'a str, String> {
    let (_, after_declaration) = source
        .split_once(declaration)
        .ok_or_else(|| format!("missing declaration `{declaration}`"))?;
    let (_, initializer) = after_declaration
        .split_once('=')
        .ok_or_else(|| format!("missing initializer for `{declaration}`"))?;
    let initializer = initializer.trim_start();
    let body = initializer
        .strip_prefix("&[")
        .or_else(|| initializer.strip_prefix('['))
        .ok_or_else(|| format!("initializer for `{declaration}` is not an array"))?;
    let (body, _) = body
        .split_once("\n];")
        .ok_or_else(|| format!("unterminated initializer for `{declaration}`"))?;
    Ok(body)
}

fn variant_count(source: &str, enum_name: &str, variant: &str) -> usize {
    source.matches(&format!("{enum_name}::{variant}")).count()
}

fn macro_argument_values(source: &str, macro_prefix: &str, argument_index: usize) -> Vec<String> {
    let mut rest = source;
    let mut values = Vec::new();
    while let Some((_, after_prefix)) = rest.split_once(macro_prefix) {
        let Some((arguments, after_invocation)) = after_prefix.split_once(')') else {
            break;
        };
        if let Some(argument) = arguments.split(',').nth(argument_index) {
            values.push(argument.trim().to_string());
        }
        rest = after_invocation;
    }
    values
}

fn macro_argument_count(
    source: &str,
    macro_prefix: &str,
    argument_index: usize,
    expected: &str,
) -> usize {
    macro_argument_values(source, macro_prefix, argument_index)
        .iter()
        .filter(|argument| argument.as_str() == expected)
        .count()
}

fn abi_contract_keys(source: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for (prefix, kind) in [
        ("UNIFFI_META_BLERADAR_CORE_FUNC_", "Function"),
        ("UNIFFI_META_BLERADAR_CORE_METHOD_", "Method"),
        ("UNIFFI_META_BLERADAR_CORE_CONSTRUCTOR_", "Constructor"),
    ] {
        keys.extend(
            find_prefixed_runs(source, prefix)
                .into_iter()
                .map(|name| format!("{kind}:{}", name.to_ascii_lowercase())),
        );
    }
    dedup_sorted(keys)
}

fn runtime_contract_keys(source: &str) -> Result<Vec<String>, String> {
    let names = macro_argument_values(source, "runtime_contract!(", 0);
    let kinds = macro_argument_values(source, "runtime_contract!(", 1);
    if names.len() != kinds.len() {
        return Err("runtime contracts have inconsistent name/kind arguments".to_string());
    }
    names
        .into_iter()
        .zip(kinds)
        .map(|(name, kind)| {
            let name = name
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or_else(|| format!("runtime contract name is not a string literal: {name}"))?;
            if !matches!(kind.as_str(), "Function" | "Method" | "Constructor") {
                return Err(format!("unknown runtime contract kind: {kind}"));
            }
            Ok(format!("{kind}:{name}"))
        })
        .collect()
}

/// Regenerates `docs/PARITY_COVERAGE.md` from `docs/NATIVE_ABI.txt` and the
/// semantic compatibility/runtime registries.
fn cmd_parity_report() -> Result<(), String> {
    let root = repo_root()?;
    let abi_path = root.join("docs/NATIVE_ABI.txt");
    let compat_path = root.join("crates/bleradar-compat/src/lib.rs");
    let out_path = root.join("docs/PARITY_COVERAGE.md");

    let abi_text = read_to_string(&abi_path)?;
    let compat = read_to_string(&compat_path)?;

    let observed_contracts = abi_contract_keys(&abi_text);
    let observed_count = observed_contracts.len();

    let source_contracts = array_const_body(&compat, "pub const CONTRACTS")?;
    let runtime_contracts = array_const_body(&compat, "pub const RUNTIME_CONTRACTS")?;
    let source_names = find_quoted_name_values(source_contracts);
    let runtime_keys = runtime_contract_keys(runtime_contracts)?;
    let runtime_count = runtime_keys.len();
    if runtime_count != observed_count {
        return Err(format!(
            "runtime contract census has {} entries, expected {observed_count}",
            runtime_count
        ));
    }
    if dedup_sorted(runtime_keys) != observed_contracts {
        return Err("runtime contract names/kinds differ from the native ABI census".to_string());
    }
    let runtime_variant_count =
        |variant| macro_argument_count(runtime_contracts, "runtime_contract!(", 2, variant);
    let verified_runtime = runtime_variant_count("VerifiedRuntime");
    let statically_reachable = runtime_variant_count("StaticallyReachable");
    let conditionally_reachable = runtime_variant_count("ConditionallyReachable");
    let unreachable = runtime_variant_count("Unreachable");
    let unknown = runtime_variant_count("Unknown");
    if verified_runtime + statically_reachable + conditionally_reachable + unreachable + unknown
        != runtime_count
    {
        return Err("runtime contract reachability census is incomplete".to_string());
    }

    let mut lines: Vec<String> = vec![
        "# Parity Coverage".to_string(),
        String::new(),
        "Generated from `docs/NATIVE_ABI.txt` and the semantic compatibility/runtime registries."
            .to_string(),
        String::new(),
        format!("- Observed UniFFI function/method/constructor symbols: **{observed_count}**"),
        format!(
            "- Contracts with runtime implementation/reachability classification: **{}**",
            runtime_count
        ),
        format!(
            "- Remaining observed symbols requiring runtime classification: **{}**",
            observed_count.saturating_sub(runtime_count)
        ),
        String::new(),
        "## Shipped implementation".to_string(),
        String::new(),
        format!("- `RUST_NATIVE`: **{runtime_count}**"),
        "- `RUST_MIGRATION_REQUIRED`: **0**".to_string(),
        "- `NON_RUST_JUSTIFIED_BOUNDARY`: **0**".to_string(),
        "- `UNKNOWN`: **0**".to_string(),
        String::new(),
        "## Reachability".to_string(),
        String::new(),
        format!("- `VERIFIED_RUNTIME`: **{verified_runtime}**"),
        format!("- `STATICALLY_REACHABLE`: **{statically_reachable}**"),
        format!("- `CONDITIONALLY_REACHABLE`: **{conditionally_reachable}**"),
        format!("- `UNREACHABLE`: **{unreachable}**"),
        format!("- `UNKNOWN`: **{unknown}**"),
        String::new(),
        "## Source-replacement parity frontier".to_string(),
        String::new(),
        format!(
            "- Differentially verified: **{}**",
            variant_count(source_contracts, "ParityStatus", "DifferentiallyVerified")
        ),
        format!(
            "- Source analogue only: **{}**",
            variant_count(source_contracts, "ParityStatus", "SourceAnalog")
        ),
        format!(
            "- Oracle only: **{}**",
            variant_count(source_contracts, "ParityStatus", "OracleOnly")
        ),
        format!(
            "- Blocked: **{}**",
            variant_count(source_contracts, "ParityStatus", "Blocked")
        ),
        String::new(),
        "Registered source-replacement contracts:".to_string(),
        String::new(),
    ];
    for name in &source_names {
        lines.push(format!("- `{name}`"));
    }
    lines.push(String::new());
    lines.push("## Interpretation".to_string());
    lines.push(String::new());
    lines.push(
        "The shipped implementations behind all 124 ABI contracts are Rust-native; this does not \
         establish parity for similarly named functions in the reconstructed source workspace. \
         Exact source-replacement parity requires characterization of inputs, outputs, side \
         effects, state, termination, and errors against the immutable oracle. Reachability and \
         evidence details are recorded in `docs/VERIFIED_RUNTIME_TOPOLOGY.md`."
            .to_string(),
    );

    let content = lines.join("\n") + "\n";
    fs::write(&out_path, content).map_err(|e| format!("writing {}: {e}", out_path.display()))?;
    println!("{}", out_path.display());
    Ok(())
}

/// Names of the only crates this workspace is allowed to depend on
/// (`docs/AUTONOMOUS_DECISIONS.md`, decision 9).
const ALLOWED: [&str; 3] = ["bleradar-core", "bleradar-compat", "bleradar-jni"];

/// Extracts every `name = "..."` package name from `Cargo.lock` text, in
/// file order. Pure text scan (no filesystem access), so it is directly
/// unit-testable against fixture lockfile content, including unparsable
/// input that yields no names at all.
fn parse_lockfile_package_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("name = \"")
            && let Some(inner) = rest.strip_suffix('"')
        {
            names.push(inner.to_string());
        }
    }
    names
}

/// Outcome of comparing a lockfile's parsed package names against the
/// allowed set, decoupled from filesystem access and `println!` side
/// effects so every branch — not just the one exercised by the real
/// `Cargo.lock` on every gate run — is directly unit-testable with fixture
/// input.
#[derive(Debug, PartialEq, Eq)]
enum DependencyPolicyOutcome {
    /// No package names could be parsed at all (empty/unparsable lockfile).
    EmptyLockfile,
    /// One or more crates outside `allowed` were found (sorted, deduped).
    ForeignCrates(Vec<String>),
    /// Every parsed name is in `allowed` (sorted copy of `allowed`, for the
    /// success message).
    Compliant(Vec<String>),
}

fn evaluate_dependency_policy(names: &[String], allowed: &[&str]) -> DependencyPolicyOutcome {
    if names.is_empty() {
        return DependencyPolicyOutcome::EmptyLockfile;
    }

    let mut foreign: Vec<String> = names
        .iter()
        .filter(|n| !allowed.contains(&n.as_str()))
        .cloned()
        .collect();
    foreign.sort();
    foreign.dedup();
    if !foreign.is_empty() {
        return DependencyPolicyOutcome::ForeignCrates(foreign);
    }

    let mut allowed_sorted: Vec<String> = allowed.iter().map(|s| (*s).to_string()).collect();
    allowed_sorted.sort_unstable();
    DependencyPolicyOutcome::Compliant(allowed_sorted)
}

/// Port of `tools/check_dependency_policy.py`: fails when the root
/// `Cargo.lock` contains any crate outside the audited workspace set.
fn cmd_check_dependency_policy() -> Result<(), String> {
    let root = repo_root()?;
    let lock_path = root.join("Cargo.lock");
    let text = read_to_string(&lock_path)?;
    let names = parse_lockfile_package_names(&text);

    match evaluate_dependency_policy(&names, &ALLOWED) {
        DependencyPolicyOutcome::EmptyLockfile => {
            println!("Dependency policy gate could not parse any package names from Cargo.lock;");
            println!("refusing to pass silently on an unreadable lockfile.");
            Err("empty package name set".to_string())
        }
        DependencyPolicyOutcome::ForeignCrates(foreign) => {
            println!("Dependency policy violation: third-party crates present in Cargo.lock:");
            for name in &foreign {
                println!("  - {name}");
            }
            println!(
                "The workspace is intentionally third-party-free (docs/AUTONOMOUS_DECISIONS.md, decision 9)."
            );
            println!(
                "Adding a dependency is a deliberate decision: record it in the decision log and"
            );
            println!(
                "update ALLOWED in xtask/src/main.rs (cmd_check_dependency_policy) in the same change."
            );
            Err("foreign crates present".to_string())
        }
        DependencyPolicyOutcome::Compliant(allowed_sorted) => {
            println!(
                "Cargo.lock contains only the audited workspace crates: {}",
                allowed_sorted.join(", ")
            );
            Ok(())
        }
    }
}

/// Port of `tools/check_oracle_integrity.py`: fails when a retained binary
/// oracle no longer matches its recorded SHA-256.
fn cmd_check_oracle_integrity() -> Result<(), String> {
    let root = repo_root()?;
    let apk_path = root.join("BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk");
    let input_sha_path = root.join("docs/INPUT_SHA256.txt");
    let zip_path = root.join("BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip");
    const ZIP_BASELINE: &str = "07d2d80ce7e6c43f4c6ccc2496d30faafb77342e7bf196b894d32c7528cf3f76";

    let apk_name = apk_path.file_name().unwrap().to_string_lossy().into_owned();
    let input_sha_name = input_sha_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let zip_name = zip_path.file_name().unwrap().to_string_lossy().into_owned();

    let input_sha_text = fs::read_to_string(&input_sha_path).unwrap_or_default();
    let expected_apk_sha = find_sha256_after_label(&input_sha_text, "Original APK SHA-256:");
    let actual_apk_sha = if apk_path.exists() {
        Some(sha256::to_hex(&sha256::sha256(&read_bytes(&apk_path)?)))
    } else {
        None
    };
    let apk_result = evaluate_oracle_hash(
        expected_apk_sha.as_deref(),
        actual_apk_sha.as_deref(),
        format!("could not parse an APK SHA-256 from {input_sha_name}"),
        format!("missing oracle file: {apk_name}"),
        &apk_name,
    );

    let actual_zip_sha = if zip_path.exists() {
        Some(sha256::to_hex(&sha256::sha256(&read_bytes(&zip_path)?)))
    } else {
        None
    };
    let zip_result = evaluate_oracle_hash(
        Some(ZIP_BASELINE),
        actual_zip_sha.as_deref(),
        String::new(), // unreachable: the zip's expected hash is a constant, never unparsable
        format!("missing oracle archive: {zip_name}"),
        &zip_name,
    );

    let failures: Vec<String> = [apk_result, zip_result]
        .into_iter()
        .filter_map(Result::err)
        .collect();

    if !failures.is_empty() {
        println!("Oracle integrity violation — immutable behavioral oracles must never change:");
        for failure in &failures {
            println!("  - {failure}");
        }
        return Err("oracle integrity violation".to_string());
    }

    println!(
        "Oracle integrity verified: APK matches docs/INPUT_SHA256.txt; migration archive matches its recorded baseline."
    );
    Ok(())
}

/// Evaluates one oracle file's SHA-256 against its expected value, without
/// touching the filesystem, so every failure branch is directly
/// unit-testable with fixture input instead of only ever being exercised
/// against the one real committed oracle: `expected == None` models an
/// unparsable/absent expected-hash record, `actual == None` models a missing
/// oracle file, and a `Some != Some` mismatch models tampering/drift.
fn evaluate_oracle_hash(
    expected: Option<&str>,
    actual: Option<&str>,
    unparsable_message: String,
    missing_message: String,
    mismatch_label: &str,
) -> Result<(), String> {
    match expected {
        None => Err(unparsable_message),
        Some(_) if actual.is_none() => Err(missing_message),
        Some(expected) => {
            let actual =
                actual.expect("checked above: Some(_) arm only reached when actual is Some");
            if actual == expected {
                Ok(())
            } else {
                Err(format!(
                    "{mismatch_label}: expected {expected}, observed {actual}"
                ))
            }
        }
    }
}

/// Finds a 64-character lowercase-hex run immediately after (whitespace
/// permitted between) a literal label, mirroring
/// `re.search(label + r"\s*([0-9a-f]{64})", text)`.
fn find_sha256_after_label(text: &str, label: &str) -> Option<String> {
    fn is_lowercase_hex_digit(b: u8) -> bool {
        b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
    }

    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let trimmed = after.trim_start_matches(|c: char| c.is_whitespace());
    let hex_len = trimmed
        .as_bytes()
        .iter()
        .take_while(|&&b| is_lowercase_hex_digit(b))
        .count();
    if hex_len >= 64 {
        // All matched bytes are single-byte ASCII, so byte offset 64 always
        // falls on a `char` boundary.
        Some(trimmed[..64].to_string())
    } else {
        None
    }
}

/// Port of `tools/apk_inventory.py`.
fn cmd_apk_inventory(args: &[String]) -> Result<(), String> {
    let apk_arg = args.first().ok_or("usage: apk-inventory <path-to-apk>")?;
    let apk_path = PathBuf::from(apk_arg);
    let data = read_bytes(&apk_path)?;

    println!("apk={}", apk_path.display());
    println!("sha256={}", sha256::to_hex(&sha256::sha256(&data)));

    let mut names = zip_reader::entry_names(&data).map_err(|e| e.to_string())?;
    names.sort();
    println!("entries={}", names.len());
    // Full entry set — not just dex/lib/manifest — so live inventory can prove
    // resources.arsc, adaptive icons, and any future packaged assets are present.
    for name in &names {
        println!("{name}");
    }
    Ok(())
}

/// Port of `tools/native_abi.sh`.
fn cmd_native_abi(args: &[String]) -> Result<(), String> {
    let lib_arg = args.first().ok_or("usage: native-abi <path-to-lib.so>")?;
    let data = read_bytes(&PathBuf::from(lib_arg))?;
    let names = elf::defined_func_and_object_symbols(&data).map_err(|e| e.to_string())?;
    for name in &names {
        println!("{name}");
    }
    Ok(())
}

/// New reverse-engineering capability (no Python precursor): lists every
/// class defined in a DEX file, underpinning `docs/DEX_CLASS_CENSUS.txt`.
fn cmd_dex_classes(args: &[String]) -> Result<(), String> {
    let dex_arg = args
        .first()
        .ok_or("usage: dex-classes <path-to-classes.dex>")?;
    let data = read_bytes(&PathBuf::from(dex_arg))?;
    let names = dex::class_names(&data).map_err(|e| e.to_string())?;
    for name in &names {
        println!("{name}");
    }
    Ok(())
}

fn cmd_vendor_advisory_db() -> Result<(), String> {
    let root = repo_root()?;
    let target = vendor::materialize_advisory_db(&root)?;
    println!("{}", target.display());
    Ok(())
}

// ---------------------------------------------------------------------
// `build-apk`: cross-compiles `bleradar-jni`, hand-packages the Android
// app under `android/app/src/main`, and signs the result — with no
// Gradle/AndroidX/Compose (unreachable from this workspace's network
// policy; see docs/ANDROID_APP.md), only the Android SDK build-tools, the
// NDK, and the JDK, all invoked as plain external tools exactly like
// `cmd_audit`/`cmd_deny` already invoke `cargo audit`/`cargo deny` above.
// Deliberately NOT part of `cmd_gates`: unlike every other gate, this one
// requires an installed Android SDK + NDK, which CI's runner does not
// provide.
// ---------------------------------------------------------------------

/// Directory (relative to the repo root) holding the hand-built Android
/// app's manifest, resources, and Java sources. See `docs/ANDROID_APP.md`
/// for why this app has no Gradle project.
const ANDROID_APP_DIR: &str = "android/app/src/main";

/// Path (relative to the repo root) of the Java façade whose real JVM→JNI→Rust
/// behavior `verify-jni-live` executes.
const NATIVE_RADAR_JAVA_PATH: &str = "android/app/src/main/java/com/hse/bleradar/NativeRadar.java";

/// Name of the cross-compiled native library, matching
/// `NativeRadar.ensureLoaded()`'s `System.loadLibrary("bleradar_jni")`.
const NATIVE_LIB_FILE_NAME: &str = "libbleradar_jni.so";

/// APK entries the current hand-built Android package must contain to remain
/// installable and reach the JNI bridge.
///
/// `resources.arsc` is required in addition to the launch/JNI trio: aapt2
/// packages the compiled resource table uncompressed (`-0 arsc`) and the
/// runtime resource lookup path depends on it. Live 2026-09-09 inventory of
/// `HSE-BLE-Radar-arm64-v1.0.0.apk` confirmed all four are present.
const REQUIRED_APK_ENTRIES: &[&str] = &[
    "AndroidManifest.xml",
    "classes.dex",
    "lib/arm64-v8a/libbleradar_jni.so",
    "resources.arsc",
];

/// Critical Java classes the built `classes.dex` must define for the app's
/// launch, scan, and JNI paths.
const REQUIRED_DEX_CLASSES: &[&str] = &[
    "com/hse/bleradar/MainActivity",
    "com/hse/bleradar/NativeRadar",
    "com/hse/bleradar/RadarScanService",
    "com/hse/bleradar/BleScanEngine",
];

/// Symbol prefix every JNI export carries; used to select the JNI-facing
/// subset of a native library's defined symbols for the contract check.
const JNI_EXPORT_PREFIX: &str = "Java_";

/// The complete JNI export contract derived from one Java façade source.
///
/// `NativeRadar.java` is the single authority: every `static native` method
/// it declares must be backed by exactly one `Java_<package>_<class>_<method>`
/// export in the built `bleradar-jni` library, and the library must export no
/// `Java_`-prefixed symbol the façade does not declare. Deriving the expected
/// set from the Java source (instead of a hand-maintained duplicate list)
/// means adding, renaming, or removing a native on either side without the
/// other fails `cargo xtask gates`, `verify-jni-live`, and
/// `verify-android-live` with the exact drift named — rather than surfacing
/// only on device as an `UnsatisfiedLinkError` at that method's first call.
#[derive(Debug, Clone, PartialEq, Eq)]
struct JniExportContract {
    /// `Java_<mangled package>_<mangled class>`.
    symbol_prefix: String,
    /// Sorted, deduplicated `static native` method names.
    native_methods: Vec<String>,
    /// Sorted expected export symbols, one per native method.
    expected_exports: Vec<String>,
}

impl JniExportContract {
    /// Parses `java_source` into the export contract it requires.
    ///
    /// Fails on: a missing `package`/`class` declaration, a `native` method
    /// that is not `static` (the façade is a static utility class and the
    /// live harness invokes natives without an instance), or two natives
    /// sharing one name (JNI overloads need `__<signature>`-mangled exports,
    /// which `bleradar-jni` deliberately does not implement).
    fn from_java_source(java_source: &str) -> Result<Self, String> {
        let code = strip_java_comments_and_literals(java_source);
        let (package, class) = java_package_and_class(&code)?;
        let native_methods = java_static_native_method_names(&code)?;
        let symbol_prefix = format!(
            "{JNI_EXPORT_PREFIX}{}_{}",
            jni_mangle(&package),
            jni_mangle(&class)
        );
        let expected_exports = native_methods
            .iter()
            .map(|name| format!("{symbol_prefix}_{}", jni_mangle(name)))
            .collect();
        Ok(Self {
            symbol_prefix,
            native_methods,
            expected_exports,
        })
    }

    /// Requires the `Java_`-prefixed subset of `defined_symbols` to equal
    /// [`Self::expected_exports`] exactly: no declared native may lack an
    /// export, and no export may lack a declaration.
    fn require_exact_exports(&self, defined_symbols: &[String]) -> Result<(), String> {
        let actual: Vec<&String> = defined_symbols
            .iter()
            .filter(|symbol| symbol.starts_with(JNI_EXPORT_PREFIX))
            .collect();
        let missing: Vec<&str> = self
            .expected_exports
            .iter()
            .filter(|expected| !actual.contains(expected))
            .map(String::as_str)
            .collect();
        let orphaned: Vec<&str> = actual
            .iter()
            .filter(|symbol| !self.expected_exports.contains(symbol))
            .map(|symbol| symbol.as_str())
            .collect();
        if missing.is_empty() && orphaned.is_empty() {
            return Ok(());
        }
        let mut problems = Vec::new();
        if !missing.is_empty() {
            problems.push(format!(
                "{} native method(s) declared in NativeRadar.java have no export: {}",
                missing.len(),
                missing.join(", ")
            ));
        }
        if !orphaned.is_empty() {
            problems.push(format!(
                "{} `{JNI_EXPORT_PREFIX}` export(s) have no `static native` declaration in NativeRadar.java: {}",
                orphaned.len(),
                orphaned.join(", ")
            ));
        }
        Err(format!(
            "JNI export contract violated: {}",
            problems.join("; ")
        ))
    }
}

/// Blanks Java comments and string/char literal contents (keeping line
/// structure and the quote characters) so declaration scanning cannot be
/// fooled by the word `native` in Javadoc prose or in a string.
fn strip_java_comments_and_literals(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    if next == '\n' {
                        out.push('\n');
                    }
                    previous = next;
                }
                out.push(' ');
            }
            '"' | '\'' => {
                out.push(c);
                let mut escaped = false;
                for next in chars.by_ref() {
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    match next {
                        '\\' => escaped = true,
                        '\n' => {
                            // Unterminated literal: keep the line break so
                            // later diagnostics stay line-accurate.
                            out.push('\n');
                            break;
                        }
                        _ if next == c => break,
                        _ => {}
                    }
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_java_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn is_java_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| is_java_identifier_char(first) && !first.is_ascii_digit())
        && chars.all(is_java_identifier_char)
}

/// Finds every whole-word occurrence of `keyword` in `code`, returning the
/// byte offset of each match.
fn whole_word_offsets<'a>(code: &'a str, keyword: &'a str) -> impl Iterator<Item = usize> + 'a {
    code.match_indices(keyword).filter_map(move |(start, _)| {
        let end = start + keyword.len();
        let before_ok = code[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_java_identifier_char(c));
        let after_ok = code[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_java_identifier_char(c));
        (before_ok && after_ok).then_some(start)
    })
}

/// Reads the `package a.b.c;` and first `class Name` declarations from
/// comment-stripped Java `code`.
fn java_package_and_class(code: &str) -> Result<(String, String), String> {
    let package = whole_word_offsets(code, "package")
        .find_map(|start| {
            let rest = &code[start + "package".len()..];
            let (name, _) = rest.split_once(';')?;
            let name = name.trim();
            (!name.is_empty() && name.split('.').all(is_java_identifier)).then(|| name.to_string())
        })
        .ok_or_else(|| "missing `package <name>;` declaration in NativeRadar.java".to_string())?;
    let class = whole_word_offsets(code, "class")
        .find_map(|start| {
            let name = code[start + "class".len()..]
                .split_whitespace()
                .next()?
                .trim_end_matches('{');
            is_java_identifier(name).then(|| name.to_string())
        })
        .ok_or_else(|| "missing `class <Name>` declaration in NativeRadar.java".to_string())?;
    Ok((package, class))
}

/// Returns the sorted, deduplicated names of every `static native` method
/// declared in comment-stripped Java `code`.
fn java_static_native_method_names(code: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for start in whole_word_offsets(code, "native") {
        let end = start + "native".len();
        let boundary = code[..start]
            .rfind([';', '{', '}'])
            .map_or(0, |index| index + 1);
        let modifiers = &code[boundary..start];
        let head_end = code[end..]
            .find('(')
            .map(|offset| end + offset)
            .ok_or_else(|| {
                format!(
                    "native declaration without a parameter list: {:?}",
                    code[boundary..end].trim()
                )
            })?;
        let head = &code[end..head_end];
        if head.contains([';', '{', '}']) {
            return Err(format!(
                "malformed native declaration: {:?}",
                code[boundary..head_end].trim()
            ));
        }
        let tokens: Vec<&str> = head.split_whitespace().collect();
        let Some((name, return_type)) = tokens.split_last() else {
            return Err(format!(
                "native declaration without a return type and name: {:?}",
                code[boundary..head_end].trim()
            ));
        };
        if return_type.is_empty() || !is_java_identifier(name) {
            return Err(format!(
                "native declaration without a return type and name: {:?}",
                code[boundary..head_end].trim()
            ));
        }
        if !modifiers.split_whitespace().any(|token| token == "static") {
            return Err(format!(
                "non-static native method `{name}` is outside the JNI façade contract (every native must be `static`)"
            ));
        }
        names.push((*name).to_string());
    }
    names.sort();
    if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(format!(
            "overloaded native method `{}`: JNI overloads need `__<signature>`-mangled exports, which bleradar-jni does not implement",
            pair[0]
        ));
    }
    Ok(names)
}

/// Applies the JNI symbol-name mangling rules (JNI Specification, "Resolving
/// Native Method Names") to one package, class, or method name: `.`/`/`
/// become `_`, `_` becomes `_1`, `;` becomes `_2`, `[` becomes `_3`, ASCII
/// alphanumerics pass through, and anything else becomes `_0` + four
/// lowercase hex digits of its code unit.
fn jni_mangle(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '.' | '/' => out.push('_'),
            '_' => out.push_str("_1"),
            ';' => out.push_str("_2"),
            '[' => out.push_str("_3"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("_0{unit:04x}"));
                }
            }
        }
    }
    out
}

/// Verifies the JNI export contract of one built native library against the
/// repository's `NativeRadar.java`, printing the matched surface on success.
fn verify_jni_export_contract(
    java_source: &str,
    library_path: &Path,
) -> Result<JniExportContract, String> {
    let contract = JniExportContract::from_java_source(java_source)?;
    let defined_symbols = elf::defined_func_and_object_symbols(&read_bytes(library_path)?)
        .map_err(|e| {
            format!(
                "parsing {}: {e} (the JNI export-contract check reads ELF64 little-endian libraries: Linux hosts and the Android cross-compile)",
                library_path.display()
            )
        })?;
    contract.require_exact_exports(&defined_symbols)?;
    println!(
        "jni export contract: {} `static native` methods in NativeRadar.java ↔ {} `{}_*` exports in {}: exact match",
        contract.native_methods.len(),
        contract.expected_exports.len(),
        contract.symbol_prefix,
        library_path.display()
    );
    Ok(contract)
}

/// Builds the host `bleradar-jni` cdylib and returns its path.
fn build_host_jni_library(root: &Path) -> Result<PathBuf, String> {
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(root)
            .args(["build", "-p", "bleradar-jni", "--locked"]);
        c
    })?;
    let host_lib = root
        .join("target/debug")
        .join(host_cdylib_file_name("bleradar_jni"));
    if !host_lib.is_file() {
        return Err(format!(
            "expected host-built native library missing: {}",
            host_lib.display()
        ));
    }
    Ok(host_lib)
}

fn cmd_check_jni_contract(args: &[String]) -> Result<(), String> {
    let root = repo_root()?;
    let library_path = match args {
        [] => build_host_jni_library(&root)?,
        [path] => PathBuf::from(path),
        _ => return Err("usage: cargo xtask check-jni-contract [lib.so]".to_string()),
    };
    let java_source = read_to_string(&root.join(NATIVE_RADAR_JAVA_PATH))?;
    verify_jni_export_contract(&java_source, &library_path).map(|_| ())
}

/// Rust standard-library target `cargo xtask build-apk` cross-compiles the JNI
/// bridge for.
const ANDROID_RUST_TARGET: &str = "aarch64-linux-android";

/// Final signed APK's committed name at the repository root.
const APK_OUTPUT_NAME: &str = "HSE-BLE-Radar-arm64-v1.0.0.apk";

/// Alias/password for the ephemeral, non-secret signing identity this
/// command generates if one is not already present. Deliberately mirrors
/// the Android SDK's own long-standing, publicly documented
/// `debug.keystore` convention (same alias, same password) precisely
/// because that convention is not a secret — it's the same well-known
/// placeholder every Android developer's local debug keystore already
/// uses — and reusing it here avoids inventing a new value that a secret
/// scanner (or a future reader) might mistake for a real credential.
const DEBUG_KEYSTORE_ALIAS: &str = "androiddebugkey";
const DEBUG_KEYSTORE_PASSWORD: &str = "android";

/// Parses a directory name like `"34.0.0"` or `"27.3.13750724"` into a
/// comparable numeric key, or `None` if any dot-separated segment is not a
/// plain non-negative integer. This deliberately filters out beta/rc-style
/// names such as `"37.2-beta1"` so version discovery only ever picks a
/// stable install.
fn parse_plain_version(name: &str) -> Option<Vec<u64>> {
    if name.is_empty() {
        return None;
    }
    name.split('.')
        .map(|segment| segment.parse().ok())
        .collect()
}

/// Picks the immediate subdirectory of `parent` with the highest
/// [`parse_plain_version`] key for which `predicate` also holds (e.g.
/// "contains an `aapt2` executable"). Returns the full path of the winner.
fn pick_highest_version_dir(
    parent: &Path,
    mut predicate: impl FnMut(&Path) -> bool,
) -> Option<PathBuf> {
    let entries = fs::read_dir(parent).ok()?;
    let mut best: Option<(Vec<u64>, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(key) = parse_plain_version(name) else {
            continue;
        };
        if !predicate(&path) {
            continue;
        }
        let is_better = best
            .as_ref()
            .map(|(best_key, _)| key > *best_key)
            .unwrap_or(true);
        if is_better {
            best = Some((key, path));
        }
    }
    best.map(|(_, path)| path)
}

/// The Android NDK's host-toolchain directory name for the platform this
/// binary is itself running on (mirrors the NDK's own `prebuilt/<tag>`
/// naming — `linux-x86_64`, `darwin-x86_64`, or `windows-x86_64`).
fn ndk_host_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

/// Platform-specific filename Cargo emits for a host `cdylib`.
fn host_cdylib_file_name(crate_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{crate_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{crate_name}.dylib")
    } else {
        format!("lib{crate_name}.so")
    }
}

/// Parses `rustup target list --installed` stdout into exact target triples.
fn installed_rust_targets(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

/// Ensures the pinned toolchain has the Rust standard library for `target`,
/// auto-installing it with `rustup target add` when missing so `build-apk`
/// works in a fresh sandbox instead of failing late with `can't find crate for
/// std`.
fn ensure_rustup_target_installed(root: &Path, target: &str) -> Result<(), String> {
    let output = Command::new("rustup")
        .current_dir(root)
        .args(["target", "list", "--installed"])
        .output()
        .map_err(|e| format!("failed to spawn rustup target list --installed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "\"rustup\" \"target\" \"list\" \"--installed\" exited with {}",
            output.status
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if installed_rust_targets(&stdout).contains(&target) {
        return Ok(());
    }

    println!("== rustup target add {target} ==");
    run_status({
        let mut c = Command::new("rustup");
        c.current_dir(root).args(["target", "add", target]);
        c
    })
}

/// Extracts the integer assigned by `public static final int <name> = ...;`
/// from Java source, matching this repository's `NativeRadar.java` contract
/// constants without needing a full Java parser.
fn find_java_static_final_int(source: &str, name: &str) -> Result<i32, String> {
    let prefix = format!("public static final int {name} = ");
    let (_, after_prefix) = source
        .split_once(&prefix)
        .ok_or_else(|| format!("missing Java constant `{name}`"))?;
    let (value_text, _) = after_prefix
        .split_once(';')
        .ok_or_else(|| format!("unterminated Java constant `{name}`"))?;
    value_text
        .trim()
        .parse::<i32>()
        .map_err(|e| format!("Java constant `{name}` value {value_text:?}: {e}"))
}

/// Creates a dedicated scratch directory under the OS temp directory for one
/// xtask command run, clearing any stale prior contents for the same process.
fn xtask_temp_dir(label: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("xtask-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Returns the Java source for a tiny live verifier that exercises the real
/// `NativeRadar.java` façade and the host-built `bleradar-jni` library.
///
/// `expected_native_count` is the number of `static native` methods the
/// xtask parser found in `NativeRadar.java`; the harness re-derives that set
/// through JVM reflection, invokes every member so the JVM must actually
/// resolve each export, and fails if the two views disagree — so neither a
/// parser blind spot nor an unexported native can pass silently.
fn jni_smoke_java_source(expected_abi_version: i32, expected_native_count: usize) -> String {
    format!(
        r#"import com.hse.bleradar.NativeRadar;

public final class JniSmoke {{
    private static void require(boolean condition, String message) {{
        if (!condition) {{
            throw new IllegalStateException(message);
        }}
    }}

    private static Object defaultArgument(Class<?> type) {{
        if (type == double.class) {{
            return 0.0d;
        }}
        if (type == float.class) {{
            return 0.0f;
        }}
        if (type == long.class) {{
            return 0L;
        }}
        if (type == int.class) {{
            return 0;
        }}
        if (type == short.class) {{
            return (short) 0;
        }}
        if (type == byte.class) {{
            return (byte) 0;
        }}
        if (type == char.class) {{
            return '\0';
        }}
        if (type == boolean.class) {{
            return false;
        }}
        throw new IllegalStateException("unsupported native parameter type: " + type.getName());
    }}

    /**
     * Resolves and invokes every native the façade declares. The JVM links a
     * native lazily at its first call, so only invoking each one proves the
     * loaded library exports it; every export is a documented total function
     * over primitives, so default arguments are safe.
     */
    private static void verifyEveryDeclaredNativeLinks() {{
        java.util.List<String> linked = new java.util.ArrayList<>();
        for (java.lang.reflect.Method method : NativeRadar.class.getDeclaredMethods()) {{
            int modifiers = method.getModifiers();
            if (!java.lang.reflect.Modifier.isNative(modifiers)) {{
                continue;
            }}
            require(
                    java.lang.reflect.Modifier.isStatic(modifiers),
                    "non-static native method: " + method.getName());
            Class<?>[] parameterTypes = method.getParameterTypes();
            Object[] arguments = new Object[parameterTypes.length];
            for (int i = 0; i < parameterTypes.length; i++) {{
                arguments[i] = defaultArgument(parameterTypes[i]);
            }}
            try {{
                method.invoke(null, arguments);
            }} catch (ReflectiveOperationException error) {{
                throw new IllegalStateException(
                        "native method " + method.getName() + " did not link/execute", error);
            }}
            linked.add(method.getName());
        }}
        java.util.Collections.sort(linked);
        require(
                linked.size() == {expected_native_count},
                "NativeRadar.java parse found {expected_native_count} static native methods but JVM reflection linked "
                        + linked.size() + ": " + linked);
        System.out.println("linked-natives=" + linked.size());
    }}

    private static void verifySuccessPath() {{
        require(NativeRadar.isAvailable(), "NativeRadar unavailable: " + NativeRadar.loadError());
        require(
                NativeRadar.abiVersion() == {expected_abi_version},
                "abiVersion mismatch: " + NativeRadar.abiVersion());
        verifyEveryDeclaredNativeLinks();
        double filtered = NativeRadar.filteredRssi(Double.NaN, -59.0, 0.35);
        require(Math.abs(filtered - (-59.0)) < 1e-9, "unexpected filtered RSSI bootstrap: " + filtered);
        double distance = NativeRadar.bleDistanceM(-59.0, -59.0, 2.0);
        require(Math.abs(distance - 1.0) < 1e-9, "unexpected distance: " + distance);
        double lower = NativeRadar.distanceLowerBoundM(-59.0, 6.0, -59.0, 2.0);
        double upper = NativeRadar.distanceUpperBoundM(-59.0, 6.0, -59.0, 2.0);
        require(lower < distance, "unexpected lower bound: " + lower);
        require(upper > distance, "unexpected upper bound: " + upper);
        require(
                NativeRadar.proximityLabel(-60.0) == NativeRadar.PROXIMITY_NEAR,
                "unexpected proximity");
        require(
                NativeRadar.signalTrend(-80.0, -60.0, 3.0) == NativeRadar.TREND_STRONGER,
                "unexpected trend");
        require(
                NativeRadar.signalConfidencePercent(8, 1.5) >= 70,
                "unexpected confidence score");
        require(
                NativeRadar.defaultCalibrationProfile() == NativeRadar.CALIBRATION_BASELINE,
                "unexpected default calibration profile");
        require(
                NativeRadar.defaultTrackingProfile() == NativeRadar.TRACKING_STANDARD,
                "unexpected default tracking profile");
        require(
                Math.abs(NativeRadar.calibrationProfileRssiAt1mDbm(NativeRadar.CALIBRATION_BASELINE) - (-59.0)) < 1e-9,
                "unexpected baseline calibration RSSI");
        require(
                Math.abs(NativeRadar.calibrationProfilePathLossExponent(NativeRadar.CALIBRATION_BASELINE) - 2.0) < 1e-9,
                "unexpected baseline calibration exponent");
        require(
                Double.isNaN(NativeRadar.calibrationProfileRssiAt1mDbm(99)),
                "invalid calibration profile did not yield NaN sentinel");
        double tracked = NativeRadar.trackingFilteredRssi(
                -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                Double.NaN);
        require(Math.abs(tracked - (-69.0)) < 1e-9, "unexpected tracked RSSI: " + tracked);
        double trackedDistance = NativeRadar.trackingDistanceM(
                -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                Double.NaN);
        require(trackedDistance > 1.0, "unexpected tracked distance: " + trackedDistance);
        require(
                NativeRadar.trackingTrend(
                        -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                        Double.NaN)
                        == NativeRadar.TREND_STRONGER,
                "unexpected tracked trend");
        require(
                NativeRadar.trackingProximity(
                        -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                        Double.NaN)
                        == NativeRadar.PROXIMITY_MID,
                "unexpected tracked proximity");
        require(
                NativeRadar.trackingDistanceProximity(
                        -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                        Double.NaN)
                        == NativeRadar.PROXIMITY_MID,
                "unexpected distance-derived tracked proximity");
        require(
                NativeRadar.trackingConfidencePercent(
                        -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                        Double.NaN)
                        >= 50,
                "unexpected tracked confidence");
        require(
                NativeRadar.trackingFreshness(
                        -80.0, -60.0, 4.0, 6, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_RESPONSIVE, 250L,
                        Double.NaN)
                        == NativeRadar.FRESHNESS_LIVE,
                "unexpected tracked freshness");
        require(
                Double.isNaN(NativeRadar.bleDistanceM(-70.0, -59.0, 0.0)),
                "invalid input did not yield NaN sentinel");
        require(
                Double.isNaN(NativeRadar.trackingDistanceM(
                        Double.NaN, -70.0, 0.0, -1, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_STANDARD, 0L,
                        Double.NaN)),
                "invalid tracking input did not yield NaN sentinel");
        double distanceWithoutTxPower = NativeRadar.trackingDistanceM(
                Double.NaN, -70.0, 0.0, 1, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_STANDARD, 0L,
                Double.NaN);
        double distanceWithTxPower = NativeRadar.trackingDistanceM(
                Double.NaN, -70.0, 0.0, 1, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_STANDARD, 0L,
                -70.0);
        require(
                Math.abs(distanceWithTxPower - 1.0) < 1e-9 && distanceWithTxPower < distanceWithoutTxPower,
                "device-advertised txPowerDbm did not override profile calibration: "
                        + distanceWithTxPower + " vs " + distanceWithoutTxPower);
        require(
                NativeRadar.trackingDistanceM(
                        Double.NaN, -70.0, 0.0, 1, NativeRadar.CALIBRATION_BASELINE, NativeRadar.TRACKING_STANDARD, 0L,
                        127.0)
                        == distanceWithoutTxPower,
                "implausible txPowerDbm (TX_POWER_NOT_PRESENT sentinel) was not ignored");
    }}

    private static void verifyFailurePath() {{
        require(!NativeRadar.isAvailable(), "expected NativeRadar to be unavailable");
        require(NativeRadar.loadError() != null, "expected loadError when library path is wrong");
    }}

    public static void main(String[] args) {{
        String mode = args.length == 0 ? "success" : args[0];
        switch (mode) {{
            case "success":
                verifySuccessPath();
                verifySuccessPath();
                System.out.println("success-ok abi=" + NativeRadar.abiVersion());
                break;
            case "failure":
                verifyFailurePath();
                System.out.println("failure-ok cause=" + NativeRadar.loadError().getClass().getName());
                break;
            default:
                throw new IllegalArgumentException("unknown mode: " + mode);
        }}
    }}
}}
"#
    )
}

/// Requires `actual` to contain every `expected` member, reporting the missing
/// subset in deterministic input order for actionable proof failures.
fn require_expected_members(
    actual: &[String],
    expected: &[&str],
    collection_name: &str,
) -> Result<(), String> {
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|expected_member| {
            !actual
                .iter()
                .any(|actual_member| actual_member == expected_member)
        })
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{collection_name} missing required members: {}",
            missing.join(", ")
        ))
    }
}

/// Locates an installed Android SDK: `ANDROID_HOME`/`ANDROID_SDK_ROOT` if
/// set to an existing directory, else this sandbox's known install path.
fn discover_sdk_root() -> Result<PathBuf, String> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(value) = env::var(var) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Ok(path);
            }
        }
    }
    let fallback = PathBuf::from("/usr/local/lib/android/sdk");
    if fallback.is_dir() {
        return Ok(fallback);
    }
    Err(
        "Android SDK not found: set ANDROID_HOME (or ANDROID_SDK_ROOT) to its install path"
            .to_string(),
    )
}

/// Picks the highest-versioned `build-tools/<version>` directory that
/// contains every tool this pipeline needs.
fn discover_build_tools(sdk_root: &Path) -> Result<PathBuf, String> {
    let build_tools_dir = sdk_root.join("build-tools");
    pick_highest_version_dir(&build_tools_dir, |path| {
        ["aapt2", "d8", "zipalign", "apksigner"]
            .iter()
            .all(|tool| path.join(tool).is_file())
    })
    .ok_or_else(|| {
        format!(
            "no build-tools/<version> under {} contains aapt2/d8/zipalign/apksigner",
            build_tools_dir.display()
        )
    })
}

/// Picks the highest plain `android-<N>/android.jar` (skips extension and
/// preview variants like `android-34-ext8` or `android-37.2-beta1`, which
/// use a different naming scheme and are not needed here).
fn discover_platform_jar(sdk_root: &Path) -> Result<PathBuf, String> {
    let platforms_dir = sdk_root.join("platforms");
    let entries = fs::read_dir(&platforms_dir)
        .map_err(|e| format!("reading {}: {e}", platforms_dir.display()))?;
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(number) = name
            .strip_prefix("android-")
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        let jar = path.join("android.jar");
        if !jar.is_file() {
            continue;
        }
        let is_better = best
            .as_ref()
            .map(|(best_n, _)| number > *best_n)
            .unwrap_or(true);
        if is_better {
            best = Some((number, jar));
        }
    }
    best.map(|(_, jar)| jar).ok_or_else(|| {
        format!(
            "no platforms/android-<N>/android.jar found under {}",
            platforms_dir.display()
        )
    })
}

/// Locates an installed Android NDK: `ANDROID_NDK_HOME`/`ANDROID_NDK_ROOT`
/// if set to an existing directory, else the highest-versioned
/// `<sdk>/ndk/<version>` directory with a usable host toolchain.
fn discover_ndk_root(sdk_root: &Path) -> Result<PathBuf, String> {
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Ok(value) = env::var(var) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return Ok(path);
            }
        }
    }
    let ndk_dir = sdk_root.join("ndk");
    let host_bin_suffix = format!("toolchains/llvm/prebuilt/{}/bin", ndk_host_tag());
    pick_highest_version_dir(&ndk_dir, |path| path.join(&host_bin_suffix).is_dir())
        .ok_or_else(|| format!("no usable ndk/<version> found under {}", ndk_dir.display()))
}

/// Extracts the first `prefix"..."` quoted value's inner text, e.g. calling
/// this with `prefix = "android:minSdkVersion=\""` on manifest source text
/// returns the digits between those quotes. A targeted text scan, not a
/// full XML parser, matching this file's existing style (see
/// `find_quoted_name_values` above).
fn find_quoted_attr(haystack: &str, prefix: &str) -> Option<String> {
    let start = haystack.find(prefix)? + prefix.len();
    let rest = &haystack[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Reads `(minSdkVersion, targetSdkVersion)` out of the manifest's
/// `<uses-sdk>` element so this command can never silently drift from what
/// `AndroidManifest.xml` itself declares.
fn parse_uses_sdk(manifest_text: &str) -> Result<(u32, u32), String> {
    let min_sdk = find_quoted_attr(manifest_text, "android:minSdkVersion=\"")
        .ok_or("AndroidManifest.xml: missing android:minSdkVersion")?;
    let target_sdk = find_quoted_attr(manifest_text, "android:targetSdkVersion=\"")
        .ok_or("AndroidManifest.xml: missing android:targetSdkVersion")?;
    let min_sdk = min_sdk
        .parse::<u32>()
        .map_err(|e| format!("android:minSdkVersion={min_sdk:?}: {e}"))?;
    let target_sdk = target_sdk
        .parse::<u32>()
        .map_err(|e| format!("android:targetSdkVersion={target_sdk:?}: {e}"))?;
    Ok((min_sdk, target_sdk))
}

/// Recursively collects every file under `dir` whose extension is exactly
/// `ext`, sorted for deterministic build-command argument order.
fn find_files_with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    collect_files_with_extension(dir, ext, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_files_with_extension(
    dir: &Path,
    ext: &str,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_extension(&path, ext, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
    Ok(())
}

/// Recreates (deletes then creates) each directory in `dirs`, so a rerun
/// never mixes stale outputs from a previous build into a new one.
fn recreate_dirs(dirs: &[&Path]) -> Result<(), String> {
    for dir in dirs {
        if dir.is_dir() {
            fs::remove_dir_all(dir).map_err(|e| format!("clearing {}: {e}", dir.display()))?;
        }
        fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    Ok(())
}

fn cmd_build_apk() -> Result<(), String> {
    let root = repo_root()?;
    let app_dir = root.join(ANDROID_APP_DIR);
    let manifest_path = app_dir.join("AndroidManifest.xml");
    let manifest_text = read_to_string(&manifest_path)?;
    let (min_sdk, target_sdk) = parse_uses_sdk(&manifest_text)?;
    println!("manifest declares minSdkVersion={min_sdk} targetSdkVersion={target_sdk}");

    let sdk_root = discover_sdk_root()?;
    let build_tools = discover_build_tools(&sdk_root)?;
    let platform_jar = discover_platform_jar(&sdk_root)?;
    let ndk_root = discover_ndk_root(&sdk_root)?;
    println!("sdk_root={}", sdk_root.display());
    println!("build_tools={}", build_tools.display());
    println!("platform_jar={}", platform_jar.display());
    println!("ndk_root={}", ndk_root.display());

    let build_dir = root.join("target/android-apk");
    let res_compiled_dir = build_dir.join("res-compiled");
    let gen_dir = build_dir.join("gen");
    let classes_dir = build_dir.join("classes");
    let dex_dir = build_dir.join("dex");
    let staging_dir = build_dir.join("staging");
    fs::create_dir_all(&build_dir).map_err(|e| format!("creating {}: {e}", build_dir.display()))?;
    recreate_dirs(&[
        &res_compiled_dir,
        &gen_dir,
        &classes_dir,
        &dex_dir,
        &staging_dir,
    ])?;

    ensure_rustup_target_installed(&root, ANDROID_RUST_TARGET)?;
    println!("== cross-compiling bleradar-jni for aarch64-linux-android (release) ==");
    let ndk_bin = ndk_root
        .join("toolchains/llvm/prebuilt")
        .join(ndk_host_tag())
        .join("bin");
    let clang = ndk_bin.join(format!("aarch64-linux-android{min_sdk}-clang"));
    if !clang.is_file() {
        return Err(format!(
            "NDK clang for API {min_sdk} not found: {}",
            clang.display()
        ));
    }
    let llvm_ar = ndk_bin.join("llvm-ar");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args([
                "build",
                "--release",
                "--locked",
                "--target",
                ANDROID_RUST_TARGET,
                "-p",
                "bleradar-jni",
            ])
            .env("CC_aarch64_linux_android", &clang)
            .env("AR_aarch64_linux_android", &llvm_ar)
            .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", &clang)
            .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_AR", &llvm_ar);
        c
    })?;
    let so_path = root
        .join("target/aarch64-linux-android/release")
        .join(NATIVE_LIB_FILE_NAME);
    if !so_path.is_file() {
        return Err(format!(
            "expected cross-compiled native library missing: {}",
            so_path.display()
        ));
    }

    println!("== aapt2 compile ==");
    run_status({
        let mut c = Command::new(build_tools.join("aapt2"));
        c.arg("compile")
            .arg("--dir")
            .arg(app_dir.join("res"))
            .arg("-o")
            .arg(&res_compiled_dir);
        c
    })?;
    let flat_files = find_files_with_extension(&res_compiled_dir, "flat")?;
    if flat_files.is_empty() {
        return Err(format!(
            "aapt2 compile produced no .flat resource files under {}",
            res_compiled_dir.display()
        ));
    }

    println!("== aapt2 link ==");
    let base_apk = build_dir.join("base.apk");
    run_status({
        let mut c = Command::new(build_tools.join("aapt2"));
        c.arg("link")
            .arg("-I")
            .arg(&platform_jar)
            .arg("--manifest")
            .arg(&manifest_path)
            .arg("-o")
            .arg(&base_apk)
            .arg("--java")
            .arg(&gen_dir)
            .args(["--min-sdk-version", &min_sdk.to_string()])
            .args(["--target-sdk-version", &target_sdk.to_string()])
            .args(["--version-code", "1", "--version-name", "1.0.0"])
            .args(["-0", "arsc"])
            .arg("--auto-add-overlay");
        for flat in &flat_files {
            c.arg("-R").arg(flat);
        }
        c
    })?;
    if !base_apk.is_file() {
        return Err(format!("aapt2 link did not produce {}", base_apk.display()));
    }

    println!("== javac ==");
    let mut java_sources = find_files_with_extension(&app_dir.join("java"), "java")?;
    java_sources.extend(find_files_with_extension(&gen_dir, "java")?);
    if java_sources.is_empty() {
        return Err("no .java sources found to compile".to_string());
    }
    run_status({
        let mut c = Command::new("javac");
        c.args(["-source", "8", "-target", "8", "-d"])
            .arg(&classes_dir)
            .arg("-classpath")
            .arg(&platform_jar);
        c.args(&java_sources);
        c
    })?;

    println!("== d8 ==");
    let class_files = find_files_with_extension(&classes_dir, "class")?;
    if class_files.is_empty() {
        return Err("javac produced no .class files".to_string());
    }
    run_status({
        let mut c = Command::new(build_tools.join("d8"));
        c.args(["--release", "--min-api"])
            .arg(min_sdk.to_string())
            .arg("--lib")
            .arg(&platform_jar)
            .arg("--output")
            .arg(&dex_dir);
        c.args(&class_files);
        c
    })?;
    let classes_dex = dex_dir.join("classes.dex");
    if !classes_dex.is_file() {
        return Err(format!("d8 did not produce {}", classes_dex.display()));
    }

    println!("== assembling APK (native lib + resources.arsc stored uncompressed) ==");
    run_status({
        let mut c = Command::new("unzip");
        c.args(["-q", "-o"])
            .arg(&base_apk)
            .arg("-d")
            .arg(&staging_dir);
        c
    })?;
    let lib_dir = staging_dir.join("lib/arm64-v8a");
    fs::create_dir_all(&lib_dir).map_err(|e| format!("creating {}: {e}", lib_dir.display()))?;
    fs::copy(&so_path, lib_dir.join(NATIVE_LIB_FILE_NAME))
        .map_err(|e| format!("copying {} into staging: {e}", so_path.display()))?;
    fs::copy(&classes_dex, staging_dir.join("classes.dex"))
        .map_err(|e| format!("copying {} into staging: {e}", classes_dex.display()))?;

    let unaligned_apk = build_dir.join("unaligned.apk");
    if unaligned_apk.is_file() {
        fs::remove_file(&unaligned_apk)
            .map_err(|e| format!("removing stale {}: {e}", unaligned_apk.display()))?;
    }
    run_status({
        let mut c = Command::new("zip");
        c.current_dir(&staging_dir).args(["-r", "-X", "-q"]);
        c.arg(&unaligned_apk);
        c.args([".", "-x", "lib/*", "-x", "resources.arsc"]);
        c
    })?;
    run_status({
        let mut c = Command::new("zip");
        c.current_dir(&staging_dir).args(["-0", "-X", "-q"]);
        c.arg(&unaligned_apk);
        c.args(["lib/arm64-v8a/libbleradar_jni.so", "resources.arsc"]);
        c
    })?;

    println!("== zipalign ==");
    let aligned_apk = build_dir.join("aligned.apk");
    if aligned_apk.is_file() {
        fs::remove_file(&aligned_apk)
            .map_err(|e| format!("removing stale {}: {e}", aligned_apk.display()))?;
    }
    run_status({
        let mut c = Command::new(build_tools.join("zipalign"));
        c.args(["-p", "-f", "4"])
            .arg(&unaligned_apk)
            .arg(&aligned_apk);
        c
    })?;

    println!("== signing ==");
    let keystore_path = build_dir.join("debug.keystore");
    if !keystore_path.is_file() {
        run_status({
            let mut c = Command::new("keytool");
            c.args(["-genkeypair", "-v", "-keystore"])
                .arg(&keystore_path)
                .args(["-storepass", DEBUG_KEYSTORE_PASSWORD])
                .args(["-keypass", DEBUG_KEYSTORE_PASSWORD])
                .args(["-alias", DEBUG_KEYSTORE_ALIAS])
                .args(["-keyalg", "RSA", "-keysize", "2048", "-validity", "10950"])
                .args(["-dname", "CN=Android Debug,O=Android,C=US"]);
            c
        })?;
    }
    let signed_apk = build_dir.join("signed.apk");
    run_status({
        let mut c = Command::new(build_tools.join("apksigner"));
        c.arg("sign")
            .arg("--ks")
            .arg(&keystore_path)
            .args(["--ks-pass", &format!("pass:{DEBUG_KEYSTORE_PASSWORD}")])
            .args(["--key-pass", &format!("pass:{DEBUG_KEYSTORE_PASSWORD}")])
            .args(["--ks-key-alias", DEBUG_KEYSTORE_ALIAS])
            .args(["--min-sdk-version", &min_sdk.to_string()])
            .args(["--v1-signing-enabled", "false"])
            .args(["--v2-signing-enabled", "true"])
            .args(["--v3-signing-enabled", "true"])
            .arg("--out")
            .arg(&signed_apk)
            .arg(&aligned_apk);
        // Pin signing-time related inputs when the tool honors them so
        // repeated builds of an unchanged payload do not churn the committed
        // APK hash solely due to wall-clock timestamps.
        c.env("SOURCE_DATE_EPOCH", "1700000000");
        c.env("TZ", "UTC");
        c
    })?;

    println!("== verifying ==");
    run_status({
        let mut c = Command::new(build_tools.join("apksigner"));
        c.args(["verify", "--print-certs"]).arg(&signed_apk);
        c
    })?;
    run_status({
        let mut c = Command::new(build_tools.join("zipalign"));
        c.args(["-c", "-v", "4"]).arg(&signed_apk);
        c
    })?;

    let output_path = root.join(APK_OUTPUT_NAME);
    fs::copy(&signed_apk, &output_path)
        .map_err(|e| format!("copying final APK to {}: {e}", output_path.display()))?;
    let size = fs::metadata(&output_path)
        .map_err(|e| format!("stat {}: {e}", output_path.display()))?
        .len();
    println!("== done: {} ({size} bytes) ==", output_path.display());
    Ok(())
}

fn cmd_verify_jni_live() -> Result<(), String> {
    let root = repo_root()?;
    let java_path = root.join(NATIVE_RADAR_JAVA_PATH);
    let java_source = read_to_string(&java_path)?;
    let expected_abi_version = find_java_static_final_int(&java_source, "EXPECTED_ABI_VERSION")?;

    println!("== building host bleradar-jni ==");
    let host_lib = build_host_jni_library(&root)?;

    println!("== jni export contract (NativeRadar.java ↔ host library) ==");
    let contract = verify_jni_export_contract(&java_source, &host_lib)?;
    let expected_native_count = contract.native_methods.len();

    let temp_dir = xtask_temp_dir("verify-jni-live");
    let src_dir = temp_dir.join("src");
    let package_dir = src_dir.join("com/hse/bleradar");
    let classes_dir = temp_dir.join("classes");
    let empty_library_dir = temp_dir.join("empty-library-path");
    recreate_dirs(&[&package_dir, &classes_dir, &empty_library_dir])?;
    fs::copy(&java_path, package_dir.join("NativeRadar.java"))
        .map_err(|e| format!("copying {} into live verifier: {e}", java_path.display()))?;
    fs::write(
        src_dir.join("JniSmoke.java"),
        jni_smoke_java_source(expected_abi_version, expected_native_count),
    )
    .map_err(|e| format!("writing JniSmoke.java: {e}"))?;

    println!("== javac NativeRadar.java + JniSmoke.java ==");
    run_status({
        let mut c = Command::new("javac");
        c.arg("-d")
            .arg(&classes_dir)
            .arg(package_dir.join("NativeRadar.java"))
            .arg(src_dir.join("JniSmoke.java"));
        c
    })?;

    println!("== java failure-path falsification ==");
    run_status({
        let mut c = Command::new("java");
        c.arg(format!(
            "-Djava.library.path={}",
            empty_library_dir.display()
        ))
        .arg("-cp")
        .arg(&classes_dir)
        .arg("JniSmoke")
        .arg("failure");
        c
    })?;

    let library_dir = host_lib.parent().ok_or_else(|| {
        format!(
            "host library has no parent directory: {}",
            host_lib.display()
        )
    })?;
    println!("== java success-path proof ==");
    run_status({
        let mut c = Command::new("java");
        c.arg(format!("-Djava.library.path={}", library_dir.display()))
            .arg("-cp")
            .arg(&classes_dir)
            .arg("JniSmoke")
            .arg("success");
        c
    })?;

    Ok(())
}

fn cmd_verify_android_live() -> Result<(), String> {
    let root = repo_root()?;

    println!("== live JNI proof ==");
    cmd_verify_jni_live()?;

    println!("== APK build proof ==");
    cmd_build_apk()?;

    let apk_path = root.join(APK_OUTPUT_NAME);
    let apk_entries = zip_reader::entry_names(&read_bytes(&apk_path)?)
        .map_err(|e| format!("parsing {}: {e}", apk_path.display()))?;
    require_expected_members(&apk_entries, REQUIRED_APK_ENTRIES, "APK entry set")?;

    let dex_path = root.join("target/android-apk/dex/classes.dex");
    let dex_classes = dex::class_names(&read_bytes(&dex_path)?)
        .map_err(|e| format!("parsing {}: {e}", dex_path.display()))?;
    require_expected_members(&dex_classes, REQUIRED_DEX_CLASSES, "DEX class set")?;

    println!("== jni export contract (NativeRadar.java ↔ cross-compiled library) ==");
    let native_lib_path = root
        .join("target/aarch64-linux-android/release")
        .join(NATIVE_LIB_FILE_NAME);
    let java_source = read_to_string(&root.join(NATIVE_RADAR_JAVA_PATH))?;
    verify_jni_export_contract(&java_source, &native_lib_path)?;

    println!("== verify-android-live complete ==");
    Ok(())
}

/// Executes the immutable v0.3.0 native oracle under `qemu-aarch64` against a
/// real Android Bionic runtime and checks that its WiFi channel<->frequency
/// outputs still match the committed executed-oracle ground truth
/// (`EXECUTED_VECTORS_PATH`). See `docs/ORACLE_DIFFERENTIAL.md`.
///
/// This is a live command (like `verify-android-live`): it needs an NDK,
/// `qemu-aarch64`, and a Bionic runtime, so it is not part of `gates`. The
/// committed vectors it drift-checks are what the ordinary CI test
/// `oracle_differential.rs` replays against the safe-Rust reconstruction.
fn cmd_oracle_differential() -> Result<(), String> {
    let root = repo_root()?;
    let sdk = discover_sdk_root()?;
    let ndk = discover_ndk_root(&sdk)?;
    let clang = ndk_aarch64_clang(&ndk)?;
    let qemu = locate_qemu_aarch64()?;
    let sysroot = prepare_bionic_sysroot(&sdk)?;

    let workdir = xtask_temp_dir("oracle-differential");
    recreate_dirs(&[&workdir])?;

    println!("== extract immutable oracle .so from migration archive ==");
    let oracle_so = extract_and_pin_oracle_so(&root, &workdir)?;
    println!("   oracle .so sha256 = {ORACLE_SO_SHA256} (verified)");

    // Place the oracle where the Bionic linker will resolve it at run time.
    let lib64 = sysroot.join("system/lib64");
    fs::copy(&oracle_so, lib64.join("libbleradar_core.so"))
        .map_err(|e| format!("copying oracle into the Bionic sysroot: {e}"))?;

    println!("== compile aarch64 differential harness (NDK) ==");
    let harness_c = workdir.join("oracle_harness.c");
    fs::write(&harness_c, ORACLE_HARNESS_C)
        .map_err(|e| format!("writing {}: {e}", harness_c.display()))?;
    let harness_bin = workdir.join("oracle_harness");
    run_status({
        let mut c = Command::new(&clang);
        c.arg(&harness_c)
            .arg("-o")
            .arg(&harness_bin)
            .arg("-L")
            .arg(&lib64)
            .arg("-lbleradar_core")
            .arg("-Wl,--allow-shlib-undefined");
        c
    })?;

    println!("== execute the oracle under qemu-aarch64 ==");
    let produced = run_capture({
        let mut c = Command::new(&qemu);
        c.arg("-L")
            .arg(&sysroot)
            .arg("-E")
            .arg("LD_LIBRARY_PATH=/system/lib64")
            .arg(&harness_bin);
        c
    })?;

    println!("== drift-check against {EXECUTED_VECTORS_PATH} ==");
    let committed = read_to_string(&root.join(EXECUTED_VECTORS_PATH))?;
    let committed_rows: Vec<&str> = committed
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let produced_rows: Vec<&str> = produced
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect();

    if produced_rows != committed_rows {
        let mut detail = String::from("(row counts match but contents differ)");
        for index in 0..committed_rows.len().max(produced_rows.len()) {
            let committed_row = committed_rows.get(index).copied().unwrap_or("<missing>");
            let produced_row = produced_rows.get(index).copied().unwrap_or("<missing>");
            if committed_row != produced_row {
                detail = format!(
                    "first drift at data row {}: committed {committed_row:?} vs executed {produced_row:?}",
                    index + 1
                );
                break;
            }
        }
        return Err(format!(
            "executed-oracle output drifted from {EXECUTED_VECTORS_PATH} \
             ({} committed vs {} executed rows); {detail}. If the sweep changed \
             intentionally, regenerate the committed vectors from this output.",
            committed_rows.len(),
            produced_rows.len()
        ));
    }

    println!(
        "oracle-differential: {} executed-oracle rows match the committed vectors",
        produced_rows.len()
    );
    Ok(())
}

/// Finds `qemu-aarch64-static` (preferred) or `qemu-aarch64`, honoring an
/// explicit `QEMU_AARCH64` override.
fn locate_qemu_aarch64() -> Result<PathBuf, String> {
    if let Ok(value) = env::var("QEMU_AARCH64") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("QEMU_AARCH64={} is not a file", path.display()));
    }
    which_in_path("qemu-aarch64-static")
        .or_else(|| which_in_path("qemu-aarch64"))
        .ok_or_else(|| {
            "qemu-aarch64 not found: install qemu-user-static, or set QEMU_AARCH64 (see docs/ORACLE_DIFFERENTIAL.md)"
                .to_string()
        })
}

/// The NDK's aarch64 API-24 Clang driver for the current host.
fn ndk_aarch64_clang(ndk_root: &Path) -> Result<PathBuf, String> {
    let clang = ndk_root
        .join("toolchains/llvm/prebuilt")
        .join(ndk_host_tag())
        .join("bin")
        .join("aarch64-linux-android24-clang");
    if clang.is_file() {
        Ok(clang)
    } else {
        Err(format!(
            "NDK aarch64 clang not found at {}",
            clang.display()
        ))
    }
}

/// Extracts `oracle/libbleradar_core.so` from the committed migration archive
/// into `workdir` (via `jar`, part of the JDK the live tier already needs) and
/// refuses to proceed unless it matches the pinned immutable-oracle SHA-256.
fn extract_and_pin_oracle_so(root: &Path, workdir: &Path) -> Result<PathBuf, String> {
    let zip = root.join(MIGRATION_ZIP_NAME);
    if !zip.is_file() {
        return Err(format!("missing migration archive: {}", zip.display()));
    }
    let jar = locate_jar()?;
    run_status({
        let mut c = Command::new(&jar);
        c.current_dir(workdir)
            .arg("xf")
            .arg(&zip)
            .arg(ORACLE_SO_ZIP_ENTRY);
        c
    })?;
    let extracted = workdir.join(ORACLE_SO_ZIP_ENTRY);
    if !extracted.is_file() {
        return Err(format!(
            "jar did not extract {ORACLE_SO_ZIP_ENTRY} from {}",
            zip.display()
        ));
    }
    let actual = sha256::to_hex(&sha256::sha256(&read_bytes(&extracted)?));
    if actual != ORACLE_SO_SHA256 {
        return Err(format!(
            "extracted oracle .so sha256 {actual} != pinned {ORACLE_SO_SHA256}; the immutable oracle must never change"
        ));
    }
    Ok(extracted)
}

/// Locates `jar` via `JAVA_HOME` or `PATH`.
fn locate_jar() -> Result<PathBuf, String> {
    if let Ok(home) = env::var("JAVA_HOME") {
        let jar = PathBuf::from(home).join("bin").join("jar");
        if jar.is_file() {
            return Ok(jar);
        }
    }
    which_in_path("jar")
        .ok_or_else(|| "jar not found (a JDK is required to unpack the oracle)".to_string())
}

/// Prepares an aarch64 Bionic sysroot (`system/bin/linker64` +
/// `system/lib64/lib{c,m,dl,c++}.so`). Uses `BIONIC_SYSROOT` when it points at a
/// prepared tree, otherwise extracts one from an installed arm64 system image
/// with `debugfs` (no root or loopback mount needed).
fn prepare_bionic_sysroot(sdk_root: &Path) -> Result<PathBuf, String> {
    if let Ok(value) = env::var("BIONIC_SYSROOT") {
        let root = PathBuf::from(value);
        if root.join("system/bin/linker64").is_file() && root.join("system/lib64/libc.so").is_file()
        {
            return Ok(root);
        }
        return Err(format!(
            "BIONIC_SYSROOT={} lacks system/bin/linker64 and system/lib64/libc.so",
            root.display()
        ));
    }

    let image = discover_arm64_system_image(sdk_root)?;
    let debugfs = which_in_path("debugfs").ok_or_else(|| {
        "debugfs not found (install e2fsprogs) and BIONIC_SYSROOT unset; see docs/ORACLE_DIFFERENTIAL.md"
            .to_string()
    })?;

    let sysroot = xtask_temp_dir("oracle-bionic-sysroot");
    let bin = sysroot.join("system/bin");
    let lib64 = sysroot.join("system/lib64");
    recreate_dirs(&[&bin, &lib64])?;

    debugfs_dump(&debugfs, &image, "/bin/linker64", &bin.join("linker64"))?;
    let mut linker = fs::metadata(bin.join("linker64"))
        .map_err(|e| format!("stat linker64: {e}"))?
        .permissions();
    linker.set_mode(0o755);
    fs::set_permissions(bin.join("linker64"), linker)
        .map_err(|e| format!("chmod linker64: {e}"))?;

    for lib in ["libc.so", "libm.so", "libdl.so", "libc++.so"] {
        debugfs_dump(&debugfs, &image, &format!("/lib64/{lib}"), &lib64.join(lib))?;
    }
    Ok(sysroot)
}

/// Finds an installed `system-images/<api>/<tag>/arm64-v8a/system.img`.
fn discover_arm64_system_image(sdk_root: &Path) -> Result<PathBuf, String> {
    let base = sdk_root.join("system-images");
    let apis = fs::read_dir(&base).map_err(|e| {
        format!(
            "no arm64 system image under {} ({e}); install one with \
             sdkmanager \"system-images;android-24;default;arm64-v8a\" or set BIONIC_SYSROOT",
            base.display()
        )
    })?;
    for api in apis.flatten() {
        let Ok(tags) = fs::read_dir(api.path()) else {
            continue;
        };
        for tag in tags.flatten() {
            let image = tag.path().join("arm64-v8a").join("system.img");
            if image.is_file() {
                return Ok(image);
            }
        }
    }
    Err(format!(
        "no arm64-v8a/system.img under {}; install one with \
         sdkmanager \"system-images;android-24;default;arm64-v8a\" or set BIONIC_SYSROOT",
        base.display()
    ))
}

/// Extracts one file from an ext4 image with `debugfs` (no mount, no root).
fn debugfs_dump(debugfs: &Path, image: &Path, source: &str, dest: &Path) -> Result<(), String> {
    run_status({
        let mut c = Command::new(debugfs);
        c.arg("-R")
            .arg(format!("dump {source} {}", dest.display()))
            .arg(image);
        c
    })?;
    let extracted = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if extracted == 0 {
        return Err(format!(
            "debugfs could not extract {source} from {}",
            image.display()
        ));
    }
    Ok(())
}

/// Returns the first `PATH` entry containing an executable named `name`.
fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Spawns `cmd`, requiring success, and returns its captured stdout.
fn run_capture(mut cmd: Command) -> Result<String, String> {
    let program = format!("{cmd:?}");
    let output = cmd
        .output()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|e| format!("{program} produced non-UTF-8 output: {e}"))
}

fn run_status(mut cmd: Command) -> Result<(), String> {
    let program = format!("{cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

fn cmd_audit() -> Result<(), String> {
    let root = repo_root()?;
    let db_path = root.join(AUDIT_DB_PATH);
    if !db_path.is_dir() {
        return Err(format!(
            "vendored advisory database missing: {}",
            db_path.display()
        ));
    }
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root)
        .args(["audit", "--db"])
        .arg(&db_path)
        .args(["--no-fetch", "--stale"]);
    run_status(cmd)
}

fn cmd_deny() -> Result<(), String> {
    let root = repo_root()?;
    vendor::materialize_advisory_db(&root)?;
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&root).args(["deny", "--offline", "check"]);
    run_status(cmd)
}

fn cmd_gates() -> Result<(), String> {
    let root = repo_root()?;

    println!("== fmt ==");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root).args(["fmt", "--all", "--check"]);
        c
    })?;

    println!("== clippy ==");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root).args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ]);
        c
    })?;

    println!("== build ==");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["build", "--workspace", "--locked"]);
        c
    })?;

    println!("== jni export contract (NativeRadar.java ↔ host bleradar-jni) ==");
    cmd_check_jni_contract(&[])?;

    println!("== test ==");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["test", "--workspace", "--locked"]);
        c
    })?;

    println!("== doc ==");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["doc", "--workspace", "--no-deps", "--locked"])
            .env("RUSTDOCFLAGS", "-D warnings");
        c
    })?;

    println!("== xtask fmt/clippy/build/test (own isolated workspace) ==");
    let xtask_manifest = root.join("xtask/Cargo.toml");
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["fmt", "--manifest-path"])
            .arg(&xtask_manifest)
            .args(["--all", "--check"]);
        c
    })?;
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["clippy", "--manifest-path"])
            .arg(&xtask_manifest)
            .args(["--all-targets", "--locked", "--", "-D", "warnings"]);
        c
    })?;
    run_status({
        let mut c = Command::new("cargo");
        c.current_dir(&root)
            .args(["test", "--manifest-path"])
            .arg(&xtask_manifest)
            .arg("--locked");
        c
    })?;

    println!("== parity report drift ==");
    cmd_parity_report()?;
    run_status({
        let mut c = Command::new("git");
        c.current_dir(&root)
            .args(["diff", "--exit-code", "docs/PARITY_COVERAGE.md"]);
        c
    })?;

    println!("== dependency policy ==");
    cmd_check_dependency_policy()?;

    println!("== oracle integrity ==");
    cmd_check_oracle_integrity()?;

    println!("== cargo audit (offline, vendored db) ==");
    cmd_audit()?;

    println!("== cargo deny (offline, vendored db) ==");
    cmd_deny()?;

    println!("== all gates green ==");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_prefixed_runs_captures_the_run_after_a_single_prefix() {
        let out = find_prefixed_runs("UNIFFI_META_BLERADAR_CORE_FUNC_HAVERSINE_M", "FUNC_");
        assert_eq!(out, vec!["HAVERSINE_M".to_string()]);
    }

    #[test]
    fn find_prefixed_runs_finds_multiple_non_overlapping_matches_in_order() {
        let out = find_prefixed_runs("PFX_ONE garbage PFX_TWO_3 more PFX_4", "PFX_");
        assert_eq!(
            out,
            vec!["ONE".to_string(), "TWO_3".to_string(), "4".to_string()]
        );
    }

    #[test]
    fn find_prefixed_runs_returns_empty_when_prefix_never_occurs() {
        assert!(find_prefixed_runs("no prefixes here", "FUNC_").is_empty());
    }

    #[test]
    fn find_prefixed_runs_returns_empty_on_empty_haystack() {
        assert!(find_prefixed_runs("", "FUNC_").is_empty());
    }

    #[test]
    fn find_prefixed_runs_ignores_a_prefix_with_no_run_characters_after_it() {
        // A trailing prefix with nothing (or only non-run characters) after
        // it is not a match, mirroring `re.findall`'s `+` requiring at least
        // one captured character.
        assert!(find_prefixed_runs("FUNC_", "FUNC_").is_empty());
        assert!(find_prefixed_runs("FUNC_!not_a_run", "FUNC_").is_empty());
    }

    #[test]
    fn find_prefixed_runs_resumes_one_byte_later_after_a_zero_length_run() {
        // The first "ABC_" is immediately followed by '!' (not a run
        // character), so it is not a match; the search must resume one byte
        // past the failed match — not past the whole prefix — and go on to
        // find the second, real occurrence.
        let out = find_prefixed_runs("ABC_!ABC_XYZ", "ABC_");
        assert_eq!(out, vec!["XYZ".to_string()]);
    }

    #[test]
    fn find_quoted_name_values_extracts_a_single_value() {
        assert_eq!(
            find_quoted_name_values(r#"name: "bearing_deg","#),
            vec!["bearing_deg".to_string()]
        );
    }

    #[test]
    fn find_quoted_name_values_extracts_multiple_values_in_original_order() {
        let haystack = r#"name: "haversine_m", other: 1, name: "bearing_deg","#;
        assert_eq!(
            find_quoted_name_values(haystack),
            vec!["haversine_m".to_string(), "bearing_deg".to_string()]
        );
    }

    #[test]
    fn find_quoted_name_values_returns_empty_when_never_present() {
        assert!(find_quoted_name_values("no matches in here").is_empty());
    }

    #[test]
    fn find_quoted_name_values_skips_empty_quotes_and_keeps_scanning() {
        // `name: ""` captures zero characters, which is not a match; the
        // search must still find the later, non-empty occurrence.
        let out = find_quoted_name_values(r#"name: "" name: "bar""#);
        assert_eq!(out, vec!["bar".to_string()]);
    }

    #[test]
    fn find_quoted_name_values_stops_at_an_unterminated_quote() {
        // No closing quote after the second prefix: nothing further can be
        // extracted, but the earlier, well-formed match is still returned.
        let out = find_quoted_name_values(r#"name: "first", name: "unterminated"#);
        assert_eq!(out, vec!["first".to_string()]);
    }

    #[test]
    fn array_const_body_extracts_only_the_named_initializer() {
        let source = "pub const FIRST: [Item; 1] = [\nItem::A,\n];\n\
                      pub const SECOND: &[Item] = &[\nItem::B,\n];\n";
        assert_eq!(
            array_const_body(source, "pub const SECOND"),
            Ok("\nItem::B,")
        );
    }

    #[test]
    fn array_const_body_reports_missing_or_unterminated_initializers() {
        assert_eq!(
            array_const_body("pub const OTHER: [Item; 0] = [\n];", "pub const WANTED"),
            Err("missing declaration `pub const WANTED`".to_string())
        );
        assert_eq!(
            array_const_body("pub const WANTED: [Item; 1] = [Item::A", "pub const WANTED"),
            Err("unterminated initializer for `pub const WANTED`".to_string())
        );
    }

    #[test]
    fn variant_count_matches_only_the_requested_variant() {
        let source = "Kind::One Kind::Two Other::One Kind::One";
        assert_eq!(variant_count(source, "Kind", "One"), 2);
        assert_eq!(variant_count(source, "Kind", "Two"), 1);
        assert_eq!(variant_count(source, "Other", "One"), 1);
    }

    #[test]
    fn macro_argument_count_handles_single_and_multiline_invocations() {
        let source = "item!(\"one\", Function, Wanted, Evidence),\n\
                      item!(\n\"two\",\nMethod,\nOther,\nEvidence\n),\n\
                      item!(\"three\", Function, WantedExtra, Evidence),";
        assert_eq!(
            macro_argument_values(source, "item!(", 0),
            vec![
                "\"one\"".to_string(),
                "\"two\"".to_string(),
                "\"three\"".to_string()
            ]
        );
        assert_eq!(macro_argument_count(source, "item!(", 2, "Wanted"), 1);
        assert_eq!(macro_argument_count(source, "item!(", 2, "Other"), 1);
        assert_eq!(macro_argument_count(source, "item!(", 2, "WantedExtra"), 1);
    }

    #[test]
    fn contract_census_comparison_preserves_kind_and_cross_kind_names() {
        let abi = "UNIFFI_META_BLERADAR_CORE_FUNC_SHARED\n\
                   UNIFFI_META_BLERADAR_CORE_METHOD_SHARED\n\
                   UNIFFI_META_BLERADAR_CORE_FUNC_ONLY_FUNCTION\n\
                   UNIFFI_META_BLERADAR_CORE_METHOD_ONLY_METHOD\n";
        let expected = abi_contract_keys(abi);
        let correct = runtime_contract_keys(
            "runtime_contract!(\"shared\", Function, Unknown, StaticReachability),\n\
             runtime_contract!(\"shared\", Method, Unknown, StaticReachability),\n\
             runtime_contract!(\"only_function\", Function, Unknown, StaticReachability),\n\
             runtime_contract!(\"only_method\", Method, Unknown, StaticReachability),",
        )
        .unwrap();
        let swapped = runtime_contract_keys(
            "runtime_contract!(\"shared\", Function, Unknown, StaticReachability),\n\
             runtime_contract!(\"shared\", Method, Unknown, StaticReachability),\n\
             runtime_contract!(\"only_function\", Method, Unknown, StaticReachability),\n\
             runtime_contract!(\"only_method\", Function, Unknown, StaticReachability),",
        )
        .unwrap();

        assert_eq!(dedup_sorted(correct), expected);
        assert_ne!(dedup_sorted(swapped), expected);
    }

    #[test]
    fn dedup_sorted_sorts_and_removes_duplicates() {
        let out = dedup_sorted(vec!["b".to_string(), "a".to_string(), "b".to_string()]);
        assert_eq!(out, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn dedup_sorted_handles_empty_input() {
        assert!(dedup_sorted(Vec::new()).is_empty());
    }

    #[test]
    fn find_sha256_after_label_extracts_a_valid_hash() {
        let hash = "0123456789abcdef".repeat(4);
        let text = format!("Original APK SHA-256: {hash}\n");
        assert_eq!(
            find_sha256_after_label(&text, "Original APK SHA-256:"),
            Some(hash)
        );
    }

    #[test]
    fn find_sha256_after_label_tolerates_varying_whitespace() {
        let hash = "0123456789abcdef".repeat(4);
        let text = format!("Label:\n   {hash}");
        assert_eq!(find_sha256_after_label(&text, "Label:"), Some(hash));
    }

    #[test]
    fn find_sha256_after_label_takes_only_the_first_64_hex_characters() {
        let hash = "0123456789abcdef".repeat(4);
        let text = format!("Label: {hash}ff extra");
        assert_eq!(find_sha256_after_label(&text, "Label:"), Some(hash));
    }

    #[test]
    fn find_sha256_after_label_rejects_a_short_hex_run() {
        let short = "0123456789abcdef".repeat(3); // 48 hex chars, not 64
        let text = format!("Label: {short}");
        assert_eq!(find_sha256_after_label(&text, "Label:"), None);
    }

    #[test]
    fn find_sha256_after_label_rejects_uppercase_hex() {
        let hash = "0123456789ABCDEF".repeat(4);
        let text = format!("Label: {hash}");
        assert_eq!(find_sha256_after_label(&text, "Label:"), None);
    }

    #[test]
    fn find_sha256_after_label_returns_none_when_label_is_absent() {
        let hash = "0123456789abcdef".repeat(4);
        let text = format!("Different label: {hash}");
        assert_eq!(find_sha256_after_label(&text, "Label:"), None);
    }

    #[test]
    fn parse_lockfile_package_names_extracts_every_name_line_in_order() {
        let text = "[[package]]\nname = \"bleradar-core\"\nversion = \"0.4.2\"\n\n\
                     [[package]]\nname = \"bleradar-compat\"\n";
        assert_eq!(
            parse_lockfile_package_names(text),
            vec!["bleradar-core".to_string(), "bleradar-compat".to_string()]
        );
    }

    #[test]
    fn parse_lockfile_package_names_returns_empty_on_unparsable_lockfile() {
        assert!(parse_lockfile_package_names("this is not a Cargo.lock at all").is_empty());
        assert!(parse_lockfile_package_names("").is_empty());
    }

    #[test]
    fn evaluate_dependency_policy_reports_empty_lockfile() {
        let names: Vec<String> = Vec::new();
        assert_eq!(
            evaluate_dependency_policy(&names, &ALLOWED),
            DependencyPolicyOutcome::EmptyLockfile
        );
    }

    #[test]
    fn evaluate_dependency_policy_reports_sorted_deduplicated_foreign_crates() {
        // "serde" appears twice and out of sorted order, alongside one
        // in-policy crate: the outcome must name only the foreign crates,
        // sorted, deduplicated.
        let names = vec![
            "bleradar-core".to_string(),
            "serde".to_string(),
            "libc".to_string(),
            "serde".to_string(),
        ];
        assert_eq!(
            evaluate_dependency_policy(&names, &ALLOWED),
            DependencyPolicyOutcome::ForeignCrates(vec!["libc".to_string(), "serde".to_string()])
        );
    }

    #[test]
    fn evaluate_dependency_policy_reports_compliant_when_every_name_is_allowed() {
        let names = vec!["bleradar-compat".to_string(), "bleradar-core".to_string()];
        assert_eq!(
            evaluate_dependency_policy(&names, &ALLOWED),
            DependencyPolicyOutcome::Compliant(vec![
                "bleradar-compat".to_string(),
                "bleradar-core".to_string(),
                "bleradar-jni".to_string(),
            ])
        );
    }

    #[test]
    fn evaluate_oracle_hash_fails_when_expected_is_unparsable() {
        assert_eq!(
            evaluate_oracle_hash(
                None,
                Some("deadbeef"),
                "could not parse expected hash".to_string(),
                "missing".to_string(),
                "label",
            ),
            Err("could not parse expected hash".to_string())
        );
    }

    #[test]
    fn evaluate_oracle_hash_fails_when_file_is_missing() {
        assert_eq!(
            evaluate_oracle_hash(
                Some("abc123"),
                None,
                "unparsable".to_string(),
                "missing oracle file: x.apk".to_string(),
                "x.apk",
            ),
            Err("missing oracle file: x.apk".to_string())
        );
    }

    #[test]
    fn evaluate_oracle_hash_fails_on_mismatch_naming_both_hashes() {
        assert_eq!(
            evaluate_oracle_hash(
                Some("expected_hash"),
                Some("actual_hash"),
                String::new(),
                String::new(),
                "oracle.apk",
            ),
            Err("oracle.apk: expected expected_hash, observed actual_hash".to_string())
        );
    }

    #[test]
    fn evaluate_oracle_hash_passes_when_hashes_match() {
        assert_eq!(
            evaluate_oracle_hash(
                Some("same_hash"),
                Some("same_hash"),
                String::new(),
                String::new(),
                "oracle.apk",
            ),
            Ok(())
        );
    }

    /// Builds a fresh, uniquely named scratch directory under the OS temp
    /// directory for a single test, so parallel test threads and repeated
    /// CI runs never share or collide on the same path.
    fn unique_temp_dir(label: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "xtask-repo-root-test-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn repo_root_from_finds_a_directory_that_already_has_both_markers() {
        let dir = unique_temp_dir("finds_a_directory_that_already_has_both_markers");
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("Cargo.toml"), "").unwrap();
        fs::write(dir.join("docs/NATIVE_ABI.txt"), "").unwrap();

        assert_eq!(repo_root_from(dir.clone()), Ok(dir.clone()));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn repo_root_from_walks_upward_through_marker_free_subdirectories() {
        let root = unique_temp_dir("walks_upward_through_marker_free_subdirectories");
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();
        fs::write(root.join("docs/NATIVE_ABI.txt"), "").unwrap();
        let nested = root.join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(repo_root_from(nested), Ok(root.clone()));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn repo_root_from_rejects_a_tree_with_no_qualifying_ancestor() {
        // Only `Cargo.toml` exists in this disposable tree, never paired
        // with `docs/NATIVE_ABI.txt`: the walk must exhaust every ancestor
        // up to the filesystem root and return `Err`, not falsely match an
        // unrelated ancestor `Cargo.toml`.
        let root = unique_temp_dir("rejects_a_tree_with_no_qualifying_ancestor");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();

        assert_eq!(
            repo_root_from(root.clone()),
            Err(
                "could not locate repository root (no ancestor has both Cargo.toml and docs/NATIVE_ABI.txt)"
                    .to_string()
            )
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn parse_plain_version_accepts_dot_separated_integers() {
        assert_eq!(parse_plain_version("34.0.0"), Some(vec![34, 0, 0]));
        assert_eq!(parse_plain_version("27"), Some(vec![27]));
        assert_eq!(
            parse_plain_version("27.3.13750724"),
            Some(vec![27, 3, 13750724])
        );
    }

    #[test]
    fn parse_plain_version_rejects_non_numeric_or_empty_input() {
        assert_eq!(parse_plain_version(""), None);
        assert_eq!(parse_plain_version("37.2-beta1"), None);
        assert_eq!(parse_plain_version("android-34-ext8"), None);
    }

    #[test]
    fn parse_plain_version_orders_numerically_not_lexicographically() {
        // Vec<u64> ordering must prefer 10 over 9 (a plain string compare
        // would wrongly prefer "9" over "10").
        assert!(parse_plain_version("10.0.0") > parse_plain_version("9.0.0"));
    }

    #[test]
    fn installed_rust_targets_splits_trimmed_non_empty_lines() {
        let stdout = "x86_64-unknown-linux-gnu\n aarch64-linux-android \n\n";
        assert_eq!(
            installed_rust_targets(stdout),
            vec!["x86_64-unknown-linux-gnu", "aarch64-linux-android"]
        );
    }

    #[test]
    fn installed_rust_targets_preserves_exact_target_names() {
        let stdout = "aarch64-linux-android\naarch64-linux-android-sim\n";
        let targets = installed_rust_targets(stdout);
        assert!(targets.contains(&"aarch64-linux-android"));
        assert!(!targets.contains(&"aarch64-linux-androi"));
    }

    #[test]
    fn find_java_static_final_int_parses_expected_abi_version() {
        let source = r#"
            public final class NativeRadar {
                public static final int EXPECTED_ABI_VERSION = 7;
            }
        "#;
        assert_eq!(
            find_java_static_final_int(source, "EXPECTED_ABI_VERSION"),
            Ok(7)
        );
    }

    #[test]
    fn find_java_static_final_int_reports_missing_or_invalid_constants() {
        assert!(
            find_java_static_final_int("class NativeRadar {}", "EXPECTED_ABI_VERSION").is_err()
        );
        assert!(
            find_java_static_final_int(
                "public static final int EXPECTED_ABI_VERSION = nope;",
                "EXPECTED_ABI_VERSION"
            )
            .is_err()
        );
    }

    #[test]
    fn jni_smoke_java_source_embeds_expected_checks() {
        let source = jni_smoke_java_source(11, 21);
        assert!(source.contains("NativeRadar.abiVersion() == 11"));
        assert!(source.contains("linked.size() == 21"));
        assert!(source.contains("verifyEveryDeclaredNativeLinks();"));
        assert!(source.contains("java.lang.reflect.Modifier.isNative(modifiers)"));
        assert!(source.contains(
            "NativeRadar.defaultCalibrationProfile() == NativeRadar.CALIBRATION_BASELINE"
        ));
        assert!(
            source
                .contains("NativeRadar.defaultTrackingProfile() == NativeRadar.TRACKING_STANDARD")
        );
        assert!(source.contains("Double.isNaN(NativeRadar.calibrationProfileRssiAt1mDbm(99))"));
        assert!(source.contains("Double.isNaN(NativeRadar.bleDistanceM(-70.0, -59.0, 0.0))"));
        assert!(source.contains("expected NativeRadar to be unavailable"));
    }

    const REAL_NATIVE_RADAR_JAVA: &str =
        include_str!("../../android/app/src/main/java/com/hse/bleradar/NativeRadar.java");

    #[test]
    fn strip_java_comments_and_literals_blanks_prose_but_keeps_code() {
        let source = "// native in a line comment\n\
                      /* native in a\n block comment */ static native int a(); \
                      String s = \"native \\\" quoted\"; char c = '\"'; static native long b();";
        let code = strip_java_comments_and_literals(source);
        assert_eq!(code.matches("native").count(), 2);
        assert!(code.contains("static native int a();"));
        assert!(code.contains("static native long b();"));
        assert!(code.contains("String s = \"\";"));
        assert!(code.contains("char c = '';"));
        // Line structure survives so later diagnostics stay line-accurate.
        assert_eq!(code.matches('\n').count(), source.matches('\n').count());
    }

    #[test]
    fn java_package_and_class_read_the_declarations() {
        let code = "package com.example.deep;\n\npublic final class Facade {\n}\n";
        assert_eq!(
            java_package_and_class(code),
            Ok(("com.example.deep".to_string(), "Facade".to_string()))
        );
        assert!(java_package_and_class("public final class Facade {}").is_err());
        assert!(java_package_and_class("package com.example;").is_err());
        // `class` inside an identifier or a string literal is not a declaration.
        assert!(java_package_and_class("package a; int classCount = 1;").is_err());
    }

    #[test]
    fn java_static_native_method_names_are_sorted_and_multi_line_tolerant() {
        let code = "package a;\npublic final class F {\n\
                    public static native double zeta(double x);\n\
                    /* stripped */ public static native int alpha(\n\
                    int first,\n\
                    long second);\n\
                    static native\n    long beta();\n\
                    public static synchronized void notNative(int nativeCount) {}\n}\n";
        assert_eq!(
            java_static_native_method_names(&strip_java_comments_and_literals(code)),
            Ok(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "zeta".to_string()
            ])
        );
    }

    #[test]
    fn java_static_native_method_names_rejects_non_static_and_overloaded_natives() {
        let non_static = "class F { public native int a(); }";
        assert!(
            java_static_native_method_names(non_static)
                .unwrap_err()
                .contains("non-static native method `a`")
        );
        let overloaded = "class F { static native int a(int x); static native int a(long x); }";
        assert!(
            java_static_native_method_names(overloaded)
                .unwrap_err()
                .contains("overloaded native method `a`")
        );
        let malformed = "class F { static native int ; }";
        assert!(java_static_native_method_names(malformed).is_err());
        assert!(java_static_native_method_names("class F { static native int a }").is_err());
    }

    #[test]
    fn jni_mangle_applies_the_specification_escapes() {
        assert_eq!(jni_mangle("com.hse.bleradar"), "com_hse_bleradar");
        assert_eq!(jni_mangle("snake_case"), "snake_1case");
        assert_eq!(jni_mangle("Ljava/lang/String;"), "Ljava_lang_String_2");
        assert_eq!(jni_mangle("[I"), "_3I");
        assert_eq!(jni_mangle("naïve"), "na_000efve");
    }

    #[test]
    fn jni_export_contract_derives_prefix_and_exports_from_the_java_source() {
        let source = "package com.example.app;\n\
                      /** Talks about native code. */\n\
                      public final class Bridge {\n\
                      public static native int abiVersion();\n\
                      public static native double snake_case(double x);\n}\n";
        let contract = JniExportContract::from_java_source(source).unwrap();
        assert_eq!(contract.symbol_prefix, "Java_com_example_app_Bridge");
        assert_eq!(
            contract.native_methods,
            vec!["abiVersion".to_string(), "snake_case".to_string()]
        );
        assert_eq!(
            contract.expected_exports,
            vec![
                "Java_com_example_app_Bridge_abiVersion".to_string(),
                "Java_com_example_app_Bridge_snake_1case".to_string()
            ]
        );
    }

    #[test]
    fn jni_export_contract_requires_an_exact_export_set() {
        let contract = JniExportContract::from_java_source(
            "package p; class C { static native int a(); static native int b(); }",
        )
        .unwrap();
        let exact = vec![
            "Java_p_C_a".to_string(),
            "Java_p_C_b".to_string(),
            "rust_eh_personality".to_string(),
        ];
        assert_eq!(contract.require_exact_exports(&exact), Ok(()));

        let missing = vec!["Java_p_C_a".to_string()];
        let error = contract.require_exact_exports(&missing).unwrap_err();
        assert!(error.contains(
            "1 native method(s) declared in NativeRadar.java have no export: Java_p_C_b"
        ));
        assert!(!error.contains("no `static native` declaration"));

        let orphaned = vec![
            "Java_p_C_a".to_string(),
            "Java_p_C_b".to_string(),
            "Java_p_C_zombie".to_string(),
        ];
        let error = contract.require_exact_exports(&orphaned).unwrap_err();
        assert!(error.contains("1 `Java_` export(s) have no `static native` declaration in NativeRadar.java: Java_p_C_zombie"));

        let both = vec!["Java_p_C_zombie".to_string()];
        let error = contract.require_exact_exports(&both).unwrap_err();
        assert!(error.contains("Java_p_C_a, Java_p_C_b"));
        assert!(error.contains("Java_p_C_zombie"));
    }

    #[test]
    fn real_native_radar_facade_parses_into_a_non_empty_contract() {
        // The real façade is the single authority for the export set; this
        // pins only what every other proof depends on (package/class prefix
        // and the abiVersion probe), never a hand-maintained member list.
        let contract = JniExportContract::from_java_source(REAL_NATIVE_RADAR_JAVA).unwrap();
        assert_eq!(contract.symbol_prefix, "Java_com_hse_bleradar_NativeRadar");
        assert!(
            contract
                .native_methods
                .iter()
                .any(|name| name == "abiVersion")
        );
        assert_eq!(
            contract.native_methods.len(),
            contract.expected_exports.len()
        );
        assert!(
            contract
                .expected_exports
                .iter()
                .all(|export| export.starts_with("Java_com_hse_bleradar_NativeRadar_"))
        );
    }

    #[test]
    fn require_expected_members_accepts_complete_sets() {
        let actual = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        assert_eq!(
            require_expected_members(&actual, &["one", "three"], "demo"),
            Ok(())
        );
    }

    #[test]
    fn require_expected_members_reports_missing_members_in_expected_order() {
        let actual = vec!["present".to_string()];
        assert_eq!(
            require_expected_members(&actual, &["missing-b", "present", "missing-a"], "demo"),
            Err("demo missing required members: missing-b, missing-a".to_string())
        );
    }

    #[test]
    fn pick_highest_version_dir_prefers_the_highest_qualifying_version() {
        let root =
            unique_temp_dir("pick_highest_version_dir_prefers_the_highest_qualifying_version");
        for name in ["9.0.0", "10.0.0", "not-a-version"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }

        let picked = pick_highest_version_dir(&root, |_| true).unwrap();

        assert_eq!(picked, root.join("10.0.0"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pick_highest_version_dir_filters_out_versions_failing_the_predicate() {
        let root =
            unique_temp_dir("pick_highest_version_dir_filters_out_versions_failing_the_predicate");
        for name in ["9.0.0", "10.0.0"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join("9.0.0").join("marker"), "").unwrap();

        let picked = pick_highest_version_dir(&root, |path| path.join("marker").is_file()).unwrap();

        assert_eq!(picked, root.join("9.0.0"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pick_highest_version_dir_returns_none_when_nothing_qualifies() {
        let root = unique_temp_dir("pick_highest_version_dir_returns_none_when_nothing_qualifies");
        fs::create_dir_all(root.join("not-a-version")).unwrap();

        assert!(pick_highest_version_dir(&root, |_| true).is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_quoted_attr_extracts_the_value_between_quotes() {
        let manifest = r#"<uses-sdk android:minSdkVersion="26" android:targetSdkVersion="34" />"#;
        assert_eq!(
            find_quoted_attr(manifest, "android:minSdkVersion=\""),
            Some("26".to_string())
        );
        assert_eq!(
            find_quoted_attr(manifest, "android:targetSdkVersion=\""),
            Some("34".to_string())
        );
    }

    #[test]
    fn find_quoted_attr_returns_none_when_prefix_or_closing_quote_missing() {
        assert_eq!(
            find_quoted_attr("no attrs here", "android:minSdkVersion=\""),
            None
        );
        assert_eq!(
            find_quoted_attr("android:minSdkVersion=\"26", "android:minSdkVersion=\""),
            None
        );
    }

    #[test]
    fn parse_uses_sdk_reads_both_versions_from_manifest_text() {
        let manifest = r#"<uses-sdk android:minSdkVersion="26" android:targetSdkVersion="34" />"#;
        assert_eq!(parse_uses_sdk(manifest), Ok((26, 34)));
    }

    #[test]
    fn parse_uses_sdk_reports_missing_attributes() {
        assert!(parse_uses_sdk("<uses-sdk />").is_err());
        assert!(parse_uses_sdk(r#"<uses-sdk android:minSdkVersion="26" />"#).is_err());
    }

    #[test]
    fn parse_uses_sdk_reports_non_numeric_values() {
        let manifest = r#"android:minSdkVersion="abc" android:targetSdkVersion="34""#;
        assert!(parse_uses_sdk(manifest).is_err());
    }

    #[test]
    fn find_files_with_extension_recurses_and_filters_by_exact_extension() {
        let root =
            unique_temp_dir("find_files_with_extension_recurses_and_filters_by_exact_extension");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("Top.java"), "").unwrap();
        fs::write(root.join("a/Mid.java"), "").unwrap();
        fs::write(root.join("a/b/Deep.java"), "").unwrap();
        fs::write(root.join("a/Ignore.txt"), "").unwrap();

        let found = find_files_with_extension(&root, "java").unwrap();

        // Sorted order: "Top.java" (starts 'T') < "a/..." (starts 'a')
        // since PathBuf's Ord compares components byte-wise, and within
        // "a/", "Mid.java" ('M') < "b/Deep.java" ('b').
        assert_eq!(
            found,
            vec![
                root.join("Top.java"),
                root.join("a/Mid.java"),
                root.join("a/b/Deep.java"),
            ]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_files_with_extension_returns_empty_when_none_match() {
        let root = unique_temp_dir("find_files_with_extension_returns_empty_when_none_match");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Ignore.txt"), "").unwrap();

        assert!(find_files_with_extension(&root, "java").unwrap().is_empty());
        fs::remove_dir_all(&root).unwrap();
    }
}

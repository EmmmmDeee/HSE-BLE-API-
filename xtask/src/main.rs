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
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Path (relative to the repo root) of the vendored, plain-file advisory
/// database `cargo audit` reads directly (no materialization needed: unlike
/// `cargo deny`, `cargo audit` tolerates a plain, non-git directory).
const AUDIT_DB_PATH: &str = "vendor/rustsec-advisory-db/advisory-db-3157b0e258782691";

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
         \x20 apk-inventory <apk>        print sha256 + dex/lib/manifest entries\n\
         \x20 native-abi <lib.so>        print sorted defined FUNC/OBJECT symbols\n\
         \x20 dex-classes <classes.dex>  print sorted class descriptors\n\
         \x20 vendor-advisory-db         materialize the offline cargo-deny advisory db\n\
         \x20 audit                      cargo audit against the vendored advisory db\n\
         \x20 deny                       cargo deny check against the vendored advisory db\n\
         \x20 gates                      run every gate (fmt/clippy/build/test/doc/checks/audit/deny)"
    );
}

/// Walks upward from the current directory looking for the repository root
/// (identified by the presence of both `Cargo.toml` and `docs/NATIVE_ABI.txt`,
/// which together are specific enough to avoid false positives from an
/// unrelated ancestor `Cargo.toml`).
fn repo_root() -> Result<PathBuf, String> {
    let mut dir = env::current_dir().map_err(|e| format!("current_dir: {e}"))?;
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

/// Port of `tools/parity_report.py`: regenerates `docs/PARITY_COVERAGE.md`
/// from `docs/NATIVE_ABI.txt` and the semantic compatibility registry.
fn cmd_parity_report() -> Result<(), String> {
    let root = repo_root()?;
    let abi_path = root.join("docs/NATIVE_ABI.txt");
    let compat_path = root.join("crates/bleradar-compat/src/lib.rs");
    let out_path = root.join("docs/PARITY_COVERAGE.md");

    let abi_text = read_to_string(&abi_path)?;
    let compat = read_to_string(&compat_path)?;

    let mut observed_count = 0usize;
    observed_count += dedup_sorted(find_prefixed_runs(
        &abi_text,
        "UNIFFI_META_BLERADAR_CORE_FUNC_",
    ))
    .len();
    let mut method_or_ctor = find_prefixed_runs(&abi_text, "UNIFFI_META_BLERADAR_CORE_METHOD_");
    method_or_ctor.extend(find_prefixed_runs(
        &abi_text,
        "UNIFFI_META_BLERADAR_CORE_CONSTRUCTOR_",
    ));
    observed_count += dedup_sorted(method_or_ctor).len();

    let registered = find_quoted_name_values(&compat);

    let mut lines: Vec<String> = vec![
        "# Parity Coverage".to_string(),
        String::new(),
        "Generated from `docs/NATIVE_ABI.txt` and the semantic compatibility registry.".to_string(),
        String::new(),
        format!("- Observed UniFFI function/method/constructor symbols: **{observed_count}**"),
        format!(
            "- Contracts with explicit semantic migration status: **{}**",
            registered.len()
        ),
        format!(
            "- Remaining observed symbols requiring semantic registration/characterization: **{}**",
            observed_count.saturating_sub(registered.len())
        ),
        String::new(),
        "## Registered semantic frontier".to_string(),
        String::new(),
    ];
    for name in &registered {
        lines.push(format!("- `{name}`"));
    }
    lines.push(String::new());
    lines.push("## Interpretation".to_string());
    lines.push(String::new());
    lines.push(
        "A symbol appearing in the APK is not automatically considered migrated. Exact parity \
         requires characterization of inputs, outputs, side effects, and errors against the \
         immutable oracle. The registry intentionally records that distinction."
            .to_string(),
    );

    let content = lines.join("\n") + "\n";
    fs::write(&out_path, content).map_err(|e| format!("writing {}: {e}", out_path.display()))?;
    println!("{}", out_path.display());
    Ok(())
}

/// Port of `tools/check_dependency_policy.py`: fails when the root
/// `Cargo.lock` contains any crate outside the audited workspace set.
fn cmd_check_dependency_policy() -> Result<(), String> {
    let root = repo_root()?;
    let lock_path = root.join("Cargo.lock");
    let text = read_to_string(&lock_path)?;

    const ALLOWED: [&str; 2] = ["bleradar-core", "bleradar-compat"];

    let mut names = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("name = \"")
            && let Some(inner) = rest.strip_suffix('"')
        {
            names.push(inner.to_string());
        }
    }

    if names.is_empty() {
        println!("Dependency policy gate could not parse any package names from Cargo.lock;");
        println!("refusing to pass silently on an unreadable lockfile.");
        return Err("empty package name set".to_string());
    }

    let mut foreign: Vec<&String> = names
        .iter()
        .filter(|n| !ALLOWED.contains(&n.as_str()))
        .collect();
    foreign.sort();
    foreign.dedup();
    if !foreign.is_empty() {
        println!("Dependency policy violation: third-party crates present in Cargo.lock:");
        for name in &foreign {
            println!("  - {name}");
        }
        println!(
            "The workspace is intentionally third-party-free (docs/AUTONOMOUS_DECISIONS.md, decision 9)."
        );
        println!("Adding a dependency is a deliberate decision: record it in the decision log and");
        println!(
            "update ALLOWED in xtask/src/main.rs (cmd_check_dependency_policy) in the same change."
        );
        return Err("foreign crates present".to_string());
    }

    let mut allowed_sorted = ALLOWED.to_vec();
    allowed_sorted.sort_unstable();
    println!(
        "Cargo.lock contains only the audited workspace crates: {}",
        allowed_sorted.join(", ")
    );
    Ok(())
}

/// Port of `tools/check_oracle_integrity.py`: fails when a retained binary
/// oracle no longer matches its recorded SHA-256.
fn cmd_check_oracle_integrity() -> Result<(), String> {
    let root = repo_root()?;
    let apk_path = root.join("BLE-Radar-Standalone-Android-ARM64-v0.3.0.apk");
    let input_sha_path = root.join("docs/INPUT_SHA256.txt");
    let zip_path = root.join("BLE-Radar-Rust-Migration-Critically-Enhanced-v0.3.0 (1).zip");
    const ZIP_BASELINE: &str = "07d2d80ce7e6c43f4c6ccc2496d30faafb77342e7bf196b894d32c7528cf3f76";

    let mut failures = Vec::new();

    let input_sha_text = fs::read_to_string(&input_sha_path).unwrap_or_default();
    let expected_apk_sha = find_sha256_after_label(&input_sha_text, "Original APK SHA-256:");
    match expected_apk_sha {
        None => failures.push(format!(
            "could not parse an APK SHA-256 from {}",
            input_sha_path.file_name().unwrap().to_string_lossy()
        )),
        Some(_) if !apk_path.exists() => failures.push(format!(
            "missing oracle file: {}",
            apk_path.file_name().unwrap().to_string_lossy()
        )),
        Some(expected) => {
            let actual = sha256::to_hex(&sha256::sha256(&read_bytes(&apk_path)?));
            if actual != expected {
                failures.push(format!(
                    "{}: expected {expected}, observed {actual}",
                    apk_path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    if !zip_path.exists() {
        failures.push(format!(
            "missing oracle archive: {}",
            zip_path.file_name().unwrap().to_string_lossy()
        ));
    } else {
        let actual = sha256::to_hex(&sha256::sha256(&read_bytes(&zip_path)?));
        if actual != ZIP_BASELINE {
            failures.push(format!(
                "{}: expected {ZIP_BASELINE}, observed {actual}",
                zip_path.file_name().unwrap().to_string_lossy()
            ));
        }
    }

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
    for name in &names {
        if name.ends_with(".dex") || name.starts_with("lib/") || name == "AndroidManifest.xml" {
            println!("{name}");
        }
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
}

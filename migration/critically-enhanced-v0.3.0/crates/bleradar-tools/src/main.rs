//! `bleradar-tools` — Rust inventory and parity commands for this snapshot.
//!
//! ```text
//! cargo run -p bleradar-tools --locked -- parity-report
//! cargo run -p bleradar-tools --locked -- apk-inventory <apk>
//! cargo run -p bleradar-tools --locked -- native-abi <lib.so>
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use bleradar_tools::elf::defined_func_and_object_symbols;
use bleradar_tools::parity::coverage_markdown;
use bleradar_tools::sha256::{sha256, to_hex};
use bleradar_tools::zip::entry_names;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    };
    let rest: Vec<String> = args.collect();
    let result = match command.as_str() {
        "parity-report" => cmd_parity_report(),
        "apk-inventory" => cmd_apk_inventory(rest.first().map(String::as_str)),
        "native-abi" => cmd_native_abi(rest.first().map(String::as_str)),
        "help" | "-h" | "--help" => {
            println!("{}", usage());
            return ExitCode::SUCCESS;
        }
        other => Err(format!("unknown command '{other}'\n{}", usage())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bleradar-tools: {message}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> &'static str {
    "usage:\n  \
     bleradar-tools parity-report\n  \
     bleradar-tools apk-inventory <apk>\n  \
     bleradar-tools native-abi <lib.so>"
}

fn cmd_parity_report() -> Result<(), String> {
    let root = snapshot_root()?;
    let abi = fs::read_to_string(root.join("docs/NATIVE_ABI.txt"))
        .map_err(|err| format!("read docs/NATIVE_ABI.txt: {err}"))?;
    let compat = fs::read_to_string(root.join("crates/bleradar-compat/src/lib.rs"))
        .map_err(|err| format!("read compat registry: {err}"))?;
    let report = coverage_markdown(&abi, &compat);
    let out = root.join("docs/PARITY_COVERAGE.md");
    fs::write(&out, report).map_err(|err| format!("write {}: {err}", out.display()))?;
    println!("{}", out.display());
    Ok(())
}

fn cmd_apk_inventory(apk: Option<&str>) -> Result<(), String> {
    let apk = apk.ok_or("usage: bleradar-tools apk-inventory <apk>")?;
    let bytes = fs::read(apk).map_err(|err| format!("read {apk}: {err}"))?;
    let names = entry_names(&bytes).map_err(|err| err.to_string())?;
    println!("apk={apk}");
    println!("sha256={}", to_hex(&sha256(&bytes)));
    println!("entries={}", names.len());
    let mut listed = names;
    listed.sort();
    for name in listed {
        if name.ends_with(".dex") || name.starts_with("lib/") || name == "AndroidManifest.xml" {
            println!("{name}");
        }
    }
    Ok(())
}

fn cmd_native_abi(library: Option<&str>) -> Result<(), String> {
    let library = library.ok_or("usage: bleradar-tools native-abi <lib.so>")?;
    let bytes = fs::read(library).map_err(|err| format!("read {library}: {err}"))?;
    let names = defined_func_and_object_symbols(&bytes).map_err(|err| err.to_string())?;
    for name in names {
        println!("{name}");
    }
    Ok(())
}

/// Directory that contains this crate. Walking from the process cwd stops at
/// the snapshot, so a run from the live repository root cannot overwrite
/// `docs/PARITY_COVERAGE.md` there.
fn snapshot_root() -> Result<PathBuf, String> {
    let mut dir = env::current_dir().map_err(|err| format!("current directory: {err}"))?;
    loop {
        if dir.join("crates/bleradar-tools/Cargo.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(
                "crates/bleradar-tools not found; run from migration/critically-enhanced-v0.3.0"
                    .to_string(),
            );
        }
    }
}

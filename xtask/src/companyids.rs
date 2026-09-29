//! `cargo xtask sync-company-ids`: refreshes the bundled Bluetooth SIG
//! company-identifier registry (`crates/bleradar-core/data/`), the one source
//! of every manufacturer name the radar shows.
//!
//! The SIG assigns new identifiers continuously, so a scan meets vendors a
//! months-old snapshot does not name. The refresh is all-or-nothing: it
//! downloads the SIG's file, checks its shape, writes it, runs the core's own
//! registry tests against it (the strict `build.rs` parser and the
//! extensional comparison of all 65,536 identifiers) and restores the previous
//! file if either objects.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::sha256::sha256;

/// Where the SIG publishes the file.
pub const UPSTREAM_URL: &str = "https://bitbucket.org/bluetooth-SIG/public/raw/main/assigned_numbers/company_identifiers/company_identifiers.yaml";
/// The bundled file, relative to the repository root.
pub const DATA_PATH: &str = "crates/bleradar-core/data/company_identifiers.yaml";
/// The provenance record beside it, relative to the repository root.
pub const SOURCE_PATH: &str = "crates/bleradar-core/data/SOURCE.md";

/// What a registry file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Assigned identifiers.
    pub entries: usize,
    /// The lowest.
    pub lowest: u16,
    /// The highest.
    pub highest: u16,
}

/// The shape of a registry file — `company_identifiers:` and then, per
/// company, a `  - value: 0xHHHH` line and a `    name: ...` line — or why it
/// is not one. This is the cheap pre-check; the core's build is the strict one.
pub fn summarize(text: &str) -> Result<Summary, String> {
    let mut lines = text.lines();
    if lines.next() != Some("company_identifiers:") {
        return Err("does not start with `company_identifiers:`".to_string());
    }
    let mut ids: Vec<u16> = Vec::new();
    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }
        let id = line
            .strip_prefix("  - value: 0x")
            .filter(|hex| hex.len() == 4)
            .and_then(|hex| u16::from_str_radix(hex, 16).ok())
            .ok_or_else(|| format!("expected `  - value: 0xHHHH`, found {line:?}"))?;
        match lines.next() {
            Some(name) if name.starts_with("    name: ") => ids.push(id),
            other => {
                return Err(format!(
                    "expected `    name: ...` after {id:#06x}, found {other:?}"
                ));
            }
        }
    }
    ids.sort_unstable();
    if let Some(pair) = ids.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(format!("{:#06x} is assigned twice", pair[0]));
    }
    match (ids.first(), ids.last()) {
        (Some(&lowest), Some(&highest)) => Ok(Summary {
            entries: ids.len(),
            lowest,
            highest,
        }),
        _ => Err("holds no company".to_string()),
    }
}

/// The provenance record for `data`, retrieved on `retrieved` (`YYYY-MM-DD`).
pub fn source_record(data: &str, summary: Summary, retrieved: &str) -> String {
    let digest: String = sha256(data.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "# Bluetooth SIG company identifiers\n\
         \n\
         `company_identifiers.yaml` is the Bluetooth SIG's own Assigned Numbers file, checked in\n\
         verbatim. `../build.rs` turns it into the tables behind `bleradar_core::adv::company_name`,\n\
         and nothing else names a manufacturer.\n\
         \n\
         | | |\n\
         |---|---|\n\
         | Upstream | {UPSTREAM_URL} |\n\
         | Retrieved | {retrieved} |\n\
         | Entries | {} ({:#06x} to {:#06x}) |\n\
         | SHA-256 | {digest} |\n\
         \n\
         Refresh with `cargo xtask sync-company-ids`: it downloads the file, checks it, runs the\n\
         core's registry tests against it and restores the previous file if they fail, then\n\
         rewrites this record. `cargo xtask`'s own tests hold this record to the data file.\n",
        summary.entries, summary.lowest, summary.highest
    )
}

/// The `YYYY-MM-DD` the record says the file was retrieved on.
fn retrieved_date(record: &str) -> Option<&str> {
    let date = record
        .lines()
        .find_map(|line| line.strip_prefix("| Retrieved | ")?.strip_suffix(" |"))?;
    let well_formed = date.len() == 10
        && date.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    well_formed.then_some(date)
}

/// UTC date as `YYYY-MM-DD`, from the system's `date`.
fn today() -> Result<String, String> {
    let output = Command::new("date")
        .args(["-u", "+%F"])
        .output()
        .map_err(|error| format!("running date: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && text.len() == 10 {
        Ok(text)
    } else {
        Err(format!("date -u +%F answered {text:?}"))
    }
}

/// Downloads the SIG's file, checks it, installs it, proves the core accepts
/// it, and records where it came from.
pub fn sync(root: &Path) -> Result<(), String> {
    let data_path = root.join(DATA_PATH);
    let source_path = root.join(SOURCE_PATH);
    let previous =
        fs::read_to_string(&data_path).map_err(|e| format!("reading {DATA_PATH}: {e}"))?;
    let previous_summary =
        summarize(&previous).map_err(|e| format!("the bundled {DATA_PATH} {e}"))?;

    println!("== downloading {UPSTREAM_URL} ==");
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "120",
            UPSTREAM_URL,
        ])
        .output()
        .map_err(|e| format!("running curl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl {UPSTREAM_URL} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let downloaded =
        String::from_utf8(output.stdout).map_err(|_| "the download is not UTF-8".to_string())?;
    let summary = summarize(&downloaded).map_err(|e| format!("the download {e}"))?;
    if summary.entries < previous_summary.entries {
        return Err(format!(
            "the download holds {} companies, fewer than the {} bundled: the SIG does not withdraw identifiers, so it is not the registry",
            summary.entries, previous_summary.entries
        ));
    }
    let record = fs::read_to_string(&source_path).unwrap_or_default();
    let record_is_current = retrieved_date(&record)
        .is_some_and(|date| record == source_record(&downloaded, summary, date));
    if downloaded == previous && record_is_current {
        println!(
            "sync-company-ids: already current ({} companies, {:#06x} to {:#06x})",
            summary.entries, summary.lowest, summary.highest
        );
        return Ok(());
    }

    if downloaded != previous {
        fs::write(&data_path, &downloaded).map_err(|e| format!("writing {DATA_PATH}: {e}"))?;
    }
    println!("== the core's registry tests against the file ==");
    let checked = Command::new("cargo")
        .args([
            "test",
            "--locked",
            "-p",
            "bleradar-core",
            "--lib",
            "adv::tests::company",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("running cargo test: {e}"))?;
    if !checked.status.success() {
        fs::write(&data_path, &previous).map_err(|e| format!("restoring {DATA_PATH}: {e}"))?;
        let log = String::from_utf8_lossy(&checked.stderr);
        let tail: Vec<&str> = log.lines().rev().take(25).collect();
        return Err(format!(
            "the core rejects the downloaded registry (the previous file is restored):\n{}",
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        ));
    }
    fs::write(&source_path, source_record(&downloaded, summary, &today()?))
        .map_err(|e| format!("writing {SOURCE_PATH}: {e}"))?;
    println!(
        "sync-company-ids: {} companies (was {}), {:#06x} to {:#06x}; the core accepts the file — commit {DATA_PATH} and {SOURCE_PATH}",
        summary.entries, previous_summary.entries, summary.lowest, summary.highest
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "company_identifiers:\n\n  - value: 0x0002\n    name: 'B'\n\n  - value: 0x0001\n    name: 'A'\n";

    #[test]
    fn a_registry_file_is_summarized_whatever_its_order() {
        assert_eq!(
            summarize(SAMPLE),
            Ok(Summary {
                entries: 2,
                lowest: 1,
                highest: 2
            })
        );
    }

    #[test]
    fn a_file_that_is_not_the_registry_is_refused_with_the_reason() {
        for (text, reason) in [
            ("", "does not start with"),
            ("<html>404</html>", "does not start with"),
            ("company_identifiers:\n", "holds no company"),
            (
                "company_identifiers:\n  - value: 0x1\n    name: 'A'\n",
                "expected `  - value: 0xHHHH`",
            ),
            (
                "company_identifiers:\n  - value: 0x0001\n",
                "expected `    name: ...`",
            ),
            (
                "company_identifiers:\n  - value: 0x0001\n    name: 'A'\n  - value: 0x0001\n    name: 'B'\n",
                "assigned twice",
            ),
        ] {
            let error = summarize(text).expect_err(text);
            assert!(error.contains(reason), "{text:?}: {error}");
        }
    }

    /// The provenance record cannot rot: the digest and the count it states
    /// are the bundled file's own.
    #[test]
    fn the_source_record_describes_the_bundled_file() {
        let root = crate::repo_root().expect("repository root");
        let data = fs::read_to_string(root.join(DATA_PATH)).expect("the bundled registry");
        let record = fs::read_to_string(root.join(SOURCE_PATH)).expect("the provenance record");
        let summary = summarize(&data).expect("the bundled registry is a registry");
        let retrieved =
            retrieved_date(&record).expect("a well-formed `| Retrieved | YYYY-MM-DD |` row");
        assert_eq!(
            record,
            source_record(&data, summary, retrieved),
            "SOURCE.md is stale"
        );
    }
}

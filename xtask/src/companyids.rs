//! `cargo xtask sync-company-ids`: refreshes the bundled Bluetooth SIG
//! company-identifier registry (`crates/bleradar-core/data/`), the one source
//! of every manufacturer name the radar shows.
//!
//! The SIG assigns new identifiers continuously, so a scan meets vendors a
//! months-old snapshot does not name. The refresh is all-or-nothing across
//! every file it touches — the data file, its provenance record, and (when
//! the registry's own bytes changed) `adv.rs`'s `COMPANY_TABLE_VERSION`: it
//! downloads the SIG's file, checks its shape, writes it, runs the core's own
//! registry tests against it (the strict `build.rs` parser and the
//! extensional comparison of all 65,536 identifiers), bumps the version, and
//! restores every file this run touched to what it held before if any step
//! objects. Each write is itself atomic (write a sibling temp file, then
//! rename over the real path) so a write that dies mid-way — a full disk, a
//! kill — can never leave a truncated file in place of a good one.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::sha256::sha256;

/// Where the SIG publishes the file.
pub const UPSTREAM_URL: &str = "https://bitbucket.org/bluetooth-SIG/public/raw/main/assigned_numbers/company_identifiers/company_identifiers.yaml";
/// The bundled file, relative to the repository root.
pub const DATA_PATH: &str = "crates/bleradar-core/data/company_identifiers.yaml";
/// The provenance record beside it, relative to the repository root.
pub const SOURCE_PATH: &str = "crates/bleradar-core/data/SOURCE.md";
/// The core source declaring `COMPANY_TABLE_VERSION`, relative to the
/// repository root — bumped here whenever [`sync`] installs a registry whose
/// bytes differ from what was bundled before, matching that constant's own
/// documented contract.
pub const ADV_PATH: &str = "crates/bleradar-core/src/adv.rs";

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

/// Replace `path`'s content with `content` atomically: write a sibling temp
/// file, then rename it over `path`. A bare `fs::write` can leave a truncated
/// file if the process dies mid-write (a full disk, a kill); `rename` on the
/// same filesystem is atomic, so `path` always ends up holding either its
/// previous full content or its new full content, never a partial one.
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| format!("writing {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path)
        .map_err(|e| format!("renaming {} to {}: {e}", tmp.display(), path.display()))
}

/// Rewrite `pub const COMPANY_TABLE_VERSION: u32 = N;` in `adv_rs`'s source
/// text to `N + 1`, or the reason it could not. The line must appear exactly
/// once, in exactly this shape — a future rename or reformat of the constant
/// fails loudly here rather than silently leaving the version un-bumped,
/// which is exactly the drift [`sync`] exists to prevent, applied to its own
/// instrument. **Pure.**
fn bump_company_table_version(adv_rs: &str) -> Result<String, String> {
    const PREFIX: &str = "pub const COMPANY_TABLE_VERSION: u32 = ";
    let mut occurrences = adv_rs.match_indices(PREFIX);
    let Some((start, _)) = occurrences.next() else {
        return Err(format!("no `{PREFIX}...` line found"));
    };
    if occurrences.next().is_some() {
        return Err(format!("more than one `{PREFIX}...` line found"));
    }
    let after = &adv_rs[start + PREFIX.len()..];
    let end = after
        .find(';')
        .ok_or_else(|| format!("`{PREFIX}...` is not terminated by `;`"))?;
    let digits = &after[..end];
    let current: u32 = digits
        .parse()
        .map_err(|_| format!("`{PREFIX}{digits};` is not a plain integer"))?;
    let bumped = current
        .checked_add(1)
        .ok_or_else(|| "COMPANY_TABLE_VERSION is already u32::MAX".to_string())?;
    Ok(format!(
        "{}{PREFIX}{bumped}{}",
        &adv_rs[..start],
        &after[end..]
    ))
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

/// Write `content` to `path` (via [`write_atomic`]), first recording `path`'s
/// pre-write text in `rollback` so a later failure in the same [`sync`] run
/// can undo every write this run made, not just its last one.
fn write_tracked(
    path: &Path,
    previous: &str,
    content: &str,
    rollback: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    rollback.push((path.to_path_buf(), previous.to_string()));
    write_atomic(path, content)
}

/// Downloads the SIG's file, checks it, installs it, proves the core accepts
/// it, bumps `adv.rs`'s `COMPANY_TABLE_VERSION` when the registry's bytes
/// actually changed, and records where the file came from.
///
/// All or nothing across every file it touches: `rollback` accumulates the
/// pre-write text of each file as it is written, and any later step that
/// fails — the core's own tests, reading the clock, a write itself — restores
/// every one of them (in reverse order) before returning the error, so a
/// failed sync can never leave `data/company_identifiers.yaml`, its
/// `SOURCE.md`, and `adv.rs`'s version mismatched with one another.
pub fn sync(root: &Path) -> Result<(), String> {
    let data_path = root.join(DATA_PATH);
    let source_path = root.join(SOURCE_PATH);
    let adv_path = root.join(ADV_PATH);
    let previous =
        fs::read_to_string(&data_path).map_err(|e| format!("reading {DATA_PATH}: {e}"))?;
    let previous_summary =
        summarize(&previous).map_err(|e| format!("the bundled {DATA_PATH} {e}"))?;
    let previous_adv =
        fs::read_to_string(&adv_path).map_err(|e| format!("reading {ADV_PATH}: {e}"))?;

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
    // Only an actual change to the registry's own bytes is a "refresh" for
    // COMPANY_TABLE_VERSION's purpose (bumped whenever the bundled snapshot
    // changes) — re-stamping SOURCE.md's date after a merely stale or
    // hand-edited record is not one.
    let data_changed = downloaded != previous;
    if !data_changed && record_is_current {
        println!(
            "sync-company-ids: already current ({} companies, {:#06x} to {:#06x})",
            summary.entries, summary.lowest, summary.highest
        );
        return Ok(());
    }

    let mut rollback: Vec<(PathBuf, String)> = Vec::new();
    let outcome: Result<(), String> = (|| {
        if data_changed {
            write_tracked(&data_path, &previous, &downloaded, &mut rollback)
                .map_err(|e| format!("writing {DATA_PATH}: {e}"))?;
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
            let log = String::from_utf8_lossy(&checked.stderr);
            let tail: Vec<&str> = log.lines().rev().take(25).collect();
            return Err(format!(
                "the core rejects the downloaded registry:\n{}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            ));
        }
        let retrieved = today()?;
        write_tracked(
            &source_path,
            &record,
            &source_record(&downloaded, summary, &retrieved),
            &mut rollback,
        )
        .map_err(|e| format!("writing {SOURCE_PATH}: {e}"))?;
        if data_changed {
            let bumped = bump_company_table_version(&previous_adv)
                .map_err(|e| format!("bumping COMPANY_TABLE_VERSION in {ADV_PATH}: {e}"))?;
            write_tracked(&adv_path, &previous_adv, &bumped, &mut rollback)
                .map_err(|e| format!("writing {ADV_PATH}: {e}"))?;
        }
        Ok(())
    })();

    if let Err(err) = outcome {
        let mut restore_errors = Vec::new();
        for (path, original) in rollback.iter().rev() {
            if let Err(e) = write_atomic(path, original) {
                restore_errors.push(format!("restoring {}: {e}", path.display()));
            }
        }
        return Err(if restore_errors.is_empty() {
            format!("{err} (every changed file this run touched is restored)")
        } else {
            format!("{err} ({})", restore_errors.join("; "))
        });
    }

    println!(
        "sync-company-ids: {} companies (was {}), {:#06x} to {:#06x}; the core accepts the file — commit {DATA_PATH}, {SOURCE_PATH}{}",
        summary.entries,
        previous_summary.entries,
        summary.lowest,
        summary.highest,
        if data_changed {
            format!(" and {ADV_PATH} (COMPANY_TABLE_VERSION bumped)")
        } else {
            String::new()
        }
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

    #[test]
    fn bump_company_table_version_increments_and_preserves_everything_else() {
        let before = "// doc\npub const COMPANY_TABLE_VERSION: u32 = 3;\n\nfn after_it() {}\n";
        let after = bump_company_table_version(before).expect("a well-formed line");
        assert_eq!(
            after,
            "// doc\npub const COMPANY_TABLE_VERSION: u32 = 4;\n\nfn after_it() {}\n"
        );
    }

    #[test]
    fn bump_company_table_version_is_refused_with_the_reason() {
        for (text, reason) in [
            (
                "pub const OTHER: u32 = 3;\n",
                "no `pub const COMPANY_TABLE_VERSION",
            ),
            (
                "pub const COMPANY_TABLE_VERSION: u32 = 3\n",
                "not terminated by `;`",
            ),
            (
                "pub const COMPANY_TABLE_VERSION: u32 = three;\n",
                "not a plain integer",
            ),
            (
                "pub const COMPANY_TABLE_VERSION: u32 = 1;\npub const COMPANY_TABLE_VERSION: u32 = 2;\n",
                "more than one",
            ),
        ] {
            let error = bump_company_table_version(text).expect_err(text);
            assert!(error.contains(reason), "{text:?}: {error}");
        }
    }

    #[test]
    fn bump_company_table_version_refuses_to_overflow() {
        let text = format!("pub const COMPANY_TABLE_VERSION: u32 = {};\n", u32::MAX);
        let error = bump_company_table_version(&text).expect_err("u32::MAX cannot be bumped");
        assert!(error.contains("u32::MAX"), "{error}");
    }

    /// The bundled `adv.rs` is, right now, in the exact shape
    /// [`bump_company_table_version`] requires — the regression Copilot's
    /// review asked for: a rename or reformat of the constant declaration
    /// that would silently defeat `sync`'s ability to bump it fails HERE,
    /// not only inside a live sync run nobody but a maintainer running the
    /// refresh would ever exercise.
    #[test]
    fn the_bundled_adv_rs_can_have_its_company_table_version_bumped() {
        let root = crate::repo_root().expect("repository root");
        let adv_rs = fs::read_to_string(root.join(ADV_PATH)).expect("adv.rs");
        bump_company_table_version(&adv_rs)
            .expect("adv.rs must declare COMPANY_TABLE_VERSION in the one shape sync bumps");
    }

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp_file_behind() {
        let dir = std::env::temp_dir().join(format!(
            "hse-companyids-write-atomic-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("registry.yaml");
        fs::write(&path, "old content").expect("seed file");

        write_atomic(&path, "new content").expect("atomic write");

        assert_eq!(
            fs::read_to_string(&path).expect("written file"),
            "new content"
        );
        assert!(
            !path.with_extension("tmp").exists(),
            "the sibling temp file must be gone once the rename lands"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

//! Materializes an ephemeral, single-commit git repository for the vendored
//! RustSec advisory database.
//!
//! `cargo-deny`'s advisories check shells out to `git show -s --format=%cI
//! HEAD` against its resolved `db-path` subdirectory, so it needs an actual
//! git commit to exist there — but this repository's tracked copy under
//! `vendor/rustsec-advisory-db/` is deliberately plain files with no
//! `.git`, so that adding it here can never turn into a nested-repository
//! "gitlink" that silently drops the vendored content on clone. See
//! `vendor/rustsec-advisory-db/NOTICE.md` for the full rationale.
//!
//! This module bridges the gap: it copies the plain, tracked files into a
//! throwaway location under the gitignored `target/` directory and commits
//! them there, fresh, every time it is called. `deny.toml`'s
//! `[advisories] db-path` points at that location.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Relative path (from the repo root) to the plain, tracked vendored files.
const VENDORED_SOURCE: &str = "vendor/rustsec-advisory-db/advisory-db-3157b0e258782691";

/// Relative path (from the repo root) to the ephemeral materialized git
/// repo; must match `deny.toml`'s `[advisories] db-path` plus this same
/// final path component.
const MATERIALIZED_TARGET: &str =
    "target/xtask-vendor/rustsec-advisory-db/advisory-db-3157b0e258782691";

/// The upstream commit date recorded for the vendored snapshot (see
/// `vendor/rustsec-advisory-db/NOTICE.md`), reused here so the materialized
/// wrapper commit's reported timestamp matches the documented provenance
/// instead of drifting to wall-clock "now" on every gate run.
const VENDORED_COMMIT_DATE: &str = "2026-08-31T11:44:04+02:00";

/// Recursively copies the contents of `src` into `dst`, creating
/// directories as needed. Vendored advisory data contains only regular
/// files and directories, so symlinks (unexpected here) are skipped rather
/// than followed or mangled.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let program = format!("{cmd:?}");
    let status = cmd
        .status()
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

/// Materializes the ephemeral advisory-db git repository under `target/`
/// from the plain vendored files, recreating it from scratch on every call.
/// Returns the absolute path to the materialized repository (a `db-path`
/// subdirectory ready for `cargo deny --offline check`).
///
/// Recreating unconditionally (rather than caching/diffing) keeps this
/// simple and correct at negligible cost: ~1200 small text files copy and
/// commit in well under a second.
pub fn materialize_advisory_db(repo_root: &Path) -> Result<PathBuf, String> {
    let source = repo_root.join(VENDORED_SOURCE);
    if !source.is_dir() {
        return Err(format!(
            "vendored advisory database source missing: {}",
            source.display()
        ));
    }

    let target = repo_root.join(MATERIALIZED_TARGET);
    if target.exists() {
        fs::remove_dir_all(&target)
            .map_err(|e| format!("clearing stale {}: {e}", target.display()))?;
    }
    copy_dir_all(&source, &target)
        .map_err(|e| format!("copying vendored advisory database: {e}"))?;

    run(Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&target))?;
    run(Command::new("git")
        .args([
            "-c",
            "user.email=xtask@localhost",
            "-c",
            "user.name=cargo xtask",
        ])
        .args(["add", "-A"])
        .current_dir(&target))?;
    run(Command::new("git")
        .args(["-c", "user.email=xtask@localhost", "-c", "user.name=cargo xtask"])
        .args(["commit", "-q", "-m"])
        .arg("Materialized vendored RustSec advisory database\n\nSee vendor/rustsec-advisory-db/NOTICE.md for provenance.")
        .env("GIT_AUTHOR_DATE", VENDORED_COMMIT_DATE)
        .env("GIT_COMMITTER_DATE", VENDORED_COMMIT_DATE)
        .current_dir(&target))?;

    Ok(target)
}

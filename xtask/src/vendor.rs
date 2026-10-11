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

use crate::git_in;

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

    // `git_in` removes the caller's GIT_DIR / GIT_WORK_TREE / GIT_INDEX_FILE,
    // which would otherwise point these commands at another repository,
    // work tree or index instead of `target`.
    run(git_in(&target).arg("init").arg("-q"))?;
    run(git_in(&target)
        .args([
            "-c",
            "user.email=xtask@localhost",
            "-c",
            "user.name=cargo xtask",
        ])
        .args(["add", "-A"]))?;
    run(git_in(&target)
        .args(["-c", "user.email=xtask@localhost", "-c", "user.name=cargo xtask"])
        .args(["commit", "-q", "-m"])
        .arg("Materialized vendored RustSec advisory database\n\nSee vendor/rustsec-advisory-db/NOTICE.md for provenance.")
        .env("GIT_AUTHOR_DATE", VENDORED_COMMIT_DATE)
        .env("GIT_COMMITTER_DATE", VENDORED_COMMIT_DATE))?;

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Set only in the child process that
    /// `materialize_ignores_git_redirects_inherited_from_the_environment`
    /// re-runs itself as: the repo root holding the vendored source.
    const HOSTILE_GIT_ENV_CHILD_ROOT: &str = "XTASK_TEST_HOSTILE_GIT_ENV_ADVISORY_ROOT";

    /// Runs `git args` in `dir` (scrubbed by [`git_in`]) and returns its
    /// stdout, or `None` when git exits non-zero.
    fn git_stdout(dir: &Path, args: &[&str]) -> Option<String> {
        let out = git_in(dir).args(args).output().expect("git runs");
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    #[test]
    fn materialize_ignores_git_redirects_inherited_from_the_environment() {
        // Review nit on #59/#60: `git init` / `add -A` / `commit` honoured
        // GIT_DIR, GIT_WORK_TREE and GIT_INDEX_FILE from the caller's
        // environment, so they wrote to the caller's repository, work tree or
        // index instead of the materialized advisory-db directory. The
        // inherited environment can only be set safely in a new process, so
        // this test re-runs itself (and only itself) with each variable set.
        if let Some(root) = env::var_os(HOSTILE_GIT_ENV_CHILD_ROOT) {
            let root = Path::new(&root);
            let target = materialize_advisory_db(root).unwrap();
            assert_eq!(target, root.join(MATERIALIZED_TARGET));
            assert!(target.join(".git").is_dir(), "no repository in target");
            assert_eq!(
                git_stdout(&target, &["log", "--format=%s"]).as_deref(),
                Some("Materialized vendored RustSec advisory database\n")
            );
            assert_eq!(
                git_stdout(&target, &["ls-files"]).as_deref(),
                Some("crates/x/RUSTSEC-0000-0000.md\nsupport.toml\n")
            );
            assert_eq!(
                git_stdout(&target, &["status", "--porcelain"]).as_deref(),
                Some("")
            );
            return;
        }
        let base = env::temp_dir().join(format!(
            "xtask-vendor-test-materialize-git-redirects-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let names = ["none", "GIT_INDEX_FILE", "GIT_DIR", "GIT_WORK_TREE", "all"];
        let mut failures = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let dir = base.join(name);
            let root = dir.join("root");
            let source = root.join(VENDORED_SOURCE);
            let decoy = dir.join("decoy");
            fs::create_dir_all(source.join("crates/x")).unwrap();
            fs::write(source.join("support.toml"), b"s\n").unwrap();
            fs::write(source.join("crates/x/RUSTSEC-0000-0000.md"), b"a\n").unwrap();
            fs::create_dir_all(&decoy).unwrap();
            fs::write(decoy.join("decoy.txt"), b"d\n").unwrap();
            assert!(git_stdout(&decoy, &["init", "-q"]).is_some());
            let decoy_git_dir = decoy.join(".git");
            let missing_index = dir.join("no-such-index");
            let hostile = [
                ("GIT_INDEX_FILE", missing_index.as_path()),
                ("GIT_DIR", decoy_git_dir.as_path()),
                ("GIT_WORK_TREE", decoy.as_path()),
            ];
            let case: Vec<(&str, &Path)> = match i {
                0 => vec![],
                4 => hostile.to_vec(),
                _ => vec![hostile[i - 1]],
            };
            let out = Command::new(env::current_exe().expect("test binary path"))
                .args([
                    "vendor::tests::materialize_ignores_git_redirects_inherited_from_the_environment",
                    "--exact",
                    "--nocapture",
                ])
                .env(HOSTILE_GIT_ENV_CHILD_ROOT, &root)
                .envs(case.iter().copied())
                .output()
                .expect("the test binary re-runs");
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !(out.status.success() && stdout.contains("1 passed")) {
                failures.push(format!(
                    "with {case:?}: materialize failed or produced the wrong repository:\n{stdout}{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            if git_stdout(&decoy, &["rev-parse", "--verify", "-q", "HEAD"]).is_some() {
                failures.push(format!(
                    "with {case:?}: a commit landed in the decoy repository"
                ));
            }
            if git_stdout(&decoy, &["ls-files"]).as_deref() != Some("") {
                failures.push(format!(
                    "with {case:?}: the decoy repository's index changed"
                ));
            }
            if missing_index.exists() {
                failures.push(format!(
                    "with {case:?}: the hostile GIT_INDEX_FILE was written"
                ));
            }
        }
        fs::remove_dir_all(&base).unwrap();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

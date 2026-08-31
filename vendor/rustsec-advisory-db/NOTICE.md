# Vendored RustSec Advisory Database

This directory vendors a snapshot of the [RustSec Advisory
Database](https://github.com/rustsec/advisory-db) so that `cargo audit` and
`cargo deny check advisories` can run **fully offline** — no network access
required for either the local gate runner (`cargo xtask gates`) or CI.

## Provenance

- Source: `https://github.com/rustsec/advisory-db`
- Upstream commit vendored: `ba9db2a77a6a0fe93bc63a3d9b730e08b145aff5`
- Upstream commit date: `2026-08-31T11:44:04+02:00`
- Vendored: `2026-08-31`
- License: [CC0-1.0](https://creativecommons.org/publicdomain/zero/1.0/) (see
  `advisory-db-3157b0e258782691/LICENSE.txt` and `.../LICENSES/`); some
  advisory text may be dual-licensed CC-BY-4.0 per upstream's own notice.

## Layout: plain files, deliberately not a git repository

`advisory-db-3157b0e258782691/` is named for the content-addressed cache
directory `cargo-audit`/`cargo-deny` derive from their default advisory
database URL (`https://github.com/rustsec/advisory-db`); that naming is
cosmetic/documentary here — the tools do not read this directory directly
(see "Why not commit it as a git repo" below).

It contains only what `cargo-audit`/`cargo-deny` actually read at runtime:

- `crates/` — per-crate advisories (`RUSTSEC-*.md`), the data that matters.
- `rust/` — advisories against the Rust compiler/standard library itself.
- `LICENSE.txt`, `LICENSES/` — upstream license texts.
- `README.md`, `support.toml` — upstream metadata some tooling reads.

Upstream's own contributor/governance material (`CONTRIBUTING.md`,
`MAINTAINERS_GUIDE.md`, `HOWTO_UNMAINTAINED.md`, `EXAMPLE_ADVISORY.md`,
`CODE_OF_CONDUCT.md`, `.github/`) and its full multi-year commit history were
intentionally **not** vendored: they are not read by either tool at check
time, and including them would multiply this directory's size for no
functional benefit (full history alone is ~40 MB vs. ~9 MB for the data
actually used).

### Why not commit it as a git repo

`cargo-audit --db <path>` happily reads a plain directory of advisory files.
`cargo-deny check advisories`, however, shells out to `git show -s
--format=%cI HEAD` against its resolved `db-path` subdirectory to report a
database timestamp, so it needs an actual git commit to exist there at check
time.

Committing that `.git` directly into this repository was considered and
rejected: `git add` from the *outer* repository treats any tracked directory
containing a `.git` entry as a nested-repository boundary and records it as a
gitlink (a bare commit-SHA pointer) instead of adding its file contents — so
the vendored advisories would silently vanish for anyone who clones this
repository normally (no submodule remote exists to resolve the gitlink
against). Plain files avoid that failure mode entirely and stay directly
diffable/greppable in review.

Instead, `cargo xtask gates` (and `cargo xtask deny`) materialize a fresh,
single-commit git repository from these plain files under gitignored
`target/xtask-vendor/rustsec-advisory-db/advisory-db-3157b0e258782691/`
immediately before invoking `cargo deny --offline check` (see
`xtask/src/vendor.rs`); `deny.toml`'s `[advisories] db-path` points there.
This is exactly a "regenerable cache": excluded from version control, but
everything required to regenerate it deterministically — the plain vendored
files plus a few lines of `xtask` Rust — is tracked.

## Refreshing this snapshot

This is a point-in-time copy, not a live mirror. To refresh it:

```sh
rm -rf /tmp/advisory-db-refresh
git clone --depth 1 https://github.com/rustsec/advisory-db.git /tmp/advisory-db-refresh
rm -rf vendor/rustsec-advisory-db/advisory-db-3157b0e258782691
mkdir -p vendor/rustsec-advisory-db/advisory-db-3157b0e258782691
cd /tmp/advisory-db-refresh
git log -1 --format='commit=%H%ndate=%cI'   # record these values below and above
cp -r crates rust LICENSE.txt LICENSES README.md support.toml \
  "$OLDPWD/vendor/rustsec-advisory-db/advisory-db-3157b0e258782691/"
```

Then update the commit hash/date recorded above and re-run `cargo xtask
gates` (or at least `cargo xtask deny` and `cargo audit --db
target/xtask-vendor/rustsec-advisory-db/advisory-db-3157b0e258782691
--no-fetch --stale`) to confirm the refreshed snapshot still parses and
reports cleanly.

A stale vendored database can only under-report newly discovered advisories
(fail safe), never fabricate a vulnerability that does not exist — but it
should still be refreshed periodically. See `docs/RETENTION_MANIFEST.md` and
`docs/AUTONOMOUS_DECISIONS.md` for how this vendoring decision fits the
project's zero-third-party-dependency and offline-first policies.

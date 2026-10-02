//! `cargo xtask capability`: read-only status of the defensive ATT&CK
//! capability ledger (`bleradar_core::CapabilityLedger`,
//! `docs/CAPABILITY_LEDGER.md`).
//!
//! Prints every technique's derived status, the corroboration/regression
//! health derived from its linked records, and the ledger's own
//! `verified_count()`. It only borrows the ledger and writes nothing but
//! stdout: no file, no ledger mutation, no status of its own.

use bleradar_core::{CapabilityLedger, CapabilityStatus};

/// `cargo xtask capability`.
pub(crate) fn cmd_capability() -> Result<(), String> {
    print!("{}", render_status(&CapabilityLedger::seed_v0()));
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// The status report for `ledger`. Every status, flag and count comes from
/// bleradar-core's derivation; this only formats it.
fn render_status(ledger: &CapabilityLedger) -> String {
    let mut out = String::from("technique  status         corroboration  regression  name\n");
    for (technique_id, row) in ledger.iter() {
        let status = ledger
            .status_of(technique_id)
            .map_or("missing", CapabilityStatus::as_str);
        out.push_str(&format!(
            "{technique_id:<10} {status:<14} {:<14} {:<11} {}\n",
            yes_no(row.links.corroboration_ok()),
            yes_no(row.links.regression_ok()),
            row.name
        ));
    }
    let counts = ledger.counts_by_status();
    let summary: Vec<String> = counts
        .iter()
        .map(|(status, count)| format!("{status}={count}"))
        .collect();
    out.push_str(&format!("counts: {}\n", summary.join(" ")));
    out.push_str(&format!("verified_count={}\n", ledger.verified_count()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bleradar_core::CapabilityEvidenceLinks;

    fn complete_links() -> CapabilityEvidenceLinks {
        CapabilityEvidenceLinks {
            source_ids: vec!["src-1".into()],
            input_ids: vec!["in-1".into()],
            execution_record_id: Some("exec-1".into()),
            output_ids: vec!["out-1".into()],
            provenance_claim_id: Some("claim-1".into()),
            corroboration_ids: vec!["corr-1".into()],
            test_ids: vec!["test-1".into()],
            benchmark_ids: vec![],
            regression_lock_ids: vec!["lock-1".into()],
            passed_test_ids: vec!["test-1".into()],
            failed_test_ids: vec![],
        }
    }

    fn line_for<'a>(report: &'a str, technique_id: &str) -> &'a str {
        report
            .lines()
            .find(|line| line.split_whitespace().next() == Some(technique_id))
            .unwrap_or_else(|| panic!("no line for {technique_id}:\n{report}"))
    }

    fn verified_count_line(report: &str) -> usize {
        report
            .lines()
            .find_map(|line| line.strip_prefix("verified_count="))
            .expect("verified_count line")
            .parse()
            .expect("verified_count number")
    }

    #[test]
    fn status_is_read_only_and_counts_equal_verified_count() {
        let mut verified = CapabilityLedger::seed_v0();
        verified
            .set_links("T1040", complete_links())
            .expect("T1040 present");
        for ledger in [CapabilityLedger::seed_v0(), verified] {
            let before = ledger.clone();
            let report = render_status(&ledger);
            assert_eq!(ledger, before, "rendering must not change the ledger");
            assert_eq!(render_status(&ledger), report, "deterministic");

            assert_eq!(verified_count_line(&report), ledger.verified_count());
            for (status, count) in ledger.counts_by_status() {
                assert!(
                    report.contains(&format!("{status}={count}")),
                    "{status}={count} missing:\n{report}"
                );
            }
            for (technique_id, row) in ledger.iter() {
                let line = line_for(&report, technique_id);
                let fields: Vec<&str> = line.split_whitespace().collect();
                let status = ledger.status_of(technique_id).expect("status");
                assert_eq!(fields[1], status.as_str(), "{line}");
                assert_eq!(fields[2], yes_no(row.links.corroboration_ok()), "{line}");
                assert_eq!(fields[3], yes_no(row.links.regression_ok()), "{line}");
            }
        }

        let seed = render_status(&CapabilityLedger::seed_v0());
        assert_eq!(verified_count_line(&seed), 0);
        assert!(line_for(&seed, "T1040").contains("Unverified"));
        assert!(line_for(&seed, "T1566").contains("NotApplicable"));
    }

    #[test]
    fn status_command_writes_nothing() {
        // The command's code (tests excluded) has no filesystem, process or
        // environment-writing call: stdout is its only output.
        let source = include_str!("capability.rs");
        let code = source
            .split("#[cfg(test)]")
            .next()
            .expect("code before tests");
        for forbidden in [
            "std::fs",
            "fs::",
            "File",
            "Command",
            "set_var",
            "remove_var",
            "set_links",
            "insert_row",
            "invalidate_test",
            "&mut",
        ] {
            assert!(!code.contains(forbidden), "{forbidden} in capability.rs");
        }
        assert!(cmd_capability().is_ok());
    }
}

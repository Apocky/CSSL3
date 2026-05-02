//! § cssl-test-coordinator · workspace-wide test orchestration (T11-D350 · W-T1)
//!
//! ‼ STUB-CONTENT · scaffold-Cargo.toml-was-tracked-but-src-was-orphaned during
//!   parallel-fanout · src/lib.rs lost during cross-branch shuffle. This file
//!   reinstates a minimal-buildable surface so workspace-cargo-check passes.
//!   Full implementation lands in a follow-up wave when test-orchestration is
//!   prioritized again.
//!
//! § PLANNED-API (forward-looking · ¬ wired)
//!   - run_workspace_tests(filters: ...) -> CoordinatorReport
//!   - parse_cargo_test_json(stream: impl Read) -> PerCrateResults
//!   - generate_html_report(report: &CoordinatorReport, out: &Path) -> Result<()>
//!   - regression_check(prior: &CoordinatorReport, current: &CoordinatorReport) -> Vec<Regression>

#![allow(dead_code)]

/// § placeholder · returns a stable build-marker so the workspace links cleanly.
pub fn coordinator_marker() -> &'static str {
    "cssl-test-coordinator · stub · T11-D350 · awaiting-impl"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_stable() {
        assert!(coordinator_marker().starts_with("cssl-test-coordinator"));
    }
}

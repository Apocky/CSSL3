// § report.rs : campaign-level aggregation + text rendering
// ══════════════════════════════════════════════════════════════════
// § I> aggregate ∀ RunResult into CampaignReport with verdict-tallies
// § I> render_text emits human-readable summary for CI logs

use crate::golden::GoldenVerdict;
use serde::{Deserialize, Serialize};

/// Single golden-comparison run within a campaign.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    pub label: String,
    pub verdict: GoldenVerdict,
    pub percent_diff: Option<f32>,
    pub max_delta: Option<u8>,
}

/// Aggregate report across many [`RunResult`] entries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CampaignReport {
    pub runs: Vec<RunResult>,
    pub ran_at_iso: String,
    pub total_runs: u64,
    pub matches: u64,
    pub drifts: u64,
    pub regressions: u64,
    pub no_golden: u64,
    pub dim_mismatches: u64,
}

impl CampaignReport {
    /// Build a report by tallying verdicts in `runs`.
    pub fn from_runs(runs: Vec<RunResult>, ran_at_iso: String) -> CampaignReport {
        let mut matches = 0;
        let mut drifts = 0;
        let mut regressions = 0;
        let mut no_golden = 0;
        let mut dim_mismatches = 0;
        for r in &runs {
            match r.verdict {
                GoldenVerdict::Match => matches += 1,
                GoldenVerdict::MinorDrift => drifts += 1,
                GoldenVerdict::MajorRegression => regressions += 1,
                GoldenVerdict::NoGolden => no_golden += 1,
                GoldenVerdict::DimensionMismatch => dim_mismatches += 1,
            }
        }
        let total_runs = runs.len() as u64;
        CampaignReport {
            runs,
            ran_at_iso,
            total_runs,
            matches,
            drifts,
            regressions,
            no_golden,
            dim_mismatches,
        }
    }
}

/// Render a campaign report as a plain-text summary.
pub fn render_text(report: &CampaignReport) -> String {
    let mut s = String::new();
    s.push_str("§ cssl-host-golden campaign-report\n");
    s.push_str(&format!("ran-at        : {}\n", report.ran_at_iso));
    s.push_str(&format!("total-runs    : {}\n", report.total_runs));
    s.push_str(&format!("matches       : {}\n", report.matches));
    s.push_str(&format!("minor-drifts  : {}\n", report.drifts));
    s.push_str(&format!("regressions   : {}\n", report.regressions));
    s.push_str(&format!("no-golden     : {}\n", report.no_golden));
    s.push_str(&format!("dim-mismatches: {}\n", report.dim_mismatches));
    s.push_str("─ runs ─\n");
    for r in &report.runs {
        let pct = r.percent_diff.map_or("-".to_string(), |p| format!("{p:.3}%"));
        let delta = r.max_delta.map_or("-".to_string(), |d| d.to_string());
        s.push_str(&format!(
            "  {:<24} {:?}  pct={}  max-Δ={}\n",
            r.label, r.verdict, pct, delta
        ));
    }
    s
}

// ─────────────────────────────────────────────────────────────────
// tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report() {
        let rep = CampaignReport::from_runs(vec![], "2026-04-30T00:00:00Z".to_string());
        assert_eq!(rep.total_runs, 0);
        assert_eq!(rep.matches, 0);
        assert_eq!(rep.drifts, 0);
        assert_eq!(rep.regressions, 0);
        assert_eq!(rep.no_golden, 0);
        assert_eq!(rep.dim_mismatches, 0);
        let text = render_text(&rep);
        assert!(text.contains("total-runs    : 0"));
        assert!(text.contains("matches       : 0"));
    }

    #[test]
    fn multi_run_counts_correct() {
        let runs = vec![
            RunResult {
                label: "alpha".into(),
                verdict: GoldenVerdict::Match,
                percent_diff: Some(0.0),
                max_delta: Some(0),
            },
            RunResult {
                label: "beta".into(),
                verdict: GoldenVerdict::Match,
                percent_diff: Some(0.0),
                max_delta: Some(0),
            },
            RunResult {
                label: "gamma".into(),
                verdict: GoldenVerdict::MinorDrift,
                percent_diff: Some(0.4),
                max_delta: Some(7),
            },
            RunResult {
                label: "delta".into(),
                verdict: GoldenVerdict::MajorRegression,
                percent_diff: Some(34.5),
                max_delta: Some(255),
            },
            RunResult {
                label: "epsilon".into(),
                verdict: GoldenVerdict::NoGolden,
                percent_diff: None,
                max_delta: None,
            },
            RunResult {
                label: "zeta".into(),
                verdict: GoldenVerdict::DimensionMismatch,
                percent_diff: None,
                max_delta: None,
            },
        ];
        let rep = CampaignReport::from_runs(runs, "2026-04-30T00:00:00Z".to_string());
        assert_eq!(rep.total_runs, 6);
        assert_eq!(rep.matches, 2);
        assert_eq!(rep.drifts, 1);
        assert_eq!(rep.regressions, 1);
        assert_eq!(rep.no_golden, 1);
        assert_eq!(rep.dim_mismatches, 1);
        let text = render_text(&rep);
        assert!(text.contains("alpha"));
        assert!(text.contains("zeta"));
        assert!(text.contains("regressions   : 1"));
    }
}

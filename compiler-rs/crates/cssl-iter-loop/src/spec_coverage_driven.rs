//! Spec-coverage-driven gap-prioritization.
//!
//! Mirrors `_drafts/phase_j/wave_ji_iteration_loop_docs.md` § 3 — agents pick
//! implementation-gaps from a PRIORITIZED gap-list. This module turns a
//! `cssl_spec_coverage::SpecCoverageReport` into a ranked `Vec<GapPriority>`
//! and exposes `pick_largest_gap` for the canonical "largest non-overlapping
//! HIGH-priority gap" pattern.
//!
//! § Priority-score heuristic (recipe per § 3 + telemetry-spec § IV)
//!   priority_score =
//!         100 × (impl_pct < 100 weighted)        ← missing > stub > partial
//!       +  50 × (test_pct < 100 weighted)        ← test-gap is half-weighted
//!       +  20 × is_high_urgency_section          ← invariant-class boost
//!       +  10 × stale_spec                       ← stale spec eats freshness
//!       +   5 × no_owner_lock                    ← unclaimed = pickable-now
//!
//! Higher score ⇒ pick first. Stable tie-breaker is the SpecAnchor key().
//!
//! § Σ-discipline
//!   This module never touches cells, entities, or biometric data. Inputs
//!   are coverage-report metadata only.

use cssl_spec_coverage::{ImplStatus, SpecAnchor, SpecCoverageReport, TestStatus};
use serde::{Deserialize, Serialize};

/// A single ranked gap-entry. Carries enough metadata to drive an agent
/// pod-claim decision without re-reading the underlying report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GapPriority {
    /// Stable identifier — `<root>::<file>::<section>`.
    pub spec_anchor_key: String,
    /// Impl status discriminant ("Implemented" | "Partial" | "Stub" | "Missing").
    pub impl_status: String,
    /// Test status discriminant ("Tested" | "Partial" | "Untested" | "NoTests").
    pub test_status: String,
    /// Numeric score ; higher = pick first. See module-doc heuristic.
    pub priority_score: u32,
    /// True when the spec-file mtime > impl-file mtime ⇒ spec is ahead of code.
    pub stale: bool,
    /// True when this gap has no owning agent / pod claim yet.
    pub unclaimed: bool,
}

/// A complete gap-ranking. Sorted descending by priority_score with stable
/// tie-breaker on the spec_anchor_key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GapRanking {
    pub entries: Vec<GapPriority>,
    /// Total count of input anchors considered (not just gaps).
    pub anchors_considered: usize,
}

impl GapRanking {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn top(&self) -> Option<&GapPriority> {
        self.entries.first()
    }
}

/// Errors during spec-coverage-driven gap-picking.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpecCoverageDrivenError {
    /// The supplied report has no gap-entries to rank.
    #[error("no gaps to rank")]
    NoGaps,
}

/// Wrapper passing the input data the gap-ranker actually needs. Keeps the
/// public API tight even if the upstream `SpecCoverageReport` evolves.
#[derive(Debug, Clone)]
pub struct GapCoverageInput<'a> {
    pub report: &'a SpecCoverageReport,
    /// Optional filter : if Some(set), only anchors whose key is in the set
    /// are considered. Used by pod-coordination so claimed gaps are skipped.
    pub claimed_keys: Option<&'a [String]>,
    /// Optional filter : if Some, only anchors whose section starts with
    /// this prefix are considered. Used to scope a slice to one spec-file.
    pub section_prefix_filter: Option<&'a str>,
}

impl<'a> GapCoverageInput<'a> {
    pub fn new(report: &'a SpecCoverageReport) -> Self {
        Self {
            report,
            claimed_keys: None,
            section_prefix_filter: None,
        }
    }

    pub fn with_claimed_keys(mut self, keys: &'a [String]) -> Self {
        self.claimed_keys = Some(keys);
        self
    }

    pub fn with_section_prefix(mut self, prefix: &'a str) -> Self {
        self.section_prefix_filter = Some(prefix);
        self
    }
}

/// Pick the single largest gap from a coverage report. Returns `None` when
/// the report has no gaps.
///
/// Convenience for the canonical agent-pick-one-gap loop ; equivalent to
/// `rank_gaps(report).entries.first().cloned()`.
pub fn pick_largest_gap(report: &SpecCoverageReport) -> Option<GapPriority> {
    let ranking = rank_gaps(report);
    ranking.entries.into_iter().next()
}

/// Rank all gaps in a coverage report into a prioritized list. Higher
/// `priority_score` first ; stable tie-breaker on the spec_anchor_key.
///
/// The input report is consumed via `gaps` + `pending_todos` ; both are
/// already classified as gaps by the upstream extractor. We deduplicate on
/// the key so a gap appearing in BOTH gaps + pending_todos is ranked once.
pub fn rank_gaps(report: &SpecCoverageReport) -> GapRanking {
    rank_gaps_with_input(GapCoverageInput::new(report))
}

/// Full-control variant : honors claimed_keys + section_prefix_filter from
/// the supplied `GapCoverageInput`.
pub fn rank_gaps_with_input(input: GapCoverageInput<'_>) -> GapRanking {
    let mut entries: Vec<GapPriority> = Vec::new();
    let mut seen_keys: Vec<String> = Vec::new();

    let mut total_considered: usize = 0;

    let emit = |entry: GapPriority, seen: &mut Vec<String>, out: &mut Vec<GapPriority>| {
        if !seen.iter().any(|k| k == &entry.spec_anchor_key) {
            seen.push(entry.spec_anchor_key.clone());
            out.push(entry);
        }
    };

    for src in [&input.report.gaps, &input.report.pending_todos] {
        for entry in src.iter() {
            total_considered += 1;
            if let Some(prefix) = input.section_prefix_filter {
                if !entry.section.starts_with(prefix) {
                    continue;
                }
            }
            if let Some(claimed) = input.claimed_keys {
                if claimed.iter().any(|c| c == &entry.key) {
                    continue;
                }
            }
            let priority = score_report_entry(entry, &input);
            emit(priority, &mut seen_keys, &mut entries);
        }
    }

    // Sort descending by score, ascending by key for tie-stable.
    entries.sort_by(|a, b| {
        b.priority_score
            .cmp(&a.priority_score)
            .then_with(|| a.spec_anchor_key.cmp(&b.spec_anchor_key))
    });

    GapRanking {
        entries,
        anchors_considered: total_considered.max(input.report.total_anchors),
    }
}

/// Compute the priority score for a single ReportEntry.
fn score_report_entry(
    entry: &cssl_spec_coverage::ReportEntry,
    input: &GapCoverageInput<'_>,
) -> GapPriority {
    let mut score: u32 = 0;

    // Impl-status weight : Missing > Stub > Partial > Implemented.
    score += match entry.impl_status.as_str() {
        "Missing" => 100,
        "Stub" => 80,
        "Partial" => 40,
        _ => 0,
    };

    // Test-status weight : Untested half-weighted ; Partial third-weighted.
    score += match entry.test_status.as_str() {
        "Untested" => 50,
        "Partial" => 25,
        _ => 0,
    };

    // High-urgency section heuristic : invariant-bearing § (e.g. § III, § V) get a boost.
    let high_urgency_markers = ["§ III", "§ V", "§ VII", "Invariants", "PRIME-DIRECTIVE"];
    if high_urgency_markers
        .iter()
        .any(|m| entry.section.contains(m))
    {
        score += 20;
    }

    // Stale-spec boost — the report's `stale` bucket carries this. We check
    // membership by key.
    let stale = input.report.stale.iter().any(|s| s.key == entry.key);
    if stale {
        score += 10;
    }

    // Unclaimed boost — if the caller didn't supply a claim-set, treat all
    // as unclaimed ; otherwise honor the absence.
    let unclaimed = match input.claimed_keys {
        None => true,
        Some(claimed) => !claimed.iter().any(|k| k == &entry.key),
    };
    if unclaimed {
        score += 5;
    }

    GapPriority {
        spec_anchor_key: entry.key.clone(),
        impl_status: entry.impl_status.clone(),
        test_status: entry.test_status.clone(),
        priority_score: score,
        stale,
        unclaimed,
    }
}

/// Helper : score a single anchor directly. Used by tests + by callers that
/// build a one-off priority outside the report-shaped API.
pub fn score_anchor(anchor: &SpecAnchor) -> u32 {
    let mut score: u32 = 0;
    score += match &anchor.impl_status {
        ImplStatus::Missing => 100,
        ImplStatus::Stub { .. } => 80,
        ImplStatus::Partial { .. } => 40,
        ImplStatus::Implemented { .. } => 0,
    };
    score += match &anchor.test_status {
        TestStatus::Untested => 50,
        TestStatus::Partial { .. } => 25,
        _ => 0,
    };
    let high_urgency_markers = ["§ III", "§ V", "§ VII", "Invariants", "PRIME-DIRECTIVE"];
    if high_urgency_markers
        .iter()
        .any(|m| anchor.section.contains(m))
    {
        score += 20;
    }
    if anchor.is_stale() {
        score += 10;
    }
    score += 5; // unclaimed default
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_spec_coverage::{ImplConfidence, ImplStatus, SpecAnchorBuilder, SpecRoot, TestStatus};

    fn missing(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Missing)
            .test_status(TestStatus::Untested)
            .build()
    }

    fn stub(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Stub {
                crate_path: "cssl-foo".into(),
            })
            .test_status(TestStatus::Untested)
            .build()
    }

    fn partial(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Partial {
                crate_path: "cssl-foo".into(),
                gaps: vec!["edge-case-x".into()],
            })
            .test_status(TestStatus::Partial {
                test_paths: vec!["t::a".into()],
                uncovered_criteria: vec!["c1".into()],
            })
            .build()
    }

    fn implemented(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Implemented {
                crate_path: "cssl-foo".into(),
                primary_module: "crate::bar".into(),
                confidence: ImplConfidence::High,
                impl_date: "2026-04-29".into(),
            })
            .test_status(TestStatus::Tested {
                test_paths: vec!["t".into()],
                last_pass_date: "2026-04-29".into(),
            })
            .build()
    }

    #[test]
    fn pick_largest_gap_returns_missing_first() {
        let anchors = vec![
            implemented("a.csl", "§ I"),
            partial("b.csl", "§ II"),
            stub("c.csl", "§ I"),
            missing("d.csl", "§ V"),
        ];
        let report = SpecCoverageReport::build(anchors.iter());
        let top = pick_largest_gap(&report).unwrap();
        // Missing(§V) outscores Stub + Partial : 100 + 50 + 20 + 5 = 175.
        assert!(top.spec_anchor_key.contains("d.csl"));
        assert!(top.priority_score >= 100);
    }

    #[test]
    fn pick_largest_gap_handles_empty_report() {
        let anchors = vec![implemented("a.csl", "§ I")];
        let report = SpecCoverageReport::build(anchors.iter());
        let top = pick_largest_gap(&report);
        assert!(top.is_none());
    }

    #[test]
    fn rank_gaps_orders_by_score_descending() {
        let anchors = vec![
            partial("a.csl", "§ I"),
            stub("b.csl", "§ I"),
            missing("c.csl", "§ V"),
        ];
        let report = SpecCoverageReport::build(anchors.iter());
        let r = rank_gaps(&report);
        assert!(r.len() >= 2);
        for w in r.entries.windows(2) {
            assert!(w[0].priority_score >= w[1].priority_score);
        }
    }

    #[test]
    fn rank_gaps_tie_breaks_stably_by_key() {
        let anchors = vec![missing("z.csl", "§ I"), missing("a.csl", "§ I")];
        let report = SpecCoverageReport::build(anchors.iter());
        let r = rank_gaps(&report);
        // Both same score ; key-asc tie-breaker ⇒ a.csl first.
        assert!(r.entries[0].spec_anchor_key.contains("a.csl"));
    }

    #[test]
    fn rank_gaps_skips_claimed_keys() {
        let anchors = vec![missing("a.csl", "§ I"), missing("b.csl", "§ II")];
        let report = SpecCoverageReport::build(anchors.iter());
        let claimed = vec![format!("CssLv3::{}::{}", "a.csl", "§ I")];
        let r = rank_gaps_with_input(GapCoverageInput::new(&report).with_claimed_keys(&claimed));
        // Only b.csl remains.
        assert_eq!(r.entries.len(), 1);
        assert!(r.entries[0].spec_anchor_key.contains("b.csl"));
    }

    #[test]
    fn rank_gaps_filters_by_section_prefix() {
        let anchors = vec![missing("a.csl", "§ I"), missing("b.csl", "§ V")];
        let report = SpecCoverageReport::build(anchors.iter());
        let r = rank_gaps_with_input(GapCoverageInput::new(&report).with_section_prefix("§ V"));
        assert_eq!(r.entries.len(), 1);
        assert!(r.entries[0].spec_anchor_key.contains("b.csl"));
    }

    #[test]
    fn score_anchor_missing_outranks_stub() {
        let m = missing("a.csl", "§ V");
        let s = stub("a.csl", "§ V");
        assert!(score_anchor(&m) > score_anchor(&s));
    }

    #[test]
    fn high_urgency_section_marker_boosts_score() {
        let plain = missing("a.csl", "§ X");
        let urgent = missing("a.csl", "§ V");
        assert!(score_anchor(&urgent) > score_anchor(&plain));
    }
}

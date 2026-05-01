//! § summary statistics over decoded trace data
//!
//! Aggregates per-label statistics (count · total · mean · p50 · p99) plus a
//! top-line global rollup. Output is deterministic across runs : the
//! `by_label` map is a `BTreeMap` so iteration is alphabetical.
//!
//! ## Percentiles
//!
//! Computed via the simple "sort + index" method. For N samples,
//! `p_k = sorted[ceil(N * k / 100) − 1]`. Adequate for N up to a few million ;
//! W6-extension may swap in a t-digest when ring-drains exceed that budget.

use crate::pair::MarkPair;
use cssl_host_rt_trace::{LabelInterner, RtEvent};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § per-label aggregate statistics.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LabelStats {
    /// Number of pair-occurrences for this label.
    pub count: u64,
    /// Sum of all `duration_us` values across the occurrences.
    pub total_us: u64,
    /// Arithmetic mean ; 0.0 when count is 0.
    pub mean_us: f64,
    /// 50th-percentile duration (microseconds).
    pub p50_us: u64,
    /// 99th-percentile duration (microseconds).
    pub p99_us: u64,
}

/// § global rollup + per-label breakdown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TraceSummary {
    /// Total raw events seen (begin + end + counter + ...).
    pub total_events: u64,
    /// Total successfully-paired mark-regions.
    pub total_pairs: u64,
    /// Sum of all paired-region durations.
    pub total_duration_us: u64,
    /// Count of the label that occurred most frequently.
    pub top_label_count: u64,
    /// String of the label that occurred most frequently. Empty when no pairs.
    pub top_label: String,
    /// Per-label aggregate statistics (alphabetical key order).
    pub by_label: BTreeMap<String, LabelStats>,
}

/// § percentile helper · `p` is the percentile in [0, 100].
fn percentile(sorted: &[u64], p: u32) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    // `ceil(N * p / 100) − 1` ; clamp into [0, N-1].
    let n = sorted.len() as u64;
    let idx = ((n.saturating_mul(u64::from(p)) + 99) / 100).saturating_sub(1);
    let idx_clamped = (idx as usize).min(sorted.len() - 1);
    sorted[idx_clamped]
}

/// § summarize raw events + paired marks against an interner.
#[must_use]
pub fn summarize(
    events: &[RtEvent],
    _interner: &LabelInterner,
    pairs: &[MarkPair],
) -> TraceSummary {
    // Per-label durations grouped for percentile computation.
    let mut by_label_durations: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut total_duration_us: u64 = 0;
    for p in pairs {
        by_label_durations
            .entry(p.label.clone())
            .or_default()
            .push(p.duration_us);
        total_duration_us = total_duration_us.saturating_add(p.duration_us);
    }

    // Reduce each bucket to LabelStats.
    let mut by_label: BTreeMap<String, LabelStats> = BTreeMap::new();
    let mut top_label = String::new();
    let mut top_label_count: u64 = 0;
    for (label, mut durations) in by_label_durations {
        durations.sort_unstable();
        let count = durations.len() as u64;
        let total_us: u64 = durations.iter().sum();
        let mean_us = if count == 0 {
            0.0
        } else {
            // § lossy-but-acceptable : mean is a display-stat ; precision
            // beyond 2^52 micros (≈ 142 years) is not meaningful here.
            #[allow(clippy::cast_precision_loss)]
            let mean = total_us as f64 / count as f64;
            mean
        };
        let p50_us = percentile(&durations, 50);
        let p99_us = percentile(&durations, 99);
        if count > top_label_count {
            top_label_count = count;
            top_label = label.clone();
        }
        by_label.insert(
            label,
            LabelStats {
                count,
                total_us,
                mean_us,
                p50_us,
                p99_us,
            },
        );
    }

    TraceSummary {
        total_events: events.len() as u64,
        total_pairs: pairs.len() as u64,
        total_duration_us,
        top_label_count,
        top_label,
        by_label,
    }
}

/// § render a [`TraceSummary`] to a one-page text report.
#[must_use]
pub fn render_text(summary: &TraceSummary) -> String {
    let mut out = String::new();
    out.push_str("§ TraceSummary\n");
    out.push_str(&format!("  total_events     : {}\n", summary.total_events));
    out.push_str(&format!("  total_pairs      : {}\n", summary.total_pairs));
    out.push_str(&format!(
        "  total_duration_us: {}\n",
        summary.total_duration_us
    ));
    out.push_str(&format!(
        "  top_label        : {} (count={})\n",
        if summary.top_label.is_empty() {
            "<none>"
        } else {
            &summary.top_label
        },
        summary.top_label_count
    ));
    out.push_str("\n  by_label :\n");
    for (label, s) in &summary.by_label {
        out.push_str(&format!(
            "    {label:30} count={count:>6} total_us={total:>10} mean_us={mean:>10.2} p50={p50:>8} p99={p99:>8}\n",
            label = label,
            count = s.count,
            total = s.total_us,
            mean = s.mean_us,
            p50 = s.p50_us,
            p99 = s.p99_us,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_rt_trace::{RtEvent, RtEventKind};

    fn pair(label: &str, dur: u64) -> MarkPair {
        MarkPair {
            label: label.to_owned(),
            start_ts: 0,
            end_ts: dur,
            duration_us: dur,
            depth: 0,
        }
    }

    fn raw_event() -> RtEvent {
        RtEvent::new(0, RtEventKind::Counter, 0)
    }

    #[test]
    fn empty_summary_zeros() {
        let interner = LabelInterner::default();
        let s = summarize(&[], &interner, &[]);
        assert_eq!(s.total_events, 0);
        assert_eq!(s.total_pairs, 0);
        assert_eq!(s.total_duration_us, 0);
        assert!(s.by_label.is_empty());
        assert_eq!(s.top_label, "");
        assert_eq!(s.top_label_count, 0);
    }

    #[test]
    fn summary_aggregates_basic() {
        let interner = LabelInterner::default();
        let events = vec![raw_event(); 7];
        let pairs = vec![pair("a", 100), pair("a", 200), pair("b", 50)];
        let s = summarize(&events, &interner, &pairs);
        assert_eq!(s.total_events, 7);
        assert_eq!(s.total_pairs, 3);
        assert_eq!(s.total_duration_us, 350);
        let a = s.by_label.get("a").expect("a present");
        assert_eq!(a.count, 2);
        assert_eq!(a.total_us, 300);
        assert!((a.mean_us - 150.0).abs() < 1e-9);
        let b = s.by_label.get("b").expect("b present");
        assert_eq!(b.count, 1);
        assert_eq!(b.total_us, 50);
    }

    #[test]
    fn top_label_is_most_frequent() {
        let interner = LabelInterner::default();
        let pairs = vec![
            pair("frame", 10),
            pair("frame", 12),
            pair("frame", 15),
            pair("draw", 5),
            pair("draw", 7),
        ];
        let s = summarize(&[], &interner, &pairs);
        assert_eq!(s.top_label, "frame");
        assert_eq!(s.top_label_count, 3);
    }

    #[test]
    fn text_rendering_includes_headers() {
        let interner = LabelInterner::default();
        let pairs = vec![pair("alpha", 100), pair("beta", 200)];
        let s = summarize(&[], &interner, &pairs);
        let txt = render_text(&s);
        assert!(txt.contains("§ TraceSummary"));
        assert!(txt.contains("total_events"));
        assert!(txt.contains("alpha"));
        assert!(txt.contains("beta"));
        assert!(txt.contains("by_label"));
    }
}

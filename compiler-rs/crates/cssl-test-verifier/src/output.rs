// § output.rs : render VerificationReport → JSON | human (CSL-glyph-dense)
// ══════════════════════════════════════════════════════════════════
// human-mode glyphs : ✓ pass · ✗ fail · ⚠ silent-fallback · ℹ unexpected
// ══════════════════════════════════════════════════════════════════

use crate::manifest::CountSpec;
use crate::verify::{Failure, VerificationReport};

pub fn render_json(report: &VerificationReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".into())
}

fn fmt_count(c: &CountSpec) -> String {
    match c {
        CountSpec::Exact(n) => format!("={}", n),
        CountSpec::Range(lo, hi) => format!("[{},{}]", lo, hi),
        CountSpec::AtLeast(n) => format!("≥{}", n),
        CountSpec::AtMost(n) => format!("≤{}", n),
        CountSpec::Any => "≥1".to_string(),
    }
}

pub fn render_human(report: &VerificationReport) -> String {
    let mut out = String::new();
    let banner = if report.passed {
        "✓ VERIFY-PASS"
    } else {
        "✗ VERIFY-FAIL"
    };
    out.push_str(&format!(
        "§ {} · events={} · expectations={} · passed={} · failed={}\n",
        banner,
        report.events_total,
        report.expectations_total,
        report.passed_count,
        report.failed_count
    ));

    if report.failures.is_empty() {
        out.push_str("  ✓ ¬ failures\n");
    } else {
        out.push_str(&format!("  ✗ failures ({}) :\n", report.failures.len()));
        for f in &report.failures {
            match f {
                Failure::MissingRequired { op, kind, expected_count } => {
                    out.push_str(&format!(
                        "    ✗ MISSING        · op={} kind={} count{}\n",
                        op,
                        kind,
                        fmt_count(expected_count)
                    ));
                }
                Failure::CountDeviation { op, kind, expected, actual } => {
                    out.push_str(&format!(
                        "    ✗ COUNT-DEV      · op={} kind={} expected{} actual={}\n",
                        op,
                        kind,
                        fmt_count(expected),
                        actual
                    ));
                }
                Failure::Forbidden { op, kind, index } => {
                    out.push_str(&format!(
                        "    ✗ FORBIDDEN-HIT  · op={} kind={} @idx={}\n",
                        op, kind, index
                    ));
                }
                Failure::OrderViolation {
                    earlier_op,
                    later_op,
                    found_at_indices,
                } => {
                    let (ia, ib) = found_at_indices;
                    let ia_s = if *ia == usize::MAX {
                        "∅".to_string()
                    } else {
                        ia.to_string()
                    };
                    out.push_str(&format!(
                        "    ✗ ORDER-VIOLATE  · {} → {} · idx_a={} idx_b={}\n",
                        earlier_op, later_op, ia_s, ib
                    ));
                }
                Failure::ResultPredicateFailed {
                    op,
                    kind,
                    predicate,
                    actual,
                } => {
                    out.push_str(&format!(
                        "    ✗ RESULT-PRED    · op={} kind={} pred={{{}}} actual={}\n",
                        op, kind, predicate, actual
                    ));
                }
                Failure::LatencyExceeded {
                    op,
                    kind,
                    expected_max_ns,
                    actual_ns,
                    index,
                } => {
                    out.push_str(&format!(
                        "    ✗ LATENCY-OVER   · op={} kind={} max_ns={} actual_ns={} @idx={}\n",
                        op, kind, expected_max_ns, actual_ns, index
                    ));
                }
                Failure::NoObservableEffect { rt_internal_event_count } => {
                    out.push_str(&format!(
                        "    ✗ NO-OBSERVABLE  · require-observable gate · rt_internal_only={}\n",
                        rt_internal_event_count
                    ));
                }
            }
        }
    }

    if !report.silent_fallbacks.is_empty() {
        out.push_str(&format!(
            "  ⚠ silent-fallbacks ({}) ← skip-events ∉ allow-skip :\n",
            report.silent_fallbacks.len()
        ));
        for sf in &report.silent_fallbacks {
            let note = sf.note.as_deref().unwrap_or("");
            out.push_str(&format!(
                "    ⚠ SKIP            · op={} @idx={} note={}\n",
                sf.op, sf.index, note
            ));
        }
    }

    if !report.unexpected_events.is_empty() {
        out.push_str(&format!(
            "  ℹ unexpected ({}) ← in-trace ∉ manifest :\n",
            report.unexpected_events.len()
        ));
        for ue in &report.unexpected_events {
            out.push_str(&format!(
                "    ℹ EXTRA           · op={} kind={} @idx={}\n",
                ue.op, ue.kind, ue.index
            ));
        }
    }

    out
}

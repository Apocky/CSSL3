// § verify.rs : comparator ← manifest × trace → VerificationReport
// ══════════════════════════════════════════════════════════════════
// § I> ordering ← events sorted by ts_ns ascending @ load-time
// § I> count ← entries-only (kind matches required.kind) ; pair-counting
//   handled by separate require for entry+exit if author wants it
// § I> silent-fallback ← any kind=skip ∉ allowed-skips
// § I> unexpected ← any (op,kind) ∉ ⋃(required ∪ forbidden ∪ allowed-skips)
//                   reported informationally only ; ¬ fail-trigger by default
// ══════════════════════════════════════════════════════════════════

use crate::events::{Event, EventKind};
use crate::manifest::{CountSpec, Manifest, PredOp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Failure {
    MissingRequired {
        op: String,
        kind: String,
        expected_count: CountSpec,
    },
    CountDeviation {
        op: String,
        kind: String,
        expected: CountSpec,
        actual: u64,
    },
    Forbidden {
        op: String,
        kind: String,
        index: usize,
    },
    OrderViolation {
        earlier_op: String,
        later_op: String,
        found_at_indices: (usize, usize),
    },
    ResultPredicateFailed {
        op: String,
        kind: String,
        predicate: String,
        actual: serde_json::Value,
    },
    LatencyExceeded {
        op: String,
        kind: String,
        expected_max_ns: u64,
        actual_ns: u64,
        index: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnexpectedEvent {
    pub op: String,
    pub kind: String,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilentFallback {
    pub op: String,
    pub index: usize,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub passed: bool,
    pub events_total: u64,
    pub expectations_total: u64,
    pub passed_count: u64,
    pub failed_count: u64,
    pub failures: Vec<Failure>,
    pub unexpected_events: Vec<UnexpectedEvent>,
    pub silent_fallbacks: Vec<SilentFallback>,
}

fn json_path<'a>(value: &'a serde_json::Value, path: &[String]) -> Option<&'a serde_json::Value> {
    let mut cur = value;
    for k in path {
        cur = cur.get(k)?;
    }
    Some(cur)
}

fn pred_evaluate(pred: &PredOp, actual: Option<&serde_json::Value>) -> bool {
    match (pred, actual) {
        (PredOp::Exists, Some(v)) => !v.is_null(),
        (PredOp::Exists, None) => false,
        (PredOp::Eq(want), Some(got)) => want == got,
        (PredOp::Eq(_), None) => false,
        (PredOp::Ne(want), Some(got)) => want != got,
        (PredOp::Ne(_), None) => true, // != X holds when value absent
        (PredOp::Gt(n), Some(got)) => got.as_f64().is_some_and(|x| x > *n),
        (PredOp::Lt(n), Some(got)) => got.as_f64().is_some_and(|x| x < *n),
        (PredOp::Gt(_), None) | (PredOp::Lt(_), None) => false,
    }
}

pub fn verify(manifest: &Manifest, events: &[Event]) -> VerificationReport {
    let mut failures: Vec<Failure> = Vec::new();
    let mut unexpected: Vec<UnexpectedEvent> = Vec::new();
    let mut silent: Vec<SilentFallback> = Vec::new();
    let mut passed_count: u64 = 0;

    let expectations_total = (manifest.required.len()
        + manifest.forbidden.len()
        + manifest.orderings.len()
        + manifest.latency_bounds.len()
        + manifest.result_predicates.len()) as u64;

    // (1) required count tolerance
    for req in &manifest.required {
        let n = events
            .iter()
            .filter(|e| e.op == req.op && e.kind == req.kind)
            .count() as u64;
        if n == 0 {
            failures.push(Failure::MissingRequired {
                op: req.op.clone(),
                kind: req.kind.as_str().to_string(),
                expected_count: req.count.clone(),
            });
        } else if !req.count.matches(n) {
            failures.push(Failure::CountDeviation {
                op: req.op.clone(),
                kind: req.kind.as_str().to_string(),
                expected: req.count.clone(),
                actual: n,
            });
        } else {
            passed_count += 1;
        }
    }

    // (2) forbidden — must NOT appear (kind optional; None = any kind)
    for f in &manifest.forbidden {
        let mut hit_any = false;
        for (i, e) in events.iter().enumerate() {
            if e.op == f.op && f.kind.map_or(true, |k| k == e.kind) {
                failures.push(Failure::Forbidden {
                    op: f.op.clone(),
                    kind: e.kind.as_str().to_string(),
                    index: i,
                });
                hit_any = true;
            }
        }
        if !hit_any {
            passed_count += 1;
        }
    }

    // (3) orderings — every occurrence of later_op must be preceded by some earlier_op
    for o in &manifest.orderings {
        let earliest_a = events.iter().enumerate().find_map(|(i, e)| {
            if e.op == o.earlier_op && e.kind == EventKind::Entry {
                Some(i)
            } else {
                None
            }
        });
        let earliest_b = events.iter().enumerate().find_map(|(i, e)| {
            if e.op == o.later_op && e.kind == EventKind::Entry {
                Some(i)
            } else {
                None
            }
        });
        match (earliest_a, earliest_b) {
            (Some(ia), Some(ib)) if ia < ib => passed_count += 1,
            (Some(ia), Some(ib)) => {
                failures.push(Failure::OrderViolation {
                    earlier_op: o.earlier_op.clone(),
                    later_op: o.later_op.clone(),
                    found_at_indices: (ia, ib),
                });
            }
            (None, Some(ib)) => {
                failures.push(Failure::OrderViolation {
                    earlier_op: o.earlier_op.clone(),
                    later_op: o.later_op.clone(),
                    found_at_indices: (usize::MAX, ib),
                });
            }
            // if later_op never occurs, ordering trivially holds (no later witness)
            (_, None) => passed_count += 1,
        }
    }

    // (4) latency bounds — per-event max_ns
    for lb in &manifest.latency_bounds {
        let mut violation = false;
        for (i, e) in events.iter().enumerate() {
            if e.op == lb.op && e.kind == lb.kind {
                if let Some(ns) = e.latency_ns {
                    if ns > lb.max_ns {
                        failures.push(Failure::LatencyExceeded {
                            op: lb.op.clone(),
                            kind: lb.kind.as_str().to_string(),
                            expected_max_ns: lb.max_ns,
                            actual_ns: ns,
                            index: i,
                        });
                        violation = true;
                    }
                }
            }
        }
        if !violation {
            passed_count += 1;
        }
    }

    // (5) result predicates — every matching event must satisfy
    for rp in &manifest.result_predicates {
        let mut violation = false;
        let mut any_match = false;
        for e in events {
            if e.op == rp.op && e.kind == rp.kind {
                any_match = true;
                let actual = json_path(&e.result, &rp.path);
                if !pred_evaluate(&rp.pred, actual) {
                    failures.push(Failure::ResultPredicateFailed {
                        op: rp.op.clone(),
                        kind: rp.kind.as_str().to_string(),
                        predicate: rp.raw.clone(),
                        actual: actual.cloned().unwrap_or(serde_json::Value::Null),
                    });
                    violation = true;
                }
            }
        }
        if !violation && any_match {
            passed_count += 1;
        }
        // if no matching events at all, the predicate is vacuously satisfied
        // — count violation falls under (1) MissingRequired
        if !violation && !any_match {
            passed_count += 1;
        }
    }

    // (6) silent-fallback alarms : any kind=skip not in allowed_skips
    for (i, e) in events.iter().enumerate() {
        if e.kind == EventKind::Skip {
            let allowed = manifest.allowed_skips.iter().any(|a| a.op == e.op);
            if !allowed {
                silent.push(SilentFallback {
                    op: e.op.clone(),
                    index: i,
                    note: e.note.clone(),
                });
            }
        }
    }

    // (7) unexpected events : (op,kind) not in any manifest set
    for (i, e) in events.iter().enumerate() {
        let referenced = manifest.required.iter().any(|r| r.op == e.op)
            || manifest.forbidden.iter().any(|f| f.op == e.op)
            || manifest.allowed_skips.iter().any(|a| a.op == e.op)
            || manifest.orderings.iter().any(|o| o.earlier_op == e.op || o.later_op == e.op)
            || manifest.latency_bounds.iter().any(|l| l.op == e.op)
            || manifest.result_predicates.iter().any(|p| p.op == e.op);
        if !referenced {
            unexpected.push(UnexpectedEvent {
                op: e.op.clone(),
                kind: e.kind.as_str().to_string(),
                index: i,
            });
        }
    }

    let failed_count = failures.len() as u64;
    let passed = failures.is_empty() && silent.is_empty();

    VerificationReport {
        passed,
        events_total: events.len() as u64,
        expectations_total,
        passed_count,
        failed_count,
        failures,
        unexpected_events: unexpected,
        silent_fallbacks: silent,
    }
}

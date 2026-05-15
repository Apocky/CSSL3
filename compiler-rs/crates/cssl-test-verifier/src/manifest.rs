// § manifest.rs : event-manifest loader (stage0 line-oriented format)
// ══════════════════════════════════════════════════════════════════
// § I> stage0-format ← line-oriented ; sibling-spec specs/67 produces
//   either CSL-native source + sidecar.manifest.txt OR transcoded
//   directly to this format. Post-stage-0 : parse CSSL source via
//   cssl-parser. Format below is the EXCHANGE-LAYER.
//
// LINES :
//   '#' ...                                    → comment
//   blank                                      → ignored
//   profile <name>                             → switches active profile
//   require <op> <kind> <count-spec>           → required event
//   forbid  <op> [kind]                        → must not appear
//   allow-skip <op>                            → skip-event allowlisted
//   order <a> before <b>                       → temporal ordering
//   latency-max <op> <kind> <ns>               → per-op latency bound
//   result <op> <kind> <jq-path> <pred>        → result-value predicate
//
// COUNT-SPEC :
//   N            → exact-N
//   [LO,HI]      → range
//   >=N | ge:N   → at-least
//   <=N | le:N   → at-most
//   *            → any (≥1)
//
// PRED-OPS :
//   != <json-literal>     ne
//   == <json-literal>     eq
//   > <number>            gt
//   < <number>            lt
//   exists                non-null
//
// JQ-PATH : dot-separated key path inside `result` (e.g. .handle, .image_idx)
//   Empty path "." = the result value itself.
// ══════════════════════════════════════════════════════════════════

use crate::events::EventKind;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CountSpec {
    Exact(u64),
    Range(u64, u64),
    AtLeast(u64),
    AtMost(u64),
    Any, // ≥1
}

impl CountSpec {
    pub fn matches(&self, n: u64) -> bool {
        match *self {
            CountSpec::Exact(k) => n == k,
            CountSpec::Range(lo, hi) => n >= lo && n <= hi,
            CountSpec::AtLeast(k) => n >= k,
            CountSpec::AtMost(k) => n <= k,
            CountSpec::Any => n >= 1,
        }
    }

    pub fn parse(s: &str) -> Result<CountSpec, ManifestError> {
        let s = s.trim();
        if s == "*" {
            return Ok(CountSpec::Any);
        }
        // accept leading "=" as exact-spec sigil (=1, =600, =[540,660] all valid)
        let s = s.strip_prefix('=').map(str::trim).unwrap_or(s);
        if let Some(rest) = s.strip_prefix(">=").or_else(|| s.strip_prefix("ge:")) {
            return rest
                .trim()
                .parse::<u64>()
                .map(CountSpec::AtLeast)
                .map_err(|_| ManifestError::BadCountSpec(s.to_string()));
        }
        if let Some(rest) = s.strip_prefix("<=").or_else(|| s.strip_prefix("le:")) {
            return rest
                .trim()
                .parse::<u64>()
                .map(CountSpec::AtMost)
                .map_err(|_| ManifestError::BadCountSpec(s.to_string()));
        }
        if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
            if parts.len() != 2 {
                return Err(ManifestError::BadCountSpec(s.to_string()));
            }
            let lo = parts[0]
                .parse::<u64>()
                .map_err(|_| ManifestError::BadCountSpec(s.to_string()))?;
            let hi = parts[1]
                .parse::<u64>()
                .map_err(|_| ManifestError::BadCountSpec(s.to_string()))?;
            return Ok(CountSpec::Range(lo, hi));
        }
        s.parse::<u64>()
            .map(CountSpec::Exact)
            .map_err(|_| ManifestError::BadCountSpec(s.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Required {
    pub op: String,
    pub kind: EventKind,
    pub count: CountSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forbidden {
    pub op: String,
    pub kind: Option<EventKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedSkip {
    pub op: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderConstraint {
    pub earlier_op: String,
    pub later_op: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBound {
    pub op: String,
    pub kind: EventKind,
    pub max_ns: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredOp {
    Ne(serde_json::Value),
    Eq(serde_json::Value),
    Gt(f64),
    Lt(f64),
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultPredicate {
    pub op: String,
    pub kind: EventKind,
    pub path: Vec<String>, // empty = whole result
    pub pred: PredOp,
    pub raw: String, // human-readable rendering
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub profile: Option<String>,
    pub required: Vec<Required>,
    pub forbidden: Vec<Forbidden>,
    pub allowed_skips: Vec<AllowedSkip>,
    pub orderings: Vec<OrderConstraint>,
    pub latency_bounds: Vec<LatencyBound>,
    pub result_predicates: Vec<ResultPredicate>,
    /// § spec-70 § item-05 (A05.1) : when true, the verifier rejects a
    /// trace that contains zero observable side-effects. Rt-internal events
    /// (process.start / process.exit / loa_startup.ctor / sentinel.{write,skip})
    /// do NOT count toward observability — only test-authored events such as
    /// `cssl_test_observe(...)` (op = test.observe) or any other non-rt-internal
    /// op (e.g. user FFI traces) satisfy the gate.
    #[serde(default)]
    pub requires_observable: bool,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("syntax error at line {line_no}: {message}")]
    Syntax { line_no: usize, message: String },
    #[error("bad count-spec: {0}")]
    BadCountSpec(String),
    #[error("bad kind: {0} (expected entry|exit|branch|skip|error)")]
    BadKind(String),
    #[error("bad predicate: {0}")]
    BadPredicate(String),
}

pub fn load_manifest(path: &Path, profile: Option<&str>) -> Result<Manifest, ManifestError> {
    let raw = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_manifest(&raw, profile)
}

fn parse_kind(s: &str) -> Result<EventKind, ManifestError> {
    Ok(match s {
        "entry" => EventKind::Entry,
        "exit" => EventKind::Exit,
        "branch" => EventKind::Branch,
        "skip" => EventKind::Skip,
        "error" => EventKind::Error,
        other => return Err(ManifestError::BadKind(other.to_string())),
    })
}

fn parse_pred(rest: &str) -> Result<PredOp, ManifestError> {
    let rest = rest.trim();
    if rest == "exists" {
        return Ok(PredOp::Exists);
    }
    if let Some(v) = rest.strip_prefix("!=") {
        let parsed: serde_json::Value =
            serde_json::from_str(v.trim()).map_err(|_| ManifestError::BadPredicate(rest.into()))?;
        return Ok(PredOp::Ne(parsed));
    }
    if let Some(v) = rest.strip_prefix("==") {
        let parsed: serde_json::Value =
            serde_json::from_str(v.trim()).map_err(|_| ManifestError::BadPredicate(rest.into()))?;
        return Ok(PredOp::Eq(parsed));
    }
    if let Some(v) = rest.strip_prefix('>') {
        let n: f64 = v
            .trim()
            .parse()
            .map_err(|_| ManifestError::BadPredicate(rest.into()))?;
        return Ok(PredOp::Gt(n));
    }
    if let Some(v) = rest.strip_prefix('<') {
        let n: f64 = v
            .trim()
            .parse()
            .map_err(|_| ManifestError::BadPredicate(rest.into()))?;
        return Ok(PredOp::Lt(n));
    }
    Err(ManifestError::BadPredicate(rest.to_string()))
}

fn split_path(p: &str) -> Vec<String> {
    let p = p.trim().trim_start_matches('.');
    if p.is_empty() {
        Vec::new()
    } else {
        p.split('.').map(|s| s.to_string()).collect()
    }
}

pub fn parse_manifest(raw: &str, profile_filter: Option<&str>) -> Result<Manifest, ManifestError> {
    let mut m = Manifest::default();
    let mut active_profile: Option<String> = None;
    let mut accept = profile_filter.is_none();

    for (i, line) in raw.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // strip trailing inline comment introduced by '#'
        let trimmed = trimmed.split('#').next().unwrap().trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut tok = trimmed.split_whitespace();
        let head = tok.next().unwrap();
        match head {
            "profile" => {
                let name = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "profile <name>".into(),
                })?;
                active_profile = Some(name.to_string());
                accept = match profile_filter {
                    None => true,
                    Some(f) => f == name,
                };
                if accept && m.profile.is_none() {
                    m.profile = Some(name.to_string());
                }
            }
            _ if !accept => {
                // skip non-active-profile lines
            }
            "require" => {
                let op = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "require <op> <kind> <count-spec>".into(),
                })?;
                let kind = parse_kind(tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "missing kind".into(),
                })?)?;
                let count_str: String = tok.collect::<Vec<_>>().join("");
                let count = CountSpec::parse(&count_str)?;
                m.required.push(Required {
                    op: op.to_string(),
                    kind,
                    count,
                });
            }
            "forbid" => {
                let op = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "forbid <op> [kind]".into(),
                })?;
                let kind = match tok.next() {
                    Some(k) => Some(parse_kind(k)?),
                    None => None,
                };
                m.forbidden.push(Forbidden {
                    op: op.to_string(),
                    kind,
                });
            }
            "allow-skip" => {
                let op = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "allow-skip <op>".into(),
                })?;
                m.allowed_skips.push(AllowedSkip { op: op.to_string() });
            }
            "require-observable" => {
                // § spec-70 § item-05 A05.1 : flips the run-and-observe gate on.
                // No arguments. Multiple occurrences are idempotent.
                m.requires_observable = true;
            }
            "order" => {
                let a = tok.next();
                let kw = tok.next();
                let b = tok.next();
                match (a, kw, b) {
                    (Some(a), Some("before"), Some(b)) => {
                        m.orderings.push(OrderConstraint {
                            earlier_op: a.to_string(),
                            later_op: b.to_string(),
                        });
                    }
                    _ => {
                        return Err(ManifestError::Syntax {
                            line_no,
                            message: "order <a> before <b>".into(),
                        })
                    }
                }
            }
            "latency-max" => {
                let op = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "latency-max <op> <kind> <ns>".into(),
                })?;
                let kind = parse_kind(tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "missing kind".into(),
                })?)?;
                let ns: u64 = tok
                    .next()
                    .ok_or_else(|| ManifestError::Syntax {
                        line_no,
                        message: "missing ns".into(),
                    })?
                    .parse()
                    .map_err(|_| ManifestError::Syntax {
                        line_no,
                        message: "ns must be u64".into(),
                    })?;
                m.latency_bounds.push(LatencyBound {
                    op: op.to_string(),
                    kind,
                    max_ns: ns,
                });
            }
            "result" => {
                let op = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "result <op> <kind> <path> <pred>".into(),
                })?;
                let kind = parse_kind(tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "missing kind".into(),
                })?)?;
                let path = tok.next().ok_or_else(|| ManifestError::Syntax {
                    line_no,
                    message: "missing path".into(),
                })?;
                let rest: String = tok.collect::<Vec<_>>().join(" ");
                let pred = parse_pred(&rest)?;
                let raw_str = format!("{} {} {}", op, path, rest);
                m.result_predicates.push(ResultPredicate {
                    op: op.to_string(),
                    kind,
                    path: split_path(path),
                    pred,
                    raw: raw_str,
                });
            }
            unknown => {
                return Err(ManifestError::Syntax {
                    line_no,
                    message: format!("unknown directive: {}", unknown),
                });
            }
        }
        let _ = active_profile;
    }
    Ok(m)
}

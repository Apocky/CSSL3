// § manifest_csl.rs : CSL3-glyph-dense event-manifest parser
// ══════════════════════════════════════════════════════════════════
// § I> spec       ← Infinity Engine specs/67_CSSL_TEST_MANIFEST.csl
// § I> directive  ← CSSL/CSL native everywhere · ¬ JSON ¬ YAML ¬ transcoding-step
// § I> consumed   ← .events.csl manifests at IE-worktree examples/*.events.csl
// § I> outputs    ← typed Manifest + side-channel CslExtras (paired-with, latency, forbid-between)
//
// Grammar overview :
//   header      :  @<key> [:] <value>
//   directive   :  expect / expect-not / allow / require-order / forbid-between / require-window
//   op-pat      :  [<src>::]<domain>.<verb>[⊗<kind>]
//   modifier    :  ·-delimited · key<cmp><val> OR bare-flag
//   block       :  §loop <name> · count≈N±P% … §end
//   section     :  §<word> · <free-text>          (¬ a directive · skipped)
//   comment     :  «← …» trailing OR full-line · «// …» any-position
//   separator   :  «═══…» line · skipped
// ══════════════════════════════════════════════════════════════════

use crate::events::EventKind;
use crate::manifest::{
    CountSpec, Forbidden, LatencyBound, Manifest, ManifestError, OrderConstraint, PredOp,
    Required, ResultPredicate,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ───────────────────────────────────────────────────────────────────
// Side-channel state preserved alongside Manifest (CSL3-specific surface
// that doesn't yet have first-class fields in Manifest). Carried via a
// wrapper so verify.rs can opt-in to enforce.
// ───────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CslExtras {
    pub headers: Vec<(String, String)>,
    pub paired_with: Vec<PairedWith>,
    pub forbid_between: Vec<ForbidBetween>,
    pub expect_nots: Vec<ExpectNot>,
    pub allowed: Vec<AllowedOp>,
    pub warnings: Vec<String>,
    pub manifest_version: Option<String>,
    pub timeout_ns: Option<u64>,
    pub binary: Option<String>,
    pub trace: Option<String>,
    pub platform: Option<String>,
    pub ignore_ops: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedWith {
    pub op: String,
    pub kind: EventKind,
    pub mate_op: String,
    pub mate_kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForbidBetween {
    pub start_op: String,
    pub start_kind: Option<EventKind>,
    pub middle_op: String,
    pub middle_kind: Option<EventKind>,
    pub end_op: String,
    pub end_kind: Option<EventKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectNot {
    pub op: String,
    pub kind: Option<EventKind>,
    pub note_substr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedOp {
    pub op: String,
    pub kind: Option<EventKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CslManifest {
    pub manifest: Manifest,
    pub extras: CslExtras,
}

// ───────────────────────────────────────────────────────────────────
// public entry
// ───────────────────────────────────────────────────────────────────
pub fn load(path: &Path) -> Result<CslManifest, ManifestError> {
    let raw = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse(&raw)
}

pub fn parse(raw: &str) -> Result<CslManifest, ManifestError> {
    let mut out = CslManifest::default();
    let mut saw_header = false;

    // loop-block state : scope multiplies count expectations
    let mut loop_count: Option<CountSpec> = None;
    let mut loop_name: Option<String> = None;

    // ── pre-pass : fold continuation lines onto previous directive ──
    // Continuation rules (per spec/67) :
    //   1. line whose first non-WS char is '·' → continues previous directive
    //   2. line that begins with whitespace AND previous-non-blank line ended
    //      with a trailing '·' → continues (rare · handles wrap-after-modifier-glyph)
    //   3. line that begins with whitespace AND previous-non-blank line was a
    //      comment-continuation (had a leading '←' or was itself a continuation) →
    //      ALSO a comment-continuation (until a known directive verb appears)
    // The folder produces a Vec<(line_no, logical_line)>.
    let folded = fold_continuations(raw);

    for (line_no, raw_line) in folded {
        let line = strip_comments(&raw_line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Skip pure CSL3 separator lines : ═══════…
        if line.chars().all(|c| c == '═' || c.is_whitespace()) {
            continue;
        }

        // Header @-fields
        if let Some(rest) = line.strip_prefix('@') {
            saw_header = true;
            parse_header(rest, &mut out)?;
            continue;
        }

        // §-prefixed lines : either control (§loop / §end) or section-marker (skip)
        if let Some(rest) = line.strip_prefix('§') {
            let rest_trim = rest.trim_start();
            // §loop <name> · count≈N±P% [· tag=...]
            if let Some(after) = rest_trim.strip_prefix("loop") {
                let after = after.trim();
                if !after.is_empty() {
                    let (name, count) = parse_loop_header(after, line_no)?;
                    loop_name = Some(name);
                    loop_count = Some(count);
                    continue;
                }
            }
            if rest_trim == "end" || rest_trim.starts_with("end ") {
                loop_name = None;
                loop_count = None;
                continue;
            }
            // any other §-section is a heading · skip
            continue;
        }

        // Directive lines
        // Split on " · " (CSL3 dot-separator). But the verb prefix uses spaces.
        // Strategy : take first whitespace-token as verb, rest is the body.
        let (verb, body) = split_first_token(line);
        match verb {
            "expect" => parse_expect(
                body,
                false,
                line_no,
                &mut out,
                loop_count.as_ref(),
                loop_name.as_deref(),
            )?,
            "expect-not" => parse_expect(
                body,
                true,
                line_no,
                &mut out,
                loop_count.as_ref(),
                loop_name.as_deref(),
            )?,
            "allow" => parse_allow(body, line_no, &mut out)?,
            "require-order" => parse_require_order(body, line_no, &mut out)?,
            "forbid-between" => parse_forbid_between(body, line_no, &mut out)?,
            "require-window" => {
                // v1 : warn-and-skip · time-window predicate not yet enforced
                out.extras.warnings.push(format!(
                    "warning: skipping unsupported directive 'require-window' at line {}",
                    line_no
                ));
            }
            _ => {
                // Unknown verb : surface as warning · ¬ fatal · forward-compat per spec/67
                out.extras.warnings.push(format!(
                    "warning: skipping unrecognized directive '{}' at line {}",
                    verb, line_no
                ));
            }
        }
    }

    if !saw_header && out.extras.headers.is_empty() {
        // Spec/67 §FILE-SHAPE : @-tagged header line is a strong signal · refusing
        // here lets `parse_manifest` (line-oriented) be tried as a fallback by main.
        return Err(ManifestError::Syntax {
            line_no: 1,
            message: "no @-header found · not a CSL3 manifest".into(),
        });
    }
    Ok(out)
}

// ───────────────────────────────────────────────────────────────────
// comment-stripping : trim trailing «← …» AND any «// …»
// ───────────────────────────────────────────────────────────────────
fn strip_comments(s: &str) -> String {
    // first, kill // line-comments
    let s = match s.find("//") {
        Some(idx) => &s[..idx],
        None => s,
    };
    // then strip ← inline-comments (multibyte char '←' = U+2190)
    let s = match s.find('←') {
        Some(idx) => &s[..idx],
        None => s,
    };
    s.to_string()
}

// fold_continuations : tokenize raw text into logical lines, concatenating
// continuation lines (leading '·' OR pure-comment-continuation lines) onto
// the prior directive line. Returns (anchor_line_no, logical_line).
fn fold_continuations(raw: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut prev_was_comment_block = false;
    for (idx, raw_line) in raw.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            prev_was_comment_block = false;
            continue;
        }
        // Comment-block continuation : indented line whose previous line was
        // a '←'-comment OR a continuation thereof → drop silently.
        let starts_with_indent = raw_line.starts_with(' ') || raw_line.starts_with('\t');
        let starts_with_arrow = trimmed.starts_with('←');
        let is_directive_start = starts_with_directive_verb(trimmed)
            || trimmed.starts_with('@')
            || trimmed.starts_with('§')
            || trimmed.starts_with("═");

        if starts_with_arrow {
            prev_was_comment_block = true;
            continue;
        }
        if starts_with_indent && prev_was_comment_block && !is_directive_start {
            // unlabeled comment continuation
            continue;
        }
        prev_was_comment_block = false;

        // Continuation onto previous directive : leading '·' (after WS)
        if trimmed.starts_with('·') {
            if let Some((_, prev)) = out.last_mut() {
                prev.push(' ');
                prev.push_str(trimmed);
                continue;
            }
        }
        out.push((line_no, raw_line.to_string()));
    }
    out
}

fn starts_with_directive_verb(s: &str) -> bool {
    matches!(
        first_word(s),
        "expect" | "expect-not" | "allow" | "require-order" | "forbid-between" | "require-window"
    )
}

fn first_word(s: &str) -> &str {
    let s = s.trim_start();
    match s.find(|c: char| c.is_whitespace()) {
        Some(i) => &s[..i],
        None => s,
    }
}

fn split_first_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(|c: char| c.is_whitespace()) {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    }
}

// Split a directive body on the CSL3 «·» separator (also accept ASCII '|' fallback).
// Tokens are returned trimmed · empty tokens dropped.
fn split_modifiers(body: &str) -> Vec<&str> {
    body.split('·').map(str::trim).filter(|s| !s.is_empty()).collect()
}

// ───────────────────────────────────────────────────────────────────
// header parsing : @key : value  OR  @key value
// ───────────────────────────────────────────────────────────────────
fn parse_header(rest: &str, out: &mut CslManifest) -> Result<(), ManifestError> {
    let (key, val) = match rest.find(|c: char| c == ':' || c.is_whitespace()) {
        Some(i) => {
            let k = rest[..i].trim().to_string();
            let v = rest[i..].trim_start_matches(|c: char| c == ':' || c.is_whitespace()).trim().to_string();
            (k, v)
        }
        None => (rest.trim().to_string(), String::new()),
    };
    out.extras.headers.push((key.clone(), val.clone()));
    match key.as_str() {
        "manifest-version" => out.extras.manifest_version = Some(val),
        "binary" => out.extras.binary = Some(val),
        "trace" => out.extras.trace = Some(val),
        "platform" => out.extras.platform = Some(val),
        "timeout" => {
            if let Ok(ns) = parse_duration_ns(&val) {
                out.extras.timeout_ns = Some(ns);
            }
        }
        "ignore" => {
            // comma-separated op-pats
            for op in val.split(',') {
                let op = op.trim();
                if !op.is_empty() {
                    out.extras.ignore_ops.push(op.to_string());
                }
            }
        }
        // env, notes, tolerate, others : retain in headers · no special semantics yet
        _ => {}
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────
// op-pat parsing : [<src>::]<domain>.<verb>[⊗<kind>]
// ───────────────────────────────────────────────────────────────────
fn parse_op_pat(s: &str) -> (String, Option<EventKind>) {
    // Strip optional src-prefix «foo::» — we keep the raw op string after
    // src::, since cssl-rt event.op excludes src.
    let s = s.trim();
    let after_src = match s.rfind("::") {
        Some(i) => &s[i + 2..],
        None => s,
    };
    if let Some(idx) = after_src.find('⊗') {
        let op = after_src[..idx].trim().to_string();
        // skip the '⊗' (3-byte UTF-8) and read the kind word
        let after = &after_src[idx + '⊗'.len_utf8()..];
        let kind_str = after
            .split(|c: char| c.is_whitespace() || c == '·')
            .next()
            .unwrap_or("")
            .trim();
        let kind = match kind_str {
            "entry" => Some(EventKind::Entry),
            "exit" => Some(EventKind::Exit),
            "branch" => Some(EventKind::Branch),
            "skip" => Some(EventKind::Skip),
            "error" => Some(EventKind::Error),
            _ => None,
        };
        (op, kind)
    } else {
        (after_src.trim().to_string(), None)
    }
}

// ───────────────────────────────────────────────────────────────────
// expect / expect-not parser
// ───────────────────────────────────────────────────────────────────
fn parse_expect(
    body: &str,
    is_not: bool,
    line_no: usize,
    out: &mut CslManifest,
    loop_count: Option<&CountSpec>,
    _loop_name: Option<&str>,
) -> Result<(), ManifestError> {
    let parts = split_modifiers(body);
    if parts.is_empty() {
        return Err(ManifestError::Syntax {
            line_no,
            message: "expect requires op-pat".into(),
        });
    }
    let (op, kind_opt) = parse_op_pat(parts[0]);
    if op.is_empty() {
        return Err(ManifestError::Syntax {
            line_no,
            message: "empty op-pat".into(),
        });
    }
    // Parse modifiers
    let mut count_spec: Option<CountSpec> = None;
    let mut latency_max_ns: Option<u64> = None;
    let mut paired_mate: Option<(String, Option<EventKind>)> = None;
    let mut result_preds: Vec<(Vec<String>, PredOp, String)> = Vec::new();
    let mut note_substr: Option<String> = None;

    for tok in &parts[1..] {
        let tok = tok.trim();
        if let Some(rest) = tok.strip_prefix("count") {
            // count=N · count∈[lo,hi] · count≈N±P% · count≥N · count≤N
            let cs = parse_count_modifier(rest).map_err(|e| ManifestError::Syntax {
                line_no,
                message: e,
            })?;
            count_spec = Some(cs);
        } else if let Some(rest) = tok.strip_prefix("latency≤") {
            latency_max_ns = Some(parse_duration_ns(rest.trim()).map_err(|e| {
                ManifestError::Syntax {
                    line_no,
                    message: e,
                }
            })?);
        } else if tok.starts_with("latency≥") {
            // lower-bound : warn-skip · v1 enforces upper-bound only
            out.extras.warnings.push(format!(
                "warning: skipping unsupported modifier 'latency≥' at line {}",
                line_no
            ));
        } else if let Some(rest) = tok.strip_prefix("paired-with=") {
            let (mate_op, mate_kind) = parse_op_pat(rest);
            paired_mate = Some((mate_op, mate_kind));
        } else if let Some(rest) = tok.strip_prefix("result⊃") {
            extract_predicates(rest, &op, &mut result_preds);
        } else if let Some(rest) = tok.strip_prefix("code=") {
            // code=N is sugar for result⊃{code=N}
            if let Ok(n) = rest.trim().parse::<i64>() {
                result_preds.push((
                    vec!["code".to_string()],
                    PredOp::Eq(serde_json::Value::from(n)),
                    format!("{{code={}}}", n),
                ));
            }
        } else if let Some(rest) = tok.strip_prefix("code≠") {
            if let Ok(n) = rest.trim().parse::<i64>() {
                result_preds.push((
                    vec!["code".to_string()],
                    PredOp::Ne(serde_json::Value::from(n)),
                    format!("{{code≠{}}}", n),
                ));
            }
        } else if let Some(rest) = tok.strip_prefix("note⊃") {
            // note⊃"..." → substring match
            let s = rest.trim();
            let s = s.trim_start_matches('"').trim_end_matches('"');
            note_substr = Some(s.to_string());
        } else if let Some(rest) = tok.strip_prefix("branch=") {
            // branch tag selector · stash as note-substr predicate v1
            note_substr = Some(rest.trim().to_string());
        } else if tok.starts_with("after=")
            || tok.starts_with("before=")
            || tok == "after-all"
            || tok == "before-all"
            || tok.starts_with("after-all=")
            || tok.starts_with("before-all=")
        {
            // ordering modifiers · we synthesize OrderConstraints from after=X / before=X
            // (after-all/before-all : warn-skip v1 · would require global pass over expects)
            if let Some(rest) = tok.strip_prefix("after=") {
                let (other_op, _other_kind) = parse_op_pat(rest);
                // earlier=other_op  →  later=op (this expectation must come AFTER other)
                out.manifest.orderings.push(OrderConstraint {
                    earlier_op: other_op,
                    later_op: op.clone(),
                });
            } else if let Some(rest) = tok.strip_prefix("before=") {
                let (other_op, _other_kind) = parse_op_pat(rest);
                out.manifest.orderings.push(OrderConstraint {
                    earlier_op: op.clone(),
                    later_op: other_op,
                });
            } else {
                out.extras.warnings.push(format!(
                    "warning: skipping unsupported ordering modifier '{}' at line {}",
                    tok, line_no
                ));
            }
        } else if tok.starts_with("args⊃") {
            // args predicates : v1 warn-skip · we don't enforce args yet
            out.extras.warnings.push(format!(
                "warning: skipping unsupported modifier 'args⊃' at line {}",
                line_no
            ));
        } else if tok.starts_with("within=") || tok.starts_with("of=") {
            out.extras.warnings.push(format!(
                "warning: skipping unsupported modifier '{}' at line {}",
                tok, line_no
            ));
        } else if tok.starts_with("src=") || tok.starts_with("tag=") {
            // metadata · informational
        } else {
            out.extras.warnings.push(format!(
                "warning: skipping unrecognized modifier '{}' at line {}",
                tok, line_no
            ));
        }
    }

    // Apply loop multiplier if active : the loop count overrides any per-line count
    let final_count = match (count_spec.clone(), loop_count) {
        (None, Some(lc)) => lc.clone(),
        (Some(cs), _) => cs,
        // no count specified, no loop : default = exact-1 per spec/67
        (None, None) => CountSpec::Exact(1),
    };

    if is_not {
        // expect-not : map to forbidden
        let fb = Forbidden {
            op: op.clone(),
            kind: kind_opt,
        };
        out.manifest.forbidden.push(fb);
        out.extras.expect_nots.push(ExpectNot {
            op: op.clone(),
            kind: kind_opt,
            note_substr: note_substr.clone(),
        });
        return Ok(());
    }

    // expect : Required + optional latency + result-preds (per kind)
    let resolved_kind = kind_opt.unwrap_or(EventKind::Exit); // op without ⊗ defaults to exit-events for count purposes
    out.manifest.required.push(Required {
        op: op.clone(),
        kind: resolved_kind,
        count: final_count,
    });
    if let Some(ns) = latency_max_ns {
        out.manifest.latency_bounds.push(LatencyBound {
            op: op.clone(),
            kind: resolved_kind,
            max_ns: ns,
        });
    }
    for (path, pred, raw) in result_preds {
        out.manifest.result_predicates.push(ResultPredicate {
            op: op.clone(),
            kind: resolved_kind,
            path,
            pred,
            raw,
        });
    }
    if let Some((mate_op, mate_kind)) = paired_mate {
        out.extras.paired_with.push(PairedWith {
            op: op.clone(),
            kind: resolved_kind,
            mate_op,
            mate_kind: mate_kind.unwrap_or(EventKind::Entry),
        });
    }

    Ok(())
}

fn parse_allow(body: &str, _line_no: usize, out: &mut CslManifest) -> Result<(), ManifestError> {
    let parts = split_modifiers(body);
    if parts.is_empty() {
        return Ok(());
    }
    let (op, kind) = parse_op_pat(parts[0]);
    out.extras.allowed.push(AllowedOp {
        op: op.clone(),
        kind,
    });
    if matches!(kind, Some(EventKind::Skip)) || kind.is_none() {
        out.manifest.allowed_skips.push(crate::manifest::AllowedSkip { op });
    }
    Ok(())
}

// require-order  A → B   OR   require-order  A before B   OR   require-order A after B
fn parse_require_order(
    body: &str,
    line_no: usize,
    out: &mut CslManifest,
) -> Result<(), ManifestError> {
    // Body may contain "→" or " before "/" after "
    let body_t = body.trim();
    let (a, b);
    if let Some(idx) = body_t.find('→') {
        let lhs = body_t[..idx].trim();
        let rhs = body_t[idx + '→'.len_utf8()..].trim();
        a = lhs;
        b = rhs;
        let (op_a, _) = parse_op_pat(a);
        let (op_b, _) = parse_op_pat(b);
        out.manifest.orderings.push(OrderConstraint {
            earlier_op: op_a,
            later_op: op_b,
        });
        return Ok(());
    }
    if let Some(idx) = body_t.find(" before ") {
        let lhs = body_t[..idx].trim();
        let rhs = body_t[idx + " before ".len()..].trim();
        let (op_a, _) = parse_op_pat(lhs);
        let (op_b, _) = parse_op_pat(rhs);
        out.manifest.orderings.push(OrderConstraint {
            earlier_op: op_a,
            later_op: op_b,
        });
        return Ok(());
    }
    if let Some(idx) = body_t.find(" after ") {
        let lhs = body_t[..idx].trim();
        let rhs = body_t[idx + " after ".len()..].trim();
        // "A after B"  ⇒  B precedes A
        let (op_a, _) = parse_op_pat(lhs);
        let (op_b, _) = parse_op_pat(rhs);
        out.manifest.orderings.push(OrderConstraint {
            earlier_op: op_b,
            later_op: op_a,
        });
        return Ok(());
    }
    Err(ManifestError::Syntax {
        line_no,
        message: "require-order needs '→' or ' before '/' after '".into(),
    })
}

// forbid-between  A  X  B    (whitespace-separated · 3 op-pats)
fn parse_forbid_between(
    body: &str,
    line_no: usize,
    out: &mut CslManifest,
) -> Result<(), ManifestError> {
    // The CSL3 form uses whitespace OR " · " · we tolerate both.
    let body_clean = body.replace('·', " ");
    let toks: Vec<&str> = body_clean.split_whitespace().collect();
    if toks.len() < 3 {
        return Err(ManifestError::Syntax {
            line_no,
            message: "forbid-between needs <A> <X> <B>".into(),
        });
    }
    let (a, ak) = parse_op_pat(toks[0]);
    let (x, xk) = parse_op_pat(toks[1]);
    let (b, bk) = parse_op_pat(toks[2]);
    out.extras.forbid_between.push(ForbidBetween {
        start_op: a,
        start_kind: ak,
        middle_op: x,
        middle_kind: xk,
        end_op: b,
        end_kind: bk,
    });
    Ok(())
}

// ───────────────────────────────────────────────────────────────────
// duration-literal : <int>[_<int>]*<unit>  unit ∈ ns|µs|us|ms|s
// ───────────────────────────────────────────────────────────────────
pub fn parse_duration_ns(raw: &str) -> Result<u64, String> {
    let s = raw.trim();
    // Find the unit suffix · scan from end for non-digit/non-_ char
    // Possible units : "ns", "µs" (2-byte UTF-8 µ), "us", "ms", "s"
    let (numeric, unit) = if let Some(rest) = s.strip_suffix("ns") {
        (rest, "ns")
    } else if let Some(rest) = s.strip_suffix("µs") {
        (rest, "us")
    } else if let Some(rest) = s.strip_suffix("us") {
        (rest, "us")
    } else if let Some(rest) = s.strip_suffix("ms") {
        (rest, "ms")
    } else if let Some(rest) = s.strip_suffix('s') {
        (rest, "s")
    } else {
        return Err(format!("bad duration literal '{}' (need ns|µs|us|ms|s)", s));
    };
    let cleaned: String = numeric.chars().filter(|c| *c != '_').collect();
    let n: u64 = cleaned
        .parse()
        .map_err(|_| format!("bad duration number '{}'", numeric))?;
    let mul: u64 = match unit {
        "ns" => 1,
        "us" => 1_000,
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        _ => unreachable!(),
    };
    n.checked_mul(mul)
        .ok_or_else(|| format!("duration overflow on '{}'", s))
}

// ───────────────────────────────────────────────────────────────────
// count modifier parsing : =N · ∈[lo,hi] · ≈N±P% · ≥N · ≤N
//   input is the tail after the literal "count" prefix
// ───────────────────────────────────────────────────────────────────
fn parse_count_modifier(rest: &str) -> Result<CountSpec, String> {
    let r = rest.trim();
    if let Some(after) = r.strip_prefix('=') {
        let n: u64 = after.trim().parse().map_err(|_| format!("bad count='{}'", after))?;
        return Ok(CountSpec::Exact(n));
    }
    if let Some(after) = r.strip_prefix('∈') {
        let s = after.trim();
        if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
            if parts.len() == 2 {
                let lo: u64 = parts[0].parse().map_err(|_| format!("bad lo '{}'", parts[0]))?;
                let hi: u64 = parts[1].parse().map_err(|_| format!("bad hi '{}'", parts[1]))?;
                return Ok(CountSpec::Range(lo, hi));
            }
        }
        return Err(format!("bad count∈ : '{}'", s));
    }
    if let Some(after) = r.strip_prefix('≈') {
        // N±P%  →  Range(N*(1-P/100), N*(1+P/100))  saturating to u64
        let s = after.trim();
        let (n_str, p_str) = match s.find('±') {
            Some(i) => (&s[..i], &s[i + '±'.len_utf8()..]),
            None => return Err(format!("bad count≈ '{}' (need ±)", s)),
        };
        let n: u64 = n_str.trim().parse().map_err(|_| format!("bad N '{}'", n_str))?;
        let p_clean: String = p_str.trim().trim_end_matches('%').chars().collect();
        let p: f64 = p_clean.parse().map_err(|_| format!("bad P '{}'", p_str))?;
        let lo = ((n as f64) * (1.0 - p / 100.0)).floor().max(0.0) as u64;
        let hi = ((n as f64) * (1.0 + p / 100.0)).ceil() as u64;
        return Ok(CountSpec::Range(lo, hi));
    }
    if let Some(after) = r.strip_prefix('≥') {
        let n: u64 = after.trim().parse().map_err(|_| format!("bad count≥ '{}'", after))?;
        return Ok(CountSpec::AtLeast(n));
    }
    if let Some(after) = r.strip_prefix('≤') {
        let n: u64 = after.trim().parse().map_err(|_| format!("bad count≤ '{}'", after))?;
        return Ok(CountSpec::AtMost(n));
    }
    Err(format!("unrecognized count-spec : 'count{}'", r))
}

// ───────────────────────────────────────────────────────────────────
// loop-header : after the literal "§loop"
//   <name> · count≈N±P% [· tag=...]
// ───────────────────────────────────────────────────────────────────
fn parse_loop_header(rest: &str, line_no: usize) -> Result<(String, CountSpec), ManifestError> {
    let parts = split_modifiers(rest);
    if parts.is_empty() {
        return Err(ManifestError::Syntax {
            line_no,
            message: "§loop needs <name>".into(),
        });
    }
    let name = parts[0].trim().to_string();
    let mut count = CountSpec::Any;
    for tok in &parts[1..] {
        if let Some(after) = tok.strip_prefix("count") {
            count = parse_count_modifier(after).map_err(|e| ManifestError::Syntax {
                line_no,
                message: e,
            })?;
        }
    }
    Ok((name, count))
}

// ───────────────────────────────────────────────────────────────────
// extract predicates from a result⊃{...} body : split by ',' into
// key<cmp>val · accept = ≠ ≤ ≥ < > .
// hex literals (0xFFFFFFFF) → numeric · int → numeric · "..." → string
// ───────────────────────────────────────────────────────────────────
fn extract_predicates(
    body: &str,
    _op: &str,
    out: &mut Vec<(Vec<String>, PredOp, String)>,
) {
    let s = body.trim();
    let inner = s.trim_start_matches('{').trim_end_matches('}');
    for clause in inner.split(',') {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }
        if let Some((path, pred, raw)) = parse_pred_clause(clause) {
            out.push((path, pred, raw));
        }
    }
}

fn parse_pred_clause(clause: &str) -> Option<(Vec<String>, PredOp, String)> {
    // Order matters : 2-char comparators first
    let comparators: &[(&str, fn(serde_json::Value) -> PredOp)] = &[
        ("≠", |v| PredOp::Ne(v)),
        ("≤", |v| {
            if let Some(n) = v.as_f64() {
                PredOp::Lt(n + f64::EPSILON.max(1.0))
            } else {
                PredOp::Eq(v)
            }
        }),
        ("≥", |v| {
            if let Some(n) = v.as_f64() {
                PredOp::Gt(n - f64::EPSILON.max(1.0))
            } else {
                PredOp::Eq(v)
            }
        }),
        ("=", |v| PredOp::Eq(v)),
        ("<", |v| {
            v.as_f64().map(PredOp::Lt).unwrap_or(PredOp::Eq(v))
        }),
        (">", |v| {
            v.as_f64().map(PredOp::Gt).unwrap_or(PredOp::Eq(v))
        }),
    ];
    for (sym, ctor) in comparators {
        if let Some(idx) = clause.find(sym) {
            let key = clause[..idx].trim().to_string();
            let val_str = clause[idx + sym.len()..].trim();
            let val = parse_pred_value(val_str);
            let raw = clause.to_string();
            let path = if key.is_empty() {
                Vec::new()
            } else {
                key.split('.').map(|s| s.to_string()).collect()
            };
            return Some((path, ctor(val), raw));
        }
    }
    None
}

fn parse_pred_value(s: &str) -> serde_json::Value {
    let s = s.trim();
    // hex literal
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if let Ok(n) = u64::from_str_radix(hex, 16) {
            return serde_json::Value::from(n);
        }
    }
    // bare number (allow _ separators)
    let cleaned: String = s.chars().filter(|c| *c != '_').collect();
    if let Ok(n) = cleaned.parse::<i64>() {
        return serde_json::Value::from(n);
    }
    if let Ok(n) = cleaned.parse::<u64>() {
        return serde_json::Value::from(n);
    }
    if let Ok(n) = cleaned.parse::<f64>() {
        return serde_json::Value::from(n);
    }
    // quoted string
    if let Some(stripped) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return serde_json::Value::String(stripped.to_string());
    }
    // bool
    if s == "true" {
        return serde_json::Value::Bool(true);
    }
    if s == "false" {
        return serde_json::Value::Bool(false);
    }
    // fall-back : treat as bare-string
    serde_json::Value::String(s.to_string())
}

// ───────────────────────────────────────────────────────────────────
// tests
// ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_basics() {
        assert_eq!(parse_duration_ns("30s").unwrap(), 30_000_000_000);
        assert_eq!(parse_duration_ns("100ms").unwrap(), 100_000_000);
        assert_eq!(parse_duration_ns("16ms").unwrap(), 16_000_000);
        assert_eq!(parse_duration_ns("1µs").unwrap(), 1_000);
        assert_eq!(parse_duration_ns("16_000_000ns").unwrap(), 16_000_000);
    }

    #[test]
    fn count_spec_modes() {
        match parse_count_modifier("=600").unwrap() {
            CountSpec::Exact(600) => {}
            other => panic!("got {:?}", other),
        }
        match parse_count_modifier("∈[540,660]").unwrap() {
            CountSpec::Range(540, 660) => {}
            other => panic!("got {:?}", other),
        }
        match parse_count_modifier("≈600±10%").unwrap() {
            CountSpec::Range(lo, hi) => {
                assert!(lo <= 540 && hi >= 660, "got [{},{}]", lo, hi);
            }
            other => panic!("got {:?}", other),
        }
        match parse_count_modifier("≥1").unwrap() {
            CountSpec::AtLeast(1) => {}
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn op_pat_with_kind() {
        let (op, kind) = parse_op_pat("window.spawn⊗exit");
        assert_eq!(op, "window.spawn");
        assert_eq!(kind, Some(EventKind::Exit));
    }

    #[test]
    fn op_pat_with_src_prefix() {
        let (op, kind) = parse_op_pat("cssl-rt::loa_startup::loa_startup.ctor⊗entry");
        assert_eq!(op, "loa_startup.ctor");
        assert_eq!(kind, Some(EventKind::Entry));
    }

    #[test]
    fn op_pat_no_kind() {
        let (op, kind) = parse_op_pat("process.start");
        assert_eq!(op, "process.start");
        assert_eq!(kind, None);
    }

    #[test]
    fn predicate_parsing() {
        let mut out = Vec::new();
        extract_predicates("{handle≠0}", "x", &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, vec!["handle".to_string()]);
        match &out[0].1 {
            PredOp::Ne(v) if v.as_i64() == Some(0) => {}
            other => panic!("got {:?}", other),
        }

        out.clear();
        extract_predicates("{image_idx≠0xFFFFFFFF}", "x", &mut out);
        assert_eq!(out.len(), 1);
        match &out[0].1 {
            PredOp::Ne(v) => {
                assert_eq!(v.as_u64(), Some(0xFFFFFFFFu64));
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn parse_minimal_manifest() {
        let raw = r#"
@binary : foo.exe
@trace  : /tmp/x.jsonl
@timeout : 30s

§boot
  expect process.start · count=1
  expect main.enter    · count=1 · after=process.start
"#;
        let m = parse(raw).expect("parses");
        assert_eq!(m.manifest.required.len(), 2);
        assert_eq!(m.extras.timeout_ns, Some(30_000_000_000));
        // after=process.start synthesizes one ordering
        assert_eq!(m.manifest.orderings.len(), 1);
    }

    #[test]
    fn parse_loop_block() {
        let raw = r#"
@binary : foo
§loop pump · count≈600±10%
  expect window.pump⊗entry
  expect window.pump⊗exit · paired-with=window.pump⊗entry
§end
"#;
        let m = parse(raw).expect("parses");
        assert_eq!(m.manifest.required.len(), 2);
        for r in &m.manifest.required {
            match &r.count {
                CountSpec::Range(lo, hi) => {
                    assert!(*lo <= 540 && *hi >= 660);
                }
                other => panic!("expected Range got {:?}", other),
            }
        }
        assert_eq!(m.extras.paired_with.len(), 1);
    }

    #[test]
    fn parse_full_cssl_game_manifest() {
        // sample shaped like examples/cssl_game.events.csl
        let raw = r#"
§ MANIFEST · cssl_game · v1
@binary : dist/cssl_game.exe
@trace  : /tmp/cssl_events.jsonl
@timeout : 30s
@platform : windows-msvc

§boot
  expect process.start                       · count=1
  expect main.enter                          · count=1 · after=process.start

§setup
  expect window.spawn⊗exit                   · count=1 · result⊃{handle≠0} · latency≤200ms
  expect-not gpu.device_create⊗skip          · note⊃"real_d3d12"
  expect-not gpu.device_create⊗error

§loop pump · count≈600±10% · tag=pump-cycle
  expect window.pump⊗entry
  expect window.pump⊗exit                    · paired-with=window.pump⊗entry · latency≤16ms
  expect gpu.swapchain_acquire⊗exit          · result⊃{image_idx≠0xFFFFFFFF}
  allow gpu.swapchain_acquire⊗branch         · branch=timeout
§end

§global
  require-order  window.spawn⊗exit            → window.pump⊗entry
  forbid-between window.destroy⊗exit  gpu.swapchain_acquire⊗entry  process.exit
"#;
        let m = parse(raw).expect("parses");
        assert!(m.manifest.required.len() >= 5);
        assert!(m.manifest.forbidden.len() >= 2);
        assert_eq!(m.manifest.orderings.len() >= 2, true);
        assert_eq!(m.extras.forbid_between.len(), 1);
        assert!(m.manifest.latency_bounds.len() >= 2);
        assert!(m.manifest.result_predicates.len() >= 2);
    }
}

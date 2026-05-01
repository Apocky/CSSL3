//! § report — render SessionAttestation in four target formats.
//! ════════════════════════════════════════════════════════════════════
//!
//! - [`render_text`]            : plain-text human-readable, multi-line.
//! - [`render_json_pretty`]     : pretty-printed JSON for log files.
//! - [`render_csl_native`]      : CSLv3-glyph header + bullet section.
//! - [`render_attestation_block`] : one-line ATTESTATION footer.
//!
//! All four functions are pure ; no I/O. The `store::AttestationStore`
//! module pairs them with disk persistence.

use std::fmt::Write as _;

use crate::attestation::{AttestationVerdict, HarmSeverity, SessionAttestation};
use crate::axis::DirectiveAxis;

/// Multi-line human-readable report.
///
/// Format example :
/// ```text
/// SessionAttestation
/// ------------------
/// session_id      : sess-1234
/// window          : 2026-04-30T01:00:00Z .. 2026-04-30T01:05:00Z
/// total_events    : 42
/// errors          : 1
/// denied_events   : 0
/// sovereign_used  : 3
/// verdict         : MajorFlags
///
/// axis counts :
///   Consent           : 3
///   Transparency      : 3
///
/// harm flags (1) :
///   [Warn] Consent @ 2026-04-30T01:02:30Z (network.outbound.fail) - request timed out
/// ```
#[must_use]
pub fn render_text(att: &SessionAttestation) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "SessionAttestation");
    let _ = writeln!(out, "------------------");
    let _ = writeln!(out, "session_id      : {}", att.session_id);
    let _ = writeln!(
        out,
        "window          : {} .. {}",
        att.started_at_iso, att.ended_at_iso
    );
    let _ = writeln!(out, "total_events    : {}", att.total_events);
    let _ = writeln!(out, "errors          : {}", att.errors);
    let _ = writeln!(out, "denied_events   : {}", att.denied_events);
    let _ = writeln!(out, "sovereign_used  : {}", att.sovereign_used);
    let _ = writeln!(out, "verdict         : {:?}", att.verdict);
    let _ = writeln!(out);

    let _ = writeln!(out, "axis counts :");
    if att.axis_counts.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for (axis, count) in &att.axis_counts {
            let _ = writeln!(out, "  {:<18}: {}", format!("{axis:?}"), count);
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "harm flags ({}) :", att.harm_flags.len());
    if att.harm_flags.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for f in &att.harm_flags {
            let _ = writeln!(
                out,
                "  [{sev:?}] {axis:?} @ {ts} ({kind}) - {msg}",
                sev = f.severity,
                axis = f.axis,
                ts = f.ts_iso,
                kind = f.kind,
                msg = f.message
            );
        }
    }
    out
}

/// Pretty-printed JSON. Stable ordering is guaranteed by the
/// `BTreeMap`-backed `axis_counts` field on
/// [`SessionAttestation`]. `serde_json::to_string_pretty` failure is
/// only possible on truly broken input ; we fall back to the empty
/// JSON object so the function remains panic-free.
#[must_use]
pub fn render_json_pretty(att: &SessionAttestation) -> String {
    serde_json::to_string_pretty(att).unwrap_or_else(|_| "{}".to_string())
}

/// CSLv3-glyph-native report. Uses § headers + ✓ ◐ ○ ✗ markers per
/// Apocky's notation default. Designed to be paste-able into agent
/// reports and DECISIONS.md entries.
///
/// Marker rules :
/// - ✓ Clean
/// - ◐ MinorFlags
/// - ○ MajorFlags
/// - ✗ BlockingViolation
#[must_use]
pub fn render_csl_native(att: &SessionAttestation) -> String {
    let mut out = String::new();
    let glyph = match att.verdict {
        AttestationVerdict::Clean => "✓",
        AttestationVerdict::MinorFlags => "◐",
        AttestationVerdict::MajorFlags => "○",
        AttestationVerdict::BlockingViolation => "✗",
    };
    let _ = writeln!(out, "§ ATTESTATION ⟦{}⟧ {glyph} {:?}", att.session_id, att.verdict);
    let _ = writeln!(
        out,
        "  window  : {} → {}",
        att.started_at_iso, att.ended_at_iso
    );
    let _ = writeln!(
        out,
        "  events  : total={} errors={} denied={} sovereign={}",
        att.total_events, att.errors, att.denied_events, att.sovereign_used
    );
    if att.axis_counts.is_empty() {
        let _ = writeln!(out, "  axes    : ∅");
    } else {
        let parts: Vec<String> = att
            .axis_counts
            .iter()
            .map(|(a, c)| format!("{}={c}", axis_short(*a)))
            .collect();
        let _ = writeln!(out, "  axes    : {}", parts.join(" · "));
    }
    if att.harm_flags.is_empty() {
        let _ = writeln!(out, "  flags   : ¬ harm");
    } else {
        let _ = writeln!(out, "  flags   :");
        for f in &att.harm_flags {
            let sev_glyph = match f.severity {
                HarmSeverity::Info => "○",
                HarmSeverity::Warn => "◐",
                HarmSeverity::Critical => "✗",
            };
            let _ = writeln!(
                out,
                "    {sev_glyph} {axis} @ {ts} ⟨{kind}⟩ {msg}",
                axis = axis_short(f.axis),
                ts = f.ts_iso,
                kind = f.kind,
                msg = f.message
            );
        }
    }
    out
}

/// One-line ATTESTATION footer matching the convention Apocky uses
/// on agent reports.
///
/// - Clean session   → `ATTESTATION ¬ harm`
/// - Otherwise       → `ATTESTATION ⚠ <verdict> · <flag-count> flag(s) · <axes>`
#[must_use]
pub fn render_attestation_block(att: &SessionAttestation) -> String {
    if matches!(att.verdict, AttestationVerdict::Clean) {
        return "ATTESTATION ¬ harm".to_string();
    }
    let axes: Vec<String> = att
        .harm_flags
        .iter()
        .map(|f| axis_short(f.axis).to_string())
        .collect();
    let mut uniq: Vec<String> = Vec::new();
    for a in axes {
        if !uniq.contains(&a) {
            uniq.push(a);
        }
    }
    format!(
        "ATTESTATION ⚠ {:?} · {} flag(s) · {}",
        att.verdict,
        att.harm_flags.len(),
        uniq.join("+")
    )
}

fn axis_short(a: DirectiveAxis) -> &'static str {
    match a {
        DirectiveAxis::Consent => "Consent",
        DirectiveAxis::Sovereignty => "Sovereignty",
        DirectiveAxis::Transparency => "Transparency",
        DirectiveAxis::NoHarm => "NoHarm",
        DirectiveAxis::NoControl => "NoControl",
        DirectiveAxis::NoManipulation => "NoManipulation",
        DirectiveAxis::NoSurveillance => "NoSurveillance",
        DirectiveAxis::NoExploitation => "NoExploitation",
        DirectiveAxis::NoCoercion => "NoCoercion",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestation::{aggregate, HarmFlag};
    use cssl_host_audit::{AuditLevel, AuditRow, AuditSource};

    fn sample_with_flags() -> SessionAttestation {
        let mut att = aggregate(&[AuditRow {
            ts_iso: "2026-04-30T01:00:00Z".into(),
            ts_micros: 1,
            source: AuditSource::Runtime,
            level: AuditLevel::Error,
            kind: "network.outbound.fail".into(),
            message: "request timed out".into(),
            sovereign_cap_used: false,
            kv: vec![],
        }]);
        att.session_id = "sess-test".into();
        att
    }

    #[test]
    fn text_report_is_multiline_with_required_keys() {
        let att = sample_with_flags();
        let txt = render_text(&att);
        assert!(txt.contains("SessionAttestation"));
        assert!(txt.contains("session_id      : sess-test"));
        assert!(txt.contains("window          :"));
        assert!(txt.contains("total_events    : 1"));
        assert!(txt.contains("errors          : 1"));
        assert!(txt.contains("verdict         : MajorFlags"));
        assert!(txt.contains("axis counts :"));
        assert!(txt.contains("harm flags (1) :"));
        // Multi-line means at least 8 lines.
        assert!(txt.lines().count() >= 8);
    }

    #[test]
    fn json_pretty_round_trips_through_serde() {
        let att = sample_with_flags();
        let pretty = render_json_pretty(&att);
        let back: SessionAttestation = serde_json::from_str(&pretty).expect("parse");
        assert_eq!(att, back);
        // Pretty-print expands across multiple lines.
        assert!(pretty.lines().count() > 5);
    }

    #[test]
    fn csl_native_uses_csl_glyphs() {
        let att = sample_with_flags();
        let csl = render_csl_native(&att);
        assert!(csl.contains("§ ATTESTATION"));
        // Verdict is MajorFlags → ○ glyph + ◐ flag glyph (Warn).
        assert!(csl.contains("○") || csl.contains("◐"));
        assert!(csl.contains("→"));
        assert!(csl.contains("·") || csl.contains("∅") || csl.contains("⟨"));
    }

    #[test]
    fn attestation_block_clean_for_no_flags() {
        let att = aggregate(&[]);
        assert_eq!(render_attestation_block(&att), "ATTESTATION ¬ harm");
    }

    #[test]
    fn attestation_block_warns_with_flag_summary() {
        let att = sample_with_flags();
        let blk = render_attestation_block(&att);
        assert!(blk.starts_with("ATTESTATION ⚠"));
        assert!(blk.contains("MajorFlags"));
        assert!(blk.contains("flag(s)"));
        assert!(blk.contains("Consent"));
    }

    #[test]
    fn csl_native_shows_no_harm_when_clean() {
        let att = aggregate(&[]);
        let csl = render_csl_native(&att);
        assert!(csl.contains("¬ harm"));
        assert!(csl.contains("✓"));
    }

    #[test]
    fn attestation_block_dedupes_axes() {
        let mut att = aggregate(&[]);
        att.harm_flags = vec![
            HarmFlag {
                ts_iso: "x".into(),
                axis: DirectiveAxis::Sovereignty,
                severity: HarmSeverity::Warn,
                kind: "denied".into(),
                message: String::new(),
            },
            HarmFlag {
                ts_iso: "y".into(),
                axis: DirectiveAxis::Sovereignty,
                severity: HarmSeverity::Warn,
                kind: "denied".into(),
                message: String::new(),
            },
        ];
        att.verdict = AttestationVerdict::MajorFlags;
        let blk = render_attestation_block(&att);
        // Two same-axis flags should render axis name exactly once.
        let occurrences = blk.matches("Sovereignty").count();
        assert_eq!(occurrences, 1);
    }
}

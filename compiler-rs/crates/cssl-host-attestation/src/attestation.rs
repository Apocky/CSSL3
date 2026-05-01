//! § attestation — SessionAttestation aggregator + verdict derivation.
//! ════════════════════════════════════════════════════════════════════
//!
//! Walks an iterable of [`cssl_host_audit::AuditRow`] and produces a
//! single [`SessionAttestation`] summarizing what the engine did per
//! directive-axis. Pure function ; deterministic ; no panics ;
//! BTreeMap-backed counts so JSON output is diff-stable across runs.

use std::collections::BTreeMap;

use cssl_host_audit::{AuditLevel, AuditRow};
use serde::{Deserialize, Serialize};

use crate::axis::{classify_event, DirectiveAxis};

/// Per-session attestation summary.
///
/// Counts and flags computed by [`aggregate`]. Field-order is the
/// canonical wire-order used by [`crate::report::render_json_pretty`]
/// and [`crate::store::AttestationStore::save`] ; do not re-order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAttestation {
    /// Caller-provided session identifier ; passed verbatim through the
    /// pipeline. Empty when [`aggregate`] is fed without `with_session_id`.
    pub session_id: String,
    /// `ts_iso` of the FIRST row encountered (chronologically). Empty
    /// when the input slice is empty.
    pub started_at_iso: String,
    /// `ts_iso` of the LAST row encountered. Empty when input is empty.
    pub ended_at_iso: String,
    /// Total number of rows ingested.
    pub total_events: u64,
    /// Count of rows with `level >= AuditLevel::Error`.
    pub errors: u64,
    /// Count of rows whose `kind` matched a denied-event keyword
    /// (`cap.deny` / `denied` / `refuse`).
    pub denied_events: u64,
    /// Count of rows with `sovereign_cap_used == true`.
    pub sovereign_used: u64,
    /// Count of events touching each directive-axis. BTreeMap gives
    /// deterministic JSON ordering keyed by `DirectiveAxis` Ord.
    pub axis_counts: BTreeMap<DirectiveAxis, u64>,
    /// Harm flags raised by this session, sorted by `ts_iso` ascending
    /// so the report reads in chronological order.
    pub harm_flags: Vec<HarmFlag>,
    /// Overall verdict derived from `harm_flags`.
    pub verdict: AttestationVerdict,
}

/// A single harm-event observation with axis + severity + context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarmFlag {
    pub ts_iso: String,
    pub axis: DirectiveAxis,
    pub severity: HarmSeverity,
    pub kind: String,
    pub message: String,
}

/// Severity of a harm-flag. Ordering : `Info < Warn < Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HarmSeverity {
    Info,
    Warn,
    Critical,
}

/// Overall session verdict derived purely from `harm_flags`.
///
/// - `Clean` : zero flags.
/// - `MinorFlags` : 1..=2 `Info`-only flags.
/// - `MajorFlags` : >=1 `Warn` or >=3 total flags (no `Critical`).
/// - `BlockingViolation` : at least one `Critical` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationVerdict {
    Clean,
    MinorFlags,
    MajorFlags,
    BlockingViolation,
}

/// Aggregate audit rows into a session attestation.
///
/// `session_id` / `started_at_iso` / `ended_at_iso` are filled from the
/// row stream. `session_id` defaults to the empty string ; callers
/// who want a stable identifier should call
/// [`SessionAttestation::with_session_id`] on the result.
#[must_use]
pub fn aggregate(audit_rows: &[AuditRow]) -> SessionAttestation {
    let mut axis_counts: BTreeMap<DirectiveAxis, u64> = BTreeMap::new();
    let mut harm_flags: Vec<HarmFlag> = Vec::new();
    let mut errors: u64 = 0;
    let mut denied_events: u64 = 0;
    let mut sovereign_used: u64 = 0;

    let mut started_at_iso = String::new();
    let mut ended_at_iso = String::new();
    let mut started_micros: u64 = u64::MAX;
    let mut ended_micros: u64 = 0;

    for row in audit_rows {
        // Time-window tracking : pick the earliest / latest by
        // `ts_micros` so out-of-order input still produces correct
        // window bounds.
        if row.ts_micros > 0 && row.ts_micros < started_micros {
            started_micros = row.ts_micros;
            started_at_iso = row.ts_iso.clone();
        }
        if row.ts_micros >= ended_micros {
            ended_micros = row.ts_micros;
            ended_at_iso = row.ts_iso.clone();
        }

        // Per-row classification + counters.
        let row_axes = classify_event(&row.kind);
        for entry in &row_axes {
            *axis_counts.entry(*entry).or_insert(0) += 1;
        }

        if row.is_error() {
            errors += 1;
        }
        if row.sovereign_cap_used {
            sovereign_used += 1;
        }

        let kl = row.kind.to_ascii_lowercase();
        let is_denied = kl.contains("cap.deny") || kl.contains("denied") || kl.contains("refuse");
        if is_denied {
            denied_events += 1;
        }

        // Harm-flag emission rules (severity-by-level + kind) :
        // - Critical-level row → Critical flag on first matching axis
        //   (or NoHarm if the kind classified to no axis).
        // - Error-level row → Warn flag on first matching axis (or NoHarm).
        // - Denied event → Info flag on Sovereignty axis.
        // - Panic / crash kind → Critical flag on NoHarm axis.
        if matches!(row.level, AuditLevel::Critical) {
            let picked = row_axes.first().copied().unwrap_or(DirectiveAxis::NoHarm);
            harm_flags.push(HarmFlag {
                ts_iso: row.ts_iso.clone(),
                axis: picked,
                severity: HarmSeverity::Critical,
                kind: row.kind.clone(),
                message: row.message.clone(),
            });
        } else if row.is_error() {
            let picked = row_axes.first().copied().unwrap_or(DirectiveAxis::NoHarm);
            harm_flags.push(HarmFlag {
                ts_iso: row.ts_iso.clone(),
                axis: picked,
                severity: HarmSeverity::Warn,
                kind: row.kind.clone(),
                message: row.message.clone(),
            });
        } else if is_denied {
            harm_flags.push(HarmFlag {
                ts_iso: row.ts_iso.clone(),
                axis: DirectiveAxis::Sovereignty,
                severity: HarmSeverity::Info,
                kind: row.kind.clone(),
                message: row.message.clone(),
            });
        } else if kl.contains("panic") || kl.contains("crash") {
            harm_flags.push(HarmFlag {
                ts_iso: row.ts_iso.clone(),
                axis: DirectiveAxis::NoHarm,
                severity: HarmSeverity::Critical,
                kind: row.kind.clone(),
                message: row.message.clone(),
            });
        }
    }

    // Sort flags by ts_iso ascending. ts_iso is ISO-8601-like so
    // lexicographic sort is also chronological for well-formed strings.
    harm_flags.sort_by(|a, b| a.ts_iso.cmp(&b.ts_iso));

    let verdict = derive_verdict(&harm_flags);
    let total_events = audit_rows.len() as u64;

    SessionAttestation {
        session_id: String::new(),
        started_at_iso,
        ended_at_iso,
        total_events,
        errors,
        denied_events,
        sovereign_used,
        axis_counts,
        harm_flags,
        verdict,
    }
}

impl SessionAttestation {
    /// Set the `session_id` field. Returns `self` so this can be
    /// chained with [`aggregate`].
    #[must_use]
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = id.into();
        self
    }
}

fn derive_verdict(flags: &[HarmFlag]) -> AttestationVerdict {
    if flags.is_empty() {
        return AttestationVerdict::Clean;
    }
    let has_critical = flags.iter().any(|f| f.severity == HarmSeverity::Critical);
    if has_critical {
        return AttestationVerdict::BlockingViolation;
    }
    let has_warn = flags.iter().any(|f| f.severity == HarmSeverity::Warn);
    if has_warn || flags.len() >= 3 {
        return AttestationVerdict::MajorFlags;
    }
    AttestationVerdict::MinorFlags
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_audit::{AuditLevel, AuditSource};

    fn row(ts_iso: &str, ts_micros: u64, kind: &str, level: AuditLevel) -> AuditRow {
        AuditRow {
            ts_iso: ts_iso.to_string(),
            ts_micros,
            source: AuditSource::Runtime,
            level,
            kind: kind.to_string(),
            message: String::new(),
            sovereign_cap_used: false,
            kv: Vec::new(),
        }
    }

    #[test]
    fn empty_input_yields_clean_verdict() {
        let att = aggregate(&[]);
        assert_eq!(att.total_events, 0);
        assert_eq!(att.verdict, AttestationVerdict::Clean);
        assert!(att.harm_flags.is_empty());
        assert!(att.axis_counts.is_empty());
    }

    #[test]
    fn errors_emit_warn_harm_flags() {
        let rows = vec![
            row("2026-04-30T01:00:00Z", 1, "frame.tick", AuditLevel::Info),
            row(
                "2026-04-30T01:00:01Z",
                2,
                "network.outbound.fail",
                AuditLevel::Error,
            ),
        ];
        let att = aggregate(&rows);
        assert_eq!(att.errors, 1);
        assert_eq!(att.harm_flags.len(), 1);
        assert_eq!(att.harm_flags[0].severity, HarmSeverity::Warn);
    }

    #[test]
    fn sovereign_cap_uses_are_counted() {
        let mut r = row("2026-04-30T01:00:00Z", 1, "network.outbound", AuditLevel::Info);
        r.sovereign_cap_used = true;
        let att = aggregate(&[r]);
        assert_eq!(att.sovereign_used, 1);
        // Network.outbound classified to Consent + Transparency.
        assert_eq!(att.axis_counts.get(&DirectiveAxis::Consent).copied(), Some(1));
        assert_eq!(
            att.axis_counts.get(&DirectiveAxis::Transparency).copied(),
            Some(1)
        );
    }

    #[test]
    fn all_axes_are_reachable_through_classifier() {
        let rows = vec![
            row("2026-04-30T01:00:00Z", 1, "network.outbound", AuditLevel::Info), // Consent + Transparency
            row("2026-04-30T01:00:01Z", 2, "audio.capture.start", AuditLevel::Info), // NoSurveillance
            row("2026-04-30T01:00:02Z", 3, "intent.denied", AuditLevel::Info), // NoControl + Sovereignty
            row("2026-04-30T01:00:03Z", 4, "ui.dark_pattern", AuditLevel::Info), // NoManipulation
            row("2026-04-30T01:00:04Z", 5, "session.lockin", AuditLevel::Info), // NoCoercion + NoExploitation
            row("2026-04-30T01:00:05Z", 6, "runtime.panic", AuditLevel::Info),  // NoHarm
        ];
        let att = aggregate(&rows);
        for axis in [
            DirectiveAxis::Consent,
            DirectiveAxis::Sovereignty,
            DirectiveAxis::Transparency,
            DirectiveAxis::NoHarm,
            DirectiveAxis::NoControl,
            DirectiveAxis::NoManipulation,
            DirectiveAxis::NoSurveillance,
            DirectiveAxis::NoExploitation,
            DirectiveAxis::NoCoercion,
        ] {
            assert!(
                att.axis_counts.contains_key(&axis),
                "axis {axis:?} missing from coverage"
            );
        }
    }

    #[test]
    fn harm_flags_sort_by_ts_ascending() {
        let rows = vec![
            row("2026-04-30T01:00:09Z", 9, "denied", AuditLevel::Info),
            row("2026-04-30T01:00:01Z", 1, "denied", AuditLevel::Info),
            row("2026-04-30T01:00:05Z", 5, "denied", AuditLevel::Info),
        ];
        let att = aggregate(&rows);
        let ts: Vec<String> = att.harm_flags.iter().map(|f| f.ts_iso.clone()).collect();
        let mut sorted = ts.clone();
        sorted.sort();
        assert_eq!(ts, sorted);
        // All denied → all Info severity → MajorFlags (3+ flags).
        assert_eq!(att.verdict, AttestationVerdict::MajorFlags);
    }

    #[test]
    fn verdict_derives_from_flag_counts_and_severity() {
        // Empty → Clean
        assert_eq!(derive_verdict(&[]), AttestationVerdict::Clean);
        // 1 Info → MinorFlags
        let f_info = HarmFlag {
            ts_iso: "x".into(),
            axis: DirectiveAxis::Sovereignty,
            severity: HarmSeverity::Info,
            kind: "denied".into(),
            message: String::new(),
        };
        assert_eq!(
            derive_verdict(std::slice::from_ref(&f_info)),
            AttestationVerdict::MinorFlags
        );
        // 1 Warn → MajorFlags
        let mut f_warn = f_info.clone();
        f_warn.severity = HarmSeverity::Warn;
        assert_eq!(
            derive_verdict(std::slice::from_ref(&f_warn)),
            AttestationVerdict::MajorFlags
        );
        // 1 Critical → BlockingViolation
        let mut f_crit = f_info;
        f_crit.severity = HarmSeverity::Critical;
        assert_eq!(
            derive_verdict(std::slice::from_ref(&f_crit)),
            AttestationVerdict::BlockingViolation
        );
    }

    #[test]
    fn time_window_uses_min_and_max_micros() {
        let rows = vec![
            row("2026-04-30T03:00:00Z", 3, "a", AuditLevel::Info),
            row("2026-04-30T01:00:00Z", 1, "b", AuditLevel::Info),
            row("2026-04-30T02:00:00Z", 2, "c", AuditLevel::Info),
        ];
        let att = aggregate(&rows);
        assert_eq!(att.started_at_iso, "2026-04-30T01:00:00Z");
        assert_eq!(att.ended_at_iso, "2026-04-30T03:00:00Z");
    }

    #[test]
    fn with_session_id_chains() {
        let att = aggregate(&[]).with_session_id("sess-1234");
        assert_eq!(att.session_id, "sess-1234");
    }

    #[test]
    fn critical_level_promotes_to_critical_flag() {
        let r = row("2026-04-30T01:00:00Z", 1, "boom", AuditLevel::Critical);
        let att = aggregate(&[r]);
        assert_eq!(att.harm_flags.len(), 1);
        assert_eq!(att.harm_flags[0].severity, HarmSeverity::Critical);
        assert_eq!(att.verdict, AttestationVerdict::BlockingViolation);
    }

    #[test]
    fn round_trip_serde_session_attestation() {
        let mut att = aggregate(&[row("2026-04-30T01:00:00Z", 1, "denied", AuditLevel::Info)]);
        att.session_id = "sess-abc".to_string();
        let s = serde_json::to_string(&att).expect("serialize");
        let back: SessionAttestation = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(att, back);
    }
}

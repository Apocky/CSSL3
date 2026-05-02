//! § kan_bridge — distill a `PlayTestReport` into a `QualitySignal`
//! that the sibling W12-3 `cssl-self-authoring-kan` ingests as bias-axes.
//!
//! § ROLE
//!   Mirrors the `QualitySignal` shape used by `cssl-content-rating`'s
//!   `kan_bridge` so the KAN-loop receives a uniform feature-vector
//!   regardless of source-channel (auto-playtest vs human-rating).
//!
//! § FIELDS
//!   - `total_q8`         : weighted-aggregate / 100 * 255 ; saturating cast
//!   - `safety_q8`        : safety / 100 * 255
//!   - `fun_q8`           : fun / 100 * 255
//!   - `balance_q8`       : balance / 100 * 255
//!   - `polish_q8`        : polish / 100 * 255
//!   - `is_publishable`   : 1 iff verdict == Publishable, else 0
//!   - `cosmetic_attest`  : 1 iff zero pay-for-power paths reachable
//!   - `crash_count`      : raw crash-counter (capped at u16::MAX)
//!   - `softlock_count`   : raw softlock-counter (capped at u16::MAX)
//!   - `determinism_ok`   : 1 iff replay-equal trace observed
//!   - `protocol_version` : matches the report's `protocol_version`
//!
//! § PRIME-DIRECTIVE
//!   The signal carries NO scene-bytes ; only aggregated numerics + bools.
//!   Safe to ship into the KAN-loop without re-applying Σ-mask gating.

use serde::{Deserialize, Serialize};

use crate::report::{PlayTestReport, ReportPublishVerdict};

/// § Compact bias-axes vector. Stored as `u8`/`u16` so the KAN-input row
/// stays ≤ 16 bytes (Sawyer-mindset : pack-don't-bloat).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualitySignal {
    /// Weighted aggregate, scaled `[0, 100] → [0, 255]`.
    pub total_q8: u8,
    /// Safety axis, scaled.
    pub safety_q8: u8,
    /// Fun axis, scaled.
    pub fun_q8: u8,
    /// Balance axis, scaled.
    pub balance_q8: u8,
    /// Polish axis, scaled.
    pub polish_q8: u8,
    /// Boolean : is the report `Publishable` ?
    pub is_publishable: u8,
    /// Boolean : did the cosmetic-axiom hold ?
    pub cosmetic_attest: u8,
    /// Saturating cast of `crashes` to `u16`.
    pub crash_count: u16,
    /// Saturating cast of `softlocks` to `u16`.
    pub softlock_count: u16,
    /// Boolean : was the replay deterministic ?
    pub determinism_ok: u8,
    /// Wire-format protocol-version (mirrors the source report).
    pub protocol_version: u32,
}

impl QualitySignal {
    /// § Construct from a [`PlayTestReport`]. The KAN-loop reads only this
    /// view ; the full report stays inside the playtest-coordinator.
    #[must_use]
    pub fn from_report(r: &PlayTestReport) -> Self {
        // q8 = score * 255 / 100 (saturating). 100 is hard-cap so this
        // can't overflow the u32 intermediate.
        let q8 = |v: u8| -> u8 { ((u32::from(v) * 255) / 100) as u8 };
        Self {
            total_q8: q8(r.total),
            safety_q8: q8(r.safety.0),
            fun_q8: q8(r.fun.0),
            balance_q8: q8(r.balance.0),
            polish_q8: q8(r.polish.0),
            is_publishable: u8::from(matches!(r.verdict, ReportPublishVerdict::Publishable)),
            cosmetic_attest: u8::from(r.cosmetic_attest),
            crash_count: u16::try_from(r.crashes).unwrap_or(u16::MAX),
            softlock_count: u16::try_from(r.softlocks).unwrap_or(u16::MAX),
            determinism_ok: u8::from(r.determinism),
            protocol_version: r.protocol_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{PlayTestReport, Suggestion};
    use crate::scoring::{Score, Thresholds};

    #[test]
    fn signal_emission_from_clean_report() {
        let r = PlayTestReport::assemble(
            7,
            42,
            0,
            0,
            true,
            true,
            Score(100),
            Score(100),
            Score(100),
            Score(100),
            Thresholds::default(),
            vec![Suggestion::new("§ ALL ✓")],
        );
        let q = QualitySignal::from_report(&r);
        assert_eq!(q.total_q8, 255);
        assert_eq!(q.safety_q8, 255);
        assert_eq!(q.is_publishable, 1);
        assert_eq!(q.cosmetic_attest, 1);
        assert_eq!(q.determinism_ok, 1);
        assert_eq!(q.crash_count, 0);
        assert_eq!(q.softlock_count, 0);
    }

    #[test]
    fn signal_emission_from_failing_report() {
        let r = PlayTestReport::assemble(
            7,
            42,
            3,
            2,
            false,
            false,
            Score(40),
            Score(50),
            Score(50),
            Score(20),
            Thresholds::default(),
            vec![],
        );
        let q = QualitySignal::from_report(&r);
        assert_eq!(q.is_publishable, 0);
        assert_eq!(q.cosmetic_attest, 0);
        assert_eq!(q.determinism_ok, 0);
        assert_eq!(q.crash_count, 3);
        assert_eq!(q.softlock_count, 2);
    }
}

//! StrictClock — the only path through which `monotonic_ns()` may be
//! observed under `DeterminismMode::Strict`.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.2 + slice-prompt
//!     T11-D161 LOGICAL-FRAME-N DISCIPLINE.
//!
//! § DISCIPLINE
//!
//!   `monotonic_ns()` ↦ `(frame_n × FRAME_NS) + sub_phase_ns_offset`
//!
//!   The `sub_phase_ns_offset` table is a build-time constant derived from
//!   `Omniverse/03_RUNTIME/01_COMPUTE_GRAPH.csl.md § V` phase-ordering :
//!
//!   - `SubPhase::Collapse`   →  0 ns        (4ms budget)
//!   - `SubPhase::Propagate`  →  4_000_000   (4ms budget)
//!   - `SubPhase::Compose`    →  8_000_000   (2ms budget)
//!   - `SubPhase::Cohomology` → 10_000_000   (2ms budget)
//!   - `SubPhase::Agency`     → 12_000_000   (2ms budget)
//!   - `SubPhase::Entropy`    → 14_000_000   (2ms budget)
//!   - `SubPhase::FrameEnd`   → 16_000_000   (= FRAME_NS marker)
//!
//! § SATURATING ARITHMETIC
//!
//!   `(frame_n × FRAME_NS)` uses saturating multiplication to defend
//!   against `frame_n` overflow ; the cap is `u64::MAX`. This matches the
//!   spec § VII.5 mitigation for sub_phase ns-offset wraparound.

use crate::FrameN;
use thiserror::Error;

/// Logical-frame budget in nanoseconds. Derived from the spec § V density
/// budget : 60Hz target ⇒ 16.66… ms ; this constant rounds-down to
/// 16_000_000 ns to keep sub-phase offsets exact.
pub const FRAME_NS: u64 = 16_000_000;

/// Per-frame compute-graph sub-phase, ordered per `§ V` phase-ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubPhase {
    /// Phase 1 : COLLAPSE — observation reduction.
    Collapse,
    /// Phase 2 : PROPAGATE — broadphase + flux + transport.
    Propagate,
    /// Phase 3 : COMPOSE — cohomology composition + lifting.
    Compose,
    /// Phase 4 : COHOMOLOGY — invariant tracking (birth/persist/transform/die).
    Cohomology,
    /// Phase 5 : AGENCY — consent + sovereignty + reversibility checks.
    Agency,
    /// Phase 6 : ENTROPY — σ-balance + drift accounting.
    Entropy,
    /// End-of-frame anchor : equals `FRAME_NS` exactly. Useful as a
    /// canonical "frame submitted" timestamp.
    FrameEnd,
}

impl SubPhase {
    /// Canonical ordered list of all six compute-graph phases (NOT
    /// including [`Self::FrameEnd`]). Useful for replay-deterministic
    /// iteration.
    pub const ORDER: [Self; 6] = [
        Self::Collapse,
        Self::Propagate,
        Self::Compose,
        Self::Cohomology,
        Self::Agency,
        Self::Entropy,
    ];

    /// Stable `&'static str` discriminant for replay-log encoding.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Collapse => "collapse",
            Self::Propagate => "propagate",
            Self::Compose => "compose",
            Self::Cohomology => "cohomology",
            Self::Agency => "agency",
            Self::Entropy => "entropy",
            Self::FrameEnd => "frame_end",
        }
    }

    /// Phase index in `[0, 6]`. `FrameEnd` is index 6.
    #[must_use]
    pub const fn index(&self) -> u8 {
        match self {
            Self::Collapse => 0,
            Self::Propagate => 1,
            Self::Compose => 2,
            Self::Cohomology => 3,
            Self::Agency => 4,
            Self::Entropy => 5,
            Self::FrameEnd => 6,
        }
    }
}

/// Sub-phase nanosecond offset within a single frame.
///
/// § SPEC : § VI.2 + T11-D161 LOGICAL-FRAME-N DISCIPLINE.
///
/// The returned value is **deterministic** — same `(frame_n, sub_phase)` ⇒
/// same ns. No wallclock leak.
#[must_use]
pub const fn sub_phase_offset_ns(sub_phase: SubPhase) -> u64 {
    match sub_phase {
        SubPhase::Collapse => 0,
        SubPhase::Propagate => 4_000_000,
        SubPhase::Compose => 8_000_000,
        SubPhase::Cohomology => 10_000_000,
        SubPhase::Agency => 12_000_000,
        SubPhase::Entropy => 14_000_000,
        SubPhase::FrameEnd => FRAME_NS,
    }
}

/// Strict-clock primitive : `monotonic_ns()` replacement under `Strict` mode.
///
/// § SPEC : § VI.2.
///
/// Returns `(frame_n × FRAME_NS) + sub_phase_offset_ns(sub_phase)` with
/// **saturating multiplication**. Bit-deterministic ; no wallclock read.
#[must_use]
pub fn strict_ns(frame_n: FrameN, sub_phase: SubPhase) -> u64 {
    let frame_part = frame_n.saturating_mul(FRAME_NS);
    let phase_part = sub_phase_offset_ns(sub_phase);
    frame_part.saturating_add(phase_part)
}

/// StrictClock — caller-facing handle. Holds a `(frame_n, sub_phase)`
/// cursor that can be advanced phase-by-phase or jumped to a specific
/// frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StrictClock {
    frame_n: FrameN,
    sub_phase: SubPhase,
}

impl StrictClock {
    /// Construct a clock at `(frame_n, Collapse)`.
    #[must_use]
    pub const fn at_frame(frame_n: FrameN) -> Self {
        Self {
            frame_n,
            sub_phase: SubPhase::Collapse,
        }
    }

    /// Construct a clock at exactly `(frame_n, sub_phase)`.
    #[must_use]
    pub const fn at(frame_n: FrameN, sub_phase: SubPhase) -> Self {
        Self {
            frame_n,
            sub_phase,
        }
    }

    /// Snapshot the current `(frame_n, sub_phase)` pair.
    #[must_use]
    pub const fn cursor(&self) -> (FrameN, SubPhase) {
        (self.frame_n, self.sub_phase)
    }

    /// Read the strict-ns at the current cursor.
    #[must_use]
    pub fn now_ns(&self) -> u64 {
        strict_ns(self.frame_n, self.sub_phase)
    }

    /// Advance to the next sub-phase. After [`SubPhase::Entropy`], the
    /// cursor wraps to `(frame_n + 1, Collapse)` — this is the
    /// canonical end-of-frame transition.
    pub fn advance_sub_phase(&mut self) -> Result<(), StrictClockError> {
        let next_phase_index = self.sub_phase.index() + 1;
        if next_phase_index >= 6 {
            // Wrap to next frame.
            self.frame_n = self
                .frame_n
                .checked_add(1)
                .ok_or(StrictClockError::FrameNOverflow)?;
            self.sub_phase = SubPhase::Collapse;
        } else {
            self.sub_phase = match next_phase_index {
                1 => SubPhase::Propagate,
                2 => SubPhase::Compose,
                3 => SubPhase::Cohomology,
                4 => SubPhase::Agency,
                5 => SubPhase::Entropy,
                _ => unreachable!("guarded by next_phase_index < 6"),
            };
        }
        Ok(())
    }

    /// Jump to an explicit `(frame_n, sub_phase)`. No wallclock read.
    pub fn jump_to(&mut self, frame_n: FrameN, sub_phase: SubPhase) {
        self.frame_n = frame_n;
        self.sub_phase = sub_phase;
    }
}

impl Default for StrictClock {
    fn default() -> Self {
        Self::at_frame(0)
    }
}

/// Errors from [`StrictClock`] operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrictClockError {
    /// `frame_n + 1` would overflow `u64`. This is a saturating-arithmetic
    /// guard ; in practice unreachable, but encoded for spec-§-VII.5
    /// landmine coverage.
    #[error("PD0161 — frame-N overflow ; logical-frame counter exhausted")]
    FrameNOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_collapse_offset_zero() {
        assert_eq!(sub_phase_offset_ns(SubPhase::Collapse), 0);
    }

    #[test]
    fn t_propagate_offset_4ms() {
        assert_eq!(sub_phase_offset_ns(SubPhase::Propagate), 4_000_000);
    }

    #[test]
    fn t_compose_offset_8ms() {
        assert_eq!(sub_phase_offset_ns(SubPhase::Compose), 8_000_000);
    }

    #[test]
    fn t_cohomology_offset_10ms() {
        assert_eq!(sub_phase_offset_ns(SubPhase::Cohomology), 10_000_000);
    }

    #[test]
    fn t_agency_offset_12ms() {
        assert_eq!(sub_phase_offset_ns(SubPhase::Agency), 12_000_000);
    }

    #[test]
    fn t_entropy_offset_14ms() {
        assert_eq!(sub_phase_offset_ns(SubPhase::Entropy), 14_000_000);
    }

    #[test]
    fn t_frame_end_eq_frame_ns() {
        assert_eq!(sub_phase_offset_ns(SubPhase::FrameEnd), FRAME_NS);
    }

    #[test]
    fn t_strict_ns_frame0_collapse_zero() {
        assert_eq!(strict_ns(0, SubPhase::Collapse), 0);
    }

    #[test]
    fn t_strict_ns_frame1_collapse_eq_frame_ns() {
        assert_eq!(strict_ns(1, SubPhase::Collapse), FRAME_NS);
    }

    #[test]
    fn t_strict_ns_frame100_propagate() {
        assert_eq!(
            strict_ns(100, SubPhase::Propagate),
            100 * FRAME_NS + 4_000_000
        );
    }

    #[test]
    fn t_strict_ns_saturates_at_u64_max() {
        assert_eq!(strict_ns(u64::MAX, SubPhase::Entropy), u64::MAX);
    }

    #[test]
    fn t_strict_ns_deterministic_repeat() {
        let a = strict_ns(42, SubPhase::Compose);
        let b = strict_ns(42, SubPhase::Compose);
        assert_eq!(a, b);
    }

    #[test]
    fn t_clock_default_at_origin() {
        let c = StrictClock::default();
        assert_eq!(c.cursor(), (0, SubPhase::Collapse));
        assert_eq!(c.now_ns(), 0);
    }

    #[test]
    fn t_clock_at_specific() {
        let c = StrictClock::at(7, SubPhase::Cohomology);
        assert_eq!(c.cursor(), (7, SubPhase::Cohomology));
        assert_eq!(c.now_ns(), 7 * FRAME_NS + 10_000_000);
    }

    #[test]
    fn t_clock_advance_within_frame() {
        let mut c = StrictClock::at_frame(0);
        c.advance_sub_phase().unwrap();
        assert_eq!(c.cursor(), (0, SubPhase::Propagate));
        c.advance_sub_phase().unwrap();
        assert_eq!(c.cursor(), (0, SubPhase::Compose));
    }

    #[test]
    fn t_clock_advance_wraps_frame() {
        let mut c = StrictClock::at(3, SubPhase::Entropy);
        c.advance_sub_phase().unwrap();
        assert_eq!(c.cursor(), (4, SubPhase::Collapse));
    }

    #[test]
    fn t_clock_advance_overflow_refuses() {
        let mut c = StrictClock::at(u64::MAX, SubPhase::Entropy);
        let r = c.advance_sub_phase();
        assert_eq!(r, Err(StrictClockError::FrameNOverflow));
    }

    #[test]
    fn t_clock_jump_to() {
        let mut c = StrictClock::default();
        c.jump_to(99, SubPhase::Agency);
        assert_eq!(c.cursor(), (99, SubPhase::Agency));
    }

    #[test]
    fn t_subphase_order_six_phases() {
        assert_eq!(SubPhase::ORDER.len(), 6);
        assert_eq!(SubPhase::ORDER[0], SubPhase::Collapse);
        assert_eq!(SubPhase::ORDER[5], SubPhase::Entropy);
    }

    #[test]
    fn t_subphase_index_dense() {
        for (i, sp) in SubPhase::ORDER.iter().enumerate() {
            assert_eq!(sp.index() as usize, i);
        }
        assert_eq!(SubPhase::FrameEnd.index(), 6);
    }

    #[test]
    fn t_subphase_as_str_stable() {
        assert_eq!(SubPhase::Collapse.as_str(), "collapse");
        assert_eq!(SubPhase::FrameEnd.as_str(), "frame_end");
    }
}

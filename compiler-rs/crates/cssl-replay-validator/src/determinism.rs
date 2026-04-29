//! Determinism mode flag — the canonical surface that gates every replay-
//! sensitive operation in `cssl-metrics` / `cssl-log` / `cssl-spec-coverage`.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § VI.1.

use crate::FrameN;

/// Determinism mode for metric-recording.
///
/// § SPEC : § VI.1 (06_l2_telemetry_spec).
///
/// The slice-prompt (T11-D161) names the variants `Strict` / `Lenient`.
/// The reference-spec § VI.1 names them `ReplayStrict` / `Realtime`. Both
/// vocabularies are honored : the slice-prompt names ARE the public API
/// (this enum) ; the reference-spec names map 1:1 via [`Self::kind`].
///
/// The spec also mentions a `Mixed` variant ; that variant is intentionally
/// elided in this slice — it is a debug-mode fallback and out-of-scope for
/// the H5 gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeterminismMode {
    /// Strict / "ReplayStrict" — metrics record to the [`crate::ReplayLog`] and
    /// every timing-based recording is a deterministic function of
    /// `(frame_n, sub_phase)`. Adaptive-sampling is refused. Wallclock
    /// reads are refused.
    ///
    /// [`crate::ReplayLog`]: crate::ReplayLog
    Strict(ReplayStrictConfig),
    /// Lenient / "Realtime" — wallclock + adaptive-sampling permitted.
    /// The [`crate::ReplayLog`] is NOT engaged ; metrics flow directly to the
    /// telemetry ring.
    Lenient,
}

impl DeterminismMode {
    /// Convenience : default `Strict` with a fixed seed (for tests).
    #[must_use]
    pub const fn strict_with_seed(seed: u64) -> Self {
        Self::Strict(ReplayStrictConfig::with_seed(seed))
    }

    /// Reduce to the kind discriminant for switch-on-mode logic.
    #[must_use]
    pub const fn kind(&self) -> DeterminismModeKind {
        match self {
            Self::Strict(_) => DeterminismModeKind::Strict,
            Self::Lenient => DeterminismModeKind::Lenient,
        }
    }

    /// Whether the [`crate::ReplayLog`] is engaged in this mode.
    ///
    /// [`crate::ReplayLog`]: crate::ReplayLog
    #[must_use]
    pub const fn engages_replay_log(&self) -> bool {
        matches!(self, Self::Strict(_))
    }

    /// Whether wallclock reads are PERMITTED in this mode.
    ///
    /// Returning `false` means `monotonic_ns()` direct-calls are refused
    /// (LM-1) — callers must route through `StrictClock`.
    #[must_use]
    pub const fn permits_wallclock(&self) -> bool {
        matches!(self, Self::Lenient)
    }

    /// Whether adaptive-sampling is permitted (LM-2).
    #[must_use]
    pub const fn permits_adaptive_sampling(&self) -> bool {
        matches!(self, Self::Lenient)
    }

    /// Whether the spec-§-VI.4 forbidden-patterns are blocked.
    #[must_use]
    pub const fn enforces_forbidden_patterns(&self) -> bool {
        matches!(self, Self::Strict(_))
    }

    /// Extract the seed for [`Strict`] mode, if applicable.
    ///
    /// [`Strict`]: Self::Strict
    #[must_use]
    pub const fn seed(&self) -> Option<u64> {
        match self {
            Self::Strict(cfg) => Some(cfg.seed),
            Self::Lenient => None,
        }
    }
}

/// Lightweight discriminant — useful as a constant tag, hash key, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeterminismModeKind {
    Strict = 1,
    Lenient = 0,
}

impl DeterminismModeKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Lenient => "lenient",
        }
    }
}

/// Configuration for [`DeterminismMode::Strict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReplayStrictConfig {
    /// Seed for sampling-decimation + replay-log key.
    pub seed: u64,
    /// Starting frame-N for the replay window.
    pub start_frame: FrameN,
    /// Whether `audit-chain` integration is engaged.
    /// (Off by default in this slice ; wired in Wave-Jθ.)
    pub audit_chain: bool,
}

impl ReplayStrictConfig {
    /// Construct with seed only ; defaults : `start_frame = 0`,
    /// `audit_chain = false`.
    #[must_use]
    pub const fn with_seed(seed: u64) -> Self {
        Self {
            seed,
            start_frame: 0,
            audit_chain: false,
        }
    }

    /// Override the start frame.
    #[must_use]
    pub const fn with_start_frame(mut self, start_frame: FrameN) -> Self {
        self.start_frame = start_frame;
        self
    }

    /// Engage the audit-chain integration (Wave-Jθ wire).
    #[must_use]
    pub const fn with_audit_chain(mut self, audit_chain: bool) -> Self {
        self.audit_chain = audit_chain;
        self
    }
}

impl Default for ReplayStrictConfig {
    fn default() -> Self {
        Self::with_seed(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_kind_disc_strict() {
        assert_eq!(
            DeterminismMode::strict_with_seed(0).kind(),
            DeterminismModeKind::Strict
        );
    }

    #[test]
    fn t_kind_disc_lenient() {
        assert_eq!(
            DeterminismMode::Lenient.kind(),
            DeterminismModeKind::Lenient
        );
    }

    #[test]
    fn t_strict_engages_replay_log() {
        assert!(DeterminismMode::strict_with_seed(7).engages_replay_log());
    }

    #[test]
    fn t_lenient_does_not_engage_replay_log() {
        assert!(!DeterminismMode::Lenient.engages_replay_log());
    }

    #[test]
    fn t_strict_refuses_wallclock() {
        assert!(!DeterminismMode::strict_with_seed(1).permits_wallclock());
    }

    #[test]
    fn t_lenient_permits_wallclock() {
        assert!(DeterminismMode::Lenient.permits_wallclock());
    }

    #[test]
    fn t_strict_refuses_adaptive() {
        assert!(!DeterminismMode::strict_with_seed(1).permits_adaptive_sampling());
    }

    #[test]
    fn t_strict_enforces_forbidden() {
        assert!(DeterminismMode::strict_with_seed(1).enforces_forbidden_patterns());
    }

    #[test]
    fn t_seed_threaded() {
        assert_eq!(
            DeterminismMode::strict_with_seed(0xC0FFEE).seed(),
            Some(0xC0FFEE)
        );
        assert_eq!(DeterminismMode::Lenient.seed(), None);
    }

    #[test]
    fn t_kind_str() {
        assert_eq!(DeterminismModeKind::Strict.as_str(), "strict");
        assert_eq!(DeterminismModeKind::Lenient.as_str(), "lenient");
    }

    #[test]
    fn t_replay_cfg_with_start_frame() {
        let cfg = ReplayStrictConfig::with_seed(1).with_start_frame(42);
        assert_eq!(cfg.start_frame, 42);
        assert_eq!(cfg.seed, 1);
    }

    #[test]
    fn t_replay_cfg_audit_off_default() {
        let cfg = ReplayStrictConfig::default();
        assert!(!cfg.audit_chain);
    }

    #[test]
    fn t_replay_cfg_audit_engaged() {
        let cfg = ReplayStrictConfig::with_seed(0).with_audit_chain(true);
        assert!(cfg.audit_chain);
    }
}

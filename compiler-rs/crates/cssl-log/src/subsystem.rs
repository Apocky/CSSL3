//! Subsystem-tag catalog matching engine subsystems.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.7.
//!
//! § STABILITY DISCIPLINE :
//!   - Adding a variant = additive ; existing payload-encodings retained.
//!   - **Renaming = §7-INTEGRITY violation** (payload-encoded as u8 ⟵ old
//!     logs would mis-decode).
//!   - Variant order is FROZEN ; new variants append at the end.
//!   - Discriminants are stable u8 values exposed via [`Self::as_u8`] +
//!     reverse-decoded via [`Self::from_u8`].

use core::fmt;

/// Subsystem-tag catalog. Mirrors spec § 2.7 verbatim. Adding a subsystem
/// requires a DECISIONS-pin per spec stability rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubsystemTag {
    /// Top-level orchestrator.
    Engine,
    /// `omega_step` driver / 1kHz tick.
    OmegaStep,
    /// `cssl-render-v2` pipeline.
    Render,
    /// Wave-physics + sdf.
    Physics,
    /// Wave-audio + wave-coupler.
    Audio,
    /// Anim-procedural + KAN-anim.
    Anim,
    /// Companion + decision tree.
    Ai,
    /// UI surface (loa-game UI).
    Ui,
    /// OpenXR / XR session.
    Xr,
    /// Host-level (Level-Zero / Vulkan).
    Host,
    /// Wave-solver + LBM.
    WaveSolver,
    /// KAN substrate.
    Kan,
    /// Gaze-collapse + foveation.
    Gaze,
    /// Companion-perspective (consent-gated rendering).
    Companion,
    /// Mise-en-abyme recursion.
    MiseEnAbyme,
    /// Hot-reload pipeline.
    HotReload,
    /// MCP IPC layer (Wave-Jθ).
    Mcp,
    /// Telemetry ring + exporter.
    Telemetry,
    /// Audit-chain.
    Audit,
    /// PD enforcement.
    PrimeDirective,
    /// Test-only emissions.
    Test,
}

impl SubsystemTag {
    /// Stable short-name (kebab-case) for sink encoding + filter parsing.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::OmegaStep => "omega-step",
            Self::Render => "render",
            Self::Physics => "physics",
            Self::Audio => "audio",
            Self::Anim => "anim",
            Self::Ai => "ai",
            Self::Ui => "ui",
            Self::Xr => "xr",
            Self::Host => "host",
            Self::WaveSolver => "wave-solver",
            Self::Kan => "kan",
            Self::Gaze => "gaze",
            Self::Companion => "companion",
            Self::MiseEnAbyme => "mise-en-abyme",
            Self::HotReload => "hot-reload",
            Self::Mcp => "mcp",
            Self::Telemetry => "telemetry",
            Self::Audit => "audit",
            Self::PrimeDirective => "prime-directive",
            Self::Test => "test",
        }
    }

    /// Stable u8 wire-encoding. Variant-order frozen — see module-doc.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Engine => 0,
            Self::OmegaStep => 1,
            Self::Render => 2,
            Self::Physics => 3,
            Self::Audio => 4,
            Self::Anim => 5,
            Self::Ai => 6,
            Self::Ui => 7,
            Self::Xr => 8,
            Self::Host => 9,
            Self::WaveSolver => 10,
            Self::Kan => 11,
            Self::Gaze => 12,
            Self::Companion => 13,
            Self::MiseEnAbyme => 14,
            Self::HotReload => 15,
            Self::Mcp => 16,
            Self::Telemetry => 17,
            Self::Audit => 18,
            Self::PrimeDirective => 19,
            Self::Test => 20,
        }
    }

    /// Reverse-decode a u8 wire-encoding back into [`SubsystemTag`].
    /// Returns `None` on out-of-range.
    #[must_use]
    pub const fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Engine),
            1 => Some(Self::OmegaStep),
            2 => Some(Self::Render),
            3 => Some(Self::Physics),
            4 => Some(Self::Audio),
            5 => Some(Self::Anim),
            6 => Some(Self::Ai),
            7 => Some(Self::Ui),
            8 => Some(Self::Xr),
            9 => Some(Self::Host),
            10 => Some(Self::WaveSolver),
            11 => Some(Self::Kan),
            12 => Some(Self::Gaze),
            13 => Some(Self::Companion),
            14 => Some(Self::MiseEnAbyme),
            15 => Some(Self::HotReload),
            16 => Some(Self::Mcp),
            17 => Some(Self::Telemetry),
            18 => Some(Self::Audit),
            19 => Some(Self::PrimeDirective),
            20 => Some(Self::Test),
            _ => None,
        }
    }

    /// Iterate all 21 variants in canonical order — used by tests +
    /// bitfield installation in [`crate::enabled`].
    #[must_use]
    pub const fn all() -> [Self; 21] {
        [
            Self::Engine,
            Self::OmegaStep,
            Self::Render,
            Self::Physics,
            Self::Audio,
            Self::Anim,
            Self::Ai,
            Self::Ui,
            Self::Xr,
            Self::Host,
            Self::WaveSolver,
            Self::Kan,
            Self::Gaze,
            Self::Companion,
            Self::MiseEnAbyme,
            Self::HotReload,
            Self::Mcp,
            Self::Telemetry,
            Self::Audit,
            Self::PrimeDirective,
            Self::Test,
        ]
    }

    /// Number of variants. Const so [`crate::enabled::EnabledTable`] +
    /// [`crate::sample::FrameCounters`] can size their arrays.
    pub const COUNT: usize = 21;
}

impl fmt::Display for SubsystemTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::SubsystemTag;

    #[test]
    fn all_returns_21_variants() {
        assert_eq!(SubsystemTag::all().len(), 21);
        assert_eq!(SubsystemTag::COUNT, 21);
    }

    #[test]
    fn as_u8_unique_per_variant() {
        let mut seen = std::collections::HashSet::new();
        for t in SubsystemTag::all() {
            assert!(seen.insert(t.as_u8()), "duplicate u8 for {t:?}");
        }
    }

    #[test]
    fn from_u8_round_trips_all() {
        for t in SubsystemTag::all() {
            assert_eq!(SubsystemTag::from_u8(t.as_u8()), Some(t));
        }
    }

    #[test]
    fn from_u8_out_of_range_is_none() {
        assert_eq!(SubsystemTag::from_u8(21), None);
        assert_eq!(SubsystemTag::from_u8(255), None);
    }

    #[test]
    fn discriminants_are_zero_indexed_and_dense() {
        for (i, t) in SubsystemTag::all().iter().enumerate() {
            assert_eq!(t.as_u8() as usize, i, "discriminant {} != index {}", t.as_u8(), i);
        }
    }

    #[test]
    fn as_str_unique_per_variant() {
        let mut seen = std::collections::HashSet::new();
        for t in SubsystemTag::all() {
            assert!(seen.insert(t.as_str()), "duplicate str for {t:?}");
        }
    }

    #[test]
    fn as_str_kebab_case() {
        // Per spec § 2.7 stability — kebab-case for filter parsing.
        for t in SubsystemTag::all() {
            let s = t.as_str();
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "non-kebab-case : {s}"
            );
            assert!(!s.starts_with('-') && !s.ends_with('-'));
        }
    }

    #[test]
    fn display_matches_as_str() {
        for t in SubsystemTag::all() {
            assert_eq!(format!("{t}"), t.as_str());
        }
    }

    #[test]
    fn engine_variant_pinned_at_zero() {
        // Engine is the canonical "top-level orchestrator" — pinned at 0
        // for default-init zero-bytes payloads.
        assert_eq!(SubsystemTag::Engine.as_u8(), 0);
    }

    #[test]
    fn pd_variant_pinned() {
        // PrimeDirective MUST be addressable for severity-classification
        // tests + audit-bridge ; pinning the discriminant catches drift.
        assert_eq!(SubsystemTag::PrimeDirective.as_u8(), 19);
    }

    #[test]
    fn ordering_matches_discriminant() {
        let all = SubsystemTag::all();
        for w in all.windows(2) {
            assert!(w[0] < w[1], "ordering mismatch {:?} < {:?}", w[0], w[1]);
        }
    }

    #[test]
    fn copy_clone_works() {
        let t = SubsystemTag::Render;
        let t2 = t;
        let t3 = t.clone();
        assert_eq!(t, t2);
        assert_eq!(t, t3);
    }
}

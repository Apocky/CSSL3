//! 30 default tunables — implements the LOAD-BEARING table in
//! `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 4.3.
//!
//! Per the T11-D164 prompt, the 30-tunable table is load-bearing. The first
//! 29 rows are copied verbatim from spec § 4.3 ; the 30th row
//! (`render.mise_en_abyme_recursion_cap`) is the substrate-side recursion-cap
//! that the prompt's landmine-list calls out by name. It is `HardReject` so
//! the scene-graph cannot accidentally blow its stack.

use crate::registry::TunableRegistry;
use crate::tunable::{
    BudgetMode, TunableId, TunableKind, TunableRange, TunableSpec, TunableValue, TweakError,
};

/// Number of default tunables installed by [`install_defaults`].
pub const DEFAULT_TUNABLE_COUNT: usize = 30;

/// Build the 30 default tunable specs in spec-order.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn default_tunable_specs() -> Vec<TunableSpec> {
    vec![
        // ─── render (6) ────────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "render.fovea_detail_budget",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(1.0 - f32::EPSILON),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "T0-fovea cell-density allowed (1.0 = full ; 0.5 = half)",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "render.foveation_aggression",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..2.0),
            default: TunableValue::F32(1.0),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "how aggressively to thin T2/T3 cells based on gaze",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "render.spectral_bands_active",
            kind: TunableKind::U32,
            range: TunableRange::U32(1..16),
            default: TunableValue::U32(15),
            budget_mode: BudgetMode::HardReject,
            description: "how many of the 16 spectral bands to render (cost-cap)",
            units: Some("bands"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "render.exposure_compensation",
            kind: TunableKind::F32,
            range: TunableRange::F32(-4.0..4.0),
            default: TunableValue::F32(0.0),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "EV offset for HDR display",
            units: Some("EV"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "render.tonemap_curve",
            kind: TunableKind::StringEnum,
            range: TunableRange::StringEnum(vec!["Reinhard", "Filmic", "ACES", "Hable"]),
            default: TunableValue::StringEnum("ACES".into()),
            budget_mode: BudgetMode::HardReject,
            description: "tonemap curve selector",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "render.shadow_resolution_log2",
            kind: TunableKind::U32,
            range: TunableRange::U32(8..14),
            default: TunableValue::U32(12),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "log2(shadow-map resolution) ; 12 = 4096",
            units: Some("log2(px)"),
            frame_boundary_defer: true,
        },
        // ─── physics (4) ───────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "physics.iter_count",
            kind: TunableKind::U32,
            range: TunableRange::U32(1..32),
            default: TunableValue::U32(8),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "LBM iterations per frame",
            units: Some("iters"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "physics.time_step_ms",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.5..16.0),
            default: TunableValue::F32(4.0),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "physics tick time step",
            units: Some("ms"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "physics.gravity_strength",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..50.0),
            default: TunableValue::F32(9.81),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "gravitational acceleration",
            units: Some("m/s^2"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "physics.collision_eps",
            kind: TunableKind::F32,
            range: TunableRange::F32(1e-5..1e-2),
            default: TunableValue::F32(1e-4),
            budget_mode: BudgetMode::HardReject,
            description: "collision-detection epsilon",
            units: Some("m"),
            frame_boundary_defer: true,
        },
        // ─── ai (4) ────────────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "ai.kan_band_weight_alpha",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(0.5),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "KAN-band weight alpha (training-loop annealing rate)",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "ai.kan_band_weight_beta",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(0.5),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "KAN-band weight beta",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "ai.fsm_state_dwell_min_ms",
            kind: TunableKind::U32,
            range: TunableRange::U32(16..2000),
            default: TunableValue::U32(250),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "minimum dwell time per FSM state",
            units: Some("ms"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "ai.policy_explore_rate",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(0.1),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "epsilon-greedy / softmax-temp explore parameter",
            units: None,
            frame_boundary_defer: true,
        },
        // ─── wave (3) ──────────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "wave.coupling_strength",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..2.0),
            default: TunableValue::F32(1.0),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "wave-unity psi-flow coupling-strength",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "wave.psi_band_count_active",
            kind: TunableKind::U32,
            range: TunableRange::U32(1..32),
            default: TunableValue::U32(16),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "how many psi-bands to integrate per tick",
            units: Some("bands"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "wave.dispersion_constant",
            kind: TunableKind::F64,
            range: TunableRange::F64(1e-3..1e3),
            default: TunableValue::F64(1.0),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "dispersion constant for psi-evolution",
            units: Some("m^2/s"),
            frame_boundary_defer: true,
        },
        // ─── audio (3) ─────────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "audio.spatial_quality",
            kind: TunableKind::StringEnum,
            range: TunableRange::StringEnum(vec!["Stereo", "Binaural", "Ambisonic", "FullHRTF"]),
            default: TunableValue::StringEnum("Binaural".into()),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "spatial-audio quality",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "audio.master_gain_db",
            kind: TunableKind::F32,
            range: TunableRange::F32(-60.0..12.0),
            default: TunableValue::F32(0.0),
            budget_mode: BudgetMode::HardReject,
            description: "master output gain (hard-clamped for hearing safety)",
            units: Some("dB"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "audio.reverb_mix_pct",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..100.0),
            default: TunableValue::F32(30.0),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "reverb wet/dry percentage",
            units: Some("%"),
            frame_boundary_defer: true,
        },
        // ─── engine (3) ────────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "engine.target_frame_rate_hz",
            kind: TunableKind::U32,
            range: TunableRange::U32(24..480),
            default: TunableValue::U32(120),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "target frame rate (vsync-honoring)",
            units: Some("Hz"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "engine.replay_record_quality",
            kind: TunableKind::StringEnum,
            range: TunableRange::StringEnum(vec!["Lossless", "NearLossless", "Compressed"]),
            default: TunableValue::StringEnum("NearLossless".into()),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "replay-record quality",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "engine.cap_budget_strict",
            kind: TunableKind::Bool,
            range: TunableRange::Bool,
            default: TunableValue::Bool(true),
            budget_mode: BudgetMode::HardReject,
            description: "when true, exceeding cap-budget halts ; when false, warns",
            units: None,
            frame_boundary_defer: true,
        },
        // ─── cohomology (2) ────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "cohomology.persistence_threshold",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(0.05),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "minimum persistence to retain a feature",
            units: None,
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "cohomology.update_interval_frames",
            kind: TunableKind::U32,
            range: TunableRange::U32(1..600),
            default: TunableValue::U32(60),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "frame-interval between cohomology updates",
            units: Some("frames"),
            frame_boundary_defer: true,
        },
        // ─── consent (2) ───────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "consent.audit_egress_buffer_ms",
            kind: TunableKind::U32,
            range: TunableRange::U32(0..1000),
            default: TunableValue::U32(100),
            budget_mode: BudgetMode::HardReject,
            description: "max audit-egress buffer-time before mandatory flush",
            units: Some("ms"),
            frame_boundary_defer: true,
        },
        TunableSpec {
            canonical_name: "consent.sigma_check_strict",
            kind: TunableKind::Bool,
            range: TunableRange::Bool,
            default: TunableValue::Bool(true),
            budget_mode: BudgetMode::HardReject,
            description: "when true, Sigma-check failure halts ; when false, warns",
            units: None,
            frame_boundary_defer: true,
        },
        // ─── replay (1) ────────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "replay.frame_buffer_size",
            kind: TunableKind::U32,
            range: TunableRange::U32(60..36000),
            default: TunableValue::U32(600),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "replay-record frame ring-buffer size (10 sec at 60Hz)",
            units: Some("frames"),
            frame_boundary_defer: true,
        },
        // ─── inspect (1) ───────────────────────────────────────────────────────
        TunableSpec {
            canonical_name: "inspect.capture_max_per_second",
            kind: TunableKind::U32,
            range: TunableRange::U32(1..240),
            default: TunableValue::U32(4),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "inspector::capture_frame rate-cap",
            units: Some("fps"),
            frame_boundary_defer: true,
        },
        // ─── extension (1) ─────────────────────────────────────────────────────
        // Per the T11-D164 landmine-list, mise-en-abyme recursion is a stability
        // surface that the substrate cannot do without. Spec § 4.3 omits it ;
        // we register it with a HardReject budget so the scene-graph cannot
        // overflow its recursion stack.
        TunableSpec {
            canonical_name: "render.mise_en_abyme_recursion_cap",
            kind: TunableKind::U32,
            range: TunableRange::U32(0..32),
            default: TunableValue::U32(4),
            budget_mode: BudgetMode::HardReject,
            description: "max recursive mise-en-abyme reflection depth",
            units: Some("levels"),
            frame_boundary_defer: true,
        },
    ]
}

/// Install the 30 default tunables into `registry`. Returns the ids in
/// declaration order (useful for tests and inspector enumeration).
///
/// Errors :
/// - [`TweakError::AlreadyRegistered`] if any default name collides with an
///   already-registered tunable.
/// - [`TweakError::DefaultOutOfRange`] if a spec author misuses the registry
///   (should never trigger from this function, but the contract is preserved).
pub fn install_defaults(registry: &mut TunableRegistry) -> Result<Vec<TunableId>, TweakError> {
    let specs = default_tunable_specs();
    let mut ids = Vec::with_capacity(specs.len());
    for spec in specs {
        ids.push(registry.register(spec)?);
    }
    Ok(ids)
}

// ─── unit-tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Cap;

    /// Convenience : create a fresh registry pre-loaded with the 30 defaults.
    fn registry_with_defaults() -> TunableRegistry {
        let mut reg = TunableRegistry::new();
        let _ = install_defaults(&mut reg);
        reg
    }

    #[test]
    fn count_is_thirty() {
        assert_eq!(default_tunable_specs().len(), DEFAULT_TUNABLE_COUNT);
        assert_eq!(DEFAULT_TUNABLE_COUNT, 30);
    }

    #[test]
    fn defaults_register_cleanly() {
        let mut reg = TunableRegistry::new();
        let ids = install_defaults(&mut reg).unwrap();
        assert_eq!(ids.len(), 30);
        assert_eq!(reg.len(), 30);
    }

    #[test]
    fn defaults_have_unique_ids() {
        let specs = default_tunable_specs();
        let mut ids: Vec<TunableId> = specs.iter().map(TunableSpec::id).collect();
        ids.sort();
        let count_before = ids.len();
        ids.dedup();
        assert_eq!(
            count_before,
            ids.len(),
            "duplicate canonical_names in defaults"
        );
    }

    #[test]
    fn defaults_have_unique_names() {
        let specs = default_tunable_specs();
        let mut names: Vec<&str> = specs.iter().map(|s| s.canonical_name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }

    #[test]
    fn every_default_in_range() {
        for spec in default_tunable_specs() {
            assert!(
                spec.range.check_in_range(&spec.default).is_ok(),
                "default out of range: {}",
                spec.canonical_name
            );
        }
    }

    #[test]
    fn every_default_kind_matches_range() {
        for spec in default_tunable_specs() {
            assert_eq!(
                spec.kind,
                spec.range.kind(),
                "kind/range mismatch: {}",
                spec.canonical_name
            );
        }
    }

    #[test]
    fn every_default_kind_matches_value() {
        for spec in default_tunable_specs() {
            assert_eq!(
                spec.kind,
                spec.default.kind(),
                "kind/default mismatch: {}",
                spec.canonical_name
            );
        }
    }

    #[test]
    fn registry_with_defaults_is_loaded() {
        let reg = registry_with_defaults();
        assert_eq!(reg.len(), DEFAULT_TUNABLE_COUNT);
    }

    #[test]
    fn safety_critical_are_hard_reject() {
        // From spec § 8 : these must be HardReject.
        let must_be_hard = [
            "render.spectral_bands_active",
            "render.tonemap_curve",
            "physics.collision_eps",
            "audio.master_gain_db",
            "engine.cap_budget_strict",
            "consent.audit_egress_buffer_ms",
            "consent.sigma_check_strict",
        ];
        let specs = default_tunable_specs();
        for name in must_be_hard {
            let spec = specs
                .iter()
                .find(|s| s.canonical_name == name)
                .unwrap_or_else(|| panic!("missing spec: {name}"));
            assert_eq!(
                spec.budget_mode,
                BudgetMode::HardReject,
                "{name} should be HardReject"
            );
        }
    }

    #[test]
    fn cap_token_required_for_set() {
        let mut reg = registry_with_defaults();
        let id = TunableId::of("ai.policy_explore_rate");
        let stage = reg.set(id, TunableValue::F32(0.3), Cap::tweak()).unwrap();
        assert_eq!(stage, crate::tunable::Stage::Pending);
    }

    #[test]
    fn frame_boundary_defer_default() {
        for spec in default_tunable_specs() {
            assert!(
                spec.frame_boundary_defer,
                "frame_boundary_defer should default to true: {}",
                spec.canonical_name
            );
        }
    }

    #[test]
    fn install_defaults_idempotent_failure() {
        let mut reg = TunableRegistry::new();
        install_defaults(&mut reg).unwrap();
        let err = install_defaults(&mut reg).unwrap_err();
        assert!(matches!(err, TweakError::AlreadyRegistered { .. }));
    }
}

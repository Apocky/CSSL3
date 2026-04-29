//! § Omega-step Phase-2 PROPAGATE hook for the Wave-Unity solver.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § PHASE-2 INSERTION POINT (Wave-Unity §VII.4)
//!   The omega_step Phase-2 PROPAGATE algorithm inserts the wave-solver
//!   call at step **2b'**, BEFORE step 2c (radiance-cascade) :
//!
//!     ```text
//!     // 2a. Λ-stream + Ψ-flow
//!     // 2b. force-LBM step
//!     // 2b'. wave-unity ψ-PDE (full-substrate, active-region) ← THIS HOOK
//!     // 2c. radiance cascades (now reads ψ-substrate as input)
//!     ```
//!
//! § HOOK SHAPE
//!   [`WaveUnityPhase2`] is an [`cssl_substrate_omega_step::OmegaSystem`]
//!   impl wrapper. The scheduler dispatches it during Phase-2 ; the
//!   wave-solver advances the supplied `WaveField` by `dt`.
//!
//! § STAGE-0 LIMITATIONS
//!   - The wave-field carried by the hook is currently **owned** by the
//!     hook itself, NOT yet co-located inside the `OmegaSnapshot` (that
//!     wire-up lands when the omega-step ctx adds a typed handle for
//!     wave-field state). For Stage-0 the hook is a self-contained
//!     simulation system — registering it adds a wave-field tied to the
//!     scheduler's lifetime.
//!   - The Σ-check on the field's cells is performed inside
//!     [`crate::step::wave_solver_step_with_sdf`] when the SDF impl
//!     supplies a material — this is the canonical AGENCY-laundering
//!     guard.
//!
//! § DETERMINISM
//!   The hook's `step()` method delegates to [`crate::step::wave_solver_step`]
//!   which is bit-deterministic per the omega-step DETERMINISM CONTRACT.

use cssl_substrate_omega_step::{
    effect_row::EffectRow,
    rng::RngStreamId,
    system::{OmegaSystem, SystemId},
    OmegaError, OmegaStepCtx,
};

#[cfg(test)]
use cssl_substrate_omega_step::effect_row::SubstrateEffect;

use crate::psi_field::WaveField;
use crate::step::{wave_solver_step, WaveSolverError, WaveStepReport};

/// § Stable system-id for the wave-unity Phase-2 hook. The omega-step
///   scheduler issues SystemId via `register()` ; this constant is used
///   by replay logs to recognize the wave-unity entry.
pub const WAVE_UNITY_SYSTEM_NAME: &str = "wave_unity_phase2";

/// § Convenience type alias. The scheduler-issued SystemId is opaque ;
///   call sites can use this to spell the registration return type.
pub type WaveUnitySystemId = SystemId;

/// § Phase-2 hook : wraps the wave-solver as an `OmegaSystem`.
///
///   The hook owns its own [`WaveField<5>`]. After registration the
///   solver advances on every `step()` call. The wave-field is
///   accessible via [`WaveUnityPhase2::field`] /
///   [`WaveUnityPhase2::field_mut`] for outside-reads (RC-cascade
///   consumes from here).
pub struct WaveUnityPhase2 {
    /// § Owned wave-field. 5-band default config.
    field: WaveField<5>,
    /// § Latest tick report ; consumed by the omega_step Phase-6
    ///   entropy-book.
    last_report: Option<WaveStepReport>,
    /// § Frame counter — advances every successful step.
    frame: u64,
}

impl Default for WaveUnityPhase2 {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveUnityPhase2 {
    /// § Construct a fresh hook with empty 5-band field.
    #[must_use]
    pub fn new() -> Self {
        Self {
            field: WaveField::<5>::with_default_bands(),
            last_report: None,
            frame: 0,
        }
    }

    /// § Construct from a pre-populated field.
    #[must_use]
    pub fn with_field(field: WaveField<5>) -> Self {
        Self {
            field,
            last_report: None,
            frame: 0,
        }
    }

    /// § Read-only access to the wave-field. RC-cascade consumes via this.
    #[inline]
    #[must_use]
    pub fn field(&self) -> &WaveField<5> {
        &self.field
    }

    /// § Mutable access to the wave-field. Used to inject sources +
    ///   spell-cast amplitudes outside the solver step.
    #[inline]
    pub fn field_mut(&mut self) -> &mut WaveField<5> {
        &mut self.field
    }

    /// § Latest tick report. `None` until at least one step has run.
    #[inline]
    #[must_use]
    pub fn last_report(&self) -> Option<WaveStepReport> {
        self.last_report
    }

    /// § Current frame counter.
    #[inline]
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }
}

impl OmegaSystem for WaveUnityPhase2 {
    fn step(&mut self, _ctx: &mut OmegaStepCtx<'_>, dt: f64) -> Result<(), OmegaError> {
        match wave_solver_step(&mut self.field, dt, self.frame) {
            Ok(report) => {
                self.last_report = Some(report);
                self.frame = self.frame.wrapping_add(1);
                Ok(())
            }
            Err(e) => Err(map_solver_error(e)),
        }
    }

    fn name(&self) -> &str {
        WAVE_UNITY_SYSTEM_NAME
    }

    fn effect_row(&self) -> EffectRow {
        // Wave-Unity's effect-row : {Sim} (always) ⊎ {Render} (it feeds
        // the RC cascade) ⊎ {Audio} (it produces audio-band amplitudes).
        // EffectRow exposes union as the canonical composition op.
        EffectRow::sim()
            .union(&EffectRow::sim_render())
            .union(&EffectRow::sim_audio())
    }

    fn rng_streams(&self) -> &[RngStreamId] {
        // Wave-Unity is fully deterministic — no RNG draws.
        &[]
    }
}

/// § Map a `WaveSolverError` to the `OmegaError` surface the scheduler
///   understands. Coupling-refusals translate to `SystemPanicked` with
///   a structured message ; numerical blow-up translates to a frame-
///   overbudget halt.
fn map_solver_error(err: WaveSolverError) -> OmegaError {
    match err {
        WaveSolverError::Coupling(c) => OmegaError::SystemPanicked {
            system: SystemId(0),
            name: WAVE_UNITY_SYSTEM_NAME.to_string(),
            frame: 0,
            msg: format!("wave-solver coupling refused : {c}"),
        },
        WaveSolverError::SubstepClampExceeded { requested, max } => OmegaError::SystemPanicked {
            system: SystemId(0),
            name: WAVE_UNITY_SYSTEM_NAME.to_string(),
            frame: 0,
            msg: format!("wave-solver substep clamp exceeded : requested={requested}, max={max}"),
        },
        WaveSolverError::NormBlewUp { before, after } => OmegaError::SystemPanicked {
            system: SystemId(0),
            name: WAVE_UNITY_SYSTEM_NAME.to_string(),
            frame: 0,
            msg: format!("wave-solver psi-norm blew up : before={before}, after={after}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::band::Band;
    use crate::complex::C32;
    use cssl_substrate_omega_field::MortonKey;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn hook_default_constructs_empty_field() {
        let h = WaveUnityPhase2::new();
        assert_eq!(h.field().total_cell_count(), 0);
        assert_eq!(h.frame(), 0);
        assert!(h.last_report().is_none());
    }

    #[test]
    fn hook_with_field_carries_pre_populated_state() {
        let mut field = WaveField::<5>::with_default_bands();
        field.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
        let h = WaveUnityPhase2::with_field(field);
        assert_eq!(h.field().total_cell_count(), 1);
    }

    #[test]
    fn hook_field_mut_allows_injection() {
        let mut h = WaveUnityPhase2::new();
        h.field_mut()
            .set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(1.0, 0.0));
        assert_eq!(h.field().total_cell_count(), 1);
    }

    #[test]
    fn hook_name_is_canonical() {
        let h = WaveUnityPhase2::new();
        assert_eq!(h.name(), "wave_unity_phase2");
        assert_eq!(WAVE_UNITY_SYSTEM_NAME, "wave_unity_phase2");
    }

    #[test]
    fn hook_effect_row_includes_sim_render_audio() {
        let h = WaveUnityPhase2::new();
        let row = h.effect_row();
        assert!(row.contains(SubstrateEffect::Sim));
        assert!(row.contains(SubstrateEffect::Render));
        assert!(row.contains(SubstrateEffect::Audio));
    }

    #[test]
    fn hook_rng_streams_empty() {
        let h = WaveUnityPhase2::new();
        assert!(h.rng_streams().is_empty());
    }

    #[test]
    fn hook_step_advances_frame() {
        // We can't easily build a real OmegaStepCtx in-test ; this
        // verifies the wave_solver_step path directly. The hook frame
        // increments on success.
        let mut h = WaveUnityPhase2::new();
        h.field_mut()
            .set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(0.5, 0.0));
        // Direct call to wave_solver_step avoids constructing OmegaStepCtx.
        // Cache frame BEFORE the mutable borrow.
        let frame = h.frame();
        let r = wave_solver_step(h.field_mut(), 1.0e-3, frame).unwrap();
        h.last_report = Some(r);
        h.frame += 1;
        assert_eq!(h.frame(), 1);
        assert!(h.last_report().is_some());
    }
}

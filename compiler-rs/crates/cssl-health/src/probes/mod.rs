//! § T11-D159 : `HealthProbe` trait + 12 mock subsystem-impls
//!
//! Each `probes::<subsystem>` module exposes a `MockProbe` newtype that
//! holds a tiny `MockState` (frame-counter + a few toggles) and returns
//! a deterministic [`crate::HealthStatus`] from `health()`. The
//! orientation is **deterministic** + **trivially-fast** (≤ 1 µs) so
//! integration-tests can build huge `HealthRegistry` graphs without
//! breaking the `≤ 100µs / probe` spec-budget.
//!
//! Real-integration slice (Wave-Jθ-4) replaces these with the actual
//! subsystem-crate impls.

use thiserror::Error;

use crate::status::HealthStatus;

pub mod anim_procedural;
pub mod fractal_amp;
pub mod gaze_collapse;
pub mod host_openxr;
pub mod physics_wave;
pub mod render_companion_perspective;
pub mod render_v2;
pub mod spectral_render;
pub mod substrate_kan;
pub mod substrate_omega_field;
pub mod wave_audio;
pub mod wave_solver;

/// Errors raised by [`HealthProbe::degrade`].
///
/// `health()` itself is **infallible** — a probe that cannot read its
/// state must return [`HealthStatus::Failed`] with kind `Unknown` or
/// `InvariantBreach` rather than propagating an error.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HealthError {
    /// Probe refused to self-degrade because its safety-policy says so
    /// (e.g. a security-critical subsystem that may not run in a
    /// reduced-fidelity mode).
    #[error("subsystem `{0}` refuses self-degradation : safety-policy")]
    DegradeRefused(&'static str),
    /// Probe is already-Failed ; can't degrade further.
    #[error("subsystem `{0}` is already-Failed ; degrade is a no-op")]
    AlreadyFailed(&'static str),
    /// Probe-internal error ; opaque to caller.
    #[error("subsystem `{0}` internal-error : {1}")]
    Internal(&'static str, &'static str),
}

/// Per-subsystem health-probe contract.
///
/// Per `_drafts/phase_j/06_l2_telemetry_spec.md` § V.4 :
///
/// - `name()` is a `&'static str` matching the **crate-name** (e.g.
///   `"cssl-render-v2"`).
/// - `health()` is `{ Telemetry<Counters>, Pure }` and **infallible**.
/// - `degrade()` is `{ Telemetry<Counters>, Audit<"subsystem-degrade"> }`
///   and may refuse via [`HealthError::DegradeRefused`].
///
/// Effect-row enforcement lives in `cssl-effects` ; this trait is the
/// runtime-side contract.
pub trait HealthProbe: Send + Sync {
    /// Crate-name (matches the workspace `[package].name`).
    fn name(&self) -> &'static str;

    /// Snapshot health right-now. Spec-budget : ≤ 100µs.
    fn health(&self) -> HealthStatus;

    /// Request the subsystem to drop into degraded-mode. Default impl
    /// is a no-op-success ; subsystems with a real degrade-path
    /// override.
    ///
    /// # Errors
    /// Returns [`HealthError::DegradeRefused`] if the subsystem's
    /// safety-policy prohibits degraded-mode operation.
    fn degrade(&self, _reason: &str) -> Result<(), HealthError> {
        Ok(())
    }
}

/// Build a fresh `Vec<Box<dyn HealthProbe>>` containing one probe-per-
/// subsystem in the canonical [`crate::SUBSYSTEMS`] order. Used by
/// the integration-tests + as a reference for orchestrator-side wiring.
#[must_use]
pub fn register_all_mock() -> Vec<Box<dyn HealthProbe>> {
    vec![
        Box::new(render_v2::MockProbe::new()),
        Box::new(physics_wave::MockProbe::new()),
        Box::new(wave_solver::MockProbe::new()),
        Box::new(spectral_render::MockProbe::new()),
        Box::new(fractal_amp::MockProbe::new()),
        Box::new(gaze_collapse::MockProbe::new()),
        Box::new(render_companion_perspective::MockProbe::new()),
        Box::new(host_openxr::MockProbe::new()),
        Box::new(anim_procedural::MockProbe::new()),
        Box::new(wave_audio::MockProbe::new()),
        Box::new(substrate_omega_field::MockProbe::new()),
        Box::new(substrate_kan::MockProbe::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SUBSYSTEMS;

    #[test]
    fn register_all_mock_yields_twelve_probes() {
        let probes = register_all_mock();
        assert_eq!(probes.len(), 12);
        let names: Vec<&str> = probes.iter().map(|p| p.name()).collect();
        assert_eq!(names.as_slice(), &SUBSYSTEMS[..]);
    }

    #[test]
    fn default_degrade_is_ok() {
        let p = render_v2::MockProbe::new();
        assert_eq!(p.degrade("test"), Ok(()));
    }
}

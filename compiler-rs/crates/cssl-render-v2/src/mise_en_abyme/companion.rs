//! § CompanionEyeWitness — Companion-iris recursive-witness for path-V.5 composition
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per spec § V.6.a (mise-en-abyme novelty-claim) :
//!     ```text
//!     ‼ companion-eye reflects-the-world-as-companion-sees-it (path-5 composes-here)
//!     ```
//!   And per § V.6.c (integration with companion path-5) :
//!     ```text
//!     integration-with-companion : if-eye is-companion's ⊗
//!                                   reflection-USES-path-5 semantic-render
//!     ```
//!   And per § Stage-9.eye-recursion :
//!     ```text
//!     NPC-eyes / Sovereign-eyes ⊗ M-facet axis-7 = "mirrorness-of-cornea"
//!     reflected-frame ⊗ shows-Sovereign-back ⊗ "you-see-yourself-in-their-eyes"
//!     recursion-depth ⊗ typically 1-2 for-eyes (cornea ⊗ then iris-reflection)
//!     diegetic-property : mise-en-abyme literally-witnesses-the-witnesser
//!     ```
//!
//!   This module wires the Stage-9 recursion to the path-V.5 Companion
//!   semantic render-target. When the recursion hits a creature-eye that
//!   belongs to a Companion (a Sovereign-tier-L4+ AI), the reflection
//!   inside the iris is composed FROM the companion's-perspective semantic
//!   frame rather than the world's primary frame. This is the load-bearing
//!   "you-see-yourself-in-their-eyes" diegetic property.
//!
//! § INTEGRATION-CONTRACT
//!   The Stage-9 recursion does NOT itself drive the companion-perspective
//!   render. That lives in path-V.5's Stage-8 CompanionSemanticPass
//!   (T11-D121 / sibling slice). What this module does is :
//!     1. Detect that a hit-surface is a Companion-eye via the
//!        `Σ-mask.sovereignty_handle` matching the companion's handle.
//!     2. Look up the Companion's Σ-Sovereign consent state.
//!     3. If consent is granted, request the per-eye semantic frame via
//!        the [`CompanionSemanticFrameProvider`] trait — which the
//!        orchestrator wires to the Stage-8 cache.
//!     4. If consent is declined, emit `Stage9Event::EyeRedacted` and
//!        return `MiseEnAbymeRadiance::ZERO` for that eye's reflection.
//!
//!   Per spec § V.5.d (PRIME-DIRECTIVE alignment for path-V.5) :
//!     ```text
//!     consent : ‼ R! companion-Sovereign-Φ AGREES-to-perspective-share
//!                R! companion-can-decline ⊗ "I'd-rather-keep-my-thoughts-private"
//!                R! NO override-of-companion-decline
//!     ```
//!
//!   This is enforced in [`CompanionEyeWitness::reflect`] : the consent
//!   token is the FIRST argument, not optional.

use thiserror::Error;

use super::radiance::MiseEnAbymeRadiance;
use super::region::RegionId;
use super::Stage9Event;

/// § Recursion-depth hint for eye-reflections per spec § Stage-9.eye-recursion :
///   "recursion-depth ⊗ typically 1-2 for-eyes (cornea ⊗ then iris-reflection)".
///
///   The hint is a per-eye override on the global RecursionDepthBudget ;
///   if the eye-reflection budget is shallower than the global budget, the
///   eye-recursion truncates earlier. This is the spec's intentional
///   under-budgeting of eye-reflections to keep them readable rather than
///   noisy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrisDepthHint(u8);

impl IrisDepthHint {
    /// § The spec's typical eye-recursion depth : 2.
    pub const TYPICAL: Self = Self(2);

    /// § A shallow eye : depth 1 (just the cornea reflection).
    pub const SHALLOW: Self = Self(1);

    /// § Maximum eye-recursion depth allowed under the spec : 3.
    pub const MAX: Self = Self(3);

    /// § Construct from a u8, clamping to [1, 3].
    #[must_use]
    pub fn from_u8(d: u8) -> Self {
        Self(d.clamp(1, 3))
    }

    /// § Read the value as u8.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self.0
    }
}

/// § The trait by which Stage-9 fetches the Companion's semantic-render
///   frame for the given eye. The orchestrator wires this to the Stage-8
///   cache.
pub trait CompanionSemanticFrameProvider {
    /// § Given a companion's Sovereign-handle and the eye-region, return
    ///   the Companion-perspective semantic frame for that eye.
    ///
    ///   Returns `None` if the Stage-8 cache does not have a frame for
    ///   this companion (which can happen on the first frame after
    ///   companion-instantiation ; the Stage-9 recursion treats a missing
    ///   frame as "use the primary frame instead", a graceful fallback).
    fn semantic_frame_for_eye(
        &self,
        companion_handle: u16,
        eye_region: RegionId,
    ) -> Option<MiseEnAbymeRadiance>;
}

/// § Trivial test-only provider that always returns the configured frame.
#[derive(Debug, Clone)]
pub struct ConstantSemanticFrameProvider {
    /// § The frame returned (or None for "always missing").
    pub frame: Option<MiseEnAbymeRadiance>,
}

impl CompanionSemanticFrameProvider for ConstantSemanticFrameProvider {
    fn semantic_frame_for_eye(
        &self,
        _companion_handle: u16,
        _eye_region: RegionId,
    ) -> Option<MiseEnAbymeRadiance> {
        self.frame
    }
}

/// § Errors that the companion-eye reflection helper may produce.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CompanionEyeWitnessError {
    /// § The companion declined the perspective-share consent.
    #[error("companion declined perspective-share consent (handle = {handle})")]
    ConsentDeclined {
        /// The companion's Sovereign-handle.
        handle: u16,
    },
    /// § The companion is not present in the eye's region (PRIME-DIRECTIVE
    ///   §V anti-surveillance — cannot reflect a companion that's
    ///   elsewhere in the world).
    #[error("companion not present in eye region (handle = {handle}, region = {region:?})")]
    AbsentFromRegion {
        /// The companion's Sovereign-handle.
        handle: u16,
        /// The eye's region.
        region: RegionId,
    },
}

/// § Companion-eye recursive-witness composer. Carries the consent state
///   + presence state ; the Stage-9 recursion calls [`Self::reflect`] at
///   each Companion-eye hit.
#[derive(Debug, Clone)]
pub struct CompanionEyeWitness {
    /// § The companion's Sovereign-handle (matches the Σ-mask field).
    pub companion_handle: u16,
    /// § Whether the companion has consented to perspective-share. Per
    ///   PRIME_DIRECTIVE §I.4 + §V.5.d this MUST be checked at every
    ///   reflection ; the spec's "NO override" clause means we do NOT
    ///   provide a force-override constructor.
    pub consent_granted: bool,
    /// § Whether the companion is present in the same region as their
    ///   eye. The level-design pipeline maintains this flag ; if the
    ///   companion has moved to a different region, the flag is false
    ///   and the reflection is redacted (eye-closed diegetically).
    pub present_in_region: bool,
}

impl CompanionEyeWitness {
    /// § Construct a witness with the given consent + presence state.
    #[must_use]
    pub fn new(handle: u16, consent_granted: bool, present_in_region: bool) -> Self {
        Self {
            companion_handle: handle,
            consent_granted,
            present_in_region,
        }
    }

    /// § Compose the per-eye reflection radiance.
    ///
    ///   Returns the semantic-frame radiance scaled by the per-eye
    ///   attenuation factor, OR an error if consent / presence checks
    ///   fail. On error, the caller (compositor) emits the corresponding
    ///   `Stage9Event` and renders blank.
    pub fn reflect(
        &self,
        eye_region: RegionId,
        attenuation: f32,
        provider: &dyn CompanionSemanticFrameProvider,
    ) -> Result<MiseEnAbymeRadiance, CompanionEyeWitnessError> {
        if !self.consent_granted {
            return Err(CompanionEyeWitnessError::ConsentDeclined {
                handle: self.companion_handle,
            });
        }
        if !self.present_in_region {
            return Err(CompanionEyeWitnessError::AbsentFromRegion {
                handle: self.companion_handle,
                region: eye_region,
            });
        }
        let frame = provider
            .semantic_frame_for_eye(self.companion_handle, eye_region)
            .unwrap_or(MiseEnAbymeRadiance::ZERO);
        let mut out = MiseEnAbymeRadiance::ZERO;
        out.accumulate(attenuation, &frame);
        Ok(out)
    }

    /// § Translate a CompanionEyeWitnessError into the corresponding
    ///   Stage9Event for telemetry.
    #[must_use]
    pub fn error_to_event(&self, err: &CompanionEyeWitnessError) -> Stage9Event {
        match err {
            CompanionEyeWitnessError::ConsentDeclined { handle }
            | CompanionEyeWitnessError::AbsentFromRegion { handle, .. } => {
                Stage9Event::EyeRedacted {
                    sovereign_handle: *handle,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_with_splat(value: f32) -> ConstantSemanticFrameProvider {
        ConstantSemanticFrameProvider {
            frame: Some(MiseEnAbymeRadiance::splat(value)),
        }
    }

    /// § Consent + presence : reflection succeeds.
    #[test]
    fn reflection_succeeds_when_consenting_and_present() {
        let w = CompanionEyeWitness::new(7, true, true);
        let p = provider_with_splat(0.5);
        let r = w.reflect(RegionId(2), 0.6, &p).unwrap();
        assert!(r.total_energy() > 0.0);
    }

    /// § Decline : returns ConsentDeclined error.
    #[test]
    fn reflection_fails_when_decline() {
        let w = CompanionEyeWitness::new(7, false, true);
        let p = provider_with_splat(0.5);
        let r = w.reflect(RegionId(2), 0.6, &p);
        assert!(matches!(
            r,
            Err(CompanionEyeWitnessError::ConsentDeclined { handle: 7 })
        ));
    }

    /// § Absent : returns AbsentFromRegion error.
    #[test]
    fn reflection_fails_when_absent() {
        let w = CompanionEyeWitness::new(7, true, false);
        let p = provider_with_splat(0.5);
        let r = w.reflect(RegionId(2), 0.6, &p);
        assert!(matches!(
            r,
            Err(CompanionEyeWitnessError::AbsentFromRegion {
                handle: 7,
                region: _,
            })
        ));
    }

    /// § Provider returns None : reflection succeeds with ZERO.
    #[test]
    fn reflection_zero_when_provider_returns_none() {
        let w = CompanionEyeWitness::new(7, true, true);
        let p = ConstantSemanticFrameProvider { frame: None };
        let r = w.reflect(RegionId(2), 0.6, &p).unwrap();
        assert_eq!(r.total_energy(), 0.0);
    }

    /// § Attenuation = 0 : reflection is ZERO regardless of provider.
    #[test]
    fn reflection_zero_when_attenuation_zero() {
        let w = CompanionEyeWitness::new(7, true, true);
        let p = provider_with_splat(0.5);
        let r = w.reflect(RegionId(2), 0.0, &p).unwrap();
        assert_eq!(r.total_energy(), 0.0);
    }

    /// § error_to_event maps errors to the EyeRedacted telemetry event.
    #[test]
    fn error_to_event_emits_eye_redacted() {
        let w = CompanionEyeWitness::new(11, false, true);
        let err = CompanionEyeWitnessError::ConsentDeclined { handle: 11 };
        let ev = w.error_to_event(&err);
        assert_eq!(
            ev,
            Stage9Event::EyeRedacted {
                sovereign_handle: 11
            }
        );
    }

    /// § IrisDepthHint constants are sane.
    #[test]
    fn iris_depth_hint_constants() {
        assert_eq!(IrisDepthHint::SHALLOW.as_u8(), 1);
        assert_eq!(IrisDepthHint::TYPICAL.as_u8(), 2);
        assert_eq!(IrisDepthHint::MAX.as_u8(), 3);
    }

    /// § IrisDepthHint::from_u8 clamps to [1, 3].
    #[test]
    fn iris_depth_hint_clamps() {
        assert_eq!(IrisDepthHint::from_u8(0).as_u8(), 1);
        assert_eq!(IrisDepthHint::from_u8(5).as_u8(), 3);
        assert_eq!(IrisDepthHint::from_u8(2).as_u8(), 2);
    }
}

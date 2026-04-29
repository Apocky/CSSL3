//! § mise_en_abyme — Stage-9 of the canonical 12-stage render pipeline
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Recursive-witness rendering for mirrors, reflective creature-eyes, and
//!   still-water surfaces. The image contains the image, recursively, with
//!   KAN-confidence-attenuation driving early-termination when "no-more-
//!   information-here" — bounded by a HARD `RECURSION_DEPTH_HARD_CAP = 5`.
//!
//! § SPEC ANCHORS (verbatim)
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.6` — path-V.6
//!     of the SIX immutable novelty paths.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-9` —
//!     pipeline-position, budget (≤ 0.8ms @ Quest-3, ≤ 0.6ms @ Vision-Pro),
//!     bounded-recursion, effect-row.
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` — bounded-recursion
//!     as a direct AGENCY-INVARIANT corollary.
//!
//! § MODULES
//!   - [`pass`]        — `MiseEnAbymePass` top-level + `RecursionDepthBudget`
//!   - [`compositor`]  — `WitnessCompositor` per-frame attenuated compose
//!   - [`mirror`]      — `MirrorSurface` SDF + KanMaterial detector
//!   - [`confidence`]  — `KanConfidence` KAN-attenuation evaluator
//!   - [`radiance`]    — `MiseEnAbymeRadiance` per-eye 16-band buffer
//!   - [`companion`]   — `CompanionEyeWitness` Companion-iris path-5 link
//!   - [`region`]      — `RegionBoundary` anti-surveillance gate
//!   - [`probe`]       — `MirrorRaymarchProbe` Stage-5-replay shim
//!   - [`cost`]        — `MiseEnAbymeCostModel` budget-gate

pub mod companion;
pub mod compositor;
pub mod confidence;
pub mod cost;
pub mod mirror;
pub mod pass;
pub mod probe;
pub mod radiance;
pub mod region;

use thiserror::Error;

pub use companion::{CompanionEyeWitness, CompanionEyeWitnessError, IrisDepthHint};
pub use compositor::{WitnessCompositor, WitnessCompositorStats};
pub use confidence::{KanConfidence, KanConfidenceInputs, KanConfidenceOutputs, MIN_CONFIDENCE};
pub use cost::{MiseEnAbymeCostModel, RuntimePlatform};
pub use mirror::{MirrorDetectionThreshold, MirrorSurface, MirrornessChannel};
pub use pass::{MiseEnAbymePass, MiseEnAbymePassConfig, RecursionDepthBudget};
pub use probe::{ConstantProbe, MirrorRaymarchProbe, ProbeResult};
pub use radiance::{MiseEnAbymeRadiance, BANDS_PER_EYE, EYES_PER_FRAME};
pub use region::{RegionBoundary, RegionId, RegionPolicy};

// ─────────────────────────────────────────────────────────────────────────
// § Public constants — load-bearing across the spec-acceptance gate.
// ─────────────────────────────────────────────────────────────────────────

/// § HARD cap on recursion depth. Per spec § Stage-9 :
///   `recursion-depth ≤ RecursionDepthMax ⊗ ALWAYS-bounded` and the dispatch
///   ticket explicitly demands `HARD cap on depth = 5`. This is a `const` so
///   the runtime cannot exceed it under any circumstance.
pub const RECURSION_DEPTH_HARD_CAP: u8 = 5;

/// § Spec § Stage-9.budget : 0.8ms @ Quest-3.
pub const STAGE9_BUDGET_QUEST3_US: u32 = 800;

/// § Spec § Stage-9.budget : 0.6ms @ Vision-Pro.
pub const STAGE9_BUDGET_VISION_PRO_US: u32 = 600;

// ─────────────────────────────────────────────────────────────────────────
// § Errors — Stage-9 fault surface.
// ─────────────────────────────────────────────────────────────────────────

/// § Stage-9 fault surface. Each variant carries enough context for the
///   compositor to either mask the affected pixel or fall back to the base
///   amplified-radiance from Stage-7.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum Stage9Error {
    /// § The recursion budget was exhausted at the soft layer (KAN-confidence
    ///   said `continue=true` past the hard cap). This is a soft failure :
    ///   the recursion stops, the partial accumulated radiance is preserved,
    ///   and the compositor flags the affected ray for telemetry.
    #[error("recursion depth exhausted at hard cap (depth = {depth}, hard cap = {hard_cap})")]
    RecursionDepthExhausted {
        /// The depth at which truncation occurred (always `hard_cap`).
        depth: u8,
        /// The compile-time hard cap.
        hard_cap: u8,
    },

    /// § The frame's wall-clock budget was exceeded. This is reported by
    ///   the cost-model gate ; the compositor may choose to skip remaining
    ///   recursive bounces this frame and emit base-radiance for any pixel
    ///   that has not yet finished.
    #[error("Stage-9 budget exceeded : {used_us}us > {budget_us}us")]
    BudgetExceeded {
        /// Microseconds consumed at the time of detection.
        used_us: u32,
        /// The configured budget for the platform.
        budget_us: u32,
    },

    /// § A Σ-Sovereign violation : the recursion attempted to reflect a
    ///   Σ-private region into a public region, OR a creature-eye reflection
    ///   was requested for a Sovereign that is not present in the eye's
    ///   region. This is the load-bearing PRIME_DIRECTIVE §I.4 + §V check.
    ///   The recursion truncates ; the affected creature-eye renders blank
    ///   ("the eye is closed" diegetically — see compositor's
    ///   `EyeRedacted` event).
    #[error(
        "Σ-Sovereign violation : reflection of region={source_region:?} into \
         region={target_region:?} forbidden by Σ-mask"
    )]
    SovereigntyViolation {
        /// The region that owns the surface being reflected.
        source_region: RegionId,
        /// The region that would receive the reflection.
        target_region: RegionId,
    },

    /// § The mirror surface's tangent-plane could not be derived (degenerate
    ///   normal). This is rare but possible when the SDF gradient is near-
    ///   zero ; we treat it as "no reflection here" and continue.
    #[error("degenerate mirror tangent-plane : SDF gradient magnitude = {gradient_magnitude}")]
    DegenerateTangentPlane {
        /// The magnitude of the SDF gradient at the surface point.
        gradient_magnitude: f32,
    },

    /// § The probe returned no hit (the reflected ray escaped the world).
    ///   This is not really an error ; the compositor treats it as
    ///   "atmospheric loss" and uses the configured atmosphere-sky color.
    #[error("reflected ray escaped world bounds")]
    ProbeMiss,
}

/// § Telemetry events emitted by Stage-9 during recursion. Per
///   `06_RENDERING_PIPELINE § Stage-9.compute step-3` :
///   `terminate-witness emitted-to-telemetry per-frame`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage9Event {
    /// § A recursive bounce terminated normally because the KAN-confidence
    ///   reported `continue = false` at the given depth.
    KanConfidenceTerminate {
        /// Depth at termination.
        depth: u8,
        /// Reported confidence at termination.
        confidence: f32,
    },
    /// § A recursive bounce terminated at the hard cap. This is emitted once
    ///   per pixel that hits the cap, and the orchestrator may use the count
    ///   to detect mirror-corridor scenes that need depth-budget tuning.
    HardCapTerminate {
        /// Always equal to [`RECURSION_DEPTH_HARD_CAP`].
        depth: u8,
    },
    /// § A creature-eye reflection was redacted because the Sovereign is not
    ///   present in the eye's region (see PRIME_DIRECTIVE §I.4).
    EyeRedacted {
        /// The Σ-handle of the absent Sovereign.
        sovereign_handle: u16,
    },
    /// § A cross-region surveillance attempt was blocked.
    SurveillanceBlocked {
        /// Source region.
        source_region: RegionId,
        /// Target region.
        target_region: RegionId,
    },
    /// § A frame's per-bounce statistics rollup (compositor emits this once
    ///   per frame after all rays are composed).
    FrameStats {
        /// Total bounces this frame.
        bounces: u32,
        /// Pixels that hit the hard cap.
        hard_cap_pixels: u32,
        /// Pixels that terminated early via KAN-confidence.
        kan_terminate_pixels: u32,
        /// Eye-redactions this frame.
        eye_redactions: u32,
        /// Surveillance-blocks this frame.
        surveillance_blocks: u32,
    },
}

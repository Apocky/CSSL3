//! § MiseEnAbymePass — top-level Stage-9 pass + RecursionDepthBudget
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The Stage-9 entry-point. Takes the per-frame inputs (mirrors detected
//!   in the primary frame, base radiance from Stage-7, KAN-confidence
//!   evaluator, region-boundary policy, optional Companion-eye witness)
//!   and produces the per-eye `MiseEnAbymeRadiance<2, 16>` output.
//!
//!   The pass implementation is HOST-side reference Rust ; the real-runtime
//!   GPU dispatch happens via the cssl-cgen-gpu-spirv backend (sibling
//!   slice T11-D125 wires that). The Rust impl serves as :
//!     - the executable spec for the GPU shader to match
//!     - the integration-test fixture for the rest of the pipeline
//!     - the runtime fallback for non-GPU hosts (Linux desktop XR
//!       without a GPU, the headless test harness, etc.)
//!
//! § BOUNDED RECURSION
//!   Per spec § Stage-9.recursion-discipline + the dispatch ticket :
//!   `HARD cap on depth = 5`. This is enforced via the [`RecursionDepthBudget`]
//!   struct which wraps a u8 + saturates on increment. The hard cap is
//!   `super::RECURSION_DEPTH_HARD_CAP` and cannot be raised at runtime.

use smallvec::SmallVec;

use super::companion::{CompanionEyeWitness, CompanionSemanticFrameProvider};
use super::compositor::{WitnessCompositor, WitnessCompositorStats};
use super::confidence::{KanConfidence, KanConfidenceInputs};
use super::mirror::{MirrorDetectionThreshold, MirrorSurface, MirrornessChannel};
use super::probe::{MirrorRaymarchProbe, ProbeResult};
use super::radiance::MiseEnAbymeRadiance;
use super::region::{RegionBoundary, RegionId};
use super::{Stage9Error, Stage9Event, RECURSION_DEPTH_HARD_CAP};

use cssl_substrate_projections::vec::Vec3;

/// § Bounded recursion-depth tracker. Wraps a u8 and refuses to advance
///   past `RECURSION_DEPTH_HARD_CAP`. The contract :
///
///   - `current()` : the current depth (0 at primary surface, 1 at first
///     mirror bounce, etc.)
///   - `try_advance()` : returns `Ok(())` if the new depth is ≤ HARD_CAP,
///     `Err(Stage9Error::RecursionDepthExhausted)` otherwise.
///
///   The HARD cap is `const`, NOT runtime-configurable. This is the
///   bounded-recursion AGENCY-INVARIANT enforcer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursionDepthBudget {
    /// § Current depth. Always `<= RECURSION_DEPTH_HARD_CAP`.
    current: u8,
}

impl Default for RecursionDepthBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl RecursionDepthBudget {
    /// § Construct at depth 0 (primary surface).
    #[must_use]
    pub const fn new() -> Self {
        Self { current: 0 }
    }

    /// § Construct at the given starting depth, clamped to HARD_CAP.
    #[must_use]
    pub fn at(d: u8) -> Self {
        Self {
            current: d.min(RECURSION_DEPTH_HARD_CAP),
        }
    }

    /// § Read the current depth.
    #[must_use]
    pub const fn current(self) -> u8 {
        self.current
    }

    /// § Predicate : true iff one more bounce is permitted.
    #[must_use]
    pub const fn can_advance(self) -> bool {
        self.current < RECURSION_DEPTH_HARD_CAP
    }

    /// § Try to advance the depth by one. Returns the new budget on
    ///   success ; on hard-cap-exhaustion returns the
    ///   `RecursionDepthExhausted` error so the caller can emit the
    ///   `HardCapTerminate` telemetry event.
    pub fn try_advance(self) -> Result<Self, Stage9Error> {
        if self.current < RECURSION_DEPTH_HARD_CAP {
            Ok(Self {
                current: self.current + 1,
            })
        } else {
            Err(Stage9Error::RecursionDepthExhausted {
                depth: self.current,
                hard_cap: RECURSION_DEPTH_HARD_CAP,
            })
        }
    }

    /// § Distance to the hard cap (how many more bounces are permitted).
    #[must_use]
    pub const fn remaining(self) -> u8 {
        RECURSION_DEPTH_HARD_CAP - self.current
    }
}

/// § Configuration for [`MiseEnAbymePass`]. Exposed as a struct so the
///   orchestrator can tune parameters without re-creating the pass.
#[derive(Clone)]
pub struct MiseEnAbymePassConfig {
    /// § The mirror-detection threshold tuple.
    pub threshold: MirrorDetectionThreshold,
    /// § Which channel of `KanMaterial` carries the mirrorness scalar.
    pub mirrorness_channel: MirrornessChannel,
    /// § The KAN-confidence evaluator.
    pub confidence: KanConfidence,
    /// § The region-boundary policy.
    pub region_boundary: RegionBoundary,
    /// § Maximum recursion depth, bounded above by `RECURSION_DEPTH_HARD_CAP`.
    pub max_depth: u8,
}

impl Default for MiseEnAbymePassConfig {
    fn default() -> Self {
        Self {
            threshold: MirrorDetectionThreshold::default(),
            mirrorness_channel: MirrornessChannel::RoughnessMetallic13_14,
            confidence: KanConfidence::default(),
            region_boundary: RegionBoundary::default(),
            max_depth: RECURSION_DEPTH_HARD_CAP,
        }
    }
}

/// § The Stage-9 pass implementation. Stateless across frames except for
///   the carried `WitnessCompositor` (which holds a per-frame stats counter).
///
///   Per spec § Stage-9.compute, the entry-point flow is :
///     1. detect mirror surfaces (caller-supplied, this pass consumes them)
///     2. for each mirror, recurse via `recurse_at_mirror(...)`
///     3. accumulate into MiseEnAbymeRadiance via WitnessCompositor
///     4. emit telemetry events
pub struct MiseEnAbymePass {
    /// § Per-frame configuration.
    pub config: MiseEnAbymePassConfig,
    /// § Compositor state — per-frame stats + telemetry buffer.
    pub compositor: WitnessCompositor,
}

impl MiseEnAbymePass {
    /// § Construct with the given configuration.
    #[must_use]
    pub fn new(config: MiseEnAbymePassConfig) -> Self {
        Self {
            config,
            compositor: WitnessCompositor::new(),
        }
    }

    /// § Construct with default substrate-canonical configuration.
    #[must_use]
    pub fn substrate_default() -> Self {
        Self::new(MiseEnAbymePassConfig::default())
    }

    /// § Begin a new frame. Resets the per-frame stats + region-block
    ///   counter. Idempotent.
    pub fn begin_frame(&mut self) {
        self.compositor.begin_frame();
        self.config.region_boundary.reset_blocks();
    }

    /// § Recursively render the witness from the given mirror surface.
    ///
    ///   Inputs :
    ///     - `mirror` : the detected mirror surface
    ///     - `view_origin` : the camera position from which the mirror
    ///       was originally hit
    ///     - `view_dir` : the incoming view direction at the mirror
    ///     - `base_radiance` : the radiance from Stage-7 (used as the
    ///       atmospheric-fallback when the recursion misses)
    ///     - `probe` : the SDF raymarch probe (Stage-5 replay)
    ///     - `companion_eye` : optional Companion-eye witness for
    ///       path-V.5 composition. If `None`, the recursion just bounces
    ///       light naturally ; if `Some`, eye-hits are routed through the
    ///       Companion's semantic frame.
    ///     - `companion_provider` : the Companion-perspective frame source
    ///   Returns the accumulated `MiseEnAbymeRadiance` for this mirror.
    pub fn recurse_at_mirror(
        &mut self,
        mirror: MirrorSurface,
        view_origin: Vec3,
        view_dir: Vec3,
        base_radiance: &MiseEnAbymeRadiance,
        probe: &dyn MirrorRaymarchProbe,
        companion_eye: Option<&CompanionEyeWitness>,
        companion_provider: Option<&dyn CompanionSemanticFrameProvider>,
    ) -> Result<MiseEnAbymeRadiance, Stage9Error> {
        // § Track recursion stack on a SmallVec to avoid heap-alloc on the
        //   hot path. Capacity is RECURSION_DEPTH_HARD_CAP+1.
        let mut stack: SmallVec<[(MirrorSurface, RegionId, Vec3, Vec3); 6]> = SmallVec::new();
        stack.push((mirror, mirror.region_id, view_origin, view_dir));

        let mut accumulator = MiseEnAbymeRadiance::ZERO;
        let mut depth = RecursionDepthBudget::new();
        // § Companion-eye reflection happens at depth=0 ONLY ; the iris
        //   recursion uses the IrisDepthHint instead of the global budget.
        //   For the simple loop here we treat the companion-eye as a one-
        //   shot reflection that contributes the semantic frame at full
        //   weight then truncates.
        if let (Some(witness), Some(prov)) = (companion_eye, companion_provider) {
            // § Compute the per-bounce attenuation using the canonical
            //   confidence evaluator at depth=0 (cornea is the primary
            //   reflective surface).
            let confidence =
                self.config
                    .confidence
                    .evaluate(KanConfidenceInputs::new(0, mirror.roughness, 0.0));
            match witness.reflect(mirror.region_id, confidence.attenuation, prov) {
                Ok(rad) => {
                    accumulator.accumulate(1.0, &rad);
                    self.compositor
                        .record_event(Stage9Event::KanConfidenceTerminate {
                            depth: 0,
                            confidence: confidence.attenuation,
                        });
                    return Ok(accumulator);
                }
                Err(err) => {
                    let ev = witness.error_to_event(&err);
                    self.compositor.record_event(ev);
                    self.compositor.tick_eye_redaction();
                    // § Per spec § V.6.d : "eye-occlusion (creature-blink/
                    //   look-away) ⊗ truncates-recursion" — return ZERO.
                    return Ok(MiseEnAbymeRadiance::ZERO);
                }
            }
        }

        while let Some((mirror, src_region, _, view_dir)) = stack.pop() {
            // § Anti-surveillance gate : reflection of `mirror.region_id`
            //   into `src_region`. The recursion is "from src_region's
            //   perspective looking at mirror.region_id" — so the surveil-
            //   policy gate decides whether seeing src→mirror is allowed.
            if !self
                .config
                .region_boundary
                .permits(src_region, mirror.region_id)
            {
                self.compositor
                    .record_event(Stage9Event::SurveillanceBlocked {
                        source_region: src_region,
                        target_region: mirror.region_id,
                    });
                self.compositor.tick_surveillance_block();
                // § Treat as zero-attenuation bounce ; continue any
                //   sibling bounces that may still be pending.
                continue;
            }

            // § Compute the reflected camera + the new view direction.
            let reflected_origin = mirror.reflect_position(view_origin);
            let reflected_dir = mirror.reflect_direction(view_dir);

            // § Probe along the reflected ray.
            let probe_result = probe.probe(reflected_origin, reflected_dir);

            // § Inputs to the confidence evaluator.
            let atmosphere = match &probe_result {
                ProbeResult::Hit { atmosphere, .. } => *atmosphere,
                ProbeResult::Miss => 1.0, // total atmospheric loss on miss
            };
            let confidence_inputs =
                KanConfidenceInputs::new(depth.current(), mirror.roughness, atmosphere);
            let confidence = self.config.confidence.evaluate(confidence_inputs);

            // § Accumulate base radiance scaled by confidence. On miss,
            //   we use the base_radiance as the atmospheric-sky fallback ;
            //   on hit we use a synthesized "just the surface color"
            //   approximation. In the full pipeline the hit-radiance comes
            //   from a Stage-5+7 cached lookup ; in this Rust reference we
            //   approximate via the base_radiance scaled to mirror.mirrorness.
            let bounce_rad = match &probe_result {
                ProbeResult::Hit { .. } => {
                    // § Approximate sub-radiance : base scaled by mirror
                    //   mirrorness. The GPU implementation will replace
                    //   this with the actual cached Stage-7 amplified
                    //   radiance at the hit point.
                    let mut r = *base_radiance;
                    r.scale(mirror.mirrorness);
                    r
                }
                ProbeResult::Miss => {
                    // § Atmospheric loss → a small grey "sky" contribution.
                    MiseEnAbymeRadiance::splat(0.05)
                }
            };
            accumulator.accumulate(confidence.attenuation, &bounce_rad);
            self.compositor.tick_bounce();

            // § Decide whether to recurse further. Three checks :
            //     - confidence says continue
            //     - depth < max_depth
            //     - depth < HARD_CAP (always, by RecursionDepthBudget)
            //     - probe hit (we have a new surface to recurse from)
            if !confidence.should_continue {
                self.compositor
                    .record_event(Stage9Event::KanConfidenceTerminate {
                        depth: depth.current(),
                        confidence: confidence.attenuation,
                    });
                self.compositor.tick_kan_terminate();
                continue;
            }
            // § Advance the depth, emitting the HardCapTerminate event if
            //   the global hard cap is reached. The runtime-configurable
            //   `max_depth` ≤ HARD_CAP also triggers a HardCapTerminate
            //   event when it caps the recursion : both forms are
            //   "the depth budget said no" so they share the same telemetry
            //   counter — the difference is only whether the cap is the
            //   spec hard-cap or a runtime-tuned softer cap.
            if depth.current() + 1 >= self.config.max_depth.min(RECURSION_DEPTH_HARD_CAP) {
                self.compositor.record_event(Stage9Event::HardCapTerminate {
                    depth: depth.current() + 1,
                });
                self.compositor.tick_hard_cap();
                continue;
            }
            let next_depth = match depth.try_advance() {
                Ok(d) => d,
                Err(_) => {
                    self.compositor.record_event(Stage9Event::HardCapTerminate {
                        depth: RECURSION_DEPTH_HARD_CAP,
                    });
                    self.compositor.tick_hard_cap();
                    continue;
                }
            };
            depth = next_depth;

            if let ProbeResult::Hit {
                position,
                gradient,
                curvature,
                material,
                region_id,
                ..
            } = probe_result
            {
                // § Build the next mirror surface from the probe hit. If it
                //   is itself a mirror, push to the recursion stack.
                let maybe_next_mirror = MirrorSurface::try_from_probe(
                    position,
                    gradient,
                    curvature,
                    &material,
                    self.config.mirrorness_channel,
                    region_id,
                    self.config.threshold,
                );
                if let Some(next_mirror) = maybe_next_mirror {
                    stack.push((
                        next_mirror,
                        mirror.region_id,
                        reflected_origin,
                        reflected_dir,
                    ));
                }
                // § If not a mirror, recursion ends naturally at this hit.
            }
        }

        Ok(accumulator)
    }

    /// § End the frame and emit the FrameStats event. Returns the per-
    ///   frame compositor stats so the runtime can act on it.
    pub fn end_frame(&mut self) -> WitnessCompositorStats {
        self.compositor.end_frame()
    }
}

#[cfg(test)]
mod tests {
    use super::super::probe::{ConstantProbe, FixedHit};
    use super::*;
    use cssl_substrate_kan::{KanMaterial, EMBEDDING_DIM};

    fn make_mirror_material(mirrorness: f32) -> KanMaterial {
        let mut emb = [0.0_f32; EMBEDDING_DIM];
        emb[7] = mirrorness;
        KanMaterial::creature_morphology(emb)
    }

    fn make_planar_mirror(mirrorness: f32, region: RegionId) -> MirrorSurface {
        MirrorSurface {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            mirrorness,
            roughness: 1.0 - mirrorness,
            region_id: region,
            curvature: 0.0,
        }
    }

    fn fixture_hit(mirrorness: f32, region: RegionId) -> FixedHit {
        FixedHit {
            position: Vec3::new(0.0, 1.0, 0.0),
            gradient: Vec3::Y,
            curvature: 0.05,
            material: make_mirror_material(mirrorness),
            region_id: region,
            atmosphere: 0.0,
        }
    }

    /// § RecursionDepthBudget starts at 0.
    #[test]
    fn budget_starts_at_zero() {
        let b = RecursionDepthBudget::new();
        assert_eq!(b.current(), 0);
        assert!(b.can_advance());
    }

    /// § RecursionDepthBudget advances up to HARD_CAP.
    #[test]
    fn budget_advances_to_hard_cap() {
        let mut b = RecursionDepthBudget::new();
        for _ in 0..RECURSION_DEPTH_HARD_CAP {
            b = b.try_advance().unwrap();
        }
        assert_eq!(b.current(), RECURSION_DEPTH_HARD_CAP);
        assert!(!b.can_advance());
    }

    /// § RecursionDepthBudget::try_advance returns Err past HARD_CAP.
    #[test]
    fn budget_rejects_past_hard_cap() {
        let b = RecursionDepthBudget::at(RECURSION_DEPTH_HARD_CAP);
        let err = b.try_advance();
        assert!(matches!(
            err,
            Err(Stage9Error::RecursionDepthExhausted { .. })
        ));
    }

    /// § RecursionDepthBudget::remaining = HARD_CAP - current.
    #[test]
    fn budget_remaining_correct() {
        let b = RecursionDepthBudget::at(2);
        assert_eq!(b.remaining(), RECURSION_DEPTH_HARD_CAP - 2);
    }

    /// § Pass with miss-probe at primary surface returns ZERO total
    ///   energy : on a probe-miss, the atmospheric extinction is set to
    ///   the maximum (1.0), the KAN-confidence atmosphere term zeroes
    ///   out the bounce attenuation, and no contribution is accumulated.
    ///   This is the "atmospheric loss" termination per spec § Stage-9.
    #[test]
    fn miss_probe_returns_zero_with_full_attenuation_loss() {
        let mut p = MiseEnAbymePass::substrate_default();
        let mirror = make_planar_mirror(0.9, RegionId(7));
        let probe = ConstantProbe::always_miss();
        let base = MiseEnAbymeRadiance::ZERO;
        p.begin_frame();
        let r = p
            .recurse_at_mirror(
                mirror,
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                &base,
                &probe,
                None,
                None,
            )
            .unwrap();
        // § With atmospheric extinction = 1.0 (probe miss) the KAN
        //   confidence is zeroed by the atmosphere factor → zero
        //   contribution accumulated. The recursion terminates.
        assert_eq!(r.total_energy(), 0.0);
    }

    /// § Pass with cross-region probe blocks via surveillance gate.
    ///   The pass uses the CorneaAxis7 channel so the creature-morphology
    ///   material (with axis-7 mirrorness) is recognized as a mirror at
    ///   the second bounce. The bounce attempts to reflect from
    ///   region-8 into region-9 → blocked under SameRegionOnly policy.
    #[test]
    fn cross_region_probe_blocked() {
        let mut cfg = MiseEnAbymePassConfig::default();
        cfg.mirrorness_channel = MirrornessChannel::CorneaAxis7;
        cfg.region_boundary = RegionBoundary::default();
        let mut p = MiseEnAbymePass::new(cfg);
        let mirror = make_planar_mirror(0.9, RegionId(8));
        let hit = fixture_hit(0.9, RegionId(9)); // different region
        let probe = ConstantProbe::always_hit(hit);
        let base = MiseEnAbymeRadiance::splat(0.5);
        p.begin_frame();
        let _r = p
            .recurse_at_mirror(
                mirror,
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                &base,
                &probe,
                None,
                None,
            )
            .unwrap();
        // We should have observed at least one surveillance block.
        let stats = p.end_frame();
        assert!(stats.surveillance_blocks >= 1);
    }

    /// § Pass with a long mirror-corridor truncates at HARD_CAP.
    #[test]
    fn mirror_corridor_truncates_at_hard_cap() {
        // § Build a "scripted" probe that always returns a mirror surface
        //   in the SAME region. This should recurse repeatedly until the
        //   KAN confidence drops below MIN_CONFIDENCE OR the HARD_CAP
        //   triggers.
        let region = RegionId(7);
        let mut cfg = MiseEnAbymePassConfig::default();
        cfg.mirrorness_channel = MirrornessChannel::CorneaAxis7;
        cfg.region_boundary = RegionBoundary::default();
        let mut p = MiseEnAbymePass::new(cfg);
        let mirror = make_planar_mirror(0.9, region);
        let hit = fixture_hit(0.9, region);
        let probe = ConstantProbe::always_hit(hit);
        let base = MiseEnAbymeRadiance::splat(1.0);
        p.begin_frame();
        let r = p
            .recurse_at_mirror(
                mirror,
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                &base,
                &probe,
                None,
                None,
            )
            .unwrap();
        let stats = p.end_frame();
        // § Energy is bounded ; recursion did not run away.
        assert!(r.total_energy() < base.total_energy() * 100.0);
        // § Either KAN-terminate or hard-cap-terminate fired.
        assert!(stats.kan_terminate_pixels + stats.hard_cap_pixels >= 1);
    }

    /// § Pass max_depth=0 means no recursion (no bounces happen).
    /// (max_depth=1 means depth=0 happens but no advance ; depth=0
    /// processes the mirror surface so 1 bounce is recorded.)
    #[test]
    fn max_depth_zero_skips_advance() {
        let mut cfg = MiseEnAbymePassConfig::default();
        cfg.mirrorness_channel = MirrornessChannel::CorneaAxis7;
        cfg.max_depth = 1;
        let mut p = MiseEnAbymePass::new(cfg);
        let mirror = make_planar_mirror(0.9, RegionId(7));
        let probe = ConstantProbe::always_hit(fixture_hit(0.9, RegionId(7)));
        let base = MiseEnAbymeRadiance::splat(0.5);
        p.begin_frame();
        let _ = p
            .recurse_at_mirror(
                mirror,
                Vec3::new(0.0, 5.0, 0.0),
                Vec3::new(0.0, -1.0, 0.0),
                &base,
                &probe,
                None,
                None,
            )
            .unwrap();
        let stats = p.end_frame();
        // § With max_depth=1 we process the depth-0 bounce only.
        assert_eq!(stats.bounces, 1);
    }

    /// § Default config uses HARD_CAP as max_depth.
    #[test]
    fn default_config_max_depth_is_hard_cap() {
        let cfg = MiseEnAbymePassConfig::default();
        assert_eq!(cfg.max_depth, RECURSION_DEPTH_HARD_CAP);
    }

    /// § Config defaults to RoughnessMetallic13_14 channel.
    #[test]
    fn default_config_channel() {
        let cfg = MiseEnAbymePassConfig::default();
        assert!(matches!(
            cfg.mirrorness_channel,
            MirrornessChannel::RoughnessMetallic13_14
        ));
    }
}

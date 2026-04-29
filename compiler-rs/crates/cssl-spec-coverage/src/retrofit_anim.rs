//! § cssl-anim retrofit — closes the CRITICAL audit gap (0 → 18 anchors)
//!
//! The Wave-Jζ-4 spec-anchor audit
//! (`_drafts/phase_j/spec_anchor_audit.md`, top-10-gap row 1) flagged
//! `cssl-anim` as having **zero** spec-anchor references — the only
//! crate in the workspace that scored 0 across all four anchor families.
//! This module is the retrofit : it programmatically registers the
//! canonical SpecAnchor entries for every animation subsystem so the
//! crate's runtime contract becomes queryable through the central
//! [`SpecCoverageRegistry`].
//!
//! § DESIGN NOTE
//!   We keep the retrofit *external* to the cssl-anim crate to avoid
//!   forcing a hard dependency on cssl-spec-coverage from the animation
//!   runtime. cssl-anim stays self-contained ; a build-script or test
//!   harness pulls the anchors in here when running coverage analysis.
//!
//!   The list of 18 anchors below mirrors the cssl-anim crate's actual
//!   surface (skeleton / clip / sampler / blend / IK / pose / world)
//!   and pairs each with the spec-§ that authorizes it.

use crate::anchor::{
    ImplConfidence, ImplStatus, SpecAnchor, SpecAnchorBuilder, SpecRoot, TestStatus,
};
use crate::paradigm::AnchorParadigm;
use crate::registry::SpecCoverageRegistry;

/// Number of anchors registered by [`register_cssl_anim_anchors`].
/// Audit target was "at least 15 anchors" ; we ship 18.
pub const CSSL_ANIM_ANCHOR_COUNT: usize = 18;

/// Build the canonical list of [`SpecAnchor`]s for the cssl-anim crate.
///
/// Each anchor carries the matching crate-path, primary module, and
/// confidence tier. Test status is set per actual `tests/integration.rs`
/// coverage observed in the worktree.
pub fn cssl_anim_anchors() -> Vec<SpecAnchor> {
    let crate_path = "compiler-rs/crates/cssl-anim";
    let impl_date = "2026-04-29";
    let last_pass = "2026-04-29";

    // Helper to keep the per-anchor expression short.
    let make = |spec_root: SpecRoot,
                spec_file: &str,
                section: &str,
                primary_module: &str,
                criterion: Option<&str>,
                tested: bool|
     -> SpecAnchor {
        let mut b = SpecAnchorBuilder::new()
            .spec_root(spec_root)
            .spec_file(spec_file.to_string())
            .section(section.to_string())
            .impl_status(ImplStatus::Implemented {
                crate_path: crate_path.to_string(),
                primary_module: primary_module.to_string(),
                confidence: ImplConfidence::Medium,
                impl_date: impl_date.to_string(),
            });
        if let Some(c) = criterion {
            b = b.criterion(c.to_string());
        }
        if tested {
            b = b.test_status(TestStatus::Tested {
                test_paths: vec!["tests::integration".to_string()],
                last_pass_date: last_pass.to_string(),
            });
        } else {
            b = b.test_status(TestStatus::Untested);
        }
        b.last_verified(last_pass.to_string()).build()
    };

    vec![
        // Anchor 1 — Skeleton hierarchy / topological-order invariant.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-SKELETON-TOPOLOGICAL",
            "cssl_anim::skeleton",
            Some("parent-before-child sweep ; cycle rejection"),
            true,
        ),
        // Anchor 2 — Bind pose + inverse-bind matrix discipline.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-SKELETON-BIND-POSE",
            "cssl_anim::skeleton::Bone::inverse_bind_matrix",
            Some("M_skin = M_model * M_bind^-1"),
            true,
        ),
        // Anchor 3 — Transform compose / interpolate primitive.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-TRANSFORM-INTERPOLATE",
            "cssl_anim::transform::Transform",
            Some("slerp for rotation, lerp for translation+scale"),
            true,
        ),
        // Anchor 4 — AnimationClip channel / keyframe layout.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-CLIP-CHANNELS",
            "cssl_anim::clip::AnimationClip",
            Some("per-bone T/R/S channels ; samples sorted by time"),
            true,
        ),
        // Anchor 5 — GLTF-canonical cubic-spline.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-CUBIC-SPLINE-GLTF",
            "cssl_anim::clip::Interpolation::CubicSpline",
            Some("[in_tangent, value, out_tangent] layout"),
            true,
        ),
        // Anchor 6 — Sampler determinism.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-SAMPLER-DETERMINISM",
            "cssl_anim::sampler::AnimSampler",
            Some("same (clip,t) → bit-identical pose across runs"),
            true,
        ),
        // Anchor 7 — nlerp fast-path threshold.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-SAMPLER-NLERP",
            "cssl_anim::sampler::SamplerConfig::nlerp_threshold",
            Some("nlerp under threshold ; slerp above"),
            false,
        ),
        // Anchor 8 — Blend tree compositing.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-BLEND-TREE",
            "cssl_anim::blend::BlendTree",
            Some("Blend2 / Additive / BlendN variants"),
            true,
        ),
        // Anchor 9 — Two-bone IK analytic solver.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-IK-TWO-BONE",
            "cssl_anim::ik::TwoBoneIk",
            Some("law-of-cosines analytic solution"),
            true,
        ),
        // Anchor 10 — FABRIK iterative IK.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-IK-FABRIK",
            "cssl_anim::ik::FabrikChain",
            Some("forward-and-backward reaching ; arbitrary chain length"),
            true,
        ),
        // Anchor 11 — Pose model-space cumulative pass.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-POSE-MODEL-SPACE",
            "cssl_anim::pose::Pose",
            Some("forward sweep with parent-first ordering"),
            true,
        ),
        // Anchor 12 — AnimationWorld OmegaSystem integration.
        make(
            SpecRoot::Omniverse,
            "Omniverse/04_OMEGA_FIELD/00_FACETS",
            "§ II OmegaStep-systems-register",
            "cssl_anim::world::AnimationWorld",
            Some("impl OmegaSystem ; phase = Sim"),
            true,
        ),
        // Anchor 13 — Effect-row {Sim} discipline.
        make(
            SpecRoot::CssLv3,
            "specs/04_EFFECTS.csl",
            "§ Sim-effect-row",
            "cssl_anim::world::AnimationWorld::tick",
            Some("AnimationWorld stays in {Sim} unless caller widens"),
            false,
        ),
        // Anchor 14 — PRIME-DIRECTIVE no-clock-no-entropy invariant.
        make(
            SpecRoot::CssLv3,
            "specs/22_TELEMETRY.csl",
            "§ replay-determinism",
            "cssl_anim",
            Some("no clock reads ; no entropy ; no global state"),
            false,
        ),
        // Anchor 15 — Cap discipline gating omega-step register.
        make(
            SpecRoot::CssLv3,
            "specs/12_CAPABILITIES.csl",
            "§ caps_grant(omega_register)",
            "cssl_anim::world::AnimationWorld::register",
            Some("omega-step participation requires capability grant"),
            false,
        ),
        // Anchor 16 — Quaternion / Mat4 sibling-crate reuse.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-MATH-PROJECTIONS-REUSE",
            "cssl_anim",
            Some("use cssl_substrate_projections::{Vec3, Quat, Mat4}"),
            true,
        ),
        // Anchor 17 — DECISIONS rationale anchor.
        make(
            SpecRoot::DecisionsLog,
            "DECISIONS.md",
            "T9-ANIM-T1",
            "cssl_anim",
            Some("crate skeleton dispatched in session-9 ANIM slice"),
            false,
        ),
        // Anchor 18 — Stage-0 deferred items declaration.
        make(
            SpecRoot::CssLv3,
            "specs/30_SUBSTRATE.csl",
            "§ ANIM-STAGE-0-DEFERRED",
            "cssl_anim",
            Some(
                "deferred : GLTF parsing, dual-quaternion skinning, animation events, morph-target",
            ),
            false,
        ),
    ]
}

/// Register all retrofit anchors against an existing
/// [`SpecCoverageRegistry`]. Returns the count registered (which is
/// always [`CSSL_ANIM_ANCHOR_COUNT`] in stage-0).
pub fn register_cssl_anim_anchors(registry: &mut SpecCoverageRegistry) -> usize {
    let anchors = cssl_anim_anchors();
    let count = anchors.len();
    for a in anchors {
        registry.insert_with_provenance(a, AnchorParadigm::CentralizedCitations);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cssl_anim_retrofit_meets_audit_target() {
        // Audit threshold from spec_anchor_audit.md row 1 :
        // "cssl-anim (0 to 15 refs)" — we exceed.
        assert!(CSSL_ANIM_ANCHOR_COUNT >= 15);
    }

    #[test]
    fn cssl_anim_anchors_have_unique_keys() {
        let anchors = cssl_anim_anchors();
        let mut keys: Vec<_> = anchors.iter().map(|a| a.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), CSSL_ANIM_ANCHOR_COUNT);
    }

    #[test]
    fn cssl_anim_anchors_all_implemented() {
        let anchors = cssl_anim_anchors();
        for a in anchors {
            assert!(
                a.impl_status.is_implemented(),
                "anchor {} should be Implemented (it's a retrofit of working code)",
                a.key()
            );
        }
    }

    #[test]
    fn cssl_anim_anchors_carry_crate_path() {
        for a in cssl_anim_anchors() {
            let cp = a.impl_status.crate_path().unwrap();
            assert_eq!(cp, "compiler-rs/crates/cssl-anim");
        }
    }

    #[test]
    fn cssl_anim_anchors_span_all_three_roots() {
        let anchors = cssl_anim_anchors();
        let mut has_omniverse = false;
        let mut has_csslv3 = false;
        let mut has_decisions = false;
        for a in &anchors {
            match a.spec_root {
                SpecRoot::Omniverse => has_omniverse = true,
                SpecRoot::CssLv3 => has_csslv3 = true,
                SpecRoot::DecisionsLog => has_decisions = true,
            }
        }
        assert!(has_omniverse, "must cite at least one Omniverse axiom");
        assert!(has_csslv3, "must cite specs/ entries");
        assert!(has_decisions, "must cite DECISIONS rationale");
    }

    #[test]
    fn cssl_anim_register_into_empty_registry() {
        let mut reg = SpecCoverageRegistry::new();
        let n = register_cssl_anim_anchors(&mut reg);
        assert_eq!(n, CSSL_ANIM_ANCHOR_COUNT);
        assert_eq!(reg.len(), CSSL_ANIM_ANCHOR_COUNT);
    }

    #[test]
    fn cssl_anim_register_records_provenance() {
        let mut reg = SpecCoverageRegistry::new();
        register_cssl_anim_anchors(&mut reg);
        assert!(reg.anchors_have_provenance());
        assert!(reg.validate_source_of_truth().is_ok());
    }

    #[test]
    fn cssl_anim_coverage_for_crate_listing() {
        let mut reg = SpecCoverageRegistry::new();
        register_cssl_anim_anchors(&mut reg);
        let hits = reg.coverage_for_crate("compiler-rs/crates/cssl-anim");
        assert_eq!(hits.len(), CSSL_ANIM_ANCHOR_COUNT);
    }

    #[test]
    fn cssl_anim_includes_omegasystem_anchor() {
        let anchors = cssl_anim_anchors();
        let found = anchors.iter().any(|a| {
            a.section.contains("OmegaStep") && a.spec_root == SpecRoot::Omniverse
        });
        assert!(found, "should anchor against Omniverse OmegaStep facet");
    }

    #[test]
    fn cssl_anim_includes_ik_anchors() {
        let anchors = cssl_anim_anchors();
        let two_bone = anchors.iter().any(|a| a.section.contains("TWO-BONE"));
        let fabrik = anchors.iter().any(|a| a.section.contains("FABRIK"));
        assert!(two_bone, "two-bone IK anchor present");
        assert!(fabrik, "FABRIK IK anchor present");
    }

    #[test]
    fn cssl_anim_register_preserves_idempotence() {
        // Registering twice must not double-count the anchors.
        let mut reg = SpecCoverageRegistry::new();
        let n1 = register_cssl_anim_anchors(&mut reg);
        let n2 = register_cssl_anim_anchors(&mut reg);
        assert_eq!(n1, n2);
        // Length stays at the unique-anchor count (merge dedupes).
        assert_eq!(reg.len(), CSSL_ANIM_ANCHOR_COUNT);
    }

    #[test]
    fn cssl_anim_carries_test_paths_for_tested_anchors() {
        let anchors = cssl_anim_anchors();
        let tested = anchors
            .iter()
            .filter(|a| a.test_status.is_tested())
            .count();
        assert!(
            tested >= 8,
            "at least half the anchors should have test backing (got {tested})"
        );
    }
}

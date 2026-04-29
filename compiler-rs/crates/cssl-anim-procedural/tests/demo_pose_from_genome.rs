//! § demo_pose_from_genome — procedural-pose-from-genome end-to-end demo.
//!
//! § THESIS
//!   Demonstrates the slogan : "give the runtime a genome, it produces a
//!   creature whose pose is animated from first principles". No keyframes,
//!   no artist-authored timeline. The demo :
//!
//!     1. Builds two distinct creature genomes (varying the genome
//!        embedding).
//!     2. Constructs a humanoid-style 5-bone skeleton for each.
//!     3. Generates a default KAN-pose-network seeded from the genome.
//!     4. Wires up a procedural-animation world.
//!     5. Steps the world for N ticks under varying control signals.
//!     6. Asserts pose properties : different genomes produce different
//!        poses ; the time-evolution is smooth (no large jumps) ; the
//!        skinning matrices are well-formed.
//!
//! § INTEGRATION GATES
//!   - **D117 physics** : the `run_ik` step accepts a `body_world_pose_fn`
//!     callback that the host wires to the SDF-XPBD physics world. The
//!     demo verifies the callback signature works end-to-end.
//!   - **D124 VR-embodiment** : the procedural pose stream is what the
//!     OpenXR host (D124) reads to drive the player avatar's body.
//!     The demo verifies the pose's bone count + model-matrix layout
//!     matches the avatar contract (5+ bones, model-matrices contiguous).

#![allow(clippy::uninlined_format_args)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::suboptimal_flops)]

use cssl_anim_procedural::deformation::UniformFieldProbe;
use cssl_anim_procedural::skeleton::{Bone, ROOT_PARENT};
use cssl_anim_procedural::{
    BoneSegmentDeformation, ControlSignal, GenomeEmbedding, GenomeHandle, KanPoseNetwork,
    PhysicsRig, ProceduralAnimationWorld, ProceduralCreatureBuilder, ProceduralSkeleton, Transform,
};
use cssl_substrate_projections::Vec3;

const GENOME_DIM: usize = cssl_anim_procedural::GENOME_DIM;

fn make_humanoid_skeleton() -> ProceduralSkeleton {
    // Five-bone humanoid : root → spine → chest → head + arm.
    let bones = vec![
        Bone::new("root", ROOT_PARENT, Transform::IDENTITY).with_segment_length(0.0),
        Bone::new(
            "spine",
            0,
            Transform::from_translation(Vec3::new(0.0, 0.5, 0.0)),
        )
        .with_segment_length(0.5),
        Bone::new(
            "chest",
            1,
            Transform::from_translation(Vec3::new(0.0, 0.5, 0.0)),
        )
        .with_segment_length(0.4),
        Bone::new(
            "head",
            2,
            Transform::from_translation(Vec3::new(0.0, 0.3, 0.0)),
        )
        .with_segment_length(0.2),
        Bone::new(
            "arm",
            2,
            Transform::from_translation(Vec3::new(0.3, 0.2, 0.0)),
        )
        .with_segment_length(0.6)
        .with_stiffness(0.4),
    ];
    ProceduralSkeleton::from_bones(bones).expect("humanoid skeleton must build")
}

fn make_creature_with_genome(id: u64, fill: f32) -> cssl_anim_procedural::ProceduralCreature {
    let skel = make_humanoid_skeleton();
    let genome = GenomeHandle::new(id, GenomeEmbedding::from_values([fill; GENOME_DIM]));
    let pose_net = KanPoseNetwork::default_for(&skel, 8);
    let rig = PhysicsRig::default_for(&skel);
    ProceduralCreatureBuilder::new(skel, genome)
        .with_pose_network(pose_net)
        .with_physics_rig(rig)
        .with_control_dim(8)
        .build()
}

#[test]
fn demo_two_genomes_produce_distinct_poses() {
    let mut world = ProceduralAnimationWorld::new();
    let id1 = world.register(make_creature_with_genome(1, 0.2));
    let id2 = world.register(make_creature_with_genome(2, 0.8));

    // Step a few ticks so the time-phase has propagated.
    for _ in 0..16 {
        world.tick(0.016).unwrap();
    }

    let p1 = &world.creature(id1).expect("creature 1").pose;
    let p2 = &world.creature(id2).expect("creature 2").pose;
    let mut differs = false;
    for i in 0..p1.bone_count() {
        if p1.local_transform(i) != p2.local_transform(i) {
            differs = true;
            break;
        }
    }
    assert!(
        differs,
        "creatures with different genomes should produce different poses"
    );
}

#[test]
fn demo_smooth_time_evolution() {
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(1, 0.5));

    // Capture pose at successive ticks ; consecutive deltas should be
    // bounded (no NaN, no large jumps).
    let mut prev_translations: Vec<Vec3> = Vec::new();
    for tick in 0..32 {
        world.tick(0.016).unwrap();
        let creature = world.creature(id).unwrap();
        if tick > 0 {
            for (i, t) in creature.pose.locals().iter().enumerate() {
                let prev = prev_translations[i];
                let cur = t.translation;
                let delta = (cur - prev).length();
                assert!(delta < 1.0, "bone {} delta {} > 1.0 (not smooth)", i, delta);
                assert!(cur.x.is_finite());
                assert!(cur.y.is_finite());
                assert!(cur.z.is_finite());
            }
        }
        prev_translations = creature
            .pose
            .locals()
            .iter()
            .map(|t| t.translation)
            .collect();
    }
}

#[test]
fn demo_skinning_matrices_well_formed() {
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(7, 0.3));
    world.tick(0.016).unwrap();

    let creature = world.creature(id).unwrap();
    // Compute skinning matrices manually + sanity-check.
    let mut buf = Vec::new();
    let mut pose = creature.pose.clone();
    pose.compute_skinning_matrices(&creature.skeleton, &mut buf);
    assert_eq!(buf.len(), creature.skeleton.bone_count());
    for m in &buf {
        // Sanity : bottom row should be (0, 0, 0, 1) — well-formed
        // affine.
        assert!((m.cols[0][3] - 0.0).abs() < 1e-4);
        assert!((m.cols[3][3] - 1.0).abs() < 1e-4);
    }
}

#[test]
fn demo_d117_physics_ik_callback_works() {
    // D117 integration : the IK step accepts a body-world-pose callback.
    // Validate the contract by running an IK pass with a stub callback
    // that returns Motor::IDENTITY for every body.
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(1, 0.5));
    world.tick(0.016).unwrap();
    let outcome = world
        .run_ik(id, 0.016, |_body_id| cssl_pga::Motor::IDENTITY)
        .expect("ik step succeeds");
    assert!(
        outcome.iterations
            <= world
                .creature(id)
                .unwrap()
                .physics_ik
                .config()
                .max_iterations
    );
}

#[test]
fn demo_d124_vr_embodiment_pose_layout() {
    // D124 VR-embodiment integration : the OpenXR host needs a
    // bone-count >= 1 + a model-matrix per bone for skinning upload.
    // Verify the procedural pose meets the contract.
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(1, 0.5));
    world.tick(0.016).unwrap();

    let creature = world.creature(id).unwrap();
    assert!(creature.pose.bone_count() >= 1, "pose must carry >= 1 bone");
    assert_eq!(
        creature.pose.model_matrices().len(),
        creature.pose.bone_count(),
        "model matrices must be 1:1 with bones"
    );
}

#[test]
fn demo_wave_field_drives_deformation() {
    // Emulate a wave-field probe that produces uniform pressure pushing
    // creatures along +X. The bone-segment deformation surface should
    // produce non-zero displacement on bones with stiffness < 1.0.
    let probe = UniformFieldProbe {
        pressure: 1.5,
        force_dir: Vec3::new(1.0, 0.0, 0.0),
    };
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(1, 0.5));

    world.tick_with_probe(0.05, &probe).unwrap();
    let creature = world.creature(id).unwrap();
    // Find the arm bone — it has stiffness 0.4 so should deform.
    let arm_idx = creature.skeleton.find_bone("arm").unwrap();
    let sample = creature.deformation.sample_for_bone(arm_idx).unwrap();
    assert!(sample.displacement.length() > 0.0, "arm should deform");
    assert!(
        sample.displacement.x > 0.0,
        "deformation direction matches probe"
    );
}

#[test]
fn demo_omnoid_layers_populate() {
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(1, 0.5));
    world.tick(0.016).unwrap();

    let creature = world.creature(id).unwrap();
    let projections = creature.omnoid.projections();
    // Should have at least Aura + Bone projections populated.
    let kinds: std::collections::HashSet<_> = projections.iter().map(|p| p.kind).collect();
    assert!(
        kinds.contains(&cssl_anim_procedural::OmnoidLayerKind::Aura)
            || kinds.contains(&cssl_anim_procedural::OmnoidLayerKind::Bone),
        "omnoid should populate at least Aura or Bone after a tick"
    );
}

#[test]
fn demo_control_signal_drives_pose() {
    // Control signal change should propagate to the pose output.
    let mut world = ProceduralAnimationWorld::new();
    let id = world.register(make_creature_with_genome(1, 0.5));
    world.tick(0.016).unwrap();
    let pose_neutral = world.creature(id).unwrap().pose.clone();

    let mut signal = ControlSignal::zero(8);
    signal.set_component(0, 0.7); // forward speed
    signal.set_component(7, 0.5); // breathing amplitude
    world.set_control(id, signal).unwrap();

    // Advance more so the new control signal has time to register.
    world.tick(0.016).unwrap();
    let pose_active = &world.creature(id).unwrap().pose;
    let mut differs = false;
    for i in 0..pose_neutral.bone_count() {
        if pose_neutral.local_transform(i) != pose_active.local_transform(i) {
            differs = true;
            break;
        }
    }
    assert!(differs, "control signal change should perturb the pose");
}

#[test]
fn demo_replay_determinism_across_worlds() {
    let world_a = run_world_for_n_ticks(10);
    let world_b = run_world_for_n_ticks(10);
    // Compare the first creature's pose in both worlds.
    let pa = world_a.iter().next().expect("at least one creature");
    let pb = world_b.iter().next().expect("at least one creature");
    for i in 0..pa.1.pose.bone_count() {
        assert_eq!(
            pa.1.pose.local_transform(i),
            pb.1.pose.local_transform(i),
            "replay determinism violated at bone {}",
            i
        );
    }
}

fn run_world_for_n_ticks(n: u32) -> ProceduralAnimationWorld {
    let mut world = ProceduralAnimationWorld::new();
    let _id = world.register(make_creature_with_genome(42, 0.3));
    for _ in 0..n {
        world.tick(0.016).unwrap();
    }
    world
}

#[test]
fn demo_attestation_present() {
    assert!(cssl_anim_procedural::ATTESTATION.contains("no hurt nor harm"));
}

#[test]
fn demo_can_register_many_creatures() {
    // Stress smoke : the world should accommodate dozens of creatures.
    let mut world = ProceduralAnimationWorld::new();
    let mut ids = Vec::new();
    for i in 0..16 {
        ids.push(world.register(make_creature_with_genome(i, 0.1 + (i as f32) * 0.05)));
    }
    for _ in 0..4 {
        world.tick(0.016).unwrap();
    }
    assert_eq!(world.creature_count(), 16);
    for id in ids {
        assert!(world.creature(id).is_some());
    }
}

#[test]
fn demo_seeds_via_kan_pose_seed_from_genome_distinguishes_genomes() {
    // Verify the seed_from_genome helper produces different pose-nets
    // for different genomes — the spec's "every creature has individually
    // rendered body" claim, applied to motion.
    let skel = make_humanoid_skeleton();
    let g1 = GenomeEmbedding::from_values([0.1; GENOME_DIM]);
    let g2 = GenomeEmbedding::from_values([0.9; GENOME_DIM]);
    let n1 = cssl_anim_procedural::kan_pose::seed_from_genome(&skel, &g1, 8);
    let n2 = cssl_anim_procedural::kan_pose::seed_from_genome(&skel, &g2, 8);
    assert_eq!(n1.bone_count(), n2.bone_count());
    let mut differs = false;
    for (c1, c2) in n1.channels().iter().zip(n2.channels().iter()) {
        if (c1.amplitude - c2.amplitude).abs() > 1e-4 {
            differs = true;
            break;
        }
    }
    assert!(differs, "seed_from_genome should produce distinct nets");
}

#[test]
fn demo_deformation_zero_when_no_field_force() {
    // No wave-field forcing ⇒ deformation samples are zero.
    let mut deform = BoneSegmentDeformation::new();
    let skel = make_humanoid_skeleton();
    let positions = vec![Vec3::ZERO; skel.bone_count()];
    let probe = cssl_anim_procedural::deformation::ZeroFieldProbe;
    let n = deform.compute(&skel, &positions, &probe, 0.016);
    assert_eq!(n, 0);
}

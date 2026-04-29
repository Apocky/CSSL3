//! Cross-module integration tests for `cssl-anim`.
//!
//! These tests exercise multi-module flows that are too coarse for any
//! single module's unit-test bench :
//!   - A skeleton + a clip + a sampler producing a pose at half-time
//!     yields the exact interpolated transform between the two
//!     keyframes (the "report-back ✓" the dispatch asks for).
//!   - A blend tree over two clips drives a skeleton through both.
//!   - A FABRIK chain solves a multi-bone reach problem.
//!   - The animation world ticks deterministically across two
//!     identical instances.

use cssl_anim::{
    AnimChannel, AnimSampler, AnimationClip, AnimationWorld, BlendNode, BlendTree, Bone,
    ClipInstance, FabrikChain, Interpolation, KeyframeR, KeyframeT, Pose, Skeleton, Transform,
    TwoBoneIk, ROOT_PARENT,
};
use cssl_substrate_omega_step::{EffectRow, OmegaSystem, SubstrateEffect};
use cssl_substrate_projections::{Quat, Vec3};

fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
    approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
}

fn build_simple_arm_skeleton() -> Skeleton {
    let bones = vec![
        Bone::new("shoulder", ROOT_PARENT, Transform::IDENTITY),
        Bone::new(
            "elbow",
            0,
            Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        ),
        Bone::new(
            "wrist",
            1,
            Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        ),
    ];
    Skeleton::from_bones(bones).expect("simple arm builds")
}

#[test]
fn sample_clip_at_half_time_produces_interpolated_pose() {
    // The headline integration test : a clip that drives bone 1 from
    // (0, 0, 0) at t=0 to (10, 0, 0) at t=1, sampled at t=0.5, must
    // produce a pose where bone 1's local translation is exactly (5, 0, 0).
    let skel = build_simple_arm_skeleton();
    let ch = AnimChannel::translation(
        1,
        Interpolation::Linear,
        vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 1.0,
                value: Vec3::new(10.0, 0.0, 0.0),
            },
        ],
    )
    .expect("translation channel constructs");
    let clip = AnimationClip::new("reach", vec![ch]);
    let sampler = AnimSampler::new();
    let mut pose = Pose::from_bind_pose(&skel);
    sampler
        .sample(&clip, 0.5, &skel, &mut pose)
        .expect("sample ok");
    assert!(
        vec3_approx_eq(
            pose.local_transforms[1].translation,
            Vec3::new(5.0, 0.0, 0.0),
            1e-5
        ),
        "elbow translation at half-time should be midpoint of (0,0,0) → (10,0,0)"
    );
}

#[test]
fn rotation_channel_at_half_is_slerp_midpoint() {
    let skel = build_simple_arm_skeleton();
    let ch = AnimChannel::rotation(
        1,
        Interpolation::Linear,
        vec![
            KeyframeR {
                time: 0.0,
                value: Quat::IDENTITY,
            },
            KeyframeR {
                time: 1.0,
                value: Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_2),
            },
        ],
    )
    .expect("rotation channel builds");
    let clip = AnimationClip::new("rotate", vec![ch]);
    let sampler = AnimSampler::new();
    let mut pose = Pose::from_bind_pose(&skel);
    sampler
        .sample(&clip, 0.5, &skel, &mut pose)
        .expect("sample");
    let v = Vec3::X;
    let got = pose.local_transforms[1].rotation.rotate(v);
    let expected_q = Quat::from_axis_angle(Vec3::Y, core::f32::consts::FRAC_PI_4);
    let expected = expected_q.rotate(v);
    assert!(vec3_approx_eq(got, expected, 1e-4));
}

#[test]
fn pose_model_transforms_propagate_through_chain() {
    let skel = build_simple_arm_skeleton();
    // Drive shoulder with a +Y translation channel.
    let ch_t = AnimChannel::translation(
        0,
        Interpolation::Linear,
        vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 1.0,
                value: Vec3::new(0.0, 5.0, 0.0),
            },
        ],
    )
    .expect("ok");
    let clip = AnimationClip::new("up", vec![ch_t]);
    let sampler = AnimSampler::new();
    let mut pose = Pose::from_bind_pose(&skel);
    sampler
        .sample(&clip, 1.0, &skel, &mut pose)
        .expect("sample");
    pose.recompute_model_transforms(&skel);
    // Wrist (bone 2) should be at shoulder (+Y 5) + elbow-relative (+X 1)
    // + wrist-relative (+X 1) — model translation (2, 5, 0).
    let m = pose.model_transforms[2];
    assert!(approx_eq(m.cols[3][0], 2.0, 1e-5));
    assert!(approx_eq(m.cols[3][1], 5.0, 1e-5));
}

#[test]
fn blend_tree_two_clips_at_half_weight_is_midpoint() {
    let skel = build_simple_arm_skeleton();
    // Two clips : one with elbow @ (0, 0, 0), one with elbow @ (10, 0, 0).
    let mk = |target: Vec3| {
        let ch = AnimChannel::translation(
            1,
            Interpolation::Linear,
            vec![KeyframeT {
                time: 0.0,
                value: target,
            }],
        )
        .expect("ok");
        AnimationClip::new("static", vec![ch])
    };
    let clip_a = mk(Vec3::ZERO);
    let clip_b = mk(Vec3::new(10.0, 0.0, 0.0));
    let mut tree = BlendTree::new();
    let h_a = tree.add_clip(clip_a);
    let h_b = tree.add_clip(clip_b);
    let n_a = tree.add_node(BlendNode::Clip {
        handle: h_a,
        time: 0.0,
    });
    let n_b = tree.add_node(BlendNode::Clip {
        handle: h_b,
        time: 0.0,
    });
    let n_blend = tree.add_node(BlendNode::Blend2 {
        a: n_a,
        b: n_b,
        weight: 0.5,
    });
    tree.set_root(n_blend);
    let sampler = AnimSampler::new();
    let mut pose = Pose::from_bind_pose(&skel);
    tree.evaluate(&skel, &sampler, &mut pose).expect("evaluate");
    assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-4));
}

#[test]
fn two_bone_ik_reaches_target_within_chain() {
    // The simple-arm-skeleton has reach 2.0 (1 + 1). Target at (1, 1, 0)
    // is at distance sqrt(2) ≈ 1.41 ≤ 2 ⇒ reachable.
    let ik = TwoBoneIk::new(
        Vec3::ZERO,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
    );
    let target = Vec3::new(1.0, 1.0, 0.0);
    let (new_mid, new_tip) = ik.solve(target).expect("solve");
    assert!(vec3_approx_eq(new_tip, target, 1e-4));
    // Bone lengths preserved.
    let l1 = (new_mid - Vec3::ZERO).length();
    let l2 = (new_tip - new_mid).length();
    assert!(approx_eq(l1, 1.0, 1e-4));
    assert!(approx_eq(l2, 1.0, 1e-4));
}

#[test]
fn fabrik_solves_three_bone_reach() {
    let chain = FabrikChain::new(vec![
        Vec3::ZERO,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(3.0, 0.0, 0.0),
    ])
    .expect("ok");
    let mut chain = chain.with_max_iterations(64).with_epsilon(1e-4);
    let target = Vec3::new(2.0, 1.0, 0.0); // reachable inside total length 3.
    let outcome = chain.solve(target).expect("solve");
    assert!(outcome.converged, "FABRIK should converge");
    let tip = chain.joints[chain.joints.len() - 1];
    assert!(vec3_approx_eq(tip, target, 1e-3));
}

#[test]
fn animation_world_advances_through_clip_phase() {
    let mut world = AnimationWorld::new();
    let skel = build_simple_arm_skeleton();
    let s_id = world.register_skeleton(skel);
    let ch = AnimChannel::translation(
        1,
        Interpolation::Linear,
        vec![
            KeyframeT {
                time: 0.0,
                value: Vec3::ZERO,
            },
            KeyframeT {
                time: 1.0,
                value: Vec3::new(10.0, 0.0, 0.0),
            },
        ],
    )
    .expect("ok");
    let clip = AnimationClip::new("walk", vec![ch]);
    let _i_id = world
        .spawn_clip_instance(ClipInstance::new(s_id, clip))
        .expect("ok");

    // Tick five times by 0.1s ⇒ phase = 0.5 ⇒ elbow translation = 5.
    for _ in 0..5 {
        world.tick(0.1).expect("tick");
    }
    let pose = world.pose(s_id).expect("pose");
    assert!(approx_eq(pose.local_transforms[1].translation.x, 5.0, 1e-4));
}

#[test]
fn animation_world_omega_system_surface_intact() {
    let world = AnimationWorld::new();
    assert!(world.name().contains("AnimationWorld"));
    let row: EffectRow = world.effect_row();
    assert!(row.contains(SubstrateEffect::Sim));
}

#[test]
fn determinism_animation_world_replays_identically() {
    fn run(seed_pair: (u32, u32)) -> Vec<f32> {
        let _ = seed_pair; // animation determinism is by-construction; seed-pair unused.
        let mut world = AnimationWorld::new();
        let s_id = world.register_skeleton(build_simple_arm_skeleton());
        let ch = AnimChannel::translation(
            1,
            Interpolation::Linear,
            vec![
                KeyframeT {
                    time: 0.0,
                    value: Vec3::ZERO,
                },
                KeyframeT {
                    time: 1.0,
                    value: Vec3::new(10.0, 0.0, 0.0),
                },
            ],
        )
        .expect("ok");
        let clip = AnimationClip::new("walk", vec![ch]);
        world
            .spawn_clip_instance(ClipInstance::new(s_id, clip))
            .expect("ok");
        let mut samples = Vec::new();
        for _ in 0..20 {
            world.tick(0.05).expect("tick");
            let pose = world.pose(s_id).expect("pose");
            samples.push(pose.local_transforms[1].translation.x);
        }
        samples
    }
    let a = run((0, 0));
    let b = run((1, 1));
    assert_eq!(a, b, "animation must be replay-deterministic");
}

#[test]
fn skeletal_chain_topological_order_invariant_holds() {
    // Author leaf-first ; expect parent-first after construction.
    let bones = vec![
        Bone::new("hand", 1, Transform::IDENTITY),
        Bone::new("forearm", 2, Transform::IDENTITY),
        Bone::new("upper_arm", ROOT_PARENT, Transform::IDENTITY),
    ];
    let s = Skeleton::from_bones(bones).expect("must reorder");
    // After topological sort, root is at index 0.
    assert_eq!(s.bone(0).expect("root").name, "upper_arm");
    // Children must reference earlier indices.
    for (i, b) in s.bones().iter().enumerate() {
        if !b.is_root() {
            assert!(b.parent_idx < i);
        }
    }
}

#[test]
fn attestation_present() {
    assert!(cssl_anim::ATTESTATION.contains("no hurt nor harm"));
}

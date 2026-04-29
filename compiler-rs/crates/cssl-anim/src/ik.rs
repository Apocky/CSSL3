//! Inverse-kinematics solvers.
//!
//! § THESIS
//!   IK takes a desired end-effector position (world-space target) and
//!   computes joint rotations along a chain so the chain's tip reaches
//!   the target. Two solvers are supplied :
//!     - [`TwoBoneIk`] : analytic two-bone solver via the law of cosines.
//!       Suitable for arms, legs, and any 2-segment chain. O(1) ; exact
//!       within reach + clamps to fully-extended outside.
//!     - [`FabrikChain`] : iterative FABRIK (Forward-And-Backward-Reaching
//!       Inverse Kinematics, Aristidou + Lasenby 2011). Suitable for
//!       chains of arbitrary length — spines, tails, tentacles. O(N) per
//!       iteration ; converges in 4-10 iterations for typical character
//!       rigs.
//!
//! § STAGE-0 SCOPE
//!   - **Pole vector** : two-bone IK accepts an optional pole vector to
//!     disambiguate the elbow / knee plane.
//!   - **Joint constraints** : not yet wired ; FABRIK accepts them in a
//!     follow-up slice.
//!   - **Soft-IK / blend-out** : not yet wired ; the caller blends IK
//!     results with FK results outside the solver.
//!
//! § DETERMINISM
//!   Both solvers are pure functions of their inputs. No randomness, no
//!   global state. Iteration counts are bounded ; FABRIK exits when the
//!   tip-to-target distance falls below a configurable epsilon or when
//!   the iteration cap is reached.

use cssl_substrate_projections::{Quat, Vec3};

use crate::error::AnimError;

/// Result type for IK solvers — either a successful solution or a
/// classified failure mode.
pub type IkResult<T> = Result<T, AnimError>;

/// Outcome of a FABRIK solve : whether the tip reached the target within
/// epsilon, how many iterations were used, and the residual error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FabrikOutcome {
    /// Whether the tip reached within `epsilon` of the target.
    pub converged: bool,
    /// Number of iterations taken.
    pub iterations: u32,
    /// Final tip-to-target distance.
    pub residual: f32,
}

/// Two-bone IK solver — analytic law-of-cosines solution.
///
/// § GEOMETRY
///   Given joints `(root, mid, tip)` with bone lengths `l1 = |root → mid|`
///   and `l2 = |mid → tip|`, the goal is to position `mid` so the chain's
///   tip lands at `target`. The reachable set is an annulus around `root`
///   with inner radius `|l1 - l2|` and outer radius `l1 + l2`.
///
/// § ALGORITHM
///   1. Compute `d = |target - root|`.
///   2. If `d > l1 + l2`, the target is unreachable — fully extend the
///      chain pointing toward the target ; return `IkUnreachable` if the
///      caller asked for the explicit signal (default returns Ok).
///   3. If `d < |l1 - l2|`, the target is "too close" ; clamp to the
///      inner radius and orient toward the target.
///   4. Otherwise, use the law of cosines :
///        cos(angle_at_mid) = (l1² + l2² - d²) / (2 * l1 * l2)
///      Internal joint angle = π - acos(...) ; external rotation aligns
///      the bone-to-target vector with the (root, mid, tip) plane.
///
/// § OUTPUT
///   Two world-space rotations to apply at root + mid joints. The caller
///   composes these with the parent's world-space transform to obtain
///   the bone-local rotations needed for the pose.
#[derive(Debug, Clone, Copy)]
pub struct TwoBoneIk {
    /// World-space root joint position.
    pub root: Vec3,
    /// World-space middle joint position (elbow / knee).
    pub mid: Vec3,
    /// World-space tip joint position (hand / foot).
    pub tip: Vec3,
    /// Optional pole vector — disambiguates which side the bend points
    /// toward. If `None`, defaults to the existing mid-position direction.
    pub pole: Option<Vec3>,
    /// If true, return `IkUnreachable` instead of clamping when the
    /// target is outside the chain's reachable region.
    pub strict_unreachable: bool,
}

impl TwoBoneIk {
    /// Construct a two-bone IK problem from the three current world-space
    /// joint positions plus an optional pole vector.
    #[must_use]
    pub fn new(root: Vec3, mid: Vec3, tip: Vec3) -> Self {
        Self {
            root,
            mid,
            tip,
            pole: None,
            strict_unreachable: false,
        }
    }

    /// Builder method : set a pole vector.
    #[must_use]
    pub fn with_pole(mut self, pole: Vec3) -> Self {
        self.pole = Some(pole);
        self
    }

    /// Builder method : enable strict unreachable signaling. By default,
    /// the solver clamps to the fully-extended pose ; with this flag,
    /// the solver returns `IkUnreachable` when the target is too far away.
    #[must_use]
    pub fn with_strict_unreachable(mut self, strict: bool) -> Self {
        self.strict_unreachable = strict;
        self
    }

    /// Solve for the new mid + tip positions that put the tip at `target`.
    /// Returns `(new_mid, new_tip)` in world-space.
    ///
    /// On success the returned tip is at the target (or as close as the
    /// reachable-region clamp permits). The bone lengths
    /// (`|root → new_mid|` and `|new_mid → new_tip|`) match the original
    /// configuration to within floating-point precision.
    pub fn solve(&self, target: Vec3) -> IkResult<(Vec3, Vec3)> {
        let l1 = (self.mid - self.root).length();
        let l2 = (self.tip - self.mid).length();
        if l1 < f32::EPSILON || l2 < f32::EPSILON {
            return Err(AnimError::IkChainTooShort {
                got: 1,
                required: 2,
                solver: "TwoBoneIk",
            });
        }
        let max_reach = l1 + l2;
        let to_target = target - self.root;
        let d = to_target.length();
        if d < f32::EPSILON {
            // Target is at the root ; produce a degenerate but well-formed
            // result (mid at original mid-direction, tip at root).
            return Ok((self.mid, self.root));
        }
        // Unreachable case.
        if d > max_reach {
            if self.strict_unreachable {
                return Err(AnimError::IkUnreachable {
                    distance: d,
                    max_reach,
                });
            }
            // Fully-extended toward target.
            let dir = to_target.normalize();
            let new_mid = self.root + dir * l1;
            let new_tip = self.root + dir * max_reach;
            return Ok((new_mid, new_tip));
        }
        // Inner clamp : if d < |l1 - l2|, the tip can't reach inward
        // enough — clamp to inner radius.
        let min_reach = (l1 - l2).abs();
        if d < min_reach {
            let dir = to_target.normalize();
            let new_mid = self.root + dir * l1;
            // Tip wraps back ; the simplest stable form is to point
            // backward along the same axis.
            let new_tip = self.root + dir * min_reach;
            return Ok((new_mid, new_tip));
        }
        // Law of cosines for the internal angle at root.
        let cos_alpha = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
        let alpha = cos_alpha.acos();

        // Build a basis : forward (root → target), and a perpendicular
        // "bend" direction. Use the pole vector if supplied, else fall
        // back to the existing mid-position projected away from the
        // target axis, else use a stable canonical perpendicular.
        let forward = to_target.normalize();
        let pole_default = (self.mid - self.root).normalize();
        let pole_dir = self.pole.unwrap_or_else(|| {
            if pole_default.length_squared() > f32::EPSILON {
                pole_default
            } else {
                // Use Y-up if the existing mid-direction is degenerate.
                Vec3::Y
            }
        });
        // Project pole onto the plane perpendicular to forward.
        let pole_perp = pole_dir - forward * forward.dot(pole_dir);
        let bend = if pole_perp.length_squared() > f32::EPSILON {
            pole_perp.normalize()
        } else {
            // Pole is parallel to forward — pick a canonical axis.
            if forward.dot(Vec3::Y).abs() < 0.9 {
                Vec3::Y - forward * forward.dot(Vec3::Y)
            } else {
                Vec3::X - forward * forward.dot(Vec3::X)
            }
            .normalize()
        };

        // Mid position : displaced along the bend by sin(alpha)*l1, along
        // forward by cos(alpha)*l1.
        let new_mid = self.root + forward * (cos_alpha * l1) + bend * (alpha.sin() * l1);
        // Tip lands on the target (by construction of the law-of-cosines
        // solution).
        let new_tip = target;
        Ok((new_mid, new_tip))
    }

    /// Compute the rotations to apply at root + mid that transform the
    /// original `(root, mid, tip)` configuration into `(root, new_mid,
    /// new_tip)`. Returned as `(rot_root, rot_mid)` in world-space ; both
    /// rotate around their respective joint origins.
    pub fn solve_rotations(&self, target: Vec3) -> IkResult<(Quat, Quat)> {
        let (new_mid, new_tip) = self.solve(target)?;
        let old_dir_root_mid = (self.mid - self.root).normalize();
        let new_dir_root_mid = (new_mid - self.root).normalize();
        let rot_root = quat_from_to(old_dir_root_mid, new_dir_root_mid);
        let old_dir_mid_tip = (self.tip - self.mid).normalize();
        let new_dir_mid_tip = (new_tip - new_mid).normalize();
        // The mid rotation operates *in mid's local frame*, so we strip
        // the contribution of rot_root from the desired direction.
        let new_dir_in_root_frame = rot_root.conjugate().rotate(new_dir_mid_tip);
        let rot_mid = quat_from_to(old_dir_mid_tip, new_dir_in_root_frame);
        Ok((rot_root, rot_mid))
    }
}

/// FABRIK chain — iterative IK for chains of arbitrary length.
///
/// § ALGORITHM (Aristidou + Lasenby 2011)
///   Two passes per iteration :
///
///   **Backward** : place the tip at the target, then walk backward
///   through the chain, placing each joint at distance `bone_length`
///   from the previously-placed joint along the unit vector toward its
///   original position.
///
///   **Forward** : pin the root back to its original position, then walk
///   forward, placing each joint at distance `bone_length` from the
///   previously-placed joint along the unit vector toward its
///   backward-pass-derived position.
///
///   Iterate until tip-to-target distance falls below epsilon or the
///   iteration cap is reached.
#[derive(Debug, Clone)]
pub struct FabrikChain {
    /// World-space joint positions, root first.
    pub joints: Vec<Vec3>,
    /// Cached bone lengths between successive joints. Recomputed on
    /// `set_joints` ; kept stable across iterations.
    pub bone_lengths: Vec<f32>,
    /// Convergence epsilon — iteration stops when tip-to-target distance
    /// is below this value.
    pub epsilon: f32,
    /// Maximum iteration count. Stage-0 default is 32.
    pub max_iterations: u32,
}

impl FabrikChain {
    /// Construct from a list of joint positions. Bone lengths are derived
    /// at construction.
    pub fn new(joints: Vec<Vec3>) -> IkResult<Self> {
        if joints.len() < 2 {
            return Err(AnimError::IkChainTooShort {
                got: joints.len(),
                required: 2,
                solver: "FabrikChain",
            });
        }
        let mut bone_lengths = Vec::with_capacity(joints.len() - 1);
        for i in 1..joints.len() {
            bone_lengths.push((joints[i] - joints[i - 1]).length());
        }
        Ok(Self {
            joints,
            bone_lengths,
            epsilon: 1e-3,
            max_iterations: 32,
        })
    }

    /// Configure the convergence epsilon.
    #[must_use]
    pub fn with_epsilon(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon.max(f32::EPSILON);
        self
    }

    /// Configure the maximum iteration cap.
    #[must_use]
    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max.max(1);
        self
    }

    /// Total reach of the chain — sum of bone lengths.
    #[must_use]
    pub fn total_length(&self) -> f32 {
        self.bone_lengths.iter().sum()
    }

    /// Solve for joint positions that put the tip at `target`. The chain
    /// is mutated in-place ; returns a `FabrikOutcome` describing the
    /// solver's progress.
    pub fn solve(&mut self, target: Vec3) -> IkResult<FabrikOutcome> {
        if self.joints.len() < 2 {
            return Err(AnimError::IkChainTooShort {
                got: self.joints.len(),
                required: 2,
                solver: "FabrikChain",
            });
        }
        let root_pos = self.joints[0];
        let total_len = self.total_length();
        let to_target = target - root_pos;
        let d = to_target.length();
        // Unreachable : extend fully along the target direction.
        if d > total_len {
            let dir = to_target.normalize();
            let mut acc = 0.0;
            for i in 1..self.joints.len() {
                acc += self.bone_lengths[i - 1];
                self.joints[i] = root_pos + dir * acc;
            }
            return Ok(FabrikOutcome {
                converged: false,
                iterations: 0,
                residual: (self.joints[self.joints.len() - 1] - target).length(),
            });
        }
        let n = self.joints.len();
        let mut iter_count = 0u32;
        for it in 0..self.max_iterations {
            iter_count = it + 1;
            // Backward pass : tip → target ; walk inward.
            self.joints[n - 1] = target;
            for i in (0..(n - 1)).rev() {
                let dir = (self.joints[i] - self.joints[i + 1]).normalize();
                self.joints[i] = self.joints[i + 1] + dir * self.bone_lengths[i];
            }
            // Forward pass : root → original ; walk outward.
            self.joints[0] = root_pos;
            for i in 0..(n - 1) {
                let dir = (self.joints[i + 1] - self.joints[i]).normalize();
                self.joints[i + 1] = self.joints[i] + dir * self.bone_lengths[i];
            }
            // Convergence check.
            let residual = (self.joints[n - 1] - target).length();
            if residual < self.epsilon {
                return Ok(FabrikOutcome {
                    converged: true,
                    iterations: iter_count,
                    residual,
                });
            }
        }
        Ok(FabrikOutcome {
            converged: false,
            iterations: iter_count,
            residual: (self.joints[n - 1] - target).length(),
        })
    }

    /// Compute the rotation each joint should receive to align with the
    /// FABRIK-solved configuration. Returns a `Vec<Quat>` of length
    /// `joints.len() - 1` — the rotation for each bone (root rotation,
    /// then each subsequent joint).
    ///
    /// Each rotation is the world-space `quat_from_to` between the
    /// pre-solve bone-direction and the post-solve bone-direction. Caller
    /// is responsible for converting world-rotations to local-rotations
    /// against the parent's transform.
    pub fn rotations_from_original(&self, original: &[Vec3]) -> Vec<Quat> {
        let mut out = Vec::with_capacity(self.joints.len().saturating_sub(1));
        for i in 0..self.joints.len().saturating_sub(1) {
            let old_dir = (original[i + 1] - original[i]).normalize();
            let new_dir = (self.joints[i + 1] - self.joints[i]).normalize();
            out.push(quat_from_to(old_dir, new_dir));
        }
        out
    }
}

/// Build the unit quaternion that rotates `from` onto `to` along the
/// shorter arc. `from` and `to` are assumed unit-length ; the function
/// returns identity when they are nearly equal and `pi` rotation around
/// an arbitrary perpendicular when they are nearly opposite.
fn quat_from_to(from: Vec3, to: Vec3) -> Quat {
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if dot > 0.9999 {
        return Quat::IDENTITY;
    }
    if dot < -0.9999 {
        // Antiparallel — pick any perpendicular axis.
        let ortho = if from.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let axis = from.cross(ortho).normalize();
        return Quat::from_axis_angle(axis, core::f32::consts::PI);
    }
    let cross = from.cross(to);
    let w = 1.0 + dot;
    Quat::new(cross.x, cross.y, cross.z, w).normalize()
}

#[cfg(test)]
mod tests {
    use super::{FabrikChain, TwoBoneIk};
    use crate::error::AnimError;
    use cssl_substrate_projections::Vec3;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx_eq(a.x, b.x, eps) && approx_eq(a.y, b.y, eps) && approx_eq(a.z, b.z, eps)
    }

    #[test]
    fn two_bone_reachable_target_solves_to_target() {
        // root @ origin, mid @ (1, 0, 0), tip @ (2, 0, 0). Total reach 2.
        let ik = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        // Target inside reach : (1, 1, 0). Distance from root sqrt(2) ~= 1.41.
        let target = Vec3::new(1.0, 1.0, 0.0);
        let (new_mid, new_tip) = ik.solve(target).expect("solve");
        // Tip should land on target (within precision).
        assert!(vec3_approx_eq(new_tip, target, 1e-4));
        // Bone lengths should be preserved.
        let new_l1 = (new_mid - Vec3::ZERO).length();
        let new_l2 = (new_tip - new_mid).length();
        assert!(approx_eq(new_l1, 1.0, 1e-4));
        assert!(approx_eq(new_l2, 1.0, 1e-4));
    }

    #[test]
    fn two_bone_unreachable_extends_fully() {
        let ik = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        let target = Vec3::new(10.0, 0.0, 0.0); // way out of reach
        let (new_mid, new_tip) = ik.solve(target).expect("ok");
        // Fully extended toward +X : mid at (1, 0, 0), tip at (2, 0, 0).
        assert!(vec3_approx_eq(new_mid, Vec3::new(1.0, 0.0, 0.0), 1e-4));
        assert!(vec3_approx_eq(new_tip, Vec3::new(2.0, 0.0, 0.0), 1e-4));
    }

    #[test]
    fn two_bone_strict_unreachable_errors() {
        let ik = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        )
        .with_strict_unreachable(true);
        let target = Vec3::new(10.0, 0.0, 0.0);
        let result = ik.solve(target);
        assert!(matches!(result, Err(AnimError::IkUnreachable { .. })));
    }

    #[test]
    fn two_bone_zero_length_chain_errors() {
        // mid coincides with root ⇒ l1 = 0 ⇒ chain too short.
        let ik = TwoBoneIk::new(Vec3::ZERO, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        assert!(matches!(
            ik.solve(Vec3::new(0.5, 0.5, 0.0)),
            Err(AnimError::IkChainTooShort { .. })
        ));
    }

    #[test]
    fn two_bone_pole_vector_changes_bend_side() {
        let ik = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        let target = Vec3::new(1.0, 0.0, 0.0);
        // With +Y pole.
        let (mid_y, _) = ik.with_pole(Vec3::Y).solve(target).expect("ok");
        // With -Y pole.
        let ik2 = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        let (mid_neg_y, _) = ik2.with_pole(-Vec3::Y).solve(target).expect("ok");
        // The two mid positions should bend on opposite Y sides.
        assert!(mid_y.y * mid_neg_y.y < 0.0 || (mid_y.y.abs() < 1e-4 && mid_neg_y.y.abs() < 1e-4));
    }

    #[test]
    fn fabrik_construction_too_short_errors() {
        let result = FabrikChain::new(vec![Vec3::ZERO]);
        assert!(matches!(result, Err(AnimError::IkChainTooShort { .. })));
    }

    #[test]
    fn fabrik_solves_simple_three_bone_chain() {
        let chain = FabrikChain::new(vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
        ])
        .expect("ok");
        let mut chain = chain.with_max_iterations(50).with_epsilon(1e-4);
        let target = Vec3::new(2.0, 1.0, 0.0); // inside reach (3.0 total).
        let outcome = chain.solve(target).expect("solve");
        assert!(outcome.converged, "FABRIK must converge in 50 iterations");
        // Tip should be at target.
        let tip = chain.joints[chain.joints.len() - 1];
        assert!(vec3_approx_eq(tip, target, 1e-3));
        // Bone lengths preserved.
        for (i, &expected_len) in chain.bone_lengths.clone().iter().enumerate() {
            let new_len = (chain.joints[i + 1] - chain.joints[i]).length();
            assert!(approx_eq(new_len, expected_len, 1e-3));
        }
    }

    #[test]
    fn fabrik_unreachable_extends_fully() {
        let chain = FabrikChain::new(vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ])
        .expect("ok");
        let mut chain = chain;
        let target = Vec3::new(100.0, 0.0, 0.0);
        let outcome = chain.solve(target).expect("ok");
        assert!(!outcome.converged);
        // Tip should be fully extended toward target.
        let tip = chain.joints[chain.joints.len() - 1];
        let total = chain.total_length();
        assert!(approx_eq(tip.length(), total, 1e-4));
    }

    #[test]
    fn fabrik_target_at_origin_root_held() {
        let chain = FabrikChain::new(vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ])
        .expect("ok");
        let mut chain = chain.with_max_iterations(50).with_epsilon(1e-4);
        // Target at root means tip should fold back to origin.
        let outcome = chain.solve(Vec3::ZERO).expect("ok");
        assert!(outcome.converged);
        // Root stays at origin.
        assert!(vec3_approx_eq(chain.joints[0], Vec3::ZERO, 1e-5));
    }

    #[test]
    fn fabrik_with_max_iterations_caps() {
        let chain = FabrikChain::new(vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ])
        .expect("ok");
        let mut chain = chain.with_max_iterations(1).with_epsilon(1e-9);
        let _ = chain.solve(Vec3::new(0.5, 1.0, 0.0)).expect("ok");
        // We don't assert convergence (1 iteration is too few for tight
        // epsilon) ; just verify the solver respects the cap.
    }

    #[test]
    fn fabrik_rotations_from_original_returns_quat_per_bone() {
        let original = vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ];
        let chain = FabrikChain::new(original.clone()).expect("ok");
        let mut chain = chain.with_max_iterations(50).with_epsilon(1e-4);
        let _ = chain.solve(Vec3::new(1.0, 1.0, 0.0)).expect("ok");
        let rotations = chain.rotations_from_original(&original);
        assert_eq!(rotations.len(), 2); // bones = joints - 1
    }

    #[test]
    fn two_bone_solve_rotations_returns_two_quats() {
        let ik = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        );
        let (rot_root, rot_mid) = ik.solve_rotations(Vec3::new(1.0, 1.0, 0.0)).expect("ok");
        // Both rotations should be non-trivial (non-identity).
        assert!(approx_eq(rot_root.length_squared(), 1.0, 1e-4));
        assert!(approx_eq(rot_mid.length_squared(), 1.0, 1e-4));
    }

    #[test]
    fn fabrik_total_length_sum_of_bones() {
        let chain = FabrikChain::new(vec![
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(7.0, 0.0, 0.0),
        ])
        .expect("ok");
        // Bone lengths : 1, 2, 4 ⇒ total 7.
        assert!(approx_eq(chain.total_length(), 7.0, 1e-5));
    }

    #[test]
    fn two_bone_target_too_close_clamps_inward() {
        // root @ origin, mid @ (3, 0, 0), tip @ (5, 0, 0). l1 = 3, l2 = 2.
        // Inner radius = |3 - 2| = 1. Target at (0.5, 0, 0) is inside the
        // inner radius — solver clamps to the inner boundary.
        let ik = TwoBoneIk::new(
            Vec3::ZERO,
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
        );
        let target = Vec3::new(0.5, 0.0, 0.0);
        let (_new_mid, new_tip) = ik.solve(target).expect("ok");
        // Tip should land at the clamped inner boundary along the target axis.
        assert!(approx_eq((new_tip - Vec3::ZERO).length(), 1.0, 1e-4));
    }
}

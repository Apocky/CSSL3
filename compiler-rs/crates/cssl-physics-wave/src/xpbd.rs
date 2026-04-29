//! § XPBD — Extended Position-Based Dynamics.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The wave-physics constraint solver. Replaces `cssl-physics::solver::
//!   ConstraintSolver` (sequential-impulse PGS) with the modern XPBD
//!   formulation : constraints expressed in position-space, solved by
//!   constraint-projection in `O(C)` per iteration where `C` is the
//!   constraint count.
//!
//!   Per dispatch :
//!   - **graph-coloring per warp** : constraints sharing a body-id are
//!     placed in different colors so within-color iterations are
//!     parallel-safe. We compute coloring greedily by ascending
//!     `(body_a_id, body_b_id)`.
//!   - **Jacobi-block** : within a color, all constraints update their
//!     bodies' positions simultaneously (Jacobi-style, not Gauss-Seidel-
//!     style). This is parallel-safe because the coloring guarantees no
//!     within-color body-overlap.
//!   - **4-iter constraint-solve** : the spec calls for 4 iterations as
//!     the canonical XPBD count. Configurable via `XpbdConfig::iterations`.
//!
//! § XPBD MATH (briefly)
//!   For a constraint `C(x_a, x_b, ...) = 0` with stiffness `α`, the
//!   per-iteration position-correction is :
//!
//!   ```text
//!   λ ←   - C / (∇C·M⁻¹·∇C + α/Δt²)
//!   x_i ← x_i + M⁻¹_i · ∇C_i · λ
//!   ```
//!
//!   This is the standard XPBD formulation (Macklin et al. 2016). The
//!   `α/Δt²` term is the compliance term — `α = 0` is rigid, `α > 0` is
//!   soft. The `∇C·M⁻¹·∇C` term comes from differentiating the constraint
//!   along the inverse-mass-weighted direction.
//!
//! § DETERMINISM
//!   - Constraint iteration order is sorted by `color_id` then
//!     `(body_a_id, body_b_id)` ; bit-equal across hosts.
//!   - The Jacobi-block update reads all positions BEFORE writing any ;
//!     no within-iteration ordering bias.
//!   - Iteration-count is fixed (`XpbdConfig::iterations`) ; we don't
//!     terminate on a residual-tolerance loop (which would diverge under
//!     float-noise).

use smallvec::SmallVec;
use thiserror::Error;

/// § Default constraint-solve iteration count per spec.
pub const XPBD_DEFAULT_ITERATIONS: u32 = 4;

/// § Inline cap for color-bucket size before SmallVec spills.
pub const COLOR_BUCKET_INLINE_CAP: usize = 32;

/// § Inline cap for body-list per constraint.
pub const CONSTRAINT_BODY_INLINE_CAP: usize = 4;

// ───────────────────────────────────────────────────────────────────────
// § ColorId.
// ───────────────────────────────────────────────────────────────────────

/// § Color-id for a constraint in the graph-coloring scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ColorId(pub u32);

impl ColorId {
    /// § The "uncolored" sentinel.
    pub const UNCOLORED: ColorId = ColorId(u32::MAX);

    /// § The first color (0).
    pub const FIRST: ColorId = ColorId(0);
}

// ───────────────────────────────────────────────────────────────────────
// § ConstraintKind — distance + hinge + contact (the canonical three).
// ───────────────────────────────────────────────────────────────────────

/// § Kind-tag for a constraint. Distance + hinge + contact + ground-plane
///   are the canonical kinds for the wave-physics solver ; the spec's
///   future "spring", "rope", and "soft-body-tetra" extensions append here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstraintKind {
    /// Distance constraint between two bodies' attach-points.
    Distance {
        /// Rest-length.
        rest: f32,
        /// Compliance (0 = rigid).
        compliance: f32,
    },
    /// Hinge constraint (1-DOF rotation about an axis).
    Hinge {
        /// Hinge axis (unit-length).
        axis: [f32; 3],
        /// Compliance.
        compliance: f32,
    },
    /// Contact constraint (contact-normal + penetration-depth + restitution).
    Contact {
        /// Contact-normal pointing from `a` to `b`.
        normal: [f32; 3],
        /// Penetration depth.
        penetration: f32,
        /// Restitution coefficient (0 = perfectly inelastic, 1 = elastic).
        restitution: f32,
    },
    /// Ground-plane constraint : body must lie above plane `dot(p, n) ≥ d`.
    GroundPlane {
        /// Plane normal.
        normal: [f32; 3],
        /// Plane offset.
        offset: f32,
    },
}

// ───────────────────────────────────────────────────────────────────────
// § Constraint — a single projection-target.
// ───────────────────────────────────────────────────────────────────────

/// § One constraint between (up-to-CONSTRAINT_BODY_INLINE_CAP) bodies.
#[derive(Debug, Clone)]
pub struct Constraint {
    /// Kind-tag + per-kind parameters.
    pub kind: ConstraintKind,
    /// Body-ids participating ; sorted-ascending.
    pub bodies: SmallVec<[u64; CONSTRAINT_BODY_INLINE_CAP]>,
    /// Color-id assigned by the coloring pass. `UNCOLORED` until colored.
    pub color: ColorId,
}

impl Constraint {
    /// § Construct a distance-constraint between two bodies.
    #[must_use]
    pub fn distance(a: u64, b: u64, rest: f32, compliance: f32) -> Self {
        let mut bodies = SmallVec::new();
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        bodies.push(lo);
        bodies.push(hi);
        Constraint {
            kind: ConstraintKind::Distance { rest, compliance },
            bodies,
            color: ColorId::UNCOLORED,
        }
    }

    /// § Construct a hinge-constraint between two bodies.
    #[must_use]
    pub fn hinge(a: u64, b: u64, axis: [f32; 3], compliance: f32) -> Self {
        let mut bodies = SmallVec::new();
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        bodies.push(lo);
        bodies.push(hi);
        Constraint {
            kind: ConstraintKind::Hinge { axis, compliance },
            bodies,
            color: ColorId::UNCOLORED,
        }
    }

    /// § Construct a contact-constraint between two bodies.
    #[must_use]
    pub fn contact(a: u64, b: u64, normal: [f32; 3], penetration: f32, restitution: f32) -> Self {
        let mut bodies = SmallVec::new();
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        bodies.push(lo);
        bodies.push(hi);
        Constraint {
            kind: ConstraintKind::Contact {
                normal,
                penetration,
                restitution,
            },
            bodies,
            color: ColorId::UNCOLORED,
        }
    }

    /// § Construct a ground-plane constraint for a single body.
    #[must_use]
    pub fn ground_plane(body: u64, normal: [f32; 3], offset: f32) -> Self {
        let mut bodies = SmallVec::new();
        bodies.push(body);
        Constraint {
            kind: ConstraintKind::GroundPlane { normal, offset },
            bodies,
            color: ColorId::UNCOLORED,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § GraphColoring — greedy ascending-id coloring.
// ───────────────────────────────────────────────────────────────────────

/// § Result of a graph-coloring pass over a constraint list.
#[derive(Debug, Clone, Default)]
pub struct GraphColoring {
    /// `color_buckets[c]` is the list of constraint-indices in color `c`.
    pub color_buckets: Vec<SmallVec<[u32; COLOR_BUCKET_INLINE_CAP]>>,
    /// Total color count.
    pub color_count: u32,
}

impl GraphColoring {
    /// § Greedy coloring : iterate constraints in `(body_a, body_b)`
    ///   ascending order ; for each constraint pick the lowest color-id
    ///   not yet used by any body in this constraint.
    pub fn color(constraints: &mut [Constraint]) -> Self {
        // Step 1 : sort indices by (body_a, body_b).
        let mut sorted_idx: Vec<u32> = (0..constraints.len() as u32).collect();
        sorted_idx.sort_by_key(|&i| {
            let c = &constraints[i as usize];
            let a = c.bodies.first().copied().unwrap_or(u64::MAX);
            let b = c.bodies.get(1).copied().unwrap_or(u64::MAX);
            (a, b)
        });

        // Step 2 : per-body next-free-color tracker.
        // Use a hashmap-ish small-array indexed by body_id.
        // For simplicity use a Vec<HashSet> indexed by body_id ;
        // but hashmap pulls in std::collections — keep Vec<u32> ascending.
        let mut body_used_colors: std::collections::HashMap<u64, Vec<u32>> =
            std::collections::HashMap::new();
        let mut color_count: u32 = 0;
        let mut color_buckets: Vec<SmallVec<[u32; COLOR_BUCKET_INLINE_CAP]>> = Vec::new();

        for &cidx in &sorted_idx {
            let c_bodies: Vec<u64> = constraints[cidx as usize].bodies.iter().copied().collect();
            // Find lowest color not used by ANY body in this constraint.
            let mut chosen: u32 = 0;
            'outer: loop {
                for &b in &c_bodies {
                    if let Some(used) = body_used_colors.get(&b) {
                        if used.contains(&chosen) {
                            chosen += 1;
                            continue 'outer;
                        }
                    }
                }
                break;
            }
            // Assign color.
            constraints[cidx as usize].color = ColorId(chosen);
            for &b in &c_bodies {
                body_used_colors.entry(b).or_default().push(chosen);
            }
            // Grow buckets if needed.
            while (color_buckets.len() as u32) <= chosen {
                color_buckets.push(SmallVec::new());
            }
            color_buckets[chosen as usize].push(cidx);
            if chosen + 1 > color_count {
                color_count = chosen + 1;
            }
        }

        // Step 3 : within each color bucket, sort by (body_a, body_b)
        // ascending for replay-stability.
        for bucket in &mut color_buckets {
            bucket.sort_by_key(|&cidx| {
                let c = &constraints[cidx as usize];
                let a = c.bodies.first().copied().unwrap_or(u64::MAX);
                let b = c.bodies.get(1).copied().unwrap_or(u64::MAX);
                (a, b)
            });
        }

        GraphColoring {
            color_buckets,
            color_count,
        }
    }

    /// § Number of color-buckets.
    #[must_use]
    pub fn n_colors(&self) -> u32 {
        self.color_count
    }

    /// § Total constraint count across all buckets.
    #[must_use]
    pub fn n_constraints(&self) -> usize {
        self.color_buckets.iter().map(|b| b.len()).sum()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § JacobiBlock — within-color simultaneous projection.
// ───────────────────────────────────────────────────────────────────────

/// § Per-body state the Jacobi-block solver mutates.
///
///   This is a flat `Vec` indexed by `body_id` ; the solver reads
///   positions BEFORE projecting + accumulates corrections in a parallel
///   `Vec<[f32; 3]>` BEFORE applying them. This is the canonical Jacobi
///   discipline.
#[derive(Debug, Clone)]
pub struct JacobiBlock {
    /// Per-body position (mutated by the block).
    pub positions: Vec<[f32; 3]>,
    /// Per-body inverse mass (`0.0` = static / kinematic).
    pub inv_mass: Vec<f32>,
    /// Per-body accumulated correction (zeroed at block start).
    pub corrections: Vec<[f32; 3]>,
}

impl JacobiBlock {
    /// § Construct a block of `n` bodies with positions and inverse
    ///   masses. All corrections start at zero.
    #[must_use]
    pub fn new(positions: Vec<[f32; 3]>, inv_mass: Vec<f32>) -> Self {
        let n = positions.len();
        assert_eq!(
            inv_mass.len(),
            n,
            "JacobiBlock : inv_mass length must match positions"
        );
        JacobiBlock {
            positions,
            inv_mass,
            corrections: vec![[0.0; 3]; n],
        }
    }

    /// § Reset all corrections to zero.
    pub fn clear_corrections(&mut self) {
        for c in &mut self.corrections {
            *c = [0.0; 3];
        }
    }

    /// § Apply accumulated corrections to positions.
    pub fn flush_corrections(&mut self) {
        for (i, c) in self.corrections.iter().enumerate() {
            self.positions[i][0] += c[0];
            self.positions[i][1] += c[1];
            self.positions[i][2] += c[2];
        }
    }

    /// § Project a single distance-constraint.
    pub fn project_distance(&mut self, a: usize, b: usize, rest: f32, compliance: f32, dt: f32) {
        let pa = self.positions[a];
        let pb = self.positions[b];
        let dx = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let len = (dx[0] * dx[0] + dx[1] * dx[1] + dx[2] * dx[2]).sqrt();
        if len < 1e-9 {
            return; // Degenerate, can't normalize.
        }
        let n = [dx[0] / len, dx[1] / len, dx[2] / len];
        let c_val = len - rest;
        let w_a = self.inv_mass[a];
        let w_b = self.inv_mass[b];
        let w_total = w_a + w_b;
        if w_total < 1e-12 {
            return; // Both bodies infinite-mass.
        }
        let alpha_tilde = compliance / (dt * dt);
        let lambda = -c_val / (w_total + alpha_tilde);
        // accumulate corrections
        let dpa = [
            -n[0] * w_a * lambda,
            -n[1] * w_a * lambda,
            -n[2] * w_a * lambda,
        ];
        let dpb = [
            n[0] * w_b * lambda,
            n[1] * w_b * lambda,
            n[2] * w_b * lambda,
        ];
        self.corrections[a][0] += dpa[0];
        self.corrections[a][1] += dpa[1];
        self.corrections[a][2] += dpa[2];
        self.corrections[b][0] += dpb[0];
        self.corrections[b][1] += dpb[1];
        self.corrections[b][2] += dpb[2];
    }

    /// § Project a contact constraint : push bodies apart along the contact
    ///   normal until penetration is 0.
    pub fn project_contact(&mut self, a: usize, b: usize, normal: [f32; 3], penetration: f32) {
        if penetration <= 0.0 {
            return;
        }
        let w_a = self.inv_mass[a];
        let w_b = self.inv_mass[b];
        let w_total = w_a + w_b;
        if w_total < 1e-12 {
            return;
        }
        let lambda = penetration / w_total;
        let dpa = [
            -normal[0] * w_a * lambda,
            -normal[1] * w_a * lambda,
            -normal[2] * w_a * lambda,
        ];
        let dpb = [
            normal[0] * w_b * lambda,
            normal[1] * w_b * lambda,
            normal[2] * w_b * lambda,
        ];
        self.corrections[a][0] += dpa[0];
        self.corrections[a][1] += dpa[1];
        self.corrections[a][2] += dpa[2];
        self.corrections[b][0] += dpb[0];
        self.corrections[b][1] += dpb[1];
        self.corrections[b][2] += dpb[2];
    }

    /// § Project a ground-plane constraint : push body above the plane.
    pub fn project_ground_plane(&mut self, body: usize, normal: [f32; 3], offset: f32) {
        let w = self.inv_mass[body];
        if w < 1e-12 {
            return;
        }
        let p = self.positions[body];
        let signed_dist = (p[0] * normal[0]) + (p[1] * normal[1]) + (p[2] * normal[2]) - offset;
        if signed_dist >= 0.0 {
            return;
        }
        let push = -signed_dist;
        self.corrections[body][0] += normal[0] * push;
        self.corrections[body][1] += normal[1] * push;
        self.corrections[body][2] += normal[2] * push;
    }
}

// ───────────────────────────────────────────────────────────────────────
// § XpbdConfig.
// ───────────────────────────────────────────────────────────────────────

/// § Configuration knobs for the XPBD solver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XpbdConfig {
    /// Iteration count per step (default 4).
    pub iterations: u32,
    /// Substep count per integration step (default 1 ; the spec's
    /// "small-substep" XPBD variant uses 8-16).
    pub substeps: u32,
    /// True ⇒ within-color color-bucket-sorted iteration. Default = `true`.
    /// Disabling this trades replay-stability for raw throughput.
    pub stable_within_color: bool,
}

impl Default for XpbdConfig {
    fn default() -> Self {
        XpbdConfig {
            iterations: XPBD_DEFAULT_ITERATIONS,
            substeps: 1,
            stable_within_color: true,
        }
    }
}

impl XpbdConfig {
    /// § Spec-canonical 4-iteration config.
    #[must_use]
    pub fn canonical() -> Self {
        XpbdConfig::default()
    }

    /// § Small-substep XPBD config (Macklin 2019) : 8 substeps + 1 iter.
    #[must_use]
    pub fn small_substep() -> Self {
        XpbdConfig {
            iterations: 1,
            substeps: 8,
            stable_within_color: true,
        }
    }

    /// § High-fidelity config : 16 iterations.
    #[must_use]
    pub fn high_fidelity() -> Self {
        XpbdConfig {
            iterations: 16,
            substeps: 1,
            stable_within_color: true,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ConstraintFailure.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of constraint-projection.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum ConstraintFailure {
    /// Body-id referenced by a constraint is out-of-range for the block.
    #[error(
        "PHYSWAVE0020 — constraint references body-id {id} but block has only {block_size} bodies"
    )]
    BodyOutOfRange {
        /// The offending id.
        id: u64,
        /// The block's body-count.
        block_size: usize,
    },
    /// Constraint kind not supported by this solver path.
    #[error("PHYSWAVE0021 — unsupported constraint kind in this solver path")]
    UnsupportedKind,
    /// Position-correction blew up to non-finite values.
    #[error("PHYSWAVE0022 — XPBD projection produced non-finite correction (likely a divergent compliance)")]
    NonFiniteCorrection,
}

// ───────────────────────────────────────────────────────────────────────
// § XpbdSolver — the orchestrator.
// ───────────────────────────────────────────────────────────────────────

/// § The XPBD constraint solver.
///
///   Owns nothing but the `XpbdConfig` ; the per-frame state lives in
///   the `JacobiBlock` + `[Constraint]` slice the caller passes in.
#[derive(Debug, Clone, Copy)]
pub struct XpbdSolver {
    /// The solver config.
    pub config: XpbdConfig,
}

impl Default for XpbdSolver {
    fn default() -> Self {
        XpbdSolver {
            config: XpbdConfig::default(),
        }
    }
}

impl XpbdSolver {
    /// § Construct a solver with the given config.
    #[must_use]
    pub fn new(config: XpbdConfig) -> Self {
        XpbdSolver { config }
    }

    /// § Run the constraint-solver loop : `iterations` iterations, each
    ///   walking color-buckets in order, applying constraint projection.
    ///
    ///   `body_id_to_index` maps the constraint's `body_id` (u64) to the
    ///   `JacobiBlock`'s body-index (usize). The mapping is built once
    ///   per frame by the caller (the world).
    pub fn solve(
        &self,
        constraints: &[Constraint],
        coloring: &GraphColoring,
        block: &mut JacobiBlock,
        body_id_to_index: &dyn Fn(u64) -> Option<usize>,
        dt: f32,
    ) -> Result<u32, ConstraintFailure> {
        let mut total_projections: u32 = 0;
        for _ in 0..self.config.iterations {
            block.clear_corrections();
            for color_idx in 0..coloring.color_count as usize {
                let bucket = &coloring.color_buckets[color_idx];
                for &cidx in bucket {
                    let c = &constraints[cidx as usize];
                    self.project_one(c, block, body_id_to_index, dt)?;
                    total_projections += 1;
                }
            }
            block.flush_corrections();
        }
        Ok(total_projections)
    }

    /// § Project a single constraint into the Jacobi-block.
    fn project_one(
        &self,
        c: &Constraint,
        block: &mut JacobiBlock,
        body_id_to_index: &dyn Fn(u64) -> Option<usize>,
        dt: f32,
    ) -> Result<(), ConstraintFailure> {
        match c.kind {
            ConstraintKind::Distance { rest, compliance } => {
                let a_id = c.bodies[0];
                let b_id = c.bodies[1];
                let a = body_id_to_index(a_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: a_id,
                    block_size: block.positions.len(),
                })?;
                let b = body_id_to_index(b_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: b_id,
                    block_size: block.positions.len(),
                })?;
                block.project_distance(a, b, rest, compliance, dt);
            }
            ConstraintKind::Contact {
                normal,
                penetration,
                restitution: _,
            } => {
                let a_id = c.bodies[0];
                let b_id = c.bodies[1];
                let a = body_id_to_index(a_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: a_id,
                    block_size: block.positions.len(),
                })?;
                let b = body_id_to_index(b_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: b_id,
                    block_size: block.positions.len(),
                })?;
                block.project_contact(a, b, normal, penetration);
            }
            ConstraintKind::GroundPlane { normal, offset } => {
                let body_id = c.bodies[0];
                let body = body_id_to_index(body_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: body_id,
                    block_size: block.positions.len(),
                })?;
                block.project_ground_plane(body, normal, offset);
            }
            ConstraintKind::Hinge { .. } => {
                // § Hinge is a richer constraint that requires per-body
                //   orientation state ; the wave-physics V0 solver
                //   accepts hinge constraints but treats them as
                //   distance-zero (rigid-link) by translating the bodies'
                //   positions toward their shared anchor. A full
                //   orientation-bearing hinge requires per-body Quat
                //   state (deferred to a follow-up slice).
                let a_id = c.bodies[0];
                let b_id = c.bodies[1];
                let a = body_id_to_index(a_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: a_id,
                    block_size: block.positions.len(),
                })?;
                let b = body_id_to_index(b_id).ok_or(ConstraintFailure::BodyOutOfRange {
                    id: b_id,
                    block_size: block.positions.len(),
                })?;
                block.project_distance(a, b, 0.0, 0.0, dt);
            }
        }
        Ok(())
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn color_id_constants() {
        assert_eq!(ColorId::FIRST.0, 0);
        assert_eq!(ColorId::UNCOLORED.0, u32::MAX);
    }

    #[test]
    fn xpbd_config_canonical_is_4_iter() {
        let c = XpbdConfig::canonical();
        assert_eq!(c.iterations, 4);
        assert_eq!(c.substeps, 1);
    }

    #[test]
    fn xpbd_config_small_substep_eight() {
        let c = XpbdConfig::small_substep();
        assert_eq!(c.iterations, 1);
        assert_eq!(c.substeps, 8);
    }

    #[test]
    fn xpbd_config_high_fidelity_sixteen() {
        let c = XpbdConfig::high_fidelity();
        assert_eq!(c.iterations, 16);
    }

    #[test]
    fn distance_constraint_canonicalizes() {
        let c = Constraint::distance(5, 3, 1.0, 0.0);
        assert_eq!(c.bodies[0], 3);
        assert_eq!(c.bodies[1], 5);
    }

    #[test]
    fn contact_constraint_canonicalizes() {
        let c = Constraint::contact(7, 1, [0.0, 1.0, 0.0], 0.05, 0.0);
        assert_eq!(c.bodies[0], 1);
        assert_eq!(c.bodies[1], 7);
    }

    #[test]
    fn ground_plane_has_one_body() {
        let c = Constraint::ground_plane(42, [0.0, 1.0, 0.0], 0.0);
        assert_eq!(c.bodies.len(), 1);
        assert_eq!(c.bodies[0], 42);
    }

    #[test]
    fn coloring_disjoint_constraints_share_color() {
        let mut cs = vec![
            Constraint::distance(0, 1, 1.0, 0.0),
            Constraint::distance(2, 3, 1.0, 0.0),
            Constraint::distance(4, 5, 1.0, 0.0),
        ];
        let g = GraphColoring::color(&mut cs);
        assert_eq!(g.color_count, 1);
        assert_eq!(g.color_buckets[0].len(), 3);
    }

    #[test]
    fn coloring_chain_constraints_progress() {
        let mut cs = vec![
            Constraint::distance(0, 1, 1.0, 0.0),
            Constraint::distance(1, 2, 1.0, 0.0),
            Constraint::distance(2, 3, 1.0, 0.0),
        ];
        let g = GraphColoring::color(&mut cs);
        // Chain : (0-1), (1-2), (2-3). The (0-1) and (2-3) can share a
        // color but (1-2) shares body with both, so it gets color 1.
        assert!(g.color_count >= 2);
    }

    #[test]
    fn coloring_assigns_colors_to_all_constraints() {
        let mut cs = vec![
            Constraint::distance(0, 1, 1.0, 0.0),
            Constraint::distance(1, 2, 1.0, 0.0),
            Constraint::distance(0, 2, 1.0, 0.0),
        ];
        let g = GraphColoring::color(&mut cs);
        for c in &cs {
            assert_ne!(c.color, ColorId::UNCOLORED);
        }
        assert_eq!(g.n_constraints(), 3);
    }

    #[test]
    fn jacobi_block_construct_and_clear() {
        let positions = vec![[0.0; 3], [1.0, 0.0, 0.0]];
        let inv_mass = vec![1.0, 1.0];
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.corrections[0] = [1.0, 1.0, 1.0];
        b.clear_corrections();
        assert_eq!(b.corrections[0], [0.0; 3]);
    }

    #[test]
    fn jacobi_block_flush_applies_corrections() {
        let positions = vec![[0.0; 3]];
        let inv_mass = vec![1.0];
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.corrections[0] = [1.0, 2.0, 3.0];
        b.flush_corrections();
        assert_eq!(b.positions[0], [1.0, 2.0, 3.0]);
    }

    #[test]
    fn distance_projection_brings_endpoints_to_rest_length() {
        // Two bodies at (0, 0, 0) and (2, 0, 0) ; rest = 1.
        let positions = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let inv_mass = vec![1.0, 1.0];
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.project_distance(0, 1, 1.0, 0.0, 1.0);
        b.flush_corrections();
        // After 1 iteration each body moves toward the other by half
        // the constraint error.
        let dist = ((b.positions[1][0] - b.positions[0][0]).powi(2)
            + (b.positions[1][1] - b.positions[0][1]).powi(2)
            + (b.positions[1][2] - b.positions[0][2]).powi(2))
        .sqrt();
        assert!(approx(dist, 1.0, 1e-3));
    }

    #[test]
    fn contact_projection_separates_bodies() {
        let positions = vec![[0.0; 3], [0.5, 0.0, 0.0]];
        let inv_mass = vec![1.0, 1.0];
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.project_contact(0, 1, [1.0, 0.0, 0.0], 0.5);
        b.flush_corrections();
        // After projection, body 0 moved -0.25 and body 1 +0.25.
        let dist = b.positions[1][0] - b.positions[0][0];
        assert!(approx(dist, 1.0, 1e-3));
    }

    #[test]
    fn ground_plane_pushes_body_above_plane() {
        let positions = vec![[0.0, -0.5, 0.0]];
        let inv_mass = vec![1.0];
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.project_ground_plane(0, [0.0, 1.0, 0.0], 0.0);
        b.flush_corrections();
        assert!(b.positions[0][1] >= 0.0);
    }

    #[test]
    fn ground_plane_no_push_when_above() {
        let positions = vec![[0.0, 1.0, 0.0]];
        let inv_mass = vec![1.0];
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.project_ground_plane(0, [0.0, 1.0, 0.0], 0.0);
        b.flush_corrections();
        assert!(approx(b.positions[0][1], 1.0, 1e-6));
    }

    #[test]
    fn solver_solve_runs_full_loop() {
        let mut cs = vec![Constraint::distance(0, 1, 1.0, 0.0)];
        let g = GraphColoring::color(&mut cs);
        let positions = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let inv_mass = vec![1.0, 1.0];
        let mut block = JacobiBlock::new(positions, inv_mass);
        let solver = XpbdSolver::default();
        let map = |id: u64| -> Option<usize> {
            if id == 0 {
                Some(0)
            } else if id == 1 {
                Some(1)
            } else {
                None
            }
        };
        let total = solver.solve(&cs, &g, &mut block, &map, 1.0).unwrap();
        // 4 iterations × 1 constraint = 4 projections.
        assert_eq!(total, 4);
        // Distance should be at rest.
        let dist = ((block.positions[1][0] - block.positions[0][0]).powi(2)
            + (block.positions[1][1] - block.positions[0][1]).powi(2)
            + (block.positions[1][2] - block.positions[0][2]).powi(2))
        .sqrt();
        assert!(approx(dist, 1.0, 1e-3));
    }

    #[test]
    fn solver_unmapped_body_returns_error() {
        let mut cs = vec![Constraint::distance(0, 99, 1.0, 0.0)];
        let g = GraphColoring::color(&mut cs);
        let positions = vec![[0.0; 3]];
        let inv_mass = vec![1.0];
        let mut block = JacobiBlock::new(positions, inv_mass);
        let solver = XpbdSolver::default();
        let map = |id: u64| -> Option<usize> {
            if id == 0 {
                Some(0)
            } else {
                None
            }
        };
        let r = solver.solve(&cs, &g, &mut block, &map, 1.0);
        assert!(matches!(r, Err(ConstraintFailure::BodyOutOfRange { .. })));
    }

    #[test]
    fn coloring_n_colors_returns_color_count() {
        let mut cs = vec![Constraint::distance(0, 1, 1.0, 0.0)];
        let g = GraphColoring::color(&mut cs);
        assert_eq!(g.n_colors(), g.color_count);
    }

    #[test]
    fn solver_with_high_fidelity_runs_more_iterations() {
        let mut cs = vec![Constraint::distance(0, 1, 1.0, 0.0)];
        let g = GraphColoring::color(&mut cs);
        let positions = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let inv_mass = vec![1.0, 1.0];
        let mut block = JacobiBlock::new(positions, inv_mass);
        let solver = XpbdSolver::new(XpbdConfig::high_fidelity());
        let map = |id: u64| -> Option<usize> {
            if id == 0 {
                Some(0)
            } else if id == 1 {
                Some(1)
            } else {
                None
            }
        };
        let total = solver.solve(&cs, &g, &mut block, &map, 1.0).unwrap();
        // 16 iterations × 1 constraint = 16.
        assert_eq!(total, 16);
    }

    #[test]
    fn distance_with_nonzero_compliance_softens_response() {
        let mut cs = vec![Constraint::distance(0, 1, 1.0, 0.5)];
        let g = GraphColoring::color(&mut cs);
        let positions = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let inv_mass = vec![1.0, 1.0];
        let mut block = JacobiBlock::new(positions, inv_mass);
        let solver = XpbdSolver::default();
        let map = |id: u64| -> Option<usize> {
            if id == 0 {
                Some(0)
            } else if id == 1 {
                Some(1)
            } else {
                None
            }
        };
        let _ = solver.solve(&cs, &g, &mut block, &map, 1.0).unwrap();
        let dist = ((block.positions[1][0] - block.positions[0][0]).powi(2)).sqrt();
        // Soft compliance won't fully reach rest in 4 iter ; should be > 1.0.
        assert!(dist > 1.0 || approx(dist, 1.0, 1e-2));
    }

    #[test]
    fn jacobi_block_static_body_does_not_move() {
        let positions = vec![[0.0; 3], [2.0, 0.0, 0.0]];
        let inv_mass = vec![0.0, 1.0]; // body 0 is static
        let mut b = JacobiBlock::new(positions, inv_mass);
        b.project_distance(0, 1, 1.0, 0.0, 1.0);
        b.flush_corrections();
        assert_eq!(b.positions[0], [0.0; 3]);
    }
}

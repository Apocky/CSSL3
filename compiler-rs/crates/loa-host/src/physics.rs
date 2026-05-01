// § T11-LOA-HOST-2 (W-LOA-host-input) · physics.rs ────────────────────────
// Axis-slide collision (capsule vs AABBs) + 8-compass-ray proprioception.
// Per scenes/player_physics.cssl design.
//
// § design-notes ───────────────────────────────────────────────────────
// Collision is run AS three independent 1D moves :
//
//     1. Try moving on +X / -X axis only. If new position would penetrate
//        any AABB, clamp to the wall surface (account for capsule-radius).
//     2. Repeat for Y axis (vertical · floor + ceiling).
//     3. Repeat for Z axis.
//
// This is the classical "axis-slide" approach : minimal state, no contact-
// manifold, no projection-onto-plane, no SAT. Result : sliding along walls
// instead of sticking. Adequate for first-person walking ; later slices may
// upgrade to swept-capsule if needed.
//
// § capsule shape ──────────────────────────────────────────────────────
//   radius = 0.4 m
//   height = 1.7 m  (total head-to-toe)
//   center = pos.y - 0.7 (eye is 0.7m above center → eye-height 1.55 above feet)
//
// We approximate capsule-vs-AABB by EXPANDING the AABB by the capsule's
// radius on the horizontal axes + by the half-height on the vertical axis,
// then doing point-vs-expanded-AABB. This is the Minkowski-sum approach.
// It's slightly conservative on capsule-corners (rounded vs box-corners
// of the dilated AABB) but the over-conservation is < 0.4m·(1-cos(45°)) ≈
// 11cm and is invisible at first-person speeds.
//
// § 8-compass-ray ──────────────────────────────────────────────────────
// Cast a ray from the camera-eye-position toward each of 8 cardinal/ordinal
// directions in the horizontal plane (N · NE · E · SE · S · SW · W · NW).
// For each ray : distance to the nearest AABB intersection along that
// direction, capped at MAX_RAY_M (50m). Used by upper layers (DM, GM,
// debug overlay) for spatial-context awareness.
//
// Naming convention :
//   N = +Z   ·  E = +X   ·  S = -Z   ·  W = -X
//   NE = +X+Z (normalized) · etc.
//
// § PRIME-DIRECTIVE ────────────────────────────────────────────────────
// Physics is local-only. No telemetry leak ; collision-events log to the
// runtime log (warnings only on collision events the user can perceive
// — wall bumps).

use cssl_rt::loa_startup::log_event;

use crate::movement::Camera;

/// Capsule radius (horizontal half-thickness). Per scenes/player_physics.cssl.
pub const CAPSULE_RADIUS: f32 = 0.4;
/// Capsule total height. Per scenes/player_physics.cssl.
pub const CAPSULE_HEIGHT: f32 = 1.7;
/// Eye offset above capsule-center.
pub const EYE_OFFSET: f32 = 0.7;
/// Max ray distance for compass proprioception.
pub const MAX_RAY_M: f32 = 50.0;

/// Axis-aligned bounding box. Half-open : a point is INSIDE if
/// `min ≤ p < max` on each axis. (Half-openness avoids double-counting
/// edges between adjacent AABBs.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }

    /// True if `p` is strictly inside this AABB.
    pub fn contains(&self, p: [f32; 3]) -> bool {
        p[0] >= self.min[0] && p[0] < self.max[0]
            && p[1] >= self.min[1] && p[1] < self.max[1]
            && p[2] >= self.min[2] && p[2] < self.max[2]
    }

    /// Expand this AABB by `r` on horizontal axes + `vh` on the vertical
    /// axis. Returns a NEW AABB (Minkowski-sum with capsule).
    #[must_use]
    pub fn expanded_by_capsule(&self, r: f32, vh: f32) -> Aabb {
        Aabb {
            min: [self.min[0] - r, self.min[1] - vh, self.min[2] - r],
            max: [self.max[0] + r, self.max[1] + vh, self.max[2] + r],
        }
    }
}

/// Room-level collider : the room AABB (player must stay INSIDE) + N
/// plinth AABBs (player must stay OUTSIDE).
///
/// Per scenes/test_room.cssl design : 40m × 40m × 8m room, 14 plinths
/// at quadrant + corner + center-axis positions.
///
/// § T11-LOA-ROOMS extension :
///   For multi-room mode, `interiors` holds the union of all room+corridor
///   AABBs ; the player must be INSIDE at least one of them. The single
///   `room` field is kept for backward-compat with `test_room()`. When the
///   collider was built via `full_world()`, `interiors` contains every
///   room and corridor and `room` is set to the world-envelope AABB.
#[derive(Debug, Clone)]
pub struct RoomCollider {
    /// Outer room bounds (or world-envelope for multi-room).
    pub room: Aabb,
    /// Plinth AABBs. Player must stay outside each.
    pub plinths: Vec<Aabb>,
    /// § T11-LOA-ROOMS : disjoint interior AABBs the camera must be inside
    /// SOME-of (multi-room mode). When empty, falls back to single-room
    /// `room` AABB containment (test-room legacy mode).
    pub interiors: Vec<Aabb>,
}

impl RoomCollider {
    /// Construct the test-room collider per scenes/test_room.cssl design.
    /// Room : (-20, 0, -20) → (20, 8, 20).
    /// Plinths : 14 boxes (1m × 2m × 1m each) at :
    ///   • 8 quadrant positions  : (±5, ±5) and (±15, ±15)
    ///   • 4 corner-calibration  : near each corner ((-18,-18), (18,-18), (-18,18), (18,18))
    ///   • 2 center-axis         : (0, 10) and (10, 0)
    pub fn test_room() -> Self {
        let room = Aabb::new([-20.0, 0.0, -20.0], [20.0, 8.0, 20.0]);
        let plinth_size = ([1.0, 2.0, 1.0], [-0.5, 0.0, -0.5]); // (size, offset_to_min)
        let mk_plinth = |x: f32, z: f32| {
            let (size, off) = plinth_size;
            Aabb::new(
                [x + off[0], off[1], z + off[2]],
                [x + off[0] + size[0], off[1] + size[1], z + off[2] + size[2]],
            )
        };
        let plinths = vec![
            // 8 quadrant plinths
            mk_plinth(5.0, 5.0),
            mk_plinth(-5.0, 5.0),
            mk_plinth(5.0, -5.0),
            mk_plinth(-5.0, -5.0),
            mk_plinth(15.0, 15.0),
            mk_plinth(-15.0, 15.0),
            mk_plinth(15.0, -15.0),
            mk_plinth(-15.0, -15.0),
            // 4 corner-calibration plinths
            mk_plinth(-18.0, -18.0),
            mk_plinth(18.0, -18.0),
            mk_plinth(-18.0, 18.0),
            mk_plinth(18.0, 18.0),
            // 2 center-axis plinths
            mk_plinth(0.0, 10.0),
            mk_plinth(10.0, 0.0),
        ];
        Self {
            room,
            plinths,
            interiors: Vec::new(),
        }
    }

    /// § T11-LOA-ROOMS : Construct the multi-room collider — TestRoom hub
    /// + 4 satellite rooms + 4 corridors connecting them.
    ///
    /// The camera is allowed to be inside the union of all 9 boxes. Doorway
    /// connectivity is implicit : the camera can cross from one room AABB
    /// into a corridor AABB seamlessly because the corridor butts up
    /// against the room wall and the wall at that section of the room is
    /// not emitted as collidable geometry.
    ///
    /// The TestRoom's 14 plinths remain as obstacle AABBs ; the satellite
    /// rooms add their own diagnostic obstacles.
    #[must_use]
    pub fn full_world() -> Self {
        use crate::room::{Corridor, Room};
        // World envelope = AABB containing every room + corridor.
        let env = crate::room::world_envelope();
        let world = Aabb::new([env.min[0], env.min[1], env.min[2]], [env.max[0], env.max[1], env.max[2]]);

        // Per-room interior AABBs (camera must be inside SOME of these).
        let mut interiors = Vec::with_capacity(9);
        for r in Room::all() {
            let b = r.bounds();
            interiors.push(Aabb::new(b.min, b.max));
        }
        for c in Corridor::all() {
            let b = c.bounds();
            interiors.push(Aabb::new(b.min, b.max));
        }

        // Plinths : start with the TestRoom's 14 (re-used from test_room).
        let base = Self::test_room();
        let mut plinths = base.plinths;

        // MaterialRoom : 16 hovering spheres in 4×4 grid (1.5m radius cubes).
        let mb = Room::MaterialRoom.bounds();
        let mcx = (mb.min[0] + mb.max[0]) * 0.5;
        let mcz = (mb.min[2] + mb.max[2]) * 0.5;
        let radius = 1.5_f32;
        let spacing = 6.0_f32;
        let sphere_y = 3.0_f32;
        for i in 0..4 {
            for j in 0..4 {
                let dx = (i as f32 - 1.5) * spacing;
                let dz = (j as f32 - 1.5) * spacing;
                let x = mcx + dx;
                let z = mcz + dz;
                plinths.push(Aabb::new(
                    [x - radius, sphere_y - radius, z - radius],
                    [x + radius, sphere_y + radius, z + radius],
                ));
            }
        }

        // ScaleRoom : reference towers every 5m along X.
        let sb = Room::ScaleRoom.bounds();
        let z_pos = sb.min[2] + 5.0;
        let heights = [1.0_f32, 2.0, 3.0, 5.0, 10.0];
        let mut x_pos = sb.min[0] + 5.0;
        let mut idx = 0;
        while x_pos < sb.max[0] - 4.0 {
            let h = heights[idx % heights.len()];
            plinths.push(Aabb::new(
                [x_pos - 0.25, 0.0, z_pos - 0.25],
                [x_pos + 0.25, h, z_pos + 0.25],
            ));
            x_pos += 5.0;
            idx += 1;
        }

        Self {
            room: world,
            plinths,
            interiors,
        }
    }

    /// Capsule-center at this eye-position. The capsule's center is `EYE_OFFSET`
    /// below the eye.
    fn capsule_center(eye: [f32; 3]) -> [f32; 3] {
        [eye[0], eye[1] - EYE_OFFSET, eye[2]]
    }

    /// Test if the capsule centered at `c` (capsule-center, NOT eye) is
    /// CLEAR — i.e. inside the room and not penetrating any plinth.
    /// Uses Minkowski-sum (point-vs-expanded-AABB).
    ///
    /// EPSILON : we shrink/expand AABBs by an additional 1mm tolerance so
    /// float-precision drift (e.g. 1.55 - 0.7 = 0.84999996) doesn't cause
    /// false-collisions when the capsule-center sits EXACTLY on a clamped
    /// boundary. 1mm is invisible at first-person scale and well below
    /// the bisection-precision (sub-mm).
    fn capsule_clear(&self, c: [f32; 3]) -> bool {
        const EPS: f32 = 1.0e-3;
        let half_h = CAPSULE_HEIGHT * 0.5;
        let r = CAPSULE_RADIUS;

        if self.interiors.is_empty() {
            // Single-room mode : SHRINK the room AABB by capsule radius/height.
            let inner = Aabb::new(
                [self.room.min[0] + r - EPS, self.room.min[1] + half_h - EPS, self.room.min[2] + r - EPS],
                [self.room.max[0] - r + EPS, self.room.max[1] - half_h + EPS, self.room.max[2] - r + EPS],
            );
            if !inner.contains(c) {
                return false;
            }
        } else {
            // § T11-LOA-ROOMS : multi-room mode. Camera is clear if its
            // capsule-center fits inside ANY of the interior AABBs (rooms +
            // corridors), shrunk by capsule dimensions on each axis.
            //
            // The capsule's HORIZONTAL projection (radius r) is the
            // critical fit-check ; vertical fit is checked too. We use a
            // generous 0.1m horizontal tolerance for ROOM ↔ CORRIDOR
            // boundary crossing : the doorway gap is 2m wide, the capsule
            // is 0.8m wide, and the rooms abut corridor-bounds with no
            // overlap, so a tighter shrink would create dead-zones at the
            // doorway threshold. Doorway-side interfaces are vertical
            // planes ; the per-axis shrink at the boundary axis is
            // skipped.
            let mut found_inside = false;
            for ib in &self.interiors {
                // Determine which axes are "boundary axes" with neighboring
                // interiors. We only shrink on axes where this AABB has no
                // neighbor touching. Concretely : for each axis, check if
                // any OTHER interior shares that face.
                let mut shrink_x_min = r - EPS;
                let mut shrink_x_max = r - EPS;
                let mut shrink_z_min = r - EPS;
                let mut shrink_z_max = r - EPS;
                for jb in &self.interiors {
                    if std::ptr::eq(ib, jb) {
                        continue;
                    }
                    // Touching face on -X side ?
                    if (jb.max[0] - ib.min[0]).abs() < 0.5
                        && jb.max[2] > ib.min[2]
                        && jb.min[2] < ib.max[2]
                    {
                        shrink_x_min = -EPS;
                    }
                    // Touching face on +X side ?
                    if (jb.min[0] - ib.max[0]).abs() < 0.5
                        && jb.max[2] > ib.min[2]
                        && jb.min[2] < ib.max[2]
                    {
                        shrink_x_max = -EPS;
                    }
                    // Touching face on -Z side ?
                    if (jb.max[2] - ib.min[2]).abs() < 0.5
                        && jb.max[0] > ib.min[0]
                        && jb.min[0] < ib.max[0]
                    {
                        shrink_z_min = -EPS;
                    }
                    // Touching face on +Z side ?
                    if (jb.min[2] - ib.max[2]).abs() < 0.5
                        && jb.max[0] > ib.min[0]
                        && jb.min[0] < ib.max[0]
                    {
                        shrink_z_max = -EPS;
                    }
                }
                let inner = Aabb::new(
                    [
                        ib.min[0] + shrink_x_min,
                        ib.min[1] + half_h - EPS,
                        ib.min[2] + shrink_z_min,
                    ],
                    [
                        ib.max[0] - shrink_x_max,
                        ib.max[1] - half_h + EPS,
                        ib.max[2] - shrink_z_max,
                    ],
                );
                if inner.contains(c) {
                    found_inside = true;
                    break;
                }
            }
            if !found_inside {
                return false;
            }
        }

        // Not-penetrating-plinth test : EXPAND each plinth by capsule radius/half-height.
        // We TIGHTEN by 1mm here (subtract EPS instead of adding) so a capsule
        // that's exactly tangent to the plinth's expanded surface is considered
        // CLEAR rather than colliding — symmetric with the inner-room loosening.
        for p in &self.plinths {
            let e = Aabb::new(
                [p.min[0] - r + EPS, p.min[1] - half_h + EPS, p.min[2] - r + EPS],
                [p.max[0] + r - EPS, p.max[1] + half_h - EPS, p.max[2] + r - EPS],
            );
            if e.contains(c) {
                return false;
            }
        }
        true
    }

    /// Axis-slide : try to move the camera by `delta` ; clamp each axis
    /// independently. Returns the validated delta (after clamping). Logs
    /// a WARN per axis that gets clamped.
    ///
    /// Implementation : for each axis, sweep the 1D segment in ≤0.1m
    /// substeps. If any substep enters a collision-volume, bisect on that
    /// substep to find the maximum sub-step length that's still clear.
    /// This is necessary to detect MID-PATH penetration (a tunneling
    /// motion that starts and ends in clear space but passes through
    /// a plinth).
    pub fn slide(&self, eye: [f32; 3], delta: [f32; 3]) -> [f32; 3] {
        const SUBSTEP_M: f32 = 0.1; // ≤10cm per substep keeps capsule (radius 0.4) from tunneling
        let mut out = [0.0_f32; 3];
        let mut current = Self::capsule_center(eye);
        for axis in 0..3 {
            let total = delta[axis];
            if total.abs() < 1e-9 {
                continue;
            }
            let sign = total.signum();
            let abs_total = total.abs();
            let n_substeps = (abs_total / SUBSTEP_M).ceil() as usize;
            // n_substeps fits in f32 without precision-loss for any reasonable
            // per-frame delta (<10m per axis ⇒ ≤100 substeps).
            #[allow(clippy::cast_precision_loss)]
            let substep_size = abs_total / n_substeps as f32;
            let mut accumulated = 0.0_f32;
            let mut blocked = false;
            for _ in 0..n_substeps {
                let mut probe = current;
                probe[axis] += sign * substep_size;
                if self.capsule_clear(probe) {
                    current[axis] = probe[axis];
                    accumulated += substep_size;
                } else {
                    // Bisect on THIS substep to find the maximum partial that's clear.
                    let mut hi = substep_size;
                    let mut lo = 0.0_f32;
                    for _ in 0..16 {
                        let mid = (lo + hi) * 0.5;
                        let mut tprobe = current;
                        tprobe[axis] += sign * mid;
                        if self.capsule_clear(tprobe) {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    current[axis] += sign * lo;
                    accumulated += lo;
                    blocked = true;
                    break;
                }
            }
            if blocked {
                log_event(
                    "WARN",
                    "loa-host/physics",
                    &format!(
                        "wall-collision · axis={} clamped from {:.4} to {:.4}",
                        axis,
                        total,
                        sign * accumulated
                    ),
                );
            }
            out[axis] = sign * accumulated;
        }
        out
    }

    /// Cast a ray from `origin` along `dir` (must be unit-length). Returns
    /// distance to nearest AABB-surface intersection, or MAX_RAY_M if none
    /// within range. Considers the room's INSIDE walls (so ray hits the wall
    /// when exiting the room) AND the plinths' OUTSIDE faces.
    fn raycast(&self, origin: [f32; 3], dir: [f32; 3]) -> f32 {
        let mut nearest = MAX_RAY_M;
        // Room-wall : we're inside the box ; nearest distance to exit.
        let troom = ray_aabb_inside(origin, dir, &self.room);
        if troom > 0.0 && troom < nearest {
            nearest = troom;
        }
        // Plinths : we're outside each ; nearest distance to enter.
        for p in &self.plinths {
            let tp = ray_aabb_outside(origin, dir, p);
            if tp > 0.0 && tp < nearest {
                nearest = tp;
            }
        }
        nearest
    }

    /// 8-compass-ray proprioception. Cast rays from camera-eye-position in
    /// all 8 cardinal/ordinal directions (horizontal plane). Distances are
    /// CAPPED at MAX_RAY_M.
    pub fn compass_distances(&self, camera: &Camera) -> CompassDistances {
        let s2 = std::f32::consts::FRAC_1_SQRT_2; // 1/√2 ≈ 0.7071
        let dirs: [[f32; 3]; 8] = [
            [0.0, 0.0, 1.0],   // N
            [s2, 0.0, s2],     // NE
            [1.0, 0.0, 0.0],   // E
            [s2, 0.0, -s2],    // SE
            [0.0, 0.0, -1.0],  // S
            [-s2, 0.0, -s2],   // SW
            [-1.0, 0.0, 0.0],  // W
            [-s2, 0.0, s2],    // NW
        ];
        let mut out = [0.0_f32; 8];
        for (i, d) in dirs.iter().enumerate() {
            out[i] = self.raycast(camera.pos, *d);
        }
        CompassDistances { dist: out }
    }
}

/// 8-compass-ray distance result. Names match cardinal compass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompassDistances {
    /// `[N, NE, E, SE, S, SW, W, NW]`
    pub dist: [f32; 8],
}

impl CompassDistances {
    pub fn n(&self) -> f32 { self.dist[0] }
    pub fn ne(&self) -> f32 { self.dist[1] }
    pub fn e(&self) -> f32 { self.dist[2] }
    pub fn se(&self) -> f32 { self.dist[3] }
    pub fn s(&self) -> f32 { self.dist[4] }
    pub fn sw(&self) -> f32 { self.dist[5] }
    pub fn w(&self) -> f32 { self.dist[6] }
    pub fn nw(&self) -> f32 { self.dist[7] }
}

// ──────────────────────────────────────────────────────────────────────
// § ray-AABB intersection helpers
// ──────────────────────────────────────────────────────────────────────

/// Origin is INSIDE the AABB ; return distance to exit-face (or 0 if not
/// inside). Used for room-wall ray-distance.
fn ray_aabb_inside(origin: [f32; 3], dir: [f32; 3], a: &Aabb) -> f32 {
    if !a.contains(origin) {
        return 0.0;
    }
    let mut t_min = f32::INFINITY;
    for axis in 0..3 {
        if dir[axis].abs() < 1e-9 {
            continue;
        }
        let t = if dir[axis] > 0.0 {
            (a.max[axis] - origin[axis]) / dir[axis]
        } else {
            (a.min[axis] - origin[axis]) / dir[axis]
        };
        if t > 0.0 && t < t_min {
            t_min = t;
        }
    }
    if t_min == f32::INFINITY {
        0.0
    } else {
        t_min
    }
}

/// Origin is OUTSIDE the AABB ; return distance to enter-face along `dir`,
/// or 0 if no intersection in the positive ray-direction. Standard slab method.
fn ray_aabb_outside(origin: [f32; 3], dir: [f32; 3], a: &Aabb) -> f32 {
    let mut t_enter: f32 = 0.0;
    let mut t_exit: f32 = f32::INFINITY;
    for axis in 0..3 {
        if dir[axis].abs() < 1e-9 {
            // Ray parallel to this axis ; if origin out of slab, no hit.
            if origin[axis] < a.min[axis] || origin[axis] >= a.max[axis] {
                return 0.0;
            }
            continue;
        }
        let inv = 1.0 / dir[axis];
        let mut t1 = (a.min[axis] - origin[axis]) * inv;
        let mut t2 = (a.max[axis] - origin[axis]) * inv;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        if t1 > t_enter {
            t_enter = t1;
        }
        if t2 < t_exit {
            t_exit = t2;
        }
        if t_enter > t_exit {
            return 0.0;
        }
    }
    if t_enter < 0.0 {
        // Origin is INSIDE the AABB — caller should have used ray_aabb_inside.
        // Return 0 to indicate "no exterior-hit ahead".
        return 0.0;
    }
    t_enter
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::suboptimal_flops, clippy::imprecise_flops)]
mod tests {
    use super::*;
    use crate::movement::Camera;

    #[test]
    fn aabb_contains_basic() {
        let a = Aabb::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(a.contains([0.5, 0.5, 0.5]));
        assert!(!a.contains([1.5, 0.5, 0.5]));
        assert!(!a.contains([-0.1, 0.5, 0.5]));
    }

    #[test]
    fn test_room_constructor() {
        let r = RoomCollider::test_room();
        assert_eq!(r.room.min, [-20.0, 0.0, -20.0]);
        assert_eq!(r.room.max, [20.0, 8.0, 20.0]);
        assert_eq!(r.plinths.len(), 14);
    }

    #[test]
    fn axis_slide_clamps_at_wall_boundary() {
        let r = RoomCollider::test_room();
        // Stand near the +X wall, eye at y=1.55. Try to move +X by 100m.
        // Should clamp at the wall (with capsule radius accounted for).
        let eye = [19.0, 1.55, 0.0];
        let delta = [100.0, 0.0, 0.0];
        let validated = r.slide(eye, delta);
        // Capsule-center starts at x=19. Inner-room extends to x = 20 - 0.4 = 19.6.
        // Max allowed step = 0.6 (sub-mm). Bisection lands within 1 mm.
        assert!(validated[0] > 0.0);
        assert!(validated[0] < 0.7);
        assert!((validated[0] - 0.6).abs() < 1.0e-3);
    }

    #[test]
    fn axis_slide_unobstructed_passes_through() {
        let r = RoomCollider::test_room();
        // Center of the room, walk forward 1m. Not hitting anything.
        let eye = [0.0, 1.55, 0.0];
        let delta = [0.0, 0.0, 1.0];
        let validated = r.slide(eye, delta);
        assert!(
            (validated[2] - 1.0).abs() < 1.0e-4,
            "expected ~1.0, got {}",
            validated[2]
        );
    }

    #[test]
    fn axis_slide_blocks_at_plinth() {
        let r = RoomCollider::test_room();
        // Approach the (5,5) plinth from south. Plinth occupies x:[4.5..5.5], z:[4.5..5.5].
        // Capsule starts at (5, 0.85, 3) — capsule-center 0.85, eye=1.55. Moving +Z by 5m
        // would put us on top of the plinth horizontally. Must clamp.
        let eye = [5.0, 1.55, 3.0];
        let delta = [0.0, 0.0, 5.0];
        let validated = r.slide(eye, delta);
        // Plinth front-face is at z=4.5 ; capsule-radius=0.4 ; max-allowed = 4.5 - 3 - 0.4 = 1.1.
        // Bisection lands within 1 mm.
        assert!((validated[2] - 1.1).abs() < 1.0e-3, "got {}", validated[2]);
    }

    #[test]
    fn compass_ray_distances_within_room_bounds() {
        let r = RoomCollider::test_room();
        let c = Camera::new(); // origin · facing +Z
        let cd = r.compass_distances(&c);
        // From (0, 1.55, 0) the cardinal walls are at distance 20m on each axis
        // (room extends -20..+20). Plinths are closer in some directions.
        // Plinth at (10, 0, 0) blocks E-ray : front-face at x=9.5 → distance 9.5.
        assert!((cd.e() - 9.5).abs() < 1e-3);
        // Plinth at (0, 0, 10) blocks N-ray : front-face at z=9.5 → distance 9.5.
        assert!((cd.n() - 9.5).abs() < 1e-3);
        // West has no plinth on the x-axis (the (0,10) plinth is on z-axis).
        // But (-5, -5) and (-5, 5) plinths are off-axis. The +X-axis-ray
        // from origin going W (negative-x) doesn't hit any plinth ; reaches wall at x=-20.
        assert!((cd.w() - 20.0).abs() < 1e-3);
        // South wall is also at z=-20 (no plinth on -z axis at x=0).
        assert!((cd.s() - 20.0).abs() < 1e-3);
        // All ray distances must be in (0, MAX_RAY_M].
        for d in cd.dist {
            assert!(d > 0.0);
            assert!(d <= MAX_RAY_M);
        }
    }

    #[test]
    fn ray_outside_slab_no_hit() {
        let a = Aabb::new([10.0, 0.0, 10.0], [11.0, 1.0, 11.0]);
        // Ray origin offset, parallel to box, no hit.
        let t = ray_aabb_outside([0.0, 0.5, 0.0], [1.0, 0.0, 0.0], &a);
        // The ray runs at y=0.5 z=0 ; it stays outside the slab z=10..11. No hit.
        assert!(t.abs() < 1.0e-6);
    }

    #[test]
    fn ray_outside_slab_direct_hit() {
        let a = Aabb::new([10.0, 0.0, -1.0], [11.0, 2.0, 1.0]);
        // Ray from (0, 1, 0) along +X. Hits front face at x=10.
        let t = ray_aabb_outside([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], &a);
        assert!((t - 10.0).abs() < 1e-5);
    }

    #[test]
    fn ray_inside_box_exits_at_wall() {
        let a = Aabb::new([-10.0, 0.0, -10.0], [10.0, 5.0, 10.0]);
        // From origin, +Z ray exits at z=10.
        let t = ray_aabb_inside([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], &a);
        assert!((t - 10.0).abs() < 1e-5);
    }

    #[test]
    fn capsule_center_offset_consistent() {
        let r = RoomCollider::test_room();
        // Eye exactly at floor + EYE_OFFSET + capsule-half-height — capsule-center
        // sits at y = 0 + half-height. We want the capsule-center to be ABOVE
        // the room floor (y >= half-height = 0.85). At eye y=1.55, capsule
        // center = 0.85, exactly on the inner-room-floor. The contains-test is
        // half-open : center.y < min.y + half_h would fail. Test that y=1.56 passes.
        assert!(r.capsule_clear([0.0, 0.85 + 1e-3, 0.0]));
    }

    // ──────────────────────────────────────────────────────────────────
    // § T11-LOA-ROOMS · multi-room collider tests
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn full_world_collider_constructs() {
        let r = RoomCollider::full_world();
        // 9 interior AABBs (5 rooms + 4 corridors).
        assert_eq!(r.interiors.len(), 9);
        // 14 (TestRoom plinths) + 16 (Material spheres) + 11 (Scale towers) = 41 plinths
        assert!(r.plinths.len() >= 14 + 16);
    }

    #[test]
    fn full_world_camera_inside_test_room_is_clear() {
        let r = RoomCollider::full_world();
        // TestRoom center, capsule-center at y=0.85, away from any plinth.
        // Pick a known empty spot near origin (no plinth).
        assert!(r.capsule_clear([3.0, 0.85, 0.0]));
    }

    #[test]
    fn full_world_camera_inside_material_room_is_clear() {
        let r = RoomCollider::full_world();
        // MaterialRoom is at z ∈ [28,58], corner spot away from spheres.
        // Spheres are centered at sphere_y=3.0 ; capsule at y=0.85 is below them.
        assert!(r.capsule_clear([0.0, 0.85, 56.0]));
    }

    #[test]
    fn full_world_camera_in_corridor_is_clear() {
        let r = RoomCollider::full_world();
        // Corridor-N runs z ∈ [20,28]. Standing inside at z=24.
        assert!(r.capsule_clear([0.0, 0.85, 24.0]));
    }

    #[test]
    fn full_world_camera_outside_world_is_blocked() {
        let r = RoomCollider::full_world();
        // Way outside the world envelope.
        assert!(!r.capsule_clear([1000.0, 0.85, 1000.0]));
    }
}

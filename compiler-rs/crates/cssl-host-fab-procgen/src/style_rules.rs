// § T11-W5c-FAB-PROCGEN : StyleRules + violation surface
// ══════════════════════════════════════════════════════════════════
//! Style-rule constraints over a [`Blueprint`].
//!
//! § Style-rules are a SECONDARY validation layer atop `Blueprint::validate`.
//! Where Blueprint-validation enforces structural coherence (unique ids,
//! no self-loops), style-rules enforce AESTHETIC + LAYOUT conventions :
//! - symmetry (None / mirror-X / mirror-Z / quad / radial-N)
//! - density falloff (parts further from origin should have smaller dims)
//! - doorway alignment (free / grid-aligned / centered-only)
//! - hard cap on number of parts
//!
//! § A failing rule produces a `StyleViolation` ; multiple violations are
//! collected — `check_blueprint` does NOT short-circuit.

use serde::{Deserialize, Serialize};

use crate::blueprint::Blueprint;

// ── enums ─────────────────────────────────────────────────────────

/// § Symmetry constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymmetryRule {
    None,
    MirrorX,
    MirrorZ,
    Quad,
    Radial(u32),
}

/// § Doorway-alignment constraint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DoorwayAlignmentRule {
    Free,
    AlignedToGrid(f32),
    CenteredOnly,
}

// ── structs ───────────────────────────────────────────────────────

/// § Style-rule bundle applied to a `Blueprint`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleRules {
    pub symmetry:           SymmetryRule,
    pub density_falloff:    f32,
    pub doorway_alignment:  DoorwayAlignmentRule,
    pub max_parts:          u32,
}

impl Default for StyleRules {
    fn default() -> Self {
        Self {
            symmetry:           SymmetryRule::None,
            density_falloff:    0.0,
            doorway_alignment:  DoorwayAlignmentRule::Free,
            max_parts:          16,
        }
    }
}

/// § A single style-rule violation report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleViolation {
    pub rule:    String,
    pub part_id: Option<u32>,
    pub detail:  String,
}

// ── impl ──────────────────────────────────────────────────────────

impl StyleRules {
    /// § Run every rule against `bp` and collect violations.
    #[must_use]
    pub fn check_blueprint(&self, bp: &Blueprint) -> Vec<StyleViolation> {
        let mut violations = Vec::new();

        // ── max-parts cap ─────────────────────────────────────────
        if bp.parts.len() as u32 > self.max_parts {
            violations.push(StyleViolation {
                rule:    "max_parts".to_string(),
                part_id: None,
                detail:  format!(
                    "blueprint has {} parts ; max allowed = {}",
                    bp.parts.len(),
                    self.max_parts
                ),
            });
        }

        // ── symmetry ──────────────────────────────────────────────
        match self.symmetry {
            SymmetryRule::None => {}
            SymmetryRule::MirrorX => {
                // For each part not on the X-axis, expect a mirror at -x.
                for p in &bp.parts {
                    let (x, _, z) = p.position_offset;
                    if x.abs() < f32::EPSILON {
                        continue;
                    }
                    let mirrored = (-x, p.position_offset.1, z);
                    let has_mirror = bp.parts.iter().any(|q| {
                        (q.position_offset.0 - mirrored.0).abs() < 0.05
                            && (q.position_offset.2 - mirrored.2).abs() < 0.05
                            && q.kind == p.kind
                    });
                    if !has_mirror {
                        violations.push(StyleViolation {
                            rule:    "symmetry_mirror_x".to_string(),
                            part_id: Some(p.id),
                            detail:  format!(
                                "no mirror-X partner for part {} at ({}, {})",
                                p.id, x, z
                            ),
                        });
                    }
                }
            }
            SymmetryRule::MirrorZ => {
                for p in &bp.parts {
                    let (x, _, z) = p.position_offset;
                    if z.abs() < f32::EPSILON {
                        continue;
                    }
                    let mirrored = (x, p.position_offset.1, -z);
                    let has_mirror = bp.parts.iter().any(|q| {
                        (q.position_offset.0 - mirrored.0).abs() < 0.05
                            && (q.position_offset.2 - mirrored.2).abs() < 0.05
                            && q.kind == p.kind
                    });
                    if !has_mirror {
                        violations.push(StyleViolation {
                            rule:    "symmetry_mirror_z".to_string(),
                            part_id: Some(p.id),
                            detail:  format!(
                                "no mirror-Z partner for part {} at ({}, {})",
                                p.id, x, z
                            ),
                        });
                    }
                }
            }
            SymmetryRule::Quad => {
                // Every off-axis part must have all 3 quadrant-mirror partners.
                for p in &bp.parts {
                    let (x, _, z) = p.position_offset;
                    if x.abs() < f32::EPSILON || z.abs() < f32::EPSILON {
                        continue;
                    }
                    let targets = [(-x, z), (x, -z), (-x, -z)];
                    for (tx, tz) in targets {
                        let has = bp.parts.iter().any(|q| {
                            (q.position_offset.0 - tx).abs() < 0.05
                                && (q.position_offset.2 - tz).abs() < 0.05
                                && q.kind == p.kind
                        });
                        if !has {
                            violations.push(StyleViolation {
                                rule:    "symmetry_quad".to_string(),
                                part_id: Some(p.id),
                                detail:  format!(
                                    "missing quad-mirror at ({tx}, {tz}) for part {}",
                                    p.id
                                ),
                            });
                        }
                    }
                }
            }
            SymmetryRule::Radial(n) => {
                // n-fold rotational : skip non-trivial check (kind+radius
                // bucketing) ; only flag if the part_count is not a multiple
                // of n (excluding origin parts).
                let off_origin: usize = bp
                    .parts
                    .iter()
                    .filter(|p| {
                        let (x, _, z) = p.position_offset;
                        x.abs() > f32::EPSILON || z.abs() > f32::EPSILON
                    })
                    .count();
                if n > 0 && off_origin as u32 % n != 0 {
                    violations.push(StyleViolation {
                        rule:    "symmetry_radial".to_string(),
                        part_id: None,
                        detail:  format!(
                            "off-origin part count {off_origin} is not a multiple of {n}"
                        ),
                    });
                }
            }
        }

        // ── density falloff ───────────────────────────────────────
        // § Convention : if `density_falloff > 0`, parts at distance >= 1.0
        //   should have width_m ≤ first_part.width_m * (1.0 - falloff). We
        //   only flag GROSS violations (size unchanged or larger at distance).
        if self.density_falloff > 0.0 && self.density_falloff < 1.0 {
            if let Some(origin_w) = bp
                .parts
                .iter()
                .find(|p| {
                    let (x, _, z) = p.position_offset;
                    x.abs() < f32::EPSILON && z.abs() < f32::EPSILON
                })
                .map(|p| p.dims.width_m)
            {
                let cap = origin_w * (1.0 - self.density_falloff);
                for p in &bp.parts {
                    let (x, _, z) = p.position_offset;
                    let dist_sq = x * x + z * z;
                    if dist_sq >= 1.0 && p.dims.width_m > cap {
                        violations.push(StyleViolation {
                            rule:    "density_falloff".to_string(),
                            part_id: Some(p.id),
                            detail:  format!(
                                "part {} width {} > falloff cap {}",
                                p.id, p.dims.width_m, cap
                            ),
                        });
                    }
                }
            }
        }

        // ── doorway alignment ─────────────────────────────────────
        match self.doorway_alignment {
            DoorwayAlignmentRule::Free => {}
            DoorwayAlignmentRule::AlignedToGrid(grid) => {
                // § grid <= 0 is a no-op (defensive). Otherwise check every
                //   part-offset is a multiple of `grid`.
                if grid > 0.0 {
                    for p in &bp.parts {
                        let (x, _, z) = p.position_offset;
                        let on_grid_x = (x / grid).round() * grid;
                        let on_grid_z = (z / grid).round() * grid;
                        if (x - on_grid_x).abs() > 1e-3 || (z - on_grid_z).abs() > 1e-3 {
                            violations.push(StyleViolation {
                                rule:    "doorway_alignment_grid".to_string(),
                                part_id: Some(p.id),
                                detail:  format!(
                                    "part {} pos ({x}, {z}) not on grid {grid}",
                                    p.id
                                ),
                            });
                        }
                    }
                }
            }
            DoorwayAlignmentRule::CenteredOnly => {
                // All parts must sit at origin. (Useful for single-room
                // composites where you forbid any part-offset.)
                for p in &bp.parts {
                    let (x, _, z) = p.position_offset;
                    if x.abs() > 1e-3 || z.abs() > 1e-3 {
                        violations.push(StyleViolation {
                            rule:    "doorway_alignment_centered".to_string(),
                            part_id: Some(p.id),
                            detail:  format!(
                                "part {} not centered at origin ({x}, {z})",
                                p.id
                            ),
                        });
                    }
                }
            }
        }

        violations
    }
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_procgen_rooms::{RoomDims, RoomKind};

    fn dims() -> RoomDims {
        RoomDims::default()
    }

    /// § Default rules + empty blueprint = no violations.
    #[test]
    fn default_no_violations() {
        let bp = Blueprint::new("e".to_string(), 0);
        let r = StyleRules::default();
        assert!(r.check_blueprint(&bp).is_empty());
    }

    /// § Exceeding `max_parts` produces exactly one violation.
    #[test]
    fn max_parts_violation() {
        let mut bp = Blueprint::new("many".to_string(), 1);
        for _ in 0..5 {
            bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        }
        let r = StyleRules {
            max_parts: 3,
            ..StyleRules::default()
        };
        let v = r.check_blueprint(&bp);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "max_parts");
    }

    /// § MirrorX symmetry : asymmetric blueprint produces violations.
    #[test]
    fn mirror_x_violation() {
        let mut bp = Blueprint::new("asym".to_string(), 2);
        bp.add_part(RoomKind::CalibrationGrid, dims(), (5.0, 0.0, 0.0), 0.0);
        // No mirror at (-5, 0, 0).
        let r = StyleRules {
            symmetry: SymmetryRule::MirrorX,
            ..StyleRules::default()
        };
        let v = r.check_blueprint(&bp);
        assert!(v.iter().any(|x| x.rule == "symmetry_mirror_x"));

        // § Adding the mirror clears the violation.
        bp.add_part(RoomKind::CalibrationGrid, dims(), (-5.0, 0.0, 0.0), 0.0);
        let v2 = r.check_blueprint(&bp);
        assert!(v2.iter().all(|x| x.rule != "symmetry_mirror_x"));
    }

    /// § CenteredOnly : any non-origin part is a violation.
    #[test]
    fn centered_only_violation() {
        let mut bp = Blueprint::new("c".to_string(), 3);
        bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        bp.add_part(RoomKind::ScaleHall, dims(), (5.0, 0.0, 0.0), 0.0);
        let r = StyleRules {
            doorway_alignment: DoorwayAlignmentRule::CenteredOnly,
            ..StyleRules::default()
        };
        let v = r.check_blueprint(&bp);
        let centered_violations: Vec<_> =
            v.iter().filter(|x| x.rule == "doorway_alignment_centered").collect();
        assert_eq!(centered_violations.len(), 1);
        assert_eq!(centered_violations[0].part_id, Some(1));
    }

    /// § AlignedToGrid : off-grid part triggers violation.
    #[test]
    fn grid_alignment_violation() {
        let mut bp = Blueprint::new("g".to_string(), 4);
        bp.add_part(RoomKind::CalibrationGrid, dims(), (5.0, 0.0, 0.0), 0.0);   // on grid 5.0
        bp.add_part(RoomKind::ScaleHall, dims(), (3.7, 0.0, 0.0), 0.0);          // off-grid
        let r = StyleRules {
            doorway_alignment: DoorwayAlignmentRule::AlignedToGrid(5.0),
            ..StyleRules::default()
        };
        let v = r.check_blueprint(&bp);
        let grid_v: Vec<_> = v.iter().filter(|x| x.rule == "doorway_alignment_grid").collect();
        assert_eq!(grid_v.len(), 1);
        assert_eq!(grid_v[0].part_id, Some(1));
    }

    /// § Round-trip serde : StyleRules serializes + deserializes losslessly.
    #[test]
    fn style_rules_serde_round_trip() {
        let r = StyleRules {
            symmetry:          SymmetryRule::Radial(6),
            density_falloff:   0.25,
            doorway_alignment: DoorwayAlignmentRule::AlignedToGrid(2.0),
            max_parts:         32,
        };
        let json = serde_json::to_string(&r).unwrap();
        let r2: StyleRules = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);

        let v = StyleViolation {
            rule:    "max_parts".to_string(),
            part_id: Some(7),
            detail:  "x".to_string(),
        };
        let vj = serde_json::to_string(&v).unwrap();
        let v2: StyleViolation = serde_json::from_str(&vj).unwrap();
        assert_eq!(v, v2);
    }
}

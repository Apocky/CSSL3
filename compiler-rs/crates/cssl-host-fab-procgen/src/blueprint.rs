// § T11-W5c-FAB-PROCGEN : Blueprint data + validation
// ══════════════════════════════════════════════════════════════════
//! Blueprint = Vec<BlueprintPart> + Vec<BlueprintConnection> + seed.
//!
//! § A `Blueprint` is the FAB (factor-augmented blueprint) authoring artifact :
//! it names a composite room as a graph of base [`cssl_host_procgen_rooms::RoomKind`]
//! parts and the doorway connections between them. Validation ensures every
//! connection refers to an existing part and that no connection is a self-loop
//! or duplicate.
//!
//! § Author either manually (`new` + `add_part` + `connect`) or via the
//! pre-built `crate::library` module.

use cssl_host_procgen_rooms::{RoomDims, RoomKind, WallSide};
use serde::{Deserialize, Serialize};

// ── structs ───────────────────────────────────────────────────────

/// § Composite-room blueprint : multiple parts + doorway connections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    pub name:        String,
    pub parts:       Vec<BlueprintPart>,
    pub connections: Vec<BlueprintConnection>,
    pub seed:        u64,
}

/// § One part of a composite blueprint : a base recipe with a position-offset
/// + Y-axis rotation applied during composition.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPart {
    pub id:              u32,
    pub kind:            RoomKind,
    pub dims:            RoomDims,
    pub position_offset: (f32, f32, f32),
    pub rotation_y:      f32,
}

/// § Doorway connection between two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlueprintConnection {
    pub from_part:    u32,
    pub from_doorway: WallSide,
    pub to_part:      u32,
    pub to_doorway:   WallSide,
}

/// § Errors surfaced by `Blueprint::validate` and `Blueprint::connect`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlueprintErr {
    UnknownPartId(u32),
    SelfLoopConnection,
    DuplicateConnection,
}

// ── impl ──────────────────────────────────────────────────────────

impl Blueprint {
    /// § Construct an empty blueprint with given name + seed.
    #[must_use]
    pub fn new(name: String, seed: u64) -> Self {
        Self {
            name,
            parts: Vec::new(),
            connections: Vec::new(),
            seed,
        }
    }

    /// § Add a part. Returns the assigned unique part-id.
    pub fn add_part(
        &mut self,
        kind: RoomKind,
        dims: RoomDims,
        pos: (f32, f32, f32),
        rot_y: f32,
    ) -> u32 {
        // § Allocate next-free id : max(existing) + 1, or 0 if empty. This keeps
        // ids stable under partial removal (not currently exposed) and
        // guarantees uniqueness even if the caller deletes parts later.
        let next_id = self.parts.iter().map(|p| p.id).max().map_or(0, |m| m + 1);
        self.parts.push(BlueprintPart {
            id:              next_id,
            kind,
            dims,
            position_offset: pos,
            rotation_y:      rot_y,
        });
        next_id
    }

    /// § Add a doorway-connection between two parts.
    ///
    /// § Errors :
    /// - `UnknownPartId` if either part-id is not present in `parts`.
    /// - `SelfLoopConnection` if `from_part == to_part`.
    /// - `DuplicateConnection` if an identical connection already exists.
    pub fn connect(
        &mut self,
        from_part: u32,
        from_door: WallSide,
        to_part: u32,
        to_door: WallSide,
    ) -> Result<(), BlueprintErr> {
        if from_part == to_part {
            return Err(BlueprintErr::SelfLoopConnection);
        }
        if !self.parts.iter().any(|p| p.id == from_part) {
            return Err(BlueprintErr::UnknownPartId(from_part));
        }
        if !self.parts.iter().any(|p| p.id == to_part) {
            return Err(BlueprintErr::UnknownPartId(to_part));
        }
        let new_conn = BlueprintConnection {
            from_part,
            from_doorway: from_door,
            to_part,
            to_doorway: to_door,
        };
        if self.connections.contains(&new_conn) {
            return Err(BlueprintErr::DuplicateConnection);
        }
        self.connections.push(new_conn);
        Ok(())
    }

    /// § Validate the whole blueprint.
    ///
    /// § Checks :
    /// - all part ids unique
    /// - every connection refers to existing parts
    /// - no self-loop connection
    /// - no duplicate connection
    pub fn validate(&self) -> Result<(), BlueprintErr> {
        // Unique part ids.
        for (i, p) in self.parts.iter().enumerate() {
            for q in &self.parts[i + 1..] {
                if p.id == q.id {
                    return Err(BlueprintErr::UnknownPartId(p.id));
                }
            }
        }
        // Connection sanity.
        for (i, c) in self.connections.iter().enumerate() {
            if c.from_part == c.to_part {
                return Err(BlueprintErr::SelfLoopConnection);
            }
            if !self.parts.iter().any(|p| p.id == c.from_part) {
                return Err(BlueprintErr::UnknownPartId(c.from_part));
            }
            if !self.parts.iter().any(|p| p.id == c.to_part) {
                return Err(BlueprintErr::UnknownPartId(c.to_part));
            }
            for d in &self.connections[i + 1..] {
                if c == d {
                    return Err(BlueprintErr::DuplicateConnection);
                }
            }
        }
        Ok(())
    }

    /// § Number of parts.
    #[must_use]
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    fn dims() -> RoomDims {
        RoomDims::default()
    }

    /// § Empty blueprint : zero parts, zero connections, validates Ok.
    #[test]
    fn empty_blueprint_validates() {
        let bp = Blueprint::new("empty".to_string(), 0);
        assert_eq!(bp.part_count(), 0);
        assert!(bp.connections.is_empty());
        bp.validate().expect("empty blueprint should validate");
    }

    /// § add_part returns increasing ids.
    #[test]
    fn add_part_returns_increasing_ids() {
        let mut bp = Blueprint::new("ids".to_string(), 1);
        let a = bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let b = bp.add_part(RoomKind::ScaleHall, dims(), (10.0, 0.0, 0.0), 0.0);
        let c = bp.add_part(RoomKind::ColorWheel, dims(), (0.0, 0.0, 10.0), 0.0);
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 2);
        assert_eq!(bp.part_count(), 3);
    }

    /// § connect succeeds for valid parts ; fails with UnknownPartId for
    /// missing ids.
    #[test]
    fn connect_validates_part_ids() {
        let mut bp = Blueprint::new("conn".to_string(), 2);
        let a = bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let b = bp.add_part(RoomKind::ScaleHall, dims(), (10.0, 0.0, 0.0), 0.0);
        bp.connect(a, WallSide::E, b, WallSide::W).expect("valid connection");
        assert_eq!(bp.connections.len(), 1);

        // § Unknown id.
        let err = bp
            .connect(a, WallSide::N, 999, WallSide::S)
            .expect_err("unknown to_part should fail");
        assert_eq!(err, BlueprintErr::UnknownPartId(999));
    }

    /// § Self-loop connection rejected.
    #[test]
    fn self_loop_connection_rejected() {
        let mut bp = Blueprint::new("self".to_string(), 3);
        let a = bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let err = bp
            .connect(a, WallSide::N, a, WallSide::S)
            .expect_err("self-loop should fail");
        assert_eq!(err, BlueprintErr::SelfLoopConnection);
        assert!(bp.connections.is_empty());
    }

    /// § Duplicate connections rejected on add ; validate also catches them
    /// if a Blueprint is constructed via direct field access.
    #[test]
    fn duplicate_connection_rejected() {
        let mut bp = Blueprint::new("dup".to_string(), 4);
        let a = bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let b = bp.add_part(RoomKind::ScaleHall, dims(), (10.0, 0.0, 0.0), 0.0);
        bp.connect(a, WallSide::E, b, WallSide::W).expect("first ok");
        let err = bp
            .connect(a, WallSide::E, b, WallSide::W)
            .expect_err("duplicate should fail");
        assert_eq!(err, BlueprintErr::DuplicateConnection);
    }

    /// § Round-trip serde : Blueprint serializes + deserializes losslessly.
    #[test]
    fn blueprint_serde_round_trip() {
        let mut bp = Blueprint::new("rt".to_string(), 5);
        let a = bp.add_part(RoomKind::CalibrationGrid, dims(), (0.0, 0.0, 0.0), 0.0);
        let b = bp.add_part(RoomKind::ColorWheel, dims(), (10.0, 0.0, 0.0), 1.57);
        bp.connect(a, WallSide::N, b, WallSide::S).unwrap();
        let json = serde_json::to_string(&bp).expect("serialize");
        let bp2: Blueprint = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(bp, bp2);
        let json2 = serde_json::to_string(&bp2).unwrap();
        assert_eq!(json, json2);
    }
}

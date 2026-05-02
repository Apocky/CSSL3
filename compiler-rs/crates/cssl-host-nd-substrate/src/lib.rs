// § T11-W19-B-NDSUBSTRATE · cssl-host-nd-substrate
// ══════════════════════════════════════════════════════════════════
// § THESIS : 3D is a CONVENTION not a constraint.
//   Lift the ω-field to N axes where each axis is a SEMANTIC DIMENSION.
//   Stage-0 N = 8 :
//     axis 0 = spatial-X
//     axis 1 = spatial-Y
//     axis 2 = spatial-Z
//     axis 3 = temporal           (now ↔ then)
//     axis 4 = mood               (joy ↔ melancholy)
//     axis 5 = arc-position       (origin ↔ apex)
//     axis 6 = causality          (cause ↔ effect)
//     axis 7 = archetype-affinity (analytic ↔ embodied)
//   Players NAVIGATE non-spatial axes (e.g., walk through "more melancholy"
//   until you arrive at a place). The dimensional-lens itself is substrate-
//   state ; observers ROTATE it via consent.
//
// § PRIME-DIRECTIVE :
//   - lens-projection is consent-gated → DimensionalLens::with_consent()
//   - per-axis bounds are enforced → ConsentError::AxisOutOfRange
//   - default-deny for unspecified axes → CoordError::NotInLens
//
// § STAGE-0 :
//   - HashMap-based sparse cell-store (NdField)
//   - const-generic N : caller chooses 4 / 8 / 16 / ...
//   - LegacyNdCoord type-aliases keep ergonomic call-sites short
// ══════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]

pub mod coord;
pub mod field;
pub mod lens;

pub use coord::{CoordError, NdCoord};
pub use field::{NdField, NdFieldStats};
pub use lens::{ConsentError, DimensionalLens, LensRotation};

/// § Stage-0 default semantic-axis count.
/// 3 spatial + 5 semantic (temporal · mood · arc · causality · archetype).
/// Bumping STAGE0_N requires re-baking lens defaults but the const-generic
/// surface is stable.
pub const STAGE0_N: usize = 8;

/// § Stage-0 type-alias : 8-axis coord with the canonical semantic mapping.
pub type Stage0Coord = NdCoord<{ STAGE0_N }>;

/// § Stage-0 type-alias : 8-axis sparse field over an arbitrary cell-payload `T`.
pub type Stage0Field<T> = NdField<T, { STAGE0_N }>;

/// § Stage-0 axis indices · canonical semantics.
/// Use these to author code that's robust to STAGE0_N bumps.
pub mod axis {
    pub const X: u8 = 0;
    pub const Y: u8 = 1;
    pub const Z: u8 = 2;
    pub const TEMPORAL: u8 = 3;
    pub const MOOD: u8 = 4;
    pub const ARC: u8 = 5;
    pub const CAUSALITY: u8 = 6;
    pub const ARCHETYPE: u8 = 7;
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn stage0_n_is_8() {
        assert_eq!(STAGE0_N, 8);
    }

    #[test]
    fn stage0_coord_has_8_axes() {
        let c: Stage0Coord = NdCoord::origin();
        assert_eq!(c.axes().len(), STAGE0_N);
    }

    #[test]
    fn stage0_field_round_trips() {
        let mut f: Stage0Field<u32> = NdField::new();
        let c: Stage0Coord = NdCoord::origin();
        f.insert(c, 42);
        assert_eq!(f.get(&c), Some(&42));
    }
}

// § damage_types.rs — 8 damage-types × 9 armor-class affinity-matrix
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § DAMAGE-TYPES § AFFINITY-MATRIX  (FROZEN-shape ; numbers may tune)
// § I> legend : Wk=1.5× · Nm=1.0× · Rs=0.6× · Im=0.0× · Ab=-0.5× (heals)
// § I> table = 9 rows (target armor-class) × 8 cols (damage-type)
// § I> apply_affinity : pure-fn ; saturating-mul ; NaN-safe
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// 8 damage-types ; matches GDD enum exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Slash = 0,
    Pierce = 1,
    Crush = 2,
    Fire = 3,
    Frost = 4,
    Shock = 5,
    Holy = 6,
    Void = 7,
}

impl DamageType {
    /// Index into 8-col affinity table.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// 9 target armor-classes ; matches GDD § AFFINITY-MATRIX rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmorClass {
    ClothLight = 0,
    LeatherMed = 1,
    PlateHeavy = 2,
    BoneSkel = 3,
    FleshBeast = 4,
    StoneGolem = 5,
    Ethereal = 6,
    HolyBlessed = 7,
    VoidCursed = 8,
}

impl ArmorClass {
    /// Index into 9-row affinity table.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// 9-row count (target armor-classes).
pub const AFFINITY_ROWS: usize = 9;
/// 8-col count (damage-types).
pub const AFFINITY_COLS: usize = 8;

// Legend constants — FROZEN per GDD.
const WK: f32 = 1.5;
const NM: f32 = 1.0;
const RS: f32 = 0.6;
const IM: f32 = 0.0;
const AB: f32 = -0.5;

/// 9 × 8 affinity-matrix ; pure-data ; bit-equal across hosts.
///
/// Rows : ArmorClass index ; Cols : DamageType index.
/// Order : Sl Pi Cr Fi Fr Sh Ho Vo  /  Cloth Leather Plate Bone Flesh Stone Ethereal Holy Void.
#[allow(clippy::approx_constant)]
pub const AFFINITY_TABLE: [[f32; AFFINITY_COLS]; AFFINITY_ROWS] = [
    // ClothLight   : Nm Wk Rs Wk Rs Nm Nm Rs
    [NM, WK, RS, WK, RS, NM, NM, RS],
    // LeatherMed   : Nm Nm Nm Nm Nm Wk Nm Nm
    [NM, NM, NM, NM, NM, WK, NM, NM],
    // PlateHeavy   : Rs Rs Wk Rs Wk Wk Nm Nm
    [RS, RS, WK, RS, WK, WK, NM, NM],
    // BoneSkel     : Wk Im Wk Nm Rs Nm Wk Nm
    [WK, IM, WK, NM, RS, NM, WK, NM],
    // FleshBeast   : Nm Wk Nm Wk Rs Nm Rs Nm
    [NM, WK, NM, WK, RS, NM, RS, NM],
    // StoneGolem   : Rs Im Wk Rs Im Im Nm Wk
    [RS, IM, WK, RS, IM, IM, NM, WK],
    // Ethereal     : Im Im Rs Nm Wk Wk Wk Nm
    [IM, IM, RS, NM, WK, WK, WK, NM],
    // HolyBlessed  : Nm Nm Nm Rs Nm Nm Ab Wk
    [NM, NM, NM, RS, NM, NM, AB, WK],
    // VoidCursed   : Nm Nm Nm Nm Nm Nm Wk Ab
    [NM, NM, NM, NM, NM, NM, WK, AB],
];

/// Returns the 9×8 affinity table by value (pure-fn ; const-friendly).
#[must_use]
pub const fn affinity_table() -> [[f32; AFFINITY_COLS]; AFFINITY_ROWS] {
    AFFINITY_TABLE
}

/// Apply per-component affinity to base damage.
///
/// `dmg` × `affinity[target_row][src_col]`. Saturating ; NaN clamped to 0.
#[must_use]
pub fn apply_affinity(dmg: f32, src: DamageType, tgt: ArmorClass) -> f32 {
    let factor = AFFINITY_TABLE[tgt.index()][src.index()];
    let raw = dmg * factor;
    if raw.is_finite() {
        raw
    } else {
        0.0
    }
}

/// Composite damage roll : multiple `(DamageType, weight)` components ; final
/// damage = Σ(component-dmg × per-component-affinity). Weights NORMALIZED-or-not
/// at caller's discretion. Saturating ; NaN-safe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DamageRoll {
    /// Components ; FFI-friendly fixed-size pairs (8 max = 1 per type).
    pub components: Vec<(DamageType, f32)>,
}

impl DamageRoll {
    /// Build a single-type roll (e.g. raw sword Slash 100%).
    #[must_use]
    pub fn single(kind: DamageType, magnitude: f32) -> Self {
        let mag = if magnitude.is_finite() {
            magnitude.max(0.0)
        } else {
            0.0
        };
        Self {
            components: vec![(kind, mag)],
        }
    }

    /// Final damage applied to target armor-class.
    #[must_use]
    pub fn apply(&self, tgt: ArmorClass) -> f32 {
        self.components
            .iter()
            .map(|(kind, mag)| apply_affinity(*mag, *kind, tgt))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affinity_table_dims_correct() {
        let t = affinity_table();
        assert_eq!(t.len(), AFFINITY_ROWS);
        assert_eq!(t[0].len(), AFFINITY_COLS);
    }

    #[test]
    fn holy_vs_void_heals_void() {
        // VoidCursed × Holy = Wk (1.5×) per table ; HolyBlessed × Holy = Ab (-0.5×) heals
        let v = apply_affinity(10.0, DamageType::Holy, ArmorClass::VoidCursed);
        assert!((v - 15.0).abs() < 1e-3);
        let h = apply_affinity(10.0, DamageType::Holy, ArmorClass::HolyBlessed);
        assert!((h - -5.0).abs() < 1e-3);
    }

    #[test]
    fn ethereal_immune_to_slash() {
        let d = apply_affinity(50.0, DamageType::Slash, ArmorClass::Ethereal);
        assert!(d.abs() < 1e-3);
    }
}

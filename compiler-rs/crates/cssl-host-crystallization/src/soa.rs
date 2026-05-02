//! § soa — Struct-of-Arrays crystal storage for SIMD-friendly hot-path access.
//! § STAGE-0 STUB · full impl coming W19-J w/ ℂ-HDC adaptation.
#![allow(dead_code)]

use crate::{Crystal, WorldPos};

/// Struct-of-Arrays crystal storage. Stage-0 wraps `Vec<Crystal>` for surface
/// stability ; future iteration replaces with parallel field-Vecs for SIMD.
#[derive(Debug, Default, Clone)]
pub struct CrystalSoA {
    crystals: Vec<Crystal>,
}

impl CrystalSoA {
    pub fn new() -> Self {
        Self { crystals: Vec::new() }
    }

    pub fn from_crystals(c: &[Crystal]) -> Self {
        Self { crystals: c.to_vec() }
    }

    pub fn to_crystals(&self) -> Vec<Crystal> {
        self.crystals.clone()
    }

    pub fn len(&self) -> usize {
        self.crystals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.crystals.is_empty()
    }

    pub fn push(&mut self, c: Crystal) {
        self.crystals.push(c);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Crystal> {
        self.crystals.iter()
    }

    /// Distance² fields for all crystals (ready-for-SIMD). Returns one i64 per crystal.
    pub fn distances_sq_to(&self, observer: WorldPos, out: &mut Vec<i64>) {
        out.clear();
        out.reserve(self.crystals.len());
        for c in &self.crystals {
            out.push(c.dist_sq_mm(observer));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrystalClass;

    #[test]
    fn round_trip_preserves_crystals() {
        let crystals = vec![
            Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1000)),
            Crystal::allocate(CrystalClass::Entity, 2, WorldPos::new(1000, 0, 0)),
        ];
        let soa = CrystalSoA::from_crystals(&crystals);
        let back = soa.to_crystals();
        assert_eq!(back.len(), 2);
        for (a, b) in crystals.iter().zip(back.iter()) {
            assert_eq!(a.fingerprint, b.fingerprint);
        }
    }

    #[test]
    fn distances_sq_match_scalar() {
        let crystals = vec![
            Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(3, 4, 0)),
            Crystal::allocate(CrystalClass::Object, 2, WorldPos::new(0, 0, 5)),
        ];
        let soa = CrystalSoA::from_crystals(&crystals);
        let mut out = Vec::new();
        soa.distances_sq_to(WorldPos::new(0, 0, 0), &mut out);
        assert_eq!(out, vec![25, 25]);
    }
}

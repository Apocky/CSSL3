// § state.rs : tick-tagged simulation-state value-type
//
// `SimState` is the minimum-viable replicated game-state for FPS netcode :
// a sparse map of cell-id → cell-value (each cell tagged with a Σ-mask).
// "Cell" is intentionally abstract — it could be a player-position, a
// projectile, an open-door flag, anything addressable by a small u32 id.
//
// Determinism : `SimState` derives Hash via a sorted-keys path so two states
// produced from the same inputs by deterministic logic compare bit-equal.
//
// ─ Sawyer/Pokémon-OG : cells are Vec<(CellId, CellValue)> kept SORTED-by-id ;
//   binary-search reads ; insertion via `set` is O(log n) lookup + O(n) shift
//   but n is small (< 1024 typical). Hashing is sorted → deterministic.

use core::cmp::Ordering;
use serde::{Deserialize, Serialize};

use crate::sigma::SigmaMask;
use crate::tick::TickId;

/// Cell address ; opaque to this crate. Producer crates pick a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct CellId(pub u32);

/// Cell value-type. Quantized fixed-point I32-vector keeps deltas cheap and
/// determinism crisp. Exotic value-types (strings, structures) are encoded
/// upstream via opaque-byte cells (`Bytes`) ; deltas there are byte-string
/// equality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellValue {
    /// 1-D scalar (Q16.16 fixed-point ; 1.0 = 65536).
    Q16(i32),
    /// 3-D vector (position / velocity / direction).
    V3(i32, i32, i32),
    /// Boolean flag (door-open / weapon-held / etc).
    Flag(bool),
    /// Counter (HP / ammo / kills-in-session).
    Count(u32),
    /// Opaque byte-string (≤ 64 bytes inline) for upstream-managed payloads.
    Bytes(Vec<u8>),
}

impl Eq for CellValue {}

impl core::hash::Hash for CellValue {
    fn hash<H: core::hash::Hasher>(&self, h: &mut H) {
        match self {
            Self::Q16(v) => {
                0u8.hash(h);
                v.hash(h);
            }
            Self::V3(x, y, z) => {
                1u8.hash(h);
                x.hash(h);
                y.hash(h);
                z.hash(h);
            }
            Self::Flag(b) => {
                2u8.hash(h);
                b.hash(h);
            }
            Self::Count(c) => {
                3u8.hash(h);
                c.hash(h);
            }
            Self::Bytes(v) => {
                4u8.hash(h);
                v.hash(h);
            }
        }
    }
}

/// A cell at a given tick : (id, value, mask).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cell {
    pub id: CellId,
    pub value: CellValue,
    pub mask: SigmaMask,
}

/// Tick-tagged simulation snapshot ; sorted-by-id for determinism.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SimState {
    pub tick: TickId,
    cells: Vec<Cell>, // INVARIANT : sorted ascending by `id`
}

impl SimState {
    #[must_use]
    pub fn new(tick: TickId) -> Self {
        Self {
            tick,
            cells: Vec::new(),
        }
    }

    /// Read-only view of the cells (sorted-by-id).
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Look up a cell by id. O(log n) binary search.
    #[must_use]
    pub fn get(&self, id: CellId) -> Option<&Cell> {
        self.cells
            .binary_search_by(|c| c.id.cmp(&id))
            .ok()
            .map(|idx| &self.cells[idx])
    }

    /// Insert or replace a cell. O(n) worst-case shift but n small.
    pub fn set(&mut self, cell: Cell) {
        match self.cells.binary_search_by(|c| c.id.cmp(&cell.id)) {
            Ok(idx) => self.cells[idx] = cell,
            Err(idx) => self.cells.insert(idx, cell),
        }
    }

    /// Remove a cell by id. Returns true if removed.
    pub fn remove(&mut self, id: CellId) -> bool {
        match self.cells.binary_search_by(|c| c.id.cmp(&id)) {
            Ok(idx) => {
                self.cells.remove(idx);
                true
            }
            Err(_) => false,
        }
    }

    /// Compare two states for bit-equality of `cells` (tick ignored).
    #[must_use]
    pub fn cells_eq(&self, other: &Self) -> bool {
        if self.cells.len() != other.cells.len() {
            return false;
        }
        for (a, b) in self.cells.iter().zip(other.cells.iter()) {
            if a.cmp_id(b) != Ordering::Equal || a.value != b.value {
                return false;
            }
        }
        true
    }
}

impl Cell {
    fn cmp_id(&self, other: &Cell) -> Ordering {
        self.id.cmp(&other.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigma::SigmaMask;

    #[test]
    fn set_keeps_sorted_invariant() {
        let mut s = SimState::new(TickId::ZERO);
        s.set(Cell {
            id: CellId(5),
            value: CellValue::Q16(1),
            mask: SigmaMask::public(),
        });
        s.set(Cell {
            id: CellId(2),
            value: CellValue::Q16(2),
            mask: SigmaMask::public(),
        });
        s.set(Cell {
            id: CellId(8),
            value: CellValue::Q16(3),
            mask: SigmaMask::public(),
        });
        let ids: Vec<u32> = s.cells().iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![2, 5, 8]);
    }

    #[test]
    fn set_replaces_existing() {
        let mut s = SimState::new(TickId::ZERO);
        s.set(Cell {
            id: CellId(1),
            value: CellValue::Count(1),
            mask: SigmaMask::public(),
        });
        s.set(Cell {
            id: CellId(1),
            value: CellValue::Count(99),
            mask: SigmaMask::public(),
        });
        assert_eq!(s.len(), 1);
        let v = s.get(CellId(1)).unwrap();
        assert_eq!(v.value, CellValue::Count(99));
    }

    #[test]
    fn cells_eq_ignores_tick() {
        let mut a = SimState::new(TickId(10));
        let mut b = SimState::new(TickId(20));
        let c = Cell {
            id: CellId(1),
            value: CellValue::Flag(true),
            mask: SigmaMask::public(),
        };
        a.set(c.clone());
        b.set(c);
        assert!(a.cells_eq(&b));
    }
}

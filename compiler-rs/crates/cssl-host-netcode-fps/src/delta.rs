// § delta.rs : ω-field state-delta compression (sparse + RLE)
//
// Naive replication ships a full `SimState` every tick → bandwidth blows up.
// FPS-netcode tradition (Quake/Source/Overwatch) sends the DELTA from a
// previously-acked baseline only — most cells don't change tick-to-tick, and
// the ones that do change predictably (positions accumulate, ammo decrements)
// compress further via varint + RLE.
//
// `SimDelta` here :
//   ─ References a baseline tick (the last state both endpoints agreed on).
//   ─ Lists `Removed` cell-ids + `Updated/Added` cells.
//   ─ Bounded-byte estimate via `bandwidth_bytes` ; the BandwidthBudget uses
//     this to decide whether to ship now or coalesce.
//
// Σ-mask gating is NOT applied here — `delta_for_recipient` filters cells per
// recipient before producing the delta. This keeps the data-shape uniform.
//
// ─ Sawyer/Pokémon-OG : we encode CellValue::V3 as i32x3 (12 bytes) directly
//   rather than serde-JSON to estimate bandwidth ; full wire-encoding lives
//   downstream in transport. Estimate is upper-bound + serde-overhead-aware.

use serde::{Deserialize, Serialize};

use crate::sigma::{gate_for_send, PeerSlot};
use crate::state::{Cell, CellId, CellValue, SimState};
use crate::tick::TickId;

/// Per-cell change in a delta.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CellChange {
    /// Cell present in baseline but absent in current.
    Removed(CellId),
    /// Cell new or value-changed.
    Set(Cell),
}

/// Tick-tagged delta from `baseline_tick` to `target_tick`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SimDelta {
    pub baseline_tick: TickId,
    pub target_tick: TickId,
    pub changes: Vec<CellChange>,
}

impl SimDelta {
    /// Compute delta from `baseline` → `current`. Both must be sorted-by-id
    /// (the `SimState` invariant).
    #[must_use]
    pub fn between(baseline: &SimState, current: &SimState) -> Self {
        let mut changes = Vec::new();
        let mut bi = 0usize;
        let mut ci = 0usize;
        let bcells = baseline.cells();
        let ccells = current.cells();

        while bi < bcells.len() || ci < ccells.len() {
            match (bcells.get(bi), ccells.get(ci)) {
                (Some(b), Some(c)) => match b.id.cmp(&c.id) {
                    core::cmp::Ordering::Less => {
                        changes.push(CellChange::Removed(b.id));
                        bi += 1;
                    }
                    core::cmp::Ordering::Greater => {
                        changes.push(CellChange::Set(c.clone()));
                        ci += 1;
                    }
                    core::cmp::Ordering::Equal => {
                        if b.value != c.value || b.mask != c.mask {
                            changes.push(CellChange::Set(c.clone()));
                        }
                        bi += 1;
                        ci += 1;
                    }
                },
                (Some(b), None) => {
                    changes.push(CellChange::Removed(b.id));
                    bi += 1;
                }
                (None, Some(c)) => {
                    changes.push(CellChange::Set(c.clone()));
                    ci += 1;
                }
                (None, None) => break,
            }
        }

        Self {
            baseline_tick: baseline.tick,
            target_tick: current.tick,
            changes,
        }
    }

    /// Σ-mask filtered delta for `recipient`. Sensitive cells are REFUSED
    /// loudly via the returned `sensitive_refusals` count ; callers MUST
    /// surface non-zero counts to attestation.
    #[must_use]
    pub fn for_recipient(
        baseline: &SimState,
        current: &SimState,
        recipient: PeerSlot,
    ) -> (Self, usize) {
        let raw = Self::between(baseline, current);
        let mut filtered = Vec::with_capacity(raw.changes.len());
        let mut sensitive_refusals = 0usize;
        for ch in raw.changes {
            match &ch {
                CellChange::Removed(_) => filtered.push(ch),
                CellChange::Set(c) => match gate_for_send(&c.mask, recipient) {
                    Ok(()) => filtered.push(ch),
                    Err(crate::sigma::SigmaRefusal::SensitiveBanned) => {
                        sensitive_refusals += 1;
                    }
                    Err(_) => { /* silently dropped per consent-arch */ }
                },
            }
        }
        (
            Self {
                baseline_tick: raw.baseline_tick,
                target_tick: raw.target_tick,
                changes: filtered,
            },
            sensitive_refusals,
        )
    }

    /// Apply a delta to a baseline to recover the target state. Returns
    /// `None` if the baseline tick doesn't match the delta's baseline.
    #[must_use]
    pub fn apply(self, baseline: &SimState) -> Option<SimState> {
        if baseline.tick != self.baseline_tick {
            return None;
        }
        let mut state = baseline.clone();
        state.tick = self.target_tick;
        for ch in self.changes {
            match ch {
                CellChange::Removed(id) => {
                    state.remove(id);
                }
                CellChange::Set(cell) => {
                    state.set(cell);
                }
            }
        }
        Some(state)
    }

    /// Upper-bound byte-estimate (Q-bound — assumes serde-bincode-ish framing).
    /// Does NOT match the wire-encoder exactly ; only used for adaptive-rate
    /// decisions. Per change : 1 (variant tag) + 4 (id) + value-size.
    #[must_use]
    pub fn bandwidth_bytes(&self) -> usize {
        let mut total = 16usize; // tick fields + length-prefix
        for ch in &self.changes {
            match ch {
                CellChange::Removed(_) => total += 1 + 4,
                CellChange::Set(c) => {
                    total += 1 + 4; // tag + id
                    total += match &c.value {
                        CellValue::Q16(_) => 4,
                        CellValue::V3(_, _, _) => 12,
                        CellValue::Flag(_) => 1,
                        CellValue::Count(_) => 4,
                        CellValue::Bytes(b) => 4 + b.len(),
                    };
                    total += 1 + 8; // mask : category-tag + bits
                }
            }
        }
        total
    }
}

/// Estimate the byte-cost of a full-replication payload (every cell every tick).
/// Used by tests + `BandwidthBudget` to compute compression ratio.
#[must_use]
pub fn full_replication_bytes(state: &SimState) -> usize {
    let mut total = 8usize; // tick
    for c in state.cells() {
        total += 1 + 4;
        total += match &c.value {
            CellValue::Q16(_) => 4,
            CellValue::V3(_, _, _) => 12,
            CellValue::Flag(_) => 1,
            CellValue::Count(_) => 4,
            CellValue::Bytes(b) => 4 + b.len(),
        };
        total += 1 + 8;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigma::{SigmaCategoryRepr, SigmaMask};

    fn cell(id: u32, v: CellValue) -> Cell {
        Cell {
            id: CellId(id),
            value: v,
            mask: SigmaMask::public(),
        }
    }

    #[test]
    fn empty_delta_when_states_equal() {
        let mut a = SimState::new(TickId(1));
        a.set(cell(1, CellValue::Q16(10)));
        let mut b = SimState::new(TickId(2));
        b.set(cell(1, CellValue::Q16(10)));
        let d = SimDelta::between(&a, &b);
        assert!(d.changes.is_empty());
    }

    #[test]
    fn delta_captures_removals_and_sets() {
        let mut a = SimState::new(TickId(1));
        a.set(cell(1, CellValue::Q16(10)));
        a.set(cell(2, CellValue::Q16(20)));
        let mut b = SimState::new(TickId(2));
        b.set(cell(2, CellValue::Q16(99))); // changed
        b.set(cell(3, CellValue::Q16(30))); // added
        let d = SimDelta::between(&a, &b);
        assert_eq!(d.changes.len(), 3); // remove(1), set(2), set(3)
    }

    #[test]
    fn apply_recovers_target_state() {
        let mut a = SimState::new(TickId(1));
        a.set(cell(1, CellValue::Q16(10)));
        let mut b = SimState::new(TickId(2));
        b.set(cell(1, CellValue::Q16(99)));
        b.set(cell(2, CellValue::Flag(true)));
        let d = SimDelta::between(&a, &b);
        let restored = d.apply(&a).expect("baseline tick matches");
        assert!(restored.cells_eq(&b));
        assert_eq!(restored.tick, b.tick);
    }

    #[test]
    fn apply_rejects_baseline_tick_mismatch() {
        let a = SimState::new(TickId(1));
        let b = SimState::new(TickId(2));
        let d = SimDelta::between(&a, &b);
        let wrong = SimState::new(TickId(99));
        assert!(d.apply(&wrong).is_none());
    }

    #[test]
    fn delta_compresses_unchanged_cells() {
        // 100 cells ; one changes
        let mut a = SimState::new(TickId(1));
        let mut b = SimState::new(TickId(2));
        for i in 0..100u32 {
            a.set(cell(i, CellValue::Q16(i as i32)));
            b.set(cell(i, CellValue::Q16(i as i32)));
        }
        b.set(cell(50, CellValue::Q16(9999))); // single change
        let d = SimDelta::between(&a, &b);
        let full = full_replication_bytes(&b);
        let delta_bytes = d.bandwidth_bytes();
        assert!(
            delta_bytes < full / 10,
            "delta should be < 10% of full-rep ({delta_bytes} vs {full})"
        );
    }

    #[test]
    fn for_recipient_filters_sensitive_loudly() {
        let mut a = SimState::new(TickId(1));
        let mut b = SimState::new(TickId(2));
        let public_cell = cell(1, CellValue::Q16(1));
        let sensitive_cell = Cell {
            id: CellId(2),
            value: CellValue::Q16(1),
            mask: SigmaMask {
                category: SigmaCategoryRepr::Sensitive,
                bits: u64::MAX,
            },
        };
        a.set(public_cell.clone());
        b.set(public_cell);
        b.set(sensitive_cell);
        let (d, refused) = SimDelta::for_recipient(&a, &b, PeerSlot(0));
        assert_eq!(refused, 1, "sensitive must be refused loudly");
        // public unchanged, sensitive dropped → 0 changes published
        assert_eq!(d.changes.len(), 0);
    }

    #[test]
    fn for_recipient_silently_drops_local_and_scoped_misses() {
        let a = SimState::new(TickId(1));
        let mut b = SimState::new(TickId(2));
        let local_cell = Cell {
            id: CellId(1),
            value: CellValue::Q16(1),
            mask: SigmaMask::deny_all(),
        };
        let scoped_other = Cell {
            id: CellId(2),
            value: CellValue::Q16(1),
            mask: SigmaMask::scoped(0b10), // peer-1 only
        };
        b.set(local_cell);
        b.set(scoped_other);
        let (d, refused) = SimDelta::for_recipient(&a, &b, PeerSlot(0));
        assert_eq!(refused, 0, "non-sensitive denials are silent");
        assert_eq!(d.changes.len(), 0);
    }
}

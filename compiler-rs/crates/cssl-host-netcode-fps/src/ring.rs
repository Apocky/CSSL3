// § ring.rs : fixed-capacity ring-buffer indexed-by-tick
//
// Both prediction (input-ring + state-ring) and rollback (frame-ring) need
// fixed-capacity ring storage indexed by `TickId`. Power-of-two capacity
// gives bit-mask ring-index ; non-power-of-two falls back to `%`.
//
// ─ Sawyer/Pokémon-OG : pre-allocated, no heap-churn at runtime ; the
//   `slots` Vec is sized once at construction. Slots track the tick they
//   hold so wrap-around lookups don't return stale data.

use crate::tick::TickId;

/// Slot in a tick-indexed ring : either empty or holds a value tagged with
/// its tick.
#[derive(Debug, Clone)]
struct RingSlot<T> {
    tick: Option<TickId>,
    value: Option<T>,
}

impl<T> Default for RingSlot<T> {
    fn default() -> Self {
        Self {
            tick: None,
            value: None,
        }
    }
}

/// Tick-indexed ring buffer ; capacity fixed at construction.
#[derive(Debug, Clone)]
pub struct TickRing<T> {
    slots: Vec<RingSlot<T>>,
}

impl<T> TickRing<T> {
    /// Construct with `cap` slots. cap must be > 0.
    #[must_use]
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(RingSlot::default());
        }
        Self { slots }
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Insert a value at `tick`. Overwrites whatever was at that ring-slot.
    pub fn set(&mut self, tick: TickId, value: T) {
        let idx = tick.ring_index(self.slots.len());
        self.slots[idx] = RingSlot {
            tick: Some(tick),
            value: Some(value),
        };
    }

    /// Get a value at `tick` if the slot still holds that exact tick.
    /// Returns `None` if the slot has been overwritten or never set.
    #[must_use]
    pub fn get(&self, tick: TickId) -> Option<&T> {
        let idx = tick.ring_index(self.slots.len());
        let slot = &self.slots[idx];
        if slot.tick == Some(tick) {
            slot.value.as_ref()
        } else {
            None
        }
    }

    /// Mutable access to the slot at `tick`, gated by the same tick-equality
    /// check as `get`.
    pub fn get_mut(&mut self, tick: TickId) -> Option<&mut T> {
        let idx = tick.ring_index(self.slots.len());
        let slot = &mut self.slots[idx];
        if slot.tick == Some(tick) {
            slot.value.as_mut()
        } else {
            None
        }
    }

    /// Range iterator from `start` to `end` inclusive ; skips empty/stale slots.
    pub fn range(
        &self,
        start: TickId,
        end: TickId,
    ) -> impl Iterator<Item = (TickId, &T)> + '_ {
        let count = (start.delta(end).max(0) as u32) + 1;
        (0..count).filter_map(move |i| {
            let cur = start.forward(i);
            let idx = cur.ring_index(self.slots.len());
            let slot = &self.slots[idx];
            if slot.tick == Some(cur) {
                slot.value.as_ref().map(|v| (cur, v))
            } else {
                None
            }
        })
    }

    /// Clear all slots.
    pub fn clear(&mut self) {
        for s in &mut self.slots {
            s.tick = None;
            s.value = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get_round_trip() {
        let mut r: TickRing<u32> = TickRing::new(64);
        r.set(TickId(5), 100);
        r.set(TickId(6), 200);
        assert_eq!(r.get(TickId(5)), Some(&100));
        assert_eq!(r.get(TickId(6)), Some(&200));
        assert_eq!(r.get(TickId(99)), None);
    }

    #[test]
    fn overwritten_slot_returns_none_for_old_tick() {
        let mut r: TickRing<u32> = TickRing::new(8);
        r.set(TickId(0), 100);
        r.set(TickId(8), 200); // wraps to slot 0
        assert_eq!(r.get(TickId(0)), None, "old tick stale after wrap-overwrite");
        assert_eq!(r.get(TickId(8)), Some(&200));
    }

    #[test]
    fn range_iterates_present_only() {
        let mut r: TickRing<u32> = TickRing::new(16);
        r.set(TickId(1), 11);
        r.set(TickId(3), 33);
        let collected: Vec<u32> = r
            .range(TickId(1), TickId(4))
            .map(|(_, v)| *v)
            .collect();
        assert_eq!(collected, vec![11, 33]);
    }
}

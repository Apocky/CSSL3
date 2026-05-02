// § snapshot.rs : SnapshotInterpolation — lerp between two server snapshots
//
// Remote-entities (other players, AI, projectiles) are NOT predicted client-
// side ; the client receives periodic server-snapshots and renders entities
// between two adjacent snapshots via linear-interpolation. This is the
// Quake/Source "ent_interp" / "interp" technique.
//
// The interpolation BUFFER trades latency for smoothness : a fixed render-lag
// (typically 100ms) is added so the client always has a snapshot ahead and
// behind to lerp between. With fewer snapshots, motion stutters ; with more
// lag, gunfights feel unresponsive.
//
// Q16.16 fixed-point throughout for determinism + cheap math. We lerp only
// the cell-types where lerp makes sense (Q16, V3) ; everything else snaps to
// the LATER snapshot at frac >= 0.5.

use serde::{Deserialize, Serialize};

use crate::state::{Cell, CellValue, SimState};
use crate::tick::TickId;

/// Default render-lag (ticks). 6 ticks @ 60Hz = 100ms ; matches Source default.
pub const DEFAULT_RENDER_LAG_TICKS: u32 = 6;

/// Snapshot pair to lerp between.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapshotPair {
    pub older: Option<SimState>,
    pub newer: Option<SimState>,
}

impl SnapshotPair {
    /// Update by appending a new snapshot ; older = previous newer ; newer = s.
    pub fn push(&mut self, s: SimState) {
        let prev_newer = self.newer.take();
        match (&self.older, &prev_newer, &s) {
            // First snapshot ever
            (None, None, _) => self.newer = Some(s),
            // Second : becomes the pair
            (None, Some(_), _) => {
                self.older = prev_newer;
                self.newer = Some(s);
            }
            // Steady state : slide window
            (Some(_), Some(_), _) => {
                self.older = prev_newer;
                self.newer = Some(s);
            }
            _ => self.newer = Some(s),
        }
    }

    /// Are both snapshots present ?
    #[must_use]
    pub fn ready(&self) -> bool {
        self.older.is_some() && self.newer.is_some()
    }

    /// Compute the interpolation fraction for `render_tick`.
    /// Returns `Some(frac)` in [0, 1] if `render_tick` lies between the pair.
    /// `frac = 0` = at older ; `frac = 1` = at newer.
    #[must_use]
    pub fn frac_for(&self, render_tick: TickId) -> Option<i32> {
        let older = self.older.as_ref()?;
        let newer = self.newer.as_ref()?;
        let span = older.tick.delta(newer.tick);
        if span <= 0 {
            return None;
        }
        let pos = older.tick.delta(render_tick);
        if pos < 0 || pos > span {
            return None;
        }
        // Q16.16 fraction
        Some(((pos as i64 * 65536) / span as i64) as i32)
    }
}

/// Lerp a SimState pair at the given Q16.16 fraction.
#[must_use]
pub fn lerp_state(
    pair: &SnapshotPair,
    render_tick: TickId,
) -> Option<SimState> {
    let frac = pair.frac_for(render_tick)?;
    let older = pair.older.as_ref()?;
    let newer = pair.newer.as_ref()?;
    let mut out = SimState::new(render_tick);

    // Walk both sorted cell arrays in parallel.
    let mut oi = 0usize;
    let mut ni = 0usize;
    let ocells = older.cells();
    let ncells = newer.cells();
    while oi < ocells.len() || ni < ncells.len() {
        match (ocells.get(oi), ncells.get(ni)) {
            (Some(a), Some(b)) => match a.id.cmp(&b.id) {
                core::cmp::Ordering::Less => {
                    // Cell present only in older — drop (fading out).
                    oi += 1;
                }
                core::cmp::Ordering::Greater => {
                    // New cell in newer — show only when frac >= 0.5.
                    if frac >= 32768 {
                        out.set(b.clone());
                    }
                    ni += 1;
                }
                core::cmp::Ordering::Equal => {
                    out.set(lerp_cell(a, b, frac));
                    oi += 1;
                    ni += 1;
                }
            },
            (Some(_), None) => oi += 1,
            (None, Some(b)) => {
                if frac >= 32768 {
                    out.set(b.clone());
                }
                ni += 1;
            }
            (None, None) => break,
        }
    }
    Some(out)
}

fn lerp_cell(a: &Cell, b: &Cell, frac_q16: i32) -> Cell {
    let value = match (&a.value, &b.value) {
        (CellValue::Q16(x), CellValue::Q16(y)) => CellValue::Q16(lerp_i32(*x, *y, frac_q16)),
        (CellValue::V3(x1, y1, z1), CellValue::V3(x2, y2, z2)) => CellValue::V3(
            lerp_i32(*x1, *x2, frac_q16),
            lerp_i32(*y1, *y2, frac_q16),
            lerp_i32(*z1, *z2, frac_q16),
        ),
        // Non-lerpable : pick newer at frac >= 0.5, older otherwise.
        _ => {
            if frac_q16 >= 32768 {
                b.value.clone()
            } else {
                a.value.clone()
            }
        }
    };
    Cell {
        id: b.id,
        value,
        mask: b.mask, // newer mask wins (consent-direction matters)
    }
}

#[inline]
fn lerp_i32(a: i32, b: i32, frac_q16: i32) -> i32 {
    // a + (b - a) * frac in Q16.16 — promote to i64 to avoid overflow.
    let delta = (b as i64) - (a as i64);
    let scaled = (delta * frac_q16 as i64) >> 16;
    (a as i64 + scaled).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sigma::SigmaMask;
    use crate::state::{CellId, CellValue};

    fn snap(tick: u32, x: i32) -> SimState {
        let mut s = SimState::new(TickId(tick));
        s.set(Cell {
            id: CellId(1),
            value: CellValue::V3(x, 0, 0),
            mask: SigmaMask::public(),
        });
        s
    }

    #[test]
    fn frac_zero_is_older_one_is_newer() {
        let mut p = SnapshotPair::default();
        p.push(snap(10, 0));
        p.push(snap(20, 100));
        assert_eq!(p.frac_for(TickId(10)), Some(0));
        assert_eq!(p.frac_for(TickId(20)), Some(65536));
    }

    #[test]
    fn lerp_midpoint_is_average() {
        let mut p = SnapshotPair::default();
        p.push(snap(10, 0));
        p.push(snap(20, 100));
        let s = lerp_state(&p, TickId(15)).unwrap();
        let v = s.get(CellId(1)).unwrap();
        match v.value {
            CellValue::V3(x, _, _) => assert_eq!(x, 50),
            _ => panic!(),
        }
    }

    #[test]
    fn out_of_range_render_tick_returns_none() {
        let mut p = SnapshotPair::default();
        p.push(snap(10, 0));
        p.push(snap(20, 100));
        assert_eq!(p.frac_for(TickId(5)), None);
        assert_eq!(p.frac_for(TickId(99)), None);
    }

    #[test]
    fn smooth_motion_no_jumps() {
        // Verify Q16.16 lerp doesn't snap : 11 evenly-spaced render-ticks
        // produce monotonically-increasing positions with bounded delta.
        let mut p = SnapshotPair::default();
        p.push(snap(0, 0));
        p.push(snap(10, 1000));
        let mut prev = -1;
        for t in 0..=10u32 {
            let s = lerp_state(&p, TickId(t)).unwrap();
            let v = s.get(CellId(1)).unwrap();
            let x = match v.value {
                CellValue::V3(x, _, _) => x,
                _ => panic!(),
            };
            if prev >= 0 {
                let d = x - prev;
                assert!((0..=110).contains(&d), "bounded step ({d})");
            }
            prev = x;
        }
    }
}

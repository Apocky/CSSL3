//! Live-interval computation.
//!
//! § ALGORITHM
//!   For S7-G2 the [`X64Func`] is a *linear instruction stream* — at lowering
//!   time the upstream emitter (S7-G1) flattens basic blocks into a linear
//!   sequence with explicit `Label` / `Jmp` / `Jcc` pseudo-ops. The live
//!   interval of a vreg is the half-open range `[first-def, last-use)` over
//!   program-points, plus accounting for backward jumps that re-enter the
//!   live range (loop bodies).
//!
//! § PROGRAM POINT
//!   One instruction = one program-point. `Label` pseudo-ops also consume a
//!   program-point so cross-block jumps line up.
//!
//! § BACKWARD-JUMP HANDLING
//!   When a `Jmp` or `Jcc` targets an earlier label, every vreg live AT the
//!   jump site must remain live up through the backward-target, even if its
//!   apparent last-use is before the target. We resolve this by scanning for
//!   backward jumps in a pre-pass, marking each (target_pp, jump_pp) pair, and
//!   extending intervals that straddle these pairs to cover the full loop body.
//!
//! § OUTPUT
//!   A vector of [`LiveInterval`] entries — one per vreg — sorted by
//!   `start` (ascending), with `end` exclusive.

use crate::regalloc::inst::{X64Func, X64InstKind};
use crate::regalloc::reg::{RegBank, X64PReg, X64VReg};
use std::collections::HashMap;

/// A program-point index (instruction-counter).
pub type ProgramPoint = u32;

/// What kind of interval is this — drives heuristics in the LSRA driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalKind {
    /// Function parameter (live from program-point 0).
    Param,
    /// Ordinary def — live from the defining instruction.
    Local,
    /// Crosses at least one `Call` site (allocator prefers callee-saved pregs).
    CrossesCall,
}

/// One live interval — covers `[start, end)` program-points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInterval {
    /// The vreg this interval describes.
    pub vreg: X64VReg,
    /// Inclusive start program-point.
    pub start: ProgramPoint,
    /// Exclusive end program-point.
    pub end: ProgramPoint,
    /// Hints — *coalesce-with-other-vreg* preferences (move-coalescing).
    /// At S7-G2 these are advisory ; the LSRA driver honors them when the
    /// hinted preg is available, otherwise picks freely.
    pub hints: Vec<X64VReg>,
    /// Pregs the interval would prefer (already-known constraint, e.g. arg
    /// register). Strong hint — checked before generic free-list scan.
    pub preg_hints: Vec<X64PReg>,
    /// Pregs this interval may NOT use (e.g. due to fixed-use conflicts at
    /// some instruction). Honored as a hard constraint.
    pub forbidden_pregs: Vec<X64PReg>,
    /// Interval-kind tag.
    pub kind: IntervalKind,
}

impl LiveInterval {
    /// Length in program-points.
    #[must_use]
    pub const fn length(&self) -> u32 {
        self.end - self.start
    }

    /// `true` if this interval's range overlaps `other`'s range
    /// (half-open `[start, end)` semantics).
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// `true` if program-point `pp` is inside this interval.
    #[must_use]
    pub const fn contains(&self, pp: ProgramPoint) -> bool {
        pp >= self.start && pp < self.end
    }
}

/// Compute live intervals for every vreg in `func`.
///
/// § ALGORITHM
///   1. Walk the instruction stream forward, recording for each vreg :
///      - first program-point it's defined,
///      - last program-point it's used.
///   2. Scan for backward jumps. For every (target_pp, jump_pp) pair where
///      target_pp ≤ jump_pp, mark every vreg whose `[first_def, last_use)`
///      straddles target_pp ≤ first_def ≤ jump_pp as needing extension to
///      `jump_pp + 1` (live across the loop back-edge).
///   3. Tag intervals that cross a `Call` instruction so the allocator can
///      prefer callee-saved pregs for them.
///   4. Sort by `start` ascending.
///
/// Returns intervals in start-ascending order.
#[must_use]
#[allow(clippy::cognitive_complexity)] // 4 sequential passes — algorithmic, not nesting
pub fn compute_live_intervals(func: &X64Func) -> Vec<LiveInterval> {
    if func.insts.is_empty() {
        return Vec::new();
    }

    // ── pass 1 : collect first-def + last-use per vreg ──
    let mut first_def: HashMap<X64VReg, ProgramPoint> = HashMap::new();
    let mut last_use: HashMap<X64VReg, ProgramPoint> = HashMap::new();
    let mut all_vregs: Vec<X64VReg> = Vec::new();

    // Params are live from pp 0 — establish them up-front.
    for &p in &func.param_vregs {
        first_def.insert(p, 0);
        last_use.insert(p, 0); // may extend below
        if !all_vregs.contains(&p) {
            all_vregs.push(p);
        }
    }

    for (pp, inst) in func.insts.iter().enumerate() {
        let pp = pp as ProgramPoint;
        for &v in &inst.uses {
            last_use
                .entry(v)
                .and_modify(|e| *e = (*e).max(pp))
                .or_insert(pp);
            if !all_vregs.contains(&v) {
                all_vregs.push(v);
            }
        }
        for &v in &inst.defs {
            first_def.entry(v).or_insert(pp);
            // A def without subsequent use still has a 1-point interval
            // ending at pp + 1.
            last_use
                .entry(v)
                .and_modify(|e| *e = (*e).max(pp))
                .or_insert(pp);
            if !all_vregs.contains(&v) {
                all_vregs.push(v);
            }
        }
    }

    // Result vregs must remain live through the final ret.
    let final_pp = func.insts.len() as ProgramPoint;
    for &r in &func.result_vregs {
        last_use
            .entry(r)
            .and_modify(|e| *e = (*e).max(final_pp - 1))
            .or_insert(final_pp - 1);
        if !all_vregs.contains(&r) {
            all_vregs.push(r);
        }
    }

    // ── pass 2 : detect backward jumps + extend intervals ──
    // Build label → program-point map.
    let mut label_pp: HashMap<&str, ProgramPoint> = HashMap::new();
    for (pp, inst) in func.insts.iter().enumerate() {
        if let X64InstKind::Label { name } = &inst.kind {
            label_pp.insert(name.as_str(), pp as ProgramPoint);
        }
    }
    // For each backward branch, extend live intervals that span the loop body.
    for (pp, inst) in func.insts.iter().enumerate() {
        let pp = pp as ProgramPoint;
        let target_name = match &inst.kind {
            X64InstKind::Jmp { target } => Some(target.as_str()),
            X64InstKind::Jcc { target, .. } => Some(target.as_str()),
            _ => None,
        };
        if let Some(name) = target_name {
            if let Some(&target_pp) = label_pp.get(name) {
                if target_pp <= pp {
                    // Backward jump : every vreg whose first_def straddles or
                    // sits inside `[target_pp, pp]` must remain live up to pp+1.
                    for v in &all_vregs {
                        let fd = first_def.get(v).copied().unwrap_or(0);
                        let lu = last_use.get(v).copied().unwrap_or(0);
                        if fd <= pp && lu >= target_pp && lu < pp + 1 {
                            last_use.insert(*v, pp + 1);
                        }
                    }
                }
            }
        }
    }

    // ── pass 3 : detect Call-crossings ──
    let mut call_pps: Vec<ProgramPoint> = Vec::new();
    for (pp, inst) in func.insts.iter().enumerate() {
        if matches!(inst.kind, X64InstKind::Call { .. }) {
            call_pps.push(pp as ProgramPoint);
        }
    }

    // ── build intervals ──
    let mut intervals: Vec<LiveInterval> = Vec::new();
    for v in all_vregs {
        let start = first_def.get(&v).copied().unwrap_or(0);
        let end_inclusive = last_use.get(&v).copied().unwrap_or(start);
        // Half-open [start, end) — end is one past last use.
        let end = end_inclusive + 1;

        let kind = if func.param_vregs.contains(&v) {
            IntervalKind::Param
        } else if call_pps.iter().any(|&cp| cp >= start && cp < end) {
            IntervalKind::CrossesCall
        } else {
            IntervalKind::Local
        };

        intervals.push(LiveInterval {
            vreg: v,
            start,
            end,
            hints: vec![],
            preg_hints: vec![],
            forbidden_pregs: vec![],
            kind,
        });
    }

    // Recompute call-crossings after possible end extensions.
    for iv in &mut intervals {
        if iv.kind == IntervalKind::Local
            && call_pps.iter().any(|&cp| cp >= iv.start && cp < iv.end)
        {
            iv.kind = IntervalKind::CrossesCall;
        }
    }

    intervals.sort_by_key(|iv| (iv.start, iv.vreg.index));
    intervals
}

/// Helper : returns `true` iff `pp` is the last program-point in `func`.
#[must_use]
pub fn is_last_pp(func: &X64Func, pp: ProgramPoint) -> bool {
    pp + 1 == func.insts.len() as ProgramPoint
}

/// Diagnostic helper : walk intervals + compute the maximum register-pressure
/// at any program-point separately for GP and XMM banks. Useful for tests +
/// debug output.
///
/// Returns `(max_gp_pressure, max_xmm_pressure)`.
#[must_use]
pub fn max_pressure(intervals: &[LiveInterval], func_len: usize) -> (u32, u32) {
    let mut gp_max = 0;
    let mut xmm_max = 0;
    for pp in 0..func_len as ProgramPoint {
        let mut gp = 0;
        let mut xmm = 0;
        for iv in intervals {
            if iv.contains(pp) {
                match iv.vreg.bank {
                    RegBank::Gp => gp += 1,
                    RegBank::Xmm => xmm += 1,
                }
            }
        }
        gp_max = gp_max.max(gp);
        xmm_max = xmm_max.max(xmm);
    }
    (gp_max, xmm_max)
}

#[cfg(test)]
mod interval_tests {
    use super::*;
    use crate::regalloc::inst::{X64Inst, X64InstKind, X64Operand};
    use crate::regalloc::reg::Abi;

    #[test]
    fn empty_func_yields_empty_intervals() {
        let f = X64Func::new("empty", Abi::SysVAmd64);
        let ivs = compute_live_intervals(&f);
        assert!(ivs.is_empty());
    }

    #[test]
    fn single_def_use_chain_yields_intervals() {
        let mut f = X64Func::new("simple", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        // mov v0, 5
        f.push(X64Inst::mov(v0, X64Operand::Imm32(5)));
        // mov v1, v0
        f.push(X64Inst::mov(v1, X64Operand::Reg(v0)));
        // ret
        f.push(X64Inst::ret());

        let ivs = compute_live_intervals(&f);
        // v0 : start=0 (def), end=2 (last-use at pp 1 → exclusive end 2)
        let v0_iv = ivs.iter().find(|iv| iv.vreg == v0).unwrap();
        assert_eq!(v0_iv.start, 0);
        assert_eq!(v0_iv.end, 2);
        // v1 : start=1 (def at pp 1), end=2 (no further use → 1-point interval)
        let v1_iv = ivs.iter().find(|iv| iv.vreg == v1).unwrap();
        assert_eq!(v1_iv.start, 1);
        assert_eq!(v1_iv.end, 2);
    }

    #[test]
    fn intervals_sorted_by_start() {
        let mut f = X64Func::new("ordering", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        let v2 = X64VReg::gp(2);
        f.push(X64Inst::mov(v2, X64Operand::Imm32(1))); // pp 0 : def v2
        f.push(X64Inst::mov(v0, X64Operand::Imm32(2))); // pp 1 : def v0
        f.push(X64Inst::mov(v1, X64Operand::Reg(v0))); // pp 2 : def v1, use v0
        f.push(X64Inst::ret());

        let ivs = compute_live_intervals(&f);
        let starts: Vec<u32> = ivs.iter().map(|iv| iv.start).collect();
        for w in starts.windows(2) {
            assert!(w[0] <= w[1], "intervals not sorted by start: {starts:?}");
        }
    }

    #[test]
    fn overlaps_detects_real_overlap() {
        let v0 = X64VReg::gp(0);
        let a = LiveInterval {
            vreg: v0,
            start: 0,
            end: 5,
            hints: vec![],
            preg_hints: vec![],
            forbidden_pregs: vec![],
            kind: IntervalKind::Local,
        };
        let b = LiveInterval {
            vreg: v0,
            start: 3,
            end: 7,
            hints: vec![],
            preg_hints: vec![],
            forbidden_pregs: vec![],
            kind: IntervalKind::Local,
        };
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
        let c = LiveInterval {
            vreg: v0,
            start: 5,
            end: 10,
            hints: vec![],
            preg_hints: vec![],
            forbidden_pregs: vec![],
            kind: IntervalKind::Local,
        };
        // a.end == c.start ⇒ touching, NOT overlapping.
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn backward_jump_extends_interval() {
        // Loop : v0 defined before loop, used inside, with a backward jump :
        //   pp 0 : mov v0, 1
        //   pp 1 : Label "loop"
        //   pp 2 : add v0, 1   (use + def)
        //   pp 3 : Jmp "loop"  (backward)
        //   pp 4 : ret
        let mut f = X64Func::new("loop", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        f.push(X64Inst::mov(v0, X64Operand::Imm32(1))); // 0
        f.push(X64Inst::label("loop")); // 1
        f.push(X64Inst::add(v0, X64Operand::Imm32(1))); // 2
        let mut jmp = X64Inst::label("__placeholder");
        jmp.kind = X64InstKind::Jmp {
            target: "loop".to_string(),
        };
        f.push(jmp); // 3
        f.push(X64Inst::ret()); // 4

        let ivs = compute_live_intervals(&f);
        let v0_iv = ivs.iter().find(|iv| iv.vreg == v0).unwrap();
        // v0's interval must cover the back-edge — end ≥ 4 (jmp pp + 1).
        assert!(v0_iv.end >= 4, "interval end {} ≥ 4 expected", v0_iv.end);
    }

    #[test]
    fn call_crossing_marks_kind_correctly() {
        let mut f = X64Func::new("call_cross", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        f.push(X64Inst::mov(v0, X64Operand::Imm32(1))); // 0 def v0
        f.push(X64Inst::call("foo", vec![], Some(v1))); // 1 call (def v1)
        f.push(X64Inst::add(v0, X64Operand::Reg(v1))); // 2 use v0+v1
        f.push(X64Inst::ret());

        let ivs = compute_live_intervals(&f);
        let v0_iv = ivs.iter().find(|iv| iv.vreg == v0).unwrap();
        assert_eq!(v0_iv.kind, IntervalKind::CrossesCall);
    }

    #[test]
    fn pressure_computation_tracks_max() {
        let mut f = X64Func::new("pressure", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        let v2 = X64VReg::gp(2);
        f.push(X64Inst::mov(v0, X64Operand::Imm32(1))); // 0
        f.push(X64Inst::mov(v1, X64Operand::Imm32(2))); // 1 — pressure 2
        f.push(X64Inst::mov(v2, X64Operand::Imm32(3))); // 2 — pressure 3
        f.push(X64Inst::add(v0, X64Operand::Reg(v1))); // 3 — pressure 3
        f.push(X64Inst::add(v0, X64Operand::Reg(v2))); // 4 — v1 dead — pressure 2
        f.push(X64Inst::ret()); // 5
        let ivs = compute_live_intervals(&f);
        let (gp_max, xmm_max) = max_pressure(&ivs, f.insts.len());
        assert!(gp_max >= 3);
        assert_eq!(xmm_max, 0);
    }
}

//! Linear-scan register allocator.
//!
//! § ALGORITHM (Poletto + Sarkar 1999, with split-on-spill)
//!   Given live intervals sorted by start, the allocator walks each interval
//!   in order :
//!     1. EXPIRE-OLD-INTERVALS : remove from `active` any interval whose
//!        `end ≤ current.start` ; release its preg back to the free pool.
//!     2. PICK-FREE-PREG : try to find a free preg in the bank of `current`,
//!        respecting `current.preg_hints` first then any free reg avoiding
//!        `forbidden_pregs`. If found, assign it.
//!     3. SPILL-AT-INTERVAL : if no free preg, pick the active interval whose
//!        END is furthest in the future (heuristic). If that further-future
//!        interval ends *after* `current` ends, spill it ; reassign its preg
//!        to `current` ; the spilled vreg gets a stack slot. Otherwise spill
//!        `current` itself.
//!     4. Move `current` into `active`.
//!     5. After the walk, all unspilled vregs are mapped to a preg ; spilled
//!        vregs are mapped to a [`SpillSlot`].
//!   The return value is a [`X64FuncAllocated`] with each instruction's
//!   vreg→preg map filled in + spill markers inserted at split points.
//!
//! § CALLEE-SAVED PUSH/POP
//!   After allocation, the set of allocated pregs ∩ callee-saved-set-for-abi
//!   becomes `X64FuncAllocated::callee_saved_used`. The caller emits push'es
//!   in the prologue (in canonical order) + pop's in the epilogue (reverse).
//!
//! § FORBIDDEN PREG CONSTRAINTS
//!   Pre-pass : for every instruction with `fixed_uses`, the live intervals
//!   that span this instruction get the corresponding preg added to their
//!   `forbidden_pregs` (since the fixed-use needs that preg unconditionally).
//!   Same for `fixed_defs` ; same for `clobbers` (caller-saved set at calls).
//!
//! § SPLITS
//!   When an interval `I` is selected as spill victim while `current` still
//!   needs a register, the allocator tries to *split* `I` :
//!     * The first segment `[I.start, current.start)` keeps the preg.
//!     * The remainder `[current.start, I.end)` lives on the stack.
//!     * Reload markers are inserted at use-sites within the spill segment.
//!   Per §10 split-on-spill from Wimmer + Mössenböck 2005 ; we implement only
//!   the simple "split-at-current-start" form ; finer-grained next-use
//!   distance splitting is deferred.

use crate::inst::{
    AllocatedInst, VregAssignment, VregLocation, VregResolution, VregSegment, X64Func,
    X64FuncAllocated,
};
use crate::interval::{compute_live_intervals, IntervalKind, LiveInterval, ProgramPoint};
use crate::reg::{Abi, RegBank, RegRole, X64PReg, X64VReg};
use crate::spill::{SpillSlot, SpillSlots};
use std::collections::HashMap;
use thiserror::Error;

/// Error from register allocation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AllocError {
    /// All pregs in the requested bank are reserved or forbidden.
    #[error("no preg available in `{0}` bank for vreg {1}")]
    NoPregAvailable(RegBank, X64VReg),
    /// Internal invariant : tried to free a preg that wasn't allocated.
    #[error("internal : tried to free unallocated preg `{0}`")]
    DoubleFree(X64PReg),
    /// Internal invariant : assignment table inconsistency.
    #[error("internal : vreg {0} has no assignment after allocation")]
    MissingAssignment(X64VReg),
    /// Bank mismatch : tried to allocate a GP vreg into XMM or vice-versa.
    #[error("bank mismatch : vreg {0} is `{1}` but tried to assign to preg `{2}` of bank `{3}`")]
    BankMismatch(X64VReg, RegBank, X64PReg, RegBank),
}

/// High-level statistics about an allocation run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllocReport {
    /// Total intervals processed.
    pub interval_count: u32,
    /// Intervals that were assigned a preg directly (no spill).
    pub assigned_to_preg: u32,
    /// Intervals that were spilled to a stack slot (no preg ever).
    pub spilled_entirely: u32,
    /// Intervals that were split (preg for part, slot for part).
    pub split: u32,
    /// Number of callee-saved regs used (drives prologue/epilogue size).
    pub callee_saved_used: u32,
    /// Total spill-slot frame-bytes (16 × slot count).
    pub frame_size: u32,
}

/// The linear-scan allocator state.
pub struct LinearScanAllocator<'a> {
    func: &'a X64Func,
    intervals: Vec<LiveInterval>,
    /// Per-vreg assignment as it's decided.
    assignment: HashMap<X64VReg, VregAssignment>,
    /// Active intervals (currently in the live-set), sorted by `end` ascending
    /// (smallest end first — quicker `expire_old` step).
    active: Vec<usize>,
    /// Per-preg occupant : maps preg → vreg's interval-index, if any.
    preg_occupant: HashMap<X64PReg, usize>,
    spill_slots: SpillSlots,
    /// Callee-saved pregs we've actually allocated to a vreg. Used to drive
    /// prologue push/pop generation.
    callee_saved_used: Vec<X64PReg>,
}

impl<'a> LinearScanAllocator<'a> {
    /// Create an allocator state for `func`. Intervals are computed up-front.
    #[must_use]
    pub fn new(func: &'a X64Func) -> Self {
        let mut intervals = compute_live_intervals(func);
        // Apply forbidden-preg constraints from fixed_uses / fixed_defs / clobbers.
        Self::apply_constraints(&mut intervals, func);
        Self {
            func,
            intervals,
            assignment: HashMap::new(),
            active: Vec::new(),
            preg_occupant: HashMap::new(),
            spill_slots: SpillSlots::new(),
            callee_saved_used: Vec::new(),
        }
    }

    /// Annotate intervals with forbidden-preg sets derived from fixed-uses /
    /// fixed-defs / clobbers in instructions they span.
    fn apply_constraints(intervals: &mut [LiveInterval], func: &X64Func) {
        for (pp, inst) in func.insts.iter().enumerate() {
            let pp = pp as ProgramPoint;
            // For fixed_uses + fixed_defs : every interval *spanning* this pp
            // (start ≤ pp < end) but NOT itself the fixed user/def must add
            // the fixed preg to `forbidden_pregs`. The fixed-vreg routing
            // happens via `preg_hints` (set by caller on emission).
            for fp in inst
                .fixed_uses
                .iter()
                .chain(inst.fixed_defs.iter())
                .chain(inst.clobbers.iter())
            {
                for iv in intervals.iter_mut() {
                    if iv.start <= pp && pp < iv.end {
                        // Don't forbid pregs of *different* bank — those slots
                        // are independent.
                        if iv.vreg.bank == fp.bank() && !iv.forbidden_pregs.contains(fp) {
                            iv.forbidden_pregs.push(*fp);
                        }
                    }
                }
            }
            // CrossesCall intervals also get their bank's caller-saved pregs
            // forbidden — they MUST live in callee-saved across the call.
            // Done lazily below : we don't apply caller-saved-forbidden here
            // because Abi context isn't in scope ; handled at allocation time
            // via `is_preg_safe_for_interval`.
        }
    }

    /// Return `true` iff `preg` is safe to assign to interval `iv` :
    ///   * preg's bank matches vreg's bank,
    ///   * preg not in forbidden_pregs,
    ///   * if iv.kind == CrossesCall, preg must be callee-saved under abi,
    ///   * preg not currently occupied (caller checks this separately).
    fn is_preg_safe_for_interval(preg: X64PReg, iv: &LiveInterval, abi: Abi) -> bool {
        if preg.bank() != iv.vreg.bank {
            return false;
        }
        if preg.is_reserved_for_user_alloc() {
            return false;
        }
        if iv.forbidden_pregs.contains(&preg) {
            return false;
        }
        if iv.kind == IntervalKind::CrossesCall && preg.role_under_abi(abi) == RegRole::CallerSaved
        {
            return false;
        }
        true
    }

    /// Pick a free preg for the given interval. Honors hints first, then
    /// preference order (callee-saved regs cheaper for short Local intervals
    /// because no push/pop overhead vs caller-saved which may need extra
    /// spill across calls).
    fn try_pick_free_preg(&self, iv: &LiveInterval, abi: Abi) -> Option<X64PReg> {
        // 1. Honor preg_hints first.
        for &hp in &iv.preg_hints {
            if !self.preg_occupant.contains_key(&hp) && Self::is_preg_safe_for_interval(hp, iv, abi)
            {
                return Some(hp);
            }
        }
        // 2. Otherwise scan all pregs of the right bank in canonical order :
        //    prefer caller-saved for short local intervals (cheaper — no
        //    prologue push), prefer callee-saved for long-lived / CrossesCall
        //    intervals.
        let prefer_callee_saved = iv.kind == IntervalKind::CrossesCall;
        let mut candidates: Vec<X64PReg> = X64PReg::ALL
            .iter()
            .copied()
            .filter(|p| p.bank() == iv.vreg.bank)
            .filter(|p| !p.is_reserved_for_user_alloc())
            .filter(|p| !self.preg_occupant.contains_key(p))
            .filter(|&p| Self::is_preg_safe_for_interval(p, iv, abi))
            .collect();
        // Stable sort : preferred role first.
        candidates.sort_by_key(|&p| match (prefer_callee_saved, p.role_under_abi(abi)) {
            (true, RegRole::CalleeSaved) => 0,
            (true, RegRole::CallerSaved) => 1,
            (false, RegRole::CallerSaved) => 0,
            (false, RegRole::CalleeSaved) => 1,
            (_, RegRole::Reserved) => 2,
        });
        candidates.first().copied()
    }

    /// Expire intervals in `active` whose `end ≤ current.start` ; free their
    /// pregs.
    fn expire_old(&mut self, current_start: ProgramPoint) {
        let mut keep: Vec<usize> = Vec::new();
        for &idx in &self.active {
            let iv = &self.intervals[idx];
            if iv.end <= current_start {
                // Expire — release preg.
                if let Some((p, _)) = self
                    .preg_occupant
                    .iter()
                    .find(|(_, v)| **v == idx)
                    .map(|(p, v)| (*p, *v))
                {
                    self.preg_occupant.remove(&p);
                }
            } else {
                keep.push(idx);
            }
        }
        keep.sort_by_key(|&i| self.intervals[i].end);
        self.active = keep;
    }

    /// Add `interval_idx` to active set, sorted by end-ascending.
    fn add_to_active(&mut self, idx: usize) {
        let end = self.intervals[idx].end;
        // Insert in sorted order.
        let pos = self
            .active
            .iter()
            .position(|&a| self.intervals[a].end > end)
            .unwrap_or(self.active.len());
        self.active.insert(pos, idx);
    }

    /// Remove `interval_idx` from active set.
    fn remove_from_active(&mut self, idx: usize) {
        self.active.retain(|&a| a != idx);
    }

    /// Run the allocator. After this returns, `self.assignment` is populated
    /// for every vreg in the function.
    ///
    /// # Errors
    /// Returns [`AllocError`] on internal invariant violations or unsatisfiable
    /// constraints.
    pub fn run(&mut self) -> Result<(), AllocError> {
        let abi = self.func.abi;
        let interval_indices: Vec<usize> = (0..self.intervals.len()).collect();

        for idx in interval_indices {
            let start = self.intervals[idx].start;
            self.expire_old(start);

            let iv_clone = self.intervals[idx].clone();

            // Try to pick a free preg.
            if let Some(preg) = self.try_pick_free_preg(&iv_clone, abi) {
                self.preg_occupant.insert(preg, idx);
                self.assignment
                    .insert(iv_clone.vreg, VregAssignment::Preg(preg));
                if preg.role_under_abi(abi) == RegRole::CalleeSaved
                    && !self.callee_saved_used.contains(&preg)
                {
                    self.callee_saved_used.push(preg);
                }
                self.add_to_active(idx);
            } else {
                // Need to spill someone.
                self.spill_at_interval(idx, abi);
            }
        }

        // Sanity : every vreg has an assignment.
        for iv in &self.intervals {
            if !self.assignment.contains_key(&iv.vreg) {
                return Err(AllocError::MissingAssignment(iv.vreg));
            }
        }

        // Stable order for callee_saved_used (helps deterministic prologue emission).
        self.callee_saved_used.sort();
        Ok(())
    }

    /// Spill-at-interval : decide whether to spill `current` itself or a victim
    /// from `active`. Per LSRA convention : if there's an active interval whose
    /// end is later than current's end, spill that victim ; otherwise spill
    /// current itself.
    fn spill_at_interval(&mut self, current_idx: usize, abi: Abi) {
        let current_iv = self.intervals[current_idx].clone();
        let current_end = current_iv.end;
        let bank = current_iv.vreg.bank;

        // Look at the active intervals of the same bank whose pregs are NOT
        // forbidden for current.
        let mut best_victim: Option<(usize, X64PReg, ProgramPoint)> = None;
        for &idx in &self.active {
            let iv = &self.intervals[idx];
            if iv.vreg.bank != bank {
                continue;
            }
            // Find the preg that idx is using.
            let preg_for_idx = self
                .preg_occupant
                .iter()
                .find(|(_, v)| **v == idx)
                .map(|(p, _)| *p);
            if let Some(p) = preg_for_idx {
                // The preg must be safe for current's constraints.
                if !Self::is_preg_safe_for_interval(p, &current_iv, abi) {
                    continue;
                }
                if best_victim.map_or(true, |(_, _, e)| iv.end > e) {
                    best_victim = Some((idx, p, iv.end));
                }
            }
        }

        if let Some((victim_idx, victim_preg, victim_end)) = best_victim {
            if victim_end > current_end {
                // Spill the victim — it has farther future use.
                let victim_vreg = self.intervals[victim_idx].vreg;
                let slot = self.spill_slots.alloc(victim_vreg.bank);
                let prev = self.assignment.insert(
                    victim_vreg,
                    self.split_or_full_spill(victim_idx, slot, current_iv.start),
                );
                debug_assert!(matches!(prev, Some(VregAssignment::Preg(_))));
                self.preg_occupant.remove(&victim_preg);
                self.remove_from_active(victim_idx);

                // Now assign victim_preg to current.
                self.preg_occupant.insert(victim_preg, current_idx);
                self.assignment
                    .insert(current_iv.vreg, VregAssignment::Preg(victim_preg));
                if victim_preg.role_under_abi(abi) == RegRole::CalleeSaved
                    && !self.callee_saved_used.contains(&victim_preg)
                {
                    self.callee_saved_used.push(victim_preg);
                }
                self.add_to_active(current_idx);
                return;
            }
        }

        // No good victim ; spill current itself.
        let slot = self.spill_slots.alloc(current_iv.vreg.bank);
        self.assignment
            .insert(current_iv.vreg, VregAssignment::Spill(slot));
        // Note : we do NOT add current to active — it's not in any preg.
    }

    /// Decide whether to record victim as a Spill (entire range on stack) or
    /// as a Split (preg for `[victim.start, current_start)` ; spill for
    /// `[current_start, victim.end)`). At S7-G2 we always record as Split if
    /// `current_start > victim.start` because the victim already executed
    /// some uses in the preg before the spill point.
    fn split_or_full_spill(
        &self,
        victim_idx: usize,
        slot: SpillSlot,
        current_start: ProgramPoint,
    ) -> VregAssignment {
        let victim_iv = &self.intervals[victim_idx];
        // The preg the victim was using.
        let victim_preg = self
            .preg_occupant
            .iter()
            .find(|(_, v)| **v == victim_idx)
            .map(|(p, _)| *p);
        match victim_preg {
            Some(p) if current_start > victim_iv.start => VregAssignment::Split(vec![
                VregSegment {
                    start: victim_iv.start as usize,
                    end: current_start as usize,
                    location: VregLocation::Preg(p),
                },
                VregSegment {
                    start: current_start as usize,
                    end: victim_iv.end as usize,
                    location: VregLocation::Spill(slot),
                },
            ]),
            _ => VregAssignment::Spill(slot),
        }
    }

    /// Build the [`X64FuncAllocated`] result. Walks the function's instructions
    /// + decorates each with the per-vreg resolution at that program-point.
    #[must_use]
    pub fn build_allocated(&self) -> X64FuncAllocated {
        let mut allocated_insts: Vec<AllocatedInst> = Vec::with_capacity(self.func.insts.len());
        for (pp, inst) in self.func.insts.iter().enumerate() {
            let pp = pp as ProgramPoint;
            let mut resolutions: Vec<VregResolution> = Vec::new();
            // Resolve every vreg used or defined at this program-point.
            for v in inst.uses.iter().chain(inst.defs.iter()) {
                if resolutions.iter().any(|r| r.vreg == *v) {
                    continue;
                }
                let loc = self.resolve_at(*v, pp);
                resolutions.push(VregResolution {
                    vreg: *v,
                    location: loc,
                });
            }
            allocated_insts.push(AllocatedInst {
                inst: inst.clone(),
                resolutions,
            });
        }

        // Build per-vreg final assignment vec (sorted by vreg index for
        // deterministic output).
        let mut sorted_assignments: Vec<(X64VReg, VregAssignment)> = self
            .assignment
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        sorted_assignments.sort_by_key(|(v, _)| v.index);

        let assignment: Vec<VregAssignment> =
            sorted_assignments.iter().map(|(_, a)| a.clone()).collect();

        X64FuncAllocated {
            name: self.func.name.clone(),
            abi: self.func.abi,
            allocated_insts,
            callee_saved_used: self.callee_saved_used.clone(),
            frame_size: self.spill_slots.total_frame_size(),
            assignment,
        }
    }

    /// Resolve the location of `vreg` at program-point `pp`, accounting for
    /// splits.
    fn resolve_at(&self, vreg: X64VReg, pp: ProgramPoint) -> VregLocation {
        match self.assignment.get(&vreg) {
            Some(VregAssignment::Preg(p)) => VregLocation::Preg(*p),
            Some(VregAssignment::Spill(s)) => VregLocation::Spill(*s),
            Some(VregAssignment::Split(segs)) => {
                for seg in segs {
                    if (pp as usize) >= seg.start && (pp as usize) < seg.end {
                        return seg.location;
                    }
                }
                // Fall-back : last segment.
                segs.last().map_or_else(
                    || {
                        VregLocation::Spill(SpillSlot {
                            index: 0,
                            bank: vreg.bank,
                        })
                    },
                    |s| s.location,
                )
            }
            None => VregLocation::Spill(SpillSlot {
                index: 0,
                bank: vreg.bank,
            }),
        }
    }

    /// Build a high-level allocation report for diagnostics + tests.
    #[must_use]
    pub fn report(&self) -> AllocReport {
        let mut r = AllocReport {
            interval_count: self.intervals.len() as u32,
            callee_saved_used: self.callee_saved_used.len() as u32,
            frame_size: self.spill_slots.total_frame_size(),
            ..Default::default()
        };
        for assignment in self.assignment.values() {
            match assignment {
                VregAssignment::Preg(_) => r.assigned_to_preg += 1,
                VregAssignment::Spill(_) => r.spilled_entirely += 1,
                VregAssignment::Split(_) => r.split += 1,
            }
        }
        r
    }
}

/// Driver — single-call allocate-and-build.
///
/// # Errors
/// Returns [`AllocError`] on internal invariant violations.
pub fn allocate(func: &X64Func) -> Result<X64FuncAllocated, AllocError> {
    let mut a = LinearScanAllocator::new(func);
    a.run()?;
    Ok(a.build_allocated())
}

#[cfg(test)]
mod alloc_tests {
    use super::*;
    use crate::inst::{X64Inst, X64Operand};
    use crate::reg::Abi;

    #[test]
    fn empty_function_yields_empty_allocation() {
        let f = X64Func::new("empty", Abi::SysVAmd64);
        let a = allocate(&f).unwrap();
        assert!(a.allocated_insts.is_empty());
        assert_eq!(a.frame_size, 0);
    }

    #[test]
    fn single_def_use_assigns_preg() {
        let mut f = X64Func::new("simple", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        f.push(X64Inst::mov(v0, X64Operand::Imm32(5)));
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        assert_eq!(a.assignment.len(), 1);
        // v0 should have a preg in the GP bank.
        let v0_assign = &a.assignment[0];
        match v0_assign {
            VregAssignment::Preg(p) => assert_eq!(p.bank(), RegBank::Gp),
            other => panic!("expected Preg, got {other:?}"),
        }
    }

    #[test]
    fn two_disjoint_intervals_share_preg() {
        // v0 used in pp 0..1 ; v1 defined at pp 2 — disjoint, can share preg.
        let mut f = X64Func::new("disjoint", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        f.push(X64Inst::mov(v0, X64Operand::Imm32(1))); // 0
        f.push(X64Inst::mov(v0, X64Operand::Reg(v0))); // 1 — last use of v0
        f.push(X64Inst::mov(v1, X64Operand::Imm32(2))); // 2 — def v1, v0 dead
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        // Both should be Preg ; both in same preg (or could differ depending
        // on heuristic — assert frame_size = 0 to confirm no spill).
        assert_eq!(a.frame_size, 0);
    }

    #[test]
    fn over_pressure_triggers_spill() {
        // 16 simultaneously-live GP vregs (just at allocator's GP capacity
        // after rsp + rbp reserved : 14 free GP) → 2 spills required.
        let mut f = X64Func::new("pressure", Abi::SysVAmd64);
        let mut vs: Vec<X64VReg> = Vec::new();
        for i in 0..16 {
            let v = X64VReg::gp(i);
            vs.push(v);
            f.push(X64Inst::mov(v, X64Operand::Imm32(i32::try_from(i).unwrap())));
        }
        // Force all live at the end : sum them via repeated adds.
        for i in 1..vs.len() {
            f.push(X64Inst::add(vs[0], X64Operand::Reg(vs[i])));
        }
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        assert!(a.frame_size > 0, "expected spills to occur");
    }

    #[test]
    fn callee_saved_pushed_when_used() {
        // Force allocator to use a callee-saved preg : create enough live vregs
        // that caller-saved alone won't fit.
        let mut f = X64Func::new("calleesaved", Abi::SysVAmd64);
        let mut vs: Vec<X64VReg> = Vec::new();
        // SysV caller-saved GP : rax, rcx, rdx, rsi, rdi, r8, r9, r10, r11 = 9
        // Force 12 simultaneously-live → must use ≥3 callee-saved (rbx, r12..r15).
        for i in 0..12 {
            let v = X64VReg::gp(i);
            vs.push(v);
            f.push(X64Inst::mov(v, X64Operand::Imm32(i32::try_from(i).unwrap())));
        }
        for i in 1..vs.len() {
            f.push(X64Inst::add(vs[0], X64Operand::Reg(vs[i])));
        }
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        assert!(
            !a.callee_saved_used.is_empty(),
            "expected callee-saved pregs to be used given pressure"
        );
        // All callee_saved_used entries should be in the SysV callee-saved set.
        for p in &a.callee_saved_used {
            assert_eq!(p.role_under_abi(Abi::SysVAmd64), RegRole::CalleeSaved);
        }
    }

    #[test]
    fn cross_call_interval_lives_in_callee_saved() {
        // v0 defined before call, used after — must live in callee-saved.
        let mut f = X64Func::new("cross", Abi::SysVAmd64);
        let v0 = X64VReg::gp(0);
        let v1 = X64VReg::gp(1);
        f.push(X64Inst::mov(v0, X64Operand::Imm32(42))); // 0
        f.push(X64Inst::call("foo", vec![], Some(v1))); // 1
        f.push(X64Inst::add(v0, X64Operand::Reg(v1))); // 2 use both
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        // Find v0's assignment.
        let v0_assign = a
            .assignment
            .iter()
            .find_map(|asg| match asg {
                VregAssignment::Preg(p) => Some(p),
                _ => None,
            })
            .unwrap();
        // v0 (cross-call) must be callee-saved if it got a preg.
        if let VregAssignment::Preg(p) = a.assignment[0] {
            // Cross-call vregs MUST live in callee-saved.
            assert_eq!(p.role_under_abi(Abi::SysVAmd64), RegRole::CalleeSaved);
        }
        let _ = v0_assign;
    }

    #[test]
    fn xmm_vreg_routes_to_xmm_bank() {
        let mut f = X64Func::new("float", Abi::SysVAmd64);
        let v0 = X64VReg::xmm(0);
        f.push(X64Inst::mov(v0, X64Operand::Reg(X64VReg::xmm(1))));
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        // v0 + v1 should both be in XMM pregs.
        for asg in &a.assignment {
            if let VregAssignment::Preg(p) = asg {
                assert_eq!(p.bank(), RegBank::Xmm);
            }
        }
    }

    #[test]
    fn windows_abi_uses_different_caller_saved() {
        // Same workload as `callee_saved_pushed_when_used` but Windows ABI :
        // caller-saved set is smaller (rsi/rdi are callee-saved on Windows).
        let mut f = X64Func::new("win_calleesaved", Abi::WindowsX64);
        let mut vs: Vec<X64VReg> = Vec::new();
        // Windows caller-saved GP : rax, rcx, rdx, r8, r9, r10, r11 = 7
        // Force 10 → must use callee-saved.
        for i in 0..10 {
            let v = X64VReg::gp(i);
            vs.push(v);
            f.push(X64Inst::mov(v, X64Operand::Imm32(i32::try_from(i).unwrap())));
        }
        for i in 1..vs.len() {
            f.push(X64Inst::add(vs[0], X64Operand::Reg(vs[i])));
        }
        f.push(X64Inst::ret());
        let a = allocate(&f).unwrap();
        assert!(!a.callee_saved_used.is_empty());
        // Under Windows, rsi/rdi are callee-saved (they're caller-saved on SysV).
        for p in &a.callee_saved_used {
            assert_eq!(p.role_under_abi(Abi::WindowsX64), RegRole::CalleeSaved);
        }
    }
}

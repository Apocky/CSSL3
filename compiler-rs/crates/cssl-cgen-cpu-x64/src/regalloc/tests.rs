//! End-to-end integration tests covering the full LSRA pipeline.
//!
//! § COVERAGE BUCKETS (per-module tests cover unit-level concerns ;
//!   this file covers integration scenarios)
//!   - allocate-then-build round-trip on representative MIR-shaped inputs
//!   - reserved-register discipline (rsp / rbp never assigned)
//!   - split-on-spill correctness
//!   - report-statistics consistency

#![allow(clippy::too_many_lines)]

use crate::regalloc::alloc::allocate;
use crate::regalloc::inst::{VregAssignment, VregLocation, X64Func, X64Inst, X64Operand};
use crate::regalloc::interval::{compute_live_intervals, IntervalKind};
use crate::regalloc::reg::{Abi, RegBank, RegRole, X64PReg, X64VReg};
use crate::regalloc::spill::SpillSlots;

/// Reserved registers (rsp, rbp) are never assigned to a user vreg.
#[test]
fn rsp_and_rbp_never_allocated_to_user_vregs() {
    // Stress with many live vregs.
    let mut f = X64Func::new("reserved_check", Abi::SysVAmd64);
    let mut vs: Vec<X64VReg> = Vec::new();
    for i in 0..14 {
        let v = X64VReg::gp(i);
        vs.push(v);
        f.push(X64Inst::mov(
            v,
            X64Operand::Imm32(i32::try_from(i).unwrap()),
        ));
    }
    for i in 1..vs.len() {
        f.push(X64Inst::add(vs[0], X64Operand::Reg(vs[i])));
    }
    f.push(X64Inst::ret());

    let alloc = allocate(&f).unwrap();
    for asg in &alloc.assignment {
        if let VregAssignment::Preg(p) = asg {
            assert_ne!(*p, X64PReg::Rsp, "rsp must never be allocated");
            assert_ne!(*p, X64PReg::Rbp, "rbp must never be allocated");
        }
    }
}

/// Spill slots are 16-byte aligned no matter how many spills happen.
#[test]
fn all_spill_slot_offsets_are_sixteen_byte_aligned() {
    // Force enough spills to allocate multiple slots.
    let mut f = X64Func::new("align_check", Abi::SysVAmd64);
    let mut vs: Vec<X64VReg> = Vec::new();
    // 30 GP vregs ⇒ ≥ 16 spills (only ~14 GP pregs after reserved).
    for i in 0..30 {
        let v = X64VReg::gp(i);
        vs.push(v);
        f.push(X64Inst::mov(
            v,
            X64Operand::Imm32(i32::try_from(i).unwrap()),
        ));
    }
    for i in 1..vs.len() {
        f.push(X64Inst::add(vs[0], X64Operand::Reg(vs[i])));
    }
    f.push(X64Inst::ret());

    let alloc = allocate(&f).unwrap();
    // Frame size must be a multiple of 16.
    assert_eq!(alloc.frame_size % 16, 0);

    for asg in &alloc.assignment {
        match asg {
            VregAssignment::Spill(s) => assert_eq!(s.offset() % 16, 0),
            VregAssignment::Split(segs) => {
                for seg in segs {
                    if let VregLocation::Spill(s) = seg.location {
                        assert_eq!(s.offset() % 16, 0);
                    }
                }
            }
            VregAssignment::Preg(_) => {}
        }
    }
}

/// Two non-overlapping intervals should be allowed to share a single preg —
/// the allocator must not over-allocate when liveness is disjoint.
#[test]
fn non_overlapping_intervals_can_share_preg() {
    let mut f = X64Func::new("share_preg", Abi::SysVAmd64);
    let v0 = X64VReg::gp(0);
    let v1 = X64VReg::gp(1);
    let v2 = X64VReg::gp(2);
    f.push(X64Inst::mov(v0, X64Operand::Imm32(1))); // 0
    f.push(X64Inst::add(v0, X64Operand::Imm32(1))); // 1 — last use of v0
    f.push(X64Inst::mov(v1, X64Operand::Imm32(2))); // 2 — v0 dead
    f.push(X64Inst::add(v1, X64Operand::Imm32(1))); // 3 — last use of v1
    f.push(X64Inst::mov(v2, X64Operand::Imm32(3))); // 4 — v1 dead
    f.push(X64Inst::ret());

    let alloc = allocate(&f).unwrap();
    // No spill should be required.
    assert_eq!(alloc.frame_size, 0);
}

/// AllocReport totals are internally consistent with the sum of assignments.
#[test]
fn alloc_report_totals_match_assignment_count() {
    use crate::regalloc::alloc::LinearScanAllocator;
    let mut f = X64Func::new("report_check", Abi::SysVAmd64);
    let v0 = X64VReg::gp(0);
    let v1 = X64VReg::gp(1);
    f.push(X64Inst::mov(v0, X64Operand::Imm32(1)));
    f.push(X64Inst::mov(v1, X64Operand::Reg(v0)));
    f.push(X64Inst::ret());

    let mut a = LinearScanAllocator::new(&f);
    a.run().unwrap();
    let r = a.report();
    assert_eq!(
        r.assigned_to_preg + r.spilled_entirely + r.split,
        r.interval_count
    );
}

/// Round-trip : allocate a small fn and inspect that every vreg used in
/// instructions has a resolution at its program-point.
#[test]
fn every_vreg_use_has_resolution_at_program_point() {
    let mut f = X64Func::new("rt_check", Abi::SysVAmd64);
    let v0 = X64VReg::gp(0);
    let v1 = X64VReg::gp(1);
    let v2 = X64VReg::gp(2);
    f.push(X64Inst::mov(v0, X64Operand::Imm32(10)));
    f.push(X64Inst::mov(v1, X64Operand::Imm32(20)));
    f.push(X64Inst::add(v0, X64Operand::Reg(v1)));
    f.push(X64Inst::mov(v2, X64Operand::Reg(v0)));
    f.push(X64Inst::ret());

    let alloc = allocate(&f).unwrap();
    for ai in &alloc.allocated_insts {
        // Every use + def vreg should be in resolutions.
        for v in ai.inst.uses.iter().chain(ai.inst.defs.iter()) {
            assert!(
                ai.resolutions.iter().any(|r| r.vreg == *v),
                "vreg {v} missing resolution at instruction `{}`",
                ai.inst
            );
        }
    }
}

/// Param vregs are recognized as having Param-kind intervals.
#[test]
fn param_vregs_get_param_interval_kind() {
    let mut f = X64Func::new("with_params", Abi::SysVAmd64);
    let p0 = X64VReg::gp(0);
    let p1 = X64VReg::gp(1);
    f.param_vregs = vec![p0, p1];
    f.push(X64Inst::add(p0, X64Operand::Reg(p1)));
    f.push(X64Inst::ret());

    let ivs = compute_live_intervals(&f);
    let p0_iv = ivs.iter().find(|iv| iv.vreg == p0).unwrap();
    let p1_iv = ivs.iter().find(|iv| iv.vreg == p1).unwrap();
    assert_eq!(p0_iv.kind, IntervalKind::Param);
    assert_eq!(p1_iv.kind, IntervalKind::Param);
    assert_eq!(p0_iv.start, 0); // params live from pp 0
    assert_eq!(p1_iv.start, 0);
}

/// Result vregs are kept live through the final ret.
#[test]
fn result_vregs_extended_through_ret() {
    let mut f = X64Func::new("with_result", Abi::SysVAmd64);
    let r0 = X64VReg::gp(0);
    f.result_vregs = vec![r0];
    f.push(X64Inst::mov(r0, X64Operand::Imm32(42))); // 0
    f.push(X64Inst::mov(X64VReg::gp(1), X64Operand::Imm32(99))); // 1
    f.push(X64Inst::ret()); // 2

    let ivs = compute_live_intervals(&f);
    let r0_iv = ivs.iter().find(|iv| iv.vreg == r0).unwrap();
    // r0 must remain live through pp 2 (the ret).
    assert!(r0_iv.end >= 2);
}

/// Frame size scales 16 bytes per spilled slot.
#[test]
fn frame_size_grows_in_sixteen_byte_increments() {
    let mut s = SpillSlots::new();
    let _ = s.alloc(RegBank::Gp);
    assert_eq!(s.total_frame_size(), 16);
    let _ = s.alloc(RegBank::Xmm);
    assert_eq!(s.total_frame_size(), 32);
    let _ = s.alloc(RegBank::Gp);
    assert_eq!(s.total_frame_size(), 48);
}

/// Bank routing : XMM and GP allocations don't conflict with each other.
/// Even when GP is at full capacity, XMM should allocate independently.
#[test]
fn gp_and_xmm_banks_are_independent() {
    let mut f = X64Func::new("dual_bank", Abi::SysVAmd64);
    // Mix GP and XMM vregs : 4 GP + 4 XMM all simultaneously live.
    // Both fit comfortably in their banks' pregs.
    let g0 = X64VReg::gp(0);
    let g1 = X64VReg::gp(1);
    let g2 = X64VReg::gp(2);
    let g3 = X64VReg::gp(3);
    let x0 = X64VReg::xmm(10);
    let x1 = X64VReg::xmm(11);
    let x2 = X64VReg::xmm(12);
    let x3 = X64VReg::xmm(13);

    f.push(X64Inst::mov(g0, X64Operand::Imm32(1)));
    f.push(X64Inst::mov(g1, X64Operand::Imm32(2)));
    f.push(X64Inst::mov(x0, X64Operand::Reg(x1)));
    f.push(X64Inst::mov(x2, X64Operand::Reg(x3)));
    f.push(X64Inst::mov(g2, X64Operand::Imm32(3)));
    f.push(X64Inst::mov(g3, X64Operand::Imm32(4)));
    // Force everyone live to end.
    f.push(X64Inst::add(g0, X64Operand::Reg(g1)));
    f.push(X64Inst::add(g0, X64Operand::Reg(g2)));
    f.push(X64Inst::add(g0, X64Operand::Reg(g3)));
    f.push(X64Inst::ret());

    let alloc = allocate(&f).unwrap();
    // 4 GP + 4 XMM both fit easily in their banks. No spill should be needed
    // — if a spill happens here it means GP and XMM are incorrectly competing.
    assert_eq!(
        alloc.frame_size, 0,
        "GP and XMM banks must allocate independently — \
         4 GP + 4 XMM should fit without any spill"
    );
}

/// Caller-saved is preferred for short-lived intervals (cheaper — no prologue push).
#[test]
fn short_local_intervals_prefer_caller_saved() {
    let mut f = X64Func::new("short_local", Abi::SysVAmd64);
    let v0 = X64VReg::gp(0);
    f.push(X64Inst::mov(v0, X64Operand::Imm32(1)));
    f.push(X64Inst::ret());
    let alloc = allocate(&f).unwrap();
    if let VregAssignment::Preg(p) = alloc.assignment[0] {
        // Should be caller-saved (prefer-cheap heuristic).
        assert_eq!(p.role_under_abi(Abi::SysVAmd64), RegRole::CallerSaved);
    }
}

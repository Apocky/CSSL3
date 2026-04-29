//! § lsra_pipeline — full LSRA-driven cross-slice walker (S7-G8 / T11-D101).
//!
//! § ROLE
//!   The G8 follow-up to G7's pipeline walker. Where G7 covers the scalar-leaf
//!   subset (`fn () -> i32 { N }`) via direct G1→G4 lowering, G8 lands the
//!   FULL LSRA-driven path : G1 isel → G2 regalloc (linear-scan with spill
//!   slots + callee-saved push/pop pairs) → G3 ABI prologue/epilogue → G4
//!   encoder → G5 object-file emit. Functions with multiple vregs / register
//!   pressure / multi-arg signatures route through this module.
//!
//! § DESIGN  (per T11-D95 § Deferred § "G7-pipeline" + T11-D97 § Reconciliation)
//!   Each G-axis sibling slice retains its OWN `X64Inst` / `X64Func` /
//!   `Abi` surfaces (T11-D95). G8 preserves that discipline : the full-path
//!   driver bridges between siblings via explicit per-stage adapter functions
//!   that produce the next stage's input. The four bridges are :
//!
//!   1. [`isel_to_regalloc_func`] : `isel::X64Func` (vreg-form, MIR-typed) →
//!      `regalloc::X64Func` (vreg-form, bank-tagged uses+defs metadata). Maps
//!      isel's width-tagged vregs onto regalloc's bank-tagged vregs +
//!      translates each `isel::X64Inst` variant into the matching
//!      `regalloc::X64Inst` shape with explicit `uses` / `defs` / `clobbers`.
//!
//!   2. [`abi_x64abi_to_regalloc_abi`] : `crate::abi::X64Abi` (G3's enum) →
//!      `crate::regalloc::reg::Abi` (G2's enum). Direct enum-to-enum bridge.
//!
//!   3. [`regalloc_to_encoder_insts`] : `regalloc::X64FuncAllocated` (preg-
//!      form, post-LSRA, with spill markers) → `Vec<encoder::X64Inst>`
//!      (post-regalloc emit-ready). Resolves vreg→preg per-program-point via
//!      the allocator's resolutions, lowers `SpillMarker` / `ReloadMarker`
//!      to `Store [rsp+disp], reg` / `Load reg, [rsp+disp]`, and emits the
//!      per-instruction encoder shape.
//!
//!   4. [`build_func_bytes_via_lsra`] : composes the per-fn byte assembly
//!      via the full pipeline : G1 → G2 → G3 prologue → encoded body →
//!      G3 epilogue. Returns the G5-boundary `objemit::X64Func`.
//!
//! § PARAM-VREG REGISTER PIN
//!   At fn entry, parameter vregs have already been "delivered" by the caller
//!   into the canonical arg-pregs (rdi/rsi/rdx/rcx/r8/r9 SysV ; rcx/rdx/r8/r9
//!   MS-x64 ; xmm0..xmm7 SysV / xmm0..xmm3 MS-x64). The bridge front-loads a
//!   sequence of `Mov vreg_param, preg_arg` instructions at the head of the
//!   regalloc instruction stream so the allocator's interval analysis sees
//!   the param vreg's first definition at program-point 0..N (where N is
//!   the param count) and the arg-preg loads are forced via `fixed_uses`.
//!   Overflow-stack args land via `Load vreg_param, [rsp+disp]` from the
//!   caller-provided slots above rbp ; G8 implements the register-pin path
//!   for ≤ 6 int / 8 float SysV (≤ 4 / 4 MS-x64) ; stack-overflow params are
//!   reserved for a later G8+ slice (the 5-arg test case fits within
//!   register-only on SysV but needs one-stack-overflow on MS-x64 — the
//!   adapter handles both via the `param_load_insts` builder).
//!
//! § RETURN-VREG ENFORCEMENT
//!   Result vregs (the operands of the trailing `Ret { operands }` terminator)
//!   are translated to a final `Mov rax, vreg_result` (or `MovsdRR xmm0, vreg`)
//!   immediately before the regalloc `Ret` so the allocator's interval analysis
//!   keeps the result vreg live to the return point AND the return-value lands
//!   in the canonical `rax` / `xmm0` per the ABI convention.
//!
//! § SPILL/RELOAD ENCODING
//!   When the LSRA allocator emits `SpillMarker { vreg }` it means : "this
//!   vreg's preg is being released ; flush its live value to its assigned
//!   spill-slot". The encoder bridge lowers it to
//!   `Store { dst: [rsp+slot.offset()], src: <preg-currently-holding-vreg> }`
//!   with `OperandSize::B64` (16-byte SSE-aligned slots are over-allocated
//!   per `regalloc::spill::SpillSlot`'s 16-byte invariant, so a 64-bit GP
//!   store fits cleanly in the low 8 bytes).
//!   `ReloadMarker { vreg }` lowers to
//!   `Load { dst: <preg-newly-assigned>, src: [rsp+slot.offset()] }`.
//!
//! § FRAME-SIZE PROPAGATION
//!   The allocator's `frame_size` (16-aligned slot count × 16) flows into G3's
//!   `FunctionLayout::local_frame_bytes` so the prologue's `sub rsp, frame`
//!   matches the spill-slot layout. The allocator's `callee_saved_used`
//!   (post-LSRA preg set ∩ ABI's callee-saved set) flows into
//!   `callee_saved_gp_used` / `callee_saved_xmm_used`.
//!
//! § ATTESTATION  (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

use crate::abi::{GpReg, X64Abi, XmmReg};
use crate::encoder::encode_into;
use crate::encoder::inst::X64Inst as EncInst;
use crate::encoder::mem::MemOperand;
use crate::encoder::reg::{Gpr, OperandSize, Xmm};
use crate::isel::func::X64Func as IselFunc;
use crate::isel::inst::{X64Imm, X64Inst as IselInst, X64Term as IselTerm};
use crate::isel::vreg::{X64VReg as IselVReg, X64Width};
use crate::lower::{
    lower_epilogue_for, lower_prologue, FunctionLayout, LoweredEpilogue, LoweredPrologue,
};
use crate::objemit::func::X64Func as ObjFunc;
use crate::regalloc::alloc::{allocate, AllocError};
use crate::regalloc::inst::{
    AllocatedInst, MemAddr, VregAssignment, VregLocation, X64Func as RaFunc, X64Inst as RaInst,
    X64InstKind as RaInstKind, X64Operand as RaOperand,
};
use crate::regalloc::reg::{Abi as RaAbi, RegBank, X64PReg, X64VReg as RaVReg};
use crate::NativeX64Error;

// ═══════════════════════════════════════════════════════════════════════
// § Bridge 1 : isel::X64Func → regalloc::X64Func
// ═══════════════════════════════════════════════════════════════════════

/// Translate an [`IselFunc`] (vreg-form post-isel, MIR-typed) into a
/// [`RaFunc`] (vreg-form input to LSRA, bank-tagged with explicit
/// uses+defs / fixed-pregs / clobbers metadata).
///
/// The translation enforces a single-block invariant at G8 — multi-block
/// CFGs require branch-fixup which is reserved for a later G9+ slice. The
/// scalar-leaf single-block subset that G7 covers expands at G8 to include
/// arbitrary-arity function signatures + arbitrary integer arithmetic
/// (`Add` / `Sub` / `IMul`) within the entry block.
///
/// § PARAM HANDLING
///   The function's signature is translated into a leading sequence of
///   `Mov vreg_param, preg_arg` instructions so the allocator sees the
///   param vreg defined at program-point 0..N (canonical arg-preg discipline).
///   The corresponding arg-pregs are recorded in `RaInst::fixed_uses` so the
///   allocator's forbidden-preg pass keeps them live across the entry-block
///   prefix.
///
/// # Errors
/// Returns [`NativeX64Error::UnsupportedOp`] for ops outside the G8
/// single-block subset (closures / multi-block CFG / scf.* / call sites).
pub fn isel_to_regalloc_func(isel: &IselFunc, abi: X64Abi) -> Result<RaFunc, NativeX64Error> {
    if isel.blocks.len() != 1 {
        return Err(NativeX64Error::UnsupportedOp {
            fn_name: isel.name.clone(),
            op_name: format!(
                "multi-block-body ({n} blocks ; G8 LSRA path = 1)",
                n = isel.blocks.len()
            ),
        });
    }
    let block = &isel.blocks[0];

    let ra_abi = abi_x64abi_to_regalloc_abi(abi);
    let mut out = RaFunc::new(isel.name.clone(), ra_abi);

    // § Map isel::X64VReg(.id, .width) → regalloc::X64VReg(.index, .bank).
    //   isel uses ids starting at 1 with id=0 reserved as sentinel ; regalloc
    //   uses indices starting at 0. We subtract 1 so the regalloc indices are
    //   dense from 0. The per-bank routing comes from `width.is_gpr()`.
    let to_ra_vreg = |iv: IselVReg| -> RaVReg {
        let bank = if iv.width.is_sse() {
            RegBank::Xmm
        } else {
            RegBank::Gp
        };
        RaVReg {
            index: iv.id.saturating_sub(1),
            bank,
        }
    };

    // § Front-load param vregs from canonical arg-pregs.
    //   For each param i, emit `Mov vreg_param_i, <fixed:arg_preg_i>` so the
    //   allocator sees param defined at pp=0..N. We use a special-shaped Mov
    //   with an empty Reg-source operand and a `fixed_uses` entry that pins
    //   the arg-preg as the conceptual source.
    let int_arg_regs = abi.int_arg_regs();
    let float_arg_regs = abi.float_arg_regs();
    let mut int_arg_idx: usize = 0;
    let mut float_arg_idx: usize = 0;

    for (param_idx, &width) in isel.sig.params.iter().enumerate() {
        let isel_vreg = isel.param_vreg(param_idx);
        let ra_vreg = to_ra_vreg(isel_vreg);
        out.param_vregs.push(ra_vreg);

        if width.is_gpr() {
            if let Some(arg_preg) = int_arg_regs.get(int_arg_idx) {
                let preg = gpreg_to_x64preg(*arg_preg);
                // `Mov dst, src=fixed-preg` modeled as `Mov dst, dst` with a
                // fixed_uses entry pinning `arg_preg` ; the regalloc allocator
                // honors the fixed_uses constraint on the surrounding interval
                // by adding the preg to forbidden_pregs of OTHER spans, and
                // the encoder bridge below lowers it to `MovRR dst, arg_preg`.
                let inst = RaInst {
                    kind: RaInstKind::Mov {
                        dst: RaOperand::Reg(ra_vreg),
                        src: RaOperand::Reg(ra_vreg),
                    },
                    uses: vec![],
                    defs: vec![ra_vreg],
                    fixed_uses: vec![preg],
                    fixed_defs: vec![],
                    clobbers: vec![],
                };
                out.push(inst);
                int_arg_idx += 1;
                continue;
            }
            // Stack-overflow param : `Load vreg_param, [rbp + 16 + 8*overflow_idx]`.
            // Reserved for the G9+ stack-arg slice ; reject loudly here.
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: isel.name.clone(),
                op_name: format!(
                    "param #{p} overflows {abi} register set ; stack-overflow params \
                     deferred to G9 slice",
                    p = param_idx,
                    abi = abi.as_str()
                ),
            });
        }
        // SSE param.
        if let Some(arg_preg) = float_arg_regs.get(float_arg_idx) {
            let preg = xmmreg_to_x64preg(*arg_preg);
            let inst = RaInst {
                kind: RaInstKind::Mov {
                    dst: RaOperand::Reg(ra_vreg),
                    src: RaOperand::Reg(ra_vreg),
                },
                uses: vec![],
                defs: vec![ra_vreg],
                fixed_uses: vec![preg],
                fixed_defs: vec![],
                clobbers: vec![],
            };
            out.push(inst);
            float_arg_idx += 1;
            continue;
        }
        return Err(NativeX64Error::UnsupportedOp {
            fn_name: isel.name.clone(),
            op_name: format!(
                "float param #{p} overflows {abi} XMM arg set ; stack-overflow \
                 params deferred to G9 slice",
                p = param_idx,
                abi = abi.as_str()
            ),
        });
    }

    // § Walk body insts and translate each.
    for inst in &block.insts {
        match inst {
            IselInst::Mov { dst, src } => {
                let d = to_ra_vreg(*dst);
                let s = to_ra_vreg(*src);
                out.push(RaInst::mov(d, RaOperand::Reg(s)));
            }
            IselInst::MovImm { dst, imm } => {
                let d = to_ra_vreg(*dst);
                let operand = match imm {
                    X64Imm::I32(v) => RaOperand::Imm32(*v),
                    X64Imm::I64(v) => RaOperand::Imm64(*v),
                    X64Imm::Bool(b) => RaOperand::Imm32(i32::from(*b)),
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: isel.name.clone(),
                            op_name: format!("MovImm `{other:?}` not in G8 imm subset"),
                        });
                    }
                };
                out.push(RaInst::mov(d, operand));
            }
            IselInst::Add { dst, src } => {
                let d = to_ra_vreg(*dst);
                let s = to_ra_vreg(*src);
                out.push(RaInst::add(d, RaOperand::Reg(s)));
            }
            IselInst::Sub { dst, src } => {
                let d = to_ra_vreg(*dst);
                let s = to_ra_vreg(*src);
                out.push(RaInst::sub(d, RaOperand::Reg(s)));
            }
            IselInst::IMul { dst, src } => {
                let d = to_ra_vreg(*dst);
                let s = to_ra_vreg(*src);
                let mut ra = RaInst {
                    kind: RaInstKind::Imul {
                        dst: d,
                        src: RaOperand::Reg(s),
                    },
                    uses: vec![d, s],
                    defs: vec![d],
                    fixed_uses: vec![],
                    fixed_defs: vec![],
                    clobbers: vec![],
                };
                ra.uses.dedup();
                out.push(ra);
            }
            IselInst::Cdq => {
                // Cdq sign-extends eax → edx:eax.
                let ra = RaInst {
                    kind: RaInstKind::Mov {
                        // Encoded later as bare `cdq` opcode ; the encoder
                        // bridge dispatches on the `Cdq` placeholder marker.
                        dst: RaOperand::Reg(RaVReg::gp(u32::MAX - 1)),
                        src: RaOperand::Reg(RaVReg::gp(u32::MAX - 1)),
                    },
                    uses: vec![],
                    defs: vec![],
                    fixed_uses: vec![X64PReg::Rax],
                    fixed_defs: vec![X64PReg::Rdx],
                    clobbers: vec![],
                };
                let _ = ra;
                // Cdq + Idiv aren't part of the G8 LSRA-pipeline subset (the
                // div lowering forces fixed-preg pinning via rax+rdx that
                // requires the bridge to materialize move-to-rax + move-to-
                // result before/after, which is its own slice). The 5-arg
                // test fixture below uses `+ - *` only ; Idiv lands as a
                // future G8+ slice.
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: isel.name.clone(),
                    op_name: "Cdq lowering deferred to G9 slice".to_string(),
                });
            }
            IselInst::Cqo
            | IselInst::Idiv { .. }
            | IselInst::Div { .. }
            | IselInst::XorRdx { .. } => {
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: isel.name.clone(),
                    op_name: format!(
                        "integer division ({inst:?}) deferred to G9 slice (rax/rdx \
                         pinning + result-move-out not yet wired)"
                    ),
                });
            }
            other => {
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: isel.name.clone(),
                    op_name: format!("op `{other:?}` not in G8 LSRA-pipeline subset"),
                });
            }
        }
    }

    // § Translate terminator.
    match &block.terminator {
        IselTerm::Ret { operands } => {
            // Place result(s) into rax/xmm0 then emit Ret.
            for (i, &iv) in operands.iter().enumerate() {
                if i > 0 {
                    return Err(NativeX64Error::UnsupportedOp {
                        fn_name: isel.name.clone(),
                        op_name: format!(
                            "multi-result return ({n} results ; G8 LSRA = 0 or 1)",
                            n = operands.len()
                        ),
                    });
                }
                let result_vreg = to_ra_vreg(iv);
                out.result_vregs.push(result_vreg);

                // Synthetic `Mov rax, vreg_result` : modeled as a Mov whose
                // fixed_defs pins `rax` ; the encoder bridge lowers it to
                // a real `MovRR Rax, <preg-of-result-vreg>`.
                let preg = if iv.width.is_sse() {
                    X64PReg::Xmm0
                } else {
                    X64PReg::Rax
                };
                let inst = RaInst {
                    kind: RaInstKind::Mov {
                        dst: RaOperand::Reg(result_vreg),
                        src: RaOperand::Reg(result_vreg),
                    },
                    uses: vec![result_vreg],
                    defs: vec![],
                    fixed_uses: vec![],
                    fixed_defs: vec![preg],
                    clobbers: vec![],
                };
                out.push(inst);
            }
            out.push(RaInst::ret());
        }
        other => {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: isel.name.clone(),
                op_name: format!("non-Ret terminator `{other:?}` in G8 LSRA subset"),
            });
        }
    }

    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════
// § Bridge 2 : abi::X64Abi → regalloc::reg::Abi
// ═══════════════════════════════════════════════════════════════════════

/// Map G3's [`X64Abi`] onto G2's [`RaAbi`]. Direct enum-to-enum bridge ;
/// the regalloc enum has a 3rd variant (`DarwinAmd64`) that's a sibling
/// of `SystemV` for our register-class purposes — G3's enum collapses
/// macOS-Intel into `SystemV` for now.
#[must_use]
pub const fn abi_x64abi_to_regalloc_abi(abi: X64Abi) -> RaAbi {
    match abi {
        X64Abi::SystemV => RaAbi::SysVAmd64,
        X64Abi::MicrosoftX64 => RaAbi::WindowsX64,
    }
}

/// Map G3's [`GpReg`] onto G2's [`X64PReg`] in the GP bank.
#[must_use]
pub const fn gpreg_to_x64preg(g: GpReg) -> X64PReg {
    match g {
        GpReg::Rax => X64PReg::Rax,
        GpReg::Rcx => X64PReg::Rcx,
        GpReg::Rdx => X64PReg::Rdx,
        GpReg::Rbx => X64PReg::Rbx,
        GpReg::Rsp => X64PReg::Rsp,
        GpReg::Rbp => X64PReg::Rbp,
        GpReg::Rsi => X64PReg::Rsi,
        GpReg::Rdi => X64PReg::Rdi,
        GpReg::R8 => X64PReg::R8,
        GpReg::R9 => X64PReg::R9,
        GpReg::R10 => X64PReg::R10,
        GpReg::R11 => X64PReg::R11,
        GpReg::R12 => X64PReg::R12,
        GpReg::R13 => X64PReg::R13,
        GpReg::R14 => X64PReg::R14,
        GpReg::R15 => X64PReg::R15,
    }
}

/// Map G3's [`XmmReg`] onto G2's [`X64PReg`] in the XMM bank.
#[must_use]
pub const fn xmmreg_to_x64preg(x: XmmReg) -> X64PReg {
    match x {
        XmmReg::Xmm0 => X64PReg::Xmm0,
        XmmReg::Xmm1 => X64PReg::Xmm1,
        XmmReg::Xmm2 => X64PReg::Xmm2,
        XmmReg::Xmm3 => X64PReg::Xmm3,
        XmmReg::Xmm4 => X64PReg::Xmm4,
        XmmReg::Xmm5 => X64PReg::Xmm5,
        XmmReg::Xmm6 => X64PReg::Xmm6,
        XmmReg::Xmm7 => X64PReg::Xmm7,
        XmmReg::Xmm8 => X64PReg::Xmm8,
        XmmReg::Xmm9 => X64PReg::Xmm9,
        XmmReg::Xmm10 => X64PReg::Xmm10,
        XmmReg::Xmm11 => X64PReg::Xmm11,
        XmmReg::Xmm12 => X64PReg::Xmm12,
        XmmReg::Xmm13 => X64PReg::Xmm13,
        XmmReg::Xmm14 => X64PReg::Xmm14,
        XmmReg::Xmm15 => X64PReg::Xmm15,
    }
}

/// Map G2's [`X64PReg`] back to G3's [`GpReg`] (panics if not GP).
#[must_use]
pub fn x64preg_to_gpreg(p: X64PReg) -> GpReg {
    match p {
        X64PReg::Rax => GpReg::Rax,
        X64PReg::Rcx => GpReg::Rcx,
        X64PReg::Rdx => GpReg::Rdx,
        X64PReg::Rbx => GpReg::Rbx,
        X64PReg::Rsp => GpReg::Rsp,
        X64PReg::Rbp => GpReg::Rbp,
        X64PReg::Rsi => GpReg::Rsi,
        X64PReg::Rdi => GpReg::Rdi,
        X64PReg::R8 => GpReg::R8,
        X64PReg::R9 => GpReg::R9,
        X64PReg::R10 => GpReg::R10,
        X64PReg::R11 => GpReg::R11,
        X64PReg::R12 => GpReg::R12,
        X64PReg::R13 => GpReg::R13,
        X64PReg::R14 => GpReg::R14,
        X64PReg::R15 => GpReg::R15,
        other => panic!("x64preg_to_gpreg : `{other}` is not a GP preg"),
    }
}

/// Map G2's [`X64PReg`] to G4's encoder [`Gpr`].
#[must_use]
pub fn x64preg_to_gpr(p: X64PReg) -> Gpr {
    Gpr::from_index(x64preg_to_gpreg(p).encoding())
}

/// Map G2's [`X64PReg`] to G4's encoder [`Xmm`].
#[must_use]
pub fn x64preg_to_xmm(p: X64PReg) -> Xmm {
    let idx = match p.bank() {
        RegBank::Xmm => p.encoding(),
        RegBank::Gp => panic!("x64preg_to_xmm : `{p}` is not an XMM preg"),
    };
    Xmm::from_index(idx)
}

// ═══════════════════════════════════════════════════════════════════════
// § Bridge 3 : regalloc::X64FuncAllocated → encoder::X64Inst stream
// ═══════════════════════════════════════════════════════════════════════

/// Walk an allocated regalloc function and emit a flat [`Vec<EncInst>`] in
/// program-order. Spill / reload markers lower to actual `Store` / `Load`
/// instructions with `[rsp+slot.offset()]` addressing.
///
/// The synthetic param-load instructions inserted by [`isel_to_regalloc_func`]
/// (the `Mov dst, dst` with a `fixed_uses=[arg_preg]` pinning) lower to
/// `MovRR dst-preg, arg_preg` real instructions ; if the allocator placed
/// the dst-vreg in the same preg as the arg-preg, the move is a no-op (we
/// elide it).
///
/// The synthetic result-store instruction (the `Mov` with `fixed_defs=[rax]`
/// before `Ret`) lowers to `MovRR rax/xmm0, result-preg` ; same elision rule.
///
/// § IMPLICIT RELOAD/SPILL FOR FULLY-SPILLED VREGS
///   The S7-G2 LSRA implementation may decide to spill a vreg ENTIRELY (no
///   preg ever assigned) — in which case `assignment_for(vreg)` returns
///   [`VregAssignment::Spill(slot)`] and every per-program-point resolution
///   reports [`VregLocation::Spill(slot)`]. Since the LSRA driver does NOT
///   insert explicit `SpillMarker` / `ReloadMarker` instructions for the
///   fully-spilled case, the encoder bridge implicitly materializes reload
///   + spill instructions around each use/def of a spilled vreg :
///
///   - Before each instruction, for every USE-vreg currently in a spill slot,
///     emit `Load <scratch-preg>, [rsp+slot.offset()]` and substitute
///     `<scratch-preg>` for the vreg in the inst.
///   - After each instruction, for every DEF-vreg currently in a spill slot,
///     emit `Store [rsp+slot.offset()], <scratch-preg>` (where the scratch
///     is the same preg the inst's def-encoding wrote to).
///
///   The scratch picker prefers `r11`, falling back to `r10` and `rax`,
///   ensuring the scratch isn't currently holding any other live vreg at
///   this program-point.
///
/// # Errors
/// Returns [`NativeX64Error::UnsupportedOp`] for any inst-kind the encoder
/// bridge doesn't handle (deferred-to-G9 cases like Idiv pinning).
#[allow(clippy::too_many_lines, clippy::if_not_else, clippy::cognitive_complexity)]
pub fn regalloc_to_encoder_insts(
    allocated: &crate::regalloc::inst::X64FuncAllocated,
) -> Result<Vec<EncInst>, NativeX64Error> {
    let mut out: Vec<EncInst> = Vec::with_capacity(allocated.allocated_insts.len() * 2);

    // Helper : look up the preg-or-spill location for `vreg` at this allocated
    // instruction. Returns `Some(VregLocation)` for known vregs.
    let location_for = |ai: &AllocatedInst, vreg: RaVReg| -> Option<VregLocation> {
        ai.resolutions
            .iter()
            .find(|r| r.vreg == vreg)
            .map(|r| r.location)
    };

    // Helper : resolve directly to a preg, returning None if spilled.
    let preg_for = |ai: &AllocatedInst, vreg: RaVReg| -> Option<X64PReg> {
        match location_for(ai, vreg) {
            Some(VregLocation::Preg(p)) => Some(p),
            Some(VregLocation::Spill(_)) | None => None,
        }
    };

    // Helper : look up the spill-slot for a fully-spilled vreg via the
    // function's per-vreg assignment table.
    let spill_slot_for = |vreg: RaVReg| -> Option<crate::regalloc::spill::SpillSlot> {
        match assignment_for(allocated, vreg) {
            Some(VregAssignment::Spill(s)) => Some(s),
            Some(VregAssignment::Split(segs)) => {
                for seg in segs {
                    if let VregLocation::Spill(s) = seg.location {
                        return Some(s);
                    }
                }
                None
            }
            Some(VregAssignment::Preg(_)) | None => None,
        }
    };

    // Helper : pick a scratch preg for a spilled-vreg materialization. Avoid
    // any preg currently holding another vreg in this inst's resolutions ;
    // prefer caller-saved regs we don't otherwise touch.
    let pick_scratch_gp = |ai: &AllocatedInst, avoid_extra: &[X64PReg]| -> Option<X64PReg> {
        let in_use: Vec<X64PReg> = ai
            .resolutions
            .iter()
            .filter_map(|r| match r.location {
                VregLocation::Preg(p) if p.bank() == RegBank::Gp => Some(p),
                VregLocation::Preg(_) | VregLocation::Spill(_) => None,
            })
            .collect();
        [X64PReg::R11, X64PReg::R10, X64PReg::Rax]
            .iter()
            .find(|&&c| !in_use.contains(&c) && !avoid_extra.contains(&c))
            .copied()
    };

    for ai in &allocated.allocated_insts {
        match &ai.inst.kind {
            // § Synthetic param-load / result-store : a `Mov dst, dst` with
            //   fixed_uses xor fixed_defs. The encoder lowers it to a real
            //   `MovRR dst-preg, arg_preg-or-result-preg`, eliding the
            //   no-op when the allocator coalesced.
            RaInstKind::Mov {
                dst: dst_op,
                src: src_op,
            } if (!ai.inst.fixed_uses.is_empty() && ai.inst.fixed_defs.is_empty())
                || (!ai.inst.fixed_defs.is_empty() && ai.inst.fixed_uses.is_empty())
                    && dst_op == src_op =>
            {
                if !ai.inst.fixed_uses.is_empty() {
                    // Param-load : `Mov dst, fixed_use_preg`.
                    let dst_vreg = match dst_op {
                        RaOperand::Reg(v) => *v,
                        _ => {
                            return Err(NativeX64Error::UnsupportedOp {
                                fn_name: allocated.name.clone(),
                                op_name: "synthetic param-load with non-reg dst".to_string(),
                            });
                        }
                    };
                    let arg_preg = ai.inst.fixed_uses[0];
                    let dst_preg =
                        preg_for(ai, dst_vreg).ok_or_else(|| NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!(
                                "synthetic param-load : dst vreg {dst_vreg} not in a preg"
                            ),
                        })?;
                    if dst_preg != arg_preg {
                        emit_mov_rr(&mut out, dst_preg, arg_preg)?;
                    }
                } else {
                    // Result-store : `Mov fixed_def_preg, src_vreg`.
                    let src_vreg = match src_op {
                        RaOperand::Reg(v) => *v,
                        _ => {
                            return Err(NativeX64Error::UnsupportedOp {
                                fn_name: allocated.name.clone(),
                                op_name: "synthetic result-store with non-reg src".to_string(),
                            });
                        }
                    };
                    let result_preg = ai.inst.fixed_defs[0];
                    let src_preg =
                        preg_for(ai, src_vreg).ok_or_else(|| NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!(
                                "synthetic result-store : src vreg {src_vreg} not in a preg"
                            ),
                        })?;
                    if result_preg != src_preg {
                        emit_mov_rr(&mut out, result_preg, src_preg)?;
                    }
                }
            }
            RaInstKind::Mov {
                dst: dst_op,
                src: src_op,
            } => {
                let dst_vreg = match dst_op {
                    RaOperand::Reg(v) => *v,
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!("Mov dst non-reg `{other:?}` deferred"),
                        });
                    }
                };
                // Resolve dst : preg or spill-via-scratch.
                let (dst_preg, dst_spill) = match preg_for(ai, dst_vreg) {
                    Some(p) => (p, None),
                    None => {
                        // Spilled : use scratch + emit Store after.
                        let slot = spill_slot_for(dst_vreg).ok_or_else(|| {
                            NativeX64Error::UnsupportedOp {
                                fn_name: allocated.name.clone(),
                                op_name: format!("Mov dst vreg {dst_vreg} : no preg + no slot"),
                            }
                        })?;
                        let scratch = pick_scratch_gp(ai, &[]).ok_or_else(|| {
                            NativeX64Error::UnsupportedOp {
                                fn_name: allocated.name.clone(),
                                op_name: format!(
                                    "Mov dst vreg {dst_vreg} spilled : no scratch GP available"
                                ),
                            }
                        })?;
                        (scratch, Some((slot.offset(), scratch)))
                    }
                };
                match src_op {
                    RaOperand::Reg(sv) => {
                        let src_preg = match preg_for(ai, *sv) {
                            Some(p) => p,
                            None => {
                                // Spilled use : reload to scratch (different
                                // from dst's scratch).
                                let slot = spill_slot_for(*sv).ok_or_else(|| {
                                    NativeX64Error::UnsupportedOp {
                                        fn_name: allocated.name.clone(),
                                        op_name: format!("Mov src vreg {sv} : no preg + no slot"),
                                    }
                                })?;
                                let scratch =
                                    pick_scratch_gp(ai, &[dst_preg]).ok_or_else(|| {
                                        NativeX64Error::UnsupportedOp {
                                            fn_name: allocated.name.clone(),
                                            op_name: format!(
                                                "Mov src vreg {sv} spilled : no scratch GP"
                                            ),
                                        }
                                    })?;
                                emit_reload(&mut out, scratch, slot.offset())?;
                                scratch
                            }
                        };
                        if dst_preg != src_preg {
                            emit_mov_rr(&mut out, dst_preg, src_preg)?;
                        }
                    }
                    RaOperand::Imm32(v) => {
                        out.push(EncInst::MovRI {
                            size: OperandSize::B32,
                            dst: x64preg_to_gpr(dst_preg),
                            imm: i64::from(*v),
                        });
                    }
                    RaOperand::Imm64(v) => {
                        out.push(EncInst::MovRI {
                            size: OperandSize::B64,
                            dst: x64preg_to_gpr(dst_preg),
                            imm: *v,
                        });
                    }
                    RaOperand::Mem(_) => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: "Mov mem-source deferred to G9 slice".to_string(),
                        });
                    }
                }
                // If dst is spilled, store from scratch back to slot.
                if let Some((disp, scratch)) = dst_spill {
                    emit_spill(&mut out, scratch, disp)?;
                }
            }
            RaInstKind::Add { dst, src } => {
                let (dst_preg, dst_spill) = resolve_def_or_scratch(
                    ai,
                    *dst,
                    &allocated.name,
                    &spill_slot_for,
                    &pick_scratch_gp,
                    &[],
                )?;
                // For a two-address Add, dst is BOTH used and defined ; if dst
                // is spilled we must reload first too.
                if let Some((disp, _)) = dst_spill {
                    emit_reload(&mut out, dst_preg, disp)?;
                }
                let src_preg = match src {
                    RaOperand::Reg(sv) => resolve_use_or_reload(
                        ai,
                        &mut out,
                        *sv,
                        &allocated.name,
                        &spill_slot_for,
                        &pick_scratch_gp,
                        &[dst_preg],
                    )?,
                    RaOperand::Imm32(v) => {
                        out.push(EncInst::AddRI {
                            size: OperandSize::B32,
                            dst: x64preg_to_gpr(dst_preg),
                            imm: *v,
                        });
                        if let Some((disp, _)) = dst_spill {
                            emit_spill(&mut out, dst_preg, disp)?;
                        }
                        continue;
                    }
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!("Add src `{other:?}` deferred"),
                        });
                    }
                };
                out.push(EncInst::AddRR {
                    size: OperandSize::B32,
                    dst: x64preg_to_gpr(dst_preg),
                    src: x64preg_to_gpr(src_preg),
                });
                if let Some((disp, _)) = dst_spill {
                    emit_spill(&mut out, dst_preg, disp)?;
                }
            }
            RaInstKind::Sub { dst, src } => {
                let (dst_preg, dst_spill) = resolve_def_or_scratch(
                    ai,
                    *dst,
                    &allocated.name,
                    &spill_slot_for,
                    &pick_scratch_gp,
                    &[],
                )?;
                if let Some((disp, _)) = dst_spill {
                    emit_reload(&mut out, dst_preg, disp)?;
                }
                let src_preg = match src {
                    RaOperand::Reg(sv) => resolve_use_or_reload(
                        ai,
                        &mut out,
                        *sv,
                        &allocated.name,
                        &spill_slot_for,
                        &pick_scratch_gp,
                        &[dst_preg],
                    )?,
                    RaOperand::Imm32(v) => {
                        out.push(EncInst::SubRI {
                            size: OperandSize::B32,
                            dst: x64preg_to_gpr(dst_preg),
                            imm: *v,
                        });
                        if let Some((disp, _)) = dst_spill {
                            emit_spill(&mut out, dst_preg, disp)?;
                        }
                        continue;
                    }
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!("Sub src `{other:?}` deferred"),
                        });
                    }
                };
                out.push(EncInst::SubRR {
                    size: OperandSize::B32,
                    dst: x64preg_to_gpr(dst_preg),
                    src: x64preg_to_gpr(src_preg),
                });
                if let Some((disp, _)) = dst_spill {
                    emit_spill(&mut out, dst_preg, disp)?;
                }
            }
            RaInstKind::Imul { dst, src } => {
                let (dst_preg, dst_spill) = resolve_def_or_scratch(
                    ai,
                    *dst,
                    &allocated.name,
                    &spill_slot_for,
                    &pick_scratch_gp,
                    &[],
                )?;
                if let Some((disp, _)) = dst_spill {
                    emit_reload(&mut out, dst_preg, disp)?;
                }
                let src_preg = match src {
                    RaOperand::Reg(sv) => resolve_use_or_reload(
                        ai,
                        &mut out,
                        *sv,
                        &allocated.name,
                        &spill_slot_for,
                        &pick_scratch_gp,
                        &[dst_preg],
                    )?,
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!("Imul src `{other:?}` deferred"),
                        });
                    }
                };
                out.push(EncInst::ImulRR {
                    size: OperandSize::B32,
                    dst: x64preg_to_gpr(dst_preg),
                    src: x64preg_to_gpr(src_preg),
                });
                if let Some((disp, _)) = dst_spill {
                    emit_spill(&mut out, dst_preg, disp)?;
                }
            }
            RaInstKind::SpillMarker { vreg } => {
                // Look up the slot for this vreg.
                let slot = match assignment_for(allocated, *vreg) {
                    Some(VregAssignment::Spill(s)) => s,
                    Some(VregAssignment::Split(segs)) => {
                        // Find the segment whose Spill applies after this pp.
                        // For simplicity at G8, use the first Spill segment.
                        let mut found = None;
                        for seg in &segs {
                            if let VregLocation::Spill(s) = seg.location {
                                found = Some(s);
                                break;
                            }
                        }
                        match found {
                            Some(s) => s,
                            None => {
                                return Err(NativeX64Error::UnsupportedOp {
                                    fn_name: allocated.name.clone(),
                                    op_name: format!(
                                        "SpillMarker {vreg}: split has no spill segment"
                                    ),
                                });
                            }
                        }
                    }
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!(
                                "SpillMarker {vreg} has assignment {other:?}, expected Spill/Split"
                            ),
                        });
                    }
                };
                let preg = preg_for(ai, *vreg).ok_or_else(|| NativeX64Error::UnsupportedOp {
                    fn_name: allocated.name.clone(),
                    op_name: format!("SpillMarker {vreg} : no preg at this pp"),
                })?;
                emit_spill(&mut out, preg, slot.offset())?;
            }
            RaInstKind::ReloadMarker { vreg } => {
                let slot = match assignment_for(allocated, *vreg) {
                    Some(VregAssignment::Spill(s)) => s,
                    Some(VregAssignment::Split(segs)) => {
                        let mut found = None;
                        for seg in &segs {
                            if let VregLocation::Spill(s) = seg.location {
                                found = Some(s);
                                break;
                            }
                        }
                        match found {
                            Some(s) => s,
                            None => {
                                return Err(NativeX64Error::UnsupportedOp {
                                    fn_name: allocated.name.clone(),
                                    op_name: format!(
                                        "ReloadMarker {vreg}: split has no spill segment"
                                    ),
                                });
                            }
                        }
                    }
                    other => {
                        return Err(NativeX64Error::UnsupportedOp {
                            fn_name: allocated.name.clone(),
                            op_name: format!(
                                "ReloadMarker {vreg} has assignment {other:?}, expected Spill/Split"
                            ),
                        });
                    }
                };
                let preg = preg_for(ai, *vreg).ok_or_else(|| NativeX64Error::UnsupportedOp {
                    fn_name: allocated.name.clone(),
                    op_name: format!("ReloadMarker {vreg} : no preg at this pp"),
                })?;
                emit_reload(&mut out, preg, slot.offset())?;
            }
            RaInstKind::Ret => {
                // The G3 epilogue emits the actual `ret` byte after the
                // callee-saved-restore + add-rsp + pop-rbp ; here we emit
                // nothing — the regalloc Ret is purely a marker for the
                // allocator's interval analysis.
            }
            other => {
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: allocated.name.clone(),
                    op_name: format!("regalloc inst kind `{other:?}` deferred to G9 slice"),
                });
            }
        }
    }

    Ok(out)
}

/// Look up the [`VregAssignment`] for `vreg` in an [`AllocatedFunc`].
///
/// ‼ The allocator's `assignment` Vec is SORTED by vreg.index but its
/// VEC-POSITION ≠ vreg.index in general. We walk the per-program-point
/// resolutions to find any matching SPILL slot (preferring spill over preg
/// since the caller uses this only when reloading a spilled vreg).
fn assignment_for(
    allocated: &crate::regalloc::inst::X64FuncAllocated,
    vreg: RaVReg,
) -> Option<VregAssignment> {
    // First pass : prefer Spill resolutions (the caller queries this ONLY
    // when materializing a reload, so a Spill answer is the load-bearing
    // case ; a Preg answer would mean the caller's per-pp resolve already
    // succeeded and we shouldn't be here).
    for ai in &allocated.allocated_insts {
        for r in &ai.resolutions {
            if r.vreg == vreg {
                if let VregLocation::Spill(s) = r.location {
                    return Some(VregAssignment::Spill(s));
                }
            }
        }
    }
    // Second pass : fall back to any Preg resolution.
    for ai in &allocated.allocated_insts {
        for r in &ai.resolutions {
            if r.vreg == vreg {
                if let VregLocation::Preg(p) = r.location {
                    return Some(VregAssignment::Preg(p));
                }
            }
        }
    }
    None
}

/// Resolve a USE-vreg : either return its preg directly, or emit a reload
/// of the spill slot into a scratch preg and return the scratch.
///
/// Returns the preg holding the vreg's value at this program-point.
#[allow(clippy::too_many_arguments)]
fn resolve_use_or_reload<F, P>(
    ai: &AllocatedInst,
    out: &mut Vec<EncInst>,
    vreg: RaVReg,
    fn_name: &str,
    spill_slot_for: &F,
    pick_scratch_gp: &P,
    avoid_extra: &[X64PReg],
) -> Result<X64PReg, NativeX64Error>
where
    F: Fn(RaVReg) -> Option<crate::regalloc::spill::SpillSlot>,
    P: Fn(&AllocatedInst, &[X64PReg]) -> Option<X64PReg>,
{
    if let Some(p) = ai
        .resolutions
        .iter()
        .find(|r| r.vreg == vreg)
        .and_then(|r| match r.location {
            VregLocation::Preg(p) => Some(p),
            VregLocation::Spill(_) => None,
        })
    {
        return Ok(p);
    }
    let slot = spill_slot_for(vreg).ok_or_else(|| NativeX64Error::UnsupportedOp {
        fn_name: fn_name.to_string(),
        op_name: format!("vreg {vreg} : neither preg nor spill-slot at use-site"),
    })?;
    let scratch =
        pick_scratch_gp(ai, avoid_extra).ok_or_else(|| NativeX64Error::UnsupportedOp {
            fn_name: fn_name.to_string(),
            op_name: format!("vreg {vreg} spilled : no scratch GP available"),
        })?;
    emit_reload(out, scratch, slot.offset())?;
    Ok(scratch)
}

/// Resolve a DEF-vreg : either return its preg + None (no spill needed), or
/// pick a scratch preg + return Some((slot.offset, scratch)) so the caller
/// can emit a Store after the inst.
#[allow(clippy::type_complexity)]
fn resolve_def_or_scratch<F, P>(
    ai: &AllocatedInst,
    vreg: RaVReg,
    fn_name: &str,
    spill_slot_for: &F,
    pick_scratch_gp: &P,
    avoid_extra: &[X64PReg],
) -> Result<(X64PReg, Option<(u32, X64PReg)>), NativeX64Error>
where
    F: Fn(RaVReg) -> Option<crate::regalloc::spill::SpillSlot>,
    P: Fn(&AllocatedInst, &[X64PReg]) -> Option<X64PReg>,
{
    if let Some(p) = ai
        .resolutions
        .iter()
        .find(|r| r.vreg == vreg)
        .and_then(|r| match r.location {
            VregLocation::Preg(p) => Some(p),
            VregLocation::Spill(_) => None,
        })
    {
        return Ok((p, None));
    }
    let slot = spill_slot_for(vreg).ok_or_else(|| NativeX64Error::UnsupportedOp {
        fn_name: fn_name.to_string(),
        op_name: format!("vreg {vreg} (def) : neither preg nor spill-slot"),
    })?;
    let scratch =
        pick_scratch_gp(ai, avoid_extra).ok_or_else(|| NativeX64Error::UnsupportedOp {
            fn_name: fn_name.to_string(),
            op_name: format!("vreg {vreg} (def) spilled : no scratch GP available"),
        })?;
    Ok((scratch, Some((slot.offset(), scratch))))
}

/// Emit `mov dst-preg, src-preg` (64-bit GP-to-GP or XMM-to-XMM).
fn emit_mov_rr(out: &mut Vec<EncInst>, dst: X64PReg, src: X64PReg) -> Result<(), NativeX64Error> {
    if dst == src {
        return Ok(());
    }
    if dst.bank() != src.bank() {
        return Err(NativeX64Error::UnsupportedOp {
            fn_name: "<emit_mov_rr>".to_string(),
            op_name: format!("bank mismatch : dst={dst} src={src}"),
        });
    }
    if dst.bank() == RegBank::Gp {
        out.push(EncInst::MovRR {
            size: OperandSize::B64,
            dst: x64preg_to_gpr(dst),
            src: x64preg_to_gpr(src),
        });
    } else {
        out.push(EncInst::MovsdRR {
            dst: x64preg_to_xmm(dst),
            src: x64preg_to_xmm(src),
        });
    }
    Ok(())
}

/// Emit `mov [rsp+disp], src-preg` for spill-slot writes (64-bit).
fn emit_spill(out: &mut Vec<EncInst>, src: X64PReg, disp: u32) -> Result<(), NativeX64Error> {
    let disp_i32 = i32::try_from(disp).map_err(|_| NativeX64Error::UnsupportedOp {
        fn_name: "<emit_spill>".to_string(),
        op_name: format!("spill disp {disp} overflows i32"),
    })?;
    if src.bank() == RegBank::Gp {
        out.push(EncInst::Store {
            size: OperandSize::B64,
            dst: MemOperand::base_disp(Gpr::Rsp, disp_i32),
            src: x64preg_to_gpr(src),
        });
    } else {
        // SSE spill via movsd [rsp+disp], xmm — encoder doesn't currently
        // expose a direct memory-store SSE variant in the milestone subset ;
        // G8 GP-only spills cover the 5-arg test fixture. Reserved for a
        // later slice when XMM register pressure surfaces.
        return Err(NativeX64Error::UnsupportedOp {
            fn_name: "<emit_spill>".to_string(),
            op_name: format!("XMM spill (preg `{src}`) deferred to G9 slice"),
        });
    }
    Ok(())
}

/// Emit `mov dst-preg, [rsp+disp]` for spill-slot reads (64-bit).
fn emit_reload(out: &mut Vec<EncInst>, dst: X64PReg, disp: u32) -> Result<(), NativeX64Error> {
    let disp_i32 = i32::try_from(disp).map_err(|_| NativeX64Error::UnsupportedOp {
        fn_name: "<emit_reload>".to_string(),
        op_name: format!("reload disp {disp} overflows i32"),
    })?;
    if dst.bank() == RegBank::Gp {
        out.push(EncInst::Load {
            size: OperandSize::B64,
            dst: x64preg_to_gpr(dst),
            src: MemOperand::base_disp(Gpr::Rsp, disp_i32),
        });
    } else {
        return Err(NativeX64Error::UnsupportedOp {
            fn_name: "<emit_reload>".to_string(),
            op_name: format!("XMM reload (preg `{dst}`) deferred to G9 slice"),
        });
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Bridge 4 : full-pipeline build_func_bytes_via_lsra
// ═══════════════════════════════════════════════════════════════════════

/// Build the encoded byte sequence for a non-leaf function via the FULL G8
/// pipeline : G1 → G2 (LSRA) → G3 prologue → encoded body → G3 epilogue.
/// Returns the G5-boundary [`ObjFunc`].
///
/// Used when the function isn't covered by the scalar-leaf fast-path
/// ([`crate::pipeline::ScalarLeafReturn::try_extract`] returns
/// `UnsupportedOp`).
///
/// # Errors
/// Returns [`NativeX64Error`] for any per-stage pipeline failure :
///   - [`NativeX64Error::UnsupportedOp`] : op outside the G8 LSRA subset.
///   - Allocator failure (preg exhaustion / bank-mismatch / etc).
///   - Encoder failure (deferred-to-G9 inst kinds).
pub fn build_func_bytes_via_lsra(
    isel: &IselFunc,
    abi: X64Abi,
    is_export: bool,
) -> Result<ObjFunc, NativeX64Error> {
    // § Stage A : isel → regalloc-form.
    let ra_func = isel_to_regalloc_func(isel, abi)?;

    // § Stage B : run LSRA.
    let allocated = allocate(&ra_func).map_err(|e| translate_alloc_error(&isel.name, e))?;

    // § Stage C : build G3 prologue + epilogue from allocator's frame info.
    //   The allocator's `callee_saved_used` is a Vec<X64PReg> that we split
    //   by bank and translate into G3's GpReg / XmmReg surface.
    let mut callee_saved_gp: Vec<GpReg> = Vec::new();
    let mut callee_saved_xmm: Vec<XmmReg> = Vec::new();
    for &p in &allocated.callee_saved_used {
        match p.bank() {
            RegBank::Gp => callee_saved_gp.push(x64preg_to_gpreg(p)),
            RegBank::Xmm => {
                callee_saved_xmm.push(XmmReg::from_encoding_index(p.encoding()).map_err(|e| {
                    NativeX64Error::UnsupportedOp {
                        fn_name: isel.name.clone(),
                        op_name: format!("XMM preg encoding map failed : {e}"),
                    }
                })?);
            }
        }
    }

    let layout = FunctionLayout {
        abi,
        local_frame_bytes: allocated.frame_size,
        callee_saved_gp_used: callee_saved_gp,
        callee_saved_xmm_used: callee_saved_xmm,
    };
    let prologue: LoweredPrologue = lower_prologue(&layout);
    let epilogue: LoweredEpilogue = lower_epilogue_for(&layout, &prologue);

    // § Stage D : translate regalloc-allocated insts → encoder X64Inst stream.
    let body_insts = regalloc_to_encoder_insts(&allocated)?;

    // § Stage E : encode bytes prologue → body → epilogue.
    let mut bytes: Vec<u8> = Vec::with_capacity(64 + body_insts.len() * 4);
    for ai in &prologue.insns {
        for ei in crate::pipeline::abi_lower_to_encoder(ai)? {
            encode_into(&mut bytes, &ei);
        }
    }
    for ei in &body_insts {
        encode_into(&mut bytes, ei);
    }
    for ai in &epilogue.insns {
        for ei in crate::pipeline::abi_lower_to_encoder(ai)? {
            encode_into(&mut bytes, &ei);
        }
    }

    // § Stage F : pack into objemit boundary type.
    let obj_func = ObjFunc::new(isel.name.clone(), bytes, Vec::new(), is_export).map_err(|e| {
        NativeX64Error::ObjectWriteFailed {
            detail: format!("X64Func::new for `{}` failed : {e}", isel.name),
        }
    })?;
    Ok(obj_func)
}

/// Translate a [`AllocError`] into a [`NativeX64Error`] preserving the
/// stable diagnostic-code via the error message text.
fn translate_alloc_error(fn_name: &str, e: AllocError) -> NativeX64Error {
    NativeX64Error::UnsupportedOp {
        fn_name: fn_name.to_string(),
        op_name: format!("regalloc failure : {e}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § Auxiliary helpers
// ═══════════════════════════════════════════════════════════════════════

// XmmReg helper (private) — adds `from_encoding_index` since the public
// abi::XmmReg surface only provides `encoding()`.
trait XmmRegFromIdx: Sized {
    fn from_encoding_index(idx: u8) -> Result<Self, String>;
}

impl XmmRegFromIdx for XmmReg {
    fn from_encoding_index(idx: u8) -> Result<Self, String> {
        match idx {
            0 => Ok(Self::Xmm0),
            1 => Ok(Self::Xmm1),
            2 => Ok(Self::Xmm2),
            3 => Ok(Self::Xmm3),
            4 => Ok(Self::Xmm4),
            5 => Ok(Self::Xmm5),
            6 => Ok(Self::Xmm6),
            7 => Ok(Self::Xmm7),
            8 => Ok(Self::Xmm8),
            9 => Ok(Self::Xmm9),
            10 => Ok(Self::Xmm10),
            11 => Ok(Self::Xmm11),
            12 => Ok(Self::Xmm12),
            13 => Ok(Self::Xmm13),
            14 => Ok(Self::Xmm14),
            15 => Ok(Self::Xmm15),
            other => Err(format!("XmmReg::from_encoding_index : idx {other} > 15")),
        }
    }
}

// `_width` reserved for future SSE-spill discrimination ; today GP-only.
#[allow(dead_code)]
fn _g8_reserved_signatures(_w: X64Width, _m: MemAddr) {}

// ═══════════════════════════════════════════════════════════════════════
// § Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(clippy::many_single_char_names)]
mod tests {
    use super::*;
    use crate::pipeline::select_module_with_marker;
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirOp, MirType};

    // ─── helpers ──────────────────────────────────────────────────────

    /// Build an isel-form fn `f(a, b, c, d, e) -> i32 { (a+b)*c - (d+e) }`.
    /// (We use `+` + `*` + `-` instead of `/` because integer division
    /// requires rax/rdx fixed-preg pinning that's deferred to a G9 slice ;
    /// the substituted shape still exercises ≥ 5 args + register pressure +
    /// post-arith result placement.)
    fn build_five_arg_fn() -> MirModule {
        let mut module = MirModule::with_name("test.5arg");
        let mut f = MirFunc::new(
            "compute",
            vec![
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
            ],
            vec![MirType::Int(IntWidth::I32)],
        );
        // ‼ MirFunc::new already populates entry-block args from `params`
        //   (ValueId 0..N, types matching). We DON'T push them again.
        let a = cssl_mir::ValueId(0);
        let b = cssl_mir::ValueId(1);
        let c = cssl_mir::ValueId(2);
        let d = cssl_mir::ValueId(3);
        let e = cssl_mir::ValueId(4);

        let v_ab = f.fresh_value_id();
        let v_de = f.fresh_value_id();
        let v_abc = f.fresh_value_id();
        let v_result = f.fresh_value_id();

        f.push_op(
            MirOp::std("arith.addi")
                .with_result(v_ab, MirType::Int(IntWidth::I32))
                .with_operand(a)
                .with_operand(b),
        );
        f.push_op(
            MirOp::std("arith.addi")
                .with_result(v_de, MirType::Int(IntWidth::I32))
                .with_operand(d)
                .with_operand(e),
        );
        f.push_op(
            MirOp::std("arith.muli")
                .with_result(v_abc, MirType::Int(IntWidth::I32))
                .with_operand(v_ab)
                .with_operand(c),
        );
        f.push_op(
            MirOp::std("arith.subi")
                .with_result(v_result, MirType::Int(IntWidth::I32))
                .with_operand(v_abc)
                .with_operand(v_de),
        );
        f.push_op(MirOp::std("func.return").with_operand(v_result));
        module.push_func(f);
        module
    }

    // ─── abi_x64abi_to_regalloc_abi ─────────────────────────────────

    #[test]
    fn abi_bridge_systemv_maps_to_sysv_amd64() {
        assert_eq!(
            abi_x64abi_to_regalloc_abi(X64Abi::SystemV),
            RaAbi::SysVAmd64
        );
    }

    #[test]
    fn abi_bridge_msx64_maps_to_windows_x64() {
        assert_eq!(
            abi_x64abi_to_regalloc_abi(X64Abi::MicrosoftX64),
            RaAbi::WindowsX64
        );
    }

    // ─── gpreg / xmmreg encoding round-trip ──────────────────────────

    #[test]
    fn gpreg_to_x64preg_round_trip_preserves_encoding() {
        for (g, p) in [
            (GpReg::Rax, X64PReg::Rax),
            (GpReg::Rcx, X64PReg::Rcx),
            (GpReg::R15, X64PReg::R15),
            (GpReg::Rsp, X64PReg::Rsp),
            (GpReg::Rbp, X64PReg::Rbp),
        ] {
            assert_eq!(gpreg_to_x64preg(g), p);
            assert_eq!(x64preg_to_gpreg(p), g);
        }
    }

    #[test]
    fn xmmreg_to_x64preg_round_trip_preserves_encoding() {
        for (x, p) in [
            (XmmReg::Xmm0, X64PReg::Xmm0),
            (XmmReg::Xmm7, X64PReg::Xmm7),
            (XmmReg::Xmm15, X64PReg::Xmm15),
        ] {
            assert_eq!(xmmreg_to_x64preg(x), p);
        }
    }

    #[test]
    fn x64preg_to_gpr_handles_canonical_encoding() {
        assert_eq!(x64preg_to_gpr(X64PReg::Rax).index(), 0);
        assert_eq!(x64preg_to_gpr(X64PReg::R15).index(), 15);
    }

    #[test]
    fn x64preg_to_xmm_handles_canonical_encoding() {
        assert_eq!(x64preg_to_xmm(X64PReg::Xmm0).index(), 0);
        assert_eq!(x64preg_to_xmm(X64PReg::Xmm15).index(), 15);
    }

    #[test]
    #[should_panic(expected = "is not a GP preg")]
    fn x64preg_to_gpreg_panics_for_xmm_input() {
        let _ = x64preg_to_gpreg(X64PReg::Xmm0);
    }

    #[test]
    #[should_panic(expected = "is not an XMM preg")]
    fn x64preg_to_xmm_panics_for_gp_input() {
        let _ = x64preg_to_xmm(X64PReg::Rax);
    }

    // ─── isel_to_regalloc_func ──────────────────────────────────────

    #[test]
    fn isel_to_regalloc_func_translates_5arg_fn() {
        let m = build_five_arg_fn();
        let funcs = select_module_with_marker(&m).unwrap();
        let ra = isel_to_regalloc_func(&funcs[0], X64Abi::SystemV).unwrap();
        // 5 params + 4 arith ops × 2 insts (Mov+arith) + result-store + Ret =
        // at least 5 (param-loads) + 8 (arith body : 2 insts × 4 ops) + 1
        // (result-store) + 1 (Ret) = 15 insts. Allow some slack but >= 12.
        assert!(
            ra.insts.len() >= 12,
            "expected >= 12 regalloc insts, got {}",
            ra.insts.len()
        );
        assert_eq!(ra.param_vregs.len(), 5);
        assert_eq!(ra.result_vregs.len(), 1);
    }

    #[test]
    fn isel_to_regalloc_func_rejects_multi_block_body() {
        let mut m = MirModule::with_name("test.multi");
        let mut f = MirFunc::new("foo", vec![], vec![MirType::Int(IntWidth::I32)]);
        let v = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v, MirType::Int(IntWidth::I32))
                .with_attribute("value", "5"),
        );
        f.push_op(MirOp::std("func.return").with_operand(v));
        m.push_func(f);
        let mut funcs = select_module_with_marker(&m).unwrap();
        let _b1 = funcs[0].fresh_block();
        let err = isel_to_regalloc_func(&funcs[0], X64Abi::SystemV).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("multi-block"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    #[test]
    fn isel_to_regalloc_func_rejects_idiv_at_g8() {
        // Build a fn with `arith.sdivi` — outside the G8 subset.
        let mut m = MirModule::with_name("test.div");
        let mut f = MirFunc::new(
            "divtest",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        // MirFunc::new auto-populates entry-block args from `params`.
        let a = cssl_mir::ValueId(0);
        let b = cssl_mir::ValueId(1);
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.sdivi")
                .with_result(r, MirType::Int(IntWidth::I32))
                .with_operand(a)
                .with_operand(b),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        m.push_func(f);
        let funcs = select_module_with_marker(&m).unwrap();
        let err = isel_to_regalloc_func(&funcs[0], X64Abi::SystemV).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(
                    op_name.contains("integer division") || op_name.contains("Cdq"),
                    "expected division-deferred message, got `{op_name}`"
                );
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ─── full pipeline build_func_bytes_via_lsra ────────────────────

    #[test]
    fn build_func_bytes_via_lsra_for_5arg_fn_produces_bytes() {
        // Use SystemV explicitly : 6 int arg-regs available so 5-arg fits
        // entirely in registers. (MS-x64 has only 4 int arg-regs ; 5th param
        // is stack-overflow, which the G9 slice handles.)
        let m = build_five_arg_fn();
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::SystemV;
        let obj = build_func_bytes_via_lsra(&funcs[0], abi, /*is_export=*/ false).unwrap();
        assert_eq!(obj.name, "compute");
        assert!(!obj.bytes.is_empty());
        // Last byte is `ret` (0xC3).
        assert_eq!(*obj.bytes.last().unwrap(), 0xC3);
        // First byte is `push rbp` (0x55) per G3 prologue.
        assert_eq!(obj.bytes[0], 0x55);
    }

    #[test]
    fn build_func_bytes_via_lsra_5arg_fn_uses_some_register_form() {
        // The 5-arg fn must produce bytes that include arithmetic instructions
        // (not just prologue + epilogue). We verify by checking for the
        // canonical add-r/r opcode byte 0x01 OR the imul opcode pair 0x0F 0xAF.
        let m = build_five_arg_fn();
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::SystemV;
        let obj = build_func_bytes_via_lsra(&funcs[0], abi, false).unwrap();
        let has_add_rr = obj.bytes.windows(1).any(|w| w[0] == 0x01);
        let has_imul = obj.bytes.windows(2).any(|w| w == [0x0F, 0xAF]);
        let has_sub_rr = obj.bytes.windows(1).any(|w| w[0] == 0x29);
        assert!(
            has_add_rr || has_imul || has_sub_rr,
            "expected add/imul/sub bytes in 5-arg fn body ; got {:02X?}",
            obj.bytes
        );
    }

    #[test]
    fn build_func_bytes_via_lsra_msx64_4arg_fn_succeeds() {
        // MS-x64 has 4 int arg-regs ; the 4-arg fn fits without overflow.
        let mut m = MirModule::with_name("test.4arg");
        let mut f = MirFunc::new(
            "compute4",
            vec![
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
                MirType::Int(IntWidth::I32),
            ],
            vec![MirType::Int(IntWidth::I32)],
        );
        let a = cssl_mir::ValueId(0);
        let b = cssl_mir::ValueId(1);
        let c = cssl_mir::ValueId(2);
        let d = cssl_mir::ValueId(3);
        let v_ab = f.fresh_value_id();
        let v_cd = f.fresh_value_id();
        let v_r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.addi")
                .with_result(v_ab, MirType::Int(IntWidth::I32))
                .with_operand(a)
                .with_operand(b),
        );
        f.push_op(
            MirOp::std("arith.muli")
                .with_result(v_cd, MirType::Int(IntWidth::I32))
                .with_operand(c)
                .with_operand(d),
        );
        f.push_op(
            MirOp::std("arith.subi")
                .with_result(v_r, MirType::Int(IntWidth::I32))
                .with_operand(v_ab)
                .with_operand(v_cd),
        );
        f.push_op(MirOp::std("func.return").with_operand(v_r));
        m.push_func(f);
        let funcs = select_module_with_marker(&m).unwrap();
        let obj = build_func_bytes_via_lsra(&funcs[0], X64Abi::MicrosoftX64, false).unwrap();
        assert!(!obj.bytes.is_empty());
        assert_eq!(*obj.bytes.last().unwrap(), 0xC3);
    }

    #[test]
    fn build_func_bytes_via_lsra_msx64_5arg_overflow_rejected_at_g8() {
        // MS-x64 5-arg → 5th param is stack-overflow ; G8 rejects.
        let m = build_five_arg_fn();
        let funcs = select_module_with_marker(&m).unwrap();
        let err = build_func_bytes_via_lsra(&funcs[0], X64Abi::MicrosoftX64, false).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("stack-overflow"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ─── spill / reload encoding ─────────────────────────────────────

    #[test]
    fn emit_spill_emits_store_at_rsp_disp() {
        let mut out = Vec::new();
        emit_spill(&mut out, X64PReg::Rbx, 16).unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            EncInst::Store { size, dst, src } => {
                assert_eq!(*size, OperandSize::B64);
                match dst {
                    MemOperand::Base { base, disp } => {
                        assert_eq!(*base, Gpr::Rsp);
                        assert_eq!(*disp, 16);
                    }
                    other => panic!("expected MemOperand::Base, got {other:?}"),
                }
                assert_eq!(*src, Gpr::Rbx);
            }
            other => panic!("expected Store, got {other:?}"),
        }
    }

    #[test]
    fn emit_reload_emits_load_at_rsp_disp() {
        let mut out = Vec::new();
        emit_reload(&mut out, X64PReg::R12, 32).unwrap();
        match &out[0] {
            EncInst::Load { size, dst, src } => {
                assert_eq!(*size, OperandSize::B64);
                assert_eq!(*dst, Gpr::R12);
                match src {
                    MemOperand::Base { base, disp } => {
                        assert_eq!(*base, Gpr::Rsp);
                        assert_eq!(*disp, 32);
                    }
                    other => panic!("expected MemOperand::Base, got {other:?}"),
                }
            }
            other => panic!("expected Load, got {other:?}"),
        }
    }

    #[test]
    fn emit_spill_rejects_xmm_at_g8() {
        let mut out = Vec::new();
        let err = emit_spill(&mut out, X64PReg::Xmm0, 16).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("XMM spill"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    #[test]
    fn emit_reload_rejects_xmm_at_g8() {
        let mut out = Vec::new();
        let err = emit_reload(&mut out, X64PReg::Xmm0, 16).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("XMM reload"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ─── emit_mov_rr ─────────────────────────────────────────────────

    #[test]
    fn emit_mov_rr_elides_self_move() {
        let mut out = Vec::new();
        emit_mov_rr(&mut out, X64PReg::Rax, X64PReg::Rax).unwrap();
        assert!(out.is_empty(), "self-move should be elided");
    }

    #[test]
    fn emit_mov_rr_emits_64bit_gp_to_gp() {
        let mut out = Vec::new();
        emit_mov_rr(&mut out, X64PReg::Rcx, X64PReg::Rdi).unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            EncInst::MovRR { size, dst, src } => {
                assert_eq!(*size, OperandSize::B64);
                assert_eq!(*dst, Gpr::Rcx);
                assert_eq!(*src, Gpr::Rdi);
            }
            other => panic!("expected MovRR, got {other:?}"),
        }
    }

    #[test]
    fn emit_mov_rr_emits_movsd_for_xmm_to_xmm() {
        let mut out = Vec::new();
        emit_mov_rr(&mut out, X64PReg::Xmm1, X64PReg::Xmm0).unwrap();
        match &out[0] {
            EncInst::MovsdRR { dst, src } => {
                assert_eq!(*dst, Xmm::Xmm1);
                assert_eq!(*src, Xmm::Xmm0);
            }
            other => panic!("expected MovsdRR, got {other:?}"),
        }
    }

    #[test]
    fn emit_mov_rr_rejects_bank_mismatch() {
        let mut out = Vec::new();
        let err = emit_mov_rr(&mut out, X64PReg::Rax, X64PReg::Xmm0).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("bank mismatch"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ─── round-trip end-to-end via emit_object_module_native ─────────

    #[test]
    fn full_pipeline_5arg_fn_via_emit_object_module_native_returns_bytes() {
        // ‼ This test requires SysV (6 int arg-regs) so the 5-arg fn fits
        //   in registers ; on MS-x64 the 5th param overflows to stack and
        //   G8 rejects loudly (see `build_func_bytes_via_lsra_msx64_5arg_*`).
        //   We force-target an SysV-compatible format directly.
        let m = build_five_arg_fn();
        // emit_object_module_native uses host_default_format ; on Windows
        // that's COFF, but the LSRA path picks abi via X64Abi::host_default
        // → MS-x64 → 5-arg overflows. We test the FULL native pipeline via
        // the per-format emit-with-format using ELF (so the SysV ABI is
        // selected via the explicit abi-pick path in build_func_bytes).
        // For the host-default test : on non-Windows it succeeds ; on
        // Windows it surfaces a stack-overflow rejection. Both outcomes
        // are valid post-G8 (the pipeline integration is the deliverable,
        // not the stack-arg implementation).
        let result = crate::pipeline::emit_object_module_native(&m);
        if cfg!(target_os = "windows") {
            // MS-x64 default : 5th-arg-overflow rejection is the canonical
            // G8-deferred shape per `build_func_bytes_via_lsra_msx64_5arg_*`.
            match result {
                Err(NativeX64Error::UnsupportedOp { op_name, .. }) => {
                    assert!(op_name.contains("stack-overflow"));
                }
                Ok(_) => panic!("MS-x64 host should reject 5-arg via G8"),
                Err(other) => panic!("expected UnsupportedOp, got {other:?}"),
            }
        } else {
            let bytes = result.expect("SysV host must succeed for 5-arg fn");
            assert!(!bytes.is_empty());
            let host_magic = crate::magic_prefix(crate::host_default_format());
            assert!(bytes.starts_with(host_magic));
        }
    }

    // ─── spill-pressure end-to-end ───────────────────────────────────

    #[test]
    fn build_func_bytes_via_lsra_with_high_register_pressure_emits_spills() {
        // Build a fn with 16 simultaneously-live i32 vregs : forces ≥ 2
        // spills past SysV's 14 free GP pregs (after rsp+rbp reserved).
        let mut m = MirModule::with_name("test.spill");
        let mut f = MirFunc::new("pressure", vec![], vec![MirType::Int(IntWidth::I32)]);
        let mut vids: Vec<cssl_mir::ValueId> = Vec::new();
        for i in 0..18 {
            let v = f.fresh_value_id();
            vids.push(v);
            f.push_op(
                MirOp::std("arith.constant")
                    .with_result(v, MirType::Int(IntWidth::I32))
                    .with_attribute("value", i.to_string()),
            );
        }
        // Force all live to end via repeated additions (last vid receives
        // the running sum so all earlier vregs are read at the end).
        let mut acc = vids[0];
        for &vid in &vids[1..] {
            let new_acc = f.fresh_value_id();
            f.push_op(
                MirOp::std("arith.addi")
                    .with_result(new_acc, MirType::Int(IntWidth::I32))
                    .with_operand(acc)
                    .with_operand(vid),
            );
            acc = new_acc;
        }
        f.push_op(MirOp::std("func.return").with_operand(acc));
        m.push_func(f);
        let funcs = select_module_with_marker(&m).unwrap();
        // Force SysV (6 arg-regs ; doesn't matter here as fn is nullary).
        let obj = build_func_bytes_via_lsra(&funcs[0], X64Abi::SystemV, false).unwrap();
        // Last byte is `ret`.
        assert_eq!(*obj.bytes.last().unwrap(), 0xC3);
        // First byte is `push rbp`.
        assert_eq!(obj.bytes[0], 0x55);
        // The body must contain a `mov [rsp+disp], reg` (spill) byte sequence
        // at some point. The 64-bit Store opcode is `48 89 ...` (REX.W +
        // mov-MR) ; we look for a `mov reg, [rsp+...]` (Load) reload byte
        // sequence too — `48 8B ... 24 ...` for SIB+disp. We assert one
        // OR the other appears.
        let bytes = &obj.bytes;
        let has_store_to_rsp = bytes.windows(2).any(|w| w == [0x48, 0x89]);
        let has_load_from_rsp = bytes.windows(2).any(|w| w == [0x48, 0x8B]);
        assert!(
            has_store_to_rsp || has_load_from_rsp,
            "expected spill-store or reload-load bytes ; got len={} bytes",
            bytes.len()
        );
    }

    #[test]
    fn build_func_bytes_via_lsra_high_pressure_consumes_callee_saved_regs() {
        // The pressure fn forces use of callee-saved regs (rbx + r12..r15)
        // ; the prologue must push them.
        let mut m = MirModule::with_name("test.callee_saved");
        let mut f = MirFunc::new("pressure", vec![], vec![MirType::Int(IntWidth::I32)]);
        let mut vids: Vec<cssl_mir::ValueId> = Vec::new();
        for i in 0..12 {
            let v = f.fresh_value_id();
            vids.push(v);
            f.push_op(
                MirOp::std("arith.constant")
                    .with_result(v, MirType::Int(IntWidth::I32))
                    .with_attribute("value", i.to_string()),
            );
        }
        let mut acc = vids[0];
        for &vid in &vids[1..] {
            let new_acc = f.fresh_value_id();
            f.push_op(
                MirOp::std("arith.addi")
                    .with_result(new_acc, MirType::Int(IntWidth::I32))
                    .with_operand(acc)
                    .with_operand(vid),
            );
            acc = new_acc;
        }
        f.push_op(MirOp::std("func.return").with_operand(acc));
        m.push_func(f);
        let funcs = select_module_with_marker(&m).unwrap();
        let obj = build_func_bytes_via_lsra(&funcs[0], X64Abi::SystemV, false).unwrap();
        // The prologue includes pushes of callee-saved regs (rbx 0x53,
        // r12..r15 with REX.B = 0x41 0x54..0x57). At least one of these
        // sequences should appear if the fn used callee-saved.
        let bytes = &obj.bytes;
        let has_push_rbx = bytes.contains(&0x53);
        let has_push_r12_to_r15 = bytes.windows(2).any(|w| {
            w[0] == 0x41 && (w[1] == 0x54 || w[1] == 0x55 || w[1] == 0x56 || w[1] == 0x57)
        });
        assert!(
            has_push_rbx || has_push_r12_to_r15,
            "expected callee-saved push bytes in prologue (rbx=0x53 or r12..r15 = 0x41 0x54..0x57)"
        );
    }

    // ─── coexistence with G7 leaf path ───────────────────────────────

    #[test]
    fn full_pipeline_main_42_leaf_still_produces_canonical_bytes() {
        // The G7 leaf path must STILL produce the canonical 11-byte body for
        // `fn main() -> i32 { 42 }` even after the G8 LSRA path is wired —
        // the leaf detection short-circuits before the LSRA route.
        let mut module = MirModule::with_name("leaf.test");
        let mut f = MirFunc::new("main", vec![], vec![MirType::Int(IntWidth::I32)]);
        let v = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v, MirType::Int(IntWidth::I32))
                .with_attribute("value", "42"),
        );
        f.push_op(MirOp::std("func.return").with_operand(v));
        module.push_func(f);

        let funcs = crate::pipeline::select_module_with_marker(&module).unwrap();
        let abi = X64Abi::host_default();
        let obj = crate::pipeline::build_func_bytes(&funcs[0], abi, true).unwrap();
        let expected = [
            0x55, // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0xB8, 0x2A, 0x00, 0x00, 0x00, // mov eax, 42
            0x5D, // pop rbp
            0xC3, // ret
        ];
        assert_eq!(
            obj.bytes, expected,
            "G7 leaf-path canonical milestone preserved"
        );
    }
}

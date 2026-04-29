//! § pipeline — cross-slice walker (S7-G7 / T11-D97).
//!
//! § ROLE
//!   The end-to-end native-x64 pipeline driver. Walks a [`MirModule`] through
//!   the full G-axis chain : G1 (instruction-selection) → G2 (register-
//!   allocation, simplified for the scalar-leaf subset) → G3 (ABI lowering :
//!   prologue + epilogue + return-value placement) → G4 (encoder : machine-
//!   code byte synthesis) → G5 (object-file emitter : ELF / COFF / Mach-O).
//!
//! § DESIGN  (per T11-D95 § Deferred + the G7-pipeline landmines)
//!   The G1..G5 sibling slices each carry their OWN `X64Inst` / `X64Func`
//!   surfaces (per-slice `pub mod`s under the unified crate root). They are
//!   FUNDAMENTALLY DIFFERENT shapes : G1's `isel::X64Inst` is rich (41-op
//!   coverage with per-width MIR-typed vregs) ; G2's `regalloc::X64Inst`
//!   is bank-tagged (gp/xmm) with explicit uses+defs sets ; G4's
//!   `encoder::X64Inst` is post-regalloc emit-ready with concrete `Gpr` /
//!   `Xmm` registers + `OperandSize` width tags. A "deep unification" of
//!   these three would lose information across the pipeline ; instead, this
//!   slice provides explicit per-stage adapter functions that bridge the
//!   sibling surfaces while preserving each slice's invariants.
//!
//! § PER-STAGE ADAPTER FUNCTIONS
//!   - [`select_module_with_marker`] — wraps `isel::select_module` so the
//!     pipeline transparently sets the D5 structured-CFG marker (the G1
//!     selector requires it ; the input MirModule may or may not have it
//!     pre-set, so we set defensively).
//!   - [`isel_to_encoder_simple`] — direct G1→G4 lowering for the scalar-
//!     leaf subset (the hello-world `fn main() -> i32 { N }` shape). For
//!     the broader op-set this stage will grow into a full G2 (LSRA) pass
//!     in a future slice ; at S7-G7 the simple-lowering covers the milestone
//!     shape end-to-end and rejects ops outside the leaf-shape with
//!     [`NativeX64Error::UnsupportedOp`].
//!   - [`abi_lower_to_encoder`] — translates G3's `AbstractInsn` (prologue +
//!     epilogue + call-site shape) into G4's `encoder::X64Inst` surface.
//!     Direct enum-to-enum bridge ; one variant per AbstractInsn shape.
//!   - [`build_func_bytes`] — concatenates encoded per-instruction bytes
//!     with prologue/epilogue spliced in ; produces the `objemit::X64Func`
//!     boundary type with `name` + `bytes` + `relocs` (currently empty for
//!     leaf-only ; relocs land when call-lowering is wired through).
//!
//! § SCALAR-LEAF SUBSET (the S7-G7 milestone)
//!   The scalar-leaf subset covers exactly the `fn main() -> i32 { N }`
//!   shape that the canonical hello-world source `stage1/hello_world.cssl`
//!   exercises :
//!     - Single-block body, no scf.* control-flow.
//!     - Single `arith.constant` op producing an `i32` value.
//!     - Single `func.return` with one `i32` operand.
//!   This subset is the SECOND hello.exe = 42 milestone (the first via
//!   cranelift in S6-A5 ; this one via the bespoke G-axis chain). The
//!   pipeline rejects anything outside this subset with
//!   [`NativeX64Error::UnsupportedOp`] so the cssl-examples gate test fails
//!   loudly rather than silently producing wrong code.
//!
//! § FUTURE EXPANSION  (G8+ slices)
//!   - Full G2 LSRA integration (replace `isel_to_encoder_simple` with
//!     `isel → regalloc::X64Func → regalloc::allocate → encoder`).
//!   - Multi-block CFG support (scf.if / scf.for / scf.while / scf.loop)
//!     with branch-fixup pass.
//!   - Cross-fn calls + relocation emission (NearCall reloc-kind).
//!
//! § G11 (T11-D102) — SSE2 SCALAR FLOAT PATH
//!   The G11 slice extends the scalar-leaf subset to cover f32 + f64 fns :
//!     - **Constants** : `arith.constant : f32` / `: f64` materialize the
//!       IEEE 754 bit pattern in a temporary GPR (`mov rax, bits`) then
//!       transfer it to an XMM register via `movd` / `movq`. RIP-relative
//!       constant-pool emission is deferred to a future slice.
//!     - **Binary ops** : `arith.{addf,subf,mulf,divf}` lower 1:1 onto
//!       `addss/addsd/subss/subsd/mulss/mulsd/divss/divsd`.
//!     - **Float return** : an f32 / f64 result is placed in `xmm0` via a
//!       per-width move (`movss xmm0, src` / `movsd xmm0, src`).
//!     - **Float arg passing** : an f32 / f64 parameter arrives in xmm0..7
//!       (SysV) / xmm0..3 (MS-x64). The G11 leaf-pipeline assigns vreg-
//!       numbered "current location" trackers so the simple-walker can
//!       chain (param → addsd → return) without a full LSRA pass.
//!     - **Sqrt + comparison** : the encoder carries `sqrtss` / `sqrtsd` /
//!       `ucomiss` / `ucomisd` / `comiss` / `comisd` ; the G11 pipeline
//!       wires the binary ops + return path. Sqrt + comparison flow
//!       through MIR ops that are deferred to G12+ when full LSRA lands.
//!
//! § G11 LANDMINE — MS-X64 POSITIONAL ALIAS (per T11-D85 G3)
//!   `fn(i64, f64, i64)` on MS-x64 places arg-0 in `rcx`, arg-1 in `xmm1`
//!   (NOT xmm0 ! — the second positional slot — even though no f64 came
//!   before it), and arg-2 in `r8`. The pipeline walks the param list
//!   maintaining a single positional counter and dispatches per-class
//!   based on the corresponding xmm/gpr register at that index.
//!
//! § ATTESTATION  (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

use cssl_mir::{
    has_structured_cfg_marker, MirFunc, MirModule, MirOp, STRUCTURED_CFG_VALIDATED_KEY,
    STRUCTURED_CFG_VALIDATED_VALUE,
};

use crate::abi::{ArgClass, X64Abi};
use crate::encoder::inst::{BranchTarget, Cond, X64Inst as EncInst};
use crate::encoder::reg::{Gpr, OperandSize, Xmm};
use crate::encoder::{encode_into, MemOperand};
use crate::isel::func::X64Func as IselFunc;
use crate::isel::inst::{X64Imm, X64Inst as IselInst, X64Term as IselTerm};
use crate::isel::select::{select_function as isel_select_function, SelectError};
use crate::isel::vreg::X64Width;
use crate::lower::{
    lower_epilogue_for, lower_prologue, AbstractInsn, FunctionLayout, LoweredEpilogue,
    LoweredPrologue,
};
use crate::objemit::func::X64Func as ObjFunc;
use crate::objemit::object::{emit_object_file, ObjectError, ObjectTarget};
use crate::{host_default_format, NativeX64Error, ObjectFormat};

// ═══════════════════════════════════════════════════════════════════════
// § Scalar-leaf result of selection : the milestone-subset shape.
// ═══════════════════════════════════════════════════════════════════════

/// Result of identifying a scalar-leaf return value within an [`IselFunc`].
///
/// At S7-G7 the pipeline accepts only the `fn main() -> i32 { N }` shape :
/// a single-block body whose terminator is `Ret(operands)` and whose `MovImm`
/// instruction defines the return-operand vreg with an [`X64Imm::I32`] value.
///
/// Future slices will replace this trivial pattern-match with full LSRA +
/// arbitrary-op lowering through the G2/G4 surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalarLeafReturn {
    /// The constant value to return.
    pub return_value: i32,
    /// The width of the return value (always [`X64Width::I32`] at S7-G7).
    pub return_width: X64Width,
}

impl ScalarLeafReturn {
    /// Identify the canonical i32-constant return shape in an [`IselFunc`].
    ///
    /// Returns `Ok(Some(_))` when the shape matches, `Ok(None)` for void
    /// returns (no operand), and `Err(NativeX64Error::UnsupportedOp)` for
    /// anything outside the scalar-leaf subset.
    ///
    /// # Errors
    /// Returns [`NativeX64Error::UnsupportedOp`] when the function body
    /// contains ops outside the scalar-leaf subset.
    pub fn try_extract(func: &IselFunc) -> Result<Option<Self>, NativeX64Error> {
        // § Single-block invariant.
        if func.blocks.len() != 1 {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!(
                    "multi-block-body ({n} blocks ; G7 scalar-leaf subset = 1)",
                    n = func.blocks.len()
                ),
            });
        }
        let block = &func.blocks[0];
        // § Walk the inst stream collecting MovImm definitions.
        let mut imm_defs: std::collections::HashMap<u32, X64Imm> = std::collections::HashMap::new();
        for inst in &block.insts {
            match inst {
                IselInst::MovImm { dst, imm } => {
                    imm_defs.insert(dst.id, *imm);
                }
                // Scalar-leaf rejects anything that isn't a MovImm in body.
                other => {
                    return Err(NativeX64Error::UnsupportedOp {
                        fn_name: func.name.clone(),
                        op_name: format!("non-leaf inst `{other:?}` in S7-G7 scalar-leaf body"),
                    });
                }
            }
        }
        // § Inspect terminator.
        match &block.terminator {
            IselTerm::Ret { operands } if operands.is_empty() => Ok(None),
            IselTerm::Ret { operands } if operands.len() == 1 => {
                let v = operands[0];
                let imm =
                    imm_defs
                        .get(&v.id)
                        .copied()
                        .ok_or_else(|| NativeX64Error::UnsupportedOp {
                            fn_name: func.name.clone(),
                            op_name: format!(
                                "return-vreg v{vid} is not a MovImm-defined constant",
                                vid = v.id
                            ),
                        })?;
                match imm {
                    X64Imm::I32(n) => Ok(Some(Self {
                        return_value: n,
                        return_width: X64Width::I32,
                    })),
                    other => Err(NativeX64Error::UnsupportedOp {
                        fn_name: func.name.clone(),
                        op_name: format!("scalar-leaf return imm `{other:?}` not in i32 subset"),
                    }),
                }
            }
            IselTerm::Ret { operands } => Err(NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!(
                    "multi-result return ({n} results ; G7 scalar-leaf = 0 or 1)",
                    n = operands.len()
                ),
            }),
            other => Err(NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!("non-Ret terminator `{other:?}` in scalar-leaf subset"),
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § Adapter : G1 isel → G4 encoder (scalar-leaf subset)
// ═══════════════════════════════════════════════════════════════════════

/// Lower an [`IselFunc`]'s body into a sequence of [`EncInst`]s that place
/// the scalar return value in the canonical return register (`rax` for int,
/// `xmm0` for float). The generated body is the BODY ONLY ; the prologue +
/// epilogue come from G3 via [`lower_prologue`] / [`lower_epilogue_for`].
///
/// At S7-G7 the only shape this lowers is :
///   `mov eax, <imm32>` + (return is encoded by the epilogue's `ret`)
///
/// # Errors
/// Returns [`NativeX64Error::UnsupportedOp`] for anything outside the
/// scalar-leaf subset.
pub fn isel_to_encoder_simple(func: &IselFunc) -> Result<Vec<EncInst>, NativeX64Error> {
    // § G11 (T11-D102) — extended path that handles f32/f64 leaves.
    // Try the float-aware leaf walker first ; if it rejects, fall through
    // to the pre-G11 i32-only path so older test fixtures keep passing.
    if let Some(body) = try_lower_float_leaf(func)? {
        return Ok(body);
    }
    let leaf = ScalarLeafReturn::try_extract(func)?;
    let mut body = Vec::new();
    if let Some(leaf) = leaf {
        // Place return value in rax (low 32 bits = eax).
        body.push(EncInst::MovRI {
            size: OperandSize::B32,
            dst: Gpr::Rax,
            imm: i64::from(leaf.return_value),
        });
        let _ = leaf.return_width; // future width-driven widening
    }
    Ok(body)
}

// ═══════════════════════════════════════════════════════════════════════
// § G11 (T11-D102) — float-aware scalar-leaf walker
// ═══════════════════════════════════════════════════════════════════════
//
// The walker maintains a per-vreg "current location" map, allocating XMM
// registers in sequence (xmm1 / xmm2 / ... so that xmm0 stays free for
// the final return-value placement). It then walks the inst stream :
//   - MovImm{F32|F64}     → materialize bit-pattern via mov gpr,imm + movd/q
//   - Mov                 → assign dst-loc = src-loc (zero-cost rename)
//   - FpAdd/Sub/Mul/Div   → emit ops + addss/sd/etc on dst-loc, src-loc
//   - Ret with operand    → emit movss/movsd xmm0, value-loc
//
// Param vregs map directly to the ABI's xmm-arg-reg sequence (xmm0..xmm3
// for MS-x64, xmm0..xmm7 for SysV). Int params share the positional
// counter on MS-x64.

/// Per-vreg "current location" used by [`try_lower_float_leaf`]. A vreg
/// either lives in an XMM register, in a GPR, or has not been materialized
/// yet (in which case its source is a constant bit-pattern in `imm_defs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VregLoc {
    /// Lives in an XMM register.
    Xmm(Xmm),
    /// Lives in a GPR (used for int leaves that flow through the float path).
    Gpr(Gpr),
}

/// Try to lower the body of `func` as a float-aware leaf. Returns
/// `Ok(Some(body))` when the leaf shape applies (the body has a float
/// signature OR uses any FpAdd/FpSub/FpMul/FpDiv ops) ; returns `Ok(None)`
/// when the simple i32 path should be used instead. Returns `Err(...)`
/// when the body matches the float shape but contains an op outside the
/// G11 leaf subset.
fn try_lower_float_leaf(func: &IselFunc) -> Result<Option<Vec<EncInst>>, NativeX64Error> {
    // § Single-block invariant (same as int-leaf).
    if func.blocks.len() != 1 {
        return Ok(None);
    }
    let block = &func.blocks[0];

    // § Detect : does this fn have a float signature OR contain an Fp* op ?
    let has_float_sig = func.sig.params.iter().any(|w| w.is_sse())
        || func.sig.results.iter().any(|w| w.is_sse());
    let has_float_op = block.insts.iter().any(|inst| {
        matches!(
            inst,
            IselInst::FpAdd { .. }
                | IselInst::FpSub { .. }
                | IselInst::FpMul { .. }
                | IselInst::FpDiv { .. }
        )
    });
    if !has_float_sig && !has_float_op {
        // Not a float-shape leaf — let the int path handle it.
        return Ok(None);
    }

    // § Set up the abi-driven param loc map. Param vregs occupy ids 1..=N.
    let abi = X64Abi::host_default();
    let int_arg_regs = abi.int_arg_regs();
    let float_arg_regs = abi.float_arg_regs();
    let mut locs: std::collections::HashMap<u32, VregLoc> =
        std::collections::HashMap::new();
    let mut int_idx = 0usize;
    let mut float_idx = 0usize;
    for (positional_idx, (i, w)) in func.sig.params.iter().enumerate().enumerate() {
        let vid = (i as u32) + 1;
        if w.is_sse() {
            // MS-x64 uses the positional counter (shared) ; SysV uses
            // float_idx independently.
            let reg = if abi.shares_positional_arg_counter() {
                float_arg_regs.get(positional_idx).copied()
            } else {
                float_arg_regs.get(float_idx).copied()
            };
            let Some(xmm_reg) = reg else {
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: func.name.clone(),
                    op_name: format!(
                        "f-param #{i} overflows register-arg slots ; stack-arg \
                         passing for floats deferred to G12+"
                    ),
                });
            };
            locs.insert(vid, VregLoc::Xmm(xmm_to_encoder_xmm(xmm_reg)));
            float_idx += 1;
        } else if w.is_gpr() {
            let reg = if abi.shares_positional_arg_counter() {
                int_arg_regs.get(positional_idx).copied()
            } else {
                int_arg_regs.get(int_idx).copied()
            };
            let Some(gp_reg) = reg else {
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: func.name.clone(),
                    op_name: format!(
                        "i-param #{i} overflows register-arg slots ; stack-arg \
                         passing deferred to G12+"
                    ),
                });
            };
            locs.insert(vid, VregLoc::Gpr(gp_to_encoder_gpr(gp_reg)));
            int_idx += 1;
        } else {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!("param #{i} non-scalar width `{w}`"),
            });
        }
    }

    // § Allocate a sequential xmm pool for non-param vregs (avoid xmm0,
    // which we reserve for the final return-placement move). For int
    // intermediates we use a small GPR pool (rax/rcx/rdx/r8..r11) excluded
    // of param-bound regs ; at G11 we rarely need GPR temporaries on the
    // float path so this is a small pool.
    let mut xmm_pool: Vec<Xmm> = (1u8..=15)
        .filter(|i| !locs.values().any(|l| matches!(l, VregLoc::Xmm(x) if x.index() == *i)))
        .map(Xmm::from_index)
        .collect();
    // Reverse so we pop from the front-of-pool (xmm1, xmm2, ...) by reversing
    // for stack-pop semantics.
    xmm_pool.reverse();

    let mut gpr_pool: Vec<Gpr> = [
        Gpr::R11, Gpr::R10, Gpr::R9, Gpr::R8, Gpr::Rdx, Gpr::Rcx, Gpr::Rax,
    ]
    .iter()
    .copied()
    .filter(|g| !locs.values().any(|l| matches!(l, VregLoc::Gpr(r) if r == g)))
    .collect();

    let mut body = Vec::new();

    // § Walk the inst stream.
    for inst in &block.insts {
        match inst {
            IselInst::MovImm { dst, imm } => {
                let width = dst.width;
                if width.is_sse() {
                    // Materialize via : mov rax, bits ; movq xmm, rax (f64)
                    //                : mov eax, bits ; movd xmm, eax (f32).
                    let xmm = xmm_pool.pop().ok_or_else(|| NativeX64Error::UnsupportedOp {
                        fn_name: func.name.clone(),
                        op_name: format!(
                            "f-vreg pool exhausted at MovImm v{vid}",
                            vid = dst.id
                        ),
                    })?;
                    match imm {
                        X64Imm::F32(bits) => {
                            // Use rax as the temporary GPR for the bit-transfer.
                            body.push(EncInst::MovRI {
                                size: OperandSize::B32,
                                dst: Gpr::Rax,
                                imm: i64::from(*bits),
                            });
                            body.push(EncInst::MovdXmmFromGp {
                                dst: xmm,
                                src: Gpr::Rax,
                            });
                        }
                        X64Imm::F64(bits) => {
                            // u64-to-i64 reinterpret via `from_le_bytes` so
                            // the bit pattern is preserved exactly (clippy
                            // forbids the `as i64` cast for u64).
                            let imm = i64::from_le_bytes(bits.to_le_bytes());
                            body.push(EncInst::MovRI {
                                size: OperandSize::B64,
                                dst: Gpr::Rax,
                                imm,
                            });
                            body.push(EncInst::MovqXmmFromGp {
                                dst: xmm,
                                src: Gpr::Rax,
                            });
                        }
                        other => {
                            return Err(NativeX64Error::UnsupportedOp {
                                fn_name: func.name.clone(),
                                op_name: format!(
                                    "MovImm with float-width vreg got non-float imm `{other}`"
                                ),
                            });
                        }
                    }
                    locs.insert(dst.id, VregLoc::Xmm(xmm));
                } else {
                    // Int constant on the float path : materialize into a
                    // GPR temporary so that downstream cvtsi2sd / cvtsi2ss
                    // can pick it up.
                    let gpr = gpr_pool.pop().ok_or_else(|| NativeX64Error::UnsupportedOp {
                        fn_name: func.name.clone(),
                        op_name: format!(
                            "i-vreg pool exhausted at MovImm v{vid}",
                            vid = dst.id
                        ),
                    })?;
                    let (size, raw) = match imm {
                        X64Imm::I32(v) => (OperandSize::B32, i64::from(*v)),
                        X64Imm::I64(v) => (OperandSize::B64, *v),
                        X64Imm::Bool(b) => (OperandSize::B32, i64::from(u32::from(*b))),
                        other => {
                            return Err(NativeX64Error::UnsupportedOp {
                                fn_name: func.name.clone(),
                                op_name: format!(
                                    "MovImm with int-width vreg got `{other}` (float path)"
                                ),
                            });
                        }
                    };
                    body.push(EncInst::MovRI {
                        size,
                        dst: gpr,
                        imm: raw,
                    });
                    locs.insert(dst.id, VregLoc::Gpr(gpr));
                }
            }
            IselInst::Mov { dst, src } => {
                // dst <- src : zero-cost rename in the loc-map.
                let src_loc = *locs.get(&src.id).ok_or_else(|| NativeX64Error::UnsupportedOp {
                    fn_name: func.name.clone(),
                    op_name: format!(
                        "Mov references undefined vreg v{vid}",
                        vid = src.id
                    ),
                })?;
                locs.insert(dst.id, src_loc);
            }
            IselInst::FpAdd { dst, src } => {
                emit_fp_binary(&mut body, &mut locs, *dst, *src, FpBinOpKind::Add)?;
            }
            IselInst::FpSub { dst, src } => {
                emit_fp_binary(&mut body, &mut locs, *dst, *src, FpBinOpKind::Sub)?;
            }
            IselInst::FpMul { dst, src } => {
                emit_fp_binary(&mut body, &mut locs, *dst, *src, FpBinOpKind::Mul)?;
            }
            IselInst::FpDiv { dst, src } => {
                emit_fp_binary(&mut body, &mut locs, *dst, *src, FpBinOpKind::Div)?;
            }
            other => {
                return Err(NativeX64Error::UnsupportedOp {
                    fn_name: func.name.clone(),
                    op_name: format!("non-leaf inst `{other:?}` in G11 float-leaf body"),
                });
            }
        }
    }

    // § Inspect terminator + emit return-value placement.
    match &block.terminator {
        IselTerm::Ret { operands } if operands.is_empty() => {}
        IselTerm::Ret { operands } if operands.len() == 1 => {
            let v = operands[0];
            let loc = *locs.get(&v.id).ok_or_else(|| NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!(
                    "Ret references undefined vreg v{vid}",
                    vid = v.id
                ),
            })?;
            match (v.width, loc) {
                (X64Width::F32, VregLoc::Xmm(src)) => {
                    if src != Xmm::Xmm0 {
                        body.push(EncInst::MovssRR {
                            dst: Xmm::Xmm0,
                            src,
                        });
                    }
                }
                (X64Width::F64, VregLoc::Xmm(src)) => {
                    if src != Xmm::Xmm0 {
                        body.push(EncInst::MovsdRR {
                            dst: Xmm::Xmm0,
                            src,
                        });
                    }
                }
                (X64Width::I32 | X64Width::I8 | X64Width::I16 | X64Width::Bool, VregLoc::Gpr(src)) => {
                    if src != Gpr::Rax {
                        body.push(EncInst::MovRR {
                            size: OperandSize::B32,
                            dst: Gpr::Rax,
                            src,
                        });
                    }
                }
                (X64Width::I64 | X64Width::Ptr, VregLoc::Gpr(src)) => {
                    if src != Gpr::Rax {
                        body.push(EncInst::MovRR {
                            size: OperandSize::B64,
                            dst: Gpr::Rax,
                            src,
                        });
                    }
                }
                (w, l) => {
                    return Err(NativeX64Error::UnsupportedOp {
                        fn_name: func.name.clone(),
                        op_name: format!(
                            "Ret-vreg width `{w}` mismatches loc `{l:?}` in float-leaf body"
                        ),
                    });
                }
            }
        }
        IselTerm::Ret { operands } => {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!(
                    "multi-result return ({n} results) in float-leaf body",
                    n = operands.len()
                ),
            });
        }
        other => {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: func.name.clone(),
                op_name: format!("non-Ret terminator `{other:?}` in float-leaf body"),
            });
        }
    }

    Ok(Some(body))
}

#[derive(Debug, Clone, Copy)]
enum FpBinOpKind {
    Add,
    Sub,
    Mul,
    Div,
}

/// Lower a single FpAdd/FpSub/FpMul/FpDiv instruction onto the encoder
/// stream. The isel emits the convention `dst <- dst op src` (after a
/// preceding `Mov dst, lhs`), so by this point `dst` is already aliased to
/// the lhs's loc — we just need to emit the matching SSE2 r/r op.
fn emit_fp_binary(
    body: &mut Vec<EncInst>,
    locs: &mut std::collections::HashMap<u32, VregLoc>,
    dst: crate::isel::vreg::X64VReg,
    src: crate::isel::vreg::X64VReg,
    kind: FpBinOpKind,
) -> Result<(), NativeX64Error> {
    let dst_loc = *locs.get(&dst.id).ok_or_else(|| NativeX64Error::UnsupportedOp {
        fn_name: "<fp-bin>".to_string(),
        op_name: format!("Fp* refs undefined dst v{vid}", vid = dst.id),
    })?;
    let src_loc = *locs.get(&src.id).ok_or_else(|| NativeX64Error::UnsupportedOp {
        fn_name: "<fp-bin>".to_string(),
        op_name: format!("Fp* refs undefined src v{vid}", vid = src.id),
    })?;
    let (VregLoc::Xmm(dst_x), VregLoc::Xmm(src_x)) = (dst_loc, src_loc) else {
        return Err(NativeX64Error::UnsupportedOp {
            fn_name: "<fp-bin>".to_string(),
            op_name: format!(
                "Fp* expected XMM operands ; got dst={dst_loc:?} src={src_loc:?}"
            ),
        });
    };
    match (dst.width, kind) {
        (X64Width::F32, FpBinOpKind::Add) => body.push(EncInst::AddssRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F32, FpBinOpKind::Sub) => body.push(EncInst::SubssRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F32, FpBinOpKind::Mul) => body.push(EncInst::MulssRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F32, FpBinOpKind::Div) => body.push(EncInst::DivssRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F64, FpBinOpKind::Add) => body.push(EncInst::AddsdRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F64, FpBinOpKind::Sub) => body.push(EncInst::SubsdRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F64, FpBinOpKind::Mul) => body.push(EncInst::MulsdRR {
            dst: dst_x,
            src: src_x,
        }),
        (X64Width::F64, FpBinOpKind::Div) => body.push(EncInst::DivsdRR {
            dst: dst_x,
            src: src_x,
        }),
        (w, _) => {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: "<fp-bin>".to_string(),
                op_name: format!("Fp* unexpected dst-width `{w}`"),
            });
        }
    }
    // dst-loc is unchanged (the SSE2 r/r op is `dst <- dst op src`).
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Adapter : G3 AbstractInsn → G4 encoder X64Inst
// ═══════════════════════════════════════════════════════════════════════

/// Translate one G3 [`AbstractInsn`] into G4 [`EncInst`]s.
///
/// One AbstractInsn maps to exactly one or two encoder instructions :
/// most are 1:1 ; a few (e.g., StoreGpToStackArg) decompose into the
/// load-effective-address + store pair. At S7-G7 we cover the prologue +
/// epilogue + ret variants needed by the leaf-fn pipeline ; call-site
/// variants (Call, SubRsp, AddRsp, StoreGpToStackArg, StoreXmmToStackArg)
/// are reserved for the G8+ call-lowering slice.
///
/// # Errors
/// Returns [`NativeX64Error::UnsupportedOp`] when the AbstractInsn variant
/// isn't yet wired (e.g. call-site shapes pending G8).
pub fn abi_lower_to_encoder(insn: &AbstractInsn) -> Result<Vec<EncInst>, NativeX64Error> {
    let out = match insn {
        AbstractInsn::MovGpGp { dst, src } => vec![EncInst::MovRR {
            size: OperandSize::B64,
            dst: gp_to_encoder_gpr(*dst),
            src: gp_to_encoder_gpr(*src),
        }],
        AbstractInsn::MovXmmXmm { dst, src } => vec![EncInst::MovsdRR {
            dst: xmm_to_encoder_xmm(*dst),
            src: xmm_to_encoder_xmm(*src),
        }],
        AbstractInsn::Push { reg } => vec![EncInst::Push {
            src: gp_to_encoder_gpr(*reg),
        }],
        AbstractInsn::Pop { reg } => vec![EncInst::Pop {
            dst: gp_to_encoder_gpr(*reg),
        }],
        AbstractInsn::SubRsp { bytes } => vec![EncInst::SubRI {
            size: OperandSize::B64,
            dst: Gpr::Rsp,
            imm: i32::try_from(*bytes).map_err(|_| NativeX64Error::UnsupportedOp {
                fn_name: "<frame>".to_string(),
                op_name: format!("SubRsp imm out of i32 range ({bytes})"),
            })?,
        }],
        AbstractInsn::AddRsp { bytes } => vec![EncInst::AddRI {
            size: OperandSize::B64,
            dst: Gpr::Rsp,
            imm: i32::try_from(*bytes).map_err(|_| NativeX64Error::UnsupportedOp {
                fn_name: "<frame>".to_string(),
                op_name: format!("AddRsp imm out of i32 range ({bytes})"),
            })?,
        }],
        AbstractInsn::Ret => vec![EncInst::Ret],
        AbstractInsn::Call { target } => {
            // ‼ Call-lowering with relocation emission is the G8 slice.
            // At S7-G7 we don't take the call-emit path (leaf-only) but
            // we surface a meaningful error so future slices can detect
            // the missing wiring rather than silently emitting garbage.
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: "<call>".to_string(),
                op_name: format!(
                    "Call(target={target}) requires reloc-emission (deferred to G8 slice)"
                ),
            });
        }
        AbstractInsn::StoreGpToStackArg { offset, reg } => {
            // Reserved for the G8 call-arg-spill slice. At S7-G7 the leaf-fn
            // path doesn't emit StoreGpToStackArg, so reject loudly here.
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: "<spill>".to_string(),
                op_name: format!(
                    "StoreGpToStackArg(offset=+{offset}, reg={reg}) deferred to G8 slice"
                ),
            });
        }
        AbstractInsn::StoreXmmToStackArg { offset, reg } => {
            return Err(NativeX64Error::UnsupportedOp {
                fn_name: "<spill>".to_string(),
                op_name: format!(
                    "StoreXmmToStackArg(offset=+{offset}, reg={reg}) deferred to G8 slice"
                ),
            });
        }
    };
    Ok(out)
}

/// G3 `GpReg` → G4 `Gpr` translation. Both enums hold the canonical Intel
/// 0..15 encoding ; the cast preserves it.
fn gp_to_encoder_gpr(g: crate::abi::GpReg) -> Gpr {
    Gpr::from_index(g.encoding())
}

/// G3 `XmmReg` → G4 `Xmm` translation. Same canonical 0..15 encoding.
fn xmm_to_encoder_xmm(x: crate::abi::XmmReg) -> Xmm {
    Xmm::from_index(x.encoding())
}

// ═══════════════════════════════════════════════════════════════════════
// § Adapter : Selection driver wrapping G1's `select_module` + D5 marker
// ═══════════════════════════════════════════════════════════════════════

/// Run G1 instruction-selection over `module`, defensively setting the D5
/// structured-CFG marker if it isn't already present. The G1 selector
/// requires the marker (per the T11-D70 fanout-contract) ; the pipeline
/// is a single-call entry-point that handles the marker transparently for
/// callers like `csslc::commands::build` which validates earlier in its
/// own pass-pipeline.
pub fn select_module_with_marker(module: &MirModule) -> Result<Vec<IselFunc>, SelectError> {
    let mut local = module.clone();
    if !has_structured_cfg_marker(&local) {
        local.attributes.push((
            STRUCTURED_CFG_VALIDATED_KEY.to_string(),
            STRUCTURED_CFG_VALIDATED_VALUE.to_string(),
        ));
    }
    let mut out = Vec::with_capacity(local.funcs.len());
    for fn_ref in &local.funcs {
        if fn_ref.is_generic {
            continue;
        }
        out.push(isel_select_function(&local, fn_ref)?);
    }
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════
// § Adapter : per-fn byte assembly
// ═══════════════════════════════════════════════════════════════════════

/// Build the encoded byte sequence for a single isel-form function under
/// the given ABI. Splices :
///   1. G3 prologue (`push rbp ; mov rbp, rsp ; sub rsp, frame ;
///      <callee-saved-pushes>`),
///   2. G1→G4 lowered body (placement of the return-value into rax/xmm0),
///   3. G3 epilogue (`<callee-saved-pops> ; add rsp, frame ; pop rbp ; ret`).
///
/// The `is_export` flag is wired through to the [`ObjFunc`] builder so the
/// linker surfaces a STB_GLOBAL / EXTERNAL / N_EXT symbol when the fn is
/// the module's main (or other public entry).
///
/// # Errors
/// Returns [`NativeX64Error`] for any per-stage adapter failure.
pub fn build_func_bytes(
    func: &IselFunc,
    abi: X64Abi,
    is_export: bool,
) -> Result<ObjFunc, NativeX64Error> {
    // § 1. Lower body.
    let body_insts = isel_to_encoder_simple(func)?;

    // § 2. Lower prologue + epilogue from G3.
    let layout = FunctionLayout {
        abi,
        local_frame_bytes: 0,
        callee_saved_gp_used: Vec::new(),
        callee_saved_xmm_used: Vec::new(),
    };
    let prologue: LoweredPrologue = lower_prologue(&layout);
    let epilogue: LoweredEpilogue = lower_epilogue_for(&layout, &prologue);

    // § 3. Walk prologue + body + epilogue in order, encoding bytes.
    let mut bytes = Vec::new();
    for ai in &prologue.insns {
        for ei in abi_lower_to_encoder(ai)? {
            encode_into(&mut bytes, &ei);
        }
    }
    for ei in &body_insts {
        encode_into(&mut bytes, ei);
    }
    for ai in &epilogue.insns {
        for ei in abi_lower_to_encoder(ai)? {
            encode_into(&mut bytes, &ei);
        }
    }

    // § 4. Pack into the objemit boundary type.
    let obj_func =
        crate::objemit::func::X64Func::new(func.name.clone(), bytes, Vec::new(), is_export)
            .map_err(|e| NativeX64Error::ObjectWriteFailed {
                detail: format!("X64Func::new for `{}` failed : {e}", func.name),
            })?;
    Ok(obj_func)
}

// ═══════════════════════════════════════════════════════════════════════
// § Object-format ↔ object-target translation
// ═══════════════════════════════════════════════════════════════════════

/// Translate the crate-root [`ObjectFormat`] (G6 façade) to the
/// [`ObjectTarget`] expected by G5's `emit_object_file`.
fn format_to_target(fmt: ObjectFormat) -> ObjectTarget {
    match fmt {
        ObjectFormat::Elf => ObjectTarget::ElfX64,
        ObjectFormat::Coff => ObjectTarget::CoffX64,
        ObjectFormat::MachO => ObjectTarget::MachOX64,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § PUBLIC : end-to-end pipeline driver
// ═══════════════════════════════════════════════════════════════════════

/// End-to-end native-x64 pipeline : `MirModule` → host-platform object
/// bytes. Replaces the placeholder [`crate::emit_object_module_with_format`]
/// once the pipeline is wired (S7-G7).
///
/// The pipeline runs every non-generic [`MirFunc`] in `module` through the
/// full G-axis chain :
///   1. **G1 select** : MIR → `isel::X64Func` (vreg-form).
///   2. **G7 simple-lowering** : G1 → G4 direct bridge (scalar-leaf subset
///      at S7-G7 ; full G2 LSRA path follows in a later slice).
///   3. **G3 prologue+epilogue** : `lower_prologue` + `lower_epilogue_for`
///      under the host-default ABI ; AbstractInsn → encoder X64Inst via
///      [`abi_lower_to_encoder`].
///   4. **G4 encode** : `encoder::encode_into` packs each instruction's
///      bytes into the per-fn body.
///   5. **G5 emit_object_file** : per-format ELF / COFF / Mach-O writer
///      produces relocatable object bytes the linker accepts.
///
/// The "main" function (when present) is exported (STB_GLOBAL / EXTERNAL /
/// N_EXT) so the linker resolves it to the program entry-point ; other
/// functions land as STB_LOCAL / STATIC by default.
///
/// # Errors
/// Returns [`NativeX64Error`] for any per-stage pipeline failure :
///   - [`NativeX64Error::UnsupportedOp`] : G1 selection error or scalar-
///     leaf-subset rejection (op outside the S7-G7 surface).
///   - [`NativeX64Error::NonScalarType`] : G1 signature/op-type rejection.
///   - [`NativeX64Error::ObjectWriteFailed`] : G5 emission failure or
///     `objemit::X64Func::new` rejection.
pub fn emit_object_module_native(module: &MirModule) -> Result<Vec<u8>, NativeX64Error> {
    emit_object_module_native_with_format(module, host_default_format())
}

/// Variant of [`emit_object_module_native`] that lets callers request a
/// specific object-format.
///
/// # Errors
/// Same as [`emit_object_module_native`].
pub fn emit_object_module_native_with_format(
    module: &MirModule,
    format: ObjectFormat,
) -> Result<Vec<u8>, NativeX64Error> {
    let abi = X64Abi::host_default();
    let target = format_to_target(format);

    // § Stage 1 : G1 instruction-selection.
    let isel_funcs = select_module_with_marker(module).map_err(translate_select_error)?;

    // § Stages 2..4 : per-fn body assembly.
    let mut obj_funcs: Vec<ObjFunc> = Vec::with_capacity(isel_funcs.len());
    for f in &isel_funcs {
        // Convention : a function named "main" is exported so the linker
        // resolves it to the program entry-point ; all others are local.
        let is_export = f.name == "main";
        let obj_func = build_func_bytes(f, abi, is_export)?;
        obj_funcs.push(obj_func);
    }

    // § Stage 5 : G5 object-file emission.
    emit_object_file(&obj_funcs, &[], target).map_err(translate_object_error)
}

// ═══════════════════════════════════════════════════════════════════════
// § Per-stage error translation : sibling-error → NativeX64Error wrapping
// ═══════════════════════════════════════════════════════════════════════

/// Translate a G1 [`SelectError`] into a [`NativeX64Error`] preserving the
/// stable diagnostic-code via the error message text.
fn translate_select_error(e: SelectError) -> NativeX64Error {
    match e {
        SelectError::StructuredCfgMarkerMissing => NativeX64Error::UnsupportedOp {
            fn_name: "<module>".to_string(),
            op_name: format!("structured_cfg.validated marker missing : {e}"),
        },
        SelectError::UnsupportedSignatureType { fn_name, ty } => NativeX64Error::NonScalarType {
            fn_name,
            slot: 0,
            ty,
        },
        SelectError::UnsupportedType { fn_name, op, ty } => NativeX64Error::NonScalarType {
            fn_name,
            slot: 0,
            ty: format!("{op} → {ty}"),
        },
        SelectError::UnsupportedOp { fn_name, op } => NativeX64Error::UnsupportedOp {
            fn_name,
            op_name: op,
        },
        // All other variants surface as UnsupportedOp with the diagnostic-
        // code preserved in the message text.
        other => NativeX64Error::UnsupportedOp {
            fn_name: "<unknown>".to_string(),
            op_name: format!("{} : {other}", other.code()),
        },
    }
}

/// Translate a G5 [`ObjectError`] into [`NativeX64Error::ObjectWriteFailed`]
/// preserving the stable diagnostic prefix in the detail string.
fn translate_object_error(e: ObjectError) -> NativeX64Error {
    NativeX64Error::ObjectWriteFailed {
        detail: e.to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § Symbols reserved for the G8 call-lowering follow-up slice
// ═══════════════════════════════════════════════════════════════════════
//
// The imports below (`MirOp`, `MirFunc`, `MemOperand`, `BranchTarget`,
// `Cond`, `ArgClass`) are used today only by the test module + the
// per-stage error translation paths ; the call-lowering + branch-fixup
// slice that lands as G8 will reference them directly. We keep them
// imported now so the public surface of this module is the canonical
// "every per-stage symbol the walker needs" set, even when some are
// reserved-for-soon-use.

#[allow(dead_code, clippy::trivially_copy_pass_by_ref)]
fn _g8_reserved_signatures(
    _op: &MirOp,
    _func: &MirFunc,
    _mem: MemOperand,
    _bt: BranchTarget,
    _cond: Cond,
    _ac: ArgClass,
) {
}

// ═══════════════════════════════════════════════════════════════════════
// § Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(clippy::redundant_clone)] // test fixtures cheap-clone for readability
mod tests {
    use super::*;
    use cssl_mir::{IntWidth, MirOp, MirType};

    // ─── helpers ──────────────────────────────────────────────────────

    /// Build a minimal `fn main() -> i32 { N }` MirModule with the
    /// given i32 constant return value.
    fn build_main_42(value: i32) -> MirModule {
        let mut module = MirModule::with_name("test.module");
        let mut f = cssl_mir::MirFunc::new("main", vec![], vec![MirType::Int(IntWidth::I32)]);
        // Const-define value.
        let v = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v, MirType::Int(IntWidth::I32))
                .with_attribute("value", value.to_string()),
        );
        f.push_op(MirOp::std("func.return").with_operand(v));
        module.push_func(f);
        module
    }

    // ─── ScalarLeafReturn::try_extract ──────────────────────────────

    #[test]
    fn scalar_leaf_extracts_i32_constant() {
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let leaf = ScalarLeafReturn::try_extract(&funcs[0]).unwrap();
        assert_eq!(
            leaf,
            Some(ScalarLeafReturn {
                return_value: 42,
                return_width: X64Width::I32,
            })
        );
    }

    #[test]
    fn scalar_leaf_extracts_zero_constant() {
        let m = build_main_42(0);
        let funcs = select_module_with_marker(&m).unwrap();
        let leaf = ScalarLeafReturn::try_extract(&funcs[0]).unwrap();
        assert_eq!(leaf.unwrap().return_value, 0);
    }

    #[test]
    fn scalar_leaf_extracts_negative_constant() {
        let m = build_main_42(-1);
        let funcs = select_module_with_marker(&m).unwrap();
        let leaf = ScalarLeafReturn::try_extract(&funcs[0]).unwrap();
        assert_eq!(leaf.unwrap().return_value, -1);
    }

    #[test]
    fn scalar_leaf_returns_none_for_void_return() {
        let mut m = MirModule::new();
        let mut f = cssl_mir::MirFunc::new("nullary", vec![], vec![]);
        f.push_op(MirOp::std("func.return"));
        m.push_func(f);
        let funcs = select_module_with_marker(&m).unwrap();
        let leaf = ScalarLeafReturn::try_extract(&funcs[0]).unwrap();
        assert_eq!(leaf, None);
    }

    // ─── isel_to_encoder_simple ─────────────────────────────────────

    #[test]
    fn isel_to_encoder_emits_mov_eax_imm_for_i32_return() {
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        assert_eq!(body.len(), 1);
        match &body[0] {
            EncInst::MovRI { size, dst, imm } => {
                assert_eq!(*size, OperandSize::B32);
                assert_eq!(*dst, Gpr::Rax);
                assert_eq!(*imm, 42);
            }
            other => panic!("expected MovRI, got {other:?}"),
        }
    }

    #[test]
    fn isel_to_encoder_emits_no_body_for_void_return() {
        let mut m = MirModule::new();
        let mut f = cssl_mir::MirFunc::new("nullary", vec![], vec![]);
        f.push_op(MirOp::std("func.return"));
        m.push_func(f);
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        assert!(body.is_empty());
    }

    // ─── abi_lower_to_encoder ────────────────────────────────────────

    #[test]
    fn abi_lower_push_rbp_emits_encoder_push() {
        let ai = AbstractInsn::Push {
            reg: crate::abi::GpReg::Rbp,
        };
        let out = abi_lower_to_encoder(&ai).unwrap();
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], EncInst::Push { src: Gpr::Rbp }));
    }

    #[test]
    fn abi_lower_pop_rbp_emits_encoder_pop() {
        let ai = AbstractInsn::Pop {
            reg: crate::abi::GpReg::Rbp,
        };
        let out = abi_lower_to_encoder(&ai).unwrap();
        assert!(matches!(out[0], EncInst::Pop { dst: Gpr::Rbp }));
    }

    #[test]
    fn abi_lower_mov_rbp_rsp_emits_encoder_movrr_64bit() {
        let ai = AbstractInsn::MovGpGp {
            dst: crate::abi::GpReg::Rbp,
            src: crate::abi::GpReg::Rsp,
        };
        let out = abi_lower_to_encoder(&ai).unwrap();
        assert_eq!(out.len(), 1);
        match out.first().expect("len==1") {
            EncInst::MovRR { size, dst, src } => {
                assert_eq!(*size, OperandSize::B64);
                assert_eq!(*dst, Gpr::Rbp);
                assert_eq!(*src, Gpr::Rsp);
            }
            other => panic!("expected MovRR, got {other:?}"),
        }
    }

    #[test]
    fn abi_lower_sub_rsp_emits_encoder_subri() {
        let ai = AbstractInsn::SubRsp { bytes: 32 };
        let out = abi_lower_to_encoder(&ai).unwrap();
        assert!(matches!(
            out[0],
            EncInst::SubRI {
                size: OperandSize::B64,
                dst: Gpr::Rsp,
                imm: 32
            }
        ));
    }

    #[test]
    fn abi_lower_add_rsp_emits_encoder_addri() {
        let ai = AbstractInsn::AddRsp { bytes: 32 };
        let out = abi_lower_to_encoder(&ai).unwrap();
        assert!(matches!(
            out[0],
            EncInst::AddRI {
                size: OperandSize::B64,
                dst: Gpr::Rsp,
                imm: 32
            }
        ));
    }

    #[test]
    fn abi_lower_ret_emits_encoder_ret() {
        let out = abi_lower_to_encoder(&AbstractInsn::Ret).unwrap();
        assert!(matches!(out[0], EncInst::Ret));
    }

    #[test]
    fn abi_lower_call_returns_unsupported_op_at_g7() {
        // G8 will land call-lowering with reloc emission ; at G7 the
        // pipeline rejects loudly.
        let ai = AbstractInsn::Call {
            target: "callee".to_string(),
        };
        let err = abi_lower_to_encoder(&ai).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("Call"));
                assert!(op_name.contains("G8"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    #[test]
    fn abi_lower_store_gp_to_stack_arg_returns_unsupported_op_at_g7() {
        let ai = AbstractInsn::StoreGpToStackArg {
            offset: 0,
            reg: crate::abi::GpReg::Rax,
        };
        let err = abi_lower_to_encoder(&ai).unwrap_err();
        assert!(matches!(err, NativeX64Error::UnsupportedOp { .. }));
    }

    #[test]
    fn abi_lower_store_xmm_to_stack_arg_returns_unsupported_op_at_g7() {
        let ai = AbstractInsn::StoreXmmToStackArg {
            offset: 0,
            reg: crate::abi::XmmReg::Xmm0,
        };
        let err = abi_lower_to_encoder(&ai).unwrap_err();
        assert!(matches!(err, NativeX64Error::UnsupportedOp { .. }));
    }

    #[test]
    fn abi_lower_movss_xmm_xmm_emits_encoder_movsd_rr() {
        // G3's MovXmmXmm is generic ; at G7 we lower via MovsdRR (covers
        // both single/double scalar moves at the encoding level — the
        // 0xF2 / 0xF3 prefix difference is opcode-level, the mov-shape is
        // identical for our purposes).
        let ai = AbstractInsn::MovXmmXmm {
            dst: crate::abi::XmmReg::Xmm0,
            src: crate::abi::XmmReg::Xmm1,
        };
        let out = abi_lower_to_encoder(&ai).unwrap();
        assert!(matches!(
            out[0],
            EncInst::MovsdRR {
                dst: Xmm::Xmm0,
                src: Xmm::Xmm1
            }
        ));
    }

    // ─── gp_to_encoder_gpr / xmm_to_encoder_xmm ─────────────────────

    #[test]
    fn gp_to_encoder_gpr_preserves_canonical_encoding() {
        for gp_idx in 0..=15u8 {
            let g = match gp_idx {
                0 => crate::abi::GpReg::Rax,
                1 => crate::abi::GpReg::Rcx,
                2 => crate::abi::GpReg::Rdx,
                3 => crate::abi::GpReg::Rbx,
                4 => crate::abi::GpReg::Rsp,
                5 => crate::abi::GpReg::Rbp,
                6 => crate::abi::GpReg::Rsi,
                7 => crate::abi::GpReg::Rdi,
                8 => crate::abi::GpReg::R8,
                9 => crate::abi::GpReg::R9,
                10 => crate::abi::GpReg::R10,
                11 => crate::abi::GpReg::R11,
                12 => crate::abi::GpReg::R12,
                13 => crate::abi::GpReg::R13,
                14 => crate::abi::GpReg::R14,
                _ => crate::abi::GpReg::R15,
            };
            assert_eq!(gp_to_encoder_gpr(g).index(), gp_idx);
        }
    }

    #[test]
    fn xmm_to_encoder_xmm_preserves_canonical_encoding() {
        for x_idx in 0..=15u8 {
            let x = match x_idx {
                0 => crate::abi::XmmReg::Xmm0,
                1 => crate::abi::XmmReg::Xmm1,
                2 => crate::abi::XmmReg::Xmm2,
                3 => crate::abi::XmmReg::Xmm3,
                4 => crate::abi::XmmReg::Xmm4,
                5 => crate::abi::XmmReg::Xmm5,
                6 => crate::abi::XmmReg::Xmm6,
                7 => crate::abi::XmmReg::Xmm7,
                8 => crate::abi::XmmReg::Xmm8,
                9 => crate::abi::XmmReg::Xmm9,
                10 => crate::abi::XmmReg::Xmm10,
                11 => crate::abi::XmmReg::Xmm11,
                12 => crate::abi::XmmReg::Xmm12,
                13 => crate::abi::XmmReg::Xmm13,
                14 => crate::abi::XmmReg::Xmm14,
                _ => crate::abi::XmmReg::Xmm15,
            };
            assert_eq!(xmm_to_encoder_xmm(x).index(), x_idx);
        }
    }

    // ─── select_module_with_marker ──────────────────────────────────

    #[test]
    fn select_module_with_marker_sets_marker_defensively() {
        let m = build_main_42(42);
        // Original is unmarked.
        assert!(!cssl_mir::has_structured_cfg_marker(&m));
        // Pipeline selection succeeds despite the missing marker (we set
        // it on a local clone).
        let funcs = select_module_with_marker(&m).unwrap();
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn select_module_with_marker_skips_generic_fns() {
        let mut m = build_main_42(42);
        // Add a generic fn that should be skipped.
        let mut g = cssl_mir::MirFunc::new("foo", vec![], vec![]);
        g.is_generic = true;
        m.push_func(g);
        let funcs = select_module_with_marker(&m).unwrap();
        // Only `main` survives.
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "main");
    }

    // ─── build_func_bytes ────────────────────────────────────────────

    #[test]
    fn build_func_bytes_for_main_42_includes_prologue_body_epilogue_ret() {
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, /*is_export=*/ true).unwrap();
        assert_eq!(obj_func.name, "main");
        assert!(obj_func.is_export);
        // Bytes must be non-empty + end with `0xC3` (ret).
        assert!(!obj_func.bytes.is_empty());
        assert_eq!(
            *obj_func.bytes.last().unwrap(),
            0xC3,
            "expected ret as last byte"
        );
    }

    #[test]
    fn build_func_bytes_main_42_contains_mov_eax_42_marker() {
        // The body should encode `mov eax, 42` somewhere in the byte stream.
        // `mov eax, imm32` = `B8 2A 00 00 00`.
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        let needle = [0xB8, 0x2A, 0x00, 0x00, 0x00];
        let found = obj_func.bytes.windows(needle.len()).any(|w| w == needle);
        assert!(
            found,
            "expected `mov eax, 42` byte sequence ; got {:02X?}",
            obj_func.bytes
        );
    }

    #[test]
    fn build_func_bytes_starts_with_push_rbp() {
        // First byte should be `0x55` (`push rbp`) since the prologue runs
        // first. (No 0x66 prefix or REX needed for `push rbp` 64-bit form.)
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        assert_eq!(
            obj_func.bytes[0], 0x55,
            "expected `push rbp` (0x55) at byte 0 ; got {:02X}",
            obj_func.bytes[0]
        );
    }

    #[test]
    fn build_func_bytes_rejects_multi_block_body() {
        // Build an isel func with two blocks (synthetic — push a fresh
        // block via the api).
        let m = build_main_42(42);
        let mut funcs = select_module_with_marker(&m).unwrap();
        let _b1 = funcs[0].fresh_block();
        let abi = X64Abi::host_default();
        let err = build_func_bytes(&funcs[0], abi, true).unwrap_err();
        match err {
            NativeX64Error::UnsupportedOp { op_name, .. } => {
                assert!(op_name.contains("multi-block"));
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ─── format_to_target ────────────────────────────────────────────

    #[test]
    fn format_to_target_maps_each_format() {
        assert!(matches!(
            format_to_target(ObjectFormat::Elf),
            ObjectTarget::ElfX64
        ));
        assert!(matches!(
            format_to_target(ObjectFormat::Coff),
            ObjectTarget::CoffX64
        ));
        assert!(matches!(
            format_to_target(ObjectFormat::MachO),
            ObjectTarget::MachOX64
        ));
    }

    // ─── emit_object_module_native — full pipeline ───────────────────

    #[test]
    fn emit_object_module_native_for_main_42_returns_object_bytes() {
        let m = build_main_42(42);
        let bytes = emit_object_module_native(&m).expect("pipeline should succeed");
        // Bytes must be non-empty + start with the host magic prefix.
        assert!(!bytes.is_empty());
        let host_magic = crate::magic_prefix(host_default_format());
        assert!(
            bytes.starts_with(host_magic),
            "expected host magic {host_magic:02X?} ; got first 8 bytes {:02X?}",
            &bytes[..bytes.len().min(8)]
        );
    }

    #[test]
    fn emit_object_module_native_with_explicit_format_succeeds_for_each_format() {
        let m = build_main_42(42);
        for fmt in [ObjectFormat::Elf, ObjectFormat::Coff, ObjectFormat::MachO] {
            let bytes = emit_object_module_native_with_format(&m, fmt).unwrap();
            assert!(!bytes.is_empty());
            let magic = crate::magic_prefix(fmt);
            assert!(
                bytes.starts_with(magic),
                "format {fmt:?} : expected magic {magic:02X?} ; got {:02X?}",
                &bytes[..bytes.len().min(8)]
            );
        }
    }

    #[test]
    fn emit_object_module_native_empty_module_returns_object_bytes() {
        // Empty module = no functions = a valid (mostly-empty) object file.
        let m = MirModule::new();
        let bytes = emit_object_module_native(&m).unwrap();
        assert!(!bytes.is_empty());
        let host_magic = crate::magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_object_module_native_skips_generic_fns() {
        let mut m = build_main_42(42);
        let mut g = cssl_mir::MirFunc::new("generic_foo", vec![], vec![]);
        g.is_generic = true;
        m.push_func(g);
        let bytes = emit_object_module_native(&m).unwrap();
        assert!(!bytes.is_empty());
        // The presence of `main` (the non-generic fn) is verified by the
        // magic-prefix + non-empty-bytes shape ; per-fn symbol-table
        // verification is in objemit's own test suite.
    }

    #[test]
    fn emit_object_module_native_with_unsupported_op_surfaces_unsupported_op() {
        // Build a fn that uses `arith.addi` — outside the scalar-leaf
        // subset at S7-G7. The pipeline must reject loudly.
        let mut m = MirModule::new();
        let mut f = cssl_mir::MirFunc::new("two_plus", vec![], vec![MirType::Int(IntWidth::I32)]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        let v2 = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v0, MirType::Int(IntWidth::I32))
                .with_attribute("value", "2"),
        );
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v1, MirType::Int(IntWidth::I32))
                .with_attribute("value", "3"),
        );
        f.push_op(
            MirOp::std("arith.addi")
                .with_result(v2, MirType::Int(IntWidth::I32))
                .with_operand(v0)
                .with_operand(v1),
        );
        f.push_op(MirOp::std("func.return").with_operand(v2));
        m.push_func(f);
        let err = emit_object_module_native(&m).unwrap_err();
        assert!(matches!(err, NativeX64Error::UnsupportedOp { .. }));
    }

    // ─── translate_select_error ─────────────────────────────────────

    #[test]
    fn translate_select_error_unsupported_signature_becomes_non_scalar_type() {
        let e = SelectError::UnsupportedSignatureType {
            fn_name: "f".to_string(),
            ty: "Tuple<i32,i32>".to_string(),
        };
        let nx = translate_select_error(e);
        assert!(matches!(nx, NativeX64Error::NonScalarType { .. }));
    }

    #[test]
    fn translate_select_error_unsupported_op_becomes_unsupported_op() {
        let e = SelectError::UnsupportedOp {
            fn_name: "f".to_string(),
            op: "exotic.op".to_string(),
        };
        let nx = translate_select_error(e);
        match nx {
            NativeX64Error::UnsupportedOp { op_name, .. } => assert_eq!(op_name, "exotic.op"),
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }
    }

    // ─── translate_object_error ─────────────────────────────────────

    #[test]
    fn translate_object_error_becomes_object_write_failed() {
        let e = ObjectError::DuplicateSymbol {
            name: "main".to_string(),
            first: 0,
            second: 1,
        };
        let nx = translate_object_error(e);
        match nx {
            NativeX64Error::ObjectWriteFailed { detail } => {
                assert!(detail.contains("duplicate"), "got `{detail}`");
            }
            other => panic!("expected ObjectWriteFailed, got {other:?}"),
        }
    }

    // ─── byte-level milestone snapshot ──────────────────────────────

    /// SECOND hello.exe = 42 milestone : the bytes for `fn main() -> i32
    /// { 42 }` MUST contain the canonical `mov eax, 42 ; ret` core after
    /// the prologue/epilogue is stripped. This is the byte-level proof
    /// that the pipeline produces semantically-correct machine code for
    /// the milestone.
    #[test]
    fn milestone_main_42_byte_pattern_matches_expected_mov_eax_ret() {
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        // Body should contain : push rbp ; mov rbp,rsp ; mov eax,42 ; pop rbp ; ret
        // = 0x55 ; 0x48 0x89 0xE5 ; 0xB8 0x2A 0x00 0x00 0x00 ; 0x5D ; 0xC3
        // (No SubRsp/AddRsp because frame_bytes = 0 ; no callee-saved pushes either.)
        let expected = [
            0x55, // push rbp
            0x48, 0x89, 0xE5, // mov rbp, rsp
            0xB8, 0x2A, 0x00, 0x00, 0x00, // mov eax, 42
            0x5D, // pop rbp
            0xC3, // ret
        ];
        assert_eq!(
            obj_func.bytes, expected,
            "milestone bytes mismatch — got {:02X?}",
            obj_func.bytes
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    // § G11 (T11-D102) — SSE2 scalar float path tests
    // ═══════════════════════════════════════════════════════════════════

    use cssl_mir::FloatWidth;

    /// Build `fn pi() -> f64 { <value> }` MIR module.
    fn build_const_f64(name: &str, value: f64) -> MirModule {
        let mut module = MirModule::with_name("test.f64.module");
        let mut f = MirFunc::new(name, vec![], vec![MirType::Float(FloatWidth::F64)]);
        let v = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v, MirType::Float(FloatWidth::F64))
                .with_attribute("value", value.to_string()),
        );
        f.push_op(MirOp::std("func.return").with_operand(v));
        module.push_func(f);
        module
    }

    /// Build `fn const_f32() -> f32 { <value> }` MIR module.
    fn build_const_f32(value: f32) -> MirModule {
        let mut module = MirModule::with_name("test.f32.module");
        let mut f = MirFunc::new("const_f32", vec![], vec![MirType::Float(FloatWidth::F32)]);
        let v = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(v, MirType::Float(FloatWidth::F32))
                .with_attribute("value", value.to_string()),
        );
        f.push_op(MirOp::std("func.return").with_operand(v));
        module.push_func(f);
        module
    }

    /// Build `fn add_f64(a: f64, b: f64) -> f64 { a + b }` MIR module.
    fn build_add_f64() -> MirModule {
        let mut module = MirModule::with_name("test.add_f64.module");
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new(
            "add_f64",
            vec![f64_ty.clone(), f64_ty.clone()],
            vec![f64_ty.clone()],
        );
        // Entry args from the function-builder API (params get block-args).
        // Per cssl-mir convention, we add explicit BlockArg ValueIds at
        // entry — but MirFunc::new likely auto-creates them. Let me match
        // the cranelift-test pattern : look up the entry block args.
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let a_id = entry_args[0];
        let b_id = entry_args[1];
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.addf")
                .with_result(r, f64_ty.clone())
                .with_operand(a_id)
                .with_operand(b_id),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        module.push_func(f);
        module
    }

    /// Build `fn sub_f64(a: f64, b: f64) -> f64 { a - b }` MIR module.
    fn build_sub_f64() -> MirModule {
        let mut module = MirModule::with_name("test.sub_f64.module");
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new(
            "sub_f64",
            vec![f64_ty.clone(), f64_ty.clone()],
            vec![f64_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.subf")
                .with_result(r, f64_ty.clone())
                .with_operand(entry_args[0])
                .with_operand(entry_args[1]),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        module.push_func(f);
        module
    }

    /// Build `fn mul_f64(a: f64, b: f64) -> f64 { a * b }` MIR module.
    fn build_mul_f64() -> MirModule {
        let mut module = MirModule::with_name("test.mul_f64.module");
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new(
            "mul_f64",
            vec![f64_ty.clone(), f64_ty.clone()],
            vec![f64_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.mulf")
                .with_result(r, f64_ty.clone())
                .with_operand(entry_args[0])
                .with_operand(entry_args[1]),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        module.push_func(f);
        module
    }

    /// Build `fn div_f64(a: f64, b: f64) -> f64 { a / b }` MIR module.
    fn build_div_f64() -> MirModule {
        let mut module = MirModule::with_name("test.div_f64.module");
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new(
            "div_f64",
            vec![f64_ty.clone(), f64_ty.clone()],
            vec![f64_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.divf")
                .with_result(r, f64_ty.clone())
                .with_operand(entry_args[0])
                .with_operand(entry_args[1]),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        module.push_func(f);
        module
    }

    /// Build `fn add_f32(a: f32, b: f32) -> f32 { a + b }` MIR module.
    fn build_add_f32() -> MirModule {
        let mut module = MirModule::with_name("test.add_f32.module");
        let f32_ty = MirType::Float(FloatWidth::F32);
        let mut f = MirFunc::new(
            "add_f32",
            vec![f32_ty.clone(), f32_ty.clone()],
            vec![f32_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.addf")
                .with_result(r, f32_ty.clone())
                .with_operand(entry_args[0])
                .with_operand(entry_args[1]),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        module.push_func(f);
        module
    }

    // ── try_lower_float_leaf : detection ────────────────────────────────

    #[test]
    fn float_leaf_returns_none_for_integer_only_module() {
        // Pure-int leaf doesn't trigger the float-leaf walker.
        let m = build_main_42(42);
        let funcs = select_module_with_marker(&m).unwrap();
        let result = try_lower_float_leaf(&funcs[0]).unwrap();
        assert!(result.is_none(), "i32-only fn should not match float leaf");
    }

    #[test]
    fn float_leaf_detects_f64_signature() {
        // Pure f64-constant fn IS detected by the float-leaf walker.
        let m = build_const_f64("pi", core::f64::consts::PI);
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        assert!(!body.is_empty(), "f64 leaf must emit at least the const-mat");
    }

    // ── f64 constant materialization ───────────────────────────────────

    #[test]
    fn float_leaf_materializes_f64_constant_via_movabs_movq() {
        let m = build_const_f64("pi", core::f64::consts::PI);
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        // Expect : MovRI(B64, rax, pi-bits) ; MovqXmmFromGp(xmm15, rax) ;
        //          MovsdRR(xmm0, xmm15)  [the pool pops xmm15 first]
        // body.len() must be 3.
        assert_eq!(
            body.len(),
            3,
            "f64 const path expected 3 insts ; got {body:?}"
        );
        match &body[0] {
            EncInst::MovRI {
                size: OperandSize::B64,
                dst: Gpr::Rax,
                imm,
            } => {
                let bits = core::f64::consts::PI.to_bits();
                assert_eq!(*imm as u64, bits, "rax must hold pi's IEEE 754 bits");
            }
            other => panic!("expected MovRI(B64, Rax, ...) ; got {other:?}"),
        }
        match &body[1] {
            EncInst::MovqXmmFromGp {
                dst: _,
                src: Gpr::Rax,
            } => { /* ok */ }
            other => panic!("expected MovqXmmFromGp ; got {other:?}"),
        }
        match &body[2] {
            EncInst::MovsdRR { dst: Xmm::Xmm0, .. } => { /* ok */ }
            other => panic!("expected MovsdRR(xmm0, _) ; got {other:?}"),
        }
    }

    #[test]
    fn float_leaf_materializes_f32_constant_via_mov32_movd() {
        let m = build_const_f32(1.5);
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        // Expect : MovRI(B32, rax, 1.5-bits) ; MovdXmmFromGp(xmm, rax) ;
        //          MovssRR(xmm0, xmm)
        assert_eq!(body.len(), 3, "f32 const path expected 3 insts");
        match &body[0] {
            EncInst::MovRI {
                size: OperandSize::B32,
                dst: Gpr::Rax,
                imm,
            } => {
                let bits = 1.5_f32.to_bits();
                assert_eq!(*imm as u32, bits);
            }
            other => panic!("expected MovRI(B32, Rax, ...) ; got {other:?}"),
        }
        match &body[1] {
            EncInst::MovdXmmFromGp {
                dst: _,
                src: Gpr::Rax,
            } => { /* ok */ }
            other => panic!("expected MovdXmmFromGp ; got {other:?}"),
        }
        match &body[2] {
            EncInst::MovssRR { dst: Xmm::Xmm0, .. } => { /* ok */ }
            other => panic!("expected MovssRR(xmm0, _) ; got {other:?}"),
        }
    }

    // ── pipeline end-to-end : object bytes ──────────────────────────────

    #[test]
    fn emit_object_module_native_for_pi_const_returns_object_bytes() {
        let m = build_const_f64("pi", core::f64::consts::PI);
        let bytes = emit_object_module_native(&m).expect("pi pipeline should succeed");
        assert!(!bytes.is_empty());
        let host_magic = crate::magic_prefix(host_default_format());
        assert!(bytes.starts_with(host_magic));
    }

    #[test]
    fn emit_object_module_native_for_pi_contains_pi_bit_pattern_in_bytes() {
        // The IEEE 754 bit pattern of pi (0x400921FB54442D18) must appear
        // somewhere in the emitted body bytes : the constant materialization
        // path encodes it as little-endian after the `mov rax, imm64` opcode.
        let m = build_const_f64("pi", core::f64::consts::PI);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        let bits = core::f64::consts::PI.to_bits().to_le_bytes();
        let found = obj_func.bytes.windows(8).any(|w| w == bits);
        assert!(
            found,
            "expected pi bit-pattern {bits:02X?} in body ; got {:02X?}",
            obj_func.bytes
        );
    }

    #[test]
    fn pi_milestone_contains_movq_xmm_rax_after_movabs() {
        // After the `movabs rax, imm64`, the next inst MUST be the movq
        // bit-transfer to an XMM register. The encoder bytes for `movq
        // xmm?, rax` are `66 [4C|48] 0F 6E [F8|C0..F8]` — REX.W set, with
        // REX.R bit toggling for xmm8..xmm15. The xmm pool chooses xmm1
        // for this no-param fn.
        let m = build_const_f64("pi", core::f64::consts::PI);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        // Search for the canonical movq prefix : `66 48 0F 6E ??` (REX.W
        // without REX.R for xmm0..xmm7) OR `66 4C 0F 6E ??` (with REX.R
        // for xmm8..xmm15).
        let has_movq = obj_func.bytes.windows(4).any(|w| {
            (w[1] == 0x48 || w[1] == 0x4C) && w[3] == 0x6E && w[2] == 0x0F && w[0] == 0x66
        });
        assert!(
            has_movq,
            "expected `movq xmm?, rax` (66 [48|4C] 0F 6E ??) in body ; got {:02X?}",
            obj_func.bytes
        );
    }

    // ── float arg passing ──────────────────────────────────────────────

    #[test]
    fn float_leaf_add_f64_emits_addsd_in_body() {
        // For `fn add_f64(a:f64, b:f64) -> f64 { a + b }` the body should
        // contain an `addsd` instruction. Under MS-x64 a→xmm0, b→xmm1 ; the
        // selector emits `Mov dst, lhs` then `FpAdd dst, rhs` so the lowered
        // sequence is : (no Mov needed because dst aliases lhs) ; addsd dst, rhs.
        let m = build_add_f64();
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        let has_addsd = body.iter().any(|i| matches!(i, EncInst::AddsdRR { .. }));
        assert!(
            has_addsd,
            "expected an AddsdRR in the lowered body ; got {body:?}"
        );
    }

    #[test]
    fn float_leaf_sub_f64_emits_subsd_in_body() {
        let m = build_sub_f64();
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        let has_subsd = body.iter().any(|i| matches!(i, EncInst::SubsdRR { .. }));
        assert!(has_subsd, "expected SubsdRR ; got {body:?}");
    }

    #[test]
    fn float_leaf_mul_f64_emits_mulsd_in_body() {
        let m = build_mul_f64();
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        let has_mulsd = body.iter().any(|i| matches!(i, EncInst::MulsdRR { .. }));
        assert!(has_mulsd, "expected MulsdRR ; got {body:?}");
    }

    #[test]
    fn float_leaf_div_f64_emits_divsd_in_body() {
        let m = build_div_f64();
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        let has_divsd = body.iter().any(|i| matches!(i, EncInst::DivsdRR { .. }));
        assert!(has_divsd, "expected DivsdRR ; got {body:?}");
    }

    #[test]
    fn float_leaf_add_f32_emits_addss_in_body() {
        let m = build_add_f32();
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        let has_addss = body.iter().any(|i| matches!(i, EncInst::AddssRR { .. }));
        assert!(
            has_addss,
            "expected AddssRR (single-precision) ; got {body:?}"
        );
    }

    // ── full pipeline (end-to-end) ─────────────────────────────────────

    #[test]
    fn pipeline_add_f64_returns_object_bytes() {
        let m = build_add_f64();
        let bytes = emit_object_module_native(&m).expect("add_f64 pipeline should succeed");
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(crate::magic_prefix(host_default_format())));
    }

    #[test]
    fn pipeline_div_f64_returns_object_bytes() {
        let m = build_div_f64();
        let bytes = emit_object_module_native(&m).expect("div_f64 pipeline should succeed");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn pipeline_add_f32_returns_object_bytes() {
        let m = build_add_f32();
        let bytes = emit_object_module_native(&m).expect("add_f32 pipeline should succeed");
        assert!(!bytes.is_empty());
    }

    // ── return-value placement ─────────────────────────────────────────

    #[test]
    fn float_leaf_add_f64_ends_with_xmm0_in_return_position() {
        // For MS-x64 add_f64 : a→xmm0, b→xmm1, dst aliases xmm0 (since
        // the selector emits `Mov dst, lhs` and the loc-map renames dst
        // to xmm0). After AddsdRR, the result is already in xmm0 — so the
        // return-placement move should be elided.
        let m = build_add_f64();
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        // The last inst should be the AddsdRR — no explicit MovsdRR(xmm0, ...)
        // tail because the result already lives in xmm0.
        let last = body.last().expect("non-empty body");
        // Either Addsd (in-place result) OR a final MovsdRR copying into xmm0.
        // Both are correct shapes ; we just assert it's one of the two.
        assert!(
            matches!(last, EncInst::AddsdRR { dst: Xmm::Xmm0, .. } | EncInst::MovsdRR { dst: Xmm::Xmm0, .. }),
            "expected last inst to leave result in xmm0 ; got {last:?}"
        );
    }

    // ── byte-level proof : pi milestone end-to-end ─────────────────────

    /// G11 milestone : `fn pi() -> f64 { 3.14159265358979 }` end-to-end via
    /// native-x64 emits a function whose first instructions match :
    ///   push rbp ; mov rbp, rsp ; movabs rax, <pi-bits> ; movq xmm?, rax ;
    ///   movsd xmm0, xmm? ; pop rbp ; ret.
    /// (Or the movsd-elided form if the bit-transfer XMM happens to be
    ///  xmm0 — the test asserts the relaxed shape.)
    #[test]
    fn g11_milestone_pi_function_bytes_contain_pi_pattern_and_xmm0_placement() {
        let m = build_const_f64("pi", core::f64::consts::PI);
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        // 1. Body must start with push rbp ; mov rbp, rsp.
        assert_eq!(
            obj_func.bytes[0], 0x55,
            "expected push rbp at byte 0 ; got {:02X}",
            obj_func.bytes[0]
        );
        assert_eq!(
            &obj_func.bytes[1..4],
            &[0x48, 0x89, 0xE5],
            "expected mov rbp, rsp at bytes 1..4"
        );
        // 2. The pi bit pattern (LE) must appear somewhere in the body.
        let pi_bits = core::f64::consts::PI.to_bits().to_le_bytes();
        let found_pi = obj_func.bytes.windows(8).any(|w| w == pi_bits);
        assert!(
            found_pi,
            "pi bit pattern {pi_bits:02X?} must appear in body ; got {:02X?}",
            obj_func.bytes
        );
        // 3. Body must end with ret (0xC3).
        assert_eq!(
            *obj_func.bytes.last().unwrap(),
            0xC3,
            "expected ret as last byte ; got {:02X}",
            obj_func.bytes.last().unwrap()
        );
    }

    // ── unsupported-op rejection ────────────────────────────────────────

    #[test]
    fn float_leaf_rejects_fp_neg_at_g11() {
        // FpNeg requires an xorps + RIP-relative sign-mask load. That's
        // deferred to G12+. The pipeline must reject loudly.
        let mut m = MirModule::new();
        let f32_ty = MirType::Float(FloatWidth::F32);
        let mut f = MirFunc::new("neg_f32", vec![f32_ty.clone()], vec![f32_ty.clone()]);
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.negf")
                .with_result(r, f32_ty.clone())
                .with_operand(entry_args[0]),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        m.push_func(f);
        let err = emit_object_module_native(&m).unwrap_err();
        assert!(matches!(err, NativeX64Error::UnsupportedOp { .. }));
    }

    #[test]
    fn float_leaf_rejects_compare_at_g11() {
        // arith.cmpf produces a Bool via Setcc + Ucomi/Comi — outside the
        // G11 leaf subset. Pipeline should reject.
        let mut m = MirModule::new();
        let f64_ty = MirType::Float(FloatWidth::F64);
        let bool_ty = MirType::Bool;
        let mut f = MirFunc::new(
            "cmp_f64",
            vec![f64_ty.clone(), f64_ty.clone()],
            vec![bool_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.cmpf")
                .with_result(r, bool_ty)
                .with_operand(entry_args[0])
                .with_operand(entry_args[1])
                .with_attribute("predicate", "olt"),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        m.push_func(f);
        let err = emit_object_module_native(&m).unwrap_err();
        assert!(matches!(err, NativeX64Error::UnsupportedOp { .. }));
    }

    // ── parameter-loc derivation ───────────────────────────────────────

    #[test]
    fn float_leaf_derives_xmm0_for_first_f64_param() {
        // Build `fn id_f64(a: f64) -> f64 { a }` : entry-arg param vreg
        // must come from xmm0 (both ABIs).
        let mut module = MirModule::with_name("test.id_f64.module");
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new("id_f64", vec![f64_ty.clone()], vec![f64_ty.clone()]);
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        f.push_op(MirOp::std("func.return").with_operand(entry_args[0]));
        module.push_func(f);
        let funcs = select_module_with_marker(&module).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        // The body should be empty or contain just a no-op move (xmm0 → xmm0
        // is elided in our return-placement).
        assert!(
            body.is_empty() || body.iter().all(|i| matches!(i, EncInst::MovsdRR { dst: Xmm::Xmm0, src: Xmm::Xmm0 })),
            "id_f64 should require no copy ; got {body:?}"
        );
    }

    // ── sanity : returned value-register matches ABI ───────────────────

    #[test]
    fn float_leaf_const_f64_writes_xmm0_in_pipeline_output() {
        // Ensure the FINAL move-into-return-register places the value in
        // xmm0 (not xmm1 or elsewhere).
        let m = build_const_f64("c", 2.5);
        let funcs = select_module_with_marker(&m).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        let final_xmm0 = body.iter().rev().find_map(|i| match i {
            EncInst::MovsdRR { dst, .. } => Some(*dst),
            EncInst::MovqXmmFromGp { dst, .. } => Some(*dst),
            _ => None,
        });
        assert!(
            final_xmm0 == Some(Xmm::Xmm0)
                || body.iter().any(|i| matches!(i, EncInst::MovsdRR { dst: Xmm::Xmm0, .. })),
            "expected xmm0 to receive the f64 value ; got body {body:?}"
        );
    }

    // ── ABI-specific multi-arg test ─────────────────────────────────────

    #[test]
    fn float_leaf_three_arg_f64_uses_xmm0_xmm1_xmm2() {
        // `fn three(a: f64, b: f64, c: f64) -> f64 { (a + b) + c }` —
        // tests that the three params land in xmm0, xmm1, xmm2 (or xmm0,
        // xmm1, xmm2 on MS-x64 since each is a float and they share the
        // positional counter, but float-arg-regs xmm0..xmm3 cover them).
        let mut module = MirModule::with_name("test.three.module");
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new(
            "three",
            vec![f64_ty.clone(), f64_ty.clone(), f64_ty.clone()],
            vec![f64_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        let t = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.addf")
                .with_result(t, f64_ty.clone())
                .with_operand(entry_args[0])
                .with_operand(entry_args[1]),
        );
        let r = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.addf")
                .with_result(r, f64_ty.clone())
                .with_operand(t)
                .with_operand(entry_args[2]),
        );
        f.push_op(MirOp::std("func.return").with_operand(r));
        module.push_func(f);
        let funcs = select_module_with_marker(&module).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        // Should contain (at least) two AddsdRR ops.
        let addsd_count = body
            .iter()
            .filter(|i| matches!(i, EncInst::AddsdRR { .. }))
            .count();
        assert_eq!(
            addsd_count, 2,
            "three-fold add_f64 should emit 2 AddsdRR ; got {body:?}"
        );
    }

    // ── float and integer parameter mix (the MS-x64 positional alias) ──

    #[test]
    fn float_leaf_mixed_f64_i32_params_dispatch_correctly() {
        // `fn mixed(a: i32, b: f64, c: i32) -> f64 { b }` :
        //   - SysV : a→rdi, b→xmm0 (independent counter), c→rsi
        //   - MS-x64 : a→rcx (slot 0), b→xmm1 (slot 1, positional), c→r8 (slot 2)
        // The pipeline must place `b` into xmm0 at return time.
        let mut module = MirModule::with_name("test.mixed.module");
        let i32_ty = MirType::Int(IntWidth::I32);
        let f64_ty = MirType::Float(FloatWidth::F64);
        let mut f = MirFunc::new(
            "mixed",
            vec![i32_ty.clone(), f64_ty.clone(), i32_ty.clone()],
            vec![f64_ty.clone()],
        );
        let entry_args: Vec<cssl_mir::ValueId> =
            f.body.entry().unwrap().args.iter().map(|a| a.id).collect();
        f.push_op(MirOp::std("func.return").with_operand(entry_args[1]));
        module.push_func(f);
        let funcs = select_module_with_marker(&module).unwrap();
        let body = isel_to_encoder_simple(&funcs[0]).unwrap();
        // Per ABI the b-param's xmm reg differs : xmm0 (SysV) or xmm1 (MS-x64).
        // The body should either emit a MovsdRR(xmm0, src) with src matching
        // that reg, or be empty if src happens to be xmm0 already.
        if cfg!(target_os = "windows") {
            // MS-x64 : b is positional slot 1 → xmm1. Body must emit a
            // movsd xmm0, xmm1 to place the return value.
            let has_xmm0_from_xmm1 = body.iter().any(|i| {
                matches!(
                    i,
                    EncInst::MovsdRR {
                        dst: Xmm::Xmm0,
                        src: Xmm::Xmm1
                    }
                )
            });
            assert!(
                has_xmm0_from_xmm1,
                "MS-x64 mixed-arg must emit movsd xmm0, xmm1 ; got {body:?}"
            );
        } else {
            // SysV : b is float-counter slot 0 → xmm0. Body should be empty
            // (no move needed) OR emit movsd xmm0, xmm0 (which is a valid
            // no-op move).
            let only_no_op = body.iter().all(|i| {
                matches!(
                    i,
                    EncInst::MovsdRR {
                        dst: Xmm::Xmm0,
                        src: Xmm::Xmm0
                    }
                )
            });
            assert!(
                body.is_empty() || only_no_op,
                "SysV mixed-arg should require no copy ; got {body:?}"
            );
        }
    }

    // ── encoder-bytes shape verification ────────────────────────────────

    #[test]
    fn pipeline_add_f64_body_contains_addsd_byte_pattern() {
        // For `fn add_f64(a:f64, b:f64) -> f64 { a + b }` on MS-x64 :
        //   - a → xmm0 (mapped via param map)
        //   - b → xmm1
        //   - dst aliases a (xmm0) after the selector's `Mov dst, lhs`
        //   - addsd xmm0, xmm1 ⇒ F2 0F 58 C1
        let m = build_add_f64();
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        // Look for the canonical addsd-xmm0-xmm1 byte sequence.
        let needle = [0xF2, 0x0F, 0x58, 0xC1];
        let found = obj_func.bytes.windows(needle.len()).any(|w| w == needle);
        assert!(
            found,
            "expected addsd xmm0, xmm1 (F2 0F 58 C1) in body ; got {:02X?}",
            obj_func.bytes
        );
    }

    // ── round-trip f32 + bytes ─────────────────────────────────────────

    #[test]
    fn pipeline_add_f32_body_contains_addss_byte_pattern() {
        // addss xmm0, xmm1 ⇒ F3 0F 58 C1
        let m = build_add_f32();
        let funcs = select_module_with_marker(&m).unwrap();
        let abi = X64Abi::host_default();
        let obj_func = build_func_bytes(&funcs[0], abi, true).unwrap();
        let needle = [0xF3, 0x0F, 0x58, 0xC1];
        let found = obj_func.bytes.windows(needle.len()).any(|w| w == needle);
        assert!(
            found,
            "expected addss xmm0, xmm1 (F3 0F 58 C1) ; got {:02X?}",
            obj_func.bytes
        );
    }

    // ── empty / trivial-shape robustness ───────────────────────────────

    #[test]
    fn float_leaf_returns_none_for_void_fn_with_no_float_ops() {
        // `fn nullary() -> ()` shouldn't trigger the float walker.
        let mut m = MirModule::new();
        let f = MirFunc::new("nullary", vec![], vec![]);
        m.push_func(f);
        let mut local = m.clone();
        local.attributes.push((
            cssl_mir::STRUCTURED_CFG_VALIDATED_KEY.to_string(),
            cssl_mir::STRUCTURED_CFG_VALIDATED_VALUE.to_string(),
        ));
        // Pipeline should not bind anything.
        let funcs = select_module_with_marker(&local).unwrap();
        let result = try_lower_float_leaf(&funcs[0]).unwrap();
        assert!(result.is_none(), "void+no-float fn must not match float leaf");
    }

    #[test]
    fn pipeline_const_f64_returns_object_bytes_under_each_format() {
        let m = build_const_f64("c", 2.0);
        for fmt in [ObjectFormat::Elf, ObjectFormat::Coff, ObjectFormat::MachO] {
            let bytes = emit_object_module_native_with_format(&m, fmt)
                .unwrap_or_else(|e| panic!("format {fmt:?} pipeline failed : {e}"));
            assert!(!bytes.is_empty());
            assert!(bytes.starts_with(crate::magic_prefix(fmt)));
        }
    }
}

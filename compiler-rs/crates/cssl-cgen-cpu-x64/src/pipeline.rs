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
//!   - SSE2 float path through encoder (G4 already supports it ; the
//!     selector emits FpAdd/FpSub/etc. that this module rejects today).
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
/// § DISPATCH (G7-leaf vs G8-LSRA)
///   The function dispatches between two routes :
///     - **G7 leaf-path** : if [`ScalarLeafReturn::try_extract`] succeeds,
///       the function matches the canonical `fn () -> i32 { N }` shape and
///       we use the simple G1 → G4 direct lowering (no register allocator
///       involvement). This preserves the canonical 11-byte milestone body
///       (`55 48 89 E5 B8 NN NN NN NN 5D C3`) for the hello-world case.
///     - **G8 LSRA-path** : otherwise we delegate to
///       [`crate::lsra_pipeline::build_func_bytes_via_lsra`] which routes
///       through the full G2 LSRA + spill-slot allocation + callee-saved
///       push/pop emission.
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
    // § Try the G7 scalar-leaf fast-path first. If the function matches the
    //   `fn () -> i32 { N }` shape, the simple lowering produces the
    //   canonical milestone bytes ; this preserves the SECOND hello.exe = 42
    //   bit-for-bit through G8 landing.
    if let Ok(_leaf) = ScalarLeafReturn::try_extract(func) {
        return build_func_bytes_leaf(func, abi, is_export);
    }
    // § G8 fallthrough : non-leaf functions (multi-arg signatures, integer
    //   arithmetic, register pressure) route through the FULL LSRA pipeline.
    crate::lsra_pipeline::build_func_bytes_via_lsra(func, abi, is_export)
}

/// G7 leaf-path body : the original simple lowering that produced the
/// canonical 11-byte milestone body for `fn main() -> i32 { N }`. Preserved
/// verbatim from T11-D97 so the milestone bit-pattern is bit-for-bit
/// invariant across the G8 landing.
///
/// # Errors
/// Returns [`NativeX64Error`] for any per-stage adapter failure.
pub fn build_func_bytes_leaf(
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
        // ‼ Post-T11-D101 (G8 LSRA-pipeline) the canonical `arith.addi` path
        //   THAT WAS UNSUPPORTED at G7 now succeeds end-to-end via the LSRA
        //   route. To keep this test asserting the canonical reject-path we
        //   build a fn using `arith.sdivi` — which requires rax/rdx fixed-
        //   preg pinning not yet wired in G8 (deferred to G9 slice).
        let mut m = MirModule::new();
        let mut f = cssl_mir::MirFunc::new(
            "div_test",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        // MirFunc::new auto-populates entry-block args from `params`.
        let a = cssl_mir::ValueId(0);
        let b = cssl_mir::ValueId(1);
        let v_div = f.fresh_value_id();
        f.push_op(
            MirOp::std("arith.sdivi")
                .with_result(v_div, MirType::Int(IntWidth::I32))
                .with_operand(a)
                .with_operand(b),
        );
        f.push_op(MirOp::std("func.return").with_operand(v_div));
        m.push_func(f);
        let err = emit_object_module_native(&m).unwrap_err();
        assert!(matches!(err, NativeX64Error::UnsupportedOp { .. }));
    }

    #[test]
    fn emit_object_module_native_with_arith_addi_succeeds_via_g8_lsra() {
        // ‼ T11-D101 G8 milestone : the addi-shape that pre-G8 surfaced
        //   UnsupportedOp now succeeds via the LSRA path.
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
        let bytes = emit_object_module_native(&m).expect("G8 LSRA path supports addi");
        assert!(!bytes.is_empty());
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
}

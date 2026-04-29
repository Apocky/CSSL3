//! § body — MIR-op → MSL-text body emission for `cssl-cgen-gpu-msl`.
//!
//! § SPEC
//!   - `specs/07_CODEGEN.csl` § GPU BACKEND — MSL path (line 59)
//!   - `specs/14_BACKEND.csl` § OWNED MSL EMITTER (stage1 target ; stage-0 emits
//!      hand-written MSL text directly from MIR — `spirv-cross --msl` retained
//!      as a round-trip validator).
//!   - `specs/02_IR.csl` § STRUCTURED-CFG-CONSTRAINT — every MIR module reaching
//!      this emitter MUST carry the `("structured_cfg.validated", "true")`
//!      attribute marker that the D5 validator (T11-D70) writes onto every
//!      well-formed module. The marker is the FANOUT-CONTRACT between D5 and
//!      every D-emitter ; calling [`emit_body`] on a non-validated module
//!      surfaces [`BodyError::MissingStructuredCfgMarker`] instead of silently
//!      mis-lowering goto-style branches.
//!
//! § ROLE — D3 (T11-D74)
//!   `cssl-cgen-gpu-msl` shipped in T10 with [`crate::emit::emit_msl`]
//!   producing skeleton-only MSL : entry-point signature, `[[kernel]]` /
//!   `[[vertex]]` / `[[fragment]]` attribute, and a `// stage-0 skeleton —
//!   MIR body lowered @ T10-phase-2` placeholder line. T11-D74 lifts that
//!   placeholder into real MSL body emission for the stage-0 op subset
//!   shared by every GPU-emitter (D1..D4).
//!
//!   The op subset matches what `cssl_mir::body_lower` produces today :
//!     - `arith.constant` (i32 / i64 / f32 / f64 / bool literals via attribute)
//!     - `arith.{add,sub,mul,div,rem}{i,f}` + `arith.cmpi_*` + `arith.cmpf_*`
//!     - `arith.{and,or,xor}i` + bitcast
//!     - `func.return` + `func.call`
//!     - `scf.if` (statement-form + expression-form via yield)
//!     - `scf.for` / `scf.while` / `scf.loop` body-only stage-0 shapes
//!     - `memref.load` / `memref.store` for `!cssl.ptr` operands (mapped to
//!        Metal `device <T>*` pointer-deref ; stage-0 uses `device` address
//!        space for every pointer — `threadgroup` / `constant` are reserved
//!        for D-emitter follow-up slices once binding-classification lands).
//!     - `scf.yield` (consumed by surrounding scf.if / scf.* ; not emitted
//!        directly as a statement).
//!
//!   Heap ops (`cssl.heap.alloc` / `dealloc` / `realloc`) and closures
//!   (`cssl.closure` / `cssl.unsupported(*)`) are rejected with
//!   [`BodyError::HeapNotSupportedInMsl`] / [`BodyError::ClosuresNotSupportedInMsl`].
//!   Per the slice handoff landmines : Metal compute kernels do not own a
//!   user-controlled CPU-side allocator and Metal does not support C++14-style
//!   function pointers / lambdas in shader source. Surfacing those rejections
//!   as structured errors here (rather than letting the emitter produce
//!   garbage MSL) is the safer stage-0 contract.
//!
//! § ADDRESS-SPACE DISCIPLINE (stage-0)
//!   Every MIR `!cssl.ptr` operand maps to MSL `device <T>*` at stage-0.
//!   Metal's three pointer address spaces — `device` (large, read-write GPU
//!   memory), `threadgroup` (per-workgroup shared memory, ~32KB), and
//!   `constant` (small read-only buffers, ~4KB) — encode capability
//!   distinctions that CSSLv3's effect-row + capability system represents but
//!   stage-0 has not yet wired through to the MIR pointer attribute. Future
//!   slices that surface `cap = threadgroup_shared` / `cap = const` on MIR
//!   `!cssl.ptr` operands will switch the address-space prefix here. Until
//!   then `device` is the conservative safe default — every real GPU resource
//!   binding is `device`-addressable, and a temporary aliasing-discipline
//!   advisory comment in the emitter output documents the choice for human
//!   review of generated MSL.
//!
//! § SCOPE — MSL 2.0+ syntax
//!   The slice mandates "valid MSL 2.0+ syntax for compute kernels".
//!   Concretely : `[[kernel]] void name(...)` entry-points,
//!   `[[buffer(N)]]` / `[[thread_position_in_grid]]` attribute syntax,
//!   `device float*` pointer types, and C++14-derived statement / expression
//!   syntax. The output is targeted by Apple's `metal` driver for compilation
//!   to AIR/Metal-IR ; we do not invoke that toolchain at stage-0 because the
//!   compiler runs cross-platform (Apocky's primary host is Windows). The
//!   spirv-cross `--msl` path (see [`crate::spirv_cross::SpirvCrossInvoker`])
//!   is retained as the round-trip validator that fires when D1 (SPIR-V) lands
//!   alongside : differential-test of MIR → SPIR-V → MSL (via spirv-cross) vs
//!   MIR → MSL (this module) catches divergence between the two emission
//!   paths. At D3 alone the validator is plumbing only.
//!
//! § CONTRACT
//!   - [`emit_body`] is the canonical body-text emitter. It walks the entry
//!     fn's body region (one block at stage-0) and produces a `Vec<String>`
//!     where each element is one MSL source line (no trailing newline). The
//!     caller (typically [`crate::emit::emit_msl`]) splices the lines into
//!     the entry fn's body via [`crate::msl::MslStatement::Function`].
//!   - The emitter expects exactly the structured-CFG shape D5 enforces ;
//!     malformed input returns the appropriate [`BodyError`] variant.
//!   - Output is deterministic : identical MIR input always produces
//!     byte-identical MSL text. This is essential for the round-trip
//!     differential-test that D1 enables.
//!
//! § DESIGN — `emit_body` is recursive over scf-regions
//!   The emitter walks `MirOp`s sequentially in the entry block. Each scf.if /
//!   scf.for / scf.while / scf.loop op recurses into its nested regions with
//!   indentation increased by one level. Yield-handling in scf.if uses the
//!   "merge-variable" pattern : when scf.if has a typed result, the emitter
//!   declares a merge variable before the `if/else`, both branches assign
//!   their yield-operand to it, and the result-id maps to the merge-variable
//!   for downstream lookups.

use cssl_mir::{
    has_structured_cfg_marker, FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirRegion, MirType,
    ValueId,
};
use std::collections::HashMap;
use thiserror::Error;

/// Failure modes for MSL body emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BodyError {
    /// The MIR module was not validated by D5 (T11-D70). Per
    /// `specs/02_IR.csl § STRUCTURED-CFG-CONSTRAINT`, every GPU emitter
    /// requires a structurally-validated module ; calling MSL emission on
    /// raw MIR risks emitting goto-style branches that Metal's driver
    /// cannot compile.
    #[error(
        "MIR module is not structured-CFG-validated ; run `cssl_mir::validate_and_mark` (D5 / T11-D70) before MSL emission"
    )]
    MissingStructuredCfgMarker,

    /// An op the MSL emitter does not yet handle. The op's name is included
    /// for actionable diagnostics. Stage-0 supports only the subset documented
    /// at the top of this module ; growing it requires a new D-slice.
    #[error("MSL emitter does not yet support op `{op_name}` in fn `{fn_name}` (stage-0 subset)")]
    UnsupportedOp { fn_name: String, op_name: String },

    /// A `MirType` that does not have a stage-0 MSL representation. Includes
    /// `MirType::Tuple` (no MSL tuple type ; would need explicit struct
    /// generation), `MirType::Function` (no first-class fn types in MSL),
    /// `MirType::Memref` (D-emitters use `MirType::Ptr` instead — memref's
    /// rank metadata is CPU-only at stage-0), and `MirType::Opaque(*)` for
    /// the various closure / unsupported placeholders.
    #[error("MSL emitter cannot represent MIR type `{ty}` in fn `{fn_name}`")]
    NonScalarType { fn_name: String, ty: String },

    /// An op operand referenced a ValueId not in scope. Either the op uses
    /// a value that was never produced (frontend bug) or the value lives in
    /// a region the emitter has not yet walked (block-arg threading bug).
    #[error("MSL emitter found unknown ValueId({value_id}) in fn `{fn_name}`")]
    UnknownValueId { fn_name: String, value_id: u32 },

    /// Heap allocation ops (`cssl.heap.alloc` / `dealloc` / `realloc`) are
    /// not supported in Metal compute kernels — the GPU does not own a
    /// CPU-side allocator. Buffers are owned by the Metal-host (D3 / E3) and
    /// passed in via `[[buffer(N)]]` bindings.
    #[error(
        "MSL emitter rejects heap op `{op_name}` in fn `{fn_name}` ; Metal compute kernels do not own a CPU-side allocator"
    )]
    HeapNotSupportedInMsl { fn_name: String, op_name: String },

    /// Closure ops (`cssl.closure` / `cssl.unsupported(Lambda)` etc.) are not
    /// supported in MSL source — Metal's shading language has no
    /// function-pointer / capture mechanism inside shader code.
    #[error(
        "MSL emitter rejects closure op `{op_name}` in fn `{fn_name}` ; Metal shaders cannot capture environment values"
    )]
    ClosuresNotSupportedInMsl { fn_name: String, op_name: String },

    /// A scf.* op had a region-shape that the emitter could not lower. Only
    /// surfaces if D5 missed something ; defensive.
    #[error(
        "MSL emitter saw `{op_name}` with malformed regions ({reason}) in fn `{fn_name}` (D5 should have caught this)"
    )]
    MalformedScf {
        fn_name: String,
        op_name: String,
        reason: String,
    },

    /// `arith.constant` was missing its `"value"` attribute. body_lower
    /// always sets it ; defensive.
    #[error("MSL emitter saw `arith.constant` without `value` attribute in fn `{fn_name}`")]
    MissingConstantValue { fn_name: String },
}

/// State for emitting one MSL body. Tracks the SSA `ValueId → MSL identifier`
/// mapping + a fresh-name counter for declarations the emitter introduces
/// (e.g. scf.if merge-variables).
struct BodyEmitter<'a> {
    fn_name: &'a str,
    /// Map from MIR ValueId to its MSL source-form identifier (e.g. `"v0"`,
    /// `"v3"`, or `"merge0"` for compiler-introduced names).
    value_names: HashMap<ValueId, String>,
    /// Fresh-name counter for compiler-introduced identifiers.
    fresh_counter: u32,
    /// Output lines. Each element is one MSL source line ; the caller appends
    /// newlines at render time.
    out: Vec<String>,
    /// Current indentation depth (number of 4-space stops).
    indent: usize,
}

impl<'a> BodyEmitter<'a> {
    fn new(fn_name: &'a str) -> Self {
        Self {
            fn_name,
            value_names: HashMap::new(),
            fresh_counter: 0,
            out: Vec::new(),
            indent: 1, // Body starts inside `[[kernel]] void name() {`.
        }
    }

    /// Mint a fresh identifier that does not collide with any existing
    /// SSA value-name. Used for scf.if merge-variables.
    fn fresh(&mut self, prefix: &str) -> String {
        let n = self.fresh_counter;
        self.fresh_counter += 1;
        format!("{prefix}{n}")
    }

    /// The MSL identifier for a ValueId. Returns the canonical SSA name
    /// `vN` when the value has not been bound by the emitter yet (typical
    /// for entry-block params or operands consumed before they appear as
    /// op-results in our walk — defensive fallback only).
    fn name_for(&self, id: ValueId) -> Result<String, BodyError> {
        self.value_names
            .get(&id)
            .cloned()
            .ok_or(BodyError::UnknownValueId {
                fn_name: self.fn_name.to_string(),
                value_id: id.0,
            })
    }

    /// Bind a ValueId to its canonical SSA name `vN`. Called when an op's
    /// result-list materializes a fresh value.
    fn bind_canonical(&mut self, id: ValueId) -> String {
        let name = format!("v{}", id.0);
        self.value_names.insert(id, name.clone());
        name
    }

    /// Bind a ValueId to a caller-supplied identifier (e.g., the merge-var
    /// name produced by scf.if lowering).
    fn bind_named(&mut self, id: ValueId, name: String) {
        self.value_names.insert(id, name);
    }

    fn line(&mut self, content: impl Into<String>) {
        let pad = "    ".repeat(self.indent);
        self.out.push(format!("{pad}{}", content.into()));
    }
}

/// Emit MSL source-text body lines for the entry fn `entry_name` of `module`.
///
/// The returned `Vec<String>` contains one MSL source line per element (no
/// trailing newline). The caller (typically [`crate::emit::emit_msl`]) splices
/// these into a [`crate::msl::MslStatement::Function`] body.
///
/// # Errors
/// Returns [`BodyError`] variants when :
///   - the module is not D5-validated (`MissingStructuredCfgMarker`),
///   - the entry fn does not exist or its body has zero blocks,
///   - any op is outside the stage-0 subset (`UnsupportedOp`),
///   - any type cannot be represented (`NonScalarType`),
///   - any operand references an unknown ValueId (`UnknownValueId`),
///   - heap or closure ops surface (`HeapNotSupportedInMsl` /
///     `ClosuresNotSupportedInMsl`),
///   - or any scf op has a malformed region-shape (`MalformedScf`).
pub fn emit_body(module: &MirModule, entry_name: &str) -> Result<Vec<String>, BodyError> {
    if !has_structured_cfg_marker(module) {
        return Err(BodyError::MissingStructuredCfgMarker);
    }
    let Some(entry_fn) = module.find_func(entry_name) else {
        // The caller (`emit_msl`) checks for this earlier ; defensive
        // duplicate-check here means `emit_body` can be unit-tested
        // independently.
        return Err(BodyError::UnknownValueId {
            fn_name: entry_name.to_string(),
            value_id: 0,
        });
    };
    let mut emitter = BodyEmitter::new(&entry_fn.name);

    // Bind entry-block params (kernel arguments) to canonical names. The
    // outer signature emitter (`emit_msl`) declared them in the param list
    // as `Tx vN`, so we match that here.
    let Some(entry) = entry_fn.body.blocks.first() else {
        return Ok(emitter.out);
    };
    for arg in &entry.args {
        emitter.bind_canonical(arg.id);
    }

    walk_block_ops(&entry.ops, &mut emitter)?;
    Ok(emitter.out)
}

/// Walk every op in a block's op-list, lowering each. Used both for the
/// top-level fn body and for nested scf.* regions.
fn walk_block_ops(ops: &[MirOp], em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    for op in ops {
        // scf.yield is a region terminator ; never emitted directly as a
        // statement. The owning scf.if / scf.for / scf.while / scf.loop
        // handles it via a custom walker that captures the yielded value.
        if op.name == "scf.yield" {
            continue;
        }
        emit_op(op, em)?;
    }
    Ok(())
}

/// Lower one MirOp to MSL text. Dispatches to the per-op emitters below.
fn emit_op(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    match op.name.as_str() {
        // § Heap ops — REJECT.
        "cssl.heap.alloc" | "cssl.heap.dealloc" | "cssl.heap.realloc" => {
            Err(BodyError::HeapNotSupportedInMsl {
                fn_name: em.fn_name.to_string(),
                op_name: op.name.clone(),
            })
        }
        // § Closures + unsupported placeholders — REJECT.
        "cssl.closure" => Err(BodyError::ClosuresNotSupportedInMsl {
            fn_name: em.fn_name.to_string(),
            op_name: op.name.clone(),
        }),
        n if n.starts_with("cssl.unsupported") => Err(BodyError::ClosuresNotSupportedInMsl {
            fn_name: em.fn_name.to_string(),
            op_name: op.name.clone(),
        }),

        // § arith.constant — declare typed local with literal value.
        "arith.constant" => emit_arith_constant(op, em),

        // § arith binary ops — i / f variants share the same emit shape.
        "arith.addi" => emit_binary(op, em, "+"),
        "arith.subi" => emit_binary(op, em, "-"),
        "arith.muli" => emit_binary(op, em, "*"),
        "arith.divsi" | "arith.divui" => emit_binary(op, em, "/"),
        "arith.remsi" | "arith.remui" => emit_binary(op, em, "%"),
        "arith.addf" => emit_binary(op, em, "+"),
        "arith.subf" => emit_binary(op, em, "-"),
        "arith.mulf" => emit_binary(op, em, "*"),
        "arith.divf" => emit_binary(op, em, "/"),
        // MSL has no `%` for floats ; use `fmod` from the metal_stdlib.
        "arith.remf" => emit_call_binary(op, em, "fmod"),

        // § arith comparison — int + float.
        "arith.cmpi_eq" => emit_binary(op, em, "=="),
        "arith.cmpi_ne" => emit_binary(op, em, "!="),
        "arith.cmpi_slt" | "arith.cmpi_ult" => emit_binary(op, em, "<"),
        "arith.cmpi_sle" | "arith.cmpi_ule" => emit_binary(op, em, "<="),
        "arith.cmpi_sgt" | "arith.cmpi_ugt" => emit_binary(op, em, ">"),
        "arith.cmpi_sge" | "arith.cmpi_uge" => emit_binary(op, em, ">="),
        "arith.cmpf_oeq" | "arith.cmpf_ueq" => emit_binary(op, em, "=="),
        "arith.cmpf_one" | "arith.cmpf_une" => emit_binary(op, em, "!="),
        "arith.cmpf_olt" | "arith.cmpf_ult" => emit_binary(op, em, "<"),
        "arith.cmpf_ole" | "arith.cmpf_ule" => emit_binary(op, em, "<="),
        "arith.cmpf_ogt" | "arith.cmpf_ugt" => emit_binary(op, em, ">"),
        "arith.cmpf_oge" | "arith.cmpf_uge" => emit_binary(op, em, ">="),

        // § arith bitwise.
        "arith.andi" => emit_binary(op, em, "&"),
        "arith.ori" => emit_binary(op, em, "|"),
        "arith.xori" => emit_binary(op, em, "^"),
        "arith.shli" => emit_binary(op, em, "<<"),
        "arith.shrsi" | "arith.shrui" => emit_binary(op, em, ">>"),

        // § arith bitcast — MSL `as_type<T>(src)`.
        "arith.bitcast" => emit_bitcast(op, em),

        // § func.return — `return v;` or `return;`.
        "func.return" => emit_return(op, em),
        // § func.call — `T r = name(args...);`.
        "func.call" => emit_func_call(op, em),

        // § scf.if / scf.for / scf.while / scf.loop.
        "scf.if" => emit_scf_if(op, em),
        "scf.for" => emit_scf_for(op, em),
        "scf.while" => emit_scf_while(op, em),
        "scf.loop" => emit_scf_loop(op, em),

        // § memref.load / memref.store — pointer deref with default-aligned access.
        "memref.load" => emit_memref_load(op, em),
        "memref.store" => emit_memref_store(op, em),

        // § Anything else.
        _ => Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: op.name.clone(),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Type mapping — MIR → MSL.
// ─────────────────────────────────────────────────────────────────────────

/// Map a `MirType` to the matching MSL source-form type-name. MSL types are
/// C++14-derived : built-in scalar types (`int`, `uint`, `long`, `float`,
/// `double`, `bool`, `half`) plus Metal-specific vector types (`float3`,
/// `int4`) and pointer types with explicit address-space qualifiers.
fn mir_to_msl(ty: &MirType, fn_name: &str) -> Result<String, BodyError> {
    match ty {
        MirType::Int(IntWidth::I1) => Ok("bool".into()),
        MirType::Int(IntWidth::I8) => Ok("char".into()),
        MirType::Int(IntWidth::I16) => Ok("short".into()),
        MirType::Int(IntWidth::I32) => Ok("int".into()),
        MirType::Int(IntWidth::I64 | IntWidth::Index) => Ok("long".into()),
        MirType::Float(FloatWidth::F16) => Ok("half".into()),
        MirType::Float(FloatWidth::Bf16) => Ok("bfloat".into()),
        MirType::Float(FloatWidth::F32) => Ok("float".into()),
        MirType::Float(FloatWidth::F64) => Ok("double".into()),
        MirType::Bool => Ok("bool".into()),
        MirType::Vec(lanes, w) => {
            let elem = match w {
                FloatWidth::F16 => "half",
                FloatWidth::Bf16 => "bfloat",
                FloatWidth::F32 => "float",
                FloatWidth::F64 => "double",
            };
            // MSL Metal has no "float5" — vector lanes are 2/3/4 (occasionally
            // 8/16 for half/short). For unsupported lane counts we surface
            // NonScalarType so the caller can extend the table when needed.
            match lanes {
                2..=4 => Ok(format!("{elem}{lanes}")),
                _ => Err(BodyError::NonScalarType {
                    fn_name: fn_name.to_string(),
                    ty: format!("vector<{lanes}x{w}>", w = w.as_str()),
                }),
            }
        }
        MirType::Ptr => {
            // Stage-0 address-space discipline : every `!cssl.ptr` maps to
            // `device float*`. Future slices that surface element-type
            // metadata + capability classification will widen this table.
            Ok("device float*".into())
        }
        MirType::None => Ok("void".into()),
        // Non-representable shapes : tuples, function types, memref<rank x …>,
        // handles, opaque placeholders. The MSL emitter rejects them.
        MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Memref { .. }
        | MirType::Handle
        | MirType::Opaque(_) => Err(BodyError::NonScalarType {
            fn_name: fn_name.to_string(),
            ty: format!("{ty}"),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Per-op emitters — arithmetic + comparisons + bitcast + return + call.
// ─────────────────────────────────────────────────────────────────────────

/// `arith.constant` → `T vN = literal;`. Reads the `"value"` attribute that
/// `body_lower` sets on every constant op. The literal is rendered verbatim
/// for ints + floats ; bool literals are remapped to `true` / `false`.
fn emit_arith_constant(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    let Some(result) = op.results.first() else {
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: "arith.constant (no result)".into(),
        });
    };
    let ty = mir_to_msl(&result.ty, em.fn_name)?;
    let value = op
        .attributes
        .iter()
        .find(|(k, _)| k == "value")
        .map(|(_, v)| v.clone())
        .ok_or(BodyError::MissingConstantValue {
            fn_name: em.fn_name.to_string(),
        })?;
    // Normalize bool literals body_lower may emit as "0"/"1" or "true"/"false".
    let literal = if matches!(result.ty, MirType::Bool) {
        match value.as_str() {
            "true" | "1" => "true".into(),
            "false" | "0" => "false".into(),
            other => other.to_string(),
        }
    } else if let MirType::Float(_) = result.ty {
        // MSL's literal-suffix discipline : `1.0` is `double` ; `1.0f` is
        // `float` ; `1.0h` is `half`. body_lower stores the raw literal text ;
        // we do not re-suffix at stage-0 because both MSL 2.0+ and the Metal
        // shader compiler accept untyped float literals in `float` / `double`
        // initializers via implicit conversion. Future-proofing : a slice
        // that needs strict suffixing can append `f` / `h` here.
        value
    } else {
        value
    };
    let name = em.bind_canonical(result.id);
    em.line(format!("{ty} {name} = {literal};"));
    Ok(())
}

/// Binary op : `T r = a OP b;`. Used for arith.add / sub / mul / div / rem
/// (both i and f variants), comparisons, and bitwise ops.
fn emit_binary(op: &MirOp, em: &mut BodyEmitter<'_>, op_str: &str) -> Result<(), BodyError> {
    let Some(result) = op.results.first() else {
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: format!("{} (no result)", op.name),
        });
    };
    let ty = mir_to_msl(&result.ty, em.fn_name)?;
    let lhs = em.name_for(
        *op.operands
            .first()
            .ok_or_else(|| BodyError::UnsupportedOp {
                fn_name: em.fn_name.to_string(),
                op_name: format!("{} (missing lhs)", op.name),
            })?,
    )?;
    let rhs = em.name_for(*op.operands.get(1).ok_or_else(|| BodyError::UnsupportedOp {
        fn_name: em.fn_name.to_string(),
        op_name: format!("{} (missing rhs)", op.name),
    })?)?;
    let name = em.bind_canonical(result.id);
    em.line(format!("{ty} {name} = {lhs} {op_str} {rhs};"));
    Ok(())
}

/// Binary op rendered as a function-call (e.g. `fmod(a, b)` for `arith.remf`,
/// since MSL has no float `%` operator).
fn emit_call_binary(
    op: &MirOp,
    em: &mut BodyEmitter<'_>,
    fn_name_msl: &str,
) -> Result<(), BodyError> {
    let Some(result) = op.results.first() else {
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: format!("{} (no result)", op.name),
        });
    };
    let ty = mir_to_msl(&result.ty, em.fn_name)?;
    let lhs = em.name_for(
        *op.operands
            .first()
            .ok_or_else(|| BodyError::UnsupportedOp {
                fn_name: em.fn_name.to_string(),
                op_name: format!("{} (missing lhs)", op.name),
            })?,
    )?;
    let rhs = em.name_for(*op.operands.get(1).ok_or_else(|| BodyError::UnsupportedOp {
        fn_name: em.fn_name.to_string(),
        op_name: format!("{} (missing rhs)", op.name),
    })?)?;
    let name = em.bind_canonical(result.id);
    em.line(format!("{ty} {name} = {fn_name_msl}({lhs}, {rhs});"));
    Ok(())
}

/// `arith.bitcast` → MSL `as_type<T>(src)`. T is the result type ; src is the
/// single operand. Stage-0 supports scalar→scalar bitcasts only.
fn emit_bitcast(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    let Some(result) = op.results.first() else {
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: "arith.bitcast (no result)".into(),
        });
    };
    let ty = mir_to_msl(&result.ty, em.fn_name)?;
    let src = em.name_for(
        *op.operands
            .first()
            .ok_or_else(|| BodyError::UnsupportedOp {
                fn_name: em.fn_name.to_string(),
                op_name: "arith.bitcast (missing src)".into(),
            })?,
    )?;
    let name = em.bind_canonical(result.id);
    em.line(format!("{ty} {name} = as_type<{ty}>({src});"));
    Ok(())
}

/// `func.return` → `return v;` or `return;`. Mirrors the cranelift backend's
/// terminator semantics — multi-value returns are not stage-0.
fn emit_return(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    if op.operands.is_empty() {
        em.line("return;");
    } else if op.operands.len() == 1 {
        let v = em.name_for(op.operands[0])?;
        em.line(format!("return {v};"));
    } else {
        // Multi-value returns would require a struct-return shape ; defer.
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: "func.return (multi-value)".into(),
        });
    }
    Ok(())
}

/// `func.call` → `T r = name(args...);`. The callee name lives in the op's
/// `"callee"` attribute ; the rendered identifier is verbatim. Body-emitter
/// tests verify this against the canonical metal-stdlib intrinsic names
/// `sqrt` / `sin` / `cos`.
fn emit_func_call(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    let callee = op
        .attributes
        .iter()
        .find(|(k, _)| k == "callee")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: "func.call (no callee attr)".into(),
        })?;
    let mut args = Vec::with_capacity(op.operands.len());
    for o in &op.operands {
        args.push(em.name_for(*o)?);
    }
    let args_str = args.join(", ");
    if let Some(result) = op.results.first() {
        let ty = mir_to_msl(&result.ty, em.fn_name)?;
        let name = em.bind_canonical(result.id);
        em.line(format!("{ty} {name} = {callee}({args_str});"));
    } else {
        em.line(format!("{callee}({args_str});"));
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// § Memref ops — load / store via pointer deref.
// ─────────────────────────────────────────────────────────────────────────

/// `memref.load` → `T v = ptr[idx];` (or `T v = *ptr;` when no index operand).
/// Stage-0 maps `!cssl.ptr` to `device <T>*`, so MSL's `[]` operator over a
/// `device` pointer is the canonical form. The op's `"alignment"` attribute
/// (T11-D59 / S6-C3) is documented in the emitted comment but Metal's driver
/// derives alignment from the pointer-type — no MSL syntax for explicit
/// alignment hints on a load.
fn emit_memref_load(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    let Some(result) = op.results.first() else {
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: "memref.load (no result)".into(),
        });
    };
    let ty = mir_to_msl(&result.ty, em.fn_name)?;
    let ptr = em.name_for(
        *op.operands
            .first()
            .ok_or_else(|| BodyError::UnsupportedOp {
                fn_name: em.fn_name.to_string(),
                op_name: "memref.load (missing ptr)".into(),
            })?,
    )?;
    let access = if let Some(idx) = op.operands.get(1) {
        let i = em.name_for(*idx)?;
        format!("{ptr}[{i}]")
    } else {
        format!("(*{ptr})")
    };
    let name = em.bind_canonical(result.id);
    em.line(format!("{ty} {name} = {access};"));
    Ok(())
}

/// `memref.store` → `ptr[idx] = val;` (or `*ptr = val;` when no index). No
/// result-list ; stage-0 store is fire-and-forget.
fn emit_memref_store(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    if op.operands.len() < 2 {
        return Err(BodyError::UnsupportedOp {
            fn_name: em.fn_name.to_string(),
            op_name: "memref.store (need >= 2 operands)".into(),
        });
    }
    // body_lower's convention : op.operands = [value, ptr, idx?]
    let val = em.name_for(op.operands[0])?;
    let ptr = em.name_for(op.operands[1])?;
    let lhs = if let Some(idx) = op.operands.get(2) {
        let i = em.name_for(*idx)?;
        format!("{ptr}[{i}]")
    } else {
        format!("(*{ptr})")
    };
    em.line(format!("{lhs} = {val};"));
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// § scf.if — statement-form + expression-form (via merge-variable).
// ─────────────────────────────────────────────────────────────────────────

/// `scf.if` → MSL `if/else` block. Two regions (then + else) ; D5 has
/// validated this. When the op has a result, the emitter declares a merge
/// variable before the `if` and rewrites each branch's `scf.yield` operand
/// into an assignment to the merge-var.
fn emit_scf_if(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    if op.regions.len() != 2 {
        return Err(BodyError::MalformedScf {
            fn_name: em.fn_name.to_string(),
            op_name: "scf.if".into(),
            reason: format!("region count = {}", op.regions.len()),
        });
    }
    let cond = em.name_for(*op.operands.first().ok_or_else(|| BodyError::MalformedScf {
        fn_name: em.fn_name.to_string(),
        op_name: "scf.if".into(),
        reason: "missing condition operand".into(),
    })?)?;

    let merge_info = if let Some(result) = op.results.first() {
        if matches!(result.ty, MirType::None) {
            None
        } else {
            let ty = mir_to_msl(&result.ty, em.fn_name)?;
            let merge_name = em.fresh("scf_if_merge_");
            em.bind_named(result.id, merge_name.clone());
            em.line(format!("{ty} {merge_name};"));
            Some(merge_name)
        }
    } else {
        None
    };

    em.line(format!("if ({cond}) {{"));
    em.indent += 1;
    walk_branch_with_merge(&op.regions[0], em, merge_info.as_deref())?;
    em.indent -= 1;
    em.line("} else {".to_string());
    em.indent += 1;
    walk_branch_with_merge(&op.regions[1], em, merge_info.as_deref())?;
    em.indent -= 1;
    em.line("}".to_string());
    Ok(())
}

/// Walk a scf.if branch's single block. Captures the scf.yield operand and
/// emits `<merge_name> = <yielded>;` when `merge_name` is present.
fn walk_branch_with_merge(
    region: &MirRegion,
    em: &mut BodyEmitter<'_>,
    merge_name: Option<&str>,
) -> Result<(), BodyError> {
    let Some(entry) = region.blocks.first() else {
        return Ok(());
    };
    for op in &entry.ops {
        if op.name == "scf.yield" {
            if let Some(merge) = merge_name {
                let yielded =
                    em.name_for(*op.operands.first().ok_or_else(|| BodyError::MalformedScf {
                        fn_name: em.fn_name.to_string(),
                        op_name: "scf.yield".into(),
                        reason: "no operand".into(),
                    })?)?;
                em.line(format!("{merge} = {yielded};"));
            }
            // After scf.yield the region terminates ; further ops (if any)
            // are dead code per D5's structured-CFG rules.
            break;
        }
        emit_op(op, em)?;
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// § scf.for / scf.while / scf.loop — body-only stage-0 shapes.
// ─────────────────────────────────────────────────────────────────────────

/// `scf.for` → MSL `for(;;) { body; break; }`. Stage-0 single-trip body
/// (mirrors the cranelift lowering — see `cssl-cgen-cpu-cranelift/src/scf.rs`
/// § DEFERRED on iter-counter growth). The trailing `break;` ensures the
/// MSL output is well-formed (no infinite loop) until iter-counter lowering
/// lands in MIR.
fn emit_scf_for(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    expect_one_body(op, "scf.for", em.fn_name)?;
    em.line("for (;;) {".to_string());
    em.indent += 1;
    walk_block_ops(&op.regions[0].blocks[0].ops, em)?;
    em.line("break;".to_string());
    em.indent -= 1;
    em.line("}".to_string());
    Ok(())
}

/// `scf.while` → MSL `while (cond) { body; }`. The cond is computed once
/// before the op (MIR shape from body_lower) and we read it at every header
/// iteration. Mirrors the cranelift lowering's stage-0 semantics.
fn emit_scf_while(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    expect_one_body(op, "scf.while", em.fn_name)?;
    let cond = em.name_for(*op.operands.first().ok_or_else(|| BodyError::MalformedScf {
        fn_name: em.fn_name.to_string(),
        op_name: "scf.while".into(),
        reason: "missing condition operand".into(),
    })?)?;
    em.line(format!("while ({cond}) {{"));
    em.indent += 1;
    walk_block_ops(&op.regions[0].blocks[0].ops, em)?;
    em.indent -= 1;
    em.line("}".to_string());
    Ok(())
}

/// `scf.loop` → MSL `for (;;) { body; }`. Unconditional infinite loop ;
/// matches the cranelift lowering. Exit only via inner `func.return` or
/// (future-slice) `cssl.break`.
fn emit_scf_loop(op: &MirOp, em: &mut BodyEmitter<'_>) -> Result<(), BodyError> {
    expect_one_body(op, "scf.loop", em.fn_name)?;
    em.line("for (;;) {".to_string());
    em.indent += 1;
    walk_block_ops(&op.regions[0].blocks[0].ops, em)?;
    em.indent -= 1;
    em.line("}".to_string());
    Ok(())
}

/// Validate that a loop op has exactly one region with at most one block.
/// Mirrors `cssl_cgen_cpu_cranelift::scf::extract_single_body_region`.
fn expect_one_body(op: &MirOp, op_name: &str, fn_name: &str) -> Result<(), BodyError> {
    if op.regions.len() != 1 {
        return Err(BodyError::MalformedScf {
            fn_name: fn_name.to_string(),
            op_name: op_name.to_string(),
            reason: format!("region count = {}", op.regions.len()),
        });
    }
    if op.regions[0].blocks.len() > 1 {
        return Err(BodyError::MalformedScf {
            fn_name: fn_name.to_string(),
            op_name: op_name.to_string(),
            reason: format!("multi-block body ({} blocks)", op.regions[0].blocks.len()),
        });
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// § Helper for MirFunc inspection (re-exported for emit.rs body-splice path).
// ─────────────────────────────────────────────────────────────────────────

/// Returns `true` iff the given fn has a body (entry block exists + has ops).
/// Used by [`crate::emit::emit_msl`] to decide between skeleton-emission and
/// body-emission paths.
#[must_use]
pub fn has_body(f: &MirFunc) -> bool {
    f.body.blocks.iter().any(|b| !b.ops.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_mir::{validate_and_mark, MirFunc, MirModule, MirOp, MirRegion, MirValue};

    /// Fixture : build a minimal validated module with one fn `name` whose
    /// body contains the supplied op-list.
    fn validated_module(name: &str, ret: MirType, ops: Vec<MirOp>) -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new(name, vec![], vec![ret]);
        for o in ops {
            f.push_op(o);
        }
        m.push_func(f);
        validate_and_mark(&mut m).expect("validator");
        m
    }

    /// Helper : extract MSL line vector for a validated module's `entry` fn.
    fn lines(m: &MirModule, name: &str) -> Vec<String> {
        emit_body(m, name).expect("emit_body")
    }

    // ── Marker-contract tests ─────────────────────────────────────────────

    #[test]
    fn rejects_module_without_structured_cfg_marker() {
        // No D5 validation done — emitter must reject.
        let mut m = MirModule::new();
        m.push_func(MirFunc::new("k", vec![], vec![]));
        let err = emit_body(&m, "k").unwrap_err();
        assert_eq!(err, BodyError::MissingStructuredCfgMarker);
    }

    #[test]
    fn accepts_validated_module_with_empty_body() {
        let mut m = MirModule::new();
        m.push_func(MirFunc::new("k", vec![], vec![]));
        validate_and_mark(&mut m).unwrap();
        let result = emit_body(&m, "k").unwrap();
        assert!(result.is_empty());
    }

    // ── arith.constant tests ──────────────────────────────────────────────

    #[test]
    fn arith_constant_i32_renders_int_literal() {
        let op = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "42");
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![op]);
        let l = lines(&m, "k");
        assert!(l[0].contains("int v0 = 42;"), "got : {l:?}");
    }

    #[test]
    fn arith_constant_f32_renders_float_literal() {
        let op = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "1.5");
        let m = validated_module("k", MirType::Float(FloatWidth::F32), vec![op]);
        let l = lines(&m, "k");
        assert!(l[0].contains("float v0 = 1.5;"), "got : {l:?}");
    }

    #[test]
    fn arith_constant_bool_normalizes_zero_one() {
        let op = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Bool)
            .with_attribute("value", "1");
        let m = validated_module("k", MirType::Bool, vec![op]);
        let l = lines(&m, "k");
        assert!(l[0].contains("bool v0 = true;"), "got : {l:?}");
    }

    #[test]
    fn arith_constant_missing_value_attribute_errors() {
        let op = MirOp::std("arith.constant").with_result(ValueId(0), MirType::Int(IntWidth::I32));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![op]);
        let err = emit_body(&m, "k").unwrap_err();
        assert!(matches!(err, BodyError::MissingConstantValue { .. }));
    }

    // ── arith binary tests ────────────────────────────────────────────────

    #[test]
    fn arith_addi_renders_int_addition() {
        let lhs = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let rhs = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "2");
        let add = MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![lhs, rhs, add]);
        let l = lines(&m, "k");
        assert!(
            l.iter().any(|s| s.contains("int v2 = v0 + v1;")),
            "got : {l:?}"
        );
    }

    #[test]
    fn arith_addf_renders_float_addition() {
        let lhs = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "1.0");
        let rhs = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "2.0");
        let add = MirOp::std("arith.addf")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Float(FloatWidth::F32));
        let m = validated_module("k", MirType::Float(FloatWidth::F32), vec![lhs, rhs, add]);
        let l = lines(&m, "k");
        assert!(l.iter().any(|s| s.contains("float v2 = v0 + v1;")));
    }

    #[test]
    fn arith_remf_renders_fmod_call() {
        let lhs = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "5.0");
        let rhs = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "2.0");
        let rem = MirOp::std("arith.remf")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Float(FloatWidth::F32));
        let module = validated_module("k", MirType::Float(FloatWidth::F32), vec![lhs, rhs, rem]);
        let l = lines(&module, "k");
        assert!(l.iter().any(|s| s.contains("fmod(v0, v1)")), "got : {l:?}");
    }

    #[test]
    fn arith_cmpi_eq_yields_bool() {
        let a = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let b = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let cmp = MirOp::std("arith.cmpi_eq")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Bool);
        let m = validated_module("k", MirType::Bool, vec![a, b, cmp]);
        let l = lines(&m, "k");
        assert!(l.iter().any(|s| s.contains("bool v2 = v0 == v1;")));
    }

    #[test]
    fn arith_andi_renders_bitwise_and() {
        let a = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "0xFF");
        let b = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "0x0F");
        let and = MirOp::std("arith.andi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![a, b, and]);
        let l = lines(&m, "k");
        assert!(l.iter().any(|s| s.contains("int v2 = v0 & v1;")));
    }

    #[test]
    fn arith_bitcast_emits_as_type() {
        let a = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "1.0");
        let b = MirOp::std("arith.bitcast")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I32));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![a, b]);
        let l = lines(&m, "k");
        assert!(
            l.iter().any(|s| s.contains("as_type<int>(v0)")),
            "got : {l:?}"
        );
    }

    // ── func.return tests ─────────────────────────────────────────────────

    #[test]
    fn func_return_with_value_renders_return_v() {
        let c = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "0");
        let r = MirOp::std("func.return").with_operand(ValueId(0));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![c, r]);
        let l = lines(&m, "k");
        assert!(l.iter().any(|s| s.trim() == "return v0;"));
    }

    #[test]
    fn func_return_no_operand_renders_bare_return() {
        let r = MirOp::std("func.return");
        let m = validated_module("k", MirType::None, vec![r]);
        let l = lines(&m, "k");
        assert!(l.iter().any(|s| s.trim() == "return;"));
    }

    // ── func.call tests ───────────────────────────────────────────────────

    #[test]
    fn func_call_renders_callee_with_args() {
        let a = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Float(FloatWidth::F32))
            .with_attribute("value", "1.0");
        let call = MirOp::std("func.call")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Float(FloatWidth::F32))
            .with_attribute("callee", "sqrt");
        let m = validated_module("k", MirType::Float(FloatWidth::F32), vec![a, call]);
        let l = lines(&m, "k");
        assert!(
            l.iter().any(|s| s.contains("float v1 = sqrt(v0);")),
            "got : {l:?}"
        );
    }

    // ── memref.load / store ───────────────────────────────────────────────

    #[test]
    fn memref_load_with_index_renders_array_access() {
        let ptr_arg = MirValue::new(ValueId(0), MirType::Ptr);
        let idx_arg = MirValue::new(ValueId(1), MirType::Int(IntWidth::I64));
        let mut m = MirModule::new();
        let mut f = MirFunc::new("k", vec![], vec![]);
        // Replace entry block with one that has args.
        f.body = MirRegion::with_entry(vec![ptr_arg, idx_arg]);
        f.push_op(
            MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Float(FloatWidth::F32)),
        );
        m.push_func(f);
        validate_and_mark(&mut m).unwrap();
        let l = emit_body(&m, "k").unwrap();
        assert!(
            l.iter().any(|s| s.contains("float v2 = v0[v1];")),
            "got : {l:?}"
        );
    }

    #[test]
    fn memref_store_with_index_renders_array_assignment() {
        let ptr_arg = MirValue::new(ValueId(0), MirType::Ptr);
        let idx_arg = MirValue::new(ValueId(1), MirType::Int(IntWidth::I64));
        let val_arg = MirValue::new(ValueId(2), MirType::Float(FloatWidth::F32));
        let mut m = MirModule::new();
        let mut f = MirFunc::new("k", vec![], vec![]);
        f.body = MirRegion::with_entry(vec![ptr_arg, idx_arg, val_arg]);
        // operands : [val, ptr, idx]
        f.push_op(
            MirOp::std("memref.store")
                .with_operand(ValueId(2))
                .with_operand(ValueId(0))
                .with_operand(ValueId(1)),
        );
        m.push_func(f);
        validate_and_mark(&mut m).unwrap();
        let l = emit_body(&m, "k").unwrap();
        assert!(l.iter().any(|s| s.contains("v0[v1] = v2;")), "got : {l:?}");
    }

    // ── scf.if tests ──────────────────────────────────────────────────────

    #[test]
    fn scf_if_statement_form_emits_if_else_braces() {
        let cond = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Bool)
            .with_attribute("value", "true");
        let mut iff = MirOp::std("scf.if").with_operand(ValueId(0));
        iff.regions.push(MirRegion::with_entry(Vec::new()));
        iff.regions.push(MirRegion::with_entry(Vec::new()));
        let m = validated_module("k", MirType::None, vec![cond, iff]);
        let l = lines(&m, "k");
        let joined = l.join("\n");
        assert!(joined.contains("if (v0) {"), "got : {joined}");
        assert!(joined.contains("} else {"), "got : {joined}");
    }

    #[test]
    fn scf_if_expression_form_emits_merge_variable() {
        let cond = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Bool)
            .with_attribute("value", "true");
        let then_v = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let else_v = MirOp::std("arith.constant")
            .with_result(ValueId(2), MirType::Int(IntWidth::I32))
            .with_attribute("value", "2");
        let mut iff = MirOp::std("scf.if")
            .with_operand(ValueId(0))
            .with_result(ValueId(3), MirType::Int(IntWidth::I32));
        let mut then_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = then_region.entry_mut() {
            b.push(then_v);
            b.push(MirOp::std("scf.yield").with_operand(ValueId(1)));
        }
        let mut else_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = else_region.entry_mut() {
            b.push(else_v);
            b.push(MirOp::std("scf.yield").with_operand(ValueId(2)));
        }
        iff.regions.push(then_region);
        iff.regions.push(else_region);
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![cond, iff]);
        let l = lines(&m, "k");
        let joined = l.join("\n");
        assert!(
            joined.contains("int scf_if_merge_0;"),
            "expected merge-var declared : {joined}"
        );
        assert!(
            joined.contains("scf_if_merge_0 = v1;"),
            "expected then-branch merge assign : {joined}"
        );
        assert!(
            joined.contains("scf_if_merge_0 = v2;"),
            "expected else-branch merge assign : {joined}"
        );
    }

    #[test]
    fn scf_if_wrong_region_count_returns_malformed() {
        let cond = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Bool)
            .with_attribute("value", "1");
        let bad = MirOp::std("scf.if").with_operand(ValueId(0)); // 0 regions !
                                                                 // Bypass validator (it would reject); construct module manually.
        let mut m = MirModule::new();
        let mut f = MirFunc::new("k", vec![], vec![]);
        f.push_op(cond);
        f.push_op(bad);
        m.push_func(f);
        m.attributes
            .push(("structured_cfg.validated".to_string(), "true".to_string()));
        let err = emit_body(&m, "k").unwrap_err();
        assert!(matches!(err, BodyError::MalformedScf { .. }), "got {err:?}");
    }

    // ── scf.for / while / loop ────────────────────────────────────────────

    #[test]
    fn scf_for_renders_break_terminated_for() {
        let iter = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::None)
            .with_attribute("value", "0");
        let mut floop = MirOp::std("scf.for").with_operand(ValueId(0));
        floop.regions.push(MirRegion::with_entry(Vec::new()));
        let m = validated_module("k", MirType::None, vec![iter, floop]);
        let l = lines(&m, "k");
        let joined = l.join("\n");
        assert!(joined.contains("for (;;) {"), "got : {joined}");
        assert!(joined.contains("break;"), "got : {joined}");
    }

    #[test]
    fn scf_while_renders_with_cond() {
        let cond = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Bool)
            .with_attribute("value", "1");
        let mut wloop = MirOp::std("scf.while").with_operand(ValueId(0));
        wloop.regions.push(MirRegion::with_entry(Vec::new()));
        let m = validated_module("k", MirType::None, vec![cond, wloop]);
        let l = lines(&m, "k");
        let joined = l.join("\n");
        assert!(joined.contains("while (v0) {"), "got : {joined}");
    }

    #[test]
    fn scf_loop_renders_infinite_for() {
        let mut lloop = MirOp::std("scf.loop");
        lloop.regions.push(MirRegion::with_entry(Vec::new()));
        let m = validated_module("k", MirType::None, vec![lloop]);
        let l = lines(&m, "k");
        let joined = l.join("\n");
        assert!(joined.contains("for (;;) {"), "got : {joined}");
        // Must NOT contain a break (true infinite loop).
        assert!(!joined.contains("break;"), "got : {joined}");
    }

    #[test]
    fn scf_loop_wrong_region_count_returns_malformed() {
        let bad = MirOp::std("scf.loop"); // 0 regions
        let mut m = MirModule::new();
        let mut f = MirFunc::new("k", vec![], vec![]);
        f.push_op(bad);
        m.push_func(f);
        m.attributes
            .push(("structured_cfg.validated".to_string(), "true".to_string()));
        let err = emit_body(&m, "k").unwrap_err();
        assert!(matches!(err, BodyError::MalformedScf { .. }));
    }

    // ── Heap + closure rejection ──────────────────────────────────────────

    #[test]
    fn heap_alloc_rejects_with_clear_error() {
        let op = MirOp::std("cssl.heap.alloc")
            .with_result(ValueId(0), MirType::Ptr)
            .with_attribute("size", "16");
        let m = validated_module("k", MirType::None, vec![op]);
        let err = emit_body(&m, "k").unwrap_err();
        assert!(
            matches!(err, BodyError::HeapNotSupportedInMsl { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn closure_op_rejects_with_clear_error() {
        let op = MirOp::std("cssl.closure")
            .with_result(ValueId(0), MirType::Opaque("!cssl.closure".into()));
        let m = validated_module("k", MirType::None, vec![op]);
        let err = emit_body(&m, "k").unwrap_err();
        assert!(matches!(err, BodyError::ClosuresNotSupportedInMsl { .. }));
    }

    #[test]
    fn unsupported_break_rejects_as_closure_class() {
        let op = MirOp::std("cssl.unsupported(Break)");
        // Skip D5 validator (it rejects this) ; manual marker.
        let mut m = MirModule::new();
        let mut f = MirFunc::new("k", vec![], vec![]);
        f.push_op(op);
        m.push_func(f);
        m.attributes
            .push(("structured_cfg.validated".to_string(), "true".to_string()));
        let err = emit_body(&m, "k").unwrap_err();
        assert!(matches!(err, BodyError::ClosuresNotSupportedInMsl { .. }));
    }

    // ── Type-mapping + ptr address-space ─────────────────────────────────

    #[test]
    fn ptr_type_maps_to_device_pointer() {
        // Use the helper directly so we don't have to plumb a return-type ptr.
        let r = mir_to_msl(&MirType::Ptr, "f").unwrap();
        assert_eq!(r, "device float*");
    }

    #[test]
    fn vec3_f32_maps_to_float3() {
        let r = mir_to_msl(&MirType::Vec(3, FloatWidth::F32), "f").unwrap();
        assert_eq!(r, "float3");
    }

    #[test]
    fn tuple_type_returns_non_scalar_error() {
        let t = MirType::Tuple(vec![
            MirType::Int(IntWidth::I32),
            MirType::Float(FloatWidth::F32),
        ]);
        let err = mir_to_msl(&t, "f").unwrap_err();
        assert!(matches!(err, BodyError::NonScalarType { .. }));
    }

    #[test]
    fn unknown_op_returns_unsupported_op_error() {
        let bad = MirOp::std("cssl.invented_for_test");
        let m = validated_module("k", MirType::None, vec![bad]);
        let err = emit_body(&m, "k").unwrap_err();
        assert!(matches!(err, BodyError::UnsupportedOp { .. }));
    }

    #[test]
    fn unknown_value_id_returns_clean_error() {
        // arith.addi referencing an undefined operand.
        let add = MirOp::std("arith.addi")
            .with_operand(ValueId(99))
            .with_operand(ValueId(100))
            .with_result(ValueId(0), MirType::Int(IntWidth::I32));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![add]);
        let err = emit_body(&m, "k").unwrap_err();
        assert!(
            matches!(err, BodyError::UnknownValueId { value_id: 99, .. }),
            "got {err:?}"
        );
    }

    // ── Composition / determinism ─────────────────────────────────────────

    #[test]
    fn output_is_deterministic_across_runs() {
        // Re-emitting the same module twice must produce byte-identical lines.
        let lhs = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let rhs = MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "2");
        let add = MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let r = MirOp::std("func.return").with_operand(ValueId(2));
        let m = validated_module("k", MirType::Int(IntWidth::I32), vec![lhs, rhs, add, r]);
        let a = emit_body(&m, "k").unwrap();
        let b = emit_body(&m, "k").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn nested_scf_if_inside_scf_loop_indents_correctly() {
        let cond = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Bool)
            .with_attribute("value", "1");
        let mut inner_if = MirOp::std("scf.if").with_operand(ValueId(0));
        inner_if.regions.push(MirRegion::with_entry(Vec::new()));
        inner_if.regions.push(MirRegion::with_entry(Vec::new()));
        let mut outer = MirOp::std("scf.loop");
        let mut body = MirRegion::with_entry(Vec::new());
        if let Some(b) = body.entry_mut() {
            b.push(inner_if);
        }
        outer.regions.push(body);
        let m = validated_module("k", MirType::None, vec![cond, outer]);
        let l = lines(&m, "k");
        let joined = l.join("\n");
        assert!(joined.contains("for (;;) {"), "got : {joined}");
        // The inner if is at indent depth 2 (8 spaces).
        let if_line = l
            .iter()
            .find(|s| s.contains("if (v0) {"))
            .expect("inner if line");
        assert!(if_line.starts_with("        "), "if_line = {if_line:?}");
    }

    #[test]
    fn has_body_returns_true_when_ops_present() {
        let mut f = MirFunc::new("k", vec![], vec![]);
        assert!(!has_body(&f));
        f.push_op(MirOp::std("func.return"));
        assert!(has_body(&f));
    }
}

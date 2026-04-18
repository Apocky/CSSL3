// T11-D20 : real Cranelift JIT execution requires fn-ptr casts.
// The unsafe uses are narrowly scoped + documented with SAFETY comments.
#![allow(unsafe_code)]

//! Real JIT execution : MIR → machine-code → in-process call.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § stage-0 throwaway.
//! § ROLE : turn a [`MirFunc`] into a callable fn-pointer via in-process JIT.
//!          This is the **stage-0.5 bridge to stage-1 self-host** : CSSLv3
//!          programs now execute.
//!
//! § STATUS (T11-D20 / this commit)
//!   **Real Cranelift integration**. `JitModule::compile` builds cranelift IR
//!   from MIR, `JitModule::finalize` JITs + links the functions, and
//!   `JitFn::call_*` invokes the compiled machine code via fn-pointer casts.
//!
//! § CANONICAL EXAMPLE
//!
//! ```ignore
//! // Build a MIR `fn add(v0: i32, v1: i32) -> i32 { v0 + v1 }`.
//! let mut m = JitModule::new();
//! let handle = m.compile(&primal).unwrap();
//! m.finalize().unwrap();
//! let result = handle.call_i64_i64_to_i64(3, 4, &m).unwrap();
//! assert_eq!(result, 7);
//! ```
//!
//! § USAGE MODEL
//!   1. `JitModule::new()` — create a fresh module backed by an in-process
//!      Cranelift JIT.
//!   2. `compile(&MirFunc)` — declare + define the fn in the cranelift module.
//!      Returns a lightweight [`JitFn`] handle.
//!   3. `finalize()` — JIT-compile all declared fns + populate their code
//!      addresses. After finalize, no more fns can be added to this module.
//!   4. `JitFn::call_*` — invoke the compiled fn via fn-ptr cast.
//!
//! § SUPPORTED MIR OPS
//!   - `arith.constant` (i32, i64, f32, f64)
//!   - `arith.addi` / `arith.subi` / `arith.muli`
//!   - `arith.addf` / `arith.subf` / `arith.mulf` / `arith.divf` / `arith.negf`
//!   - `func.return`
//!   Other ops return [`JitError::UnsupportedMirOp`] at compile-time.
//!
//! § T11-D21+ DEFERRED
//!   - Control flow : `scf.if` / `scf.for` → CLIF blocks + brif.
//!   - Comparisons + select : `arith.cmpf` / `arith.select` (already lowered to
//!     CLIF text in T11-D18 ; JIT activation needs same path).
//!   - Memref load/store.
//!   - Multi-return tuple fns.
//!   - Min/Max/Abs/Sign direct JIT (currently only their FAdd/FSub/FMul/FDiv/
//!     FNeg primitive-chain paths are JIT-executable).

use std::collections::HashMap;

use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule as ClJitModule};
use cranelift_module::{FuncId, Linkage, Module};
use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirOp, MirType, ValueId};
use thiserror::Error;

/// JIT compilation + execution error surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum JitError {
    /// MIR fn has a feature the stage-0 JIT does not support.
    #[error("unsupported MIR feature in `{fn_name}` : {reason}")]
    UnsupportedFeature { fn_name: String, reason: String },
    /// A MIR op is not yet JIT-lowered.
    #[error("unsupported MIR op in `{fn_name}` : `{op_name}` (stage-0 JIT scalars-arith-only)")]
    UnsupportedMirOp { fn_name: String, op_name: String },
    /// Cranelift reported a lowering / codegen error.
    #[error("cranelift lowering failed for `{fn_name}` : {detail}")]
    LoweringFailed { fn_name: String, detail: String },
    /// Requested fn-name not present in the JIT module.
    #[error("no such JIT-compiled fn : `{name}`")]
    UnknownFunction { name: String },
    /// Tried to compile after finalize.
    #[error("JIT module already finalized ; create a new module to compile more fns")]
    AlreadyFinalized,
    /// Tried to call before finalize.
    #[error("JIT module must be finalized before calling compiled fns")]
    NotFinalized,
    /// Tried to call with the wrong signature.
    #[error("fn `{name}` signature mismatch : expected {expected}, got {actual}")]
    SignatureMismatch {
        name: String,
        expected: String,
        actual: String,
    },
}

/// Lightweight handle to a JIT-compiled function. Holds the metadata needed
/// to validate calls + the fn-ptr address (populated on [`JitModule::finalize`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitFn {
    /// Primal fn name (from MIR).
    pub name: String,
    /// Number of params.
    pub param_count: usize,
    /// Whether the fn has a single scalar result.
    pub has_result: bool,
    /// The MIR param types — used to validate `call_*` method signatures.
    pub param_types: Vec<MirType>,
    /// The MIR result type, if any.
    pub result_type: Option<MirType>,
}

impl JitFn {
    /// Call as `fn(i64, i64) -> i64`. Validates the MIR signature matches.
    ///
    /// # Errors
    /// Returns [`JitError::SignatureMismatch`] if the param/result types are
    /// not both `i64`. [`JitError::NotFinalized`] if the module hasn't been
    /// finalized. [`JitError::UnknownFunction`] if the fn isn't in `module`.
    pub fn call_i64_i64_to_i64(&self, a: i64, b: i64, module: &JitModule) -> Result<i64, JitError> {
        self.check_sig(
            &[MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I64)],
            MirType::Int(IntWidth::I64),
        )?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: `addr` was produced by `cranelift_jit::JITModule::
        // get_finalized_function` which returns a pointer into the JIT
        // module's executable memory. The module is kept alive as long as
        // `module: &JitModule` is borrowed. The MIR signature check above
        // confirmed the compiled fn has shape `(i64, i64) -> i64`, matching
        // the fn-pointer type we're transmuting to.
        let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(addr) };
        Ok(f(a, b))
    }

    /// Call as `fn(i32, i32) -> i32`. See [`Self::call_i64_i64_to_i64`] for safety rationale.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_i32_i32_to_i32(&self, a: i32, b: i32, module: &JitModule) -> Result<i32, JitError> {
        self.check_sig(
            &[MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)],
            MirType::Int(IntWidth::I32),
        )?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn(i32, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        Ok(f(a, b))
    }

    /// Call as `fn(f32, f32) -> f32`.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_f32_f32_to_f32(&self, a: f32, b: f32, module: &JitModule) -> Result<f32, JitError> {
        self.check_sig(
            &[
                MirType::Float(FloatWidth::F32),
                MirType::Float(FloatWidth::F32),
            ],
            MirType::Float(FloatWidth::F32),
        )?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn(f32, f32) -> f32 = unsafe { std::mem::transmute(addr) };
        Ok(f(a, b))
    }

    /// Call as `fn() -> i32`.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_unit_to_i32(&self, module: &JitModule) -> Result<i32, JitError> {
        self.check_sig(&[], MirType::Int(IntWidth::I32))?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn() -> i32 = unsafe { std::mem::transmute(addr) };
        Ok(f())
    }

    fn check_sig(
        &self,
        expected_params: &[MirType],
        expected_result: MirType,
    ) -> Result<(), JitError> {
        let actual_sig = format!("{:?} -> {:?}", self.param_types, self.result_type);
        if self.param_types != expected_params
            || self.result_type.as_ref() != Some(&expected_result)
        {
            return Err(JitError::SignatureMismatch {
                name: self.name.clone(),
                expected: format!("{expected_params:?} -> {expected_result:?}"),
                actual: actual_sig,
            });
        }
        Ok(())
    }
}

/// A JIT-compiled module holding one-or-more [`JitFn`]s. Owns the Cranelift
/// JITModule (keeping compiled machine-code alive).
pub struct JitModule {
    inner: Option<ClJitModule>,
    builder_ctx: FunctionBuilderContext,
    codegen_ctx: Context,
    /// fn-name → (FuncId, code-addr-after-finalize).
    fn_table: HashMap<String, (FuncId, Option<*const u8>)>,
    /// Metadata handles for each compiled fn (mirrors fn_table keys).
    handles: Vec<JitFn>,
    finalized: bool,
}

impl core::fmt::Debug for JitModule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("JitModule")
            .field("fn_count", &self.handles.len())
            .field("finalized", &self.finalized)
            .finish()
    }
}

impl JitModule {
    /// New empty JIT module backed by a Cranelift in-process JIT.
    ///
    /// # Panics
    /// Panics if Cranelift cannot build an ISA for the host (shouldn't happen
    /// on any supported target).
    #[must_use]
    pub fn new() -> Self {
        let mut flag_builder = settings::builder();
        // Enable position-independent code (required on some platforms).
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("cranelift native isa unavailable : {msg}");
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let inner = ClJitModule::new(builder);
        Self {
            inner: Some(inner),
            builder_ctx: FunctionBuilderContext::new(),
            codegen_ctx: Context::new(),
            fn_table: HashMap::new(),
            handles: Vec::new(),
            finalized: false,
        }
    }

    /// Compile a [`MirFunc`] into the JIT module. Returns a handle ; the fn
    /// is not callable until [`Self::finalize`] is called.
    ///
    /// # Errors
    /// Returns [`JitError::AlreadyFinalized`] if called after finalize.
    /// [`JitError::UnsupportedFeature`] for multi-result fns.
    /// [`JitError::UnsupportedMirOp`] for op-names stage-0 doesn't JIT.
    /// [`JitError::LoweringFailed`] on Cranelift codegen errors.
    pub fn compile(&mut self, primal: &MirFunc) -> Result<JitFn, JitError> {
        if self.finalized {
            return Err(JitError::AlreadyFinalized);
        }
        if primal.results.len() > 1 {
            return Err(JitError::UnsupportedFeature {
                fn_name: primal.name.clone(),
                reason: format!(
                    "{} results ; stage-0 JIT supports ≤ 1",
                    primal.results.len()
                ),
            });
        }

        let Some(module) = self.inner.as_mut() else {
            return Err(JitError::AlreadyFinalized);
        };

        // Build the cranelift Signature from MIR param/result types.
        // Use the host ISA's default calling-convention — on Windows this is
        // WindowsFastcall, on Linux/macOS it's SystemV. Rust `extern "C"` on
        // each target matches this same convention, so fn-ptr casts work.
        let call_conv = module.isa().default_call_conv();
        let mut sig = Signature::new(call_conv);
        for (idx, p_ty) in primal.params.iter().enumerate() {
            let cl_ty = mir_to_cl_type(p_ty).ok_or_else(|| JitError::UnsupportedFeature {
                fn_name: primal.name.clone(),
                reason: format!("param #{idx} type `{p_ty}` not scalar-JIT-able"),
            })?;
            sig.params.push(AbiParam::new(cl_ty));
        }
        for (idx, r_ty) in primal.results.iter().enumerate() {
            let cl_ty = mir_to_cl_type(r_ty).ok_or_else(|| JitError::UnsupportedFeature {
                fn_name: primal.name.clone(),
                reason: format!("result #{idx} type `{r_ty}` not scalar-JIT-able"),
            })?;
            sig.returns.push(AbiParam::new(cl_ty));
        }

        // Declare + define the fn in the cranelift JIT module.
        let func_id = module
            .declare_function(&primal.name, Linkage::Export, &sig)
            .map_err(|e| JitError::LoweringFailed {
                fn_name: primal.name.clone(),
                detail: format!("declare_function : {e}"),
            })?;

        self.codegen_ctx.clear();
        self.codegen_ctx.func.signature = sig.clone();
        self.codegen_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

        // Build the function body from MIR ops.
        {
            let mut builder =
                FunctionBuilder::new(&mut self.codegen_ctx.func, &mut self.builder_ctx);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);

            // MIR ValueId → cranelift Value map.
            let mut value_map: HashMap<ValueId, cranelift_codegen::ir::Value> = HashMap::new();
            // Wire block-args to MIR param ValueIds (ValueId(0) → block_param[0], etc.).
            let block_params: Vec<_> = builder.block_params(entry).to_vec();
            for (idx, &bp) in block_params.iter().enumerate() {
                value_map.insert(ValueId(idx as u32), bp);
            }

            let Some(entry_block) = primal.body.blocks.first() else {
                return Err(JitError::UnsupportedFeature {
                    fn_name: primal.name.clone(),
                    reason: "empty body (no blocks)".to_string(),
                });
            };

            let mut saw_return = false;
            for op in &entry_block.ops {
                if lower_op_to_cl(op, &mut builder, &mut value_map, &primal.name)? {
                    saw_return = true;
                }
            }

            if !saw_return {
                // No explicit return ; emit a default return based on result type.
                if primal.results.is_empty() {
                    builder.ins().return_(&[]);
                } else {
                    return Err(JitError::UnsupportedFeature {
                        fn_name: primal.name.clone(),
                        reason: "fn has result but body has no func.return".to_string(),
                    });
                }
            }

            builder.finalize();
        }

        // Define the function in the module.
        module
            .define_function(func_id, &mut self.codegen_ctx)
            .map_err(|e| JitError::LoweringFailed {
                fn_name: primal.name.clone(),
                detail: format!("define_function : {e}"),
            })?;

        let handle = JitFn {
            name: primal.name.clone(),
            param_count: primal.params.len(),
            has_result: !primal.results.is_empty(),
            param_types: primal.params.clone(),
            result_type: primal.results.first().cloned(),
        };
        self.fn_table.insert(primal.name.clone(), (func_id, None));
        self.handles.push(handle.clone());
        Ok(handle)
    }

    /// Finalize the JIT module : compile all declared fns to machine code +
    /// link them. After this, calls are executable via `JitFn::call_*`.
    ///
    /// # Errors
    /// Returns [`JitError::LoweringFailed`] if Cranelift fails finalization.
    pub fn finalize(&mut self) -> Result<(), JitError> {
        if self.finalized {
            return Ok(());
        }
        let Some(module) = self.inner.as_mut() else {
            return Err(JitError::AlreadyFinalized);
        };
        module
            .finalize_definitions()
            .map_err(|e| JitError::LoweringFailed {
                fn_name: "<module>".to_string(),
                detail: format!("finalize_definitions : {e}"),
            })?;
        // Populate code-addrs for each compiled fn.
        let names: Vec<String> = self.fn_table.keys().cloned().collect();
        for name in names {
            let (func_id, _) = self.fn_table[&name];
            let addr = module.get_finalized_function(func_id);
            self.fn_table.insert(name, (func_id, Some(addr)));
        }
        self.finalized = true;
        Ok(())
    }

    /// Look up a compiled fn's handle by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&JitFn> {
        self.handles.iter().find(|f| f.name == name)
    }

    /// Number of compiled fns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    /// `true` iff no fns compiled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// `true` iff the module has been finalized (fns callable).
    #[must_use]
    pub const fn is_finalized(&self) -> bool {
        self.finalized
    }

    /// Whether the JIT backend is activated (real Cranelift wired in).
    /// T11-D20 : always `true`.
    #[must_use]
    pub const fn is_activated() -> bool {
        true
    }

    /// Look up the raw code-address for a fn. Internal helper used by
    /// `JitFn::call_*` methods.
    fn code_addr_for(&self, name: &str) -> Result<*const u8, JitError> {
        if !self.finalized {
            return Err(JitError::NotFinalized);
        }
        let entry = self
            .fn_table
            .get(name)
            .ok_or_else(|| JitError::UnknownFunction {
                name: name.to_string(),
            })?;
        entry.1.ok_or(JitError::NotFinalized)
    }
}

impl Default for JitModule {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Type + Op lowering helpers.
// ─────────────────────────────────────────────────────────────────────────

/// Map a MIR scalar type to the Cranelift `ir::Type`.
fn mir_to_cl_type(mir: &MirType) -> Option<cranelift_codegen::ir::Type> {
    match mir {
        MirType::Int(w) => Some(match w {
            IntWidth::I1 => cl_types::I8, // cranelift has no b1 param
            IntWidth::I8 => cl_types::I8,
            IntWidth::I16 => cl_types::I16,
            IntWidth::I32 => cl_types::I32,
            IntWidth::I64 | IntWidth::Index => cl_types::I64,
        }),
        MirType::Float(w) => Some(match w {
            FloatWidth::F16 | FloatWidth::Bf16 => return None, // not yet in stable CLIF
            FloatWidth::F32 => cl_types::F32,
            FloatWidth::F64 => cl_types::F64,
        }),
        MirType::Bool => Some(cl_types::I8),
        _ => None,
    }
}

/// Lower a single MIR op into the cranelift function being built. Returns
/// `Ok(true)` if the op was a terminator (`func.return`), else `Ok(false)`.
#[allow(clippy::too_many_lines)]
fn lower_op_to_cl(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    match op.name.as_str() {
        "arith.constant" => {
            let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
                fn_name: fn_name.to_string(),
                detail: "arith.constant with no result".to_string(),
            })?;
            let value_str = op
                .attributes
                .iter()
                .find(|(k, _)| k == "value")
                .map_or("0", |(_, v)| v.as_str());
            let cl_ty = mir_to_cl_type(&r.ty).ok_or_else(|| JitError::UnsupportedFeature {
                fn_name: fn_name.to_string(),
                reason: format!("const result type `{}` not scalar", r.ty),
            })?;
            let v = if cl_ty == cl_types::F32 {
                let f: f32 = value_str.parse().unwrap_or(0.0);
                builder.ins().f32const(f)
            } else if cl_ty == cl_types::F64 {
                let f: f64 = value_str.parse().unwrap_or(0.0);
                builder.ins().f64const(f)
            } else {
                let i: i64 = value_str.parse().unwrap_or(0);
                builder.ins().iconst(cl_ty, i)
            };
            value_map.insert(r.id, v);
            Ok(false)
        }
        "arith.addi" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().iadd(a, c)
        }),
        "arith.subi" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().isub(a, c)
        }),
        "arith.muli" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().imul(a, c)
        }),
        "arith.addf" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fadd(a, c)
        }),
        "arith.subf" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fsub(a, c)
        }),
        "arith.mulf" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fmul(a, c)
        }),
        "arith.divf" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fdiv(a, c)
        }),
        "arith.negf" => emit_unary(op, builder, value_map, fn_name, |b, a| b.ins().fneg(a)),
        "func.return" => {
            let args: Result<Vec<_>, _> = op
                .operands
                .iter()
                .map(|vid| {
                    value_map
                        .get(vid)
                        .copied()
                        .ok_or_else(|| JitError::LoweringFailed {
                            fn_name: fn_name.to_string(),
                            detail: format!("func.return references unknown ValueId({})", vid.0),
                        })
                })
                .collect();
            builder.ins().return_(&args?);
            Ok(true)
        }
        other => Err(JitError::UnsupportedMirOp {
            fn_name: fn_name.to_string(),
            op_name: other.to_string(),
        }),
    }
}

fn emit_binary<F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    emit: F,
) -> Result<bool, JitError>
where
    F: FnOnce(
        &mut FunctionBuilder<'_>,
        cranelift_codegen::ir::Value,
        cranelift_codegen::ir::Value,
    ) -> cranelift_codegen::ir::Value,
{
    let (Some(&a_id), Some(&b_id)) = (op.operands.first(), op.operands.get(1)) else {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} expected 2 operands", op.name),
        });
    };
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("{} has no result", op.name),
    })?;
    let a = *value_map
        .get(&a_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("unknown operand ValueId({})", a_id.0),
        })?;
    let b = *value_map
        .get(&b_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("unknown operand ValueId({})", b_id.0),
        })?;
    let v = emit(builder, a, b);
    value_map.insert(r.id, v);
    Ok(false)
}

fn emit_unary<F>(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    emit: F,
) -> Result<bool, JitError>
where
    F: FnOnce(
        &mut FunctionBuilder<'_>,
        cranelift_codegen::ir::Value,
    ) -> cranelift_codegen::ir::Value,
{
    let Some(&a_id) = op.operands.first() else {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("{} expected 1 operand", op.name),
        });
    };
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("{} has no result", op.name),
    })?;
    let a = *value_map
        .get(&a_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("unknown operand ValueId({})", a_id.0),
        })?;
    let v = emit(builder, a);
    value_map.insert(r.id, v);
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::{JitError, JitModule};
    use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirOp, MirType, MirValue, ValueId};

    fn i32_ty() -> MirType {
        MirType::Int(IntWidth::I32)
    }

    fn i64_ty() -> MirType {
        MirType::Int(IntWidth::I64)
    }

    fn f32_ty() -> MirType {
        MirType::Float(FloatWidth::F32)
    }

    /// Hand-build MIR `fn add(v0: i32, v1: i32) -> i32 { v0 + v1 }`.
    fn hand_built_add_i32() -> MirFunc {
        let mut f = MirFunc::new("add_i32", vec![i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().expect("entry block");
            entry.args = vec![
                MirValue::new(ValueId(0), i32_ty()),
                MirValue::new(ValueId(1), i32_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        f
    }

    #[test]
    fn jit_module_is_activated_in_stage_0_5() {
        assert!(JitModule::is_activated());
    }

    #[test]
    fn empty_module_is_empty_not_finalized() {
        let m = JitModule::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert!(!m.is_finalized());
    }

    #[test]
    fn compile_records_handle_before_finalize() {
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_add_i32()).unwrap();
        assert_eq!(h.name, "add_i32");
        assert_eq!(h.param_count, 2);
        assert!(h.has_result);
        assert!(!m.is_finalized());
    }

    #[test]
    fn call_before_finalize_returns_not_finalized() {
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_add_i32()).unwrap();
        let err = h.call_i32_i32_to_i32(3, 4, &m).unwrap_err();
        assert_eq!(err, JitError::NotFinalized);
    }

    #[test]
    fn add_i32_roundtrip_3_plus_4_equals_7() {
        // ═══════════════════════════════════════════════════════════════════
        // § THE STAGE-0.5 KILLER TEST : first CSSLv3-derived program executes.
        // ═══════════════════════════════════════════════════════════════════
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_add_i32()).unwrap();
        m.finalize().unwrap();
        let result = h.call_i32_i32_to_i32(3, 4, &m).unwrap();
        assert_eq!(result, 7);
    }

    #[test]
    fn add_i32_handles_negative_inputs() {
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_add_i32()).unwrap();
        m.finalize().unwrap();
        assert_eq!(h.call_i32_i32_to_i32(-5, 10, &m).unwrap(), 5);
        assert_eq!(
            h.call_i32_i32_to_i32(i32::MAX / 2, i32::MAX / 2, &m)
                .unwrap(),
            i32::MAX - 1
        );
    }

    #[test]
    fn add_i64_roundtrip() {
        let mut f = MirFunc::new("add_i64", vec![i64_ty(), i64_ty()], vec![i64_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), i64_ty()),
                MirValue::new(ValueId(1), i64_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), i64_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        assert_eq!(
            h.call_i64_i64_to_i64(100_000_000_000, 23, &m).unwrap(),
            100_000_000_023
        );
    }

    #[test]
    fn mul_f32_roundtrip() {
        let mut f = MirFunc::new("mul_f32", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()),
                MirValue::new(ValueId(1), f32_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.mulf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        let result = h.call_f32_f32_to_f32(2.5, 4.0, &m).unwrap();
        assert!((result - 10.0).abs() < 1e-6);
    }

    #[test]
    fn const_fn_returning_42() {
        // fn answer() -> i32 { 42 }
        let mut f = MirFunc::new("answer", vec![], vec![i32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(0), i32_ty())
                    .with_attribute("value", "42"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(0)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        assert_eq!(h.call_unit_to_i32(&m).unwrap(), 42);
    }

    #[test]
    fn compile_rejects_multi_result_fn() {
        let primal = MirFunc::new("multi", vec![], vec![i32_ty(), i32_ty()]);
        let mut m = JitModule::new();
        let err = m.compile(&primal).unwrap_err();
        assert!(matches!(err, JitError::UnsupportedFeature { .. }));
    }

    #[test]
    fn compile_rejects_unsupported_mir_op() {
        let mut f = MirFunc::new("weird", vec![i32_ty()], vec![i32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), i32_ty())];
            entry
                .ops
                .push(MirOp::std("cssl.mystery").with_operand(ValueId(0)));
        }
        let mut m = JitModule::new();
        let err = m.compile(&f).unwrap_err();
        assert!(matches!(err, JitError::UnsupportedMirOp { .. }));
    }

    #[test]
    fn compile_after_finalize_errors() {
        let mut m = JitModule::new();
        m.compile(&hand_built_add_i32()).unwrap();
        m.finalize().unwrap();
        let err = m.compile(&hand_built_add_i32()).unwrap_err();
        assert_eq!(err, JitError::AlreadyFinalized);
    }

    #[test]
    fn sig_mismatch_on_wrong_call_arm() {
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_add_i32()).unwrap();
        m.finalize().unwrap();
        // add_i32 has i32-i32 sig ; calling i64-arm should mismatch.
        let err = h.call_i64_i64_to_i64(3, 4, &m).unwrap_err();
        assert!(matches!(err, JitError::SignatureMismatch { .. }));
    }

    #[test]
    fn unknown_function_lookup_errors() {
        let m = JitModule::new();
        // module.get returns None ; the call path through the handle's code_addr_for
        // surfaces UnknownFunction when name isn't registered.
        let fake_handle = super::JitFn {
            name: "ghost".to_string(),
            param_count: 2,
            has_result: true,
            param_types: vec![i32_ty(), i32_ty()],
            result_type: Some(i32_ty()),
        };
        let err = fake_handle.call_i32_i32_to_i32(1, 2, &m).unwrap_err();
        // Module is not finalized — that's the earlier gate.
        assert_eq!(err, JitError::NotFinalized);
    }

    #[test]
    fn module_debug_is_nondestructive() {
        let mut m = JitModule::new();
        m.compile(&hand_built_add_i32()).unwrap();
        let dbg_str = format!("{m:?}");
        assert!(dbg_str.contains("JitModule"));
        assert!(dbg_str.contains("fn_count"));
    }

    #[test]
    fn finalize_is_idempotent() {
        let mut m = JitModule::new();
        m.compile(&hand_built_add_i32()).unwrap();
        m.finalize().unwrap();
        // Second finalize should be a no-op.
        m.finalize().unwrap();
        assert!(m.is_finalized());
    }
}

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
//!   - `arith.cmpf` / `arith.cmpi` with predicate attributes (T11-D21)
//!   - `arith.select` (T11-D21) — enables scene-SDF min/max/abs gradient bodies
//!   - `func.return`
//!   Other ops return [`JitError::UnsupportedMirOp`] at compile-time.
//!
//! § T11-D22+ DEFERRED
//!   - Control flow : `scf.if` / `scf.for` → CLIF blocks + brif.
//!   - Inter-fn calls : `func.call` to other fns in the same JIT module.
//!   - Memref load/store.
//!   - Multi-return tuple fns.
//!   - Scene-SDF runtime-gradient verification : JIT-compile AD walker's fwd
//!     variant of `@differentiable fn scene(a, b) { min(a, b) }` + execute +
//!     compare against central-differences — **closes killer-app at runtime**.

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
    /// Number of params (original MIR param count — does NOT include the
    /// synthetic out-param pointers added for multi-result fns).
    pub param_count: usize,
    /// Whether the fn has a single scalar result.
    pub has_result: bool,
    /// The MIR param types — used to validate `call_*` method signatures.
    pub param_types: Vec<MirType>,
    /// The first MIR result type, if any (legacy single-result API).
    pub result_type: Option<MirType>,
    /// T11-D30 : all MIR result types (for multi-result fns).
    pub all_result_types: Vec<MirType>,
    /// T11-D30 : `true` if the cranelift signature uses out-param pointers
    /// instead of a direct return value. Multi-result fns (2+ results) are
    /// compiled this way.
    pub uses_out_params: bool,
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

    /// Call as `fn(i32) -> i32`. T11-D38 : canonical shape for single-arg
    /// monomorphized identity / unary-integer fns like `id_i32(x : i32)`.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_i32_to_i32(&self, a: i32, module: &JitModule) -> Result<i32, JitError> {
        self.check_sig(&[MirType::Int(IntWidth::I32)], MirType::Int(IntWidth::I32))?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(addr) };
        Ok(f(a))
    }

    /// Call as `fn(f32) -> f32`. Used for single-arg differentiable fns like sqrt/sin/cos.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_f32_to_f32(&self, a: f32, module: &JitModule) -> Result<f32, JitError> {
        self.check_sig(
            &[MirType::Float(FloatWidth::F32)],
            MirType::Float(FloatWidth::F32),
        )?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn(f32) -> f32 = unsafe { std::mem::transmute(addr) };
        Ok(f(a))
    }

    /// Call as `fn(f32, f32, f32) -> f32`. Canonical AD reverse-mode shape
    /// for 2-param primals : `fn f_bwd(a, b, d_y) -> d_x` (single-adjoint
    /// extracted from the multi-result bwd variant).
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_f32_f32_f32_to_f32(
        &self,
        a: f32,
        b: f32,
        c: f32,
        module: &JitModule,
    ) -> Result<f32, JitError> {
        let f32m = || MirType::Float(FloatWidth::F32);
        self.check_sig(&[f32m(), f32m(), f32m()], f32m())?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn(f32, f32, f32) -> f32 = unsafe { std::mem::transmute(addr) };
        Ok(f(a, b, c))
    }

    /// Call as `fn(f32, f32, f32, f32) -> f32`. The canonical shape of an
    /// AD forward-mode tangent body for a 2-argument scalar primal :
    /// `fn f_fwd(a, b, d_a, d_b) -> d_y`.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_f32_f32_f32_f32_to_f32(
        &self,
        a: f32,
        b: f32,
        d_a: f32,
        d_b: f32,
        module: &JitModule,
    ) -> Result<f32, JitError> {
        let f32m = || MirType::Float(FloatWidth::F32);
        self.check_sig(&[f32m(), f32m(), f32m(), f32m()], f32m())?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let f: extern "C" fn(f32, f32, f32, f32) -> f32 = unsafe { std::mem::transmute(addr) };
        Ok(f(a, b, d_a, d_b))
    }

    /// Call as `fn(f32, f32, f32, f32, f32, f32, f32, f32) -> f32`. T11-D35
    /// canonical shape for the fwd-tangent-only variant of a 4-scalar-param
    /// primal (produced by scalarizing `fn f(p : vec3<f32>, r : f32)` — the
    /// 3 lanes of `p` plus `r` give 4 primals, and the walker **interleaves**
    /// `[p0, d_p0, p1, d_p1, p2, d_p2, r, d_r]` per the fwd-mode convention in
    /// `cssl-autodiff/src/substitute.rs::synthesize_tangent_params`).
    ///
    /// Used by the `sphere_sdf(p : vec3<f32>, r : f32) -> f32 { length(p) - r }`
    /// end-to-end runtime-gradient test : seeding `d_p0 = 1, d_p1 = d_p2 = d_r = 0`
    /// extracts `∂/∂p_0 sphere_sdf`, expected to equal `normalize(p).x` at the
    /// evaluation point.
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    #[allow(clippy::too_many_arguments)] // 8 scalars mirror the MIR param list 1:1
    pub fn call_f32x8_to_f32(
        &self,
        arg0: f32,
        arg1: f32,
        arg2: f32,
        arg3: f32,
        arg4: f32,
        arg5: f32,
        arg6: f32,
        arg7: f32,
        module: &JitModule,
    ) -> Result<f32, JitError> {
        let f32m = || MirType::Float(FloatWidth::F32);
        self.check_sig(
            &[
                f32m(),
                f32m(),
                f32m(),
                f32m(),
                f32m(),
                f32m(),
                f32m(),
                f32m(),
            ],
            f32m(),
        )?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let fn_ptr: extern "C" fn(f32, f32, f32, f32, f32, f32, f32, f32) -> f32 =
            unsafe { std::mem::transmute(addr) };
        Ok(fn_ptr(arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7))
    }

    /// Call as `fn(f32, f32, f32, f32, f32) -> f32`. T11-D37 canonical shape
    /// for a bwd variant of a 4-scalar-param primal *after* single-adjoint
    /// extraction (`extract_bwd_single_adjoint`) : inputs are
    /// `(p_0, p_1, p_2, r, d_y) -> d_<index>`. Used by the bwd-mode verification
    /// of `sphere_sdf(p : vec3<f32>, r : f32)` — each extracted adjoint
    /// corresponds to one lane of `normalize(p)` (or `-1` for the `r` adjoint).
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_f32x5_to_f32(
        &self,
        arg0: f32,
        arg1: f32,
        arg2: f32,
        arg3: f32,
        arg4: f32,
        module: &JitModule,
    ) -> Result<f32, JitError> {
        let f32m = || MirType::Float(FloatWidth::F32);
        self.check_sig(&[f32m(), f32m(), f32m(), f32m(), f32m()], f32m())?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`.
        let fn_ptr: extern "C" fn(f32, f32, f32, f32, f32) -> f32 =
            unsafe { std::mem::transmute(addr) };
        Ok(fn_ptr(arg0, arg1, arg2, arg3, arg4))
    }

    /// Call a 2-param bwd variant compiled with out-param ABI : native
    /// cranelift signature `(a: f32, b: f32, d_y: f32, *mut f32, *mut f32) -> ()`.
    /// Returns the pair `(d_a, d_b)` by allocating stack slots for the two
    /// adjoints, passing their addresses as out-params, and reading back.
    ///
    /// # Errors
    /// Returns [`JitError::SignatureMismatch`] if the fn's MIR signature isn't
    /// 3 f32 params + 2 f32 results using out-param ABI.
    #[allow(clippy::similar_names)] // out_da / out_db : paired bwd-adjoint outputs
    pub fn call_bwd_2_f32_f32_f32_to_f32f32(
        &self,
        a: f32,
        b: f32,
        d_y: f32,
        module: &JitModule,
    ) -> Result<(f32, f32), JitError> {
        let f32m = || MirType::Float(FloatWidth::F32);
        if !self.uses_out_params
            || self.param_types != [f32m(), f32m(), f32m()]
            || self.all_result_types != [f32m(), f32m()]
        {
            return Err(JitError::SignatureMismatch {
                name: self.name.clone(),
                expected: "(f32, f32, f32) -> (f32, f32) via out-param ABI".to_string(),
                actual: format!(
                    "{:?} -> {:?} (out_params={})",
                    self.param_types, self.all_result_types, self.uses_out_params
                ),
            });
        }
        let addr = module.code_addr_for(&self.name)?;
        let mut out_da: f32 = 0.0;
        let mut out_db: f32 = 0.0;
        // SAFETY: see `call_i64_i64_to_i64`. Out-param pointers are to
        // locals on this function's stack ; the JIT fn dereferences them
        // only during the call (no escape). Types match: both `*mut f32`
        // on the Rust side and pointer-to-f32 on the cranelift side.
        let f: extern "C" fn(f32, f32, f32, *mut f32, *mut f32) = unsafe {
            std::mem::transmute::<*const u8, extern "C" fn(f32, f32, f32, *mut f32, *mut f32)>(addr)
        };
        f(a, b, d_y, &mut out_da, &mut out_db);
        Ok((out_da, out_db))
    }

    /// Call as `fn(i64) -> i32`. Canonical shape for a memref.load that takes
    /// a raw host-pointer and returns the loaded scalar (T11-D59 / S6-C3).
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_i64_to_i32(&self, ptr: i64, module: &JitModule) -> Result<i32, JitError> {
        self.check_sig(&[MirType::Int(IntWidth::I64)], MirType::Int(IntWidth::I32))?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`. The fn-pointer cast matches the
        // MIR signature `(i64) -> i32` validated above.
        let f: extern "C" fn(i64) -> i32 = unsafe { std::mem::transmute(addr) };
        Ok(f(ptr))
    }

    /// Call as `fn(i64) -> i64`. Canonical shape for a memref.load on i64
    /// element-type (T11-D59 / S6-C3).
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_i64_to_i64(&self, ptr: i64, module: &JitModule) -> Result<i64, JitError> {
        self.check_sig(&[MirType::Int(IntWidth::I64)], MirType::Int(IntWidth::I64))?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`. The fn-pointer cast matches the
        // MIR signature `(i64) -> i64` validated above.
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(addr) };
        Ok(f(ptr))
    }

    /// Call as `fn(i64) -> f32`. Canonical shape for a memref.load on f32
    /// element-type (T11-D59 / S6-C3).
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_i64_to_f32(&self, ptr: i64, module: &JitModule) -> Result<f32, JitError> {
        self.check_sig(
            &[MirType::Int(IntWidth::I64)],
            MirType::Float(FloatWidth::F32),
        )?;
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`. The fn-pointer cast matches the
        // MIR signature `(i64) -> f32` validated above.
        let f: extern "C" fn(i64) -> f32 = unsafe { std::mem::transmute(addr) };
        Ok(f(ptr))
    }

    /// Call as `fn(i32, i64) -> ()`. Canonical shape for a memref.store of an
    /// i32 value to a raw host-pointer (T11-D59 / S6-C3).
    ///
    /// # Errors
    /// See [`Self::call_i64_i64_to_i64`].
    pub fn call_i32_i64_to_unit(
        &self,
        val: i32,
        ptr: i64,
        module: &JitModule,
    ) -> Result<(), JitError> {
        if self.param_types != [MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I64)]
            || !self.all_result_types.is_empty()
        {
            return Err(JitError::SignatureMismatch {
                name: self.name.clone(),
                expected: "(i32, i64) -> ()".to_string(),
                actual: format!("{:?} -> {:?}", self.param_types, self.all_result_types),
            });
        }
        let addr = module.code_addr_for(&self.name)?;
        // SAFETY: see `call_i64_i64_to_i64`. The fn-pointer cast matches the
        // MIR signature `(i32, i64) -> ()` validated above.
        let f: extern "C" fn(i32, i64) = unsafe { std::mem::transmute(addr) };
        f(val, ptr);
        Ok(())
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

        // T11-D30 : multi-result fns are lowered via out-params. The cranelift
        // signature appends one pointer-param per excess result ; the body's
        // terminator (func.return / cssl.diff.bwd_return) is rewritten to
        // store each operand through its corresponding out-param pointer and
        // then emit `return ()`.
        let use_out_params = primal.results.len() > 1;

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
        if use_out_params {
            // Validate result types are all scalar-JIT-able.
            for (idx, r_ty) in primal.results.iter().enumerate() {
                if mir_to_cl_type(r_ty).is_none() {
                    return Err(JitError::UnsupportedFeature {
                        fn_name: primal.name.clone(),
                        reason: format!("result #{idx} type `{r_ty}` not scalar-JIT-able"),
                    });
                }
            }
            // Append one pointer param per result (native-word-sized).
            let ptr_ty = module.isa().pointer_type();
            for _ in 0..primal.results.len() {
                sig.params.push(AbiParam::new(ptr_ty));
            }
            // Return type is void.
        } else {
            for (idx, r_ty) in primal.results.iter().enumerate() {
                let cl_ty = mir_to_cl_type(r_ty).ok_or_else(|| JitError::UnsupportedFeature {
                    fn_name: primal.name.clone(),
                    reason: format!("result #{idx} type `{r_ty}` not scalar-JIT-able"),
                })?;
                sig.returns.push(AbiParam::new(cl_ty));
            }
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

        // T11-D26 / T11-D29 : pre-scan body ops for `func.call` with callees :
        //   - User-defined callees found in `self.fn_table` → declare_func_in_func
        //     to make the previously-compiled fn visible to this caller.
        //   - Transcendental callees (sin/cos/exp/log) → declare as external
        //     `Linkage::Import` fns (sinf/cosf/expf/logf from libm) + ref-in-func.
        //   - Other intrinsics (min/max/abs/sqrt/fneg) are inlined as cranelift
        //     instructions directly — no FuncRef needed.
        let callee_refs = {
            let mut refs: HashMap<String, cranelift_codegen::ir::FuncRef> = HashMap::new();
            if let Some(entry_block) = primal.body.blocks.first() {
                for op in &entry_block.ops {
                    if op.name == "func.call" {
                        if let Some((_, callee)) = op.attributes.iter().find(|(k, _)| k == "callee")
                        {
                            let callee_name = callee.clone();
                            if refs.contains_key(&callee_name) {
                                continue;
                            }
                            // Transcendental : declare libm extern.
                            if let Some(libm_sym) = transcendental_extern_name(&callee_name) {
                                let mut transc_sig = Signature::new(call_conv);
                                transc_sig.params.push(AbiParam::new(cl_types::F32));
                                transc_sig.returns.push(AbiParam::new(cl_types::F32));
                                let extern_id = module
                                    .declare_function(libm_sym, Linkage::Import, &transc_sig)
                                    .map_err(|e| JitError::LoweringFailed {
                                        fn_name: primal.name.clone(),
                                        detail: format!("declare libm `{libm_sym}` : {e}"),
                                    })?;
                                let fref = module
                                    .declare_func_in_func(extern_id, &mut self.codegen_ctx.func);
                                refs.insert(callee_name, fref);
                                continue;
                            }
                            // Other intrinsic (min/max/abs/sqrt/fneg) : inlined
                            // as cranelift-native insts — skip.
                            if is_inline_intrinsic_callee(&callee_name) {
                                continue;
                            }
                            // User-defined : look up in fn_table.
                            if let Some((callee_id, _)) = self.fn_table.get(&callee_name) {
                                let fref = module
                                    .declare_func_in_func(*callee_id, &mut self.codegen_ctx.func);
                                refs.insert(callee_name, fref);
                            }
                        }
                    }
                }
            }
            refs
        };

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
            let Some(entry_block) = primal.body.blocks.first() else {
                return Err(JitError::UnsupportedFeature {
                    fn_name: primal.name.clone(),
                    reason: "empty body (no blocks)".to_string(),
                });
            };
            // Wire block-args to the **actual** MIR ValueIds listed in
            // `entry_block.args` — walker-emitted fns use non-sequential IDs
            // after `synthesize_tangent_params` interleaves primals + tangents.
            let block_params: Vec<_> = builder.block_params(entry).to_vec();
            let arg_value_ids: Vec<ValueId> = entry_block.args.iter().map(|a| a.id).collect();
            let primal_param_count = arg_value_ids.len();
            if primal_param_count == block_params.len() {
                // Simple case : param-count matches block-param-count (no out-params).
                for (arg_id, &bp) in arg_value_ids.iter().zip(block_params.iter()) {
                    value_map.insert(*arg_id, bp);
                }
            } else if use_out_params
                && primal_param_count + primal.results.len() == block_params.len()
            {
                // Multi-result via out-params : first N block-params are the
                // original args ; last M are the out-ptrs.
                for (arg_id, &bp) in arg_value_ids
                    .iter()
                    .zip(block_params.iter().take(primal_param_count))
                {
                    value_map.insert(*arg_id, bp);
                }
            } else {
                // Fallback for primal fns with empty `entry.args` : map by index.
                for (idx, &bp) in block_params.iter().enumerate() {
                    value_map.insert(ValueId(idx as u32), bp);
                }
            }

            // Collect out-param cranelift Values for use at return-terminator.
            let out_param_values: Vec<cranelift_codegen::ir::Value> = if use_out_params {
                block_params
                    .iter()
                    .skip(primal_param_count)
                    .copied()
                    .collect()
            } else {
                Vec::new()
            };

            let mut saw_return = false;
            for op in &entry_block.ops {
                if use_out_params && (op.name == "func.return" || op.name == "cssl.diff.bwd_return")
                {
                    // T11-D30 : multi-result return — store each operand
                    // through its out-param pointer + emit `return ()`.
                    if op.operands.len() != out_param_values.len() {
                        return Err(JitError::LoweringFailed {
                            fn_name: primal.name.clone(),
                            detail: format!(
                                "multi-result return has {} operands but {} out-params",
                                op.operands.len(),
                                out_param_values.len()
                            ),
                        });
                    }
                    for (vid, &out_ptr) in op.operands.iter().zip(out_param_values.iter()) {
                        let v = *value_map.get(vid).ok_or_else(|| JitError::LoweringFailed {
                            fn_name: primal.name.clone(),
                            detail: format!("multi-return references unknown ValueId({})", vid.0),
                        })?;
                        builder.ins().store(
                            cranelift_codegen::ir::MemFlags::trusted(),
                            v,
                            out_ptr,
                            0,
                        );
                    }
                    builder.ins().return_(&[]);
                    saw_return = true;
                    break;
                }
                if lower_op_to_cl(op, &mut builder, &mut value_map, &primal.name, &callee_refs)? {
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
            all_result_types: primal.results.clone(),
            uses_out_params: use_out_params,
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
///
/// `callee_refs` : pre-declared cranelift FuncRefs for user-defined callees
/// that this fn references. Populated by the compile-pass pre-scan.
#[allow(clippy::too_many_lines)]
fn lower_op_to_cl(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    callee_refs: &HashMap<String, cranelift_codegen::ir::FuncRef>,
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
        "arith.cmpf" => lower_cmpf(op, builder, value_map, fn_name),
        "arith.cmpi" => lower_cmpi(op, builder, value_map, fn_name),
        "arith.select" => lower_select(op, builder, value_map, fn_name),
        // T11-D59 / S6-C3 : memref.load / memref.store — non-volatile
        // raw-pointer load + store with optional ptr+offset and explicit
        // alignment override. See `specs/02_IR.csl § MEMORY-OPS`.
        "memref.load" => lower_memref_load(op, builder, value_map, fn_name),
        "memref.store" => lower_memref_store(op, builder, value_map, fn_name),
        "func.call" => lower_intrinsic_call(op, builder, value_map, fn_name, callee_refs),
        // T11-D58 / S6-C1 : scf.if → cranelift brif + extended-blocks. The
        // shared helper in `crate::scf` walks the two regions and threads
        // the yielded value (when present) through a merge-block parameter.
        "scf.if" => lower_scf_if_in_jit(op, builder, value_map, fn_name, callee_refs),
        // scf.yield is consumed by `lower_scf_if_in_jit` directly. Encountering
        // it at the outer dispatch level means it leaked outside its parent
        // region — that's a structured-CFG violation, but for stage-0 we
        // accept it as a no-op so legacy hand-built MIR (without parent
        // scf.if) keeps lowering. D5 (StructuredCfgValidator) will reject
        // bare scf.yield at the outer level once it lands.
        "scf.yield" => Ok(false),
        // `cssl.diff.bwd_return` is the AD walker's bwd-variant terminator —
        // it carries one-operand-per-primal-float-param holding that param's
        // accumulated adjoint. Lower identically to `func.return` since the
        // operands + result-shape match exactly.
        "func.return" | "cssl.diff.bwd_return" => {
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

/// Lower `arith.cmpf %a, %b {predicate = "ole"}` → cranelift `fcmp <cc> a, b`.
fn lower_cmpf(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let pred_str = predicate_attr(op)?;
    let cc = parse_float_cc(pred_str).ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("unknown arith.cmpf predicate `{pred_str}`"),
    })?;
    emit_binary(op, builder, value_map, fn_name, |b, a, c| {
        b.ins().fcmp(cc, a, c)
    })
}

/// Lower `arith.cmpi %a, %b {predicate = "slt"}` → cranelift `icmp <cc> a, b`.
fn lower_cmpi(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let pred_str = predicate_attr(op)?;
    let cc = parse_int_cc(pred_str).ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("unknown arith.cmpi predicate `{pred_str}`"),
    })?;
    emit_binary(op, builder, value_map, fn_name, |b, a, c| {
        b.ins().icmp(cc, a, c)
    })
}

/// Lower `func.call` whose callee is a recognized intrinsic — map to a
/// cranelift intrinsic instruction. Covers the math fns the AD walker
/// specializes via `specialize_transcendental` :
///   `min` / `math.min` / `fmin` → `fmin`
///   `max` / `math.max` / `fmax` → `fmax`
///   `abs` / `math.abs` / `fabs` → `fabs`
///   `sqrt` / `math.sqrt` / `sqrtf` → `sqrt`
///   `neg` → `fneg`
/// User-defined inter-fn calls are not yet JIT-able ; they return
/// [`JitError::UnsupportedMirOp`].
fn lower_intrinsic_call(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    callee_refs: &HashMap<String, cranelift_codegen::ir::FuncRef>,
) -> Result<bool, JitError> {
    let (_, callee) = op
        .attributes
        .iter()
        .find(|(k, _)| k == "callee")
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "func.call missing `callee` attribute".to_string(),
        })?;
    let callee_str = callee.as_str();
    match callee_str {
        "min" | "math.min" | "fmin" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fmin(a, c)
        }),
        "max" | "math.max" | "fmax" => emit_binary(op, builder, value_map, fn_name, |b, a, c| {
            b.ins().fmax(a, c)
        }),
        "abs" | "math.abs" | "fabs" | "math.absf" => {
            emit_unary(op, builder, value_map, fn_name, |b, a| b.ins().fabs(a))
        }
        "sqrt" | "math.sqrt" | "sqrtf" | "math.sqrtf" => {
            emit_unary(op, builder, value_map, fn_name, |b, a| b.ins().sqrt(a))
        }
        "neg" | "fneg" => emit_unary(op, builder, value_map, fn_name, |b, a| b.ins().fneg(a)),
        // Transcendentals : emit call to libm extern declared in the pre-scan
        // (sinf/cosf/expf/logf). Sig is `(f32) -> f32`.
        "sin" | "cos" | "exp" | "log" | "ln" | "math.sin" | "math.cos" | "math.exp"
        | "math.log" => {
            if let Some(&func_ref) = callee_refs.get(callee_str) {
                let a = op
                    .operands
                    .first()
                    .ok_or_else(|| JitError::LoweringFailed {
                        fn_name: fn_name.to_string(),
                        detail: format!("{callee_str} : no operand"),
                    })?;
                let v = *value_map.get(a).ok_or_else(|| JitError::LoweringFailed {
                    fn_name: fn_name.to_string(),
                    detail: format!("{callee_str} : unknown operand ValueId({})", a.0),
                })?;
                let inst = builder.ins().call(func_ref, &[v]);
                let results = builder.inst_results(inst);
                if let Some(r) = op.results.first() {
                    if let Some(&first_res) = results.first() {
                        value_map.insert(r.id, first_res);
                    }
                }
                Ok(false)
            } else {
                Err(JitError::LoweringFailed {
                    fn_name: fn_name.to_string(),
                    detail: format!(
                        "transcendental `{callee_str}` : libm extern not declared (pre-scan bug)"
                    ),
                })
            }
        }
        _ => {
            // T11-D26 : user-defined callee — look up the pre-declared FuncRef
            // and emit a cranelift `call` to it.
            if let Some(&func_ref) = callee_refs.get(callee_str) {
                let mut args: Vec<cranelift_codegen::ir::Value> =
                    Vec::with_capacity(op.operands.len());
                for vid in &op.operands {
                    let v = *value_map.get(vid).ok_or_else(|| JitError::LoweringFailed {
                        fn_name: fn_name.to_string(),
                        detail: format!(
                            "func.call to `{callee_str}` : unknown operand ValueId({})",
                            vid.0
                        ),
                    })?;
                    args.push(v);
                }
                let inst = builder.ins().call(func_ref, &args);
                let results = builder.inst_results(inst);
                if let Some(r) = op.results.first() {
                    if let Some(&first_res) = results.first() {
                        value_map.insert(r.id, first_res);
                    }
                }
                Ok(false)
            } else {
                Err(JitError::UnsupportedMirOp {
                    fn_name: fn_name.to_string(),
                    op_name: format!(
                        "func.call callee=`{callee_str}` (callee not compiled in this JIT module)"
                    ),
                })
            }
        }
    }
}

/// Return `true` if the callee name is a recognized math intrinsic handled
/// directly by `lower_intrinsic_call`. Public for test introspection.
#[must_use]
pub fn is_intrinsic_callee(name: &str) -> bool {
    is_inline_intrinsic_callee(name) || transcendental_extern_name(name).is_some()
}

/// Intrinsics that `lower_intrinsic_call` emits as direct cranelift insts
/// (no extern needed). Covers ops with a native CLIF instruction.
fn is_inline_intrinsic_callee(name: &str) -> bool {
    matches!(
        name,
        "min"
            | "math.min"
            | "fmin"
            | "max"
            | "math.max"
            | "fmax"
            | "abs"
            | "math.abs"
            | "fabs"
            | "math.absf"
            | "sqrt"
            | "math.sqrt"
            | "sqrtf"
            | "math.sqrtf"
            | "neg"
            | "fneg"
    )
}

/// Map a MIR-level transcendental callee name to the libm symbol that
/// cranelift should link against. Returns `None` for non-transcendentals.
/// Stage-0.5 f32-only — single-precision libm symbols.
fn transcendental_extern_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "sin" | "math.sin" => "sinf",
        "cos" | "math.cos" => "cosf",
        "exp" | "math.exp" => "expf",
        "log" | "ln" | "math.log" => "logf",
        _ => return None,
    })
}

/// Lower `arith.select %cond, %t, %f` → cranelift `select cond, t, f`.
fn lower_select(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let (Some(&c_id), Some(&t_id), Some(&f_id)) =
        (op.operands.first(), op.operands.get(1), op.operands.get(2))
    else {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "arith.select expected 3 operands (cond, t, f)".to_string(),
        });
    };
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "arith.select has no result".to_string(),
    })?;
    let cond = *value_map
        .get(&c_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("unknown select-cond ValueId({})", c_id.0),
        })?;
    let t = *value_map
        .get(&t_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("unknown select-true ValueId({})", t_id.0),
        })?;
    let f = *value_map
        .get(&f_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("unknown select-false ValueId({})", f_id.0),
        })?;
    let v = builder.ins().select(cond, t, f);
    value_map.insert(r.id, v);
    Ok(false)
}

fn predicate_attr(op: &MirOp) -> Result<&str, JitError> {
    op.attributes
        .iter()
        .find(|(k, _)| k == "predicate")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: String::new(),
            detail: format!("{} missing `predicate` attribute", op.name),
        })
}

// ───────────────────────────────────────────────────────────────────────
// § T11-D59 / S6-C3 : memref.load + memref.store lowering helpers.
//
// Operand shape (per `specs/02_IR.csl § MEMORY-OPS`) :
//   memref.load  : (ptr : i64 [, offset : i64]) -> elem-T
//   memref.store : (val : T,   ptr : i64 [, offset : i64]) -> ()
//
// Optional `"alignment"` attribute overrides the natural-alignment of the
// element type. Codegen never under-aligns : if the override is < natural,
// we still emit the natural-aligned form (the type-checker is responsible
// for rejecting under-alignments before codegen sees the op).
//
// MemFlags : non-volatile, no aliasing assertion. Atomicity / volatility
// are deferred to a later phase tied to the effect-row infrastructure.
// ───────────────────────────────────────────────────────────────────────

/// Read `"alignment"` attribute as a `u32` ; default to natural alignment of
/// `elem_ty`. Returns the larger of (override, natural) so we never emit an
/// under-aligned access. `None` if `elem_ty` has no natural alignment (caller
/// should already have rejected non-scalar element types).
fn memref_alignment(op: &MirOp, elem_ty: &MirType) -> Option<u32> {
    let natural = elem_ty.natural_alignment()?;
    let parsed = op
        .attributes
        .iter()
        .find(|(k, _)| k == "alignment")
        .and_then(|(_, v)| v.parse::<u32>().ok());
    Some(parsed.map_or(natural, |a| a.max(natural)))
}

/// Memflags for a non-volatile, well-aligned access. Cranelift's `aligned()`
/// asserts that the runtime address satisfies the alignment we record ; the
/// CSSLv3 type-checker + cap-system are responsible for ensuring the
/// alignment claim is true before this op reaches codegen.
fn memref_flags(_align: u32) -> cranelift_codegen::ir::MemFlags {
    let mut flags = cranelift_codegen::ir::MemFlags::new();
    flags.set_aligned();
    flags
}

/// Resolve the effective load/store address : `ptr` if no offset operand, else
/// `iadd ptr, offset`. Both operands must already be present in the value-map.
fn memref_effective_addr(
    builder: &mut FunctionBuilder<'_>,
    value_map: &HashMap<ValueId, cranelift_codegen::ir::Value>,
    ptr_id: ValueId,
    offset_id: Option<ValueId>,
    fn_name: &str,
) -> Result<cranelift_codegen::ir::Value, JitError> {
    let ptr = *value_map
        .get(&ptr_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("memref op references unknown ptr ValueId({})", ptr_id.0),
        })?;
    if let Some(off_id) = offset_id {
        let off = *value_map
            .get(&off_id)
            .ok_or_else(|| JitError::LoweringFailed {
                fn_name: fn_name.to_string(),
                detail: format!("memref op references unknown offset ValueId({})", off_id.0),
            })?;
        Ok(builder.ins().iadd(ptr, off))
    } else {
        Ok(ptr)
    }
}

/// Lower `%r = memref.load %ptr [, %offset] : <elem-T>`.
fn lower_memref_load(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "memref.load with no result".to_string(),
    })?;
    let elem_ty = mir_to_cl_type(&r.ty).ok_or_else(|| JitError::UnsupportedFeature {
        fn_name: fn_name.to_string(),
        reason: format!("memref.load result type `{}` is not a stage-0 scalar", r.ty),
    })?;
    let &ptr_id = op
        .operands
        .first()
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.load expected at least 1 operand (ptr)".to_string(),
        })?;
    let offset_id = op.operands.get(1).copied();
    if op.operands.len() > 2 {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "memref.load expected 1 or 2 operands ; got {}",
                op.operands.len()
            ),
        });
    }
    let align = memref_alignment(op, &r.ty).ok_or_else(|| JitError::UnsupportedFeature {
        fn_name: fn_name.to_string(),
        reason: format!("memref.load element `{}` has no natural alignment", r.ty),
    })?;
    let addr = memref_effective_addr(builder, value_map, ptr_id, offset_id, fn_name)?;
    let flags = memref_flags(align);
    let v = builder.ins().load(elem_ty, flags, addr, 0);
    value_map.insert(r.id, v);
    Ok(false)
}

/// Lower `memref.store %val, %ptr [, %offset]`.
fn lower_memref_store(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    if !op.results.is_empty() {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "memref.store must have 0 results ; got {}",
                op.results.len()
            ),
        });
    }
    let &val_id = op
        .operands
        .first()
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "memref.store expected operands (val, ptr [, offset])".to_string(),
        })?;
    let &ptr_id = op.operands.get(1).ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "memref.store expected at least 2 operands (val, ptr)".to_string(),
    })?;
    let offset_id = op.operands.get(2).copied();
    if op.operands.len() > 3 {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "memref.store expected 2 or 3 operands ; got {}",
                op.operands.len()
            ),
        });
    }
    let val = *value_map
        .get(&val_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("memref.store unknown val ValueId({})", val_id.0),
        })?;
    let val_ty = builder.func.dfg.value_type(val);
    let mir_elem = cl_to_mir_for_align(val_ty);
    let align = mir_elem
        .as_ref()
        .and_then(|t| memref_alignment(op, t))
        .ok_or_else(|| JitError::UnsupportedFeature {
            fn_name: fn_name.to_string(),
            reason: format!("memref.store value type `{val_ty}` has no natural alignment"),
        })?;
    let addr = memref_effective_addr(builder, value_map, ptr_id, offset_id, fn_name)?;
    let flags = memref_flags(align);
    builder.ins().store(flags, val, addr, 0);
    Ok(false)
}

/// Reverse of `mir_to_cl_type` for the scalar set the JIT supports — used by
/// memref.store to derive an alignment when the op has no explicit type
/// attribute and the element-MIR-type isn't carried as an op-result. Returns
/// `None` for non-scalar cranelift types (vector, ref, etc.).
fn cl_to_mir_for_align(t: cranelift_codegen::ir::Type) -> Option<MirType> {
    if t == cl_types::I8 {
        Some(MirType::Int(IntWidth::I8))
    } else if t == cl_types::I16 {
        Some(MirType::Int(IntWidth::I16))
    } else if t == cl_types::I32 {
        Some(MirType::Int(IntWidth::I32))
    } else if t == cl_types::I64 {
        Some(MirType::Int(IntWidth::I64))
    } else if t == cl_types::F32 {
        Some(MirType::Float(FloatWidth::F32))
    } else if t == cl_types::F64 {
        Some(MirType::Float(FloatWidth::F64))
    } else {
        None
    }
}

/// Map MLIR-style float-cmp predicate strings to cranelift's `FloatCC`.
fn parse_float_cc(s: &str) -> Option<cranelift_codegen::ir::condcodes::FloatCC> {
    use cranelift_codegen::ir::condcodes::FloatCC as F;
    Some(match s {
        "eq" | "oeq" => F::Equal,
        "ne" | "one" => F::NotEqual,
        "olt" | "lt" => F::LessThan,
        "ole" | "le" => F::LessThanOrEqual,
        "ogt" | "gt" => F::GreaterThan,
        "oge" | "ge" => F::GreaterThanOrEqual,
        "ult" => F::UnorderedOrLessThan,
        "ule" => F::UnorderedOrLessThanOrEqual,
        "ugt" => F::UnorderedOrGreaterThan,
        "uge" => F::UnorderedOrGreaterThanOrEqual,
        "ord" => F::Ordered,
        "uno" => F::Unordered,
        _ => return None,
    })
}

/// Map MLIR-style int-cmp predicate strings to cranelift's `IntCC`.
fn parse_int_cc(s: &str) -> Option<cranelift_codegen::ir::condcodes::IntCC> {
    use cranelift_codegen::ir::condcodes::IntCC as I;
    Some(match s {
        "eq" => I::Equal,
        "ne" => I::NotEqual,
        "slt" => I::SignedLessThan,
        "sle" => I::SignedLessThanOrEqual,
        "sgt" => I::SignedGreaterThan,
        "sge" => I::SignedGreaterThanOrEqual,
        "ult" => I::UnsignedLessThan,
        "ule" => I::UnsignedLessThanOrEqual,
        "ugt" => I::UnsignedGreaterThan,
        "uge" => I::UnsignedGreaterThanOrEqual,
        _ => return None,
    })
}

/// Adapter : delegate `scf.if` lowering to the shared [`crate::scf`] helper,
/// turning [`crate::scf::BackendOrScfError`] into [`JitError`] so the outer
/// JIT dispatch keeps a single error type. The closure passed in re-enters
/// [`lower_op_to_cl`] for each op inside a branch — that's how nested ops
/// (arith, intrinsic calls, even nested scf.if) reach the right lowerer
/// without `scf.rs` having to know about JIT internals.
fn lower_scf_if_in_jit(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
    callee_refs: &HashMap<String, cranelift_codegen::ir::FuncRef>,
) -> Result<bool, JitError> {
    crate::scf::lower_scf_if(
        op,
        builder,
        value_map,
        fn_name,
        |branch_op, b, vm, name| -> Result<bool, JitError> {
            lower_op_to_cl(branch_op, b, vm, name, callee_refs)
        },
    )
    .map_err(|e| match e {
        crate::scf::BackendOrScfError::Scf(scf_err) => JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("scf.if : {scf_err}"),
        },
        crate::scf::BackendOrScfError::Backend(jit_err) => jit_err,
    })
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
    fn compile_multi_result_empty_body_errors() {
        // T11-D30 : multi-result fns now compile via out-params, BUT an empty
        // body can't emit a valid return (the fallback branch only auto-emits
        // `return ()` for zero-result fns). So an empty-bodied multi-result
        // fn errors with "no func.return" rather than being accepted.
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
            all_result_types: vec![i32_ty()],
            uses_out_params: false,
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D21 : JIT-exec arith.cmpf + arith.select for scene-SDF min(a, b).
    // ─────────────────────────────────────────────────────────────────────

    /// Build MIR for `fn fmin(a: f32, b: f32) -> f32 { if a <= b then a else b }` :
    ///   v2 = cmpf "ole" v0, v1
    ///   v3 = select v2, v0, v1
    ///   return v3
    fn hand_built_fmin_f32() -> MirFunc {
        let mut f = MirFunc::new("fmin", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().expect("entry");
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()),
                MirValue::new(ValueId(1), f32_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.cmpf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Bool)
                    .with_attribute("predicate", "ole"),
            );
            entry.ops.push(
                MirOp::std("arith.select")
                    .with_operand(ValueId(2))
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(3), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        f
    }

    #[test]
    fn scene_sdf_min_a_b_jit_roundtrip() {
        // ═══════════════════════════════════════════════════════════════════
        // § SCENE-SDF MILESTONE : piecewise-linear min(a, b) executes via JIT
        //   using the branchful tangent body shape (cmpf + select) that the
        //   AD walker emits for @differentiable scene fns.
        // ═══════════════════════════════════════════════════════════════════
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_fmin_f32()).unwrap();
        m.finalize().unwrap();

        // min(3.0, 5.0) == 3.0  (first branch wins)
        let r1 = h.call_f32_f32_to_f32(3.0, 5.0, &m).unwrap();
        assert!((r1 - 3.0).abs() < 1e-6, "expected 3.0, got {r1}");

        // min(7.0, 2.0) == 2.0  (second branch wins)
        let r2 = h.call_f32_f32_to_f32(7.0, 2.0, &m).unwrap();
        assert!((r2 - 2.0).abs() < 1e-6, "expected 2.0, got {r2}");

        // min(-1.0, 1.0) == -1.0  (negative handling)
        let r3 = h.call_f32_f32_to_f32(-1.0, 1.0, &m).unwrap();
        assert!((r3 - (-1.0)).abs() < 1e-6, "expected -1.0, got {r3}");

        // min(4.2, 4.2) == 4.2  (cusp case — "ole" picks a, which equals b)
        let r4 = h.call_f32_f32_to_f32(4.2, 4.2, &m).unwrap();
        assert!((r4 - 4.2).abs() < 1e-6, "expected 4.2, got {r4}");
    }

    #[test]
    fn scene_sdf_max_a_b_jit_roundtrip() {
        // Symmetric to min : fn fmax(a, b) = if a >= b then a else b
        let mut f = MirFunc::new("fmax", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()),
                MirValue::new(ValueId(1), f32_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.cmpf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Bool)
                    .with_attribute("predicate", "oge"),
            );
            entry.ops.push(
                MirOp::std("arith.select")
                    .with_operand(ValueId(2))
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(3), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();

        assert!((h.call_f32_f32_to_f32(3.0, 5.0, &m).unwrap() - 5.0).abs() < 1e-6);
        assert!((h.call_f32_f32_to_f32(7.0, 2.0, &m).unwrap() - 7.0).abs() < 1e-6);
    }

    #[test]
    fn cmpi_slt_plus_select_jit_roundtrip() {
        // fn imin(a: i32, b: i32) -> i32 { if a < b then a else b }
        let mut f = MirFunc::new("imin", vec![i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), i32_ty()),
                MirValue::new(ValueId(1), i32_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.cmpi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Bool)
                    .with_attribute("predicate", "slt"),
            );
            entry.ops.push(
                MirOp::std("arith.select")
                    .with_operand(ValueId(2))
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(3), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();

        assert_eq!(h.call_i32_i32_to_i32(3, 5, &m).unwrap(), 3);
        assert_eq!(h.call_i32_i32_to_i32(10, -7, &m).unwrap(), -7);
    }

    #[test]
    fn compose_arith_and_select_jit_roundtrip() {
        // fn abs_diff_min(a: f32, b: f32) -> f32 {
        //     // t = a - b ; return if t >= 0 then t else -t
        //     let t = a - b;
        //     let cmp = t >= 0.0;
        //     let neg_t = -t;
        //     if cmp then t else neg_t
        // }
        let mut f = MirFunc::new("fabs_diff", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()),
                MirValue::new(ValueId(1), f32_ty()),
            ];
            // v2 = a - b
            entry.ops.push(
                MirOp::std("arith.subf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), f32_ty()),
            );
            // v3 = 0.0
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(3), f32_ty())
                    .with_attribute("value", "0.0"),
            );
            // v4 = cmpf oge v2, v3
            entry.ops.push(
                MirOp::std("arith.cmpf")
                    .with_operand(ValueId(2))
                    .with_operand(ValueId(3))
                    .with_result(ValueId(4), MirType::Bool)
                    .with_attribute("predicate", "oge"),
            );
            // v5 = -v2
            entry.ops.push(
                MirOp::std("arith.negf")
                    .with_operand(ValueId(2))
                    .with_result(ValueId(5), f32_ty()),
            );
            // v6 = select v4, v2, v5
            entry.ops.push(
                MirOp::std("arith.select")
                    .with_operand(ValueId(4))
                    .with_operand(ValueId(2))
                    .with_operand(ValueId(5))
                    .with_result(ValueId(6), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(6)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();

        // |3.0 - 5.0| == 2.0
        assert!((h.call_f32_f32_to_f32(3.0, 5.0, &m).unwrap() - 2.0).abs() < 1e-6);
        // |10.0 - 3.0| == 7.0
        assert!((h.call_f32_f32_to_f32(10.0, 3.0, &m).unwrap() - 7.0).abs() < 1e-6);
        // |a - a| == 0.0
        assert!(h.call_f32_f32_to_f32(42.0, 42.0, &m).unwrap().abs() < 1e-6);
    }

    /// Hand-built MIR for the forward-mode tangent of `scene(a, b) = min(a, b)`.
    ///   fn scene_fwd(a: f32, b: f32, d_a: f32, d_b: f32) -> f32 {
    ///     let cmp = a <= b;
    ///     select(cmp, d_a, d_b)
    ///   }
    ///
    /// This is *exactly* the shape T11-D15's [`crate::emit_min_fwd`][^n]
    /// emits — it's the AD walker's forward-tangent body for `Primitive::Min`.
    ///
    /// [^n]: See `cssl_autodiff::substitute::emit_min_fwd`.
    fn hand_built_scene_sdf_min_fwd() -> MirFunc {
        let mut f = MirFunc::new(
            "scene_fwd",
            vec![f32_ty(), f32_ty(), f32_ty(), f32_ty()],
            vec![f32_ty()],
        );
        f.next_value_id = 4;
        {
            let entry = f.body.entry_mut().expect("entry");
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()), // a
                MirValue::new(ValueId(1), f32_ty()), // b
                MirValue::new(ValueId(2), f32_ty()), // d_a
                MirValue::new(ValueId(3), f32_ty()), // d_b
            ];
            entry.ops.push(
                MirOp::std("arith.cmpf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(4), MirType::Bool)
                    .with_attribute("predicate", "ole"),
            );
            entry.ops.push(
                MirOp::std("arith.select")
                    .with_operand(ValueId(4))
                    .with_operand(ValueId(2))
                    .with_operand(ValueId(3))
                    .with_result(ValueId(5), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(5)));
        }
        f
    }

    #[test]
    fn killer_app_scene_sdf_min_gradient_matches_central_difference() {
        // ═══════════════════════════════════════════════════════════════════
        // § KILLER-APP RUNTIME VERIFICATION (T11-D22) :
        //   AD walker emits a forward-tangent body for `min(a, b)` as
        //   `cmpf ole + select`. We JIT compile the primal AND the tangent
        //   in the same module, then numerically verify :
        //
        //     tangent_wrt_a(a, b) = (min(a+h, b) - min(a-h, b)) / 2h
        //     tangent_wrt_b(a, b) = (min(a, b+h) - min(a, b-h)) / 2h
        //
        //   for sample points away from the cusp `a = b`.
        //   At cusp, sharp-min is sub-gradient-valued — test avoids those.
        //
        //   This closes the killer-app loop at runtime : structural
        //   verification (T11-D16) + runtime numerical agreement (here).
        // ═══════════════════════════════════════════════════════════════════

        let mut m = JitModule::new();
        let scene = m.compile(&hand_built_fmin_f32()).unwrap();
        let scene_fwd = m.compile(&hand_built_scene_sdf_min_fwd()).unwrap();
        m.finalize().unwrap();
        assert!(m.is_finalized());

        // Samples chosen away from the cusp `a = b` (subgradient region).
        let samples: &[(f32, f32)] = &[
            (3.0, 5.0),   // a < b : grad wrt a = 1.0, wrt b = 0.0
            (5.0, 3.0),   // a > b : grad wrt a = 0.0, wrt b = 1.0
            (-1.0, 1.0),  // a < b w/ negative
            (10.0, -2.0), // a > b w/ negative b
            (0.5, 2.5),
            (-7.3, 0.1),
        ];

        let h = 1e-3_f32;
        for &(a, b) in samples {
            // ────────────────────────────────────────
            // § Gradient w.r.t. a : seed (d_a = 1, d_b = 0).
            // ────────────────────────────────────────
            let tangent_a = scene_fwd
                .call_f32_f32_f32_f32_to_f32(a, b, 1.0, 0.0, &m)
                .unwrap();
            let scene_plus = scene.call_f32_f32_to_f32(a + h, b, &m).unwrap();
            let scene_minus = scene.call_f32_f32_to_f32(a - h, b, &m).unwrap();
            let numerical_a = (scene_plus - scene_minus) / (2.0 * h);
            assert!(
                (tangent_a - numerical_a).abs() < 1e-3,
                "gradient wrt a mismatch @ (a={a}, b={b}) : JIT-tangent={tangent_a} vs central-diff={numerical_a}"
            );

            // ────────────────────────────────────────
            // § Gradient w.r.t. b : seed (d_a = 0, d_b = 1).
            // ────────────────────────────────────────
            let tangent_b = scene_fwd
                .call_f32_f32_f32_f32_to_f32(a, b, 0.0, 1.0, &m)
                .unwrap();
            let scene_plus_b = scene.call_f32_f32_to_f32(a, b + h, &m).unwrap();
            let scene_minus_b = scene.call_f32_f32_to_f32(a, b - h, &m).unwrap();
            let numerical_b = (scene_plus_b - scene_minus_b) / (2.0 * h);
            assert!(
                (tangent_b - numerical_b).abs() < 1e-3,
                "gradient wrt b mismatch @ (a={a}, b={b}) : JIT-tangent={tangent_b} vs central-diff={numerical_b}"
            );
        }
    }

    #[test]
    #[allow(clippy::similar_names)] // t_a_at_* + t_b_at_* are paired by design
    fn killer_app_scene_sdf_min_exact_gradient_values() {
        // More specific : at (a=3, b=5) with a < b, the tangent body should
        // produce d_a when seeded (1, 0) and d_b when seeded (0, 1). Verifies
        // the branchful select picks the correct branch.
        let mut m = JitModule::new();
        m.compile(&hand_built_fmin_f32()).unwrap();
        let scene_fwd = m.compile(&hand_built_scene_sdf_min_fwd()).unwrap();
        m.finalize().unwrap();

        // a < b : min = a, so grad wrt a = 1, grad wrt b = 0.
        let t_a_at_3_5 = scene_fwd
            .call_f32_f32_f32_f32_to_f32(3.0, 5.0, 1.0, 0.0, &m)
            .unwrap();
        let t_b_at_3_5 = scene_fwd
            .call_f32_f32_f32_f32_to_f32(3.0, 5.0, 0.0, 1.0, &m)
            .unwrap();
        assert!((t_a_at_3_5 - 1.0).abs() < 1e-6);
        assert!(t_b_at_3_5.abs() < 1e-6);

        // a > b : min = b, so grad wrt a = 0, grad wrt b = 1.
        let t_a_at_8_2 = scene_fwd
            .call_f32_f32_f32_f32_to_f32(8.0, 2.0, 1.0, 0.0, &m)
            .unwrap();
        let t_b_at_8_2 = scene_fwd
            .call_f32_f32_f32_f32_to_f32(8.0, 2.0, 0.0, 1.0, &m)
            .unwrap();
        assert!(t_a_at_8_2.abs() < 1e-6);
        assert!((t_b_at_8_2 - 1.0).abs() < 1e-6);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D26 : inter-fn JIT calls — fn body contains `call %other_fn`.
    // ─────────────────────────────────────────────────────────────────────

    /// Hand-build `fn double(x: f32) -> f32 { x + x }`.
    fn hand_built_double_f32() -> MirFunc {
        let mut f = MirFunc::new("double", vec![f32_ty()], vec![f32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
            entry.ops.push(
                MirOp::std("arith.addf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        f
    }

    /// Hand-build `fn caller(x: f32) -> f32 { double(x) + 1.0 }`.
    fn hand_built_caller_f32() -> MirFunc {
        let mut f = MirFunc::new("caller", vec![f32_ty()], vec![f32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
            // v1 = call double(v0)
            entry.ops.push(
                MirOp::std("func.call")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty())
                    .with_attribute("callee", "double"),
            );
            // v2 = 1.0
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(2), f32_ty())
                    .with_attribute("value", "1.0"),
            );
            // v3 = v1 + v2
            entry.ops.push(
                MirOp::std("arith.addf")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(3), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        f
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D29 : libm transcendentals via extern declaration.
    // ─────────────────────────────────────────────────────────────────────

    /// Hand-build `fn sinf_wrap(x: f32) -> f32 { sin(x) }`.
    fn hand_built_sin_wrap() -> MirFunc {
        let mut f = MirFunc::new("sinf_wrap", vec![f32_ty()], vec![f32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
            entry.ops.push(
                MirOp::std("func.call")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty())
                    .with_attribute("callee", "sin"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        f
    }

    #[test]
    fn libm_sin_jit_roundtrip() {
        use core::f32::consts::PI;
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_sin_wrap()).expect("compile sin_wrap");
        m.finalize().unwrap();

        // sin(0) = 0, sin(π/2) = 1, sin(π) ≈ 0.
        let sin_0 = h.call_f32_to_f32(0.0, &m).unwrap();
        assert!(sin_0.abs() < 1e-5, "sin(0) ≈ 0, got {sin_0}");
        let sin_half_pi = h.call_f32_to_f32(PI / 2.0, &m).unwrap();
        assert!(
            (sin_half_pi - 1.0).abs() < 1e-5,
            "sin(π/2) ≈ 1, got {sin_half_pi}"
        );
        let sin_pi = h.call_f32_to_f32(PI, &m).unwrap();
        assert!(sin_pi.abs() < 1e-5, "sin(π) ≈ 0, got {sin_pi}");
    }

    #[test]
    fn libm_cos_jit_roundtrip() {
        use core::f32::consts::PI;
        let mut f = MirFunc::new("cosf_wrap", vec![f32_ty()], vec![f32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
            entry.ops.push(
                MirOp::std("func.call")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty())
                    .with_attribute("callee", "cos"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).expect("compile cos_wrap");
        m.finalize().unwrap();

        let cos_0 = h.call_f32_to_f32(0.0, &m).unwrap();
        assert!((cos_0 - 1.0).abs() < 1e-5, "cos(0) = 1, got {cos_0}");
        let cos_pi = h.call_f32_to_f32(PI, &m).unwrap();
        assert!((cos_pi - (-1.0)).abs() < 1e-5, "cos(π) = -1, got {cos_pi}");
    }

    #[test]
    fn libm_exp_log_roundtrip() {
        use core::f32::consts::E;
        // fn expf_wrap(x) = exp(x) ; fn logf_wrap(x) = log(x).
        let mut exp_f = MirFunc::new("expf_wrap", vec![f32_ty()], vec![f32_ty()]);
        exp_f.next_value_id = 1;
        {
            let entry = exp_f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
            entry.ops.push(
                MirOp::std("func.call")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty())
                    .with_attribute("callee", "exp"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut log_f = MirFunc::new("logf_wrap", vec![f32_ty()], vec![f32_ty()]);
        log_f.next_value_id = 1;
        {
            let entry = log_f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
            entry.ops.push(
                MirOp::std("func.call")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty())
                    .with_attribute("callee", "log"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let h_exp = m.compile(&exp_f).expect("compile exp_wrap");
        let h_log = m.compile(&log_f).expect("compile log_wrap");
        m.finalize().unwrap();

        let exp_0 = h_exp.call_f32_to_f32(0.0, &m).unwrap();
        assert!((exp_0 - 1.0).abs() < 1e-5, "exp(0) = 1, got {exp_0}");
        let exp_1 = h_exp.call_f32_to_f32(1.0, &m).unwrap();
        assert!((exp_1 - E).abs() < 1e-4, "exp(1) = e, got {exp_1}");

        let log_e = h_log.call_f32_to_f32(E, &m).unwrap();
        assert!((log_e - 1.0).abs() < 1e-4, "log(e) = 1, got {log_e}");
        let log_1 = h_log.call_f32_to_f32(1.0, &m).unwrap();
        assert!(log_1.abs() < 1e-5, "log(1) = 0, got {log_1}");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D30 : multi-result native via out-param ABI.
    // ─────────────────────────────────────────────────────────────────────

    /// Hand-build a synthetic bwd-like fn `(a, b, d_y) -> (d_a, d_b)` where
    /// `d_a = b * d_y` and `d_b = a * d_y`. This mimics what the AD walker
    /// emits for `fn mul(a, b) { a * b }` in bwd mode (minus chain-rule
    /// composition), with a terminator `cssl.diff.bwd_return d_a, d_b`.
    fn hand_built_multi_result_bwd() -> MirFunc {
        let mut f = MirFunc::new(
            "multi_bwd",
            vec![f32_ty(), f32_ty(), f32_ty()],
            vec![f32_ty(), f32_ty()],
        );
        f.next_value_id = 3;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()), // a
                MirValue::new(ValueId(1), f32_ty()), // b
                MirValue::new(ValueId(2), f32_ty()), // d_y
            ];
            // d_a = b * d_y
            entry.ops.push(
                MirOp::std("arith.mulf")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(3), f32_ty()),
            );
            // d_b = a * d_y
            entry.ops.push(
                MirOp::std("arith.mulf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(4), f32_ty()),
            );
            entry.ops.push(
                MirOp::std("cssl.diff.bwd_return")
                    .with_operand(ValueId(3))
                    .with_operand(ValueId(4)),
            );
        }
        f
    }

    #[test]
    #[allow(clippy::similar_names)] // d_a / d_b : paired bwd-adjoint outputs
    fn multi_result_native_via_out_params() {
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_multi_result_bwd()).expect("compile");
        assert!(h.uses_out_params);
        assert_eq!(h.all_result_types.len(), 2);
        m.finalize().unwrap();

        // At (a=3, b=5, d_y=1) : d_a = b*d_y = 5, d_b = a*d_y = 3.
        let (d_a, d_b) = h
            .call_bwd_2_f32_f32_f32_to_f32f32(3.0, 5.0, 1.0, &m)
            .unwrap();
        assert!(
            (d_a - 5.0).abs() < 1e-6,
            "d_a @ (3, 5) : expected 5, got {d_a}"
        );
        assert!(
            (d_b - 3.0).abs() < 1e-6,
            "d_b @ (3, 5) : expected 3, got {d_b}"
        );

        // Chain-rule via d_y scaling.
        let (d_a2, d_b2) = h
            .call_bwd_2_f32_f32_f32_to_f32f32(2.0, 4.0, 0.5, &m)
            .unwrap();
        assert!((d_a2 - 2.0).abs() < 1e-6); // 4 * 0.5
        assert!((d_b2 - 1.0).abs() < 1e-6); // 2 * 0.5
    }

    #[test]
    fn multi_result_sig_mismatch_rejects_wrong_call_shape() {
        // Single-result fn rejected by the out-param call helper.
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_add_i32()).unwrap();
        m.finalize().unwrap();
        let err = h
            .call_bwd_2_f32_f32_f32_to_f32f32(1.0, 2.0, 3.0, &m)
            .unwrap_err();
        assert!(matches!(err, JitError::SignatureMismatch { .. }));
    }

    #[test]
    fn inter_fn_call_jit_roundtrip() {
        // Compile callee FIRST, then caller.
        let mut m = JitModule::new();
        m.compile(&hand_built_double_f32()).unwrap();
        let caller_h = m.compile(&hand_built_caller_f32()).unwrap();
        m.finalize().unwrap();

        // caller(5.0) = double(5) + 1 = 10 + 1 = 11.
        let r = caller_h.call_f32_to_f32(5.0, &m).unwrap();
        assert!((r - 11.0).abs() < 1e-5, "expected 11.0, got {r}");

        // caller(-3.0) = double(-3) + 1 = -6 + 1 = -5.
        let r2 = caller_h.call_f32_to_f32(-3.0, &m).unwrap();
        assert!((r2 - (-5.0)).abs() < 1e-5, "expected -5.0, got {r2}");
    }

    #[test]
    fn inter_fn_call_unknown_callee_errors() {
        // Caller references a callee that was never compiled.
        let mut m = JitModule::new();
        let err = m.compile(&hand_built_caller_f32()).unwrap_err();
        assert!(matches!(err, JitError::UnsupportedMirOp { .. }));
    }

    #[test]
    fn multi_fn_jit_module_shares_finalize() {
        // Two fns, one finalize — verify both are callable after.
        let mut m = JitModule::new();
        let f1 = m.compile(&hand_built_add_i32()).unwrap();
        let f2 = m.compile(&hand_built_fmin_f32()).unwrap();
        m.finalize().unwrap();
        assert_eq!(f1.call_i32_i32_to_i32(10, 20, &m).unwrap(), 30);
        assert!((f2.call_f32_f32_to_f32(2.5, 3.5, &m).unwrap() - 2.5).abs() < 1e-6);
    }

    #[test]
    fn cmpf_unknown_predicate_errors() {
        let mut f = MirFunc::new("bad", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), f32_ty()),
                MirValue::new(ValueId(1), f32_ty()),
            ];
            entry.ops.push(
                MirOp::std("arith.cmpf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Bool)
                    .with_attribute("predicate", "xyzzy"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(0)));
        }
        let mut m = JitModule::new();
        let err = m.compile(&f).unwrap_err();
        assert!(matches!(err, JitError::LoweringFailed { .. }));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D58 / S6-C1 — scf.if → cranelift brif lowering
    // ─────────────────────────────────────────────────────────────────────
    //
    // The hand-built MIR fixtures here exercise the structured-control-flow
    // contract from `body_lower::lower_if`. Each scf.if op carries :
    //   - operand[0] : i1/i8 condition value (from arith.cmpi / cmpf or a
    //     direct boolean param)
    //   - regions[0] : then-branch — entry block with its body ops + a
    //     trailing `scf.yield <value-id>` when typed
    //   - regions[1] : else-branch — same shape, possibly empty
    //   - results[0] : the merge-block-param's MIR-id + type (or MirType::None)
    //
    // The shared `crate::scf::lower_scf_if` helper turns this into :
    //   brif cond, then_block, else_block
    //   then_block : <branch-ops> ; jump merge_block(<then-yield-arg>)
    //   else_block : <branch-ops> ; jump merge_block(<else-yield-arg>)
    //   merge_block(<param>) : <continuation>

    fn bool_ty() -> MirType {
        MirType::Bool
    }

    /// Hand-build `fn pick(cond: bool, a: i32, b: i32) -> i32 { if cond { a } else { b } }`.
    fn hand_built_pick_i32() -> MirFunc {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("pick", vec![bool_ty(), i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 4;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), bool_ty()),
                MirValue::new(ValueId(1), i32_ty()),
                MirValue::new(ValueId(2), i32_ty()),
            ];
            // Then-region : yield v1
            let mut then_blk = MirBlock::new("entry");
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            // Else-region : yield v2
            let mut else_blk = MirBlock::new("entry");
            else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            let mut else_region = MirRegion::new();
            else_region.push(else_blk);
            // scf.if v0 -> v3 : i32
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(3), i32_ty()),
            );
            // return v3
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        f
    }

    #[test]
    fn scf_if_picks_then_arm_when_cond_true() {
        let mut m = JitModule::new();
        let h = m.compile(&hand_built_pick_i32()).unwrap();
        m.finalize().unwrap();
        // Need a 3-arg call shape : (i8, i32, i32) -> i32.
        // Use the raw fn-ptr cast since we don't have a 3-arg-mixed helper.
        let addr = m.code_addr_for("pick").unwrap();
        // SAFETY: `addr` points into the JIT's executable memory, kept live
        // by `m`. Cranelift signature : (i8, i32, i32) -> i32 with default
        // call-conv = matching `extern "C"` on host.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, i32, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        assert_eq!(f(1, 100, 200), 100);
        assert_eq!(h.name, "pick");
    }

    #[test]
    fn scf_if_picks_else_arm_when_cond_false() {
        let mut m = JitModule::new();
        m.compile(&hand_built_pick_i32()).unwrap();
        m.finalize().unwrap();
        let addr = m.code_addr_for("pick").unwrap();
        // SAFETY: same as `scf_if_picks_then_arm_when_cond_true`.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, i32, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        assert_eq!(f(0, 100, 200), 200);
    }

    /// Hand-build `fn pick_f32(cond: bool, a: f32, b: f32) -> f32 { if cond { a } else { b } }`.
    fn hand_built_pick_f32() -> MirFunc {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "pick_f32",
            vec![bool_ty(), f32_ty(), f32_ty()],
            vec![f32_ty()],
        );
        f.next_value_id = 4;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), bool_ty()),
                MirValue::new(ValueId(1), f32_ty()),
                MirValue::new(ValueId(2), f32_ty()),
            ];
            let mut then_blk = MirBlock::new("entry");
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            let mut else_blk = MirBlock::new("entry");
            else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            let mut else_region = MirRegion::new();
            else_region.push(else_blk);
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(3), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(3)));
        }
        f
    }

    #[test]
    fn scf_if_yields_f32_through_merge_block_param() {
        let mut m = JitModule::new();
        m.compile(&hand_built_pick_f32()).unwrap();
        m.finalize().unwrap();
        let addr = m.code_addr_for("pick_f32").unwrap();
        // SAFETY: see `scf_if_picks_then_arm_when_cond_true`.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, f32, f32) -> f32 = unsafe { std::mem::transmute(addr) };
        assert!((f(1, 1.5, 2.5) - 1.5).abs() < 1e-6);
        assert!((f(0, 1.5, 2.5) - 2.5).abs() < 1e-6);
    }

    /// Hand-build `fn body_with_arith(cond: bool, a: i32) -> i32 {
    ///   if cond { a + 1 } else { a - 1 }
    /// }`.
    /// Exercises non-yield ops INSIDE a branch (the +1 / -1 must happen in
    /// the then/else block before the merge-jump).
    fn hand_built_branch_arith_i32() -> MirFunc {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("branch_arith", vec![bool_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 7;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), bool_ty()),
                MirValue::new(ValueId(1), i32_ty()),
            ];
            // Constant 1 (used by both branches).
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(2), i32_ty())
                    .with_attribute("value", "1"),
            );
            // Then : v3 = v1 + v2 ; yield v3
            let mut then_blk = MirBlock::new("entry");
            then_blk.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(3), i32_ty()),
            );
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(3)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            // Else : v4 = v1 - v2 ; yield v4
            let mut else_blk = MirBlock::new("entry");
            else_blk.ops.push(
                MirOp::std("arith.subi")
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(4), i32_ty()),
            );
            else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(4)));
            let mut else_region = MirRegion::new();
            else_region.push(else_blk);
            // scf.if v0 -> v5
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(5), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(5)));
        }
        f
    }

    #[test]
    fn scf_if_branch_arith_then_runs_in_then_block() {
        let mut m = JitModule::new();
        m.compile(&hand_built_branch_arith_i32()).unwrap();
        m.finalize().unwrap();
        let addr = m.code_addr_for("branch_arith").unwrap();
        // SAFETY: see prior tests.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        assert_eq!(f(1, 10), 11);
    }

    #[test]
    fn scf_if_branch_arith_else_runs_in_else_block() {
        let mut m = JitModule::new();
        m.compile(&hand_built_branch_arith_i32()).unwrap();
        m.finalize().unwrap();
        let addr = m.code_addr_for("branch_arith").unwrap();
        // SAFETY: see prior tests.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        assert_eq!(f(0, 10), 9);
    }

    /// Hand-build `fn no_else(cond: bool, a: i32) -> i32 { if cond { 0 } a }`.
    /// Statement-form scf.if : the if has no else → no merge-block-param.
    /// Lowering should still produce a runnable fn that returns `a`
    /// regardless of `cond`.
    fn hand_built_stmt_if_i32() -> MirFunc {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("stmt_if", vec![bool_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 4;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), bool_ty()),
                MirValue::new(ValueId(1), i32_ty()),
            ];
            // Then : just an arith.constant (side-effect-free at JIT level
            // ; here it's a no-op we drop on the floor). No scf.yield.
            let mut then_blk = MirBlock::new("entry");
            then_blk.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(2), i32_ty())
                    .with_attribute("value", "0"),
            );
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            // Else : empty.
            let else_region = MirRegion::new();
            // scf.if : no result type (statement-only).
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_region(else_region)
                    .with_result(ValueId(3), MirType::None),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        f
    }

    #[test]
    fn scf_if_statement_form_lowers_without_merge_param() {
        let mut m = JitModule::new();
        m.compile(&hand_built_stmt_if_i32()).unwrap();
        m.finalize().unwrap();
        let addr = m.code_addr_for("stmt_if").unwrap();
        // SAFETY: see prior tests.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        // Both branches drop through to `return a`.
        assert_eq!(f(1, 42), 42);
        assert_eq!(f(0, 42), 42);
    }

    /// Hand-build `fn nested(cond1: bool, cond2: bool, a: i32, b: i32, c: i32) -> i32 {
    ///   if cond1 { if cond2 { a } else { b } } else { c }
    /// }`.
    /// Tests recursion through `lower_scf_if` : the inner scf.if is dispatched
    /// while the outer's then-block is the active cursor.
    fn hand_built_nested_if_i32() -> MirFunc {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new(
            "nested",
            vec![bool_ty(), bool_ty(), i32_ty(), i32_ty(), i32_ty()],
            vec![i32_ty()],
        );
        f.next_value_id = 7;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), bool_ty()),
                MirValue::new(ValueId(1), bool_ty()),
                MirValue::new(ValueId(2), i32_ty()),
                MirValue::new(ValueId(3), i32_ty()),
                MirValue::new(ValueId(4), i32_ty()),
            ];
            // Inner scf.if : if cond2 { a } else { b } -> v5
            let mut inner_then_blk = MirBlock::new("entry");
            inner_then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            let mut inner_then_region = MirRegion::new();
            inner_then_region.push(inner_then_blk);
            let mut inner_else_blk = MirBlock::new("entry");
            inner_else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(3)));
            let mut inner_else_region = MirRegion::new();
            inner_else_region.push(inner_else_blk);
            let inner_if = MirOp::std("scf.if")
                .with_operand(ValueId(1))
                .with_region(inner_then_region)
                .with_region(inner_else_region)
                .with_result(ValueId(5), i32_ty());
            // Outer then : nested if + yield v5
            let mut outer_then_blk = MirBlock::new("entry");
            outer_then_blk.ops.push(inner_if);
            outer_then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(5)));
            let mut outer_then_region = MirRegion::new();
            outer_then_region.push(outer_then_blk);
            // Outer else : yield v4 (c)
            let mut outer_else_blk = MirBlock::new("entry");
            outer_else_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(4)));
            let mut outer_else_region = MirRegion::new();
            outer_else_region.push(outer_else_blk);
            // Outer scf.if -> v6
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(outer_then_region)
                    .with_region(outer_else_region)
                    .with_result(ValueId(6), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(6)));
        }
        f
    }

    #[test]
    fn scf_if_nested_evaluates_through_correct_arms() {
        let mut m = JitModule::new();
        m.compile(&hand_built_nested_if_i32()).unwrap();
        m.finalize().unwrap();
        let addr = m.code_addr_for("nested").unwrap();
        // SAFETY: see prior tests. Sig: (i8, i8, i32, i32, i32) -> i32.
        #[allow(unsafe_code)]
        let f: extern "C" fn(i8, i8, i32, i32, i32) -> i32 = unsafe { std::mem::transmute(addr) };
        // cond1=1 cond2=1 -> a
        assert_eq!(f(1, 1, 10, 20, 30), 10);
        // cond1=1 cond2=0 -> b
        assert_eq!(f(1, 0, 10, 20, 30), 20);
        // cond1=0 -> c (cond2 doesn't matter)
        assert_eq!(f(0, 1, 10, 20, 30), 30);
        assert_eq!(f(0, 0, 10, 20, 30), 30);
    }

    #[test]
    fn scf_if_with_wrong_region_count_errors_cleanly() {
        use cssl_mir::{MirBlock, MirRegion};
        let mut f = MirFunc::new("bad_regions", vec![bool_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 3;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), bool_ty()),
                MirValue::new(ValueId(1), i32_ty()),
            ];
            // Build an scf.if with only ONE region — should error.
            let mut then_blk = MirBlock::new("entry");
            then_blk
                .ops
                .push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            let mut then_region = MirRegion::new();
            then_region.push(then_blk);
            entry.ops.push(
                MirOp::std("scf.if")
                    .with_operand(ValueId(0))
                    .with_region(then_region)
                    .with_result(ValueId(2), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let err = m.compile(&f).unwrap_err();
        assert!(
            matches!(&err, JitError::LoweringFailed { detail, .. } if detail.contains("scf.if")),
            "unexpected error : {err:?}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════
    // § T11-D59 (S6-C3) : memref.load + memref.store JIT tests.
    //
    // These tests build hand-crafted MIR that takes a raw pointer (passed
    // as i64) and either loads from it or stores to it via the new
    // memref.load / memref.store ops. End-to-end roundtrip uses Rust-stack
    // storage so we never touch the cssl-rt allocator (capability-aware
    // alloc-deref pairing is a B-phase concern, not C3).
    // ═════════════════════════════════════════════════════════════════════

    /// `fn load_i32(ptr: i64) -> i32 { memref.load ptr }`
    fn build_memref_load_i32_fn(name: &str) -> MirFunc {
        let mut f = MirFunc::new(name, vec![i64_ty()], vec![i32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), i64_ty())];
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        f
    }

    #[test]
    fn memref_load_i32_returns_value_at_pointer() {
        let mut storage: i32 = 0x1234_5678_i32;
        let ptr = std::ptr::from_mut::<i32>(&mut storage) as i64;
        let mut m = JitModule::new();
        let h = m.compile(&build_memref_load_i32_fn("load_i32")).unwrap();
        m.finalize().unwrap();
        let v = h.call_i64_to_i32(ptr, &m).unwrap();
        assert_eq!(v, 0x1234_5678_i32);
    }

    #[test]
    fn memref_load_i64_returns_value_at_pointer() {
        let mut storage: i64 = 0x0FED_CBA9_8765_4321_i64;
        let ptr = std::ptr::from_mut::<i64>(&mut storage) as i64;
        let mut f = MirFunc::new("load_i64", vec![i64_ty()], vec![i64_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), i64_ty())];
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), i64_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        let v = h.call_i64_to_i64(ptr, &m).unwrap();
        assert_eq!(v, 0x0FED_CBA9_8765_4321_i64);
    }

    #[test]
    fn memref_load_f32_returns_value_at_pointer() {
        let mut storage: f32 = -2.5_f32;
        let ptr = std::ptr::from_mut::<f32>(&mut storage) as i64;
        let mut f = MirFunc::new("load_f32", vec![i64_ty()], vec![f32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), i64_ty())];
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), f32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        let v = h.call_i64_to_f32(ptr, &m).unwrap();
        assert!((v - (-2.5_f32)).abs() < 1e-6);
    }

    #[test]
    fn memref_store_i32_writes_value_through_pointer() {
        let mut storage: i32 = 0_i32;
        let ptr = std::ptr::from_mut::<i32>(&mut storage) as i64;
        let mut f = MirFunc::new("store_i32", vec![i32_ty(), i64_ty()], vec![]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), i32_ty()),
                MirValue::new(ValueId(1), i64_ty()),
            ];
            entry.ops.push(
                MirOp::std("memref.store")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1)),
            );
            entry.ops.push(MirOp::std("func.return"));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        let val: i32 = i32::from_ne_bytes(0xDEAD_BEEF_u32.to_ne_bytes());
        h.call_i32_i64_to_unit(val, ptr, &m).unwrap();
        assert_eq!(storage, val);
    }

    #[test]
    fn memref_store_then_load_roundtrip_i32() {
        let mut storage: i32 = 0_i32;
        let ptr = std::ptr::from_mut::<i32>(&mut storage) as i64;

        // Store
        let mut store_fn = MirFunc::new("store_it", vec![i32_ty(), i64_ty()], vec![]);
        store_fn.next_value_id = 2;
        {
            let entry = store_fn.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), i32_ty()),
                MirValue::new(ValueId(1), i64_ty()),
            ];
            entry.ops.push(
                MirOp::std("memref.store")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1)),
            );
            entry.ops.push(MirOp::std("func.return"));
        }
        let mut store_m = JitModule::new();
        let sh = store_m.compile(&store_fn).unwrap();
        store_m.finalize().unwrap();
        let pattern: i32 = i32::from_ne_bytes(0x55AA_55AA_u32.to_ne_bytes());
        sh.call_i32_i64_to_unit(pattern, ptr, &store_m).unwrap();
        assert_eq!(storage, pattern);

        // Load
        let mut load_m = JitModule::new();
        let lh = load_m
            .compile(&build_memref_load_i32_fn("load_it"))
            .unwrap();
        load_m.finalize().unwrap();
        let observed = lh.call_i64_to_i32(ptr, &load_m).unwrap();
        assert_eq!(observed, pattern);
    }

    #[test]
    fn memref_load_with_offset_operand() {
        let storage: [i32; 4] = [10, 20, 30, 40];
        let base = storage.as_ptr() as i64;

        let mut f = MirFunc::new("load_at_offset", vec![i64_ty(), i64_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), i64_ty()),
                MirValue::new(ValueId(1), i64_ty()),
            ];
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        assert_eq!(h.param_types, [i64_ty(), i64_ty()]);
        assert_eq!(h.result_type.as_ref(), Some(&i32_ty()));
        let addr = m.code_addr_for("load_at_offset").unwrap();
        // SAFETY: see `call_i64_i64_to_i64` ; checked sig matches transmute.
        let f_ptr: extern "C" fn(i64, i64) -> i32 = unsafe { std::mem::transmute(addr) };
        let v = f_ptr(base, 8);
        assert_eq!(v, 30);
    }

    #[test]
    fn memref_load_explicit_alignment_attribute_succeeds() {
        let stored: i32 = i32::from_ne_bytes(0xCAFE_F00D_u32.to_ne_bytes());
        let mut storage: i32 = stored;
        let ptr = std::ptr::from_mut::<i32>(&mut storage) as i64;
        let mut f = MirFunc::new("load_aligned", vec![i64_ty()], vec![i32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), i64_ty())];
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), i32_ty())
                    .with_attribute("alignment", "8"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let h = m.compile(&f).unwrap();
        m.finalize().unwrap();
        assert_eq!(h.call_i64_to_i32(ptr, &m).unwrap(), stored);
    }

    #[test]
    fn memref_load_too_many_operands_errors() {
        let mut f = MirFunc::new("bad_load", vec![i64_ty()], vec![i32_ty()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![MirValue::new(ValueId(0), i64_ty())];
            entry.ops.push(
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), i32_ty()),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(1)));
        }
        let mut m = JitModule::new();
        let err = m.compile(&f).unwrap_err();
        assert!(matches!(err, JitError::LoweringFailed { .. }));
    }

    #[test]
    fn memref_store_with_result_errors() {
        let mut f = MirFunc::new("bad_store", vec![i32_ty(), i64_ty()], vec![]);
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                MirValue::new(ValueId(0), i32_ty()),
                MirValue::new(ValueId(1), i64_ty()),
            ];
            entry.ops.push(
                MirOp::std("memref.store")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), i32_ty()),
            );
            entry.ops.push(MirOp::std("func.return"));
        }
        let mut m = JitModule::new();
        let err = m.compile(&f).unwrap_err();
        assert!(matches!(err, JitError::LoweringFailed { .. }));
    }
}

//! JIT execution surface : MIR → machine-code → in-process call.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § stage-0 throwaway.
//! § ROLE : turn a [`MirFunc`] into a callable fn-pointer via in-process JIT.
//!          This is the **bridge slice to stage-1 self-host** : once programs
//!          can execute, the compiler can describe itself in CSSLv3 and
//!          bootstrap.
//!
//! § STATUS (T11-D19 / this commit)
//!   **API surface designed + stubbed**. Full `cranelift-jit` + `cranelift-frontend`
//!   integration is blocked on a Rust toolchain bump (current pin : 1.75.0 ;
//!   cranelift 0.115 + its transitive `indexmap 2.14` require `edition2024`
//!   support which lands in Rust 1.85+). See [`DECISIONS.md § T11-D19`] for
//!   the toolchain-bump gate.
//!
//!   The API here is the **exact shape** the real implementation will expose.
//!   Swapping in real Cranelift is a pure internal change once the toolchain
//!   pin moves.
//!
//! § NEXT-STEP
//!   1. Bump `rust-toolchain.toml` pin to ≥ 1.85.0 (documented DECISIONS entry).
//!   2. Add `cranelift-jit`, `cranelift-frontend`, `cranelift-codegen`,
//!      `cranelift-module` to `cssl-cgen-cpu-cranelift/Cargo.toml`.
//!   3. Implement `JitModule::compile` via `FunctionBuilder` + `JITBuilder` +
//!      `JITModule::finalize_definitions()`.
//!   4. Replace [`JitError::NotActivated`] returns with actual ok-paths.
//!
//! § SPEC-NOTE
//!   This is the first module in the codebase that explicitly commits to an
//!   execution model. Stage-0 everything-is-static is ending ; stage-0.5 has a
//!   runtime-execution surface, even if today it only returns `NotActivated`.

use cssl_mir::MirFunc;
use thiserror::Error;

/// JIT compilation + execution error surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum JitError {
    /// The JIT backend is not yet activated — see module-doc for the
    /// toolchain-bump path. Every `jit_compile` call today returns this.
    #[error(
        "cranelift-jit not activated : blocked on Rust toolchain bump ≥ 1.85.0 \
         (current pin : 1.75.0) ; see DECISIONS.md § T11-D19 for the gate"
    )]
    NotActivated,
    /// The MIR function had a feature the JIT does not support.
    #[error("unsupported MIR feature in `{fn_name}` : {reason}")]
    UnsupportedFeature { fn_name: String, reason: String },
    /// Cranelift reported a lowering error on the function.
    #[error("cranelift lowering failed for `{fn_name}` : {detail}")]
    LoweringFailed { fn_name: String, detail: String },
    /// The requested fn-name was not present in the JIT module.
    #[error("no such JIT-compiled fn : `{name}`")]
    UnknownFunction { name: String },
}

/// A handle to a JIT-compiled function.
///
/// When activated, this wraps a Cranelift `FuncId` + function-pointer. Today
/// it's an opaque marker that records the primal fn's signature so the test
/// harness can assert structural expectations without executing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitFn {
    /// The primal fn name (from MIR).
    pub name: String,
    /// Number of params the fn takes.
    pub param_count: usize,
    /// Whether the fn has a single scalar result.
    pub has_result: bool,
}

impl JitFn {
    /// Attempt to call the JIT-compiled fn with two `i64` args + return an `i64`.
    /// Used by the canonical `add(i64, i64) -> i64` roundtrip test.
    ///
    /// # Errors
    /// Currently always returns [`JitError::NotActivated`] until the toolchain
    /// bump lands.
    pub fn call_i64_i64_to_i64(&self, _a: i64, _b: i64) -> Result<i64, JitError> {
        Err(JitError::NotActivated)
    }

    /// Attempt to call the JIT-compiled fn with two `f32` args + return an `f32`.
    ///
    /// # Errors
    /// Currently always returns [`JitError::NotActivated`].
    pub fn call_f32_f32_to_f32(&self, _a: f32, _b: f32) -> Result<f32, JitError> {
        Err(JitError::NotActivated)
    }
}

/// A JIT-compiled module holding one-or-more [`JitFn`]s.
///
/// When activated, this wraps a `cranelift_jit::JITModule`. Today it records
/// the primal-fn shapes so tests can assert the API is well-typed.
#[derive(Debug, Default)]
pub struct JitModule {
    fns: Vec<JitFn>,
}

impl JitModule {
    /// New empty JIT module.
    #[must_use]
    pub const fn new() -> Self {
        Self { fns: Vec::new() }
    }

    /// Compile a [`MirFunc`] into a [`JitFn`]. Records the shape + returns the
    /// handle. Real compilation is gated behind [`JitError::NotActivated`].
    ///
    /// # Errors
    /// Returns [`JitError::UnsupportedFeature`] if the MIR fn has > 1 result.
    /// Once activated, returns [`JitError::LoweringFailed`] on Cranelift errors.
    pub fn compile(&mut self, primal: &MirFunc) -> Result<JitFn, JitError> {
        if primal.results.len() > 1 {
            return Err(JitError::UnsupportedFeature {
                fn_name: primal.name.clone(),
                reason: format!("{} results ; stage-0 supports ≤ 1", primal.results.len()),
            });
        }
        let handle = JitFn {
            name: primal.name.clone(),
            param_count: primal.params.len(),
            has_result: !primal.results.is_empty(),
        };
        self.fns.push(handle.clone());
        Ok(handle)
    }

    /// Look up a compiled fn by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&JitFn> {
        self.fns.iter().find(|f| f.name == name)
    }

    /// Number of compiled fns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fns.len()
    }

    /// `true` iff no fns compiled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fns.is_empty()
    }

    /// Whether the JIT backend is activated (i.e., real Cranelift wired in).
    /// Today always returns `false`. Flip to `true` in T11-D19-full.
    #[must_use]
    pub const fn is_activated() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{JitError, JitFn, JitModule};
    use cssl_mir::{IntWidth, MirFunc, MirType};

    #[test]
    fn jit_module_is_not_activated_in_stage_0() {
        assert!(!JitModule::is_activated());
    }

    #[test]
    fn compile_records_primal_shape() {
        let i32_ty = MirType::Int(IntWidth::I32);
        let primal = MirFunc::new("add", vec![i32_ty.clone(), i32_ty.clone()], vec![i32_ty]);
        let mut m = JitModule::new();
        let handle = m.compile(&primal).unwrap();
        assert_eq!(handle.name, "add");
        assert_eq!(handle.param_count, 2);
        assert!(handle.has_result);
    }

    #[test]
    fn compile_rejects_multi_result_fn() {
        let i32_ty = MirType::Int(IntWidth::I32);
        let primal = MirFunc::new("multi", vec![], vec![i32_ty.clone(), i32_ty]);
        let mut m = JitModule::new();
        let err = m.compile(&primal).unwrap_err();
        assert!(matches!(err, JitError::UnsupportedFeature { .. }));
    }

    #[test]
    fn call_returns_not_activated_until_toolchain_bumped() {
        let handle = JitFn {
            name: "add".into(),
            param_count: 2,
            has_result: true,
        };
        let err = handle.call_i64_i64_to_i64(3, 4).unwrap_err();
        assert_eq!(err, JitError::NotActivated);
    }

    #[test]
    fn call_f32_also_returns_not_activated() {
        let handle = JitFn {
            name: "mul".into(),
            param_count: 2,
            has_result: true,
        };
        let err = handle.call_f32_f32_to_f32(1.0, 2.0).unwrap_err();
        assert_eq!(err, JitError::NotActivated);
    }

    #[test]
    fn module_get_finds_registered_fns() {
        let i32_ty = MirType::Int(IntWidth::I32);
        let primal = MirFunc::new("my_fn", vec![], vec![i32_ty]);
        let mut m = JitModule::new();
        m.compile(&primal).unwrap();
        assert_eq!(m.len(), 1);
        assert!(!m.is_empty());
        assert!(m.get("my_fn").is_some());
        assert!(m.get("nonexistent").is_none());
    }

    #[test]
    fn empty_module_is_empty() {
        let m = JitModule::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn jit_error_not_activated_message_mentions_toolchain() {
        let msg = format!("{}", JitError::NotActivated);
        assert!(msg.contains("toolchain"));
        assert!(msg.contains("1.85.0"));
    }

    #[test]
    fn unsupported_feature_message_includes_fn_name() {
        let e = JitError::UnsupportedFeature {
            fn_name: "my_fn".into(),
            reason: "weird thing".into(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("my_fn"));
        assert!(msg.contains("weird thing"));
    }
}

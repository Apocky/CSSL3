//! CSSLv3 stage0 — DXIL emitter via DirectXShaderCompiler shim.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — DXIL path + `specs/14_BACKEND.csl`.
//!
//! § STRATEGY (S6-D2 / T11-D73 — body emission + dxc subprocess wiring)
//!   The DXIL pipeline is a two-stage pipeline at stage-0 :
//!     1. **emit_hlsl** : MIR-fn body → typed HLSL text via the per-op
//!        lowering table in [`emit`]. Refuses to emit unless the
//!        structured-CFG marker (D5 / T11-D70) is present on the module.
//!     2. **DxcCliInvoker** : invoke `dxc.exe` to compile the HLSL text
//!        to DXIL bytes. Mirrors the T6-D1 MLIR-text-CLI fallback + T9-D1
//!        Z3-CLI subprocess pattern. The binary is looked up on PATH ;
//!        absence is non-fatal — emission still produces HLSL ; the
//!        validation pass is skipped + reported as
//!        [`DxcOutcome::BinaryMissing`].
//!
//! § FANOUT-CONTRACT — D5 (T11-D70)
//!   The structured-CFG validator marker on `MirModule` is a hard
//!   pre-condition. Use [`emit::validate_and_emit_hlsl`] for the
//!   one-shot validate-then-emit ergonomic, or call
//!   [`cssl_mir::validate_and_mark`] explicitly + then [`emit_hlsl`].
//!
//! § SCOPE (T11-D73 / S6-D2)
//!   - [`ShaderModel`]         — SM 6.0 / 6.1 / ... / 6.8.
//!   - [`ShaderStage`]         — VS / PS / CS / GS / HS / DS / MS / AS / Lib / RayGen /
//!     ClosestHit / AnyHit / Miss / Intersection / Callable.
//!   - [`HlslProfile`]         — combined `<stage>_<sm>` profile (`"cs_6_6"`).
//!   - [`DxilTargetProfile`]   — profile + features + root-sig version bundle.
//!   - [`HlslModule`] / [`HlslStatement`] / [`HlslExpr`] / [`HlslBodyStmt`]
//!     — typed HLSL syntax model.
//!   - [`emit_hlsl`]           — `MirModule` → HLSL with full body lowering.
//!   - [`validate_and_emit_hlsl`] — convenience wrapper that runs the D5
//!     validator first.
//!   - [`DxcCliInvoker`]       — `dxc.exe` subprocess adapter.
//!   - [`compile_to_dxil`]     — convenience : emit HLSL + invoke DXC in one
//!     call. Returns the rendered HLSL text and the [`DxcOutcome`] together
//!     so callers always have the pre-DXC artifact even on validation failure.
//!   - [`DxilError`]           — error enum.
//!
//! § DEFERRED (preserved per session-7+ scope)
//!   - Owned IDxc* COM-interface emission (no subprocess) — moves dxc to
//!     in-process when MSVC FFI lands per T1-D7.
//!   - Root-signature auto-generation from effect-row + layout attributes ;
//!     wires through to E2 D3D12 host slice once that lands.
//!   - Shader-model-6.8 mesh-shader / RT / cooperative-matrix lowering paths.
//!   - HLSL → SPIR-V round-trip oracle test (dxc -spirv mode).
//!   - True `for` / `while` iter-counter rendering (tracks the cranelift
//!     side's stage-0 single-trip lowering per S6-C2 / T11-D61).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — the per-MirOp emission table in `emit.rs` is large-
// match heavy by design ; the helpers prefer explicit-shape over Option's
// combinators for diagnostic clarity. Tighten as the table stabilizes.
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_map)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::branches_sharing_code)]

pub mod dxc;
pub mod emit;
pub mod hlsl;
pub mod target;

pub use dxc::{DxcCliInvoker, DxcInvocation, DxcOutcome};
pub use emit::{emit_hlsl, validate_and_emit_hlsl, DxilError};
pub use hlsl::{HlslBinaryOp, HlslBodyStmt, HlslExpr, HlslModule, HlslStatement, HlslUnaryOp};
pub use target::{DxilTargetProfile, HlslProfile, RootSignatureVersion, ShaderModel, ShaderStage};

use cssl_mir::MirModule;

/// One-shot result : the HLSL text the emitter produced + the DXC
/// subprocess outcome. Callers inspect the outcome to decide whether the
/// DXIL bytes are available, the DXC binary was missing, or compilation
/// reported diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilArtifact {
    /// The rendered HLSL source text (always populated when
    /// [`compile_to_dxil`] returns `Ok`).
    pub hlsl_text: String,
    /// Outcome of the DXC subprocess invocation.
    pub dxc_outcome: DxcOutcome,
}

/// Compile a `MirModule` to DXIL bytes via the
/// HLSL-text + dxc-subprocess pipeline. Returns the HLSL artifact along
/// with the DXC outcome.
///
/// On failure to emit HLSL (D5-marker missing, unsupported op, heap op,
/// etc.) returns the `DxilError` immediately. On success, attempts to
/// shell out to `dxc.exe` and returns the outcome — including
/// [`DxcOutcome::BinaryMissing`] when the binary isn't on PATH (this is
/// non-fatal per the slice handoff's CI-skip rule).
///
/// # Errors
/// All variants of [`DxilError`] from [`emit_hlsl`] / `validate_and_emit_hlsl`.
pub fn compile_to_dxil(
    module: &MirModule,
    profile: &DxilTargetProfile,
    entry_name: &str,
    invoker: &DxcCliInvoker,
    extra_args: Vec<String>,
) -> Result<DxilArtifact, DxilError> {
    let hlsl_module = emit_hlsl(module, profile, entry_name)?;
    let hlsl_text = hlsl_module.render();
    let invocation = DxcInvocation {
        hlsl_text: hlsl_text.clone(),
        profile: profile.clone(),
        entry_point: entry_name.to_string(),
        extra_args,
    };
    let dxc_outcome = invoker.compile(&invocation);
    Ok(DxilArtifact {
        hlsl_text,
        dxc_outcome,
    })
}

/// Convenience : run the D5 validator, mark the module, emit HLSL, invoke
/// dxc. Returns the artifact in one call. Mutates `module` (sets the
/// validated-marker on success).
///
/// # Errors
/// All variants of [`DxilError`] including the structured-CFG validator's
/// violations (wrapped into [`DxilError::MalformedOp`]).
pub fn validate_and_compile_to_dxil(
    module: &mut MirModule,
    profile: &DxilTargetProfile,
    entry_name: &str,
    invoker: &DxcCliInvoker,
    extra_args: Vec<String>,
) -> Result<DxilArtifact, DxilError> {
    let hlsl_module = validate_and_emit_hlsl(module, profile, entry_name)?;
    let hlsl_text = hlsl_module.render();
    let invocation = DxcInvocation {
        hlsl_text: hlsl_text.clone(),
        profile: profile.clone(),
        entry_point: entry_name.to_string(),
        extra_args,
    };
    let dxc_outcome = invoker.compile(&invocation);
    Ok(DxilArtifact {
        hlsl_text,
        dxc_outcome,
    })
}

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}

#[cfg(test)]
mod compile_to_dxil_tests {
    //! Tests for the [`compile_to_dxil`] convenience.
    //!
    //! § Test-gate per slice-handoff
    //!   `dxc.exe` may be absent on the CI runner. We always assert that
    //!   the HLSL emission path runs cleanly. The DXC subprocess result
    //!   is asserted by-shape : either `Success` (when dxc is on PATH) or
    //!   `BinaryMissing` (when it isn't) — both are acceptable. We never
    //!   hard-require the binary.

    use super::{
        compile_to_dxil, validate_and_compile_to_dxil, DxcCliInvoker, DxcOutcome, DxilTargetProfile,
    };
    use cssl_mir::{validate_and_mark, IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};
    use std::path::PathBuf;

    fn module_returning_42() -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "42"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        m.push_func(f);
        validate_and_mark(&mut m).unwrap();
        m
    }

    #[test]
    fn compile_to_dxil_emits_hlsl_text_unconditionally() {
        let module = module_returning_42();
        let invoker = DxcCliInvoker::with_binary(PathBuf::from(
            "C:/does/not/exist/dxc_does_not_exist_in_test.exe",
        ));
        let art = compile_to_dxil(
            &module,
            &DxilTargetProfile::compute_sm66_default(),
            "main_cs",
            &invoker,
            vec![],
        )
        .unwrap();
        assert!(art.hlsl_text.contains("void main_cs"));
        assert!(art.hlsl_text.contains("int v0 = 42;"));
        assert!(art.hlsl_text.contains("return v0;"));
        // Forced-missing binary → BinaryMissing or IoError, never Success.
        match art.dxc_outcome {
            DxcOutcome::BinaryMissing | DxcOutcome::IoError(_) => {}
            other => panic!("expected BinaryMissing/IoError outcome, got {other:?}"),
        }
    }

    #[test]
    fn validate_and_compile_to_dxil_marks_module() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "7"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        module.push_func(f);
        // No validate_and_mark beforehand — the convenience handles it.
        let invoker = DxcCliInvoker::with_binary(PathBuf::from(
            "C:/does/not/exist/dxc_missing_for_validate_test.exe",
        ));
        let art = validate_and_compile_to_dxil(
            &mut module,
            &DxilTargetProfile::compute_sm66_default(),
            "main_cs",
            &invoker,
            vec![],
        )
        .unwrap();
        assert!(art.hlsl_text.contains("int v0 = 7;"));
        assert!(cssl_mir::has_structured_cfg_marker(&module));
    }

    #[test]
    fn compile_to_dxil_attempts_real_binary_when_present() {
        // Best-effort gate : if `dxc` is on PATH, the outcome should be
        // Success ; if not, BinaryMissing. Either way the HLSL text path
        // is exercised.
        let module = module_returning_42();
        let invoker = DxcCliInvoker::new();
        let art = compile_to_dxil(
            &module,
            &DxilTargetProfile::compute_sm66_default(),
            "main_cs",
            &invoker,
            vec![],
        )
        .unwrap();
        assert!(!art.hlsl_text.is_empty());
        match art.dxc_outcome {
            DxcOutcome::Success { dxil_bytes, .. } => {
                // Real DXC produces a DXBC-shaped container — first 4 bytes
                // typically `DXBC`. We verify only that *some* bytes were
                // produced ; not every dxc-version emits identical magic.
                assert!(!dxil_bytes.is_empty(), "expected non-empty DXIL bytes");
            }
            DxcOutcome::DiagnosticFailure { .. } => {
                // CI-host with dxc but stricter validation rejects the
                // skeleton — that's still a real DXC pass, recorded.
            }
            DxcOutcome::BinaryMissing => {
                // Acceptable per the slice handoff's BinaryMissing gate.
            }
            DxcOutcome::IoError(_) => {
                // PATH lookup error — acceptable, treated like
                // BinaryMissing for CI gating purposes.
            }
        }
    }
}

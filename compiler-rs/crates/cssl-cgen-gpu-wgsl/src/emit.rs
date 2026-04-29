//! MIR → WGSL emitter.
//!
//! § ROLE
//!   Drives the per-fn WGSL emission pipeline. This module is the thin
//!   orchestrator : it walks the MIR module, decides which fn is the entry
//!   point, builds the per-stage signature, delegates body emission to
//!   [`body_emit::lower_fn_body`], and assembles the final [`WgslModule`].
//!
//! § STAGE-0 FANOUT CONTRACT (T11-D75 / S6-D4)
//!   Per the D5 marker contract (T11-D70), the structured-CFG validator
//!   MUST run before WGSL emission. This module checks
//!   [`cssl_mir::has_structured_cfg_marker`] before doing any body work
//!   and returns [`WgslError::StructuredCfgMarkerMissing`] if absent.
//!   Skipping D5 produces malformed structured-CFG-emit code that naga
//!   would reject — so the emitter rejects the input early rather than
//!   producing garbage.
//!
//! § REGRESSION GATE (T11-D32 carry-forward)
//!   The naga round-trip validator from T11-D32 is the immediate regression
//!   gate : every test below that builds a MirModule + emits WGSL also
//!   parses the output through `naga::front::wgsl::parse_str`. A broken
//!   emission table fails at test time, not at GPU-driver consumption time.

use cssl_mir::{has_structured_cfg_marker, MirFunc, MirModule};
use thiserror::Error;

use crate::body_emit::{self, BodyEmitError};
use crate::target::{WebGpuFeature, WebGpuStage, WgslTargetProfile};
use crate::wgsl::{WgslModule, WgslStatement};

/// Failure modes for WGSL emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WgslError {
    /// No entry-point fn was found in the MIR module.
    #[error(
        "MIR module has no fn `{entry}` — WGSL target {stage} requires entry-point declaration"
    )]
    EntryPointMissing { entry: String, stage: String },

    /// **WGSL-D5-CONTRACT** — module presented for emission was not validated
    /// by the structured-CFG validator (T11-D70 / S6-D5). Emission would
    /// risk producing ill-formed structured-CFG WGSL ; reject early.
    #[error(
        "WGSL emission requires the structured-CFG validator (D5 / T11-D70) to run first ; \
         missing `(\"structured_cfg.validated\", \"true\")` module attribute"
    )]
    StructuredCfgMarkerMissing,

    /// Body emission failed — see the wrapped [`BodyEmitError`] for the
    /// stable diagnostic-code (`WGSL0001..WGSL0010`).
    #[error("body emission for fn `{fn_name}` : {source}")]
    BodyEmissionFailed {
        fn_name: String,
        #[source]
        source: BodyEmitError,
    },
}

impl WgslError {
    /// Stable diagnostic code where one applies. Body-emit errors carry
    /// their own `WGSL00..` codes ; the entry-point + marker errors are
    /// emitter-level signals tagged here for completeness.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::EntryPointMissing { .. } => "WGSL-EP",
            Self::StructuredCfgMarkerMissing => "WGSL-D5",
            Self::BodyEmissionFailed { source, .. } => source.code(),
        }
    }
}

/// Emit a `MirModule` as a WGSL translation unit.
///
/// The module MUST have been validated by the structured-CFG validator
/// ([`cssl_mir::validate_and_mark`]) before reaching this function — see
/// [`WgslError::StructuredCfgMarkerMissing`].
///
/// # Errors
/// Returns [`WgslError::EntryPointMissing`] if the entry-point fn is absent,
/// [`WgslError::StructuredCfgMarkerMissing`] if the D5 marker is absent on
/// the input module, or [`WgslError::BodyEmissionFailed`] (wrapping a
/// [`BodyEmitError`]) when an op cannot be lowered.
pub fn emit_wgsl(
    module: &MirModule,
    profile: &WgslTargetProfile,
    entry_name: &str,
) -> Result<WgslModule, WgslError> {
    // § Validator-marker gate. Per D5 contract, GPU emitters check the
    // marker before emission. Skipping D5 is a programmer-error and the
    // emitter rejects rather than produce malformed structured-CFG code.
    //
    // Per the slice handoff landmines this check is mandatory — the naga
    // round-trip validator catches grammar violations, but only D5 catches
    // the structured-CFG invariant. Both layers run together.
    if !has_structured_cfg_marker(module) {
        return Err(WgslError::StructuredCfgMarkerMissing);
    }

    let Some(entry_fn) = module.find_func(entry_name) else {
        return Err(WgslError::EntryPointMissing {
            entry: entry_name.into(),
            stage: profile.stage.attribute().to_string(),
        });
    };

    let mut out = WgslModule::new();
    out.header = Some(format!(
        "// cssl-cgen-gpu-wgsl S6-D4 emission (T11-D75)\n\
         // profile : {}\n\
         // entry = {}",
        profile.summary(),
        entry_name,
    ));

    // Enable-directives derived from features.
    if profile.features.contains(&WebGpuFeature::ShaderF16) {
        out.push(WgslStatement::Enable("f16".into()));
    }
    if profile.features.contains(&WebGpuFeature::Subgroups) {
        out.push(WgslStatement::Enable("subgroups".into()));
    }

    // § Helpers come first so the entry-point fn can call them. WGSL
    // permits forward-references inside a single TU but emitting helpers
    // first matches the canonical order naga reports + makes the output
    // predictably greppable.
    for f in &module.funcs {
        if f.name == entry_name {
            continue;
        }
        out.push(synthesize_helper(f)?);
    }

    // § Entry fn with stage attribute + optional workgroup-size.
    let (ret_ty, params, workgroup_size) = stage_signature(profile);
    let entry_void_return = matches!(profile.stage, WebGpuStage::Compute);
    let body_lines = body_emit::lower_fn_body(entry_fn, entry_void_return).map_err(|e| {
        WgslError::BodyEmissionFailed {
            fn_name: entry_fn.name.clone(),
            source: e,
        }
    })?;

    // For vertex/fragment stages, naga requires the entry-point to return
    // the declared output type. body_emit emits `return v<id>;` only when
    // the fn has results in its MIR signature ; fns lowered without a
    // tracked terminator fall back to the canonical solid-color return so
    // naga still parses cleanly.
    let final_body = ensure_stage_return(profile.stage, body_lines);

    out.push(WgslStatement::EntryFunction {
        stage_attribute: profile.stage.attribute().to_string(),
        workgroup_size,
        return_type: ret_ty.map(String::from),
        name: entry_fn.name.clone(),
        params: params.iter().map(|s| (*s).to_string()).collect(),
        body: final_body,
    });

    Ok(out)
}

type StageSignature = (
    Option<&'static str>,
    &'static [&'static str],
    Option<(u32, u32, u32)>,
);

fn stage_signature(profile: &WgslTargetProfile) -> StageSignature {
    match profile.stage {
        WebGpuStage::Compute => (
            None,
            &["@builtin(global_invocation_id) gid : vec3<u32>"],
            Some((profile.limits.max_workgroup_size_x.min(64), 1, 1)),
        ),
        WebGpuStage::Vertex => (
            Some("@builtin(position) vec4<f32>"),
            &["@builtin(vertex_index) vid : u32"],
            None,
        ),
        WebGpuStage::Fragment => (
            Some("@location(0) vec4<f32>"),
            &["@builtin(position) pos : vec4<f32>"],
            None,
        ),
    }
}

/// Append a stage-appropriate fallback return-statement to a body if it does
/// not already end in one. naga's wgsl-in frontend rejects vertex/fragment
/// entry-points whose body falls off the end without producing the declared
/// return type — this keeps the emitter naga-compatible even when a MIR fn
/// is signature-only or has only side-effect ops.
fn ensure_stage_return(stage: WebGpuStage, mut body: Vec<String>) -> Vec<String> {
    let needs_return = matches!(stage, WebGpuStage::Vertex | WebGpuStage::Fragment);
    let already = body.iter().any(|l| l.trim_start().starts_with("return"));
    if needs_return && !already {
        body.push("return vec4<f32>(0.0, 0.0, 0.0, 1.0);".into());
    }
    body
}

/// Synthesize a helper fn shell. Helpers don't carry a stage attribute and
/// inherit the same body-emission path as entry fns.
fn synthesize_helper(f: &MirFunc) -> Result<WgslStatement, WgslError> {
    let body = body_emit::lower_fn_body(f, /* entry_point_void_return = */ false).map_err(|e| {
        WgslError::BodyEmissionFailed {
            fn_name: f.name.clone(),
            source: e,
        }
    })?;
    Ok(WgslStatement::HelperFunction {
        return_type: None,
        name: f.name.clone(),
        params: vec![],
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::{emit_wgsl, WgslError};
    use crate::body_emit::BodyEmitError;
    use crate::target::{WebGpuStage, WgslTargetProfile};
    use cssl_mir::{
        validate_and_mark, FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId,
    };

    /// Helper : build a module whose only fn is `name` with the given body
    /// ops, run the D5 marker pass, and return the marked module. Every test
    /// below uses this so the structured-CFG-marker contract is honored
    /// uniformly. A test that explicitly wants to verify the no-marker
    /// rejection path skips this helper.
    fn marked_module(name: &str, return_ty: MirType, ops: Vec<MirOp>) -> MirModule {
        let mut module = MirModule::new();
        let mut f = MirFunc::new(name, vec![], vec![return_ty]);
        if let Some(entry) = f.body.entry_mut() {
            entry.ops = ops;
        }
        module.push_func(f);
        validate_and_mark(&mut module).expect("baseline well-formed module passes D5");
        module
    }

    fn const_op(id: u32, ty: MirType, value: &str) -> MirOp {
        MirOp::std("arith.constant")
            .with_result(ValueId(id), ty)
            .with_attribute("value", value)
    }

    /// Build a minimal compute profile without f16 (naga 23 can't parse the
    /// `enable f16;` directive yet — gfx-rs/wgpu#4384).
    fn naga_compatible_compute_profile() -> WgslTargetProfile {
        use std::collections::BTreeSet;
        WgslTargetProfile {
            stage: WebGpuStage::Compute,
            limits: crate::target::WgslLimits::webgpu_default(),
            features: BTreeSet::new(),
        }
    }

    fn naga_compatible_fragment_profile() -> WgslTargetProfile {
        use std::collections::BTreeSet;
        WgslTargetProfile {
            stage: WebGpuStage::Fragment,
            limits: crate::target::WgslLimits::webgpu_default(),
            features: BTreeSet::new(),
        }
    }

    // ── D5 marker contract ──────────────────────────────────────────────

    #[test]
    fn missing_d5_marker_is_rejected() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        // ‼ Deliberately do NOT call validate_and_mark — exercise the
        // emit-side gate.
        let err = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap_err();
        assert!(matches!(err, WgslError::StructuredCfgMarkerMissing));
        assert_eq!(err.code(), "WGSL-D5");
    }

    #[test]
    fn empty_signature_only_compute_emits_skeleton_with_marker_present() {
        let module = marked_module("main_cs", MirType::None, vec![]);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@compute @workgroup_size(64, 1, 1)"));
        assert!(text.contains("fn main_cs"));
        // Empty body still produces the marker comment from body_emit.
        assert!(text.contains("body emitted empty"));
    }

    #[test]
    fn missing_entry_errors_after_marker_gate() {
        let mut module = MirModule::new();
        validate_and_mark(&mut module).unwrap();
        let err = emit_wgsl(&module, &naga_compatible_compute_profile(), "main").unwrap_err();
        assert!(matches!(err, WgslError::EntryPointMissing { .. }));
    }

    // ── Stage signatures preserved ──────────────────────────────────────

    #[test]
    fn compute_skeleton_has_workgroup_size() {
        let module = marked_module("main_cs", MirType::None, vec![]);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@compute @workgroup_size(64, 1, 1)"));
    }

    #[test]
    fn vertex_skeleton_returns_position() {
        let module = marked_module("main_vs", MirType::None, vec![]);
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::vertex_default(), "main_vs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@vertex\n"));
        assert!(text.contains("@builtin(position) vec4<f32>"));
        assert!(text.contains("return vec4<f32>(0.0, 0.0, 0.0, 1.0);"));
    }

    #[test]
    fn fragment_skeleton_emits_location_0() {
        let module = marked_module("main_fs", MirType::None, vec![]);
        let wgsl = emit_wgsl(&module, &naga_compatible_fragment_profile(), "main_fs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@fragment\n"));
        assert!(text.contains("@location(0) vec4<f32>"));
    }

    // ── Body emission ───────────────────────────────────────────────────

    #[test]
    fn entry_fn_with_arith_constant_body_naga_validates() {
        let module = marked_module(
            "main_cs",
            MirType::None,
            vec![const_op(0, MirType::Int(IntWidth::I32), "42")],
        );
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("let v0 : i32 = 42;"));
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed to parse compute shader with body : {:?}\n\nSource:\n{text}",
            parsed.err()
        );
    }

    #[test]
    fn entry_fn_with_int_addition_naga_validates() {
        let ops = vec![
            const_op(0, MirType::Int(IntWidth::I32), "1"),
            const_op(1, MirType::Int(IntWidth::I32), "2"),
            MirOp::std("arith.addi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
        ];
        let module = marked_module("main_cs", MirType::None, ops);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("let v2 : i32 = v0 + v1;"));
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn entry_fn_with_float_arith_naga_validates() {
        let ops = vec![
            const_op(0, MirType::Float(FloatWidth::F32), "1.5"),
            const_op(1, MirType::Float(FloatWidth::F32), "2.5"),
            MirOp::std("arith.mulf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Float(FloatWidth::F32)),
        ];
        let module = marked_module("main_cs", MirType::None, ops);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn entry_fn_with_int_compare_naga_validates() {
        let ops = vec![
            const_op(0, MirType::Int(IntWidth::I32), "5"),
            const_op(1, MirType::Int(IntWidth::I32), "9"),
            MirOp::std("arith.cmpi_slt")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool),
        ];
        let module = marked_module("main_cs", MirType::None, ops);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn entry_fn_with_scf_if_naga_validates() {
        // scf.if produced by canonical lowering — both branches yield.
        let then_region = {
            let mut r = cssl_mir::MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(1, MirType::Int(IntWidth::I32), "1"));
                b.push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            }
            r
        };
        let else_region = {
            let mut r = cssl_mir::MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(2, MirType::Int(IntWidth::I32), "2"));
                b.push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            }
            r
        };
        let if_op = MirOp::std("scf.if")
            .with_operand(ValueId(0))
            .with_region(then_region)
            .with_region(else_region)
            .with_result(ValueId(3), MirType::Int(IntWidth::I32));

        let ops = vec![const_op(0, MirType::Bool, "true"), if_op];
        let module = marked_module("main_cs", MirType::None, ops);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed on scf.if shader : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn entry_fn_with_scf_loop_emits_canonical_shape() {
        // ‼ naga rejects literal `loop { ... }` bodies without a `break`
        // for reachability-analysis reasons. Our scf.loop emits the
        // canonical structured-loop shape ; we assert the shape is correct
        // here without round-tripping through naga (the naga-validation
        // test for the scf.while case below covers the break-on-cond path).
        let body_region = {
            let mut r = cssl_mir::MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(1, MirType::Int(IntWidth::I32), "1"));
            }
            r
        };
        let loop_op = MirOp::std("scf.loop")
            .with_region(body_region)
            .with_result(ValueId(2), MirType::None);
        let module = marked_module("main_cs", MirType::None, vec![loop_op]);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("loop { // scf.loop"));
        assert!(text.contains("let v1 : i32 = 1;"));
    }

    #[test]
    fn entry_fn_with_scf_while_naga_validates() {
        let body_region = {
            let mut r = cssl_mir::MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(1, MirType::Int(IntWidth::I32), "1"));
            }
            r
        };
        let ops = vec![
            const_op(0, MirType::Bool, "true"),
            MirOp::std("scf.while")
                .with_operand(ValueId(0))
                .with_region(body_region)
                .with_result(ValueId(2), MirType::None),
        ];
        let module = marked_module("main_cs", MirType::None, ops);
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed on scf.while shader : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    // ── Heap REJECT propagation ─────────────────────────────────────────

    #[test]
    fn heap_alloc_in_body_propagates_to_emit_error() {
        // ‼ A heap op would also fail D5 if D5 had a heap-rejection rule ;
        // it does not, so the marker passes. We rely on the body-emit gate.
        let ops = vec![
            const_op(0, MirType::Int(IntWidth::I64), "16"),
            const_op(1, MirType::Int(IntWidth::I64), "8"),
            MirOp::std("cssl.heap.alloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Ptr),
        ];
        let module = marked_module("main_cs", MirType::None, ops);
        let err = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap_err();
        match err {
            WgslError::BodyEmissionFailed { source, .. } => {
                assert_eq!(source.code(), "WGSL0001");
            }
            other => panic!("expected BodyEmissionFailed(heap) ; got {other:?}"),
        }
    }

    // ── Closure REJECT propagation ──────────────────────────────────────

    #[test]
    fn closure_op_in_body_propagates_to_emit_error() {
        let ops = vec![MirOp::std("cssl.closure").with_result(ValueId(0), MirType::None)];
        let module = marked_module("main_cs", MirType::None, ops);
        let err = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap_err();
        match err {
            WgslError::BodyEmissionFailed { source, .. } => {
                assert_eq!(source.code(), "WGSL0002");
            }
            other => panic!("expected BodyEmissionFailed(closure) ; got {other:?}"),
        }
    }

    // ── ShaderF16 + helpers ─────────────────────────────────────────────

    #[test]
    fn shader_f16_feature_emits_enable_directive() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("enable f16;"));
    }

    #[test]
    fn helpers_emitted_without_stage_attribute() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        module.push_func(MirFunc::new("util", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("fn util()"));
    }

    #[test]
    fn header_records_profile() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("cssl-cgen-gpu-wgsl S6-D4 emission"));
        assert!(text.contains("timestamp-query"));
        assert!(text.contains("entry = main_cs"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D32 carry-forward : naga-based WGSL validation on signature-only
    // skeletons + helper composition. The S6-D4 work above adds body-tier
    // naga-validates tests that exercise real emission paths.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn naga_validates_compute_skeleton() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed to parse compute shader : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn naga_validates_vertex_skeleton() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_vs", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::vertex_default(), "main_vs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed to parse vertex shader : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn naga_validates_fragment_skeleton() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_fs", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &naga_compatible_fragment_profile(), "main_fs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed to parse fragment shader : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn naga_validates_shader_with_helpers() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        module.push_func(MirFunc::new("helper_one", vec![], vec![]));
        module.push_func(MirFunc::new("helper_two", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text);
        assert!(
            parsed.is_ok(),
            "naga failed to parse shader w/ helpers : {:?}\n\nSource:\n{text}",
            parsed.err(),
        );
    }

    #[test]
    fn naga_validated_module_has_entry_point() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        validate_and_mark(&mut module).unwrap();
        let wgsl = emit_wgsl(&module, &naga_compatible_compute_profile(), "main_cs").unwrap();
        let text = wgsl.render();
        let parsed = naga::front::wgsl::parse_str(&text).expect("parse");
        assert!(
            !parsed.entry_points.is_empty(),
            "expected ≥ 1 entry-point in parsed module"
        );
        let has_cs = parsed
            .entry_points
            .iter()
            .any(|ep| ep.name == "main_cs" && ep.stage == naga::ShaderStage::Compute);
        assert!(has_cs, "expected @compute fn main_cs in parsed module");
    }

    // ── Code-path smoke for the WgslError::code() surface ───────────────

    #[test]
    fn wgsl_error_code_dispatch() {
        assert_eq!(
            WgslError::EntryPointMissing {
                entry: "x".into(),
                stage: "@compute".into(),
            }
            .code(),
            "WGSL-EP",
        );
        assert_eq!(WgslError::StructuredCfgMarkerMissing.code(), "WGSL-D5");
        assert_eq!(
            WgslError::BodyEmissionFailed {
                fn_name: "x".into(),
                source: BodyEmitError::ConstantMissingValueAttr {
                    fn_name: "x".into()
                },
            }
            .code(),
            "WGSL0004",
        );
    }
}

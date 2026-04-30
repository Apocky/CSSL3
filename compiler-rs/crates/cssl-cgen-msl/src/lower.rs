//! `MIR-shader-fn → MSL source string` lowering driver.
//!
//! § ROLE — top-level entry that wires a [`cssl_mir::MirModule`] into a
//!         renderable [`MslSourceModule`] for one chosen entry-point and
//!         pipeline-stage. Phase-1 emits a stage-correct entry-function
//!         signature plus a placeholder body when the MIR fn has no ops ;
//!         non-trivial lowering of the structured-CFG body is reserved for
//!         phase-2 (tracked under W-G3-β / T11-D269-followup).
//!
//! § PIPELINE-STAGE SIGNATURES
//!   - kernel    : `void <name>(uint3 gid [[thread_position_in_grid]],
//!                              device <ret>* out [[buffer(0)]])`
//!   - vertex    : `float4 <name>(uint vid [[vertex_id]])`
//!   - fragment  : `float4 <name>(float4 pos [[position]])`
//!
//! § BINDING-LAYOUT
//!   The driver also accepts an explicit [`BindingLayout`] that lets the
//!   caller specify additional buffer / texture / sampler slots (e.g. for
//!   the LoA-v13 GPU resource set) that get spliced into the entry-fn
//!   parameter list at codegen time.

use thiserror::Error;

use cssl_mir::{MirFunc, MirModule};

use crate::emit::{MslDecl, MslParam, MslSourceModule, StageAttr};
use crate::types::{mir_type_to_msl, BindAttr, MslScalar, MslType};

/// Failure modes for MSL lowering.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MslLowerError {
    /// No function with the requested entry-name is present in the module.
    #[error("MIR module has no fn `{0}` — required for stage `{1}`")]
    EntryMissing(String, String),
    /// A parameter or return type is not Metal-emittable (e.g. `tuple<…>`,
    /// `memref<…>`, `!cssl.handle`). Caller must lower these via stdlib
    /// before reaching codegen.
    #[error("MIR fn `{name}` has non-Metal-emittable type `{ty}` at position `{position}`")]
    UnsupportedType {
        name: String,
        ty: String,
        position: String,
    },
}

/// One additional resource binding the caller wants spliced into the entry
/// signature. Independent of the stage-builtin params (gid / vid / pos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceBinding {
    /// `device <pointee>* <name> [[buffer(slot)]]`.
    Buffer {
        name: String,
        pointee: MslType,
        slot: u32,
    },
    /// Texture binding with kind / element / access supplied by caller.
    Texture {
        name: String,
        ty: MslType,
        slot: u32,
    },
    /// `sampler <name> [[sampler(slot)]]`.
    Sampler { name: String, slot: u32 },
}

impl ResourceBinding {
    /// Lower this binding into an [`MslParam`].
    #[must_use]
    pub fn to_param(&self) -> MslParam {
        match self {
            Self::Buffer {
                name,
                pointee,
                slot,
            } => MslParam::buffer(name.clone(), pointee.clone(), *slot),
            Self::Texture { name, ty, slot } => MslParam::texture(name.clone(), ty.clone(), *slot),
            Self::Sampler { name, slot } => MslParam::sampler(name.clone(), *slot),
        }
    }
}

/// Caller-supplied binding layout merged into the entry-function signature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingLayout {
    /// Additional resource bindings appended after the stage-builtin params.
    pub bindings: Vec<ResourceBinding>,
}

impl BindingLayout {
    /// Empty layout (zero bindings).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add a buffer binding ; returns `&mut self` for builder-style chaining.
    pub fn with_buffer(
        &mut self,
        name: impl Into<String>,
        pointee: MslType,
        slot: u32,
    ) -> &mut Self {
        self.bindings.push(ResourceBinding::Buffer {
            name: name.into(),
            pointee,
            slot,
        });
        self
    }

    /// Add a texture binding.
    pub fn with_texture(
        &mut self,
        name: impl Into<String>,
        ty: MslType,
        slot: u32,
    ) -> &mut Self {
        self.bindings.push(ResourceBinding::Texture {
            name: name.into(),
            ty,
            slot,
        });
        self
    }

    /// Add a sampler binding.
    pub fn with_sampler(&mut self, name: impl Into<String>, slot: u32) -> &mut Self {
        self.bindings.push(ResourceBinding::Sampler {
            name: name.into(),
            slot,
        });
        self
    }
}

/// Driver options for [`lower_module`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerOptions {
    /// Pipeline stage of the entry function.
    pub stage: StageAttr,
    /// Entry-function name in the MIR module.
    pub entry: String,
    /// Caller-supplied resource bindings.
    pub bindings: BindingLayout,
    /// Generated-comment header (None = omit).
    pub header: Option<String>,
}

impl LowerOptions {
    /// Default options targeting a compute kernel named `compute_main`.
    #[must_use]
    pub fn kernel(entry: impl Into<String>) -> Self {
        Self {
            stage: StageAttr::Kernel,
            entry: entry.into(),
            bindings: BindingLayout::default(),
            header: Some("// cssl-cgen-msl stage-0 emission (kernel)".into()),
        }
    }

    /// Default options targeting a vertex shader.
    #[must_use]
    pub fn vertex(entry: impl Into<String>) -> Self {
        Self {
            stage: StageAttr::Vertex,
            entry: entry.into(),
            bindings: BindingLayout::default(),
            header: Some("// cssl-cgen-msl stage-0 emission (vertex)".into()),
        }
    }

    /// Default options targeting a fragment shader.
    #[must_use]
    pub fn fragment(entry: impl Into<String>) -> Self {
        Self {
            stage: StageAttr::Fragment,
            entry: entry.into(),
            bindings: BindingLayout::default(),
            header: Some("// cssl-cgen-msl stage-0 emission (fragment)".into()),
        }
    }
}

/// Lower a `MirModule` to a renderable `MslSourceModule`.
///
/// # Errors
/// Returns [`MslLowerError::EntryMissing`] when the named entry-fn is absent,
/// or [`MslLowerError::UnsupportedType`] when a return-type is not legally
/// expressible in Metal.
pub fn lower_module(
    module: &MirModule,
    opts: &LowerOptions,
) -> Result<MslSourceModule, MslLowerError> {
    let Some(entry) = module.find_func(&opts.entry) else {
        return Err(MslLowerError::EntryMissing(
            opts.entry.clone(),
            opts.stage.attribute().into(),
        ));
    };

    let mut out = MslSourceModule::new();
    out.header = opts.header.clone();
    out.seed_prelude();

    // Emit any helper function signatures up-front so the entry-fn body can
    // reference them. Stage-0 emits skeletons only.
    for f in &module.funcs {
        if f.name == opts.entry {
            continue;
        }
        out.push(synthesize_helper(f));
    }

    // Build the entry-function declaration.
    let return_ty = entry_return_type(opts.stage, entry)?;
    let mut params = stage_default_params(opts.stage);
    for b in &opts.bindings.bindings {
        params.push(b.to_param());
    }

    let body = synthesize_body(opts.stage, entry, &return_ty);

    out.push(MslDecl::Function {
        stage: Some(opts.stage),
        return_ty,
        name: entry.name.clone(),
        params,
        body,
    });

    Ok(out)
}

/// Convenience : lower a module + render to one source-string.
///
/// # Errors
/// Same as [`lower_module`].
pub fn lower_to_source(
    module: &MirModule,
    opts: &LowerOptions,
) -> Result<String, MslLowerError> {
    Ok(lower_module(module, opts)?.render())
}

/// Choose the entry-function return type.
///
/// kernel  → `void` (Metal compute kernels must return void)
/// vertex  → first-result of MIR fn (must lower to vector-of-float, default `float4`)
/// fragment→ first-result of MIR fn (must lower to vector-of-float, default `float4`)
fn entry_return_type(stage: StageAttr, f: &MirFunc) -> Result<MslType, MslLowerError> {
    match stage {
        StageAttr::Kernel => Ok(MslType::Void),
        StageAttr::Vertex | StageAttr::Fragment => {
            // If the MIR fn declares a result, attempt to map it ;
            // otherwise fall back to float4.
            if let Some(t) = f.results.first() {
                if let Some(msl_t) = mir_type_to_msl(t) {
                    Ok(msl_t)
                } else {
                    Err(MslLowerError::UnsupportedType {
                        name: f.name.clone(),
                        ty: t.to_string(),
                        position: "return".into(),
                    })
                }
            } else {
                Ok(MslType::Vector(MslScalar::Float, 4))
            }
        }
    }
}

/// Stage-default builtin parameters before caller-supplied bindings.
fn stage_default_params(stage: StageAttr) -> Vec<MslParam> {
    match stage {
        StageAttr::Kernel => vec![MslParam::builtin(
            "gid",
            MslType::Vector(MslScalar::UInt, 3),
            BindAttr::ThreadPositionInGrid,
        )],
        StageAttr::Vertex => vec![MslParam::builtin(
            "vid",
            MslType::Scalar(MslScalar::UInt),
            BindAttr::VertexId,
        )],
        StageAttr::Fragment => vec![MslParam::builtin(
            "pos",
            MslType::Vector(MslScalar::Float, 4),
            BindAttr::Position,
        )],
    }
}

/// Synthesize a stage-correct placeholder body until full MIR-body lowering
/// is wired in W-G3-β. Returns lines without leading indentation ; the
/// emitter renders them inside a 4-space indent block.
fn synthesize_body(stage: StageAttr, f: &MirFunc, return_ty: &MslType) -> Vec<String> {
    let mut body = vec![format!(
        "// stage-0 skeleton — MIR fn `{}` ; phase-2 wires real body lowering",
        f.name
    )];
    body.push(format!(
        "// MIR signature : {} params, {} results",
        f.params.len(),
        f.results.len()
    ));
    match stage {
        StageAttr::Kernel => {
            // Kernels return void ; no return statement.
            body.push("// kernel-stage : compute work goes here".into());
        }
        StageAttr::Vertex | StageAttr::Fragment => {
            body.push(format!("{return_ty} _out = {return_ty}(0.0);"));
            body.push("return _out;".into());
        }
    }
    body
}

/// Build a helper-function skeleton from a non-entry MIR fn.
fn synthesize_helper(f: &MirFunc) -> MslDecl {
    let return_ty = f
        .results
        .first()
        .and_then(mir_type_to_msl)
        .unwrap_or(MslType::Void);
    MslDecl::Function {
        stage: None,
        return_ty,
        name: f.name.clone(),
        params: vec![],
        body: vec![format!(
            "// helper skeleton (stage-0) — MIR params : {} ; results : {}",
            f.params.len(),
            f.results.len()
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::{lower_module, lower_to_source, BindingLayout, LowerOptions, MslLowerError};
    use crate::types::{MslScalar, MslType};
    use cssl_mir::{FloatWidth, MirFunc, MirModule, MirType};

    fn module_with_entry(name: &str) -> MirModule {
        let mut m = MirModule::new();
        m.push_func(MirFunc::new(name, vec![], vec![]));
        m
    }

    #[test]
    fn missing_entry_returns_error() {
        let module = MirModule::new();
        let opts = LowerOptions::kernel("compute_main");
        let err = lower_module(&module, &opts).unwrap_err();
        assert!(matches!(err, MslLowerError::EntryMissing(ref n, _) if n == "compute_main"));
    }

    #[test]
    fn kernel_lowers_with_default_thread_position_param() {
        let module = module_with_entry("compute_main");
        let opts = LowerOptions::kernel("compute_main");
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("[[kernel]]"));
        assert!(src.contains("void compute_main"));
        assert!(src.contains("uint3 gid [[thread_position_in_grid]]"));
        // Prelude is present.
        assert!(src.contains("#include <metal_stdlib>"));
        assert!(src.contains("using namespace metal;"));
    }

    #[test]
    fn vertex_lowers_with_default_vertex_id() {
        let module = module_with_entry("main_vs");
        let opts = LowerOptions::vertex("main_vs");
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("[[vertex]]"));
        assert!(src.contains("float4 main_vs"));
        assert!(src.contains("uint vid [[vertex_id]]"));
        assert!(src.contains("return _out;"));
    }

    #[test]
    fn fragment_lowers_with_position_input() {
        let module = module_with_entry("main_fs");
        let opts = LowerOptions::fragment("main_fs");
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("[[fragment]]"));
        assert!(src.contains("float4 main_fs"));
        assert!(src.contains("float4 pos [[position]]"));
        assert!(src.contains("return _out;"));
    }

    #[test]
    fn binding_layout_appends_buffer_param() {
        let module = module_with_entry("compute_main");
        let mut opts = LowerOptions::kernel("compute_main");
        opts.bindings
            .with_buffer("output", MslType::Scalar(MslScalar::Float), 0)
            .with_buffer("input", MslType::Scalar(MslScalar::Float), 1);
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("device float* output [[buffer(0)]]"));
        assert!(src.contains("device float* input [[buffer(1)]]"));
    }

    #[test]
    fn binding_layout_supports_texture_and_sampler() {
        let module = module_with_entry("main_fs");
        let mut opts = LowerOptions::fragment("main_fs");
        opts.bindings
            .with_texture(
                "tex",
                MslType::Texture {
                    kind: crate::types::TextureKind::D2,
                    elem: MslScalar::Float,
                    access: crate::types::TextureAccess::Sample,
                },
                0,
            )
            .with_sampler("samp", 0);
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("texture2d<float, access::sample> tex [[texture(0)]]"));
        assert!(src.contains("sampler samp [[sampler(0)]]"));
    }

    #[test]
    fn vertex_with_unsupported_return_type_errors() {
        let mut module = MirModule::new();
        let f = MirFunc::new(
            "main_vs",
            vec![],
            vec![MirType::Memref {
                shape: vec![Some(4)],
                elem: Box::new(MirType::Float(FloatWidth::F32)),
            }],
        );
        module.push_func(f);
        let opts = LowerOptions::vertex("main_vs");
        let err = lower_module(&module, &opts).unwrap_err();
        assert!(matches!(err, MslLowerError::UnsupportedType { .. }));
    }

    #[test]
    fn helper_fns_emitted_with_no_stage_attribute() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("compute_main", vec![], vec![]));
        module.push_func(MirFunc::new("util", vec![], vec![]));
        let opts = LowerOptions::kernel("compute_main");
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("[[kernel]]"));
        // util appears before the entry but without a stage tag.
        let util_pos = src.find("void util()").expect("helper present");
        let kernel_attr_pos = src.find("[[kernel]]").expect("kernel attr present");
        // Helper preceded by either prelude or comment, never [[kernel]].
        let preceding = &src[..util_pos];
        assert!(
            !preceding.lines().last().unwrap_or("").contains("[[kernel]]"),
            "got : {src}"
        );
        // The kernel attribute appears later in the file than the helper.
        assert!(util_pos < kernel_attr_pos);
    }

    #[test]
    fn header_appears_in_rendered_source() {
        let module = module_with_entry("compute_main");
        let opts = LowerOptions::kernel("compute_main");
        let src = lower_to_source(&module, &opts).unwrap();
        assert!(src.contains("cssl-cgen-msl stage-0 emission (kernel)"));
    }

    #[test]
    fn binding_layout_default_is_empty() {
        let layout = BindingLayout::empty();
        assert!(layout.bindings.is_empty());
    }

    #[test]
    fn lower_to_source_round_trips_through_module_render() {
        let module = module_with_entry("compute_main");
        let opts = LowerOptions::kernel("compute_main");
        let direct = lower_to_source(&module, &opts).unwrap();
        let via_module = lower_module(&module, &opts).unwrap().render();
        assert_eq!(direct, via_module);
    }
}

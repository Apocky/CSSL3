//! MSL source-text emitter.
//!
//! § ROLE — direct text-builder for Metal Shading Language modules.
//!         Statement-typed AST → canonical string output. No spirv-cross
//!         shim ; this is the Apple-native path.
//!
//! § PIPELINE
//!   `MslSourceModule` accumulates [`MslDecl`] declarations in author-order ;
//!   `render()` flattens the module to a single source-string ready to feed
//!   `MTLDevice.makeLibrary(source:options:)` or saved to a `.metal` file.

use core::fmt::Write as _;

use crate::types::{BindAttr, MslType};

/// Pipeline stage attribute attached to entry-functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageAttr {
    /// `[[kernel]]` — compute shader.
    Kernel,
    /// `[[vertex]]` — vertex shader.
    Vertex,
    /// `[[fragment]]` — fragment shader.
    Fragment,
}

impl StageAttr {
    /// Metal `[[…]]` attribute string.
    #[must_use]
    pub const fn attribute(self) -> &'static str {
        match self {
            Self::Kernel => "[[kernel]]",
            Self::Vertex => "[[vertex]]",
            Self::Fragment => "[[fragment]]",
        }
    }

    /// Default return type for this stage when the MIR fn returns `none`.
    #[must_use]
    pub const fn default_return(self) -> &'static str {
        match self {
            Self::Kernel => "void",
            Self::Vertex | Self::Fragment => "float4",
        }
    }
}

/// One MSL parameter declaration : `<type> <name> [[attr]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MslParam {
    /// Metal type (rendered before the name).
    pub ty: MslType,
    /// Identifier name.
    pub name: String,
    /// Binding / builtin attribute (optional).
    pub attr: Option<BindAttr>,
}

impl MslParam {
    /// Build a buffer parameter shortcut : `device <ty>* <name> [[buffer(N)]]`.
    #[must_use]
    pub fn buffer(name: impl Into<String>, pointee: MslType, slot: u32) -> Self {
        Self {
            ty: MslType::device_ptr(pointee),
            name: name.into(),
            attr: Some(BindAttr::Buffer(slot)),
        }
    }

    /// Build a texture parameter shortcut.
    #[must_use]
    pub fn texture(name: impl Into<String>, ty: MslType, slot: u32) -> Self {
        Self {
            ty,
            name: name.into(),
            attr: Some(BindAttr::Texture(slot)),
        }
    }

    /// Build a sampler parameter shortcut.
    #[must_use]
    pub fn sampler(name: impl Into<String>, slot: u32) -> Self {
        Self {
            ty: MslType::Sampler,
            name: name.into(),
            attr: Some(BindAttr::Sampler(slot)),
        }
    }

    /// Build a builtin parameter (no buffer slot ; e.g. `[[thread_position_in_grid]]`).
    #[must_use]
    pub fn builtin(name: impl Into<String>, ty: MslType, attr: BindAttr) -> Self {
        Self {
            ty,
            name: name.into(),
            attr: Some(attr),
        }
    }

    /// Render `<ty> <name> [[attr]]` for inclusion in a function-parameter list.
    #[must_use]
    pub fn render(&self) -> String {
        match &self.attr {
            Some(a) => format!("{} {} {}", self.ty, self.name, a.render()),
            None => format!("{} {}", self.ty, self.name),
        }
    }
}

/// One named struct-field : `<type> <name>;` plus optional attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MslField {
    /// Field type.
    pub ty: MslType,
    /// Field name.
    pub name: String,
    /// Optional `[[user(…)]]` / `[[attribute(N)]]` decorator.
    pub attr: Option<BindAttr>,
}

impl MslField {
    /// New field without attribute.
    #[must_use]
    pub fn plain(ty: MslType, name: impl Into<String>) -> Self {
        Self {
            ty,
            name: name.into(),
            attr: None,
        }
    }

    /// Render this field as a `<ty> <name> [[attr]];` line.
    #[must_use]
    pub fn render(&self) -> String {
        match &self.attr {
            Some(a) => format!("{} {} {};", self.ty, self.name, a.render()),
            None => format!("{} {};", self.ty, self.name),
        }
    }
}

/// One top-level MSL declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MslDecl {
    /// `#include <metal_stdlib>`.
    Include(String),
    /// `using namespace metal;`.
    UsingNamespace(String),
    /// `// some comment`.
    Comment(String),
    /// `struct Name { fields … };`.
    Struct {
        /// Struct name.
        name: String,
        /// Fields in order.
        fields: Vec<MslField>,
    },
    /// `typedef <existing> <new>;`.
    Typedef {
        /// Existing type or expression.
        existing: String,
        /// New name introduced.
        new: String,
    },
    /// `constant <ty> <name> = <init>;` — file-scope constant.
    Constant {
        /// Type of the constant.
        ty: MslType,
        /// Identifier.
        name: String,
        /// Initializer expression as raw text.
        init: String,
    },
    /// Function declaration : either entry-point (with stage attribute) or helper.
    Function {
        /// `[[kernel]]` / `[[vertex]]` / `[[fragment]]` (None = helper).
        stage: Option<StageAttr>,
        /// Return type (rendered before the name).
        return_ty: MslType,
        /// Function name.
        name: String,
        /// Parameter list.
        params: Vec<MslParam>,
        /// Function body lines (unindented author text — emitter adds 4-space indent).
        body: Vec<String>,
    },
    /// Verbatim pass-through line.
    Raw(String),
}

impl MslDecl {
    /// Render this declaration as MSL text (terminated by a newline).
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        match self {
            Self::Include(h) => writeln!(out, "#include <{h}>").unwrap(),
            Self::UsingNamespace(ns) => writeln!(out, "using namespace {ns};").unwrap(),
            Self::Comment(c) => writeln!(out, "// {c}").unwrap(),
            Self::Struct { name, fields } => {
                writeln!(out, "struct {name} {{").unwrap();
                for f in fields {
                    writeln!(out, "    {}", f.render()).unwrap();
                }
                writeln!(out, "}};").unwrap();
            }
            Self::Typedef { existing, new } => {
                writeln!(out, "typedef {existing} {new};").unwrap();
            }
            Self::Constant { ty, name, init } => {
                writeln!(out, "constant {ty} {name} = {init};").unwrap();
            }
            Self::Function {
                stage,
                return_ty,
                name,
                params,
                body,
            } => {
                if let Some(s) = stage {
                    writeln!(out, "{}", s.attribute()).unwrap();
                }
                let plist = params
                    .iter()
                    .map(MslParam::render)
                    .collect::<Vec<_>>()
                    .join(",\n    ");
                if params.is_empty() {
                    writeln!(out, "{return_ty} {name}() {{").unwrap();
                } else {
                    writeln!(out, "{return_ty} {name}(\n    {plist}\n) {{").unwrap();
                }
                for line in body {
                    writeln!(out, "    {line}").unwrap();
                }
                writeln!(out, "}}").unwrap();
            }
            Self::Raw(line) => writeln!(out, "{line}").unwrap(),
        }
        out
    }
}

/// MSL translation-unit accumulator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MslSourceModule {
    /// Optional generated-header comment block.
    pub header: Option<String>,
    /// Top-level declarations in author order.
    pub decls: Vec<MslDecl>,
}

impl MslSourceModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a declaration.
    pub fn push(&mut self, d: MslDecl) {
        self.decls.push(d);
    }

    /// Seed the canonical MSL prelude : `#include <metal_stdlib>` +
    /// `using namespace metal;`. Apple's compiler accepts source either way ;
    /// the prelude makes the canonical Metal builtins (`thread_position_in_grid`,
    /// `float4`, etc.) usable without explicit qualification.
    pub fn seed_prelude(&mut self) {
        self.push(MslDecl::Include("metal_stdlib".into()));
        self.push(MslDecl::Include("simd/simd.h".into()));
        self.push(MslDecl::UsingNamespace("metal".into()));
    }

    /// Render the entire module as one MSL source string.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(h) = &self.header {
            writeln!(out, "{h}").unwrap();
            writeln!(out).unwrap();
        }
        for d in &self.decls {
            out.push_str(&d.render());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{MslDecl, MslField, MslParam, MslSourceModule, StageAttr};
    use crate::types::{BindAttr, MslScalar, MslType, TextureAccess, TextureKind};

    #[test]
    fn prelude_seeds_metal_stdlib_and_namespace() {
        let mut m = MslSourceModule::new();
        m.seed_prelude();
        let s = m.render();
        assert!(s.contains("#include <metal_stdlib>"));
        assert!(s.contains("#include <simd/simd.h>"));
        assert!(s.contains("using namespace metal;"));
    }

    #[test]
    fn struct_with_fields_renders() {
        let s = MslDecl::Struct {
            name: "Vertex".into(),
            fields: vec![
                MslField::plain(MslType::Vector(MslScalar::Float, 3), "position"),
                MslField::plain(MslType::Vector(MslScalar::Float, 2), "uv"),
            ],
        };
        let r = s.render();
        assert!(r.contains("struct Vertex {"));
        assert!(r.contains("float3 position;"));
        assert!(r.contains("float2 uv;"));
        assert!(r.contains("};"));
    }

    #[test]
    fn kernel_with_buffer_param_renders() {
        let f = MslDecl::Function {
            stage: Some(StageAttr::Kernel),
            return_ty: MslType::Void,
            name: "compute_main".into(),
            params: vec![
                MslParam::builtin(
                    "gid",
                    MslType::Vector(MslScalar::UInt, 3),
                    BindAttr::ThreadPositionInGrid,
                ),
                MslParam::buffer("output", MslType::Scalar(MslScalar::Float), 0),
            ],
            body: vec!["output[gid.x] = float(gid.x);".into()],
        };
        let r = f.render();
        assert!(r.contains("[[kernel]]"));
        assert!(r.contains("void compute_main"));
        assert!(r.contains("uint3 gid [[thread_position_in_grid]]"));
        assert!(r.contains("device float* output [[buffer(0)]]"));
        assert!(r.contains("output[gid.x] = float(gid.x);"));
    }

    #[test]
    fn vertex_function_renders_with_stage_in() {
        let f = MslDecl::Function {
            stage: Some(StageAttr::Vertex),
            return_ty: MslType::Vector(MslScalar::Float, 4),
            name: "main_vs".into(),
            params: vec![MslParam::builtin(
                "vid",
                MslType::Scalar(MslScalar::UInt),
                BindAttr::VertexId,
            )],
            body: vec!["return float4(0.0, 0.0, 0.0, 1.0);".into()],
        };
        let r = f.render();
        assert!(r.contains("[[vertex]]"));
        assert!(r.contains("float4 main_vs"));
        assert!(r.contains("uint vid [[vertex_id]]"));
    }

    #[test]
    fn fragment_function_with_position_renders() {
        let f = MslDecl::Function {
            stage: Some(StageAttr::Fragment),
            return_ty: MslType::Vector(MslScalar::Float, 4),
            name: "main_fs".into(),
            params: vec![MslParam::builtin(
                "pos",
                MslType::Vector(MslScalar::Float, 4),
                BindAttr::Position,
            )],
            body: vec!["return float4(1.0, 0.0, 0.0, 1.0);".into()],
        };
        let r = f.render();
        assert!(r.contains("[[fragment]]"));
        assert!(r.contains("float4 main_fs"));
        assert!(r.contains("float4 pos [[position]]"));
    }

    #[test]
    fn texture_and_sampler_params_render() {
        let f = MslDecl::Function {
            stage: Some(StageAttr::Fragment),
            return_ty: MslType::Vector(MslScalar::Float, 4),
            name: "main_fs".into(),
            params: vec![
                MslParam::texture(
                    "tex",
                    MslType::Texture {
                        kind: TextureKind::D2,
                        elem: MslScalar::Float,
                        access: TextureAccess::Sample,
                    },
                    0,
                ),
                MslParam::sampler("samp", 0),
            ],
            body: vec!["return tex.sample(samp, float2(0.5));".into()],
        };
        let r = f.render();
        assert!(r.contains("texture2d<float, access::sample> tex [[texture(0)]]"));
        assert!(r.contains("sampler samp [[sampler(0)]]"));
    }

    #[test]
    fn typedef_renders() {
        let t = MslDecl::Typedef {
            existing: "float4".into(),
            new: "RGBA".into(),
        };
        let r = t.render();
        assert!(r.contains("typedef float4 RGBA;"));
    }

    #[test]
    fn constant_renders() {
        let c = MslDecl::Constant {
            ty: MslType::Scalar(MslScalar::Float),
            name: "PI".into(),
            init: "3.14159".into(),
        };
        let r = c.render();
        assert!(r.contains("constant float PI = 3.14159;"));
    }

    #[test]
    fn helper_function_no_stage_attribute() {
        let f = MslDecl::Function {
            stage: None,
            return_ty: MslType::Scalar(MslScalar::Float),
            name: "saturate1".into(),
            params: vec![MslParam {
                ty: MslType::Scalar(MslScalar::Float),
                name: "x".into(),
                attr: None,
            }],
            body: vec!["return clamp(x, 0.0, 1.0);".into()],
        };
        let r = f.render();
        assert!(r.contains("float saturate1"));
        // Helpers should not carry stage attributes.
        assert!(!r.contains("[[kernel]]"));
        assert!(!r.contains("[[vertex]]"));
        assert!(!r.contains("[[fragment]]"));
    }

    #[test]
    fn module_header_appears_first() {
        let mut m = MslSourceModule::new();
        m.header = Some("// generated by cssl-cgen-msl".into());
        m.seed_prelude();
        let s = m.render();
        let header_pos = s.find("generated").unwrap();
        let include_pos = s.find("metal_stdlib").unwrap();
        assert!(header_pos < include_pos);
    }
}

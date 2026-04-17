//! Skeletal HLSL source builder.
//!
//! § SCOPE (T10-phase-1)
//!   Represents a minimal HLSL translation unit : top-level declarations + fn bodies.
//!   Phase-1 emits empty-body fns for each MIR entry point ; phase-2 lowers MIR bodies
//!   to HLSL statements.

use core::fmt::Write as _;

/// One HLSL top-level statement (struct / cbuffer / fn-decl / raw pass-through).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlslStatement {
    /// `cbuffer Name : register(b0) { ... }` — represented as raw body text.
    CBuffer {
        name: String,
        body: String,
        register: Option<String>,
    },
    /// `struct Name { field1; field2; ... };`.
    Struct { name: String, fields: Vec<String> },
    /// `RWStructuredBuffer<T> Name : register(u0);`.
    RwBuffer {
        element_type: String,
        name: String,
        register: Option<String>,
    },
    /// A fn declaration : `ReturnType Name(params) : semantic { body }`.
    Function {
        return_type: String,
        name: String,
        params: Vec<String>,
        attributes: Vec<String>,
        semantic: Option<String>,
        body: Vec<String>,
    },
    /// Raw pass-through line (caller-formatted).
    Raw(String),
}

impl HlslStatement {
    /// Render this statement as HLSL source text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        match self {
            Self::CBuffer {
                name,
                body,
                register,
            } => {
                write!(out, "cbuffer {name}").unwrap();
                if let Some(r) = register {
                    write!(out, " : register({r})").unwrap();
                }
                writeln!(out, " {{\n{body}\n}};").unwrap();
            }
            Self::Struct { name, fields } => {
                writeln!(out, "struct {name} {{").unwrap();
                for f in fields {
                    writeln!(out, "    {f}").unwrap();
                }
                writeln!(out, "}};").unwrap();
            }
            Self::RwBuffer {
                element_type,
                name,
                register,
            } => {
                write!(out, "RWStructuredBuffer<{element_type}> {name}").unwrap();
                if let Some(r) = register {
                    write!(out, " : register({r})").unwrap();
                }
                writeln!(out, ";").unwrap();
            }
            Self::Function {
                return_type,
                name,
                params,
                attributes,
                semantic,
                body,
            } => {
                for a in attributes {
                    writeln!(out, "{a}").unwrap();
                }
                write!(out, "{return_type} {name}({})", params.join(", ")).unwrap();
                if let Some(s) = semantic {
                    write!(out, " : {s}").unwrap();
                }
                writeln!(out, " {{").unwrap();
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

/// Skeletal HLSL translation unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlslModule {
    /// Optional `#pragma` header comment block.
    pub header: Option<String>,
    /// Top-level statements in declaration order.
    pub statements: Vec<HlslStatement>,
}

impl HlslModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a statement.
    pub fn push(&mut self, s: HlslStatement) {
        self.statements.push(s);
    }

    /// Render the whole module as HLSL text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(h) = &self.header {
            writeln!(out, "{h}").unwrap();
            writeln!(out).unwrap();
        }
        for s in &self.statements {
            out.push_str(&s.render());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{HlslModule, HlslStatement};

    #[test]
    fn struct_statement_rendering() {
        let s = HlslStatement::Struct {
            name: "Vertex".into(),
            fields: vec![
                "float3 position : POSITION;".into(),
                "float2 uv : TEXCOORD;".into(),
            ],
        };
        let r = s.render();
        assert!(r.contains("struct Vertex"));
        assert!(r.contains("float3 position : POSITION;"));
    }

    #[test]
    fn function_statement_rendering() {
        let s = HlslStatement::Function {
            return_type: "void".into(),
            name: "main".into(),
            params: vec!["uint3 tid : SV_DispatchThreadID".into()],
            attributes: vec!["[numthreads(64, 1, 1)]".into()],
            semantic: None,
            body: vec!["// stage-0 skeleton".into()],
        };
        let r = s.render();
        assert!(r.contains("[numthreads(64, 1, 1)]"));
        assert!(r.contains("void main(uint3 tid : SV_DispatchThreadID)"));
    }

    #[test]
    fn rw_buffer_statement_rendering() {
        let s = HlslStatement::RwBuffer {
            element_type: "float4".into(),
            name: "OutBuf".into(),
            register: Some("u0".into()),
        };
        let r = s.render();
        assert!(r.contains("RWStructuredBuffer<float4> OutBuf : register(u0);"));
    }

    #[test]
    fn cbuffer_statement_rendering() {
        let s = HlslStatement::CBuffer {
            name: "Globals".into(),
            body: "    float4x4 view_proj;".into(),
            register: Some("b0".into()),
        };
        let r = s.render();
        assert!(r.contains("cbuffer Globals : register(b0) {"));
        assert!(r.contains("float4x4 view_proj;"));
    }

    #[test]
    fn module_assembly() {
        let mut m = HlslModule::new();
        m.header = Some("// autogenerated by cssl-cgen-gpu-dxil".into());
        m.push(HlslStatement::Raw("#define FOO 1".into()));
        m.push(HlslStatement::Function {
            return_type: "void".into(),
            name: "main".into(),
            params: vec![],
            attributes: vec!["[numthreads(1,1,1)]".into()],
            semantic: None,
            body: vec!["// empty".into()],
        });
        let r = m.render();
        assert!(r.contains("autogenerated by cssl-cgen-gpu-dxil"));
        assert!(r.contains("#define FOO 1"));
        assert!(r.contains("void main()"));
    }
}

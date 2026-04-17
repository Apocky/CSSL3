//! Skeletal MSL source builder.

use core::fmt::Write as _;

/// One MSL top-level statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MslStatement {
    /// `#include <metal_stdlib>` etc.
    Include(String),
    /// `using namespace metal;`.
    UsingNamespace(String),
    /// `struct Name { field; field; };`.
    Struct { name: String, fields: Vec<String> },
    /// `typedef ExistingType NewName;`.
    Typedef { existing: String, new: String },
    /// Entry function : `[[kernel]] return_type name(params) { body }`.
    Function {
        stage_attribute: Option<String>,
        return_type: String,
        name: String,
        params: Vec<String>,
        body: Vec<String>,
    },
    /// Raw pass-through line.
    Raw(String),
}

impl MslStatement {
    /// Render this statement as MSL text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        match self {
            Self::Include(h) => writeln!(out, "#include <{h}>").unwrap(),
            Self::UsingNamespace(ns) => writeln!(out, "using namespace {ns};").unwrap(),
            Self::Struct { name, fields } => {
                writeln!(out, "struct {name} {{").unwrap();
                for f in fields {
                    writeln!(out, "    {f}").unwrap();
                }
                writeln!(out, "}};").unwrap();
            }
            Self::Typedef { existing, new } => {
                writeln!(out, "typedef {existing} {new};").unwrap();
            }
            Self::Function {
                stage_attribute,
                return_type,
                name,
                params,
                body,
            } => {
                if let Some(a) = stage_attribute {
                    writeln!(out, "{a}").unwrap();
                }
                writeln!(out, "{return_type} {name}({}) {{", params.join(", ")).unwrap();
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

/// MSL translation unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MslModule {
    /// Optional generated-comment header.
    pub header: Option<String>,
    /// Top-level statements in declaration order.
    pub statements: Vec<MslStatement>,
}

impl MslModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a statement.
    pub fn push(&mut self, s: MslStatement) {
        self.statements.push(s);
    }

    /// Seed with the canonical MSL prelude (`#include <metal_stdlib>` + `using namespace metal;`).
    pub fn seed_prelude(&mut self) {
        self.push(MslStatement::Include("metal_stdlib".into()));
        self.push(MslStatement::UsingNamespace("metal".into()));
    }

    /// Render the whole module as MSL text.
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
    use super::{MslModule, MslStatement};

    #[test]
    fn include_statement_renders() {
        let s = MslStatement::Include("metal_stdlib".into());
        let r = s.render();
        assert!(r.contains("#include <metal_stdlib>"));
    }

    #[test]
    fn using_namespace_renders() {
        let s = MslStatement::UsingNamespace("metal".into());
        let r = s.render();
        assert!(r.contains("using namespace metal;"));
    }

    #[test]
    fn function_with_kernel_attribute_renders() {
        let s = MslStatement::Function {
            stage_attribute: Some("[[kernel]]".into()),
            return_type: "void".into(),
            name: "compute_main".into(),
            params: vec![
                "uint3 gid [[thread_position_in_grid]]".into(),
                "device float4* out [[buffer(0)]]".into(),
            ],
            body: vec!["// stage-0 skeleton".into()],
        };
        let r = s.render();
        assert!(r.contains("[[kernel]]"));
        assert!(r.contains("void compute_main(uint3 gid"));
        assert!(r.contains("device float4* out"));
    }

    #[test]
    fn struct_renders() {
        let s = MslStatement::Struct {
            name: "Vertex".into(),
            fields: vec!["float3 position;".into(), "float2 uv;".into()],
        };
        let r = s.render();
        assert!(r.contains("struct Vertex"));
        assert!(r.contains("float3 position;"));
    }

    #[test]
    fn module_prelude_seeds_stdlib() {
        let mut m = MslModule::new();
        m.seed_prelude();
        let r = m.render();
        assert!(r.contains("#include <metal_stdlib>"));
        assert!(r.contains("using namespace metal;"));
    }

    #[test]
    fn module_header_rendering() {
        let mut m = MslModule::new();
        m.header = Some("// generated".into());
        let r = m.render();
        assert!(r.starts_with("// generated"));
    }
}

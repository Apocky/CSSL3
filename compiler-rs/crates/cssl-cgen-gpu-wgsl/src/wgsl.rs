//! Skeletal WGSL source builder.

use core::fmt::Write as _;

/// One WGSL top-level statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgslStatement {
    /// `enable f16;` / `enable subgroups;` etc.
    Enable(String),
    /// `struct Name { @align(16) field : type, ... };`.
    Struct { name: String, fields: Vec<String> },
    /// `@group(g) @binding(b) var<storage, read_write> name : type;`.
    Binding {
        group: u32,
        binding: u32,
        address_space: String,
        access: Option<String>,
        name: String,
        ty: String,
    },
    /// Entry function : `@compute @workgroup_size(x,y,z) fn name(params) { body }`.
    EntryFunction {
        stage_attribute: String,
        workgroup_size: Option<(u32, u32, u32)>,
        return_type: Option<String>,
        name: String,
        params: Vec<String>,
        body: Vec<String>,
    },
    /// Helper function : `fn name(params) -> ret { body }`.
    HelperFunction {
        return_type: Option<String>,
        name: String,
        params: Vec<String>,
        body: Vec<String>,
    },
    /// Raw pass-through line.
    Raw(String),
}

impl WgslStatement {
    /// Render this statement as WGSL text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        match self {
            Self::Enable(name) => writeln!(out, "enable {name};").unwrap(),
            Self::Struct { name, fields } => {
                writeln!(out, "struct {name} {{").unwrap();
                for f in fields {
                    writeln!(out, "    {f},").unwrap();
                }
                writeln!(out, "}};").unwrap();
            }
            Self::Binding {
                group,
                binding,
                address_space,
                access,
                name,
                ty,
            } => {
                let space_str = access.as_ref().map_or_else(
                    || format!("<{address_space}>"),
                    |a| format!("<{address_space}, {a}>"),
                );
                writeln!(
                    out,
                    "@group({group}) @binding({binding}) var{space_str} {name} : {ty};"
                )
                .unwrap();
            }
            Self::EntryFunction {
                stage_attribute,
                workgroup_size,
                return_type,
                name,
                params,
                body,
            } => {
                write!(out, "{stage_attribute}").unwrap();
                if let Some((x, y, z)) = workgroup_size {
                    write!(out, " @workgroup_size({x}, {y}, {z})").unwrap();
                }
                writeln!(out).unwrap();
                write!(out, "fn {name}({})", params.join(", ")).unwrap();
                if let Some(r) = return_type {
                    write!(out, " -> {r}").unwrap();
                }
                writeln!(out, " {{").unwrap();
                for line in body {
                    writeln!(out, "    {line}").unwrap();
                }
                writeln!(out, "}}").unwrap();
            }
            Self::HelperFunction {
                return_type,
                name,
                params,
                body,
            } => {
                write!(out, "fn {name}({})", params.join(", ")).unwrap();
                if let Some(r) = return_type {
                    write!(out, " -> {r}").unwrap();
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

/// WGSL translation unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WgslModule {
    /// Optional generated-comment header.
    pub header: Option<String>,
    /// Top-level statements in declaration order.
    pub statements: Vec<WgslStatement>,
}

impl WgslModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a statement.
    pub fn push(&mut self, s: WgslStatement) {
        self.statements.push(s);
    }

    /// Render the whole module as WGSL text.
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
    use super::{WgslModule, WgslStatement};

    #[test]
    fn enable_directive_renders() {
        let s = WgslStatement::Enable("f16".into());
        let r = s.render();
        assert!(r.contains("enable f16;"));
    }

    #[test]
    fn struct_renders() {
        let s = WgslStatement::Struct {
            name: "Particle".into(),
            fields: vec![
                "@align(16) position : vec3<f32>".into(),
                "velocity : vec3<f32>".into(),
            ],
        };
        let r = s.render();
        assert!(r.contains("struct Particle"));
        assert!(r.contains("position : vec3<f32>,"));
    }

    #[test]
    fn binding_renders() {
        let s = WgslStatement::Binding {
            group: 0,
            binding: 2,
            address_space: "storage".into(),
            access: Some("read_write".into()),
            name: "particles".into(),
            ty: "array<Particle>".into(),
        };
        let r = s.render();
        assert!(r.contains(
            "@group(0) @binding(2) var<storage, read_write> particles : array<Particle>;"
        ));
    }

    #[test]
    fn entry_function_compute_renders() {
        let s = WgslStatement::EntryFunction {
            stage_attribute: "@compute".into(),
            workgroup_size: Some((64, 1, 1)),
            return_type: None,
            name: "main".into(),
            params: vec!["@builtin(global_invocation_id) gid : vec3<u32>".into()],
            body: vec!["// stage-0 skeleton".into()],
        };
        let r = s.render();
        assert!(r.contains("@compute @workgroup_size(64, 1, 1)"));
        assert!(r.contains("fn main(@builtin(global_invocation_id) gid : vec3<u32>)"));
    }

    #[test]
    fn entry_function_vertex_renders() {
        let s = WgslStatement::EntryFunction {
            stage_attribute: "@vertex".into(),
            workgroup_size: None,
            return_type: Some("@builtin(position) vec4<f32>".into()),
            name: "main_vs".into(),
            params: vec!["@builtin(vertex_index) vid : u32".into()],
            body: vec!["return vec4<f32>(0.0);".into()],
        };
        let r = s.render();
        assert!(r.contains("@vertex\n"));
        assert!(r.contains(
            "fn main_vs(@builtin(vertex_index) vid : u32) -> @builtin(position) vec4<f32>"
        ));
    }

    #[test]
    fn helper_function_renders() {
        let s = WgslStatement::HelperFunction {
            return_type: Some("f32".into()),
            name: "add".into(),
            params: vec!["a : f32".into(), "b : f32".into()],
            body: vec!["return a + b;".into()],
        };
        let r = s.render();
        assert!(r.contains("fn add(a : f32, b : f32) -> f32"));
        assert!(r.contains("return a + b;"));
    }

    #[test]
    fn module_header_rendering() {
        let mut m = WgslModule::new();
        m.header = Some("// generated".into());
        let r = m.render();
        assert!(r.starts_with("// generated"));
    }
}

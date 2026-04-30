//! WGSL entry-point shape, builtin variables, and binding decorators.
//!
//! § SPEC : W3C "WebGPU Shading Language" §§ Pipeline-creation +
//!         §§ Built-in Values + §§ Resource Binding.
//!
//! Three entry-point kinds are first-class : `@compute`, `@vertex`,
//! `@fragment`. Each entry-point declares :
//!
//!   - its stage attribute,
//!   - optional `@workgroup_size(X, Y, Z)` (compute only),
//!   - parameters with `@builtin(...)` or `@location(...)` decorators,
//!   - a return type with `@builtin(...)` or `@location(...)` decorator
//!     (vertex / fragment).
//!
//! Resource bindings are *module-level* declarations of the form
//! `@group(N) @binding(M) var<addr_space, access> name : type;`, declared
//! once at module scope and referenced by name from inside entry-points.

use core::fmt;

use crate::types::WgslType;

/// Kind of WGSL entry-point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryPointKind {
    /// `@compute @workgroup_size(X, Y, Z)`.
    Compute { wg_x: u32, wg_y: u32, wg_z: u32 },
    /// `@vertex` — output is `@builtin(position) vec4<f32>`.
    Vertex,
    /// `@fragment` — output is `@location(0) vec4<f32>`.
    Fragment,
}

impl EntryPointKind {
    /// Render the WGSL stage-attribute(s) prefix string.
    /// e.g., `"@compute @workgroup_size(64, 1, 1)"` or `"@vertex"`.
    #[must_use]
    pub fn attr(&self) -> String {
        match self {
            Self::Compute { wg_x, wg_y, wg_z } => {
                format!("@compute @workgroup_size({wg_x}, {wg_y}, {wg_z})")
            }
            Self::Vertex => "@vertex".to_string(),
            Self::Fragment => "@fragment".to_string(),
        }
    }

    /// `true` iff this is a compute entry-point.
    #[must_use]
    pub const fn is_compute(&self) -> bool {
        matches!(self, Self::Compute { .. })
    }
}

/// WGSL `@builtin(...)` value-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    /// `@builtin(position)` — vec4<f32> ; vertex-output / fragment-input.
    Position,
    /// `@builtin(global_invocation_id)` — vec3<u32> ; compute.
    GlobalInvocationId,
    /// `@builtin(local_invocation_id)` — vec3<u32> ; compute.
    LocalInvocationId,
    /// `@builtin(workgroup_id)` — vec3<u32> ; compute.
    WorkgroupId,
    /// `@builtin(num_workgroups)` — vec3<u32> ; compute.
    NumWorkgroups,
    /// `@builtin(vertex_index)` — u32 ; vertex.
    VertexIndex,
    /// `@builtin(instance_index)` — u32 ; vertex.
    InstanceIndex,
    /// `@builtin(front_facing)` — bool ; fragment.
    FrontFacing,
    /// `@builtin(sample_index)` — u32 ; fragment.
    SampleIndex,
}

impl Builtin {
    /// Canonical WGSL builtin-name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::GlobalInvocationId => "global_invocation_id",
            Self::LocalInvocationId => "local_invocation_id",
            Self::WorkgroupId => "workgroup_id",
            Self::NumWorkgroups => "num_workgroups",
            Self::VertexIndex => "vertex_index",
            Self::InstanceIndex => "instance_index",
            Self::FrontFacing => "front_facing",
            Self::SampleIndex => "sample_index",
        }
    }

    /// Required WGSL surface-type for this builtin.
    #[must_use]
    pub fn required_type(self) -> WgslType {
        match self {
            Self::Position => WgslType::VecF32(4),
            Self::GlobalInvocationId
            | Self::LocalInvocationId
            | Self::WorkgroupId
            | Self::NumWorkgroups => WgslType::VecU32(3),
            Self::VertexIndex | Self::InstanceIndex | Self::SampleIndex => WgslType::U32,
            Self::FrontFacing => WgslType::Bool,
        }
    }
}

impl fmt::Display for Builtin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@builtin({})", self.name())
    }
}

/// Address-space + access for a resource binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKind {
    /// `var<storage, read>` — read-only storage buffer.
    StorageRead,
    /// `var<storage, read_write>` — RW storage buffer.
    StorageReadWrite,
    /// `var<uniform>` — uniform buffer.
    Uniform,
    /// `var` (default) — sampled-texture / sampler / storage-texture handle.
    Resource,
}

impl BindingKind {
    /// Render the WGSL `var<...>` qualifier (without trailing space).
    #[must_use]
    pub const fn var_qualifier(self) -> &'static str {
        match self {
            Self::StorageRead => "var<storage, read>",
            Self::StorageReadWrite => "var<storage, read_write>",
            Self::Uniform => "var<uniform>",
            Self::Resource => "var",
        }
    }
}

/// A module-level resource binding.
#[derive(Debug, Clone)]
pub struct Binding {
    /// `@group(N)`.
    pub group: u32,
    /// `@binding(M)`.
    pub binding: u32,
    /// Address-space + access kind.
    pub kind: BindingKind,
    /// Identifier (must be a valid WGSL ident).
    pub name: String,
    /// WGSL surface-type.
    pub ty: WgslType,
}

impl Binding {
    /// Render as a WGSL module-level declaration line (no trailing newline).
    #[must_use]
    pub fn to_decl(&self) -> String {
        format!(
            "@group({g}) @binding({b}) {q} {n} : {t};",
            g = self.group,
            b = self.binding,
            q = self.kind.var_qualifier(),
            n = self.name,
            t = self.ty,
        )
    }
}

/// Header (top-of-source) declarations for a WGSL module.
#[derive(Debug, Clone, Default)]
pub struct ShaderHeader {
    /// `enable f16;` and similar feature-enables.
    pub enables: Vec<String>,
    /// Module-level resource bindings.
    pub bindings: Vec<Binding>,
}

impl ShaderHeader {
    /// Produce the `enable ...;` lines.
    #[must_use]
    pub fn enables_block(&self) -> String {
        if self.enables.is_empty() {
            String::new()
        } else {
            self.enables
                .iter()
                .map(|e| format!("enable {e};"))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
    }

    /// Produce the bindings block.
    #[must_use]
    pub fn bindings_block(&self) -> String {
        if self.bindings.is_empty() {
            String::new()
        } else {
            self.bindings
                .iter()
                .map(Binding::to_decl)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_point_attr_strings_render() {
        assert_eq!(
            EntryPointKind::Compute { wg_x: 64, wg_y: 1, wg_z: 1 }.attr(),
            "@compute @workgroup_size(64, 1, 1)"
        );
        assert_eq!(EntryPointKind::Vertex.attr(), "@vertex");
        assert_eq!(EntryPointKind::Fragment.attr(), "@fragment");
    }

    #[test]
    fn builtin_required_types_match_spec() {
        assert_eq!(Builtin::Position.required_type(), WgslType::VecF32(4));
        assert_eq!(
            Builtin::GlobalInvocationId.required_type(),
            WgslType::VecU32(3)
        );
        assert_eq!(Builtin::VertexIndex.required_type(), WgslType::U32);
        assert_eq!(Builtin::FrontFacing.required_type(), WgslType::Bool);
    }

    #[test]
    fn builtin_display_form() {
        assert_eq!(Builtin::Position.to_string(), "@builtin(position)");
        assert_eq!(
            Builtin::GlobalInvocationId.to_string(),
            "@builtin(global_invocation_id)"
        );
    }

    #[test]
    fn binding_decl_format() {
        let b = Binding {
            group: 0,
            binding: 1,
            kind: BindingKind::StorageReadWrite,
            name: "buf".into(),
            ty: WgslType::Array { elem: Box::new(WgslType::F32), len: None },
        };
        assert_eq!(
            b.to_decl(),
            "@group(0) @binding(1) var<storage, read_write> buf : array<f32>;"
        );
    }

    #[test]
    fn binding_kind_qualifiers() {
        assert_eq!(BindingKind::StorageRead.var_qualifier(), "var<storage, read>");
        assert_eq!(
            BindingKind::StorageReadWrite.var_qualifier(),
            "var<storage, read_write>"
        );
        assert_eq!(BindingKind::Uniform.var_qualifier(), "var<uniform>");
        assert_eq!(BindingKind::Resource.var_qualifier(), "var");
    }

    #[test]
    fn header_blocks_render() {
        let h = ShaderHeader {
            enables: vec!["f16".into()],
            bindings: vec![Binding {
                group: 0,
                binding: 0,
                kind: BindingKind::Uniform,
                name: "params".into(),
                ty: WgslType::VecF32(4),
            }],
        };
        assert_eq!(h.enables_block(), "enable f16;\n");
        assert_eq!(
            h.bindings_block(),
            "@group(0) @binding(0) var<uniform> params : vec4<f32>;\n"
        );
    }
}

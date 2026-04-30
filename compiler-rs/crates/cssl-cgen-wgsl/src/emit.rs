//! High-level driver : `MirModule` → WGSL source-string text.
//!
//! § DESIGN
//!
//! Walks the module-level fns, picks an `EntryPointKind` per fn (driven
//! by either `EmitConfig.fn_stages` or by a heuristic on the fn name),
//! and emits :
//!
//! ```text
//! 1. enable …; lines (from EmitConfig.enables)
//! 2. Module-level @group(N) @binding(M) var<…> name : ty; decls
//!    (from EmitConfig.bindings)
//! 3. One WGSL function per MirFunc via lower::lower_fn.
//! ```
//!
//! The output is a single `String` of valid WGSL source. The browser /
//! wgpu host hands this string to `device.createShaderModule({ code })`
//! for compilation by the platform's WGSL compiler.

use core::fmt::Write as _;
use std::collections::HashMap;

use cssl_mir::func::MirModule;
use thiserror::Error;

use crate::lower::{lower_fn, FnLowerError};
use crate::shader::{Binding, EntryPointKind, ShaderHeader};
use crate::DEFAULT_COMPUTE_WG;

/// Configuration for [`emit_wgsl_source`].
#[derive(Debug, Clone, Default)]
pub struct EmitConfig {
    /// `enable …;` directives (e.g., `["f16"]`).
    pub enables: Vec<String>,
    /// Module-level resource bindings.
    pub bindings: Vec<Binding>,
    /// Per-fn entry-point kind. Fn-name → kind. Falls back to
    /// [`EntryPointKind::Compute`] with `DEFAULT_COMPUTE_WG` when absent.
    pub fn_stages: HashMap<String, EntryPointKind>,
}

impl EmitConfig {
    /// Construct an empty default config (all-compute, no bindings).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builder : assign a stage to a fn-name. Returns `self` for chaining.
    #[must_use]
    pub fn with_stage(mut self, fn_name: impl Into<String>, kind: EntryPointKind) -> Self {
        self.fn_stages.insert(fn_name.into(), kind);
        self
    }

    /// Builder : add a binding.
    #[must_use]
    pub fn with_binding(mut self, b: Binding) -> Self {
        self.bindings.push(b);
        self
    }

    /// Builder : add an `enable …;`.
    #[must_use]
    pub fn with_enable(mut self, name: impl Into<String>) -> Self {
        self.enables.push(name.into());
        self
    }
}

/// Top-level error from [`emit_wgsl_source`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EmitError {
    /// A fn body could not be lowered.
    #[error("function `{fn_name}` failed to lower : {source}")]
    Function {
        fn_name: String,
        #[source]
        source: FnLowerError,
    },
    /// Empty module — nothing to emit.
    #[error("module has no functions to emit")]
    EmptyModule,
}

/// Emit a complete WGSL source-string for `module` under `config`.
///
/// § ERRORS · returns [`EmitError::Function`] on per-fn lowering failure or
/// [`EmitError::EmptyModule`] if the module has no fns.
pub fn emit_wgsl_source(module: &MirModule, config: &EmitConfig) -> Result<String, EmitError> {
    if module.funcs.is_empty() {
        return Err(EmitError::EmptyModule);
    }

    let header = ShaderHeader {
        enables: config.enables.clone(),
        bindings: config.bindings.clone(),
    };

    let mut out = String::new();

    // 1) Banner — informational only ; valid WGSL comment.
    writeln!(
        &mut out,
        "// CSSLv3 cssl-cgen-wgsl — emitted WGSL source (T11-D270 / W-G4)"
    )
    .unwrap();
    writeln!(
        &mut out,
        "// module : `{}`",
        module.name.as_deref().unwrap_or("<anon>")
    )
    .unwrap();
    writeln!(&mut out, "// fns    : {}", module.funcs.len()).unwrap();

    // 2) Enables.
    let enables = header.enables_block();
    if !enables.is_empty() {
        out.push('\n');
        out.push_str(&enables);
    }

    // 3) Bindings.
    let bindings = header.bindings_block();
    if !bindings.is_empty() {
        out.push('\n');
        out.push_str(&bindings);
    }

    // 4) Per-fn emission.
    for func in &module.funcs {
        let kind = config.fn_stages.get(&func.name).copied().unwrap_or_else(|| {
            // Heuristic : fn-name prefix → stage.
            let name = func.name.as_str();
            if name.starts_with("vs_") || name == "vs_main" {
                EntryPointKind::Vertex
            } else if name.starts_with("fs_") || name == "fs_main" {
                EntryPointKind::Fragment
            } else {
                EntryPointKind::Compute {
                    wg_x: DEFAULT_COMPUTE_WG.0,
                    wg_y: DEFAULT_COMPUTE_WG.1,
                    wg_z: DEFAULT_COMPUTE_WG.2,
                }
            }
        });

        out.push('\n');
        let lowered = lower_fn(func, kind).map_err(|source| EmitError::Function {
            fn_name: func.name.clone(),
            source,
        })?;
        out.push_str(&lowered);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader::{BindingKind, EntryPointKind};
    use crate::types::WgslType;
    use cssl_mir::block::MirRegion;
    use cssl_mir::func::{MirFunc, MirModule};
    use cssl_mir::value::{FloatWidth, IntWidth, MirType};

    fn mk_module(name: &str) -> MirModule {
        MirModule {
            name: Some(name.into()),
            funcs: Vec::new(),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn header_emits_banner_module_name_fn_count() {
        let mut m = mk_module("test_mod");
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        let cfg = EmitConfig::empty();
        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("// CSSLv3 cssl-cgen-wgsl"));
        assert!(src.contains("// module : `test_mod`"));
        assert!(src.contains("// fns    : 1"));
    }

    #[test]
    fn type_mapping_renders_in_emitted_source() {
        let mut m = mk_module("types");
        let mut f = MirFunc::new(
            "kernel",
            vec![MirType::Float(FloatWidth::F32), MirType::Int(IntWidth::I32)],
            vec![],
        );
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        let cfg = EmitConfig::empty();
        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("p0 : f32"));
        assert!(src.contains("p1 : i32"));
    }

    #[test]
    fn compute_entry_point_emits_workgroup_size() {
        let mut m = mk_module("compute_mod");
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        let cfg = EmitConfig::empty().with_stage(
            "kernel",
            EntryPointKind::Compute { wg_x: 256, wg_y: 1, wg_z: 1 },
        );
        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("@compute @workgroup_size(256, 1, 1)"));
        assert!(src.contains("fn kernel("));
    }

    #[test]
    fn vertex_entry_point_emits_vertex_attr_and_position_return() {
        let mut m = mk_module("vert_mod");
        let mut f = MirFunc::new("vs_main", vec![MirType::Int(IntWidth::Index)], vec![]);
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        // No explicit stage — heuristic should pick Vertex via vs_main name.
        let cfg = EmitConfig::empty();
        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("@vertex fn vs_main("));
        assert!(src.contains("@builtin(vertex_index) p0 : u32"));
        assert!(src.contains("-> @builtin(position) vec4<f32>"));
    }

    #[test]
    fn fragment_entry_point_emits_fragment_attr_and_location_return() {
        let mut m = mk_module("frag_mod");
        let mut f = MirFunc::new("fs_main", vec![], vec![]);
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        let cfg = EmitConfig::empty();
        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("@fragment fn fs_main("));
        assert!(src.contains("-> @location(0) vec4<f32>"));
        assert!(src.contains("return vec4<f32>(1.0, 1.0, 1.0, 1.0);"));
    }

    #[test]
    fn binding_decorators_emitted_in_module_header() {
        let mut m = mk_module("binds");
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        let cfg = EmitConfig::empty()
            .with_binding(Binding {
                group: 0,
                binding: 0,
                kind: BindingKind::Uniform,
                name: "params".into(),
                ty: WgslType::VecF32(4),
            })
            .with_binding(Binding {
                group: 0,
                binding: 1,
                kind: BindingKind::StorageReadWrite,
                name: "buf".into(),
                ty: WgslType::Array { elem: Box::new(WgslType::F32), len: None },
            });

        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("@group(0) @binding(0) var<uniform> params : vec4<f32>;"));
        assert!(src.contains(
            "@group(0) @binding(1) var<storage, read_write> buf : array<f32>;"
        ));
    }

    #[test]
    fn empty_module_errors() {
        let m = mk_module("empty");
        let err = emit_wgsl_source(&m, &EmitConfig::empty()).unwrap_err();
        assert_eq!(err, EmitError::EmptyModule);
    }

    #[test]
    fn enable_directive_renders_when_set() {
        let mut m = mk_module("enable_test");
        let mut f = MirFunc::new("kernel", vec![], vec![]);
        f.body = MirRegion::with_entry(vec![]);
        m.funcs.push(f);

        let cfg = EmitConfig::empty().with_enable("f16");
        let src = emit_wgsl_source(&m, &cfg).unwrap();
        assert!(src.contains("enable f16;"));
    }
}

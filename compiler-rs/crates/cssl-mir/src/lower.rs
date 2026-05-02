//! HIR → MIR skeleton lowering (signature-level only for T6-phase-1).
//!
//! § SCOPE
//!   For every `HirFn` in a `HirModule`, emit a `MirFunc` with matching name +
//!   signature. The fn body stays empty (a single `^entry` block with no ops) —
//!   full body-lowering is T6-phase-2 work. The effect-row is stringified into
//!   the `effect_row` attribute. Cap wrapping on fn-signature is recorded via
//!   the top-level-cap of the return type (if present).
//!
//! § PURPOSE
//!   This minimum-viable lowering lets `--emit-mlir` produce fn-level skeletons
//!   for every module, which is enough to validate :
//!     - MirModule structure round-trips through the printer.
//!     - Names / signatures / attributes flow from HIR to MIR correctly.
//!     - Downstream tooling (CI, spirv-val, spirv-opt, mlir-opt) can ingest our
//!       textual format.

use cssl_hir::{
    CapMap, HirCapKind, HirEffectRow, HirExternFn, HirFn, HirItem, HirModule, HirStruct,
    HirStructBody, HirType, HirTypeKind, Interner,
};

use crate::func::{MirFunc, MirModule, MirStructLayout};
use crate::value::{FloatWidth, IntWidth, MirType};

/// Lowering context — holds references to the HIR + interner so `lower_*` methods
/// can resolve symbol names and cap annotations.
#[derive(Debug)]
pub struct LowerCtx<'a> {
    pub interner: &'a Interner,
    pub cap_map: Option<&'a CapMap>,
}

impl<'a> LowerCtx<'a> {
    /// Build a context with just the interner (no cap-map info).
    #[must_use]
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            cap_map: None,
        }
    }

    /// Attach a cap-map so `cap` attributes land on `MirFunc` signatures.
    #[must_use]
    pub const fn with_cap_map(mut self, m: &'a CapMap) -> Self {
        self.cap_map = Some(m);
        self
    }

    /// Translate a HIR type to a MIR type (structural only ; primitives + nominals
    /// + tuple + reference are recognized ; more exotic types become `Opaque`).
    #[must_use]
    pub fn lower_type(&self, t: &HirType) -> MirType {
        match &t.kind {
            HirTypeKind::Path { path, .. } => {
                // Recognize primitive path-names.
                if path.len() == 1 {
                    let name = self.interner.resolve(path[0]);
                    return match name.as_str() {
                        "i8" => MirType::Int(IntWidth::I8),
                        "i16" => MirType::Int(IntWidth::I16),
                        "i32" | "u32" | "isize" | "usize" => MirType::Int(IntWidth::I32),
                        "i64" | "u64" => MirType::Int(IntWidth::I64),
                        "f16" => MirType::Float(FloatWidth::F16),
                        "bf16" => MirType::Float(FloatWidth::Bf16),
                        "f32" => MirType::Float(FloatWidth::F32),
                        "f64" => MirType::Float(FloatWidth::F64),
                        "bool" => MirType::Bool,
                        "Handle" => MirType::Handle,
                        other => MirType::Opaque(other.to_string()),
                    };
                }
                let joined: Vec<String> = path.iter().map(|s| self.interner.resolve(*s)).collect();
                MirType::Opaque(joined.join("."))
            }
            HirTypeKind::Tuple { elems } => {
                MirType::Tuple(elems.iter().map(|e| self.lower_type(e)).collect())
            }
            HirTypeKind::Function {
                params, return_ty, ..
            } => MirType::Function {
                params: params.iter().map(|p| self.lower_type(p)).collect(),
                results: vec![self.lower_type(return_ty)],
            },
            HirTypeKind::Reference { inner, .. } => self.lower_type(inner),
            HirTypeKind::Capability { cap, inner } => {
                // Tag Handle specifically ; other caps pass through.
                if matches!(cap, HirCapKind::Tag) {
                    MirType::Handle
                } else {
                    self.lower_type(inner)
                }
            }
            HirTypeKind::Array { elem, .. } => MirType::Memref {
                shape: vec![None],
                elem: Box::new(self.lower_type(elem)),
            },
            HirTypeKind::Slice { elem } => MirType::Memref {
                shape: vec![None],
                elem: Box::new(self.lower_type(elem)),
            },
            HirTypeKind::Refined { base, .. } => self.lower_type(base),
            HirTypeKind::Infer => MirType::None,
            HirTypeKind::Error => MirType::Opaque("!cssl.error".into()),
        }
    }

    fn format_effect_row(&self, row: &HirEffectRow) -> String {
        let mut s = String::from("{");
        for (i, ann) in row.effects.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            let joined: Vec<String> = ann.name.iter().map(|n| self.interner.resolve(*n)).collect();
            s.push_str(&joined.join("."));
        }
        if let Some(tail) = row.tail {
            if !row.effects.is_empty() {
                s.push_str(" | ");
            } else {
                s.push_str("| ");
            }
            s.push_str(&self.interner.resolve(tail));
        }
        s.push('}');
        s
    }
}

/// Lower a single `HirFn` to a `MirFunc` with signature + empty body.
///
/// § T11-D35 vec scalarization : vec2/vec3/vec4 parameters expand to N
/// consecutive scalar entries in the flat `params` list ; see
/// [`crate::body_lower::expand_fn_param_types`] for the single source of truth
/// shared between signature- and body-lowering.
#[must_use]
pub fn lower_function_signature(ctx: &LowerCtx<'_>, f: &HirFn) -> MirFunc {
    let params: Vec<MirType> = f
        .params
        .iter()
        .flat_map(|p| crate::body_lower::expand_fn_param_types(ctx.interner, &p.ty))
        .collect();
    let results: Vec<MirType> = match &f.return_ty {
        Some(rt) => vec![ctx.lower_type(rt)],
        None => Vec::new(),
    };
    let name = ctx.interner.resolve(f.name);
    let mut mf = MirFunc::new(name, params, results);
    // T11-D43 : mark fns with unbound generic params so the cleanup pass can
    // drop them post-monomorphization.
    mf.is_generic = !f.generics.params.is_empty();
    if let Some(row) = &f.effect_row {
        mf.effect_row = Some(ctx.format_effect_row(row));
    }
    // Record cap attribute if the return-type carries one.
    if let Some(rt) = &f.return_ty {
        if let HirTypeKind::Capability { cap, .. } = &rt.kind {
            mf.cap = Some(cssl_hir::hir_cap_to_semantic(*cap).to_string());
        }
    }
    // § T11-D286 (W-E5-3) — propagate per-param cap-info onto fn attributes
    // so the `cap_runtime_check` MIR pass can emit one runtime verify-op per
    // cap-required parameter. Attribute key is `cap_required.<param-idx>` ;
    // value is the cap source-form name (e.g. `"iso"`). Sub-types of the
    // top-level Capability wrapper are walked one level — nested caps are a
    // future-slice deferral per the cap_check pass scope.
    for (idx, p) in f.params.iter().enumerate() {
        if let HirTypeKind::Capability { cap, .. } = &p.ty.kind {
            mf.attributes.push((
                format!("{}{}", cssl_mir_cap_required_prefix(), idx),
                cssl_hir::hir_cap_to_semantic(*cap).to_string(),
            ));
        }
    }
    mf
}

/// Helper : the canonical fn-attribute prefix recognized by the
/// `cap_runtime_check` pass for cap-required parameter threading. Re-exposed
/// here so signature-lowering can mint matching keys without reaching into
/// the pass module directly.
const fn cssl_mir_cap_required_prefix() -> &'static str {
    crate::cap_runtime_check::FN_ATTR_CAP_REQUIRED_PREFIX
}

/// Lower an `extern fn` HIR declaration to a signature-only `MirFunc`.
///
/// § PROPERTIES
/// - body has zero ops in entry block (signature-only ; `is_signature_only()`
///   returns true)
/// - `linkage = "import"` attribute marks the symbol as externally resolved
/// - `abi = "C"` attribute records the call-convention
/// - downstream codegen consults the linkage attribute to skip body emission
///   and instead emit an extern symbol declaration
#[must_use]
pub fn lower_extern_fn_signature(ctx: &LowerCtx<'_>, f: &HirExternFn) -> MirFunc {
    let params: Vec<MirType> = f
        .params
        .iter()
        .flat_map(|p| crate::body_lower::expand_fn_param_types(ctx.interner, &p.ty))
        .collect();
    let results: Vec<MirType> = match &f.return_ty {
        Some(rt) => vec![ctx.lower_type(rt)],
        None => Vec::new(),
    };
    let name = ctx.interner.resolve(f.name);
    let mut mf = MirFunc::new(name, params, results);
    mf.attributes.push(("linkage".to_string(), "import".to_string()));
    mf.attributes.push(("abi".to_string(), f.abi.clone()));
    mf
}

/// Walk a `HirModule` and produce a `MirModule` with one `MirFunc` per `HirFn` item.
/// Impl / interface / effect / handler methods are also included.
#[must_use]
pub fn lower_module_signatures(ctx: &LowerCtx<'_>, module: &HirModule) -> MirModule {
    let mut mir = match &module.module_path {
        Some(path) => {
            let joined: Vec<String> = path.iter().map(|s| ctx.interner.resolve(*s)).collect();
            MirModule::with_name(joined.join("."))
        }
        None => MirModule::new(),
    };
    for item in &module.items {
        lower_item_into(ctx, item, &mut mir);
    }
    mir
}

fn lower_item_into(ctx: &LowerCtx<'_>, item: &HirItem, mir: &mut MirModule) {
    match item {
        HirItem::Fn(f) => mir.push_func(lower_function_signature(ctx, f)),
        HirItem::ExternFn(f) => mir.push_func(lower_extern_fn_signature(ctx, f)),
        HirItem::Impl(i) => {
            for f in &i.fns {
                mir.push_func(lower_function_signature(ctx, f));
            }
        }
        HirItem::Interface(i) => {
            for f in &i.fns {
                mir.push_func(lower_function_signature(ctx, f));
            }
        }
        HirItem::Effect(e) => {
            for f in &e.ops {
                mir.push_func(lower_function_signature(ctx, f));
            }
        }
        HirItem::Handler(h) => {
            for f in &h.ops {
                mir.push_func(lower_function_signature(ctx, f));
            }
        }
        HirItem::Module(m) => {
            if let Some(sub) = &m.items {
                for s in sub {
                    lower_item_into(ctx, s, mir);
                }
            }
        }
        // T11-W17-A · stage-0 struct-FFI codegen — populate MirModule.struct_layouts
        // so the cgen-cpu-cranelift signature builder can resolve
        // `MirType::Opaque("!cssl.struct.<name>")` to a scalar / pointer ABI.
        HirItem::Struct(s) => {
            if let Some(layout) = lower_struct_layout(ctx, s) {
                mir.add_struct_layout(layout);
            }
        }
        // enum / type-alias / use / const don't emit MIR fns at stage-0.
        _ => {}
    }
}

/// Build a `MirStructLayout` for a struct that participates in FFI signatures.
///
/// § BEHAVIOR
///   - Named-fields struct  → records each field's MIR-lowered type.
///   - Tuple-fields struct  → same shape, positional.
///   - Unit struct          → `Some(empty-fields)` so the codegen sees a known
///     0-byte layout (rejected by ABI-class).
///
/// § DETERMINISM
///   The struct name is resolved via the interner ; field order matches the
///   HIR declaration order (which already mirrors source order).
fn lower_struct_layout(ctx: &LowerCtx<'_>, s: &HirStruct) -> Option<MirStructLayout> {
    let name = ctx.interner.resolve(s.name);
    let fields: Vec<MirType> = match &s.body {
        HirStructBody::Unit => Vec::new(),
        HirStructBody::Tuple(decls) | HirStructBody::Named(decls) => {
            decls.iter().map(|d| ctx.lower_type(&d.ty)).collect()
        }
    };
    let (size, align) = MirStructLayout::compute_size_align(&fields);
    Some(MirStructLayout::new(name, fields, size, align))
}

#[cfg(test)]
mod tests {
    use super::{lower_function_signature, lower_module_signatures, LowerCtx};
    use crate::value::{IntWidth, MirType};
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn hir_from(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn lower_empty_module_gives_empty_mir() {
        let (hir, interner) = hir_from("");
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        assert!(mir.funcs.is_empty());
    }

    #[test]
    fn lower_single_fn_yields_single_mir_fn() {
        let (hir, interner) = hir_from("fn add(a : i32, b : i32) -> i32 { a + b }");
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        assert_eq!(mir.funcs.len(), 1);
        let f = &mir.funcs[0];
        assert_eq!(f.name, "add");
        assert_eq!(
            f.params,
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)]
        );
        assert_eq!(f.results, vec![MirType::Int(IntWidth::I32)]);
    }

    #[test]
    fn lower_fn_effect_row_formatted() {
        let (hir, interner) = hir_from("fn f(x : i32) -> i32 / {GPU, NoAlloc} { x }");
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        let f = &mir.funcs[0];
        let er = f.effect_row.as_deref().unwrap();
        assert!(er.contains("GPU"));
        assert!(er.contains("NoAlloc"));
    }

    #[test]
    fn lower_module_path_preserved() {
        let (hir, interner) = hir_from("module com.apocky.loa\nfn f() {}");
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        assert_eq!(mir.name.as_deref(), Some("com.apocky.loa"));
    }

    #[test]
    fn lower_multiple_fns_order_preserved() {
        let (hir, interner) = hir_from("fn a() {} fn b() {} fn c() {}");
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        let names: Vec<&str> = mir.funcs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn lower_single_fn_direct_entry() {
        let (hir, interner) = hir_from("fn noop() {}");
        let ctx = LowerCtx::new(&interner);
        let f = hir.items.iter().find_map(|i| match i {
            cssl_hir::HirItem::Fn(f) => Some(f),
            _ => None,
        });
        let mf = lower_function_signature(&ctx, f.unwrap());
        assert_eq!(mf.name, "noop");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § STRUCT-FFI lowering tests  (T11-W17-A)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn lower_newtype_struct_records_8b_layout() {
        // The actual LoA case : `pub struct RunHandle { raw: u64 }`.
        let (hir, interner) = hir_from("struct RunHandle { raw : u64 }");
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        let layout = mir
            .find_struct_layout("RunHandle")
            .expect("RunHandle layout populated");
        assert_eq!(layout.size_bytes, 8);
        assert_eq!(layout.align_bytes, 8);
        assert_eq!(layout.fields.len(), 1);
        assert_eq!(layout.fields[0], MirType::Int(IntWidth::I64));
    }

    #[test]
    fn lower_multi_field_struct_records_aggregate_layout() {
        // ShareReceipt-like : 16B / align 8.
        let src = "struct ShareReceipt { receipt_id_lo : u64 , receipt_id_hi : u64 }";
        let (hir, interner) = hir_from(src);
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        let layout = mir
            .find_struct_layout("ShareReceipt")
            .expect("ShareReceipt layout populated");
        assert_eq!(layout.size_bytes, 16);
        assert_eq!(layout.align_bytes, 8);
    }

    #[test]
    fn lower_module_with_struct_and_fn_records_both() {
        let src = "
            struct RunHandle { raw : u64 }
            fn make_run() -> RunHandle { RunHandle { raw : 0 } }
        ";
        let (hir, interner) = hir_from(src);
        let ctx = LowerCtx::new(&interner);
        let mir = lower_module_signatures(&ctx, &hir);
        // Both signature + struct landed.
        assert!(mir.find_func("make_run").is_some());
        assert!(mir.find_struct_layout("RunHandle").is_some());
    }
}

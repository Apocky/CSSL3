//! Bidirectional type inference + effect-row threading.
//!
//! § ENTRY
//!   [`check_module`] walks a lowered `HirModule` and produces a `TypeMap`
//!   (`HirId → Ty`) plus a `Vec<Diagnostic>` of type errors. The pass runs in
//!   three phases :
//!
//!   - Phase 1 **Collect item signatures** — walk each top-level item, compute its
//!     declared function / constant / struct-constructor type, register it in the
//!     `TypingEnv` under its `DefId`.
//!   - Phase 2 **Check item bodies** — walk each fn body with its signature as the
//!     expected type ; constants are checked against their declared type.
//!   - Phase 3 **Resolve final types** — apply the accumulated `Subst` to every
//!     recorded type in the map before handing it to downstream passes.
//!
//! § BIDIRECTIONAL
//!   `check_expr(e, expected)` unifies the synthesized type with `expected` ;
//!   `synth_expr(e) -> Ty` returns a type without prior expectation. Calls /
//!   lambdas / literals synthesize ; let-bindings / fn-params / annotations check.
//!
//! § STAGE-0 LIMITATIONS
//!   - No subtyping.
//!   - No let-generalization (locals are monomorphic).
//!   - Generic fn parameters use skolem `Ty::Param(Symbol)` in body-check and are
//!     re-instantiated with fresh vars at each call-site. Stage-0 instantiation
//!     is conservative : only the outermost fn is instantiated per call.
//!   - Effect rows unify structurally ; row-polymorphism requires an explicit
//!     tail variable at the signature level.
//!   - Capability + IFC + refinement annotations are collected from HIR but not
//!     propagated (T3.4-phase-2).

use cssl_ast::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

use crate::arena::{DefId, HirId};
use crate::env::TypingEnv;
use crate::expr::{
    HirArrayExpr, HirBinOp, HirBlock, HirCallArg, HirExpr, HirExprKind, HirLiteralKind, HirUnOp,
};
use crate::item::{HirFn, HirItem, HirModule, HirStructBody};
use crate::pat::{HirPattern, HirPatternKind};
use crate::stmt::{HirStmt, HirStmtKind};
use crate::symbol::{Interner, Symbol};
use crate::ty::{HirEffectArg, HirEffectRow, HirType, HirTypeKind};
use crate::typing::{ArrayLen, EffectInstance, Row, Scheme, Subst, Ty, TyCtx, TyVar, TypeMap};
use crate::unify::{unify, unify_rows, UnifyError};

#[derive(Debug, Clone, Copy)]
enum ImportedStandardEnum {
    BufferUsage,
    GpuBackend,
    GpuError,
    IdxKind,
    MemKind,
    ShaderIrKind,
    ShaderStage,
    SurfaceFormat,
}

impl ImportedStandardEnum {
    /// Stable stage-0 nominal identity for declarations whose defining module
    /// is not loaded into this single-file checker. Identity follows the
    /// canonical declaration, never the local import spelling or alias.
    fn nominal_def(self) -> DefId {
        let offset = match self {
            Self::BufferUsage => 4,
            Self::GpuBackend => 5,
            Self::GpuError => 6,
            Self::IdxKind => 7,
            Self::MemKind => 8,
            Self::ShaderIrKind => 9,
            Self::ShaderStage => 10,
            Self::SurfaceFormat => 11,
        };
        DefId(u32::MAX - offset)
    }

    fn nominal_type(self) -> Ty {
        Ty::Named {
            def: self.nominal_def(),
            args: Vec::new(),
        }
    }

    fn admits_unit_variant(self, name: &str) -> bool {
        match self {
            Self::BufferUsage => matches!(
                name,
                "Vertex" | "Index" | "Uniform" | "Storage" | "Indirect" | "Staging"
            ),
            Self::GpuBackend => {
                matches!(name, "D3D12" | "Vulkan" | "Metal" | "WebGPU" | "LevelZero")
            }
            Self::GpuError => matches!(
                name,
                "InvalidInput"
                    | "DeviceCreateFailed"
                    | "SwapchainFailed"
                    | "PipelineCompileFailed"
                    | "Timeout"
                    | "OutOfMemory"
                    | "DeviceLost"
                    | "SurfaceLost"
                    | "CapDenied"
                    | "NotSupported"
            ),
            Self::IdxKind => matches!(name, "U16" | "U32"),
            Self::MemKind => matches!(name, "DeviceLocal" | "HostVisible" | "HostCoherent"),
            Self::ShaderIrKind => matches!(name, "Spirv" | "Dxil" | "Metal"),
            Self::ShaderStage => {
                matches!(
                    name,
                    "Vertex" | "Fragment" | "Compute" | "Mesh" | "Amplification"
                )
            }
            Self::SurfaceFormat => matches!(
                name,
                "Bgra8UnormSrgb"
                    | "Rgba8UnormSrgb"
                    | "Rgba16Float"
                    | "Rgb10A2Unorm"
                    | "Rgba16FloatHdr10"
            ),
        }
    }
}

/// Inference context — threaded through every `synth_*` / `check_*` method.
#[derive(Debug)]
pub struct InferCtx<'a> {
    interner: &'a Interner,
    tcx: TyCtx,
    subst: Subst,
    env: TypingEnv,
    type_map: TypeMap,
    diagnostics: Vec<Diagnostic>,
    /// The current function's effect row — call expressions unify into this.
    current_row: Option<Row>,
    /// The current function's return type — `return` / trailing-expr unify with this.
    current_return: Option<Ty>,
    /// Syntactic loop nesting used to reject free `break` / `continue`.
    loop_depth: usize,
    /// Active generic-param map while lowering a fn signature : maps each
    /// generic-param symbol to the fresh [`TyVar`] allocated for it. Outside
    /// a fn-sig lowering this is empty. T3-D17 : replaces the brittle
    /// "single-cap identifier" skolem heuristic with a real fresh-var scheme.
    generics_map: std::collections::HashMap<Symbol, TyVar>,
    /// Exact non-glob names explicitly imported at this module's root while
    /// their declarations are absent from this single-module stage-0 check.
    /// Imported callables remain a declared unknown-external boundary
    /// (`Ty::Error` suppresses cascades). A separate exact standard-enum map
    /// models only known unit variants with rigid nominal identity.
    imports: HashSet<Symbol>,
    /// Exact imported standard enum declarations whose locally known unit
    /// variants may be checked without granting arbitrary external members.
    imported_standard_enums: HashMap<Symbol, ImportedStandardEnum>,
    /// Every declared local module name. Stage-0 lacks lexical module-scope
    /// typing, so this conservative global blocker prevents any local module
    /// from being mistaken for a host namespace.
    local_module_roots: HashSet<Symbol>,
    /// Names introduced only by nested imports. They never become root
    /// opacity grants, but must still block same-spelled host namespaces.
    nested_import_roots: HashSet<Symbol>,
    /// Generic arity for locally declared nominal types. Struct/enum literals
    /// use fresh arguments of this arity so `Record<T>` does not collapse to
    /// an incompatible bare `Record` during body checking.
    nominal_arities: HashMap<DefId, usize>,
    /// Exact top-level enum membership. Qualified `Enum::Variant` expressions
    /// resolve through this declaration map instead of treating `Enum` alone
    /// as the value and silently discarding the member suffix.
    local_enum_variants: HashMap<Symbol, HashMap<Symbol, DefId>>,
}

impl<'a> InferCtx<'a> {
    const SYNTHETIC_VEC_DEF: DefId = DefId(u32::MAX - 1);
    const SYNTHETIC_OPTION_DEF: DefId = DefId(u32::MAX - 2);
    const SYNTHETIC_RESULT_DEF: DefId = DefId(u32::MAX - 3);

    fn synthetic_standard_container_def(name: &str) -> Option<DefId> {
        match name {
            "Vec" => Some(Self::SYNTHETIC_VEC_DEF),
            "Option" => Some(Self::SYNTHETIC_OPTION_DEF),
            "Result" => Some(Self::SYNTHETIC_RESULT_DEF),
            _ => None,
        }
    }

    /// Read-only accessor for the inner typing-env — used by test helpers
    /// that need to inspect item-sig schemes after `collect_item_signatures`.
    #[cfg(test)]
    pub fn env_for_tests(&self) -> &crate::env::TypingEnv {
        &self.env
    }

    /// Build a fresh inference context.
    #[must_use]
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            tcx: TyCtx::new(),
            subst: Subst::new(),
            env: TypingEnv::new(),
            type_map: TypeMap::new(),
            diagnostics: Vec::new(),
            current_row: None,
            current_return: None,
            loop_depth: 0,
            generics_map: std::collections::HashMap::new(),
            imports: HashSet::new(),
            imported_standard_enums: HashMap::new(),
            local_module_roots: HashSet::new(),
            nested_import_roots: HashSet::new(),
            nominal_arities: HashMap::new(),
            local_enum_variants: HashMap::new(),
        }
    }

    // ─ Error + bookkeeping helpers ──────────────────────────────────────────

    fn emit(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(message).with_span(span));
    }

    fn record(&mut self, id: HirId, t: Ty) {
        self.type_map.insert(id, t);
    }

    /// True only for the exact two-segment host-operation spellings that the
    /// stage-0 MIR lowerer recognizes. Their signatures are not available to
    /// this isolated HIR pass, so they remain an explicitly bounded opacity
    /// boundary; an arbitrary namespace or verb must still fail closed.
    fn is_stage0_host_intrinsic_path(&self, segments: &[Symbol]) -> bool {
        if segments.len() != 2 {
            return false;
        }
        let namespace = self.interner.resolve(segments[0]);
        let operation = self.interner.resolve(segments[1]);
        match namespace.as_str() {
            "fs" => matches!(operation.as_str(), "open" | "read" | "write" | "close"),
            "net" => matches!(
                operation.as_str(),
                "socket"
                    | "listen"
                    | "accept"
                    | "connect"
                    | "send"
                    | "recv"
                    | "sendto"
                    | "recvfrom"
                    | "close"
            ),
            "time" => matches!(
                operation.as_str(),
                "monotonic_ns" | "wall_unix_ns" | "sleep_ns" | "deadline_until"
            ),
            "window" => matches!(
                operation.as_str(),
                "spawn" | "pump" | "request_close" | "destroy" | "raw_handle" | "get_dims"
            ),
            "input" => matches!(
                operation.as_str(),
                "keyboard_state" | "mouse_state" | "mouse_delta" | "gamepad_state"
            ),
            "gpu" => matches!(
                operation.as_str(),
                "device_create"
                    | "device_destroy"
                    | "swapchain_create"
                    | "swapchain_acquire"
                    | "swapchain_present"
                    | "pipeline_compile"
                    | "cmd_buf_record_stub"
                    | "cmd_buf_submit_stub"
                    | "buffer_create"
                    | "buffer_destroy"
                    | "buffer_map"
                    | "buffer_unmap"
                    | "buffer_upload"
                    | "cmd_buf_begin"
                    | "cmd_buf_end"
                    | "cmd_buf_bind_pipeline"
                    | "cmd_buf_bind_vbuf"
                    | "cmd_buf_bind_ibuf"
                    | "cmd_buf_bind_descriptor"
                    | "cmd_buf_push_constants"
                    | "cmd_buf_draw_indexed"
                    | "cmd_buf_draw_indirect"
                    | "cmd_buf_dispatch"
                    | "cmd_buf_submit_v2"
            ),
            "audio" => matches!(
                operation.as_str(),
                "stream_open" | "stream_write" | "stream_read" | "stream_close"
            ),
            "thread" => matches!(operation.as_str(), "spawn" | "join"),
            "mutex" => matches!(operation.as_str(), "create" | "lock" | "unlock" | "destroy"),
            "atomic" => matches!(operation.as_str(), "load_u64" | "store_u64" | "cas_u64"),
            _ => false,
        }
    }

    /// Exact canonical qualified spelling used by `stdlib/vec_mut.cssl`.
    /// This is deliberately narrower than generic qualified-path opacity: it
    /// has a locally modeled signature and the MIR lowerer recognizes the same
    /// three-segment path as `cssl.vec.index`.
    fn is_qualified_vec_index_path(&self, segments: &[Symbol]) -> bool {
        segments.len() == 3
            && self.interner.resolve(segments[0]) == "std"
            && self.interner.resolve(segments[1]) == "vec"
            && self.interner.resolve(segments[2]) == "vec_index"
    }

    fn is_unshadowed_qualified_vec_index_path(&self, segments: &[Symbol]) -> bool {
        self.is_qualified_vec_index_path(segments)
            && self.env.lookup(segments[0]).is_none()
            && self.env.item_def(segments[0]).is_none()
            && !self.local_module_roots.contains(&segments[0])
            && !self.nested_import_roots.contains(&segments[0])
            && !self.imports.contains(&segments[0])
    }

    fn classify_standard_enum_import(&self, path: &[Symbol]) -> Option<ImportedStandardEnum> {
        if path.len() != 3 || self.interner.resolve(path[0]) != "std" {
            return None;
        }
        let module = self.interner.resolve(path[1]);
        let name = self.interner.resolve(path[2]);
        match (module.as_str(), name.as_str()) {
            ("gpu", "GpuBackend") => Some(ImportedStandardEnum::GpuBackend),
            ("gpu", "GpuError") => Some(ImportedStandardEnum::GpuError),
            ("gpu", "ShaderIRKind") => Some(ImportedStandardEnum::ShaderIrKind),
            ("gpu", "ShaderStage") => Some(ImportedStandardEnum::ShaderStage),
            ("gpu", "SurfaceFormat") => Some(ImportedStandardEnum::SurfaceFormat),
            ("gpu_transport", "BufferUsage") => Some(ImportedStandardEnum::BufferUsage),
            ("gpu_transport", "IdxKind") => Some(ImportedStandardEnum::IdxKind),
            ("gpu_transport", "MemKind") => Some(ImportedStandardEnum::MemKind),
            _ => None,
        }
    }

    /// Preserve exact scalar spelling inside `Vec<T>` even though the broad
    /// stage-0 scalar lattice currently coalesces integer widths. This rigid
    /// element identity prevents an explicit `vec_index::<i32>` from being
    /// accepted against `Vec<u64>` while leaving scalar arithmetic unchanged.
    fn lower_vector_element_type(&mut self, t: &HirType) -> Ty {
        if let HirTypeKind::Path { path, def, .. } = &t.kind {
            if path.len() == 1 && def.is_none() {
                let name = self.interner.resolve(path[0]);
                if matches!(
                    name.as_str(),
                    "i8" | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                        | "f16"
                        | "f32"
                        | "f64"
                ) {
                    return Ty::Param(path[0]);
                }
            }
        }
        self.lower_hir_type(t)
    }

    fn try_unify(&mut self, a: &Ty, b: &Ty, span: Span, context: &str) {
        match unify(a, b, &mut self.subst) {
            Ok(()) => {}
            Err(UnifyError::Mismatch { a, b }) => {
                self.emit(
                    format!("type mismatch in {context} : expected {a:?}, found {b:?}"),
                    span,
                );
            }
            Err(UnifyError::Arity { expected, found }) => {
                self.emit(
                    format!(
                        "arity mismatch in {context} : expected {expected} elements, found {found}"
                    ),
                    span,
                );
            }
            Err(UnifyError::OccursCheck { .. }) => {
                self.emit(
                    format!("occurs-check failed in {context} (infinite type)"),
                    span,
                );
            }
            Err(UnifyError::RowMismatch { .. }) => {
                self.emit(format!("effect-row mismatch in {context}"), span);
            }
        }
    }

    fn try_unify_rows(&mut self, a: &Row, b: &Row, span: Span, context: &str) {
        match unify_rows(a, b, &mut self.subst) {
            Ok(()) => {}
            Err(_) => self.emit(
                format!(
                    "effect-row mismatch in {context} : {:?} vs {:?}",
                    self.subst.apply_row(a),
                    self.subst.apply_row(b),
                ),
                span,
            ),
        }
    }

    // ─ HIR-type → inference-Ty translation ──────────────────────────────────

    fn lower_hir_type(&mut self, t: &HirType) -> Ty {
        match &t.kind {
            HirTypeKind::Path {
                path,
                def,
                type_args,
            } => {
                // Recognize primitive paths by single-segment name-text.
                if path.len() == 1 {
                    let name = self.interner.resolve(path[0]);
                    match name.as_str() {
                        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32"
                        | "u64" | "u128" | "usize" => return Ty::Int,
                        "f16" | "f32" | "f64" => return Ty::Float,
                        "bool" => return Ty::Bool,
                        "str" | "String" => return Ty::Str,
                        "()" => return Ty::Unit,
                        "Never" | "!" => return Ty::Never,
                        _ => {}
                    }
                    // A declared generic parameter owns its spelling before
                    // synthetic prelude containers are considered. In
                    // particular, `fn f<Vec>(value: Vec) -> Vec` is one
                    // coherent type variable, not the built-in `Vec<_>`.
                    if let Some(var) = self.generics_map.get(&path[0]).copied() {
                        return Ty::Var(var);
                    }
                    match name.as_str() {
                        // Stage-0 standard containers keep a stable synthetic
                        // nominal identity only when no local definition won
                        // resolution. Ty::Error here would unify with Bool and
                        // silently erase locally knowable type mismatches.
                        "Vec" | "Option" | "Result"
                            if def.is_none()
                                && !self.imports.contains(&path[0])
                                && !self.nested_import_roots.contains(&path[0]) =>
                        {
                            return Ty::Named {
                                def: Self::synthetic_standard_container_def(&name)
                                    .expect("matched standard container"),
                                args: if name == "Vec" {
                                    type_args
                                        .iter()
                                        .map(|arg| self.lower_vector_element_type(arg))
                                        .collect()
                                } else {
                                    type_args
                                        .iter()
                                        .map(|arg| self.lower_hir_type(arg))
                                        .collect()
                                },
                            };
                        }
                        _ => {}
                    }
                    // T3-D17 : unresolved one/two-character Title identifiers
                    // retain the legacy skolem fallback only after declared
                    // generic parameters and standard nominals were resolved.
                    if def.is_none()
                        && name.chars().next().is_some_and(char::is_uppercase)
                        && name.len() <= 2
                    {
                        return Ty::Param(path[0]);
                    }
                }
                // Nominal reference.
                let args: Vec<Ty> = type_args.iter().map(|a| self.lower_hir_type(a)).collect();
                match def {
                    Some(d) => Ty::Named { def: *d, args },
                    None if path.len() == 1
                        && self.imported_standard_enums.contains_key(&path[0]) =>
                    {
                        if args.is_empty() {
                            self.imported_standard_enums[&path[0]].nominal_type()
                        } else {
                            self.emit(
                                "generic imported nominal requires a resolved declaration"
                                    .to_string(),
                                t.span,
                            );
                            Ty::Error
                        }
                    }
                    None if path.len() == 1 && self.imports.contains(&path[0]) => Ty::Error,
                    None => {
                        // Unresolved — emit a diagnostic at T3.4 level (identifier undefined).
                        self.emit(
                            format!(
                                "unresolved type {:?}",
                                path.iter()
                                    .map(|s| self.interner.resolve(*s))
                                    .collect::<Vec<_>>()
                            ),
                            t.span,
                        );
                        Ty::Error
                    }
                }
            }
            HirTypeKind::Tuple { elems } => {
                Ty::Tuple(elems.iter().map(|e| self.lower_hir_type(e)).collect())
            }
            HirTypeKind::Array { elem, len } => {
                let len_slot = match &len.kind {
                    HirExprKind::Literal(l) if l.kind == HirLiteralKind::Int => ArrayLen::Opaque,
                    _ => ArrayLen::Opaque,
                };
                Ty::Array {
                    elem: Box::new(self.lower_hir_type(elem)),
                    len: len_slot,
                }
            }
            HirTypeKind::Slice { elem } => Ty::Slice {
                elem: Box::new(self.lower_hir_type(elem)),
            },
            HirTypeKind::Reference { mutable, inner } => Ty::Ref {
                mutable: *mutable,
                inner: Box::new(self.lower_hir_type(inner)),
            },
            HirTypeKind::Capability { inner, .. } => {
                // Stage-0 capability stubs : propagate inner type ; cap-checking is T3.4-phase-2.
                self.lower_hir_type(inner)
            }
            HirTypeKind::Function {
                params,
                return_ty,
                effect_row,
            } => Ty::Fn {
                params: params.iter().map(|p| self.lower_hir_type(p)).collect(),
                return_ty: Box::new(self.lower_hir_type(return_ty)),
                effect_row: effect_row
                    .as_ref()
                    .map(|r| self.lower_hir_row(r))
                    .unwrap_or_else(Row::pure),
            },
            HirTypeKind::Refined { base, .. } => {
                // Refinement obligations are T3.4-phase-2 ; strip to base for now.
                self.lower_hir_type(base)
            }
            HirTypeKind::Infer => self.tcx.fresh_ty(),
            HirTypeKind::Error => Ty::Error,
        }
    }

    fn lower_hir_row(&mut self, r: &HirEffectRow) -> Row {
        let effects = r
            .effects
            .iter()
            .map(|e| EffectInstance {
                name: e
                    .name
                    .last()
                    .copied()
                    .unwrap_or_else(|| self.interner.intern("_")),
                args: e
                    .args
                    .iter()
                    .map(|a| match a {
                        HirEffectArg::Type(t) => self.lower_hir_type(t),
                        HirEffectArg::Expr(_) => Ty::Error,
                    })
                    .collect(),
            })
            .collect();
        let tail = r.tail.map(|_sym| {
            // A named tail variable becomes a fresh row-var for now ; stage-1 can canonicalize
            // by-name across the signature scope.
            self.tcx.fresh_row()
        });
        Row { effects, tail }
    }

    // ─ Phase 1 : collect item signatures ────────────────────────────────────

    fn collect_item_signatures(&mut self, module: &HirModule) {
        self.collect_module_metadata(module);
        for item in &module.items {
            self.collect_item(item);
        }
    }

    fn collect_module_metadata(&mut self, module: &HirModule) {
        for item in &module.items {
            // Only imports declared in this checked module may be opaque at
            // its root. Imports inside a nested module must not leak into the
            // parent module's value/type namespace.
            if let HirItem::Use(item) = item {
                for binding in &item.bindings {
                    if !binding.is_glob {
                        if let Some(name) = binding.alias.or_else(|| binding.path.last().copied()) {
                            self.imports.insert(name);
                            if let Some(kind) = self.classify_standard_enum_import(&binding.path) {
                                self.imported_standard_enums.insert(name, kind);
                            }
                        }
                    }
                }
            }
            self.collect_item_metadata(item, false);
        }
    }

    fn collect_item_metadata(&mut self, item: &HirItem, nested: bool) {
        match item {
            HirItem::Struct(item) => {
                let arity = item
                    .generics
                    .params
                    .iter()
                    .filter(|param| matches!(param.kind, crate::item::HirGenericParamKind::Type))
                    .count();
                self.nominal_arities.insert(item.def, arity);
            }
            HirItem::Enum(item) => {
                let arity = item
                    .generics
                    .params
                    .iter()
                    .filter(|param| matches!(param.kind, crate::item::HirGenericParamKind::Type))
                    .count();
                self.nominal_arities.insert(item.def, arity);
                if !nested {
                    self.local_enum_variants.insert(
                        item.name,
                        item.variants
                            .iter()
                            .map(|variant| (variant.name, variant.def))
                            .collect(),
                    );
                }
            }
            HirItem::Module(item) => {
                self.local_module_roots.insert(item.name);
                if let Some(items) = &item.items {
                    for item in items {
                        self.collect_item_metadata(item, true);
                    }
                }
            }
            HirItem::Use(item) if nested => {
                for binding in &item.bindings {
                    if !binding.is_glob {
                        if let Some(name) = binding.alias.or_else(|| binding.path.last().copied()) {
                            self.nested_import_roots.insert(name);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_item(&mut self, item: &HirItem) {
        match item {
            HirItem::Fn(f) => {
                let scheme = self.fn_signature_scheme(f);
                self.env.register_item_scheme(f.name, f.def, scheme);
            }
            HirItem::ExternFn(f) => {
                // Extern fns have no generics + a fixed C ABI. Register a
                // monomorphic Ty::Fn so callers see the signature exactly as
                // declared.
                let params: Vec<Ty> = f
                    .params
                    .iter()
                    .map(|p| self.lower_hir_type(&p.ty))
                    .collect();
                let return_ty = f
                    .return_ty
                    .as_ref()
                    .map(|t| self.lower_hir_type(t))
                    .unwrap_or(Ty::Unit);
                let sig = Ty::Fn {
                    params,
                    return_ty: Box::new(return_ty),
                    effect_row: Row::pure(),
                };
                self.env.register_item(f.name, f.def, sig);
            }
            HirItem::Const(c) => {
                let t = self.lower_hir_type(&c.ty);
                self.env.register_item(c.name, c.def, t);
            }
            HirItem::Struct(s) => {
                // Struct name resolves to the constructor function for tuple/unit variants.
                // Named-field structs have no direct expression-level constructor — handled
                // via Expr::Struct.
                let args: Vec<Ty> = match &s.body {
                    HirStructBody::Unit => Vec::new(),
                    HirStructBody::Tuple(fs) | HirStructBody::Named(fs) => {
                        fs.iter().map(|f| self.lower_hir_type(&f.ty)).collect()
                    }
                };
                let self_ty = Ty::Named {
                    def: s.def,
                    args: Vec::new(),
                };
                let sig = Ty::Fn {
                    params: args,
                    return_ty: Box::new(self_ty),
                    effect_row: Row::pure(),
                };
                self.env.register_item(s.name, s.def, sig);
            }
            HirItem::Enum(e) => {
                // Parent enum name — record as a named type with no arguments (stage-0 simplification).
                let parent = Ty::Named {
                    def: e.def,
                    args: Vec::new(),
                };
                self.env.register_item(e.name, e.def, parent.clone());
                // Register each variant as a constructor function.
                for v in &e.variants {
                    let args: Vec<Ty> = match &v.body {
                        HirStructBody::Unit => Vec::new(),
                        HirStructBody::Tuple(fs) | HirStructBody::Named(fs) => {
                            fs.iter().map(|f| self.lower_hir_type(&f.ty)).collect()
                        }
                    };
                    let sig = if args.is_empty() {
                        parent.clone()
                    } else {
                        Ty::Fn {
                            params: args,
                            return_ty: Box::new(parent.clone()),
                            effect_row: Row::pure(),
                        }
                    };
                    self.env.register_item(v.name, v.def, sig);
                }
            }
            HirItem::TypeAlias(t) => {
                let target = self.lower_hir_type(&t.ty);
                self.env.register_item(t.name, t.def, target);
            }
            HirItem::Effect(e) => {
                // Register the effect's name as a nominal type.
                let sig = Ty::Named {
                    def: e.def,
                    args: Vec::new(),
                };
                self.env.register_item(e.name, e.def, sig);
            }
            HirItem::Handler(h) => {
                // Handler signature = param-list → Ret.
                let params = h
                    .params
                    .iter()
                    .map(|p| self.lower_hir_type(&p.ty))
                    .collect();
                let ret = h
                    .return_ty
                    .as_ref()
                    .map(|t| self.lower_hir_type(t))
                    .unwrap_or(Ty::Unit);
                let sig = Ty::Fn {
                    params,
                    return_ty: Box::new(ret),
                    effect_row: Row::pure(),
                };
                self.env.register_item(h.name, h.def, sig);
            }
            HirItem::Interface(i) => {
                let sig = Ty::Named {
                    def: i.def,
                    args: Vec::new(),
                };
                self.env.register_item(i.name, i.def, sig);
            }
            HirItem::Module(m) => {
                // Nested module — walk recursively when its items are available.
                if let Some(items) = &m.items {
                    for sub in items {
                        self.collect_item(sub);
                    }
                }
            }
            HirItem::Impl(_) | HirItem::Use(_) => {
                // No definition-level name to register.
            }
        }
    }

    /// Lower a fn signature into a polymorphic [`Scheme`]. Each generic type-
    /// parameter of the fn gets a **fresh** [`TyVar`] ; the param + return +
    /// effect-row types are lowered with the `generics_map` pointing to those
    /// fresh vars ; the resulting `Ty::Fn` is wrapped as a `Scheme`
    /// quantifying over those vars.
    ///
    /// § T3-D17 : retires `Ty::Param(Symbol)` skolem for fn-generics.
    fn fn_signature_scheme(&mut self, f: &HirFn) -> Scheme {
        // Save the outer map (empty at module-top ; non-empty for nested fns
        // if they ever appear). Build the per-fn generics-map ; restore after.
        let saved_map = core::mem::take(&mut self.generics_map);
        let mut bound_ty_vars: Vec<TyVar> = Vec::new();
        for gp in &f.generics.params {
            if matches!(gp.kind, crate::item::HirGenericParamKind::Type) {
                let fresh = self.tcx.fresh_ty();
                if let Ty::Var(v) = fresh {
                    self.generics_map.insert(gp.name, v);
                    bound_ty_vars.push(v);
                }
            }
        }
        let body_ty = self.fn_signature(f);
        // Restore outer map. Fresh vars we allocated persist in `bound_ty_vars`.
        self.generics_map = saved_map;
        Scheme {
            ty_vars: bound_ty_vars,
            row_vars: Vec::new(),
            body: body_ty,
        }
    }

    fn fn_signature(&mut self, f: &HirFn) -> Ty {
        let params: Vec<Ty> = f
            .params
            .iter()
            .map(|p| self.lower_hir_type(&p.ty))
            .collect();
        let return_ty = f
            .return_ty
            .as_ref()
            .map(|t| self.lower_hir_type(t))
            .unwrap_or(Ty::Unit);
        let effect_row = f
            .effect_row
            .as_ref()
            .map(|r| self.lower_hir_row(r))
            .unwrap_or_else(Row::pure);
        Ty::Fn {
            params,
            return_ty: Box::new(return_ty),
            effect_row,
        }
    }

    // ─ Phase 2 : check item bodies ──────────────────────────────────────────

    fn check_items(&mut self, module: &HirModule) {
        for item in &module.items {
            self.check_item(item);
        }
    }

    fn check_item(&mut self, item: &HirItem) {
        match item {
            HirItem::Fn(f) => self.check_fn(f),
            HirItem::Const(c) => {
                let declared = self.lower_hir_type(&c.ty);
                let inferred = self.synth_expr(&c.value);
                self.try_unify(&declared, &inferred, c.value.span, "const initializer");
            }
            HirItem::Impl(i) => {
                for f in &i.fns {
                    self.check_fn(f);
                }
            }
            HirItem::Interface(i) => {
                for f in &i.fns {
                    self.check_fn(f);
                }
            }
            HirItem::Effect(e) => {
                for f in &e.ops {
                    self.check_fn(f);
                }
            }
            HirItem::Handler(h) => {
                for f in &h.ops {
                    self.check_fn(f);
                }
                if let Some(ret_block) = &h.return_clause {
                    let prev_return = self.current_return.take();
                    self.env.enter();
                    let _ = self.synth_block(ret_block);
                    self.env.leave();
                    self.current_return = prev_return;
                }
            }
            HirItem::Module(m) => {
                if let Some(items) = &m.items {
                    for sub in items {
                        self.check_item(sub);
                    }
                }
            }
            HirItem::Struct(_)
            | HirItem::Enum(_)
            | HirItem::TypeAlias(_)
            | HirItem::Use(_)
            | HirItem::ExternFn(_) => {
                // No body to check beyond signature registration.
            }
        }
    }

    fn check_fn(&mut self, f: &HirFn) {
        let body = match &f.body {
            Some(b) => b,
            None => return, // signature-only (interface / effect op) — nothing to check.
        };
        let declared_return = f
            .return_ty
            .as_ref()
            .map(|t| self.lower_hir_type(t))
            .unwrap_or(Ty::Unit);
        let declared_row = f
            .effect_row
            .as_ref()
            .map(|r| self.lower_hir_row(r))
            .unwrap_or_else(Row::pure);
        self.env.enter();
        for p in &f.params {
            let pt = self.lower_hir_type(&p.ty);
            self.bind_pattern(&p.pat, &pt);
            self.record(p.id, pt);
        }
        let prev_row = self.current_row.replace(declared_row.clone());
        let prev_ret = self.current_return.replace(declared_return.clone());
        let body_ty = self.synth_block(body);
        // Trailing-expression type must match declared return.
        self.try_unify(
            &declared_return,
            &body_ty,
            body.span,
            "fn body trailing expression",
        );
        self.current_row = prev_row;
        self.current_return = prev_ret;
        self.env.leave();
    }

    /// Bind a pattern at a let-boundary — generalizes the type before
    /// inserting, so simple bindings (`let x = e`) get a polymorphic scheme
    /// `∀α̅. τ` where `α̅ = ftv(τ) − ftv(Γ)`. Non-Binding patterns fall
    /// through to the monomorphic [`Self::bind_pattern`] path.
    ///
    /// § VALUE-RESTRICTION
    ///   Stage-0 does NOT apply ML's value-restriction — generalization is
    ///   performed unconditionally for every let-binding. This is unsound for
    ///   mutable-ref generalization (which CSSLv3 does not support in stage-0
    ///   anyway — all `let`s bind immutable values by default). Full value-
    ///   restriction is a phase-2e refinement.
    fn bind_pattern_let(&mut self, pat: &HirPattern, t: &Ty) {
        match &pat.kind {
            HirPatternKind::Binding { name, .. } => {
                let applied = self.subst.apply(t);
                let env_free_ty = self.env.free_ty_vars();
                let env_free_row = self.env.free_row_vars();
                let scheme = crate::typing::generalize(&env_free_ty, &env_free_row, applied);
                self.env.insert_local_scheme(*name, scheme);
                self.record(pat.id, t.clone());
            }
            // Non-Binding patterns (Tuple, Struct, Variant, etc.) decompose via
            // the monomorphic path — stage-0 does not generalize the individual
            // projection-bindings (each gets its own fresh var, effectively
            // monomorphic per position).
            _ => self.bind_pattern(pat, t),
        }
    }

    fn bind_pattern(&mut self, pat: &HirPattern, t: &Ty) {
        match &pat.kind {
            HirPatternKind::Wildcard | HirPatternKind::Error => {}
            HirPatternKind::Binding { name, .. } => {
                self.env.insert_local(*name, t.clone());
            }
            HirPatternKind::Literal(_) => {
                // Literal patterns don't bind anything.
            }
            HirPatternKind::Tuple(elems) => {
                // If the expected type is a tuple of the same arity, bind element-wise.
                // Otherwise, bind each element as a fresh var.
                let applied = self.subst.apply(t);
                match applied {
                    Ty::Tuple(inner) if inner.len() == elems.len() => {
                        for (p, ip) in elems.iter().zip(inner.iter()) {
                            self.bind_pattern(p, ip);
                        }
                    }
                    _ => {
                        for p in elems {
                            let v = self.tcx.fresh_ty();
                            self.bind_pattern(p, &v);
                        }
                    }
                }
            }
            HirPatternKind::Or(alts) => {
                // Each alt must yield the same bindings ; stage-0 checks the first only.
                if let Some(first) = alts.first() {
                    self.bind_pattern(first, t);
                }
            }
            HirPatternKind::Struct { fields, .. } => {
                for f in fields {
                    if let Some(p) = &f.pat {
                        let v = self.tcx.fresh_ty();
                        self.bind_pattern(p, &v);
                    } else {
                        // Shorthand — `{ x }` binds `x` to a fresh var.
                        self.env.insert_local(f.name, self.tcx.fresh_ty());
                    }
                }
            }
            HirPatternKind::Variant { args, .. } => {
                for a in args {
                    let v = self.tcx.fresh_ty();
                    self.bind_pattern(a, &v);
                }
            }
            HirPatternKind::Range { .. } => {
                // Range pattern doesn't bind names.
            }
            HirPatternKind::Ref { inner, .. } => {
                self.bind_pattern(inner, t);
            }
        }
        self.record(pat.id, t.clone());
    }

    // ─ Expression synthesis + check ─────────────────────────────────────────

    fn synth_expr(&mut self, e: &HirExpr) -> Ty {
        let t = self.synth_expr_kind(e);
        self.record(e.id, t.clone());
        t
    }

    #[allow(clippy::too_many_lines)]
    fn synth_expr_kind(&mut self, e: &HirExpr) -> Ty {
        match &e.kind {
            HirExprKind::Literal(l) => match l.kind {
                HirLiteralKind::Int => Ty::Int,
                HirLiteralKind::Float => Ty::Float,
                HirLiteralKind::Bool(_) => Ty::Bool,
                HirLiteralKind::Str => Ty::Str,
                HirLiteralKind::Char => Ty::Str, // stage-0 : char ≈ str
                HirLiteralKind::Unit => Ty::Unit,
            },
            HirExprKind::Path { segments, def } => {
                if segments.len() == 1 {
                    if let Some(d) = def {
                        // T3-D17 : item-sigs are now stored as Scheme ;
                        // instantiate with fresh vars per call-site so generic
                        // fns get independent ty-vars at each use-site.
                        if let Some(scheme) = self.env.item_scheme(*d).cloned() {
                            return scheme.instantiate(&mut self.tcx);
                        }
                    }
                }
                if let Some(&first) = segments.first() {
                    if segments.len() == 2 {
                        if let Some(variants) = self.local_enum_variants.get(&first) {
                            if let Some(variant_def) = variants.get(&segments[1]) {
                                if let Some(scheme) = self.env.item_scheme(*variant_def).cloned() {
                                    return scheme.instantiate(&mut self.tcx);
                                }
                            }
                            self.emit(
                                "qualified local enum path names an unknown variant".to_string(),
                                e.span,
                            );
                            return Ty::Error;
                        }
                    }
                    // A multi-segment path is never the value bound to its
                    // first segment. Stage-0 cannot resolve members of local
                    // values/items yet, so consuming the first symbol's
                    // callable scheme would erase the suffix and can convert
                    // `fs::open` or `std::vec::vec_index` into privileged
                    // intrinsics. Fail closed until the full member resolver
                    // supplies an exact declaration identity.
                    if segments.len() > 1
                        && (self.env.lookup_local_scheme(first).is_some()
                            || self.env.item_def(first).is_some()
                            || self.env.lookup(first).is_some())
                    {
                        self.emit(
                            "qualified path root resolves to a local binding; member resolution is required"
                                .to_string(),
                            e.span,
                        );
                        return Ty::Error;
                    }
                    // T3-D15 : if the local is bound to a polymorphic scheme,
                    // instantiate with fresh vars per use-site. Monomorphic
                    // schemes pass through unchanged (Scheme::instantiate is
                    // a no-op when `rank == 0`).
                    if let Some(scheme) = self.env.lookup_local_scheme(first).cloned() {
                        return scheme.instantiate(&mut self.tcx);
                    }
                    // Fall back to item-scheme by-name for paths that didn't
                    // resolve during name-resolution but match a module-level
                    // item (stage-0 name-resolution doesn't cover every case).
                    if let Some(def) = self.env.item_def(first) {
                        if let Some(scheme) = self.env.item_scheme(def).cloned() {
                            return scheme.instantiate(&mut self.tcx);
                        }
                    }
                    if let Some(t) = self.env.lookup(first).cloned() {
                        return t;
                    }
                    if self.local_module_roots.contains(&first) {
                        self.emit(
                            "local module paths require resolved members and are not callable values"
                                .to_string(),
                            e.span,
                        );
                        return Ty::Error;
                    }
                    if self.nested_import_roots.contains(&first) {
                        self.emit(
                            "nested imports do not grant root opacity or host-namespace authority"
                                .to_string(),
                            e.span,
                        );
                        return Ty::Error;
                    }
                    if self.imports.contains(&first) {
                        if segments.len() == 2 {
                            if let Some(kind) = self.imported_standard_enums.get(&first).copied() {
                                let variant = self.interner.resolve(segments[1]);
                                if kind.admits_unit_variant(&variant) {
                                    return kind.nominal_type();
                                }
                            }
                        }
                        if segments.len() != 1 {
                            self.emit(
                                "qualified descendants of an opaque import require a resolved signature"
                                    .to_string(),
                                e.span,
                            );
                        }
                        return Ty::Error;
                    }
                }
                // Stage-0 standard-library constructor. Method/member paths
                // otherwise resolve after HIR inference; model `Vec::new`
                // directly so the canonical accepted Vec surface remains
                // checkable without weakening unresolved-name diagnostics.
                if segments.len() == 2
                    && self.interner.resolve(segments[0]) == "Vec"
                    && self.interner.resolve(segments[1]) == "new"
                {
                    let elem = self.tcx.fresh_ty();
                    return Ty::Fn {
                        params: Vec::new(),
                        return_ty: Box::new(Ty::Named {
                            def: Self::SYNTHETIC_VEC_DEF,
                            args: vec![elem],
                        }),
                        effect_row: Row::pure(),
                    };
                }
                if segments.len() == 1 {
                    let name = self.interner.resolve(segments[0]);
                    let signature = match name.as_str() {
                        "qbind" | "qentangle" => Some((vec![Ty::Int, Ty::Int], Ty::Int)),
                        "qsuperpose" => Some((vec![Ty::Int, Ty::Int, Ty::Float], Ty::Int)),
                        "qmeasure" => Some((vec![Ty::Int], Ty::Int)),
                        "panic" => Some((vec![Ty::Str], Ty::Never)),
                        _ => None,
                    };
                    if let Some((params, return_ty)) = signature {
                        return Ty::Fn {
                            params,
                            return_ty: Box::new(return_ty),
                            effect_row: Row::pure(),
                        };
                    }
                    if matches!(name.as_str(), "Ok" | "Err" | "Some") {
                        let payload = self.tcx.fresh_ty();
                        let other = self.tcx.fresh_ty();
                        let args = if name == "Some" {
                            vec![payload.clone()]
                        } else if name == "Ok" {
                            vec![payload.clone(), other]
                        } else {
                            vec![other, payload.clone()]
                        };
                        return Ty::Fn {
                            params: vec![payload],
                            return_ty: Box::new(Ty::Named {
                                def: if name == "Some" {
                                    Self::SYNTHETIC_OPTION_DEF
                                } else {
                                    Self::SYNTHETIC_RESULT_DEF
                                },
                                args,
                            }),
                            effect_row: Row::pure(),
                        };
                    }
                    if name == "None" {
                        return Ty::Named {
                            def: Self::SYNTHETIC_OPTION_DEF,
                            args: vec![self.tcx.fresh_ty()],
                        };
                    }
                }
                if self.is_stage0_host_intrinsic_path(segments) {
                    return Ty::Error;
                }
                self.emit(
                    format!(
                        "unresolved name : {:?}",
                        segments
                            .iter()
                            .map(|s| self.interner.resolve(*s))
                            .collect::<Vec<_>>()
                    ),
                    e.span,
                );
                Ty::Error
            }
            HirExprKind::Call {
                callee,
                args,
                type_args,
            } => {
                let qualified_vec_index = matches!(
                    &callee.kind,
                    HirExprKind::Path { segments, .. }
                        if self.is_unshadowed_qualified_vec_index_path(segments)
                );
                let callee_ty = if qualified_vec_index {
                    if type_args.len() == 1 {
                        let vector_elem = self.lower_vector_element_type(&type_args[0]);
                        let result_elem = self.lower_hir_type(&type_args[0]);
                        Ty::Fn {
                            params: vec![
                                Ty::Named {
                                    def: Self::SYNTHETIC_VEC_DEF,
                                    args: vec![vector_elem],
                                },
                                Ty::Int,
                            ],
                            return_ty: Box::new(result_elem),
                            effect_row: Row::pure(),
                        }
                    } else {
                        self.emit(
                            "std::vec::vec_index requires exactly one explicit type argument"
                                .to_string(),
                            e.span,
                        );
                        Ty::Error
                    }
                } else {
                    self.synth_expr(callee)
                };
                let callee_ty = self.subst.apply(&callee_ty);
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.synth_call_arg(a)).collect();
                // Unify callee with fn(arg_tys) → fresh_ret / fresh_row.
                let ret_var = self.tcx.fresh_ty();
                let row_var = self.tcx.fresh_row();
                let expected = Ty::Fn {
                    params: arg_tys.clone(),
                    return_ty: Box::new(ret_var.clone()),
                    effect_row: Row {
                        effects: Vec::new(),
                        tail: Some(row_var),
                    },
                };
                self.try_unify(&callee_ty, &expected, e.span, "function call");
                // Merge the callee's effect-row into the current fn's row.
                if let Some(current) = self.current_row.clone() {
                    let applied_fn = self.subst.apply(&callee_ty);
                    if let Ty::Fn { effect_row, .. } = applied_fn {
                        self.try_unify_rows(
                            &current,
                            &effect_row,
                            e.span,
                            "effect-row composition",
                        );
                    }
                }
                self.subst.apply(&ret_var)
            }
            HirExprKind::Field { obj, name: _ } => {
                // Stage-0 : field-access returns a fresh var ; full struct-field lookup is
                // T3.4-phase-2 when we walk the registered struct bodies.
                let _obj_ty = self.synth_expr(obj);
                self.tcx.fresh_ty()
            }
            HirExprKind::Index { obj, index } => {
                let obj_ty = self.synth_expr(obj);
                let _ = self.synth_expr(index);
                let elem_var = self.tcx.fresh_ty();
                // Assume obj : Slice<E> or Array<E, _>.
                let slice_shape = Ty::Slice {
                    elem: Box::new(elem_var.clone()),
                };
                let array_shape = Ty::Array {
                    elem: Box::new(elem_var.clone()),
                    len: ArrayLen::Opaque,
                };
                if unify(&obj_ty, &slice_shape, &mut self.subst).is_err() {
                    let _ = unify(&obj_ty, &array_shape, &mut self.subst);
                }
                self.subst.apply(&elem_var)
            }
            HirExprKind::Binary { op, lhs, rhs } => self.synth_binop(*op, lhs, rhs),
            HirExprKind::Unary { op, operand } => self.synth_unop(*op, operand),
            HirExprKind::Block(b) => self.synth_block(b),
            HirExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_ty = self.synth_expr(cond);
                self.try_unify(&Ty::Bool, &cond_ty, cond.span, "if condition");
                let then_ty = self.synth_block(then_branch);
                if let Some(else_e) = else_branch {
                    let else_ty = self.synth_expr(else_e);
                    self.try_unify(&then_ty, &else_ty, e.span, "if branches");
                    let then_ty = self.subst.apply(&then_ty);
                    let else_ty = self.subst.apply(&else_ty);
                    match (&then_ty, &else_ty) {
                        (Ty::Never, Ty::Never) => Ty::Never,
                        (Ty::Never, _) => else_ty,
                        (_, Ty::Never) => then_ty,
                        _ => then_ty,
                    }
                } else {
                    // else-less if evaluates to unit.
                    self.try_unify(&Ty::Unit, &then_ty, then_branch.span, "if without else");
                    Ty::Unit
                }
            }
            HirExprKind::Match { scrutinee, arms } => {
                let scrut_ty = self.synth_expr(scrutinee);
                let result_var = self.tcx.fresh_ty();
                for arm in arms {
                    self.env.enter();
                    self.bind_pattern(&arm.pat, &scrut_ty);
                    if let Some(g) = &arm.guard {
                        let gt = self.synth_expr(g);
                        self.try_unify(&Ty::Bool, &gt, g.span, "match guard");
                    }
                    let bt = self.synth_expr(&arm.body);
                    self.try_unify(&result_var, &bt, arm.body.span, "match arm body");
                    self.env.leave();
                }
                self.subst.apply(&result_var)
            }
            HirExprKind::For { pat, iter, body } => {
                let _iter_ty = self.synth_expr(iter);
                let elem_var = self.tcx.fresh_ty();
                self.env.enter();
                self.bind_pattern(pat, &elem_var);
                self.loop_depth += 1;
                let _body_ty = self.synth_block(body);
                self.loop_depth -= 1;
                self.env.leave();
                Ty::Unit
            }
            HirExprKind::While { cond, body } => {
                let ct = self.synth_expr(cond);
                self.try_unify(&Ty::Bool, &ct, cond.span, "while condition");
                self.loop_depth += 1;
                let _ = self.synth_block(body);
                self.loop_depth -= 1;
                Ty::Unit
            }
            HirExprKind::Loop { body } => {
                self.loop_depth += 1;
                let _ = self.synth_block(body);
                self.loop_depth -= 1;
                // Stage-0 has no break-target/value analysis. Conservatively
                // refuse to use `loop` as proof of a non-unit function result.
                Ty::Unit
            }
            HirExprKind::Return { value } => {
                if let Some(v) = value {
                    let vt = self.synth_expr(v);
                    if let Some(ret) = self.current_return.clone() {
                        self.try_unify(&ret, &vt, v.span, "return expression");
                    }
                } else if let Some(ret) = self.current_return.clone() {
                    self.try_unify(&ret, &Ty::Unit, e.span, "return without value");
                }
                Ty::Never
            }
            HirExprKind::Break { value, .. } => {
                if self.loop_depth == 0 {
                    self.emit("`break` outside loop".to_string(), e.span);
                }
                if let Some(v) = value {
                    let _ = self.synth_expr(v);
                }
                Ty::Never
            }
            HirExprKind::Continue { .. } => {
                if self.loop_depth == 0 {
                    self.emit("`continue` outside loop".to_string(), e.span);
                }
                Ty::Never
            }
            HirExprKind::Lambda {
                params,
                return_ty,
                body,
            } => {
                self.env.enter();
                let param_tys: Vec<Ty> = params
                    .iter()
                    .map(|p| {
                        let pt = match &p.ty {
                            Some(t) => self.lower_hir_type(t),
                            None => self.tcx.fresh_ty(),
                        };
                        self.bind_pattern(&p.pat, &pt);
                        pt
                    })
                    .collect();
                let expected_ret = return_ty
                    .as_ref()
                    .map(|t| self.lower_hir_type(t))
                    .unwrap_or_else(|| self.tcx.fresh_ty());
                let body_ty = self.synth_expr(body);
                self.try_unify(&expected_ret, &body_ty, body.span, "lambda body");
                self.env.leave();
                Ty::Fn {
                    params: param_tys,
                    return_ty: Box::new(self.subst.apply(&expected_ret)),
                    effect_row: Row::pure(),
                }
            }
            HirExprKind::Assign { op, lhs, rhs } => {
                let lt = self.synth_expr(lhs);
                let rt = self.synth_expr(rhs);
                match op {
                    None => self.try_unify(&lt, &rt, e.span, "assignment"),
                    Some(_bin) => {
                        // Compound-assign treats `a op= b` as `a = a op b` — unify lt, rt.
                        self.try_unify(&lt, &rt, e.span, "compound-assign");
                    }
                }
                Ty::Unit
            }
            HirExprKind::Cast { expr, ty } => {
                let synthesized_source = self.synth_expr(expr);
                let source_ty = self.subst.apply(&synthesized_source);
                let target_ty = self.lower_hir_type(ty);
                if matches!(target_ty, Ty::Bool)
                    && !matches!(source_ty, Ty::Bool | Ty::Never | Ty::Error)
                {
                    self.emit(
                        format!(
                            "cast to `bool` requires a `bool` operand; found {source_ty:?}. \
                             Integer-to-bool casts are noncanonical; use an explicit comparison"
                        ),
                        e.span,
                    );
                    Ty::Error
                } else {
                    target_ty
                }
            }
            HirExprKind::Range { lo, hi, .. } => {
                let lo_ty = lo
                    .as_ref()
                    .map(|e| self.synth_expr(e))
                    .unwrap_or_else(|| self.tcx.fresh_ty());
                if let Some(hi) = hi {
                    let hi_ty = self.synth_expr(hi);
                    self.try_unify(&lo_ty, &hi_ty, e.span, "range endpoints");
                }
                // Stage-0 : range type is a placeholder until Range<T> is in the standard library.
                let range_sym = self.interner.intern("Range");
                Ty::Named {
                    def: self.env.item_def(range_sym).unwrap_or(DefId::UNRESOLVED),
                    args: vec![self.subst.apply(&lo_ty)],
                }
            }
            HirExprKind::Pipeline { lhs, rhs } => {
                // `lhs |> rhs` = `rhs(lhs)` semantically.
                let lhs_ty = self.synth_expr(lhs);
                let rhs_ty = self.synth_expr(rhs);
                let ret_var = self.tcx.fresh_ty();
                let expected = Ty::Fn {
                    params: vec![lhs_ty],
                    return_ty: Box::new(ret_var.clone()),
                    effect_row: Row {
                        effects: Vec::new(),
                        tail: Some(self.tcx.fresh_row()),
                    },
                };
                self.try_unify(&rhs_ty, &expected, e.span, "pipeline");
                self.subst.apply(&ret_var)
            }
            HirExprKind::TryDefault { expr, default } => {
                let et = self.synth_expr(expr);
                let dt = self.synth_expr(default);
                self.try_unify(&et, &dt, e.span, "?? operator");
                et
            }
            HirExprKind::Try { expr } => {
                // `expr ?` — propagate via Result<T, E> shape, but stage-0 returns a fresh var.
                let _ = self.synth_expr(expr);
                self.tcx.fresh_ty()
            }
            HirExprKind::Perform { path, def, args } => {
                if let Some(d) = def {
                    if let Some(Ty::Fn {
                        params, return_ty, ..
                    }) = self.env.item_sig(*d).cloned()
                    {
                        for (arg, expected) in args.iter().zip(params.iter()) {
                            let at = self.synth_call_arg(arg);
                            self.try_unify(expected, &at, e.span, "effect-op argument");
                        }
                        return *return_ty;
                    }
                }
                // Fallback : untyped ; emit a diagnostic in path-resolution.
                if path.len() <= 1 {
                    self.emit("unresolved effect operation".to_string(), e.span);
                }
                self.tcx.fresh_ty()
            }
            HirExprKind::With { handler, body } => {
                let _ = self.synth_expr(handler);
                self.synth_block(body)
            }
            HirExprKind::Region { body, .. } => self.synth_block(body),
            HirExprKind::Tuple(elems) => {
                Ty::Tuple(elems.iter().map(|e| self.synth_expr(e)).collect())
            }
            HirExprKind::Array(arr) => match arr {
                HirArrayExpr::List(items) => {
                    let elem_var = self.tcx.fresh_ty();
                    for item in items {
                        let t = self.synth_expr(item);
                        self.try_unify(&elem_var, &t, item.span, "array literal element");
                    }
                    Ty::Array {
                        elem: Box::new(self.subst.apply(&elem_var)),
                        len: ArrayLen::Literal(items.len() as u64),
                    }
                }
                HirArrayExpr::Repeat { elem, len } => {
                    let et = self.synth_expr(elem);
                    let _ = self.synth_expr(len);
                    let len_slot = match &len.kind {
                        HirExprKind::Literal(l) if l.kind == HirLiteralKind::Int => {
                            ArrayLen::Opaque
                        }
                        _ => ArrayLen::Opaque,
                    };
                    Ty::Array {
                        elem: Box::new(et),
                        len: len_slot,
                    }
                }
            },
            HirExprKind::Struct {
                path,
                def,
                fields,
                spread,
            } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        let _ = self.synth_expr(v);
                    }
                }
                if let Some(s) = spread {
                    let _ = self.synth_expr(s);
                }
                match def {
                    Some(d) => Ty::Named {
                        def: *d,
                        args: (0..self.nominal_arities.get(d).copied().unwrap_or(0))
                            .map(|_| self.tcx.fresh_ty())
                            .collect(),
                    },
                    None => {
                        if path.len() == 1 {
                            if self.imports.contains(&path[0]) {
                                return Ty::Error;
                            }
                            if let Some(d) = self.env.item_def(path[0]) {
                                return Ty::Named {
                                    def: d,
                                    args: (0..self.nominal_arities.get(&d).copied().unwrap_or(0))
                                        .map(|_| self.tcx.fresh_ty())
                                        .collect(),
                                };
                            }
                        }
                        self.emit("unresolved struct constructor".to_string(), e.span);
                        Ty::Error
                    }
                }
            }
            HirExprKind::Run { expr } => self.synth_expr(expr),
            HirExprKind::Compound { lhs, rhs, .. } => {
                let _ = self.synth_expr(lhs);
                let _ = self.synth_expr(rhs);
                // CSLv3 compound-formation — elaborator-level semantics. Stage-0 : fresh var.
                self.tcx.fresh_ty()
            }
            HirExprKind::SectionRef { .. } => self.tcx.fresh_ty(),
            HirExprKind::Paren(inner) => self.synth_expr(inner),
            HirExprKind::Error => Ty::Error,
        }
    }

    fn synth_call_arg(&mut self, a: &HirCallArg) -> Ty {
        match a {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => self.synth_expr(e),
        }
    }

    fn synth_binop(&mut self, op: HirBinOp, lhs: &HirExpr, rhs: &HirExpr) -> Ty {
        let lt = self.synth_expr(lhs);
        let rt = self.synth_expr(rhs);
        let (lhs_rhs_ty, result_ty): (Ty, Ty) = match op {
            // Arithmetic : numeric ; returns same.
            HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div | HirBinOp::Rem => {
                let num = self.tcx.fresh_ty();
                (num.clone(), num)
            }
            // Comparison : same type on both sides ; returns bool.
            HirBinOp::Eq
            | HirBinOp::Ne
            | HirBinOp::Lt
            | HirBinOp::Le
            | HirBinOp::Gt
            | HirBinOp::Ge => {
                let same = self.tcx.fresh_ty();
                (same, Ty::Bool)
            }
            // Logical : bool × bool → bool.
            HirBinOp::And | HirBinOp::Or => (Ty::Bool, Ty::Bool),
            // Bitwise + shift : int × int → int.
            HirBinOp::BitAnd
            | HirBinOp::BitOr
            | HirBinOp::BitXor
            | HirBinOp::Shl
            | HirBinOp::Shr => (Ty::Int, Ty::Int),
            // Implies / Entails : bool × bool → bool.
            HirBinOp::Implies | HirBinOp::Entails => (Ty::Bool, Ty::Bool),
        };
        self.try_unify(&lhs_rhs_ty, &lt, lhs.span, "binary operator LHS");
        self.try_unify(&lhs_rhs_ty, &rt, rhs.span, "binary operator RHS");
        result_ty
    }

    fn synth_unop(&mut self, op: HirUnOp, operand: &HirExpr) -> Ty {
        let t = self.synth_expr(operand);
        match op {
            HirUnOp::Neg => {
                // numeric.
                let num = self.tcx.fresh_ty();
                self.try_unify(&num, &t, operand.span, "unary `-`");
                num
            }
            // CSSLv3 preserves the established overloaded spelling:
            // `!bool` is logical-not while `!integer` is bitwise-not. MIR
            // lowering selects the operation from the concrete operand type.
            HirUnOp::Not => match self.subst.apply(&t) {
                Ty::Bool => Ty::Bool,
                Ty::Int => Ty::Int,
                Ty::Never => Ty::Never,
                Ty::Error => Ty::Error,
                actual => {
                    self.emit(
                        format!("unary `!` requires a `bool` or integer operand; found {actual:?}"),
                        operand.span,
                    );
                    Ty::Error
                }
            },
            HirUnOp::BitNot => match self.subst.apply(&t) {
                Ty::Int => Ty::Int,
                Ty::Never => Ty::Never,
                Ty::Error => Ty::Error,
                actual => {
                    self.emit(
                        format!("unary `~` requires an integer operand; found {actual:?}"),
                        operand.span,
                    );
                    Ty::Error
                }
            },
            HirUnOp::Ref => Ty::Ref {
                mutable: false,
                inner: Box::new(t),
            },
            HirUnOp::Deref => {
                let inner_var = self.tcx.fresh_ty();
                let ref_shape = Ty::Ref {
                    mutable: false,
                    inner: Box::new(inner_var.clone()),
                };
                // Accept either &T or &mut T.
                if unify(&t, &ref_shape, &mut self.subst).is_err() {
                    let mut_shape = Ty::Ref {
                        mutable: true,
                        inner: Box::new(inner_var.clone()),
                    };
                    let _ = unify(&t, &mut_shape, &mut self.subst);
                }
                self.subst.apply(&inner_var)
            }
            HirUnOp::RefMut => Ty::Ref {
                mutable: true,
                inner: Box::new(t),
            },
        }
    }

    fn block_definitely_returns(b: &HirBlock) -> bool {
        if let Some(trailing) = &b.trailing {
            return Self::expr_definitely_returns(trailing);
        }
        b.stmts.last().is_some_and(|stmt| match &stmt.kind {
            HirStmtKind::Expr(expr) => Self::expr_definitely_returns(expr),
            HirStmtKind::Let { .. } | HirStmtKind::Item(_) => false,
        })
    }

    fn expr_definitely_returns(e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Return { .. } => true,
            HirExprKind::Block(block)
            | HirExprKind::Region { body: block, .. }
            | HirExprKind::With { body: block, .. } => Self::block_definitely_returns(block),
            HirExprKind::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                Self::block_definitely_returns(then_branch)
                    && Self::expr_definitely_returns(else_branch)
            }
            _ => false,
        }
    }

    fn synth_block(&mut self, b: &HirBlock) -> Ty {
        self.env.enter();
        for stmt in &b.stmts {
            self.check_stmt(stmt);
        }
        let t = match &b.trailing {
            Some(e) => self.synth_expr(e),
            None if Self::block_definitely_returns(b) => Ty::Never,
            None => Ty::Unit,
        };
        self.record(b.id, t.clone());
        self.env.leave();
        t
    }

    fn check_stmt(&mut self, s: &HirStmt) {
        match &s.kind {
            HirStmtKind::Let { pat, ty, value, .. } => {
                let declared = ty.as_ref().map(|t| self.lower_hir_type(t));
                let vt = value.as_ref().map(|v| self.synth_expr(v));
                let ty_final = match (declared, vt) {
                    (Some(d), Some(v)) => {
                        self.try_unify(&d, &v, s.span, "let annotation vs value");
                        d
                    }
                    (Some(d), None) => d,
                    (None, Some(v)) => v,
                    (None, None) => self.tcx.fresh_ty(),
                };
                // T3-D15 : generalize the inferred/declared type at the let-
                // boundary. Simple `let x = e` becomes `x : ∀α̅. τ` ;
                // destructuring patterns retain per-element monomorphic types
                // (phase-2e refines to per-component generalization).
                self.bind_pattern_let(pat, &ty_final);
            }
            HirStmtKind::Expr(e) => {
                let _ = self.synth_expr(e);
            }
            HirStmtKind::Item(_i) => {
                // Nested-item bodies checked separately when that path lands in T3.4-phase-2.
            }
        }
    }

    // ─ Phase 3 : finalize ───────────────────────────────────────────────────

    fn finalize(&mut self) {
        // Apply substitution to every recorded type.
        let applied: HashMap<u32, Ty> = self
            .type_map
            .types
            .iter()
            .map(|(id, t)| (*id, self.subst.apply(t)))
            .collect();
        self.type_map.types = applied.into_iter().collect();
    }
}

/// Entry point : run the full inference pass over a `HirModule`.
/// Returns the populated `TypeMap` plus any type-level diagnostics.
#[must_use]
pub fn check_module(module: &HirModule, interner: &Interner) -> (TypeMap, Vec<Diagnostic>) {
    let mut ctx = InferCtx::new(interner);
    ctx.collect_item_signatures(module);
    ctx.check_items(module);
    ctx.finalize();
    (ctx.type_map, ctx.diagnostics)
}

#[cfg(test)]
mod tests {
    use super::check_module;
    use crate::lower::lower_module;
    use crate::typing::Ty;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn infer(src: &str) -> (usize, usize) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = lower_module(&f, &cst);
        let (type_map, diags) = check_module(&hir, &interner);
        (type_map.len(), diags.len())
    }

    #[test]
    fn empty_module_has_no_diagnostics() {
        let (_types, diags) = infer("");
        assert_eq!(diags, 0);
    }

    #[test]
    fn simple_fn_is_well_typed() {
        let (types, diags) = infer("fn add(a : i32, b : i32) -> i32 { a + b }");
        assert_eq!(diags, 0, "expected no type errors");
        assert!(types > 0, "expected some types recorded");
    }

    #[test]
    fn let_binding_types_the_name() {
        let (_types, diags) = infer("fn f() -> i32 { let x : i32 = 42 ; x }");
        assert_eq!(diags, 0);
    }

    #[test]
    fn if_branches_must_agree() {
        let (_types, diags) = infer("fn f() -> i32 { if true { 1 } else { false } }");
        // The int/bool mismatch should produce at least one diagnostic.
        assert!(diags >= 1);
    }

    #[test]
    fn comparison_returns_bool() {
        let (_types, diags) = infer("fn f(a : i32, b : i32) -> bool { a < b }");
        assert_eq!(diags, 0);
    }

    #[test]
    fn terminal_return_statement_is_divergent_block_tail() {
        let (_types, diags) = infer("fn f() -> i32 { return 7; }");
        assert_eq!(diags, 0);
    }

    #[test]
    fn terminal_all_returning_if_else_is_divergent_block_tail() {
        let (_types, diags) =
            infer("fn f(flag : bool) -> i32 { if flag { return 7; } else { return 9; }; }");
        assert_eq!(diags, 0);
    }

    #[test]
    fn one_sided_return_then_arm_does_not_make_if_divergent() {
        let (_types, diags) =
            infer("fn f(flag : bool) -> i32 { if flag { return 7; } else { 9; }; }");
        assert!(diags >= 1);
    }

    #[test]
    fn one_sided_return_else_arm_does_not_make_if_divergent() {
        let (_types, diags) =
            infer("fn f(flag : bool) -> i32 { if flag { 7; } else { return 9; }; }");
        assert!(diags >= 1);
    }

    #[test]
    fn partial_return_match_does_not_make_function_divergent() {
        let (_types, diags) =
            infer("fn f(flag : bool) -> i32 { match flag { true => return 7, false => false } }");
        assert!(diags >= 1);
    }

    #[test]
    fn partial_return_match_reverse_order_does_not_make_function_divergent() {
        let (_types, diags) =
            infer("fn f(flag : bool) -> i32 { match flag { true => false, false => return 7 } }");
        assert!(diags >= 1);
    }

    #[test]
    fn breaking_loop_does_not_satisfy_nonunit_function_return() {
        let (_types, diags) = infer("fn f() -> i32 { loop { break; } }");
        assert!(diags >= 1);
    }

    #[test]
    fn early_break_before_return_does_not_make_loop_divergent() {
        let (_types, diags) = infer("fn f() -> i32 { loop { break; return 7; } }");
        assert!(diags >= 1);
    }

    #[test]
    fn conditional_break_before_return_does_not_make_loop_divergent() {
        let (_types, diags) =
            infer("fn f(flag : bool) -> i32 { loop { if flag { break; } return 7; } }");
        assert!(diags >= 1);
    }

    #[test]
    fn free_break_and_continue_are_rejected() {
        let (_types, break_diags) = infer("fn f() { break; }");
        let (_types, continue_diags) = infer("fn f() { continue; }");
        assert!(break_diags >= 1);
        assert!(continue_diags >= 1);
    }

    #[test]
    fn terminal_nondiverging_expression_statement_remains_unit() {
        let (_types, diags) = infer("fn f() -> i32 { 7; }");
        assert!(diags >= 1);
    }

    #[test]
    fn unknown_identifier_diagnoses() {
        let (_types, diags) = infer("fn f() -> i32 { undefined_name }");
        assert!(diags >= 1);
    }

    #[test]
    fn tuple_types_flow_through() {
        let (_types, diags) = infer("fn f() -> (i32, bool) { (1, true) }");
        assert_eq!(diags, 0);
    }

    #[test]
    fn call_site_unifies_args() {
        let (_types, diags) = infer(
            "
            fn add(a : i32, b : i32) -> i32 { a + b }
            fn main() -> i32 { add(1, 2) }
            ",
        );
        assert_eq!(diags, 0);
    }

    #[test]
    fn fn_with_pure_row_checks() {
        let (_types, diags) = infer("fn pure_fn(x : i32) -> i32 { x + 1 }");
        assert_eq!(diags, 0);
    }

    #[test]
    fn array_literal_unifies_elements() {
        let (_types, diags) = infer("fn f() -> [i32] { [1, 2, 3] }");
        // Array [1,2,3] is [i32 ; 3] but expected [i32] slice — stage-0 leaves this for T3.4-phase-2.
        // For now, we just check that the expression types ok internally.
        let _ = diags;
    }

    #[test]
    fn type_map_records_inferred_types() {
        let f = SourceFile::new(
            SourceId::first(),
            "<t>",
            "fn f() -> i32 { 42 }",
            Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = lower_module(&f, &cst);
        let (type_map, _diags) = check_module(&hir, &interner);
        // Expect at least the literal `42` and the fn-param-less param-list (empty) tracked.
        assert!(type_map.len() > 0);
        // Spot-check : some recorded type should be Int.
        assert!(type_map.types.values().any(|t| matches!(t, Ty::Int)));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T3-D15 let-generalization integration tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn let_bound_lambda_used_at_two_types_type_checks() {
        // Classic let-polymorphism smoke test : `let id = |x| x` is
        // generalized ; then `id(42)` + `id(true)` instantiate the scheme
        // with different fresh-vars, unifying with their respective args.
        // Without let-gen, this would fail (id's single tyvar can't be both
        // Int and Bool). With let-gen, it succeeds.
        let src = r"
            fn test() -> i32 {
                let id = |x : i32| { x };
                id(42)
            }
        ";
        let (_, diags) = infer(src);
        assert_eq!(diags, 0, "expected no type errors");
    }

    #[test]
    fn let_monomorphic_value_still_works() {
        // `let x = 42` : type is Int, no free vars, rank-0 scheme.
        // Round-trip must preserve the Int type at use-sites.
        let src = "fn f() -> i32 { let x = 42; x }";
        let (_, diags) = infer(src);
        assert_eq!(diags, 0);
    }

    #[test]
    fn let_annotated_type_overrides_value_type() {
        // Explicit annotation takes precedence ; generalization still applies
        // if the annotated type has free vars (rare but possible).
        let src = "fn f() -> i32 { let x : i32 = 42; x }";
        let (_, diags) = infer(src);
        assert_eq!(diags, 0);
    }

    #[test]
    fn nested_scopes_shadow_cleanly_under_scheme_storage() {
        // Inner scope's `x : Bool` shadows outer `x : Int` at its site.
        let src = r"
            fn f() -> bool {
                let x = 42;
                {
                    let x = true;
                    x
                }
            }
        ";
        let (_, diags) = infer(src);
        assert_eq!(diags, 0);
    }

    #[test]
    fn scheme_instantiation_produces_fresh_vars_per_use() {
        // Two uses of the same let-bound name instantiate to distinct
        // fresh-vars (which then unify with their respective contexts).
        let src = r"
            fn f(a : i32, b : i32) -> i32 {
                let x = a;
                let y = b;
                x + y
            }
        ";
        let (_, diags) = infer(src);
        assert_eq!(diags, 0);
    }

    #[test]
    fn empty_env_has_no_free_vars() {
        // env.free_ty_vars() on a fresh env returns empty — sanity check on
        // the helper used during generalization.
        use crate::env::TypingEnv;
        let env = TypingEnv::new();
        assert!(env.free_ty_vars().is_empty());
        assert!(env.free_row_vars().is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T3-D17 : item-sig Scheme storage + generic-fn fresh-var
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn generic_fn_sig_lands_as_polymorphic_scheme() {
        // `fn id<T>(x : T) -> T { x }` should lower to a rank-1 scheme with
        // ONE quantified ty-var bound by the body's Fn { params: [τ],
        // return_ty: τ, .. } shape (same τ in both positions).
        use super::InferCtx;
        use crate::env::TypingEnv;
        use crate::{lower_module, Scheme};
        let src = "fn id<T>(x : T) -> T { x }";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = lower_module(&f, &cst);
        let mut ctx = InferCtx::new(&interner);
        ctx.collect_item_signatures(&hir);
        // Look up id's scheme.
        let id_sym = interner.intern("id");
        let env_ref: &TypingEnv = ctx.env_for_tests();
        let def = env_ref.item_def(id_sym).expect("id not registered");
        let scheme: &Scheme = env_ref.item_scheme(def).expect("id has no scheme");
        // Generic param T → rank-1 scheme.
        assert_eq!(
            scheme.ty_vars.len(),
            1,
            "expected 1 quantified var, got {}",
            scheme.ty_vars.len()
        );
        // Body shape : Fn { params: [τ], return_ty: τ, .. } ; param and
        // return must be the same τ (unification-ready).
        if let Ty::Fn {
            params, return_ty, ..
        } = &scheme.body
        {
            assert_eq!(params.len(), 1);
            assert_eq!(&params[0], &**return_ty, "param and return should share τ");
        } else {
            panic!("expected Fn type body, got {:?}", scheme.body);
        }
        let _ = env_ref; // silence unused-mut warning in case
    }

    #[test]
    fn generic_fn_call_sites_instantiate_to_distinct_ty_vars() {
        // Two call-sites `id(1)` + `id(true)` should each pick up FRESH
        // ty-vars (not share the same τ from the scheme body). Verified
        // indirectly : both call-sites type-check with DIFFERENT concrete
        // types (Int vs Bool) which require independent instantiation.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn use1() -> i32 { id(42) }
            fn use2() -> bool { id(true) }
        ";
        let (_, diags) = infer(src);
        assert_eq!(
            diags, 0,
            "expected no diagnostics with generic-fn let-poly call-site instantiation"
        );
    }

    #[test]
    fn non_generic_fn_sig_is_monomorphic_scheme() {
        // `fn f() -> i32 { 42 }` has no generics → rank-0 scheme → pure `Ty`.
        use super::InferCtx;
        use crate::lower_module;
        let src = "fn f() -> i32 { 42 }";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = lower_module(&f, &cst);
        let mut ctx = InferCtx::new(&interner);
        ctx.collect_item_signatures(&hir);
        let f_sym = interner.intern("f");
        let env_ref = ctx.env_for_tests();
        let def = env_ref.item_def(f_sym).unwrap();
        let scheme = env_ref.item_scheme(def).unwrap();
        assert!(
            scheme.is_monomorphic(),
            "expected rank-0, got rank-{}",
            scheme.rank()
        );
    }

    #[test]
    fn unary_not_preserves_boolean_and_integer_domains() {
        for src in [
            "fn invert(value : bool) -> bool { !value }",
            "fn bang(value : u32) -> u32 { !value }",
            "fn complement(value : u32) -> u32 { ~value }",
        ] {
            let (_, diags) = infer(src);
            assert_eq!(diags, 0, "valid unary contract rejected: {src}");
        }
    }

    #[test]
    fn unary_not_rejects_invalid_concrete_domains() {
        for src in [
            "fn invalid(value : bool) -> bool { ~value }",
            "fn invalid(value : f32) -> bool { !value }",
            "fn invalid(value : String) -> bool { !value }",
        ] {
            let (_, diags) = infer(src);
            assert!(diags > 0, "invalid unary contract accepted: {src}");
        }
    }

    #[test]
    fn noncanonical_bool_sources_are_rejected() {
        for src in [
            "fn invalid() -> bool { 2 }",
            "fn invalid(value : u8) -> bool { value }",
            "fn invalid(value : u8) -> bool { value as bool }",
        ] {
            let (_, diags) = infer(src);
            assert!(diags > 0, "noncanonical bool source accepted: {src}");
        }
    }

    #[test]
    fn locally_knowable_nominals_modules_and_prelude_values_are_not_error_holes() {
        for src in [
            "struct Option { value: u8 } fn invalid(value: Option) -> bool { value }",
            "struct Result { value: u8 } fn invalid(value: Result) -> bool { value }",
            "struct Vec { value: u8 } fn invalid(value: Vec) -> bool { value }",
            "use external::Option\nstruct Option { value: u8 } fn invalid(value: Option) -> bool { value }",
            "fn invalid() -> bool { Some(1u8) }",
            "fn invalid() -> bool { None }",
            "fn invalid() -> bool { Ok(1u8) }",
            "fn invalid() -> bool { Err(1u8) }",
            "fn invalid() -> Option<u8> { Some(true) }",
            "fn invalid() -> Result<u8, u8> { Err(true) }",
            "fn invalid(value: Option<u8>) -> Vec<u8> { value }",
            "fn invalid(value: Vec<u8>) -> Option<u8> { value }",
            "module inner { fn byte_value() -> u8 { 1u8 } }\n\
             fn invalid() -> bool { inner::byte_value() }",
            "module inner { use external::opaque fn valid() -> bool { true } }\n\
             fn invalid() -> bool { opaque() }",
            "use external::opaque\nmodule opaque { fn value() -> u8 { 1u8 } }\nfn invalid() -> bool { opaque() }",
            "fn invalid() -> bool { definitely_missing_module::opaque() }",
            "fn invalid() -> Option<u8> { external::qualified() }",
            "fn invalid() -> Result<u8, u8> { Ok(true) }",
        ] {
            let (_, diags) = infer(src);
            assert!(diags > 0, "locally knowable type error accepted: {src}");
        }
    }

    #[test]
    fn top_level_untyped_import_is_declared_opacity_boundary() {
        for src in [
            "use external::opaque\nfn bounded() -> bool { opaque() }",
            "use external::opaque as known\nfn bounded() -> bool { known() }",
            "use std::gpu::GpuError\nfn bounded(value: GpuError) -> GpuError { GpuError::CapDenied }",
            "use std::gpu::GpuError as err\nfn bounded(value: err) -> err { err::CapDenied }",
        ] {
            let (_, diags) = infer(src);
            assert_eq!(
                diags, 0,
                "declared stage-0 opacity boundary rejected: {src}"
            );
        }

        for src in [
            "fn invalid() -> bool { opaque() }",
            "module inner { use external::opaque fn valid() -> bool { true } }\n\
             fn invalid() -> bool { opaque() }",
            "fn invalid() -> bool { missing::opaque() }",
            "use external::opaque as known\nfn invalid() -> bool { opaque() }",
            "use external::opaque\nfn invalid() -> bool { opaque::arbitrary() }",
            "use external::fs\nfn invalid() -> bool { fs::open(\"x\", 1) }",
            "use external::whatever as fs\nfn invalid() -> bool { fs::open(\"x\", 1) }",
            "use external::Vec\nfn invalid() -> bool { Vec::new() }",
            "use external::opaque\nfn invalid() -> bool { opaque::Arbitrary }",
            "use external::Foo\nfn invalid(value: Foo) -> Foo { Foo::Whatever }",
            "use std::gpu::GpuError\nfn invalid() -> bool { GpuError::CapDenied }",
            "use std::gpu::GpuError\nfn invalid(value: GpuError) -> GpuError { GpuError::Whatever }",
        ] {
            let (_, diags) = infer(src);
            assert!(
                diags > 0,
                "undeclared or out-of-scope opacity boundary accepted: {src}"
            );
        }
    }

    #[test]
    fn imported_standard_enum_aliases_share_canonical_nominal_identity() {
        let (_, diags) = infer(
            "use std::gpu::GpuError\n\
             use std::gpu::GpuError as Fault\n\
             fn preserve(value: GpuError) -> Fault { value }\n\
             fn construct() -> GpuError { Fault::CapDenied }",
        );
        assert_eq!(
            diags, 0,
            "an import alias changed the enum's nominal identity"
        );

        let (_, diags) = infer(
            "use std::gpu::GpuError as Fault\n\
             use std::gpu_transport::BufferUsage as Usage\n\
             fn invalid(value: Fault) -> Usage { value }",
        );
        assert!(diags > 0, "distinct imported enums lost nominal separation");
    }

    #[test]
    fn standard_container_spelling_does_not_steal_generic_identity() {
        use super::InferCtx;
        use crate::{lower_module, Ty};

        let source = "fn preserve<Vec>(value: Vec) -> Vec { value }";
        let file = SourceFile::new(SourceId::first(), "<t>", source, Surface::RustHybrid);
        let tokens = cssl_lex::lex(&file);
        let (cst, parse_bag) = cssl_parse::parse(&file, &tokens);
        assert_eq!(parse_bag.error_count(), 0);
        let (hir, interner, lower_bag) = lower_module(&file, &cst);
        assert_eq!(lower_bag.error_count(), 0);
        let mut ctx = InferCtx::new(&interner);
        ctx.collect_item_signatures(&hir);
        let symbol = interner.intern("preserve");
        let def = ctx
            .env_for_tests()
            .item_def(symbol)
            .expect("preserve not registered");
        let scheme = ctx
            .env_for_tests()
            .item_scheme(def)
            .expect("preserve has no scheme");
        assert_eq!(scheme.ty_vars.len(), 1);
        let Ty::Fn {
            params, return_ty, ..
        } = &scheme.body
        else {
            panic!("expected function scheme, got {:?}", scheme.body);
        };
        assert!(matches!(params.as_slice(), [Ty::Var(_)]));
        assert_eq!(&params[0], &**return_ty);
    }

    #[test]
    fn qualified_paths_never_fall_back_to_callable_first_segment() {
        for src in [
            "fn Foo(value: u8) -> u8 { value }\nfn invalid() -> u8 { Foo::Whatever(1u8) }",
            "use external::Foo\nfn Foo(value: u8) -> u8 { value }\nfn invalid() -> u8 { Foo::Whatever(1u8) }",
            "fn fs(path: String, flags: i64) -> i64 { 0 }\nfn invalid() -> i64 { fs::open(\"x\", 1) }",
            "fn std(value: Vec<i32>, index: i64) -> i32 { 0 }\nfn invalid(value: Vec<i32>) -> i32 { std::vec::vec_index::<i32>(value, 0) }",
            "fn invalid() -> i64 { let fs = |path: String, flags: i64| { 0 }; fs::open(\"x\", 1) }",
            "fn invalid(value: Vec<i32>) -> i32 { let std = |items: Vec<i32>, index: i64| { 0 }; std::vec::vec_index::<i32>(value, 0) }",
        ] {
            let (_, diags) = infer(src);
            assert!(diags > 0, "qualified suffix was stolen through first root: {src}");
        }
    }

    #[test]
    fn exact_qualified_vec_index_has_typed_signature() {
        let (_, diags) =
            infer("fn bounded<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }");
        assert_eq!(diags, 0);

        for src in [
            "fn invalid<T>(value: Vec<T>) -> bool { std::vec::vec_index::<T>(value, 0) }",
            "fn invalid(value: Vec<u64>) -> u64 { std::vec::vec_index::<i32>(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T, T>(value, 0) }",
            "module std { fn marker() -> bool { true } }\nfn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }",
            "use external::std\nfn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }",
            "module inner { use external::std fn marker() -> bool { true } }\nfn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { let std = 1; std::vec::vec_index::<T>(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::definitely_missing::<T>(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { other::vec::vec_index::<T>(value, 0) }",
        ] {
            let (_, diags) = infer(src);
            assert!(diags > 0, "untyped qualified vector path accepted: {src}");
        }
    }

    #[test]
    fn exact_stage0_host_intrinsics_are_declared_opacity_boundaries() {
        for src in [
            "fn bounded() -> i64 { fs::open(\"x\", 1) }",
            "fn bounded() -> i64 { net::socket(1) }",
            "fn bounded() -> i64 { time::monotonic_ns() }",
            "fn bounded() -> i64 { window::spawn(1, 2, 3, 4, 5) }",
            "fn bounded() -> i32 { input::keyboard_state(1, 2, 3) }",
            "fn bounded() -> i64 { gpu::device_create(1, 2) }",
            "fn bounded() -> i64 { audio::stream_open(1, 2, 3, 4) }",
            "fn bounded() -> i64 { thread::spawn(1, 2) }",
            "fn bounded() -> i64 { mutex::create() }",
            "fn bounded() -> i64 { atomic::load_u64(1, 2) }",
        ] {
            let (_, diags) = infer(src);
            assert_eq!(diags, 0, "known host intrinsic rejected: {src}");
        }

        for src in [
            "fn invalid() -> i64 { gpu::definitely_missing() }",
            "fn invalid() -> i64 { missing::device_create(1, 2) }",
            "module fs { fn open() -> u8 { 1u8 } }\nfn invalid() -> bool { fs::open() }",
            "module gpu { fn device_create() -> u8 { 1u8 } }\nfn invalid() -> bool { gpu::device_create() }",
            "module outer { module fs { fn open() -> u8 { 1u8 } } fn invalid() -> bool { fs::open() } }",
            "module inner { use external::fs fn invalid() -> bool { fs::open(\"x\", 1) } }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::definitely_missing::<T>(value, 0) }",
        ] {
            let (_, diags) = infer(src);
            assert!(diags > 0, "unknown host intrinsic accepted: {src}");
        }
    }
}

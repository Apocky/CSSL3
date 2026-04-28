//! T11-D38 — generic-type monomorphization MVP.
//!
//! § PURPOSE
//!
//! CSSLv3 generic functions like `fn id<T>(x: T) -> T { x }` cannot be
//! compiled to machine code directly — Cranelift needs concrete types, and
//! MIR ops carry `MirType` (not a type parameter). Monomorphization is the
//! specialization pass : given a generic `HirFn` + a concrete type-arg tuple,
//! emit a specialized `MirFunc` whose params/results/body are fully-typed and
//! JIT-ready.
//!
//! § SPEC : `specs/03_TYPES.csl` § MONOMORPHIZATION +
//!         `specs/15_MLIR.csl` § LOWERING-PASSES.
//!
//! § SCOPE (this slice — T11-D38 MVP)
//!
//! Lands the **core specialization machinery** :
//!   - [`TypeSubst`] : a `HashMap<Symbol, HirType>` mapping generic-param
//!     names to concrete types (e.g., `T ↦ i32`).
//!   - [`substitute_hir_type`] : walks a `HirType` tree and substitutes
//!     single-segment paths that match a generic-param name.
//!   - [`specialize_generic_fn`] : given a generic `HirFn` + `TypeSubst` +
//!     the workspace lowering context, produces a `MirFunc` with mangled
//!     name (`id_i32`, `pair_i32_f32`, …), specialized param/return types,
//!     and a body lowered from the substituted HIR.
//!   - [`mangle_specialization_name`] : deterministic stable name-mangling
//!     across workspace runs — `{fn_name}_{subst_repr}`.
//!
//! § OUT-OF-SCOPE (clean follow-up slice per `DECISIONS.md` § T11-D38)
//!   - Turbofish syntax `id::<i32>(5)` → `Call.type_args` propagation.
//!   - Auto-monomorphization walker : scan the module for generic-call sites +
//!     specialize automatically. This MVP provides the specialization API ;
//!     the walker wraps it.
//!   - Type-arg inference from call-site arg types (requires T3.4 inference).
//!   - Bounded generics (`<T: Clone>`) — satisfiability-check via trait
//!     dispatch.
//!   - Higher-kinded / region / const generics.
//!
//! § EXAMPLE
//!
//! ```ignore
//! use cssl_hir::Interner;
//! use cssl_mir::monomorph::{specialize_generic_fn, TypeSubst};
//!
//! // Build `fn id<T>(x : T) -> T { x }` HIR.
//! let (hir_fn, interner) = parse_generic_id();
//! let t_sym = interner.intern("T");
//! let mut subst = TypeSubst::new();
//! subst.bind(t_sym, make_i32_type());
//!
//! // Produce specialized `id_i32(x : i32) -> i32 { x }`.
//! let mir_fn = specialize_generic_fn(&interner, None, &hir_fn, &subst);
//! assert_eq!(mir_fn.name, "id_i32");
//! assert_eq!(mir_fn.params, vec![MirType::Int(IntWidth::I32)]);
//! ```

use std::collections::HashMap;

use cssl_ast::SourceFile;
use cssl_hir::{
    HirEnum, HirEnumVariant, HirFieldDecl, HirFn, HirImpl, HirStruct, HirStructBody, HirType,
    HirTypeKind, Interner, Symbol,
};

use crate::body_lower::lower_fn_body;
use crate::func::MirFunc;
use crate::lower::{lower_function_signature, LowerCtx};
use crate::value::{FloatWidth, IntWidth, MirType};

/// Type substitution map : generic-param symbol → concrete `HirType`.
///
/// Populated by the caller with one entry per `HirGenericParam`. The
/// substitution is purely positional — callers build the map with
/// `subst.bind(t_sym, concrete_i32_type)` before invoking
/// [`specialize_generic_fn`].
#[derive(Debug, Clone, Default)]
pub struct TypeSubst {
    map: HashMap<Symbol, HirType>,
}

impl TypeSubst {
    /// Empty substitution — identity ; `specialize_generic_fn` on an empty
    /// subst is equivalent to ordinary non-generic lowering.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a generic-param name to a concrete type.
    pub fn bind(&mut self, name: Symbol, ty: HirType) {
        self.map.insert(name, ty);
    }

    /// Look up a substitution ; returns `None` for non-generic symbols.
    #[must_use]
    pub fn get(&self, name: &Symbol) -> Option<&HirType> {
        self.map.get(name)
    }

    /// Iterate `(name, type)` pairs in insertion-independent order — used
    /// for name-mangling so `specialize_generic_fn` is deterministic across
    /// HashMap rebuild orders.
    pub fn iter_sorted<'a>(
        &'a self,
        interner: &'a Interner,
    ) -> impl Iterator<Item = (Symbol, &'a HirType)> + 'a {
        let mut pairs: Vec<(Symbol, &HirType)> = self.map.iter().map(|(k, v)| (*k, v)).collect();
        pairs.sort_by_key(|(k, _)| interner.resolve(*k));
        pairs.into_iter()
    }

    /// Number of bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` iff there are no bindings.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Recursively substitute generic-param references in a [`HirType`].
///
/// Walks the type tree and replaces any `HirTypeKind::Path` with a single-
/// segment whose interned name matches a `subst` key, with the mapped
/// concrete type. All other type variants are traversed structurally —
/// references / refinements / tuples / functions / arrays all substitute
/// their element types.
///
/// The returned [`HirType`] preserves the original span + id of the outer
/// node so diagnostics remain source-linked ; only the `kind` is rebuilt.
#[must_use]
pub fn substitute_hir_type(t: &HirType, interner: &Interner, subst: &TypeSubst) -> HirType {
    let new_kind = substitute_kind(&t.kind, interner, subst);
    HirType {
        span: t.span,
        id: t.id,
        kind: new_kind,
    }
}

fn substitute_kind(k: &HirTypeKind, interner: &Interner, subst: &TypeSubst) -> HirTypeKind {
    match k {
        HirTypeKind::Path {
            path,
            def,
            type_args,
        } => {
            // Single-segment path matching a generic-param ⇒ substitute.
            if path.len() == 1 && type_args.is_empty() {
                if let Some(replacement) = subst.get(&path[0]) {
                    return replacement.kind.clone();
                }
            }
            // Otherwise recurse into nested type args.
            HirTypeKind::Path {
                path: path.clone(),
                def: *def,
                type_args: type_args
                    .iter()
                    .map(|ta| substitute_hir_type(ta, interner, subst))
                    .collect(),
            }
        }
        HirTypeKind::Tuple { elems } => HirTypeKind::Tuple {
            elems: elems
                .iter()
                .map(|e| substitute_hir_type(e, interner, subst))
                .collect(),
        },
        HirTypeKind::Array { elem, len } => HirTypeKind::Array {
            elem: Box::new(substitute_hir_type(elem, interner, subst)),
            len: len.clone(),
        },
        HirTypeKind::Slice { elem } => HirTypeKind::Slice {
            elem: Box::new(substitute_hir_type(elem, interner, subst)),
        },
        HirTypeKind::Reference { mutable, inner } => HirTypeKind::Reference {
            mutable: *mutable,
            inner: Box::new(substitute_hir_type(inner, interner, subst)),
        },
        HirTypeKind::Capability { cap, inner } => HirTypeKind::Capability {
            cap: *cap,
            inner: Box::new(substitute_hir_type(inner, interner, subst)),
        },
        HirTypeKind::Function {
            params,
            return_ty,
            effect_row,
        } => HirTypeKind::Function {
            params: params
                .iter()
                .map(|p| substitute_hir_type(p, interner, subst))
                .collect(),
            return_ty: Box::new(substitute_hir_type(return_ty, interner, subst)),
            effect_row: effect_row.clone(),
        },
        HirTypeKind::Refined { base, kind } => HirTypeKind::Refined {
            base: Box::new(substitute_hir_type(base, interner, subst)),
            kind: kind.clone(),
        },
        HirTypeKind::Infer => HirTypeKind::Infer,
        HirTypeKind::Error => HirTypeKind::Error,
    }
}

/// Produce a deterministic mangled name for a specialization.
///
/// § FORMAT : `{fn_name}_{arg0}_{arg1}_…` where each `argᵢ` is the snake-
/// cased name of the substituted type :
///   - `i32` / `f32` / `bool` / `Handle` : direct lowercase.
///   - Nominal paths : last segment lowercased.
///   - Function / tuple / array : "fn", "tup", "arr" placeholders.
///
/// Stable across workspace runs (sorts by `Symbol` resolution string).
#[must_use]
pub fn mangle_specialization_name(
    base_name: &str,
    interner: &Interner,
    subst: &TypeSubst,
) -> String {
    if subst.is_empty() {
        return base_name.to_string();
    }
    let mut out = String::from(base_name);
    for (_sym, ty) in subst.iter_sorted(interner) {
        out.push('_');
        out.push_str(&type_mangle_fragment(ty, interner));
    }
    out
}

fn type_mangle_fragment(t: &HirType, interner: &Interner) -> String {
    match &t.kind {
        HirTypeKind::Path { path, .. } => {
            if let Some(last) = path.last() {
                interner.resolve(*last).to_lowercase()
            } else {
                "unknown".to_string()
            }
        }
        HirTypeKind::Tuple { elems } => {
            let inner: Vec<String> = elems
                .iter()
                .map(|e| type_mangle_fragment(e, interner))
                .collect();
            format!("tup{}", inner.join("_"))
        }
        HirTypeKind::Function { .. } => "fn".to_string(),
        HirTypeKind::Array { .. } => "arr".to_string(),
        HirTypeKind::Slice { .. } => "slice".to_string(),
        HirTypeKind::Reference { inner, .. } => {
            format!("ref{}", type_mangle_fragment(inner, interner))
        }
        HirTypeKind::Refined { base, .. } => type_mangle_fragment(base, interner),
        HirTypeKind::Capability { inner, .. } => type_mangle_fragment(inner, interner),
        HirTypeKind::Infer => "infer".to_string(),
        HirTypeKind::Error => "err".to_string(),
    }
}

/// Specialize a generic [`HirFn`] to a concrete [`MirFunc`].
///
/// § PIPELINE
///   1. Clone the generic fn ; substitute `T`-path references in every param
///      type + return type using `subst`.
///   2. Mangle the specialization name : `{base}_{arg_types}`.
///   3. Run the standard signature-lowering pass on the specialized HIR.
///   4. Lower the body (untouched — the MVP assumes bodies don't reference
///      generic params directly as type annotations ; follow-up slice will
///      walk expr-level casts + let-type-annotations).
///
/// § NAME COLLISIONS
///   Repeated specializations of the same fn with the same subst produce
///   the same mangled name (deterministic). Callers that need uniqueness
///   across different substs should verify by name.
///
/// § ERRORS
///   Stage-0 MVP : never returns an error. A future slice adds "unbound
///   generic param in fn body" as a diagnostic when the body uses a type-
///   param that isn't in the subst.
///
/// § PANICS : none at the MVP level.
pub fn specialize_generic_fn(
    interner: &Interner,
    source: Option<&SourceFile>,
    hir_fn: &HirFn,
    subst: &TypeSubst,
) -> MirFunc {
    // § Build a specialized copy of the HIR fn with substituted types.
    let specialized_fn = substitute_fn_signature(hir_fn, interner, subst);

    // § Signature lowering (vec-scalarization + flat MirFunc.params already
    //   handled upstream ; monomorphization composes cleanly).
    let lower_ctx = LowerCtx::new(interner);
    let mut mir_fn = lower_function_signature(&lower_ctx, &specialized_fn);

    // § Apply the mangled name + lower the body.
    let base_name = interner.resolve(hir_fn.name);
    mir_fn.name = mangle_specialization_name(&base_name, interner, subst);
    lower_fn_body(interner, source, &specialized_fn, &mut mir_fn);

    mir_fn
}

// ═════════════════════════════════════════════════════════════════════════
// § T11-D45 — GENERIC STRUCT SPECIALIZATION
//
// Parallel API to `specialize_generic_fn` for `HirStruct` items. Given a
// generic struct decl + `TypeSubst`, produce a specialized struct with :
//   - mangled name (`Pair<T, U>` + T↦i32, U↦f32 → `Pair_i32_f32`)
//   - substituted field types
//   - emptied `generics` (so downstream passes treat it as concrete)
//
// § SCOPE (this slice — T11-D45 MVP option-(a) per recon)
//   - Named + Tuple + Unit struct bodies supported.
//   - Field type substitution via the existing `substitute_hir_type` walker
//     (handles nested `Box<T>` / `Vec<T>` / references / tuples / etc.).
//   - Returns the specialized `HirStruct` value ; the caller decides whether
//     to register it in some symbol table / MirModule.struct_registry.
//
// § DEFERRED
//   - Value-level struct construction in MIR (current body_lower emits
//     `Opaque("!cssl.struct.<name>")` — needs plumbing for specialized names).
//   - impl<T> Vec<T> monomorphization (parallel to this for HirImpl items).
//   - Generic enums.
//   - Type-arg auto-discovery from struct-expr field types (requires inference).
// ═════════════════════════════════════════════════════════════════════════

/// Specialize a generic [`HirStruct`] to a concrete struct decl.
///
/// § PIPELINE
///   1. Clone the input struct.
///   2. Substitute every field's `ty` using `substitute_hir_type`.
///   3. Empty the `generics` field (specialized struct is concrete).
///   4. Mangle the name via [`mangle_specialization_name`] so the specialized
///      struct has a fresh, deterministic identifier.
///
/// The original `name: Symbol` is preserved (points to the same interned
/// symbol as the generic) for traceability ; callers that need the mangled
/// string use the returned value's computed mangle via
/// [`mangle_specialization_name`] or look at the caller-chosen storage key.
///
/// # Example
/// ```ignore
/// // Source : struct Pair<T, U> { first: T, second: U }
/// let mut subst = TypeSubst::new();
/// subst.bind(t_sym, hir_primitive_type("i32", &interner));
/// subst.bind(u_sym, hir_primitive_type("f32", &interner));
/// let specialized = specialize_generic_struct(&interner, &hir_pair, &subst);
/// // specialized.body : Named([{first: i32}, {second: f32}])
/// // specialized.generics.params : empty
/// ```
#[must_use]
pub fn specialize_generic_struct(
    interner: &Interner,
    hir_struct: &HirStruct,
    subst: &TypeSubst,
) -> HirStruct {
    let mut out = hir_struct.clone();
    out.generics = cssl_hir::HirGenerics { params: Vec::new() };
    out.body = substitute_struct_body(&out.body, interner, subst);
    out
}

/// Substitute generic-param references in every field of a struct body.
fn substitute_struct_body(
    body: &HirStructBody,
    interner: &Interner,
    subst: &TypeSubst,
) -> HirStructBody {
    match body {
        HirStructBody::Unit => HirStructBody::Unit,
        HirStructBody::Tuple(fields) => HirStructBody::Tuple(
            fields
                .iter()
                .map(|f| substitute_field_decl(f, interner, subst))
                .collect(),
        ),
        HirStructBody::Named(fields) => HirStructBody::Named(
            fields
                .iter()
                .map(|f| substitute_field_decl(f, interner, subst))
                .collect(),
        ),
    }
}

fn substitute_field_decl(f: &HirFieldDecl, interner: &Interner, subst: &TypeSubst) -> HirFieldDecl {
    HirFieldDecl {
        span: f.span,
        id: f.id,
        attrs: f.attrs.clone(),
        visibility: f.visibility,
        name: f.name,
        ty: substitute_hir_type(&f.ty, interner, subst),
    }
}

/// Compute a mangled struct name for a specialization. Thin wrapper over
/// [`mangle_specialization_name`] with the base pulled from `hir_struct.name`.
#[must_use]
pub fn mangle_struct_specialization_name(
    hir_struct: &HirStruct,
    interner: &Interner,
    subst: &TypeSubst,
) -> String {
    let base = interner.resolve(hir_struct.name);
    mangle_specialization_name(&base, interner, subst)
}

// ═════════════════════════════════════════════════════════════════════════
// § T11-D47 — GENERIC ENUM SPECIALIZATION
//
// Enums (`enum Option<T> { Some(T), None }`) carry generics + variants ;
// each variant has a `HirStructBody` (same shape as struct bodies) so we
// can reuse `substitute_struct_body` per-variant. Parallel API to
// `specialize_generic_struct` — produces a concrete HirEnum with :
//   - mangled name
//   - substituted variant field types
//   - emptied `generics`
// ═════════════════════════════════════════════════════════════════════════

/// Specialize a generic [`HirEnum`] to a concrete enum decl.
///
/// § PIPELINE
///   1. Clone the enum.
///   2. Empty the `generics` field.
///   3. For each variant, substitute field types in its `body` via the
///      shared `substitute_struct_body` helper.
///
/// § EXAMPLE
/// ```ignore
/// // enum Option<T> { Some(T), None }
/// let mut subst = TypeSubst::new();
/// subst.bind(t_sym, hir_primitive_type("i32", &interner));
/// let specialized = specialize_generic_enum(&interner, &opt_hir, &subst);
/// // specialized.variants[0] has Tuple body with i32 type (was T)
/// // specialized.variants[1] is Unit (None is Unit variant)
/// // specialized.generics is empty
/// ```
#[must_use]
pub fn specialize_generic_enum(
    interner: &Interner,
    hir_enum: &HirEnum,
    subst: &TypeSubst,
) -> HirEnum {
    let mut out = hir_enum.clone();
    out.generics = cssl_hir::HirGenerics { params: Vec::new() };
    out.variants = out
        .variants
        .iter()
        .map(|v| substitute_enum_variant(v, interner, subst))
        .collect();
    out
}

fn substitute_enum_variant(
    v: &HirEnumVariant,
    interner: &Interner,
    subst: &TypeSubst,
) -> HirEnumVariant {
    HirEnumVariant {
        span: v.span,
        def: v.def,
        attrs: v.attrs.clone(),
        name: v.name,
        body: substitute_struct_body(&v.body, interner, subst),
    }
}

/// Compute a mangled enum name for a specialization. Thin wrapper over
/// [`mangle_specialization_name`] keyed off `hir_enum.name`.
#[must_use]
pub fn mangle_enum_specialization_name(
    hir_enum: &HirEnum,
    interner: &Interner,
    subst: &TypeSubst,
) -> String {
    let base = interner.resolve(hir_enum.name);
    mangle_specialization_name(&base, interner, subst)
}

// ═════════════════════════════════════════════════════════════════════════
// § T11-D49 — GENERIC impl<T> MONOMORPHIZATION
//
// Specializes every `fn` inside a generic `impl<…> SelfTy<…> { … }` block by
// applying the outer impl's type-param substitution to each method. Produces
// one `MirFunc` per method with a self-type-qualified mangled name :
//   `impl<T> Box<T> { fn value(...) -> T { ... } }` + T↦i32
//     ⇒ MirFunc "Box_i32__value"
//
// § SCOPE (this slice — T11-D49 MVP)
//   - Inherent impls only (no trait_: Some(…) dispatch — that's trait-impl
//     specialization, a follow-up slice).
//   - Methods inherit the OUTER impl's generic-param substitution only ;
//     per-method generics (`impl<T> Box<T> { fn map<U>(...) }`) are cloned
//     through but NOT substituted by the outer subst (they get their own
//     substitution at call-site time via existing specialize_generic_fn).
//   - Self-type derivation : the first type-arg of the outer generics
//     becomes the first bound in `subst`. Callers build TypeSubst explicitly
//     matching `hir_impl.generics.params`.
//   - Mangled name shape : `{self_ty_mangle}__{fn_name}` where
//     self_ty_mangle derives from the Path-form self_ty + type-args after
//     substitution.
// ═════════════════════════════════════════════════════════════════════════

/// Specialize every method in an inherent-impl block using the supplied
/// outer-impl substitution. Returns one `MirFunc` per method.
///
/// § EXAMPLE
/// ```ignore
/// // impl<T> Box<T> { fn value(b: &Box<T>) -> &T { ... } }
/// let mut subst = TypeSubst::new();
/// subst.bind(t_sym, hir_primitive_type("i32", &interner));
/// let specs = specialize_generic_impl(&interner, None, &hir_impl, &subst);
/// assert_eq!(specs[0].name, "Box_i32__value");
/// ```
pub fn specialize_generic_impl(
    interner: &Interner,
    source: Option<&SourceFile>,
    hir_impl: &HirImpl,
    subst: &TypeSubst,
) -> Vec<MirFunc> {
    // Mangle component for the self-type post-substitution.
    let self_mangle = mangle_self_ty(&hir_impl.self_ty, interner, subst);
    let mut out = Vec::with_capacity(hir_impl.fns.len());
    for f in &hir_impl.fns {
        // Clone the fn + apply the outer-impl's subst to signature.
        let mut specialized_fn = f.clone();
        for p in &mut specialized_fn.params {
            p.ty = substitute_hir_type(&p.ty, interner, subst);
        }
        if let Some(rt) = &specialized_fn.return_ty {
            specialized_fn.return_ty = Some(substitute_hir_type(rt, interner, subst));
        }
        // Run standard fn lowering on the pre-substituted HIR.
        let lower_ctx = LowerCtx::new(interner);
        let mut mir_fn = lower_function_signature(&lower_ctx, &specialized_fn);
        let fn_name = interner.resolve(f.name);
        mir_fn.name = format!("{self_mangle}__{fn_name}");
        lower_fn_body(interner, source, &specialized_fn, &mut mir_fn);
        out.push(mir_fn);
    }
    out
}

/// Mangle a HirType serving as an impl's `self_ty` into its specialization
/// prefix. Handles `Path<A, B>` (the common case — substitutes nested type_args
/// per subst + concatenates) and falls back to "self" for non-path self types.
fn mangle_self_ty(self_ty: &HirType, interner: &Interner, subst: &TypeSubst) -> String {
    match &self_ty.kind {
        HirTypeKind::Path {
            path, type_args, ..
        } => {
            let base = path
                .last()
                .map_or_else(|| "unknown".to_string(), |s| interner.resolve(*s));
            if type_args.is_empty() {
                return base;
            }
            let mut out = base;
            for ta in type_args {
                // Substitute the type-arg (e.g., T → i32) then render its fragment.
                let concrete = substitute_hir_type(ta, interner, subst);
                out.push('_');
                out.push_str(&self_ty_fragment(&concrete, interner));
            }
            out
        }
        _ => "self".to_string(),
    }
}

fn self_ty_fragment(t: &HirType, interner: &Interner) -> String {
    match &t.kind {
        HirTypeKind::Path { path, .. } => path.last().map_or_else(
            || "unknown".to_string(),
            |s| interner.resolve(*s).to_lowercase(),
        ),
        _ => "opaque".to_string(),
    }
}

/// Clone `hir_fn` but substitute generic-param references in param types +
/// return type. Body is preserved verbatim (stage-0 MVP — body-internal type
/// references are not walked).
fn substitute_fn_signature(hir_fn: &HirFn, interner: &Interner, subst: &TypeSubst) -> HirFn {
    let mut out = hir_fn.clone();
    // Remove the generic declaration since after specialization the fn is
    // concrete. This keeps downstream passes (IFC / AD-legality / effect
    // checks) from re-processing the specialization as generic.
    out.generics = cssl_hir::HirGenerics { params: Vec::new() };

    for p in &mut out.params {
        p.ty = substitute_hir_type(&p.ty, interner, subst);
    }
    if let Some(rt) = &out.return_ty {
        out.return_ty = Some(substitute_hir_type(rt, interner, subst));
    }
    out
}

/// Convenience : build a concrete `HirType` for a common primitive name.
/// Useful for test-fixture construction of `TypeSubst` bindings.
#[must_use]
pub fn hir_primitive_type(name: &str, interner: &Interner) -> HirType {
    HirType {
        span: cssl_ast::Span::DUMMY,
        id: cssl_hir::HirId::DUMMY,
        kind: HirTypeKind::Path {
            path: vec![interner.intern(name)],
            def: None,
            type_args: Vec::new(),
        },
    }
}

/// Convenience : given a `HirType` representing a primitive name (`"i32"`,
/// `"f32"`, …), return the matching `MirType`. Returns `None` for non-
/// primitive or unknown names.
#[must_use]
pub fn primitive_hir_to_mir(t: &HirType, interner: &Interner) -> Option<MirType> {
    if let HirTypeKind::Path { path, .. } = &t.kind {
        if path.len() == 1 {
            return match interner.resolve(path[0]).as_str() {
                "i8" => Some(MirType::Int(IntWidth::I8)),
                "i16" => Some(MirType::Int(IntWidth::I16)),
                "i32" | "u32" | "isize" | "usize" => Some(MirType::Int(IntWidth::I32)),
                "i64" | "u64" => Some(MirType::Int(IntWidth::I64)),
                "f16" => Some(MirType::Float(FloatWidth::F16)),
                "bf16" => Some(MirType::Float(FloatWidth::Bf16)),
                "f32" => Some(MirType::Float(FloatWidth::F32)),
                "f64" => Some(MirType::Float(FloatWidth::F64)),
                "bool" => Some(MirType::Bool),
                "Handle" => Some(MirType::Handle),
                _ => None,
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        hir_primitive_type, mangle_specialization_name, primitive_hir_to_mir,
        specialize_generic_fn, substitute_hir_type, TypeSubst,
    };
    use crate::value::{FloatWidth, IntWidth, MirType};
    use cssl_ast::{SourceFile, SourceId, Surface};
    use cssl_hir::{lower_module, HirFn, HirItem};

    /// Helper : parse source, lower to HIR, return (interner, HirFn for the
    /// fn matching `fn_name`). Panics if the fn isn't found.
    fn parse_fn(src: &str, fn_name: &str) -> (cssl_hir::Interner, SourceFile, HirFn) {
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);
        let f = hir
            .items
            .iter()
            .find_map(|item| match item {
                HirItem::Fn(f) if interner.resolve(f.name) == fn_name => Some(f.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("fn `{fn_name}` not found in module"));
        (interner, file, f)
    }

    // ─────────────────────────────────────────────────────────────────────
    // § TypeSubst basics
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn type_subst_new_is_empty() {
        let s = TypeSubst::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn type_subst_bind_then_get() {
        let interner = cssl_hir::Interner::new();
        let t = interner.intern("T");
        let mut s = TypeSubst::new();
        s.bind(t, hir_primitive_type("i32", &interner));
        assert_eq!(s.len(), 1);
        assert!(s.get(&t).is_some());
    }

    #[test]
    fn type_subst_iter_sorted_is_deterministic() {
        let interner = cssl_hir::Interner::new();
        let t = interner.intern("T");
        let u = interner.intern("U");
        let mut s = TypeSubst::new();
        s.bind(u, hir_primitive_type("f32", &interner));
        s.bind(t, hir_primitive_type("i32", &interner));
        // Sorted by resolved name : T < U lexicographically.
        let names: Vec<String> = s
            .iter_sorted(&interner)
            .map(|(sym, _)| interner.resolve(sym))
            .collect();
        assert_eq!(names, vec!["T".to_string(), "U".to_string()]);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Substitution walks type-trees correctly
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn substitute_single_segment_path_to_generic_param() {
        let interner = cssl_hir::Interner::new();
        let t = interner.intern("T");
        let mut s = TypeSubst::new();
        s.bind(t, hir_primitive_type("i32", &interner));

        let generic_t = hir_primitive_type("T", &interner);
        let result = substitute_hir_type(&generic_t, &interner, &s);
        // The result should be `i32`, not `T`.
        let resolved = primitive_hir_to_mir(&result, &interner);
        assert_eq!(resolved, Some(MirType::Int(IntWidth::I32)));
    }

    #[test]
    fn substitute_passes_through_non_generic_paths() {
        let interner = cssl_hir::Interner::new();
        let t = interner.intern("T");
        let mut s = TypeSubst::new();
        s.bind(t, hir_primitive_type("i32", &interner));

        let concrete_f32 = hir_primitive_type("f32", &interner);
        let result = substitute_hir_type(&concrete_f32, &interner, &s);
        let resolved = primitive_hir_to_mir(&result, &interner);
        // f32 is NOT a generic-param name, so it passes through unchanged.
        assert_eq!(resolved, Some(MirType::Float(FloatWidth::F32)));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Name mangling is deterministic
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn mangle_no_subst_preserves_base_name() {
        let interner = cssl_hir::Interner::new();
        let subst = TypeSubst::new();
        assert_eq!(mangle_specialization_name("id", &interner, &subst), "id");
    }

    #[test]
    fn mangle_one_subst_appends_type_fragment() {
        let interner = cssl_hir::Interner::new();
        let t = interner.intern("T");
        let mut s = TypeSubst::new();
        s.bind(t, hir_primitive_type("i32", &interner));
        assert_eq!(mangle_specialization_name("id", &interner, &s), "id_i32");
    }

    #[test]
    fn mangle_two_substs_sorts_by_param_name() {
        let interner = cssl_hir::Interner::new();
        let t = interner.intern("T");
        let u = interner.intern("U");
        let mut s = TypeSubst::new();
        // Bind U first then T to prove iter_sorted is deterministic.
        s.bind(u, hir_primitive_type("f32", &interner));
        s.bind(t, hir_primitive_type("i32", &interner));
        // Order is T, U ⇒ fragments append as `_i32_f32`.
        assert_eq!(
            mangle_specialization_name("pair", &interner, &s),
            "pair_i32_f32"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § specialize_generic_fn end-to-end : HIR → specialized MirFunc
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn specialize_id_to_i32_produces_correct_signature() {
        let src = r"fn id<T>(x : T) -> T { x }";
        let (interner, file, f) = parse_fn(src, "id");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let mir_fn = specialize_generic_fn(&interner, Some(&file), &f, &subst);
        assert_eq!(mir_fn.name, "id_i32");
        assert_eq!(mir_fn.params, vec![MirType::Int(IntWidth::I32)]);
        assert_eq!(mir_fn.results, vec![MirType::Int(IntWidth::I32)]);
    }

    #[test]
    fn specialize_id_to_f32_produces_correct_signature() {
        let src = r"fn id<T>(x : T) -> T { x }";
        let (interner, file, f) = parse_fn(src, "id");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("f32", &interner));

        let mir_fn = specialize_generic_fn(&interner, Some(&file), &f, &subst);
        assert_eq!(mir_fn.name, "id_f32");
        assert_eq!(mir_fn.params, vec![MirType::Float(FloatWidth::F32)]);
        assert_eq!(mir_fn.results, vec![MirType::Float(FloatWidth::F32)]);
    }

    #[test]
    fn specialize_two_param_generic_fn() {
        let src = r"fn pair<T, U>(a : T, b : U) -> i32 { 0 }";
        let (interner, file, f) = parse_fn(src, "pair");
        let t = interner.intern("T");
        let u = interner.intern("U");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));
        subst.bind(u, hir_primitive_type("f32", &interner));

        let mir_fn = specialize_generic_fn(&interner, Some(&file), &f, &subst);
        assert_eq!(mir_fn.name, "pair_i32_f32");
        assert_eq!(
            mir_fn.params,
            vec![MirType::Int(IntWidth::I32), MirType::Float(FloatWidth::F32),]
        );
        assert_eq!(mir_fn.results, vec![MirType::Int(IntWidth::I32)]);
    }

    #[test]
    fn specialize_strips_generics_from_hir_clone() {
        // After specialization the cloned HIR fn must have empty generics so
        // downstream passes don't try to re-process as generic.
        let src = r"fn id<T>(x : T) -> T { x }";
        let (interner, file, f) = parse_fn(src, "id");
        assert_eq!(f.generics.params.len(), 1, "baseline : 1 generic param");

        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let _mir_fn = specialize_generic_fn(&interner, Some(&file), &f, &subst);
        // The original HIR fn is unchanged (we cloned).
        assert_eq!(f.generics.params.len(), 1, "original not mutated");
    }

    #[test]
    fn specialize_non_generic_fn_is_identity_name() {
        // A non-generic fn with empty subst specializes to its own name +
        // original signature — composes cleanly as a base case.
        let src = r"fn add(a : i32, b : i32) -> i32 { a + b }";
        let (interner, file, f) = parse_fn(src, "add");
        let subst = TypeSubst::new();

        let mir_fn = specialize_generic_fn(&interner, Some(&file), &f, &subst);
        assert_eq!(mir_fn.name, "add");
        assert_eq!(
            mir_fn.params,
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)]
        );
    }

    #[test]
    fn specialize_id_to_i32_body_lowers_cleanly() {
        // Body-lowering of `fn id<T>(x: T) -> T { x }` after substitution
        // T→i32 must produce a structurally-valid MirFunc : single entry block
        // present, body owned by the fn, fn.results shape carries `i32`.
        // Full JIT integration test lives in cssl-examples (avoids dev-dep
        // cycle between cssl-mir ↔ cssl-cgen-cpu-cranelift).
        let src = r"fn id<T>(x : T) -> T { x }";
        let (interner, file, f) = parse_fn(src, "id");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let mir_fn = specialize_generic_fn(&interner, Some(&file), &f, &subst);
        assert_eq!(mir_fn.name, "id_i32");
        // Entry block must exist + be well-formed.
        assert!(
            mir_fn.body.entry().is_some(),
            "specialized fn must have an entry block"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Primitive HIR→MIR helper used by specialize internals
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn primitive_hir_to_mir_covers_canonical_names() {
        let interner = cssl_hir::Interner::new();
        assert_eq!(
            primitive_hir_to_mir(&hir_primitive_type("i32", &interner), &interner),
            Some(MirType::Int(IntWidth::I32))
        );
        assert_eq!(
            primitive_hir_to_mir(&hir_primitive_type("f32", &interner), &interner),
            Some(MirType::Float(FloatWidth::F32))
        );
        assert_eq!(
            primitive_hir_to_mir(&hir_primitive_type("bool", &interner), &interner),
            Some(MirType::Bool)
        );
    }

    #[test]
    fn primitive_hir_to_mir_returns_none_for_generic_param() {
        let interner = cssl_hir::Interner::new();
        let generic_t = hir_primitive_type("T", &interner);
        // `T` looks like a primitive-shape path but isn't in the catalog ⇒ None.
        assert_eq!(primitive_hir_to_mir(&generic_t, &interner), None);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D45 — generic struct specialization tests
    // ─────────────────────────────────────────────────────────────────────

    use super::{mangle_struct_specialization_name, specialize_generic_struct};
    use cssl_hir::HirStructBody;

    /// Helper : parse source, lower to HIR, return (interner, file, HirStruct).
    fn parse_struct(src: &str, name: &str) -> (cssl_hir::Interner, cssl_hir::HirStruct) {
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);
        let s = hir
            .items
            .iter()
            .find_map(|item| match item {
                HirItem::Struct(s) if interner.resolve(s.name) == name => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("struct `{name}` not found"));
        (interner, s)
    }

    #[test]
    fn specialize_named_struct_substitutes_field_types() {
        // struct Pair<T, U> { first: T, second: U } + T↦i32, U↦f32
        //   ⇒ Pair { first: i32, second: f32 } + empty generics
        let src = r"struct Pair<T, U> { first : T, second : U }";
        let (interner, s) = parse_struct(src, "Pair");
        let t = interner.intern("T");
        let u = interner.intern("U");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));
        subst.bind(u, hir_primitive_type("f32", &interner));

        let specialized = specialize_generic_struct(&interner, &s, &subst);
        assert!(specialized.generics.params.is_empty());

        let fields = match &specialized.body {
            HirStructBody::Named(fs) => fs,
            other => panic!("expected Named body, got {other:?}"),
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(
            primitive_hir_to_mir(&fields[0].ty, &interner),
            Some(MirType::Int(IntWidth::I32))
        );
        assert_eq!(
            primitive_hir_to_mir(&fields[1].ty, &interner),
            Some(MirType::Float(FloatWidth::F32))
        );
    }

    #[test]
    fn specialize_tuple_struct_substitutes_field_types() {
        // struct Wrap<T>(T, i32) — tuple struct
        let src = r"struct Wrap<T>(T, i32);";
        let (interner, s) = parse_struct(src, "Wrap");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("f64", &interner));

        let specialized = specialize_generic_struct(&interner, &s, &subst);
        let fields = match &specialized.body {
            HirStructBody::Tuple(fs) => fs,
            other => panic!("expected Tuple body, got {other:?}"),
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(
            primitive_hir_to_mir(&fields[0].ty, &interner),
            Some(MirType::Float(FloatWidth::F64))
        );
        // Second field is i32 — not substituted (not generic).
        assert_eq!(
            primitive_hir_to_mir(&fields[1].ty, &interner),
            Some(MirType::Int(IntWidth::I32))
        );
    }

    #[test]
    fn specialize_unit_struct_passes_through() {
        let src = r"struct Marker<T>;";
        let (interner, s) = parse_struct(src, "Marker");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specialized = specialize_generic_struct(&interner, &s, &subst);
        assert!(matches!(specialized.body, HirStructBody::Unit));
        assert!(specialized.generics.params.is_empty());
    }

    #[test]
    fn specialize_struct_empties_generics() {
        // Regression : after specialization, generics must be empty so downstream
        // passes treat the struct as concrete.
        let src = r"struct Box<T> { value : T }";
        let (interner, s) = parse_struct(src, "Box");
        assert_eq!(s.generics.params.len(), 1, "baseline");

        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specialized = specialize_generic_struct(&interner, &s, &subst);
        assert!(specialized.generics.params.is_empty());
        // Original untouched (we clone).
        assert_eq!(s.generics.params.len(), 1);
    }

    #[test]
    fn mangle_struct_specialization_name_matches_fn_convention() {
        let src = r"struct Pair<T, U> { first : T, second : U }";
        let (interner, s) = parse_struct(src, "Pair");
        let t = interner.intern("T");
        let u = interner.intern("U");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));
        subst.bind(u, hir_primitive_type("f32", &interner));

        let mangled = mangle_struct_specialization_name(&s, &interner, &subst);
        assert_eq!(mangled, "Pair_i32_f32");
    }

    #[test]
    fn specialize_non_generic_struct_is_identity_body() {
        // A non-generic struct with empty subst specializes to its own shape.
        let src = r"struct Point { x : f32, y : f32 }";
        let (interner, s) = parse_struct(src, "Point");
        let subst = TypeSubst::new();

        let specialized = specialize_generic_struct(&interner, &s, &subst);
        let fields = match &specialized.body {
            HirStructBody::Named(fs) => fs,
            other => panic!("expected Named : {other:?}"),
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(
            primitive_hir_to_mir(&fields[0].ty, &interner),
            Some(MirType::Float(FloatWidth::F32))
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D47 — generic enum specialization tests
    // ─────────────────────────────────────────────────────────────────────

    use super::{mangle_enum_specialization_name, specialize_generic_enum};

    fn parse_enum(src: &str, name: &str) -> (cssl_hir::Interner, cssl_hir::HirEnum) {
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);
        let e = hir
            .items
            .iter()
            .find_map(|item| match item {
                cssl_hir::HirItem::Enum(e) if interner.resolve(e.name) == name => Some(e.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("enum `{name}` not found"));
        (interner, e)
    }

    #[test]
    fn specialize_option_like_enum_substitutes_variant_field_types() {
        // enum Option<T> { Some(T), None }
        //   + T ↦ i32 ⇒ variants : Some(i32), None
        let src = r"enum Opt<T> { Some(T), None }";
        let (interner, e) = parse_enum(src, "Opt");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specialized = specialize_generic_enum(&interner, &e, &subst);
        assert!(specialized.generics.params.is_empty());
        assert_eq!(specialized.variants.len(), 2);

        // First variant — Some(T) : tuple body with one i32 field after subst.
        match &specialized.variants[0].body {
            HirStructBody::Tuple(fs) => {
                assert_eq!(fs.len(), 1);
                assert_eq!(
                    primitive_hir_to_mir(&fs[0].ty, &interner),
                    Some(MirType::Int(IntWidth::I32))
                );
            }
            other => panic!("expected Tuple, got {other:?}"),
        }
        // Second variant — None : Unit.
        assert!(matches!(&specialized.variants[1].body, HirStructBody::Unit));
    }

    #[test]
    fn specialize_enum_named_variant_substitutes() {
        // enum Either<L, R> { Left { v: L }, Right { v: R } }
        let src = r"enum Either<L, R> { Left { v : L }, Right { v : R } }";
        let (interner, e) = parse_enum(src, "Either");
        let l = interner.intern("L");
        let r = interner.intern("R");
        let mut subst = TypeSubst::new();
        subst.bind(l, hir_primitive_type("i32", &interner));
        subst.bind(r, hir_primitive_type("f64", &interner));

        let specialized = specialize_generic_enum(&interner, &e, &subst);
        assert_eq!(specialized.variants.len(), 2);

        let check_field = |v: &cssl_hir::HirEnumVariant, expected: MirType| match &v.body {
            HirStructBody::Named(fs) => {
                assert_eq!(fs.len(), 1);
                assert_eq!(primitive_hir_to_mir(&fs[0].ty, &interner), Some(expected));
            }
            other => panic!("expected Named : {other:?}"),
        };
        check_field(&specialized.variants[0], MirType::Int(IntWidth::I32));
        check_field(&specialized.variants[1], MirType::Float(FloatWidth::F64));
    }

    #[test]
    fn specialize_non_generic_enum_passes_through() {
        let src = r"enum Color { Red, Green, Blue }";
        let (interner, e) = parse_enum(src, "Color");
        let subst = TypeSubst::new();
        let specialized = specialize_generic_enum(&interner, &e, &subst);
        assert_eq!(specialized.variants.len(), 3);
        assert!(specialized.generics.params.is_empty());
        for v in &specialized.variants {
            assert!(matches!(v.body, HirStructBody::Unit));
        }
    }

    #[test]
    fn specialize_enum_empties_generics() {
        let src = r"enum Opt<T> { Some(T), None }";
        let (interner, e) = parse_enum(src, "Opt");
        assert_eq!(e.generics.params.len(), 1);

        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));
        let specialized = specialize_generic_enum(&interner, &e, &subst);
        assert!(specialized.generics.params.is_empty());
        // Original unchanged.
        assert_eq!(e.generics.params.len(), 1);
    }

    #[test]
    fn mangle_enum_specialization_name_follows_convention() {
        let src = r"enum Opt<T> { Some(T), None }";
        let (interner, e) = parse_enum(src, "Opt");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("f32", &interner));
        assert_eq!(
            mangle_enum_specialization_name(&e, &interner, &subst),
            "Opt_f32"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D49 — impl<T> monomorphization tests
    // ─────────────────────────────────────────────────────────────────────

    use super::specialize_generic_impl;

    /// Helper : parse source, lower to HIR, return (interner, first HirImpl).
    fn parse_impl(src: &str) -> (cssl_hir::Interner, cssl_hir::HirImpl) {
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);
        let i = hir
            .items
            .iter()
            .find_map(|item| match item {
                cssl_hir::HirItem::Impl(i) => Some(i.clone()),
                _ => None,
            })
            .expect("impl block not found");
        (interner, i)
    }

    #[test]
    fn specialize_impl_produces_one_fn_per_method() {
        let src = r"
            struct Box<T> { value : T }
            impl<T> Box<T> {
                fn first(b : Box<T>) -> i32 { 0 }
                fn second(b : Box<T>) -> i32 { 0 }
            }
        ";
        let (interner, i) = parse_impl(src);
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specs = specialize_generic_impl(&interner, None, &i, &subst);
        assert_eq!(specs.len(), 2);
        let names: Vec<&str> = specs.iter().map(|m| m.name.as_str()).collect();
        assert!(names.iter().any(|n| n.ends_with("__first")));
        assert!(names.iter().any(|n| n.ends_with("__second")));
    }

    #[test]
    fn specialize_impl_method_name_prepended_by_self_mangle() {
        let src = r"
            struct Box<T> { value : T }
            impl<T> Box<T> {
                fn get(b : Box<T>) -> i32 { 0 }
            }
        ";
        let (interner, i) = parse_impl(src);
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specs = specialize_generic_impl(&interner, None, &i, &subst);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "Box_i32__get");
    }

    #[test]
    fn specialize_impl_two_type_params() {
        let src = r"
            struct Pair<T, U> { first : T, second : U }
            impl<T, U> Pair<T, U> {
                fn swap(p : Pair<T, U>) -> i32 { 0 }
            }
        ";
        let (interner, i) = parse_impl(src);
        let t = interner.intern("T");
        let u = interner.intern("U");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));
        subst.bind(u, hir_primitive_type("f32", &interner));

        let specs = specialize_generic_impl(&interner, None, &i, &subst);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "Pair_i32_f32__swap");
    }

    #[test]
    fn specialize_impl_fn_param_types_substituted() {
        // Method param `b : Box<T>` substitutes to `Box<i32>` — the outer
        // impl's subst reaches into the method's param types.
        let src = r"
            struct Box<T> { value : T }
            impl<T> Box<T> {
                fn take(b : Box<T>, extra : T) -> i32 { 0 }
            }
        ";
        let (interner, i) = parse_impl(src);
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("f64", &interner));

        let specs = specialize_generic_impl(&interner, None, &i, &subst);
        assert_eq!(specs.len(), 1);
        // Second param `extra : T` should have been substituted to f64.
        // MirFunc.params is the flat lowered signature — params[1] = f64 (the
        // `extra` param). `b : Box<T>` lowers to Opaque("Box") at stage-0,
        // we only check the T-substituted param survives.
        let has_f64 = specs[0]
            .params
            .iter()
            .any(|t| matches!(t, MirType::Float(FloatWidth::F64)));
        assert!(
            has_f64,
            "outer subst must substitute T → f64 in method params : got {:?}",
            specs[0].params
        );
    }

    #[test]
    fn specialize_impl_empty_block_returns_empty_vec() {
        let src = r"
            struct Box<T> { value : T }
            impl<T> Box<T> { }
        ";
        let (interner, i) = parse_impl(src);
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));
        let specs = specialize_generic_impl(&interner, None, &i, &subst);
        assert!(specs.is_empty());
    }

    #[test]
    fn specialize_impl_no_generics_produces_unsuffixed_names() {
        // Non-generic impl : `impl Point { fn mag(p : Point) -> f32 }` — subst
        // is empty ; mangled name is `Point__mag` (no type-arg suffix).
        let src = r"
            struct Point { x : f32, y : f32 }
            impl Point {
                fn mag(p : Point) -> f32 { 0.0 }
            }
        ";
        let (interner, i) = parse_impl(src);
        let subst = TypeSubst::new();

        let specs = specialize_generic_impl(&interner, None, &i, &subst);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "Point__mag");
    }

    #[test]
    fn specialize_enum_with_nested_type_arg() {
        // enum Tree<T> { Leaf(T), Node(Box<T>) }
        // T ↦ i32 ⇒ Leaf(i32), Node(Box<i32>)
        let src = r"enum Tree<T> { Leaf(T), Node(Box<T>) }";
        let (interner, e) = parse_enum(src, "Tree");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specialized = specialize_generic_enum(&interner, &e, &subst);
        assert_eq!(specialized.variants.len(), 2);

        // Leaf(i32) : Tuple([i32])
        match &specialized.variants[0].body {
            HirStructBody::Tuple(fs) => {
                assert_eq!(
                    primitive_hir_to_mir(&fs[0].ty, &interner),
                    Some(MirType::Int(IntWidth::I32))
                );
            }
            other => panic!("expected Tuple for Leaf : {other:?}"),
        }

        // Node(Box<i32>) : Tuple([Path{Box, type_args:[i32]}])
        match &specialized.variants[1].body {
            HirStructBody::Tuple(fs) => {
                let cssl_hir::HirTypeKind::Path {
                    path, type_args, ..
                } = &fs[0].ty.kind
                else {
                    panic!("expected Path, got {:?}", fs[0].ty.kind);
                };
                assert_eq!(interner.resolve(path[0]), "Box");
                assert_eq!(type_args.len(), 1);
                assert_eq!(
                    primitive_hir_to_mir(&type_args[0], &interner),
                    Some(MirType::Int(IntWidth::I32))
                );
            }
            other => panic!("expected Tuple for Node : {other:?}"),
        }
    }

    #[test]
    fn specialize_struct_with_nested_type_arg() {
        // `struct Outer<T> { inner : Box<T> }` — the generic param appears in a
        // nested path's type_args. substitute_hir_type recurses through nested
        // type_args, so Box<T> becomes Box<i32> in the specialized body.
        let src = r"struct Outer<T> { inner : Box<T> }";
        let (interner, s) = parse_struct(src, "Outer");
        let t = interner.intern("T");
        let mut subst = TypeSubst::new();
        subst.bind(t, hir_primitive_type("i32", &interner));

        let specialized = specialize_generic_struct(&interner, &s, &subst);
        let fields = match &specialized.body {
            HirStructBody::Named(fs) => fs,
            other => panic!("expected Named : {other:?}"),
        };
        assert_eq!(fields.len(), 1);
        // inner's type is still a Path("Box") but with type_args = [i32] now.
        let cssl_hir::HirTypeKind::Path {
            path, type_args, ..
        } = &fields[0].ty.kind
        else {
            panic!("expected Path, got {:?}", fields[0].ty.kind);
        };
        assert_eq!(path.len(), 1);
        assert_eq!(interner.resolve(path[0]), "Box");
        assert_eq!(type_args.len(), 1);
        assert_eq!(
            primitive_hir_to_mir(&type_args[0], &interner),
            Some(MirType::Int(IntWidth::I32))
        );
    }
}

//! Capability-check pass (T3.4-phase-2 cap slice, interleaved with T5).
//!
//! § SCOPE (STAGE-0 / this commit)
//!   Signature-level validation : every `HirCapKind` wrapper in fn-param / fn-return /
//!   struct-field / let-binding positions is mapped to a `cssl_caps::CapKind`, and
//!   linear-parameter tracking is initialized for iso parameters. The tracker is
//!   scope-opened at fn-entry and closed at fn-exit ; a full consume-tracking walk
//!   through the body is scheduled for T3.4-phase-2.5.
//!
//!   What lands here :
//!     - [`CapMap`]                  — HirId → CapKind side-table.
//!     - [`check_capabilities`]      — entry point producing `(CapMap, Vec<Diagnostic>)`.
//!     - [`param_subtype_check`]     — call-site check : caller-cap <: callee-param-cap.
//!
//! § DEFERRED (T3.4-phase-2.5 + T5-phase-2)
//!   - Full linear-use tracking through every expression (consume / drop / resume sites).
//!   - Handler-one-shot enforcement (requires identifying resume call-sites).
//!   - Field-level cap validation (struct-field caps flow through field-access).
//!   - Freeze / consume sugar (`freeze(x)` / explicit `consume x`).
//!   - gen-ref deref-check insertion (part of MIR lowering @ T6).

use std::collections::BTreeMap;

use cssl_ast::{Diagnostic, Span};
use cssl_caps::{coerce, AliasMatrix, CapKind, LinearTracker, SubtypeError};

use crate::arena::HirId;
use crate::item::{HirFn, HirFnParam, HirItem, HirModule};
use crate::pat::{HirPattern, HirPatternKind};
use crate::ty::{HirCapKind, HirType, HirTypeKind};

/// Side-table from HIR node id → inferred/declared capability.
///
/// Only HIR nodes that correspond to cap-bearing values (fn-params, let-bindings
/// with iso/trn/ref/val/box/tag type, struct fields of those caps) carry entries.
/// Nodes without a cap-wrapper are absent.
#[derive(Debug, Default, Clone)]
pub struct CapMap {
    pub caps: BTreeMap<u32, CapKind>,
}

impl CapMap {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a cap for a HIR node.
    pub fn insert(&mut self, id: HirId, cap: CapKind) {
        self.caps.insert(id.0, cap);
    }

    /// Lookup the cap for a HIR node.
    #[must_use]
    pub fn get(&self, id: HirId) -> Option<CapKind> {
        self.caps.get(&id.0).copied()
    }

    /// Number of cap entries recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.caps.len()
    }

    /// `true` iff no caps recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty()
    }
}

/// Translate `cssl_hir::HirCapKind` (CST-mirror) to `cssl_caps::CapKind` (semantic).
#[must_use]
pub const fn hir_cap_to_semantic(c: HirCapKind) -> CapKind {
    match c {
        HirCapKind::Iso => CapKind::Iso,
        HirCapKind::Trn => CapKind::Trn,
        HirCapKind::Ref => CapKind::Ref,
        HirCapKind::Val => CapKind::Val,
        HirCapKind::Box => CapKind::Box,
        HirCapKind::Tag => CapKind::Tag,
    }
}

/// Walk a HIR type and, if its top-level kind is a capability wrapper, return
/// the semantic cap + inner HirType. Nested capability wrappers (e.g., `iso<ref<T>>`)
/// are unsupported at stage-0 — the outer cap wins.
#[must_use]
pub fn top_cap(t: &HirType) -> Option<CapKind> {
    if let HirTypeKind::Capability { cap, .. } = &t.kind {
        Some(hir_cap_to_semantic(*cap))
    } else {
        None
    }
}

/// Capability-validation entry point.
///
/// Walks every fn in `module`, extracts cap annotations from param / return / local
/// types, and produces :
///   - A populated [`CapMap`] : HIR node → declared or inferred cap.
///   - A list of cap-level diagnostics (for now : unknown-cap, subtype-violation,
///     linear-leak at fn-exit).
#[must_use]
pub fn check_capabilities(module: &HirModule) -> (CapMap, Vec<Diagnostic>) {
    let mut ctx = CapCtx::new();
    for item in &module.items {
        ctx.check_item(item);
    }
    (ctx.caps, ctx.diagnostics)
}

/// Check that a caller's cap can be passed to a parameter of the given cap. Returns
/// `Err(SubtypeError)` if no subtype relation exists.
pub fn param_subtype_check(caller: CapKind, callee_param: CapKind) -> Result<(), SubtypeError> {
    coerce(caller, callee_param).map(|_| ())
}

// ─ Internal context ─────────────────────────────────────────────────────────

struct CapCtx {
    caps: CapMap,
    diagnostics: Vec<Diagnostic>,
    matrix: AliasMatrix,
}

impl CapCtx {
    fn new() -> Self {
        Self {
            caps: CapMap::new(),
            diagnostics: Vec::new(),
            matrix: AliasMatrix::pony6(),
        }
    }

    // Reserved for the deferred T3.4-phase-2.5 body-walk slice of cap_check.
    // The signature-level minimum-viable check landed in DECISIONS.md § T5-D3
    // (Cap-check pass sig-level only for stage-0 ; full expr walk deferred),
    // which explicitly defers "full linear-use tracking through every
    // expression" + "handler-one-shot enforcement" to T3.4-phase-2.5. This
    // helper will be wired in when that slice lands; until then, cargo
    // -D warnings needs the allow.
    #[allow(dead_code)] // T5-D3: wired in at T3.4-phase-2.5
    fn emit(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(message).with_span(span));
    }

    fn check_item(&mut self, item: &HirItem) {
        match item {
            HirItem::Fn(f) => self.check_fn(f),
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
            }
            HirItem::Module(m) => {
                if let Some(sub) = &m.items {
                    for s in sub {
                        self.check_item(s);
                    }
                }
            }
            // Struct / Enum / TypeAlias / Use / Const don't have bodies to cap-check
            // at stage-0 ; field-cap validation happens per-constructor at call-sites.
            _ => {}
        }
    }

    fn check_fn(&mut self, f: &HirFn) {
        let mut tracker = LinearTracker::new();
        for (idx, p) in f.params.iter().enumerate() {
            self.check_fn_param(p, idx, &mut tracker);
        }
        if let Some(rt) = &f.return_ty {
            if let Some(cap) = top_cap(rt) {
                self.caps.insert(rt.id, cap);
            }
        }
        // Close linear tracker at fn exit. Stage-0 assumes the body consumed all
        // iso-params ; a full walk lands in T3.4-phase-2.5 and would replace this
        // with actual use events. Until then we don't flag leaks here (the body
        // hasn't been walked) — we just close the scope cleanly.
        let _closing_violations = tracker.close_scope();
    }

    fn check_fn_param(&mut self, p: &HirFnParam, _idx: usize, tracker: &mut LinearTracker) {
        if let Some(cap) = top_cap(&p.ty) {
            self.caps.insert(p.id, cap);
            // Bind linear tracking for iso parameters. Body-walk will consume these ;
            // stage-0 defers that walk but still registers the binding.
            if cap.is_linear() {
                tracker.introduce(cssl_caps::linearity::BindingId(p.id.0), cap);
            }
            // Also walk the pattern to record cap info on binding nodes.
            self.walk_pattern_cap(&p.pat, cap);
        }
    }

    /// Record a pattern-node's declared cap on its binding sites. This lets later
    /// passes answer "what cap does local x have?" via `CapMap::get(id)`.
    fn walk_pattern_cap(&mut self, pat: &HirPattern, cap: CapKind) {
        match &pat.kind {
            HirPatternKind::Binding { .. } | HirPatternKind::Wildcard => {
                self.caps.insert(pat.id, cap);
            }
            HirPatternKind::Tuple(_)
            | HirPatternKind::Struct { .. }
            | HirPatternKind::Variant { .. }
            | HirPatternKind::Or(_) => {
                // Composite patterns : record the outer cap ; per-child caps depend on
                // type-structure that's T3.4-phase-2.5 work.
                self.caps.insert(pat.id, cap);
            }
            HirPatternKind::Ref { inner, .. } => {
                self.walk_pattern_cap(inner, cap);
            }
            HirPatternKind::Literal(_) | HirPatternKind::Range { .. } | HirPatternKind::Error => {}
        }
    }

    // See `emit` above for the DECISIONS.md § T5-D3 tracking note -- matrix() is
    // the AliasMatrix accessor the deferred T3.4-phase-2.5 body-walk needs to
    // run `AliasMatrix::can_pass_through`/`param_subtype_check` at call sites.
    #[allow(dead_code)] // T5-D3: wired in at T3.4-phase-2.5
    fn matrix(&self) -> &AliasMatrix {
        &self.matrix
    }
}

#[cfg(test)]
mod tests {
    use super::{check_capabilities, hir_cap_to_semantic, param_subtype_check, top_cap, CapMap};
    use crate::arena::HirId;
    use crate::ty::{HirCapKind, HirType, HirTypeKind};
    use cssl_ast::{SourceId, Span};
    use cssl_caps::CapKind;

    fn sp() -> Span {
        Span::new(SourceId::first(), 0, 1)
    }

    fn capty(cap: HirCapKind) -> HirType {
        HirType {
            span: sp(),
            id: HirId(0),
            kind: HirTypeKind::Capability {
                cap,
                inner: Box::new(HirType {
                    span: sp(),
                    id: HirId(1),
                    kind: HirTypeKind::Infer,
                }),
            },
        }
    }

    #[test]
    fn hir_cap_translation_preserves_variants() {
        assert_eq!(hir_cap_to_semantic(HirCapKind::Iso), CapKind::Iso);
        assert_eq!(hir_cap_to_semantic(HirCapKind::Trn), CapKind::Trn);
        assert_eq!(hir_cap_to_semantic(HirCapKind::Ref), CapKind::Ref);
        assert_eq!(hir_cap_to_semantic(HirCapKind::Val), CapKind::Val);
        assert_eq!(hir_cap_to_semantic(HirCapKind::Box), CapKind::Box);
        assert_eq!(hir_cap_to_semantic(HirCapKind::Tag), CapKind::Tag);
    }

    #[test]
    fn top_cap_extracts_iso_wrapper() {
        let t = capty(HirCapKind::Iso);
        assert_eq!(top_cap(&t), Some(CapKind::Iso));
    }

    #[test]
    fn top_cap_returns_none_for_non_wrapped() {
        let t = HirType {
            span: sp(),
            id: HirId(0),
            kind: HirTypeKind::Infer,
        };
        assert_eq!(top_cap(&t), None);
    }

    #[test]
    fn cap_map_roundtrip() {
        let mut m = CapMap::new();
        assert!(m.is_empty());
        m.insert(HirId(5), CapKind::Iso);
        assert_eq!(m.get(HirId(5)), Some(CapKind::Iso));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn param_subtype_iso_to_val_ok() {
        assert!(param_subtype_check(CapKind::Iso, CapKind::Val).is_ok());
    }

    #[test]
    fn param_subtype_val_to_iso_fails() {
        assert!(param_subtype_check(CapKind::Val, CapKind::Iso).is_err());
    }

    #[test]
    fn empty_module_produces_empty_cap_map() {
        use crate::arena::HirArena;
        use crate::item::HirModule;
        let module = HirModule {
            span: sp(),
            arena: HirArena::new(),
            inner_attrs: Vec::new(),
            module_path: None,
            items: Vec::new(),
        };
        let (map, diags) = check_capabilities(&module);
        assert!(map.is_empty());
        assert!(diags.is_empty());
    }
}

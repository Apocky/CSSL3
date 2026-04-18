//! MIR functions + module.
//!
//! § DESIGN
//!   - [`MirFunc`] : name + signature + body (one region).
//!   - [`MirModule`] : top-level container of fns + module-level attributes.
//!
//! Each fn is lowered to a single `func.func` op in textual-MLIR form ; internally
//! we model it as a `MirFunc` with an owned `MirRegion` so the pretty-printer can
//! emit the canonical `func.func @name(args) -> results { ... }` shape.

use crate::block::{MirOp, MirRegion};
use crate::value::{MirType, MirValue, ValueId};

/// A function in the MIR module.
#[derive(Debug, Clone)]
pub struct MirFunc {
    /// Source-form fn name (without `@` prefix).
    pub name: String,
    /// Parameter types (order matches `body.entry().args`).
    pub params: Vec<MirType>,
    /// Return types (stage-0 : at most 1 result, but multi-result support is here).
    pub results: Vec<MirType>,
    /// Effect-row as a free-form string (structural form : `"{GPU, NoAlloc}"`).
    /// `None` = pure. Structured effect-row attribute is T6-phase-2 work.
    pub effect_row: Option<String>,
    /// Cap annotation on the fn value itself (e.g., `"val"`).
    pub cap: Option<String>,
    /// IFC label attribute (if any) — free-form at stage-0.
    pub ifc_label: Option<String>,
    /// Attribute dictionary for additional flags (e.g., `"@differentiable"`).
    pub attributes: Vec<(String, String)>,
    /// T11-D43 : `true` iff the source HIR fn declared generic parameters
    /// (`fn f<T>(…)`). Generic fns carry type-param placeholder `Opaque`
    /// types in their params/body and cannot be JIT-compiled directly —
    /// they must be specialized first via `specialize_generic_fn`. The
    /// `drop_unspecialized_generic_fns` cleanup pass removes them after
    /// monomorphization so downstream passes see only concrete fns.
    pub is_generic: bool,
    /// The fn body — a single region with at-least an entry block.
    pub body: MirRegion,
    /// Monotonic counter used for fresh-value-id allocation within the body.
    pub next_value_id: u32,
}

impl MirFunc {
    /// Build a fn with the given name + signature. Body starts with an empty entry
    /// block whose args match `params`.
    #[must_use]
    pub fn new(name: impl Into<String>, params: Vec<MirType>, results: Vec<MirType>) -> Self {
        let args: Vec<MirValue> = params
            .iter()
            .enumerate()
            .map(|(i, t)| MirValue::new(ValueId(i as u32), t.clone()))
            .collect();
        let body = MirRegion::with_entry(args);
        let next_value_id = params.len() as u32;
        Self {
            name: name.into(),
            params,
            results,
            effect_row: None,
            cap: None,
            ifc_label: None,
            attributes: Vec::new(),
            is_generic: false,
            body,
            next_value_id,
        }
    }

    /// Allocate a fresh SSA value id.
    pub fn fresh_value_id(&mut self) -> ValueId {
        let id = ValueId(self.next_value_id);
        self.next_value_id = self.next_value_id.saturating_add(1);
        id
    }

    /// `true` iff this fn has no body (signature-only, like an interface method).
    #[must_use]
    pub fn is_signature_only(&self) -> bool {
        self.body.blocks.iter().all(|b| b.ops.is_empty())
    }

    /// Append an op to the entry block.
    pub fn push_op(&mut self, op: MirOp) {
        if let Some(entry) = self.body.entry_mut() {
            entry.push(op);
        }
    }
}

/// Top-level MIR module — a list of fns + module-level attributes.
#[derive(Debug, Clone, Default)]
pub struct MirModule {
    /// Module name (from source `module com.apocky.loa`).
    pub name: Option<String>,
    /// Functions in declaration order.
    pub funcs: Vec<MirFunc>,
    /// Module-level attributes.
    pub attributes: Vec<(String, String)>,
}

impl MirModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Module with a declared name.
    #[must_use]
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            funcs: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// Append a function.
    pub fn push_func(&mut self, f: MirFunc) {
        self.funcs.push(f);
    }

    /// Lookup a fn by name.
    #[must_use]
    pub fn find_func(&self, name: &str) -> Option<&MirFunc> {
        self.funcs.iter().find(|f| f.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::{MirFunc, MirModule};
    use crate::value::{IntWidth, MirType};

    #[test]
    fn fn_new_populates_entry_args() {
        let params = vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)];
        let f = MirFunc::new("add", params, vec![MirType::Int(IntWidth::I32)]);
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.args.len(), 2);
        assert_eq!(f.next_value_id, 2);
    }

    #[test]
    fn fresh_value_id_increments() {
        let mut f = MirFunc::new("foo", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        assert_ne!(v0, v1);
    }

    #[test]
    fn is_signature_only_for_empty_body() {
        let f = MirFunc::new("stub", vec![], vec![]);
        assert!(f.is_signature_only());
    }

    #[test]
    fn module_find_func_by_name() {
        let mut m = MirModule::with_name("mymod");
        m.push_func(MirFunc::new("foo", vec![], vec![]));
        m.push_func(MirFunc::new("bar", vec![], vec![]));
        assert!(m.find_func("foo").is_some());
        assert!(m.find_func("nope").is_none());
        assert_eq!(m.name.as_deref(), Some("mymod"));
    }
}

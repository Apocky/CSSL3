//! Call-site dispatch for AD : map a primal callee → its `_fwd` / `_bwd`
//! variant + marshal dual-args into the variant's signature.
//!
//! § SPEC : `specs/05_AUTODIFF.csl § IMPLEMENTATION`
//!   "Call(g, args)  ⇒  call g_fwd (or g_bwd) with dual-args"
//!
//! § SHAPE
//!   The variant signature shape (synthesized by [`crate::substitute`]) is :
//!     primal : fn g(a : f32, b : f32) -> f32
//!     fwd    : fn g_fwd(a : f32, d_a : f32, b : f32, d_b : f32) -> (f32, f32)
//!     bwd    : fn g_bwd(a : f32, b : f32, d_y : f32) -> (f32, f32)
//!
//!   Forward-mode marshalling for `g(a, b)` :
//!     - Emit a func.call to `g_fwd` with operands [a, d_a, b, d_b]
//!     - The 2 results are (y, d_y) ; the existing primal-result keeps its
//!       value-id, and a fresh value-id is allocated for the tangent so the
//!       outer fwd-pass can thread d_y forward.
//!
//!   Reverse-mode marshalling for `g(a, b)` (where bwd-pass is walking ops in
//!   reverse and `d_y` is the seeded adjoint of the call result) :
//!     - Emit a func.call to `g_bwd` with operands [a, b, d_y]
//!     - The N results are (d_a, d_b, ...) — one adjoint per primal float-
//!       param. The walker accumulates these into the existing TangentMap
//!       entries for a / b.
//!
//! § AUTO-BUILD
//!   The walker (`AdWalker`) builds a [`CalleeVariantTable`] populated with one
//!   entry per `@differentiable` fn discovered from the HIR module — each
//!   entry maps `name` → (`name_fwd`, `name_bwd`). At call-site lowering, the
//!   substitute walker consults the table to choose the right variant ; if a
//!   callee isn't in the table, the walker falls back to the stage-0 placeholder
//!   so AD-through-non-differentiable-call doesn't silently corrupt gradients.

use std::collections::HashMap;

/// One callee → variants entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalleeVariants {
    /// The fwd-variant fn name (typically `<primal>_fwd`).
    pub fwd: String,
    /// The bwd-variant fn name (typically `<primal>_bwd`).
    pub bwd: String,
}

impl CalleeVariants {
    /// Build the canonical pair `(<name>_fwd, <name>_bwd)` for a primal name.
    #[must_use]
    pub fn canonical(primal: &str) -> Self {
        Self {
            fwd: format!("{primal}_fwd"),
            bwd: format!("{primal}_bwd"),
        }
    }
}

/// Lookup table for AD-aware call-dispatch.
#[derive(Debug, Clone, Default)]
pub struct CalleeVariantTable {
    inner: HashMap<String, CalleeVariants>,
}

impl CalleeVariantTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a table from a list of `@differentiable` fn names — auto-generates
    /// the canonical `_fwd` / `_bwd` variant names for each.
    pub fn from_diff_fn_names<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut t = Self::new();
        for name in names {
            let n = name.into();
            t.insert(n.clone(), CalleeVariants::canonical(&n));
        }
        t
    }

    /// Insert / overwrite a callee → variants entry.
    pub fn insert(&mut self, primal: impl Into<String>, variants: CalleeVariants) {
        self.inner.insert(primal.into(), variants);
    }

    /// Lookup the variants for a callee name. Returns `None` for non-
    /// differentiable callees ; callers must fall back to the placeholder
    /// emission path in that case.
    #[must_use]
    pub fn lookup(&self, callee: &str) -> Option<&CalleeVariants> {
        self.inner.get(callee)
    }

    /// `true` iff a callee is in the table.
    #[must_use]
    pub fn contains(&self, callee: &str) -> bool {
        self.inner.contains_key(callee)
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` iff no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over (primal-name, variants).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CalleeVariants)> {
        self.inner.iter()
    }
}

/// Marshalling result : the operand list to pass to a `_fwd` callee +
/// freshly-allocated value-ids that the caller must reserve in its SSA-counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FwdCallMarshal {
    /// Operand value-ids in `[a, d_a, b, d_b, ...]` interleaved order.
    pub operands: Vec<u32>,
    /// Result value-ids in `[y, d_y]` order ; the caller wires these into the
    /// emitted `func.call` op.
    pub results: Vec<u32>,
}

/// Marshalling result : the operand list to pass to a `_bwd` callee.
///
/// § OPERAND SHAPE :  `[primal_args..., d_y]`  — primal args first, then the
/// seeded adjoint of the call result. The bwd-callee returns one adjoint per
/// primal float-param.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BwdCallMarshal {
    /// Operand value-ids in `[a, b, ..., d_y]` order.
    pub operands: Vec<u32>,
    /// Result value-ids — one adjoint per primal float-param.
    pub results: Vec<u32>,
}

/// Build the operand-list for a fwd-mode call to `_fwd` callee.
///
/// `primal_args` are the value-ids already passed to the primal call ;
/// `tangent_lookup` is consulted for each to fetch the tangent ; missing
/// entries fall back to the primal value-id (zero-tangent encoding compatible
/// with [`crate::substitute::tangent_or_zero`]).
///
/// `next_id` is bumped by 2 — one for the primal-result value-id, one for the
/// tangent. The returned [`FwdCallMarshal`] carries both.
///
/// The interleaved `[a, d_a, b, d_b, ...]` shape matches the param-synthesis
/// in [`crate::substitute::synthesize_tangent_params`] under `DiffMode::Fwd`.
pub fn marshal_fwd_call_operands(
    primal_args: &[u32],
    tangent_lookup: impl Fn(u32) -> Option<u32>,
    next_id: &mut u32,
) -> FwdCallMarshal {
    let mut operands = Vec::with_capacity(primal_args.len() * 2);
    for &arg in primal_args {
        operands.push(arg);
        // Use the tangent if we have one ; otherwise re-use the primal id as
        // the implicit-zero encoding (matches `tangent_or_zero` semantics).
        let tangent = tangent_lookup(arg).unwrap_or(arg);
        operands.push(tangent);
    }
    let primal_result = *next_id;
    *next_id = next_id.saturating_add(1);
    let tangent_result = *next_id;
    *next_id = next_id.saturating_add(1);
    FwdCallMarshal {
        operands,
        results: vec![primal_result, tangent_result],
    }
}

/// Build the operand-list for a bwd-mode call to `_bwd` callee.
///
/// `primal_args` are the value-ids of the primal call's args (used as-is in
/// bwd-mode — the bwd-callee receives them verbatim and re-runs whatever
/// primal-recompute it needs internally). `d_y` is the seeded adjoint of the
/// call result (typically already in the outer-fn TangentMap).
///
/// `n_float_args` is the number of float-typed primal args ; the bwd-callee
/// returns that many adjoints (one per float-param). `next_id` is bumped by
/// `n_float_args` to allocate the result value-ids.
pub fn marshal_bwd_call_operands(
    primal_args: &[u32],
    d_y: u32,
    n_float_args: usize,
    next_id: &mut u32,
) -> BwdCallMarshal {
    let mut operands: Vec<u32> = primal_args.to_vec();
    operands.push(d_y);
    let mut results = Vec::with_capacity(n_float_args);
    for _ in 0..n_float_args {
        results.push(*next_id);
        *next_id = next_id.saturating_add(1);
    }
    BwdCallMarshal { operands, results }
}

#[cfg(test)]
mod tests {
    use super::{
        marshal_bwd_call_operands, marshal_fwd_call_operands, CalleeVariantTable, CalleeVariants,
    };
    use std::collections::HashMap;

    #[test]
    fn canonical_variants_appends_fwd_bwd() {
        let v = CalleeVariants::canonical("scene_sdf");
        assert_eq!(v.fwd, "scene_sdf_fwd");
        assert_eq!(v.bwd, "scene_sdf_bwd");
    }

    #[test]
    fn empty_table_lookup_returns_none() {
        let t = CalleeVariantTable::new();
        assert!(t.lookup("anything").is_none());
        assert!(t.is_empty());
    }

    #[test]
    fn from_diff_fn_names_populates_canonical_pairs() {
        let t = CalleeVariantTable::from_diff_fn_names(["a", "b", "c"]);
        assert_eq!(t.len(), 3);
        assert_eq!(t.lookup("a").unwrap().fwd, "a_fwd");
        assert_eq!(t.lookup("b").unwrap().bwd, "b_bwd");
        assert!(t.contains("c"));
        assert!(!t.contains("d"));
    }

    #[test]
    fn insert_overwrites_existing_entry() {
        let mut t = CalleeVariantTable::new();
        t.insert("g", CalleeVariants::canonical("g"));
        t.insert(
            "g",
            CalleeVariants {
                fwd: "g_custom_fwd".into(),
                bwd: "g_custom_bwd".into(),
            },
        );
        assert_eq!(t.lookup("g").unwrap().fwd, "g_custom_fwd");
    }

    #[test]
    fn marshal_fwd_call_interleaves_primal_and_tangent() {
        // Primal call : g(%0, %1) where TangentMap : {0 → 100, 1 → 101}
        let mut tmap = HashMap::new();
        tmap.insert(0u32, 100u32);
        tmap.insert(1u32, 101u32);
        let mut next_id = 200;
        let m = marshal_fwd_call_operands(&[0, 1], |v| tmap.get(&v).copied(), &mut next_id);
        // Operands : [0, 100, 1, 101]
        assert_eq!(m.operands, vec![0, 100, 1, 101]);
        // Results : [200, 201]
        assert_eq!(m.results, vec![200, 201]);
        assert_eq!(next_id, 202);
    }

    #[test]
    fn marshal_fwd_call_falls_back_to_zero_for_missing_tangent() {
        // No tangent for arg 0 → falls back to using primal id (zero-encoding).
        let mut next_id = 50;
        let m = marshal_fwd_call_operands(&[7], |_| None, &mut next_id);
        assert_eq!(m.operands, vec![7, 7]);
        assert_eq!(m.results, vec![50, 51]);
    }

    #[test]
    fn marshal_bwd_call_appends_d_y_and_allocates_n_results() {
        // Primal call : g(%0, %1) ; d_y = %42 ; expect 2 result ids allocated.
        let mut next_id = 100;
        let m = marshal_bwd_call_operands(&[0, 1], 42, 2, &mut next_id);
        // Operands : [0, 1, 42]
        assert_eq!(m.operands, vec![0, 1, 42]);
        // Results : [100, 101]
        assert_eq!(m.results, vec![100, 101]);
        assert_eq!(next_id, 102);
    }

    #[test]
    fn marshal_bwd_with_zero_float_args_allocates_no_results() {
        // Edge case : a fn with no differentiable params (still has d_y for
        // the result) — bwd call returns no adjoint values.
        let mut next_id = 10;
        let m = marshal_bwd_call_operands(&[], 99, 0, &mut next_id);
        assert_eq!(m.operands, vec![99]);
        assert!(m.results.is_empty());
        assert_eq!(next_id, 10);
    }

    #[test]
    fn marshal_fwd_with_three_args_produces_six_operands() {
        // 3-arg call : g(a, b, c) with all tangents set.
        let mut tmap = HashMap::new();
        tmap.insert(0u32, 10u32);
        tmap.insert(1u32, 11u32);
        tmap.insert(2u32, 12u32);
        let mut next_id = 100;
        let m = marshal_fwd_call_operands(&[0, 1, 2], |v| tmap.get(&v).copied(), &mut next_id);
        assert_eq!(m.operands, vec![0, 10, 1, 11, 2, 12]);
        assert_eq!(m.operands.len(), 6);
    }

    #[test]
    fn iter_visits_every_entry() {
        let t = CalleeVariantTable::from_diff_fn_names(["x", "y", "z"]);
        let count = t.iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn variants_equality() {
        let a = CalleeVariants::canonical("foo");
        let b = CalleeVariants::canonical("foo");
        assert_eq!(a, b);
        let c = CalleeVariants::canonical("bar");
        assert_ne!(a, c);
    }

    #[test]
    fn variant_table_clone_preserves_entries() {
        let t = CalleeVariantTable::from_diff_fn_names(["a", "b"]);
        let clone = t.clone();
        // Both the original and the clone must continue to carry both entries.
        assert_eq!(t.len(), 2);
        assert_eq!(clone.len(), 2);
        assert!(clone.contains("a"));
        assert!(clone.contains("b"));
    }
}

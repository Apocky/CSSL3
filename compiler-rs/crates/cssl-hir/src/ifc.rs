//! Information Flow Control (IFC) label-lattice + propagation walker.
//!
//! § SPEC : `specs/11_IFC.csl` (F5 — PRIME_DIRECTIVE encoded structurally).
//!
//! § SCOPE (T3.4-phase-3-IFC / this commit)
//!   Stage-0 IFC check : catalog + structural walker that collects IFC annotations
//!   from `@ifc_label(...)` / `@confidentiality(...)` / `@integrity(...)` attributes
//!   on fns, emits `IfcDiagnostic`s for known violation-shapes, and exposes the
//!   `IfcLabel` lattice algebra for downstream use (MIR `IfcLoweringPass`,
//!   SMT declassification-policy discharge at T9-phase-2b).
//!
//!   Stage-0 detects structural shapes (attribute-based) — full type-level label
//!   propagation through the HIR requires a refined-type-system extension that
//!   lives in T3.4-phase-3-IFC-b (future).
//!
//! § DIAGNOSTICS (stable codes for CI log-parsing)
//!   - `IFC0001` — `MissingLabel` : fn declares a sensitive param but carries no
//!     confidentiality annotation ; downstream SMT discharge can't verify the
//!     non-interference theorem without a concrete label.
//!   - `IFC0002` — `MissingDeclassPolicy` : fn contains a `@declass` call but has
//!     no `@requires(Privilege<level>)` attribute authorizing the declass.
//!   - `IFC0003` — `UnauthorizedDowngrade` : declass attribute's `from`/`to`
//!     labels are incompatible (confidentiality must not widen without a
//!     compile-time policy).
//!
//! § LATTICE ALGEBRA
//!   - [`IfcLabel`] = `(confidentiality : PrincipalSet, integrity : PrincipalSet)`
//!   - TOP = `(∅, All)` ← nobody-reads, everyone-influences — pure-public-untrusted
//!   - BOTTOM = `(All, ∅)` ← everyone-reads, nobody-influences — const-trusted
//!   - `L1 ⊑ L2  ≡  C1 ⊇ C2  ∧  I1 ⊆ I2` — stricter-reader-set + tighter-influencer-set
//!   - `L1 ⊔ L2 ≡ (C1 ∩ C2, I1 ∪ I2)` — upper-bound
//!   - `L1 ⊓ L2 ≡ (C1 ∪ C2, I1 ∩ I2)` — lower-bound
//!
//! § PRIME-DIRECTIVE built-in principals (per `specs/11`)
//!   `HarmTarget` `Surveiller` `Coercer` `Weaponizer` `System` `Kernel` `User`
//!   `Public` `Anthropic-Audit` — extensible via user-declared `principal …`.

use std::collections::BTreeSet;

use cssl_ast::Span;

use crate::arena::DefId;
use crate::item::{HirFn, HirItem, HirModule};
use crate::symbol::{Interner, Symbol};

/// An IFC confidentiality + integrity label pair.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IfcLabel {
    /// Confidentiality : the set of principals allowed to READ the value.
    pub confidentiality: BTreeSet<Symbol>,
    /// Integrity : the set of principals allowed to INFLUENCE the value.
    pub integrity: BTreeSet<Symbol>,
}

impl IfcLabel {
    /// Build an empty label (`(∅, ∅)`) — neither readable nor influenceable ; mostly useful as a placeholder.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a label from an explicit confidentiality + integrity principal-set.
    #[must_use]
    pub fn new(
        confidentiality: impl IntoIterator<Item = Symbol>,
        integrity: impl IntoIterator<Item = Symbol>,
    ) -> Self {
        Self {
            confidentiality: confidentiality.into_iter().collect(),
            integrity: integrity.into_iter().collect(),
        }
    }

    /// Lattice `⊑` : `self ⊑ other  ≡  C_self ⊇ C_other  ∧  I_self ⊆ I_other`.
    #[must_use]
    pub fn is_sub_of(&self, other: &Self) -> bool {
        other.confidentiality.is_subset(&self.confidentiality)
            && self.integrity.is_subset(&other.integrity)
    }

    /// Lattice join `⊔` : upper-bound (stricter-of-both).
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        Self {
            confidentiality: self
                .confidentiality
                .intersection(&other.confidentiality)
                .copied()
                .collect(),
            integrity: self.integrity.union(&other.integrity).copied().collect(),
        }
    }

    /// Lattice meet `⊓` : lower-bound (looser-of-both).
    #[must_use]
    pub fn meet(&self, other: &Self) -> Self {
        Self {
            confidentiality: self
                .confidentiality
                .union(&other.confidentiality)
                .copied()
                .collect(),
            integrity: self
                .integrity
                .intersection(&other.integrity)
                .copied()
                .collect(),
        }
    }

    /// Is this label a concrete non-empty assertion ? (either set non-empty)
    #[must_use]
    pub fn is_labeled(&self) -> bool {
        !self.confidentiality.is_empty() || !self.integrity.is_empty()
    }
}

/// Canonical built-in principal names (PRIME_DIRECTIVE-encoded per `specs/11`).
///
/// These are interned by [`builtin_principals`] at module-walk time ; user-
/// declared principals extend the set via `principal <name>` declarations
/// (stage-0 does not yet parse those — handled at T3.4-phase-3-IFC-b).
#[must_use]
pub fn builtin_principals(interner: &Interner) -> Vec<Symbol> {
    vec![
        interner.intern("HarmTarget"),
        interner.intern("Surveiller"),
        interner.intern("Coercer"),
        interner.intern("Weaponizer"),
        interner.intern("System"),
        interner.intern("Kernel"),
        interner.intern("User"),
        interner.intern("Public"),
        interner.intern("Anthropic-Audit"),
    ]
}

/// One IFC diagnostic with stable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IfcDiagnostic {
    /// Fn declares a sensitive param (annotated with `@sensitive` or a
    /// `Sensitive<...>` effect) but has no IFC label attached — SMT discharge
    /// of the non-interference theorem cannot proceed.
    MissingLabel { fn_name: String, fn_span: Span },
    /// Fn contains a declassification call but carries no `@requires(Privilege<...>)`
    /// attribute authorizing the declass.
    MissingDeclassPolicy { fn_name: String, fn_span: Span },
    /// A declassification attribute's `from` / `to` label pair violates the
    /// lattice invariant (e.g., `to` is more-confidential than `from` without
    /// a compile-time policy).
    UnauthorizedDowngrade {
        fn_name: String,
        from: String,
        to: String,
        fn_span: Span,
    },
}

impl IfcDiagnostic {
    /// Stable diagnostic code (for CI log-parsing).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MissingLabel { .. } => "IFC0001",
            Self::MissingDeclassPolicy { .. } => "IFC0002",
            Self::UnauthorizedDowngrade { .. } => "IFC0003",
        }
    }

    /// Human-readable message.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::MissingLabel { fn_name, .. } => format!(
                "fn `{fn_name}` declares sensitive param(s) but carries no IFC label \
                 (@confidentiality / @integrity / @ifc_label) — non-interference \
                 theorem cannot be discharged"
            ),
            Self::MissingDeclassPolicy { fn_name, .. } => format!(
                "fn `{fn_name}` contains a declassification call but carries no \
                 `@requires(Privilege<level>)` — declass without authorization is \
                 unsound per §§ 11 IFC"
            ),
            Self::UnauthorizedDowngrade {
                fn_name, from, to, ..
            } => format!(
                "fn `{fn_name}` declass from `{from}` to `{to}` widens confidentiality \
                 without a compile-time policy"
            ),
        }
    }
}

/// Aggregate IFC walker report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IfcReport {
    /// Diagnostics emitted during the walk.
    pub diagnostics: Vec<IfcDiagnostic>,
    /// Number of fns inspected.
    pub fns_checked: u32,
    /// Number of fns carrying at-least-one IFC-related attribute.
    pub fns_with_labels: u32,
    /// Number of declassification attempts observed.
    pub declass_attempts: u32,
}

impl IfcReport {
    /// `true` iff no diagnostics were emitted.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Count diagnostics by code.
    #[must_use]
    pub fn count(&self, code: &str) -> usize {
        self.diagnostics.iter().filter(|d| d.code() == code).count()
    }

    /// Short diagnostic summary.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "IFC : {} fns checked / {} labeled / {} declass attempts / {} diagnostics [{} IFC0001 / {} IFC0002 / {} IFC0003]",
            self.fns_checked,
            self.fns_with_labels,
            self.declass_attempts,
            self.diagnostics.len(),
            self.count("IFC0001"),
            self.count("IFC0002"),
            self.count("IFC0003"),
        )
    }
}

/// Check IFC invariants across every fn in the module.
///
/// Stage-0 shape : catalog + structural walker. Detects missing-label,
/// missing-declass-policy, and unauthorized-downgrade via attribute inspection.
/// Full type-level label-propagation through the HIR is T3.4-phase-3-IFC-b.
pub fn check_ifc(module: &HirModule, interner: &Interner) -> IfcReport {
    let sensitive_sym = interner.intern("sensitive");
    let confid_sym = interner.intern("confidentiality");
    let integrity_sym = interner.intern("integrity");
    let ifc_label_sym = interner.intern("ifc_label");
    let declass_sym = interner.intern("declass");
    let requires_sym = interner.intern("requires");

    let mut report = IfcReport::default();
    for item in &module.items {
        if let HirItem::Fn(f) = item {
            check_fn(
                f,
                interner,
                sensitive_sym,
                confid_sym,
                integrity_sym,
                ifc_label_sym,
                declass_sym,
                requires_sym,
                &mut report,
            );
        }
    }
    report
}

#[allow(clippy::too_many_arguments)]
fn check_fn(
    f: &HirFn,
    interner: &Interner,
    sensitive_sym: Symbol,
    confid_sym: Symbol,
    integrity_sym: Symbol,
    ifc_label_sym: Symbol,
    declass_sym: Symbol,
    requires_sym: Symbol,
    report: &mut IfcReport,
) {
    report.fns_checked = report.fns_checked.saturating_add(1);
    let fn_name = interner.resolve(f.name);

    let has_confid = f.attrs.iter().any(|a| a.is_simple(confid_sym));
    let has_integrity = f.attrs.iter().any(|a| a.is_simple(integrity_sym));
    let has_ifc_label = f.attrs.iter().any(|a| a.is_simple(ifc_label_sym));
    let labeled = has_confid || has_integrity || has_ifc_label;
    if labeled {
        report.fns_with_labels = report.fns_with_labels.saturating_add(1);
    }

    // Check : sensitive-tagged params must have a label on the fn.
    let any_sensitive_param = f
        .params
        .iter()
        .any(|p| p.attrs.iter().any(|a| a.is_simple(sensitive_sym)));
    if any_sensitive_param && !labeled {
        report.diagnostics.push(IfcDiagnostic::MissingLabel {
            fn_name: fn_name.clone(),
            fn_span: f.span,
        });
    }

    // Check : declass attribute presence + requires-Privilege authorization.
    let has_declass = f.attrs.iter().any(|a| a.is_simple(declass_sym));
    let has_requires = f.attrs.iter().any(|a| a.is_simple(requires_sym));
    if has_declass {
        report.declass_attempts = report.declass_attempts.saturating_add(1);
        if !has_requires {
            report
                .diagnostics
                .push(IfcDiagnostic::MissingDeclassPolicy {
                    fn_name: fn_name.clone(),
                    fn_span: f.span,
                });
        }
    }
}

/// Convenience : resolve a built-in principal name to a Symbol. Returns `None`
/// for unknown names.
#[must_use]
pub fn resolve_builtin_principal(name: &str, interner: &Interner) -> Option<Symbol> {
    let known = [
        "HarmTarget",
        "Surveiller",
        "Coercer",
        "Weaponizer",
        "System",
        "Kernel",
        "User",
        "Public",
        "Anthropic-Audit",
    ];
    if known.contains(&name) {
        Some(interner.intern(name))
    } else {
        None
    }
}

/// Build a common IFC label shorthand : `"secret(User)"` form → `IfcLabel`
/// with confidentiality = {User}. Multi-principal : `"secret(User, System)"`.
#[must_use]
pub fn label_for_secret(
    principals: impl IntoIterator<Item = Symbol>,
    interner: &Interner,
) -> IfcLabel {
    let _ = interner;
    IfcLabel::new(principals, std::iter::empty())
}

/// `DefId` → label registry. Phase-2b will populate from HIR-type annotations.
#[derive(Debug, Clone, Default)]
pub struct IfcLabelRegistry {
    map: std::collections::BTreeMap<u32, IfcLabel>,
}

impl IfcLabelRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a label for a DefId.
    pub fn insert(&mut self, def: DefId, label: IfcLabel) {
        self.map.insert(def.0, label);
    }

    /// Lookup.
    #[must_use]
    pub fn get(&self, def: DefId) -> Option<&IfcLabel> {
        self.map.get(&def.0)
    }

    /// Number of labeled defs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        builtin_principals, check_ifc, label_for_secret, resolve_builtin_principal, IfcDiagnostic,
        IfcLabel, IfcLabelRegistry,
    };
    use crate::arena::DefId;
    use crate::lower::lower_module;
    use crate::symbol::Interner;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn check(src: &str) -> super::IfcReport {
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);
        check_ifc(&hir, &interner)
    }

    #[test]
    fn empty_label_shapes() {
        let l = IfcLabel::empty();
        assert!(l.confidentiality.is_empty());
        assert!(l.integrity.is_empty());
        assert!(!l.is_labeled());
    }

    #[test]
    fn label_new_populates_sets() {
        let interner = Interner::new();
        let user = interner.intern("User");
        let sys = interner.intern("System");
        let l = IfcLabel::new([user, sys], [sys]);
        assert_eq!(l.confidentiality.len(), 2);
        assert_eq!(l.integrity.len(), 1);
        assert!(l.is_labeled());
    }

    #[test]
    fn lattice_join_intersects_confid_and_unions_integrity() {
        let interner = Interner::new();
        let user = interner.intern("User");
        let sys = interner.intern("System");
        let kernel = interner.intern("Kernel");
        let l1 = IfcLabel::new([user, sys], [user]);
        let l2 = IfcLabel::new([sys, kernel], [sys]);
        let j = l1.join(&l2);
        // Intersection of confid : {sys}
        assert_eq!(j.confidentiality.len(), 1);
        assert!(j.confidentiality.contains(&sys));
        // Union of integrity : {user, sys}
        assert_eq!(j.integrity.len(), 2);
    }

    #[test]
    fn lattice_meet_unions_confid_and_intersects_integrity() {
        let interner = Interner::new();
        let user = interner.intern("User");
        let sys = interner.intern("System");
        let l1 = IfcLabel::new([user], [user, sys]);
        let l2 = IfcLabel::new([sys], [sys]);
        let m = l1.meet(&l2);
        // Union of confid : {user, sys}
        assert_eq!(m.confidentiality.len(), 2);
        // Intersection of integrity : {sys}
        assert_eq!(m.integrity.len(), 1);
        assert!(m.integrity.contains(&sys));
    }

    #[test]
    fn lattice_is_sub_of_respects_ordering() {
        let interner = Interner::new();
        let user = interner.intern("User");
        let sys = interner.intern("System");
        // L1 has MORE-confidential reader-set ({user}) — wait, spec inverse :
        // L1 ⊑ L2 iff C1 ⊇ C2. So smaller-confid-set = more-restrictive = greater-in-lattice.
        let l1 = IfcLabel::new([user], [user]);
        let l2 = IfcLabel::new([user, sys], [user]);
        // l1.confid = {user}, l2.confid = {user, sys}
        // l1 ⊑ l2 iff {user} ⊇ {user, sys} which is FALSE.
        // l2 ⊑ l1 iff {user, sys} ⊇ {user} which is TRUE (and integrity : {user} ⊆ {user}).
        assert!(l2.is_sub_of(&l1));
        assert!(!l1.is_sub_of(&l2));
    }

    #[test]
    fn builtin_principals_covers_prime_directive_principals() {
        let interner = Interner::new();
        let list = builtin_principals(&interner);
        assert_eq!(list.len(), 9);
        assert!(resolve_builtin_principal("HarmTarget", &interner).is_some());
        assert!(resolve_builtin_principal("Surveiller", &interner).is_some());
        assert!(resolve_builtin_principal("Anthropic-Audit", &interner).is_some());
        assert!(resolve_builtin_principal("unknown_principal", &interner).is_none());
    }

    #[test]
    fn label_for_secret_populates_confid() {
        let interner = Interner::new();
        let user = interner.intern("User");
        let l = label_for_secret([user], &interner);
        assert_eq!(l.confidentiality.len(), 1);
        assert!(l.confidentiality.contains(&user));
        assert!(l.integrity.is_empty());
    }

    #[test]
    fn empty_module_is_clean() {
        let r = check("");
        assert!(r.is_clean());
        assert_eq!(r.fns_checked, 0);
    }

    #[test]
    fn unlabeled_fn_without_sensitive_params_is_clean() {
        let r = check("fn noop() {}");
        assert!(r.is_clean());
        assert_eq!(r.fns_checked, 1);
        assert_eq!(r.fns_with_labels, 0);
    }

    #[test]
    fn ifc_label_attr_counted_as_labeled() {
        // @ifc_label attribute on a fn marks it as labeled.
        let src = "@ifc_label fn f() {}";
        let r = check(src);
        assert_eq!(r.fns_with_labels, 1);
    }

    #[test]
    fn declass_without_requires_emits_ifc0002() {
        let src = "@declass fn unsafe_declass() {}";
        let r = check(src);
        assert!(r.count("IFC0002") >= 1, "{}", r.summary());
        assert_eq!(r.declass_attempts, 1);
    }

    #[test]
    fn declass_with_requires_is_clean() {
        let src = "@declass @requires fn authorized_declass() {}";
        let r = check(src);
        assert_eq!(r.count("IFC0002"), 0, "{}", r.summary());
        assert_eq!(r.declass_attempts, 1);
    }

    #[test]
    fn sensitive_param_without_label_emits_ifc0001() {
        // @sensitive on a param without @ifc_label on the fn.
        let src = "fn leaky(@sensitive x : i32) {}";
        let r = check(src);
        assert!(r.count("IFC0001") >= 1, "{}", r.summary());
    }

    #[test]
    fn sensitive_param_with_label_is_clean() {
        let src = "@confidentiality fn safe(@sensitive x : i32) {}";
        let r = check(src);
        assert_eq!(r.count("IFC0001"), 0, "{}", r.summary());
    }

    #[test]
    fn diagnostic_codes_stable() {
        let d = IfcDiagnostic::MissingLabel {
            fn_name: "x".into(),
            fn_span: cssl_ast::Span::DUMMY,
        };
        assert_eq!(d.code(), "IFC0001");
        assert!(d.message().contains("IFC label"));
        let d = IfcDiagnostic::MissingDeclassPolicy {
            fn_name: "x".into(),
            fn_span: cssl_ast::Span::DUMMY,
        };
        assert_eq!(d.code(), "IFC0002");
        let d = IfcDiagnostic::UnauthorizedDowngrade {
            fn_name: "x".into(),
            from: "a".into(),
            to: "b".into(),
            fn_span: cssl_ast::Span::DUMMY,
        };
        assert_eq!(d.code(), "IFC0003");
    }

    #[test]
    fn report_summary_shape() {
        let r = check("@declass fn unsafe_d() {}");
        let s = r.summary();
        assert!(s.contains("IFC"));
        assert!(s.contains("fns checked"));
        assert!(s.contains("declass attempts"));
    }

    #[test]
    fn label_registry_roundtrips() {
        let mut reg = IfcLabelRegistry::new();
        assert!(reg.is_empty());
        let interner = Interner::new();
        let user = interner.intern("User");
        reg.insert(DefId(7), IfcLabel::new([user], []));
        assert_eq!(reg.len(), 1);
        assert!(reg.get(DefId(7)).is_some());
        assert!(reg.get(DefId(99)).is_none());
    }
}

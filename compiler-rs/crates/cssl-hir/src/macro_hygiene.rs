//! Macro-hygiene compile-time validation pass.
//!
//! § SPEC : `specs/13_MACROS.csl` § TIER-HIERARCHY + § HYGIENE (Racket /
//!          Flatt et al. lineage).
//!
//! § SCOPE (T3.4-phase-3-macro-hygiene / this pass)
//!   Structural walker mirroring [`super::ad_legality`] (T3-D11), [`super::ifc`]
//!   (T3-D12), and [`super::staged_check`] (T3-D13). Validates attribute-level
//!   invariants on macro declarations — specifically :
//!
//!   (1) Every `@hygienic` attribute must co-occur with exactly one macro-tier-
//!       declaring attribute (`@attr_macro`, `@declarative`, or `@proc_macro`).
//!       A bare `@hygienic` on a non-macro item is almost certainly a typo and
//!       would silently no-op the hygiene guarantee.
//!
//!   (2) No item may carry more than one tier-declaring attribute. `@attr_macro
//!       @proc_macro fn foo() {}` is structurally ambiguous (which tier?) and
//!       rejected.
//!
//!   (3) `@attr_macro` / `@declarative` / `@proc_macro` on a fn without the
//!       mandatory `@hygienic` companion is a soft warning (stage-0 uses it
//!       only for the diagnostic ; future phases may promote to hard error).
//!
//!   The full Racket-lineage set-of-scopes algorithm (tracking `HygieneMark`
//!   on every identifier, flipping scopes on expansion) is deferred to
//!   phase-2e — that work requires HIR to carry per-identifier scope sets
//!   which stage-0 does not yet thread through lowering.
//!
//! § DIAGNOSTICS (stable codes for CI log-parsing)
//!   - `MAC0001` — `HygienicOnNonMacroDefinition` : `@hygienic` without any
//!     macro-tier-declaring companion.
//!   - `MAC0002` — `ConflictingMacroTiers` : multiple tier-declaring attrs on
//!     the same item.
//!   - `MAC0003` — `MacroWithoutHygienic` : a tier-declaring attr without the
//!     `@hygienic` hardening companion.
//!
//! § API shape
//!   Follows the established walker-pattern :
//!     `check_macro_hygiene(&HirModule, &Interner) -> MacroHygieneReport`
//!     `MacroHygieneReport { diagnostics, checked_item_count }`
//!     `MacroHygieneDiagnostic { code, span, message }`

use cssl_ast::Span;

use crate::item::{HirFn, HirItem, HirModule};
use crate::symbol::{Interner, Symbol};

/// Diagnostic code for a macro-hygiene violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroHygieneCode {
    /// `@hygienic` without any tier-declaring companion attr (MAC0001).
    HygienicOnNonMacroDefinition,
    /// Multiple tier-declaring attrs on the same item (MAC0002).
    ConflictingMacroTiers,
    /// Tier-declaring attr without the `@hygienic` companion (MAC0003).
    MacroWithoutHygienic,
}

impl MacroHygieneCode {
    /// Canonical code string for CI-log parsing.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::HygienicOnNonMacroDefinition => "MAC0001",
            Self::ConflictingMacroTiers => "MAC0002",
            Self::MacroWithoutHygienic => "MAC0003",
        }
    }
}

/// A single macro-hygiene violation diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroHygieneDiagnostic {
    pub code: MacroHygieneCode,
    pub span: Span,
    pub message: String,
}

impl MacroHygieneDiagnostic {
    /// Render a short one-line diagnostic message suitable for the log.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{} : {}", self.code.code(), self.message)
    }
}

/// Report emitted by [`check_macro_hygiene`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MacroHygieneReport {
    /// Every diagnostic collected during the walk.
    pub diagnostics: Vec<MacroHygieneDiagnostic>,
    /// Number of items with at least one macro-related attribute that the
    /// walker examined. `MAC0001` / `MAC0002` / `MAC0003` counts go into
    /// `diagnostics.len()`.
    pub checked_item_count: u32,
}

impl MacroHygieneReport {
    /// `true` iff the report has no diagnostics.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Summary line for CI log output.
    #[must_use]
    pub fn summary(&self) -> String {
        let by_code = |c: MacroHygieneCode| self.diagnostics.iter().filter(|d| d.code == c).count();
        format!(
            "macro-hygiene : {} items checked ; {} MAC0001 / {} MAC0002 / {} MAC0003",
            self.checked_item_count,
            by_code(MacroHygieneCode::HygienicOnNonMacroDefinition),
            by_code(MacroHygieneCode::ConflictingMacroTiers),
            by_code(MacroHygieneCode::MacroWithoutHygienic),
        )
    }
}

/// Walk `module` and validate macro-hygiene invariants. Returns a
/// [`MacroHygieneReport`] regardless of whether violations were found.
#[must_use]
pub fn check_macro_hygiene(module: &HirModule, interner: &Interner) -> MacroHygieneReport {
    let mut report = MacroHygieneReport::default();
    let tier_names = TierNames::intern(interner);
    walk_items(&module.items, &tier_names, &mut report, interner);
    report
}

/// Recursively walk items, including nested modules + impl methods.
fn walk_items(
    items: &[HirItem],
    tiers: &TierNames,
    report: &mut MacroHygieneReport,
    interner: &Interner,
) {
    for item in items {
        walk_item(item, tiers, report, interner);
    }
}

fn walk_item(
    item: &HirItem,
    tiers: &TierNames,
    report: &mut MacroHygieneReport,
    interner: &Interner,
) {
    match item {
        HirItem::Fn(f) => check_fn(f, tiers, report),
        HirItem::Impl(impl_block) => {
            for method in &impl_block.fns {
                check_fn(method, tiers, report);
            }
        }
        HirItem::Module(nested) => {
            if let Some(sub) = &nested.items {
                walk_items(sub, tiers, report, interner);
            }
        }
        HirItem::Effect(_)
        | HirItem::Handler(_)
        | HirItem::Struct(_)
        | HirItem::Enum(_)
        | HirItem::Interface(_)
        | HirItem::TypeAlias(_)
        | HirItem::Use(_)
        | HirItem::Const(_) => {
            // Stage-0 : only fns can be macros. Future phases may extend to
            // tier-1 `@attr_macro` on structs (annotation-style).
        }
    }
}

fn check_fn(f: &HirFn, tiers: &TierNames, report: &mut MacroHygieneReport) {
    let classification = classify_attrs(&f.attrs, tiers);
    let has_any_macro_attr = classification.has_hygienic || classification.tier_declaring_count > 0;
    if !has_any_macro_attr {
        return;
    }

    report.checked_item_count = report.checked_item_count.saturating_add(1);

    // MAC0002 : multiple tier-declaring attrs.
    if classification.tier_declaring_count > 1 {
        report.diagnostics.push(MacroHygieneDiagnostic {
            code: MacroHygieneCode::ConflictingMacroTiers,
            span: classification.first_tier_span.unwrap_or(f.span),
            message: format!(
                "item carries {} conflicting macro-tier declarations (expected exactly 1)",
                classification.tier_declaring_count
            ),
        });
    }

    // MAC0001 : `@hygienic` without any tier-declaring companion.
    if classification.has_hygienic && classification.tier_declaring_count == 0 {
        report.diagnostics.push(MacroHygieneDiagnostic {
            code: MacroHygieneCode::HygienicOnNonMacroDefinition,
            span: classification.hygienic_span.unwrap_or(f.span),
            message: "`@hygienic` has no effect without a tier-declaring attribute \
                 (`@attr_macro` / `@declarative` / `@proc_macro`)"
                .to_string(),
        });
    }

    // MAC0003 : tier-declaring attr without the `@hygienic` hardening.
    if classification.tier_declaring_count > 0 && !classification.has_hygienic {
        report.diagnostics.push(MacroHygieneDiagnostic {
            code: MacroHygieneCode::MacroWithoutHygienic,
            span: classification.first_tier_span.unwrap_or(f.span),
            message: "macro-declaring attribute without `@hygienic` companion — \
                 identifier capture is possible"
                .to_string(),
        });
    }
}

/// Summary of which macro-related attributes an item carries.
#[derive(Debug, Default)]
struct AttrClassification {
    has_hygienic: bool,
    hygienic_span: Option<Span>,
    tier_declaring_count: u32,
    first_tier_span: Option<Span>,
}

fn classify_attrs(attrs: &[crate::attr::HirAttr], tiers: &TierNames) -> AttrClassification {
    let mut out = AttrClassification::default();
    for a in attrs {
        if a.path.len() != 1 {
            continue;
        }
        let sym = a.path[0];
        if sym == tiers.hygienic {
            out.has_hygienic = true;
            out.hygienic_span.get_or_insert(a.span);
        } else if sym == tiers.attr_macro || sym == tiers.declarative || sym == tiers.proc_macro {
            out.tier_declaring_count = out.tier_declaring_count.saturating_add(1);
            out.first_tier_span.get_or_insert(a.span);
        }
    }
    out
}

/// Pre-interned symbols for the four attribute names we recognize.
struct TierNames {
    hygienic: Symbol,
    attr_macro: Symbol,
    declarative: Symbol,
    proc_macro: Symbol,
}

impl TierNames {
    fn intern(interner: &Interner) -> Self {
        Self {
            hygienic: interner.intern("hygienic"),
            attr_macro: interner.intern("attr_macro"),
            declarative: interner.intern("declarative"),
            proc_macro: interner.intern("proc_macro"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{check_macro_hygiene, MacroHygieneCode};
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn parse(src: &str) -> (crate::HirModule, crate::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = crate::lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn empty_module_yields_no_diagnostics() {
        let (hir, interner) = parse("");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean());
        assert_eq!(report.checked_item_count, 0);
    }

    #[test]
    fn fn_without_macro_attrs_is_not_checked() {
        let (hir, interner) = parse("fn plain() -> i32 { 42 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean());
        assert_eq!(report.checked_item_count, 0);
    }

    #[test]
    fn hygienic_only_emits_mac0001() {
        let (hir, interner) = parse("@hygienic fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].code,
            MacroHygieneCode::HygienicOnNonMacroDefinition
        );
        assert!(report.diagnostics[0].message.contains("`@hygienic`"));
    }

    #[test]
    fn tier_declaring_without_hygienic_emits_mac0003() {
        let (hir, interner) = parse("@declarative fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].code,
            MacroHygieneCode::MacroWithoutHygienic
        );
    }

    #[test]
    fn declarative_with_hygienic_is_clean() {
        let (hir, interner) = parse("@declarative @hygienic fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean(), "{}", report.summary());
        assert_eq!(report.checked_item_count, 1);
    }

    #[test]
    fn attr_macro_with_hygienic_is_clean() {
        let (hir, interner) = parse("@attr_macro @hygienic fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    #[test]
    fn proc_macro_with_hygienic_is_clean() {
        let (hir, interner) = parse("@proc_macro @hygienic fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    #[test]
    fn two_tier_declarations_emit_mac0002() {
        let (hir, interner) = parse("@declarative @attr_macro @hygienic fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == MacroHygieneCode::ConflictingMacroTiers));
    }

    #[test]
    fn two_tier_declarations_without_hygienic_emit_mac0002_and_mac0003() {
        let (hir, interner) = parse("@declarative @attr_macro fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == MacroHygieneCode::ConflictingMacroTiers));
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.code == MacroHygieneCode::MacroWithoutHygienic));
    }

    #[test]
    fn diagnostic_render_contains_code() {
        let (hir, interner) = parse("@hygienic fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.diagnostics[0].render().contains("MAC0001"));
    }

    #[test]
    fn summary_includes_all_code_counts() {
        let (hir, interner) =
            parse("@hygienic fn a() -> i32 { 0 } @declarative fn b() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        let s = report.summary();
        assert!(s.contains("items checked"));
        assert!(s.contains("MAC0001"));
        assert!(s.contains("MAC0002"));
        assert!(s.contains("MAC0003"));
    }

    #[test]
    fn multiple_clean_macros_all_counted() {
        let (hir, interner) = parse(
            "@declarative @hygienic fn a() -> i32 { 0 } \
             @attr_macro @hygienic fn b() -> i32 { 0 } \
             @proc_macro @hygienic fn c() -> i32 { 0 }",
        );
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean(), "{}", report.summary());
        assert_eq!(report.checked_item_count, 3);
    }

    #[test]
    fn multi_segment_macro_attr_is_ignored() {
        // `@cssl.macros.declarative` has path length > 1 ; stage-0 only
        // recognizes single-segment attrs. Multi-segment paths are
        // user-namespaced and don't trigger hygiene diagnostics.
        let (hir, interner) = parse("@cssl.macros.declarative fn foo() -> i32 { 0 }");
        let report = check_macro_hygiene(&hir, &interner);
        assert!(report.is_clean());
        assert_eq!(report.checked_item_count, 0);
    }
}

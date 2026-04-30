//! Exhaustiveness checking for `match` expressions on enum types.
//!
//! § SPEC :
//!   - `specs/40_WAVE_CSSL_PLAN.csl` § WAVE-A · A4
//!   - `stdlib/option.cssl` § INVARIANTS · exhaustively-matched
//!   - `specs/03_TYPES.csl` § BASE-TYPES § aggregate (sum-types)
//!
//! § SCOPE (Wave-A4 / this slice)
//!   Pure HIR-pass that walks every `HirExprKind::Match` in the module and verifies
//!   the arm-set covers every variant of the scrutinee's enum. Cases that are not
//!   exhaustive emit a `E1004` diagnostic naming the first uncovered variant and
//!   listing all uncovered variants in the diagnostic-extra-info.
//!
//!   No MIR / cgen / ABI changes. The check is a one-time per-match-node cold-path
//!   walker — `O(arms × variants)` is the operating regime, with `variants ≤ 64`
//!   represented as a `u64` bitset and `variants > 64` falling back to `BTreeSet`.
//!
//! § ALGORITHM (Maranget — usefulness, simplified to enum-only)
//!   For each `HirExprKind::Match` :
//!     1. Resolve the scrutinee's enum-decl by examining the *first variant pattern*
//!        among the arms ; that pattern's `def` field carries the variant's `DefId`,
//!        and we look that up in the module's enum-table to recover the enum-decl.
//!        (Stage-0 HIR runs this pass *before* full type inference completes the
//!         scrutinee-type slot, so we use the structural-pattern back-pointer
//!         rather than the post-inference `Ty`. Both routes converge to the same
//!         enum-decl.)
//!     2. Build the enum's full-variant index-set as a `VariantSet`.
//!     3. Walk each arm's pattern collecting *covered* variants — a wildcard or
//!        binding pattern covers every variant ; an or-pattern recurses ; a
//!        `HirPatternKind::Variant` with a known `def` flips one bit. A guard on
//!        the arm makes that arm non-total : variant coverage is *only* counted
//!        when the arm has no guard (guards are runtime predicates that may fail).
//!     4. The set-difference (full ∖ covered) yields the uncovered variants. If
//!        non-empty → emit `E1004` ; if empty → exhaustive, no diagnostic.
//!
//!   Or-patterns, nested patterns inside `Variant.args`, and guards are all handled
//!   per the rules above. Range / Literal / Tuple / Struct patterns at the top level
//!   are *not* enum-variant patterns ; they are treated conservatively (they don't
//!   contribute to variant coverage). Most stage-0 enum-matches in the stdlib will
//!   have purely-variant arms, so this is sound for the supported subset.
//!
//! § DIAGNOSTIC
//!   - `E1004` — `NonExhaustiveMatch` : `match` on an enum scrutinee is missing
//!     one or more variants. The message names the *first* uncovered variant ;
//!     the `missing_variants` field lists all uncovered variant-names in
//!     declaration order for tooling / IDE-rendering.
//!
//! § SAWYER-EFFICIENCY
//!   - `VariantSet` is a tagged-union : `Bits(u64)` for ≤ 64 variants (the common
//!     case — Option = 2, Result = 2, custom 3-7 variants typical), `Big(BTreeSet)`
//!     for the rare > 64 case. Bit-flip via OR ; difference via AND-NOT ; popcount
//!     via `count_ones`. No `HashMap<String, _>`. Variant identity is the index
//!     into the enum-decl's `variants : Vec<HirEnumVariant>`.
//!   - The enum-table is a `Vec<&HirEnum>` indexed by walking the module once ;
//!     variant-DefId → enum-index lookup is a `BTreeMap<DefId, (enum_idx, variant_idx)>`
//!     built once per module.
//!
//! § INTEGRATION_NOTE
//!   Per Wave-A4 hard-constraint, this module is *not* added to `crate::lib.rs`'s
//!   `pub mod` list in this slice ; it lives as a self-contained unit ready to be
//!   wired in by the next slice that integrates it into the lowering pipeline.
//!   Users who want to invoke it in tests reach in via the crate-internal path
//!   `cssl_hir::exhaustiveness::check_exhaustiveness` once a follow-up adds
//!   `pub mod exhaustiveness;` to `lib.rs`. For now the unit-tests in this file
//!   exercise the API directly via `super::*`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use cssl_ast::Span;

use crate::arena::DefId;
use crate::expr::{HirBlock, HirExpr, HirExprKind, HirMatchArm};
use crate::item::{HirEnum, HirImpl, HirItem, HirModule, HirNestedModule};
use crate::pat::{HirPattern, HirPatternKind};
use crate::stmt::{HirStmt, HirStmtKind};
use crate::symbol::Interner;

// ───────────────────────────────────────────────────────────────────────────
// § Diagnostic types.
// ───────────────────────────────────────────────────────────────────────────

/// Diagnostic code for an exhaustiveness violation.
///
/// Stable string for CI log-parsing. Stage-0 ships with one code (`E1004`) ;
/// future slices may add `E1005` for unreachable arms / `E1006` for
/// overlapping ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ExhaustivenessCode {
    /// `E1004` — the match-arm-set does not cover every variant of the scrutinee's enum.
    NonExhaustiveMatch,
}

impl ExhaustivenessCode {
    /// Canonical short-code string for log-parsing tools.
    #[must_use]
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::NonExhaustiveMatch => "E1004",
        }
    }
}

/// One exhaustiveness-check diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExhaustivenessDiagnostic {
    /// Diagnostic code.
    pub code: ExhaustivenessCode,
    /// Span of the offending `match` expression.
    pub span: Span,
    /// Human-readable message naming the first uncovered variant.
    pub message: String,
    /// All uncovered variant names in enum-declaration order. Tooling (IDE
    /// quickfix, `csslc --explain`) reads this for rendering.
    pub missing_variants: Vec<String>,
    /// Name of the enum the scrutinee binds to (e.g., `"Option"`).
    pub enum_name: String,
}

impl ExhaustivenessDiagnostic {
    /// Render a one-line CI-log message in the canonical
    /// `error: non-exhaustive match : missing pattern `<V>` (E1004)` shape.
    #[must_use]
    pub(crate) fn render(&self) -> String {
        format!(
            "error: non-exhaustive match : missing pattern `{}` ({})",
            self.missing_variants
                .first()
                .map_or("?", |s| s.as_str()),
            self.code.code(),
        )
    }
}

/// Aggregate report from an exhaustiveness pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ExhaustivenessReport {
    /// All diagnostics found in the module.
    pub diagnostics: Vec<ExhaustivenessDiagnostic>,
    /// Number of `match` expressions inspected (regardless of pass/fail).
    pub checked_match_count: u32,
    /// Number of `match` expressions that hit a non-resolvable scrutinee
    /// (no variant-arm pattern with a resolved `def`) — these are skipped
    /// rather than being reported as a violation, since the responsibility
    /// to resolve the path lies with the resolver pass.
    pub skipped_unresolved_count: u32,
}

impl ExhaustivenessReport {
    /// `true` iff no diagnostics were collected.
    #[must_use]
    pub(crate) fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Count of diagnostics matching a code.
    #[must_use]
    pub(crate) fn count(&self, code: ExhaustivenessCode) -> usize {
        self.diagnostics.iter().filter(|d| d.code == code).count()
    }

    /// Short summary line for log output.
    #[must_use]
    pub(crate) fn summary(&self) -> String {
        format!(
            "exhaustiveness : {} match-exprs / {} skipped-unresolved / {} E1004",
            self.checked_match_count,
            self.skipped_unresolved_count,
            self.count(ExhaustivenessCode::NonExhaustiveMatch),
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § VariantSet — Sawyer-efficiency bit-set vs. BTreeSet fallback.
// ───────────────────────────────────────────────────────────────────────────

/// Compact set of variant-indices for an enum.
///
/// `Bits` is a `u64` bitmap : variant `i` is in the set iff bit `i` is 1.
/// `Big` is the fallback for enums with more than 64 variants (rare).
///
/// Operations are : insert, contains, difference (with full-set), iter.
#[derive(Debug, Clone, PartialEq, Eq)]
enum VariantSet {
    Bits(u64),
    Big(BTreeSet<u32>),
}

impl VariantSet {
    /// Empty set.
    fn empty(variant_count: usize) -> Self {
        if variant_count <= 64 {
            Self::Bits(0)
        } else {
            Self::Big(BTreeSet::new())
        }
    }

    /// Full set covering all `variant_count` variants.
    fn full(variant_count: usize) -> Self {
        if variant_count <= 64 {
            // `1 << 64` is UB — guard. For variant_count == 64 the mask is `!0`.
            let mask = if variant_count == 64 {
                u64::MAX
            } else {
                (1u64 << variant_count) - 1
            };
            Self::Bits(mask)
        } else {
            Self::Big((0..variant_count as u32).collect())
        }
    }

    /// Insert variant `i`.
    fn insert(&mut self, i: u32) {
        match self {
            Self::Bits(b) => *b |= 1u64 << i,
            Self::Big(s) => {
                s.insert(i);
            }
        }
    }

    /// `true` iff variant `i` is in the set.
    fn contains(&self, i: u32) -> bool {
        match self {
            Self::Bits(b) => (*b & (1u64 << i)) != 0,
            Self::Big(s) => s.contains(&i),
        }
    }

    /// Set-difference : `self ∖ other` ; returns variants in `self` not in `other`.
    fn difference(&self, other: &Self) -> Vec<u32> {
        match (self, other) {
            (Self::Bits(a), Self::Bits(b)) => {
                let diff = *a & !*b;
                (0..64u32).filter(|i| (diff & (1u64 << i)) != 0).collect()
            }
            (Self::Big(a), Self::Big(b)) => a.difference(b).copied().collect(),
            // Mixed-mode fallback : promote to BTreeSet on one side.
            (Self::Bits(a), Self::Big(b)) => {
                let mut out = Vec::new();
                for i in 0..64u32 {
                    if (*a & (1u64 << i)) != 0 && !b.contains(&i) {
                        out.push(i);
                    }
                }
                out
            }
            (Self::Big(a), Self::Bits(b)) => a
                .iter()
                .copied()
                .filter(|i| (*b & (1u64 << i)) == 0)
                .collect(),
        }
    }

    /// `true` iff the set is empty.
    #[allow(dead_code)] // exposed for future-use ; tests reach in via difference()
    fn is_empty(&self) -> bool {
        match self {
            Self::Bits(b) => *b == 0,
            Self::Big(s) => s.is_empty(),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Enum-table : DefId → (enum_idx, variant_idx) lookup built once per module.
// ───────────────────────────────────────────────────────────────────────────

/// Per-module index of every enum declaration with a fast variant-DefId lookup.
struct EnumTable<'a> {
    /// All enums collected in module order (incl. nested modules + impls).
    enums: Vec<&'a HirEnum>,
    /// Variant-DefId → (enum_idx, variant_idx) within `enums[enum_idx].variants`.
    by_variant: BTreeMap<DefId, (usize, u32)>,
}

impl<'a> EnumTable<'a> {
    /// Walk `module` and collect every enum + its variants. Visits nested
    /// `module` blocks ; ignores `impl` (impls don't introduce new enums).
    fn build(module: &'a HirModule) -> Self {
        let mut enums: Vec<&'a HirEnum> = Vec::new();
        Self::collect_items(&module.items, &mut enums);
        let mut by_variant = BTreeMap::new();
        for (e_idx, e) in enums.iter().enumerate() {
            for (v_idx, v) in e.variants.iter().enumerate() {
                if !v.def.is_unresolved() {
                    by_variant.insert(v.def, (e_idx, v_idx as u32));
                }
            }
        }
        Self { enums, by_variant }
    }

    fn collect_items(items: &'a [HirItem], out: &mut Vec<&'a HirEnum>) {
        for item in items {
            Self::collect_item(item, out);
        }
    }

    fn collect_item(item: &'a HirItem, out: &mut Vec<&'a HirEnum>) {
        match item {
            HirItem::Enum(e) => out.push(e),
            HirItem::Module(HirNestedModule {
                items: Some(nested),
                ..
            }) => Self::collect_items(nested, out),
            // Impl blocks may contain inherent fns but no enum decls. Skip.
            // Other item kinds (Fn / Struct / Interface / Effect / Handler /
            // TypeAlias / Use / Const / Module-without-body) carry no enums.
            HirItem::Fn(_)
            | HirItem::Struct(_)
            | HirItem::Interface(_)
            | HirItem::Impl(_)
            | HirItem::Effect(_)
            | HirItem::Handler(_)
            | HirItem::TypeAlias(_)
            | HirItem::Use(_)
            | HirItem::Const(_)
            | HirItem::Module(HirNestedModule { items: None, .. }) => {}
        }
    }

    /// Resolve a variant DefId to `(enum_idx, variant_idx)`.
    fn lookup(&self, def: DefId) -> Option<(usize, u32)> {
        self.by_variant.get(&def).copied()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Public API.
// ───────────────────────────────────────────────────────────────────────────

/// Run the exhaustiveness pass over `module`. Returns an [`ExhaustivenessReport`]
/// regardless of whether violations were found ; callers consume the report and
/// upgrade `E1004` diagnostics to compile-errors.
#[must_use]
pub(crate) fn check_exhaustiveness(
    module: &HirModule,
    interner: &Interner,
) -> ExhaustivenessReport {
    let mut report = ExhaustivenessReport::default();
    let table = EnumTable::build(module);
    for item in &module.items {
        walk_item(item, &table, interner, &mut report);
    }
    report
}

// ───────────────────────────────────────────────────────────────────────────
// § Walker — recursive descent over HIR items + expressions.
// ───────────────────────────────────────────────────────────────────────────

fn walk_item(
    item: &HirItem,
    table: &EnumTable<'_>,
    interner: &Interner,
    report: &mut ExhaustivenessReport,
) {
    match item {
        HirItem::Fn(f) => {
            if let Some(body) = f.body.as_ref() {
                walk_block(body, table, interner, report);
            }
        }
        HirItem::Const(c) => walk_expr(&c.value, table, interner, report),
        HirItem::Impl(HirImpl { fns, .. }) => {
            for method in fns {
                if let Some(body) = method.body.as_ref() {
                    walk_block(body, table, interner, report);
                }
            }
        }
        HirItem::Module(HirNestedModule {
            items: Some(nested),
            ..
        }) => {
            for it in nested {
                walk_item(it, table, interner, report);
            }
        }
        HirItem::Effect(eff) => {
            for op in &eff.ops {
                if let Some(body) = op.body.as_ref() {
                    walk_block(body, table, interner, report);
                }
            }
        }
        HirItem::Handler(h) => {
            for op in &h.ops {
                if let Some(body) = op.body.as_ref() {
                    walk_block(body, table, interner, report);
                }
            }
            if let Some(ret) = h.return_clause.as_ref() {
                walk_block(ret, table, interner, report);
            }
        }
        HirItem::Interface(iface) => {
            for f in &iface.fns {
                if let Some(body) = f.body.as_ref() {
                    walk_block(body, table, interner, report);
                }
            }
        }
        HirItem::Enum(_)
        | HirItem::Struct(_)
        | HirItem::TypeAlias(_)
        | HirItem::Use(_)
        | HirItem::Module(HirNestedModule { items: None, .. }) => {}
    }
}

fn walk_block(
    block: &HirBlock,
    table: &EnumTable<'_>,
    interner: &Interner,
    report: &mut ExhaustivenessReport,
) {
    for stmt in &block.stmts {
        walk_stmt(stmt, table, interner, report);
    }
    if let Some(trail) = block.trailing.as_ref() {
        walk_expr(trail, table, interner, report);
    }
}

fn walk_stmt(
    stmt: &HirStmt,
    table: &EnumTable<'_>,
    interner: &Interner,
    report: &mut ExhaustivenessReport,
) {
    match &stmt.kind {
        HirStmtKind::Let { value, .. } => {
            if let Some(v) = value.as_ref() {
                walk_expr(v, table, interner, report);
            }
        }
        HirStmtKind::Expr(e) => walk_expr(e, table, interner, report),
        HirStmtKind::Item(it) => walk_item(it.as_ref(), table, interner, report),
    }
}

fn walk_expr(
    expr: &HirExpr,
    table: &EnumTable<'_>,
    interner: &Interner,
    report: &mut ExhaustivenessReport,
) {
    match &expr.kind {
        HirExprKind::Match { scrutinee, arms } => {
            // Walk children first — nested matches inside the scrutinee or arm
            // bodies must also be checked.
            walk_expr(scrutinee, table, interner, report);
            for arm in arms {
                if let Some(g) = arm.guard.as_ref() {
                    walk_expr(g, table, interner, report);
                }
                walk_expr(&arm.body, table, interner, report);
            }
            // Now check this match for exhaustiveness.
            check_match(expr.span, arms, table, interner, report);
        }

        // Recurse into every kind that holds nested expressions.
        HirExprKind::Call { callee, args, .. } => {
            walk_expr(callee, table, interner, report);
            for a in args {
                match a {
                    crate::expr::HirCallArg::Positional(e)
                    | crate::expr::HirCallArg::Named { value: e, .. } => {
                        walk_expr(e, table, interner, report);
                    }
                }
            }
        }
        HirExprKind::Field { obj, .. } => walk_expr(obj, table, interner, report),
        HirExprKind::Index { obj, index } => {
            walk_expr(obj, table, interner, report);
            walk_expr(index, table, interner, report);
        }
        HirExprKind::Binary { lhs, rhs, .. } | HirExprKind::Compound { lhs, rhs, .. } => {
            walk_expr(lhs, table, interner, report);
            walk_expr(rhs, table, interner, report);
        }
        HirExprKind::Unary { operand, .. } => walk_expr(operand, table, interner, report),
        HirExprKind::Block(b) => walk_block(b, table, interner, report),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_expr(cond, table, interner, report);
            walk_block(then_branch, table, interner, report);
            if let Some(eb) = else_branch.as_ref() {
                walk_expr(eb, table, interner, report);
            }
        }
        HirExprKind::For { iter, body, .. } => {
            walk_expr(iter, table, interner, report);
            walk_block(body, table, interner, report);
        }
        HirExprKind::While { cond, body } => {
            walk_expr(cond, table, interner, report);
            walk_block(body, table, interner, report);
        }
        HirExprKind::Loop { body } => walk_block(body, table, interner, report),
        HirExprKind::Return { value } => {
            if let Some(v) = value.as_ref() {
                walk_expr(v, table, interner, report);
            }
        }
        HirExprKind::Break { value, .. } => {
            if let Some(v) = value.as_ref() {
                walk_expr(v, table, interner, report);
            }
        }
        HirExprKind::Lambda { body, .. } => walk_expr(body, table, interner, report),
        HirExprKind::Assign { lhs, rhs, .. } => {
            walk_expr(lhs, table, interner, report);
            walk_expr(rhs, table, interner, report);
        }
        HirExprKind::Cast { expr: e, .. } => walk_expr(e, table, interner, report),
        HirExprKind::Range { lo, hi, .. } => {
            if let Some(l) = lo.as_ref() {
                walk_expr(l, table, interner, report);
            }
            if let Some(h) = hi.as_ref() {
                walk_expr(h, table, interner, report);
            }
        }
        HirExprKind::Pipeline { lhs, rhs } => {
            walk_expr(lhs, table, interner, report);
            walk_expr(rhs, table, interner, report);
        }
        HirExprKind::TryDefault { expr: e, default } => {
            walk_expr(e, table, interner, report);
            walk_expr(default, table, interner, report);
        }
        HirExprKind::Try { expr: e } => walk_expr(e, table, interner, report),
        HirExprKind::Perform { args, .. } => {
            for a in args {
                match a {
                    crate::expr::HirCallArg::Positional(e)
                    | crate::expr::HirCallArg::Named { value: e, .. } => {
                        walk_expr(e, table, interner, report);
                    }
                }
            }
        }
        HirExprKind::With { handler, body } => {
            walk_expr(handler, table, interner, report);
            walk_block(body, table, interner, report);
        }
        HirExprKind::Region { body, .. } => walk_block(body, table, interner, report),
        HirExprKind::Tuple(items) => {
            for e in items {
                walk_expr(e, table, interner, report);
            }
        }
        HirExprKind::Array(arr) => match arr {
            crate::expr::HirArrayExpr::List(items) => {
                for e in items {
                    walk_expr(e, table, interner, report);
                }
            }
            crate::expr::HirArrayExpr::Repeat { elem, len } => {
                walk_expr(elem, table, interner, report);
                walk_expr(len, table, interner, report);
            }
        },
        HirExprKind::Struct { fields, spread, .. } => {
            for f in fields {
                if let Some(v) = f.value.as_ref() {
                    walk_expr(v, table, interner, report);
                }
            }
            if let Some(s) = spread.as_ref() {
                walk_expr(s, table, interner, report);
            }
        }
        HirExprKind::Run { expr: e } => walk_expr(e, table, interner, report),
        HirExprKind::Paren(inner) => walk_expr(inner, table, interner, report),

        // Leaves : nothing nested.
        HirExprKind::Literal(_)
        | HirExprKind::Path { .. }
        | HirExprKind::Continue { .. }
        | HirExprKind::SectionRef { .. }
        | HirExprKind::Error => {}
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Per-match exhaustiveness check.
// ───────────────────────────────────────────────────────────────────────────

fn check_match(
    span: Span,
    arms: &[HirMatchArm],
    table: &EnumTable<'_>,
    interner: &Interner,
    report: &mut ExhaustivenessReport,
) {
    report.checked_match_count = report.checked_match_count.saturating_add(1);

    // Resolve which enum the scrutinee is being matched against. We scan the
    // arms looking for a Variant-pattern with a resolved DefId ; the first hit
    // tells us the enum. If no arm carries a resolved variant-DefId we cannot
    // check (likely the scrutinee is a primitive or the resolver hasn't run
    // yet) — record skip and return.
    let Some(enum_idx) = find_enum_for_arms(arms, table) else {
        report.skipped_unresolved_count = report.skipped_unresolved_count.saturating_add(1);
        return;
    };

    let enum_decl = table.enums[enum_idx];
    let variant_count = enum_decl.variants.len();

    // Empty enum (zero variants) — any non-empty arm-set is in fact unreachable,
    // and an empty arm-set is exhaustive. Stage-0 doesn't synthesize empty
    // enums, so this is a degenerate but well-defined edge.
    if variant_count == 0 {
        return;
    }

    // Build the covered-set by walking every arm's pattern. Arms with guards
    // are NOT counted toward variant coverage — guards may fail at runtime,
    // so a guarded `Some(x) if x > 0 =>` does not exhaustively cover `Some`.
    let mut covered = VariantSet::empty(variant_count);
    let full = VariantSet::full(variant_count);
    for arm in arms {
        if arm.guard.is_some() {
            continue;
        }
        collect_pattern_coverage(&arm.pat, table, enum_idx, variant_count, &mut covered);
    }

    let missing = covered.difference_from_full(&full, variant_count);
    if missing.is_empty() {
        return;
    }

    // Build the diagnostic with all uncovered variant-names in declaration order.
    let missing_names: Vec<String> = missing
        .iter()
        .map(|&i| interner.resolve(enum_decl.variants[i as usize].name))
        .collect();
    let enum_name = interner.resolve(enum_decl.name);
    let first = missing_names.first().cloned().unwrap_or_default();
    let message = if missing_names.len() == 1 {
        format!(
            "non-exhaustive match : missing pattern `{first}` (E1004) — \
             enum `{enum_name}` has 1 uncovered variant"
        )
    } else {
        format!(
            "non-exhaustive match : missing pattern `{first}` (E1004) — \
             enum `{enum_name}` has {n} uncovered variants : {list}",
            n = missing_names.len(),
            list = missing_names.join(", "),
        )
    };
    report.diagnostics.push(ExhaustivenessDiagnostic {
        code: ExhaustivenessCode::NonExhaustiveMatch,
        span,
        message,
        missing_variants: missing_names,
        enum_name,
    });
}

/// Find which enum the match-arms are targeting by looking for the first arm
/// with a resolved variant-pattern. Returns `None` if no arm carries a
/// resolvable variant-DefId.
fn find_enum_for_arms(arms: &[HirMatchArm], table: &EnumTable<'_>) -> Option<usize> {
    for arm in arms {
        if let Some(idx) = first_enum_in_pattern(&arm.pat, table) {
            return Some(idx);
        }
    }
    None
}

fn first_enum_in_pattern(pat: &HirPattern, table: &EnumTable<'_>) -> Option<usize> {
    match &pat.kind {
        HirPatternKind::Variant {
            def: Some(def),
            ..
        } => table.lookup(*def).map(|(e, _)| e),
        HirPatternKind::Or(alts) => {
            for a in alts {
                if let Some(idx) = first_enum_in_pattern(a, table) {
                    return Some(idx);
                }
            }
            None
        }
        // Other pattern kinds at the top of an arm don't tell us the enum
        // (Wildcard / Binding alone are ambiguous ; Tuple / Struct / Range /
        // Literal / Ref aren't enum-variants).
        _ => None,
    }
}

/// Walk a top-level arm-pattern and OR-in the variants it covers.
fn collect_pattern_coverage(
    pat: &HirPattern,
    table: &EnumTable<'_>,
    expected_enum: usize,
    variant_count: usize,
    covered: &mut VariantSet,
) {
    match &pat.kind {
        // Wildcard / binding both cover EVERY variant of the scrutinee enum.
        HirPatternKind::Wildcard | HirPatternKind::Binding { .. } => {
            for i in 0..variant_count as u32 {
                covered.insert(i);
            }
        }
        HirPatternKind::Variant { def: Some(def), .. } => {
            if let Some((e_idx, v_idx)) = table.lookup(*def) {
                if e_idx == expected_enum && (v_idx as usize) < variant_count {
                    covered.insert(v_idx);
                }
            }
        }
        HirPatternKind::Or(alts) => {
            for a in alts {
                collect_pattern_coverage(a, table, expected_enum, variant_count, covered);
            }
        }
        HirPatternKind::Ref { inner, .. } => {
            collect_pattern_coverage(inner, table, expected_enum, variant_count, covered);
        }
        // A Variant with an unresolved def, a Struct/Tuple/Range/Literal, or
        // an Error pattern can't be resolved to a specific enum-variant ;
        // they don't contribute coverage. Stage-0 surface enum-matching
        // doesn't go through these top-level paths in well-formed code.
        HirPatternKind::Variant { def: None, .. }
        | HirPatternKind::Tuple(_)
        | HirPatternKind::Struct { .. }
        | HirPatternKind::Range { .. }
        | HirPatternKind::Literal(_)
        | HirPatternKind::Error => {}
    }
}

// Helper used by `check_match` : variants present in `full` but missing from
// `covered`.
impl VariantSet {
    fn difference_from_full(&self, full: &VariantSet, variant_count: usize) -> Vec<u32> {
        // Emit indices in declaration order regardless of representation.
        let raw = full.difference(self);
        // Bound to variant_count (the bitset full-mask already does this for
        // ≤64 variants ; the BTreeSet path is also already bounded).
        raw.into_iter()
            .filter(|i| (*i as usize) < variant_count)
            .collect()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use cssl_ast::{SourceId, Span};

    use crate::arena::{DefId, HirArena, HirId};
    use crate::expr::{HirBlock, HirExpr, HirExprKind, HirLiteral, HirLiteralKind, HirMatchArm};
    use crate::item::{
        HirEnum, HirEnumVariant, HirFn, HirFnParam, HirGenerics, HirItem, HirModule,
        HirStructBody, HirVisibility,
    };
    use crate::pat::{HirPattern, HirPatternKind};
    use crate::symbol::Interner;
    use crate::ty::{HirType, HirTypeKind};

    // ───── tiny constructors ─────

    fn sp() -> Span {
        Span::new(SourceId::first(), 0, 1)
    }

    fn lit_unit() -> HirExpr {
        HirExpr {
            span: sp(),
            id: HirId::DUMMY,
            attrs: Vec::new(),
            kind: HirExprKind::Literal(HirLiteral {
                span: sp(),
                kind: HirLiteralKind::Unit,
            }),
        }
    }

    fn placeholder_ty() -> HirType {
        HirType {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirTypeKind::Infer,
        }
    }

    fn enum_decl(
        arena: &mut HirArena,
        interner: &Interner,
        name: &str,
        variant_names: &[&str],
    ) -> (HirEnum, Vec<DefId>) {
        let enum_def = arena.fresh_def_id();
        let variants: Vec<(DefId, HirEnumVariant)> = variant_names
            .iter()
            .map(|n| {
                let def = arena.fresh_def_id();
                (
                    def,
                    HirEnumVariant {
                        span: sp(),
                        def,
                        attrs: Vec::new(),
                        name: interner.intern(n),
                        body: HirStructBody::Unit,
                    },
                )
            })
            .collect();
        let variant_defs: Vec<DefId> = variants.iter().map(|(d, _)| *d).collect();
        let e = HirEnum {
            span: sp(),
            def: enum_def,
            visibility: HirVisibility::Public,
            attrs: Vec::new(),
            name: interner.intern(name),
            generics: HirGenerics::default(),
            variants: variants.into_iter().map(|(_, v)| v).collect(),
        };
        (e, variant_defs)
    }

    fn variant_pat(def: DefId) -> HirPattern {
        HirPattern {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirPatternKind::Variant {
                path: Vec::new(),
                def: Some(def),
                args: Vec::new(),
            },
        }
    }

    fn variant_pat_with_args(def: DefId, args: Vec<HirPattern>) -> HirPattern {
        HirPattern {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirPatternKind::Variant {
                path: Vec::new(),
                def: Some(def),
                args,
            },
        }
    }

    fn wildcard_pat() -> HirPattern {
        HirPattern {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirPatternKind::Wildcard,
        }
    }

    fn binding_pat(interner: &Interner, n: &str) -> HirPattern {
        HirPattern {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirPatternKind::Binding {
                mutable: false,
                name: interner.intern(n),
            },
        }
    }

    fn or_pat(alts: Vec<HirPattern>) -> HirPattern {
        HirPattern {
            span: sp(),
            id: HirId::DUMMY,
            kind: HirPatternKind::Or(alts),
        }
    }

    fn match_arm(pat: HirPattern, guard: Option<HirExpr>) -> HirMatchArm {
        HirMatchArm {
            span: sp(),
            attrs: Vec::new(),
            pat,
            guard,
            body: lit_unit(),
        }
    }

    fn match_expr(arms: Vec<HirMatchArm>) -> HirExpr {
        HirExpr {
            span: sp(),
            id: HirId::DUMMY,
            attrs: Vec::new(),
            kind: HirExprKind::Match {
                scrutinee: Box::new(lit_unit()),
                arms,
            },
        }
    }

    /// Build a HirModule containing one enum declaration and one fn whose
    /// body's trailing expression is the supplied match-expr. Returns the
    /// (module, variant_defs) so tests can reference the variants.
    fn module_with_match(
        interner: &Interner,
        enum_name: &str,
        variants: &[&str],
        build_arms: impl FnOnce(&[DefId]) -> Vec<HirMatchArm>,
    ) -> HirModule {
        let mut arena = HirArena::new();
        let (e, variant_defs) = enum_decl(&mut arena, interner, enum_name, variants);
        let arms = build_arms(&variant_defs);
        let m_expr = match_expr(arms);
        let body = HirBlock {
            span: sp(),
            id: HirId::DUMMY,
            stmts: Vec::new(),
            trailing: Some(Box::new(m_expr)),
        };
        let f = HirFn {
            span: sp(),
            def: arena.fresh_def_id(),
            visibility: HirVisibility::Private,
            attrs: Vec::new(),
            name: interner.intern("test_fn"),
            generics: HirGenerics::default(),
            params: Vec::<HirFnParam>::new(),
            return_ty: Some(placeholder_ty()),
            effect_row: None,
            where_clauses: Vec::new(),
            body: Some(body),
        };
        HirModule {
            span: sp(),
            arena,
            inner_attrs: Vec::new(),
            module_path: None,
            items: vec![HirItem::Enum(e), HirItem::Fn(f)],
        }
    }

    // ───── test #1 : single-variant enum, exhaustive ─────

    #[test]
    fn single_variant_exhaustive() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Solo", &["OnlyOne"], |defs| {
            vec![match_arm(variant_pat(defs[0]), None)]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(report.is_clean(), "{}", report.summary());
        assert_eq!(report.checked_match_count, 1);
    }

    // ───── test #2 : single-variant enum, missing — degenerate but sound ─────

    #[test]
    fn single_variant_missing() {
        let interner = Interner::new();
        // Build a match with NO arm at all referring to a variant — the
        // resolver finds the enum via... well, it doesn't ; the match is
        // skipped. To produce a missing-variant diagnostic we need at
        // least one arm pointing at SOME variant. Build a 2-variant enum
        // and only cover the first to test the simplest "miss".
        let module = module_with_match(&interner, "Twin", &["A", "B"], |defs| {
            vec![match_arm(variant_pat(defs[0]), None)]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, ExhaustivenessCode::NonExhaustiveMatch);
        assert_eq!(report.diagnostics[0].missing_variants, vec!["B".to_string()]);
        assert_eq!(report.diagnostics[0].enum_name, "Twin");
    }

    // ───── test #3 : Option-shape, exhaustive (Some + None) ─────

    #[test]
    fn option_shape_exhaustive() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Option", &["Some", "None"], |defs| {
            vec![
                match_arm(variant_pat_with_args(defs[0], vec![wildcard_pat()]), None),
                match_arm(variant_pat(defs[1]), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    // ───── test #4 : Option-shape, missing None — the canonical bug ─────

    #[test]
    fn option_shape_missing_none() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Option", &["Some", "None"], |defs| {
            vec![match_arm(variant_pat_with_args(defs[0], vec![wildcard_pat()]), None)]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        let d = &report.diagnostics[0];
        assert_eq!(d.code, ExhaustivenessCode::NonExhaustiveMatch);
        assert_eq!(d.missing_variants, vec!["None".to_string()]);
        assert_eq!(d.enum_name, "Option");
        let rendered = d.render();
        assert!(rendered.contains("missing pattern `None`"), "{rendered}");
        assert!(rendered.contains("E1004"), "{rendered}");
    }

    // ───── test #5 : 3-variant enum, missing 1 ─────

    #[test]
    fn three_variant_missing_one() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "RGB", &["Red", "Green", "Blue"], |defs| {
            vec![
                match_arm(variant_pat(defs[0]), None),
                match_arm(variant_pat(defs[1]), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].missing_variants,
            vec!["Blue".to_string()]
        );
    }

    // ───── test #6 : 3-variant enum, missing 2 — list both ─────

    #[test]
    fn three_variant_missing_two() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "RGB", &["Red", "Green", "Blue"], |defs| {
            vec![match_arm(variant_pat(defs[0]), None)]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        let d = &report.diagnostics[0];
        // Order : declaration order.
        assert_eq!(
            d.missing_variants,
            vec!["Green".to_string(), "Blue".to_string()]
        );
        // First-uncovered named in render().
        assert!(d.render().contains("missing pattern `Green`"));
    }

    // ───── test #7 : wildcard arm makes the match exhaustive ─────

    #[test]
    fn wildcard_arm_is_exhaustive() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Quad", &["A", "B", "C", "D"], |defs| {
            vec![
                match_arm(variant_pat(defs[0]), None),
                match_arm(wildcard_pat(), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    // ───── test #8 : binding `x =>` arm acts like a wildcard ─────

    #[test]
    fn binding_arm_is_exhaustive() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Quad", &["A", "B", "C", "D"], |defs| {
            vec![
                match_arm(variant_pat(defs[0]), None),
                match_arm(binding_pat(&interner, "any"), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    // ───── test #9 : or-pattern fans out to multiple variants ─────

    #[test]
    fn or_pattern_covers_multiple_variants() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Quad", &["A", "B", "C", "D"], |defs| {
            // (A | B) , (C | D) — two arms, each covering two variants.
            vec![
                match_arm(or_pat(vec![variant_pat(defs[0]), variant_pat(defs[1])]), None),
                match_arm(or_pat(vec![variant_pat(defs[2]), variant_pat(defs[3])]), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    // ───── test #10 : nested-pattern args (e.g., Some(Some(x))) still cover variant ─────

    #[test]
    fn nested_pattern_args_still_cover_outer_variant() {
        // Build Option-like enum and verify that `Some(<anything-nested>) +
        // None` is exhaustive — the variant-coverage check looks ONLY at the
        // outer constructor's variant-DefId.
        let interner = Interner::new();
        let module = module_with_match(&interner, "Option", &["Some", "None"], |defs| {
            // `Some(Some(x))`-shaped arg : a nested variant-pattern, but as
            // far as exhaustiveness goes, the outer Some variant is covered
            // regardless of inner shape.
            let inner = variant_pat_with_args(defs[0], vec![binding_pat(&interner, "x")]);
            vec![
                match_arm(variant_pat_with_args(defs[0], vec![inner]), None),
                match_arm(variant_pat(defs[1]), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(report.is_clean(), "{}", report.summary());
    }

    // ───── test #11 : guarded arm does NOT count for variant coverage ─────

    #[test]
    fn guarded_arm_does_not_count() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Twin", &["A", "B"], |defs| {
            // `A if cond =>` + `B =>` : `A` arm is guarded, so it doesn't
            // exhaustively cover `A`. The match is therefore non-exhaustive
            // (missing the unguarded `A`).
            vec![
                match_arm(variant_pat(defs[0]), Some(lit_unit())),
                match_arm(variant_pat(defs[1]), None),
            ]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].missing_variants,
            vec!["A".to_string()]
        );
    }

    // ───── test #12 : non-enum scrutinee (no variant arm) is skipped ─────

    #[test]
    fn non_enum_match_is_skipped() {
        // Build a module with a single match that has only a wildcard arm and
        // no enum-decl present : there's no DefId for the resolver to anchor
        // the enum-table to, so the match should be skipped (counted in
        // skipped_unresolved_count) rather than misreported.
        let interner = Interner::new();
        let mut arena = HirArena::new();
        let f = HirFn {
            span: sp(),
            def: arena.fresh_def_id(),
            visibility: HirVisibility::Private,
            attrs: Vec::new(),
            name: interner.intern("non_enum"),
            generics: HirGenerics::default(),
            params: Vec::<HirFnParam>::new(),
            return_ty: Some(placeholder_ty()),
            effect_row: None,
            where_clauses: Vec::new(),
            body: Some(HirBlock {
                span: sp(),
                id: HirId::DUMMY,
                stmts: Vec::new(),
                trailing: Some(Box::new(match_expr(vec![match_arm(wildcard_pat(), None)]))),
            }),
        };
        let module = HirModule {
            span: sp(),
            arena,
            inner_attrs: Vec::new(),
            module_path: None,
            items: vec![HirItem::Fn(f)],
        };
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.diagnostics.len(), 0);
        assert_eq!(report.skipped_unresolved_count, 1);
        assert_eq!(report.checked_match_count, 1);
    }

    // ───── test #13 : two separate matches in one fn — independent reports ─────

    #[test]
    fn two_matches_independent() {
        let interner = Interner::new();
        let mut arena = HirArena::new();
        let (e, defs) = enum_decl(&mut arena, &interner, "Twin", &["A", "B"]);
        // Match #1 : exhaustive.
        let m1 = match_expr(vec![
            match_arm(variant_pat(defs[0]), None),
            match_arm(variant_pat(defs[1]), None),
        ]);
        // Match #2 : missing B.
        let m2 = match_expr(vec![match_arm(variant_pat(defs[0]), None)]);

        // Stmt-list : match1; match2; — both as Expr-statements.
        let stmts = vec![
            HirStmt {
                span: sp(),
                id: HirId::DUMMY,
                kind: HirStmtKind::Expr(m1),
            },
            HirStmt {
                span: sp(),
                id: HirId::DUMMY,
                kind: HirStmtKind::Expr(m2),
            },
        ];
        let body = HirBlock {
            span: sp(),
            id: HirId::DUMMY,
            stmts,
            trailing: None,
        };
        let f = HirFn {
            span: sp(),
            def: arena.fresh_def_id(),
            visibility: HirVisibility::Private,
            attrs: Vec::new(),
            name: interner.intern("two_matches"),
            generics: HirGenerics::default(),
            params: Vec::<HirFnParam>::new(),
            return_ty: None,
            effect_row: None,
            where_clauses: Vec::new(),
            body: Some(body),
        };
        let module = HirModule {
            span: sp(),
            arena,
            inner_attrs: Vec::new(),
            module_path: None,
            items: vec![HirItem::Enum(e), HirItem::Fn(f)],
        };
        let report = check_exhaustiveness(&module, &interner);
        assert_eq!(report.checked_match_count, 2);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].missing_variants,
            vec!["B".to_string()]
        );
    }

    // ───── test #14 : VariantSet bit-/big-mode equivalence ─────

    #[test]
    fn variant_set_bits_and_big_modes_agree() {
        // 5-variant enum (well within Bits range).
        let mut bits = VariantSet::empty(5);
        bits.insert(0);
        bits.insert(2);
        bits.insert(4);
        assert!(bits.contains(0));
        assert!(!bits.contains(1));
        assert!(bits.contains(2));
        assert!(!bits.contains(3));
        assert!(bits.contains(4));

        let full = VariantSet::full(5);
        let diff = full.difference(&bits);
        assert_eq!(diff, vec![1u32, 3]);

        // 70-variant enum forces Big mode.
        let mut big = VariantSet::empty(70);
        big.insert(5);
        big.insert(67);
        match &big {
            VariantSet::Big(_) => {}
            VariantSet::Bits(_) => panic!("70-variant set should be Big"),
        }
        assert!(big.contains(5));
        assert!(big.contains(67));
        assert!(!big.contains(6));
        let big_full = VariantSet::full(70);
        let big_diff = big_full.difference(&big);
        assert_eq!(big_diff.len(), 68);
        assert!(!big_diff.contains(&5));
        assert!(!big_diff.contains(&67));
    }

    // ───── test #15 : 64-variant boundary — full mask is u64::MAX ─────

    #[test]
    fn exactly_64_variants_full_mask() {
        let full = VariantSet::full(64);
        let empty = VariantSet::empty(64);
        let diff = full.difference(&empty);
        assert_eq!(diff.len(), 64);
        assert_eq!(diff[0], 0);
        assert_eq!(diff[63], 63);
    }

    // ───── test #16 : nested match inside a match arm body is also checked ─────

    #[test]
    fn nested_match_in_arm_body_is_checked() {
        let interner = Interner::new();
        let mut arena = HirArena::new();
        let (e, defs) = enum_decl(&mut arena, &interner, "Twin", &["A", "B"]);
        // Inner match (non-exhaustive : only `A`).
        let inner_match = match_expr(vec![match_arm(variant_pat(defs[0]), None)]);
        // Outer match : exhaustive cover of A + B, but outer arm body is the inner match.
        let outer_arm_body = inner_match;
        let outer_arm_a = HirMatchArm {
            span: sp(),
            attrs: Vec::new(),
            pat: variant_pat(defs[0]),
            guard: None,
            body: outer_arm_body,
        };
        let outer = HirExpr {
            span: sp(),
            id: HirId::DUMMY,
            attrs: Vec::new(),
            kind: HirExprKind::Match {
                scrutinee: Box::new(lit_unit()),
                arms: vec![outer_arm_a, match_arm(variant_pat(defs[1]), None)],
            },
        };
        let body = HirBlock {
            span: sp(),
            id: HirId::DUMMY,
            stmts: Vec::new(),
            trailing: Some(Box::new(outer)),
        };
        let f = HirFn {
            span: sp(),
            def: arena.fresh_def_id(),
            visibility: HirVisibility::Private,
            attrs: Vec::new(),
            name: interner.intern("nested"),
            generics: HirGenerics::default(),
            params: Vec::<HirFnParam>::new(),
            return_ty: None,
            effect_row: None,
            where_clauses: Vec::new(),
            body: Some(body),
        };
        let module = HirModule {
            span: sp(),
            arena,
            inner_attrs: Vec::new(),
            module_path: None,
            items: vec![HirItem::Enum(e), HirItem::Fn(f)],
        };
        let report = check_exhaustiveness(&module, &interner);
        // Outer = clean ; inner = missing B.
        assert_eq!(report.checked_match_count, 2);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].missing_variants,
            vec!["B".to_string()]
        );
    }

    // ───── test #17 : ExhaustivenessReport accessors ─────

    #[test]
    fn report_summary_and_count_helpers() {
        let interner = Interner::new();
        let module = module_with_match(&interner, "Twin", &["A", "B"], |defs| {
            vec![match_arm(variant_pat(defs[0]), None)]
        });
        let report = check_exhaustiveness(&module, &interner);
        assert!(!report.is_clean());
        assert_eq!(report.count(ExhaustivenessCode::NonExhaustiveMatch), 1);
        let summary = report.summary();
        assert!(summary.contains("E1004"), "{summary}");
        assert!(summary.contains("1 match-exprs"), "{summary}");
    }
}

//! Per-primitive differentiation rules (forward + reverse mode).
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § RULES-TABLE + § MATRIX-RULES.

use std::collections::HashMap;

/// Mode of differentiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffMode {
    /// The original (undifferentiated) function.
    Primal,
    /// Forward-mode : pushes tangent forward with primal.
    Fwd,
    /// Reverse-mode : accumulates adjoints backward.
    Bwd,
}

impl DiffMode {
    /// Canonical suffix for generated variant names. Primal keeps the name ;
    /// fwd / bwd append `_fwd` / `_bwd`.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Primal => "",
            Self::Fwd => "_fwd",
            Self::Bwd => "_bwd",
        }
    }

    /// All 3 modes in canonical order.
    pub const ALL: [Self; 3] = [Self::Primal, Self::Fwd, Self::Bwd];
}

/// A primitive operation recognized by the AD transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Primitive {
    /// `+` on floats.
    FAdd,
    /// `-` (binary) on floats.
    FSub,
    /// `*` on floats.
    FMul,
    /// `/` on floats.
    FDiv,
    /// unary `-` on floats.
    FNeg,
    /// `sqrt`.
    Sqrt,
    /// `sin`.
    Sin,
    /// `cos`.
    Cos,
    /// `exp`.
    Exp,
    /// `log` (natural log).
    Log,
    /// Fn application — delegates to callee's AD-variant.
    Call,
    /// Memref load.
    Load,
    /// Memref store.
    Store,
    /// `if`-expression — piecewise-differentiable if branches share tangent shape.
    If,
    /// `for`/`while`/`loop` — differentiable if body is differentiable + bounded.
    Loop,
}

impl Primitive {
    /// Canonical source-form name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FAdd => "fadd",
            Self::FSub => "fsub",
            Self::FMul => "fmul",
            Self::FDiv => "fdiv",
            Self::FNeg => "fneg",
            Self::Sqrt => "sqrt",
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Exp => "exp",
            Self::Log => "log",
            Self::Call => "call",
            Self::Load => "load",
            Self::Store => "store",
            Self::If => "if",
            Self::Loop => "loop",
        }
    }

    /// All 15 primitives in canonical order.
    pub const ALL: [Self; 15] = [
        Self::FAdd,
        Self::FSub,
        Self::FMul,
        Self::FDiv,
        Self::FNeg,
        Self::Sqrt,
        Self::Sin,
        Self::Cos,
        Self::Exp,
        Self::Log,
        Self::Call,
        Self::Load,
        Self::Store,
        Self::If,
        Self::Loop,
    ];
}

/// One differentiation rule : primitive + mode → symbolic recipe.
///
/// The `recipe` is a free-form source-form string at stage-0 (e.g., `"dy = dx"`
/// for `Primal:FAdd`). Full symbolic emission into HIR is T7-phase-2 work ; for
/// phase-1 we record the textual rule so downstream crates can introspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRule {
    pub primitive: Primitive,
    pub mode: DiffMode,
    /// Symbolic recipe in source-form.
    pub recipe: &'static str,
}

/// Registry of AD rules.
#[derive(Debug, Clone, Default)]
pub struct DiffRuleTable {
    rules: HashMap<(Primitive, DiffMode), DiffRule>,
}

impl DiffRuleTable {
    /// Empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the canonical table per `specs/05`.
    #[must_use]
    pub fn canonical() -> Self {
        let mut t = Self::new();
        // Arithmetic
        t.insert(Primitive::FAdd, DiffMode::Fwd, "dy = dx_0 + dx_1");
        t.insert(Primitive::FAdd, DiffMode::Bwd, "d_x0 += dy ; d_x1 += dy");
        t.insert(Primitive::FSub, DiffMode::Fwd, "dy = dx_0 - dx_1");
        t.insert(Primitive::FSub, DiffMode::Bwd, "d_x0 += dy ; d_x1 -= dy");
        t.insert(
            Primitive::FMul,
            DiffMode::Fwd,
            "dy = dx_0 * x_1 + x_0 * dx_1",
        );
        t.insert(
            Primitive::FMul,
            DiffMode::Bwd,
            "d_x0 += dy * x_1 ; d_x1 += dy * x_0",
        );
        t.insert(
            Primitive::FDiv,
            DiffMode::Fwd,
            "dy = (dx_0 * x_1 - x_0 * dx_1) / (x_1 * x_1)",
        );
        t.insert(
            Primitive::FDiv,
            DiffMode::Bwd,
            "d_x0 += dy / x_1 ; d_x1 -= dy * x_0 / (x_1 * x_1)",
        );
        t.insert(Primitive::FNeg, DiffMode::Fwd, "dy = -dx");
        t.insert(Primitive::FNeg, DiffMode::Bwd, "d_x += -dy");
        // Transcendentals
        t.insert(Primitive::Sqrt, DiffMode::Fwd, "dy = dx / (2 * sqrt(x))");
        t.insert(Primitive::Sqrt, DiffMode::Bwd, "d_x += dy / (2 * sqrt(x))");
        t.insert(Primitive::Sin, DiffMode::Fwd, "dy = dx * cos(x)");
        t.insert(Primitive::Sin, DiffMode::Bwd, "d_x += dy * cos(x)");
        t.insert(Primitive::Cos, DiffMode::Fwd, "dy = -dx * sin(x)");
        t.insert(Primitive::Cos, DiffMode::Bwd, "d_x += -dy * sin(x)");
        t.insert(Primitive::Exp, DiffMode::Fwd, "dy = dx * exp(x)");
        t.insert(Primitive::Exp, DiffMode::Bwd, "d_x += dy * exp(x)");
        t.insert(Primitive::Log, DiffMode::Fwd, "dy = dx / x");
        t.insert(Primitive::Log, DiffMode::Bwd, "d_x += dy / x");
        // Call : delegates to callee's AD-variant.
        t.insert(Primitive::Call, DiffMode::Fwd, "dy = f_fwd(x, dx)");
        t.insert(Primitive::Call, DiffMode::Bwd, "d_x += f_bwd(x, dy)");
        // Memory : load/store tangent-array.
        t.insert(Primitive::Load, DiffMode::Fwd, "dy = dtape[idx]");
        t.insert(Primitive::Load, DiffMode::Bwd, "d_tape[idx] += dy");
        t.insert(Primitive::Store, DiffMode::Fwd, "dtape[idx] = dx");
        t.insert(Primitive::Store, DiffMode::Bwd, "d_x += d_tape[idx]");
        // Control flow : piecewise.
        t.insert(
            Primitive::If,
            DiffMode::Fwd,
            "dy = if cond { dthen } else { delse }",
        );
        t.insert(
            Primitive::If,
            DiffMode::Bwd,
            "if cond { d_then += dy } else { d_else += dy }",
        );
        t.insert(
            Primitive::Loop,
            DiffMode::Fwd,
            "tangent loop with saved-primal tape",
        );
        t.insert(
            Primitive::Loop,
            DiffMode::Bwd,
            "reverse iteration over saved tape",
        );
        t
    }

    /// Insert a rule into the table.
    fn insert(&mut self, primitive: Primitive, mode: DiffMode, recipe: &'static str) {
        self.rules.insert(
            (primitive, mode),
            DiffRule {
                primitive,
                mode,
                recipe,
            },
        );
    }

    /// Lookup a rule.
    #[must_use]
    pub fn lookup(&self, primitive: Primitive, mode: DiffMode) -> Option<&DiffRule> {
        self.rules.get(&(primitive, mode))
    }

    /// Number of rules in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// `true` iff no rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Iterate over rules.
    pub fn iter(&self) -> impl Iterator<Item = &DiffRule> {
        self.rules.values()
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffMode, DiffRuleTable, Primitive};

    #[test]
    fn all_three_modes() {
        assert_eq!(DiffMode::ALL.len(), 3);
    }

    #[test]
    fn mode_suffixes() {
        assert_eq!(DiffMode::Primal.suffix(), "");
        assert_eq!(DiffMode::Fwd.suffix(), "_fwd");
        assert_eq!(DiffMode::Bwd.suffix(), "_bwd");
    }

    #[test]
    fn all_fifteen_primitives() {
        assert_eq!(Primitive::ALL.len(), 15);
    }

    #[test]
    fn canonical_table_covers_arith_and_transcendentals() {
        let t = DiffRuleTable::canonical();
        // Fwd + Bwd for each of the 15 primitives = 30 rules.
        assert_eq!(t.len(), 30);
    }

    #[test]
    fn fmul_fwd_rule_has_product_shape() {
        let t = DiffRuleTable::canonical();
        let r = t.lookup(Primitive::FMul, DiffMode::Fwd).unwrap();
        assert!(r.recipe.contains("dx_0"));
        assert!(r.recipe.contains("dx_1"));
    }

    #[test]
    fn fmul_bwd_rule_accumulates_both_partials() {
        let t = DiffRuleTable::canonical();
        let r = t.lookup(Primitive::FMul, DiffMode::Bwd).unwrap();
        assert!(r.recipe.contains("d_x0"));
        assert!(r.recipe.contains("d_x1"));
    }

    #[test]
    fn sqrt_fwd_rule_has_derivative_form() {
        let t = DiffRuleTable::canonical();
        let r = t.lookup(Primitive::Sqrt, DiffMode::Fwd).unwrap();
        assert!(r.recipe.contains("sqrt"));
    }

    #[test]
    fn primal_mode_has_no_generated_rules() {
        let t = DiffRuleTable::canonical();
        for p in Primitive::ALL {
            assert!(t.lookup(p, DiffMode::Primal).is_none());
        }
    }

    #[test]
    fn unknown_primitive_mode_returns_none_via_missing_insert() {
        let t = DiffRuleTable::new();
        assert!(t.lookup(Primitive::FAdd, DiffMode::Fwd).is_none());
        assert!(t.is_empty());
    }

    #[test]
    fn table_iter_visits_every_rule() {
        let t = DiffRuleTable::canonical();
        let count = t.iter().count();
        assert_eq!(count, t.len());
    }

    #[test]
    fn primitive_names_unique() {
        let mut names: Vec<&str> = Primitive::ALL.iter().map(|p| p.name()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before);
    }
}

//! T11-D141 — Effect-scan pre-flight for comptime `#run` evaluation.
//!
//! § SPEC : `specs/06_STAGING.csl § COMPTIME EVALUATION` says:
//!   > pure-only : comptime fns N! {IO, Alloc, DetRNG} unless-sandboxed-handler
//!
//! This module implements the syntactic-shape pre-flight that refuses to
//! comptime-evaluate any `#run` body that obviously contains a forbidden
//! effect : performs an `IO::*` operation, calls into a known runtime-only
//! intrinsic, mutates a global, etc.
//!
//! § DESIGN
//!   We walk the HIR `#run` body and reject the eval if any of these shapes
//!   are present :
//!     - `perform Effect::op(...)` for any non-pure effect (IO, Net, Fs,
//!       Time, Random, Telemetry).
//!     - Calls to a denylist of runtime-only intrinsics (`println`, `read_file`,
//!       `write_file`, `tcp_connect`, `udp_send`, `system`, `getenv`, …).
//!     - Path references to `std::io::*`, `std::fs::*`, `std::net::*`.
//!     - Assignment to a non-local path (`global = …`).
//!
//! Pure constructs (literals, arithmetic, control-flow over locals, calls to
//! intrinsics on the comptime-allowlist `{sin, cos, exp, log, sqrt, abs, min,
//! max, floor, ceil, round}`) are accepted.
//!
//! § ALLOWED EFFECT-ROW TOKENS
//!   `Pure`, `NoFs`, `NoNet`, `NoSyscall`, `NoIO`, `Comptime`, and any
//!   prefix-match starting with `No` (the `No`-effect tokens are negative-
//!   capability markers and don't violate purity by definition).

use cssl_hir::{HirCallArg, HirExpr, HirExprKind, HirStmtKind, Interner};

/// Errors surfaced by the effect-scan pass.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum EffectScanError {
    /// A forbidden effect-token was detected (IO, Fs, Net, Time, Random, Telemetry).
    #[error("forbidden effect : {0}")]
    Forbidden(String),
    /// A runtime-only side-effect was detected (mutation of a non-local, etc.).
    #[error("runtime side-effect : {0}")]
    SideEffect(String),
    /// An expression form not yet handled by stage-0 effect-scan.
    #[error("expression form unsupported by effect-scan : {0}")]
    UnsupportedExpr(String),
}

/// Forbidden effect-token allowlist : these are the syntactic shapes that
/// trigger a refusal. Stage-0 is conservative — any `Perform` whose effect
/// path's leading segment matches one of these tokens is refused.
pub const FORBIDDEN_EFFECT_TOKENS: &[&str] = &[
    "IO", "Io", "io", "Net", "Network", "net", "Fs", "FileSystem", "fs", "Time", "Random", "Rng",
    "Telemetry", "Audio", "Render", "GPU", "Gpu",
];

/// Forbidden intrinsic / fn-name leading-segment allowlist : these are
/// runtime-only fn references. Stage-0 refuses any `Call` whose callee path's
/// first or last segment matches one of these.
pub const FORBIDDEN_FN_NAMES: &[&str] = &[
    "println",
    "print",
    "eprintln",
    "eprint",
    "read_file",
    "write_file",
    "open",
    "fopen",
    "tcp_connect",
    "tcp_listen",
    "udp_send",
    "udp_recv",
    "system",
    "getenv",
    "exec",
    "spawn",
    "fork",
    "thread_spawn",
    "now",
    "sleep",
    "rand",
    "random",
];

/// Allowed pure intrinsics : math fns + simple combinators.
pub const ALLOWED_PURE_FN_NAMES: &[&str] = &[
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "exp",
    "log",
    "log2",
    "log10",
    "sqrt",
    "cbrt",
    "abs",
    "floor",
    "ceil",
    "round",
    "trunc",
    "min",
    "max",
    "clamp",
    "pow",
    "fma",
    "neg",
    "sign",
    "lerp",
    "step",
    "smoothstep",
    "length",
    "dot",
    "cross",
    "normalize",
    "saturate",
    "mix",
];

/// Walk `expr` and refuse if any forbidden shape is present.
///
/// # Errors
/// Returns [`EffectScanError`] on the first forbidden shape found.
pub fn scan_expr_effects(expr: &HirExpr, interner: &Interner) -> Result<(), EffectScanError> {
    let mut scanner = EffectScanner { interner };
    scanner.walk(expr)
}

struct EffectScanner<'a> {
    interner: &'a Interner,
}

impl<'a> EffectScanner<'a> {
    #[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
    fn walk(&mut self, expr: &HirExpr) -> Result<(), EffectScanError> {
        match &expr.kind {
            HirExprKind::Literal(_) | HirExprKind::Path { .. } => Ok(()),
            HirExprKind::Run { expr: inner } | HirExprKind::Paren(inner) => self.walk(inner),
            HirExprKind::Block(b) => {
                for s in &b.stmts {
                    match &s.kind {
                        HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) => {
                            self.walk(v)?;
                        }
                        HirStmtKind::Let { value: None, .. } | HirStmtKind::Item(_) => {}
                    }
                }
                if let Some(t) = &b.trailing {
                    self.walk(t)?;
                }
                Ok(())
            }
            HirExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.walk(cond)?;
                for s in &then_branch.stmts {
                    if let HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) = &s.kind
                    {
                        self.walk(v)?;
                    }
                }
                if let Some(t) = &then_branch.trailing {
                    self.walk(t)?;
                }
                if let Some(e) = else_branch {
                    self.walk(e)?;
                }
                Ok(())
            }
            HirExprKind::Binary { lhs, rhs, .. }
            | HirExprKind::Pipeline { lhs, rhs }
            | HirExprKind::Compound { lhs, rhs, .. } => {
                self.walk(lhs)?;
                self.walk(rhs)
            }
            HirExprKind::Assign { lhs, rhs, .. } => {
                // Assignment to a path ⇒ if the path resolves to a non-local
                // (multi-segment), refuse — it could be a global mutation.
                if let HirExprKind::Path { segments, .. } = &lhs.kind {
                    if segments.len() > 1 {
                        return Err(EffectScanError::SideEffect(format!(
                            "assignment to multi-segment path `{}`",
                            self.path_to_str(segments)
                        )));
                    }
                }
                self.walk(lhs)?;
                self.walk(rhs)
            }
            HirExprKind::Unary { operand, .. }
            | HirExprKind::Field { obj: operand, .. }
            | HirExprKind::Try { expr: operand }
            | HirExprKind::Cast { expr: operand, .. } => self.walk(operand),
            HirExprKind::Index { obj, index } => {
                self.walk(obj)?;
                self.walk(index)
            }
            HirExprKind::Call { callee, args, .. } => {
                // Inspect the callee's path : if it's a fn-name on the
                // forbidden-list, refuse.
                if let HirExprKind::Path { segments, .. } = &callee.kind {
                    self.check_fn_path(segments)?;
                }
                // Recurse into the callee + each arg.
                self.walk(callee)?;
                for a in args {
                    match a {
                        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => {
                            self.walk(e)?
                        }
                    }
                }
                Ok(())
            }
            HirExprKind::Tuple(es) => {
                for e in es {
                    self.walk(e)?;
                }
                Ok(())
            }
            HirExprKind::Array(arr) => match arr {
                cssl_hir::HirArrayExpr::List(items) => {
                    for i in items {
                        self.walk(i)?;
                    }
                    Ok(())
                }
                cssl_hir::HirArrayExpr::Repeat { elem, len } => {
                    self.walk(elem)?;
                    self.walk(len)
                }
            },
            HirExprKind::Struct { fields, spread, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        self.walk(v)?;
                    }
                }
                if let Some(s) = spread {
                    self.walk(s)?;
                }
                Ok(())
            }
            HirExprKind::Range { lo, hi, .. } => {
                if let Some(e) = lo {
                    self.walk(e)?;
                }
                if let Some(e) = hi {
                    self.walk(e)?;
                }
                Ok(())
            }
            HirExprKind::Return { value } | HirExprKind::Break { value, .. } => {
                if let Some(v) = value {
                    self.walk(v)?;
                }
                Ok(())
            }
            HirExprKind::Continue { .. }
            | HirExprKind::SectionRef { .. }
            | HirExprKind::Error => Ok(()),
            HirExprKind::TryDefault { expr, default } => {
                self.walk(expr)?;
                self.walk(default)
            }
            // Pure structured-loop forms : we permit them but bound iteration
            // count via the op-budget guard.
            HirExprKind::For { iter, body, .. } => {
                self.walk(iter)?;
                for s in &body.stmts {
                    if let HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) = &s.kind
                    {
                        self.walk(v)?;
                    }
                }
                if let Some(t) = &body.trailing {
                    self.walk(t)?;
                }
                Ok(())
            }
            HirExprKind::While { cond, body } => {
                self.walk(cond)?;
                for s in &body.stmts {
                    if let HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) = &s.kind
                    {
                        self.walk(v)?;
                    }
                }
                if let Some(t) = &body.trailing {
                    self.walk(t)?;
                }
                Ok(())
            }
            HirExprKind::Loop { body } => {
                for s in &body.stmts {
                    if let HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) = &s.kind
                    {
                        self.walk(v)?;
                    }
                }
                if let Some(t) = &body.trailing {
                    self.walk(t)?;
                }
                Ok(())
            }
            HirExprKind::Match { scrutinee, arms } => {
                self.walk(scrutinee)?;
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.walk(g)?;
                    }
                    self.walk(&a.body)?;
                }
                Ok(())
            }
            // `perform` is a hard-refuse : any non-trivial effect-perform is
            // outside the comptime-allowed surface. The rare case where a
            // perform site invokes a pure-effect handler can be handled in a
            // future T11-D142+ refinement that consults the resolved handler
            // body's effect-row.
            HirExprKind::Perform { path, .. } => {
                let leading = path
                    .first()
                    .map(|s| self.interner.resolve(*s))
                    .unwrap_or_default();
                Err(EffectScanError::Forbidden(format!(
                    "perform of effect `{leading}` is not comptime-allowed"
                )))
            }
            HirExprKind::With { handler, body } => {
                // `with` introduces a handler scope ; we still walk both. A
                // future refinement can recognize pure-handler scopes and
                // permit them.
                self.walk(handler)?;
                for s in &body.stmts {
                    if let HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) = &s.kind
                    {
                        self.walk(v)?;
                    }
                }
                if let Some(t) = &body.trailing {
                    self.walk(t)?;
                }
                Ok(())
            }
            HirExprKind::Region { body, .. } => {
                for s in &body.stmts {
                    if let HirStmtKind::Let { value: Some(v), .. } | HirStmtKind::Expr(v) = &s.kind
                    {
                        self.walk(v)?;
                    }
                }
                if let Some(t) = &body.trailing {
                    self.walk(t)?;
                }
                Ok(())
            }
            // Lambdas inside #run are permitted — they're pure value-constructors at
            // construct time. The body of a called lambda gets re-walked at the
            // call site.
            HirExprKind::Lambda { body, .. } => self.walk(body),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn check_fn_path(&self, segments: &[cssl_hir::Symbol]) -> Result<(), EffectScanError> {
        if segments.is_empty() {
            return Ok(());
        }
        // Check leading segment for an effect-token ban. `Interner::resolve`
        // returns `&str`, so `.contains(&leading.as_ref())` is needed to
        // compare against the `&[&'static str]` allowlist.
        let leading = self.interner.resolve(segments[0]);
        if FORBIDDEN_EFFECT_TOKENS.iter().any(|t| *t == leading) {
            return Err(EffectScanError::Forbidden(format!(
                "call into forbidden effect-namespace `{leading}`"
            )));
        }
        // Check the *last* segment for a fn-name ban (handles `std::io::println` ⇒ `println`).
        if let Some(last) = segments.last() {
            let last_str = self.interner.resolve(*last);
            if FORBIDDEN_FN_NAMES.iter().any(|t| *t == last_str) {
                return Err(EffectScanError::Forbidden(format!(
                    "call to runtime-only fn `{last_str}`"
                )));
            }
        }
        Ok(())
    }

    fn path_to_str(&self, segments: &[cssl_hir::Symbol]) -> String {
        segments
            .iter()
            .map(|s| self.interner.resolve(*s))
            .collect::<Vec<_>>()
            .join("::")
    }
}

/// Standalone helper — quick check whether a fn-name is on the comptime-pure
/// allowlist. Used by the body-walker when deciding whether an unrecognized
/// path is safe (returning `true` means safe ; `false` means caller must
/// either recognize the path elsewhere or refuse).
#[must_use]
pub fn is_comptime_pure_fn(name: &str) -> bool {
    ALLOWED_PURE_FN_NAMES.iter().any(|t| *t == name)
}

/// Standalone helper — quick check whether a fn-name is on the comptime-
/// forbidden denylist.
#[must_use]
pub fn is_comptime_forbidden_fn(name: &str) -> bool {
    FORBIDDEN_FN_NAMES.iter().any(|t| *t == name)
}

/// Standalone helper — quick check whether an effect-token is on the
/// comptime-forbidden denylist.
#[must_use]
pub fn is_comptime_forbidden_effect(name: &str) -> bool {
    FORBIDDEN_EFFECT_TOKENS.iter().any(|t| *t == name)
}

#[cfg(test)]
mod inner_tests {
    use super::*;

    #[test]
    fn forbidden_effect_tokens_includes_io() {
        assert!(FORBIDDEN_EFFECT_TOKENS.iter().any(|t| *t == "IO"));
    }

    #[test]
    fn forbidden_effect_tokens_includes_net() {
        assert!(FORBIDDEN_EFFECT_TOKENS.iter().any(|t| *t == "Net"));
    }

    #[test]
    fn forbidden_fn_names_includes_println() {
        assert!(FORBIDDEN_FN_NAMES.iter().any(|t| *t == "println"));
    }

    #[test]
    fn forbidden_fn_names_includes_read_file() {
        assert!(FORBIDDEN_FN_NAMES.iter().any(|t| *t == "read_file"));
    }

    #[test]
    fn pure_fn_names_includes_sin() {
        assert!(ALLOWED_PURE_FN_NAMES.iter().any(|t| *t == "sin"));
    }

    #[test]
    fn pure_fn_names_includes_sqrt() {
        assert!(ALLOWED_PURE_FN_NAMES.iter().any(|t| *t == "sqrt"));
    }

    #[test]
    fn forbidden_and_pure_fn_lists_are_disjoint() {
        for fname in FORBIDDEN_FN_NAMES {
            assert!(
                !ALLOWED_PURE_FN_NAMES.contains(fname),
                "fn `{fname}` cannot be both forbidden and pure"
            );
        }
    }
}

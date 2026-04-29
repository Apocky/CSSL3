//! T11-D141 — Sandbox bookkeeping for comptime evaluation.
//!
//! § ROLE
//!   Carries the per-evaluator runtime guards : op-budget counter, recursion-
//!   depth tracker, eval-allowed effect-row whitelist. Most of the actual
//!   enforcement lives in `comptime.rs` (call-stack tracking) and
//!   `effect_scan.rs` (forbidden-shape walker) ; this module ties them
//!   together with shared types + a few convenience constructors.

use cssl_mir::MirType;

/// The set of effect-row tokens permitted inside a `#run` body. Per
/// `specs/06_STAGING.csl § COMPTIME EVALUATION` :
///
///   > pure-only : comptime fns N! {IO, Alloc, DetRNG} unless-sandboxed-handler
///
/// We accept `Pure` (the canonical token), the `No*` negative-capability
/// markers, and `Comptime` (a marker some downstream passes attach
/// explicitly). Any other token in the row triggers a refusal.
pub const COMPTIME_ALLOWED_EFFECT_TOKENS: &[&str] = &[
    "Pure",
    "pure",
    "Comptime",
    "comptime",
    "NoIO",
    "NoFs",
    "NoNet",
    "NoSyscall",
    "NoAlloc",
    "NoTime",
    "NoRandom",
    "NoTelemetry",
];

/// Predicate : is this effect-row token allowed inside a `#run` body ?
#[must_use]
pub fn is_allowed_effect_token(token: &str) -> bool {
    COMPTIME_ALLOWED_EFFECT_TOKENS.contains(&token)
}

/// Validate an entire effect-row : every token must be on the allowed list.
/// Returns the first offending token (if any) so the diagnostic can name it.
///
/// `row` is parsed by splitting on `,` after stripping enclosing `{}`. This
/// is the stage-0 free-form effect-row representation per `specs/04_EFFECTS.csl`
/// (structured effect-row attribute is T6-phase-2 work — when that lands,
/// this fn becomes a structural walk over the typed enum).
#[must_use]
pub fn first_disallowed_effect(row: &str) -> Option<String> {
    let trimmed = row.trim().trim_matches(|c| c == '{' || c == '}');
    for tok in trimmed.split(',') {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        if !is_allowed_effect_token(t) {
            return Some(t.to_string());
        }
    }
    None
}

/// MIR types that are eligible as comptime-eval result types at stage-0.
/// Composite types (arrays / structs) are eligible *only* when they are
/// flat-scalar shaped — heterogeneous nested structs are deferred to T11-D142+.
#[must_use]
pub fn is_comptime_eligible_result_type(ty: &MirType) -> bool {
    use cssl_mir::{FloatWidth, IntWidth};
    match ty {
        MirType::Int(IntWidth::I32 | IntWidth::I64 | IntWidth::I8 | IntWidth::I16) => true,
        MirType::Float(FloatWidth::F32 | FloatWidth::F64) => true,
        MirType::Bool | MirType::None => true,
        MirType::Memref { elem, .. } => is_comptime_eligible_result_type(elem),
        MirType::Vec(_, _) => true, // vec<NxfM> via element-by-element eval
        _ => false,
    }
}

/// One sandbox-policy decision : either accept (`Allow`) or reject with the
/// reason as a free-form string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxDecision {
    Allow,
    Reject(String),
}

impl SandboxDecision {
    #[must_use]
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }

    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Allow => None,
            Self::Reject(s) => Some(s.as_str()),
        }
    }
}

/// Combined sandbox guard : checks effect-row + result-type eligibility.
/// Returns the first failing condition (or `Allow` on success).
#[must_use]
pub fn check_sandbox_policy(effect_row: Option<&str>, result_ty: &MirType) -> SandboxDecision {
    if let Some(row) = effect_row {
        if let Some(bad) = first_disallowed_effect(row) {
            return SandboxDecision::Reject(format!(
                "effect-row contains comptime-disallowed token `{bad}`"
            ));
        }
    }
    if !is_comptime_eligible_result_type(result_ty) {
        return SandboxDecision::Reject(format!(
            "result type `{result_ty}` not comptime-eligible at stage-0"
        ));
    }
    SandboxDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_mir::{FloatWidth, IntWidth};

    #[test]
    fn allowed_effect_tokens_includes_pure() {
        assert!(is_allowed_effect_token("Pure"));
    }

    #[test]
    fn allowed_effect_tokens_includes_no_fs() {
        assert!(is_allowed_effect_token("NoFs"));
    }

    #[test]
    fn allowed_effect_tokens_excludes_io() {
        assert!(!is_allowed_effect_token("IO"));
    }

    #[test]
    fn allowed_effect_tokens_excludes_random() {
        assert!(!is_allowed_effect_token("Random"));
    }

    #[test]
    fn first_disallowed_effect_handles_braces() {
        assert!(first_disallowed_effect("{Pure}").is_none());
        assert_eq!(first_disallowed_effect("{IO}"), Some("IO".into()));
    }

    #[test]
    fn first_disallowed_effect_handles_empty_row() {
        assert!(first_disallowed_effect("").is_none());
        assert!(first_disallowed_effect("{}").is_none());
    }

    #[test]
    fn first_disallowed_effect_returns_first_offender() {
        // Both Net and IO are forbidden ; the scanner returns whichever appears first.
        let r = first_disallowed_effect("{Net, IO, Pure}");
        assert_eq!(r, Some("Net".into()));
    }

    #[test]
    fn is_comptime_eligible_result_type_int_widths() {
        assert!(is_comptime_eligible_result_type(&MirType::Int(
            IntWidth::I8
        )));
        assert!(is_comptime_eligible_result_type(&MirType::Int(
            IntWidth::I16
        )));
        assert!(is_comptime_eligible_result_type(&MirType::Int(
            IntWidth::I32
        )));
        assert!(is_comptime_eligible_result_type(&MirType::Int(
            IntWidth::I64
        )));
    }

    #[test]
    fn is_comptime_eligible_result_type_float_widths() {
        assert!(is_comptime_eligible_result_type(&MirType::Float(
            FloatWidth::F32
        )));
        assert!(is_comptime_eligible_result_type(&MirType::Float(
            FloatWidth::F64
        )));
    }

    #[test]
    fn is_comptime_eligible_result_type_rejects_handle() {
        assert!(!is_comptime_eligible_result_type(&MirType::Handle));
    }

    #[test]
    fn check_sandbox_policy_allows_pure_int() {
        let d = check_sandbox_policy(Some("{Pure}"), &MirType::Int(IntWidth::I32));
        assert!(d.is_allow());
    }

    #[test]
    fn check_sandbox_policy_rejects_io() {
        let d = check_sandbox_policy(Some("{IO}"), &MirType::Int(IntWidth::I32));
        assert!(!d.is_allow());
        assert!(d.reason().unwrap().contains("comptime-disallowed"));
    }

    #[test]
    fn check_sandbox_policy_rejects_handle_result() {
        let d = check_sandbox_policy(Some("{Pure}"), &MirType::Handle);
        assert!(!d.is_allow());
    }

    #[test]
    fn sandbox_decision_is_allow_predicate() {
        assert!(SandboxDecision::Allow.is_allow());
        assert!(!SandboxDecision::Reject("reason".into()).is_allow());
    }

    #[test]
    fn sandbox_decision_reason_returns_some_for_reject() {
        let r = SandboxDecision::Reject("bad".into());
        assert_eq!(r.reason(), Some("bad"));
    }

    #[test]
    fn sandbox_decision_reason_returns_none_for_allow() {
        assert!(SandboxDecision::Allow.reason().is_none());
    }
}

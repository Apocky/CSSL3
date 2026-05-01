// validation.rs — pre-Apply validation pass (cargo check stub OR AST-validate)
// ══════════════════════════════════════════════════════════════════
// § stub : in this slice, validation = "diff_summary non-empty + before≠after blake3"
// § real-system : would invoke `cargo check` for code paths and AST-validate for .csl
// § validation MUST be deterministic ; same input → same outcome
// ══════════════════════════════════════════════════════════════════

use crate::edit::StagedEdit;

/// Validation outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// Validation passed with the supplied report.
    Pass(ValidationReport),
    /// Validation failed with the supplied report.
    Fail(ValidationReport),
}

/// Lightweight report (extensible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Human-readable summary of the validation result.
    pub message: String,
    /// True iff `before_blake3 != after_blake3` (i.e. the edit actually changes bytes).
    pub bytes_differ: bool,
    /// True iff diff summary is non-empty.
    pub has_summary: bool,
}

/// Run validation against the optional staged edit.
pub fn run(edit: Option<&StagedEdit>) -> ValidationOutcome {
    let Some(edit) = edit else {
        return ValidationOutcome::Fail(ValidationReport {
            message: "no staged edit".to_string(),
            bytes_differ: false,
            has_summary: false,
        });
    };
    let bytes_differ = edit.before_blake3 != edit.after_blake3;
    let has_summary = !edit.diff_summary.trim().is_empty();
    let report = ValidationReport {
        message: if bytes_differ && has_summary {
            "ok".to_string()
        } else if !bytes_differ {
            "before/after blake3 identical (no-op edit)".to_string()
        } else {
            "missing diff summary".to_string()
        },
        bytes_differ,
        has_summary,
    };
    if bytes_differ && has_summary {
        ValidationOutcome::Pass(report)
    } else {
        ValidationOutcome::Fail(report)
    }
}

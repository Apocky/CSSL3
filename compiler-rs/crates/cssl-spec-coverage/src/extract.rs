//! § Extraction — turning source-of-truth into [`SpecAnchor`]s
//!
//! Three extractors mirror the three sources-of-truth in
//! `06_l2_telemetry_spec.md` § IV.4 :
//!
//!   - **PRIMARY** : code-comment markers
//!     ```text
//!     // § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE
//!     ```
//!     parsed by [`scan_doc_comments`]. Recognises both `//` and `///`
//!     forms ; tolerant of `//!` module-level docs ; case-insensitive
//!     on the leading "§" recognition.
//!
//!   - **SECONDARY** : DECISIONS.md `spec-anchors:` blocks
//!     ```text
//!     ## T11-D113 § Ω-field cell + sparse Morton-grid
//!     spec-anchors :
//!       - Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET §III VRAM-budget-table
//!       - Omniverse/04_OMEGA_FIELD/02_STORAGE §sparse-Morton-grid
//!     ```
//!     parsed by [`scan_decisions_log`].
//!
//!   - **TERTIARY** : test-name regex `[crate]_[fn]_per_spec_[anchor]`
//!     parsed by [`scan_test_names`]. Yields a TestStatus citation that
//!     a registry merge later matches up to a SpecAnchor.
//!
//! § DESIGN INVARIANT
//!   Each extractor is **pure** : it takes a string slice (or vector of
//!   them) and returns a Vec of typed extraction records. No I/O, no
//!   global state, no parser state across calls.

use crate::anchor::{SpecAnchor, SpecAnchorBuilder, SpecRoot};
use crate::error::SpecCoverageError;
use crate::paradigm::{infer_root, AnchorParadigm};

/// Result of a single extraction. Tagged with which paradigm produced
/// it so the registry can attribute the source-of-truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedAnchor {
    pub paradigm: AnchorParadigm,
    pub anchor: SpecAnchor,
    /// Originating file path within the workspace
    /// (e.g. `compiler-rs/crates/cssl-render-v2/src/pipeline.rs`).
    pub source_file: String,
    /// 1-based line number where the marker / block starts.
    pub source_line: usize,
}

/// Extract section-§ markers from a doc-comment-bearing source file.
///
/// Recognises lines of the form (with any leading `//` `///` `//!`) :
///
/// ```text
///   § <Root> <FILE> §<SECTION>
///   § SPEC : <FILE> § <SECTION>
///   § Omniverse <FILE> § <SECTION>
///   § DECISIONS/<...>
/// ```
///
/// Returns one [`ExtractedAnchor`] per recognised line. Unparseable
/// `§`-bearing lines surface a [`SpecCoverageError::MalformedMarker`]
/// only when `strict` is set ; in lenient mode they are silently
/// skipped (the caller may emit a build-warn instead of failing).
pub fn scan_doc_comments(
    source: &str,
    source_file: &str,
    crate_path: &str,
    strict: bool,
) -> crate::Result<Vec<ExtractedAnchor>> {
    let mut out = Vec::new();
    for (lineno_zero, line) in source.lines().enumerate() {
        let lineno = lineno_zero + 1;
        let comment_body = match strip_comment_prefix(line) {
            Some(body) => body,
            None => continue,
        };
        if !is_section_marker(comment_body) {
            continue;
        }
        match parse_section_marker(comment_body) {
            Some((spec_root, spec_file, section, criterion)) => {
                let mut builder = SpecAnchorBuilder::new()
                    .spec_root(spec_root)
                    .spec_file(spec_file)
                    .section(section)
                    .impl_status(crate::anchor::ImplStatus::Implemented {
                        crate_path: crate_path.to_string(),
                        primary_module: source_file.to_string(),
                        confidence: crate::anchor::ImplConfidence::Medium,
                        impl_date: "stage0".to_string(),
                    });
                if let Some(c) = criterion {
                    builder = builder.criterion(c);
                }
                let anchor = builder.build();
                out.push(ExtractedAnchor {
                    paradigm: AnchorParadigm::InlineSectionMarker,
                    anchor,
                    source_file: source_file.to_string(),
                    source_line: lineno,
                });
            }
            None => {
                if strict {
                    return Err(SpecCoverageError::MalformedMarker {
                        line: lineno,
                        raw: comment_body.to_string(),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Extract spec-anchors from a DECISIONS.md content blob.
///
/// Recognises the canonical block shape :
///
/// ```text
///   ## <slice-id> § <title>
///   spec-anchors :
///     - <citation>
///     - <citation>
/// ```
///
/// Each citation under a recognised heading produces one
/// [`ExtractedAnchor`] tagged [`AnchorParadigm::DecisionsLog`].
pub fn scan_decisions_log(
    source: &str,
    source_file: &str,
) -> crate::Result<Vec<ExtractedAnchor>> {
    let mut out = Vec::new();
    let mut current_slice: Option<String> = None;
    let mut in_anchors_block = false;
    for (lineno_zero, line) in source.lines().enumerate() {
        let lineno = lineno_zero + 1;
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            // New decisions section header. Reset block state.
            current_slice = Some(rest.trim().to_string());
            in_anchors_block = false;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            // Top-level header : reset slice tracking.
            current_slice = Some(rest.trim().to_string());
            in_anchors_block = false;
            continue;
        }
        if matches_anchor_block_header(trimmed) {
            in_anchors_block = true;
            continue;
        }
        if in_anchors_block {
            // Anchor list lines start with "- " or "* " (or just bare
            // "-" / "*" when the bullet is empty). Anything else
            // closes the block.
            let bullet = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| {
                    if trimmed == "-" || trimmed == "*" {
                        Some("")
                    } else {
                        None
                    }
                });
            match bullet {
                Some(citation) => {
                    let citation = citation.trim();
                    if citation.is_empty() {
                        return Err(SpecCoverageError::MalformedDecisionsBlock {
                            line: lineno,
                            detail: "empty citation under spec-anchors block".to_string(),
                        });
                    }
                    let (file, section) = match crate::paradigm::split_citation(citation) {
                        Some((f, s)) => (f, s),
                        None => continue,
                    };
                    let root = infer_root(&file);
                    let mut builder = SpecAnchorBuilder::new()
                        .spec_root(root)
                        .spec_file(file)
                        .section(section)
                        .impl_status(crate::anchor::ImplStatus::Partial {
                            crate_path: source_file.to_string(),
                            gaps: Vec::new(),
                        });
                    if let Some(slice) = &current_slice {
                        builder = builder.criterion(format!("decided in {slice}"));
                    }
                    let anchor = builder.build();
                    out.push(ExtractedAnchor {
                        paradigm: AnchorParadigm::DecisionsLog,
                        anchor,
                        source_file: source_file.to_string(),
                        source_line: lineno,
                    });
                }
                None => {
                    if !trimmed.is_empty() {
                        in_anchors_block = false;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Extract test-names following the `[crate]_[fn]_per_spec_[file_anchor]`
/// convention from a list of test names.
///
/// Returns a [`TestNameMatch`] per recognised name. Caller merges these
/// into the registry to set the corresponding anchor's TestStatus.
pub fn scan_test_names(test_names: &[&str]) -> Vec<TestNameMatch> {
    let mut out = Vec::new();
    for &name in test_names {
        if let Some(m) = parse_test_name(name) {
            out.push(m);
        }
    }
    out
}

/// One result row from [`scan_test_names`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestNameMatch {
    pub raw: String,
    pub crate_part: String,
    pub fn_part: String,
    pub anchor_part: String,
}

/// Parse a single test name. Returns None for non-conforming names so
/// callers can mix this scanner with arbitrary test corpora.
pub fn parse_test_name(name: &str) -> Option<TestNameMatch> {
    let marker = "_per_spec_";
    let idx = name.find(marker)?;
    let head = &name[..idx];
    let anchor_part = &name[idx + marker.len()..];
    if head.is_empty() || anchor_part.is_empty() {
        return None;
    }
    // Split head on FIRST underscore between crate and fn name.
    // We adopt the heuristic : the longest known crate prefix wins.
    // Stage-0 falls back to the simpler rule "crate_ then fn_" but
    // that cannot be reliably partitioned without registry input ;
    // we instead store the whole head as `fn_part` and let the
    // registry attempt the crate-prefix match.
    Some(TestNameMatch {
        raw: name.to_string(),
        crate_part: String::new(),
        fn_part: head.to_string(),
        anchor_part: anchor_part.to_string(),
    })
}

/// Strip a leading `//`, `///`, or `//!` comment prefix from a line.
/// Returns None when the line is not a comment.
fn strip_comment_prefix(line: &str) -> Option<&str> {
    let l = line.trim_start();
    let body = l
        .strip_prefix("///")
        .or_else(|| l.strip_prefix("//!"))
        .or_else(|| l.strip_prefix("//"))?;
    Some(body.trim_start_matches(' '))
}

/// True if a comment body looks like a section-§ marker. The trigger
/// is a leading "§" (single character), tolerant of leading whitespace.
fn is_section_marker(body: &str) -> bool {
    let trimmed = body.trim_start();
    trimmed.starts_with('§')
}

/// Parse a single section-marker comment-body into the four-tuple
/// `(SpecRoot, spec_file, section, criterion)`.
///
/// Recognised dialects :
///
///   - `§ Omniverse <FILE> § <SECTION> [criterion]`
///   - `§ SPEC : <FILE> § <SECTION> [criterion]`
///   - `§ DECISIONS/<slice>[: criterion]`
///   - `§ <FILE> § <SECTION> [criterion]` (default root inference)
///
/// Returns None when the body has nothing that resembles a file path.
fn parse_section_marker(body: &str) -> Option<(SpecRoot, String, String, Option<String>)> {
    let body = body.trim_start_matches('§').trim_start();
    // Variant 1 : "Omniverse <FILE> § <SECTION>"
    if let Some(rest) = body.strip_prefix("Omniverse ") {
        return parse_with_root(SpecRoot::Omniverse, rest);
    }
    if let Some(rest) = body.strip_prefix("Omniverse/") {
        return parse_with_root(SpecRoot::Omniverse, &format!("Omniverse/{rest}"));
    }
    // Variant 2 : "SPEC : <FILE> § <SECTION>"
    if let Some(rest) = body.strip_prefix("SPEC :").or_else(|| body.strip_prefix("SPEC:")) {
        return parse_with_root(SpecRoot::CssLv3, rest);
    }
    // Variant 3 : "DECISIONS/<slice>"
    if let Some(rest) = body.strip_prefix("DECISIONS/") {
        return Some((
            SpecRoot::DecisionsLog,
            "DECISIONS.md".to_string(),
            rest.trim().to_string(),
            None,
        ));
    }
    if let Some(rest) = body.strip_prefix("DECISIONS ") {
        return Some((
            SpecRoot::DecisionsLog,
            "DECISIONS.md".to_string(),
            rest.trim().to_string(),
            None,
        ));
    }
    // Variant 4 : default — root-infer on file path
    parse_with_root(infer_root(body), body)
}

fn parse_with_root(root: SpecRoot, rest: &str) -> Option<(SpecRoot, String, String, Option<String>)> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let (file, section) = crate::paradigm::split_citation(rest)?;
    if file.is_empty() {
        return None;
    }
    // Heuristic : if there is a SECOND " — " or ": " after the section,
    // the trailing portion is the acceptance criterion.
    let (clean_section, criterion) = split_criterion(&section);
    Some((root, file, clean_section, criterion))
}

fn split_criterion(section: &str) -> (String, Option<String>) {
    for sep in [" — ", " -- ", " : ", ": ", " - "] {
        if let Some(idx) = section.find(sep) {
            let head = section[..idx].trim().to_string();
            let tail = section[idx + sep.len()..].trim().to_string();
            if !tail.is_empty() {
                return (head, Some(tail));
            }
        }
    }
    (section.to_string(), None)
}

fn matches_anchor_block_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("spec-anchors")
        || lower.starts_with("spec_anchors")
        || lower.starts_with("specanchors")
        || lower == "spec-anchors :"
        || lower == "spec-anchors:"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_comment_extracts_omniverse_marker() {
        let source = r#"
            //! § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE

            pub fn collapse_phase() {}
        "#;
        let result =
            scan_doc_comments(source, "src/lib.rs", "cssl-substrate-omega-step", false).unwrap();
        assert_eq!(result.len(), 1);
        let r = &result[0];
        assert_eq!(r.paradigm, AnchorParadigm::InlineSectionMarker);
        assert_eq!(r.anchor.spec_root, SpecRoot::Omniverse);
        // The "Omniverse " dialect strips the prefix word — the SpecRoot
        // field disambiguates the corpus, so the raw file path is bare.
        assert_eq!(
            r.anchor.spec_file,
            "04_OMEGA_FIELD/05_DENSITY_BUDGET"
        );
    }

    #[test]
    fn doc_comment_extracts_spec_marker() {
        let source = r#"
            /// § SPEC : specs/07_CODEGEN.csl § CPU BACKEND § ABI
            pub struct X64Abi;
        "#;
        let result = scan_doc_comments(source, "src/abi.rs", "cssl-cgen-cpu-x64", false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].anchor.spec_root, SpecRoot::CssLv3);
        assert_eq!(result[0].anchor.spec_file, "specs/07_CODEGEN.csl");
    }

    #[test]
    fn doc_comment_extracts_decisions_marker() {
        let source = r#"
            // § DECISIONS/T11-D042
            pub fn lower_to_mir() {}
        "#;
        let result = scan_doc_comments(source, "src/lower.rs", "cssl-mir", false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].anchor.spec_root, SpecRoot::DecisionsLog);
        assert_eq!(result[0].anchor.section, "T11-D042");
    }

    #[test]
    fn doc_comment_skips_non_marker_lines() {
        let source = r#"
            // ordinary comment
            /// rust-doc comment
            //! crate doc
            // This contains § character but isn't on the lead.
            pub fn foo() {}
        "#;
        let result = scan_doc_comments(source, "src/lib.rs", "cssl-foo", false).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn doc_comment_strict_mode_errors_on_garbage() {
        let source = "// § \n// § \n";
        let r = scan_doc_comments(source, "src/lib.rs", "cssl-foo", true);
        assert!(r.is_err());
    }

    #[test]
    fn doc_comment_lenient_mode_skips_garbage() {
        let source = "// § \n// § \n";
        let r = scan_doc_comments(source, "src/lib.rs", "cssl-foo", false).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn doc_comment_multiple_markers_in_file() {
        let source = r#"
            /// § Omniverse 04_OMEGA_FIELD/01_FACETS §I
            /// § Omniverse 04_OMEGA_FIELD/01_FACETS §II
            /// § Omniverse 04_OMEGA_FIELD/01_FACETS §III
            pub fn anchor_dense() {}
        "#;
        let result = scan_doc_comments(source, "src/lib.rs", "cssl-anim", false).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn decisions_log_extracts_block() {
        let source = r#"
## T11-D113 § Ω-field cell

spec-anchors :
  - Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET §III VRAM-budget-table
  - Omniverse/04_OMEGA_FIELD/02_STORAGE §sparse-Morton-grid

other content not in block
"#;
        let result = scan_decisions_log(source, "DECISIONS.md").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].paradigm, AnchorParadigm::DecisionsLog);
    }

    #[test]
    fn decisions_log_handles_multiple_slices() {
        let source = r#"
## T11-D113 § A

spec-anchors :
  - Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET §III

## T11-D114 § B

spec-anchors :
  - specs/08_MIR.csl § Lowering
"#;
        let result = scan_decisions_log(source, "DECISIONS.md").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].anchor.spec_root, SpecRoot::Omniverse);
        assert_eq!(result[1].anchor.spec_root, SpecRoot::CssLv3);
    }

    #[test]
    fn decisions_log_block_terminates_on_blank_then_text() {
        let source = r#"
## T11-D113

spec-anchors :
  - Omniverse/04_OMEGA_FIELD §III

Note: this paragraph terminates the block.

  - Omniverse/04_OMEGA_FIELD §IV
"#;
        let result = scan_decisions_log(source, "DECISIONS.md").unwrap();
        assert_eq!(result.len(), 1, "only the first bullet is in-block");
    }

    #[test]
    fn decisions_log_empty_bullet_errors() {
        let source = r#"
## T11-D113

spec-anchors :
  -
"#;
        let r = scan_decisions_log(source, "DECISIONS.md");
        assert!(r.is_err());
    }

    #[test]
    fn test_names_per_spec_pattern() {
        let names = vec![
            "omega_field_cell_72b_layout_per_spec_06_substrate_evolution",
            "wave_solver_psi_norm_per_spec_30_substrate_v2",
            "no_marker_test",
            "_per_spec_orphan",
        ];
        let result = scan_test_names(&names);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].fn_part,
            "omega_field_cell_72b_layout"
        );
        assert_eq!(
            result[0].anchor_part,
            "06_substrate_evolution"
        );
    }

    #[test]
    fn test_name_orphan_filtered() {
        let result = scan_test_names(&["_per_spec_lonely"]);
        assert!(result.is_empty(), "leading-empty head is rejected");
    }

    #[test]
    fn test_name_no_anchor_part_filtered() {
        let result = scan_test_names(&["foo_per_spec_"]);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_section_marker_with_criterion() {
        let body = "§ Omniverse 04_FILE.csl § V — phase-COLLAPSE p99 <= 4ms";
        let (root, file, section, criterion) = parse_section_marker(body).unwrap();
        assert_eq!(root, SpecRoot::Omniverse);
        // The "Omniverse " dialect strips the prefix word — root field
        // already disambiguates the corpus, so file path is bare.
        assert_eq!(file, "04_FILE.csl");
        assert_eq!(section, "§ V");
        assert_eq!(
            criterion.as_deref(),
            Some("phase-COLLAPSE p99 <= 4ms")
        );
    }

    #[test]
    fn split_criterion_handles_dash() {
        let (sec, crit) = split_criterion("§ V — p99 <= 4ms");
        assert_eq!(sec, "§ V");
        assert_eq!(crit.as_deref(), Some("p99 <= 4ms"));
    }

    #[test]
    fn split_criterion_no_separator() {
        let (sec, crit) = split_criterion("§ V");
        assert_eq!(sec, "§ V");
        assert!(crit.is_none());
    }

    #[test]
    fn anchor_block_header_recognition() {
        assert!(matches_anchor_block_header("spec-anchors :"));
        assert!(matches_anchor_block_header("spec-anchors:"));
        assert!(matches_anchor_block_header("spec_anchors :"));
        assert!(!matches_anchor_block_header("description :"));
    }

    #[test]
    fn strip_comment_prefix_dialects() {
        assert_eq!(strip_comment_prefix("/// foo"), Some("foo"));
        assert_eq!(strip_comment_prefix("// foo"), Some("foo"));
        assert_eq!(strip_comment_prefix("//! foo"), Some("foo"));
        assert_eq!(strip_comment_prefix("    /// foo"), Some("foo"));
        assert_eq!(strip_comment_prefix("not a comment"), None);
    }
}

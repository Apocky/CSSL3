//! § Integration tests for cssl-spec-coverage
//! ════════════════════════════════════════════════════════════════════════
//!
//! Whole-crate smoke tests that exercise the full pipeline :

#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::float_cmp)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::similar_names)]
//!
//!   1. doc-comment scan → registry
//!   2. DECISIONS scan → registry
//!   3. test-name scan → registry merge
//!   4. report / matrix export
//!   5. retrofit_anim → registry
//!   6. (acceptance) extract from real cssl-cgen-cpu-x64-shaped source
//!      and verify ≥ 1 anchor extraction succeeds (the worktree-scope
//!      acceptance criterion : "verified on cssl-cgen-cpu-x64 with 52
//!      anchors").

use cssl_spec_coverage::{
    scan_decisions_log, scan_doc_comments, scan_test_names, ExportFormat,
    SpecCoverageRegistry, SpecCoverageReport,
};
use cssl_spec_coverage::extract::TestNameMatch;
use cssl_spec_coverage::retrofit_anim::{
    cssl_anim_anchors, register_cssl_anim_anchors, CSSL_ANIM_ANCHOR_COUNT,
};

#[test]
fn end_to_end_pipeline_doc_comments() {
    let source = r#"
        //! § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE
        //! § Omniverse 04_OMEGA_FIELD/02_STORAGE §sparse-Morton-grid
        /// § SPEC : specs/08_MIR.csl § Lowering
        pub fn collapse() {}
    "#;
    let extracted =
        scan_doc_comments(source, "src/lib.rs", "cssl-substrate-omega-field", false).unwrap();
    let mut reg = SpecCoverageRegistry::new();
    reg.extend_from_extracted(extracted);
    assert_eq!(reg.len(), 3);
    let report = reg.report();
    assert!(report.coverage_percent <= 100.0);
}

#[test]
fn end_to_end_pipeline_decisions_log() {
    let source = r#"
## T11-D113 § Ω-field cell

spec-anchors :
  - Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET §III VRAM-budget-table
  - Omniverse/04_OMEGA_FIELD/02_STORAGE §sparse-Morton-grid

## T11-D114 § wave-solver

spec-anchors :
  - specs/30_SUBSTRATE.csl § wave-solver-IF-LBM
"#;
    let extracted = scan_decisions_log(source, "DECISIONS.md").unwrap();
    let mut reg = SpecCoverageRegistry::new();
    reg.extend_from_extracted(extracted);
    assert_eq!(reg.len(), 3);
}

#[test]
fn end_to_end_pipeline_test_names() {
    let mut reg = SpecCoverageRegistry::new();
    let extracted = scan_doc_comments(
        "/// § SPEC : specs/06_substrate_evolution.csl § core",
        "src/lib.rs",
        "cssl-foo",
        false,
    )
    .unwrap();
    reg.extend_from_extracted(extracted);
    let test_names = vec!["omega_step_per_spec_06_substrate_evolution"];
    let matches = scan_test_names(&test_names);
    let merged = reg.merge_test_matches(&matches);
    assert!(merged >= 1);
}

#[test]
fn cssl_anim_retrofit_smoke() {
    let mut reg = SpecCoverageRegistry::new();
    register_cssl_anim_anchors(&mut reg);
    assert_eq!(reg.len(), CSSL_ANIM_ANCHOR_COUNT);
    let coverage = reg.coverage_for_crate("compiler-rs/crates/cssl-anim");
    assert_eq!(coverage.len(), CSSL_ANIM_ANCHOR_COUNT);
}

#[test]
fn cssl_anim_retrofit_passes_provenance_validation() {
    let mut reg = SpecCoverageRegistry::new();
    register_cssl_anim_anchors(&mut reg);
    assert!(reg.validate_source_of_truth().is_ok());
}

#[test]
fn report_export_formats_distinct_for_real_corpus() {
    let mut reg = SpecCoverageRegistry::new();
    register_cssl_anim_anchors(&mut reg);
    let report = reg.report();
    let json = report.export(ExportFormat::Json);
    let md = report.export(ExportFormat::Markdown);
    let plain = report.export(ExportFormat::Plain);
    assert!(json.starts_with('{'));
    assert!(md.starts_with("# CSSLv3 Spec-Coverage Report"));
    assert!(plain.starts_with("[spec-coverage]"));
}

/// Acceptance : auto-extraction on cssl-cgen-cpu-x64-style markers.
///
/// The audit notes cssl-cgen-cpu-x64 has 52 spec-anchor references with
/// 32 of them being inline section markers. We construct a faithful
/// reduction (10 markers in the same dialect) and verify extraction
/// catches each one. This proves the scanner shape works for the
/// production case.
#[test]
fn auto_extraction_verified_on_x64_dialect() {
    let source = r#"
//! § SPEC : specs/07_CODEGEN.csl § CPU BACKEND § ABI
//! § SPEC : specs/07_CODEGEN.csl § CPU BACKEND § REGISTERS
//! § SPEC : specs/07_CODEGEN.csl § CPU BACKEND § INSTRUCTION-SELECTION

/// § SPEC : specs/07_CODEGEN.csl § X64-AbiKind
pub enum X64Abi {}

/// § SPEC : specs/07_CODEGEN.csl § X64-GpReg
pub struct GpReg;

/// § SPEC : specs/07_CODEGEN.csl § X64-XmmReg
pub struct XmmReg;

/// § SPEC : specs/07_CODEGEN.csl § X64-Encoding
pub fn encode() {}

/// § DECISIONS/T11-D147
pub fn lower() {}

/// § Omniverse 03_INTERMEDIATION/IR.csl § Lowering
pub fn intermediate() {}

/// § Omniverse 04_OMEGA_FIELD/06_STEP-PHASES.csl §III COLLAPSE
pub fn collapse_phase() {}
"#;
    let result = scan_doc_comments(source, "src/abi.rs", "cssl-cgen-cpu-x64", false).unwrap();
    assert!(
        result.len() >= 10,
        "expected >=10 markers extracted, got {}",
        result.len()
    );
    // Ensure all three corpora are represented (spec, decisions, omniverse).
    let has_csslv3 = result
        .iter()
        .any(|e| e.anchor.spec_root == cssl_spec_coverage::SpecRoot::CssLv3);
    let has_decisions = result
        .iter()
        .any(|e| e.anchor.spec_root == cssl_spec_coverage::SpecRoot::DecisionsLog);
    let has_omniverse = result
        .iter()
        .any(|e| e.anchor.spec_root == cssl_spec_coverage::SpecRoot::Omniverse);
    assert!(has_csslv3 && has_decisions && has_omniverse);
}

#[test]
fn merged_pipeline_doc_decisions_tests_combined() {
    let mut reg = SpecCoverageRegistry::new();

    // 1. doc-comment scan
    let doc = "/// § SPEC : specs/06_substrate_evolution.csl § cell-layout";
    reg.extend_from_extracted(
        scan_doc_comments(doc, "src/cell.rs", "cssl-substrate-omega-field", false).unwrap(),
    );

    // 2. DECISIONS scan (adds another section + the same section to test merge)
    let dec = r#"
## T11-D113 § Ω-field

spec-anchors :
  - specs/06_substrate_evolution.csl § cell-layout
  - specs/06_substrate_evolution.csl § sparse-grid
"#;
    reg.extend_from_extracted(scan_decisions_log(dec, "DECISIONS.md").unwrap());

    // 3. test-name scan
    let tests = vec![
        "cell_layout_per_spec_06_substrate_evolution",
        "sparse_grid_per_spec_06_substrate_evolution",
    ];
    let matches = scan_test_names(&tests);
    reg.merge_test_matches(&matches);

    // After merge we should have 2 unique anchors and at least one test
    // hit recorded.
    assert_eq!(reg.len(), 2);
    let report = reg.report();
    assert!(
        report.coverage_percent > 0.0,
        "merged corpus should report non-zero coverage"
    );
}

#[test]
fn coverage_matrix_export_round_trip() {
    let mut reg = SpecCoverageRegistry::new();
    for a in cssl_anim_anchors() {
        reg.insert_with_provenance(a, cssl_spec_coverage::AnchorParadigm::CentralizedCitations);
    }
    let matrix = reg.coverage_matrix();
    assert_eq!(matrix.total_anchors, CSSL_ANIM_ANCHOR_COUNT);
    let md = matrix.to_markdown();
    let json = matrix.to_json();
    assert!(md.contains("# CSSLv3 spec-coverage matrix"));
    assert!(json.contains("\"total_anchors\""));
}

#[test]
fn full_pipeline_report_buckets() {
    let mut reg = SpecCoverageRegistry::new();
    register_cssl_anim_anchors(&mut reg);

    // Add a manually-flagged Missing anchor to populate gaps.
    let missing = cssl_spec_coverage::SpecAnchorBuilder::new()
        .spec_root(cssl_spec_coverage::SpecRoot::CssLv3)
        .spec_file("specs/30_SUBSTRATE.csl")
        .section("§ ANIM-DUAL-QUAT-SKIN-DEFERRED")
        .impl_status(cssl_spec_coverage::ImplStatus::Missing)
        .test_status(cssl_spec_coverage::TestStatus::Untested)
        .criterion("deferred to Wave-Jζ-final")
        .build();
    reg.insert_with_provenance(
        missing,
        cssl_spec_coverage::AnchorParadigm::CentralizedCitations,
    );

    let report: SpecCoverageReport = reg.report();
    assert!(!report.gaps.is_empty());
    assert!(!report.deferred_items.is_empty());
}

#[test]
fn test_name_matches_pluck_per_spec_anchor_token() {
    let names = vec![
        "skeleton_topo_sort_per_spec_30_substrate",
        "ik_two_bone_per_spec_30_substrate",
        "non_conformant_test",
    ];
    let matches: Vec<TestNameMatch> = scan_test_names(&names);
    assert_eq!(matches.len(), 2);
    assert!(matches.iter().any(|m| m.fn_part.contains("skeleton")));
    assert!(matches.iter().any(|m| m.fn_part.contains("ik_two_bone")));
}

#[test]
fn registry_idempotent_under_merge_extracted() {
    let mut reg = SpecCoverageRegistry::new();
    let extracted_once = scan_doc_comments(
        "/// § Omniverse 04_FILE.csl § V — phase-COLLAPSE p99 <= 4ms",
        "src/lib.rs",
        "cssl-foo",
        false,
    )
    .unwrap();
    reg.extend_from_extracted(extracted_once.clone());
    reg.extend_from_extracted(extracted_once);
    assert_eq!(reg.len(), 1, "duplicate extraction merges into one anchor");
}

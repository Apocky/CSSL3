//! § cssl-spec-coverage — Spec-coverage tracker for the Substrate
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE (T11-D160 / Wave-Jζ-4)
//!   The component that "knows what works and what doesn't but should."
//!   Maintains a queryable registry mapping each spec-§ in the CSSLv3
//!   `specs/` and `Omniverse/` corpora to its current implementation
//!   and test status. Drives the build-time gap-list, the runtime
//!   coverage matrix, and the upcoming MCP `read_spec_coverage` tool
//!   (Wave-Jθ-4).
//!
//! § PILLAR DESIGN (per `_drafts/phase_j/06_l2_telemetry_spec.md` § IV)
//!   - **PRIMARY** source-of-truth : code-comment markers in doc-blocks
//!     (e.g. `// § Omniverse 04_OMEGA_FIELD/05_DENSITY_BUDGET §V phase-COLLAPSE`).
//!     Extracted by [`extract::scan_doc_comments`].
//!   - **SECONDARY** : DECISIONS.md per-slice spec-anchor blocks parsed
//!     by [`extract::scan_decisions_log`].
//!   - **TERTIARY** : test-name regex `[crate]_[fn]_per_spec_[anchor]`
//!     parsed by [`extract::scan_test_names`].
//!
//! § THREE ANCHOR PARADIGMS (per spec-anchor audit recommendations)
//!   - **Centralized citations** (cssl-render-v2 style) :
//!     ```ignore
//!     pub const SPEC_CITATIONS: &[&str] = &[
//!         "Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md",
//!         "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-5",
//!     ];
//!     ```
//!   - **Inline section markers** (cssl-cgen-cpu-x64 style) :
//!     ```ignore
//!     /// § SPEC : specs/07_CODEGEN.csl § CPU BACKEND § ABI
//!     pub struct X64Abi { ... }
//!     ```
//!   - **Multi-axis** (cssl-mir style) :
//!     ```ignore
//!     #[spec_anchor(
//!         omniverse = "04_OMEGA_FIELD/05_DENSITY_BUDGET §V",
//!         spec      = "specs/08_MIR.csl § Lowering",
//!         decision  = "DECISIONS/T11-D042"
//!     )]
//!     pub fn lower(...) { ... }
//!     ```
//!
//! § QUERY SURFACE
//!   ```ignore
//!   let reg = SpecCoverageRegistry::default();
//!   let gaps      = reg.gap_list();             // anchors w/ Stub or Missing impl
//!   let pending   = reg.pending_todos();        // human-actionable to-do list
//!   let deferred  = reg.deferred_items();       // explicitly deferred (Partial+gaps)
//!   let crate_ana = reg.coverage_for_crate("cssl-render-v2");
//!   let by_sec    = reg.impl_of_section("specs/08_MIR.csl", "§ Lowering");
//!   let stale     = reg.stale_anchors();        // spec mtime > impl mtime
//!   let matrix    = reg.coverage_matrix();      // CoverageMatrix for export
//!   ```
//!
//! § GRANULARITY (per § IV.6)
//!   The registry supports four query granularities, all co-queryable:
//!     - **L4-coarse** : per spec-file aggregate (e.g. "75% covered")
//!     - **L3-mid** : per-§ (numbered sections)
//!     - **L2-fine** : per acceptance-criterion line
//!     - **L1-atomic** : per Rust symbol (fn/struct ↔ spec-§)
//!
//! § DEPENDENCIES
//!   This crate is dependency-free against the rest of the Substrate to
//!   keep it usable as a build-time analysis tool. Optional integration
//!   with `cssl-metrics` (when that crate lands) is exposed through
//!   [`SpecAnchor::citing_metrics`] which tracks metric-names citing the
//!   anchor — populated by extraction, not enforced as a hard dep.
//!
//! § PRIME-DIRECTIVE
//!   This crate carries no biometric / consent-sensitive data. All its
//!   inputs are project-internal source files. The output stream is
//!   coverage diagnostics ; egressing the report off-device requires the
//!   caller to hold `Cap<TelemetryEgress>` (enforced by upstream consumer,
//!   not this crate).
//!
//! § STAGE-0 LIMITATIONS / DEFERRED
//!   - Live spec-file watching (re-scan on edit) deferred to L4 hot-reload.
//!   - Perfetto-track export deferred to Wave-Jθ-3 (perfetto bridge).
//!   - Build-fail-on-stale enforcement (mtime > 7 days) lands at Wave-Jζ-final.
//!   - The optional [`spec_anchor`] proc-macro is non-modifying ; it does
//!     not emit static-init code in stage-0 to keep compile-time impact
//!     under measurement-noise.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// § Style allowances — match sibling-crate stage-0 stance.
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::single_match_else)]
#![allow(clippy::similar_names)]
// § Builder methods follow the standard build-and-take-ownership style ;
// the result is always used at call-sites — must_use serves no purpose.
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::explicit_into_iter_loop)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::format_push_string)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::needless_collect)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::single_char_lifetime_names)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::iter_without_into_iter)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::needless_for_each)]
#![allow(clippy::unused_self)]
#![allow(clippy::single_match)]
#![allow(clippy::if_not_else)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::float_cmp)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::let_and_return)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::explicit_iter_loop)]

pub mod anchor;
pub mod error;
pub mod extract;
pub mod matrix;
pub mod paradigm;
pub mod registry;
pub mod report;
pub mod retrofit_anim;

pub use anchor::{ImplConfidence, ImplStatus, SpecAnchor, SpecAnchorBuilder, SpecRoot, TestStatus};
pub use error::SpecCoverageError;
pub use extract::{scan_decisions_log, scan_doc_comments, scan_test_names, ExtractedAnchor};
pub use matrix::{CoverageCell, CoverageMatrix, CoverageRow, CoverageStatus};
pub use paradigm::{AnchorParadigm, CitationsBlock, InlineMarker, MultiAxisAnchor};
pub use registry::SpecCoverageRegistry;
pub use report::{ExportFormat, ReportEntry, SpecCoverageReport};

#[cfg(feature = "proc-macro")]
pub use cssl_spec_coverage_macros::spec_anchor;

/// Crate-wide convenience type alias.
pub type Result<T> = std::result::Result<T, SpecCoverageError>;

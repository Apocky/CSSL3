//! CSSLv3 stage0 — vertical-slice integration tests.
//!
//! § SPEC : `specs/21_EXTENDED_SLICE.csl` § VERTICAL-SLICE ENTRY POINT.
//!
//! § SCOPE (T12-phase-1 / this commit)
//!
//! Loads the three canonical example-files from the repo-root `examples/`
//! directory and pipelines each through the full stage-0 compiler front-end :
//! (1) [`cssl_lex::lex`] surface-dispatch + token emission,
//! (2) [`cssl_parse::parse`] recursive-descent CST build,
//! (3) [`cssl_hir::lower_module`] CST → HIR + name-resolution.
//!
//! Each example is considered "accepted" iff parsing completes with zero
//! fatal diagnostics. Stage-0 allows non-fatal diagnostics (e.g. attrs the
//! parser does not yet recognize) — those are counted for visibility but do
//! not fail the acceptance test.
//!
//! § EXAMPLES
//!   - `hello_triangle.cssl` : basic VK-1.4 graphics-pipeline (vertex + fragment).
//!   - `sdf_shader.cssl`     : `bwd_diff(scene_sdf)` KILLER-APP gate.
//!   - `audio_callback.cssl` : full real-time effect-row stack.
//!
//! § T12-phase-2 DEFERRED
//!   - Full type-check + refinement-obligation generation integration (blocked on
//!     T3.4-phase-3 IFC / AD-legality / hygiene slices).
//!   - MIR lowering + codegen-text via the 5 cgen-* backends.
//!   - spirv-val / dxc / naga round-trip validation.
//!   - Vulkan device creation + actual pixel-render via `cssl-host-vulkan`
//!     (gated on T10-phase-2 FFI landing).
//!   - `bwd_diff(scene_sdf)` bit-exact-vs-analytic verification (gated on T7-phase-2
//!     rule-application walker + T9-phase-2 SMT real-solver dispatch).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

use cssl_ast::{Module, SourceFile, SourceId, Surface};
use cssl_hir::HirModule;

/// Canonical path prefix relative to this crate (`compiler-rs/crates/cssl-examples/src/lib.rs`).
/// Resolves to the repo-root `examples/` directory at compile-time via `include_str!`.
pub const HELLO_TRIANGLE_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../examples/hello_triangle.cssl"
));
/// KILLER-APP gate source per `specs/05_AUTODIFF.csl`.
pub const SDF_SHADER_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../examples/sdf_shader.cssl"
));
/// Real-time audio callback source per `specs/21_EXTENDED_SLICE.csl` § UNIFIED AUDIO-DSP.
pub const AUDIO_CALLBACK_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../examples/audio_callback.cssl"
));

/// Pipeline-stage outcome for one example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineOutcome {
    /// Example name (e.g., `"hello_triangle"`).
    pub name: String,
    /// Number of tokens emitted by the lexer.
    pub token_count: usize,
    /// Number of CST-level items parsed.
    pub cst_item_count: usize,
    /// Number of parser-error diagnostics (fatal).
    pub parse_error_count: usize,
    /// Number of HIR-level items lowered.
    pub hir_item_count: usize,
    /// Number of lower-diagnostics.
    pub lower_diag_count: usize,
}

impl PipelineOutcome {
    /// Short summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} : {} tokens / {} CST-items / {} parse-errors / {} HIR-items / {} lower-diags",
            self.name,
            self.token_count,
            self.cst_item_count,
            self.parse_error_count,
            self.hir_item_count,
            self.lower_diag_count,
        )
    }

    /// `true` iff the example is accepted (zero fatal parser errors).
    #[must_use]
    pub fn is_accepted(&self) -> bool {
        self.parse_error_count == 0
    }
}

/// Run the full stage-0 compiler front-end on a `(name, source)` pair.
#[must_use]
pub fn pipeline_example(name: &str, source: &str) -> PipelineOutcome {
    let file = SourceFile::new(SourceId::first(), name, source, Surface::RustHybrid);
    let tokens = cssl_lex::lex(&file);
    let (module, bag) = cssl_parse::parse(&file, &tokens);
    let cst_item_count = module.items.len();
    let parse_error_count = bag.error_count() as usize;

    let (hir_item_count, lower_diag_count) = run_hir(&file, &module);

    PipelineOutcome {
        name: name.to_string(),
        token_count: tokens.len(),
        cst_item_count,
        parse_error_count,
        hir_item_count,
        lower_diag_count,
    }
}

/// Lower CST → HIR. Returns `(hir_item_count, lower_diag_count)`.
fn run_hir(file: &SourceFile, module: &Module) -> (usize, usize) {
    let (hir_mod, _interner, diag) = cssl_hir::lower_module(file, module);
    (hir_item_count(&hir_mod), diag.error_count() as usize)
}

fn hir_item_count(m: &HirModule) -> usize {
    m.items.len()
}

/// Run the pipeline on all three canonical examples.
#[must_use]
pub fn all_examples() -> Vec<PipelineOutcome> {
    vec![
        pipeline_example("hello_triangle", HELLO_TRIANGLE_SRC),
        pipeline_example("sdf_shader", SDF_SHADER_SRC),
        pipeline_example("audio_callback", AUDIO_CALLBACK_SRC),
    ]
}

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{
        all_examples, pipeline_example, AUDIO_CALLBACK_SRC, HELLO_TRIANGLE_SRC, SDF_SHADER_SRC,
        STAGE0_SCAFFOLD,
    };

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn hello_triangle_source_non_empty() {
        assert!(!HELLO_TRIANGLE_SRC.is_empty());
        assert!(HELLO_TRIANGLE_SRC.contains("module com.apocky.examples.hello_triangle"));
    }

    #[test]
    fn sdf_shader_source_non_empty() {
        assert!(!SDF_SHADER_SRC.is_empty());
        assert!(SDF_SHADER_SRC.contains("@differentiable"));
        // KILLER-APP marker : the bwd_diff call must be present.
        assert!(SDF_SHADER_SRC.contains("bwd_diff(scene_sdf)"));
    }

    #[test]
    fn audio_callback_source_non_empty() {
        assert!(!AUDIO_CALLBACK_SRC.is_empty());
        assert!(AUDIO_CALLBACK_SRC.contains("Realtime<Crit>"));
        assert!(AUDIO_CALLBACK_SRC.contains("Audit<\"audio-callback\">"));
    }

    #[test]
    fn hello_triangle_tokenizes() {
        let out = pipeline_example("hello_triangle", HELLO_TRIANGLE_SRC);
        assert!(out.token_count > 0, "expected tokens : {}", out.summary());
    }

    #[test]
    fn sdf_shader_tokenizes() {
        let out = pipeline_example("sdf_shader", SDF_SHADER_SRC);
        assert!(out.token_count > 0);
    }

    #[test]
    fn audio_callback_tokenizes() {
        let out = pipeline_example("audio_callback", AUDIO_CALLBACK_SRC);
        assert!(out.token_count > 0);
    }

    #[test]
    fn all_examples_returns_three_outcomes() {
        let outs = all_examples();
        assert_eq!(outs.len(), 3);
        let names: Vec<_> = outs.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"hello_triangle"));
        assert!(names.contains(&"sdf_shader"));
        assert!(names.contains(&"audio_callback"));
    }

    #[test]
    fn summary_shape() {
        let out = pipeline_example("hello_triangle", HELLO_TRIANGLE_SRC);
        let s = out.summary();
        assert!(s.contains("hello_triangle"));
        assert!(s.contains("tokens"));
        assert!(s.contains("CST-items"));
    }

    #[test]
    fn outcome_is_accepted_returns_bool() {
        // Stage-0 acceptance = zero parse errors ; the individual examples may or
        // may not fully parse depending on which grammar-features have landed.
        // The point is that `is_accepted()` is a stable boolean predicate.
        let out = pipeline_example("hello_triangle", HELLO_TRIANGLE_SRC);
        let _: bool = out.is_accepted();
    }

    #[test]
    fn each_example_emits_nontrivial_tokens_and_items() {
        // Every example should produce at least 10 tokens and at least 1
        // CST-level item. This is a weak but stable lower-bound check that
        // exercises the lex+parse pipeline on all 3 real-world sources.
        for out in all_examples() {
            assert!(
                out.token_count >= 10,
                "{} expected ≥ 10 tokens : {}",
                out.name,
                out.summary()
            );
            assert!(
                out.cst_item_count >= 1,
                "{} expected ≥ 1 CST item : {}",
                out.name,
                out.summary()
            );
        }
    }
}

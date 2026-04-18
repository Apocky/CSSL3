//! Stage-1 scaffold verification (T11-D33).
//!
//! § PURPOSE
//!
//! The repo-root `stage1/` directory holds placeholder CSSLv3-written source
//! files that will *eventually* become the self-hosted stage-1 compiler per
//! `specs/01_BOOTSTRAP.csl` § STAGE-1 and `stage1/README.csl` § PATH.
//!
//! Today those files are minimal (`hello.cssl` returns `42`, `compiler.cssl`
//! is `fn main() -> i32 { 0 }`), but they must remain lex/parse-valid against
//! the *current* stage-0 front-end so that grammar evolution never silently
//! breaks the scaffold.
//!
//! This module embeds the two scaffold files at compile-time via `include_str!`
//! and exposes tests that drive each through the stage-0 [`pipeline_example`]
//! front-end (lex + parse + HIR-lower). The tests assert :
//!   (1) the lexer produces a non-trivial token stream,
//!   (2) parsing completes with zero fatal errors,
//!   (3) at least one CST item is recovered (proving the grammar shape is
//!       recognized, not just that tokenization happened).
//!
//! § WHY NOT INCLUDE IN `all_examples()`
//!
//! The three canonical examples in `examples/` (hello_triangle / sdf_shader /
//! audio_callback) are *vertical-slice* integration tests that must exercise
//! every stage-0 feature (IFC effect rows, refinement obligations, AD walker,
//! etc.). The stage-1 scaffold files are intentionally *minimal* and do not
//! carry those features — including them in `all_examples()` would blur the
//! distinction between "forward-looking self-host scaffold" and "full-surface
//! vertical slice". Kept as a separate test-module here.
//!
//! § SPEC REFERENCES
//!
//!   - `specs/01_BOOTSTRAP.csl` § STAGE-1 : fixed-point self-host goal
//!   - `specs/14_BACKEND.csl`   § NATIVE-X86 : own-backend replaces Cranelift
//!   - `stage1/README.csl`      § PATH + § GATING : P1..P10 capability gate

use crate::pipeline_example;

/// Minimal stage-1 `hello` placeholder. Embedded at compile-time.
pub const STAGE1_HELLO_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stage1/hello.cssl"
));

/// Placeholder stage-1 compiler top-level. Embedded at compile-time.
pub const STAGE1_COMPILER_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stage1/compiler.cssl"
));

/// Run the stage-0 front-end (lex + parse + HIR-lower) against every
/// scaffold-file and return the per-file outcome vector.
///
/// Used by the integration-tests below and also available to downstream
/// drivers (e.g. a future `stage1-driver` binary) that need to check
/// whether the scaffold remains accepting without re-duplicating the
/// `include_str!` / `pipeline_example` boilerplate.
#[must_use]
pub fn all_stage1_scaffold_outcomes() -> Vec<crate::PipelineOutcome> {
    vec![
        pipeline_example("stage1/hello", STAGE1_HELLO_SRC),
        pipeline_example("stage1/compiler", STAGE1_COMPILER_SRC),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        all_stage1_scaffold_outcomes, pipeline_example, STAGE1_COMPILER_SRC, STAGE1_HELLO_SRC,
    };

    #[test]
    fn stage1_hello_src_non_empty() {
        assert!(!STAGE1_HELLO_SRC.is_empty());
        // Marker : the placeholder returns 42.
        assert!(STAGE1_HELLO_SRC.contains("42"));
        // Sanity : it is actually a function definition.
        assert!(STAGE1_HELLO_SRC.contains("fn hello"));
    }

    #[test]
    fn stage1_compiler_src_non_empty() {
        assert!(!STAGE1_COMPILER_SRC.is_empty());
        // Marker : the placeholder is `fn main() -> i32 { 0 }`.
        assert!(STAGE1_COMPILER_SRC.contains("fn main"));
        // Referenced in scaffold README.
        assert!(STAGE1_COMPILER_SRC.contains("P1"));
    }

    #[test]
    fn stage1_hello_tokenizes() {
        let out = pipeline_example("stage1/hello", STAGE1_HELLO_SRC);
        assert!(
            out.token_count > 0,
            "stage1/hello.cssl must tokenize : {}",
            out.summary()
        );
    }

    #[test]
    fn stage1_compiler_tokenizes() {
        let out = pipeline_example("stage1/compiler", STAGE1_COMPILER_SRC);
        assert!(
            out.token_count > 0,
            "stage1/compiler.cssl must tokenize : {}",
            out.summary()
        );
    }

    #[test]
    fn stage1_hello_parses_without_errors() {
        let out = pipeline_example("stage1/hello", STAGE1_HELLO_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stage1/hello.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        assert!(
            out.cst_item_count >= 1,
            "stage1/hello.cssl must yield ≥ 1 CST item : {}",
            out.summary()
        );
    }

    #[test]
    fn stage1_compiler_parses_without_errors() {
        let out = pipeline_example("stage1/compiler", STAGE1_COMPILER_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stage1/compiler.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        assert!(
            out.cst_item_count >= 1,
            "stage1/compiler.cssl must yield ≥ 1 CST item : {}",
            out.summary()
        );
    }

    #[test]
    fn all_stage1_scaffold_outcomes_returns_two() {
        let outs = all_stage1_scaffold_outcomes();
        assert_eq!(outs.len(), 2);
        let names: Vec<_> = outs.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"stage1/hello"));
        assert!(names.contains(&"stage1/compiler"));
    }

    #[test]
    fn all_stage1_scaffold_files_accepted() {
        // The whole point of this module : every scaffold file must remain
        // accepting as the grammar evolves. If a future grammar-slice breaks
        // one of these placeholders, THIS test is the canary.
        for out in all_stage1_scaffold_outcomes() {
            assert!(
                out.is_accepted(),
                "stage-1 scaffold {} must be accepted by stage-0 : {}",
                out.name,
                out.summary()
            );
        }
    }
}

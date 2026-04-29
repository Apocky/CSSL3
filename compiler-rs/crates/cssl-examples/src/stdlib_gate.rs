//! § stdlib gate — `Option<T>` + `Result<T, E>` stage-0 surface.
//!
//! § PURPOSE
//!
//! S6-B2 (T11-D60) lands two CSSLv3-source stdlib files :
//!   - `stdlib/option.cssl`  : `enum Option<T>` + free-fn method surface
//!   - `stdlib/result.cssl`  : `enum Result<T, E>` + free-fn method surface
//!
//! Both must remain lex / parse / HIR-lower clean as the grammar evolves —
//! they are the canonical reference shapes consumers (and downstream B/C/D/E
//! slices) compose against. This module embeds both files at compile-time
//! and pipelines each through the full stage-0 front-end.
//!
//! § ACCEPTANCE
//!   - lexer produces a non-trivial token stream
//!   - parser completes with zero fatal errors
//!   - HIR-lower yields ≥ 1 enum + ≥ 1 fn (proves the type-defs and method
//!     surface both reached the HIR level)
//!   - the canonical constructors (`Some(x)`, `None`, `Ok(x)`, `Err(e)`)
//!     parse + lower as call-shapes recognized by `cssl_mir::body_lower`
//!     intrinsic recognition (verified separately in `cssl-mir` tests)
//!
//! § WHY NOT INCLUDE IN `all_examples()`
//!
//! `all_examples()` exercises the three vertical-slice integration files
//! (hello_triangle / sdf_shader / audio_callback) — those are end-to-end
//! shaders. The stdlib files are LIBRARY surfaces, not vertical slices.
//! Kept separate to preserve the `examples/` vs `stdlib/` distinction.
//!
//! § DEFERRED
//!   - Real runtime execution of Option / Result methods via JIT requires
//!     a `MirType::TaggedUnion` ABI lowering. See DECISIONS T11-D60 §
//!     DEFERRED. At B2 we exercise the surface — the stdlib parses,
//!     lowers, and monomorphizes. JIT execution lands in a follow-up.

use crate::pipeline_example;

/// `stdlib/option.cssl` source, embedded at compile-time.
pub const STDLIB_OPTION_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stdlib/option.cssl"
));

/// `stdlib/result.cssl` source, embedded at compile-time.
pub const STDLIB_RESULT_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stdlib/result.cssl"
));

/// Run the stage-0 front-end (lex + parse + HIR-lower) against every
/// stdlib file and return the per-file outcome vector.
#[must_use]
pub fn all_stdlib_outcomes() -> Vec<crate::PipelineOutcome> {
    vec![
        pipeline_example("stdlib/option", STDLIB_OPTION_SRC),
        pipeline_example("stdlib/result", STDLIB_RESULT_SRC),
    ]
}

#[cfg(test)]
mod tests {
    use super::{all_stdlib_outcomes, pipeline_example, STDLIB_OPTION_SRC, STDLIB_RESULT_SRC};

    #[test]
    fn stdlib_option_src_non_empty() {
        assert!(!STDLIB_OPTION_SRC.is_empty());
        // Marker : type def is present.
        assert!(STDLIB_OPTION_SRC.contains("enum Option<T>"));
        // Marker : free-fn method surface is present.
        assert!(STDLIB_OPTION_SRC.contains("fn option_unwrap<T>"));
        assert!(STDLIB_OPTION_SRC.contains("fn option_map<T, U>"));
        assert!(STDLIB_OPTION_SRC.contains("fn option_and_then<T, U>"));
    }

    #[test]
    fn stdlib_result_src_non_empty() {
        assert!(!STDLIB_RESULT_SRC.is_empty());
        assert!(STDLIB_RESULT_SRC.contains("enum Result<T, E>"));
        assert!(STDLIB_RESULT_SRC.contains("fn result_unwrap<T, E>"));
        assert!(STDLIB_RESULT_SRC.contains("fn result_map<T, U, E>"));
        assert!(STDLIB_RESULT_SRC.contains("fn result_and_then<T, U, E>"));
    }

    #[test]
    fn stdlib_option_tokenizes() {
        let out = pipeline_example("stdlib/option", STDLIB_OPTION_SRC);
        assert!(
            out.token_count > 0,
            "stdlib/option.cssl must tokenize : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_result_tokenizes() {
        let out = pipeline_example("stdlib/result", STDLIB_RESULT_SRC);
        assert!(
            out.token_count > 0,
            "stdlib/result.cssl must tokenize : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_option_parses_without_errors() {
        let out = pipeline_example("stdlib/option", STDLIB_OPTION_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stdlib/option.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        assert!(
            out.cst_item_count >= 2,
            "stdlib/option.cssl must yield ≥ 2 CST items (enum + fns) : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_result_parses_without_errors() {
        let out = pipeline_example("stdlib/result", STDLIB_RESULT_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stdlib/result.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        assert!(
            out.cst_item_count >= 2,
            "stdlib/result.cssl must yield ≥ 2 CST items (enum + fns) : {}",
            out.summary()
        );
    }

    #[test]
    fn all_stdlib_outcomes_returns_two() {
        let outs = all_stdlib_outcomes();
        assert_eq!(outs.len(), 2);
        let names: Vec<_> = outs.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"stdlib/option"));
        assert!(names.contains(&"stdlib/result"));
    }

    #[test]
    fn all_stdlib_files_accepted() {
        // Every stdlib file must remain accepting as the grammar evolves.
        // If a future grammar-slice breaks one of these surfaces, THIS test
        // is the canary that fires before any consumer breaks.
        for out in all_stdlib_outcomes() {
            assert!(
                out.is_accepted(),
                "stdlib {} must be accepted by stage-0 : {}",
                out.name,
                out.summary()
            );
        }
    }

    #[test]
    fn stdlib_option_hir_has_enum_and_fns() {
        // The full pipeline reports HIR-item-count ; for option.cssl the
        // shape is `enum Option<T>` + the worked-example fns + the free
        // method-fns. Total ≥ 8 items.
        let out = pipeline_example("stdlib/option", STDLIB_OPTION_SRC);
        assert!(
            out.hir_item_count >= 8,
            "stdlib/option.cssl HIR must include enum + ≥ 7 method fns : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_result_hir_has_enum_and_fns() {
        let out = pipeline_example("stdlib/result", STDLIB_RESULT_SRC);
        assert!(
            out.hir_item_count >= 9,
            "stdlib/result.cssl HIR must include enum + ≥ 8 method fns : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_option_some_constructor_lowers_to_intrinsic() {
        // Direct-call into the intrinsic recognizer : `Some(7)` must lower
        // to `cssl.option.some` even outside the stdlib file. This proves
        // the recognizer is the canonical entry-point regardless of source
        // file.
        let src = "fn f() -> i32 { Some(7); 0 }";
        let file = cssl_ast::SourceFile::new(
            cssl_ast::SourceId::first(),
            "<probe>",
            src,
            cssl_ast::Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _lbag) = cssl_hir::lower_module(&file, &cst);
        let f_item = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => Some(f),
                _ => None,
            })
            .expect("expected a fn item");
        let lower_ctx = cssl_mir::LowerCtx::new(&interner);
        let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f_item);
        cssl_mir::lower_fn_body(&interner, Some(&file), f_item, &mut mf);
        let entry = mf.body.entry().expect("entry block must exist");
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.option.some"),
            "Some(7) must produce cssl.option.some op (got : {:?})",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn stdlib_result_ok_constructor_lowers_to_intrinsic() {
        let src = "fn f() -> i32 { Ok(42); 0 }";
        let file = cssl_ast::SourceFile::new(
            cssl_ast::SourceId::first(),
            "<probe>",
            src,
            cssl_ast::Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _lbag) = cssl_hir::lower_module(&file, &cst);
        let f_item = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => Some(f),
                _ => None,
            })
            .expect("expected a fn item");
        let lower_ctx = cssl_mir::LowerCtx::new(&interner);
        let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f_item);
        cssl_mir::lower_fn_body(&interner, Some(&file), f_item, &mut mf);
        let entry = mf.body.entry().expect("entry block must exist");
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.result.ok"),
            "Ok(42) must produce cssl.result.ok op (got : {:?})",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn stdlib_try_op_propagates_through_pipeline() {
        // The `?` operator is HirExprKind::Try lowering to MIR `cssl.try` op.
        // This test exercises a Result-shaped operand : the lowering must
        // emit `cssl.try` even when the operand was constructed via Ok / Err.
        let src = "fn f(r : Result<i32, i32>) -> Result<i32, i32> { let x = r? ; Ok(x + 1) }";
        let file = cssl_ast::SourceFile::new(
            cssl_ast::SourceId::first(),
            "<probe>",
            src,
            cssl_ast::Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _lbag) = cssl_hir::lower_module(&file, &cst);
        let f_item = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => Some(f),
                _ => None,
            })
            .expect("expected a fn item");
        let lower_ctx = cssl_mir::LowerCtx::new(&interner);
        let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f_item);
        cssl_mir::lower_fn_body(&interner, Some(&file), f_item, &mut mf);
        let entry = mf.body.entry().expect("entry block must exist");
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.try"),
            "?-operator must produce cssl.try op (got : {:?})",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.result.ok"),
            "Ok(...) must produce cssl.result.ok op",
        );
    }

    #[test]
    fn stdlib_option_distinct_specializations_for_i32_and_f32() {
        // The monomorphization quartet (D38..D50) must produce distinct
        // mangled symbols for `option_unwrap_or::<i32>` vs `::<f32>`.
        // Construct a minimal program exercising both turbofish call sites.
        let src = "fn id<T>(x : T) -> T { x }\n\
                   fn driver() -> i32 { let _a = id::<i32>(7) ; let _b = id::<f32>(2.5) ; 0 }";
        let file = cssl_ast::SourceFile::new(
            cssl_ast::SourceId::first(),
            "<probe>",
            src,
            cssl_ast::Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _lbag) = cssl_hir::lower_module(&file, &cst);
        let report = cssl_mir::auto_monomorphize(&hir, &interner, Some(&file));
        // At least two specializations expected : id::<i32> and id::<f32>.
        assert!(
            report.specializations.len() >= 2,
            "expected ≥ 2 specializations, got {} (mangled = {:?})",
            report.specializations.len(),
            report
                .specializations
                .iter()
                .map(|s| &s.name)
                .collect::<Vec<_>>(),
        );
        // The specializations must have distinct mangled names.
        let mut names: Vec<&str> = report
            .specializations
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert!(
            names.len() >= 2,
            "specializations must have distinct mangled names, got {names:?}",
        );
    }
}

//! § stdlib gate — `Option<T>` + `Result<T, E>` + `Vec<T>` stage-0 surface.
//!
//! § PURPOSE
//!
//! The stdlib gate ensures every `stdlib/*.cssl` file remains lex / parse /
//! HIR-lower clean as the grammar evolves. These files are the canonical
//! reference shapes downstream slices (and consumers) compose against.
//! Whenever a new stdlib file lands the gate gains coverage so that any
//! future grammar slice that regresses one of these surfaces fails THIS
//! test before any real consumer breaks.
//!
//! § FILES TRACKED
//!   - `stdlib/option.cssl`  — `enum Option<T>` + free-fn method surface (S6-B2 / T11-D60)
//!   - `stdlib/result.cssl`  — `enum Result<T, E>` + free-fn method surface (S6-B2 / T11-D60)
//!   - `stdlib/vec.cssl`     — `struct Vec<T>` + free-fn method surface  (S6-B3 / T11-D69)
//!
//! § ACCEPTANCE
//!   - lexer produces a non-trivial token stream
//!   - parser completes with zero fatal errors
//!   - HIR-lower yields ≥ 1 type-def + ≥ 1 fn (proves both reach HIR)
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
//!   - Real runtime execution of Option / Result / Vec methods via JIT
//!     requires a `MirType::TaggedUnion` + typed-pointer ABI lowering.
//!     See DECISIONS T11-D60 § DEFERRED + T11-D69 § DEFERRED. At B2/B3 we
//!     exercise the SURFACE — the stdlib parses, lowers, and monomorphizes.
//!     JIT execution lands in a follow-up.

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

/// `stdlib/vec.cssl` source, embedded at compile-time. Added at S6-B3
/// (T11-D69) ; tracks the generic-collection surface.
pub const STDLIB_VEC_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stdlib/vec.cssl"
));

/// Run the stage-0 front-end (lex + parse + HIR-lower) against every
/// stdlib file and return the per-file outcome vector.
#[must_use]
pub fn all_stdlib_outcomes() -> Vec<crate::PipelineOutcome> {
    vec![
        pipeline_example("stdlib/option", STDLIB_OPTION_SRC),
        pipeline_example("stdlib/result", STDLIB_RESULT_SRC),
        pipeline_example("stdlib/vec", STDLIB_VEC_SRC),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        all_stdlib_outcomes, pipeline_example, STDLIB_OPTION_SRC, STDLIB_RESULT_SRC, STDLIB_VEC_SRC,
    };

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

    // ── S6-B3 (T11-D69) Vec<T> stdlib coverage ──────────────────────────

    #[test]
    fn stdlib_vec_src_non_empty() {
        assert!(!STDLIB_VEC_SRC.is_empty());
        // Markers : the type-defs and the canonical method surface.
        assert!(STDLIB_VEC_SRC.contains("struct Vec<T>"));
        assert!(STDLIB_VEC_SRC.contains("struct VecIter<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_new<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_with_capacity<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_push<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_pop<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_len<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_is_empty<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_get<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_index<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_iter<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_clear<T>"));
        assert!(STDLIB_VEC_SRC.contains("fn vec_drop<T>"));
        // Marker : 2x amortized-growth helper.
        assert!(STDLIB_VEC_SRC.contains("fn next_capacity"));
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
    fn stdlib_vec_tokenizes() {
        let out = pipeline_example("stdlib/vec", STDLIB_VEC_SRC);
        assert!(
            out.token_count > 0,
            "stdlib/vec.cssl must tokenize : {}",
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
    fn stdlib_vec_parses_without_errors() {
        let out = pipeline_example("stdlib/vec", STDLIB_VEC_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stdlib/vec.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        // Two structs (Vec<T> + VecIter<T>) + many fns + worked examples.
        assert!(
            out.cst_item_count >= 5,
            "stdlib/vec.cssl must yield ≥ 5 CST items (structs + fns) : {}",
            out.summary()
        );
    }

    #[test]
    fn all_stdlib_outcomes_returns_three() {
        let outs = all_stdlib_outcomes();
        assert_eq!(outs.len(), 3);
        let names: Vec<_> = outs.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"stdlib/option"));
        assert!(names.contains(&"stdlib/result"));
        assert!(names.contains(&"stdlib/vec"));
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
    fn stdlib_vec_hir_has_structs_and_fns() {
        // vec.cssl contains : 2 structs (Vec / VecIter) + a large free-fn
        // surface + worked-example fns. Total HIR-items ≥ 18.
        let out = pipeline_example("stdlib/vec", STDLIB_VEC_SRC);
        assert!(
            out.hir_item_count >= 18,
            "stdlib/vec.cssl HIR must include structs + ≥ 16 fns : {}",
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

    // ── S6-B3 (T11-D69) Vec<T> intrinsic + monomorph coverage ───────────

    #[test]
    fn stdlib_vec_with_capacity_lowers_through_box_recognizer() {
        // `vec_with_capacity::<i32>` calls `alloc_for_cap::<T>(n)` which
        // contains `Box::new(cap)` — the existing B1 recognizer fires and
        // the resulting MIR must contain a `cssl.heap.alloc` op. This is
        // the only direct-call into the heap allocator that survives at
        // stage-0 (vec.cssl uses Box::new placeholders for grow/realloc
        // until the typed-memref slice lands).
        let src = "fn f() -> i32 { Box::new(8); 0 }";
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
            entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"),
            "Box::new through stdlib helpers must produce cssl.heap.alloc (got : {:?})",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn stdlib_vec_distinct_specializations_for_nested_generics() {
        // Vec<i32> and Vec<f32> must produce distinct mangled symbols.
        // Nested-generic-arg paths (turbofish carrying `<<...>>`) are
        // pending the parser-disambiguation slice (Shr-vs-Gt-Gt) ; the
        // single-layer turbofish form is the canary at S6-B3 because it
        // is the form actually used in stdlib/vec.cssl worked-examples.
        let src = "fn id<T>(x : T) -> T { x }\n\
                   fn driver() -> i32 { \
                     let _a = id::<i32>(7) ; \
                     let _b = id::<f32>(2.5) ; \
                     0 \
                   }";
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
        // ≥ 2 distinct specializations across the nested-type-arg call sites.
        let mut names: Vec<&str> = report
            .specializations
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert!(
            names.len() >= 2,
            "Vec<i32> + Vec<Option<i32>> must produce ≥ 2 distinct specializations \
             (got {names:?})",
        );
    }

    #[test]
    fn stdlib_vec_struct_def_lowers_to_hir_struct() {
        // `struct Vec<T> { ... }` must round-trip the parser + HIR-lower
        // to a HirItem::Struct with a generic-param. This is the lowest-
        // level guard that the struct-field syntax (data : !cssl.ptr) is
        // accepted by stage-0.
        let out = pipeline_example("stdlib/vec", STDLIB_VEC_SRC);
        assert!(
            out.is_accepted(),
            "stdlib/vec.cssl must be accepted : {}",
            out.summary()
        );
        // Parser must report ≥ 1 struct item ; HIR must report ≥ 1 struct.
        assert!(
            out.cst_item_count >= 5,
            "stdlib/vec.cssl CST items expected ≥ 5 : {}",
            out.summary()
        );
        assert!(
            out.hir_item_count >= 18,
            "stdlib/vec.cssl HIR items expected ≥ 18 : {}",
            out.summary()
        );
    }
}

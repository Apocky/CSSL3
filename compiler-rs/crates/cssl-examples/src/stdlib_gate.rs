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
//!   - `stdlib/string.cssl`  — `String` (`Vec<u8>`-backed UTF-8) + `StrSlice`
//!                              fat-pointer + `char` USV + minimal `format(...)`
//!                              builtin (S6-B4 / T11-D71)
//!   - `stdlib/fs.cssl`      — `File` + `IoError` sum-type + `open / close /
//!                              read_some / write_all / read_to_string` free-fn
//!                              method surface (S6-B5 / T11-D76). Uses
//!                              syntactic-recognizer-emitted `cssl.fs.*` ops
//!                              with the `(io_effect, "true")` marker for
//!                              {IO} effect-row threading.
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

/// `stdlib/string.cssl` source, embedded at compile-time. Added at S6-B4
/// (T11-D71) ; tracks the String + &str + char + format surface.
pub const STDLIB_STRING_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stdlib/string.cssl"
));

/// `stdlib/fs.cssl` source, embedded at compile-time. Added at S6-B5
/// (T11-D76) ; tracks the file-I/O surface (File / IoError /
/// open / close / write_all / read_to_string).
pub const STDLIB_FS_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../stdlib/fs.cssl"
));

/// Run the stage-0 front-end (lex + parse + HIR-lower) against every
/// stdlib file and return the per-file outcome vector.
#[must_use]
pub fn all_stdlib_outcomes() -> Vec<crate::PipelineOutcome> {
    vec![
        pipeline_example("stdlib/option", STDLIB_OPTION_SRC),
        pipeline_example("stdlib/result", STDLIB_RESULT_SRC),
        pipeline_example("stdlib/vec", STDLIB_VEC_SRC),
        pipeline_example("stdlib/string", STDLIB_STRING_SRC),
        pipeline_example("stdlib/fs", STDLIB_FS_SRC),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        all_stdlib_outcomes, pipeline_example, STDLIB_FS_SRC, STDLIB_OPTION_SRC, STDLIB_RESULT_SRC,
        STDLIB_STRING_SRC, STDLIB_VEC_SRC,
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
    fn all_stdlib_outcomes_returns_five() {
        let outs = all_stdlib_outcomes();
        assert_eq!(outs.len(), 5);
        let names: Vec<_> = outs.iter().map(|o| o.name.as_str()).collect();
        assert!(names.contains(&"stdlib/option"));
        assert!(names.contains(&"stdlib/result"));
        assert!(names.contains(&"stdlib/vec"));
        assert!(names.contains(&"stdlib/string"));
        assert!(names.contains(&"stdlib/fs"));
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

    // ── S6-B4 (T11-D71) String + &str + char + format coverage ──────────

    #[test]
    fn stdlib_string_src_non_empty() {
        assert!(!STDLIB_STRING_SRC.is_empty());
        // Markers : the type-defs and the canonical method surface.
        assert!(STDLIB_STRING_SRC.contains("struct String"));
        assert!(STDLIB_STRING_SRC.contains("struct StrSlice"));
        assert!(STDLIB_STRING_SRC.contains("struct FromUtf8Error"));
        assert!(STDLIB_STRING_SRC.contains("fn string_new"));
        assert!(STDLIB_STRING_SRC.contains("fn string_from_utf8"));
        assert!(STDLIB_STRING_SRC.contains("fn string_from_utf8_unchecked"));
        assert!(STDLIB_STRING_SRC.contains("fn string_with_capacity"));
        assert!(STDLIB_STRING_SRC.contains("fn string_len"));
        assert!(STDLIB_STRING_SRC.contains("fn string_is_empty"));
        assert!(STDLIB_STRING_SRC.contains("fn string_push"));
        assert!(STDLIB_STRING_SRC.contains("fn string_concat"));
        assert!(STDLIB_STRING_SRC.contains("fn string_as_str"));
        assert!(STDLIB_STRING_SRC.contains("fn char_from_u32"));
        assert!(STDLIB_STRING_SRC.contains("fn char_at"));
        assert!(STDLIB_STRING_SRC.contains("fn format"));
    }

    #[test]
    fn stdlib_string_tokenizes() {
        let out = pipeline_example("stdlib/string", STDLIB_STRING_SRC);
        assert!(
            out.token_count > 0,
            "stdlib/string.cssl must tokenize : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_string_parses_without_errors() {
        let out = pipeline_example("stdlib/string", STDLIB_STRING_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stdlib/string.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        // 3 structs (String / StrSlice / FromUtf8Error) + many fns +
        // worked examples.
        assert!(
            out.cst_item_count >= 5,
            "stdlib/string.cssl must yield ≥ 5 CST items : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_string_hir_has_structs_and_fns() {
        // string.cssl contains : 3 structs (String / StrSlice /
        // FromUtf8Error) + a large free-fn surface + worked-example fns.
        // Total HIR-items ≥ 22.
        let out = pipeline_example("stdlib/string", STDLIB_STRING_SRC);
        assert!(
            out.hir_item_count >= 22,
            "stdlib/string.cssl HIR must include structs + ≥ 19 fns : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_string_format_recognizer_lowers_to_intrinsic() {
        // Direct probe : the bare `format("hello")` shape must lower to
        // `cssl.string.format` even outside the stdlib file. Mirrors the
        // B2 / B3 probe pattern.
        let src = "fn f() -> i32 { format(\"hello\"); 0 }";
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
            entry.ops.iter().any(|o| o.name == "cssl.string.format"),
            "format(\"hello\") must produce cssl.string.format op (got : {:?})",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn stdlib_string_format_records_specifier_and_arg_counts() {
        // The recognizer must record both `spec_count` and `arg_count` as
        // op-attributes so a deferred validator slice can flag mismatches.
        // Verified indirectly by parsing a 2-spec / 2-arg call shape and
        // confirming the attributes survive through MIR.
        let src = "fn f() -> i32 { format(\"a = {} b = {}\", 1, 2); 0 }";
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
        let fmt_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("expected cssl.string.format op");
        let spec = fmt_op
            .attributes
            .iter()
            .find(|(k, _)| k == "spec_count")
            .map(|(_, v)| v.as_str());
        let argc = fmt_op
            .attributes
            .iter()
            .find(|(k, _)| k == "arg_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(spec, Some("2"));
        assert_eq!(argc, Some("2"));
    }

    #[test]
    fn stdlib_string_distinct_specializations_for_nested_generics() {
        // String wraps Vec<u8> ; the monomorph quartet must produce a
        // distinct specialization for `vec_*::<u8>` vs `vec_*::<i32>`.
        // Mirrors the B3 nested-generic-arg canary.
        let src = "fn id<T>(x : T) -> T { x }\n\
                   fn driver() -> i32 { \
                     let _a = id::<u8>(7) ; \
                     let _b = id::<i32>(2) ; \
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
        let mut names: Vec<&str> = report
            .specializations
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert!(
            names.len() >= 2,
            "id::<u8> + id::<i32> must produce ≥ 2 distinct specializations \
             (got {names:?})",
        );
    }

    #[test]
    fn stdlib_string_struct_def_lowers_to_hir_struct() {
        // `struct String { bytes : Vec<u8> }` must round-trip parser +
        // HIR-lower to a HirItem::Struct. This is the lowest-level guard
        // that the struct-field-with-nested-generic-arg syntax is
        // accepted by stage-0.
        let out = pipeline_example("stdlib/string", STDLIB_STRING_SRC);
        assert!(
            out.is_accepted(),
            "stdlib/string.cssl must be accepted : {}",
            out.summary()
        );
        assert!(
            out.cst_item_count >= 5,
            "stdlib/string.cssl CST items expected ≥ 5 : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_string_char_literal_lowers_to_i32_constant() {
        // Source-level `'a'` lexes as a CharLit and lowers to an i32
        // constant per `cssl_mir::body_lower::lower_literal`. This is the
        // foundation of B4's char USV invariant.
        let src = "fn f() -> i32 { 'A' }";
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
        // The char literal must produce an arith.constant op.
        assert!(
            entry.ops.iter().any(|o| o.name == "arith.constant"),
            "char literal must lower to arith.constant : {:?}",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn stdlib_string_str_literal_lowers_through_pipeline() {
        // String literal `"hello"` lexes as StringLiteral and lowers to
        // `MirType::Opaque("!cssl.string")` per existing literal lowering.
        // This test confirms the path remains green at B4.
        let src = "fn f() -> i32 { let _x = \"hello\"; 0 }";
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
        // Confirm the body actually lowered to ops (string-literal path
        // through arith.constant + let-binding path).
        assert!(
            !entry.ops.is_empty(),
            "string literal body must lower to ≥ 1 MIR op",
        );
    }

    // ── S6-B5 (T11-D76) file-I/O stdlib coverage ────────────────────────

    #[test]
    fn stdlib_fs_src_non_empty() {
        assert!(!STDLIB_FS_SRC.is_empty());
        // Markers : the type-defs and the canonical surface.
        assert!(STDLIB_FS_SRC.contains("struct File"));
        assert!(STDLIB_FS_SRC.contains("enum IoError"));
        assert!(STDLIB_FS_SRC.contains("NotFound"));
        assert!(STDLIB_FS_SRC.contains("PermissionDenied"));
        assert!(STDLIB_FS_SRC.contains("AlreadyExists"));
        assert!(STDLIB_FS_SRC.contains("InvalidInput"));
        assert!(STDLIB_FS_SRC.contains("WriteZero"));
        assert!(STDLIB_FS_SRC.contains("Other"));
        // Free-fn surface
        assert!(STDLIB_FS_SRC.contains("fn open("));
        assert!(STDLIB_FS_SRC.contains("fn close("));
        assert!(STDLIB_FS_SRC.contains("fn write_all("));
        assert!(STDLIB_FS_SRC.contains("fn read_some("));
        assert!(STDLIB_FS_SRC.contains("fn read_to_string("));
        // OPEN_* flag accessors
        assert!(STDLIB_FS_SRC.contains("fn open_read"));
        assert!(STDLIB_FS_SRC.contains("fn open_write"));
        assert!(STDLIB_FS_SRC.contains("fn open_create"));
        assert!(STDLIB_FS_SRC.contains("fn open_truncate"));
        // IoError-discriminant accessors (cssl-rt code-table mirror)
        assert!(STDLIB_FS_SRC.contains("fn io_err_code_not_found"));
        assert!(STDLIB_FS_SRC.contains("fn io_err_code_other"));
        // Effect-row marker note (preserved through documentation)
        assert!(STDLIB_FS_SRC.contains("io_effect"));
        assert!(STDLIB_FS_SRC.contains("{IO}"));
    }

    #[test]
    fn stdlib_fs_tokenizes() {
        let out = pipeline_example("stdlib/fs", STDLIB_FS_SRC);
        assert!(
            out.token_count > 0,
            "stdlib/fs.cssl must tokenize : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_fs_parses_without_errors() {
        let out = pipeline_example("stdlib/fs", STDLIB_FS_SRC);
        assert_eq!(
            out.parse_error_count,
            0,
            "stdlib/fs.cssl must parse cleanly through stage-0 : {}",
            out.summary()
        );
        // 1 struct (File) + 1 enum (IoError) + many fns + worked examples.
        assert!(
            out.cst_item_count >= 5,
            "stdlib/fs.cssl must yield ≥ 5 CST items : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_fs_hir_has_struct_enum_and_fns() {
        // fs.cssl contains : 1 struct (File), 1 enum (IoError), the
        // OPEN_* + io_err_code_* accessor fns, and the free-fn method
        // surface (open / close / write_all / read_some / read_to_string)
        // + worked examples + last_error_kind / _os.
        // Total HIR-items ≥ 18.
        let out = pipeline_example("stdlib/fs", STDLIB_FS_SRC);
        assert!(
            out.hir_item_count >= 18,
            "stdlib/fs.cssl HIR must include struct + enum + ≥ 16 fns : {}",
            out.summary()
        );
    }

    #[test]
    fn stdlib_fs_open_recognizer_lowers_to_intrinsic() {
        // Direct probe : the bare `fs::open(path, flags)` shape must lower
        // to `cssl.fs.open` even outside the stdlib file. Mirrors the
        // B2 / B3 / B4 probe pattern.
        let src = "fn f(p : &str) -> i64 { fs::open(p, 1) }";
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
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.fs.open")
            .expect("fs::open must produce cssl.fs.open op");
        // Confirm the io_effect marker is recorded (per slice handoff
        // {IO} effect-row threading).
        let io_effect = op
            .attributes
            .iter()
            .find(|(k, _)| k == "io_effect")
            .map(|(_, v)| v.as_str());
        assert_eq!(io_effect, Some("true"));
    }

    #[test]
    fn stdlib_fs_close_recognizer_lowers_to_intrinsic() {
        let src = "fn f(h : i64) -> i64 { fs::close(h) }";
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
            entry.ops.iter().any(|o| o.name == "cssl.fs.close"),
            "fs::close must produce cssl.fs.close op",
        );
    }

    #[test]
    fn stdlib_fs_read_write_recognizers_emit_distinct_ops() {
        // Confirms read + write recognizers produce distinct ops + the
        // io_effect marker rides on each. A single MIR fn body can host
        // both ops without name collision.
        let src = "fn f(h : i64, p : i64, n : i64) -> i64 { \
                   let _r = fs::read(h, p, n) ; \
                   let w = fs::write(h, p, n) ; \
                   w }";
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
            entry.ops.iter().any(|o| o.name == "cssl.fs.read"),
            "fs::read must produce cssl.fs.read op",
        );
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.fs.write"),
            "fs::write must produce cssl.fs.write op",
        );
    }
}

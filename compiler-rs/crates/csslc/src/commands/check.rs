//! § commands::check — `csslc check <input.cssl>`.
//!
//! Frontend-only orchestration : load source → lex → parse → HIR-lower → type-check.
//! Reports any errors via `diag` and returns a non-zero exit code if the
//! source has errors. No emission.

use std::path::Path;
use std::process::ExitCode;

use crate::cli::CheckArgs;
use crate::diag;
use crate::exit_code;

pub fn run(args: &CheckArgs) -> ExitCode {
    let source = match std::fs::read_to_string(&args.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("csslc: {}", diag::fs_error(&args.input, &e));
            return ExitCode::from(exit_code::USER_ERROR);
        }
    };
    run_with_source(&args.input, &source)
}

/// Invoke the frontend pipeline on `(path, source)`. Splits out for
/// in-process tests that synthesize source without touching the file
/// system.
pub fn run_with_source(path: &Path, source: &str) -> ExitCode {
    use cssl_ast::{SourceFile, SourceId, Surface};

    let file = SourceFile::new(
        SourceId::first(),
        path.display().to_string(),
        source,
        Surface::RustHybrid,
    );
    let tokens = cssl_lex::lex(&file);
    let (cst, parse_bag) = cssl_parse::parse(&file, &tokens);
    let parse_errors = diag::emit_diagnostics(path, &parse_bag);
    if parse_errors > 0 {
        eprintln!("csslc: check failed — {parse_errors} parse error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let (hir_mod, interner, lower_bag) = cssl_hir::lower_module(&file, &cst);
    let lower_errors = diag::emit_diagnostics(path, &lower_bag);
    if lower_errors > 0 {
        eprintln!("csslc: check failed — {lower_errors} HIR-lower error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    let (_type_map, type_diagnostics) = cssl_hir::check_module(&hir_mod, &interner);
    let type_errors = diag::emit_type_diagnostics(path, &file, &type_diagnostics);
    if type_errors > 0 {
        eprintln!("csslc: check failed — {type_errors} type-check error(s)");
        return ExitCode::from(exit_code::USER_ERROR);
    }

    eprintln!("csslc: check {} : OK", path.display());
    ExitCode::from(exit_code::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_with_empty_source_succeeds() {
        let code = run_with_source(Path::new("empty.cssl"), "");
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn check_with_minimal_module_succeeds() {
        let src = "module com.apocky.examples.hello\n\
                   fn main() -> i32 { 42 }\n";
        let code = run_with_source(Path::new("hello.cssl"), src);
        let ok: ExitCode = ExitCode::from(exit_code::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{ok:?}"));
    }

    #[test]
    fn check_with_missing_file_returns_user_error() {
        let args = CheckArgs {
            input: std::path::PathBuf::from("/nonexistent/foo.cssl"),
        };
        let code = run(&args);
        let err: ExitCode = ExitCode::from(exit_code::USER_ERROR);
        assert_eq!(format!("{code:?}"), format!("{err:?}"));
    }

    #[test]
    fn check_preserves_bool_and_integer_not_contracts() {
        let src = "fn bool_not(value: bool) -> bool { !value }\n\
                   fn int_bang(value: u32) -> u32 { !value }\n\
                   fn int_tilde(value: u32) -> u32 { ~value }\n";
        let code = run_with_source(Path::new("valid_not.cssl"), src);
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(exit_code::SUCCESS))
        );
    }

    #[test]
    fn check_rejects_noncanonical_bool_ingress_and_invalid_unary_domains() {
        let invalid = [
            "fn invalid(value: bool) -> bool { ~value }",
            "fn invalid() -> bool { 2 }",
            "fn invalid(value: u8) -> bool { value }",
            "fn invalid(value: u8) -> bool { value as bool }",
            "fn invalid(value: f32) -> bool { !value }",
            "fn invalid(value: String) -> bool { !value }",
        ];
        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for (index, src) in invalid.iter().enumerate() {
            let code = run_with_source(Path::new("invalid_type.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "case {index} accepted: {src}"
            );
        }
    }

    #[test]
    fn check_keeps_unknown_unqualified_local_names_fatal() {
        let src = "fn invalid() -> i32 { definitely_missing_local }";
        let code = run_with_source(Path::new("unknown_local.cssl"), src);
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(exit_code::USER_ERROR))
        );
    }

    #[test]
    fn opaque_external_surfaces_do_not_mask_local_bool_violations() {
        let invalid = [
            "use std::result::Result\n\
             use external::opaque\n\
             fn invalid(value: u8) -> bool {\n\
                 let a: Result<u8, u8> = external::qualified(value);\n\
                 let b = opaque(value);\n\
                 value\n\
             }",
            "use std::option::Option\n\
             fn invalid(value: bool) -> bool {\n\
                 let a: Option<u8> = external::qualified();\n\
                 ~value\n\
             }",
        ];
        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for src in invalid {
            let code = run_with_source(Path::new("opaque_with_local_error.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "local violation masked: {src}"
            );
        }
    }

    #[test]
    fn locally_knowable_names_cannot_escape_through_opaque_type_holes() {
        let invalid = [
            "struct Option { value: u8 } fn invalid(value: Option) -> bool { value }",
            "struct Result { value: u8 } fn invalid(value: Result) -> bool { value }",
            "struct Vec { value: u8 } fn invalid(value: Vec) -> bool { value }",
            "use external::Option\nstruct Option { value: u8 } fn invalid(value: Option) -> bool { value }",
            "fn invalid() -> bool { Some(1u8) }",
            "fn invalid() -> bool { None }",
            "fn invalid() -> bool { Ok(1u8) }",
            "fn invalid() -> bool { Err(1u8) }",
            "fn invalid() -> Option<u8> { Some(true) }",
            "fn invalid() -> Result<u8, u8> { Err(true) }",
            "fn invalid(value: Option<u8>) -> Vec<u8> { value }",
            "fn invalid(value: Vec<u8>) -> Option<u8> { value }",
            "module inner { fn byte_value() -> u8 { 1u8 } }\n\
             fn invalid() -> bool { inner::byte_value() }",
            "module inner { use external::opaque fn valid() -> bool { true } }\n\
             fn invalid() -> bool { opaque() }",
            "use external::opaque\nmodule opaque { fn value() -> u8 { 1u8 } }\nfn invalid() -> bool { opaque() }",
            "fn invalid() -> bool { definitely_missing_module::opaque() }",
            "fn invalid() -> Option<u8> { external::qualified() }",
            "fn invalid() -> Result<u8, u8> { Ok(true) }",
            "fn Foo(value: u8) -> u8 { value }\nfn invalid() -> u8 { Foo::Whatever(1u8) }",
            "use external::Foo\nfn Foo(value: u8) -> u8 { value }\nfn invalid() -> u8 { Foo::Whatever(1u8) }",
        ];
        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for (index, src) in invalid.iter().enumerate() {
            let code = run_with_source(Path::new("local_opaque_escape.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "case {index} accepted: {src}"
            );
        }
    }

    #[test]
    fn top_level_untyped_import_is_declared_opacity_boundary() {
        for src in [
            "use external::opaque\nfn bounded() -> bool { opaque() }",
            "use external::opaque as known\nfn bounded() -> bool { known() }",
            "use std::gpu::GpuError\nfn bounded(value: GpuError) -> GpuError { GpuError::CapDenied }",
            "use std::gpu::GpuError as err\nfn bounded(value: err) -> err { err::CapDenied }",
            "use std::gpu::GpuError\n\
             use std::gpu::GpuError as Fault\n\
             fn preserve(value: GpuError) -> Fault { value }\n\
             fn construct() -> GpuError { Fault::CapDenied }",
            "fn preserve<Vec>(value: Vec) -> Vec { value }",
        ] {
            let code = run_with_source(Path::new("declared_opacity.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                format!("{:?}", ExitCode::from(exit_code::SUCCESS)),
                "declared stage-0 opacity boundary rejected: {src}"
            );
        }

        let invalid = [
            "fn invalid() -> bool { opaque() }",
            "module inner { use external::opaque fn valid() -> bool { true } }\n\
             fn invalid() -> bool { opaque() }",
            "fn invalid() -> bool { missing::opaque() }",
            "use external::opaque as known\nfn invalid() -> bool { opaque() }",
            "use external::opaque\nfn invalid() -> bool { opaque::arbitrary() }",
            "use external::fs\nfn invalid() -> bool { fs::open(\"x\", 1) }",
            "use external::whatever as fs\nfn invalid() -> bool { fs::open(\"x\", 1) }",
            "use external::Vec\nfn invalid() -> bool { Vec::new() }",
            "use external::opaque\nfn invalid() -> bool { opaque::Arbitrary }",
            "use external::Foo\nfn invalid(value: Foo) -> Foo { Foo::Whatever }",
            "use std::gpu::GpuError\nfn invalid() -> bool { GpuError::CapDenied }",
            "use std::gpu::GpuError\nfn invalid(value: GpuError) -> GpuError { GpuError::Whatever }",
            "use std::gpu::GpuError as Fault\n\
             use std::gpu_transport::BufferUsage as Usage\n\
             fn invalid(value: Fault) -> Usage { value }",
        ];
        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for src in invalid {
            let code = run_with_source(Path::new("undeclared_opacity.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "undeclared or out-of-scope opacity boundary accepted: {src}"
            );
        }
    }

    #[test]
    fn exact_qualified_vec_index_is_typed_and_bounded() {
        let valid = "fn bounded<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }";
        let code = run_with_source(Path::new("qualified_vec_index.cssl"), valid);
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(exit_code::SUCCESS))
        );

        for src in [
            "fn invalid<T>(value: Vec<T>) -> bool { std::vec::vec_index::<T>(value, 0) }",
            "fn invalid(value: Vec<u64>) -> u64 { std::vec::vec_index::<i32>(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T, T>(value, 0) }",
            "module std { fn marker() -> bool { true } }\nfn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }",
            "use external::std\nfn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }",
            "module inner { use external::std fn marker() -> bool { true } }\nfn invalid<T>(value: Vec<T>) -> T { std::vec::vec_index::<T>(value, 0) }",
            "fn invalid<T>(value: Vec<T>) -> T { let std = 1; std::vec::vec_index::<T>(value, 0) }",
            "fn std(value: Vec<i32>, index: i64) -> i32 { 0 }\n\
             fn invalid(value: Vec<i32>) -> i32 { std::vec::vec_index::<i32>(value, 0) }",
            "fn invalid(value: Vec<i32>) -> i32 {\n\
                 let std = |items: Vec<i32>, index: i64| { 0 };\n\
                 std::vec::vec_index::<i32>(value, 0)\n\
             }",
            "fn invalid<T>(value: Vec<T>) -> T { other::vec::vec_index::<T>(value, 0) }",
        ] {
            let code = run_with_source(Path::new("invalid_qualified_vec_index.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                format!("{:?}", ExitCode::from(exit_code::USER_ERROR))
            );
        }
    }

    #[test]
    fn exact_stage0_host_intrinsics_are_declared_opacity_boundaries() {
        for src in [
            "fn bounded() -> i64 { fs::open(\"x\", 1) }",
            "fn bounded() -> i64 { net::socket(1) }",
            "fn bounded() -> i64 { time::monotonic_ns() }",
            "fn bounded() -> i64 { window::spawn(1, 2, 3, 4, 5) }",
            "fn bounded() -> i32 { input::keyboard_state(1, 2, 3) }",
            "fn bounded() -> i64 { gpu::device_create(1, 2) }",
            "fn bounded() -> i64 { audio::stream_open(1, 2, 3, 4) }",
            "fn bounded() -> i64 { thread::spawn(1, 2) }",
            "fn bounded() -> i64 { mutex::create() }",
            "fn bounded() -> i64 { atomic::load_u64(1, 2) }",
        ] {
            let code = run_with_source(Path::new("known_host_intrinsic.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                format!("{:?}", ExitCode::from(exit_code::SUCCESS)),
                "known host intrinsic rejected: {src}"
            );
        }

        let expected = format!("{:?}", ExitCode::from(exit_code::USER_ERROR));
        for src in [
            "fn invalid() -> i64 { gpu::definitely_missing() }",
            "fn invalid() -> i64 { missing::device_create(1, 2) }",
            "module fs { fn open() -> u8 { 1u8 } }\nfn invalid() -> bool { fs::open() }",
            "module gpu { fn device_create() -> u8 { 1u8 } }\nfn invalid() -> bool { gpu::device_create() }",
            "module outer { module fs { fn open() -> u8 { 1u8 } } fn invalid() -> bool { fs::open() } }",
            "module inner { use external::fs fn invalid() -> bool { fs::open(\"x\", 1) } }",
            "fn fs(path: String, flags: i64) -> i64 { 0 }\n\
             fn invalid() -> i64 { fs::open(\"x\", 1) }",
            "fn invalid() -> i64 {\n\
                 let fs = |path: String, flags: i64| { 0 };\n\
                 fs::open(\"x\", 1)\n\
             }",
            "fn invalid<T>(value: Vec<T>) -> T { std::vec::definitely_missing::<T>(value, 0) }",
        ] {
            let code = run_with_source(Path::new("unknown_host_intrinsic.cssl"), src);
            assert_eq!(
                format!("{code:?}"),
                expected,
                "unknown host intrinsic accepted: {src}"
            );
        }
    }
}

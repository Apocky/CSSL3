use std::path::Path;
use std::process::ExitCode;

use csslc::cli::{Backend, BuildArgs, EmitMode};
use csslc::commands::build;
use csslc::exit_code;

#[test]
fn p1a_bool_not_probe_emits_cranelift_object() {
    let input = Path::new("tests/fixtures/abi_bool_not_probe.cssl");
    let source = include_str!("fixtures/abi_bool_not_probe.cssl");
    let output = std::env::temp_dir().join(format!(
        "csslc_p1a_bool_not_{}.obj",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    let args = BuildArgs {
        input: input.to_path_buf(),
        output: Some(output.clone()),
        target: Some("x86_64-pc-windows-msvc".to_string()),
        emit: EmitMode::Object,
        opt_level: 0,
        backend: Backend::Cranelift,
        module_paths: Vec::new(),
    };
    let code = build::run_with_source(input, source, &args);
    assert_eq!(
        format!("{code:?}"),
        format!("{:?}", ExitCode::from(exit_code::SUCCESS))
    );
    assert!(output.exists());
    let _ = std::fs::remove_file(output);
}

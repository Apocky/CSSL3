//! § cli — argv parsing for `csslc`.
//!
//! Hand-rolled stage-0 parser ; clap is not a workspace dep yet.
//! Each subcommand has its own typed args struct so the dispatcher in
//! [`crate::run`] consumes a strongly-typed [`Command`].

use std::path::PathBuf;

// ───────────────────────────────────────────────────────────────────────
// § Command — top-level subcommand variants
// ───────────────────────────────────────────────────────────────────────

/// One of the `csslc` subcommands, each with its own arg shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `csslc build foo.cssl [-o foo.exe] [--target=...] [--emit=...] [--opt-level=N]`
    Build(BuildArgs),
    /// `csslc check foo.cssl`
    Check(CheckArgs),
    /// `csslc fmt foo.cssl`
    Fmt(FmtArgs),
    /// `csslc test [--update-golden]`
    Test(TestArgs),
    /// `csslc emit-mlir foo.cssl`
    EmitMlir(EmitMlirArgs),
    /// `csslc verify foo.cssl`
    Verify(VerifyArgs),
    /// `csslc version`
    Version,
    /// `csslc help` / `csslc -h` / `csslc --help`
    Help,
}

/// `build` subcommand args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildArgs {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub emit: EmitMode,
    pub opt_level: u8,
}

/// `check` subcommand args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckArgs {
    pub input: PathBuf,
}

/// `fmt` subcommand args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmtArgs {
    pub input: PathBuf,
}

/// `test` subcommand args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestArgs {
    pub update_golden: bool,
}

/// `emit-mlir` subcommand args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitMlirArgs {
    pub input: PathBuf,
}

/// `verify` subcommand args.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyArgs {
    pub input: PathBuf,
}

/// What the `build` subcommand should emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitMode {
    /// Cranelift-text (CLIF) — the default at stage-0 since real object
    /// emission lands in S6-A3.
    Mlir,
    /// SPIR-V binary (when GPU body lowering lands in S6-D1).
    Spirv,
    /// WGSL text (when S6-D4 lands).
    Wgsl,
    /// DXIL binary (when S6-D2 lands).
    Dxil,
    /// MSL text (when S6-D3 lands).
    Msl,
    /// Raw object (.o / .obj / .so) — wires through to S6-A3 once landed.
    Object,
    /// Final executable (S6-A4 wires the linker).
    Exe,
}

impl EmitMode {
    /// Default emit mode for `build` when no `--emit=...` is given.
    #[must_use]
    pub const fn default_for_build() -> Self {
        Self::Exe
    }

    /// Parse the `--emit=<mode>` argument value.
    ///
    /// # Errors
    /// Returns an error description if `s` doesn't match a known mode.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "mlir" => Ok(Self::Mlir),
            "spirv" => Ok(Self::Spirv),
            "wgsl" => Ok(Self::Wgsl),
            "dxil" => Ok(Self::Dxil),
            "msl" => Ok(Self::Msl),
            "object" | "obj" => Ok(Self::Object),
            "exe" | "executable" => Ok(Self::Exe),
            other => Err(format!(
                "unknown --emit value '{other}' \
                 (expected one of : mlir | spirv | wgsl | dxil | msl | object | exe)"
            )),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § parse — main entry-point
// ───────────────────────────────────────────────────────────────────────

/// Parse a full argv vector (including program name at `args[0]`) into a
/// typed [`Command`].
///
/// # Errors
/// Returns a human-readable error string for surfaces in CLI usage:
/// missing subcommand, unknown subcommand, missing input file, malformed
/// flag, etc.
pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() {
        return Err("missing argv (no program name)".to_string());
    }

    let rest: &[String] = if args.len() == 1 {
        return Err("missing subcommand".to_string());
    } else {
        &args[1..]
    };

    // Top-level help-flag detection works before subcommand dispatch.
    if rest
        .first()
        .map(|s| matches!(s.as_str(), "-h" | "--help" | "help"))
        .unwrap_or(false)
    {
        return Ok(Command::Help);
    }
    if rest
        .first()
        .map(|s| matches!(s.as_str(), "version" | "--version" | "-V"))
        .unwrap_or(false)
    {
        return Ok(Command::Version);
    }

    let subcommand = &rest[0];
    let sub_args = &rest[1..];
    match subcommand.as_str() {
        "build" => parse_build(sub_args).map(Command::Build),
        "check" => parse_check(sub_args).map(Command::Check),
        "fmt" => parse_fmt(sub_args).map(Command::Fmt),
        "test" => parse_test(sub_args).map(Command::Test),
        "emit-mlir" => parse_emit_mlir(sub_args).map(Command::EmitMlir),
        "verify" => parse_verify(sub_args).map(Command::Verify),
        other => Err(format!(
            "unknown subcommand '{other}' \
             (try : build | check | fmt | test | emit-mlir | verify | version | help)"
        )),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § per-subcommand parsers
// ───────────────────────────────────────────────────────────────────────

fn parse_build(args: &[String]) -> Result<BuildArgs, String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut target: Option<String> = None;
    let mut emit: Option<EmitMode> = None;
    let mut opt_level: u8 = 0;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-o" | "--output" => {
                i += 1;
                if i >= args.len() {
                    return Err(format!("'{a}' requires a value"));
                }
                output = Some(PathBuf::from(&args[i]));
            }
            s if s.starts_with("--output=") => {
                output = Some(PathBuf::from(s.trim_start_matches("--output=")));
            }
            s if s.starts_with("--target=") => {
                target = Some(s.trim_start_matches("--target=").to_string());
            }
            s if s.starts_with("--emit=") => {
                let v = s.trim_start_matches("--emit=");
                emit = Some(EmitMode::parse(v)?);
            }
            s if s.starts_with("--opt-level=") => {
                let v = s.trim_start_matches("--opt-level=");
                opt_level = v
                    .parse::<u8>()
                    .map_err(|e| format!("'--opt-level={v}' is not a valid u8 ({e})"))?;
                if opt_level > 3 {
                    return Err(format!("--opt-level must be 0..=3 (got {opt_level})"));
                }
            }
            s if s.starts_with('-') => {
                return Err(format!("unknown flag '{s}' for 'build' subcommand"));
            }
            _ => {
                if input.is_some() {
                    return Err(format!(
                        "'build' takes a single positional <input> ; \
                         already saw '{}', then '{}'",
                        input.as_ref().unwrap().display(),
                        a
                    ));
                }
                input = Some(PathBuf::from(a));
            }
        }
        i += 1;
    }

    let input = input.ok_or_else(|| "'build' requires <input.cssl>".to_string())?;
    Ok(BuildArgs {
        input,
        output,
        target,
        emit: emit.unwrap_or_else(EmitMode::default_for_build),
        opt_level,
    })
}

fn parse_check(args: &[String]) -> Result<CheckArgs, String> {
    let input = parse_single_input("check", args)?;
    Ok(CheckArgs { input })
}

fn parse_fmt(args: &[String]) -> Result<FmtArgs, String> {
    let input = parse_single_input("fmt", args)?;
    Ok(FmtArgs { input })
}

fn parse_test(args: &[String]) -> Result<TestArgs, String> {
    let mut update_golden = false;
    for a in args {
        match a.as_str() {
            "--update-golden" => update_golden = true,
            other => {
                return Err(format!("unknown arg '{other}' for 'test' subcommand"));
            }
        }
    }
    Ok(TestArgs { update_golden })
}

fn parse_emit_mlir(args: &[String]) -> Result<EmitMlirArgs, String> {
    let input = parse_single_input("emit-mlir", args)?;
    Ok(EmitMlirArgs { input })
}

fn parse_verify(args: &[String]) -> Result<VerifyArgs, String> {
    let input = parse_single_input("verify", args)?;
    Ok(VerifyArgs { input })
}

/// Helper for subcommands whose only positional is `<input.cssl>`.
fn parse_single_input(subcommand: &str, args: &[String]) -> Result<PathBuf, String> {
    let mut input: Option<PathBuf> = None;
    for a in args {
        if a.starts_with('-') {
            return Err(format!(
                "'{subcommand}' takes only <input.cssl> at stage-0 ; saw flag '{a}'"
            ));
        }
        if input.is_some() {
            return Err(format!(
                "'{subcommand}' takes a single positional <input> ; saw '{}' and '{}'",
                input.as_ref().unwrap().display(),
                a
            ));
        }
        input = Some(PathBuf::from(a));
    }
    input.ok_or_else(|| format!("'{subcommand}' requires <input.cssl>"))
}

// ───────────────────────────────────────────────────────────────────────
// § usage text
// ───────────────────────────────────────────────────────────────────────

/// The canonical `--help` text for `csslc`.
#[must_use]
pub fn usage() -> String {
    "csslc — CSSLv3 stage-0 compiler\n\
     \n\
     USAGE\n\
       csslc <subcommand> [args]\n\
     \n\
     SUBCOMMANDS\n\
       build       compile a .cssl source into an artifact\n\
       check       front-end + type-check, no emission\n\
       fmt         (stage-0 stub) format a .cssl source\n\
       test        (stage-0 stub) run project tests\n\
       emit-mlir   dump the lowered MIR for inspection\n\
       verify      run all walkers + SMT-translate\n\
       version     print the toolchain version\n\
       help        print this message\n\
     \n\
     BUILD FLAGS\n\
       -o, --output <path>      write the artifact here\n\
       --target=<triple>        Rust-style target triple (e.g., x86_64-pc-windows-msvc)\n\
       --emit=<mode>            mlir | spirv | wgsl | dxil | msl | object | exe\n\
       --opt-level=N            optimization level 0..=3 (default 0)\n\
     \n\
     EXAMPLES\n\
       csslc build hello.cssl -o hello.exe\n\
       csslc check stage1/hello_world.cssl\n\
       csslc emit-mlir scene.cssl > scene.mlir\n\
     "
    .to_string()
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        std::iter::once("csslc".to_string())
            .chain(parts.iter().map(|s| (*s).to_string()))
            .collect()
    }

    #[test]
    fn empty_argv_returns_error() {
        let r = parse(&[]);
        assert!(r.is_err());
    }

    #[test]
    fn no_subcommand_returns_error() {
        let r = parse(&["csslc".to_string()]);
        assert!(r.is_err());
    }

    #[test]
    fn unknown_subcommand_returns_error() {
        let r = parse(&argv(&["frobnicate"]));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unknown subcommand"));
    }

    #[test]
    fn help_subcommand_resolves() {
        assert_eq!(parse(&argv(&["help"])).unwrap(), Command::Help);
        assert_eq!(parse(&argv(&["-h"])).unwrap(), Command::Help);
        assert_eq!(parse(&argv(&["--help"])).unwrap(), Command::Help);
    }

    #[test]
    fn version_subcommand_resolves() {
        assert_eq!(parse(&argv(&["version"])).unwrap(), Command::Version);
        assert_eq!(parse(&argv(&["--version"])).unwrap(), Command::Version);
        assert_eq!(parse(&argv(&["-V"])).unwrap(), Command::Version);
    }

    #[test]
    fn build_basic() {
        let cmd = parse(&argv(&["build", "hello.cssl"])).unwrap();
        match cmd {
            Command::Build(args) => {
                assert_eq!(args.input, PathBuf::from("hello.cssl"));
                assert_eq!(args.output, None);
                assert_eq!(args.emit, EmitMode::Exe);
                assert_eq!(args.opt_level, 0);
            }
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn build_with_output_dash_o() {
        let cmd = parse(&argv(&["build", "hello.cssl", "-o", "hello.exe"])).unwrap();
        match cmd {
            Command::Build(args) => assert_eq!(args.output, Some(PathBuf::from("hello.exe"))),
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn build_with_output_long_form() {
        let cmd = parse(&argv(&["build", "hello.cssl", "--output=hello.exe"])).unwrap();
        match cmd {
            Command::Build(args) => assert_eq!(args.output, Some(PathBuf::from("hello.exe"))),
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn build_with_target() {
        let cmd = parse(&argv(&[
            "build",
            "hello.cssl",
            "--target=x86_64-pc-windows-msvc",
        ]))
        .unwrap();
        match cmd {
            Command::Build(args) => {
                assert_eq!(args.target, Some("x86_64-pc-windows-msvc".to_string()));
            }
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn build_with_emit() {
        let cmd = parse(&argv(&["build", "hello.cssl", "--emit=mlir"])).unwrap();
        match cmd {
            Command::Build(args) => assert_eq!(args.emit, EmitMode::Mlir),
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn build_with_unknown_emit_returns_error() {
        let r = parse(&argv(&["build", "hello.cssl", "--emit=yaml"]));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unknown --emit value"));
    }

    #[test]
    fn build_with_opt_level() {
        let cmd = parse(&argv(&["build", "hello.cssl", "--opt-level=2"])).unwrap();
        match cmd {
            Command::Build(args) => assert_eq!(args.opt_level, 2),
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn build_with_invalid_opt_level_returns_error() {
        let r = parse(&argv(&["build", "hello.cssl", "--opt-level=9"]));
        assert!(r.is_err());
    }

    #[test]
    fn build_without_input_returns_error() {
        let r = parse(&argv(&["build"]));
        assert!(r.is_err());
    }

    #[test]
    fn build_with_two_inputs_returns_error() {
        let r = parse(&argv(&["build", "a.cssl", "b.cssl"]));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("single positional"));
    }

    #[test]
    fn build_with_unknown_flag_returns_error() {
        let r = parse(&argv(&["build", "hello.cssl", "--frobnicate"]));
        assert!(r.is_err());
    }

    #[test]
    fn check_basic() {
        let cmd = parse(&argv(&["check", "hello.cssl"])).unwrap();
        match cmd {
            Command::Check(args) => assert_eq!(args.input, PathBuf::from("hello.cssl")),
            other => panic!("expected Check, got {other:?}"),
        }
    }

    #[test]
    fn check_without_input_returns_error() {
        let r = parse(&argv(&["check"]));
        assert!(r.is_err());
    }

    #[test]
    fn fmt_basic() {
        let cmd = parse(&argv(&["fmt", "hello.cssl"])).unwrap();
        match cmd {
            Command::Fmt(args) => assert_eq!(args.input, PathBuf::from("hello.cssl")),
            other => panic!("expected Fmt, got {other:?}"),
        }
    }

    #[test]
    fn test_subcommand_basic() {
        let cmd = parse(&argv(&["test"])).unwrap();
        match cmd {
            Command::Test(args) => assert!(!args.update_golden),
            other => panic!("expected Test, got {other:?}"),
        }
    }

    #[test]
    fn test_subcommand_with_update_golden() {
        let cmd = parse(&argv(&["test", "--update-golden"])).unwrap();
        match cmd {
            Command::Test(args) => assert!(args.update_golden),
            other => panic!("expected Test, got {other:?}"),
        }
    }

    #[test]
    fn emit_mlir_basic() {
        let cmd = parse(&argv(&["emit-mlir", "hello.cssl"])).unwrap();
        match cmd {
            Command::EmitMlir(args) => assert_eq!(args.input, PathBuf::from("hello.cssl")),
            other => panic!("expected EmitMlir, got {other:?}"),
        }
    }

    #[test]
    fn verify_basic() {
        let cmd = parse(&argv(&["verify", "hello.cssl"])).unwrap();
        match cmd {
            Command::Verify(args) => assert_eq!(args.input, PathBuf::from("hello.cssl")),
            other => panic!("expected Verify, got {other:?}"),
        }
    }

    #[test]
    fn emit_mode_parses_canonical_strings() {
        assert_eq!(EmitMode::parse("mlir").unwrap(), EmitMode::Mlir);
        assert_eq!(EmitMode::parse("spirv").unwrap(), EmitMode::Spirv);
        assert_eq!(EmitMode::parse("wgsl").unwrap(), EmitMode::Wgsl);
        assert_eq!(EmitMode::parse("dxil").unwrap(), EmitMode::Dxil);
        assert_eq!(EmitMode::parse("msl").unwrap(), EmitMode::Msl);
        assert_eq!(EmitMode::parse("object").unwrap(), EmitMode::Object);
        assert_eq!(EmitMode::parse("obj").unwrap(), EmitMode::Object);
        assert_eq!(EmitMode::parse("exe").unwrap(), EmitMode::Exe);
        assert_eq!(EmitMode::parse("executable").unwrap(), EmitMode::Exe);
    }

    #[test]
    fn usage_text_mentions_all_subcommands() {
        let u = usage();
        for s in [
            "build",
            "check",
            "fmt",
            "test",
            "emit-mlir",
            "verify",
            "version",
            "help",
        ] {
            assert!(u.contains(s), "usage missing subcommand '{s}'");
        }
    }
}

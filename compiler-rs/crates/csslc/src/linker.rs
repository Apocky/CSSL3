//! § linker — subprocess linker invocation for `csslc build --emit=exe`
//! ════════════════════════════════════════════════════════════════════
//!
//! Spec : `specs/14_BACKEND.csl § LINKING-MODEL`.
//!
//! § ROLE
//!   Take one or more `.o` / `.obj` files produced by S6-A3 + the cssl-rt
//!   static archive (when wired) and link them into a runnable executable.
//!
//! § DETECTION ORDER  (S6-A4)
//!   1. `$CSSL_LINKER` env-var (user-override — explicit absolute path).
//!   2. Rustup-bundled `rust-lld` (LLVM linker) at the active toolchain's
//!      `rustlib/<triple>/bin/rust-lld[.exe]`. This is the most-portable
//!      stage-0 default since rustup ships it on every Rust install.
//!   3. `cl.exe` / `lld-link.exe` (Windows MSVC + LLVM toolchain).
//!   4. `clang` / `cc` / `gcc` (Unix toolchains).
//!   5. `link.exe` only as a last resort — Git-Bash's `/usr/bin/link.exe`
//!      is GNU `link(1)` (a `ln(1)`-alias), NOT MSVC `link.exe` ; we don't
//!      want to invoke that by mistake.
//!
//! § STAGE-0 SCOPE
//!   - Single object input → single executable output.
//!   - Console subsystem on Windows ; default subsystem elsewhere.
//!   - cssl-rt static-link is wired as an OPTIONAL extra-input parameter ;
//!     the hello.exe gate (S6-A5) doesn't need it because `fn main() -> i32
//!     { 42 }` calls no FFI symbols.
//!   - No shared-library production, no cross-compile.
//!
//! § FUTURE
//!   - lld-link target-triple plumbing (cross-compile from any host).
//!   - Static-link cssl-rt by default once it's emitted as a `staticlib`.
//!   - libpath inference for MSVC LIB env / Windows SDK ucrt.
//!   - Fat-binary assembly + dylib output.

use std::path::{Path, PathBuf};
use std::process::Command;

// ───────────────────────────────────────────────────────────────────────
// § LinkerKind — discriminated tag for invocation flavor
// ───────────────────────────────────────────────────────────────────────

/// Type of linker. Each kind maps to a different argument shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkerKind {
    /// MSVC `link.exe` discovered at a Visual Studio install with associated
    /// `/LIBPATH:...` directories pre-resolved (so the user does not need
    /// to be inside a Developer-Command-Prompt-style environment). Default
    /// libraries `libcmt.lib + libucrt.lib + kernel32.lib` are auto-added.
    MsvcLinkAuto {
        path: PathBuf,
        lib_paths: Vec<PathBuf>,
    },
    /// LLVM `rust-lld` from a rustup toolchain (`-flavor link` on Windows
    /// MSVC, `-flavor gnu` on Linux, `-flavor darwin` on macOS).
    RustLld {
        path: PathBuf,
        host_flavor: LldFlavor,
    },
    /// `lld-link.exe` (LLVM project, Windows MSVC linker driver).
    LldLink(PathBuf),
    /// MSVC `link.exe` (full path required ; Git-Bash's `/usr/bin/link.exe`
    /// is the GNU `link(1)` and is filtered out before we get here).
    MsvcLink(PathBuf),
    /// MSVC compiler-driver `cl.exe` (`cl /Fe`).
    MsvcCl(PathBuf),
    /// `clang` (Unix-y driver).
    Clang(PathBuf),
    /// `gcc` (Unix-y driver).
    Gcc(PathBuf),
    /// `cc` (Unix-y driver — typically a symlink to gcc/clang).
    Cc(PathBuf),
}

/// LLD subcommand flavor — controls the argument-style LLD presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LldFlavor {
    /// MSVC-style (`/OUT:foo.exe /SUBSYSTEM:CONSOLE foo.obj`).
    Link,
    /// GNU-ld-style (`-o foo foo.o`).
    Gnu,
    /// Apple-ld-style (`-arch x86_64 -o foo foo.o`).
    Darwin,
}

impl LldFlavor {
    /// Default flavor for the host. Uses `target_env` (msvc / gnu) on
    /// Windows so windows-gnu (MinGW / MSYS2) toolchains pick the GNU
    /// flavor and windows-msvc picks the MSVC flavor.
    #[must_use]
    pub const fn host_default() -> Self {
        if cfg!(all(target_os = "windows", target_env = "msvc")) {
            Self::Link
        } else if cfg!(target_os = "macos") {
            Self::Darwin
        } else {
            // windows-gnu / linux-* / *-bsd ⇒ GNU flavor
            Self::Gnu
        }
    }

    /// `-flavor` argument value for `rust-lld`.
    #[must_use]
    pub const fn flavor_arg(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Gnu => "gnu",
            Self::Darwin => "darwin",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § LinkError — failure modes
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum LinkError {
    /// No linker found on PATH or via rustup. Includes a hint about
    /// `$CSSL_LINKER` env-var override.
    NotFound { tried: Vec<String> },
    /// Linker binary was found but failed to launch (e.g., perms, ENOEXEC).
    SpawnFailed { binary: String, err: std::io::Error },
    /// Linker exited with non-zero status. `stderr` captured.
    NonZeroExit {
        binary: String,
        status: Option<i32>,
        stderr: String,
    },
    /// `$CSSL_LINKER` override was set but the path does not exist or is
    /// not executable.
    OverrideUnusable { path: String },
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { tried } => {
                write!(
                    f,
                    "no linker found ; tried: {}\n\
                     hint : set $CSSL_LINKER to an absolute path",
                    tried.join(", ")
                )
            }
            Self::SpawnFailed { binary, err } => {
                write!(f, "linker `{binary}` failed to spawn : {err}")
            }
            Self::NonZeroExit {
                binary,
                status,
                stderr,
            } => write!(
                f,
                "linker `{binary}` exited {} :\n{}",
                status.map_or_else(|| "(signal)".to_string(), |s| s.to_string()),
                stderr,
            ),
            Self::OverrideUnusable { path } => {
                write!(
                    f,
                    "$CSSL_LINKER='{path}' is not an existing executable file"
                )
            }
        }
    }
}

impl std::error::Error for LinkError {}

// ───────────────────────────────────────────────────────────────────────
// § detect_linker — walk env + PATH + rustup
// ───────────────────────────────────────────────────────────────────────

/// Attempt to find a working linker. Order : `$CSSL_LINKER` override →
/// rustup `rust-lld` → `lld-link` → `clang` → `gcc` → `cc` → MSVC `cl.exe`
/// → MSVC `link.exe` (excluding GNU-coreutils `/usr/bin/link.exe`).
///
/// # Errors
/// Returns [`LinkError::NotFound`] if no linker matched. Returns
/// [`LinkError::OverrideUnusable`] if `$CSSL_LINKER` is set but bogus.
pub fn detect_linker() -> Result<LinkerKind, LinkError> {
    // § 1. Honor explicit override.
    if let Ok(p) = std::env::var("CSSL_LINKER") {
        let pb = PathBuf::from(&p);
        if !pb.exists() {
            return Err(LinkError::OverrideUnusable { path: p });
        }
        // Best-effort flavor inference from the file name.
        let stem = pb.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        return Ok(match stem.to_ascii_lowercase().as_str() {
            "lld-link" => LinkerKind::LldLink(pb),
            "link" => LinkerKind::MsvcLink(pb),
            "cl" => LinkerKind::MsvcCl(pb),
            "clang" => LinkerKind::Clang(pb),
            "gcc" => LinkerKind::Gcc(pb),
            "cc" => LinkerKind::Cc(pb),
            // "rust-lld" or any unrecognized stem ⇒ assume rust-lld-style.
            _ => LinkerKind::RustLld {
                path: pb,
                host_flavor: LldFlavor::host_default(),
            },
        });
    }

    let mut tried = Vec::new();

    // § 2. MSVC link.exe with auto-resolved lib paths (Windows preferred).
    if cfg!(target_os = "windows") {
        if let Some(info) = find_msvc_link_auto() {
            return Ok(LinkerKind::MsvcLinkAuto {
                path: info.link_exe,
                lib_paths: info.lib_paths,
            });
        }
        tried.push("MSVC link.exe (auto-discovered)".to_string());
    }

    // § 3. Rustup-bundled rust-lld.
    if let Some(p) = find_rust_lld() {
        return Ok(LinkerKind::RustLld {
            path: p,
            host_flavor: LldFlavor::host_default(),
        });
    }
    tried.push("rust-lld".to_string());

    // § 3. PATH-resident binaries (in priority order).
    if let Some(p) = which("lld-link") {
        return Ok(LinkerKind::LldLink(p));
    }
    tried.push("lld-link".to_string());

    if let Some(p) = which("clang") {
        return Ok(LinkerKind::Clang(p));
    }
    tried.push("clang".to_string());

    if let Some(p) = which("gcc") {
        return Ok(LinkerKind::Gcc(p));
    }
    tried.push("gcc".to_string());

    // `cc` is sometimes a symlink to gcc/clang on Unix.
    if let Some(p) = which("cc") {
        return Ok(LinkerKind::Cc(p));
    }
    tried.push("cc".to_string());

    // § 4. MSVC. Skip `link.exe` if it resolves to GNU coreutils.
    if let Some(p) = which("cl") {
        return Ok(LinkerKind::MsvcCl(p));
    }
    tried.push("cl".to_string());
    if let Some(p) = which("link") {
        if !is_likely_gnu_link(&p) {
            return Ok(LinkerKind::MsvcLink(p));
        }
    }
    tried.push("link.exe (MSVC)".to_string());

    Err(LinkError::NotFound { tried })
}

/// True iff this `link` binary lives under a path that suggests it is the
/// GNU coreutils `link(1)` rather than MSVC `link.exe`.
fn is_likely_gnu_link(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    s.contains("/usr/bin/")
        || s.contains("\\usr\\bin\\")
        || s.contains("git/usr/bin")
        || s.contains("msys")
        || s.contains("mingw")
        || s.contains("cygwin")
}

/// Search PATH for an executable. Returns its absolute path, or None.
fn which(stem: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let candidate = if ext.is_empty() {
                dir.join(stem)
            } else {
                dir.join(format!("{stem}{ext}"))
            };
            if candidate.is_file() && (cfg!(windows) || is_executable(&candidate)) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    p.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_p: &Path) -> bool {
    true
}

/// MSVC link.exe + the lib-paths it needs to resolve libcmt / libucrt /
/// kernel32 against, all discovered without any active dev-shell env.
struct MsvcLinkInfo {
    link_exe: PathBuf,
    lib_paths: Vec<PathBuf>,
}

/// Walk the standard Visual Studio + Windows SDK install paths to find
/// `link.exe` plus the required `/LIBPATH:...` directories. Returns
/// `None` if any required component is missing.
fn find_msvc_link_auto() -> Option<MsvcLinkInfo> {
    // § 1. Find the most-recent VS install with x64 link.exe.
    let mut vs_roots = Vec::new();
    for prefix in [
        r"C:\Program Files\Microsoft Visual Studio",
        r"C:\Program Files (x86)\Microsoft Visual Studio",
    ] {
        let root = Path::new(prefix);
        if !root.exists() {
            continue;
        }
        // Each year (2017 / 2019 / 2022 / 18 / …) is a subdir.
        for year_entry in std::fs::read_dir(root).ok()?.flatten() {
            // Each year contains an edition (Community / Professional / Enterprise / BuildTools / Preview).
            for edition_entry in std::fs::read_dir(year_entry.path()).ok()?.flatten() {
                let msvc_root = edition_entry.path().join("VC").join("Tools").join("MSVC");
                if msvc_root.is_dir() {
                    vs_roots.push(msvc_root);
                }
            }
        }
    }

    let mut best_link: Option<(PathBuf, PathBuf)> = None; // (link.exe, vc_lib_x64)
    for vs_root in &vs_roots {
        let entries = std::fs::read_dir(vs_root).ok()?;
        for ver in entries.flatten() {
            let link_exe = ver
                .path()
                .join("bin")
                .join("Hostx64")
                .join("x64")
                .join("link.exe");
            let lib_x64 = ver.path().join("lib").join("x64");
            if link_exe.is_file() && lib_x64.is_dir() {
                // Pick the lexicographically-largest (newest version).
                if best_link
                    .as_ref()
                    .map_or(true, |(_, prev_lib)| lib_x64 > *prev_lib)
                {
                    best_link = Some((link_exe, lib_x64));
                }
            }
        }
    }
    let (link_exe, vc_lib_x64) = best_link?;

    // § 2. Find the latest Windows SDK lib version with ucrt + um for x64.
    let sdk_lib_root = Path::new(r"C:\Program Files (x86)\Windows Kits\10\Lib");
    let mut best_sdk: Option<(PathBuf, PathBuf)> = None; // (ucrt_x64, um_x64)
    if sdk_lib_root.is_dir() {
        for ver in std::fs::read_dir(sdk_lib_root).ok()?.flatten() {
            let ucrt = ver.path().join("ucrt").join("x64");
            let um = ver.path().join("um").join("x64");
            if ucrt.is_dir()
                && um.is_dir()
                && best_sdk.as_ref().map_or(true, |(prev, _)| ucrt > *prev)
            {
                best_sdk = Some((ucrt, um));
            }
        }
    }
    let (ucrt_x64, um_x64) = best_sdk?;

    Some(MsvcLinkInfo {
        link_exe,
        lib_paths: vec![vc_lib_x64, ucrt_x64, um_x64],
    })
}

/// Locate `rust-lld` under the active rustup toolchain.
fn find_rust_lld() -> Option<PathBuf> {
    // Use `rustc --print=sysroot` to find the toolchain root, then walk
    // `rustlib/<triple>/bin/rust-lld[.exe]`.
    let out = Command::new("rustc").arg("--print=sysroot").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let sysroot = String::from_utf8(out.stdout).ok()?.trim().to_string();
    let rustlib = Path::new(&sysroot).join("lib").join("rustlib");
    let exe = if cfg!(windows) {
        "rust-lld.exe"
    } else {
        "rust-lld"
    };
    let entries = std::fs::read_dir(&rustlib).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin").join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

// ───────────────────────────────────────────────────────────────────────
// § build_command — synthesize the subprocess Command
// ───────────────────────────────────────────────────────────────────────

/// Construct a `Command` for the given linker + inputs + output.
///
/// `extra_libs` may include things like `cssl-rt.lib` once cssl-rt ships
/// as a staticlib. At stage-0 hello.exe (S6-A5) leaves it empty.
pub fn build_command(
    kind: &LinkerKind,
    object_inputs: &[PathBuf],
    output: &Path,
    extra_libs: &[String],
) -> Command {
    match kind {
        LinkerKind::MsvcLinkAuto { path, lib_paths } => {
            let mut cmd = Command::new(path);
            cmd.arg(format!("/OUT:{}", output.display()));
            cmd.arg("/SUBSYSTEM:CONSOLE");
            cmd.arg("/NOLOGO");
            for lp in lib_paths {
                cmd.arg(format!("/LIBPATH:{}", lp.display()));
            }
            // Default libraries needed for a vanilla `int main()` C ABI.
            cmd.arg("libcmt.lib");
            cmd.arg("libucrt.lib");
            cmd.arg("kernel32.lib");
            for o in object_inputs {
                cmd.arg(o);
            }
            for lib in extra_libs {
                cmd.arg(lib);
            }
            cmd
        }
        LinkerKind::RustLld { path, host_flavor } => {
            let mut cmd = Command::new(path);
            cmd.arg("-flavor").arg(host_flavor.flavor_arg());
            apply_lld_args(&mut cmd, *host_flavor, object_inputs, output, extra_libs);
            cmd
        }
        LinkerKind::LldLink(path) | LinkerKind::MsvcLink(path) => {
            let mut cmd = Command::new(path);
            apply_lld_args(&mut cmd, LldFlavor::Link, object_inputs, output, extra_libs);
            cmd
        }
        LinkerKind::MsvcCl(path) => {
            // cl.exe driver mode : `cl /Fe<output> input.obj`.
            let mut cmd = Command::new(path);
            cmd.arg(format!("/Fe:{}", output.display()));
            for o in object_inputs {
                cmd.arg(o);
            }
            for lib in extra_libs {
                cmd.arg(lib);
            }
            cmd
        }
        LinkerKind::Clang(path) | LinkerKind::Gcc(path) | LinkerKind::Cc(path) => {
            let mut cmd = Command::new(path);
            cmd.arg("-o").arg(output);
            for o in object_inputs {
                cmd.arg(o);
            }
            for lib in extra_libs {
                cmd.arg(format!("-l{lib}"));
            }
            cmd
        }
    }
}

fn apply_lld_args(
    cmd: &mut Command,
    flavor: LldFlavor,
    object_inputs: &[PathBuf],
    output: &Path,
    extra_libs: &[String],
) {
    match flavor {
        LldFlavor::Link => {
            cmd.arg(format!("/OUT:{}", output.display()));
            cmd.arg("/SUBSYSTEM:CONSOLE");
            for o in object_inputs {
                cmd.arg(o);
            }
            for lib in extra_libs {
                cmd.arg(lib);
            }
        }
        LldFlavor::Gnu | LldFlavor::Darwin => {
            cmd.arg("-o").arg(output);
            for o in object_inputs {
                cmd.arg(o);
            }
            for lib in extra_libs {
                cmd.arg(format!("-l{lib}"));
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § link — high-level "do everything" entry
// ───────────────────────────────────────────────────────────────────────

/// Link `object_inputs` into `output`. Detects a linker, builds the
/// command, runs the subprocess, captures stderr.
///
/// # Errors
/// Bubbles up [`LinkError`] on detection / spawn / exit-status problems.
pub fn link(
    object_inputs: &[PathBuf],
    output: &Path,
    extra_libs: &[String],
) -> Result<(), LinkError> {
    let kind = detect_linker()?;
    let mut cmd = build_command(&kind, object_inputs, output, extra_libs);
    let display = format!("{kind:?}");
    let out = cmd.output().map_err(|e| LinkError::SpawnFailed {
        binary: display.clone(),
        err: e,
    })?;
    if out.status.success() {
        return Ok(());
    }
    Err(LinkError::NonZeroExit {
        binary: display,
        status: out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

// ───────────────────────────────────────────────────────────────────────
// § tests — detection logic + command-shape only ; no actual linking
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lld_flavor_host_default_is_platform_appropriate() {
        let f = LldFlavor::host_default();
        if cfg!(all(target_os = "windows", target_env = "msvc")) {
            assert_eq!(f, LldFlavor::Link);
        } else if cfg!(target_os = "macos") {
            assert_eq!(f, LldFlavor::Darwin);
        } else {
            // windows-gnu / linux-* / *-bsd ⇒ GNU flavor
            assert_eq!(f, LldFlavor::Gnu);
        }
    }

    #[test]
    fn lld_flavor_arg_strings() {
        assert_eq!(LldFlavor::Link.flavor_arg(), "link");
        assert_eq!(LldFlavor::Gnu.flavor_arg(), "gnu");
        assert_eq!(LldFlavor::Darwin.flavor_arg(), "darwin");
    }

    #[test]
    fn build_command_for_rust_lld_link_includes_out_arg() {
        let kind = LinkerKind::RustLld {
            path: PathBuf::from("rust-lld"),
            host_flavor: LldFlavor::Link,
        };
        let cmd = build_command(
            &kind,
            &[PathBuf::from("hello.obj")],
            Path::new("hello.exe"),
            &[],
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(args.contains(&"-flavor".to_string()));
        assert!(args.contains(&"link".to_string()));
        assert!(args.iter().any(|a| a.starts_with("/OUT:")));
        assert!(args.contains(&"/SUBSYSTEM:CONSOLE".to_string()));
        assert!(args.iter().any(|a| a.contains("hello.obj")));
    }

    #[test]
    fn build_command_for_rust_lld_gnu_uses_dash_o() {
        let kind = LinkerKind::RustLld {
            path: PathBuf::from("rust-lld"),
            host_flavor: LldFlavor::Gnu,
        };
        let cmd = build_command(&kind, &[PathBuf::from("hello.o")], Path::new("hello"), &[]);
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(args.contains(&"-flavor".to_string()));
        assert!(args.contains(&"gnu".to_string()));
        assert!(args.contains(&"-o".to_string()));
    }

    #[test]
    fn build_command_for_msvc_cl_uses_fe_arg() {
        let kind = LinkerKind::MsvcCl(PathBuf::from("cl.exe"));
        let cmd = build_command(
            &kind,
            &[PathBuf::from("hello.obj")],
            Path::new("hello.exe"),
            &[],
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(args.iter().any(|a| a.starts_with("/Fe:")));
    }

    #[test]
    fn build_command_for_clang_uses_dash_o() {
        let kind = LinkerKind::Clang(PathBuf::from("clang"));
        let cmd = build_command(&kind, &[PathBuf::from("hello.o")], Path::new("hello"), &[]);
        assert!(
            cmd.get_args()
                .any(|s| s.to_string_lossy() == "-o")
        );
    }

    #[test]
    fn build_command_propagates_extra_libs_for_clang() {
        let kind = LinkerKind::Clang(PathBuf::from("clang"));
        let cmd = build_command(
            &kind,
            &[PathBuf::from("hello.o")],
            Path::new("hello"),
            &["c".to_string(), "m".to_string()],
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(args.contains(&"-lc".to_string()));
        assert!(args.contains(&"-lm".to_string()));
    }

    #[test]
    fn build_command_propagates_extra_libs_for_lld_link() {
        let kind = LinkerKind::LldLink(PathBuf::from("lld-link"));
        let cmd = build_command(
            &kind,
            &[PathBuf::from("hello.obj")],
            Path::new("hello.exe"),
            &["libcmt.lib".to_string()],
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(args.iter().any(|a| a == "libcmt.lib"));
    }

    #[test]
    fn is_likely_gnu_link_recognizes_usr_bin() {
        assert!(is_likely_gnu_link(Path::new("/usr/bin/link.exe")));
        assert!(is_likely_gnu_link(Path::new(
            "C:\\Program Files\\Git\\usr\\bin\\link.exe"
        )));
        assert!(is_likely_gnu_link(Path::new("C:/msys64/usr/bin/link.exe")));
    }

    #[test]
    fn is_likely_gnu_link_does_not_match_msvc_path() {
        assert!(!is_likely_gnu_link(Path::new(
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC\\14.40\\bin\\Hostx64\\x64\\link.exe"
        )));
    }

    #[test]
    fn link_error_display_is_human_readable() {
        let e = LinkError::NotFound {
            tried: vec!["rust-lld".to_string(), "clang".to_string()],
        };
        let s = format!("{e}");
        assert!(s.contains("no linker found"));
        assert!(s.contains("rust-lld"));
        assert!(s.contains("CSSL_LINKER"));
    }

    #[test]
    fn link_error_override_unusable_includes_path() {
        let e = LinkError::OverrideUnusable {
            path: "/nonexistent/foo".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("/nonexistent/foo"));
    }

    #[test]
    fn detect_linker_finds_something_on_this_machine() {
        // This integration-level test is best-effort : on the developer's
        // machine we expect rust-lld to be present via rustup. CI runners
        // also have rust toolchains. If detection fails, the test reports
        // what was tried so the failure is actionable.
        match detect_linker() {
            Ok(kind) => {
                eprintln!("detected linker : {kind:?}");
            }
            Err(LinkError::NotFound { tried }) => {
                eprintln!("no linker on this host ; tried : {tried:?}");
                // Don't fail the test : on minimal CI / sandbox envs there
                // may genuinely be no linker. The detection logic itself
                // is exercised by the other unit tests.
            }
            Err(other) => panic!("unexpected detection error : {other}"),
        }
    }
}

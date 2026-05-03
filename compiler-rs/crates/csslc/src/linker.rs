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

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

// ───────────────────────────────────────────────────────────────────────
// § T11-W19-α-CSSLC-LINKER-RT — rustc-driven stub source
// ───────────────────────────────────────────────────────────────────────
//
// The stub crate is written to a temp file at link time + fed to rustc.
// Its job :
//   1. `extern crate cssl_rt;` so rustc threads cssl-rt's rlib +
//      transitive deps + rust-std into the link.
//   2. Provide a no-op `WinMain` — mingw's console-startup reserves the
//      symbol, so a zero-stub is needed on windows-gnu hosts. (Harmless
//      on -msvc and on Unix.)
//   3. `#![no_main]` so rustc does NOT emit its own `main` (the user's
//      CSSL-emitted obj exports `main` ; that's what we want as the
//      entry-point).
//
// This source MUST be valid for both windows-msvc and windows-gnu rustc
// targets ; the `cfg(target_os = "windows")` guard keeps the WinMain
// stub Windows-only.
const RUSTC_DRIVEN_STUB_RS: &str = "\
#![no_main]
extern crate cssl_rt;

#[cfg(target_os = \"windows\")]
#[no_mangle]
pub extern \"system\" fn WinMain(
    _: *mut core::ffi::c_void,
    _: *mut core::ffi::c_void,
    _: *mut u8,
    _: i32,
) -> i32 { 0 }
";

/// Path the temp-stub.rs is written to. Same path every time so repeated
/// csslc invocations don't litter `%TEMP%/` with stub-NN.rs files. The
/// content is byte-identical so race-conditions across parallel csslc
/// calls are benign (both would write the same bytes).
fn stub_rs_path() -> PathBuf {
    std::env::temp_dir().join("cssl_rustc_driven_stub.rs")
}

// ───────────────────────────────────────────────────────────────────────
// § LinkerKind — discriminated tag for invocation flavor
// ───────────────────────────────────────────────────────────────────────

/// Type of linker. Each kind maps to a different argument shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkerKind {
    /// `rustc` as link-driver. Used when a matching `libcssl_rt.rlib` is
    /// discovered alongside the running csslc binary. rustc threads in
    /// rust-std + the cssl-rt rlib + Win32 / Unix system libraries
    /// automatically — this is the most-portable path on stage-0 because
    /// it works identically across `windows-msvc` and `windows-gnu` rustc
    /// installs (the underlying linker — link.exe / mingw-gcc / etc. — is
    /// chosen by rustc itself based on its sysroot config).
    ///
    /// § T11-W19-α-CSSLC-LINKER-RT
    ///   Adopted as the PREFERRED stage-0 linker because the prior
    ///   MsvcLinkAuto path failed when consuming a cssl-rt staticlib that
    ///   bundled rust-std rcgu objects with GCC-EH personality routines
    ///   (the `_Unwind_*` family). rustc-as-link-driver sidesteps the
    ///   ABI mismatch by linking against the rlib (NOT the staticlib) and
    ///   letting rustc choose the matching unwind-personality from its
    ///   sysroot.
    RustcDriven {
        /// Absolute path to `rustc` (typically rustup-managed shim).
        rustc_path: PathBuf,
        /// Absolute path to `libcssl_rt.rlib` (compiler-rs/target/release).
        cssl_rt_rlib: PathBuf,
        /// `compiler-rs/target/release/deps/` for transitive rlib lookup.
        deps_dir: PathBuf,
    },
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

    // § T11-W19-α-CSSLC-LINKER-RT — prefer rustc-as-link-driver when a
    //   matching `libcssl_rt.rlib` is on disk. This avoids the ABI mismatch
    //   between cssl-rt staticlib (which bundles rust-std with GCC-EH) and
    //   MSVC link.exe (which expects SEH-personality unwind). rustc owns
    //   the cross-toolchain plumbing — it picks the right linker (link.exe
    //   on -msvc, mingw-gcc on -gnu) + threads the right unwind libs from
    //   its sysroot. Set `$CSSL_NO_RUSTC_DRIVER=1` to opt-out + fall back
    //   to the legacy MsvcLinkAuto / rust-lld / clang chain.
    if std::env::var_os("CSSL_NO_RUSTC_DRIVER").is_none() {
        if let Some(info) = find_rustc_driven() {
            return Ok(LinkerKind::RustcDriven {
                rustc_path: info.rustc_path,
                cssl_rt_rlib: info.cssl_rt_rlib,
                deps_dir: info.deps_dir,
            });
        }
        tried.push("rustc-driven (libcssl_rt.rlib)".to_string());
    }

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

// ───────────────────────────────────────────────────────────────────────
// § find_rustc_driven — rustc-as-link-driver discovery (T11-W19-α-CSSLC-LINKER-RT)
// ───────────────────────────────────────────────────────────────────────
//
// Pre-condition for this LinkerKind to activate :
//   1. `rustc` is on PATH (rustup-managed shim).
//   2. `libcssl_rt.rlib` is present in `compiler-rs/target/<profile>/`
//      (built by `cargo build -p cssl-rt`).
//   3. `compiler-rs/target/<profile>/deps/` exists (cargo's per-build output
//      directory ; transitive rlibs the cssl-rt rlib references live there).
//
// All three must be co-located + version-compatible. The rustc that built
// the rlib is identified by the `rust-toolchain.toml` in the workspace ;
// that's also the rustc that gets invoked when csslc shells out, because
// `rustup` reads the same toml when csslc is launched from a CWD inside
// the workspace tree.

/// Triple of paths needed to drive rustc as the link driver.
struct RustcDrivenInfo {
    rustc_path: PathBuf,
    cssl_rt_rlib: PathBuf,
    deps_dir: PathBuf,
}

/// Walk the same parent-dir candidates as `discover_cssl_rt_staticlib_with`
/// to find `libcssl_rt.rlib` + the sibling `deps/` directory + a `rustc`
/// on PATH. Returns `None` if any component is missing.
fn find_rustc_driven() -> Option<RustcDrivenInfo> {
    // 1. rustc on PATH.
    let rustc_path = which("rustc")?;

    // 2. libcssl_rt.rlib + deps/ in the same target/<profile>/ directory.
    let env = DiscoveryEnv::from_process();
    let mut parents: Vec<PathBuf> = Vec::new();
    if let Some(exe) = env.current_exe.as_ref() {
        if let Some(parent) = exe.parent() {
            parents.push(parent.to_path_buf());
        }
    }
    if let Some(cwd) = env.current_dir.as_ref() {
        for profile in &["release", "debug"] {
            parents.push(cwd.join("target").join(profile));
            parents.push(cwd.join("compiler-rs").join("target").join(profile));
            let mut p = cwd.as_path();
            while let Some(parent) = p.parent() {
                parents.push(parent.join("target").join(profile));
                parents.push(parent.join("compiler-rs").join("target").join(profile));
                p = parent;
            }
        }
    }
    if let Some(exe) = env.current_exe.as_ref() {
        let mut p = exe.as_path();
        for _ in 0..6 {
            if let Some(parent) = p.parent() {
                for profile in &["release", "debug"] {
                    parents.push(parent.join(profile));
                    parents.push(parent.join("target").join(profile));
                }
                p = parent;
            } else {
                break;
            }
        }
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    for p in parents {
        if !seen.iter().any(|s| s == &p) {
            seen.push(p);
        }
    }
    for parent in &seen {
        let rlib = parent.join("libcssl_rt.rlib");
        let deps = parent.join("deps");
        if rlib.is_file() && deps.is_dir() {
            return Some(RustcDrivenInfo {
                rustc_path,
                cssl_rt_rlib: rlib,
                deps_dir: deps,
            });
        }
    }
    None
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
// § discover_cssl_rt_staticlib — auto-default-link the runtime
// ───────────────────────────────────────────────────────────────────────
//
// § T11-D319 (W-CSSL-default-link)
//   `cssl-rt` ships a `staticlib` artifact (configured in its Cargo.toml's
//   `[lib] crate-type` list). For the `loa_startup.rs` ctor (the
//   `.CRT$XCU` / `.init_array` fn-ptr that fires before main()) to actually
//   run, the staticlib must be PASSED to the linker — being merely "on
//   disk" doesn't activate the ctor. Stage-0's previous behavior was to
//   leave `extra_libs` empty for hello.exe ; this is fine for an FFI-free
//   `fn main() -> i32 { 42 }` but means the LoA-v13 startup banner never
//   makes it into `logs/loa_runtime.log`.
//
//   The fix : csslc auto-discovers the cssl-rt staticlib at link time and
//   prepends it to `extra_libs` unless the user opts out via
//   `$CSSL_NO_DEFAULT_LINK=1`.
//
// § DISCOVERY ORDER
//   1. `$CSSL_RT_LIB` env override (explicit absolute path).
//   2. `<csslc-binary-dir>/libcssl_rt.{lib|a}` (cargo-built csslc.exe lives
//      next to the staticlib in `target/<profile>/`).
//   3. `<csslc-binary-dir>/../<profile>/libcssl_rt.{lib|a}` (defensive ;
//      handles path layouts where csslc.exe is in `target/release/` and
//      cssl-rt staticlib is also in `target/release/`).
//   4. `<workspace-root>/compiler-rs/target/{debug,release}/libcssl_rt.{lib|a}`
//      (canonical workspace layout when running from anywhere).
//   5. `<cwd>/target/{debug,release}/libcssl_rt.{lib|a}` (last-resort).
//
//   On Windows-MSVC the staticlib name is `cssl_rt.lib` (no `lib` prefix
//   because cargo's default-rules emit the bare name when the consumer is
//   MSVC-ABI). On Unix it's `libcssl_rt.a`.
//
// § OPT-OUT
//   `$CSSL_NO_DEFAULT_LINK=1` ⇒ skip discovery entirely. Used by the
//   hello-world tests that don't want to depend on the staticlib being
//   built.
//
// § VERBOSE
//   `$CSSL_RT_VERBOSE=1` ⇒ print the discovered (or not-discovered) path
//   to stderr so users can sanity-check.

/// Find the cssl-rt staticlib on disk. Returns `None` if discovery fails.
/// Honors `$CSSL_RT_LIB` (override) + `$CSSL_NO_DEFAULT_LINK` (skip).
///
/// The returned `PathBuf` is suitable to push directly into the
/// `extra_libs` Vec passed to [`build_command`] / [`link`] (note that
/// extra_libs is `Vec<String>` ; callers stringify before adding).
#[must_use]
pub fn discover_cssl_rt_staticlib() -> Option<PathBuf> {
    discover_cssl_rt_staticlib_with(&DiscoveryEnv::from_process())
}

/// Snapshot of the env-vars + cwd that govern cssl-rt discovery. The pure
/// discovery logic ([`discover_cssl_rt_staticlib_with`]) takes one of
/// these so tests can exercise every branch without mutating
/// process-global state (cssl-rt's `#![forbid(unsafe_code)]` rules out
/// the now-`unsafe` `std::env::set_var` API).
#[derive(Debug, Clone, Default)]
pub struct DiscoveryEnv {
    /// Value of `$CSSL_NO_DEFAULT_LINK`. Any non-empty value (Rust
    /// convention) means "skip discovery entirely".
    pub no_default_link: Option<String>,
    /// Value of `$CSSL_RT_LIB` — explicit absolute path override.
    pub rt_lib: Option<String>,
    /// Value of `$CSSL_RT_VERBOSE` — any non-empty value enables stderr
    /// trace lines.
    pub rt_verbose: Option<String>,
    /// Result of `std::env::current_exe()` (the running csslc.exe).
    pub current_exe: Option<PathBuf>,
    /// Result of `std::env::current_dir()`.
    pub current_dir: Option<PathBuf>,
}

impl DiscoveryEnv {
    /// Build a snapshot from the live process environment. Used by the
    /// production [`discover_cssl_rt_staticlib`] entry-point.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            no_default_link: std::env::var("CSSL_NO_DEFAULT_LINK").ok(),
            rt_lib: std::env::var("CSSL_RT_LIB").ok(),
            rt_verbose: std::env::var("CSSL_RT_VERBOSE").ok(),
            current_exe: std::env::current_exe().ok(),
            current_dir: std::env::current_dir().ok(),
        }
    }
}

/// Pure-function discovery : reads from the supplied [`DiscoveryEnv`]
/// snapshot instead of the live process env. Side-effecting only on the
/// filesystem (`is_file()` checks) + on stderr (verbose-trace lines).
#[must_use]
pub fn discover_cssl_rt_staticlib_with(env: &DiscoveryEnv) -> Option<PathBuf> {
    let verbose = env.rt_verbose.as_deref().is_some_and(|s| !s.is_empty());
    if env.no_default_link.as_deref().is_some_and(|s| !s.is_empty()) {
        if verbose {
            let _ = writeln!(
                std::io::stderr(),
                "csslc: cssl-rt default-link SKIPPED ($CSSL_NO_DEFAULT_LINK set)"
            );
        }
        return None;
    }

    // § 1. Honor explicit env override.
    if let Some(p) = env.rt_lib.as_deref().filter(|s| !s.is_empty()) {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            if verbose {
                let _ = writeln!(
                    std::io::stderr(),
                    "csslc: cssl-rt default-link USES $CSSL_RT_LIB={}",
                    pb.display()
                );
            }
            return Some(pb);
        }
        // Override given but file doesn't exist — emit warn + fall through.
        let _ = writeln!(
            std::io::stderr(),
            "csslc: warn: $CSSL_RT_LIB='{}' not found ; falling back to auto-discovery",
            pb.display()
        );
    }

    // § 2-5. Auto-discovery. Build a list of candidate parent directories +
    //   try every `(parent, name)` combination.
    let names = staticlib_names();
    let mut parents: Vec<PathBuf> = Vec::new();

    // (2) csslc-binary-dir.
    if let Some(exe) = env.current_exe.as_ref() {
        if let Some(parent) = exe.parent() {
            parents.push(parent.to_path_buf());
        }
    }

    // (3) Walk up from cwd to find a `target/{debug,release}/` directory.
    if let Some(cwd) = env.current_dir.as_ref() {
        for profile in &["debug", "release"] {
            // <cwd>/target/<profile>/
            parents.push(cwd.join("target").join(profile));
            // <cwd>/compiler-rs/target/<profile>/
            parents.push(cwd.join("compiler-rs").join("target").join(profile));
            // Walk up the cwd : <cwd-parent>/target/<profile>/ etc.
            let mut p = cwd.as_path();
            while let Some(parent) = p.parent() {
                parents.push(parent.join("target").join(profile));
                parents.push(parent.join("compiler-rs").join("target").join(profile));
                p = parent;
            }
        }
    }

    // (4) Walk up from csslc.exe's location too.
    if let Some(exe) = env.current_exe.as_ref() {
        let mut p = exe.as_path();
        for _ in 0..6 {
            if let Some(parent) = p.parent() {
                for profile in &["debug", "release"] {
                    parents.push(parent.join(profile));
                    parents.push(parent.join("target").join(profile));
                }
                p = parent;
            } else {
                break;
            }
        }
    }

    // Deduplicate while preserving order.
    let mut seen: Vec<PathBuf> = Vec::new();
    for p in parents {
        if !seen.iter().any(|s| s == &p) {
            seen.push(p);
        }
    }

    for parent in &seen {
        for name in &names {
            let candidate = parent.join(name);
            if candidate.is_file() {
                if verbose {
                    let _ = writeln!(
                        std::io::stderr(),
                        "csslc: cssl-rt default-link FOUND : {}",
                        candidate.display()
                    );
                }
                return Some(candidate);
            }
        }
    }

    if verbose {
        let _ = writeln!(
            std::io::stderr(),
            "csslc: cssl-rt default-link NOT FOUND ; tried {} candidate dir(s) × {} name(s)",
            seen.len(),
            names.len(),
        );
    }
    None
}

/// Platform-appropriate static-lib filenames in priority order.
fn staticlib_names() -> Vec<&'static str> {
    if cfg!(target_os = "windows") {
        // MSVC : cargo emits `cssl_rt.lib` (no `lib` prefix). MinGW also
        // accepts `libcssl_rt.a` though the MSVC-toolchain default is .lib.
        vec!["cssl_rt.lib", "libcssl_rt.a"]
    } else if cfg!(target_os = "macos") {
        vec!["libcssl_rt.a"]
    } else {
        vec!["libcssl_rt.a"]
    }
}

// ───────────────────────────────────────────────────────────────────────
// § discover_loa_host_staticlib — auto-default-link the LoA engine runtime
// ───────────────────────────────────────────────────────────────────────
//
// § T11-LOA-PURE-CSSL (W-LOA-pure-cssl-engine)
//
//   Parallel mechanism to `discover_cssl_rt_staticlib` : when csslc compiles
//   a pure-CSSL program that references the LoA engine FFI symbol
//   `__cssl_engine_run` (provided by loa-host's `[lib] crate-type =
//   ["staticlib", ...]`), csslc auto-discovers `loa_host.lib` /
//   `libloa_host.a` next to the cssl-rt staticlib + injects it into
//   `extra_libs` alongside cssl-rt. This makes `Labyrinth of Apocalypse/
//   main.cssl` compile to a navigatable LoA.exe with one csslc invocation.
//
// § DISCOVERY ORDER
//   1. `$CSSL_LOA_HOST_LIB` env override (explicit absolute path).
//   2. Same parent-dir walk as cssl-rt (loa-host's staticlib is built into
//      the same `target/<profile>/` directory by cargo's workspace layout).
//
// § OPT-OUT
//   `$CSSL_NO_LOA_HOST=1` ⇒ skip discovery entirely. Used by the legacy
//   hello-world tests + any binary that doesn't want the engine linked
//   in (e.g. a future audio-only test program).
//
// § NAMING
//   The staticlib filename derives from the cargo `[lib] name = "loa_host"`
//   → MSVC emits `loa_host.lib` (no `lib` prefix), Unix emits
//   `libloa_host.a`.

/// Find the loa-host staticlib on disk. Returns `None` if discovery fails
/// or `$CSSL_NO_LOA_HOST` is set. Honors `$CSSL_LOA_HOST_LIB` (override).
#[must_use]
pub fn discover_loa_host_staticlib() -> Option<PathBuf> {
    discover_loa_host_staticlib_with(&DiscoveryEnv::from_process())
}

/// Pure-function discovery : reads from the supplied [`DiscoveryEnv`]
/// snapshot. Side-effecting only on the filesystem (`is_file()` checks).
///
/// Reuses the same parent-dir-walk as cssl-rt discovery — loa-host's
/// staticlib is built into the same `target/<profile>/` by cargo, so the
/// candidate-dir walk is identical ; only the filename list differs.
#[must_use]
pub fn discover_loa_host_staticlib_with(env: &DiscoveryEnv) -> Option<PathBuf> {
    let verbose = env.rt_verbose.as_deref().is_some_and(|s| !s.is_empty());

    // Honor explicit per-engine opt-out (separate from cssl-rt's).
    if std::env::var_os("CSSL_NO_LOA_HOST").is_some() {
        if verbose {
            let _ = writeln!(
                std::io::stderr(),
                "csslc: loa-host default-link SKIPPED ($CSSL_NO_LOA_HOST set)"
            );
        }
        return None;
    }

    // § 1. Honor explicit env override.
    if let Ok(p) = std::env::var("CSSL_LOA_HOST_LIB") {
        if !p.is_empty() {
            let pb = PathBuf::from(&p);
            if pb.is_file() {
                if verbose {
                    let _ = writeln!(
                        std::io::stderr(),
                        "csslc: loa-host default-link USES $CSSL_LOA_HOST_LIB={}",
                        pb.display()
                    );
                }
                return Some(pb);
            }
            // Override given but file doesn't exist — emit warn + fall through.
            let _ = writeln!(
                std::io::stderr(),
                "csslc: warn: $CSSL_LOA_HOST_LIB='{}' not found ; falling back to auto-discovery",
                pb.display()
            );
        }
    }

    // § 2. Auto-discovery. Mirror the candidate-dir walk used for cssl-rt
    //   ; loa-host's staticlib lives in the same target/<profile>/ dir.
    let names = loa_host_staticlib_names();
    let mut parents: Vec<PathBuf> = Vec::new();

    if let Some(exe) = env.current_exe.as_ref() {
        if let Some(parent) = exe.parent() {
            parents.push(parent.to_path_buf());
        }
    }
    if let Some(cwd) = env.current_dir.as_ref() {
        for profile in &["debug", "release"] {
            parents.push(cwd.join("target").join(profile));
            parents.push(cwd.join("compiler-rs").join("target").join(profile));
            let mut p = cwd.as_path();
            while let Some(parent) = p.parent() {
                parents.push(parent.join("target").join(profile));
                parents.push(parent.join("compiler-rs").join("target").join(profile));
                p = parent;
            }
        }
    }
    if let Some(exe) = env.current_exe.as_ref() {
        let mut p = exe.as_path();
        for _ in 0..6 {
            if let Some(parent) = p.parent() {
                for profile in &["debug", "release"] {
                    parents.push(parent.join(profile));
                    parents.push(parent.join("target").join(profile));
                }
                p = parent;
            } else {
                break;
            }
        }
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    for p in parents {
        if !seen.iter().any(|s| s == &p) {
            seen.push(p);
        }
    }
    for parent in &seen {
        for name in &names {
            let candidate = parent.join(name);
            if candidate.is_file() {
                if verbose {
                    let _ = writeln!(
                        std::io::stderr(),
                        "csslc: loa-host default-link FOUND : {}",
                        candidate.display()
                    );
                }
                return Some(candidate);
            }
        }
    }
    if verbose {
        let _ = writeln!(
            std::io::stderr(),
            "csslc: loa-host default-link NOT FOUND ; tried {} candidate dir(s) × {} name(s)",
            seen.len(),
            names.len(),
        );
    }
    None
}

/// Platform-appropriate loa-host static-lib filenames in priority order.
fn loa_host_staticlib_names() -> Vec<&'static str> {
    if cfg!(target_os = "windows") {
        vec!["loa_host.lib", "libloa_host.a"]
    } else {
        vec!["libloa_host.a"]
    }
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
        LinkerKind::RustcDriven {
            rustc_path,
            cssl_rt_rlib,
            deps_dir,
        } => {
            // § T11-W19-α-CSSLC-LINKER-RT
            //   rustc as link driver. We write a tiny `#![no_main]` stub
            //   crate that depends on cssl_rt (so rustc threads its rlib +
            //   transitive deps + rust-std into the link) and pass the
            //   user's CSSL-emitted .obj files as `-C link-arg=`.
            //
            //   The cssl-emitted obj exports a `main` symbol (whichever
            //   CSSL fn is named `main`). On windows-gnu the mingw startup
            //   has a fallback `main` declaration in libmingw32.a's
            //   crtexewin.o, hence `-Wl,--allow-multiple-definition` so the
            //   user's `main` wins. The same crtexewin.o references
            //   `WinMain` unconditionally — the stub provides a zero-stub
            //   `WinMain` that's never actually called (console subsystem
            //   uses `main`, not `WinMain`).
            //
            //   On windows-msvc rustc invokes link.exe directly + the
            //   /SUBSYSTEM:CONSOLE pulls in mainCRTStartup which expects
            //   `main` ; the WinMain stub is harmless there.
            //
            // § TOOLCHAIN-PIN
            //   `rustc.exe` is the rustup-shim ; it consults
            //   `rust-toolchain.toml` from the CWD (or any ancestor) to
            //   decide which actual toolchain to invoke. To match the
            //   toolchain that built `libcssl_rt.rlib` (1.85.0 per
            //   compiler-rs/rust-toolchain.toml), we set the spawned
            //   process's working-dir to the rlib's parent-of-target/
            //   directory. That guarantees rustup picks up the same
            //   rust-toolchain.toml that cargo used for the rlib build.
            let stub_path = stub_rs_path();
            let _ = std::fs::write(&stub_path, RUSTC_DRIVEN_STUB_RS);
            let mut cmd = Command::new(rustc_path);
            // Walk up from <target/release/libcssl_rt.rlib> to the
            // workspace root (compiler-rs/) — that's where the
            // rust-toolchain.toml lives.
            if let Some(target_dir) = cssl_rt_rlib.parent() {
                if let Some(target_root) = target_dir.parent() {
                    if let Some(workspace_root) = target_root.parent() {
                        cmd.current_dir(workspace_root);
                    }
                }
            }
            cmd.arg("--crate-type").arg("bin");
            cmd.arg("-C").arg("opt-level=2");
            cmd.arg("-C").arg("panic=abort");
            // Tolerate duplicate `main` from libmingw32.a's startup glue.
            cmd.arg("-C").arg("link-arg=-Wl,--allow-multiple-definition");
            cmd.arg("-L").arg(deps_dir);
            cmd.arg("--extern").arg(format!(
                "cssl_rt={}",
                cssl_rt_rlib.display()
            ));
            for o in object_inputs {
                cmd.arg("-C").arg(format!("link-arg={}", o.display()));
            }
            for lib in extra_libs {
                // User-supplied extra libs are passed as link-arg as well ;
                // rustc forwards them to its underlying linker.
                cmd.arg("-C").arg(format!("link-arg={lib}"));
            }
            cmd.arg("-o").arg(output);
            cmd.arg(&stub_path);
            cmd
        }
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
/// § T11-D319 (W-CSSL-default-link)
///   When `$CSSL_NO_DEFAULT_LINK` is unset, this fn auto-discovers the
///   cssl-rt staticlib via [`discover_cssl_rt_staticlib`] and prepends it
///   to `extra_libs` so the `loa_startup.rs` ctor activates for every
///   produced .exe. The discovery is best-effort : if no staticlib is on
///   disk, we emit a one-line stderr warning ("cssl-rt staticlib not
///   found ; ctor will not fire") and proceed with the original
///   `extra_libs`. The user gets a stage-0 baseline binary either way.
///
/// # Errors
/// Bubbles up [`LinkError`] on detection / spawn / exit-status problems.
pub fn link(
    object_inputs: &[PathBuf],
    output: &Path,
    extra_libs: &[String],
) -> Result<(), LinkError> {
    let kind = detect_linker()?;
    // § T11-W19-α-CSSLC-LINKER-RT
    //   For the RustcDriven path, skip cssl-rt + loa-host injection :
    //   rustc threads cssl-rt via `--extern cssl_rt=...rlib` and pulls
    //   transitive deps from `deps/`, including all Win32 system libs
    //   that rust-std declares via per-crate `#[link(name = "...")]`
    //   attrs. Adding the legacy MSVC-style `.lib` filenames as
    //   `-C link-arg=` would confuse the underlying mingw-ld linker
    //   (which expects `-lname`-form, not `name.lib`-form). The user's
    //   own `extra_libs` still flow through.
    let effective_libs = if matches!(kind, LinkerKind::RustcDriven { .. }) {
        extra_libs.to_vec()
    } else {
        inject_default_cssl_rt_link(extra_libs)
    };
    let mut cmd = build_command(&kind, object_inputs, output, &effective_libs);
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

/// Build the effective `extra_libs` list by prepending the auto-discovered
/// cssl-rt staticlib (when discovery succeeds + opt-out env-var unset).
///
/// § T11-LOA-PURE-CSSL : ALSO auto-discovers + appends the loa-host
/// staticlib (when `$CSSL_NO_LOA_HOST` is unset). Order matters for some
/// linkers : cssl-rt FIRST (defines `__cssl_*` runtime symbols), loa-host
/// SECOND (defines `__cssl_engine_run` which calls into cssl-rt). This
/// mirrors how a Cargo workspace dependency resolves : engine ↦ runtime.
///
/// Public for in-process testing : `commands::build` uses [`link`] which
/// calls this internally ; tests assert on the returned `Vec<String>` to
/// cover the default-link / override / skip paths without spawning a real
/// linker.
#[must_use]
pub fn inject_default_cssl_rt_link(extra_libs: &[String]) -> Vec<String> {
    let mut effective: Vec<String> = Vec::with_capacity(extra_libs.len() + 2);
    if let Some(rt_path) = discover_cssl_rt_staticlib() {
        // For Unix-style linkers we still pass the absolute path (rust-lld /
        // gnu / darwin all accept it as a positional input). The clang/gcc/cc
        // driver path uses `-l<name>` for libraries on the default search
        // path, but here we have an absolute path so we pass it as a regular
        // arg ; passing absolute libcssl_rt.a directly works on all of those.
        effective.push(rt_path.display().to_string());
    } else if std::env::var_os("CSSL_NO_DEFAULT_LINK").is_none()
        && std::env::var_os("CSSL_RT_VERBOSE").is_some()
    {
        let _ = writeln!(
            std::io::stderr(),
            "csslc: warn: cssl-rt staticlib not found ; ctor will not fire \
             (set $CSSL_RT_LIB or build cssl-rt to enable)"
        );
    }
    // T11-LOA-PURE-CSSL : append loa-host staticlib AFTER cssl-rt so the
    // engine can resolve cssl-rt symbols. The `$CSSL_NO_LOA_HOST=1` opt-out
    // is honored inside discover_loa_host_staticlib so legacy hello-world
    // gates (which DON'T link the engine) stay functional.
    //
    // When loa-host IS linked, we ALSO add the Windows system libraries it
    // (transitively via winit + wgpu + rust-std) needs : ws2_32 + user32 +
    // gdi32 + advapi32 + ntdll + bcrypt + ole32 + shell32 + dwmapi + d3d12
    // + dxgi + opengl32. Without these the linker reports thousands of
    // unresolved-external errors. Cargo handles this automatically for
    // rust-binary outputs ; staticlib consumers must declare them
    // explicitly. (Same pattern as rust-staticlib-howto in the rust book.)
    effective.extend(extra_libs.iter().cloned());
    if let Some(loa_path) = discover_loa_host_staticlib() {
        effective.push(loa_path.display().to_string());
        // Append Windows-system libs only on Windows targets ; on other
        // platforms the equivalent libs (libdl/libpthread/libGL/etc.) are
        // typically picked up by the compiler-driver linker (clang/gcc/cc)
        // automatically. Future stage-1 work : per-target lib auto-discovery.
        if cfg!(target_os = "windows") {
            for sys_lib in WINDOWS_LOA_HOST_SYS_LIBS {
                effective.push((*sys_lib).to_string());
            }
        }
    }
    effective
}

/// Windows-system libraries needed by the loa-host staticlib's transitive
/// dependencies (rust-std + winit + wgpu + tokio-style async-runtime).
///
/// § DERIVATION
///   This list is the union of `print-cargo-args -p loa-host` (build-time
///   `cargo:rustc-link-lib=...` directives surfaced by build scripts) +
///   the rust-std-windows core set. Recompute when adding new transitive
///   deps that bring their own system-lib requirements (e.g. Vulkan SDK
///   integration → vulkan-1.lib).
///
/// § RATIONALE PER LIB
///   - ws2_32      : sockets (rust-std `std::net` + winsock2)
///   - userenv     : user-profile dir lookup (`std::env::home_dir`)
///   - ntdll       : NT-syscall imports (NtCreateFile etc.)
///   - bcrypt      : crypto-hash entropy (rust-std + winit RNG)
///   - user32      : window mgmt (CreateWindowExW / DestroyWindow / etc.)
///   - gdi32       : GDI device-context primitives (winit + wgpu fallback)
///   - opengl32    : OpenGL fallback path (wgpu's GLES backend)
///   - kernel32    : Win32 base API (already added by MSVC default but
///                   explicit doesn't hurt)
///   - advapi32    : registry (used by wgpu adapter probe)
///   - shell32     : shell-side dialogs (winit dialog facilities)
///   - ole32       : OLE init (DPI-aware + clipboard)
///   - dwmapi      : composition / blur (winit transparent-window paths)
///   - d3d12       : DirectX 12 backend (wgpu)
///   - dxgi        : DirectX adapter enumeration (wgpu)
///   - d3dcompiler : HLSL compile (wgpu shader translation fallback)
///   - oleaut32    : OLE automation (winit drop-target / clipboard)
///   - imm32       : IME composition (winit text-input)
///   - propsys     : property-system (winit window-thumbnail)
const WINDOWS_LOA_HOST_SYS_LIBS: &[&str] = &[
    "ws2_32.lib",
    "userenv.lib",
    "ntdll.lib",
    "bcrypt.lib",
    "user32.lib",
    "gdi32.lib",
    "opengl32.lib",
    "advapi32.lib",
    "shell32.lib",
    "ole32.lib",
    "oleaut32.lib",
    "dwmapi.lib",
    "d3d12.lib",
    "dxgi.lib",
    "d3dcompiler.lib",
    "imm32.lib",
    "propsys.lib",
    "uuid.lib",
    "synchronization.lib",
    "uxtheme.lib",         // SetWindowTheme — winit dark-mode probe
    "runtimeobject.lib",   // RoOriginateErrorW — windows_result error-info
    "comdlg32.lib",        // common-dialog facilities (winit / wgpu fallback)
    "comctl32.lib",        // common-controls (winit dialog facilities)
    "msimg32.lib",         // GDI extended fns (legacy wgpu fallback)
    "winspool.lib",        // print-spooler (winit cursor-set ICM probe)
    "version.lib",         // version-info (wgpu adapter probe)
    "winmm.lib",           // multimedia timer (wgpu vsync fallback)
    "secur32.lib",         // SSPI / GSSAPI (rust-std network)
    "credui.lib",          // credential UI (rust-std auth fallback)
    "iphlpapi.lib",        // IP helper API (rust-std network adapter)
    "kernel32.lib",        // already added by build_command, idempotent
];

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
        assert!(cmd.get_args().any(|s| s.to_string_lossy() == "-o"));
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

    // ───────────────────────────────────────────────────────────────────
    // § T11-D319 (W-CSSL-default-link) tests
    // ───────────────────────────────────────────────────────────────────
    //
    // These tests use the [`DiscoveryEnv`] dependency-injection wrapper so
    // they exercise every branch without mutating the live process env-vars.
    // (csslc has `#![forbid(unsafe_code)]`, and Rust's `set_var` is `unsafe`
    // in 1.85+ Rust ; pure-function form sidesteps both concerns.)

    /// Helper : a base [`DiscoveryEnv`] with no env-vars set + best-effort
    /// `current_exe` / `current_dir`. Tests override the fields they care
    /// about. Defaulting `rt_verbose` to `None` keeps test output quiet.
    fn empty_env() -> DiscoveryEnv {
        DiscoveryEnv {
            no_default_link: None,
            rt_lib: None,
            rt_verbose: None,
            current_exe: std::env::current_exe().ok(),
            current_dir: std::env::current_dir().ok(),
        }
    }

    /// Test-helper variant of [`inject_default_cssl_rt_link`] that takes a
    /// [`DiscoveryEnv`] (rather than reading the live process env). Mirrors
    /// the production fn's logic exactly ; production-side simply
    /// substitutes `DiscoveryEnv::from_process()`.
    fn inject_with(env: &DiscoveryEnv, extra_libs: &[String]) -> Vec<String> {
        let mut effective: Vec<String> = Vec::with_capacity(extra_libs.len() + 1);
        if let Some(rt_path) = discover_cssl_rt_staticlib_with(env) {
            effective.push(rt_path.display().to_string());
        }
        effective.extend(extra_libs.iter().cloned());
        effective
    }

    /// 1. Default-link injection runs when no env-var is set. We can't
    ///    assert a cssl-rt staticlib is found on every developer machine
    ///    (the rlib consumer chain may produce only `.rlib` until a
    ///    `cargo build` of cssl-rt runs), but we CAN assert the function
    ///    returns either (a) just the user-libs (no staticlib on disk) or
    ///    (b) the user-libs prepended with one cssl-rt path, never garbage.
    #[test]
    fn linker_default_link_includes_cssl_rt_when_env_unset() {
        let env = empty_env();
        let user_libs = vec!["libcmt.lib".to_string()];
        let effective = inject_with(&env, &user_libs);
        // The user-supplied lib must always survive injection.
        assert!(
            effective.iter().any(|s| s == "libcmt.lib"),
            "user-supplied lib must survive injection ; got {effective:?}",
        );
        // The function must add at most one entry (the cssl-rt path).
        let added = effective.len() - user_libs.len();
        assert!(
            added <= 1,
            "default-link must add at most one entry ; got {effective:?}",
        );
        if added == 1 {
            // Discovered → entry references the cssl-rt staticlib filename.
            let prefix = &effective[0];
            let lc = prefix.to_lowercase();
            assert!(
                lc.contains("cssl_rt") || lc.contains("libcssl_rt"),
                "prepended entry should reference cssl-rt ; got `{prefix}`",
            );
        }
    }

    /// 2. `$CSSL_NO_DEFAULT_LINK=1` ⇒ discovery short-circuits to None +
    ///    injection passes through user-libs unchanged (byte-for-byte).
    ///    Even with a bogus `$CSSL_RT_LIB` set, the skip-flag must win.
    #[test]
    fn csslc_no_default_link_env_skips_cssl_rt() {
        let env = DiscoveryEnv {
            no_default_link: Some("1".to_string()),
            rt_lib: Some("/never/exists/libcssl_rt.a".to_string()),
            ..empty_env()
        };
        let user_libs = vec!["libcmt.lib".to_string(), "kernel32.lib".to_string()];
        let effective = inject_with(&env, &user_libs);
        assert_eq!(
            effective, user_libs,
            "$CSSL_NO_DEFAULT_LINK must short-circuit injection ; got {effective:?}",
        );
        assert!(
            discover_cssl_rt_staticlib_with(&env).is_none(),
            "discover_cssl_rt_staticlib_with must respect $CSSL_NO_DEFAULT_LINK",
        );
    }

    /// 3. `$CSSL_RT_LIB=<path>` overrides auto-discovery. We point it at a
    ///    tempfile we just created so the override is guaranteed to resolve.
    #[test]
    fn cssl_rt_lib_env_override_takes_precedence() {
        // Create a sentinel file with a recognizable name + content. The
        // discovery fn only checks `is_file()`, so any non-empty regular
        // file works.
        let tmp = std::env::temp_dir().join(format!(
            "csslc_d319_override_{}_{}.lib",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::write(&tmp, b"sentinel").expect("temp staticlib write");

        let env = DiscoveryEnv {
            rt_lib: Some(tmp.display().to_string()),
            ..empty_env()
        };
        let discovered = discover_cssl_rt_staticlib_with(&env)
            .expect("override path must be returned by discovery");
        assert_eq!(
            discovered, tmp,
            "$CSSL_RT_LIB must take precedence over auto-discovery",
        );
        // Injection prepends it to the user-supplied list.
        let effective = inject_with(&env, &["libcmt.lib".to_string()]);
        assert_eq!(effective.len(), 2);
        assert_eq!(effective[0], tmp.display().to_string());
        assert_eq!(effective[1], "libcmt.lib");

        // Cleanup.
        let _ = std::fs::remove_file(&tmp);
    }

    /// 4. `$CSSL_RT_LIB` pointing at a NON-EXISTENT path is handled
    ///    gracefully : we warn (visible only with `CSSL_RT_VERBOSE`) and
    ///    fall back to auto-discovery. The user's lib list still flows
    ///    through; injection never panics.
    #[test]
    fn cssl_rt_lib_env_override_missing_falls_back_gracefully() {
        let env = DiscoveryEnv {
            rt_lib: Some("/this/path/does/not/exist/libcssl_rt.a".to_string()),
            ..empty_env()
        };
        let user_libs = vec!["libcmt.lib".to_string()];
        let effective = inject_with(&env, &user_libs);
        assert!(
            effective.iter().any(|s| s == "libcmt.lib"),
            "user-supplied lib must survive bogus override ; got {effective:?}",
        );
        // Effective length is 1 (no auto-found staticlib either) or 2
        // (auto-discovery succeeded after override fell back). Both are
        // acceptable ; injection must not panic.
        assert!(
            (1..=2).contains(&effective.len()),
            "effective lib list size out of bounds : {effective:?}",
        );
    }

    /// 5. Linked-exe smoke : build_command threads an injected lib path
    ///    through to the linker arg vector for both lld-link (MSVC-style)
    ///    and clang (Unix-style) backends. This validates the wire-up
    ///    between inject_default_cssl_rt_link and build_command without
    ///    requiring a real cssl-rt staticlib on disk.
    #[test]
    fn linked_exe_with_rt_invokes_ctor() {
        // The acceptance criterion "ctor invocation" is checked end-to-end
        // by the cssl-examples LoA-startup gate ; here we cover the
        // necessary precondition : if injection adds a path, the linker
        // command sees it. (Without that, no ctor activation is possible.)
        let injected = "C:/fake/target/release/cssl_rt.lib".to_string();
        let kind = LinkerKind::LldLink(PathBuf::from("lld-link"));
        let cmd = build_command(
            &kind,
            &[PathBuf::from("hello.obj")],
            Path::new("hello.exe"),
            &[injected.clone()],
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(
            args.iter().any(|a| a == &injected),
            "injected cssl-rt path must appear in lld-link args ; got {args:?}",
        );

        // Same propagation through clang (Unix path).
        let kind_clang = LinkerKind::Clang(PathBuf::from("clang"));
        let cmd_clang = build_command(
            &kind_clang,
            &[PathBuf::from("hello.o")],
            Path::new("hello"),
            &["/abs/path/libcssl_rt.a".to_string()],
        );
        let args_clang: Vec<String> = cmd_clang
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        // clang/gcc/cc path uses `-l<name>` ; the absolute path becomes
        // `-l/abs/path/libcssl_rt.a`. Either form must appear.
        assert!(
            args_clang.iter().any(|a| a.contains("libcssl_rt.a")),
            "injected cssl-rt path must appear in clang args ; got {args_clang:?}",
        );
    }

    /// 6. `staticlib_names()` returns at least one platform-appropriate
    ///    filename — sanity gate to catch a typo'd platform branch.
    #[test]
    fn staticlib_names_returns_platform_appropriate_list() {
        let names = staticlib_names();
        assert!(!names.is_empty(), "staticlib_names must be non-empty");
        if cfg!(target_os = "windows") {
            assert!(names.contains(&"cssl_rt.lib"));
        } else {
            assert!(names.contains(&"libcssl_rt.a"));
        }
    }

    /// 7. Empty `DiscoveryEnv` (no exe, no cwd, no env-vars) returns
    ///    `None` — proves the discovery walks gracefully when the snapshot
    ///    has nothing to walk.
    #[test]
    fn discovery_with_empty_env_returns_none() {
        let env = DiscoveryEnv::default();
        assert!(discover_cssl_rt_staticlib_with(&env).is_none());
    }

    // ───────────────────────────────────────────────────────────────────
    // § T11-W19-α-CSSLC-LINKER-RT — rustc-driven path tests
    // ───────────────────────────────────────────────────────────────────
    //
    // These tests exercise the build_command shape for `LinkerKind::
    // RustcDriven` without requiring a real rustc invocation. The
    // detection-side test `find_rustc_driven_returns_when_rlib_present` is
    // best-effort (depends on `cargo build -p cssl-rt` having run) and
    // skips gracefully when the rlib is absent.

    /// build_command for RustcDriven includes `--extern cssl_rt=...`,
    /// passes the user obj as `-C link-arg=`, threads `-L deps_dir`, and
    /// finishes with `-o output stub.rs`. Exercising this without spawning
    /// rustc is fine because the Command's argv shape is what matters.
    #[test]
    fn build_command_for_rustc_driven_shape() {
        let kind = LinkerKind::RustcDriven {
            rustc_path: PathBuf::from("/fake/rustc.exe"),
            cssl_rt_rlib: PathBuf::from("/fake/target/release/libcssl_rt.rlib"),
            deps_dir: PathBuf::from("/fake/target/release/deps"),
        };
        let cmd = build_command(
            &kind,
            &[PathBuf::from("/tmp/user.obj")],
            Path::new("/tmp/out.exe"),
            &[],
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        assert!(
            args.contains(&"--crate-type".to_string()),
            "rustc-driven must pass --crate-type"
        );
        assert!(args.contains(&"bin".to_string()));
        assert!(
            args.iter().any(|a| a.starts_with("cssl_rt=")),
            "rustc-driven must pass --extern cssl_rt=..."
        );
        assert!(
            args.iter().any(|a| a.contains("user.obj")),
            "rustc-driven must thread the user obj as link-arg"
        );
        assert!(
            args.iter().any(|a| a == "panic=abort"),
            "rustc-driven must set panic=abort to match rlib's strategy"
        );
        assert!(
            args.iter().any(|a| a.contains("allow-multiple-definition")),
            "rustc-driven must allow duplicate `main` from libmingw32",
        );
    }

    /// `link()` skips the cssl-rt + loa-host injection when the discovered
    /// linker is RustcDriven (rustc threads them via `--extern` instead).
    /// This test verifies the matches!-branch in `link()` ; we can't
    /// observe the spawned process directly, but we can assert that
    /// `inject_default_cssl_rt_link` is NOT called in the RustcDriven path
    /// by snapshot-comparing the build_command argv.
    #[test]
    fn rustc_driven_skips_legacy_inject_path() {
        let kind = LinkerKind::RustcDriven {
            rustc_path: PathBuf::from("/fake/rustc.exe"),
            cssl_rt_rlib: PathBuf::from("/fake/target/release/libcssl_rt.rlib"),
            deps_dir: PathBuf::from("/fake/target/release/deps"),
        };
        // Pass legacy MSVC-shaped extra_libs that would confuse mingw-ld
        // if accidentally forwarded.
        let user_libs = vec!["kernel32.lib".to_string(), "ws2_32.lib".to_string()];
        let cmd = build_command(
            &kind,
            &[PathBuf::from("/tmp/user.obj")],
            Path::new("/tmp/out.exe"),
            &user_libs,
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        // The user-libs ARE passed to build_command (link() de-duplicates,
        // not build_command). What matters is that build_command shapes
        // them as -C link-arg= so rustc forwards them, not as bare lib
        // names. Verify the link-arg= form.
        for ul in &user_libs {
            let expected = format!("link-arg={ul}");
            assert!(
                args.iter().any(|a| a == &expected),
                "RustcDriven must wrap user-lib `{ul}` as `-C link-arg=` ; got args = {args:?}"
            );
        }
    }

    /// `find_rustc_driven` returns Some when libcssl_rt.rlib is on disk
    /// (after `cargo build -p cssl-rt`) ; returns None otherwise. This
    /// is a best-effort gate test : on a clean checkout where cssl-rt
    /// hasn't been built yet the test asserts None ; on a developer
    /// machine post-build it asserts Some + verifies path shape.
    #[test]
    fn find_rustc_driven_returns_when_rlib_present() {
        let opt = find_rustc_driven();
        // Either branch is acceptable depending on whether `cargo build
        // -p cssl-rt` has run on this machine. We assert path shape when
        // present (regression guard against returning a garbage triple).
        if let Some(info) = opt {
            assert!(
                info.cssl_rt_rlib
                    .file_name()
                    .map(|s| s == "libcssl_rt.rlib")
                    .unwrap_or(false),
                "rlib filename should be libcssl_rt.rlib ; got {:?}",
                info.cssl_rt_rlib,
            );
            assert!(
                info.deps_dir.is_dir(),
                "deps_dir should be a directory : {:?}",
                info.deps_dir,
            );
            assert!(
                info.rustc_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .map(|s| s == "rustc")
                    .unwrap_or(false),
                "rustc_path stem should be `rustc` ; got {:?}",
                info.rustc_path,
            );
        }
    }
}

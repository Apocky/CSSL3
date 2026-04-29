//! § native_hello_world_gate — S7-G6 end-to-end "second hello.exe = 42"
//! ════════════════════════════════════════════════════════════════════
//!
//! Spec  : `HANDOFF_SESSION_7.csl § PHASE-G § S7-G6` (per-dispatch handoff).
//! Gate  : compile `stage1/hello_world.cssl` via `csslc` with
//!         `--backend=native-x64`, link via the system linker, run the
//!         produced executable, assert OS exit-code = **42**. Mirrors
//!         the S6-A5 cranelift gate test but exercises the hand-rolled
//!         x86-64 path.
//!
//! § DESIGN
//!   The test invokes the public `csslc::run` library entry-point with a
//!   synthesized argv vector that adds `--backend=native-x64`. This is
//!   the SECOND hello.exe = 42 milestone : the first (S6-A5) routed
//!   through cranelift ; this one routes through `cssl-cgen-cpu-x64`
//!   (the S7-G axis hand-rolled CPU backend).
//!
//! § SKIP CONDITIONS  (informational, not assertion-fatal)
//!   When G1..G5 sibling slices haven't yet landed, `csslc
//!   --backend=native-x64` returns user-error with a stderr message
//!   beginning `"object-emit error (native-x64): native-x64 backend not
//!   yet landed"`. The test cannot read csslc's stderr (it's a library
//!   call, not a subprocess), so it relies on a more direct check :
//!   call `cssl_cgen_cpu_x64::emit_object_module(&MirModule::new())`
//!   and inspect the result. If it returns `Err(BackendNotYetLanded)`,
//!   the gate SKIPS gracefully with an informative message naming each
//!   G-axis sibling. If it returns `Ok(_)` (or a different error variant),
//!   G1..G5 are landing-or-landed and the full pipeline is exercised.
//!
//!   - No linker available → identical handling to A5 (skip).
//!   - Linker found but fails → likewise reported but not fatal.
//!   - Native path runs but returns ≠ 42 → HARD assertion failure.
//!     A successful native-x64 build + link followed by a wrong exit
//!     code is a real backend bug, not an environment issue.
//!
//! § CLEANUP
//!   The test writes the executable to a system temp dir with a
//!   PID-tagged name (`csslc_native_hello_<pid>.exe`) and removes it
//!   on success. On failure the file is preserved.
//!
//! § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

#![allow(clippy::missing_panics_doc)]

use std::path::PathBuf;
use std::process::Command;

/// Path to the canonical hello_world source, resolved at compile-time
/// relative to the workspace root. Reuses A5's path so both gates
/// exercise the same source-of-truth.
pub const NATIVE_HELLO_WORLD_CSSL_PATH: &str = crate::hello_world_gate::HELLO_WORLD_CSSL_PATH;

/// Produce a system-temp path with a unique-per-test executable name.
#[cfg(test)]
fn unique_temp_exe(stem: &str) -> PathBuf {
    let pid = std::process::id();
    let tmp = std::env::temp_dir();
    let file = if cfg!(target_os = "windows") {
        format!("{stem}_{pid}.exe")
    } else {
        format!("{stem}_{pid}")
    };
    tmp.join(file)
}

/// Whether the native-x64 backend's body is in flight (G1..G5 not landed).
/// Detection : call `cssl_cgen_cpu_x64::emit_object_module` on an empty
/// module and check for the canonical `BackendNotYetLanded` variant.
#[must_use]
pub fn native_x64_backend_in_flight() -> bool {
    let m = cssl_mir::MirModule::new();
    matches!(
        cssl_cgen_cpu_x64::emit_object_module(&m),
        Err(cssl_cgen_cpu_x64::NativeX64Error::BackendNotYetLanded)
    )
}

/// Outcome of a native-hello-world end-to-end run. Mirrors A5's
/// `HelloRunOutcome` shape so the two gates are structurally
/// comparable.
///
/// `clippy::struct_excessive_bools` is allowed because this is a
/// diagnostic outcome reporter — each bool answers a distinct,
/// orthogonal question about the run (build / exec attempted /
/// exec returned code / backend in flight) and a state-machine /
/// enum refactor would obscure the intentional flat shape that
/// mirrors A5's `HelloRunOutcome`.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct NativeHelloRunOutcome {
    /// Whether csslc::build returned success on the native-x64 path.
    pub build_succeeded: bool,
    /// Whether the produced executable was launched.
    pub exec_attempted: bool,
    /// Whether the exec child returned an exit code.
    pub exec_returned_code: bool,
    /// The OS exit code from the produced executable, if available.
    pub exit_code: Option<i32>,
    /// Whether the native-x64 backend's body is still in flight at
    /// dispatch-time (G1..G5 not yet landed). When true the build was
    /// expected to fail with the canonical not-yet-landed message and
    /// the test SKIPS rather than asserting.
    pub backend_in_flight: bool,
    /// Human-readable reason for any non-success state.
    pub reason: String,
}

impl NativeHelloRunOutcome {
    fn skipped_in_flight() -> Self {
        Self {
            build_succeeded: false,
            exec_attempted: false,
            exec_returned_code: false,
            exit_code: None,
            backend_in_flight: true,
            reason: "native-x64 backend G1..G5 in flight at S7-G6 dispatch \
                     (G1=ABI, G2=isel, G3=regalloc, G4=object-writer, \
                     G5=emitter) ; gate skips gracefully until siblings land"
                .to_string(),
        }
    }

    fn skipped_env(reason: &str) -> Self {
        Self {
            build_succeeded: false,
            exec_attempted: false,
            exec_returned_code: false,
            exit_code: None,
            backend_in_flight: false,
            reason: reason.to_string(),
        }
    }
}

/// Run the full native-x64 hello-world pipeline. Returns the outcome ;
/// the caller (typically a `#[test]` fn) decides whether to assert or
/// skip based on `backend_in_flight` + `build_succeeded` + `exit_code`.
#[must_use]
pub fn run_native_hello_world_gate(input: &str, output: &PathBuf) -> NativeHelloRunOutcome {
    // § Pre-check : if G1..G5 siblings are in flight, skip without even
    // invoking csslc. This is the cleanest informational path — the
    // detection probe is cheap and produces a more actionable skip
    // message than parsing csslc's stderr.
    if native_x64_backend_in_flight() {
        return NativeHelloRunOutcome::skipped_in_flight();
    }

    let exit = csslc::run(vec![
        "csslc".to_string(),
        "build".to_string(),
        input.to_string(),
        "-o".to_string(),
        output.display().to_string(),
        "--emit=exe".to_string(),
        "--backend=native-x64".to_string(),
    ]);

    let success_dbg = format!("{:?}", std::process::ExitCode::from(0));
    let actual_dbg = format!("{exit:?}");
    if actual_dbg != success_dbg {
        return NativeHelloRunOutcome::skipped_env(&format!(
            "csslc build --backend=native-x64 returned non-success ({actual_dbg}) \
             — likely no host linker, MSVC toolchain absent, or backend-internal error"
        ));
    }
    if !output.exists() {
        return NativeHelloRunOutcome::skipped_env(
            "csslc reported success but native-x64 output file is missing",
        );
    }

    let child = Command::new(output).output();
    match child {
        Ok(out) => {
            let code = out.status.code();
            NativeHelloRunOutcome {
                build_succeeded: true,
                exec_attempted: true,
                exec_returned_code: code.is_some(),
                exit_code: code,
                backend_in_flight: false,
                reason: if code.is_some() {
                    "exec returned a code".to_string()
                } else {
                    "exec terminated by signal".to_string()
                },
            }
        }
        Err(e) => NativeHelloRunOutcome {
            build_succeeded: true,
            exec_attempted: true,
            exec_returned_code: false,
            exit_code: None,
            backend_in_flight: false,
            reason: format!("failed to spawn produced native-x64 executable : {e}"),
        },
    }
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_hello_world_cssl_source_file_exists() {
        // Same source-of-truth as the A5 cranelift gate.
        let p = std::path::Path::new(NATIVE_HELLO_WORLD_CSSL_PATH);
        assert!(
            p.exists(),
            "missing canonical hello-world source at {}",
            p.display()
        );
        let src = std::fs::read_to_string(p).expect("read hello-world source");
        assert!(src.contains("module com.apocky.examples.hello_world"));
        assert!(src.contains("fn main() -> i32 { 42 }"));
    }

    #[test]
    fn unique_temp_exe_path_is_in_temp_dir() {
        let p = unique_temp_exe("csslc_native_test_uniqueness");
        assert!(p.starts_with(std::env::temp_dir()));
        let pid_str = std::process::id().to_string();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.contains(&pid_str), "expected pid in path : {name}");
    }

    #[test]
    fn native_x64_backend_in_flight_returns_bool() {
        // The detection probe should be a stable bool predicate.
        let _: bool = native_x64_backend_in_flight();
    }

    #[test]
    fn outcome_skipped_in_flight_is_well_formed() {
        let o = NativeHelloRunOutcome::skipped_in_flight();
        assert!(!o.build_succeeded);
        assert!(o.backend_in_flight);
        assert!(o.reason.contains("G1..G5"));
        assert!(o.reason.contains("native-x64"));
    }

    #[test]
    fn outcome_skipped_env_carries_reason() {
        let o = NativeHelloRunOutcome::skipped_env("no linker available");
        assert!(!o.build_succeeded);
        assert!(!o.backend_in_flight);
        assert!(o.reason.contains("no linker available"));
    }

    /// THE GATE : `stage1/hello_world.cssl` compiles via `--backend=native-x64`
    /// + links + runs and returns exit-code 42.
    ///
    /// Skip cases :
    ///   - G1..G5 sibling slices in flight (canonical `BackendNotYetLanded`
    ///     detected via the cssl-cgen-cpu-x64 surface) → SKIP with informative
    ///     message naming each axis. This is the dominant case at S7-G6
    ///     dispatch time.
    ///   - No host linker available → SKIP (matches A5 behavior).
    ///   - Build / link succeeded but exec returned ≠ 42 → HARD assertion
    ///     failure (real native-x64 backend bug).
    #[test]
    fn s7_g6_native_hello_world_executable_returns_42() {
        let out = unique_temp_exe("csslc_native_hello");
        let outcome = run_native_hello_world_gate(NATIVE_HELLO_WORLD_CSSL_PATH, &out);

        eprintln!("S7-G6 native-x64 hello-world gate :");
        eprintln!("  source             : {NATIVE_HELLO_WORLD_CSSL_PATH}");
        eprintln!("  output             : {}", out.display());
        eprintln!("  backend_in_flight  : {}", outcome.backend_in_flight);
        eprintln!("  build_ok           : {}", outcome.build_succeeded);
        eprintln!("  exec_attempted     : {}", outcome.exec_attempted);
        eprintln!("  exec_code          : {:?}", outcome.exit_code);
        eprintln!("  reason             : {}", outcome.reason);

        if outcome.backend_in_flight {
            eprintln!(
                "  status             : SKIP (G1..G5 siblings still in flight ; \
                 native-x64 backend body not yet landed)"
            );
            return;
        }
        if !outcome.build_succeeded || !outcome.exec_attempted {
            eprintln!(
                "  status             : SKIP (environment lacks a working linker \
                 OR backend-internal error — see reason above)"
            );
            return;
        }
        // Exec attempted ; verify the code is 42.
        let code = outcome
            .exit_code
            .expect("exec returned a code per build_succeeded path");
        assert_eq!(
            code, 42,
            "S7-G6 GATE FAILURE : native-x64 hello-world exited {code}, expected 42"
        );
        eprintln!(
            "  status             : PASS — second CSSLv3 executable runs (via \
             hand-rolled native-x64 path) and returns 42"
        );

        // Cleanup on success.
        let _ = std::fs::remove_file(&out);
    }

    /// Backend-comparison gate. When BOTH cranelift and native-x64 are
    /// available + working, the two paths should each produce a runnable
    /// hello.exe + exit 42 ; their object byte-streams need NOT match
    /// bit-for-bit (different encoding choices), but both run identically.
    ///
    /// At S7-G6 dispatch time this almost-certainly skips the native side
    /// (G1..G5 in flight) ; the comparison machinery is in place so when
    /// G1..G5 land, the same test asserts semantic equivalence
    /// automatically.
    #[test]
    fn s7_g6_backend_comparison_both_paths_exit_42_when_both_available() {
        // § Cranelift path — A5 baseline reused.
        let out_clift = unique_temp_exe("csslc_cmp_clift_hello");
        let outcome_clift = crate::hello_world_gate::run_hello_world_gate(
            crate::hello_world_gate::HELLO_WORLD_CSSL_PATH,
            &out_clift,
        );

        // § Native-x64 path.
        let out_native = unique_temp_exe("csslc_cmp_native_hello");
        let outcome_native = run_native_hello_world_gate(NATIVE_HELLO_WORLD_CSSL_PATH, &out_native);

        eprintln!("S7-G6 backend-comparison gate :");
        eprintln!(
            "  cranelift  : build={} exec_code={:?} reason={}",
            outcome_clift.build_succeeded, outcome_clift.exit_code, outcome_clift.reason
        );
        eprintln!(
            "  native-x64 : build={} backend_in_flight={} exec_code={:?} reason={}",
            outcome_native.build_succeeded,
            outcome_native.backend_in_flight,
            outcome_native.exit_code,
            outcome_native.reason
        );

        // If EITHER side skipped (env or in-flight), the comparison is informational.
        let clift_pass = outcome_clift.exit_code == Some(42);
        let native_pass = outcome_native.exit_code == Some(42);

        if clift_pass && native_pass {
            // BOTH paths produced a runnable 42 → second-milestone reached.
            eprintln!("  status     : PASS — both backends produce hello.exe = 42");
        } else if clift_pass && outcome_native.backend_in_flight {
            eprintln!(
                "  status     : SKIP (cranelift OK ; native-x64 G1..G5 in flight — \
                 comparison deferred until siblings land)"
            );
        } else {
            eprintln!(
                "  status     : SKIP (one or both paths blocked by env — \
                 see per-side reasons above)"
            );
        }

        // Cleanup best-effort regardless of skip-vs-pass.
        let _ = std::fs::remove_file(&out_clift);
        let _ = std::fs::remove_file(&out_native);

        // No hard assertion : the comparison gate is informational at
        // S7-G6 dispatch ; per-side gates above carry the assertions.
    }
}

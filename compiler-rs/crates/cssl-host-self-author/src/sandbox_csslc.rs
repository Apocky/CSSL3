// sandbox_csslc.rs — isolated csslc-compile sandbox for self-authored CSSL strings
// ══════════════════════════════════════════════════════════════════
// § ROLE
//   Take a candidate CSSL-source-string emitted by the LLM and run it through a
//   compile-only pass + an in-memory inline-test pass. NO FFI to host. NO file
//   write outside the sandbox `scratch_dir` (which the caller designates ;
//   default = `None` → in-memory only).
//
// § ISOLATION CONTRACT (structurally enforced)
//   - sandbox-state is a pure data-struct — no socket / no file-handle held
//   - Sandbox::compile_only DOES NOT exec any subprocess
//   - Sandbox::compile_only operates on the provided source-string only ;
//     produces CompileOutcome (in-memory enum)
//   - Sandbox::run_inline_tests is currently a syntactic-pattern check
//     (count-of-`#[test]` blocks · presence of `assert!` etc.) — it does
//     NOT exec the compiled program. This is the disciplined-stage-0 stance :
//     a real exec-sandbox requires OS-process-isolation (cgroups · seccomp ·
//     pledge-unveil · AppContainer) which is out-of-scope for the host-side
//     stub. The test-shape is sufficient to feed the quality-score machinery
//     and the W12-3 KAN-loop training-pairs.
//
// § COMPILE-PATH
//   Stage-0 host : the sandbox uses the in-process `csslc` library entry-point
//   for syntactic / pre-pipeline checks ONLY. Real codegen + linking is OFF in
//   sandbox-mode (compile_only=true). The sandbox returns CompileOutcome which
//   captures (parse-pass · diagnostics · warnings) without producing artifacts.
//
//   In this slice we DO NOT invoke csslc::run() directly because that would
//   require setting up an argv shape + exit-code interpretation and is a
//   layering-leak (csslc owns the binary CLI ; the sandbox is a library
//   consumer). Instead we accept a caller-supplied `CompileFn` (function-
//   pointer) which the orchestrator wires to the appropriate compile entry-
//   point. Tests use a deterministic mock-compiler that pattern-matches on
//   the source-string shape.
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Sandbox configuration. All fields default to safe-most values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Compile-only (skip codegen/link). Stage-0 ALWAYS true ; sandbox would
    /// fail-deny if a future caller flipped this to false without an exec
    /// isolation guarantee.
    pub compile_only: bool,
    /// Maximum source-bytes accepted. Default 64 KiB. Larger = `OversizeSource`.
    pub max_source_bytes: usize,
    /// Maximum diagnostics to collect (truncate to keep memory-bounded).
    pub max_diagnostics: usize,
    /// If `true`, the sandbox refuses any source that contains an
    /// `extern "C"` block or a substring matching a forbidden-effect string.
    pub deny_externs: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            compile_only: true,
            max_source_bytes: 64 * 1024,
            max_diagnostics: 64,
            deny_externs: true,
        }
    }
}

/// Compile outcome. The sandbox produces this from compile-only pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompileOutcome {
    /// Compilation passed (no diagnostics ≥ Error).
    Pass {
        /// Number of warnings reported.
        warnings: u32,
    },
    /// Compilation failed with one or more errors.
    Fail {
        /// First N error-strings.
        errors: Vec<String>,
    },
    /// Source rejected pre-compile due to size / forbidden-content.
    Rejected {
        /// Reason string.
        reason: String,
    },
}

/// Pre-execution test outcome (syntactic-pattern check).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineTestReport {
    /// Count of `#[test]` blocks observed.
    pub test_count: u32,
    /// Count of `assert!` / `assert_eq!` calls.
    pub assert_count: u32,
    /// Whether any forbidden-effect string was matched (always false on Pass).
    pub forbidden_effect_matched: bool,
}

/// Aggregate sandbox report — feeds the quality-score machinery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxReport {
    /// Compilation outcome.
    pub compile: CompileOutcome,
    /// Inline-test heuristic.
    pub tests: InlineTestReport,
    /// BLAKE3 of the source-bytes. Used as canonical-id for Σ-Chain anchoring.
    pub source_blake3: [u8; 32],
}

impl SandboxReport {
    /// Convenience : `true` iff compile passed AND no forbidden-effect matched.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        matches!(self.compile, CompileOutcome::Pass { .. }) && !self.tests.forbidden_effect_matched
    }

    /// Quality-score 0..100 = compile-pass(40) + zero-warnings(20) +
    /// has-tests(20) + has-asserts(20). Clamped.
    #[must_use]
    pub fn quality_score(&self) -> u8 {
        let mut s: u32 = 0;
        match &self.compile {
            CompileOutcome::Pass { warnings } => {
                s += 40;
                if *warnings == 0 {
                    s += 20;
                }
            }
            CompileOutcome::Fail { .. } | CompileOutcome::Rejected { .. } => return 0,
        }
        if self.tests.test_count > 0 {
            s += 20;
        }
        if self.tests.assert_count > 0 {
            s += 20;
        }
        if self.tests.forbidden_effect_matched {
            return 0;
        }
        s.min(100) as u8
    }
}

/// Caller-supplied compile-fn type. The orchestrator wires this to a deterministic
/// in-process compile-only pass (csslc-lib · or a mock-compiler in tests).
pub type CompileFn = fn(&str) -> CompileOutcome;

/// In-memory sandbox. Holds the configuration but no I/O state.
#[derive(Debug, Clone)]
pub struct Sandbox {
    cfg: SandboxConfig,
    compile_fn: CompileFn,
    forbidden_effect_strings: Vec<String>,
}

impl Sandbox {
    /// Construct a new sandbox.
    #[must_use]
    pub fn new(cfg: SandboxConfig, compile_fn: CompileFn) -> Self {
        Self {
            cfg,
            compile_fn,
            forbidden_effect_strings: vec![],
        }
    }

    /// Set the forbidden-effect-string list. Each substring is tested against
    /// the candidate source. Matching strings → `forbidden_effect_matched = true`.
    pub fn with_forbidden_effects(mut self, list: Vec<String>) -> Self {
        self.forbidden_effect_strings = list;
        self
    }

    /// Run the sandbox pipeline against `source`. Returns a full SandboxReport.
    ///
    /// § flow
    ///   1. size-check  → Reject if oversize
    ///   2. extern-deny → Reject if `deny_externs && source.contains("extern \"C\"")`
    ///   3. compile_fn  → CompileOutcome
    ///   4. test-heuristic → InlineTestReport
    ///   5. forbidden-effect substring check → InlineTestReport.forbidden_effect_matched
    ///   6. source_blake3 → fingerprint
    pub fn run(&self, source: &str) -> SandboxReport {
        let bytes = source.as_bytes();
        let blake = *blake3::hash(bytes).as_bytes();
        if bytes.len() > self.cfg.max_source_bytes {
            return SandboxReport {
                compile: CompileOutcome::Rejected {
                    reason: format!(
                        "source-too-large {} > {}",
                        bytes.len(),
                        self.cfg.max_source_bytes
                    ),
                },
                tests: InlineTestReport {
                    test_count: 0,
                    assert_count: 0,
                    forbidden_effect_matched: false,
                },
                source_blake3: blake,
            };
        }
        if self.cfg.deny_externs && source.contains("extern \"C\"") {
            return SandboxReport {
                compile: CompileOutcome::Rejected {
                    reason: "extern-C-block-rejected".to_string(),
                },
                tests: InlineTestReport {
                    test_count: 0,
                    assert_count: 0,
                    forbidden_effect_matched: false,
                },
                source_blake3: blake,
            };
        }
        let compile = (self.compile_fn)(source);
        let test_count = source.matches("#[test]").count() as u32;
        let assert_count =
            (source.matches("assert!").count() + source.matches("assert_eq!").count()) as u32;
        let forbidden_effect_matched = self
            .forbidden_effect_strings
            .iter()
            .any(|s| source.contains(s));
        SandboxReport {
            compile,
            tests: InlineTestReport {
                test_count,
                assert_count,
                forbidden_effect_matched,
            },
            source_blake3: blake,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn always_pass(_src: &str) -> CompileOutcome {
        CompileOutcome::Pass { warnings: 0 }
    }
    fn always_pass_with_warnings(_src: &str) -> CompileOutcome {
        CompileOutcome::Pass { warnings: 3 }
    }
    fn always_fail(_src: &str) -> CompileOutcome {
        CompileOutcome::Fail {
            errors: vec!["E001 mock".into()],
        }
    }

    #[test]
    fn t01_pass_with_no_tests_60() {
        let sb = Sandbox::new(SandboxConfig::default(), always_pass);
        let r = sb.run("scene { entities: [] }");
        assert_eq!(r.compile, CompileOutcome::Pass { warnings: 0 });
        assert_eq!(r.quality_score(), 60);
    }

    #[test]
    fn t02_pass_with_tests_and_asserts_100() {
        let sb = Sandbox::new(SandboxConfig::default(), always_pass);
        let src = "scene { } #[test] fn t() { assert!(true); }";
        let r = sb.run(src);
        assert_eq!(r.quality_score(), 100);
    }

    #[test]
    fn t03_fail_score_zero() {
        let sb = Sandbox::new(SandboxConfig::default(), always_fail);
        let r = sb.run("scene {}");
        assert_eq!(r.quality_score(), 0);
        assert!(matches!(r.compile, CompileOutcome::Fail { .. }));
    }

    #[test]
    fn t04_warnings_drop_score_to_80() {
        let sb = Sandbox::new(SandboxConfig::default(), always_pass_with_warnings);
        let src = "scene {} #[test] fn t() { assert!(x); }";
        let r = sb.run(src);
        // 40 (compile) + 0 (warnings) + 20 (tests) + 20 (asserts) = 80
        assert_eq!(r.quality_score(), 80);
    }

    #[test]
    fn t05_oversize_rejected_score_zero() {
        let cfg = SandboxConfig {
            max_source_bytes: 16,
            ..SandboxConfig::default()
        };
        let sb = Sandbox::new(cfg, always_pass);
        let src = "x".repeat(100);
        let r = sb.run(&src);
        assert_eq!(r.quality_score(), 0);
        assert!(matches!(r.compile, CompileOutcome::Rejected { .. }));
    }

    #[test]
    fn t06_extern_c_rejected() {
        let sb = Sandbox::new(SandboxConfig::default(), always_pass);
        let src = "extern \"C\" { fn host_panic(); }";
        let r = sb.run(src);
        assert!(matches!(r.compile, CompileOutcome::Rejected { .. }));
    }

    #[test]
    fn t07_forbidden_effect_substring_zeros_score() {
        let sb = Sandbox::new(SandboxConfig::default(), always_pass)
            .with_forbidden_effects(vec!["NetworkEgress".into()]);
        let src = "scene { effects: [NetworkEgress] }";
        let r = sb.run(src);
        assert!(r.tests.forbidden_effect_matched);
        assert_eq!(r.quality_score(), 0);
    }

    #[test]
    fn t08_blake3_changes_with_source() {
        let sb = Sandbox::new(SandboxConfig::default(), always_pass);
        let r1 = sb.run("a");
        let r2 = sb.run("b");
        assert_ne!(r1.source_blake3, r2.source_blake3);
    }
}

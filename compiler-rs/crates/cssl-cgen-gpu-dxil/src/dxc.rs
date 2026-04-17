//! `dxc.exe` subprocess invoker — compiles HLSL-text → DXIL binary.
//!
//! § STRATEGY
//!   Mirrors the T6-D1 MLIR-text-CLI + T9-D1 Z3-CLI fallback pattern. The `dxc` binary
//!   is looked up on PATH ; if absent, emission still succeeds (the HLSL text is
//!   returned) but validation is marked unavailable. CI installs DXC via the
//!   Windows-SDK / VS-Build-Tools package per `specs/07` § VALIDATION PIPELINE.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::target::DxilTargetProfile;

/// A prepared DXC invocation — HLSL text + profile + entry-point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxcInvocation {
    /// HLSL source text to compile.
    pub hlsl_text: String,
    /// HLSL profile (`-T cs_6_6`).
    pub profile: DxilTargetProfile,
    /// Entry-point name (`-E main`).
    pub entry_point: String,
    /// Extra args (e.g., `-Zi` / `-Qembed_debug`).
    pub extra_args: Vec<String>,
}

/// Outcome of a DXC subprocess invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxcOutcome {
    /// DXC binary was found + compilation succeeded. Binary bytes returned.
    Success { dxil_bytes: Vec<u8>, stderr: String },
    /// DXC binary was found but compilation reported diagnostics (non-zero status).
    DiagnosticFailure {
        status: i32,
        stdout: String,
        stderr: String,
    },
    /// DXC binary was not found on PATH — emission succeeded but validation unavailable.
    BinaryMissing,
    /// IO error communicating with the DXC subprocess (wraps the error description).
    IoError(String),
}

/// Wrapper around the `dxc` CLI binary.
#[derive(Debug, Clone, Default)]
pub struct DxcCliInvoker {
    /// Override path to the DXC binary. `None` = look up `dxc` on PATH.
    pub binary_path: Option<PathBuf>,
}

impl DxcCliInvoker {
    /// New invoker using the default `dxc` on PATH.
    #[must_use]
    pub const fn new() -> Self {
        Self { binary_path: None }
    }

    /// New invoker with an explicit binary path.
    #[must_use]
    pub fn with_binary(path: PathBuf) -> Self {
        Self {
            binary_path: Some(path),
        }
    }

    /// Spawn the DXC process and compile the HLSL text to DXIL bytes.
    ///
    /// Returns [`DxcOutcome::BinaryMissing`] if the binary is not on PATH (non-fatal).
    #[must_use]
    pub fn compile(&self, inv: &DxcInvocation) -> DxcOutcome {
        let binary = self
            .binary_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("dxc");
        let mut cmd = Command::new(binary);
        cmd.arg("-T")
            .arg(inv.profile.profile.render())
            .arg("-E")
            .arg(&inv.entry_point)
            .arg("-HV")
            .arg("2021");
        if inv.profile.enable_16_bit_types {
            cmd.arg("-enable-16bit-types");
        }
        if inv.profile.enable_dynamic_resources {
            cmd.arg("-DDYNAMIC_RESOURCES");
        }
        for a in &inv.extra_args {
            cmd.arg(a);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return DxcOutcome::BinaryMissing,
            Err(e) => return DxcOutcome::IoError(format!("{e}")),
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(inv.hlsl_text.as_bytes()) {
                return DxcOutcome::IoError(format!("stdin write : {e}"));
            }
        }
        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => return DxcOutcome::IoError(format!("wait : {e}")),
        };
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            DxcOutcome::Success {
                dxil_bytes: output.stdout,
                stderr,
            }
        } else {
            DxcOutcome::DiagnosticFailure {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DxcCliInvoker, DxcOutcome};
    use crate::target::DxilTargetProfile;
    use std::path::PathBuf;

    #[test]
    fn new_invoker_has_no_binary_override() {
        let inv = DxcCliInvoker::new();
        assert!(inv.binary_path.is_none());
    }

    #[test]
    fn with_binary_stores_path() {
        let inv = DxcCliInvoker::with_binary(PathBuf::from("C:/bin/dxc.exe"));
        assert!(inv.binary_path.is_some());
    }

    #[test]
    fn default_matches_new() {
        let a = DxcCliInvoker::default();
        assert!(a.binary_path.is_none());
    }

    #[test]
    fn outcome_variants_construct() {
        // Construct each outcome for coverage + equality shape.
        let a = DxcOutcome::BinaryMissing;
        let b = DxcOutcome::DiagnosticFailure {
            status: 1,
            stdout: String::new(),
            stderr: "error X".into(),
        };
        let c = DxcOutcome::Success {
            dxil_bytes: vec![0x44, 0x58, 0x42, 0x43], // DXBC magic
            stderr: String::new(),
        };
        let d = DxcOutcome::IoError("pipe broken".into());
        assert_eq!(a, DxcOutcome::BinaryMissing);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(c, d);
    }

    #[test]
    fn compile_with_missing_binary_does_not_panic() {
        // Force a binary that can't exist.
        let inv = DxcCliInvoker::with_binary(PathBuf::from(
            "C:/does/not/exist/no_such_dxc_binary_12345.exe",
        ));
        let invocation = super::DxcInvocation {
            hlsl_text: "void main() {}".into(),
            profile: DxilTargetProfile::compute_sm66_default(),
            entry_point: "main".into(),
            extra_args: vec![],
        };
        let outcome = inv.compile(&invocation);
        // Must be BinaryMissing ; CI runners without DXC still see a clean outcome.
        assert!(matches!(
            outcome,
            DxcOutcome::BinaryMissing | DxcOutcome::IoError(_)
        ));
    }
}

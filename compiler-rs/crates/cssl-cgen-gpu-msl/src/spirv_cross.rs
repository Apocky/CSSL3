//! `spirv-cross --msl` subprocess invoker.
//!
//! § STRATEGY
//!   Mirrors the `DxcCliInvoker` pattern from `cssl-cgen-gpu-dxil`. Phase-1 invokes
//!   `spirv-cross` as an external process to validate SPIR-V → MSL round-trip —
//!   used only in CI + tooling contexts. The CLI absence path returns
//!   [`SpirvCrossOutcome::BinaryMissing`] without a panic.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::target::MslTargetProfile;

/// Prepared spirv-cross invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpirvCrossInvocation {
    /// SPIR-V binary blob to translate.
    pub spirv_bytes: Vec<u8>,
    /// MSL target profile (version + platform + stage).
    pub profile: MslTargetProfile,
    /// Extra args.
    pub extra_args: Vec<String>,
}

/// Outcome of a spirv-cross run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpirvCrossOutcome {
    /// Success — returns the emitted MSL text.
    Success { msl_text: String, stderr: String },
    /// Failure — spirv-cross reported diagnostics.
    DiagnosticFailure {
        status: i32,
        stdout: String,
        stderr: String,
    },
    /// The binary was not found on PATH — fallback "internal-emit" path should be used.
    BinaryMissing,
    /// IO error communicating with the subprocess.
    IoError(String),
}

/// CLI wrapper.
#[derive(Debug, Clone, Default)]
pub struct SpirvCrossInvoker {
    /// Override path to the spirv-cross binary. `None` = look up `spirv-cross` on PATH.
    pub binary_path: Option<PathBuf>,
}

impl SpirvCrossInvoker {
    /// New invoker using the default `spirv-cross` on PATH.
    #[must_use]
    pub const fn new() -> Self {
        Self { binary_path: None }
    }

    /// New invoker with explicit path.
    #[must_use]
    pub fn with_binary(path: PathBuf) -> Self {
        Self {
            binary_path: Some(path),
        }
    }

    /// Invoke the subprocess. Non-panicking ; returns [`SpirvCrossOutcome::BinaryMissing`] when
    /// the binary is not found.
    #[must_use]
    pub fn translate(&self, inv: &SpirvCrossInvocation) -> SpirvCrossOutcome {
        let binary = self
            .binary_path
            .as_deref()
            .and_then(|p| p.to_str())
            .unwrap_or("spirv-cross");
        let mut cmd = Command::new(binary);
        cmd.arg("--msl")
            .arg("--msl-version")
            .arg(format!(
                "{}000",
                (inv.profile.version.underscored().replace('_', ""))
            ))
            .arg("--stage")
            .arg(stage_name(inv.profile.stage));
        for a in &inv.extra_args {
            cmd.arg(a);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return SpirvCrossOutcome::BinaryMissing
            }
            Err(e) => return SpirvCrossOutcome::IoError(format!("{e}")),
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(&inv.spirv_bytes) {
                return SpirvCrossOutcome::IoError(format!("stdin : {e}"));
            }
        }
        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => return SpirvCrossOutcome::IoError(format!("wait : {e}")),
        };
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            SpirvCrossOutcome::Success {
                msl_text: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr,
            }
        } else {
            SpirvCrossOutcome::DiagnosticFailure {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr,
            }
        }
    }
}

fn stage_name(stage: crate::target::MetalStage) -> &'static str {
    use crate::target::MetalStage as S;
    match stage {
        S::Vertex => "vert",
        S::Fragment => "frag",
        S::Kernel => "comp",
        S::Object | S::Mesh => "mesh",
        S::Tile => "comp",
        S::VisibleFunction => "callable",
    }
}

#[cfg(test)]
mod tests {
    use super::{SpirvCrossInvocation, SpirvCrossInvoker, SpirvCrossOutcome};
    use crate::target::MslTargetProfile;
    use std::path::PathBuf;

    #[test]
    fn new_invoker_no_binary() {
        let inv = SpirvCrossInvoker::new();
        assert!(inv.binary_path.is_none());
    }

    #[test]
    fn with_binary_stores_path() {
        let inv = SpirvCrossInvoker::with_binary(PathBuf::from("/usr/local/bin/spirv-cross"));
        assert!(inv.binary_path.is_some());
    }

    #[test]
    fn outcome_equality_shapes() {
        assert_eq!(
            SpirvCrossOutcome::BinaryMissing,
            SpirvCrossOutcome::BinaryMissing
        );
        let a = SpirvCrossOutcome::Success {
            msl_text: "void main() {}".into(),
            stderr: String::new(),
        };
        let b = SpirvCrossOutcome::Success {
            msl_text: "void main() {}".into(),
            stderr: String::new(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn translate_with_missing_binary_non_panicking() {
        let inv = SpirvCrossInvoker::with_binary(PathBuf::from(
            "C:/does/not/exist/no_spirv_cross_12345.exe",
        ));
        let outcome = inv.translate(&SpirvCrossInvocation {
            spirv_bytes: vec![0x03, 0x02, 0x23, 0x07], // SPIR-V magic
            profile: MslTargetProfile::kernel_default(),
            extra_args: vec![],
        });
        assert!(matches!(
            outcome,
            SpirvCrossOutcome::BinaryMissing | SpirvCrossOutcome::IoError(_)
        ));
    }
}

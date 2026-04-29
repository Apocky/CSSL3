//! `OmegaError` — every failure-mode the scheduler can surface.
//!
//! § Variants are STABLE from S8-H2 forward ; renaming = major-version-bump
//!   per the T11-D76 ABI-stability invariant. New variants append-only.

use thiserror::Error;

use crate::rng::RngStreamId;
use crate::system::SystemId;

/// All failures the omega-step scheduler can produce.
///
/// § Discriminants
/// | variant                 | meaning                                                           |
/// |-------------------------|-------------------------------------------------------------------|
/// | `DependencyCycle`       | systems form a cycle in the read/write dep graph                  |
/// | `SystemPanicked`        | a system's `step()` returned an error                             |
/// | `FrameOverbudget`       | `dt_used` exceeded the configured frame budget                    |
/// | `ConsentRevoked`        | a consent gate (`omega_register` etc.) was revoked mid-flight     |
/// | `UnknownSystem`         | scheduler asked to operate on a `SystemId` it never registered    |
/// | `DuplicateName`         | two systems registered with the same `name()`                     |
/// | `HaltedByKill`          | `halt()` was called ; subsequent `step()` calls report this       |
/// | `DeterminismViolation`  | a non-determinism source was detected (entropy RNG, fast-math, …) |
/// | `RngStreamUnregistered` | system requested an `RngStreamId` it never declared               |
// `Eq` is intentionally NOT derived : `FrameOverbudget` carries `f64` fields
// which are not `Eq` (NaN != NaN). `PartialEq` is sufficient for diagnostic
// equality assertions in tests + downstream consumers.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum OmegaError {
    /// Two or more systems form a cycle in their `dependencies()` declaration.
    /// Resolving requires editing the systems' read/write declarations or
    /// breaking the cycle with an explicit ordering constraint.
    #[error("OMEGA0001 — dependency cycle detected at system {system:?}")]
    DependencyCycle { system: SystemId },

    /// A system's `step()` returned an error. The error message is propagated
    /// verbatim ; the scheduler does not retry.
    #[error("OMEGA0002 — system {system:?} ('{name}') panicked at frame {frame}: {msg}")]
    SystemPanicked {
        system: SystemId,
        name: String,
        frame: u64,
        msg: String,
    },

    /// Wallclock `dt` exceeded the configured frame budget. Whether this is
    /// fatal or recoverable depends on `OverbudgetPolicy` (see `scheduler`).
    #[error(
        "OMEGA0003 — frame {frame} overbudget (used {dt_used} s, budget {budget} s, policy {policy})"
    )]
    FrameOverbudget {
        frame: u64,
        dt_used: f64,
        budget: f64,
        policy: &'static str,
    },

    /// A consent gate was revoked. The scheduler refuses to continue and
    /// surfaces an audit-log entry for the gate.
    #[error("OMEGA0004 — consent revoked for system {system:?} on gate '{gate}'")]
    ConsentRevoked {
        system: SystemId,
        gate: &'static str,
    },

    /// Operation referenced a `SystemId` that was never registered with the
    /// scheduler.
    #[error("OMEGA0005 — unknown system {id:?}")]
    UnknownSystem { id: SystemId },

    /// Two systems registered with byte-identical `name()` ; the scheduler
    /// requires unique names for replay-log readability.
    #[error("OMEGA0006 — duplicate system name '{name}'")]
    DuplicateName { name: String },

    /// `halt()` was invoked ; this is a terminal state. All subsequent
    /// `step()` calls report this error until the scheduler is dropped.
    #[error("OMEGA0007 — scheduler halted: {reason}")]
    HaltedByKill { reason: String },

    /// A determinism violation was detected. Examples : a system attempted
    /// to call `rand::thread_rng()` ; fast-math probe tripped ; FTZ/DAZ
    /// flags absent on a `PureDet` declaration.
    #[error("OMEGA0008 — determinism violation at frame {frame}: {kind}")]
    DeterminismViolation { frame: u64, kind: &'static str },

    /// A system requested an `RngStreamId` it never declared in
    /// `rng_streams()`. All RNG streams must be declared upfront so the
    /// scheduler can seed them deterministically.
    #[error("OMEGA0009 — RNG stream {stream:?} not registered for system {system:?}")]
    RngStreamUnregistered {
        system: SystemId,
        stream: RngStreamId,
    },
}

impl OmegaError {
    /// Stable five-character diagnostic code prefix for this variant. Used
    /// by the audit-chain to bucket errors by category at low overhead.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::DependencyCycle { .. } => "OMEGA0001",
            Self::SystemPanicked { .. } => "OMEGA0002",
            Self::FrameOverbudget { .. } => "OMEGA0003",
            Self::ConsentRevoked { .. } => "OMEGA0004",
            Self::UnknownSystem { .. } => "OMEGA0005",
            Self::DuplicateName { .. } => "OMEGA0006",
            Self::HaltedByKill { .. } => "OMEGA0007",
            Self::DeterminismViolation { .. } => "OMEGA0008",
            Self::RngStreamUnregistered { .. } => "OMEGA0009",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_stable() {
        assert_eq!(
            OmegaError::DependencyCycle {
                system: SystemId(0)
            }
            .code(),
            "OMEGA0001"
        );
        assert_eq!(
            OmegaError::HaltedByKill {
                reason: "kill-switch".into()
            }
            .code(),
            "OMEGA0007"
        );
        assert_eq!(
            OmegaError::DeterminismViolation {
                frame: 0,
                kind: "entropy",
            }
            .code(),
            "OMEGA0008"
        );
    }

    #[test]
    fn display_renders() {
        let e = OmegaError::FrameOverbudget {
            frame: 7,
            dt_used: 0.020,
            budget: 0.016,
            policy: "Halt",
        };
        let s = e.to_string();
        assert!(s.contains("frame 7"));
        assert!(s.contains("Halt"));
    }
}

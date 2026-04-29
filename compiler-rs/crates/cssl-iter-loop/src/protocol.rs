//! 9-step bug-fix iteration-loop state-machine.
//!
//! Mirrors `_drafts/phase_j/wave_ji_iteration_loop_docs.md` § 1 — the canonical
//! ATTACH → STATE → FOCUS → IDENTIFY → PATCH → RELOAD → VERIFY → COMMIT → ITERATE
//! flow. The state-machine is a pure transition graph : each method takes
//! `self` by value and returns the next state (or an error). Callers drive the
//! machine to the verified-and-committed terminal or to the explicit `Failed`
//! sink ; intermediate inspection is supported via `kind()` + `current_step()`.
//!
//! § Σ-discipline
//!   No state in this enum carries cell-data, biometric data, or sovereign-
//!   private payloads. The `IssueReport.detail` and `FailureReason.message`
//!   fields are short human-prose strings ; downstream consumers must avoid
//!   embedding raw paths (use the `cssl_log::PathHashField` discipline if
//!   serializing to disk via cssl-log sinks).
//!
//! § INTEGRATION-POINT D233/01 — `McpSessionStub` swaps to real
//!   `cssl_mcp_server::Session` when S2-A2 D229 lands. The stub captures
//!   the SHAPE the state-machine needs (a session-id + transport descriptor)
//!   without depending on the upstream crate.
//!
//! § INTEGRATION-POINT D233/02 — `EngineState` swaps to real
//!   `cssl_substrate_omega_field::EngineStateSnapshot` when that crate lands.

use std::fmt;

use blake3::Hash as Blake3Hash;
use serde::{Deserialize, Serialize};

/// Opaque commit-hash. Stored as the canonical 32-byte BLAKE3 digest so the
/// type round-trips through serde + audit-bus identically across hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitHash(pub [u8; 32]);

impl CommitHash {
    /// Construct from a hex-encoded git SHA. Pads with the canonical zero-
    /// suffix because git SHAs are 40 hex chars (20 bytes) ; the high 12
    /// bytes are reserved for future tagging (e.g. signed-commit marker).
    pub fn from_git_sha_hex(hex: &str) -> Result<Self, ProtocolError> {
        let trimmed = hex.trim();
        if trimmed.len() != 40 {
            return Err(ProtocolError::InvalidCommitHash {
                detail: format!("expected 40 hex chars, got {}", trimmed.len()),
            });
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in trimmed.as_bytes().chunks(2).enumerate().take(20) {
            let s = std::str::from_utf8(chunk).map_err(|e| ProtocolError::InvalidCommitHash {
                detail: format!("non-utf8 in hex : {e}"),
            })?;
            bytes[i] = u8::from_str_radix(s, 16).map_err(|e| ProtocolError::InvalidCommitHash {
                detail: format!("non-hex char : {e}"),
            })?;
        }
        Ok(Self(bytes))
    }

    /// Convenience hex-encoding for log-records + commit-message anchors.
    pub fn to_hex(self) -> String {
        let mut out = String::with_capacity(64);
        for b in self.0 {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
}

impl From<Blake3Hash> for CommitHash {
    fn from(h: Blake3Hash) -> Self {
        Self(*h.as_bytes())
    }
}

/// Identifier for a single MCP session — opaque 16-byte handle. Real
/// implementations derive this from a CSPRNG ; the stub uses caller-provided
/// bytes so tests can pin deterministic ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub [u8; 16]);

impl SessionId {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

/// Identifier returned by a hot-reload swap. Used to correlate verify-phase
/// telemetry with the swap that introduced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReloadId(pub u64);

/// § INTEGRATION-POINT D233/01 — stub for the real MCP session.
///
/// Carries the minimum the iteration-loop needs : a session-id + a transport
/// descriptor. Real implementations replace this with `cssl_mcp_server::Session`
/// when S2-A2 D229 lands ; the swap is one type alias.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSessionStub {
    pub session_id: SessionId,
    pub transport: String,
    pub principal: String,
}

impl McpSessionStub {
    pub fn new(
        session_id: SessionId,
        transport: impl Into<String>,
        principal: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            transport: transport.into(),
            principal: principal.into(),
        }
    }
}

/// § INTEGRATION-POINT D233/02 — stub for the real engine-state snapshot.
///
/// Captures the aggregate-only fields the state-machine needs to reason about
/// "is the engine alive, what frame is it on, what's its health". No cell-data
/// here — that flows through cssl-inspect and never enters this layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineState {
    pub frame_n: u64,
    pub tick_rate_hz_x1000: u32,
    pub health_overall: EngineHealth,
    pub audit_chain_seq: u64,
}

impl EngineState {
    pub fn fresh(frame_n: u64) -> Self {
        Self {
            frame_n,
            tick_rate_hz_x1000: 60_000, // 60 Hz × 1000 fixed-point
            health_overall: EngineHealth::Healthy,
            audit_chain_seq: 0,
        }
    }
}

/// Three-tier engine health — mirrors the `EngineStateSnapshot.health`
/// enum from 08_l5_mcp_llm_spec.md § 13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineHealth {
    Healthy,
    Degraded,
    Critical,
}

/// What kind of issue the iteration-loop is fixing. Drives the verify-phase
/// invariant-set + the commit-message template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    /// Cosmetic / documentation drift. Verify = lint pass.
    Cosmetic,
    /// Functional bug : visible behavior diverges from spec.
    Bug,
    /// Performance regression : tail latency or memory growth above bound.
    PerfRegression,
    /// Invariant violation : asserted contract broken (highest priority).
    InvariantViolation,
}

/// Issue identified at step-3/4. Carries the spec-anchor + invariant-id so
/// step-7 verification can re-check the exact conditions that triggered the
/// loop. The `detail` string is a short human-prose summary safe for audit-
/// chain inclusion (no biometric / sovereign-private payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueReport {
    pub severity: IssueSeverity,
    pub spec_anchor_key: String,
    pub failing_invariant: Option<String>,
    pub detail: String,
}

impl IssueReport {
    pub fn new(
        severity: IssueSeverity,
        spec_anchor_key: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            spec_anchor_key: spec_anchor_key.into(),
            failing_invariant: None,
            detail: detail.into(),
        }
    }

    pub fn with_invariant(mut self, name: impl Into<String>) -> Self {
        self.failing_invariant = Some(name.into());
        self
    }
}

/// Reason a state-machine ran ended in `Failed`.
///
/// Reflects the four canonical failure-shapes from `wave_ji_iteration_loop_docs.md` :
///   - `MaxIterations` : exhausted the 3-cycle Critic-veto budget
///   - `InvariantStillFailing` : verify-phase detected the patch did not fix
///   - `RegressionIntroduced` : verify-phase found NEW failures
///   - `KillSwitchFired` : PRIME-DIRECTIVE violation halted the engine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    MaxIterations {
        cycles: u32,
    },
    InvariantStillFailing {
        name: String,
        message: String,
    },
    RegressionIntroduced {
        message: String,
    },
    KillSwitchFired {
        message: String,
    },
    /// Catch-all for unexpected failure shapes ; carries a short prose tag.
    Other {
        message: String,
    },
}

impl fmt::Display for FailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FailureReason::MaxIterations { cycles } => {
                write!(f, "max-iterations exhausted after {cycles} cycles")
            }
            FailureReason::InvariantStillFailing { name, message } => {
                write!(f, "invariant '{name}' still failing : {message}")
            }
            FailureReason::RegressionIntroduced { message } => {
                write!(f, "regression introduced : {message}")
            }
            FailureReason::KillSwitchFired { message } => {
                write!(f, "kill-switch fired : {message}")
            }
            FailureReason::Other { message } => write!(f, "{message}"),
        }
    }
}

/// The 9-step iteration-loop state. Each variant is a checkpoint that the
/// state-machine reaches in canonical order. `Failed` is a terminal sink that
/// any step can transition into. `Committed` is the success terminal.
///
/// Step mapping (per `_drafts/phase_j/wave_ji_iteration_loop_docs.md` § 1.1) :
///   1. ATTACH    ⇒ `Attached`
///   2. STATE     ⇒ `StateQueried`
///   3. FOCUS     ⇒ folded into `Identified` — the focus-data is implicit in `IssueReport`
///   4. IDENTIFY  ⇒ `Identified`
///   5. PATCH     ⇒ `Patched`
///   6. RELOAD    ⇒ `HotReloaded`
///   7. VERIFY    ⇒ `Verified`
///   8. COMMIT    ⇒ `Committed`
///   9. ITERATE   ⇒ implemented as `iterate_back_to_identify()` transition
///
/// The mapping collapses step-3 (FOCUS) into step-4 (IDENTIFY) because the
/// focus-call returns data that is interpreted in the identify step ; the
/// state-transition graph stays identical to the spec's narrative flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IterationLoopState {
    /// Step-1 : engine attached + handshake complete. CapSet finalized.
    Attached { mcp_session: McpSessionStub },

    /// Step-2 : engine state queried ; ready to focus on the suspect region.
    StateQueried { snapshot: EngineState },

    /// Step-3-4 : issue identified ; spec-anchor + invariant + history correlated.
    Identified { issue: IssueReport },

    /// Step-5 : source-file patched (Edit/Write). Git commit reserved for step-8.
    Patched { commit: CommitHash },

    /// Step-6 : hot-reload applied. ReloadId correlates with verify-phase telemetry.
    HotReloaded { reload_id: ReloadId },

    /// Step-7 : verification result. `invariant_pass=true` ⇒ ready to commit.
    Verified { invariant_pass: bool },

    /// Step-8-9 : final commit landed ; loop terminates successfully.
    Committed { final_commit: CommitHash },

    /// Terminal failure sink. Any step can transition here on error.
    Failed { reason: FailureReason },
}

impl IterationLoopState {
    /// Discriminant string for log/diagnostic use.
    pub fn kind(&self) -> &'static str {
        match self {
            IterationLoopState::Attached { .. } => "Attached",
            IterationLoopState::StateQueried { .. } => "StateQueried",
            IterationLoopState::Identified { .. } => "Identified",
            IterationLoopState::Patched { .. } => "Patched",
            IterationLoopState::HotReloaded { .. } => "HotReloaded",
            IterationLoopState::Verified { .. } => "Verified",
            IterationLoopState::Committed { .. } => "Committed",
            IterationLoopState::Failed { .. } => "Failed",
        }
    }

    /// Step number (1-9 ; Failed = 0). Useful for rendering progress bars.
    pub fn step_number(&self) -> u8 {
        match self {
            IterationLoopState::Failed { .. } => 0,
            IterationLoopState::Attached { .. } => 1,
            IterationLoopState::StateQueried { .. } => 2,
            IterationLoopState::Identified { .. } => 4,
            IterationLoopState::Patched { .. } => 5,
            IterationLoopState::HotReloaded { .. } => 6,
            IterationLoopState::Verified { .. } => 7,
            IterationLoopState::Committed { .. } => 8,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            IterationLoopState::Committed { .. } | IterationLoopState::Failed { .. }
        )
    }

    pub fn is_success(&self) -> bool {
        matches!(self, IterationLoopState::Committed { .. })
    }
}

/// Errors emitted by the protocol state-machine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// Caller invoked a transition that is not legal from the current state.
    #[error("invalid transition : from {from} ⇒ {to}")]
    InvalidTransition {
        from: &'static str,
        to: &'static str,
    },

    /// Commit-hash parsing failed.
    #[error("invalid commit-hash : {detail}")]
    InvalidCommitHash { detail: String },

    /// Iteration-cycle budget exhausted.
    #[error("max-iterations exhausted : {cycles} cycles consumed")]
    MaxIterationsExhausted { cycles: u32 },

    /// The state-machine was called after reaching a terminal state.
    #[error("terminal state ; no further transitions permitted")]
    TerminalState,
}

/// Driver for the 9-step iteration-loop state-machine.
///
/// Owns the state + the cycle-counter + the max-cycles bound (default 3 per
/// pod-composition rules § 03 Critic-veto policy). Transitions return new
/// machines so callers can fork at decision-points (e.g. one branch retries
/// from IDENTIFY, another commits — both are valid states).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolStateMachine {
    state: IterationLoopState,
    cycle: u32,
    max_cycles: u32,
}

impl ProtocolStateMachine {
    /// Default max-iterations bound from `wave_ji_iteration_loop_docs.md` § 1.10.
    pub const DEFAULT_MAX_CYCLES: u32 = 3;

    /// Construct a fresh state-machine starting at step-1 ATTACH.
    pub fn attach(mcp_session: McpSessionStub) -> Self {
        Self {
            state: IterationLoopState::Attached { mcp_session },
            cycle: 0,
            max_cycles: Self::DEFAULT_MAX_CYCLES,
        }
    }

    /// Construct with a non-default max-cycles (e.g. for slow-bug deep-dives).
    pub fn attach_with_max_cycles(mcp_session: McpSessionStub, max_cycles: u32) -> Self {
        Self {
            state: IterationLoopState::Attached { mcp_session },
            cycle: 0,
            max_cycles,
        }
    }

    pub fn current_state(&self) -> &IterationLoopState {
        &self.state
    }

    pub fn current_step(&self) -> u8 {
        self.state.step_number()
    }

    pub fn cycle(&self) -> u32 {
        self.cycle
    }

    pub fn max_cycles(&self) -> u32 {
        self.max_cycles
    }

    /// Step-2 : transition Attached ⇒ StateQueried.
    pub fn query_state(self, snapshot: EngineState) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::Attached { .. } => Ok(Self {
                state: IterationLoopState::StateQueried { snapshot },
                ..self
            }),
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "StateQueried",
            }),
        }
    }

    /// Step-4 : transition StateQueried (or Verified-fail-loop) ⇒ Identified.
    pub fn identify(self, issue: IssueReport) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::StateQueried { .. }
            | IterationLoopState::Verified {
                invariant_pass: false,
            } => Ok(Self {
                state: IterationLoopState::Identified { issue },
                ..self
            }),
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "Identified",
            }),
        }
    }

    /// Step-5 : transition Identified ⇒ Patched.
    pub fn patch(self, commit: CommitHash) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::Identified { .. } => Ok(Self {
                state: IterationLoopState::Patched { commit },
                ..self
            }),
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "Patched",
            }),
        }
    }

    /// Step-6 : transition Patched ⇒ HotReloaded.
    pub fn hot_reload(self, reload_id: ReloadId) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::Patched { .. } => Ok(Self {
                state: IterationLoopState::HotReloaded { reload_id },
                ..self
            }),
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "HotReloaded",
            }),
        }
    }

    /// Step-7 : transition HotReloaded ⇒ Verified.
    pub fn verify(self, invariant_pass: bool) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::HotReloaded { .. } => Ok(Self {
                state: IterationLoopState::Verified { invariant_pass },
                ..self
            }),
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "Verified",
            }),
        }
    }

    /// Step-8 : transition Verified(true) ⇒ Committed.
    pub fn commit(self, final_commit: CommitHash) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::Verified {
                invariant_pass: true,
            } => Ok(Self {
                state: IterationLoopState::Committed { final_commit },
                ..self
            }),
            IterationLoopState::Verified {
                invariant_pass: false,
            } => Err(ProtocolError::InvalidTransition {
                from: "Verified(false)",
                to: "Committed",
            }),
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "Committed",
            }),
        }
    }

    /// Step-9 : transition Verified(false) ⇒ Identified (refined hypothesis)
    /// AFTER incrementing the cycle counter. If the cycle would exceed
    /// `max_cycles`, the machine moves to `Failed{ MaxIterations }` instead.
    pub fn iterate(self, refined_issue: IssueReport) -> Result<Self, ProtocolError> {
        match self.state {
            IterationLoopState::Verified {
                invariant_pass: false,
            } => {
                let next_cycle = self.cycle + 1;
                if next_cycle >= self.max_cycles {
                    Ok(Self {
                        state: IterationLoopState::Failed {
                            reason: FailureReason::MaxIterations { cycles: next_cycle },
                        },
                        cycle: next_cycle,
                        max_cycles: self.max_cycles,
                    })
                } else {
                    Ok(Self {
                        state: IterationLoopState::Identified {
                            issue: refined_issue,
                        },
                        cycle: next_cycle,
                        max_cycles: self.max_cycles,
                    })
                }
            }
            ref s => Err(ProtocolError::InvalidTransition {
                from: s.kind(),
                to: "Iterate→Identified",
            }),
        }
    }

    /// Universal fail-transition. Any non-terminal state can move to `Failed`.
    /// Idempotent if already `Failed` — preserves the original reason.
    pub fn fail(self, reason: FailureReason) -> Self {
        if matches!(self.state, IterationLoopState::Failed { .. }) {
            return self;
        }
        Self {
            state: IterationLoopState::Failed { reason },
            ..self
        }
    }

    /// Helper : true if the machine is at a terminal (success or failure).
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Helper : true if terminal AND successful.
    pub fn is_success(&self) -> bool {
        self.state.is_success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_session() -> McpSessionStub {
        McpSessionStub::new(SessionId::from_bytes([1u8; 16]), "stdio", "DevModeChild")
    }

    fn fresh_issue() -> IssueReport {
        IssueReport::new(
            IssueSeverity::Bug,
            "Omniverse/02_CSSL/05_wave_solver § III.2",
            "psi_norm violated by 0.003",
        )
        .with_invariant("wave_solver.psi_norm_conserved")
    }

    fn fresh_commit() -> CommitHash {
        CommitHash([7u8; 32])
    }

    #[test]
    fn protocol_starts_at_attached() {
        let m = ProtocolStateMachine::attach(fresh_session());
        assert_eq!(m.current_state().kind(), "Attached");
        assert_eq!(m.current_step(), 1);
        assert_eq!(m.cycle(), 0);
        assert!(!m.is_terminal());
    }

    #[test]
    fn protocol_full_happy_path_to_commit() {
        let m = ProtocolStateMachine::attach(fresh_session());
        let m = m.query_state(EngineState::fresh(12000)).unwrap();
        assert_eq!(m.current_step(), 2);
        let m = m.identify(fresh_issue()).unwrap();
        assert_eq!(m.current_step(), 4);
        let m = m.patch(fresh_commit()).unwrap();
        assert_eq!(m.current_step(), 5);
        let m = m.hot_reload(ReloadId(42)).unwrap();
        assert_eq!(m.current_step(), 6);
        let m = m.verify(true).unwrap();
        assert_eq!(m.current_step(), 7);
        let m = m.commit(fresh_commit()).unwrap();
        assert_eq!(m.current_step(), 8);
        assert!(m.is_terminal());
        assert!(m.is_success());
    }

    #[test]
    fn protocol_cannot_skip_state_query() {
        let m = ProtocolStateMachine::attach(fresh_session());
        let r = m.identify(fresh_issue());
        assert!(matches!(r, Err(ProtocolError::InvalidTransition { .. })));
    }

    #[test]
    fn protocol_cannot_commit_on_failed_verify() {
        let m = ProtocolStateMachine::attach(fresh_session())
            .query_state(EngineState::fresh(1))
            .unwrap()
            .identify(fresh_issue())
            .unwrap()
            .patch(fresh_commit())
            .unwrap()
            .hot_reload(ReloadId(0))
            .unwrap()
            .verify(false)
            .unwrap();
        let r = m.commit(fresh_commit());
        assert!(matches!(
            r,
            Err(ProtocolError::InvalidTransition {
                from: "Verified(false)",
                ..
            })
        ));
    }

    #[test]
    fn protocol_iterate_loops_back_to_identified() {
        let m = ProtocolStateMachine::attach(fresh_session())
            .query_state(EngineState::fresh(1))
            .unwrap()
            .identify(fresh_issue())
            .unwrap()
            .patch(fresh_commit())
            .unwrap()
            .hot_reload(ReloadId(0))
            .unwrap()
            .verify(false)
            .unwrap();
        let m2 = m.iterate(fresh_issue()).unwrap();
        assert_eq!(m2.current_state().kind(), "Identified");
        assert_eq!(m2.cycle(), 1);
    }

    #[test]
    fn protocol_iterate_exhausts_max_cycles() {
        // max=2 ⇒ 1 successful re-loop, 2nd iterate-attempt fails over.
        let mut m = ProtocolStateMachine::attach_with_max_cycles(fresh_session(), 2)
            .query_state(EngineState::fresh(1))
            .unwrap()
            .identify(fresh_issue())
            .unwrap()
            .patch(fresh_commit())
            .unwrap()
            .hot_reload(ReloadId(0))
            .unwrap()
            .verify(false)
            .unwrap();
        m = m.iterate(fresh_issue()).unwrap();
        // back at Identified, cycle=1
        m = m.patch(fresh_commit()).unwrap();
        m = m.hot_reload(ReloadId(1)).unwrap();
        m = m.verify(false).unwrap();
        m = m.iterate(fresh_issue()).unwrap();
        // Should have flipped to Failed{MaxIterations}.
        assert!(matches!(
            m.current_state(),
            IterationLoopState::Failed { .. }
        ));
        assert_eq!(m.cycle(), 2);
    }

    #[test]
    fn protocol_fail_transition_idempotent() {
        let m = ProtocolStateMachine::attach(fresh_session()).fail(FailureReason::Other {
            message: "test".into(),
        });
        assert!(matches!(
            m.current_state(),
            IterationLoopState::Failed { .. }
        ));
        let m2 = m.clone().fail(FailureReason::KillSwitchFired {
            message: "second".into(),
        });
        if let IterationLoopState::Failed { reason } = m2.current_state() {
            // First-fail wins, idempotent.
            assert_eq!(format!("{reason}"), "test");
        } else {
            panic!("expected Failed");
        }
    }

    #[test]
    fn commit_hash_round_trip() {
        let hex = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let h = CommitHash::from_git_sha_hex(hex).unwrap();
        let s = h.to_hex();
        assert!(s.starts_with("deadbeefdeadbeef"));
    }

    #[test]
    fn commit_hash_rejects_short_sha() {
        let r = CommitHash::from_git_sha_hex("deadbeef");
        assert!(matches!(r, Err(ProtocolError::InvalidCommitHash { .. })));
    }

    #[test]
    fn commit_hash_rejects_non_hex() {
        let r = CommitHash::from_git_sha_hex("zzdeadbeef".repeat(4).as_str());
        assert!(matches!(r, Err(ProtocolError::InvalidCommitHash { .. })));
    }

    #[test]
    fn failure_reason_display() {
        let r = FailureReason::MaxIterations { cycles: 3 };
        assert!(format!("{r}").contains("max-iterations"));
        let r = FailureReason::InvariantStillFailing {
            name: "psi_norm".into(),
            message: "drift".into(),
        };
        assert!(format!("{r}").contains("psi_norm"));
        let r = FailureReason::KillSwitchFired {
            message: "biometric-egress".into(),
        };
        assert!(format!("{r}").contains("kill-switch"));
    }
}

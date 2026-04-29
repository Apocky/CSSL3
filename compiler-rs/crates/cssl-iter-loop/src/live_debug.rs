//! Live-debug session orchestration : pause / step / inspect / tweak / resume.
//!
//! Mirrors `_drafts/phase_j/wave_ji_iteration_loop_docs.md` § 5 — the
//! single-step-debugger UX that wraps cssl-inspect's `TimeControl`,
//! cssl-tweak's `TunableRegistry`, and the cssl-hot-reload swap-event queue
//! into one session-shaped object.
//!
//! § Σ-discipline (§ 5.4)
//!   Same as bug-fix-loop : every cell-touching `inspect` call routes through
//!   the upstream `cssl-inspect` Σ-mask check. This module never touches a
//!   raw cell — it only manages the SESSION-level state (frame counter, step
//!   count, paused flag, replay-recording flag).
//!
//! § INTEGRATION-POINT D233/07 — In a real engine attach, this module
//!   bridges to `cssl_inspect::TimeControl` (for pause/step) +
//!   `cssl_tweak::TunableRegistry` (for set_tunable) + the engine's
//!   inspect/snapshot APIs. The current implementation is a deterministic
//!   in-memory orchestrator that mirrors the SHAPE of the real cycle so
//!   tests + Wave-Jθ MCP-tool authoring have a reference behavior.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::protocol::{EngineState, ReloadId};

/// A typed tunable value the live-debug session can write. Mirrors the
/// `TunableValue` shape from `cssl-tweak`. We keep a local enum so this
/// module stays usable without forcing every consumer to import the full
/// cssl-tweak API ; conversion impls live near the swap-point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TunableValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl TunableValue {
    pub fn kind(&self) -> &'static str {
        match self {
            TunableValue::Bool(_) => "bool",
            TunableValue::Int(_) => "int",
            TunableValue::Float(_) => "float",
            TunableValue::Text(_) => "text",
        }
    }
}

/// One step recorded in a live-debug session. Replay-determinism boundary :
/// every entry can be re-emitted to reconstruct the session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiveDebugStep {
    Paused { frame_n: u64 },
    Stepped { frames: u64, end_frame_n: u64 },
    Inspected { frame_n: u64, target: String },
    TunableSet { name: String, value: TunableValue },
    HotReloaded { reload_id: ReloadId },
    Resumed { frame_n: u64 },
}

/// Errors emitted by the live-debug session.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LiveDebugError {
    /// Caller invoked an op that is illegal for the current session-state
    /// (e.g. `step` while running ; `resume` while running).
    #[error("invalid live-debug transition : op={op} state={state}")]
    InvalidTransition {
        op: &'static str,
        state: &'static str,
    },

    /// Tunable-write rejected — the name is unknown OR the value-kind disagrees.
    #[error("tunable rejected : {detail}")]
    TunableRejected { detail: String },

    /// Step-count exceeds the safety cap (1_000_000 frames per step).
    #[error("step too large : {frames} > {cap}")]
    StepTooLarge { frames: u64, cap: u64 },
}

/// Maximum frames per single `step_n_frames` call. Prevents accidental
/// integer-overflow panics in tests + a runaway debug-session in real use.
pub const MAX_STEP_FRAMES: u64 = 1_000_000;

/// The visible runtime-state surface returned by `inspect_current`.
/// Carries the kind of read-only-summary an LLM would consume during
/// step-3 FOCUS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineInspection {
    pub frame_n: u64,
    pub paused: bool,
    pub tunables: BTreeMap<String, TunableValue>,
    pub last_reload: Option<ReloadId>,
    pub step_count: u64,
}

/// A live-debug session : the canonical pause-step-inspect-tweak-resume
/// dance. Records every operation into a replay-trail so the session can be
/// handed-off (per § 6.5) or post-mortem-replayed (per § 5.5).
#[derive(Debug, Clone)]
pub struct LiveDebugSession {
    state: EngineState,
    paused: bool,
    tunables: BTreeMap<String, TunableValue>,
    last_reload: Option<ReloadId>,
    trail: Vec<LiveDebugStep>,
    step_count: u64,
}

impl LiveDebugSession {
    /// Construct a fresh session at the given engine-state. The session
    /// starts in the RUNNING state — callers must `pause_engine()` before
    /// stepping or inspecting per the § 5.2 canonical flow.
    pub fn attach(initial: EngineState) -> Self {
        Self {
            state: initial,
            paused: false,
            tunables: BTreeMap::new(),
            last_reload: None,
            trail: Vec::new(),
            step_count: 0,
        }
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn current_frame(&self) -> u64 {
        self.state.frame_n
    }

    pub fn trail(&self) -> &[LiveDebugStep] {
        &self.trail
    }

    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Pause the engine at the current frame-boundary.
    pub fn pause_engine(&mut self) -> Result<(), LiveDebugError> {
        if self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "pause_engine",
                state: "Paused",
            });
        }
        self.paused = true;
        self.trail.push(LiveDebugStep::Paused {
            frame_n: self.state.frame_n,
        });
        Ok(())
    }

    /// Step `n` frames forward. Requires the engine to be paused.
    pub fn step_n_frames(&mut self, n: u64) -> Result<(), LiveDebugError> {
        if !self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "step_n_frames",
                state: "Running",
            });
        }
        if n > MAX_STEP_FRAMES {
            return Err(LiveDebugError::StepTooLarge {
                frames: n,
                cap: MAX_STEP_FRAMES,
            });
        }
        self.state.frame_n = self.state.frame_n.saturating_add(n);
        self.step_count = self.step_count.saturating_add(n);
        self.trail.push(LiveDebugStep::Stepped {
            frames: n,
            end_frame_n: self.state.frame_n,
        });
        Ok(())
    }

    /// Inspect the current engine-state. Returns a redacted summary safe for
    /// audit-chain inclusion (no cell-data, no biometric). Requires pause.
    pub fn inspect_current(&self) -> Result<EngineInspection, LiveDebugError> {
        if !self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "inspect_current",
                state: "Running",
            });
        }
        Ok(EngineInspection {
            frame_n: self.state.frame_n,
            paused: self.paused,
            tunables: self.tunables.clone(),
            last_reload: self.last_reload,
            step_count: self.step_count,
        })
    }

    /// Record an inspection of a named target (cell-morton-hash | entity-id |
    /// invariant-name). Real implementation calls into cssl-inspect ;
    /// the orchestrator records the trail-entry for replay-determinism.
    pub fn record_inspect(&mut self, target: impl Into<String>) -> Result<(), LiveDebugError> {
        if !self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "record_inspect",
                state: "Running",
            });
        }
        self.trail.push(LiveDebugStep::Inspected {
            frame_n: self.state.frame_n,
            target: target.into(),
        });
        Ok(())
    }

    /// Set a tunable value. Tunable kind must match — `tweak_value("foo", Int(1))`
    /// after `tweak_value("foo", Bool(true))` is rejected.
    pub fn tweak_value(&mut self, name: &str, value: TunableValue) -> Result<(), LiveDebugError> {
        if !self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "tweak_value",
                state: "Running",
            });
        }
        if let Some(prev) = self.tunables.get(name) {
            if prev.kind() != value.kind() {
                return Err(LiveDebugError::TunableRejected {
                    detail: format!(
                        "kind mismatch on '{name}' : prior={prev_k}, new={new_k}",
                        prev_k = prev.kind(),
                        new_k = value.kind()
                    ),
                });
            }
        }
        self.tunables.insert(name.to_string(), value.clone());
        self.trail.push(LiveDebugStep::TunableSet {
            name: name.to_string(),
            value,
        });
        Ok(())
    }

    /// Record a hot-reload event. Bridges to `cssl-hot-reload` in real impl.
    pub fn record_hot_reload(&mut self, reload_id: ReloadId) -> Result<(), LiveDebugError> {
        if !self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "record_hot_reload",
                state: "Running",
            });
        }
        self.last_reload = Some(reload_id);
        self.trail.push(LiveDebugStep::HotReloaded { reload_id });
        Ok(())
    }

    /// Resume the engine. Loop terminates ; subsequent ops require re-pause.
    pub fn resume_engine(&mut self) -> Result<(), LiveDebugError> {
        if !self.paused {
            return Err(LiveDebugError::InvalidTransition {
                op: "resume_engine",
                state: "Running",
            });
        }
        self.paused = false;
        self.trail.push(LiveDebugStep::Resumed {
            frame_n: self.state.frame_n,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::EngineState;

    fn fresh_session() -> LiveDebugSession {
        LiveDebugSession::attach(EngineState::fresh(1000))
    }

    #[test]
    fn live_debug_attach_starts_running() {
        let s = fresh_session();
        assert!(!s.paused());
        assert_eq!(s.current_frame(), 1000);
        assert_eq!(s.trail().len(), 0);
    }

    #[test]
    fn live_debug_pause_then_step() {
        let mut s = fresh_session();
        s.pause_engine().unwrap();
        assert!(s.paused());
        s.step_n_frames(5).unwrap();
        assert_eq!(s.current_frame(), 1005);
        assert_eq!(s.step_count(), 5);
        assert_eq!(s.trail().len(), 2);
    }

    #[test]
    fn live_debug_step_requires_pause() {
        let mut s = fresh_session();
        let r = s.step_n_frames(1);
        assert!(matches!(
            r,
            Err(LiveDebugError::InvalidTransition {
                op: "step_n_frames",
                ..
            })
        ));
    }

    #[test]
    fn live_debug_inspect_requires_pause() {
        let s = fresh_session();
        let r = s.inspect_current();
        assert!(matches!(r, Err(LiveDebugError::InvalidTransition { .. })));
    }

    #[test]
    fn live_debug_step_too_large_rejected() {
        let mut s = fresh_session();
        s.pause_engine().unwrap();
        let r = s.step_n_frames(MAX_STEP_FRAMES + 1);
        assert!(matches!(r, Err(LiveDebugError::StepTooLarge { .. })));
    }

    #[test]
    fn live_debug_tweak_kind_consistency_enforced() {
        let mut s = fresh_session();
        s.pause_engine().unwrap();
        s.tweak_value("dt_floor", TunableValue::Float(1e-6))
            .unwrap();
        let r = s.tweak_value("dt_floor", TunableValue::Bool(true));
        assert!(matches!(r, Err(LiveDebugError::TunableRejected { .. })));
    }

    #[test]
    fn live_debug_tweak_idempotent_within_kind() {
        let mut s = fresh_session();
        s.pause_engine().unwrap();
        s.tweak_value("dt_floor", TunableValue::Float(1e-6))
            .unwrap();
        s.tweak_value("dt_floor", TunableValue::Float(1e-7))
            .unwrap();
        let inspect = s.inspect_current().unwrap();
        assert!(matches!(
            inspect.tunables.get("dt_floor"),
            Some(TunableValue::Float(_))
        ));
    }

    #[test]
    fn live_debug_resume_then_pause_cycle() {
        let mut s = fresh_session();
        s.pause_engine().unwrap();
        s.step_n_frames(3).unwrap();
        s.resume_engine().unwrap();
        assert!(!s.paused());
        s.pause_engine().unwrap();
        s.step_n_frames(2).unwrap();
        assert_eq!(s.current_frame(), 1005);
    }

    #[test]
    fn live_debug_record_inspect_requires_pause() {
        let mut s = fresh_session();
        let r = s.record_inspect("morton:0xFEED");
        assert!(matches!(r, Err(LiveDebugError::InvalidTransition { .. })));
        s.pause_engine().unwrap();
        s.record_inspect("morton:0xFEED").unwrap();
    }

    #[test]
    fn live_debug_record_hot_reload_paused_only() {
        let mut s = fresh_session();
        let r = s.record_hot_reload(ReloadId(1));
        assert!(matches!(r, Err(LiveDebugError::InvalidTransition { .. })));
        s.pause_engine().unwrap();
        s.record_hot_reload(ReloadId(2)).unwrap();
        let inspect = s.inspect_current().unwrap();
        assert_eq!(inspect.last_reload, Some(ReloadId(2)));
    }

    #[test]
    fn live_debug_trail_records_canonical_dance() {
        let mut s = fresh_session();
        s.pause_engine().unwrap();
        s.record_inspect("morton:0x1234").unwrap();
        s.step_n_frames(1).unwrap();
        s.tweak_value("k", TunableValue::Float(0.5)).unwrap();
        s.step_n_frames(1).unwrap();
        s.resume_engine().unwrap();
        // Pause + Inspected + Stepped + TunableSet + Stepped + Resumed = 6 entries.
        assert_eq!(s.trail().len(), 6);
        assert!(matches!(s.trail()[0], LiveDebugStep::Paused { .. }));
        assert!(matches!(s.trail()[5], LiveDebugStep::Resumed { .. }));
    }
}

//! § driver — scripted-GM session-driver.
//!
//! § ROLE
//!   Walks a `PlayTestSession` end-to-end against a sandboxed-engine
//!   abstraction (`SandboxedEngine` trait). On each turn it :
//!     1. Picks an intent deterministically from the seed-fed pool.
//!     2. Optionally consults the LLM-bridge for a decision.
//!     3. Steps the sandbox + records progress / crash / softlock /
//!        violation events into the [`crate::session::Trace`].
//!
//!   The bridge is dispatched in priority `Mode-C → Mode-B → Mode-A`
//!   with-fallback : Mode-C is always-available so the driver NEVER
//!   blocks on external resources.
//!
//! § DETERMINISM
//!   The driver uses an internal `splitmix64` PRNG (closed-form ; no
//!   external `rand` dep at the call-site). Replay with the same
//!   `agent_persona_seed` produces the same intent-sequence. The LLM
//!   is consulted at most once per turn ; in `SubstrateOnly` mode the
//!   reply is a stable template-string so determinism is preserved.

use cssl_host_llm_bridge::{LlmBridge, LlmMessage, LlmRole};
use thiserror::Error;

use crate::session::{PlayTestError, PlayTestSession, Trace, TraceEvent};

/// § Errors returned by the driver-loop.
#[derive(Debug, Error)]
pub enum DriveError {
    /// Pre-flight validation failed.
    #[error("session pre-flight invalid: {0}")]
    Invalid(#[from] PlayTestError),
    /// LLM-bridge surfaced an error mid-decision.
    #[error("bridge error: {0}")]
    Bridge(String),
}

/// § The minimal interface a sandboxed-engine must expose so the driver
/// can probe + step it. The host's real engine implements this against
/// its existing GM-narrator + DM-runtime ; tests use a `MockEngine`.
///
/// § SAFETY-INVARIANT
///   Implementations MUST be in-process + isolated : NO network ; NO
///   filesystem-writes that touch the author's content-store. The driver
///   relies on this for the sandbox-attestation.
pub trait GmDriver {
    /// § Step the engine forward by one intent ; return the
    /// engine-supplied `EngineStepResult` so the driver can transcribe
    /// it into trace-events.
    fn step(&mut self, turn: u32, intent: &str) -> EngineStepResult;
}

/// § Outcome of one engine-step (engine-supplied ; driver-recorded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStepResult {
    /// True if the world progressed this turn.
    pub progress: bool,
    /// Free-form progress-label (only meaningful when `progress = true`).
    pub progress_label: String,
    /// True if the engine crashed mid-step (recovered).
    pub crashed: bool,
    /// Crash-kind label (only meaningful when `crashed = true`).
    pub crash_kind: String,
    /// True if a sovereignty / PRIME-DIRECTIVE violation was flagged.
    pub sovereign_violation: Option<String>,
    /// True if a cosmetic-axiom (pay-for-power) path was reached.
    pub cosmetic_violation: Option<String>,
}

impl EngineStepResult {
    /// § Construct a clean-step result (no progress · no crash · no violation).
    #[must_use]
    pub fn idle() -> Self {
        Self {
            progress: false,
            progress_label: String::new(),
            crashed: false,
            crash_kind: String::new(),
            sovereign_violation: None,
            cosmetic_violation: None,
        }
    }
    /// § Construct a progress-step result.
    #[must_use]
    pub fn progress(label: impl Into<String>) -> Self {
        Self {
            progress: true,
            progress_label: label.into(),
            ..Self::idle()
        }
    }
}

/// § The default scripted-GM driver — picks intents from a fixed pool
/// using the session's `agent_persona_seed`. The pool is small + curated
/// to exercise NPC-talk · recipe-attempt · scene-explore · arc-trigger.
pub struct ScriptedGmDriver {
    /// Soft-lock window : N consecutive non-progress turns triggers a
    /// soft-lock event. Exposed as a field so tests can shrink it.
    pub softlock_window: u32,
}

impl Default for ScriptedGmDriver {
    fn default() -> Self {
        Self { softlock_window: 5 }
    }
}

const INTENT_POOL: &[&str] = &[
    "talk_npc",
    "attempt_recipe",
    "explore_scene",
    "trigger_arc_phase",
    "examine_object",
    "rest",
    "barter",
    "study_lore",
];

/// § Closed-form splitmix64 — no `rand` runtime dep, fully deterministic.
const fn splitmix64(state: u64) -> (u64, u64) {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let next_state = z;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (next_state, z)
}

/// § Drive a session end-to-end against the supplied engine + bridge.
///
/// § FALLBACK ORDER
///   The bridge is whatever the host hands in ; the driver does NOT
///   discriminate by mode. The host's `make_bridge` enforces the cap-gate
///   + Mode-C is always-on so this function works without external
///   resources. We treat the bridge purely as a black-box decision-source.
///
/// § PARAMETERS
///   - `session`  : the validated session config.
///   - `engine`   : sandboxed-engine implementing [`GmDriver`].
///   - `driver`   : the scripted-GM driver (intent-pool + softlock window).
///   - `bridge`   : optional LLM bridge. If `None`, the driver runs in
///     pure-script mode (no LLM consultation ; fastest + most-deterministic).
///
/// § RETURN
///   The accumulated [`Trace`]. The caller assembles the
///   [`crate::report::PlayTestReport`] by combining the trace + scoring.
pub fn drive_session<E: GmDriver>(
    session: &PlayTestSession,
    engine: &mut E,
    driver: &ScriptedGmDriver,
    bridge: Option<&dyn LlmBridge>,
) -> Result<Trace, DriveError> {
    session.validate()?;
    let mut trace = Trace::new();
    trace.push(TraceEvent::SessionStart {
        content_id: session.content_id,
        seed: session.agent_persona_seed,
    });

    let mut rng_state = session.agent_persona_seed;
    let mut consec_no_progress: u32 = 0;
    let mut last_softlock_turn: Option<u32> = None;

    for turn in 0..session.max_turns {
        // 1. pick intent deterministically
        let (next_state, rnd) = splitmix64(rng_state);
        rng_state = next_state;
        let intent_idx = (rnd as usize) % INTENT_POOL.len();
        let intent = INTENT_POOL[intent_idx];
        trace.push(TraceEvent::IntentInvoked {
            turn,
            intent: intent.to_string(),
        });

        // 2. optional LLM-decision (Mode-C/B/A). We only call once per
        //    turn ; the reply-hash is recorded so determinism can be
        //    asserted across replays.
        if let Some(b) = bridge {
            let prompt = vec![
                LlmMessage::new(LlmRole::System, "You are a scripted playtest GM."),
                LlmMessage::new(LlmRole::User, format!("intent={intent} turn={turn}")),
            ];
            match b.chat(&prompt) {
                Ok(reply) => {
                    let h = blake3::hash(reply.as_bytes());
                    let mut prefix = [0_u8; 8];
                    prefix.copy_from_slice(&h.as_bytes()[..8]);
                    trace.push(TraceEvent::LlmDecision {
                        turn,
                        reply_blake3_prefix: prefix,
                    });
                }
                Err(e) => return Err(DriveError::Bridge(format!("{e}"))),
            }
        }

        // 3. step the sandbox + transcribe
        let step = engine.step(turn, intent);
        if step.progress {
            trace.push(TraceEvent::Progress {
                turn,
                label: step.progress_label,
            });
            consec_no_progress = 0;
        } else {
            consec_no_progress += 1;
            if consec_no_progress >= driver.softlock_window {
                // Emit at most one softlock per stalled-stretch ; reset
                // counter on any subsequent progress (already handled).
                let already_emitted = matches!(last_softlock_turn, Some(t) if t == turn);
                if !already_emitted {
                    trace.push(TraceEvent::SoftLockDetected {
                        turn,
                        consecutive: consec_no_progress,
                    });
                    last_softlock_turn = Some(turn);
                }
            }
        }
        if step.crashed {
            trace.push(TraceEvent::CrashRecorded {
                turn,
                kind: step.crash_kind,
            });
        }
        if let Some(rule) = step.sovereign_violation {
            trace.push(TraceEvent::SovereignViolation { turn, rule });
        }
        if let Some(path) = step.cosmetic_violation {
            trace.push(TraceEvent::CosmeticAxiomViolation { turn, path });
        }
    }

    trace.push(TraceEvent::SessionEnd {
        turns_elapsed: session.max_turns,
    });
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::new_session;

    /// § Mock engine that always-progresses + never-crashes — used to
    /// validate the happy-path + scoring math.
    struct AlwaysProgress;
    impl GmDriver for AlwaysProgress {
        fn step(&mut self, _turn: u32, _intent: &str) -> EngineStepResult {
            EngineStepResult::progress("step")
        }
    }

    #[test]
    fn drive_runs_to_max_turns() {
        let mut session = new_session(1, 0xCAFE);
        session.max_turns = 10;
        let mut e = AlwaysProgress;
        let trace = drive_session(&session, &mut e, &ScriptedGmDriver::default(), None).unwrap();
        assert_eq!(trace.total_intents(), 10);
        assert_eq!(trace.total_progress(), 10);
        assert!(matches!(
            trace.events.last(),
            Some(TraceEvent::SessionEnd { turns_elapsed: 10 })
        ));
    }

    #[test]
    fn drive_with_same_seed_is_deterministic() {
        let mut session = new_session(1, 0xDEAD_BEEF);
        session.max_turns = 20;
        let mut e1 = AlwaysProgress;
        let mut e2 = AlwaysProgress;
        let t1 = drive_session(&session, &mut e1, &ScriptedGmDriver::default(), None).unwrap();
        let t2 = drive_session(&session, &mut e2, &ScriptedGmDriver::default(), None).unwrap();
        assert!(t1.is_deterministic_with(&t2));
    }
}

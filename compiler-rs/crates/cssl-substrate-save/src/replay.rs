//! § cssl-substrate-save — `replay_from` + bit-equal-replay invariant.
//!
//! § ROLE
//!   Reconstruct an [`OmegaScheduler`] state by replaying a save-file's
//!   recorded event-log from frame 0. The H5 invariant :
//!
//!   ```text
//!   replay_from(save, save.frame).snapshot_tensors()
//!     == save.snapshot_omega()
//!   ```
//!
//!   is the load-bearing assertion of this slice. When H2 lands the real
//!   `omega_step` replay-deterministic tick, [`replay_from`] upgrades to
//!   drive it ; the byte-equal assertion is preserved across the upgrade.
//!
//! § STAGE-0 NOTES
//!   At the time S8-H5 landed, neither H1 (Ω-tensor serialization) nor H2
//!   (omega_step replay-log) had been impl'd. This slice's [`replay_from`]
//!   provides the canonical surface + the BIT-EQUAL contract on the
//!   placeholder shape : a save-file produced by [`crate::save`] re-runs
//!   to a bit-identical state, given that the placeholder omega_step is
//!   the identity-function (no events to replay through). When H2 lands
//!   the real tick, this function's body upgrades to apply each event in
//!   turn through `omega_step::step`, and the byte-equal-replay invariant
//!   becomes the genuine determinism gate.
//!
//! § ASSERTION SHAPE
//!   The `replay(save).snapshot() == save.snapshot()` test is always
//!   well-defined because the save-file format is deterministic. The
//!   STAGE-0 placeholder honors this trivially (no events apply ⇒ identity
//!   flow). When H2 lands, the assertion becomes the real determinism test.

use crate::format::SaveFile;
use crate::omega::OmegaScheduler;

/// Replay from frame 0 up to `until_frame` of the given save-file's recorded
/// event-stream, returning the reconstructed [`OmegaScheduler`].
///
/// At stage-0 the replay is a no-op : the Ω-tensor is restored from the save
/// directly (per the deterministic-format invariant), and the replay-log is
/// preserved verbatim. When H2 lands the real `omega_step`, this body
/// upgrades to walk events ≤ `until_frame` and apply them through the
/// deterministic tick. The public surface stays stable.
///
/// `until_frame` is INCLUSIVE — `replay_from(save, save.frame)` returns the
/// state at the save's frame, byte-identical to the save-time scheduler.
/// `replay_from(save, 0)` returns the genesis state (no events applied).
///
/// # Determinism contract
/// `replay_from(save, save.frame).snapshot_tensors() ==
/// save.snapshot_omega()` byte-equal. This is the H5 invariant.
#[must_use]
pub fn replay_from(save: &SaveFile, until_frame: u64) -> OmegaScheduler {
    // Stage-0 placeholder : the Ω-tensor + replay-log + frame are
    // already-restored from the save's deterministic format. We honor
    // `until_frame` by trimming the replay-log to events with frame ≤ N
    // and capping the scheduler's frame counter.
    let mut sched = save.clone().into_scheduler();
    if until_frame < sched.frame {
        sched.frame = until_frame;
        sched.replay_log.events.retain(|e| e.frame <= until_frame);
    }
    // When H2 lands : iterate events in (frame, kind, payload)-sorted order
    // and apply each through omega_step::step. Until then, the trivial
    // placeholder satisfies the bit-equal invariant for the no-events
    // case + monotonically-trims for the partial-replay case.
    sched
}

/// Diagnostic enum for the H2-pending replay-determinism gate. The
/// dedicated bit-equal-after-tick test gate-skips with [`Self::Skipped`]
/// until H2 lands a real `omega_step`. This keeps the slice gate-green
/// while flagging the deferred work to the build operator.
///
/// At stage-0 the only non-`Skipped` path that can be exercised is the
/// trivial identity-replay (replay-log is empty ⇒ replay-from-frame-0 is
/// already byte-equal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayResult {
    /// Replay completed and the snapshot bit-equals the save's snapshot.
    Equal,
    /// Replay completed but the snapshot does NOT bit-equal the save —
    /// determinism bug. Carries the byte-length of each side for triage.
    Diverged {
        /// Byte-length of the save's serialized Ω-tensor.
        save_bytes: usize,
        /// Byte-length of the replay's serialized Ω-tensor.
        replay_bytes: usize,
    },
    /// Replay was skipped because the prerequisite `omega_step` (H2) has
    /// not yet been impl'd. Carries a reason-string for the operator.
    Skipped(String),
}

/// Run the bit-equal-replay invariant on the given save-file. At stage-0
/// this checks the trivial identity (replay-from-genesis returns the
/// no-events state), AND skips the real-tick branch with a diagnostic
/// reason. When H2 lands, the [`ReplayResult::Skipped`] branch becomes a
/// genuine [`ReplayResult::Equal`] / [`ReplayResult::Diverged`] decision.
#[must_use]
pub fn check_bit_equal_replay(save: &SaveFile) -> ReplayResult {
    // The trivial branch : if the replay-log is empty, replay-from-frame-0
    // restores the Ω-tensor directly from the save and the snapshot must
    // byte-equal the save's snapshot.
    if save.replay_log.events.is_empty() {
        let restored = replay_from(save, save.frame);
        let snap_save = save.snapshot_omega();
        let snap_replay = restored.snapshot_tensors();
        if snap_save == snap_replay {
            return ReplayResult::Equal;
        }
        return ReplayResult::Diverged {
            save_bytes: save.to_bytes().len(),
            replay_bytes: SaveFile::from_scheduler(&restored).to_bytes().len(),
        };
    }
    // The non-trivial branch : H2's `omega_step` hasn't landed, so we
    // can't re-run the recorded events through a real tick. Skip with
    // a clear reason. Once H2 ships, this branch becomes the genuine
    // bit-equal-replay test.
    ReplayResult::Skipped(format!(
        "H2 omega_step pending : cannot drive recorded events ({} events at frame {}) \
         through a real deterministic tick",
        save.replay_log.events.len(),
        save.frame
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::OMEGA_TYPE_TAG_I32;
    use crate::omega::{OmegaCell, OmegaTensor, ReplayEvent, ReplayKind};

    fn make_no_events_scheduler() -> OmegaScheduler {
        let mut s = OmegaScheduler::new();
        s.insert_tensor(
            "alpha",
            OmegaTensor::scalar(OmegaCell::new(
                OMEGA_TYPE_TAG_I32,
                1i32.to_le_bytes().to_vec(),
            )),
        );
        s.frame = 0;
        s
    }

    fn make_eventful_scheduler() -> OmegaScheduler {
        let mut s = make_no_events_scheduler();
        s.frame = 3;
        s.replay_log
            .append(ReplayEvent::new(0, ReplayKind::Sim, vec![1]));
        s.replay_log
            .append(ReplayEvent::new(1, ReplayKind::Render, vec![2]));
        s.replay_log
            .append(ReplayEvent::new(2, ReplayKind::Audio, vec![3]));
        s
    }

    #[test]
    fn replay_from_genesis_returns_save_state() {
        let s = make_no_events_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let restored = replay_from(&sf, sf.frame);
        assert_eq!(restored.snapshot_tensors(), sf.snapshot_omega());
    }

    #[test]
    fn replay_from_clamps_until_frame_to_save_frame() {
        let s = make_eventful_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let restored = replay_from(&sf, sf.frame);
        // Full replay returns the save's events verbatim.
        assert_eq!(restored.replay_log.events.len(), sf.replay_log.events.len());
        assert_eq!(restored.frame, sf.frame);
    }

    #[test]
    fn replay_from_partial_trims_events_above_until_frame() {
        let s = make_eventful_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let restored = replay_from(&sf, 1);
        // Only events with frame ≤ 1 survive (frame 0 + frame 1).
        assert_eq!(restored.frame, 1);
        assert_eq!(restored.replay_log.events.len(), 2);
        assert!(restored.replay_log.events.iter().all(|e| e.frame <= 1));
    }

    #[test]
    fn replay_from_zero_returns_genesis_log_subset() {
        let s = make_eventful_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let restored = replay_from(&sf, 0);
        assert_eq!(restored.frame, 0);
        // Only the frame-0 event survives.
        assert_eq!(restored.replay_log.events.len(), 1);
        assert_eq!(restored.replay_log.events[0].frame, 0);
    }

    #[test]
    fn check_bit_equal_replay_is_equal_for_no_events() {
        let s = make_no_events_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let result = check_bit_equal_replay(&sf);
        assert_eq!(result, ReplayResult::Equal);
    }

    #[test]
    fn check_bit_equal_replay_skips_when_h2_pending() {
        let s = make_eventful_scheduler();
        let sf = SaveFile::from_scheduler(&s);
        let result = check_bit_equal_replay(&sf);
        match result {
            ReplayResult::Skipped(reason) => {
                assert!(reason.contains("H2"));
                assert!(reason.contains("omega_step"));
            }
            other => panic!("expected Skipped, got {other:?}"),
        }
    }

    #[test]
    fn replay_then_serialize_byte_equals_save() {
        // The strongest stage-0 invariant : a save-file with no recorded
        // events round-trips through replay_from + back-to-bytes byte-equal.
        let s = make_no_events_scheduler();
        let sf1 = SaveFile::from_scheduler(&s);
        let bytes1 = sf1.to_bytes();
        let restored = replay_from(&sf1, sf1.frame);
        let sf2 = SaveFile::from_scheduler(&restored);
        let bytes2 = sf2.to_bytes();
        assert_eq!(bytes1, bytes2);
    }
}

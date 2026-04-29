//! Per-fn AD tape : record control-flow branches taken on the forward pass
//! so the reverse pass can replay them in reverse iteration / branch-order.
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § IMPLEMENTATION § per-op rules-table :
//!   - `If / Loop  ⇒  record branch-taken / iter-count on tape for bwd-replay`
//!   - `Match      ⇒  same as If (record arm-taken)`
//! § TAPE + CHECKPOINTING § "tape : linear-typed buffer scoped to bwd_diff
//!   call (iso-capability)".
//!
//! § SCOPE (T11-D140 / this commit)
//!   - [`BranchEvent`] : one tape-cell — either a scf.if arm-index, a scf.for /
//!     scf.while / scf.loop iter-count, or a scf.match arm-index.
//!   - [`BranchTape`] : per-fn ring-buffer of [`BranchEvent`] cells. Reverse-
//!     pass replay walks the tape from tail-to-head ; primal values from the
//!     forward checkpoint feed the bwd-body.
//!   - [`TapeReplay`] : cursor over a [`BranchTape`] that yields events in
//!     reverse iteration order (tail → head), validating shape on each pop.
//!
//!   Tape-storage at this slice is the abstract data-structure used by the
//!   AD walker to author the symbolic record / replay attributes attached to
//!   bwd-variant ops. Real on-device buffer allocation (iso-capability +
//!   thread-local / workgroup-shared / global-SSBO) is a downstream slice ;
//!   the structure here defines the per-fn-call shape so that buffer-alloc
//!   pass + reverse-replay JIT/codegen can plug in without re-wiring.
//!
//! § OVERFLOW DISCIPLINE
//!   The tape is bounded — a default capacity of 1024 events ([`DEFAULT_TAPE_CAP`])
//!   is enough for typical procedural-graphs with bounded recursion. Push-on-
//!   full-tape returns [`TapeError::Overflow`] rather than reallocating —
//!   recursive-fn AD must therefore declare an explicit fuel bound, matching
//!   `specs/05_AUTODIFF.csl § LIMITATIONS § "arbitrary-recursion : must be
//!   bounded ({NoRecurse} OR bounded-fuel param)"`.

use core::fmt;

/// Default tape capacity (events). Sized for typical procedural-graph + bounded-
/// recursion AD ; recursive fns at the cusp must lift this via [`BranchTape::with_capacity`].
pub const DEFAULT_TAPE_CAP: usize = 1024;

/// One tape-cell describing a control-flow event observed on the forward pass.
///
/// The reverse-pass walks the tape backwards : an `If { arm }` event tells the
/// bwd-replay which branch to enter (so adjoint-accumulation flows through the
/// same arm that the forward chose) ; a `For { iters }` event tells it how
/// many times to spin the loop-body backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchEvent {
    /// `scf.if` : `arm` ∈ {0, 1} (then=0, else=1). The bwd replay
    /// re-enters the same arm, with seeded adjoints, walking the arm's ops in
    /// reverse to accumulate adjoint-contributions.
    If { arm: u8 },
    /// `scf.match` (T11-D140 reuse of the If wire-protocol) : `arm` is the
    /// 0-indexed match-arm taken on the forward pass ; bwd re-enters the same
    /// arm. At the AD level, scf.match shares semantics with scf.if : both
    /// record-and-replay an arm-index (see `specs/05` § per-op rules-table :
    /// "Match ⇒ same as If (record arm-taken)").
    Match { arm: u32 },
    /// `scf.for` : `iters` is the iteration count observed on the forward
    /// pass. The bwd replay rewinds the loop counter from `iters - 1` down
    /// to 0, re-executing the loop-body's bwd-pass (the body is the per-
    /// iteration adjoint-accumulator).
    For { iters: u32 },
    /// `scf.while` : `iters` is the count of body-executions before the
    /// condition first returned false. Bwd replay rewinds in the same shape
    /// as `For`. The cond-eval itself is non-differentiable (boolean
    /// condition is `@NoDiff` per the spec) so we don't tape its ops.
    While { iters: u32 },
    /// `scf.loop` : structural-CFG loop ; same shape as scf.for at the tape
    /// level — `iters` is the observed iteration-count.
    Loop { iters: u32 },
}

impl BranchEvent {
    /// Canonical short name used in MIR attributes (e.g.,
    /// `"diff_branch_event"="if"`).
    #[must_use]
    pub const fn kind_str(self) -> &'static str {
        match self {
            Self::If { .. } => "if",
            Self::Match { .. } => "match",
            Self::For { .. } => "for",
            Self::While { .. } => "while",
            Self::Loop { .. } => "loop",
        }
    }

    /// Numeric payload — `arm` for if/match, `iters` for for/while/loop.
    #[must_use]
    pub const fn payload(self) -> u32 {
        match self {
            Self::If { arm } => arm as u32,
            Self::Match { arm } => arm,
            Self::For { iters } | Self::While { iters } | Self::Loop { iters } => iters,
        }
    }

    /// Render the event as a stable serialization for tape-attribute strings.
    /// The format is `"<kind>:<payload>"` so the reverse-replay attribute-walker
    /// can split on `':'` and dispatch on `kind_str`.
    #[must_use]
    pub fn encode(self) -> String {
        format!("{}:{}", self.kind_str(), self.payload())
    }

    /// Inverse of [`Self::encode`] — returns `None` when the input is malformed
    /// (unknown kind / non-numeric payload / missing separator).
    #[must_use]
    pub fn decode(s: &str) -> Option<Self> {
        let (kind, payload) = s.split_once(':')?;
        let n: u32 = payload.parse().ok()?;
        match kind {
            "if" => {
                if n > 1 {
                    return None;
                }
                Some(Self::If { arm: n as u8 })
            }
            "match" => Some(Self::Match { arm: n }),
            "for" => Some(Self::For { iters: n }),
            "while" => Some(Self::While { iters: n }),
            "loop" => Some(Self::Loop { iters: n }),
            _ => None,
        }
    }
}

impl fmt::Display for BranchEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

/// Errors that can occur during tape ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapeError {
    /// Push attempted on a full tape ; recursive AD must increase capacity or
    /// declare a tighter recursion fuel.
    Overflow,
    /// Pop attempted on an empty replay-cursor.
    Underflow,
    /// Pop returned an event-kind that didn't match the expected kind. The
    /// payload carries (expected, actual) for diagnostic output.
    KindMismatch {
        expected: &'static str,
        actual: &'static str,
    },
}

impl fmt::Display for TapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => f.write_str("tape overflow"),
            Self::Underflow => f.write_str("tape underflow"),
            Self::KindMismatch { expected, actual } => {
                write!(
                    f,
                    "tape kind mismatch : expected `{expected}` got `{actual}`"
                )
            }
        }
    }
}

impl std::error::Error for TapeError {}

/// Per-fn AD tape : ring-buffer of [`BranchEvent`] cells.
///
/// § INVARIANTS
///   - `events.len() ≤ capacity`.
///   - Every fwd-pass `record_*` call appends to `events` ; reverse replay
///     consumes events from the back via [`TapeReplay`].
///   - Capacity is fixed at construction time ; growing is intentionally not
///     supported (per spec : recursive AD must declare a bound).
#[derive(Debug, Clone)]
pub struct BranchTape {
    events: Vec<BranchEvent>,
    capacity: usize,
}

impl BranchTape {
    /// Empty tape with [`DEFAULT_TAPE_CAP`] capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_TAPE_CAP)
    }

    /// Empty tape with a custom capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Number of events on the tape.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// `true` iff the tape carries no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Tape capacity (max events).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// `true` iff the tape is at capacity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.events.len() >= self.capacity
    }

    /// Snapshot view of the tape contents (head → tail order).
    #[must_use]
    pub fn events(&self) -> &[BranchEvent] {
        &self.events
    }

    /// Generic event-push. Returns [`TapeError::Overflow`] when the tape is
    /// full ; the caller is expected to react by either widening the tape (
    /// `with_capacity`) or trimming the recursion fuel.
    ///
    /// # Errors
    /// Returns [`TapeError::Overflow`] when the tape is at capacity.
    pub fn record(&mut self, event: BranchEvent) -> Result<(), TapeError> {
        if self.is_full() {
            return Err(TapeError::Overflow);
        }
        self.events.push(event);
        Ok(())
    }

    /// Convenience : record a scf.if arm. `arm` ∈ {0, 1} ; arm > 1 is silently
    /// clamped to 1 (the else-arm) to keep the tape-cell shape valid.
    ///
    /// # Errors
    /// Returns [`TapeError::Overflow`] when the tape is at capacity.
    pub fn record_if(&mut self, arm: u8) -> Result<(), TapeError> {
        let arm = arm.min(1);
        self.record(BranchEvent::If { arm })
    }

    /// Convenience : record a scf.match arm-index.
    ///
    /// # Errors
    /// Returns [`TapeError::Overflow`] when the tape is at capacity.
    pub fn record_match(&mut self, arm: u32) -> Result<(), TapeError> {
        self.record(BranchEvent::Match { arm })
    }

    /// Convenience : record a scf.for iter-count.
    ///
    /// # Errors
    /// Returns [`TapeError::Overflow`] when the tape is at capacity.
    pub fn record_for(&mut self, iters: u32) -> Result<(), TapeError> {
        self.record(BranchEvent::For { iters })
    }

    /// Convenience : record a scf.while body-execution count.
    ///
    /// # Errors
    /// Returns [`TapeError::Overflow`] when the tape is at capacity.
    pub fn record_while(&mut self, iters: u32) -> Result<(), TapeError> {
        self.record(BranchEvent::While { iters })
    }

    /// Convenience : record a scf.loop iter-count.
    ///
    /// # Errors
    /// Returns [`TapeError::Overflow`] when the tape is at capacity.
    pub fn record_loop(&mut self, iters: u32) -> Result<(), TapeError> {
        self.record(BranchEvent::Loop { iters })
    }

    /// Build a reverse-replay cursor over this tape. The cursor pops from the
    /// tail (last-recorded event first) so the bwd-pass walks events in the
    /// reverse-of-record order — matching the structural reverse-walk of MIR
    /// ops in [`crate::substitute::substitute_bwd`].
    #[must_use]
    pub fn replay(&self) -> TapeReplay<'_> {
        TapeReplay {
            tape: self,
            cursor: self.events.len(),
        }
    }

    /// Reset the tape — drops all recorded events but keeps the allocated
    /// capacity. Used between successive bwd_diff calls on the same fn so
    /// the buffer can be reused without reallocation.
    pub fn reset(&mut self) {
        self.events.clear();
    }
}

impl Default for BranchTape {
    fn default() -> Self {
        Self::new()
    }
}

/// A cursor that pops events from a [`BranchTape`] in reverse-record order.
///
/// The reverse-mode AD walker uses [`TapeReplay`] in step with its own
/// reverse-iteration over MIR ops — when the walker hits a structured-control-
/// flow op (scf.if / scf.for / scf.while / scf.loop), it pops the matching
/// event from the replay cursor and dispatches the bwd-body accordingly.
#[derive(Debug)]
pub struct TapeReplay<'a> {
    tape: &'a BranchTape,
    cursor: usize,
}

impl TapeReplay<'_> {
    /// Number of events still pending replay (cursor position from head).
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.cursor
    }

    /// `true` iff no events remain.
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        self.cursor == 0
    }

    /// Pop the most-recently-recorded event (LIFO order). Returns
    /// [`TapeError::Underflow`] when the cursor has consumed the head.
    ///
    /// # Errors
    /// Returns [`TapeError::Underflow`] when the cursor is past the tape head.
    pub fn pop(&mut self) -> Result<BranchEvent, TapeError> {
        if self.cursor == 0 {
            return Err(TapeError::Underflow);
        }
        self.cursor -= 1;
        Ok(self.tape.events[self.cursor])
    }

    /// Pop the next event + assert it is an `If`. Returns the arm-index.
    ///
    /// # Errors
    /// Returns [`TapeError::Underflow`] or [`TapeError::KindMismatch`].
    pub fn pop_if(&mut self) -> Result<u8, TapeError> {
        match self.pop()? {
            BranchEvent::If { arm } => Ok(arm),
            other => Err(TapeError::KindMismatch {
                expected: "if",
                actual: other.kind_str(),
            }),
        }
    }

    /// Pop the next event + assert it is a `Match`. Returns the arm-index.
    ///
    /// # Errors
    /// Returns [`TapeError::Underflow`] or [`TapeError::KindMismatch`].
    pub fn pop_match(&mut self) -> Result<u32, TapeError> {
        match self.pop()? {
            BranchEvent::Match { arm } => Ok(arm),
            other => Err(TapeError::KindMismatch {
                expected: "match",
                actual: other.kind_str(),
            }),
        }
    }

    /// Pop the next event + assert it is a `For`. Returns the iter-count.
    ///
    /// # Errors
    /// Returns [`TapeError::Underflow`] or [`TapeError::KindMismatch`].
    pub fn pop_for(&mut self) -> Result<u32, TapeError> {
        match self.pop()? {
            BranchEvent::For { iters } => Ok(iters),
            other => Err(TapeError::KindMismatch {
                expected: "for",
                actual: other.kind_str(),
            }),
        }
    }

    /// Pop the next event + assert it is a `While`. Returns the iter-count.
    ///
    /// # Errors
    /// Returns [`TapeError::Underflow`] or [`TapeError::KindMismatch`].
    pub fn pop_while(&mut self) -> Result<u32, TapeError> {
        match self.pop()? {
            BranchEvent::While { iters } => Ok(iters),
            other => Err(TapeError::KindMismatch {
                expected: "while",
                actual: other.kind_str(),
            }),
        }
    }

    /// Pop the next event + assert it is a `Loop`. Returns the iter-count.
    ///
    /// # Errors
    /// Returns [`TapeError::Underflow`] or [`TapeError::KindMismatch`].
    pub fn pop_loop(&mut self) -> Result<u32, TapeError> {
        match self.pop()? {
            BranchEvent::Loop { iters } => Ok(iters),
            other => Err(TapeError::KindMismatch {
                expected: "loop",
                actual: other.kind_str(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BranchEvent, BranchTape, TapeError, DEFAULT_TAPE_CAP};

    #[test]
    fn default_capacity_is_1024() {
        let t = BranchTape::new();
        assert_eq!(t.capacity(), DEFAULT_TAPE_CAP);
        assert_eq!(t.capacity(), 1024);
    }

    #[test]
    fn empty_tape_replay_is_underflow() {
        let t = BranchTape::new();
        let mut replay = t.replay();
        assert!(replay.is_exhausted());
        assert_eq!(replay.pop(), Err(TapeError::Underflow));
    }

    #[test]
    fn record_if_then_replay_pops_lifo() {
        let mut t = BranchTape::new();
        t.record_if(0).unwrap(); // then-arm
        t.record_if(1).unwrap(); // else-arm
        t.record_for(7).unwrap();
        let mut replay = t.replay();
        assert_eq!(replay.remaining(), 3);
        // LIFO : last-recorded first.
        assert_eq!(replay.pop_for().unwrap(), 7);
        assert_eq!(replay.pop_if().unwrap(), 1);
        assert_eq!(replay.pop_if().unwrap(), 0);
        assert!(replay.is_exhausted());
    }

    #[test]
    fn record_match_with_arm_index() {
        let mut t = BranchTape::new();
        t.record_match(3).unwrap();
        let mut r = t.replay();
        assert_eq!(r.pop_match().unwrap(), 3);
    }

    #[test]
    fn record_for_iter_count() {
        let mut t = BranchTape::new();
        t.record_for(100).unwrap();
        assert_eq!(t.replay().pop_for().unwrap(), 100);
    }

    #[test]
    fn record_while_iter_count() {
        let mut t = BranchTape::new();
        t.record_while(42).unwrap();
        assert_eq!(t.replay().pop_while().unwrap(), 42);
    }

    #[test]
    fn record_loop_iter_count() {
        let mut t = BranchTape::new();
        t.record_loop(8).unwrap();
        assert_eq!(t.replay().pop_loop().unwrap(), 8);
    }

    #[test]
    fn record_to_full_returns_overflow() {
        let mut t = BranchTape::with_capacity(2);
        t.record_if(0).unwrap();
        t.record_if(1).unwrap();
        assert!(t.is_full());
        assert_eq!(t.record_if(0), Err(TapeError::Overflow));
    }

    #[test]
    fn recursive_call_tape_overflow_returns_explicit_error() {
        // Simulate a recursive-fn AD that exceeds the declared bound — the tape
        // must surface a typed error rather than silently growing.
        let mut t = BranchTape::with_capacity(4);
        for _ in 0..4 {
            t.record_for(1).unwrap();
        }
        assert!(t.is_full());
        // 5th recursion attempt overflows.
        assert!(t.record_for(1).is_err());
    }

    #[test]
    fn pop_kind_mismatch_returns_typed_error() {
        let mut t = BranchTape::new();
        t.record_for(3).unwrap();
        let mut r = t.replay();
        let err = r.pop_if().unwrap_err();
        match err {
            TapeError::KindMismatch { expected, actual } => {
                assert_eq!(expected, "if");
                assert_eq!(actual, "for");
            }
            other => panic!("expected KindMismatch, got {other:?}"),
        }
    }

    #[test]
    fn reset_clears_events_keeps_capacity() {
        let mut t = BranchTape::with_capacity(8);
        t.record_if(0).unwrap();
        t.record_if(1).unwrap();
        assert_eq!(t.len(), 2);
        t.reset();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
        assert_eq!(t.capacity(), 8);
        // Reset does not re-arm overflow — we can keep recording.
        t.record_if(1).unwrap();
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn arm_clamp_to_else_when_oob() {
        // The if-arm payload is logically u8 ∈ {0, 1} ; record_if clamps to 1.
        let mut t = BranchTape::new();
        t.record_if(255).unwrap();
        let mut r = t.replay();
        assert_eq!(r.pop_if().unwrap(), 1);
    }

    #[test]
    fn event_encode_round_trip() {
        for ev in [
            BranchEvent::If { arm: 0 },
            BranchEvent::If { arm: 1 },
            BranchEvent::Match { arm: 7 },
            BranchEvent::For { iters: 100 },
            BranchEvent::While { iters: 0 },
            BranchEvent::Loop { iters: 1 },
        ] {
            let s = ev.encode();
            let back = BranchEvent::decode(&s).unwrap();
            assert_eq!(ev, back);
        }
    }

    #[test]
    fn event_decode_rejects_malformed() {
        assert!(BranchEvent::decode("").is_none());
        assert!(BranchEvent::decode("if").is_none());
        assert!(BranchEvent::decode("if:notanumber").is_none());
        assert!(BranchEvent::decode("if:2").is_none()); // arm out of range
        assert!(BranchEvent::decode("unknown:0").is_none());
    }

    #[test]
    fn event_kind_str_canonical() {
        assert_eq!(BranchEvent::If { arm: 0 }.kind_str(), "if");
        assert_eq!(BranchEvent::Match { arm: 0 }.kind_str(), "match");
        assert_eq!(BranchEvent::For { iters: 0 }.kind_str(), "for");
        assert_eq!(BranchEvent::While { iters: 0 }.kind_str(), "while");
        assert_eq!(BranchEvent::Loop { iters: 0 }.kind_str(), "loop");
    }

    #[test]
    fn event_payload_extracts_arm_or_iters() {
        assert_eq!(BranchEvent::If { arm: 1 }.payload(), 1);
        assert_eq!(BranchEvent::Match { arm: 9 }.payload(), 9);
        assert_eq!(BranchEvent::For { iters: 42 }.payload(), 42);
        assert_eq!(BranchEvent::While { iters: 5 }.payload(), 5);
        assert_eq!(BranchEvent::Loop { iters: 0 }.payload(), 0);
    }

    #[test]
    fn replay_remaining_decreases_with_each_pop() {
        let mut t = BranchTape::new();
        t.record_if(0).unwrap();
        t.record_if(1).unwrap();
        t.record_if(0).unwrap();
        let mut r = t.replay();
        assert_eq!(r.remaining(), 3);
        r.pop().unwrap();
        assert_eq!(r.remaining(), 2);
        r.pop().unwrap();
        assert_eq!(r.remaining(), 1);
        r.pop().unwrap();
        assert!(r.is_exhausted());
    }

    #[test]
    fn nested_control_flow_tape_shape() {
        // Mimic a fwd-pass over a fn with shape :
        //   for i in 0..3 { if cond { ... } else { ... } }
        // The tape records : For{3}, If{x}, If{y}, If{z} (3 if-events).
        let mut t = BranchTape::new();
        t.record_for(3).unwrap();
        t.record_if(0).unwrap();
        t.record_if(1).unwrap();
        t.record_if(0).unwrap();
        let mut r = t.replay();
        // Reverse order : last if first, then for.
        assert_eq!(r.pop_if().unwrap(), 0);
        assert_eq!(r.pop_if().unwrap(), 1);
        assert_eq!(r.pop_if().unwrap(), 0);
        assert_eq!(r.pop_for().unwrap(), 3);
        assert!(r.is_exhausted());
    }

    #[test]
    fn tape_error_display_messages_contain_kind() {
        assert_eq!(format!("{}", TapeError::Overflow), "tape overflow");
        assert_eq!(format!("{}", TapeError::Underflow), "tape underflow");
        let mismatch = TapeError::KindMismatch {
            expected: "if",
            actual: "for",
        };
        assert!(format!("{mismatch}").contains("if"));
        assert!(format!("{mismatch}").contains("for"));
    }

    #[test]
    fn tape_clone_preserves_events() {
        let mut t = BranchTape::with_capacity(8);
        t.record_if(0).unwrap();
        t.record_for(5).unwrap();
        let copy = t.clone();
        assert_eq!(copy.len(), 2);
        assert_eq!(copy.events()[0], BranchEvent::If { arm: 0 });
        assert_eq!(copy.events()[1], BranchEvent::For { iters: 5 });
    }
}

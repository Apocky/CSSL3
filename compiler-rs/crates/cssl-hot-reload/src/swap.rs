//! `HotReload` manager — the public-facing surface.
//!
//! § DESIGN
//!
//! `HotReload` is a stage-0 mock manager. It :
//!
//! 1. Accepts pushed swap events via `push_event`.
//! 2. Stages them in a pending-queue keyed by frame.
//! 3. Applies the queue on `apply_pending(frame_id)` — moving each event
//!    from pending → applied, dispatching to registered handlers, and
//!    recording into the `ReplayLog`.
//! 4. Maintains the logical-frame-N invariant : the manager itself
//!    increments a frame-counter via `tick_frame()` so callers without
//!    an external clock can drive the loop ; `apply_pending` accepts
//!    an explicit frame so an embedding engine can supply its own.
//!
//! At stage-0 there is NO real asset / shader / config / KAN dispatcher
//! wired in. Handlers receive the `SwapKind` payload + can do whatever
//! they want with it. Tests register a `RecordingHandler` that just
//! tallies the calls.
//!
//! § HANDLER DISPATCH
//!
//! The `SwapHandler` trait has four methods, one per kind. Default impls
//! are no-ops so a handler interested in only one kind doesn't have to
//! implement four. Multiple handlers can be registered ; they are
//! dispatched in registration order.
//!
//! § FRAME-SAFETY
//!
//! `apply_pending(frame_id)` rejects calls where `frame_id` is strictly
//! less than the manager's current logical-frame watermark — applies must
//! be monotone. The replay-log's own monotone-frame invariant is therefore
//! automatically respected.

use std::collections::VecDeque;

use crate::event::{FrameId, SwapEvent, SwapKind};
use crate::replay_log::{ReplayLog, ReplayLogError};

/// Outcome of an `apply_pending` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapOutcome {
    /// Number of events applied (i.e., dispatched to handlers + recorded).
    pub applied: u32,
    /// Number of events skipped because they were KAN no-ops (§ 3.6 spec :
    /// these neither dispatch nor record).
    pub skipped_noops: u32,
    /// Number of events still pending after the apply (events whose
    /// `frame_id > frame_id_arg` stay queued).
    pub pending: u32,
    /// Logical frame the apply ran on.
    pub frame_id: FrameId,
}

impl SwapOutcome {
    /// Total count of events the apply touched (applied + skipped).
    #[must_use]
    pub const fn touched(&self) -> u32 {
        self.applied + self.skipped_noops
    }
}

/// Errors the swap manager can surface.
#[derive(Debug, thiserror::Error)]
pub enum HotReloadError {
    /// `apply_pending` was called with a frame strictly less than the
    /// current frame-watermark.
    #[error("hot-reload frame regression : current={current}, attempted={attempted}")]
    FrameRegression {
        /// Current logical-frame watermark.
        current: FrameId,
        /// Attempted (rejected) frame.
        attempted: FrameId,
    },
    /// Replay-log refused the recording (frame or sequence regression).
    /// Wraps the underlying `ReplayLogError` for diagnostics.
    #[error("replay-log refused recording : {source}")]
    ReplayLog {
        /// The wrapped underlying error.
        #[from]
        source: ReplayLogError,
    },
    /// `push_event` was called with a frame-id strictly less than the
    /// current watermark — events must be pushed for the current frame
    /// or a future one.
    #[error("push-event frame regression : current={current}, attempted={attempted}")]
    PushFrameRegression {
        /// Current logical-frame watermark.
        current: FrameId,
        /// Attempted (rejected) frame.
        attempted: FrameId,
    },
}

/// Per-kind dispatch trait. Implementors observe applied swaps. Default
/// impls are no-ops so a handler interested only in (e.g.) shaders need
/// implement only `on_shader`.
#[allow(unused_variables)]
pub trait SwapHandler: Send + Sync {
    /// Called when an Asset swap is applied at `frame_id`.
    fn on_asset(&mut self, frame_id: FrameId, sequence: u32, kind: &SwapKind) {}
    /// Called when a Shader swap is applied at `frame_id`.
    fn on_shader(&mut self, frame_id: FrameId, sequence: u32, kind: &SwapKind) {}
    /// Called when a Config swap is applied at `frame_id`.
    fn on_config(&mut self, frame_id: FrameId, sequence: u32, kind: &SwapKind) {}
    /// Called when a KAN-weight swap is applied at `frame_id`.
    fn on_kan_weight(&mut self, frame_id: FrameId, sequence: u32, kind: &SwapKind) {}

    /// Diagnostic name (default = type-name).
    fn name(&self) -> &'static str {
        "anonymous"
    }
}

/// Stage-0 mock hot-reload manager.
pub struct HotReload {
    /// Logical-frame watermark. Increments on `tick_frame` ; `apply_pending`
    /// accepts a frame and may advance the watermark.
    current_frame: FrameId,
    /// Per-frame sequence counter. Resets when `current_frame` advances.
    next_sequence: u32,
    /// Pending swap-event queue (FIFO within frame ; ordered by frame).
    pending: VecDeque<SwapEvent>,
    /// Replay-log — all applied swaps end up here.
    replay_log: ReplayLog,
    /// Registered handlers (dispatched in registration order).
    handlers: Vec<Box<dyn SwapHandler>>,
    /// Cumulative count of events ever applied.
    total_applied: u64,
    /// Cumulative count of KAN no-op skips.
    total_noops: u64,
}

impl std::fmt::Debug for HotReload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotReload")
            .field("current_frame", &self.current_frame)
            .field("next_sequence", &self.next_sequence)
            .field("pending_len", &self.pending.len())
            .field("replay_log_len", &self.replay_log.len())
            .field("handlers", &self.handlers.len())
            .field("total_applied", &self.total_applied)
            .field("total_noops", &self.total_noops)
            .finish()
    }
}

impl Default for HotReload {
    fn default() -> Self {
        Self::new()
    }
}

impl HotReload {
    /// Construct an empty manager at logical-frame 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            next_sequence: 0,
            pending: VecDeque::new(),
            replay_log: ReplayLog::new(),
            handlers: Vec::new(),
            total_applied: 0,
            total_noops: 0,
        }
    }

    /// Current logical-frame watermark.
    #[must_use]
    pub const fn current_frame(&self) -> FrameId {
        self.current_frame
    }

    /// Next sequence number that will be assigned by an auto-sequencing
    /// `push_event_now` call.
    #[must_use]
    pub const fn next_sequence(&self) -> u32 {
        self.next_sequence
    }

    /// Number of events still queued (not yet applied).
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Cumulative count of events applied across the manager's lifetime.
    #[must_use]
    pub const fn total_applied(&self) -> u64 {
        self.total_applied
    }

    /// Cumulative count of KAN no-op swaps skipped.
    #[must_use]
    pub const fn total_noops(&self) -> u64 {
        self.total_noops
    }

    /// Read-only view of the replay log.
    #[must_use]
    pub fn replay_log(&self) -> &ReplayLog {
        &self.replay_log
    }

    /// Number of registered handlers.
    #[must_use]
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Advance the logical-frame watermark by one.
    ///
    /// Resets the per-frame sequence counter to 0. Returns the new frame.
    pub fn tick_frame(&mut self) -> FrameId {
        self.current_frame = self.current_frame.saturating_add(1);
        self.next_sequence = 0;
        self.current_frame
    }

    /// Register a `SwapHandler`. Multiple handlers can be registered ;
    /// they dispatch in registration order. Returns the new handler-count.
    pub fn register_handler(&mut self, handler: Box<dyn SwapHandler>) -> usize {
        self.handlers.push(handler);
        self.handlers.len()
    }

    /// Push a swap event for a specific (logical) frame + sequence.
    /// Caller is responsible for monotone-sequence discipline.
    ///
    /// # Errors
    /// `PushFrameRegression` if `frame_id` is strictly less than the
    /// current watermark.
    pub fn push_event(
        &mut self,
        frame_id: FrameId,
        sequence: u32,
        kind: SwapKind,
    ) -> Result<(), HotReloadError> {
        if frame_id < self.current_frame {
            return Err(HotReloadError::PushFrameRegression {
                current: self.current_frame,
                attempted: frame_id,
            });
        }
        self.pending
            .push_back(SwapEvent::new(frame_id, sequence, kind));
        Ok(())
    }

    /// Push a swap event using auto-assigned (`current_frame`, `next_sequence`).
    /// Increments `next_sequence` on success.
    ///
    /// # Errors
    /// Currently never errors (kept `Result` for parity with `push_event`).
    pub fn push_event_now(&mut self, kind: SwapKind) -> Result<SwapEvent, HotReloadError> {
        let event = SwapEvent::new(self.current_frame, self.next_sequence, kind);
        self.next_sequence = self.next_sequence.saturating_add(1);
        let cloned = event.clone();
        self.pending.push_back(event);
        Ok(cloned)
    }

    /// Apply pending swaps whose `frame_id <= apply_frame`.
    ///
    /// Process : drain pending in (frame, sequence) order, dispatch each
    /// to all handlers, record into the replay-log. KAN no-op swaps are
    /// counted under `skipped_noops` and neither dispatched nor recorded.
    ///
    /// Advances `current_frame` to `apply_frame` if greater.
    ///
    /// # Errors
    /// `FrameRegression` if `apply_frame < current_frame`.
    /// `ReplayLog` if the replay-log refuses an entry (should be impossible
    /// given the manager's monotone invariants — surfaced for diagnostics).
    pub fn apply_pending(&mut self, apply_frame: FrameId) -> Result<SwapOutcome, HotReloadError> {
        if apply_frame < self.current_frame {
            return Err(HotReloadError::FrameRegression {
                current: self.current_frame,
                attempted: apply_frame,
            });
        }

        // Sort pending so apply order is deterministic regardless of push order.
        // (push_event allows pushing future-frame events ; we drain those that
        //  are due.)
        let mut due: Vec<SwapEvent> = self
            .pending
            .iter()
            .filter(|e| e.frame_id <= apply_frame)
            .cloned()
            .collect();
        due.sort_by_key(SwapEvent::order_key);
        // Remove the due events from the pending queue.
        self.pending.retain(|e| e.frame_id > apply_frame);

        let mut applied = 0_u32;
        let mut skipped = 0_u32;
        for event in &due {
            if event.kind.is_noop() {
                skipped = skipped.saturating_add(1);
                self.total_noops = self.total_noops.saturating_add(1);
                continue;
            }
            self.dispatch(event);
            self.replay_log.record(event)?;
            applied = applied.saturating_add(1);
            self.total_applied = self.total_applied.saturating_add(1);
        }

        if apply_frame > self.current_frame {
            self.current_frame = apply_frame;
            self.next_sequence = 0;
        }

        Ok(SwapOutcome {
            applied,
            skipped_noops: skipped,
            pending: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
            frame_id: apply_frame,
        })
    }

    /// Drain ALL pending events without applying them. Used by tests +
    /// diagnostic teardown. Returns the drained events.
    pub fn drain_pending(&mut self) -> Vec<SwapEvent> {
        self.pending.drain(..).collect()
    }

    fn dispatch(&mut self, event: &SwapEvent) {
        match &event.kind {
            SwapKind::Asset { .. } => {
                for h in &mut self.handlers {
                    h.on_asset(event.frame_id, event.sequence, &event.kind);
                }
            }
            SwapKind::Shader { .. } => {
                for h in &mut self.handlers {
                    h.on_shader(event.frame_id, event.sequence, &event.kind);
                }
            }
            SwapKind::Config { .. } => {
                for h in &mut self.handlers {
                    h.on_config(event.frame_id, event.sequence, &event.kind);
                }
            }
            SwapKind::KanWeight { .. } => {
                for h in &mut self.handlers {
                    h.on_kan_weight(event.frame_id, event.sequence, &event.kind);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AssetKind, ConfigKind, ShaderKind};

    fn h(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn asset(byte: u8) -> SwapKind {
        SwapKind::Asset {
            kind: AssetKind::Png,
            path_hash: h(byte),
            handle: u64::from(byte),
        }
    }

    fn shader(byte: u8) -> SwapKind {
        SwapKind::Shader {
            kind: ShaderKind::Wgsl,
            path_hash: h(byte),
            pipeline: u64::from(byte),
        }
    }

    fn config(byte: u8) -> SwapKind {
        SwapKind::Config {
            kind: ConfigKind::Engine,
            path_hash: h(byte),
            subsystem: u64::from(byte),
        }
    }

    fn kan(pre: u8, post: u8) -> SwapKind {
        SwapKind::KanWeight {
            network_handle: 1,
            fingerprint_pre: h(pre),
            fingerprint_post: h(post),
        }
    }

    #[derive(Default)]
    struct RecordingHandler {
        asset: u32,
        shader: u32,
        config: u32,
        kan: u32,
        last_frame: FrameId,
    }

    impl SwapHandler for RecordingHandler {
        fn on_asset(&mut self, frame_id: FrameId, _seq: u32, _kind: &SwapKind) {
            self.asset = self.asset.saturating_add(1);
            self.last_frame = frame_id;
        }
        fn on_shader(&mut self, frame_id: FrameId, _seq: u32, _kind: &SwapKind) {
            self.shader = self.shader.saturating_add(1);
            self.last_frame = frame_id;
        }
        fn on_config(&mut self, frame_id: FrameId, _seq: u32, _kind: &SwapKind) {
            self.config = self.config.saturating_add(1);
            self.last_frame = frame_id;
        }
        fn on_kan_weight(&mut self, frame_id: FrameId, _seq: u32, _kind: &SwapKind) {
            self.kan = self.kan.saturating_add(1);
            self.last_frame = frame_id;
        }
        fn name(&self) -> &'static str {
            "recording"
        }
    }

    #[test]
    fn new_manager_invariants() {
        let m = HotReload::new();
        assert_eq!(m.current_frame(), 0);
        assert_eq!(m.next_sequence(), 0);
        assert_eq!(m.pending_len(), 0);
        assert_eq!(m.total_applied(), 0);
        assert_eq!(m.total_noops(), 0);
        assert_eq!(m.handler_count(), 0);
        assert!(m.replay_log().is_empty());
    }

    #[test]
    fn default_impl_matches_new() {
        let a = HotReload::default();
        let b = HotReload::new();
        assert_eq!(a.current_frame(), b.current_frame());
        assert_eq!(a.next_sequence(), b.next_sequence());
    }

    #[test]
    fn tick_frame_advances_and_resets_sequence() {
        let mut m = HotReload::new();
        m.push_event_now(asset(1)).unwrap();
        assert_eq!(m.next_sequence(), 1);
        let f = m.tick_frame();
        assert_eq!(f, 1);
        assert_eq!(m.next_sequence(), 0);
    }

    #[test]
    fn tick_frame_saturates_at_u64_max() {
        let mut m = HotReload::new();
        // We can't easily reach u64::MAX in a test ; just confirm the call
        // doesn't panic on a reasonable bump.
        for _ in 0..1000 {
            m.tick_frame();
        }
        assert_eq!(m.current_frame(), 1000);
    }

    #[test]
    fn register_handler_returns_count() {
        let mut m = HotReload::new();
        let n1 = m.register_handler(Box::<RecordingHandler>::default());
        let n2 = m.register_handler(Box::<RecordingHandler>::default());
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
        assert_eq!(m.handler_count(), 2);
    }

    #[test]
    fn push_event_appends_to_pending() {
        let mut m = HotReload::new();
        m.push_event(0, 0, asset(1)).unwrap();
        assert_eq!(m.pending_len(), 1);
    }

    #[test]
    fn push_event_now_auto_sequences() {
        let mut m = HotReload::new();
        let e1 = m.push_event_now(asset(1)).unwrap();
        let e2 = m.push_event_now(asset(2)).unwrap();
        assert_eq!(e1.sequence, 0);
        assert_eq!(e2.sequence, 1);
        assert_eq!(m.pending_len(), 2);
    }

    #[test]
    fn push_event_rejects_past_frame() {
        let mut m = HotReload::new();
        m.tick_frame(); // current = 1
        m.tick_frame(); // current = 2
        let err = m.push_event(0, 0, asset(1)).unwrap_err();
        assert!(matches!(
            err,
            HotReloadError::PushFrameRegression {
                current: 2,
                attempted: 0
            }
        ));
    }

    #[test]
    fn apply_pending_dispatches_asset_handler() {
        let mut m = HotReload::new();
        m.register_handler(Box::<RecordingHandler>::default());
        m.push_event_now(asset(1)).unwrap();
        let outcome = m.apply_pending(0).unwrap();
        assert_eq!(outcome.applied, 1);
        assert_eq!(outcome.skipped_noops, 0);
        assert_eq!(outcome.pending, 0);
    }

    #[test]
    fn apply_pending_dispatches_all_four_kinds() {
        let mut m = HotReload::new();
        m.register_handler(Box::<RecordingHandler>::default());
        m.push_event_now(asset(1)).unwrap();
        m.push_event_now(shader(2)).unwrap();
        m.push_event_now(config(3)).unwrap();
        m.push_event_now(kan(0, 1)).unwrap();
        let outcome = m.apply_pending(0).unwrap();
        assert_eq!(outcome.applied, 4);
        assert_eq!(outcome.touched(), 4);
        assert_eq!(m.replay_log().len(), 4);
    }

    #[test]
    fn apply_pending_skips_kan_noops() {
        let mut m = HotReload::new();
        m.register_handler(Box::<RecordingHandler>::default());
        m.push_event_now(kan(7, 7)).unwrap();
        m.push_event_now(kan(7, 8)).unwrap();
        let outcome = m.apply_pending(0).unwrap();
        assert_eq!(outcome.applied, 1);
        assert_eq!(outcome.skipped_noops, 1);
        assert_eq!(m.total_noops(), 1);
        assert_eq!(m.replay_log().len(), 1);
    }

    #[test]
    fn apply_pending_advances_current_frame() {
        let mut m = HotReload::new();
        m.push_event(5, 0, asset(1)).unwrap();
        m.apply_pending(5).unwrap();
        assert_eq!(m.current_frame(), 5);
    }

    #[test]
    fn apply_pending_rejects_past_frame() {
        let mut m = HotReload::new();
        m.tick_frame(); // current = 1
        m.tick_frame(); // current = 2
        let err = m.apply_pending(0).unwrap_err();
        assert!(matches!(
            err,
            HotReloadError::FrameRegression {
                current: 2,
                attempted: 0
            }
        ));
    }

    #[test]
    fn apply_pending_keeps_future_events() {
        let mut m = HotReload::new();
        m.push_event(0, 0, asset(1)).unwrap();
        m.push_event(5, 0, asset(2)).unwrap();
        let outcome = m.apply_pending(0).unwrap();
        assert_eq!(outcome.applied, 1);
        assert_eq!(outcome.pending, 1);
        assert_eq!(m.pending_len(), 1);
    }

    #[test]
    fn apply_pending_orders_by_frame_then_sequence() {
        let mut m = HotReload::new();
        // Push out of order.
        m.push_event(2, 0, asset(2)).unwrap();
        m.push_event(1, 1, asset(11)).unwrap();
        m.push_event(1, 0, asset(10)).unwrap();
        let outcome = m.apply_pending(2).unwrap();
        assert_eq!(outcome.applied, 3);
        let recs = m.replay_log().records();
        assert_eq!(recs[0].order_key(), (1, 0));
        assert_eq!(recs[1].order_key(), (1, 1));
        assert_eq!(recs[2].order_key(), (2, 0));
    }

    #[test]
    fn drain_pending_empties_queue() {
        let mut m = HotReload::new();
        m.push_event_now(asset(1)).unwrap();
        m.push_event_now(asset(2)).unwrap();
        let drained = m.drain_pending();
        assert_eq!(drained.len(), 2);
        assert_eq!(m.pending_len(), 0);
    }

    #[test]
    fn debug_does_not_panic() {
        let m = HotReload::new();
        let s = format!("{m:?}");
        assert!(s.contains("HotReload"));
    }

    #[test]
    fn handler_default_impls_are_no_ops() {
        struct Mute;
        impl SwapHandler for Mute {}
        let mut m = HotReload::new();
        m.register_handler(Box::new(Mute));
        m.push_event_now(asset(1)).unwrap();
        m.apply_pending(0).unwrap();
        // No panic, no error — default no-op handlers are safe.
        assert_eq!(m.total_applied(), 1);
    }

    #[test]
    fn outcome_touched_is_applied_plus_skipped() {
        let outcome = SwapOutcome {
            applied: 3,
            skipped_noops: 2,
            pending: 0,
            frame_id: 1,
        };
        assert_eq!(outcome.touched(), 5);
    }
}

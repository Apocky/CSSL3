//! Integration tests for `cssl-hot-reload`.
//!
//! Coverage matrix :
//!   - 4 swap-kinds (Asset / Shader / Config / KanWeight)
//!   - push-event driver (the only stage-0 driver)
//!   - replay-log records each applied swap with logical-frame-N
//!   - handler dispatch (single + multi handler ; per-kind routing)
//!   - error cases : frame regression on push, frame regression on apply,
//!     replay-log monotone-frame guard, replay-log monotone-sequence guard
//!   - KAN no-op skip semantics
//!   - frame-boundary apply (events queued at frame N apply at frame N)
//!
//! The integration test count target is "60+" combined with unit tests ;
//! the per-module unit tests already provide ~48, this file adds another
//! 30+ for an integration-level safety net.

use cssl_hot_reload::event::{AssetKind, ConfigKind, FrameId, ShaderKind};
use cssl_hot_reload::{
    HotReload, HotReloadError, ReplayLog, ReplayLogError, SwapEvent, SwapHandler, SwapKind,
};

// ─────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────

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

fn asset_kind(kind: AssetKind, byte: u8) -> SwapKind {
    SwapKind::Asset {
        kind,
        path_hash: h(byte),
        handle: u64::from(byte),
    }
}

fn shader(kind: ShaderKind, byte: u8) -> SwapKind {
    SwapKind::Shader {
        kind,
        path_hash: h(byte),
        pipeline: u64::from(byte),
    }
}

fn config(kind: ConfigKind, byte: u8) -> SwapKind {
    SwapKind::Config {
        kind,
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
struct TallyHandler {
    asset: u32,
    shader: u32,
    config: u32,
    kan: u32,
    last_frame: FrameId,
    last_sequence: u32,
    log: Vec<(FrameId, u32, &'static str)>,
}

impl SwapHandler for TallyHandler {
    fn on_asset(&mut self, frame_id: FrameId, sequence: u32, _kind: &SwapKind) {
        self.asset = self.asset.saturating_add(1);
        self.last_frame = frame_id;
        self.last_sequence = sequence;
        self.log.push((frame_id, sequence, "asset"));
    }
    fn on_shader(&mut self, frame_id: FrameId, sequence: u32, _kind: &SwapKind) {
        self.shader = self.shader.saturating_add(1);
        self.last_frame = frame_id;
        self.last_sequence = sequence;
        self.log.push((frame_id, sequence, "shader"));
    }
    fn on_config(&mut self, frame_id: FrameId, sequence: u32, _kind: &SwapKind) {
        self.config = self.config.saturating_add(1);
        self.last_frame = frame_id;
        self.last_sequence = sequence;
        self.log.push((frame_id, sequence, "config"));
    }
    fn on_kan_weight(&mut self, frame_id: FrameId, sequence: u32, _kind: &SwapKind) {
        self.kan = self.kan.saturating_add(1);
        self.last_frame = frame_id;
        self.last_sequence = sequence;
        self.log.push((frame_id, sequence, "kan"));
    }
    fn name(&self) -> &'static str {
        "tally"
    }
}

#[derive(Default)]
struct ShaderOnlyHandler {
    count: u32,
}

impl SwapHandler for ShaderOnlyHandler {
    fn on_shader(&mut self, _frame_id: FrameId, _seq: u32, _kind: &SwapKind) {
        self.count = self.count.saturating_add(1);
    }
}

// ─────────────────────────────────────────────────────────────────────
// 1) 4-kind dispatch
// ─────────────────────────────────────────────────────────────────────

#[test]
fn dispatches_asset_swap() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(asset(1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
}

#[test]
fn dispatches_shader_swap() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(shader(ShaderKind::SpirV, 1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
}

#[test]
fn dispatches_config_swap() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(config(ConfigKind::Engine, 1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
}

#[test]
fn dispatches_kan_swap() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(kan(0, 1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
}

#[test]
fn dispatches_all_four_in_one_frame() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(asset(1)).unwrap();
    m.push_event_now(shader(ShaderKind::Wgsl, 2)).unwrap();
    m.push_event_now(config(ConfigKind::AiTunables, 3)).unwrap();
    m.push_event_now(kan(0, 1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 4);
    assert_eq!(m.replay_log().len(), 4);
}

// ─────────────────────────────────────────────────────────────────────
// 2) push_event = only stage-0 driver
// ─────────────────────────────────────────────────────────────────────

#[test]
fn push_event_appends_for_specified_frame() {
    let mut m = HotReload::new();
    m.push_event(0, 0, asset(1)).unwrap();
    m.push_event(0, 1, asset(2)).unwrap();
    m.push_event(1, 0, asset(3)).unwrap();
    assert_eq!(m.pending_len(), 3);
}

#[test]
fn push_event_rejects_strictly_past_frame() {
    let mut m = HotReload::new();
    m.tick_frame();
    m.tick_frame();
    let err = m.push_event(0, 0, asset(1)).unwrap_err();
    assert!(matches!(err, HotReloadError::PushFrameRegression { .. }));
}

#[test]
fn push_event_now_auto_assigns_current_frame() {
    let mut m = HotReload::new();
    m.tick_frame();
    let e = m.push_event_now(asset(1)).unwrap();
    assert_eq!(e.frame_id, 1);
    assert_eq!(e.sequence, 0);
}

#[test]
fn push_event_now_increments_sequence() {
    let mut m = HotReload::new();
    let e0 = m.push_event_now(asset(0)).unwrap();
    let e1 = m.push_event_now(asset(1)).unwrap();
    let e2 = m.push_event_now(asset(2)).unwrap();
    assert_eq!(e0.sequence, 0);
    assert_eq!(e1.sequence, 1);
    assert_eq!(e2.sequence, 2);
}

#[test]
fn no_implicit_event_pump() {
    // Stage-0 must never produce events on its own — only push_event drives.
    let mut m = HotReload::new();
    m.tick_frame();
    m.tick_frame();
    let out = m.apply_pending(2).unwrap();
    assert_eq!(out.applied, 0);
    assert!(m.replay_log().is_empty());
}

// ─────────────────────────────────────────────────────────────────────
// 3) Replay-log records each applied swap with logical-frame-N
// ─────────────────────────────────────────────────────────────────────

#[test]
fn replay_log_records_logical_frame_not_walltime() {
    let mut m = HotReload::new();
    m.push_event(7, 0, asset(1)).unwrap();
    m.apply_pending(7).unwrap();
    let r = &m.replay_log().records()[0];
    assert_eq!(r.frame_id, 7);
}

#[test]
fn replay_log_preserves_sequence_within_frame() {
    let mut m = HotReload::new();
    m.push_event(3, 0, asset(1)).unwrap();
    m.push_event(3, 2, asset(2)).unwrap();
    m.push_event(3, 5, asset(3)).unwrap();
    m.apply_pending(3).unwrap();
    let recs = m.replay_log().records();
    assert_eq!(recs[0].sequence, 0);
    assert_eq!(recs[1].sequence, 2);
    assert_eq!(recs[2].sequence, 5);
}

#[test]
fn replay_log_payload_byte_equal_to_originating_event() {
    let mut m = HotReload::new();
    let kind = config(ConfigKind::ReplayPolicy, 99);
    m.push_event_now(kind.clone()).unwrap();
    m.apply_pending(0).unwrap();
    let recorded = m.replay_log().records()[0].payload.clone();
    assert_eq!(recorded, kind);
}

#[test]
fn replay_log_excludes_kan_noops() {
    let mut m = HotReload::new();
    m.push_event_now(kan(7, 7)).unwrap();
    m.push_event_now(kan(7, 7)).unwrap();
    m.push_event_now(kan(7, 8)).unwrap();
    m.apply_pending(0).unwrap();
    assert_eq!(m.replay_log().len(), 1);
}

#[test]
fn replay_log_grows_monotonically_across_frames() {
    let mut m = HotReload::new();
    for f in 0..10_u64 {
        m.push_event(f, 0, asset(u8::try_from(f).unwrap_or(0)))
            .unwrap();
        m.apply_pending(f).unwrap();
    }
    let recs = m.replay_log().records();
    assert_eq!(recs.len(), 10);
    for (i, r) in recs.iter().enumerate() {
        assert_eq!(r.frame_id, u64::try_from(i).unwrap());
    }
}

#[test]
fn replay_log_directly_rejects_frame_regression() {
    let mut log = ReplayLog::new();
    let e1 = SwapEvent::new(5, 0, asset(0));
    let e2 = SwapEvent::new(3, 0, asset(0));
    log.record(&e1).unwrap();
    let err = log.record(&e2).unwrap_err();
    assert!(matches!(err, ReplayLogError::FrameRegression { .. }));
}

#[test]
fn replay_log_directly_rejects_sequence_regression() {
    let mut log = ReplayLog::new();
    let e1 = SwapEvent::new(5, 3, asset(0));
    let e2 = SwapEvent::new(5, 1, asset(0));
    log.record(&e1).unwrap();
    let err = log.record(&e2).unwrap_err();
    assert!(matches!(err, ReplayLogError::SequenceRegression { .. }));
}

#[test]
fn replay_log_records_filter_by_frame() {
    let mut log = ReplayLog::new();
    log.record(&SwapEvent::new(1, 0, asset(0))).unwrap();
    log.record(&SwapEvent::new(1, 1, asset(0))).unwrap();
    log.record(&SwapEvent::new(2, 0, asset(0))).unwrap();
    assert_eq!(log.records_in_frame(1).len(), 2);
    assert_eq!(log.records_in_frame(2).len(), 1);
}

#[test]
fn replay_log_records_filter_by_tag() {
    let mut log = ReplayLog::new();
    log.record(&SwapEvent::new(0, 0, asset(0))).unwrap();
    log.record(&SwapEvent::new(1, 0, shader(ShaderKind::SpirV, 0)))
        .unwrap();
    log.record(&SwapEvent::new(2, 0, config(ConfigKind::Engine, 0)))
        .unwrap();
    log.record(&SwapEvent::new(3, 0, kan(0, 1))).unwrap();
    assert_eq!(log.records_by_tag("asset").len(), 1);
    assert_eq!(log.records_by_tag("shader").len(), 1);
    assert_eq!(log.records_by_tag("config").len(), 1);
    assert_eq!(log.records_by_tag("kan-weight").len(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// 4) Handler dispatch
// ─────────────────────────────────────────────────────────────────────

#[test]
fn handler_receives_correct_kind() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(asset(1)).unwrap();
    m.push_event_now(asset(2)).unwrap();
    m.push_event_now(shader(ShaderKind::Wgsl, 3)).unwrap();
    m.apply_pending(0).unwrap();
    // We can't pull TallyHandler back out of Box<dyn ...> without unsafe ;
    // we verify via replay-log instead, which mirrors handler-dispatch.
    let asset_recs = m
        .replay_log()
        .records()
        .iter()
        .filter(|r| r.payload.tag() == "asset")
        .count();
    let shader_recs = m
        .replay_log()
        .records()
        .iter()
        .filter(|r| r.payload.tag() == "shader")
        .count();
    assert_eq!(asset_recs, 2);
    assert_eq!(shader_recs, 1);
}

#[test]
fn multiple_handlers_dispatched_in_order() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.register_handler(Box::<TallyHandler>::default());
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(asset(1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    // 1 event, 3 handlers — manager counts events not handler-calls.
    assert_eq!(out.applied, 1);
    assert_eq!(m.handler_count(), 3);
}

#[test]
fn handler_with_only_one_kind_implemented_compiles_and_runs() {
    let mut m = HotReload::new();
    m.register_handler(Box::<ShaderOnlyHandler>::default());
    // Push other kinds — they should NOT panic even though ShaderOnly only
    // overrides on_shader. The default impls are no-ops.
    m.push_event_now(asset(1)).unwrap();
    m.push_event_now(config(ConfigKind::Engine, 2)).unwrap();
    m.push_event_now(shader(ShaderKind::Dxil, 3)).unwrap();
    m.push_event_now(kan(0, 1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 4);
}

#[test]
fn no_handlers_still_records_replay_log() {
    let mut m = HotReload::new();
    m.push_event_now(asset(1)).unwrap();
    m.apply_pending(0).unwrap();
    assert_eq!(m.replay_log().len(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// 5) Error cases
// ─────────────────────────────────────────────────────────────────────

#[test]
fn apply_pending_rejects_past_frame_arg() {
    let mut m = HotReload::new();
    m.tick_frame();
    m.tick_frame();
    let err = m.apply_pending(0).unwrap_err();
    assert!(matches!(err, HotReloadError::FrameRegression { .. }));
}

#[test]
fn push_event_with_past_frame_returns_specific_error() {
    let mut m = HotReload::new();
    m.tick_frame();
    let err = m.push_event(0, 0, asset(1)).unwrap_err();
    match err {
        HotReloadError::PushFrameRegression { current, attempted } => {
            assert_eq!(current, 1);
            assert_eq!(attempted, 0);
        }
        _ => panic!("expected PushFrameRegression"),
    }
}

#[test]
fn apply_pending_with_past_frame_returns_specific_error() {
    let mut m = HotReload::new();
    for _ in 0..3 {
        m.tick_frame();
    }
    let err = m.apply_pending(1).unwrap_err();
    match err {
        HotReloadError::FrameRegression { current, attempted } => {
            assert_eq!(current, 3);
            assert_eq!(attempted, 1);
        }
        _ => panic!("expected FrameRegression"),
    }
}

#[test]
fn replay_log_drain_resets_watermark() {
    let mut log = ReplayLog::new();
    log.record(&SwapEvent::new(10, 0, asset(0))).unwrap();
    let drained = log.drain();
    assert_eq!(drained.len(), 1);
    // After drain, log accepts any frame again.
    log.record(&SwapEvent::new(1, 0, asset(0))).unwrap();
    assert_eq!(log.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────
// 6) KAN no-op semantics
// ─────────────────────────────────────────────────────────────────────

#[test]
fn kan_noop_counted_separately_from_applied() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(kan(0, 0)).unwrap();
    m.push_event_now(kan(1, 1)).unwrap();
    m.push_event_now(kan(2, 3)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
    assert_eq!(out.skipped_noops, 2);
    assert_eq!(out.touched(), 3);
    assert_eq!(m.total_noops(), 2);
}

#[test]
fn kan_noop_does_not_invoke_handler_dispatch() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(kan(7, 7)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 0);
    assert_eq!(out.skipped_noops, 1);
    // Replay log must NOT have a record for the no-op.
    assert!(m.replay_log().is_empty());
}

// ─────────────────────────────────────────────────────────────────────
// 7) Frame-boundary apply
// ─────────────────────────────────────────────────────────────────────

#[test]
fn events_pushed_for_future_frame_stay_pending() {
    let mut m = HotReload::new();
    m.push_event(5, 0, asset(1)).unwrap();
    m.push_event(5, 1, asset(2)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 0);
    assert_eq!(out.pending, 2);
    let out = m.apply_pending(5).unwrap();
    assert_eq!(out.applied, 2);
    assert_eq!(out.pending, 0);
}

#[test]
fn apply_pending_drains_only_due_events() {
    let mut m = HotReload::new();
    m.push_event(1, 0, asset(1)).unwrap();
    m.push_event(2, 0, asset(2)).unwrap();
    m.push_event(3, 0, asset(3)).unwrap();
    let out = m.apply_pending(2).unwrap();
    assert_eq!(out.applied, 2);
    assert_eq!(out.pending, 1);
    assert_eq!(m.pending_len(), 1);
}

#[test]
fn apply_at_same_frame_repeatedly_drains_each_call() {
    let mut m = HotReload::new();
    m.push_event(0, 0, asset(1)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
    // Re-apply at same frame — nothing to drain.
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 0);
    // Push more for same frame, apply again.
    m.push_event(0, 1, asset(2)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 1);
}

#[test]
fn apply_advances_current_frame_then_apply_resets_sequence() {
    let mut m = HotReload::new();
    m.push_event(5, 0, asset(1)).unwrap();
    m.apply_pending(5).unwrap();
    assert_eq!(m.current_frame(), 5);
    assert_eq!(m.next_sequence(), 0);
}

// ─────────────────────────────────────────────────────────────────────
// 8) Mixed-kind ordering
// ─────────────────────────────────────────────────────────────────────

#[test]
fn pushed_out_of_order_apply_in_order() {
    let mut m = HotReload::new();
    m.push_event(3, 0, asset(3)).unwrap();
    m.push_event(1, 1, shader(ShaderKind::Wgsl, 11)).unwrap();
    m.push_event(2, 0, config(ConfigKind::Engine, 2)).unwrap();
    m.push_event(1, 0, asset(10)).unwrap();
    m.apply_pending(3).unwrap();
    let recs = m.replay_log().records();
    assert_eq!(recs.len(), 4);
    assert_eq!(recs[0].order_key(), (1, 0));
    assert_eq!(recs[1].order_key(), (1, 1));
    assert_eq!(recs[2].order_key(), (2, 0));
    assert_eq!(recs[3].order_key(), (3, 0));
}

#[test]
fn each_asset_kind_routes_through_pump() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(asset_kind(AssetKind::Png, 1)).unwrap();
    m.push_event_now(asset_kind(AssetKind::Gltf, 2)).unwrap();
    m.push_event_now(asset_kind(AssetKind::Wav, 3)).unwrap();
    m.push_event_now(asset_kind(AssetKind::Ttf, 4)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 4);
    assert_eq!(
        m.replay_log()
            .records()
            .iter()
            .filter(|r| r.payload.tag() == "asset")
            .count(),
        4
    );
}

#[test]
fn each_shader_kind_routes_through_pump() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(shader(ShaderKind::SpirV, 1)).unwrap();
    m.push_event_now(shader(ShaderKind::Dxil, 2)).unwrap();
    m.push_event_now(shader(ShaderKind::Msl, 3)).unwrap();
    m.push_event_now(shader(ShaderKind::Wgsl, 4)).unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 4);
}

#[test]
fn each_config_kind_routes_through_pump() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    m.push_event_now(config(ConfigKind::Engine, 1)).unwrap();
    m.push_event_now(config(ConfigKind::RenderTunables, 2))
        .unwrap();
    m.push_event_now(config(ConfigKind::AiTunables, 3)).unwrap();
    m.push_event_now(config(ConfigKind::PhysicsTunables, 4))
        .unwrap();
    m.push_event_now(config(ConfigKind::AudioTunables, 5))
        .unwrap();
    m.push_event_now(config(ConfigKind::CapBudget, 6)).unwrap();
    m.push_event_now(config(ConfigKind::ReplayPolicy, 7))
        .unwrap();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 7);
}

// ─────────────────────────────────────────────────────────────────────
// 9) Invariants & accumulators
// ─────────────────────────────────────────────────────────────────────

#[test]
fn total_applied_is_cumulative_across_frames() {
    let mut m = HotReload::new();
    for f in 0..5_u64 {
        m.push_event(f, 0, asset(0)).unwrap();
        m.apply_pending(f).unwrap();
    }
    assert_eq!(m.total_applied(), 5);
}

#[test]
fn total_noops_is_cumulative_across_frames() {
    let mut m = HotReload::new();
    for f in 0..5_u64 {
        m.push_event(f, 0, kan(0, 0)).unwrap();
        m.apply_pending(f).unwrap();
    }
    assert_eq!(m.total_noops(), 5);
    assert_eq!(m.total_applied(), 0);
    assert!(m.replay_log().is_empty());
}

#[test]
fn drain_pending_does_not_record_in_replay_log() {
    let mut m = HotReload::new();
    m.push_event_now(asset(1)).unwrap();
    m.push_event_now(asset(2)).unwrap();
    m.drain_pending();
    assert!(m.replay_log().is_empty());
    assert_eq!(m.total_applied(), 0);
}

#[test]
fn manager_can_be_reused_after_apply() {
    let mut m = HotReload::new();
    m.push_event_now(asset(1)).unwrap();
    m.apply_pending(0).unwrap();
    m.tick_frame();
    m.push_event_now(shader(ShaderKind::Wgsl, 2)).unwrap();
    m.apply_pending(1).unwrap();
    assert_eq!(m.total_applied(), 2);
    assert_eq!(m.replay_log().len(), 2);
}

#[test]
fn replay_log_view_is_immutable() {
    let mut m = HotReload::new();
    m.push_event_now(asset(1)).unwrap();
    m.apply_pending(0).unwrap();
    let view: &[_] = m.replay_log().records();
    assert_eq!(view.len(), 1);
    // The view's lifetime is bound to &m so we cannot mutate via it ;
    // this test is mostly compile-time enforcement.
}

// ─────────────────────────────────────────────────────────────────────
// 10) End-to-end record/replay parity
// ─────────────────────────────────────────────────────────────────────

#[test]
fn replay_log_records_match_originating_events_byte_equal() {
    let mut m = HotReload::new();
    m.register_handler(Box::<TallyHandler>::default());
    let kinds = [
        asset(11),
        shader(ShaderKind::SpirV, 22),
        config(ConfigKind::Engine, 33),
        kan(0, 99),
        asset(44),
    ];
    let mut originals = Vec::new();
    for k in &kinds {
        let e = m.push_event_now(k.clone()).unwrap();
        originals.push(e);
    }
    m.apply_pending(0).unwrap();
    let recs = m.replay_log().records();
    assert_eq!(recs.len(), 5);
    for (orig, rec) in originals.iter().zip(recs.iter()) {
        assert_eq!(orig.frame_id, rec.frame_id);
        assert_eq!(orig.sequence, rec.sequence);
        assert_eq!(orig.kind, rec.payload);
    }
}

#[test]
fn replay_log_clone_produces_equal_log() {
    let mut m = HotReload::new();
    m.push_event_now(asset(1)).unwrap();
    m.push_event_now(shader(ShaderKind::Wgsl, 2)).unwrap();
    m.apply_pending(0).unwrap();
    let cloned = m.replay_log().clone();
    assert_eq!(cloned.records().len(), 2);
    assert_eq!(cloned.records(), m.replay_log().records());
}

#[test]
fn empty_apply_returns_zero_outcome() {
    let mut m = HotReload::new();
    let out = m.apply_pending(0).unwrap();
    assert_eq!(out.applied, 0);
    assert_eq!(out.skipped_noops, 0);
    assert_eq!(out.pending, 0);
    assert_eq!(out.frame_id, 0);
}

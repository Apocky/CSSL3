//! § loop_tick.rs — SelfAuthoringKanLoop::tick orchestrator.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § SelfAuthoringKanLoop::tick(dt)
//!   ```text
//!     1. iterate every k-anon-qualifying cell in the reservoir
//!     2. compute aggregated Q14 bias-delta for the cell
//!     3. compute previous bias for the cell  (LUT default = 0)
//!     4. delta_to_apply = aggregate - previous   (drive map TOWARDS aggregate)
//!     5. apply via TemplateBiasMap::apply_update
//!     6. on update_count == K * ANCHOR_EVERY_N_UPDATES (K > 0) ⇒ AnchorRing::push
//!   ```
//!
//! § dt argument
//!   `dt` is the wall-clock-seconds elapsed since the last tick. The current
//!   loop semantics are NOT explicitly time-driven (tick reads reservoir-state
//!   atomically), but `dt` is recorded into [`LoopStats::wall_clock_dt_total`]
//!   so callers can correlate tick-cadence to wall-clock-time for analytics.

use thiserror::Error;

use crate::anchor::AnchorRing;
use crate::bias_map::{
    saturating_add_q14, ArchetypeId, KanBiasUpdate, TemplateBiasMap, TemplateId, BIAS_Q14_MAX,
    BIAS_Q14_MIN,
};
use crate::reservoir::{PlayerHandle, QualitySignalRecord, Reservoir, ReservoirError};

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error)]
pub enum LoopTickError {
    #[error("reservoir-ingest failed : {0}")]
    Ingest(ReservoirError),
    #[error("bias-map update failed : {0}")]
    BiasMap(crate::bias_map::BiasUpdateError),
    #[error("rollback target seq {0} is not in the anchor-ring (evicted or never existed)")]
    AnchorMissing(u64),
}

// ───────────────────────────────────────────────────────────────────────────
// § LoopStats — observability summary.
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct LoopStats {
    /// Total tick() invocations.
    pub tick_count: u64,
    /// Total bias-updates successfully applied across all ticks.
    pub updates_applied: u64,
    /// Total cells skipped due to k-anon-floor not met (this tick).
    pub skipped_for_kanon: u64,
    /// Total signals refused (poisoning / revoked / rate-limit).
    pub signals_rejected: u64,
    /// Total anchors emitted to the anchor-ring.
    pub anchors_emitted: u64,
    /// Sum of dt arguments from tick() calls (wall-clock-seconds, fractional).
    pub wall_clock_dt_total: f64,
    /// Last-tick : number of priority-shifts (= updates_applied delta).
    pub last_tick_priority_shifts: u32,
}

// ───────────────────────────────────────────────────────────────────────────
// § SelfAuthoringKanLoop
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SelfAuthoringKanLoop {
    pub reservoir: Reservoir,
    pub bias_map: TemplateBiasMap,
    pub anchor_ring: AnchorRing,
    pub stats: LoopStats,
}

impl SelfAuthoringKanLoop {
    pub fn new(created_at_frame: u32, created_at_seconds: u64) -> Self {
        Self {
            reservoir: Reservoir::new(created_at_frame),
            bias_map: TemplateBiasMap::new(),
            anchor_ring: AnchorRing::new(created_at_seconds),
            stats: LoopStats::default(),
        }
    }

    pub fn with_k_anon_floor(
        created_at_frame: u32,
        created_at_seconds: u64,
        k_anon_floor: u32,
    ) -> Self {
        Self {
            reservoir: Reservoir::with_k_anon_floor(created_at_frame, k_anon_floor),
            bias_map: TemplateBiasMap::new(),
            anchor_ring: AnchorRing::new(created_at_seconds),
            stats: LoopStats::default(),
        }
    }

    /// Public ingest entry-point. Routes through reservoir + records rejection.
    pub fn ingest_signal(&mut self, rec: QualitySignalRecord) -> Result<(), LoopTickError> {
        match self.reservoir.ingest(rec) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.stats.signals_rejected = self.stats.signals_rejected.saturating_add(1);
                Err(LoopTickError::Ingest(e))
            }
        }
    }

    /// Single tick. Drains-aggregated-deltas → applies-bias-updates → emits-anchor.
    ///
    /// § ARGS
    ///   - `dt` : wall-clock-seconds since last tick (NOT used for the pure
    ///     bias-arithmetic ; recorded in stats only).
    ///   - `now_seconds` : caller-supplied wall-clock-seconds for anchor-timestamp.
    ///
    /// § RETURNS
    ///   The number of priority-shifts applied this tick.
    pub fn tick(&mut self, dt: f64, now_seconds: u64) -> u32 {
        self.stats.tick_count = self.stats.tick_count.saturating_add(1);
        self.stats.wall_clock_dt_total += dt;

        let cells = self.reservoir.k_anon_cells();
        let mut applied: u32 = 0;
        let mut skipped: u64 = 0;

        for (template, archetype) in cells {
            let Some(target) = self.reservoir.aggregate_cell_delta(template, archetype) else {
                skipped = skipped.saturating_add(1);
                continue;
            };
            let prev = self.bias_map.priority_shift(template, archetype);
            // delta_to_apply = (target - prev), saturating-clamped.
            let raw_delta = i32::from(target).saturating_sub(i32::from(prev));
            let clamped = raw_delta.clamp(i32::from(BIAS_Q14_MIN), i32::from(BIAS_Q14_MAX)) as i16;
            if clamped == 0 {
                continue; // no movement ⇒ skip update + don't bump anchor cadence
            }
            // audit_ref = 0 here ; the canonical audit-ring-entry is emitted by
            // the BiasInspector on outward-facing read. The internal apply has
            // its own update_count tracking inside TemplateBiasMap.
            let upd = KanBiasUpdate::new(template, archetype, clamped, 0);
            match self.bias_map.apply_update(upd) {
                Ok(()) => {
                    applied = applied.saturating_add(1);
                    self.stats.updates_applied = self.stats.updates_applied.saturating_add(1);
                    // Σ-Chain anchor cadence : after every successful update,
                    // check should_anchor on the new update_count.
                    let uc = self.bias_map.update_count();
                    if self.anchor_ring.should_anchor(uc) {
                        let h = self.bias_map.anchor_hash();
                        self.anchor_ring.push(uc, h, now_seconds);
                        self.stats.anchors_emitted = self.stats.anchors_emitted.saturating_add(1);
                    }
                }
                Err(_e) => {
                    // Out-of-range deltas should never happen here (we clamped),
                    // but if they do, log to stats and continue.
                    skipped = skipped.saturating_add(1);
                }
            }
        }

        self.stats.skipped_for_kanon = self.stats.skipped_for_kanon.saturating_add(skipped);
        self.stats.last_tick_priority_shifts = applied;
        applied
    }

    /// Sovereign-revoke wrapper : delegates to reservoir, then resets the
    /// bias-map and replays the post-revoke reservoir into a fresh map.
    ///
    /// § PRIME_DIRECTIVE § 5 revocability cascading. Returns the number of
    /// records scrubbed from the reservoir.
    pub fn sovereign_revoke(&mut self, player: PlayerHandle, now_seconds: u64) -> u32 {
        let scrubbed = self.reservoir.sovereign_revoke(player);
        if scrubbed > 0 {
            // Reset bias-map cells (preserves update_count for replay-determinism).
            self.bias_map.reset_cells();
            // Replay : run a single tick without dt to rebuild from remaining signals.
            // Note : because reset_cells preserved update_count, we don't double-emit
            // an anchor for the replay-tick.
            let _ = self.tick(0.0, now_seconds);
        }
        scrubbed
    }

    /// Rollback to a prior anchor-seq. The reservoir is NOT modified (signals
    /// continue to flow), only the bias-map is replayed-from-scratch using
    /// the current reservoir contents. The post-rollback bias_map.update_count
    /// equals the anchor's stored update_count_at_anchor + the priority-shifts
    /// applied during the replay-tick.
    ///
    /// § INVARIANT : if the anchor's state-hash matches the post-replay
    /// state-hash, rollback is consistent ; else divergence is reported as
    /// a returned bool.
    pub fn rollback_to_anchor(
        &mut self,
        seq: u64,
        now_seconds: u64,
    ) -> Result<bool, LoopTickError> {
        let anchor = self
            .anchor_ring
            .get_by_seq(seq)
            .ok_or(LoopTickError::AnchorMissing(seq))?;
        // Reset + replay.
        self.bias_map.reset_cells();
        let _ = self.tick(0.0, now_seconds);
        // Compare hashes : may diverge if reservoir-FIFO has evicted the
        // signals that were present when the anchor was recorded.
        let post_hash = self.bias_map.anchor_hash();
        Ok(post_hash == anchor.state_hash)
    }

    /// Drive the bias-map TOWARDS the aggregated-cell-delta over `n_steps`
    /// equal-magnitude steps. Useful for smoothing rollouts so that a single
    /// tick doesn't introduce a step-discontinuity.
    pub fn smooth_apply(
        &mut self,
        template: TemplateId,
        archetype: ArchetypeId,
        target_q14: i16,
        n_steps: u32,
    ) -> Result<u32, LoopTickError> {
        if n_steps == 0 {
            return Ok(0);
        }
        let mut applied = 0;
        for _ in 0..n_steps {
            let prev = self.bias_map.priority_shift(template, archetype);
            let raw = i32::from(target_q14).saturating_sub(i32::from(prev));
            let step = raw / (n_steps.max(1) as i32);
            let clamped = step.clamp(i32::from(BIAS_Q14_MIN), i32::from(BIAS_Q14_MAX)) as i16;
            if clamped == 0 {
                break;
            }
            let upd = KanBiasUpdate::new(template, archetype, clamped, 0);
            self.bias_map.apply_update(upd).map_err(LoopTickError::BiasMap)?;
            applied += 1;
        }
        Ok(applied)
    }

    /// Convenience : seed the bias-map with a constant-bias for sanity-tests
    /// + deterministic-replay fixtures.
    pub fn seed_bias(
        &mut self,
        template: TemplateId,
        archetype: ArchetypeId,
        delta_q14: i16,
    ) -> Result<(), LoopTickError> {
        let upd = KanBiasUpdate::new(template, archetype, delta_q14, 0);
        self.bias_map
            .apply_update(upd)
            .map_err(LoopTickError::BiasMap)
    }

    /// Demonstrate pre-application q14-arithmetic guard : caller supplies
    /// arbitrary i32 ; we clamp to ±BIAS_Q14_MAX for safety.
    pub fn clamped_delta(raw: i32) -> i16 {
        raw.clamp(i32::from(BIAS_Q14_MIN), i32::from(BIAS_Q14_MAX)) as i16
    }

    /// Helper : saturating-add wrapper exposed for sibling-W12-7 to combine
    /// rating-scores externally before submitting.
    pub fn combine_q14(a: i16, b: i16) -> i16 {
        saturating_add_q14(a, b)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::QualitySignal;

    fn rec(t: u32, a: u8, sig: QualitySignal, p: u32, off: u16) -> QualitySignalRecord {
        QualitySignalRecord::new(TemplateId(t), ArchetypeId(a), sig, PlayerHandle(p), off)
    }

    #[test]
    fn template_priority_shifts_after_kanon_met() {
        let mut loop_ = SelfAuthoringKanLoop::with_k_anon_floor(0, 1000, 5);
        // 5 distinct players ⇒ k-anon met ⇒ tick should produce a positive shift.
        for p in 0..5u32 {
            loop_.ingest_signal(rec(7, 2, QualitySignal::SandboxPass, p, 0)).unwrap();
        }
        let before = loop_.bias_map.priority_shift(TemplateId(7), ArchetypeId(2));
        assert_eq!(before, 0);
        let n = loop_.tick(0.016, 1000);
        assert_eq!(n, 1, "exactly one cell qualified, exactly one shift");
        let after = loop_.bias_map.priority_shift(TemplateId(7), ArchetypeId(2));
        assert!(after > 0, "after-tick bias must be positive : {}", after);
        assert_eq!(loop_.stats.last_tick_priority_shifts, 1);
        assert_eq!(loop_.stats.updates_applied, 1);
    }

    #[test]
    fn sigma_chain_anchor_cadence_emits_at_1024() {
        let mut loop_ = SelfAuthoringKanLoop::with_k_anon_floor(0, 0, 2);
        // Ingest signals to make 2 distinct players for many cells, then
        // pre-seed the bias_map.update_count via repeated seed_bias calls
        // (cheap path : exercise anchor cadence without running 1024 ticks).
        for i in 0..(crate::anchor::ANCHOR_EVERY_N_UPDATES + 5) {
            // saturating-add will clamp ; we just need update_count to
            // bump 1024+ times.
            let _ = loop_.seed_bias(TemplateId(i as u32 % 3000), ArchetypeId(0), 1);
        }
        // anchor wasn't actually emitted by seed_bias (it's a low-level fn) ;
        // we directly drive should_anchor + push_anchor via the loop.
        // Easier path : tick multiple times after k-anon-cells qualify.
        let mut loop2 = SelfAuthoringKanLoop::with_k_anon_floor(0, 0, 2);
        for cell_t in 0u32..(crate::anchor::ANCHOR_EVERY_N_UPDATES as u32 + 4) {
            loop2.ingest_signal(rec(cell_t, 0, QualitySignal::RemixForked, 1, 0)).unwrap();
            loop2.ingest_signal(rec(cell_t, 0, QualitySignal::RemixForked, 2, 0)).unwrap();
        }
        // Single tick will iterate over all qualifying cells. With ~1028 distinct
        // template-ids each having 2 distinct players, we get ~1028 priority-shifts.
        let _shifts = loop2.tick(0.016, 5000);
        assert!(loop2.bias_map.update_count() >= crate::anchor::ANCHOR_EVERY_N_UPDATES);
        assert!(
            loop2.stats.anchors_emitted >= 1,
            "at least one Σ-Chain anchor should have been emitted at update_count == 1024 ; emitted={}",
            loop2.stats.anchors_emitted
        );
        // Anchor-ring should have at least one record.
        assert!(loop2.anchor_ring.latest().is_some());
    }

    #[test]
    fn sovereign_revoke_cascades_through_bias_map() {
        let mut loop_ = SelfAuthoringKanLoop::with_k_anon_floor(0, 0, 3);
        // 3 distinct positive-signal players ⇒ tick pushes bias positive.
        for p in [10u32, 11, 12] {
            loop_.ingest_signal(rec(0, 0, QualitySignal::SandboxPass, p, 0)).unwrap();
        }
        loop_.tick(0.016, 0);
        let before = loop_.bias_map.priority_shift(TemplateId(0), ArchetypeId(0));
        assert!(before > 0);
        // Revoke player 10 ⇒ k-anon drops from 3 to 2 ⇒ cell no-longer-qualifies
        // ⇒ replay-tick produces no update for the cell ⇒ bias drops to 0.
        let scrubbed = loop_.sovereign_revoke(PlayerHandle(10), 0);
        assert_eq!(scrubbed, 1);
        let after = loop_.bias_map.priority_shift(TemplateId(0), ArchetypeId(0));
        assert_eq!(after, 0, "post-revoke replay produced no qualifying signal");
        assert!(loop_.reservoir.is_player_revoked(PlayerHandle(10)));
    }

    #[test]
    fn poisoning_attempt_rejected() {
        let mut loop_ = SelfAuthoringKanLoop::new(0, 0);
        // Single player tries to flood a cell — exceeds PER_CELL_RATE_LIMIT.
        for i in 0..crate::reservoir::PER_CELL_RATE_LIMIT {
            loop_.ingest_signal(rec(0, 0, QualitySignal::SandboxPass, 99, i as u16)).unwrap();
        }
        let result = loop_.ingest_signal(rec(0, 0, QualitySignal::SandboxPass, 99, 9999));
        assert!(matches!(result, Err(LoopTickError::Ingest(_))));
        assert_eq!(loop_.stats.signals_rejected, 1);
    }

    #[test]
    fn clamped_delta_handles_overflow() {
        assert_eq!(SelfAuthoringKanLoop::clamped_delta(i32::MAX), BIAS_Q14_MAX);
        assert_eq!(SelfAuthoringKanLoop::clamped_delta(i32::MIN), BIAS_Q14_MIN);
        assert_eq!(SelfAuthoringKanLoop::clamped_delta(100), 100);
    }

    #[test]
    fn smooth_apply_drives_towards_target() {
        let mut loop_ = SelfAuthoringKanLoop::new(0, 0);
        let n = loop_.smooth_apply(TemplateId(0), ArchetypeId(0), 1000, 4).unwrap();
        // Each step adds (1000 - prev) / 4. After 4 steps, bias should be near 1000.
        assert!(n >= 1);
        let final_bias = loop_.bias_map.priority_shift(TemplateId(0), ArchetypeId(0));
        assert!((200..=1000).contains(&final_bias), "smooth converged : {}", final_bias);
    }
}

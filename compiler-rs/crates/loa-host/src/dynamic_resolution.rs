//! § dynamic_resolution — adaptive render-resolution scaler for substrate-render.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-DYNRES-SCALER · Apocky-greenlit (2026-05-02)
//!
//! § APOCKY-DIRECTIVE
//!   Target 1440p144 (6.94 ms / frame) · when frame-time exceeds budget,
//!   scale render-resolution down (1.0 → 0.85 → 0.71) ; when frame-time is
//!   well under budget, scale back up. Floor 0.5× (720p of 1440p), cap 1.0×.
//!   Lerp toward target over ~30 frames (smooth · no jank).
//!
//! § PARADIGM
//!   - Q0.16 fixed-point scale (`current_scale_q16` · 65_536 = 1.0×).
//!     ∵ Apocky preference : fixed-point math over float where possible.
//!   - EMA on frame-time (α ≈ 1/16 · 4-frame half-life · cheap u64).
//!   - Lerp toward target-scale at ~3.3 % per frame ⇒ ≈ 30 frames to
//!     converge (visually smooth).
//!   - Honours `LOA_DYN_RES=0` env-var to disable (returns native dims).
//!   - Future · KAN-bias hooks (W18+) so per-display + per-scene learning
//!     adapts the scaler curve as just-another-KAN-parameter.
//!
//! § SCALE-LADDER (informational ; scaler is continuous in Q0.16, the
//!   ladder shows roughly where it lands during convergence)
//!
//!     1.00 ── native            (65_536 q16)   ← cap
//!     0.85 ── 1440p × 0.85      (55_705 q16)
//!     0.71 ── 1440p × 0.71      (46_530 q16)
//!     0.50 ── 720p of 1440p     (32_768 q16)   ← floor
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. The scaler reads frame-time only ; no surveillance, no
//! identifying telemetry. Sovereignty-respecting fast-path : the env-var
//! `LOA_DYN_RES=0` deterministically disables all adaptation.

#![allow(clippy::module_name_repetitions)]

/// Q0.16 fixed-point scale of 1.0×.
pub const Q16_ONE: u32 = 65_536;

/// Default floor 0.5× (720p of 1440p · per Apocky-spec).
pub const Q16_FLOOR_DEFAULT: u32 = Q16_ONE / 2; // 32_768

/// Default cap 1.0× (no upscale-from-native).
pub const Q16_CAP_DEFAULT: u32 = Q16_ONE;

/// Default 1440p144 budget · 6_944 µs ≈ 1 / 144 s.
pub const TARGET_FRAME_US_144HZ: u64 = 6_944;

/// EMA shift · 1 / (1 << EMA_SHIFT) is the new-sample weight.
/// 4 ⇒ α = 1/16 ⇒ ≈ 4-frame half-life. Cheap (single shift+add).
pub const EMA_SHIFT: u32 = 4;

/// Lerp step per-frame, in Q0.16 units. 2_185 q16 ≈ 0.0333 ≈ 1/30 ⇒
/// converges toward target in ≈ 30 frames (one quarter-second @ 144 Hz).
pub const LERP_STEP_Q16: u32 = 2_185;

/// Hysteresis guard band around the budget. We only scale DOWN once the
/// EMA exceeds the budget, and only scale UP once the EMA is comfortably
/// under (≈ 80 % of budget). Without this the scaler thrashes around the
/// boundary frame-to-frame.
pub const HYSTERESIS_UP_NUM: u64 = 80;
pub const HYSTERESIS_UP_DEN: u64 = 100;

/// Per-frame adaptive scaler. Q0.16 fixed-point throughout · no float
/// in the hot path. Substrate-paradigm-pure.
///
/// Construct with `Scaler::new()` (defaults to 1440p144 budget · floor
/// 0.5× · cap 1.0×) or `Scaler::with_target(us)` for non-default budgets.
#[derive(Debug, Clone, Copy)]
pub struct Scaler {
    /// Current resolution-scale, Q0.16. Starts at 1.0× (`Q16_ONE`).
    pub current_scale_q16: u32,
    /// Target resolution-scale we are lerping toward, Q0.16. Updated each
    /// frame from the EMA frame-time vs the budget.
    pub target_scale_q16: u32,
    /// EMA of observed frame-time in microseconds. 0 means "no samples
    /// yet" — the first observe-frame seeds the EMA directly.
    pub ema_frame_us: u64,
    /// Frame budget in microseconds (e.g. 6_944 for 1440p144).
    pub target_frame_us: u64,
    /// Floor (lower-bound) on `current_scale_q16`.
    pub min_scale_q16: u32,
    /// Cap (upper-bound) on `current_scale_q16`.
    pub max_scale_q16: u32,
    /// Disabled flag (mirrors `LOA_DYN_RES=0`). When true, `render_dims`
    /// always returns native dims and `observe_frame` is a no-op.
    pub disabled: bool,
}

impl Default for Scaler {
    fn default() -> Self { Self::new() }
}

impl Scaler {
    /// Construct a scaler with the 1440p144 budget · 0.5×..1.0× scale band.
    /// Reads `LOA_DYN_RES` env-var on construction · "0" disables.
    #[must_use]
    pub fn new() -> Self {
        Self::with_target(TARGET_FRAME_US_144HZ)
    }

    /// Construct a scaler with an explicit frame-budget (microseconds).
    /// Honours `LOA_DYN_RES=0` to disable.
    #[must_use]
    pub fn with_target(target_frame_us: u64) -> Self {
        let disabled = std::env::var("LOA_DYN_RES")
            .map(|v| v.trim() == "0")
            .unwrap_or(false);
        Self {
            current_scale_q16: Q16_CAP_DEFAULT,
            target_scale_q16: Q16_CAP_DEFAULT,
            ema_frame_us: 0,
            target_frame_us,
            min_scale_q16: Q16_FLOOR_DEFAULT,
            max_scale_q16: Q16_CAP_DEFAULT,
            disabled,
        }
    }

    /// Return the current scale as a float (for telemetry/HUD only ; the
    /// hot path stays in Q0.16).
    #[must_use]
    pub fn scale_f32(&self) -> f32 {
        (self.current_scale_q16 as f32) / (Q16_ONE as f32)
    }

    /// True iff the scaler has been disabled by `LOA_DYN_RES=0`.
    #[must_use]
    pub fn is_disabled(&self) -> bool { self.disabled }

    /// Manually override the disabled flag (useful for tests).
    pub fn set_disabled(&mut self, d: bool) { self.disabled = d; }

    /// Feed one frame-time observation into the EMA + adjust the target
    /// scale toward the budget. Cheap : 2 mul-adds for EMA, 1 compare,
    /// 1 lerp. Safe to call from the per-frame hot-path.
    pub fn observe_frame(&mut self, frame_us: u64) {
        if self.disabled { return; }
        // Update EMA.
        if self.ema_frame_us == 0 {
            self.ema_frame_us = frame_us;
        } else {
            // ema = ema + (sample - ema) >> EMA_SHIFT, with safe-sub.
            let prev = self.ema_frame_us as i128;
            let sample = frame_us as i128;
            let delta = (sample - prev) >> EMA_SHIFT;
            let next = (prev + delta).max(0) as u64;
            self.ema_frame_us = next;
        }

        // Recompute target scale from EMA vs budget.
        // Hysteresis : only scale up once we are well-under (80 % of budget).
        let budget = self.target_frame_us.max(1);
        let ema = self.ema_frame_us;
        let well_under = ema * HYSTERESIS_UP_DEN < budget * HYSTERESIS_UP_NUM;

        if ema > budget {
            // Over budget — pick a scale ∝ (budget / ema) in Q0.16 (so the
            // pixel-count scales linearly with the over-shoot).
            // ratio_q16 = budget * Q16_ONE / ema, clamped to [min, max].
            let ratio_q16 = (budget as u128 * Q16_ONE as u128 / ema as u128) as u32;
            self.target_scale_q16 = ratio_q16.clamp(self.min_scale_q16, self.max_scale_q16);
        } else if well_under {
            // Well under budget — let the target drift back toward the cap.
            self.target_scale_q16 = self.max_scale_q16;
        }
        // Otherwise (in the hysteresis dead-band) leave target unchanged.

        // Lerp current toward target by LERP_STEP_Q16.
        if self.current_scale_q16 < self.target_scale_q16 {
            let next = self.current_scale_q16.saturating_add(LERP_STEP_Q16);
            self.current_scale_q16 = next.min(self.target_scale_q16);
        } else if self.current_scale_q16 > self.target_scale_q16 {
            let next = self.current_scale_q16.saturating_sub(LERP_STEP_Q16);
            self.current_scale_q16 = next.max(self.target_scale_q16);
        }
        // Final clamp · belt-and-braces.
        self.current_scale_q16 = self.current_scale_q16.clamp(self.min_scale_q16, self.max_scale_q16);
    }

    /// Return the rendered dimensions for the given native panel size,
    /// applied to the current Q0.16 scale. Snaps both axes to a multiple
    /// of 8 to match the substrate-resonance compute-shader 8×8 workgroup
    /// dispatch (avoids edge-pixel underflow at the boundary).
    #[must_use]
    pub fn render_dims(&self, native_w: u32, native_h: u32) -> (u32, u32) {
        if self.disabled || self.current_scale_q16 == self.max_scale_q16 {
            return (native_w, native_h);
        }
        let s = self.current_scale_q16 as u64;
        // (native * s + Q16_ONE/2) / Q16_ONE  · rounded.
        let half = (Q16_ONE / 2) as u64;
        let w = ((native_w as u64 * s + half) / Q16_ONE as u64) as u32;
        let h = ((native_h as u64 * s + half) / Q16_ONE as u64) as u32;
        // Snap to 8-pixel-multiple (workgroup-aligned). Floor is 8.
        let w = (w / 8).max(1) * 8;
        let h = (h / 8).max(1) * 8;
        (w, h)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// § T11-W18-DYNRES-SCALER · unit-tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper · construct a fresh scaler with deterministic defaults
    /// (env-var influence disabled regardless of host shell).
    fn fresh() -> Scaler {
        let mut s = Scaler::new();
        s.set_disabled(false);
        s
    }

    #[test]
    fn defaults_are_one_native_until_first_observe() {
        let s = fresh();
        assert_eq!(s.current_scale_q16, Q16_ONE);
        assert_eq!(s.target_scale_q16, Q16_ONE);
        assert_eq!(s.target_frame_us, TARGET_FRAME_US_144HZ);
        assert_eq!(s.min_scale_q16, Q16_FLOOR_DEFAULT);
        assert_eq!(s.max_scale_q16, Q16_CAP_DEFAULT);
        // 1440p native passes through 1:1 at boot.
        assert_eq!(s.render_dims(2560, 1440), (2560, 1440));
    }

    #[test]
    fn over_budget_triggers_scale_down() {
        // Frame-time 13_888 µs ≈ 2× budget · scaler should target ≈ 0.5.
        let mut s = fresh();
        for _ in 0..200 {
            s.observe_frame(13_888);
        }
        // After 200 frames the EMA has fully saturated and the lerp has
        // reached the target. Target ratio ≈ 6_944 / 13_888 = 0.5 ⇒
        // clamped to floor 0.5 × Q16_ONE = 32_768.
        assert_eq!(s.current_scale_q16, Q16_FLOOR_DEFAULT);
        let (w, h) = s.render_dims(2560, 1440);
        // Scaled to 1280×720 (snapped to 8) — 720p of 1440p.
        assert_eq!((w, h), (1280, 720));
    }

    #[test]
    fn well_under_budget_recovers_to_cap() {
        let mut s = fresh();
        // Drop to 0.5× by simulating heavy frames first.
        for _ in 0..200 { s.observe_frame(20_000); }
        assert_eq!(s.current_scale_q16, Q16_FLOOR_DEFAULT);
        // Now feed easy frames (1 ms — vastly under 6.9 ms budget).
        for _ in 0..200 { s.observe_frame(1_000); }
        // Should have recovered all the way to the cap.
        assert_eq!(s.current_scale_q16, Q16_CAP_DEFAULT);
    }

    #[test]
    fn hysteresis_dead_band_holds_target_steady() {
        // Construct a scaler whose EMA is already in the dead-band [80 %..
        // 100 %] of budget, then verify a single-observe in that band leaves
        // the target unchanged. This isolates the dead-band logic from the
        // EMA's transient when crossing the band.
        let mut s = fresh();
        // Lock target to the floor (simulating recovery from a heavy
        // workload), set EMA to 90 % of budget (inside dead-band).
        s.target_scale_q16 = Q16_FLOOR_DEFAULT;
        s.current_scale_q16 = Q16_FLOOR_DEFAULT;
        s.ema_frame_us = (TARGET_FRAME_US_144HZ * 90) / 100;
        let target_before = s.target_scale_q16;
        // Feed one in-band sample. Target should be unchanged (dead-band).
        // Sample value matches EMA so EMA stays put.
        s.observe_frame(s.ema_frame_us);
        assert_eq!(s.target_scale_q16, target_before,
            "target drifted to {} from {} inside dead-band",
            s.target_scale_q16, target_before);
    }

    #[test]
    fn floor_and_cap_clamp_extreme_observations() {
        // Catastrophic frame-time (10× budget). Should clamp at floor.
        let mut s = fresh();
        for _ in 0..500 { s.observe_frame(70_000); }
        assert_eq!(s.current_scale_q16, Q16_FLOOR_DEFAULT);

        // Trivial frame-time (1 µs). Should clamp at cap.
        let mut s2 = fresh();
        for _ in 0..500 { s2.observe_frame(1); }
        assert_eq!(s2.current_scale_q16, Q16_CAP_DEFAULT);
    }

    #[test]
    fn q016_precision_round_trip() {
        // 0.85 in Q0.16 = 55_705 (round 0.85 × 65536 = 55_705.6 ⇒ 55_705).
        let mut s = fresh();
        s.current_scale_q16 = 55_705;
        // 2560 × 0.85 = 2176 → snapped 2176, 1440 × 0.85 = 1224 → snapped 1224.
        let (w, h) = s.render_dims(2560, 1440);
        assert_eq!(w, 2176);
        assert_eq!(h, 1224);
        // Float view matches within Q16_ONE precision.
        let f = s.scale_f32();
        assert!((f - 0.85).abs() < 1.0 / (Q16_ONE as f32));
    }

    #[test]
    fn lerp_takes_about_thirty_frames_over_full_band() {
        // From 1.0 → 0.5 over the full band the lerp needs roughly
        // (Q16_CAP - Q16_FLOOR) / LERP_STEP_Q16 frames. Verify it's in the
        // expected ballpark (≤ 32 frames for the half-band drop).
        let mut s = fresh();
        s.current_scale_q16 = Q16_CAP_DEFAULT;
        s.target_scale_q16 = Q16_FLOOR_DEFAULT;
        s.ema_frame_us = 20_000; // big EMA so target stays at floor
        let mut frames = 0;
        while s.current_scale_q16 > Q16_FLOOR_DEFAULT && frames < 100 {
            s.observe_frame(20_000);
            frames += 1;
        }
        assert!(frames <= 32 && frames >= 12,
            "frames={} outside expected ~30 lerp-window", frames);
    }

    #[test]
    fn disabled_returns_native_and_does_not_observe() {
        let mut s = fresh();
        s.set_disabled(true);
        for _ in 0..500 { s.observe_frame(1_000_000); }
        assert_eq!(s.ema_frame_us, 0); // never observed
        assert_eq!(s.render_dims(2560, 1440), (2560, 1440));
        assert_eq!(s.current_scale_q16, Q16_CAP_DEFAULT);
    }

    #[test]
    fn render_dims_snaps_to_workgroup_multiple() {
        let mut s = fresh();
        // 0.71 in Q0.16 = 46_530.
        s.current_scale_q16 = 46_530;
        let (w, h) = s.render_dims(1920, 1080);
        // Both must be multiples of 8 (compute-shader 8×8 workgroup).
        assert_eq!(w % 8, 0);
        assert_eq!(h % 8, 0);
        // Sane neighbourhoods : 1920×0.71 ≈ 1363 ⇒ snap-down to 1360.
        assert!(w >= 1360 && w <= 1368, "w={}", w);
        assert!(h >= 760  && h <= 768,  "h={}", h);
    }

    #[test]
    fn ema_seeds_on_first_sample_then_smooths() {
        let mut s = fresh();
        s.observe_frame(8_000);
        assert_eq!(s.ema_frame_us, 8_000); // seed
        s.observe_frame(8_000);
        // Second sample · same value · should stay flat.
        assert_eq!(s.ema_frame_us, 8_000);
        // Spike to 16_000 · EMA shifts toward it but slowly.
        s.observe_frame(16_000);
        // delta = (16000 - 8000) >> 4 = 500. New ema = 8500.
        assert_eq!(s.ema_frame_us, 8_500);
    }

    #[test]
    fn target_recomputes_every_observe() {
        let mut s = fresh();
        // Start at cap, push way over budget.
        s.observe_frame(13_888);
        // EMA seeded at 13_888 — over budget. target ≈ 6944/13888 ≈ 0.5.
        assert!(s.target_scale_q16 <= Q16_ONE / 2 + 8,
            "target_scale_q16={} unexpectedly high", s.target_scale_q16);
    }
}

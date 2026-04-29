//! Bus routing — submix structure between voices and the master output.
//!
//! § DESIGN
//!   The mixer's bus structure mirrors a traditional console :
//!     ```text
//!       voice_1 ─┐
//!       voice_2 ─┼─ Bus("sfx") ─┐
//!       voice_3 ─┘              │
//!                                ├─ MasterBus → AudioStream
//!       voice_4 ──── Bus("music") ┘
//!     ```
//!   Each voice routes into exactly one bus (or the master directly).
//!   Each bus has its own gain + optional effect chain. The master
//!   bus carries the final limiter that prevents inter-sample peaks
//!   from clipping the device.
//!
//! § BUS LIFECYCLE
//!   Buses are pre-allocated at mixer construction (or via
//!   `Mixer::create_bus`) and live for the mixer's lifetime. Voice
//!   routing is set at `play()` time via `PlayParams::with_bus()` and
//!   can be reconfigured later via `Mixer::set_voice_bus()`.
//!
//! § DETERMINISM
//!   Bus iteration is by `BusId`, which is monotone-ascending. Bus
//!   sums happen in id order so two replays produce bit-equal mix.

use crate::dsp::EffectChain;

/// Stable identifier for a registered bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BusId(pub u32);

/// A submix bus — accumulates voice output, applies gain + effects,
/// then routes into the master.
pub struct Bus {
    /// Stable id.
    pub id: BusId,
    /// Human-readable name (debug-aid).
    pub name: String,
    /// Pre-effect gain (1.0 = unity).
    pub gain: f32,
    /// Effect chain applied per render.
    pub effects: EffectChain,
    /// Whether the bus is muted (silent regardless of voice activity).
    pub muted: bool,
    /// Solo flag — when ANY bus is solo'd, only solo'd buses contribute
    /// to master. Default `false`.
    pub solo: bool,
    /// Per-bus accumulator. Pre-allocated to the mixer's frame budget.
    /// Reused across renders ; never grows on the hot path.
    pub(crate) accumulator: Vec<f32>,
}

impl Bus {
    /// Construct a fresh bus. `accumulator_capacity` should equal the
    /// mixer's `(frames_per_block * channels)` so the hot path never
    /// reallocates.
    #[must_use]
    pub fn new(id: BusId, name: impl Into<String>, accumulator_capacity: usize) -> Self {
        Self {
            id,
            name: name.into(),
            gain: 1.0,
            effects: EffectChain::new(),
            muted: false,
            solo: false,
            accumulator: vec![0.0; accumulator_capacity],
        }
    }

    /// Resize the accumulator. Called by `Mixer::set_block_size` when
    /// the per-block frame count changes. Hot-path code does NOT call
    /// this — it's a setup-time operation.
    pub fn resize_accumulator(&mut self, capacity: usize) {
        self.accumulator.resize(capacity, 0.0);
    }

    /// Zero the accumulator. Called at the start of every render block.
    pub fn clear_accumulator(&mut self) {
        for slot in &mut self.accumulator {
            *slot = 0.0;
        }
    }

    /// Accumulator as a slice.
    #[must_use]
    pub fn accumulator(&self) -> &[f32] {
        &self.accumulator
    }

    /// Accumulator as a mutable slice — used by the mixer's voice loop.
    pub fn accumulator_mut(&mut self) -> &mut [f32] {
        &mut self.accumulator
    }

    /// Apply the bus's effect chain + gain to the accumulator. Returns
    /// the post-effect, post-gain slice.
    pub fn process_in_place(&mut self, channels: usize, sample_rate: u32) {
        if self.muted {
            for slot in &mut self.accumulator {
                *slot = 0.0;
            }
            return;
        }
        // Effects first (run on raw mix), then gain.
        self.effects.process(&mut self.accumulator, channels, sample_rate);
        if (self.gain - 1.0).abs() > 1e-7 {
            for slot in &mut self.accumulator {
                *slot *= self.gain;
            }
        }
    }
}

impl core::fmt::Debug for Bus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Bus")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("gain", &self.gain)
            .field("muted", &self.muted)
            .field("solo", &self.solo)
            .field("accumulator_len", &self.accumulator.len())
            .field("effects", &self.effects)
            .finish()
    }
}

/// Master bus — final stage before submission to the audio device.
///
/// § MASTER LIMITER
///   The master always carries an inter-sample-peak limiter that
///   prevents clipping when bus sums exceed the [-1, 1] device range.
///   The limiter is a soft-knee design : signals below `threshold`
///   pass unaffected ; signals above are compressed via a smooth-knee
///   tanh-based curve.
pub struct MasterBus {
    /// Master gain (post-bus-sum, pre-limiter).
    pub gain: f32,
    /// Limiter threshold (default 0.95 = -0.45 dBFS headroom).
    pub limiter_threshold: f32,
    /// Whether to enable the limiter (default true).
    pub limiter_enabled: bool,
    /// Master accumulator.
    pub(crate) accumulator: Vec<f32>,
    /// Master effect chain (runs before the limiter).
    pub effects: EffectChain,
}

impl MasterBus {
    /// Construct a master bus with the default limiter threshold.
    #[must_use]
    pub fn new(accumulator_capacity: usize) -> Self {
        Self {
            gain: 1.0,
            limiter_threshold: 0.95,
            limiter_enabled: true,
            accumulator: vec![0.0; accumulator_capacity],
            effects: EffectChain::new(),
        }
    }

    /// Resize the accumulator. Setup-time only.
    pub fn resize_accumulator(&mut self, capacity: usize) {
        self.accumulator.resize(capacity, 0.0);
    }

    /// Zero the accumulator. Called at the start of every render block.
    pub fn clear_accumulator(&mut self) {
        for slot in &mut self.accumulator {
            *slot = 0.0;
        }
    }

    /// Accumulator as a slice.
    #[must_use]
    pub fn accumulator(&self) -> &[f32] {
        &self.accumulator
    }

    /// Mix a slice of bus output into the master accumulator.
    pub fn mix_in(&mut self, src: &[f32]) {
        let n = src.len().min(self.accumulator.len());
        for i in 0..n {
            self.accumulator[i] += src[i];
        }
    }

    /// Apply gain + effects + limiter. Always runs (not gated on `gain != 1`)
    /// because the limiter is the canonical clip-protection path.
    pub fn finalize(&mut self, channels: usize, sample_rate: u32) {
        if (self.gain - 1.0).abs() > 1e-7 {
            for slot in &mut self.accumulator {
                *slot *= self.gain;
            }
        }
        self.effects.process(&mut self.accumulator, channels, sample_rate);
        if self.limiter_enabled {
            apply_limiter(&mut self.accumulator, self.limiter_threshold);
        }
    }

    /// Accumulator as a mutable slice — used internally for testing /
    /// advanced routing.
    pub fn accumulator_mut(&mut self) -> &mut [f32] {
        &mut self.accumulator
    }
}

impl Default for MasterBus {
    fn default() -> Self {
        Self::new(512)
    }
}

impl core::fmt::Debug for MasterBus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MasterBus")
            .field("gain", &self.gain)
            .field("limiter_threshold", &self.limiter_threshold)
            .field("limiter_enabled", &self.limiter_enabled)
            .field("accumulator_len", &self.accumulator.len())
            .field("effects", &self.effects)
            .finish()
    }
}

/// Soft-knee limiter — tanh-based smooth saturation above threshold.
///
/// § DESIGN
///   The limiter uses a `tanh`-shaped soft knee :
///     x_out = sign(x) * threshold * tanh(|x| / threshold)  if |x| > threshold
///           = x                                            otherwise
///   This preserves transients within the linear region + smoothly
///   compresses peaks. The tanh curve is symmetric so the limiter is
///   DC-bias-preserving.
fn apply_limiter(buffer: &mut [f32], threshold: f32) {
    let t = threshold.max(1e-6);
    for sample in buffer.iter_mut() {
        let abs = sample.abs();
        if abs > t {
            let s = if *sample >= 0.0 { 1.0 } else { -1.0 };
            // Soft knee : tanh maps (0, ∞) → (0, 1), scaled by t.
            *sample = s * t * (abs / t).tanh();
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn bus_id_ord() {
        assert!(BusId(1) < BusId(2));
    }

    #[test]
    fn bus_new_unity_gain_no_mute() {
        let b = Bus::new(BusId(0), "sfx", 256);
        assert_eq!(b.gain, 1.0);
        assert!(!b.muted);
        assert!(!b.solo);
        assert_eq!(b.accumulator.len(), 256);
    }

    #[test]
    fn bus_resize_accumulator_extends() {
        let mut b = Bus::new(BusId(0), "sfx", 64);
        b.resize_accumulator(128);
        assert_eq!(b.accumulator.len(), 128);
    }

    #[test]
    fn bus_clear_accumulator_zeros() {
        let mut b = Bus::new(BusId(0), "sfx", 4);
        for slot in &mut b.accumulator {
            *slot = 0.7;
        }
        b.clear_accumulator();
        assert!(b.accumulator.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn bus_process_muted_zeros_signal() {
        let mut b = Bus::new(BusId(0), "sfx", 4);
        for slot in &mut b.accumulator {
            *slot = 0.5;
        }
        b.muted = true;
        b.process_in_place(2, 48_000);
        assert!(b.accumulator.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn bus_process_applies_gain() {
        let mut b = Bus::new(BusId(0), "sfx", 4);
        for slot in &mut b.accumulator {
            *slot = 0.5;
        }
        b.gain = 2.0;
        b.process_in_place(2, 48_000);
        for slot in &b.accumulator {
            assert!((slot - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn bus_process_unity_gain_passthrough() {
        let mut b = Bus::new(BusId(0), "sfx", 4);
        for (i, slot) in b.accumulator.iter_mut().enumerate() {
            *slot = (i as f32) * 0.1;
        }
        let before = b.accumulator.clone();
        b.process_in_place(2, 48_000);
        for (a, c) in before.iter().zip(b.accumulator.iter()) {
            assert!((a - c).abs() < 1e-6);
        }
    }

    #[test]
    fn master_new_unity_gain() {
        let m = MasterBus::new(256);
        assert_eq!(m.gain, 1.0);
        assert!(m.limiter_enabled);
        assert_eq!(m.accumulator.len(), 256);
    }

    #[test]
    fn master_default_capacity_512() {
        let m = MasterBus::default();
        assert_eq!(m.accumulator.len(), 512);
    }

    #[test]
    fn master_mix_in_sums() {
        let mut m = MasterBus::new(4);
        let src = [0.1, 0.2, 0.3, 0.4];
        m.mix_in(&src);
        m.mix_in(&src);
        for (a, b) in m.accumulator.iter().zip([0.2, 0.4, 0.6, 0.8]) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn master_finalize_applies_gain() {
        let mut m = MasterBus::new(4);
        for slot in &mut m.accumulator {
            *slot = 0.4;
        }
        m.gain = 0.5;
        m.limiter_enabled = false; // isolate gain effect
        m.finalize(2, 48_000);
        for slot in &m.accumulator {
            assert!((slot - 0.2).abs() < 1e-6);
        }
    }

    #[test]
    fn limiter_passes_below_threshold() {
        let mut buf = [0.5_f32; 4];
        apply_limiter(&mut buf, 0.95);
        for s in &buf {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn limiter_compresses_above_threshold() {
        let mut buf = [2.0_f32; 4];
        apply_limiter(&mut buf, 0.95);
        for s in &buf {
            // tanh(2.0/0.95) * 0.95 ≈ 0.95 * tanh(2.105) ≈ 0.95 * 0.971 ≈ 0.922
            // Below 0.95 (threshold) ; this is the soft-knee curve.
            assert!(s.abs() <= 0.95);
            assert!(s.abs() > 0.7);
        }
    }

    #[test]
    fn limiter_preserves_sign() {
        let mut buf = [-2.0_f32, 2.0, -0.3, 0.3];
        apply_limiter(&mut buf, 0.95);
        assert!(buf[0] < 0.0);
        assert!(buf[1] > 0.0);
        assert!((buf[2] - -0.3).abs() < 1e-6);
        assert!((buf[3] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn limiter_zero_threshold_safe() {
        // Threshold 0 is invalid but limiter should clamp to epsilon, not divide-by-zero.
        let mut buf = [1.0_f32];
        apply_limiter(&mut buf, 0.0);
        // tanh(1/1e-6) ≈ 1.0 → output ≈ 1e-6 * 1 ≈ ~0.
        assert!(buf[0].abs() < 0.001);
    }

    #[test]
    fn master_finalize_with_limiter_clamps() {
        let mut m = MasterBus::new(4);
        for slot in &mut m.accumulator {
            *slot = 2.0;
        }
        m.finalize(2, 48_000);
        // After limiter, |sample| < threshold (0.95).
        for s in &m.accumulator {
            assert!(s.abs() < 0.96);
        }
    }
}

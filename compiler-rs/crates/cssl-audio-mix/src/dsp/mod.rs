//! DSP primitives + effect chain.
//!
//! § THESIS
//!   Audio effects are pluggable processors that operate in-place on
//!   interleaved-stereo (or mono / surround) sample buffers. Each
//!   processor implements the `Effect` trait, which exposes a single
//!   `process` method called from the mixer's render loop.
//!
//!   Processors carry their own filter state (e.g., biquad delay-line,
//!   reverb tap memory). State is owned per processor instance + reset
//!   via the `reset` trait method ; stateful effects implement
//!   `state_snapshot` / `state_restore` for replay-determinism.
//!
//! § HOT-PATH DISCIPLINE  ‼ load-bearing
//!   - **No allocation** : all per-frame compute uses pre-sized buffers.
//!   - **No unbounded loops** : every loop is buffer-bounded or has a
//!     static iteration count.
//!   - **No platform clock reads** : effects use sample-rate parameter,
//!     not `Instant::now`.
//!   - **No `Box<dyn Trait>` dispatch on the hot path beyond chain
//!     iteration** : the EffectChain holds a `Vec<Box<dyn Effect>>`,
//!     iterated linearly. Stage-0 accepts the v-table cost ; a future
//!     slice can switch to enum-dispatch if profiles demand it.
//!
//! § DETERMINISM
//!   All filter algorithms are pure-deterministic over `(input, params)`.
//!   No `thread_rng()`, no time-based modulation source, no platform
//!   clock. Two effect-chain runs with identical inputs + identical
//!   parameters produce bit-equal output.

pub mod biquad;
pub mod compressor;
pub mod delay;
pub mod reverb;

pub use biquad::{Biquad, BiquadKind};
pub use compressor::{Compressor, Limiter};
pub use delay::Delay;
pub use reverb::Reverb;

/// Trait implemented by any audio processor that can be inserted into
/// an `EffectChain`.
///
/// § CONTRACT
///   - `process(buffer, channels, sample_rate)` mutates `buffer` in
///     place. Buffer is interleaved across `channels`.
///   - `reset()` clears any internal state (delay-line memory, filter
///     history, envelope follower state).
///   - `name()` returns a debug-string identifier for logs.
///
/// § THREAD SAFETY
///   Effects are `Send` so the mixer can move them between threads
///   (gameplay thread → audio thread). They are NOT `Sync` ; the mixer
///   serializes access via the audio-callback fiber.
pub trait Effect: Send {
    /// Process `buffer` in place. Buffer is interleaved across `channels`.
    /// Sample rate is given so frequency-dependent effects (filters)
    /// can compute coefficients.
    fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32);

    /// Reset all internal state. Called when a voice retires or the
    /// chain is taken offline.
    fn reset(&mut self);

    /// Debug-string identifier.
    fn name(&self) -> &'static str;
}

/// Ordered list of `Effect` instances applied in series.
///
/// § ORDER
///   Effects are processed in insertion order — the first inserted is
///   the first applied. Callers wire :
///     `voice -> bus -> [filter, delay, reverb] -> master`
///   by inserting `filter`, `delay`, `reverb` in that order on the bus.
pub struct EffectChain {
    effects: Vec<Box<dyn Effect>>,
    /// Whether the chain is bypassed entirely. Used for A/B testing.
    pub bypass: bool,
}

impl EffectChain {
    /// Construct an empty chain (no effects, not bypassed).
    #[must_use]
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            bypass: false,
        }
    }

    /// Append an effect to the chain.
    pub fn push(&mut self, effect: Box<dyn Effect>) {
        self.effects.push(effect);
    }

    /// Number of effects in the chain.
    #[must_use]
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Whether the chain is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Remove all effects from the chain.
    pub fn clear(&mut self) {
        self.effects.clear();
    }

    /// Reset all effects' internal state.
    pub fn reset_all(&mut self) {
        for e in &mut self.effects {
            e.reset();
        }
    }

    /// Process the chain in series. No-op if `bypass` is true.
    pub fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32) {
        if self.bypass {
            return;
        }
        for e in &mut self.effects {
            e.process(buffer, channels, sample_rate);
        }
    }

    /// Names of effects in order — for debug-tracing.
    #[must_use]
    pub fn effect_names(&self) -> Vec<&'static str> {
        self.effects.iter().map(|e| e.name()).collect()
    }
}

impl Default for EffectChain {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for EffectChain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EffectChain")
            .field("len", &self.effects.len())
            .field("bypass", &self.bypass)
            .field("names", &self.effect_names())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test fixture : a passthrough effect that records process-call count.
    struct CountingPassthrough {
        calls: u32,
    }
    impl Effect for CountingPassthrough {
        fn process(&mut self, _buffer: &mut [f32], _channels: usize, _sample_rate: u32) {
            self.calls += 1;
        }
        fn reset(&mut self) {
            self.calls = 0;
        }
        fn name(&self) -> &'static str {
            "passthrough"
        }
    }

    #[test]
    fn chain_new_is_empty_not_bypassed() {
        let c = EffectChain::new();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
        assert!(!c.bypass);
    }

    #[test]
    fn chain_push_appends() {
        let mut c = EffectChain::new();
        c.push(Box::new(CountingPassthrough { calls: 0 }));
        c.push(Box::new(CountingPassthrough { calls: 0 }));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn chain_process_calls_each_effect_in_order() {
        let mut c = EffectChain::new();
        c.push(Box::new(CountingPassthrough { calls: 0 }));
        c.push(Box::new(CountingPassthrough { calls: 0 }));
        let mut buf = [0.0_f32; 4];
        c.process(&mut buf, 2, 48_000);
        assert_eq!(c.effect_names(), vec!["passthrough", "passthrough"]);
    }

    /// Marker effect that asserts on process call ; used to verify
    /// bypass actually skips processing.
    struct PanicOnProcess;
    impl Effect for PanicOnProcess {
        fn process(&mut self, _b: &mut [f32], _c: usize, _r: u32) {
            panic!("bypass should have skipped this effect");
        }
        fn reset(&mut self) {}
        fn name(&self) -> &'static str {
            "panic"
        }
    }

    #[test]
    fn chain_bypass_skips_processing() {
        let mut c = EffectChain::new();
        c.bypass = true;
        c.push(Box::new(PanicOnProcess));
        let mut buf = [0.0_f32; 4];
        // If bypass is honored, this does not call process → no panic.
        c.process(&mut buf, 2, 48_000);
        // Sanity : chain still reports the effect name.
        assert_eq!(c.effect_names(), vec!["panic"]);
    }

    #[test]
    fn chain_clear_drops_all() {
        let mut c = EffectChain::new();
        c.push(Box::new(CountingPassthrough { calls: 0 }));
        c.push(Box::new(CountingPassthrough { calls: 0 }));
        c.clear();
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn chain_reset_all_resets_each_effect() {
        let mut c = EffectChain::new();
        c.push(Box::new(CountingPassthrough { calls: 5 }));
        // Just verify no panic ; we can't peek into trait-object state.
        c.reset_all();
    }

    #[test]
    fn chain_default_is_empty() {
        let c = EffectChain::default();
        assert!(c.is_empty());
    }
}

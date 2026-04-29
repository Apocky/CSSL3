//! Atomic accumulation for shared adjoints in the reverse-pass.
//!
//! § SPEC : `specs/05_AUTODIFF.csl § GPU AUTODIFF` :
//!   `atomic-accumulation : OpAtomicFAdd if-available else CAS-loop emulation`.
//!
//! § DESIGN
//!   In a multi-thread reverse-pass, multiple lanes may write to the same
//!   parameter cotangent simultaneously (e.g. weight-sharing in KAN /
//!   spectral-BRDF training). The accumulator must be atomic. SPIR-V provides
//!   two paths.
//!
//!   - `OpAtomicFAdd` (capability `AtomicFloat32AddEXT`, extension
//!   `SPV_EXT_shader_atomic_float_add`) — direct hardware atomic on
//!   `f32` (Arc A770 + RTX-50 + RDNA-4 + M3-class). Single instruction,
//!   monotonic-or-stronger ordering.
//!
//!   - CAS-loop emulation : `OpAtomicCompareExchange` on the bit-pattern
//!   of the float (read-modify-write inside a SPIR-V loop). Slower but
//!   universally available on any SPIR-V 1.5+ profile. Used as the
//!   fallback when the AtomicFloat extension is absent.
//!
//!   The CPU-side simulator here mirrors both modes so the SPIR-V emitter
//!   tests can validate equivalence on a per-record basis.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Available atomic strategies for `f32` adjoint accumulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicMode {
    /// Hardware `OpAtomicFAdd` on `f32` (capability `AtomicFloat32AddEXT`).
    NativeFAddF32,
    /// Hardware `OpAtomicFAdd` on `f64` (capability `AtomicFloat64AddEXT`,
    /// not yet a separate cap variant in our catalog ; emitted under
    /// `Float64` + ExtShaderAtomicFloatAdd).
    NativeFAddF64,
    /// CAS-loop emulation via `OpAtomicCompareExchange` (no extra cap).
    CasLoopEmulation,
}

impl AtomicMode {
    /// Required SPIR-V capability for this mode.
    #[must_use]
    pub const fn required_capability(self) -> Option<&'static str> {
        match self {
            Self::NativeFAddF32 => Some("AtomicFloat32AddEXT"),
            Self::NativeFAddF64 => Some("AtomicFloat32AddEXT"), // same cap covers 32 + 64 in EXT
            Self::CasLoopEmulation => None,
        }
    }

    /// Required SPIR-V extension for this mode.
    #[must_use]
    pub const fn required_extension(self) -> Option<&'static str> {
        match self {
            Self::NativeFAddF32 | Self::NativeFAddF64 => {
                Some("SPV_EXT_shader_atomic_float_add")
            }
            Self::CasLoopEmulation => None,
        }
    }

    /// Stable text-form name (used in the SPIR-V emitter's op-attribute
    /// records).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NativeFAddF32 => "native-fadd-f32",
            Self::NativeFAddF64 => "native-fadd-f64",
            Self::CasLoopEmulation => "cas-loop",
        }
    }
}

/// Diagnostic enum returned by [`AtomicAdjointAccumulator::pick_fallback`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicFallback {
    /// Native fast-path is available ; no fallback needed.
    Available,
    /// Native unavailable — caller must use CAS-loop.
    UseCasLoop,
}

/// CPU-side simulator of the atomic-FAdd accumulator. Each accumulator
/// instance simulates one f32 (or f64) atomic cell.
pub struct AtomicAdjointAccumulator {
    /// Bit-pattern of the current value. Uses `AtomicU32` for f32, `AtomicU64`
    /// for f64 ; we discriminate via the `is_f64` flag.
    bits32: AtomicU32,
    bits64: AtomicU64,
    is_f64: bool,
}

impl AtomicAdjointAccumulator {
    /// Construct an f32 accumulator initialized to zero.
    #[must_use]
    pub fn f32() -> Self {
        Self {
            bits32: AtomicU32::new(0),
            bits64: AtomicU64::new(0),
            is_f64: false,
        }
    }

    /// Construct an f64 accumulator initialized to zero.
    #[must_use]
    pub fn f64() -> Self {
        Self {
            bits32: AtomicU32::new(0),
            bits64: AtomicU64::new(0),
            is_f64: true,
        }
    }

    /// True iff this is an f64 accumulator.
    #[must_use]
    pub const fn is_f64(&self) -> bool {
        self.is_f64
    }

    /// Read current value.
    #[must_use]
    pub fn read(&self) -> f64 {
        if self.is_f64 {
            f64::from_bits(self.bits64.load(Ordering::SeqCst))
        } else {
            f64::from(f32::from_bits(self.bits32.load(Ordering::SeqCst)))
        }
    }

    /// Native FAdd (`OpAtomicFAdd`) — single-step atomic add.
    /// On non-supporting hardware, the SPIR-V emitter would route through
    /// [`Self::cas_add`] instead.
    pub fn native_add(&self, delta: f64) {
        if self.is_f64 {
            // Simulated direct add — backing store is RMW via fetch_update.
            self.bits64
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |bits| {
                    let v = f64::from_bits(bits);
                    Some((v + delta).to_bits())
                })
                .ok();
        } else {
            self.bits32
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |bits| {
                    let v = f64::from(f32::from_bits(bits));
                    Some(((v + delta) as f32).to_bits())
                })
                .ok();
        }
    }

    /// CAS-loop emulation. Equivalent semantics to [`Self::native_add`] but
    /// modeled with explicit retry. Both modes converge to the same final
    /// value but the CAS path may be slower under contention.
    pub fn cas_add(&self, delta: f64) {
        if self.is_f64 {
            loop {
                let old_bits = self.bits64.load(Ordering::SeqCst);
                let old_val = f64::from_bits(old_bits);
                let new_val = old_val + delta;
                let new_bits = new_val.to_bits();
                if self
                    .bits64
                    .compare_exchange(
                        old_bits,
                        new_bits,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    return;
                }
            }
        } else {
            loop {
                let old_bits = self.bits32.load(Ordering::SeqCst);
                let old_val = f64::from(f32::from_bits(old_bits));
                let new_val = old_val + delta;
                let new_bits = (new_val as f32).to_bits();
                if self
                    .bits32
                    .compare_exchange(
                        old_bits,
                        new_bits,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    )
                    .is_ok()
                {
                    return;
                }
            }
        }
    }

    /// Mode-dispatched add. Picks `native_add` when mode is native, else CAS.
    pub fn add(&self, delta: f64, mode: AtomicMode) {
        match mode {
            AtomicMode::NativeFAddF32 | AtomicMode::NativeFAddF64 => self.native_add(delta),
            AtomicMode::CasLoopEmulation => self.cas_add(delta),
        }
    }

    /// Reset to zero.
    pub fn reset(&self) {
        self.bits32.store(0, Ordering::SeqCst);
        self.bits64.store(0, Ordering::SeqCst);
    }

    /// Pick the fallback mode for a given target's atomic-FAdd capability.
    /// Used by the SPIR-V emitter when a target advertises absence of the
    /// `AtomicFloat32AddEXT` capability.
    #[must_use]
    pub const fn pick_fallback(has_native_fadd: bool) -> AtomicFallback {
        if has_native_fadd {
            AtomicFallback::Available
        } else {
            AtomicFallback::UseCasLoop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_native_add_round_trip() {
        let a = AtomicAdjointAccumulator::f32();
        a.native_add(1.5);
        a.native_add(0.5);
        let v = a.read();
        assert!((v - 2.0).abs() < 1e-6);
    }

    #[test]
    fn f64_native_add_round_trip() {
        let a = AtomicAdjointAccumulator::f64();
        a.native_add(1.0_f64 / 3.0);
        a.native_add(2.0_f64 / 3.0);
        let v = a.read();
        assert!((v - 1.0).abs() < 1e-12);
    }

    #[test]
    fn cas_add_matches_native_add_for_single_thread() {
        let a = AtomicAdjointAccumulator::f32();
        let b = AtomicAdjointAccumulator::f32();
        for _ in 0..100 {
            a.native_add(0.01);
            b.cas_add(0.01);
        }
        // Both should match within fp tolerance.
        let va = a.read();
        let vb = b.read();
        assert!((va - vb).abs() < 1e-3);
    }

    #[test]
    fn mode_dispatch_picks_native() {
        let a = AtomicAdjointAccumulator::f32();
        a.add(1.0, AtomicMode::NativeFAddF32);
        assert!((a.read() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mode_dispatch_picks_cas() {
        let a = AtomicAdjointAccumulator::f32();
        a.add(2.0, AtomicMode::CasLoopEmulation);
        assert!((a.read() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn pick_fallback_when_native_unavailable() {
        let f = AtomicAdjointAccumulator::pick_fallback(false);
        assert_eq!(f, AtomicFallback::UseCasLoop);
    }

    #[test]
    fn pick_no_fallback_when_native_available() {
        let f = AtomicAdjointAccumulator::pick_fallback(true);
        assert_eq!(f, AtomicFallback::Available);
    }

    #[test]
    fn reset_clears_value() {
        let a = AtomicAdjointAccumulator::f32();
        a.native_add(5.0);
        a.reset();
        assert_eq!(a.read(), 0.0);
    }

    #[test]
    fn native_modes_carry_f32_extension() {
        assert_eq!(
            AtomicMode::NativeFAddF32.required_capability(),
            Some("AtomicFloat32AddEXT")
        );
        assert_eq!(
            AtomicMode::NativeFAddF32.required_extension(),
            Some("SPV_EXT_shader_atomic_float_add")
        );
    }

    #[test]
    fn cas_loop_carries_no_extension() {
        assert_eq!(AtomicMode::CasLoopEmulation.required_capability(), None);
        assert_eq!(AtomicMode::CasLoopEmulation.required_extension(), None);
    }

    #[test]
    fn convergent_multi_thread_cas_add() {
        // Single-threaded simulation of a 16-lane reverse-pass accumulating
        // into the same parameter ; sum of partial-deltas should be exact.
        let a = AtomicAdjointAccumulator::f64();
        let deltas: Vec<f64> = (0..16).map(f64::from).collect();
        for d in &deltas {
            a.cas_add(*d);
        }
        let expected: f64 = deltas.iter().sum();
        assert!((a.read() - expected).abs() < 1e-12);
    }
}

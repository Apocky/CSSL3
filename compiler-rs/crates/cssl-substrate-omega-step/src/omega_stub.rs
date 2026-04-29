//! Minimal Ω-tensor stub for H2 → H1 handoff.
//!
//! § THESIS
//!   S8-H1 lands the canonical Ω-tensor type per `specs/30_SUBSTRATE.csl
//!   § Ω-TENSOR § STRUCTURE`. H2 (this slice) lands the omega_step contract
//!   on top. To allow H2 to proceed without H1 having merged, this module
//!   supplies a **minimal, deliberately stub-shaped** `OmegaSnapshot` that
//!   names the load-bearing fields a system might mutate during a step.
//!
//!   The H1 replacement strategy :
//!   - When H1 lands, this `OmegaSnapshot` becomes a thin re-export shim
//!     that wraps `cssl_substrate_omega_tensor::Omega`.
//!   - Field names below match the spec entries (`world`, `scene`,
//!     `projections`, `sim`, `audio`, `net`, `save`, `telemetry`, `audit`)
//!     so call-sites are byte-identical across the swap.
//!
//! § WHAT THIS STUB DOES NOT DO
//!   - No real ECS / archetype-pool — `world` is a `BTreeMap<u64, OmegaStubField>`
//!     in stub form. H1 replaces with the canonical archetype-pool surface.
//!   - No real GPU resources — `scene` is a stub. H1 wires the SDF-grid + lights.
//!   - No real audio DSP graph — `audio` is a stub.
//!   - No real network session — `net` is `None` in stub form (DEFERRED).
//!   - No real save journal — `save` is a stub.
//!   - No real telemetry ring — `telemetry` is a counter ; H2 routes to
//!     `cssl-telemetry::ring::TelemetryRing` directly.
//!
//! § DETERMINISM
//!   The stub uses `BTreeMap` (NOT `HashMap`) so iteration order is
//!   deterministic. Replay-tests rely on this — swapping to `HashMap` here
//!   would silently break `OmegaScheduler::replay_from(log)` bit-equality.

use std::collections::BTreeMap;

/// A simple typed-value field used by the stub Ω-tensor. Real H1 replaces
/// this with the full `Omega` struct (world / scene / sim / projections / …).
#[derive(Debug, Clone, PartialEq)]
pub enum OmegaStubField {
    /// Integer field — used by tests for counters + accumulators.
    Int(i64),
    /// Float field — used by tests for physics-step accumulators. Stage-0
    /// uses `f64` because that's what `omega_step(dt: f64)` provides.
    Float(f64),
    /// String field — used by tests for halt-reason / replay-marker tags.
    Text(String),
    /// Byte-buffer field — used by tests that snapshot multi-byte state.
    Bytes(Vec<u8>),
}

impl OmegaStubField {
    /// Bit-equality compare. Required for replay-determinism tests :
    /// `f64` `PartialEq` returns `false` for `NaN == NaN` ; the bit-equality
    /// form returns `true` when the IEEE-754 byte-pattern matches exactly.
    #[must_use]
    pub fn bit_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            _ => false,
        }
    }
}

/// Minimal snapshot of Ω-tensor state for stage-0 H2 testing.
///
/// § Determinism : `BTreeMap` chosen over `HashMap` so iteration order is
///   reproducible across runs (replay-determinism contract).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OmegaSnapshot {
    /// Monotonic tick counter. Incremented by `OmegaScheduler::step()`.
    pub epoch: u64,
    /// Stub of the canonical `world` field — keyed by entity-id.
    pub world: BTreeMap<u64, OmegaStubField>,
    /// Stub of the canonical `scene` field — keyed by name.
    pub scene: BTreeMap<String, OmegaStubField>,
    /// Stub of the canonical `sim` field — keyed by name.
    pub sim: BTreeMap<String, OmegaStubField>,
    /// Stub of the canonical `audio` field — keyed by channel-id.
    pub audio: BTreeMap<u32, OmegaStubField>,
    /// Stub of the canonical `save` field — keyed by save-slot-id.
    pub save: BTreeMap<u64, OmegaStubField>,
    /// Stub kill-flag. Real H1 replaces this with the `iso<KillToken>`
    /// linear-token machinery from `specs/30 § ω_halt()`.
    pub kill_consumed: bool,
    /// Stub halt-reason. Populated by `OmegaScheduler::halt(reason)` ;
    /// observed by `OmegaSystem::step()` via the ctx halt-flag.
    pub halt_reason: Option<String>,
}

impl OmegaSnapshot {
    /// Construct a fresh, empty snapshot with `epoch = 0`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bit-equality compare across every field. Used by replay-tests to
    /// assert that two scheduler runs produced byte-identical state.
    #[must_use]
    pub fn bit_eq(&self, other: &Self) -> bool {
        if self.epoch != other.epoch
            || self.kill_consumed != other.kill_consumed
            || self.halt_reason != other.halt_reason
        {
            return false;
        }
        Self::map_bit_eq_u64(&self.world, &other.world)
            && Self::map_bit_eq_str(&self.scene, &other.scene)
            && Self::map_bit_eq_str(&self.sim, &other.sim)
            && Self::map_bit_eq_u32(&self.audio, &other.audio)
            && Self::map_bit_eq_u64(&self.save, &other.save)
    }

    fn map_bit_eq_u64(
        a: &BTreeMap<u64, OmegaStubField>,
        b: &BTreeMap<u64, OmegaStubField>,
    ) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .zip(b.iter())
            .all(|((ka, va), (kb, vb))| ka == kb && va.bit_eq(vb))
    }

    fn map_bit_eq_u32(
        a: &BTreeMap<u32, OmegaStubField>,
        b: &BTreeMap<u32, OmegaStubField>,
    ) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .zip(b.iter())
            .all(|((ka, va), (kb, vb))| ka == kb && va.bit_eq(vb))
    }

    fn map_bit_eq_str(
        a: &BTreeMap<String, OmegaStubField>,
        b: &BTreeMap<String, OmegaStubField>,
    ) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .zip(b.iter())
            .all(|((ka, va), (kb, vb))| ka == kb && va.bit_eq(vb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_bit_eq_distinguishes_nan_payload() {
        // Two distinct NaN payloads — bit_eq must distinguish them.
        let a = OmegaStubField::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let b = OmegaStubField::Float(f64::from_bits(0x7ff8_0000_0000_0002));
        assert!(!a.bit_eq(&b));
    }

    #[test]
    fn float_bit_eq_treats_identical_nan_as_equal() {
        let a = OmegaStubField::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let b = OmegaStubField::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        assert!(a.bit_eq(&b));
    }

    #[test]
    fn snapshot_default_empty() {
        let s = OmegaSnapshot::new();
        assert_eq!(s.epoch, 0);
        assert!(s.world.is_empty());
        assert!(s.sim.is_empty());
        assert!(!s.kill_consumed);
    }

    #[test]
    fn snapshot_bit_eq_self() {
        let s = OmegaSnapshot::new();
        assert!(s.bit_eq(&s));
    }

    #[test]
    fn snapshot_bit_eq_detects_world_drift() {
        let mut a = OmegaSnapshot::new();
        let mut b = OmegaSnapshot::new();
        a.world.insert(0, OmegaStubField::Int(1));
        b.world.insert(0, OmegaStubField::Int(2));
        assert!(!a.bit_eq(&b));
    }

    #[test]
    fn snapshot_btreemap_iter_deterministic() {
        // Insert in scrambled order ; iterate ; expect sorted order.
        let mut s = OmegaSnapshot::new();
        s.sim.insert("z".into(), OmegaStubField::Int(3));
        s.sim.insert("a".into(), OmegaStubField::Int(1));
        s.sim.insert("m".into(), OmegaStubField::Int(2));
        let keys: Vec<&String> = s.sim.keys().collect();
        assert_eq!(keys, [&"a".to_string(), &"m".to_string(), &"z".to_string()]);
    }
}

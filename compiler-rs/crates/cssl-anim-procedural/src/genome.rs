//! § Genome embedding + control signal — KAN pose-network inputs.
//!
//! § THESIS
//!   The procedural pose-network's contract is :
//!     `KAN(genome_embedding, time, control_signal) → bone-local Transforms`
//!
//!   - **`genome_embedding`** — a 32-D float vector that captures the
//!     creature's stable identity. Drawn from the
//!     `KanMaterial::creature_morphology` 32-D embedding (see
//!     [`GenomeHandle::embedding`]). Stable for the creature's lifetime ;
//!     does not change under wave-field deformation or behavior decisions.
//!
//!   - **`time`** — wall-clock or simulation-time. Time enters the network
//!     as a phase-encoded vector (sin/cos at multiple frequencies) so the
//!     KAN can produce smooth periodic motion (idle breathing, gait
//!     cycles, blinking) without learning explicit clocks. See
//!     [`encode_time_phase`].
//!
//!   - **`control_signal`** — a small (8-D default) vector representing
//!     the creature's current behavior intent : "walk forward at 0.7
//!     speed", "look up", "raise left arm". The behavior-priors KAN
//!     (Axiom 10 § VII active inference) emits this signal ; the pose-
//!     KAN consumes it.
//!
//! § DETERMINISM
//!   Every input here is byte-stable. Same `(genome_handle, time,
//!   control_signal)` triple produces bit-identical pose output.

use cssl_substrate_kan::{KanMaterial, KanMaterialKind};

/// Fixed dimensionality of the creature genome embedding. Matches the
/// substrate-spec `KanMaterial::EMBEDDING_DIM = 32`.
pub const GENOME_DIM: usize = 32;

/// Default dimensionality of the control signal (behavior-prior output).
/// Stage-0 default ; a richer signal (per-limb intent + emotional
/// modulation) lands once the behavior-priors KAN graduates.
pub const CONTROL_SIGNAL_DIM_DEFAULT: usize = 8;

/// Number of frequency bands used by the time-phase encoder. Each band
/// contributes `(sin, cos)` to the network input ; total contribution is
/// `2 * TIME_PHASE_BANDS` floats.
pub const TIME_PHASE_BANDS: usize = 4;

/// A genome embedding suitable for input to the pose KAN. Wraps a fixed-
/// size 32-D float array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenomeEmbedding {
    /// The 32-D embedding values.
    pub values: [f32; GENOME_DIM],
}

impl Default for GenomeEmbedding {
    fn default() -> Self {
        Self::ZERO
    }
}

impl GenomeEmbedding {
    /// All-zero embedding — a sentinel for untrained / uninitialized
    /// creatures.
    pub const ZERO: Self = Self {
        values: [0.0; GENOME_DIM],
    };

    /// Construct from an explicit value array.
    #[must_use]
    pub const fn from_values(values: [f32; GENOME_DIM]) -> Self {
        Self { values }
    }

    /// Fingerprint-like hash of the embedding bytes. Used by the
    /// world-level identity check : two creatures with identical genomes
    /// produce identical poses ; this hash is the cache key.
    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        // Deterministic 64-bit FNV-1a over the raw bytes. Simple +
        // total ; matches the determinism-discipline of the substrate.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for v in &self.values {
            for b in v.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }

    /// Pull the embedding from a KanMaterial (the spec's canonical
    /// genome carrier). Returns `None` if the material is not a
    /// `creature_morphology` variant (other variants don't carry a
    /// genome-shaped embedding).
    #[must_use]
    pub fn from_kan_material(m: &KanMaterial) -> Option<Self> {
        match m.kind {
            KanMaterialKind::CreatureMorphology => Some(Self {
                values: m.embedding,
            }),
            _ => None,
        }
    }
}

/// Stable handle to a genome embedding. Carries an embedding + an opaque
/// id that downstream consumers can use to cache pose evaluations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenomeHandle {
    /// Caller-assigned identifier. Stable for the genome's lifetime.
    pub id: u64,
    /// The 32-D embedding values.
    pub embedding: GenomeEmbedding,
}

impl GenomeHandle {
    /// Construct from id + embedding.
    #[must_use]
    pub const fn new(id: u64, embedding: GenomeEmbedding) -> Self {
        Self { id, embedding }
    }

    /// Get the embedding values.
    #[must_use]
    pub fn embedding(&self) -> &GenomeEmbedding {
        &self.embedding
    }
}

/// Behavior-control signal : a short float vector representing the
/// creature's current "intent". Conventional encoding (stage-0) :
///   - `[0]` : forward locomotion speed in `[-1, 1]` (back / forward).
///   - `[1]` : strafe speed in `[-1, 1]` (left / right).
///   - `[2]` : turn rate in `[-1, 1]` (rotational intent).
///   - `[3]` : crouch / stand axis in `[0, 1]`.
///   - `[4]` : look pitch in `[-1, 1]`.
///   - `[5]` : look yaw in `[-1, 1]`.
///   - `[6]` : reach amount (arm extension intent) in `[0, 1]`.
///   - `[7]` : breathing-amplitude scalar in `[0, 1]`.
///
/// Higher-dimensional signals are supported by sizing the inner vector
/// appropriately ; the pose-KAN's input layer is sized to match at
/// construction.
#[derive(Debug, Clone, Default)]
pub struct ControlSignal {
    /// The control values.
    values: Vec<f32>,
}

impl ControlSignal {
    /// All-zero control signal of dimension `dim` (no intent).
    #[must_use]
    pub fn zero(dim: usize) -> Self {
        Self {
            values: vec![0.0; dim],
        }
    }

    /// Construct from an explicit vector.
    #[must_use]
    pub fn from_values(values: Vec<f32>) -> Self {
        Self { values }
    }

    /// Construct a stage-0 default signal of dimension 8.
    #[must_use]
    pub fn stage0_default() -> Self {
        Self::zero(CONTROL_SIGNAL_DIM_DEFAULT)
    }

    /// Read-only access to the underlying vector.
    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Mutable access (rarely needed ; behavior-priors typically
    /// reconstructs the signal each tick).
    pub fn values_mut(&mut self) -> &mut [f32] {
        &mut self.values
    }

    /// Dimensionality.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.values.len()
    }

    /// Set a single component, clamped to `[-1, 1]` for the conventional
    /// channels and to `[0, 1]` for crouch/breathing/reach. Caller-side
    /// channel-aware clamping is the canonical path ; this helper is for
    /// quick experiments only.
    pub fn set_component(&mut self, idx: usize, value: f32) {
        if let Some(slot) = self.values.get_mut(idx) {
            *slot = value;
        }
    }

    /// Forward locomotion speed in `[-1, 1]`. Channel 0.
    #[must_use]
    pub fn forward_speed(&self) -> f32 {
        self.values.first().copied().unwrap_or(0.0).clamp(-1.0, 1.0)
    }

    /// Reach amount in `[0, 1]`. Channel 6.
    #[must_use]
    pub fn reach(&self) -> f32 {
        self.values.get(6).copied().unwrap_or(0.0).clamp(0.0, 1.0)
    }

    /// Breathing amplitude in `[0, 1]`. Channel 7.
    #[must_use]
    pub fn breathing(&self) -> f32 {
        self.values.get(7).copied().unwrap_or(0.0).clamp(0.0, 1.0)
    }
}

/// Encode a scalar time `t` into a phase-encoded vector of length
/// `2 * TIME_PHASE_BANDS`. Each band `k` (0-indexed) contributes
/// `(sin(t * 2^k * 2π), cos(t * 2^k * 2π))` so the network sees the same
/// time at multiple temporal frequencies — the foundation for smooth
/// periodic motion (gait, breathing, idle sway).
#[must_use]
pub fn encode_time_phase(t: f32) -> [f32; 2 * TIME_PHASE_BANDS] {
    let mut out = [0.0; 2 * TIME_PHASE_BANDS];
    let two_pi = std::f32::consts::TAU;
    for k in 0..TIME_PHASE_BANDS {
        let freq = (1u32 << k) as f32;
        let phase = t * freq * two_pi;
        out[2 * k] = phase.sin();
        out[2 * k + 1] = phase.cos();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genome_zero_is_default() {
        let g = GenomeEmbedding::default();
        for v in g.values {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn genome_dim_is_32() {
        assert_eq!(GENOME_DIM, 32);
    }

    #[test]
    fn stable_hash_is_deterministic() {
        let g = GenomeEmbedding::from_values([0.5; GENOME_DIM]);
        let h1 = g.stable_hash();
        let h2 = g.stable_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn stable_hash_distinguishes_different_genomes() {
        let g1 = GenomeEmbedding::from_values([0.5; GENOME_DIM]);
        let g2 = GenomeEmbedding::from_values([0.6; GENOME_DIM]);
        assert_ne!(g1.stable_hash(), g2.stable_hash());
    }

    #[test]
    fn from_kan_material_creature_morphology_succeeds() {
        let m = KanMaterial::creature_morphology([0.25; GENOME_DIM]);
        let g = GenomeEmbedding::from_kan_material(&m).expect("creature_morphology variant");
        assert_eq!(g.values, [0.25; GENOME_DIM]);
    }

    #[test]
    fn from_kan_material_other_variant_returns_none() {
        let m = KanMaterial::single_band_brdf([0.0; GENOME_DIM]);
        assert!(GenomeEmbedding::from_kan_material(&m).is_none());
    }

    #[test]
    fn control_signal_zero_dim_zero_values() {
        let s = ControlSignal::zero(8);
        assert_eq!(s.dim(), 8);
        for v in s.values() {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn control_signal_set_component() {
        let mut s = ControlSignal::zero(8);
        s.set_component(0, 0.5);
        assert_eq!(s.forward_speed(), 0.5);
    }

    #[test]
    fn control_signal_forward_speed_clamps() {
        let mut s = ControlSignal::zero(8);
        s.set_component(0, 5.0);
        assert_eq!(s.forward_speed(), 1.0);
    }

    #[test]
    fn control_signal_reach_clamps_negative() {
        let mut s = ControlSignal::zero(8);
        s.set_component(6, -0.5);
        assert_eq!(s.reach(), 0.0);
    }

    #[test]
    fn time_phase_zero_is_canonical() {
        let p = encode_time_phase(0.0);
        // sin(0) = 0, cos(0) = 1 across all bands.
        for k in 0..TIME_PHASE_BANDS {
            assert!((p[2 * k]).abs() < 1e-6);
            assert!((p[2 * k + 1] - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn time_phase_periodic_at_unit_time() {
        // sin(2π) = 0 ≈ sin(0) ; cos(2π) = 1 ≈ cos(0). Lowest band only.
        let p = encode_time_phase(1.0);
        assert!(p[0].abs() < 1e-4);
        assert!((p[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn time_phase_higher_band_oscillates_faster() {
        // At t = 0.25, the lowest band is at quarter-period (sin ≈ 1)
        // while band 1 (frequency 2) is at half-period (sin ≈ 0).
        let p = encode_time_phase(0.25);
        assert!((p[0] - 1.0).abs() < 1e-3);
        assert!(p[2].abs() < 1e-3);
    }

    #[test]
    fn genome_handle_carries_id() {
        let h = GenomeHandle::new(42, GenomeEmbedding::ZERO);
        assert_eq!(h.id, 42);
    }

    #[test]
    fn control_signal_default_dim_is_eight() {
        let s = ControlSignal::stage0_default();
        assert_eq!(s.dim(), CONTROL_SIGNAL_DIM_DEFAULT);
    }
}

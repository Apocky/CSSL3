//! § bias — KAN-bias-vector + Σ-mask consent-gate
//!
//! Per W13-8 spec :
//!   - KAN-bias-vector tunes-rarity-distribution toward aesthetic preference
//!   - Σ-mask-gated · default-deny · player-opts-in
//!   - Bias-update-cap : ¬ runaway-amplification
//!
//! ## Default-deny (Σ-mask-gating)
//!
//! [`KanBiasConsent::denied`] is the default. While consent is denied,
//! [`apply_bias_to_distribution`] is the identity — the public drop-rates
//! are returned unchanged. The player must explicitly opt-in via a
//! [`KanBiasConsent::granted`]-bearing token (carrying a non-zero session-hash)
//! to receive bias-modulated rolls.
//!
//! ## Update-cap
//!
//! Per-call delta is clamped to [`MAX_BIAS_DELTA`] regardless of input
//! magnitude. This blocks runaway amplification even if the upstream KAN
//! emits an unbounded bias vector.

use serde::{Deserialize, Serialize};

use crate::distribution::DropRateDistribution;

/// Dimensionality of the KAN-bias vector — one bias-weight per rarity tier.
pub const BIAS_DIM: usize = 6;

/// Hard-cap on per-rarity bias-delta to prevent runaway amplification.
/// Bias `v[i]` is clamped to `[-MAX_BIAS_DELTA, +MAX_BIAS_DELTA]` BEFORE
/// applying to the base distribution.
pub const MAX_BIAS_DELTA: f32 = 0.05;

// ───────────────────────────────────────────────────────────────────────
// § KanBiasConsent
// ───────────────────────────────────────────────────────────────────────

/// Σ-mask consent-gate for KAN-bias application. Default-deny.
///
/// Constructed via [`KanBiasConsent::denied`] (default) or
/// [`KanBiasConsent::granted`] (player explicit opt-in). The [`session_hash`]
/// field carries the consent-token from the UI flow ; zero is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KanBiasConsent {
    /// True iff player has opted-in. Default-deny means false.
    pub granted: bool,
    /// Consent-token from the UI flow ; zero rejected (matches Σ-Chain
    /// convention — see `cssl_host_sigma_chain`).
    pub session_hash: u64,
}

impl KanBiasConsent {
    /// Default-deny consent. KAN-bias will NOT be applied.
    #[must_use]
    pub const fn denied() -> Self {
        Self { granted: false, session_hash: 0 }
    }

    /// Granted consent with a non-zero session-hash. Zero hash collapses to
    /// `denied()` to prevent accidental opt-in via uninitialized memory.
    #[must_use]
    pub const fn granted(session_hash: u64) -> Self {
        if session_hash == 0 {
            return Self::denied();
        }
        Self { granted: true, session_hash }
    }

    /// True iff this consent permits bias application.
    #[must_use]
    pub const fn permits(&self) -> bool {
        self.granted && self.session_hash != 0
    }
}

impl Default for KanBiasConsent {
    fn default() -> Self {
        Self::denied()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § KanBiasVector
// ───────────────────────────────────────────────────────────────────────

/// Per-rarity bias-weight vector. `weights[i]` is the additive shift applied
/// to the base-rate for rarity `i` (in canonical `[Common..Mythic]` order).
///
/// Each weight is clamped to `[-MAX_BIAS_DELTA, +MAX_BIAS_DELTA]` BEFORE
/// application — see [`Self::clamped`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KanBiasVector {
    /// Bias-weights indexed by [`cssl_host_gear_archetype::Rarity::all`] order.
    pub weights: [f32; BIAS_DIM],
}

impl KanBiasVector {
    /// Zero-bias vector — applying yields the input distribution unchanged.
    #[must_use]
    pub const fn zero() -> Self {
        Self { weights: [0.0; BIAS_DIM] }
    }

    /// Construct from explicit weights (will be clamped on apply).
    #[must_use]
    pub const fn new(weights: [f32; BIAS_DIM]) -> Self {
        Self { weights }
    }

    /// Returns a copy with each weight clamped to `[-MAX_BIAS_DELTA, +MAX_BIAS_DELTA]`.
    /// NaN sanitized to 0.0.
    #[must_use]
    pub fn clamped(&self) -> Self {
        let mut out = [0.0_f32; BIAS_DIM];
        for (i, w) in self.weights.iter().enumerate() {
            let safe = if w.is_nan() { 0.0 } else { *w };
            out[i] = safe.clamp(-MAX_BIAS_DELTA, MAX_BIAS_DELTA);
        }
        Self { weights: out }
    }

    /// Apply this bias to a base-distribution under the consent-gate.
    ///
    /// **Behavior** :
    ///   - If `consent.permits()` is `false` → returns `base` unchanged
    ///     (Σ-mask default-deny).
    ///   - Otherwise → adds clamped weights to base-rates, floors at 0.0,
    ///     renormalizes so sum = 1.0.
    ///
    /// The output distribution always preserves the public-curve **shape**
    /// (Common ≥ Uncommon ≥ Rare ... usually) but exact rates shift within
    /// the cap budget.
    #[must_use]
    pub fn apply_to(&self, base: &DropRateDistribution, consent: &KanBiasConsent) -> DropRateDistribution {
        if !consent.permits() {
            return *base;
        }
        let clamped = self.clamped();
        let mut rates = base.rates;
        for (i, w) in clamped.weights.iter().enumerate() {
            rates[i] = (rates[i] + *w).max(0.0);
        }
        DropRateDistribution { rates }.renormalized()
    }
}

impl Default for KanBiasVector {
    fn default() -> Self {
        Self::zero()
    }
}

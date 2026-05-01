// § cocreative_overlay.rs — L4 layer ; player-attuned reaction-style
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § COCREATIVE-BIAS-OVERLAY
// § I> reads ONLY 16-D bias-vec (norm) from cssl-host-cocreative
// § I> NEVER reads Sensitive<biometric|gaze|face|body> — STRUCTURALLY-banned (SIG0003)
// § I> mood-mod : f(reputation) → terse|warm|poetic|aloof
// ════════════════════════════════════════════════════════════════════

use crate::DetRng;
use crate::audit::{AuditEvent, AuditSink, kind};
use serde::{Deserialize, Serialize};

/// NPC mood-color produced by reputation modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mood {
    /// Player-rep low → curt, short responses.
    Terse,
    /// Neutral rep → ordinary tone.
    Plain,
    /// Positive rep → friendly, expansive.
    Warm,
    /// High awe / wonder rep → flowery, poetic.
    Poetic,
    /// Strongly negative → withdrawn, distant.
    Aloof,
}

/// Marker error : caller attempted to feed Sensitive<*> input into the overlay.
///
/// § I> The overlay's *types* don't accept Sensitive<*> — this enum only fires
/// at the runtime-detect layer of the SIG0003 gate (compile-time gate is
/// the structural absence of those input types from this module's signatures).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensitiveScopeViolation {
    /// Biometric input attempted.
    Biometric,
    /// Gaze-tracking input attempted.
    Gaze,
    /// Face-recognition input attempted.
    Face,
    /// Body-pose tracking input attempted.
    Body,
}

impl SensitiveScopeViolation {
    /// Stable string-id for audit-attribs.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            SensitiveScopeViolation::Biometric => "biometric",
            SensitiveScopeViolation::Gaze => "gaze",
            SensitiveScopeViolation::Face => "face",
            SensitiveScopeViolation::Body => "body",
        }
    }
}

/// Pool-weighted-select over dialogue-id pool, biased by 16-D bias-vec.
///
/// § I> bias is dotted with a one-hot pool-position-vector ; weights normalized
///   then sampled via inverse-CDF on `rng`.
/// § I> Returns 0 on empty pool (caller-must-validate-non-empty).
pub fn bias_modulate_dialogue_choice(
    pool: &[u32],
    bias: &[f32; 16],
    rng: &mut DetRng,
) -> u32 {
    if pool.is_empty() {
        return 0;
    }
    // Map each pool-entry to a non-negative weight via bias-vector indexing.
    let mut weights: Vec<f32> = Vec::with_capacity(pool.len());
    let mut total = 0.0_f32;
    for (i, _) in pool.iter().enumerate() {
        // Index modulo 16 — bias-channel cycles for pools > 16.
        let w = bias[i % 16].max(0.0) + 1e-3; // floor avoids zero-total
        weights.push(w);
        total += w;
    }
    let r = rng.next_f32() * total;
    let mut acc = 0.0_f32;
    for (i, w) in weights.iter().enumerate() {
        acc += *w;
        if r <= acc {
            return pool[i];
        }
    }
    pool[pool.len() - 1]
}

/// Map reputation ∈ [-1, +1] → Mood.
///
/// § I> reputation is from cssl-host-cocreative ; ¬ raw-player-telemetry.
#[must_use]
pub fn bias_mood_color(reputation: f32) -> Mood {
    let r = reputation.clamp(-1.0, 1.0);
    if r < -0.66 {
        Mood::Aloof
    } else if r < -0.20 {
        Mood::Terse
    } else if r < 0.20 {
        Mood::Plain
    } else if r < 0.66 {
        Mood::Warm
    } else {
        Mood::Poetic
    }
}

/// Runtime-side SIG0003 gate : caller passes a flagged Sensitive-input attempt ;
/// overlay rejects + emits `npc.scope_violation`.
///
/// § I> Type-level gate is the absence of those input-channels from this
/// module's public API ; this fn handles the runtime-detect path only.
pub fn record_scope_violation(v: SensitiveScopeViolation, sink: &dyn AuditSink) {
    sink.emit(
        AuditEvent::bare(kind::SCOPE_VIOLATION)
            .with("sig", "SIG0003")
            .with("input", v.tag()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pool_returns_zero() {
        let mut r = DetRng::new(1);
        let b = [1.0_f32; 16];
        assert_eq!(bias_modulate_dialogue_choice(&[], &b, &mut r), 0);
    }

    #[test]
    fn deterministic_select() {
        let pool = [10_u32, 20, 30];
        let b = [1.0_f32; 16];
        let mut r1 = DetRng::new(42);
        let mut r2 = DetRng::new(42);
        for _ in 0..32 {
            assert_eq!(
                bias_modulate_dialogue_choice(&pool, &b, &mut r1),
                bias_modulate_dialogue_choice(&pool, &b, &mut r2)
            );
        }
    }

    #[test]
    fn mood_buckets_correct() {
        assert_eq!(bias_mood_color(-0.9), Mood::Aloof);
        assert_eq!(bias_mood_color(-0.4), Mood::Terse);
        assert_eq!(bias_mood_color(0.0), Mood::Plain);
        assert_eq!(bias_mood_color(0.4), Mood::Warm);
        assert_eq!(bias_mood_color(0.9), Mood::Poetic);
    }
}

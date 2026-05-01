// § status_effects.rs — 16 status-effects + stack-policy (per GDD § STATUS-EFFECTS)
// ════════════════════════════════════════════════════════════════════
// § I> 16 effects exact ; FROZEN-set
// § I> stack-policy : RefreshDuration | AddDuration | AddIntensity | Independent
// § I> Petrify : Freeze-stack ≥ 3 ⇒ Petrify (caller orchestrates promotion)
// § I> Charm-vs-Sovereign FORBIDDEN per PRIME-DIRECTIVE ; caller gates externally
// § I> tick decrements duration ; effects with ≤0 dur removed
// § I> ¬ panic ; saturating arithmetic
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// 16 status-effects ; matches GDD enum exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StatusEffect {
    Bleed,
    Burn,
    Freeze,
    Stun,
    Slow,
    Poison,
    Curse,
    Charm,
    Sleep,
    Petrify,
    Soaked,
    ShockVulnerable,
    Marked,
    Resolve,
    Berserk,
    Phased,
}

/// Stack-policy when same-kind effect is applied again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StackPolicy {
    /// Refresh duration to new value (default).
    RefreshDuration,
    /// Add to existing duration (Bleed, Burn, Poison).
    AddDuration,
    /// Increase magnitude ; duration uses max (Curse, Marked).
    AddIntensity,
    /// Independent instances coexist (rare — multiple Bleed sources).
    Independent,
}

impl StatusEffect {
    /// Default stack-policy per GDD § DURATION-RULES.
    #[must_use]
    pub const fn default_stack_policy(self) -> StackPolicy {
        match self {
            Self::Bleed | Self::Burn | Self::Poison => StackPolicy::AddDuration,
            Self::Curse | Self::Marked => StackPolicy::AddIntensity,
            _ => StackPolicy::RefreshDuration,
        }
    }
}

/// Live status-effect instance on an actor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatusInstance {
    pub kind: StatusEffect,
    /// Remaining duration in seconds (NOT ticks ; dt-driven).
    pub duration_secs: f32,
    /// Magnitude (e.g. damage-per-tick for DoTs ; stack-count for AddIntensity).
    pub magnitude: f32,
}

impl StatusInstance {
    /// Construct a new instance with NaN/negative-clamping.
    #[must_use]
    pub fn new(kind: StatusEffect, duration_secs: f32, magnitude: f32) -> Self {
        let dur = if duration_secs.is_finite() {
            duration_secs.max(0.0)
        } else {
            0.0
        };
        let mag = if magnitude.is_finite() {
            magnitude.max(0.0)
        } else {
            0.0
        };
        Self {
            kind,
            duration_secs: dur,
            magnitude: mag,
        }
    }
}

/// Apply stack-policy when adding a new instance to an existing effect-vec.
///
/// Returns `true` iff a new instance was pushed ; otherwise existing was merged.
/// Caller passes the new instance ; we mutate the vec accordingly.
pub fn apply_with_policy(effects: &mut Vec<StatusInstance>, new_inst: StatusInstance) -> bool {
    let policy = new_inst.kind.default_stack_policy();
    match policy {
        StackPolicy::Independent => {
            effects.push(new_inst);
            true
        }
        _ => {
            if let Some(existing) = effects.iter_mut().find(|e| e.kind == new_inst.kind) {
                match policy {
                    StackPolicy::RefreshDuration => {
                        existing.duration_secs = new_inst.duration_secs.max(existing.duration_secs);
                        existing.magnitude = new_inst.magnitude.max(existing.magnitude);
                    }
                    StackPolicy::AddDuration => {
                        existing.duration_secs += new_inst.duration_secs;
                    }
                    StackPolicy::AddIntensity => {
                        existing.magnitude += new_inst.magnitude;
                        existing.duration_secs = existing.duration_secs.max(new_inst.duration_secs);
                    }
                    StackPolicy::Independent => unreachable!("handled above"),
                }
                false
            } else {
                effects.push(new_inst);
                true
            }
        }
    }
}

/// Tick all status-effects forward by `dt` seconds ; remove expired ones.
/// Petrify-promotion : if Freeze magnitude ≥ 3.0, promote to Petrify (caller
/// observes via post-state vec contents).
pub fn tick_status(effects: &mut Vec<StatusInstance>, dt: f32) {
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    for e in effects.iter_mut() {
        e.duration_secs = (e.duration_secs - dt).max(0.0);
    }
    // Petrify promotion : Freeze stack ≥ 3 ⇒ Petrify
    let mut promote = false;
    if let Some(freeze) = effects.iter().find(|e| e.kind == StatusEffect::Freeze) {
        if freeze.magnitude >= 3.0 && freeze.duration_secs > 0.0 {
            promote = true;
        }
    }
    if promote {
        // Remove freeze instance(s) ; insert/refresh Petrify
        let petrify_dur = effects
            .iter()
            .filter(|e| e.kind == StatusEffect::Freeze)
            .map(|e| e.duration_secs)
            .fold(0.0_f32, f32::max);
        effects.retain(|e| e.kind != StatusEffect::Freeze);
        let p = StatusInstance::new(StatusEffect::Petrify, petrify_dur, 1.0);
        apply_with_policy(effects, p);
    }
    // Remove expired
    effects.retain(|e| e.duration_secs > 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_duration_for_bleed() {
        let mut v = vec![StatusInstance::new(StatusEffect::Bleed, 5.0, 2.0)];
        let pushed = apply_with_policy(&mut v, StatusInstance::new(StatusEffect::Bleed, 3.0, 1.0));
        assert!(!pushed);
        assert_eq!(v.len(), 1);
        assert!((v[0].duration_secs - 8.0).abs() < 1e-3);
    }

    #[test]
    fn refresh_duration_for_stun() {
        let mut v = vec![StatusInstance::new(StatusEffect::Stun, 1.0, 1.0)];
        let _ = apply_with_policy(&mut v, StatusInstance::new(StatusEffect::Stun, 3.0, 1.0));
        assert_eq!(v.len(), 1);
        assert!((v[0].duration_secs - 3.0).abs() < 1e-3);
    }

    #[test]
    fn tick_removes_expired() {
        let mut v = vec![
            StatusInstance::new(StatusEffect::Burn, 0.5, 5.0),
            StatusInstance::new(StatusEffect::Curse, 10.0, 1.0),
        ];
        tick_status(&mut v, 1.0);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, StatusEffect::Curse);
    }
}

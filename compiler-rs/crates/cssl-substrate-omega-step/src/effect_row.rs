//! Substrate-specific effect-row : `{Sim}` base + `{Render}`/`{Audio}`/`{Net}`/`{Save}`
//! /`{Replay}` augmentations per `specs/30_SUBSTRATE.csl § EFFECT-ROWS § SUBSTRATE-EFFECTS`.
//!
//! § THESIS
//!   The 28-effect built-in set in `specs/04_EFFECTS.csl` is the universe
//!   of effects the compiler knows about. The Substrate adds 6 more via
//!   `specs/30_SUBSTRATE.csl § SUBSTRATE-EFFECTS` :
//!     `{Sim}` `{Render}` `{Audio}` `{Net}` `{Save}` `{Replay}`
//!   plus consumes the existing `{Telemetry<scope>}` family.
//!
//!   This module declares those 6 + an `EffectRow` aggregator that
//!   `OmegaSystem::effect_row()` returns. The aggregator is the surface
//!   the scheduler inspects to decide :
//!     - which RNG-stream-class to seed (replay-mode only? PureDet?)
//!     - whether the system is run on the audio-callback thread
//!     - whether `{Net}` is permitted (consent-token check)
//!     - whether `{Save}` is permitted (consent-token check)
//!
//! § COMPOSITION RULES (mirror `specs/30 § COMPOSITION-RULES`)
//!   - `{Sim}` is always the base. A system without `{Sim}` is rejected
//!     at registration with `OmegaError::DeterminismViolation { kind: "no-Sim" }`.
//!   - `{Sim} ⊎ {Render}` permitted ; render reads frozen-sim-val.
//!   - `{Sim} ⊎ {Audio}` permitted but the system is hoisted onto the
//!     audio-callback fiber per `specs/30 § PHASES`.
//!   - `{Net} ⊎ {PureDet}` rejected unless the row also carries `{Replay}`
//!     (replay-mode replaces network with recorded-trace).
//!   - `{Save}` always implies `{Audit<"save-journal">}` ; the scheduler
//!     drops a synthetic audit-entry on every step where any system
//!     declared `{Save}`.
//!
//! § FORBIDDEN COMPOSITIONS (mirror `specs/30 § FORBIDDEN-COMPOSITIONS`)
//!   These are HARD compile-error-shape rejects ; the scheduler refuses
//!   to register the system :
//!     - `{Sim} ⊎ {Sensitive<"weapon">}` — absolute compile-error
//!     - `{Render} ⊎ {Sensitive<"surveillance">}` — absolute
//!     - `{Net} ⊎ {Sensitive<"surveillance">}` — absolute
//!   No flag, no env-var, no override permits these. PRIME_DIRECTIVE §1.

use std::fmt;

/// The 6 Substrate-specific effects from `specs/30_SUBSTRATE.csl § SUBSTRATE-EFFECTS`.
///
/// § Discriminants are STABLE from S8-H2 forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubstrateEffect {
    /// `{Sim}` — the simulation-tick context. Every `OmegaSystem` MUST
    /// carry this in its effect-row ; it's the base discipline.
    Sim,
    /// `{Render}` — GPU render-graph context (phases 7–8 of omega_step).
    /// Implies `{GPU, Region<'frame>, Backend<B>}` per spec.
    Render,
    /// `{Audio}` — audio-DSP context (phase 6 of omega_step). Implies
    /// `{NoAlloc, NoUnbounded, Deadline<1ms>, Realtime<Crit>, PureDet}`.
    /// Systems with this effect run on a dedicated audio-callback fiber.
    Audio,
    /// `{Net}` — network IO (phases 2 + 11). Requires `ConsentToken<"net">`
    /// at the OmegaConsent layer. DEFERRED in stub form — see
    /// `specs/30 § DEFERRED § D-1`.
    Net,
    /// `{Save}` — save-journal-append context (phase 12). Requires
    /// `ConsentToken<"fs">` + auto-generates `{Audit<"save-journal">}`.
    Save,
    /// `{Replay}` — replay-mode marker. When present, `{Net}` is permitted
    /// alongside `{PureDet}` because network is replaced by the recorded trace.
    Replay,
}

impl fmt::Display for SubstrateEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Sim => "{Sim}",
            Self::Render => "{Render}",
            Self::Audio => "{Audio}",
            Self::Net => "{Net}",
            Self::Save => "{Save}",
            Self::Replay => "{Replay}",
        };
        f.write_str(name)
    }
}

/// Aggregated effect-row a system declares. Order is normalized at
/// construction so `EffectRow::eq` is set-equality (not list-equality).
///
/// § STAGE-0 SHAPE
///   Stage-0 stores effects as a sorted, dedup'd `Vec<SubstrateEffect>`.
///   This keeps `Display` deterministic (load-bearing for replay-log
///   readability) without pulling in a hash-set dep.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectRow {
    effects: Vec<SubstrateEffect>,
}

impl EffectRow {
    /// Construct from a slice. Effects are deduplicated + sorted by
    /// canonical-discriminant order so `EffectRow::eq` is set-equality.
    #[must_use]
    pub fn from_slice(effects: &[SubstrateEffect]) -> Self {
        let mut v: Vec<SubstrateEffect> = effects.to_vec();
        v.sort();
        v.dedup();
        Self { effects: v }
    }

    /// The canonical `{Sim}` base effect-row used by simple systems.
    #[must_use]
    pub fn sim() -> Self {
        Self::from_slice(&[SubstrateEffect::Sim])
    }

    /// `{Sim, Render}` — the most common composite for visual systems.
    #[must_use]
    pub fn sim_render() -> Self {
        Self::from_slice(&[SubstrateEffect::Sim, SubstrateEffect::Render])
    }

    /// `{Sim, Audio}` — for systems that run on the audio-callback fiber.
    #[must_use]
    pub fn sim_audio() -> Self {
        Self::from_slice(&[SubstrateEffect::Sim, SubstrateEffect::Audio])
    }

    /// `{Sim, Save}` — for systems that emit save-journal entries.
    #[must_use]
    pub fn sim_save() -> Self {
        Self::from_slice(&[SubstrateEffect::Sim, SubstrateEffect::Save])
    }

    /// Full read-only access to the effect list (sorted + deduplicated).
    #[must_use]
    pub fn effects(&self) -> &[SubstrateEffect] {
        &self.effects
    }

    /// Whether this row contains a particular effect.
    #[must_use]
    pub fn contains(&self, e: SubstrateEffect) -> bool {
        self.effects.contains(&e)
    }

    /// Row union — `{Sim} ⊎ {Render} = {Sim, Render}`.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut v = self.effects.clone();
        v.extend(other.effects.iter().copied());
        Self::from_slice(&v)
    }

    /// Whether this row is well-formed for omega_step inclusion. Mirrors
    /// `specs/30 § COMPOSITION-RULES § validity` :
    /// - MUST contain `{Sim}` (the base discipline)
    /// - MUST NOT contain `{Net}` without either consent OR `{Replay}` ;
    ///   stage-0 flags the bare `{Net}` case as invalid because no
    ///   ConsentToken type is wired yet (DEFERRED to H1).
    ///
    /// Returns the canonical reason-string for the violation, or `None`
    /// if the row is well-formed.
    #[must_use]
    pub fn validate(&self) -> Option<&'static str> {
        if !self.contains(SubstrateEffect::Sim) {
            return Some("no-Sim");
        }
        if self.contains(SubstrateEffect::Net) && !self.contains(SubstrateEffect::Replay) {
            return Some("Net-without-Replay-or-consent");
        }
        None
    }
}

impl fmt::Display for EffectRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        let mut first = true;
        for e in &self.effects {
            if !first {
                f.write_str(", ")?;
            }
            // Trim the surrounding `{}` from the per-effect Display ;
            // we only render the inner names inside the row braces.
            let s = e.to_string();
            let inner = s.trim_start_matches('{').trim_end_matches('}');
            f.write_str(inner)?;
            first = false;
        }
        f.write_str("}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_is_the_base() {
        let row = EffectRow::sim();
        assert!(row.contains(SubstrateEffect::Sim));
        assert!(row.validate().is_none());
    }

    #[test]
    fn missing_sim_is_invalid() {
        let row = EffectRow::from_slice(&[SubstrateEffect::Render]);
        assert_eq!(row.validate(), Some("no-Sim"));
    }

    #[test]
    fn net_without_replay_is_invalid() {
        let row = EffectRow::from_slice(&[SubstrateEffect::Sim, SubstrateEffect::Net]);
        assert_eq!(row.validate(), Some("Net-without-Replay-or-consent"));
    }

    #[test]
    fn net_with_replay_is_valid() {
        let row = EffectRow::from_slice(&[
            SubstrateEffect::Sim,
            SubstrateEffect::Net,
            SubstrateEffect::Replay,
        ]);
        assert!(row.validate().is_none());
    }

    #[test]
    fn from_slice_dedups_and_sorts() {
        let row = EffectRow::from_slice(&[
            SubstrateEffect::Render,
            SubstrateEffect::Sim,
            SubstrateEffect::Sim, // dup
            SubstrateEffect::Audio,
        ]);
        // Sorted by enum-discriminant order : Sim < Render < Audio.
        assert_eq!(
            row.effects(),
            &[
                SubstrateEffect::Sim,
                SubstrateEffect::Render,
                SubstrateEffect::Audio
            ]
        );
    }

    #[test]
    fn union_is_associative_and_commutative() {
        let a = EffectRow::sim();
        let b = EffectRow::from_slice(&[SubstrateEffect::Render]);
        let c = EffectRow::from_slice(&[SubstrateEffect::Audio]);
        let lhs = a.union(&b).union(&c);
        let rhs = c.union(&a).union(&b);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn display_renders_in_canonical_order() {
        let row = EffectRow::from_slice(&[SubstrateEffect::Render, SubstrateEffect::Sim]);
        // Sim < Render in discriminant-order ⇒ Sim appears first.
        assert_eq!(row.to_string(), "{Sim, Render}");
    }
}

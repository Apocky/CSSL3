//! § wired_loot — combat-end-driven loot-drop wired into the loa-host event-loop.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16-WIREUP-LOOT (W13-8 → loa-host event-loop)
//!
//! § ROLE
//!   Per-frame `tick(state, dt_ms, event)` ingests CombatEndedEvent and rolls
//!   a `LootItem` via the wrapped `cssl-host-loot::roll_loot` crate. Cap-gated :
//!   default-deny — drops happen ONLY when the host explicitly grants the cap
//!   on a combat-ended frame.
//!
//! § Σ-CAP-GATE attestation
//!   - default-deny : `LootEvent { combat_ended: true, allow_drop: false }`
//!     produces NO drop. The cap is the only path to a roll.
//!   - cosmetic-only-axiom : the wrapped crate's structural-attestation
//!     guarantees no `LootAffix::StatBuff(...)` variant exists ; this slice
//!     re-exports `attest_no_pay_for_power` so the host can re-verify.
//!   - KAN-bias is Σ-mask-gated default-deny inside the wrapped crate ;
//!     this slice surfaces `KanBiasConsent::denied()` as the default
//!     consent passed to `roll_loot`.
//!
//! § Σ-CHAIN ANCHOR
//!   On each successful drop, the host SHOULD anchor the LootDropEvent to
//!   the canonical Σ-Chain via `cssl-host-loot::anchor_drop_to_sigma_chain`.
//!   This slice exposes the helper but does NOT call it directly (chain-
//!   anchoring is integrator-controlled · sibling-agent territory).
//!
//! § ATTESTATION
//!   ¬ harm · ¬ pay-for-power · ¬ surveillance.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]

pub use cssl_host_loot::{
    anchor_drop_to_sigma_chain, attest_no_pay_for_power, roll_loot, DropRateDistribution,
    KanBiasConsent, LootContext, LootDropEvent, LootItem,
};
pub use cssl_host_gear_archetype::Rarity;

/// § Per-frame loot-event from the host's combat / encounter system.
/// `combat_ended` fires the frame the encounter resolves ; `allow_drop`
/// is the Σ-cap-gate that gates the actual drop.
#[derive(Debug, Clone, Copy, Default)]
pub struct LootEvent {
    pub combat_ended: bool,
    /// Σ-cap-gate : drops happen ONLY if true. Default-deny.
    pub allow_drop: bool,
    /// Encounter-seed for deterministic drop ; replay-bit-equal across hosts.
    pub seed_lo: u64,
    pub seed_hi: u64,
}

/// § Persistent loot-state (per local-player encounter feed).
pub struct LootState {
    /// Drop distribution (PUBLIC = canonical 8-tier).
    pub distribution: DropRateDistribution,
    /// KAN-bias consent — default-deny (player must explicitly opt-in).
    pub bias_consent: KanBiasConsent,
    /// Counter of drops produced (HUD + replay).
    pub drops_produced: u64,
    /// Last item rolled (for HUD display).
    pub last_item: Option<LootItem>,
    /// Counter of cap-gate denials (telemetry signal · helps detect stuck caps).
    pub gate_denials: u64,
}

impl Default for LootState {
    fn default() -> Self {
        Self::new()
    }
}

impl LootState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            distribution: DropRateDistribution::PUBLIC,
            bias_consent: KanBiasConsent::denied(),
            drops_produced: 0,
            last_item: None,
            gate_denials: 0,
        }
    }
}

/// Per-frame tick — checks for combat-ended events and rolls loot when
/// `allow_drop` is granted. Returns Some(LootItem) on successful roll,
/// None on no-event or cap-denial.
pub fn tick(state: &mut LootState, _dt_ms: f32, event: LootEvent) -> Option<LootItem> {
    if !event.combat_ended {
        return None;
    }
    if !event.allow_drop {
        // Cap-denied : count the denial for telemetry, no drop.
        state.gate_denials = state.gate_denials.saturating_add(1);
        return None;
    }
    let ctx = LootContext::default_for_combat_end();
    let seed_u128 = ((event.seed_hi as u128) << 64) | (event.seed_lo as u128);
    let item = roll_loot(&state.distribution, &state.bias_consent, &ctx, seed_u128);
    state.drops_produced = state.drops_produced.saturating_add(1);
    state.last_item = Some(item.clone());
    Some(item)
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_empty() {
        let s = LootState::default();
        assert_eq!(s.drops_produced, 0);
        assert!(s.last_item.is_none());
    }

    #[test]
    fn tick_no_combat_no_drop() {
        let mut s = LootState::new();
        let event = LootEvent {
            combat_ended: false,
            allow_drop: true,
            seed_lo: 1,
            seed_hi: 2,
        };
        let result = tick(&mut s, 16.6, event);
        assert!(result.is_none());
        assert_eq!(s.drops_produced, 0);
    }

    #[test]
    fn tick_combat_ended_default_deny() {
        let mut s = LootState::new();
        let event = LootEvent {
            combat_ended: true,
            allow_drop: false, // CAP DENIED
            seed_lo: 1,
            seed_hi: 2,
        };
        let result = tick(&mut s, 16.6, event);
        assert!(result.is_none());
        assert_eq!(s.drops_produced, 0);
        assert_eq!(s.gate_denials, 1);
    }

    #[test]
    fn tick_combat_ended_with_cap_drops_item() {
        let mut s = LootState::new();
        let event = LootEvent {
            combat_ended: true,
            allow_drop: true,
            seed_lo: 0xDEAD_BEEF,
            seed_hi: 0xBAD_F00D,
        };
        let result = tick(&mut s, 16.6, event);
        assert!(result.is_some());
        assert_eq!(s.drops_produced, 1);
        assert!(s.last_item.is_some());
    }

    #[test]
    fn dropped_item_passes_no_pay_for_power_attestation() {
        let mut s = LootState::new();
        let event = LootEvent {
            combat_ended: true,
            allow_drop: true,
            seed_lo: 0xC0FFEE,
            seed_hi: 0xBABE,
        };
        let item = tick(&mut s, 16.6, event).expect("drop produced");
        assert!(attest_no_pay_for_power(&item));
    }

    #[test]
    fn deterministic_drops_for_same_seed() {
        let mut s1 = LootState::new();
        let mut s2 = LootState::new();
        let event = LootEvent {
            combat_ended: true,
            allow_drop: true,
            seed_lo: 0x1234_5678_9ABC_DEF0,
            seed_hi: 0x0FED_CBA9_8765_4321,
        };
        let i1 = tick(&mut s1, 16.6, event).unwrap();
        let i2 = tick(&mut s2, 16.6, event).unwrap();
        // Replay-bit-equal : same seed → same rarity at minimum.
        assert_eq!(i1.rarity, i2.rarity);
    }

    #[test]
    fn gate_denial_counter_accumulates() {
        let mut s = LootState::new();
        let denied_event = LootEvent {
            combat_ended: true,
            allow_drop: false,
            seed_lo: 1,
            seed_hi: 2,
        };
        for _ in 0..5 {
            let _ = tick(&mut s, 16.6, denied_event);
        }
        assert_eq!(s.gate_denials, 5);
        assert_eq!(s.drops_produced, 0);
    }
}

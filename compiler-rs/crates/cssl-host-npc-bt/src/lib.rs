// § T11-W7-RD-B6-NPC-BT : cssl-host-npc-bt — root module
// ════════════════════════════════════════════════════════════════════
// § I> NPC-AI = narrow-BT + GOAP ; ¬ AGI ; ¬ self-improvement
// § I> 5-tier stack : L0 perception → L1 BT → L2 GOAP → L3 routine → L4 cocreative-overlay
// § I> Sensitive<biometric|gaze|face|body> STRUCTURALLY-banned (SIG0003)
// § I> determinism : BTreeMap iter ; splitmix64 RNG ; replay-bit-equal
// § I> safety : forbid(unsafe_code) · no panics in lib · all-failures via Result/Option
// § I> exports : perception · bt · conditions · actions · decorators · goap
//                routines · economy · cocreative_overlay · lod · audit
// ════════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]

pub mod actions;
pub mod audit;
pub mod bt;
pub mod cocreative_overlay;
pub mod conditions;
pub mod decorators;
pub mod economy;
pub mod goap;
pub mod lod;
pub mod perception;
pub mod routines;

pub use actions::ActionKind;
pub use audit::{AuditEvent, AuditSink, NoopAuditSink, RecordingAuditSink};
pub use bt::{BtNode, BtStatus, NpcWorldRef, tick};
pub use cocreative_overlay::{
    Mood, SensitiveScopeViolation, bias_modulate_dialogue_choice, bias_mood_color,
};
pub use conditions::ConditionKind;
pub use decorators::DecoratorKind;
pub use economy::{MarketPrice, PlayerTrade, apply_player_trade, tick_market};
pub use goap::{FactValue, GoapAction, GoapState, plan};
pub use lod::{LodTier, should_tick, tick_freq_hz_for_tier, tier_for_distance};
pub use perception::{Perception, SensedEntity, SensedKind};
pub use routines::{HourBlock, RoutineArchetype, RoutineActivity, daily_schedule};

/// Crate-level metadata banner ← attests § PRIME-DIRECTIVE structurally.
///
/// § I> consent=OS · violation=bug · no-override-exists
/// § I> NPC-perception is LOCAL-only ; never reads player-private-state
/// § I> Sensitive<biometric|gaze|face|body> banned at-type-level (SIG0003)
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • violation=bug • no-override-exists ; NPC-AI=narrow ; ¬AGI ; ¬surveillance";

/// Crate version (matches Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Splitmix64 deterministic RNG — used by routine-jitter + cocreative pool-select.
///
/// Public so tests can construct + assert state. Replay-bit-equal across hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DetRng {
    state: u64,
}

impl DetRng {
    /// Construct from explicit seed. Adjacent seeds produce uncorrelated streams
    /// after the first mix-step.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Current state — for replay-manifest headers.
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// Advance state, return next u64 — splitmix64 canonical step (Vigna 2014).
    #[allow(clippy::unreadable_literal)]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Next u32 — high 32 bits of next_u64.
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Next f32 ∈ [0, 1) — IEEE-754 bit-equal output.
    pub fn next_f32(&mut self) -> f32 {
        let bits = self.next_u32() >> 8;
        (bits as f32) / ((1u32 << 24) as f32)
    }

    /// Range [0, n) — modulo-bias acceptable for game-roll use ;
    /// deterministic-replay is the load-bearing axiom.
    pub fn range_u32(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.next_u32() % n
    }
}

impl Default for DetRng {
    fn default() -> Self {
        Self::new(0xDEAD_BEEF_CAFE_F00D)
    }
}

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_nonempty() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
        assert!(PRIME_DIRECTIVE_BANNER.contains("¬AGI"));
    }

    #[test]
    fn version_matches_cargo() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn det_rng_same_seed_same_stream() {
        let mut a = DetRng::new(42);
        let mut b = DetRng::new(42);
        for _ in 0..256 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn det_rng_f32_in_unit() {
        let mut r = DetRng::new(7);
        for _ in 0..1024 {
            let v = r.next_f32();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn det_rng_range_zero_returns_zero() {
        let mut r = DetRng::new(1);
        assert_eq!(r.range_u32(0), 0);
    }
}

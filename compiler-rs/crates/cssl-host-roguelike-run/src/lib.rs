// § T11-W7-RD-B2 : cssl-host-roguelike-run — root module
// ════════════════════════════════════════════════════════════════════
// § I> run-state-machine ← GDDs/ROGUELIKE_LOOP.csl
//   Hub → BiomeSelect → Floor(N=3..12) → BossArena → Reward → BiomeSelect | Hub
// § I> 8-biome DAG (¬ tree) · seed-pinned u128 · soft-perma default
// § I> meta-progression bridges + gift-economy run-share
// § I> safety : forbid(unsafe_code) · no panics in lib · all-failures via Result
// § I> determinism : BTreeMap/BTreeSet for serde-stable ; ¬ HashMap on serialized paths
// ════════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]

pub mod run_state;
pub mod biome_dag;
pub mod seed;
pub mod floor_count;
pub mod death;
pub mod meta_progress;
pub mod run_share;
pub mod dm_handoff;

pub use biome_dag::{Biome, BiomeDag, BiomeDagErr};
pub use death::{apply_death_penalty, DeathOutcome, SoftPermaCarryover};
pub use dm_handoff::{DmSceneEditRequest, GmIntroProseRequest, HandoffEvent};
pub use floor_count::floor_count_for;
pub use meta_progress::{MetaErr, MetaProgress};
pub use run_share::{RunShareReceipt, RunShareScoring, ScreenshotHandle};
pub use run_state::{RunPhase, RunState, RunStateErr};
pub use seed::{derive_rng_u32, derive_rng_u64, pin_seed, DetRng};

/// § PRIME-DIRECTIVE banner ← attest structurally per global preferences.
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • violation=bug • no-override-exists";

/// Crate version (matches Cargo.toml) — surfaced for run-share receipt headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Spec-anchor : the GDD this crate implements.
pub const SPEC_ANCHOR: &str = "GDDs/ROGUELIKE_LOOP.csl";

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_nonempty() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_matches_cargo() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn spec_anchor_points_at_gdd() {
        assert_eq!(SPEC_ANCHOR, "GDDs/ROGUELIKE_LOOP.csl");
    }
}

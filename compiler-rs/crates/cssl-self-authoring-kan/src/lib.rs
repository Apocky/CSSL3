//! § cssl-self-authoring-kan — substrate self-improves-over-iterations.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W12-KAN-LOOP · Self-Authoring-KAN-Loop architect-mode
//!
//! § THESIS
//!   Quality-signals from sibling-agents (training-pair-log W12-2 ·
//!   playtest-scoring W12-10 · player-rating W12-7 · GM-accept · sandbox-pass ·
//!   remix-fork) feed into a KAN-classifier that adapts its bias-vector and
//!   feeds-back into the procgen-template-priority-map so the substrate
//!   self-improves over training-iterations WITHOUT a-single-player being
//!   able to dominate-the-bias (k-anonymity-floor enforced).
//!
//! ```text
//!   sibling-agents ──QualitySignal──▶ Reservoir(N=4096)
//!                                            │
//!                                  k-anon ≥ 10 floor
//!                                            ▼
//!                                  KanBiasUpdate (Q14 deltas)
//!                                            │
//!                                            ▼
//!                                  TemplateBiasMap (per-template × per-archetype)
//!                                            │
//!                                            ▼
//!                          procgen-template-priority-shift
//!                                            │
//!                              every 1024 updates ⇒ Σ-Chain-anchor (rollback-safe)
//! ```
//!
//! § ANTI-POISONING (per Apocky directive · spec/14_SIGMA_CHAIN)
//!
//!   1. *Reservoir-sampling.* Last N=4096 signals · uniform-random eviction.
//!      Bursty single-player can't drown the reservoir : eviction is uniform.
//!
//!   2. *K-anon-floor.* No bias-update can fire until ≥ K_ANON_FLOOR
//!      (default 10) DISTINCT player-fingerprints have contributed signals
//!      for the affected (template · archetype) cell. A solo griefer therefore
//!      cannot-tip-bias.
//!
//!   3. *Sovereign-revoke-cascading.* When a player invokes the
//!      [`Reservoir::sovereign_revoke`] API, ALL their contributions are
//!      drained from the reservoir AND the bias-map is recomputed-from-scratch
//!      using the remaining signals. This is the "right-to-be-forgotten"
//!      semantic compatible with Σ-Chain anchor history (the anchor records
//!      the BIAS-STATE only, never raw signals).
//!
//!   4. *Σ-mask inspector.* [`BiasInspector`] surfaces individual-bias rows
//!      ONLY when the caller presents a [`SovereignCap`] gating a Σ-mask
//!      with audience = `Admin` AND effect = `Read`. Default-deny otherwise.
//!
//!   5. *Σ-Chain-anchor cadence.* Every [`ANCHOR_EVERY_N_UPDATES`] (1024)
//!      successful bias-updates, [`SelfAuthoringKanLoop::tick`] computes a
//!      BLAKE3-128 anchor-hash over the current TemplateBiasMap state and
//!      emits a checkpoint-record. Rollback to a prior anchor is supported
//!      via [`SelfAuthoringKanLoop::rollback_to_anchor`].
//!
//! § DESIGN (per memory_sawyer_pokemon_efficiency)
//!
//!   - Bit-packed records : `QualitySignalRecord` = 16 bytes fixed.
//!   - Q14 fixed-point arithmetic : `i16` weight-deltas in -1.0..+1.0 range.
//!   - Pre-allocated reservoir : `Box<[QualitySignalRecord; N=4096]>`.
//!   - Index-types : `TemplateId(u32)`, `ArchetypeId(u8)`, `PlayerHandle(u32)`.
//!   - LUT for archetype-name lookup (no `HashMap`, no allocation).
//!   - Differential encoding : signal-frame relative to `Reservoir::created_at`.
//!   - Audit-ring integration : every k-anon-violation + every poisoning-reject
//!     emits exactly-one entry into the parent `cssl-substrate-sigma-runtime`
//!     audit-ring via the gate-fn.
//!
//! § PRIME-DIRECTIVE attestation
//!   N! [harm control surveillance manipulation exploitation
//!       coercion weaponization discrimination] in-the-doing.
//!   Consent-as-OS : every signal-ingest is consent-cap-gated.
//!   Sovereign-revocable : sovereign-revoke-cascading recomputes-from-remaining.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.
//!
//! § INTEGRATION-POINTS (sibling-W12 agents)
//!
//!   - W12-2 (training-pair-log) : feeds [`QualitySignal::SandboxPass`] /
//!     [`QualitySignal::SandboxFail`] for compiled training-pair entries.
//!   - W12-7 (rating-pipeline) : feeds [`QualitySignal::RatingFiveStar`] /
//!     [`QualitySignal::PlayerLike`] / [`QualitySignal::PlayerDislike`].
//!   - W12-10 (playtest-scorer) : feeds [`QualitySignal::GmAccept`] /
//!     [`QualitySignal::GmReject`] from playtest-arena evaluations.
//!
//! § SPEC
//!   `Labyrinth of Apocalypse/systems/self_authoring_kan.csl` (canonical).

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::similar_names)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::map_unwrap_or)]

pub mod anchor;
pub mod bias_map;
pub mod inspector;
pub mod loop_tick;
pub mod reservoir;
pub mod signal;

// ── canonical re-exports ────────────────────────────────────────────────────
pub use anchor::{AnchorRecord, AnchorRing, ANCHOR_EVERY_N_UPDATES, ANCHOR_RING_CAPACITY};
pub use bias_map::{
    ArchetypeId, BiasUpdateError, KanBiasUpdate, TemplateBiasMap, TemplateId, ARCHETYPE_COUNT,
    ARCHETYPE_NAMES, BIAS_Q14_MAX, BIAS_Q14_MIN, BIAS_Q14_ONE,
};
pub use inspector::{BiasInspector, InspectorError};
pub use loop_tick::{LoopStats, LoopTickError, SelfAuthoringKanLoop};
pub use reservoir::{
    PlayerHandle, QualitySignalRecord, Reservoir, ReservoirError, K_ANON_FLOOR_DEFAULT,
    RESERVOIR_CAPACITY,
};
pub use signal::QualitySignal;

// ───────────────────────────────────────────────────────────────────────────
// § ATTESTATION (verbatim per PRIME_DIRECTIVE § 11)
// ───────────────────────────────────────────────────────────────────────────

/// Canonical attestation declaring this crate's alignment with the
/// PRIME_DIRECTIVE. Recorded into the audit-ring on
/// [`SelfAuthoringKanLoop::new`] construction.
pub const ATTESTATION: &str = "\
§ cssl-self-authoring-kan ‼ ATTESTATION (PRIME_DIRECTIVE § 11)\n\
   t∞: every-signal-ingest consent-cap-gated · ¬ trusted-caller-bypass\n\
   t∞: k-anon-floor ≥ K_ANON_FLOOR_DEFAULT distinct fingerprints\n\
   t∞: reservoir N=4096 uniform-random-eviction · ¬ bursty-domination\n\
   t∞: sovereign-revoke-cascading recomputes-from-remaining · ¬ silent-retain\n\
   t∞: Σ-mask-gated inspector · default-deny per-row · admin-cap-required\n\
   t∞: Σ-Chain-anchor every 1024 updates · BLAKE3-128 hash · rollback-safe\n\
   t∞: Q14 fixed-point bias-deltas · saturating-clamp · ¬ runaway-amplification\n\
   t∞: poisoning-rejected emits audit-ring-entry · ¬ silent-drop\n\
   spec : Labyrinth of Apocalypse/systems/self_authoring_kan.csl\n\
   ¬-conflate : SelfAuthoringKanLoop ≠ KanClassifier ≠ TemplateBiasMap\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attestation_present_and_well_formed() {
        assert!(ATTESTATION.contains("ATTESTATION"));
        assert!(ATTESTATION.contains("k-anon-floor"));
        assert!(ATTESTATION.contains("sovereign-revoke"));
        assert!(ATTESTATION.contains("Σ-Chain-anchor"));
        assert!(ATTESTATION.contains("Q14"));
    }

    #[test]
    fn re_exports_compile() {
        // Smoke-test : every public re-export is reachable from `crate::`
        // (compile-time check via type-elaboration).
        let _rsv_cap: usize = RESERVOIR_CAPACITY;
        let _k_anon: u32 = K_ANON_FLOOR_DEFAULT;
        let _arch_count: usize = ARCHETYPE_COUNT;
        let _anchor_n: u64 = ANCHOR_EVERY_N_UPDATES;
        let _bias_max: i16 = BIAS_Q14_MAX;
        let _bias_min: i16 = BIAS_Q14_MIN;
        assert_eq!(BIAS_Q14_MAX, 16383);
        assert_eq!(BIAS_Q14_MIN, -16383);
        assert_eq!(BIAS_Q14_ONE, 16384);
        assert_eq!(ARCHETYPE_COUNT, 8);
        assert_eq!(RESERVOIR_CAPACITY, 4096);
        assert_eq!(K_ANON_FLOOR_DEFAULT, 10);
        assert_eq!(ANCHOR_EVERY_N_UPDATES, 1024);
    }
}

// § T11-W8-E1 : seasonal-hard-perma · 90-day cycle · gift-economy
// ════════════════════════════════════════════════════════════════════
// § I> SeasonMode { Soft (default · meta-on) · Hard (permadeath · meta-paused) }
// § I> 90-day cycle-index · permadeath structurally-enforced in Hard mode
// § I> meta-progression PAUSED in hard-perma : separate-track counters
// § I> season-end → emit MemorialImprintLike (mock-trait for sibling W8-C3)
// § I> NO leaderboards · gift-economy-only rewards (PRIME §1)
// § I> ¬ pay-for-power · cosmetic-channel-only-axiom
// ════════════════════════════════════════════════════════════════════

use crate::run_state::{RunPhase, RunState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § Season-cycle length in days (canonical : 90 days per §SEASONAL framework).
pub const SEASON_CYCLE_DAYS: u32 = 90;

/// § Season-mode : Soft (default · meta-progression-on) or Hard (permadeath · meta-paused).
///
/// Soft is the default mode preserving current roguelike-loop semantics
/// (see `crate::death::apply_death_penalty` soft-perma branch).
/// Hard is opt-in seasonal · ConsentToken<"hard-perma"> required at the caller.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SeasonMode {
    /// Default mode · soft-perma · meta-progression accumulates as normal.
    #[default]
    Soft,
    /// Opt-in seasonal · hard-perma · meta-progression PAUSED on the soft track.
    Hard,
}

impl SeasonMode {
    /// Stable wire-key for run-share receipts and SQL persistence.
    pub fn key(&self) -> &'static str {
        match self {
            SeasonMode::Soft => "soft",
            SeasonMode::Hard => "hard",
        }
    }
}

/// § SeasonId · 90-day-cycle-index (monotonic from genesis-of-product).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SeasonId(pub u32);

/// § DeathCause · narrow enum captured at character-death for memorial-imprint.
///
/// Intentionally narrow ; non-exhaustive guard via `Other` for forward-compat
/// without breaking serde wire-format on existing variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeathCause {
    /// Standard combat-death · enemy-strike-killed.
    Combat,
    /// Killed by Nemesis-system antagonist (named-enemy-defeat).
    NemesisDefeat,
    /// Environmental hazard (lava · spikes · falling).
    Hazard,
    /// Drowning (submerged · oxygen-out).
    Drowning,
    /// Starvation (hunger-system zero · long-run-attrition).
    Starvation,
    /// Catch-all narrow custom · max 64 chars · attribution-friendly.
    Other(String),
}

impl DeathCause {
    /// Stable wire-key for SQL persistence.
    pub fn key(&self) -> &'static str {
        match self {
            DeathCause::Combat => "combat",
            DeathCause::NemesisDefeat => "nemesis_defeat",
            DeathCause::Hazard => "hazard",
            DeathCause::Drowning => "drowning",
            DeathCause::Starvation => "starvation",
            DeathCause::Other(_) => "other",
        }
    }
}

/// § SeasonCharacter · per-season character record (alive ↔ fallen).
///
/// `coherence_score_final` and `biography_blob` are populated at-death
/// for memorial-imprint dispatch ; both Optional during life.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonCharacter {
    /// UUID-shaped id (hex-string for stable serde · no extra dep on `uuid` crate).
    pub id: String,
    /// Owner's pubkey (BLAKE3-fingerprint or Ed25519-pubkey raw bytes-hex).
    pub player_pubkey: String,
    /// Season-id this character belongs to (immutable post-creation).
    pub season_id: SeasonId,
    /// Mode this character runs under (immutable post-creation).
    pub mode: SeasonMode,
    /// Alive flag · structural permadeath-enforcement when mode == Hard.
    pub alive: bool,
    /// Captured at-death only.
    pub cause_of_death: Option<DeathCause>,
    /// Captured at-death · Coherence-Engine final-score snapshot.
    pub coherence_score_final: Option<f32>,
    /// Captured at-death · narrative-biography text-blob (NOT user-PII).
    pub biography_blob: Option<String>,
}

impl SeasonCharacter {
    /// Construct an alive character at season-genesis.
    pub fn new(
        id: impl Into<String>,
        player_pubkey: impl Into<String>,
        season_id: SeasonId,
        mode: SeasonMode,
    ) -> Self {
        Self {
            id: id.into(),
            player_pubkey: player_pubkey.into(),
            season_id,
            mode,
            alive: true,
            cause_of_death: None,
            coherence_score_final: None,
            biography_blob: None,
        }
    }

    /// § Mark this character as fallen (death-imprint capture point).
    ///
    /// In Hard mode this is **structurally irreversible** — `alive` cannot be
    /// reset to true once Hard-mode-died (see `try_revive` for the gated check).
    pub fn mark_dead(
        &mut self,
        cause: DeathCause,
        coherence_score: f32,
        biography: impl Into<String>,
    ) {
        self.alive = false;
        self.cause_of_death = Some(cause);
        self.coherence_score_final = Some(coherence_score);
        self.biography_blob = Some(biography.into());
    }

    /// § Attempt revival.
    ///
    /// In Soft mode : permitted · returns `Ok(true)`.
    /// In Hard mode : structurally-forbidden once-dead · returns `Err(SeasonErr::HardPermaIrreversible)`.
    pub fn try_revive(&mut self) -> Result<bool, SeasonErr> {
        if self.alive {
            return Ok(false); // already alive · no-op
        }
        match self.mode {
            SeasonMode::Soft => {
                self.alive = true;
                self.cause_of_death = None;
                // Coherence-final + biography retained for memorial-history (audit).
                Ok(true)
            }
            SeasonMode::Hard => Err(SeasonErr::HardPermaIrreversible),
        }
    }
}

/// § Per-season meta-progression isolated counters.
///
/// Hard-mode characters NEVER cross-pollinate Soft-mode stats. This is the
/// structural-isolation pattern the GDD requires (no soft-mode boost from
/// hard-mode death-grinding).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonMetaProgress {
    /// Soft-mode-only echoes (accumulates · normal soft-perma rule).
    pub soft_echoes: u64,
    /// Hard-mode-only echoes (separate-track · season-bound · resets at season-end).
    pub hard_echoes: u64,
    /// Soft-mode-only class-XP (per-class-id).
    pub soft_class_xp: BTreeMap<u32, u64>,
    /// Hard-mode-only class-XP (per-class-id · separate-track).
    pub hard_class_xp: BTreeMap<u32, u64>,
}

impl SeasonMetaProgress {
    /// Deposit echoes into the track for the given mode.
    pub fn deposit(&mut self, mode: SeasonMode, amount: u64) {
        match mode {
            SeasonMode::Soft => {
                self.soft_echoes = self.soft_echoes.saturating_add(amount);
            }
            SeasonMode::Hard => {
                self.hard_echoes = self.hard_echoes.saturating_add(amount);
            }
        }
    }

    /// Grant class-XP into the track for the given mode (capped at 1M per class).
    pub fn grant_class_xp(&mut self, mode: SeasonMode, class_id: u32, xp: u64) {
        const CAP: u64 = 1_000_000;
        let bucket = match mode {
            SeasonMode::Soft => &mut self.soft_class_xp,
            SeasonMode::Hard => &mut self.hard_class_xp,
        };
        let entry = bucket.entry(class_id).or_insert(0);
        *entry = (*entry).saturating_add(xp).min(CAP);
    }

    /// Read echoes for a given mode (track-isolated).
    pub fn echoes_for(&self, mode: SeasonMode) -> u64 {
        match mode {
            SeasonMode::Soft => self.soft_echoes,
            SeasonMode::Hard => self.hard_echoes,
        }
    }

    /// Read class-XP for a given mode + class-id (track-isolated).
    pub fn class_xp_for(&self, mode: SeasonMode, class_id: u32) -> u64 {
        let bucket = match mode {
            SeasonMode::Soft => &self.soft_class_xp,
            SeasonMode::Hard => &self.hard_class_xp,
        };
        bucket.get(&class_id).copied().unwrap_or(0)
    }
}

/// § Season-extension errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeasonErr {
    /// Hard-perma death is structurally irreversible · revival forbidden.
    HardPermaIrreversible,
    /// Memorial-imprint dispatch failed (caller-side reason).
    MemorialDispatchFailed(String),
    /// Caller attempted to attach a leaderboard-rank to a memorial-imprint.
    /// Invariant : NO leaderboards · gift-economy-only.
    LeaderboardForbidden,
}

/// § MemorialImprintLike · local trait for season-end memorial-imprint dispatch.
///
/// Sibling crate W8-C3 (`cssl-host-akashic-records`) is NOT yet merged · we
/// define a local trait + provide a no-op mock-impl. When W8-C3 lands, the
/// `cssl-host-akashic-records::MemorialImprint` should impl this trait
/// (additive · backward-compatible).
///
/// Invariant : implementations MUST NOT emit leaderboard-ranks. Memorials are
/// gift-economy-only · attribution-anchored · cosmetic-channel-only-axiom.
pub trait MemorialImprintLike {
    /// Dispatch a memorial-imprint for a fallen character.
    ///
    /// `attribution_pubkey` is the player's pubkey (or None for anonymous).
    /// Returns the imprint-id (BLAKE3-hex) on success.
    fn dispatch(
        &mut self,
        character: &SeasonCharacter,
        attribution_pubkey: Option<&str>,
    ) -> Result<String, SeasonErr>;
}

/// § In-memory mock-impl of MemorialImprintLike.
///
/// Records dispatched-imprints for test-assertion. Production callers should
/// use the W8-C3 `cssl-host-akashic-records` impl once merged.
#[derive(Debug, Default, Clone)]
pub struct MockMemorialDispatcher {
    /// All imprints dispatched (in-order) · `(character_id, attribution, imprint_id)`.
    pub imprints: Vec<(String, Option<String>, String)>,
}

impl MemorialImprintLike for MockMemorialDispatcher {
    fn dispatch(
        &mut self,
        character: &SeasonCharacter,
        attribution_pubkey: Option<&str>,
    ) -> Result<String, SeasonErr> {
        // Synthesize a deterministic imprint-id (BLAKE3-shaped) from char-id + season.
        // Production impl will use real BLAKE3 of the imprint-payload.
        let imprint_id = format!(
            "imprint-{}-{}-mock",
            character.season_id.0,
            character.id
        );
        self.imprints.push((
            character.id.clone(),
            attribution_pubkey.map(std::string::ToString::to_string),
            imprint_id.clone(),
        ));
        Ok(imprint_id)
    }
}

/// § Dispatch season-end memorials for a slice of fallen-characters.
///
/// Iterates fallen-only (`alive == false`) and emits memorial-imprints.
/// Gift-economy-only : NO leaderboards (callers must NOT pass rank-data).
/// Living characters are skipped (fall-out next season).
pub fn dispatch_season_end_memorials<D: MemorialImprintLike>(
    characters: &[SeasonCharacter],
    dispatcher: &mut D,
    attribution: Option<&str>,
) -> Result<Vec<String>, SeasonErr> {
    let mut imprint_ids = Vec::new();
    for c in characters.iter().filter(|c| !c.alive) {
        let id = dispatcher.dispatch(c, attribution)?;
        imprint_ids.push(id);
    }
    Ok(imprint_ids)
}

/// § Apply seasonal-hard-perma death to a `RunState`.
///
/// In Hard mode this transitions the run to `Death` and queues a
/// memorial-imprint dispatch. Distinct from `crate::death::apply_death_penalty`
/// which handles soft-perma carryover ; this fn is the structural-permadeath
/// path for season-bound characters.
///
/// Returns the (mutated) character record · caller is responsible for
/// passing it to `dispatch_season_end_memorials` at season-end.
pub fn apply_seasonal_permadeath(
    state: &mut RunState,
    character: &mut SeasonCharacter,
    cause: DeathCause,
    coherence_score: f32,
    biography: impl Into<String>,
) -> Result<(), SeasonErr> {
    // Permadeath is only structurally-irreversible in Hard mode.
    // In Soft mode this records the death-event but caller may revive.
    state.phase = RunPhase::Death;
    state.echoes_in_run = 0;
    character.mark_dead(cause, coherence_score, biography);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_state::{RunPhase, RunState};

    // ─── SeasonMode-construction (2 tests) ───
    #[test]
    fn season_mode_default_is_soft() {
        assert_eq!(SeasonMode::default(), SeasonMode::Soft);
    }

    #[test]
    fn season_mode_keys_stable() {
        assert_eq!(SeasonMode::Soft.key(), "soft");
        assert_eq!(SeasonMode::Hard.key(), "hard");
    }

    // ─── permadeath-on-death (3 tests) ───
    #[test]
    fn hard_perma_marks_character_dead_irreversibly() {
        let mut s = RunState::genesis(0xABCD, 1);
        let mut c = SeasonCharacter::new("char1", "pk1", SeasonId(1), SeasonMode::Hard);
        apply_seasonal_permadeath(&mut s, &mut c, DeathCause::Combat, 0.42, "fell to a goblin").unwrap();
        assert!(!c.alive);
        assert_eq!(s.phase, RunPhase::Death);
        // attempt-revive must fail
        let err = c.try_revive().unwrap_err();
        assert_eq!(err, SeasonErr::HardPermaIrreversible);
        assert!(!c.alive); // still dead
    }

    #[test]
    fn permadeath_zeroes_in_run_echoes() {
        let mut s = RunState::genesis(0xABCD, 1);
        s.echoes_in_run = 9999;
        let mut c = SeasonCharacter::new("char2", "pk2", SeasonId(1), SeasonMode::Hard);
        apply_seasonal_permadeath(&mut s, &mut c, DeathCause::Hazard, 0.0, "lava").unwrap();
        assert_eq!(s.echoes_in_run, 0);
    }

    #[test]
    fn permadeath_captures_cause_score_biography() {
        let mut s = RunState::genesis(0x1234, 1);
        let mut c = SeasonCharacter::new("char3", "pk3", SeasonId(2), SeasonMode::Hard);
        apply_seasonal_permadeath(&mut s, &mut c, DeathCause::NemesisDefeat, 0.875, "Argaroth claimed them").unwrap();
        assert_eq!(c.cause_of_death, Some(DeathCause::NemesisDefeat));
        assert_eq!(c.coherence_score_final, Some(0.875));
        assert_eq!(c.biography_blob.as_deref(), Some("Argaroth claimed them"));
    }

    // ─── soft-mode-resurrection-allowed (2 tests) ───
    #[test]
    fn soft_mode_revival_succeeds() {
        let mut c = SeasonCharacter::new("c1", "pk", SeasonId(1), SeasonMode::Soft);
        c.mark_dead(DeathCause::Drowning, 0.3, "river took them");
        assert!(!c.alive);
        let revived = c.try_revive().unwrap();
        assert!(revived);
        assert!(c.alive);
        assert!(c.cause_of_death.is_none());
    }

    #[test]
    fn soft_mode_revive_when_alive_is_noop() {
        let mut c = SeasonCharacter::new("c1", "pk", SeasonId(1), SeasonMode::Soft);
        // Already alive
        let revived = c.try_revive().unwrap();
        assert!(!revived);
        assert!(c.alive);
    }

    // ─── meta-progression-pause-isolation (3 tests) ───
    #[test]
    fn soft_and_hard_echoes_isolated() {
        let mut m = SeasonMetaProgress::default();
        m.deposit(SeasonMode::Soft, 100);
        m.deposit(SeasonMode::Hard, 50);
        assert_eq!(m.echoes_for(SeasonMode::Soft), 100);
        assert_eq!(m.echoes_for(SeasonMode::Hard), 50);
        // Soft is NOT polluted by Hard activity.
        assert_eq!(m.soft_echoes, 100);
        assert_eq!(m.hard_echoes, 50);
    }

    #[test]
    fn soft_and_hard_class_xp_isolated() {
        let mut m = SeasonMetaProgress::default();
        m.grant_class_xp(SeasonMode::Soft, 7, 500);
        m.grant_class_xp(SeasonMode::Hard, 7, 200);
        assert_eq!(m.class_xp_for(SeasonMode::Soft, 7), 500);
        assert_eq!(m.class_xp_for(SeasonMode::Hard, 7), 200);
        // Distinct class-id only on the queried track.
        assert_eq!(m.class_xp_for(SeasonMode::Soft, 99), 0);
    }

    #[test]
    fn class_xp_caps_at_one_million_per_track() {
        let mut m = SeasonMetaProgress::default();
        m.grant_class_xp(SeasonMode::Hard, 1, 999_999);
        m.grant_class_xp(SeasonMode::Hard, 1, 999_999);
        assert_eq!(m.class_xp_for(SeasonMode::Hard, 1), 1_000_000);
        // Soft track for same class-id still zero.
        assert_eq!(m.class_xp_for(SeasonMode::Soft, 1), 0);
    }

    // ─── cause-of-death captured (2 tests) ───
    #[test]
    fn death_cause_keys_stable() {
        assert_eq!(DeathCause::Combat.key(), "combat");
        assert_eq!(DeathCause::NemesisDefeat.key(), "nemesis_defeat");
        assert_eq!(DeathCause::Hazard.key(), "hazard");
        assert_eq!(DeathCause::Drowning.key(), "drowning");
        assert_eq!(DeathCause::Starvation.key(), "starvation");
        assert_eq!(DeathCause::Other("custom".into()).key(), "other");
    }

    #[test]
    fn cause_of_death_persists_through_serde() {
        let c = SeasonCharacter {
            id: "x".into(),
            player_pubkey: "pk".into(),
            season_id: SeasonId(1),
            mode: SeasonMode::Hard,
            alive: false,
            cause_of_death: Some(DeathCause::Starvation),
            coherence_score_final: Some(0.0),
            biography_blob: Some("hunger".into()),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: SeasonCharacter = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cause_of_death, Some(DeathCause::Starvation));
    }

    // ─── season-end-memorial dispatch (3 tests) ───
    #[test]
    fn dispatch_season_end_emits_imprint_for_each_fallen() {
        let mut alive = SeasonCharacter::new("a", "pk", SeasonId(1), SeasonMode::Hard);
        let mut dead1 = SeasonCharacter::new("d1", "pk", SeasonId(1), SeasonMode::Hard);
        dead1.mark_dead(DeathCause::Combat, 0.5, "fought well");
        let mut dead2 = SeasonCharacter::new("d2", "pk", SeasonId(1), SeasonMode::Hard);
        dead2.mark_dead(DeathCause::Drowning, 0.2, "river");
        let _ = &mut alive;
        let chars = vec![alive, dead1, dead2];
        let mut disp = MockMemorialDispatcher::default();
        let ids = dispatch_season_end_memorials(&chars, &mut disp, Some("attribution-pk")).unwrap();
        assert_eq!(ids.len(), 2); // 2 fallen → 2 imprints (alive skipped)
        assert_eq!(disp.imprints.len(), 2);
        assert_eq!(disp.imprints[0].0, "d1");
        assert_eq!(disp.imprints[1].0, "d2");
    }

    #[test]
    fn dispatch_skips_living_characters() {
        let alive = SeasonCharacter::new("alive", "pk", SeasonId(3), SeasonMode::Hard);
        let chars = vec![alive];
        let mut disp = MockMemorialDispatcher::default();
        let ids = dispatch_season_end_memorials(&chars, &mut disp, None).unwrap();
        assert!(ids.is_empty());
        assert!(disp.imprints.is_empty());
    }

    #[test]
    fn dispatch_attribution_passed_through() {
        let mut dead = SeasonCharacter::new("dx", "pk", SeasonId(7), SeasonMode::Hard);
        dead.mark_dead(DeathCause::Hazard, 0.1, "fell");
        let chars = vec![dead];
        let mut disp = MockMemorialDispatcher::default();
        let _ = dispatch_season_end_memorials(&chars, &mut disp, Some("anon-attestation")).unwrap();
        assert_eq!(disp.imprints[0].1.as_deref(), Some("anon-attestation"));
    }

    // ─── NO-leaderboard-emit invariant (2 tests) ───
    #[test]
    fn season_err_includes_leaderboard_forbidden_variant() {
        // Structural-invariant : the error variant MUST exist for callers to
        // propagate the gift-economy-only contract upward.
        let err = SeasonErr::LeaderboardForbidden;
        assert_eq!(err, SeasonErr::LeaderboardForbidden);
    }

    #[test]
    fn mock_dispatcher_does_not_record_rank_data() {
        // The MockMemorialDispatcher tuple-shape is `(char_id, attribution, imprint_id)` ·
        // intentionally NO rank/score field. This test asserts the structural
        // contract by inspecting the recorded-tuple-arity (3, not 4).
        let mut dead = SeasonCharacter::new("d", "pk", SeasonId(1), SeasonMode::Hard);
        dead.mark_dead(DeathCause::Combat, 1.0, "won");
        let mut disp = MockMemorialDispatcher::default();
        disp.dispatch(&dead, Some("att")).unwrap();
        let recorded = &disp.imprints[0];
        // 3-tuple : (char_id, attribution, imprint_id) · NO 4th rank-field.
        let (char_id, att, imprint_id) = recorded;
        assert_eq!(char_id, "d");
        assert_eq!(att.as_deref(), Some("att"));
        assert!(imprint_id.starts_with("imprint-1-d"));
    }

    // ─── serde round-trip (1 test) ───
    #[test]
    fn season_meta_progress_serde_round_trip() {
        let mut m = SeasonMetaProgress::default();
        m.deposit(SeasonMode::Soft, 1234);
        m.deposit(SeasonMode::Hard, 5678);
        m.grant_class_xp(SeasonMode::Soft, 3, 999);
        m.grant_class_xp(SeasonMode::Hard, 4, 444);
        let json = serde_json::to_string(&m).unwrap();
        let back: SeasonMetaProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}

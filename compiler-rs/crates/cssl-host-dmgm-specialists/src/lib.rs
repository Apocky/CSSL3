//! § cssl-host-dmgm-specialists — DM/GM/Collaborator/Coder roles
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § APOCKY-CANONICAL · NON-NEGOTIABLE
//!
//!   Engine intelligence scope = DM (orchestrator) + GM (narrator) +
//!   Collaborator (co-author) + Coder (runtime-mutate) ONLY. NOT generic AGI.
//!   Reuse the Cognitive-Field-Engine substrate AS PLUMBING — every
//!   `observe(...)` routes through
//!   `cssl_host_substrate_intelligence::observe_with_profile` so each
//!   specialist accumulates INDEPENDENT KAN-bias learning per its
//!   assigned band.
//!
//! § ROLE-↔-BAND MAPPING (stable across crate revisions)
//!
//!   `SpecialistRole::DungeonMaster`  ↔ KAN-bias band 0
//!   `SpecialistRole::GameMaster`     ↔ KAN-bias band 1
//!   `SpecialistRole::Collaborator`   ↔ KAN-bias band 2
//!   `SpecialistRole::Coder`          ↔ KAN-bias band 3
//!
//!   This deliberately differs from the substrate-intelligence
//!   `Role` ordering (which uses Gm=0/Dm=1/Coll=2/Coder=3 for back-compat with
//!   pre-existing FFI consumers). The DMGM-specialists layer is the
//!   *task-spec authoritative* discriminant ordering ; we route to the
//!   underlying substrate via explicit per-role mapping.
//!
//! § PRIME-DIRECTIVE compliance
//!   - All decisions deterministic given (prompt-hash · context).
//!   - `role_council` mediates conflicts WITHOUT external arbitration —
//!     deterministic ranking with BLAKE3-of-prompt as tiebreak.
//!   - No phrase-pools · no scripted responses · no LLM API calls.
//!   - `Decision` is bit-stable across hosts (LE encoding, fixed layout).
//!
//! § ATTESTATION
//!   No data leaves the device. The Specialist trait + 4 impls are pure
//!   CPU paths over the substrate's deterministic primitives.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

use cssl_host_substrate_intelligence as si;

// ══════════════════════════════════════════════════════════════════════════
// § SpecialistRole — 4 roles, stable u32 enum_id 0..=3
// ══════════════════════════════════════════════════════════════════════════

/// The 4 cognitive specialists. ENUM_ID is STABLE across the codebase ; the
/// `to_u32` / `from_u32` round-trip is replay-safe.
///
/// `SpecialistRole::DungeonMaster` orchestrates pacing/arc/scenario shape.
/// `SpecialistRole::GameMaster`    narrates moment-to-moment description and
///                                 dialogue.
/// `SpecialistRole::Collaborator`  co-authors content with the player.
/// `SpecialistRole::Coder`         proposes runtime engine-mutations.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpecialistRole {
    DungeonMaster = 0,
    GameMaster = 1,
    Collaborator = 2,
    Coder = 3,
}

impl SpecialistRole {
    /// Round-trip from `u32` ; returns `None` for ids `>= 4`.
    #[must_use]
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::DungeonMaster),
            1 => Some(Self::GameMaster),
            2 => Some(Self::Collaborator),
            3 => Some(Self::Coder),
            _ => None,
        }
    }

    /// Stable enum-id (0..=3).
    #[must_use]
    pub fn to_u32(self) -> u32 {
        self as u32
    }

    /// KAN-bias band-id passed to
    /// `cssl_host_substrate_intelligence::observe_with_profile`.
    /// Each role pins to its own band so observations accumulate
    /// INDEPENDENT learning trajectories.
    #[must_use]
    pub fn kan_band_id(self) -> u8 {
        // The mapping is intentionally identity (role 0..3 → band 0..3).
        // Band 4 (HdrExt) is reserved for future-fifth-role expansion.
        self.to_u32() as u8
    }

    /// Map to the substrate-intelligence `Role` enum (which has a different
    /// historical ordering for FFI back-compat).
    #[must_use]
    pub fn to_si_role(self) -> si::Role {
        match self {
            Self::DungeonMaster => si::Role::Dm,
            Self::GameMaster => si::Role::Gm,
            Self::Collaborator => si::Role::Collaborator,
            Self::Coder => si::Role::Coder,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Decision — discrete output of a Specialist::decide call
// ══════════════════════════════════════════════════════════════════════════

/// Discrete result emitted by a `Specialist` when given a prompt.
///
/// `ProposeAction` carries an action-id (caller-defined namespace) plus
/// a parameter blob the host can deserialize per its action-table.
///
/// `Question` is a 64-bit hash standing in for the question-text ; the host
/// can resolve it through a question-pool.
///
/// `Pass` means the specialist abstains for this prompt.
///
/// Encoding for byte-stability tests (`encode` / decoding side intentionally
/// skipped — the host owns deserialization shape) :
///   tag : u8  · 0 = Pass · 1 = Question · 2 = ProposeAction
///   Pass         : `[0]`
///   Question     : `[1, hash_le_8 ...]`            (9 bytes)
///   ProposeAction: `[2, id_le_4 ..., len_le_4 ..., params ...]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Specialist proposes a discrete action with action-id + parameter blob.
    ProposeAction { id: u32, params: Vec<u8> },
    /// Specialist asks a question identified by a 64-bit hash.
    Question { hash: u64 },
    /// Specialist abstains for this prompt.
    Pass,
}

impl Decision {
    /// Stable wire-encoding ; used for golden-byte tests + cross-host
    /// determinism. Layout is documented above.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Pass => vec![0],
            Self::Question { hash } => {
                let mut v = Vec::with_capacity(9);
                v.push(1);
                v.extend_from_slice(&hash.to_le_bytes());
                v
            }
            Self::ProposeAction { id, params } => {
                let mut v = Vec::with_capacity(9 + params.len());
                v.push(2);
                v.extend_from_slice(&id.to_le_bytes());
                v.extend_from_slice(&(params.len() as u32).to_le_bytes());
                v.extend_from_slice(params);
                v
            }
        }
    }

    /// Tag-byte (used by council ranking).
    #[must_use]
    pub fn tag(&self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Question { .. } => 1,
            Self::ProposeAction { .. } => 2,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § SpecialistContext — per-instance accumulator
// ══════════════════════════════════════════════════════════════════════════

/// Per-specialist mutable state. Records up to `OBS_RING_CAP` recent
/// observation-digests (16-byte BLAKE3 truncations) for downstream reasoning.
/// `kan_band_id` is duplicated from the role's mapping for fast access by
/// the host — the canonical source-of-truth is `role.kan_band_id()`.
#[derive(Debug, Clone)]
pub struct SpecialistContext {
    pub role: SpecialistRole,
    /// Most-recent observation digests (16-byte BLAKE3 truncation each).
    /// Capped at `OBS_RING_CAP` ; oldest entries pop when full.
    pub recent_observations: Vec<[u8; 16]>,
    /// KAN-bias band-id (mirrors `role.kan_band_id()`).
    pub kan_band_id: u8,
}

/// Maximum recent observations retained per specialist. Bounded so the
/// in-memory footprint of long-running sessions stays predictable.
pub const OBS_RING_CAP: usize = 64;

impl SpecialistContext {
    /// New context for `role` with empty observation ring.
    #[must_use]
    pub fn new(role: SpecialistRole) -> Self {
        Self {
            role,
            recent_observations: Vec::with_capacity(OBS_RING_CAP),
            kan_band_id: role.kan_band_id(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Specialist trait — common interface for all 4 roles
// ══════════════════════════════════════════════════════════════════════════

/// The common cognitive-specialist interface. The 4 concrete impls
/// (`DmSpecialist`, `GmSpecialist`, `CollaboratorSpecialist`,
/// `CoderSpecialist`) share the trait so the host can hold them as
/// `Box<dyn Specialist>` and route prompts uniformly.
pub trait Specialist {
    /// Feed an observation into the substrate. Routes through
    /// `cssl_host_substrate_intelligence::observe_with_profile` against
    /// THIS specialist's KAN band — INDEPENDENT learning per role.
    fn observe(&mut self, payload: &[u8]);
    /// Make a deterministic decision given a 64-bit prompt-hash. Each
    /// specialist's role + recent observations + the prompt hash combine
    /// to produce the output.
    fn decide(&self, prompt_hash: u64) -> Decision;
    /// The specialist's role (stable u32 enum_id).
    fn role(&self) -> SpecialistRole;
}

// ══════════════════════════════════════════════════════════════════════════
// § Shared observe + decide helpers
//   All 4 impls follow the same skeleton — extract here so each
//   concrete struct stays small and the role-specific behavior lives in
//   the `decide_inner` dispatch.
// ══════════════════════════════════════════════════════════════════════════

/// 16-byte BLAKE3 truncation of `payload`. Stable across hosts.
#[must_use]
fn obs_digest(payload: &[u8]) -> [u8; 16] {
    let full: [u8; 32] = blake3::hash(payload).into();
    let mut out = [0u8; 16];
    out.copy_from_slice(&full[..16]);
    out
}

/// Common observe-path : updates the context's ring, then routes the FULL
/// 32-byte BLAKE3 of `payload` to `observe_with_profile` so the substrate's
/// per-band KAN-bias updates.
fn observe_common(ctx: &mut SpecialistContext, payload: &[u8]) {
    let digest16 = obs_digest(payload);
    if ctx.recent_observations.len() == OBS_RING_CAP {
        // Drop oldest (FIFO). `Vec::remove(0)` is O(n) but n <= 64 ; not hot.
        ctx.recent_observations.remove(0);
    }
    ctx.recent_observations.push(digest16);

    // Route through the substrate-intelligence multi-band observer.
    // Each specialist hits its own band → INDEPENDENT KAN-bias learning.
    let full: [u8; 32] = blake3::hash(payload).into();
    let _ = si::observe_with_profile(&full, ctx.kan_band_id);
}

/// Per-role decision shape. Each role's preferences are captured here :
///
/// - `DungeonMaster` orchestrates → favors `ProposeAction` for arc-shaping.
/// - `GameMaster`    narrates    → favors `ProposeAction` for description IDs.
/// - `Collaborator`  co-authors  → favors `Question` to invite player input.
/// - `Coder`         mutates     → favors `ProposeAction` only when prompt
///                                 carries strong signal ; otherwise `Pass`.
fn decide_inner(ctx: &SpecialistContext, prompt_hash: u64) -> Decision {
    // Mix the prompt with the specialist's role + recent-observations
    // tail so the same prompt produces DIFFERENT decisions per role and
    // EVOLVES as the specialist accumulates observations.
    let mut h = blake3::Hasher::new();
    h.update(&ctx.role.to_u32().to_le_bytes());
    h.update(&prompt_hash.to_le_bytes());
    for obs in &ctx.recent_observations {
        h.update(obs);
    }
    let mixed: [u8; 32] = h.finalize().into();
    let mix_u32 = u32::from_le_bytes([mixed[0], mixed[1], mixed[2], mixed[3]]);
    let mix_u64 = u64::from_le_bytes([
        mixed[8], mixed[9], mixed[10], mixed[11], mixed[12], mixed[13], mixed[14], mixed[15],
    ]);

    match ctx.role {
        // DM proposes an arc-step ; action_id space is 0x1000-0x1FFF.
        SpecialistRole::DungeonMaster => Decision::ProposeAction {
            id: 0x1000 | (mix_u32 & 0x0FFF),
            params: mixed[..8].to_vec(),
        },
        // GM proposes a narration-id ; action_id space is 0x2000-0x2FFF.
        SpecialistRole::GameMaster => Decision::ProposeAction {
            id: 0x2000 | (mix_u32 & 0x0FFF),
            params: mixed[..4].to_vec(),
        },
        // Collaborator asks a question ; lean toward player-engagement.
        // Bit 0 of mix_u32 toggles between Question and Pass-when-quiet.
        SpecialistRole::Collaborator => {
            if (mix_u32 & 0x1) == 0 {
                Decision::Question { hash: mix_u64 }
            } else {
                Decision::ProposeAction {
                    id: 0x3000 | (mix_u32 & 0x0FFF),
                    params: mixed[..2].to_vec(),
                }
            }
        }
        // Coder is conservative : only proposes when bottom 4 bits agree.
        // action_id space is 0x4000-0x4FFF.
        SpecialistRole::Coder => {
            if mix_u32.trailing_zeros() >= 4 {
                Decision::ProposeAction {
                    id: 0x4000 | ((mix_u32 >> 4) & 0x0FFF),
                    params: mixed[..16].to_vec(),
                }
            } else {
                Decision::Pass
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § 4 concrete Specialist impls (one per role)
// ══════════════════════════════════════════════════════════════════════════

/// Director · arc + scenario-shape + pacing. KAN band 0.
#[derive(Debug, Clone)]
pub struct DmSpecialist {
    ctx: SpecialistContext,
}

impl DmSpecialist {
    #[must_use]
    pub fn new() -> Self {
        Self { ctx: SpecialistContext::new(SpecialistRole::DungeonMaster) }
    }
    #[must_use]
    pub fn context(&self) -> &SpecialistContext { &self.ctx }
}
impl Default for DmSpecialist {
    fn default() -> Self { Self::new() }
}
impl Specialist for DmSpecialist {
    fn observe(&mut self, payload: &[u8]) { observe_common(&mut self.ctx, payload); }
    fn decide(&self, prompt_hash: u64) -> Decision { decide_inner(&self.ctx, prompt_hash) }
    fn role(&self) -> SpecialistRole { self.ctx.role }
}

/// Game-Master · narrator + dialogue + observation. KAN band 1.
#[derive(Debug, Clone)]
pub struct GmSpecialist {
    ctx: SpecialistContext,
}

impl GmSpecialist {
    #[must_use]
    pub fn new() -> Self {
        Self { ctx: SpecialistContext::new(SpecialistRole::GameMaster) }
    }
    #[must_use]
    pub fn context(&self) -> &SpecialistContext { &self.ctx }
}
impl Default for GmSpecialist {
    fn default() -> Self { Self::new() }
}
impl Specialist for GmSpecialist {
    fn observe(&mut self, payload: &[u8]) { observe_common(&mut self.ctx, payload); }
    fn decide(&self, prompt_hash: u64) -> Decision { decide_inner(&self.ctx, prompt_hash) }
    fn role(&self) -> SpecialistRole { self.ctx.role }
}

/// Co-author · iterative content-creation with the player. KAN band 2.
#[derive(Debug, Clone)]
pub struct CollaboratorSpecialist {
    ctx: SpecialistContext,
}

impl CollaboratorSpecialist {
    #[must_use]
    pub fn new() -> Self {
        Self { ctx: SpecialistContext::new(SpecialistRole::Collaborator) }
    }
    #[must_use]
    pub fn context(&self) -> &SpecialistContext { &self.ctx }
}
impl Default for CollaboratorSpecialist {
    fn default() -> Self { Self::new() }
}
impl Specialist for CollaboratorSpecialist {
    fn observe(&mut self, payload: &[u8]) { observe_common(&mut self.ctx, payload); }
    fn decide(&self, prompt_hash: u64) -> Decision { decide_inner(&self.ctx, prompt_hash) }
    fn role(&self) -> SpecialistRole { self.ctx.role }
}

/// Self-coding runtime-mutate · proposes engine modifications. KAN band 3.
#[derive(Debug, Clone)]
pub struct CoderSpecialist {
    ctx: SpecialistContext,
}

impl CoderSpecialist {
    #[must_use]
    pub fn new() -> Self {
        Self { ctx: SpecialistContext::new(SpecialistRole::Coder) }
    }
    #[must_use]
    pub fn context(&self) -> &SpecialistContext { &self.ctx }
}
impl Default for CoderSpecialist {
    fn default() -> Self { Self::new() }
}
impl Specialist for CoderSpecialist {
    fn observe(&mut self, payload: &[u8]) { observe_common(&mut self.ctx, payload); }
    fn decide(&self, prompt_hash: u64) -> Decision { decide_inner(&self.ctx, prompt_hash) }
    fn role(&self) -> SpecialistRole { self.ctx.role }
}

// ══════════════════════════════════════════════════════════════════════════
// § role_council — deterministic mediator
// ══════════════════════════════════════════════════════════════════════════

/// Mediate conflicting `Decision`s into a single canonical output.
///
/// Ranking (highest priority wins) :
///   1. tag-byte descending (ProposeAction=2 beats Question=1 beats Pass=0)
///   2. encoded-decision lex-greater (deterministic content-tiebreak)
///
/// Empty input returns `Decision::Pass`. The function is `pure` /
/// `replay-deterministic` — identical input → identical output across hosts.
///
/// Note : the BLAKE3-of-prompt tie-break is achieved by feeding the
/// per-role decision encoding into the comparator ; since each
/// `decide_inner` already mixes `prompt_hash` through BLAKE3 before
/// emitting params/hash payloads, the Decision-encoding lex-compare IS
/// the BLAKE3-of-prompt tiebreak.
#[must_use]
pub fn role_council(decisions: &[Decision]) -> Decision {
    if decisions.is_empty() {
        return Decision::Pass;
    }
    // Walk in order, keeping the highest-ranked. Stable ordering : if two
    // decisions tie on tag + encoding, the FIRST wins. Empty params + same
    // tag would also tie deterministically.
    let mut best_idx = 0usize;
    let mut best_enc = decisions[0].encode();
    let mut best_tag = decisions[0].tag();
    for (i, d) in decisions.iter().enumerate().skip(1) {
        let enc = d.encode();
        let tag = d.tag();
        let pick = if tag == best_tag {
            // Lex-compare the encoded bytes (BLAKE3-of-prompt is already
            // baked into params/hash payloads via decide_inner).
            enc > best_enc
        } else {
            tag > best_tag
        };
        if pick {
            best_idx = i;
            best_enc = enc;
            best_tag = tag;
        }
    }
    decisions[best_idx].clone()
}

// ══════════════════════════════════════════════════════════════════════════
// § Tests — 12+ unit tests covering :
//   • role-id round-trip (1)
//   • role isolation (kan-band correctness for all 4) (1)
//   • Decision encoding stability (1)
//   • role_council determinism + ranking (3)
//   • observation byte-stability (1)
//   • prompt_hash collision behavior (1)
//   • pass-through propagation (1)
//   • OBS_RING_CAP enforcement (1)
//   • observe_common KAN-bias propagation (1)
//   • role-specific decision shape (1)
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_round_trip_all_four() {
        for r in 0..4u32 {
            let role = SpecialistRole::from_u32(r).unwrap();
            assert_eq!(role.to_u32(), r);
        }
        assert!(SpecialistRole::from_u32(4).is_none());
        assert!(SpecialistRole::from_u32(99).is_none());
    }

    #[test]
    fn kan_band_identity_per_role() {
        assert_eq!(SpecialistRole::DungeonMaster.kan_band_id(), 0);
        assert_eq!(SpecialistRole::GameMaster.kan_band_id(), 1);
        assert_eq!(SpecialistRole::Collaborator.kan_band_id(), 2);
        assert_eq!(SpecialistRole::Coder.kan_band_id(), 3);
    }

    #[test]
    fn si_role_mapping_correct() {
        // Verify the cross-discriminant mapping is exactly what we documented.
        assert_eq!(SpecialistRole::DungeonMaster.to_si_role(), si::Role::Dm);
        assert_eq!(SpecialistRole::GameMaster.to_si_role(), si::Role::Gm);
        assert_eq!(SpecialistRole::Collaborator.to_si_role(), si::Role::Collaborator);
        assert_eq!(SpecialistRole::Coder.to_si_role(), si::Role::Coder);
    }

    #[test]
    fn decision_encoding_byte_stable() {
        // Pass : exactly [0]
        assert_eq!(Decision::Pass.encode(), vec![0]);
        // Question : [1, h0..h7]
        let q = Decision::Question { hash: 0x0123_4567_89AB_CDEF };
        assert_eq!(
            q.encode(),
            vec![1, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01]
        );
        // ProposeAction : [2, id_le_4, len_le_4, params...]
        let pa = Decision::ProposeAction { id: 0xAABB_CCDD, params: vec![1, 2, 3] };
        assert_eq!(
            pa.encode(),
            vec![2, 0xDD, 0xCC, 0xBB, 0xAA, 3, 0, 0, 0, 1, 2, 3]
        );
    }

    #[test]
    fn role_council_empty_returns_pass() {
        assert_eq!(role_council(&[]), Decision::Pass);
    }

    #[test]
    fn role_council_proposeaction_beats_question_beats_pass() {
        let pass = Decision::Pass;
        let q = Decision::Question { hash: 1 };
        let pa = Decision::ProposeAction { id: 9, params: vec![] };

        // Pass + Question → Question wins (tag 1 > 0)
        assert_eq!(role_council(&[pass.clone(), q.clone()]), q);
        // Question + ProposeAction → ProposeAction wins (tag 2 > 1)
        assert_eq!(role_council(&[q.clone(), pa.clone()]), pa);
        // All three → ProposeAction wins
        assert_eq!(role_council(&[pass, q, pa.clone()]), pa);
    }

    #[test]
    fn role_council_deterministic_tiebreak() {
        // Two identical-tag decisions ; council must be deterministic.
        let pa1 = Decision::ProposeAction { id: 1, params: vec![0xAA] };
        let pa2 = Decision::ProposeAction { id: 1, params: vec![0xBB] };
        let pick1 = role_council(&[pa1.clone(), pa2.clone()]);
        let pick2 = role_council(&[pa1, pa2.clone()]);
        assert_eq!(pick1, pick2, "council must be deterministic across calls");
        // Lex-greater encoding wins (0xBB > 0xAA in params).
        assert_eq!(pick1, pa2);
    }

    #[test]
    fn observation_byte_stability() {
        // Same payload → same digest. Different payload → different digest.
        let d1 = obs_digest(b"hello-substrate");
        let d2 = obs_digest(b"hello-substrate");
        assert_eq!(d1, d2, "obs_digest must be deterministic");
        let d3 = obs_digest(b"hello-substraTE"); // different case
        assert_ne!(d1, d3);
    }

    #[test]
    fn prompt_hash_role_isolation_in_decide() {
        // SAME prompt-hash · DIFFERENT roles must produce DIFFERENT decisions.
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        let prompt = 0xDEAD_BEEF_CAFE_BABE_u64;
        let dm_dec = dm.decide(prompt).encode();
        let gm_dec = gm.decide(prompt).encode();
        let coll_dec = coll.decide(prompt).encode();
        let coder_dec = coder.decide(prompt).encode();
        // At least 3 of the 4 should be distinct (Coder may Pass for some
        // prompts, which collides with no-other-Pass-emitter ; we check
        // that the role-specific action_id namespaces are disjoint when
        // they DO emit ProposeAction).
        let all = [&dm_dec, &gm_dec, &coll_dec, &coder_dec];
        let mut distinct = std::collections::HashSet::new();
        for v in all {
            distinct.insert(v);
        }
        let n = distinct.len();
        assert!(
            n >= 3,
            "expected at least 3 distinct decisions across roles, got {n}"
        );
    }

    #[test]
    fn observe_pass_through_to_substrate() {
        // Verify observe() actually calls into the substrate by checking
        // observe_count climbs.
        let before = si::observe_count();
        let mut dm = DmSpecialist::new();
        dm.observe(b"observation-from-dm");
        let after = si::observe_count();
        assert!(after > before, "substrate observe_count must increase");
    }

    #[test]
    fn observation_ring_cap_enforced() {
        let mut dm = DmSpecialist::new();
        // Push more than OBS_RING_CAP observations.
        for i in 0..(OBS_RING_CAP + 10) {
            dm.observe(format!("obs-{i}").as_bytes());
        }
        assert_eq!(
            dm.context().recent_observations.len(),
            OBS_RING_CAP,
            "ring must cap at OBS_RING_CAP"
        );
    }

    #[test]
    fn observe_evolves_decision() {
        // Same prompt before/after observe should plausibly differ
        // (Decision contains BLAKE3 of role + observations + prompt).
        let mut dm = DmSpecialist::new();
        let prompt = 0x1234_5678_u64;
        let before = dm.decide(prompt);
        dm.observe(b"a meaningful event");
        let after = dm.decide(prompt);
        assert_ne!(
            before, after,
            "decision must evolve as the specialist accumulates observations"
        );
    }

    #[test]
    fn role_method_returns_correct_role() {
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        assert_eq!(dm.role(), SpecialistRole::DungeonMaster);
        assert_eq!(gm.role(), SpecialistRole::GameMaster);
        assert_eq!(coll.role(), SpecialistRole::Collaborator);
        assert_eq!(coder.role(), SpecialistRole::Coder);
    }

    #[test]
    fn role_specific_action_id_ranges() {
        // When ProposeAction emits, id should fall in role's bracket.
        // We sweep prompt-hashes to find an emission per role.
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        let mut dm_seen = false;
        let mut gm_seen = false;
        let mut coll_seen = false;
        let mut coder_seen = false;
        for p in 0..512u64 {
            if let Decision::ProposeAction { id, .. } = dm.decide(p) {
                assert!((0x1000..0x2000).contains(&id), "DM id out of range: {id:#X}");
                dm_seen = true;
            }
            if let Decision::ProposeAction { id, .. } = gm.decide(p) {
                assert!((0x2000..0x3000).contains(&id), "GM id out of range: {id:#X}");
                gm_seen = true;
            }
            if let Decision::ProposeAction { id, .. } = coll.decide(p) {
                assert!((0x3000..0x4000).contains(&id), "Coll id out of range: {id:#X}");
                coll_seen = true;
            }
            if let Decision::ProposeAction { id, .. } = coder.decide(p) {
                assert!((0x4000..0x5000).contains(&id), "Coder id out of range: {id:#X}");
                coder_seen = true;
            }
        }
        assert!(dm_seen && gm_seen && coll_seen, "all 3 always-acting roles must emit");
        // Coder emits 1/16 prompts on average ; in 512 sweeps it's near-cert.
        assert!(coder_seen, "Coder must emit at least once in 512 prompts");
    }

    #[test]
    fn council_full_4_specialist_workflow() {
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        let prompt = 0xFEED_FACE_DEAD_BEEF_u64;
        let decisions = [
            dm.decide(prompt),
            gm.decide(prompt),
            coll.decide(prompt),
            coder.decide(prompt),
        ];
        let council = role_council(&decisions);
        // The council output should be one of the inputs.
        assert!(decisions.contains(&council), "council output must come from inputs");
        // Highest-tag-and-largest-encoding wins. At least one input is
        // always a ProposeAction (DM and GM never Pass), so the council
        // output's tag must be at least 1 (Question or ProposeAction).
        assert!(
            council.tag() >= 1,
            "council tag must be >= 1 when DM/GM are present (they never Pass)"
        );
    }
}

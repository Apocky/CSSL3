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
//!
//! § PERSISTENCE — sovereignty-attested
//!   `specialist_persist` / `specialist_load` write a versioned binary file
//!   to a caller-supplied `&Path` (default-helper points at `~/.loa/`).
//!   The file format is a stable v0x01 layout :
//!
//!   ```text
//!   [0]               : SPECIALIST_PERSIST_FORMAT_V1 (= 0x01)
//!   [1]               : SpecialistRole::to_u32() as u8 (0..=3)
//!   [2]               : kan_band_id (0..=3 today · 0..=4 future)
//!   [3..=4]           : recent_observations.len() LE u16 (0..=OBS_RING_CAP)
//!   [5..5+16N]        : N × 16-byte BLAKE3-truncated digests (FIFO order)
//!   [5+16N..5+16N+32] : 32-byte BLAKE3 digest of all preceding bytes
//!   ```
//!
//!   `specialist_council_persist` packs all 4 specialists with framing.
//!   Format v0x10 :
//!
//!   ```text
//!   [0]               : COUNCIL_PERSIST_FORMAT_V1 (= 0x10)
//!   [1]               : count (4 today · 0..=255 future)
//!   [2..2+2*count]    : count × u16-LE record-sizes
//!   [..]              : concatenated individual specialist records
//!   [end-32..end]     : 32-byte BLAKE3 digest of all preceding bytes
//!   ```
//!
//!   § PRIME-DIRECTIVE compliance
//!   - LOCAL-ONLY · paths must be local filesystem paths. Callers passing
//!     a network/cloud path bypass this attestation.
//!   - NO TELEMETRY · persistence does not phone home, does not emit
//!     metrics, does not log payloads.
//!   - BLAKE3-INTEGRITY · every persisted file ends with a 32-byte BLAKE3
//!     digest of all preceding bytes. Tampering is detected on load and
//!     rejected with `ErrorKind::InvalidData`.
//!   - SOVEREIGN-OPT-OUT · setting environment variable
//!     `LOA_SPECIALIST_NO_PERSIST=1` makes `specialist_persist` and
//!     `specialist_council_persist` no-op (returns `Ok(())` without
//!     writing). The user owns every byte that hits disk on their
//!     substrate.
//!   - FORWARD-COMPAT-READERS · the v0x01 / v0x10 byte tags are reserved.
//!     A future v0x02 reader will accept v0x01 files (BLAKE3-verified)
//!     so existing learning persists across upgrades.

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
// § Persistence — sovereignty-attested binary roundtrip with BLAKE3 integrity
//   Mirrors the kan_bias_persist / kan_bias_load pattern in
//   cssl-host-substrate-intelligence (format v0x02) ; this layer is the
//   *specialist-context* persistence (one file per specialist · one file
//   per council · same versioning discipline · same BLAKE3 integrity).
// ══════════════════════════════════════════════════════════════════════════

/// File-format header byte for `specialist_persist` v1 records.
///
/// A future v2 reader MUST still accept v1 files (mirrors the
/// `kan_bias_load` v1/v2 forward-compat pattern in
/// `cssl-host-substrate-intelligence`).
pub const SPECIALIST_PERSIST_FORMAT_V1: u8 = 0x01;

/// File-format header byte for `specialist_council_persist` v1 records.
///
/// Distinct from `SPECIALIST_PERSIST_FORMAT_V1` so a corrupt-or-mistyped
/// load attempt against the wrong file shape is rejected on header byte
/// before BLAKE3 verification — fail-fast on shape mismatch.
pub const COUNCIL_PERSIST_FORMAT_V1: u8 = 0x10;

/// Environment-variable that, when set to `"1"`, makes `specialist_persist`
/// and `specialist_council_persist` no-op (return `Ok(())` without
/// writing). The user remains in control of every byte that hits disk.
pub const ENV_NO_PERSIST: &str = "LOA_SPECIALIST_NO_PERSIST";

/// Single specialist-record overhead (header + role + band + ring-len + digest)
/// = 1 + 1 + 1 + 2 + 32 = 37 bytes minimum (zero observations).
pub const SPECIALIST_RECORD_MIN: usize = 1 + 1 + 1 + 2 + 32;
/// Maximum specialist-record size = 37 + 16 × `OBS_RING_CAP` = 1061 bytes.
/// Exposed as `pub` so callers can pre-size buffers and verify byte-stable
/// invariants in their own integration tests.
pub const SPECIALIST_RECORD_MAX: usize = SPECIALIST_RECORD_MIN + 16 * OBS_RING_CAP;

/// Check the sovereign opt-out env-var. Returns true when persistence
/// should be skipped (env-var set to "1").
fn no_persist_opt_out() -> bool {
    std::env::var(ENV_NO_PERSIST).ok().as_deref() == Some("1")
}

/// Encode a `SpecialistContext` into a v1 byte-record (header + body +
/// BLAKE3-digest). Pure function ; no I/O.
fn encode_specialist_v1(ctx: &SpecialistContext) -> Vec<u8> {
    let n = ctx.recent_observations.len();
    debug_assert!(n <= OBS_RING_CAP, "ring overflow ; OBS_RING_CAP enforced by observe_common");
    let body_len = 1 + 1 + 1 + 2 + 16 * n; // header + role + band + ring-len + payload
    let mut buf = Vec::with_capacity(body_len + 32);
    buf.push(SPECIALIST_PERSIST_FORMAT_V1);
    buf.push(ctx.role.to_u32() as u8);
    buf.push(ctx.kan_band_id);
    buf.extend_from_slice(&(n as u16).to_le_bytes());
    for obs in &ctx.recent_observations {
        buf.extend_from_slice(obs);
    }
    // BLAKE3 of header+body, appended.
    let digest: [u8; 32] = blake3::hash(&buf).into();
    buf.extend_from_slice(&digest);
    buf
}

/// Decode a v1 byte-record back into a `SpecialistContext` after verifying
/// header byte + role-id + ring-len bounds + BLAKE3 digest.
fn decode_specialist_v1(bytes: &[u8]) -> std::io::Result<SpecialistContext> {
    use std::io::{Error, ErrorKind};

    if bytes.len() < SPECIALIST_RECORD_MIN {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "specialist record too short : {} < {} bytes",
                bytes.len(),
                SPECIALIST_RECORD_MIN
            ),
        ));
    }
    if bytes[0] != SPECIALIST_PERSIST_FORMAT_V1 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "unknown specialist persist version : 0x{:02X} (expected 0x{:02X})",
                bytes[0], SPECIALIST_PERSIST_FORMAT_V1
            ),
        ));
    }

    let role_byte = bytes[1];
    let Some(role) = SpecialistRole::from_u32(u32::from(role_byte)) else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("invalid role-id byte : {role_byte}"),
        ));
    };
    let kan_band_id = bytes[2];
    let ring_len = u16::from_le_bytes([bytes[3], bytes[4]]) as usize;
    if ring_len > OBS_RING_CAP {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("ring-len {ring_len} exceeds OBS_RING_CAP {OBS_RING_CAP}"),
        ));
    }

    let expected_total = SPECIALIST_RECORD_MIN + 16 * ring_len;
    if bytes.len() != expected_total {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "specialist record length mismatch : got {} · expected {} (ring-len {ring_len})",
                bytes.len(),
                expected_total
            ),
        ));
    }

    // Verify BLAKE3 digest on header+body (everything except the trailing
    // 32-byte digest).
    let body_end = expected_total - 32;
    let computed: [u8; 32] = blake3::hash(&bytes[..body_end]).into();
    let stored = &bytes[body_end..];
    if computed.as_slice() != stored {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "BLAKE3 integrity check failed on specialist record",
        ));
    }

    // Reconstruct the observation ring.
    let mut obs = Vec::with_capacity(ring_len);
    for i in 0..ring_len {
        let off = 5 + 16 * i;
        let mut d = [0u8; 16];
        d.copy_from_slice(&bytes[off..off + 16]);
        obs.push(d);
    }
    Ok(SpecialistContext {
        role,
        recent_observations: obs,
        kan_band_id,
    })
}

/// Persist a `SpecialistContext` to `path` using format v0x01 (1 + 1 + 1 +
/// 2 + 16N + 32 bytes). Creates parent directories on demand.
///
/// Returns `Ok(())` on success or when `LOA_SPECIALIST_NO_PERSIST=1` is
/// set in the environment (sovereign opt-out — no bytes hit disk).
///
/// Errors are I/O errors from filesystem-write or directory-creation.
///
/// # Examples
///
/// ```no_run
/// use cssl_host_dmgm_specialists::{DmSpecialist, Specialist, specialist_persist};
/// let mut dm = DmSpecialist::new();
/// dm.observe(b"a learning event");
/// let path = std::env::temp_dir().join("dm_persist.bin");
/// specialist_persist(dm.context(), &path).expect("persist roundtrip");
/// ```
pub fn specialist_persist(
    context: &SpecialistContext,
    path: &std::path::Path,
) -> std::io::Result<()> {
    if no_persist_opt_out() {
        return Ok(());
    }
    let buf = encode_specialist_v1(context);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &buf)
}

/// Load a `SpecialistContext` from `path` ; verifies header byte +
/// ring-length bounds + BLAKE3 digest before returning.
///
/// Returns `Err(ErrorKind::InvalidData)` on : unknown header byte ·
/// ring-len exceeding `OBS_RING_CAP` · length mismatch · BLAKE3 digest
/// mismatch.
///
/// Returns `Err(ErrorKind::NotFound)` when the file does not exist.
///
/// # Examples
///
/// ```no_run
/// use cssl_host_dmgm_specialists::specialist_load;
/// let path = std::env::temp_dir().join("dm_persist.bin");
/// let ctx = specialist_load(&path).expect("file readable + integrity ok");
/// assert_eq!(ctx.role.to_u32(), 0); // DungeonMaster
/// ```
pub fn specialist_load(path: &std::path::Path) -> std::io::Result<SpecialistContext> {
    let bytes = std::fs::read(path)?;
    decode_specialist_v1(&bytes)
}

/// Persist all specialists in a council into a single file using format
/// v0x10. Sizes-table + concatenated records + trailing BLAKE3 digest.
///
/// Returns `Ok(())` on `LOA_SPECIALIST_NO_PERSIST=1`.
///
/// # Errors
/// Filesystem-write failures · directory-create failures · empty input
/// (`InvalidInput`) · count > 255 (`InvalidInput` ; format limit).
pub fn specialist_council_persist(
    specialists: &[&dyn Specialist],
    path: &std::path::Path,
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    if no_persist_opt_out() {
        return Ok(());
    }
    if specialists.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "council persist requires at least 1 specialist",
        ));
    }
    if specialists.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "council persist supports at most 255 specialists",
        ));
    }

    // Encode each specialist via the v1 record path. We need a way to
    // pull the SpecialistContext from a `&dyn Specialist` ; the trait
    // surface intentionally hides ctx for impl-flexibility, so we
    // reconstruct an equivalent context by reading back the stable
    // public fields (role + kan-band) and replaying observations.
    //
    // Today all 4 concrete impls expose `context()` but the trait does
    // not. We add a private helper that requires concrete dispatch.
    // For cross-impl uniformity we use the council-helper that invokes
    // each specialist's `role()` → recovers a fresh-default context
    // EXCEPT for the recent_observations, which are accumulator state.
    //
    // To preserve learning, callers should use the typed-array overload
    // `specialist_council_persist_typed` defined below ; this dyn-trait
    // entry-point persists a fresh-default context per role (which is
    // still useful for council-shape-only persistence, e.g. role
    // assignments without learned observations). The richer overload
    // is what loa-host should call.
    let mut records: Vec<Vec<u8>> = Vec::with_capacity(specialists.len());
    for s in specialists {
        let ctx = SpecialistContext::new(s.role());
        records.push(encode_specialist_v1(&ctx));
    }

    write_council_v1(&records, path)
}

/// Typed-array overload of `specialist_council_persist` — preserves each
/// specialist's `recent_observations` because it accepts concrete
/// `SpecialistContext` references (which expose the ring directly).
/// loa-host uses this entry-point so council-state survives restart with
/// learning intact.
///
/// # Errors
/// Same as `specialist_council_persist`.
pub fn specialist_council_persist_typed(
    contexts: &[&SpecialistContext],
    path: &std::path::Path,
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    if no_persist_opt_out() {
        return Ok(());
    }
    if contexts.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "council persist requires at least 1 context",
        ));
    }
    if contexts.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "council persist supports at most 255 contexts",
        ));
    }

    let records: Vec<Vec<u8>> = contexts.iter().map(|c| encode_specialist_v1(c)).collect();
    write_council_v1(&records, path)
}

/// Common persistence path for council records — assembles the framing,
/// computes BLAKE3, writes to disk.
fn write_council_v1(records: &[Vec<u8>], path: &std::path::Path) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    let count = records.len();
    let table_len = 2 * count;
    let payload_len: usize = records.iter().map(Vec::len).sum();
    // Each individual record ≤ SPECIALIST_RECORD_MAX = 1061 ; u16 sizes-
    // table is fine. Validate before encoding into u16.
    for (i, r) in records.iter().enumerate() {
        if r.len() > u16::MAX as usize {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("specialist record {i} exceeds u16 size limit"),
            ));
        }
    }

    let body_len = 2 + table_len + payload_len; // header + count + sizes + payloads
    let mut buf = Vec::with_capacity(body_len + 32);
    buf.push(COUNCIL_PERSIST_FORMAT_V1);
    buf.push(count as u8);
    for r in records {
        buf.extend_from_slice(&(r.len() as u16).to_le_bytes());
    }
    for r in records {
        buf.extend_from_slice(r);
    }
    let digest: [u8; 32] = blake3::hash(&buf).into();
    buf.extend_from_slice(&digest);

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, &buf)
}

/// Load a council file and return a `Vec<SpecialistContext>` reconstructed
/// from each individual specialist record.
///
/// # Errors
/// `InvalidData` on : bad header byte · count = 0 · ring-len exceeding
/// `OBS_RING_CAP` · individual record corrupt · trailing BLAKE3 mismatch.
pub fn specialist_council_load(
    path: &std::path::Path,
) -> std::io::Result<Vec<SpecialistContext>> {
    use std::io::{Error, ErrorKind};

    let bytes = std::fs::read(path)?;
    if bytes.len() < 2 + 32 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("council file too short : {} bytes", bytes.len()),
        ));
    }
    if bytes[0] != COUNCIL_PERSIST_FORMAT_V1 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "unknown council persist version : 0x{:02X} (expected 0x{:02X})",
                bytes[0], COUNCIL_PERSIST_FORMAT_V1
            ),
        ));
    }
    let count = bytes[1] as usize;
    if count == 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "council file declares zero specialists",
        ));
    }
    let table_off = 2usize;
    let table_len = 2 * count;
    if bytes.len() < table_off + table_len + 32 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "council file shorter than declared sizes table",
        ));
    }

    // Read sizes table.
    let mut sizes = Vec::with_capacity(count);
    let mut total_payload = 0usize;
    for i in 0..count {
        let off = table_off + 2 * i;
        let s = u16::from_le_bytes([bytes[off], bytes[off + 1]]) as usize;
        sizes.push(s);
        total_payload = total_payload.checked_add(s).ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "council payload overflow")
        })?;
    }

    let payload_off = table_off + table_len;
    let body_end = payload_off + total_payload;
    if bytes.len() != body_end + 32 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "council file length mismatch : got {} · expected {}",
                bytes.len(),
                body_end + 32
            ),
        ));
    }

    // Verify trailing BLAKE3 digest.
    let computed: [u8; 32] = blake3::hash(&bytes[..body_end]).into();
    if computed.as_slice() != &bytes[body_end..] {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "BLAKE3 integrity check failed on council file",
        ));
    }

    // Decode each specialist record.
    let mut out = Vec::with_capacity(count);
    let mut cursor = payload_off;
    for size in sizes {
        let end = cursor + size;
        let ctx = decode_specialist_v1(&bytes[cursor..end])?;
        out.push(ctx);
        cursor = end;
    }
    Ok(out)
}

/// Default per-role persist path under `~/.loa/specialist_<role>.bin`.
///
/// Returns `~/.loa/specialist_dm.bin` for DM, `..._gm.bin` for GM,
/// `..._coll.bin` for Collaborator, `..._coder.bin` for Coder.
///
/// Falls back to `./specialist_<role>.bin` if the user's home directory
/// cannot be resolved (rare on Win32+macOS+Linux ; prevents panic).
#[must_use]
pub fn specialist_default_persist_path(role: SpecialistRole) -> std::path::PathBuf {
    let suffix = match role {
        SpecialistRole::DungeonMaster => "specialist_dm.bin",
        SpecialistRole::GameMaster => "specialist_gm.bin",
        SpecialistRole::Collaborator => "specialist_coll.bin",
        SpecialistRole::Coder => "specialist_coder.bin",
    };
    // Resolve ~/.loa/ — Windows uses USERPROFILE, Unix uses HOME. We
    // intentionally avoid the `dirs` crate dependency to keep this
    // crate's tree narrow and fully on-substrate.
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    home.map_or_else(
        || std::path::PathBuf::from(suffix),
        |h| h.join(".loa").join(suffix),
    )
}

/// Default council persist path : `~/.loa/specialist_council.bin`.
#[must_use]
pub fn specialist_council_default_persist_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    home.map_or_else(
        || std::path::PathBuf::from("specialist_council.bin"),
        |h| h.join(".loa").join("specialist_council.bin"),
    )
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

    // ══════════════════════════════════════════════════════════════════
    // § Persistence tests — sovereignty-attested binary roundtrip
    //   8+ tests : roundtrip-bytes · invalid-version-rejected ·
    //   invalid-checksum-rejected · ring-len-bounds · empty-ring ·
    //   empty-file · file-not-exists · BLAKE3-digest-correct ·
    //   council-roundtrip · env-var-no-persist · per-role-default-path ·
    //   ring-cap-enforced-on-load.
    // ══════════════════════════════════════════════════════════════════

    /// Build a tempfile path inside std::env::temp_dir that is unique
    /// per test invocation (PID + a per-test discriminator).
    fn tmpfile(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("cssl_specialist_persist_test");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("{name}_{}.bin", std::process::id()))
    }

    /// Process-wide test-mutex so env-var manipulation in one test does
    /// not race with other parallel persistence tests reading
    /// `LOA_SPECIALIST_NO_PERSIST`. Mirrors the `KAN_TEST_LOCK` pattern
    /// in `cssl-host-substrate-intelligence`.
    static PERSIST_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn persist_roundtrip_empty_ring() {
        // Sovereignty : the env-var must be unset for this test.
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let dm = DmSpecialist::new();
        // No observations.
        assert_eq!(dm.context().recent_observations.len(), 0);

        let path = tmpfile("dm_empty");
        specialist_persist(dm.context(), &path).expect("persist ok");
        let loaded = specialist_load(&path).expect("load ok");
        assert_eq!(loaded.role, SpecialistRole::DungeonMaster);
        assert_eq!(loaded.kan_band_id, 0);
        assert_eq!(loaded.recent_observations.len(), 0);
        let _ = std::fs::remove_file(&path);

        // Ensure the saved bytes are exactly the minimum record size.
        // (Re-persist + read len.)
        specialist_persist(dm.context(), &path).expect("persist ok");
        let bytes = std::fs::read(&path).expect("readable");
        assert_eq!(bytes.len(), SPECIALIST_RECORD_MIN, "min record = 37 bytes");
        // Use dm so we don't trip an unused warning.
        let _ = dm.role();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_roundtrip_with_observations() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let mut gm = GmSpecialist::new();
        for i in 0..7u8 {
            gm.observe(format!("ev-{i}-payload").as_bytes());
        }
        let path = tmpfile("gm_obs7");
        specialist_persist(gm.context(), &path).expect("persist ok");
        let loaded = specialist_load(&path).expect("load ok");
        assert_eq!(loaded.role, SpecialistRole::GameMaster);
        assert_eq!(loaded.kan_band_id, 1);
        assert_eq!(loaded.recent_observations.len(), 7);
        // Each observation digest must match the source ring.
        for (a, b) in gm.context().recent_observations.iter().zip(&loaded.recent_observations) {
            assert_eq!(a, b, "observation digest must roundtrip byte-stable");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_full_ring_cap() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let mut coder = CoderSpecialist::new();
        for i in 0..(OBS_RING_CAP + 5) {
            coder.observe(format!("event-{i}").as_bytes());
        }
        // Ring should be capped at OBS_RING_CAP.
        assert_eq!(coder.context().recent_observations.len(), OBS_RING_CAP);

        let path = tmpfile("coder_full");
        specialist_persist(coder.context(), &path).expect("persist ok");
        let bytes = std::fs::read(&path).expect("readable");
        // Max record size = 37 + 16 * 64 = 1061 bytes.
        assert_eq!(bytes.len(), SPECIALIST_RECORD_MAX, "max record = 1061 bytes");

        let loaded = specialist_load(&path).expect("load ok");
        assert_eq!(loaded.recent_observations.len(), OBS_RING_CAP);
        assert_eq!(loaded.role, SpecialistRole::Coder);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_rejects_invalid_version() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let path = tmpfile("bad_version");
        // Header byte 0xFF instead of SPECIALIST_PERSIST_FORMAT_V1.
        let mut buf = vec![0u8; SPECIALIST_RECORD_MIN];
        buf[0] = 0xFF;
        // Even include a valid BLAKE3 over the first (min-32) bytes so we
        // confirm rejection happens on header byte BEFORE digest check.
        let body_end = SPECIALIST_RECORD_MIN - 32;
        let digest: [u8; 32] = blake3::hash(&buf[..body_end]).into();
        buf[body_end..].copy_from_slice(&digest);
        std::fs::write(&path, &buf).expect("fixture writes");

        let err = specialist_load(&path).expect_err("must reject bad version");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("unknown specialist persist version"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_rejects_invalid_checksum() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let mut dm = DmSpecialist::new();
        dm.observe(b"some event");
        let path = tmpfile("bad_checksum");
        specialist_persist(dm.context(), &path).expect("persist ok");

        // Corrupt the trailing digest byte (flip the last byte).
        let mut bytes = std::fs::read(&path).expect("readable");
        let last = bytes.len() - 1;
        bytes[last] = bytes[last].wrapping_add(0x01);
        std::fs::write(&path, &bytes).expect("rewrite");

        let err = specialist_load(&path).expect_err("must reject corrupted digest");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("BLAKE3 integrity check failed"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_rejects_ring_len_too_large() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let path = tmpfile("ring_overflow");
        // Hand-craft a record claiming ring_len = OBS_RING_CAP + 1 = 65.
        let bad_n = (OBS_RING_CAP + 1) as u16;
        let body_len = 5 + 16 * bad_n as usize;
        let mut buf = vec![0u8; body_len];
        buf[0] = SPECIALIST_PERSIST_FORMAT_V1;
        buf[1] = 0; // role DM
        buf[2] = 0; // band 0
        buf[3..5].copy_from_slice(&bad_n.to_le_bytes());
        // Compute valid digest so rejection is triggered by ring-len check.
        let digest: [u8; 32] = blake3::hash(&buf).into();
        buf.extend_from_slice(&digest);
        std::fs::write(&path, &buf).expect("fixture writes");

        let err = specialist_load(&path).expect_err("must reject overflow ring-len");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("ring-len 65 exceeds OBS_RING_CAP"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_rejects_empty_file() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let path = tmpfile("empty");
        std::fs::write(&path, &[] as &[u8]).expect("fixture writes");
        let err = specialist_load(&path).expect_err("must reject empty file");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("specialist record too short"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_rejects_missing_file() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let path = std::env::temp_dir().join("definitely_does_not_exist_xyzzy_12345.bin");
        let _ = std::fs::remove_file(&path); // ensure absent
        let err = specialist_load(&path).expect_err("must reject missing file");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn persist_blake3_digest_byte_correct() {
        // Verify the trailing 32 bytes equal BLAKE3(header + body).
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let mut coll = CollaboratorSpecialist::new();
        coll.observe(b"co-author beat-1");
        coll.observe(b"co-author beat-2");
        let path = tmpfile("digest_check");
        specialist_persist(coll.context(), &path).expect("persist ok");
        let bytes = std::fs::read(&path).expect("readable");
        assert!(bytes.len() >= 32);
        let body_end = bytes.len() - 32;
        let computed: [u8; 32] = blake3::hash(&bytes[..body_end]).into();
        assert_eq!(
            &bytes[body_end..],
            computed.as_slice(),
            "trailing 32 bytes must equal BLAKE3 of preceding bytes"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_council_roundtrip_typed() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let mut dm = DmSpecialist::new();
        let mut gm = GmSpecialist::new();
        let mut coll = CollaboratorSpecialist::new();
        let mut coder = CoderSpecialist::new();
        // Each specialist gets a distinct number of observations.
        for i in 0..3 { dm.observe(format!("dm-{i}").as_bytes()); }
        for i in 0..7 { gm.observe(format!("gm-{i}").as_bytes()); }
        for i in 0..2 { coll.observe(format!("coll-{i}").as_bytes()); }
        for i in 0..5 { coder.observe(format!("coder-{i}").as_bytes()); }

        let path = tmpfile("council_typed");
        let ctxs: Vec<&SpecialistContext> = vec![
            dm.context(),
            gm.context(),
            coll.context(),
            coder.context(),
        ];
        specialist_council_persist_typed(&ctxs, &path).expect("council persist ok");

        let loaded = specialist_council_load(&path).expect("council load ok");
        assert_eq!(loaded.len(), 4);
        assert_eq!(loaded[0].role, SpecialistRole::DungeonMaster);
        assert_eq!(loaded[0].recent_observations.len(), 3);
        assert_eq!(loaded[1].role, SpecialistRole::GameMaster);
        assert_eq!(loaded[1].recent_observations.len(), 7);
        assert_eq!(loaded[2].role, SpecialistRole::Collaborator);
        assert_eq!(loaded[2].recent_observations.len(), 2);
        assert_eq!(loaded[3].role, SpecialistRole::Coder);
        assert_eq!(loaded[3].recent_observations.len(), 5);
        // Verify ring contents byte-for-byte.
        for (a, b) in dm.context().recent_observations.iter().zip(&loaded[0].recent_observations) {
            assert_eq!(a, b);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_council_dyn_trait_path_compiles_and_writes() {
        // The dyn-trait overload doesn't preserve observations (per its
        // doc-comment) — verify it at least produces a loadable file.
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let dm = DmSpecialist::new();
        let gm = GmSpecialist::new();
        let coll = CollaboratorSpecialist::new();
        let coder = CoderSpecialist::new();
        let path = tmpfile("council_dyn");
        let specs: [&dyn Specialist; 4] = [&dm, &gm, &coll, &coder];
        specialist_council_persist(&specs, &path).expect("dyn-trait council persist ok");
        let loaded = specialist_council_load(&path).expect("dyn-trait council load ok");
        assert_eq!(loaded.len(), 4);
        // dyn-trait path drops observations (intentional) — all rings empty.
        for ctx in &loaded {
            assert_eq!(ctx.recent_observations.len(), 0);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_council_rejects_bad_header() {
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let path = tmpfile("council_bad_hdr");
        // Hand-craft a council file with bad header byte.
        let mut buf = vec![0u8; 100];
        buf[0] = 0x55; // not COUNCIL_PERSIST_FORMAT_V1 = 0x10
        std::fs::write(&path, &buf).expect("fixture writes");
        let err = specialist_council_load(&path).expect_err("must reject bad council header");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("unknown council persist version"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_no_persist_env_var_skips_write() {
        // The sovereign opt-out : LOA_SPECIALIST_NO_PERSIST=1 → no bytes
        // hit disk, persist returns Ok(()).
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        let path = tmpfile("no_persist_optout");
        // Ensure the file doesn't exist beforehand.
        let _ = std::fs::remove_file(&path);

        // Set env-var BEFORE any observations so we never write.
        std::env::set_var(ENV_NO_PERSIST, "1");
        let mut dm = DmSpecialist::new();
        dm.observe(b"learning event");
        specialist_persist(dm.context(), &path).expect("opt-out is Ok");
        // File MUST NOT exist after opt-out persist.
        assert!(!path.exists(), "no-persist env-var must skip file write");

        // Council variant also honors the opt-out.
        let council_path = tmpfile("no_persist_council_optout");
        let _ = std::fs::remove_file(&council_path);
        let ctxs: Vec<&SpecialistContext> = vec![dm.context()];
        specialist_council_persist_typed(&ctxs, &council_path).expect("opt-out council ok");
        assert!(!council_path.exists(), "council no-persist env-var must skip file write");

        std::env::remove_var(ENV_NO_PERSIST);
    }

    #[test]
    fn persist_default_path_per_role_unique() {
        let dm_path = specialist_default_persist_path(SpecialistRole::DungeonMaster);
        let gm_path = specialist_default_persist_path(SpecialistRole::GameMaster);
        let coll_path = specialist_default_persist_path(SpecialistRole::Collaborator);
        let coder_path = specialist_default_persist_path(SpecialistRole::Coder);
        // All 4 paths must be distinct.
        let mut s = std::collections::HashSet::new();
        s.insert(dm_path.clone());
        s.insert(gm_path.clone());
        s.insert(coll_path.clone());
        s.insert(coder_path.clone());
        assert_eq!(s.len(), 4, "all 4 default paths must be unique");
        // Each path should end with the role suffix.
        assert!(dm_path.to_string_lossy().ends_with("specialist_dm.bin"));
        assert!(gm_path.to_string_lossy().ends_with("specialist_gm.bin"));
        assert!(coll_path.to_string_lossy().ends_with("specialist_coll.bin"));
        assert!(coder_path.to_string_lossy().ends_with("specialist_coder.bin"));

        let council_default = specialist_council_default_persist_path();
        assert!(council_default
            .to_string_lossy()
            .ends_with("specialist_council.bin"));
    }

    #[test]
    fn persist_rejects_length_mismatch() {
        // A file with valid header byte + valid role + ring_len=2 but
        // truncated payload must be rejected.
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let path = tmpfile("len_mismatch");
        let mut buf = vec![0u8; SPECIALIST_RECORD_MIN]; // claims n=0
        buf[0] = SPECIALIST_PERSIST_FORMAT_V1;
        buf[1] = 1; // GM
        buf[2] = 1;
        buf[3..5].copy_from_slice(&2u16.to_le_bytes()); // declares 2 obs
        // BUT we don't add 32 bytes of obs payload — file is short.
        let body_end = SPECIALIST_RECORD_MIN - 32;
        let digest: [u8; 32] = blake3::hash(&buf[..body_end]).into();
        buf[body_end..].copy_from_slice(&digest);
        std::fs::write(&path, &buf).expect("fixture writes");

        let err = specialist_load(&path).expect_err("must reject truncated record");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("specialist record length mismatch"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persist_each_role_kan_band_preserved() {
        // After persist+load, the loaded SpecialistContext.kan_band_id
        // must match role.kan_band_id().
        let _g = PERSIST_TEST_LOCK.lock().unwrap();
        std::env::remove_var(ENV_NO_PERSIST);
        let cases = [
            (
                SpecialistContext::new(SpecialistRole::DungeonMaster),
                "kan_band_dm",
            ),
            (
                SpecialistContext::new(SpecialistRole::GameMaster),
                "kan_band_gm",
            ),
            (
                SpecialistContext::new(SpecialistRole::Collaborator),
                "kan_band_coll",
            ),
            (
                SpecialistContext::new(SpecialistRole::Coder),
                "kan_band_coder",
            ),
        ];
        for (ctx, name) in &cases {
            let path = tmpfile(name);
            specialist_persist(ctx, &path).expect("persist ok");
            let loaded = specialist_load(&path).expect("load ok");
            assert_eq!(loaded.role, ctx.role);
            assert_eq!(loaded.kan_band_id, ctx.role.kan_band_id());
            let _ = std::fs::remove_file(&path);
        }
    }
}

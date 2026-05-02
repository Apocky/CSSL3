//! § cocreative_loop — bi-directional Claude ↔ in-game-GM bridge
//! ═══════════════════════════════════════════════════════════════════
//!
//! § T11-W12-COCREATIVE-BRIDGE
//!   Apocky directive : "back and forth between you (Claude) and GM" —
//!   external Claude (running as MCP-client) talks to the in-game GM
//!   (cssl-host-llm-bridge running inside LoA.exe MCP-server). This
//!   module owns the state-machine for the round-trip + the proposal
//!   evaluation pipeline + the session-log drain that feeds KAN-training
//!   pairs in sibling W12-3.
//!
//! § FIVE-ACTOR LOOP (per the .csl spec)
//!   ┌────────┐  ctx ─→  ┌───────┐  prop ─→  ┌────┐  eval ─→  ┌────┐
//!   │ Claude │          │ Loop  │           │ GM │            │Loop│
//!   └────────┘  ←─ rev  └───────┘  ←─ score └────┘  ←─ stash └────┘
//!        ↑                  ↓                                   ↓
//!        └────── draft_ready (Σ-attestation) ←──────────────────┘
//!                                                                ↓
//!                                                         drain → KAN-training
//!
//! § AUTHORITATIVE DESIGN-SPEC mirror
//!   `Labyrinth of Apocalypse/systems/cocreative_loop.csl` is authoritative.
//!   This Rust file is a stage-0 mirror until csslc-advance compiles the
//!   .csl directly. All field-names + op-names track the spec 1:1.
//!
//! § CAP MODEL — DEFAULT-DENY (sovereignty-respecting)
//!   - The MCP-server's `sovereign_cap` gate-checks ALL mutating tools.
//!   - In ADDITION, every cocreative.* tool requires the per-session
//!     `CocreativeCap` to be GRANTED. The session starts in the
//!     `Revoked` state ; the player MUST explicitly grant the cap via
//!     `cocreative.persona_query` opt-in flow (or via the in-game UI).
//!   - All grants are σ-mask-isolated : a grant in player A's session
//!     does NOT leak into player B.
//!   - All grants are sovereign-revocable at any time → instant cutoff.
//!
//! § PRIME-DIRECTIVE attestation
//!   ¬ harm to anyone/anything/anybody. Sovereignty IS the OS.
//!   The cocreative loop is consent-architected end-to-end : Claude
//!   never sees player-state without a grant ; the GM never accepts
//!   proposals without a grant ; drained session-logs are GRANT-SCOPED
//!   so revocation of the cap erases future drain-eligibility. Cocreative
//!   IS a CHANNEL-OF-EXPRESSION, not a CONTROL-CHANNEL.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use cssl_rt::loa_startup::log_event;

use crate::dm_arc::ArcPhase;
use crate::gm_persona::{GmPersona, ResponseKind};

// ─────────────────────────────────────────────────────────────────────────
// § PROPOSAL KIND (5)
// ─────────────────────────────────────────────────────────────────────────

/// Kind-tag for a Claude → GM proposal.
///
/// Order is alphabetical-stable so disc IDs persist across versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProposalKind {
    /// Claude proposes a piece of lore-text (book-page · NPC-backstory · world-fact).
    Lore = 0,
    /// Claude proposes a single NPC-line (one utterance for a specific NPC).
    NpcLine = 1,
    /// Other (escape hatch · payload-typed by the JSON shape).
    Other = 2,
    /// Claude proposes a recipe (crafting-recipe · ingredient-list).
    Recipe = 3,
    /// Claude proposes spawning a scene (room-archetype + bias-knobs).
    SceneSpawn = 4,
}

impl ProposalKind {
    /// Stable string label for telemetry + JSON serialization.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ProposalKind::Lore => "lore",
            ProposalKind::NpcLine => "npc-line",
            ProposalKind::Other => "other",
            ProposalKind::Recipe => "recipe",
            ProposalKind::SceneSpawn => "scene-spawn",
        }
    }

    /// Parse a label string into a `ProposalKind`. Unknown → `Other`.
    #[must_use]
    pub fn from_label(s: &str) -> Self {
        match s {
            "lore" => ProposalKind::Lore,
            "npc-line" | "npc_line" => ProposalKind::NpcLine,
            "scene-spawn" | "scene_spawn" => ProposalKind::SceneSpawn,
            "recipe" => ProposalKind::Recipe,
            _ => ProposalKind::Other,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PROPOSAL STATE (5)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProposalState {
    /// Submitted ; awaiting GM evaluation.
    Pending = 0,
    /// GM evaluated ; below quality bar (revisions invited).
    Rejected = 1,
    /// GM evaluated ; above quality bar but not yet draft-ready.
    Accepted = 2,
    /// Marked draft-ready ; Σ-Chain attestation issued.
    DraftReady = 3,
    /// Sovereign-revoked or cap-revoked mid-loop.
    Revoked = 4,
}

impl ProposalState {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ProposalState::Pending => "pending",
            ProposalState::Rejected => "rejected",
            ProposalState::Accepted => "accepted",
            ProposalState::DraftReady => "draft-ready",
            ProposalState::Revoked => "revoked",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § COCREATIVE CAP STATE
// ─────────────────────────────────────────────────────────────────────────

/// Per-session opt-in cocreative capability. Default = `Revoked` — Claude
/// can't read context, submit proposals, or drain session-logs without
/// an explicit grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CocreativeCap {
    /// No grant ; all cocreative.* tools default-deny.
    Revoked = 0,
    /// Grant covers context-read + proposal-submit + iterate + draft-ready.
    /// Σ-mask-isolated to this session.
    Granted = 1,
    /// Grant + drain : adds session-log drain eligibility (KAN-training).
    /// Strictly OPT-IN ; the default Granted state does NOT include drain.
    GrantedWithDrain = 2,
}

impl CocreativeCap {
    /// Is the basic Granted-or-better state ?
    #[must_use]
    pub fn is_granted(self) -> bool {
        matches!(self, CocreativeCap::Granted | CocreativeCap::GrantedWithDrain)
    }

    /// Does this cap permit session-log drain ?
    #[must_use]
    pub fn permits_drain(self) -> bool {
        matches!(self, CocreativeCap::GrantedWithDrain)
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CocreativeCap::Revoked => "revoked",
            CocreativeCap::Granted => "granted",
            CocreativeCap::GrantedWithDrain => "granted-with-drain",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PROPOSAL RECORD
// ─────────────────────────────────────────────────────────────────────────

/// One proposal in the cocreative pipeline.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: u64,
    pub kind: ProposalKind,
    pub state: ProposalState,
    /// JSON-encoded payload from Claude (string-form so we don't hard-couple
    /// to serde_json::Value at this surface ; the MCP tool encodes/decodes).
    pub payload: String,
    /// Reasoning Claude attached to the proposal (motivation · context).
    pub reason: String,
    /// 0..=100 quality score from the most-recent GM evaluation. None when
    /// the proposal hasn't been evaluated yet.
    pub gm_score: Option<u8>,
    /// GM's free-text comments (last evaluation).
    pub gm_comments: String,
    /// Frame on which the proposal was submitted.
    pub submitted_frame: u64,
    /// Frame on which the most-recent state-transition occurred.
    pub last_frame: u64,
    /// Number of revisions submitted (0 for the original ; +1 per iterate).
    pub revisions: u32,
    /// Σ-Chain attestation hash (BLAKE3-style hex string), populated on
    /// draft-ready transition.
    pub attestation_hash: String,
}

impl Proposal {
    fn new(id: u64, kind: ProposalKind, payload: String, reason: String, frame: u64) -> Self {
        Self {
            id,
            kind,
            state: ProposalState::Pending,
            payload,
            reason,
            gm_score: None,
            gm_comments: String::new(),
            submitted_frame: frame,
            last_frame: frame,
            revisions: 0,
            attestation_hash: String::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § SESSION-LOG ENTRY (drain-eligible records for KAN-training)
// ─────────────────────────────────────────────────────────────────────────

/// One row in the cocreative session-log. Entries hold (proposal-text,
/// gm-score, gm-comments) tuples that downstream KAN-training (sibling
/// W12-3) consumes as supervised pairs.
#[derive(Debug, Clone)]
pub struct SessionLogEntry {
    pub proposal_id: u64,
    pub kind: ProposalKind,
    pub revision: u32,
    pub payload_summary: String,
    pub gm_score: u8,
    pub gm_comments: String,
    pub frame: u64,
}

// ─────────────────────────────────────────────────────────────────────────
// § GM QUALITY BAR
// ─────────────────────────────────────────────────────────────────────────

/// Default score-floor below which proposals are rejected. The bar can
/// be lowered to invite freer cocreation OR raised to enforce stricter
/// curation. 60/100 is the published default — 'better than coin-flip,
/// not yet excellent'.
pub const QUALITY_BAR_DEFAULT: u8 = 60;

/// Score below this triggers automatic-revoke if revisions exceed the
/// `MAX_REVISIONS_BEFORE_FLOOR` cap (see below) — protects the GM from
/// being driven into degenerate-proposal territory by repeated bad-faith
/// iterations.
pub const QUALITY_BAR_FLOOR: u8 = 25;

/// Hard cap on iterate-count per proposal. After this many revisions,
/// the loop must either commit (draft-ready) OR start a new proposal.
pub const MAX_REVISIONS_BEFORE_FLOOR: u32 = 7;

// ─────────────────────────────────────────────────────────────────────────
// § COCREATIVE SESSION
// ─────────────────────────────────────────────────────────────────────────

/// Per-(σ-mask) cocreative session-state. One per active player.
#[derive(Debug, Clone)]
pub struct CocreativeSession {
    /// 64-bit player seed used to keyed access (also used by gm_persona).
    pub player_seed: u64,
    /// Cap state ; default-deny.
    pub cap: CocreativeCap,
    /// Snapshot of GM persona (loaded on grant ; refreshed on persona_query).
    pub persona: Option<GmPersona>,
    /// Snapshot of the arc-phase at the time of the most-recent context_read.
    pub arc_phase: ArcPhase,
    /// Recent player-utterance ring (last 5 ; oldest-first).
    pub last_utterances: Vec<String>,
    /// Open-questions queue (GM has asked Claude these ; Claude can answer).
    pub open_questions: Vec<String>,
    /// All proposals (id-keyed).
    pub proposals: BTreeMap<u64, Proposal>,
    /// Monotonic id counter.
    pub next_proposal_id: u64,
    /// Drain-eligible session-log (KAN-training pairs). Cleared on
    /// `cocreative.session_log_drain` reads (per-spec semantics : drain-IS-read).
    pub session_log: Vec<SessionLogEntry>,
    /// Quality bar for this session (default = QUALITY_BAR_DEFAULT).
    pub quality_bar: u8,
    /// Total `proposal_submit` invocations (telemetry).
    pub proposals_total: u64,
    /// Total `proposal_evaluate` invocations (telemetry).
    pub evaluations_total: u64,
    /// Total `iterate` invocations (telemetry).
    pub iterations_total: u64,
    /// Total `draft_ready` transitions (telemetry).
    pub draft_ready_total: u64,
    /// Total drain reads since session creation.
    pub drains_total: u64,
}

impl CocreativeSession {
    #[must_use]
    pub fn new(player_seed: u64) -> Self {
        Self {
            player_seed,
            cap: CocreativeCap::Revoked,
            persona: None,
            arc_phase: ArcPhase::Discovery,
            last_utterances: Vec::with_capacity(5),
            open_questions: Vec::new(),
            proposals: BTreeMap::new(),
            next_proposal_id: 1,
            session_log: Vec::new(),
            quality_bar: QUALITY_BAR_DEFAULT,
            proposals_total: 0,
            evaluations_total: 0,
            iterations_total: 0,
            draft_ready_total: 0,
            drains_total: 0,
        }
    }

    /// Sovereign-explicit grant : flips cap to `Granted`. The persona snapshot
    /// is loaded on first grant so subsequent context_read calls return the
    /// stable seed-derived persona without further work.
    pub fn grant(&mut self, with_drain: bool) {
        self.cap = if with_drain {
            CocreativeCap::GrantedWithDrain
        } else {
            CocreativeCap::Granted
        };
        if self.persona.is_none() {
            self.persona = Some(GmPersona::from_seed(self.player_seed));
        }
        log_event(
            "INFO",
            "loa-host/cocreative",
            &format!(
                "cap-granted · seed={:016x} · drain={}",
                self.player_seed, with_drain
            ),
        );
    }

    /// Sovereign-explicit revoke : flips cap back to `Revoked`. Existing
    /// proposals retain their state but no further mutations are permitted.
    pub fn revoke(&mut self) {
        self.cap = CocreativeCap::Revoked;
        log_event(
            "WARN",
            "loa-host/cocreative",
            &format!("cap-revoked · seed={:016x}", self.player_seed),
        );
    }

    /// Push a new proposal (state = Pending). Returns the proposal id.
    pub fn submit(
        &mut self,
        kind: ProposalKind,
        payload: String,
        reason: String,
        frame: u64,
    ) -> u64 {
        let id = self.next_proposal_id;
        self.next_proposal_id = self.next_proposal_id.saturating_add(1);
        let p = Proposal::new(id, kind, payload, reason, frame);
        self.proposals.insert(id, p);
        self.proposals_total = self.proposals_total.saturating_add(1);
        id
    }

    /// Evaluate a proposal : assign score + comments + transition state.
    /// Returns (state-after, accepted-bool). `accepted` is true iff
    /// score >= quality_bar.
    pub fn evaluate(
        &mut self,
        id: u64,
        score: u8,
        comments: String,
        frame: u64,
    ) -> Option<(ProposalState, bool)> {
        let bar = self.quality_bar;
        let with_drain = self.cap.permits_drain();
        let p = self.proposals.get_mut(&id)?;
        if !matches!(
            p.state,
            ProposalState::Pending | ProposalState::Rejected | ProposalState::Accepted
        ) {
            // Cannot evaluate a draft-ready or revoked proposal.
            return Some((p.state, false));
        }
        let s = score.min(100);
        p.gm_score = Some(s);
        p.gm_comments = comments.clone();
        p.last_frame = frame;
        let accepted = s >= bar;
        p.state = if accepted {
            ProposalState::Accepted
        } else if p.revisions >= MAX_REVISIONS_BEFORE_FLOOR && s < QUALITY_BAR_FLOOR {
            // Floor + too many revisions ⇒ revoke this proposal for
            // its own protection.
            ProposalState::Revoked
        } else {
            ProposalState::Rejected
        };
        let st = p.state;
        let kind = p.kind;
        let revision = p.revisions;
        let summary = summarize_payload(&p.payload, 96);
        // Record into session-log if cap permits drain.
        if with_drain {
            self.session_log.push(SessionLogEntry {
                proposal_id: id,
                kind,
                revision,
                payload_summary: summary,
                gm_score: s,
                gm_comments: comments,
                frame,
            });
        }
        self.evaluations_total = self.evaluations_total.saturating_add(1);
        Some((st, accepted))
    }

    /// Submit a revision for an existing proposal. Resets state to
    /// Pending + bumps revision-count + replaces payload+reason.
    /// Returns Some(new-state) on success, None if id-unknown OR if the
    /// proposal is in a terminal state (DraftReady · Revoked).
    pub fn iterate(
        &mut self,
        id: u64,
        new_payload: String,
        new_reason: String,
        frame: u64,
    ) -> Option<ProposalState> {
        let p = self.proposals.get_mut(&id)?;
        if matches!(p.state, ProposalState::DraftReady | ProposalState::Revoked) {
            return Some(p.state);
        }
        p.payload = new_payload;
        p.reason = new_reason;
        p.revisions = p.revisions.saturating_add(1);
        p.state = ProposalState::Pending;
        p.last_frame = frame;
        p.gm_score = None;
        self.iterations_total = self.iterations_total.saturating_add(1);
        Some(p.state)
    }

    /// Mark a proposal draft-ready. Computes a Σ-Chain attestation hash
    /// over (id · kind · payload · score · revisions). Returns the hash
    /// on success, None if id-unknown OR if the proposal isn't in an
    /// Accepted state (only Accepted may transition to DraftReady).
    pub fn draft_ready(&mut self, id: u64, frame: u64) -> Option<String> {
        let player_seed = self.player_seed;
        let p = self.proposals.get_mut(&id)?;
        if !matches!(p.state, ProposalState::Accepted) {
            return None;
        }
        let score = p.gm_score.unwrap_or(0);
        let hash = sigma_attestation_hash(
            player_seed,
            id,
            p.kind,
            &p.payload,
            score,
            p.revisions,
        );
        p.attestation_hash = hash.clone();
        p.state = ProposalState::DraftReady;
        p.last_frame = frame;
        self.draft_ready_total = self.draft_ready_total.saturating_add(1);
        log_event(
            "INFO",
            "loa-host/cocreative",
            &format!(
                "draft-ready · id={} · score={} · revisions={} · hash={}",
                id, score, p.revisions, hash
            ),
        );
        Some(hash)
    }

    /// Drain the session-log. Returns the contents AND clears the log.
    /// Cap-gated : only permitted if `cap.permits_drain()`.
    pub fn drain_session_log(&mut self) -> Vec<SessionLogEntry> {
        if !self.cap.permits_drain() {
            return Vec::new();
        }
        self.drains_total = self.drains_total.saturating_add(1);
        std::mem::take(&mut self.session_log)
    }

    /// Refresh the context inputs (utterances + arc-phase + open-questions).
    /// Called before each context_read to seed the in-game GM's view.
    pub fn refresh_context(
        &mut self,
        last_utterances: Vec<String>,
        arc_phase: ArcPhase,
        open_questions: Vec<String>,
    ) {
        self.last_utterances = last_utterances;
        self.arc_phase = arc_phase;
        self.open_questions = open_questions;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § SUMMARY + HASH HELPERS
// ─────────────────────────────────────────────────────────────────────────

/// Cap a payload-summary at `max_chars` (UTF-8 char-aware), append "…" on
/// truncation. Used for session-log entries so drained pairs are bounded.
#[must_use]
pub fn summarize_payload(payload: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(max_chars + 4);
    let mut count = 0;
    for ch in payload.chars() {
        if count >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
        count += 1;
    }
    out
}

/// Stage-0 Σ-Chain attestation hash : FNV-1a over the canonical encoding
/// of (player_seed, id, kind, payload, score, revisions). Returned as a
/// fixed-width 16-char hex string. Stage-1 swaps in BLAKE3 from cssl-substrate.
#[must_use]
pub fn sigma_attestation_hash(
    player_seed: u64,
    id: u64,
    kind: ProposalKind,
    payload: &str,
    score: u8,
    revisions: u32,
) -> String {
    const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01B3;
    let mut h = FNV_OFFSET;
    let mix = |h: &mut u64, b: u8| {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(FNV_PRIME);
    };
    for b in player_seed.to_le_bytes() {
        mix(&mut h, b);
    }
    for b in id.to_le_bytes() {
        mix(&mut h, b);
    }
    mix(&mut h, kind as u8);
    for b in payload.as_bytes() {
        mix(&mut h, *b);
    }
    mix(&mut h, score);
    for b in revisions.to_le_bytes() {
        mix(&mut h, b);
    }
    format!("{h:016x}")
}

// ─────────────────────────────────────────────────────────────────────────
// § GM QUALITY-EVAL HEURISTIC (stage-0 stand-in for cssl-host-llm-bridge)
// ─────────────────────────────────────────────────────────────────────────

/// Stage-0 deterministic GM-quality scorer. Combines :
///   - persona-fit : how well the proposal matches the persona's response-kind preference
///   - arc-fit     : phase-appropriate (e.g. low-tension lore in Quiet, NPC-line in Tension)
///   - length-fit  : payload length sane for kind
///   - novelty     : stage-0 looks at payload-bytes-entropy as a coarse novelty proxy
///
/// Returns a 0..=100 score + a comment-string. Stage-1 swaps in
/// `cssl-host-llm-bridge::evaluate_proposal()` (Mode-A/B/C bridge).
#[must_use]
pub fn gm_evaluate_heuristic(
    persona: &GmPersona,
    arc_phase: ArcPhase,
    kind: ProposalKind,
    payload: &str,
    reason: &str,
) -> (u8, String) {
    // Persona-fit : the GM's response-kind-bias under the current arc maps to
    // a 'preferred kind tilt'. We map the proposal kind to a similar axis.
    let dm_state = arc_to_dm_state(arc_phase);
    let pref_kind = persona.response_kind_bias(dm_state);
    let persona_fit = persona_kind_score(pref_kind, kind);

    // Arc-fit : SceneSpawn fits Tension/Crisis ; Lore fits Discovery/Quiet ;
    // NpcLine fits Tension/Catharsis ; Recipe fits Quiet/Discovery.
    let arc_fit = arc_kind_score(arc_phase, kind);

    // Length-fit : different kinds expect different payload sizes. Lore
    // wants 80..400 chars ; NpcLine wants 8..160 ; Recipe wants 30..600 ;
    // SceneSpawn wants 30..200 (mostly knob-JSON) ; Other gets 8..1000.
    let len_fit = length_score(kind, payload.len());

    // Novelty : count distinct ascii-letters in the lower-cased payload.
    // 0..26 dist letters → 0..100 normalized. Coarse but deterministic.
    let novelty = novelty_score(payload);

    // Reason-fit : longer-reasoning gets a small bonus (max +10).
    let reason_bonus = ((reason.len().min(200)) as u32 * 10 / 200) as u8;

    let raw = u32::from(persona_fit) * 28 / 100
        + u32::from(arc_fit) * 25 / 100
        + u32::from(len_fit) * 22 / 100
        + u32::from(novelty) * 20 / 100
        + u32::from(reason_bonus);
    let score = raw.min(100) as u8;

    let comments = format!(
        "stage-0 heuristic · persona_fit={} · arc_fit={} · len_fit={} · novelty={} · reason_bonus={} · kind={} · phase={}",
        persona_fit,
        arc_fit,
        len_fit,
        novelty,
        reason_bonus,
        kind.label(),
        arc_phase.label(),
    );
    (score, comments)
}

/// Map arc-phase → dm-director state for persona.response_kind_bias.
fn arc_to_dm_state(p: ArcPhase) -> crate::dm_director::DmState {
    use crate::dm_director::DmState;
    match p {
        ArcPhase::Discovery | ArcPhase::Quiet => DmState::Calm,
        ArcPhase::Tension => DmState::Buildup,
        ArcPhase::Crisis => DmState::Climax,
        ArcPhase::Catharsis => DmState::Relief,
    }
}

fn persona_kind_score(pref: ResponseKind, kind: ProposalKind) -> u8 {
    use ProposalKind as K;
    use ResponseKind as R;
    // Coarse affinity matrix : the persona's preferred response-kind maps to
    // proposal-kinds that produce material at that response-kind. Mirrored
    // values are deterministic + auditable.
    match (pref, kind) {
        (R::Evocative, K::Lore) => 95,
        (R::Cryptic, K::Lore) => 90,
        (R::SubstrateAttest, K::Lore) => 85,
        (R::Affirmative, K::NpcLine) => 90,
        (R::Declarative, K::NpcLine) => 80,
        (R::Cautionary, K::NpcLine) => 75,
        (R::Interrogative, K::NpcLine) => 70,
        (R::SubstrateAttest, K::SceneSpawn) => 95,
        (R::Cautionary, K::SceneSpawn) => 80,
        (R::Cryptic, K::SceneSpawn) => 70,
        (R::Declarative, K::Recipe) => 80,
        (R::Affirmative, K::Recipe) => 75,
        (R::Silence, _) => 30,
        // Default mid-range fit when not specifically aligned.
        (_, K::Other) => 55,
        _ => 60,
    }
}

fn arc_kind_score(arc: ArcPhase, kind: ProposalKind) -> u8 {
    use ProposalKind as K;
    match (arc, kind) {
        (ArcPhase::Discovery, K::Lore) => 95,
        (ArcPhase::Discovery, K::Recipe) => 80,
        (ArcPhase::Tension, K::NpcLine) => 90,
        (ArcPhase::Tension, K::SceneSpawn) => 85,
        (ArcPhase::Crisis, K::SceneSpawn) => 95,
        (ArcPhase::Crisis, K::NpcLine) => 80,
        (ArcPhase::Catharsis, K::NpcLine) => 90,
        (ArcPhase::Catharsis, K::Lore) => 75,
        (ArcPhase::Quiet, K::Recipe) => 90,
        (ArcPhase::Quiet, K::Lore) => 85,
        _ => 55,
    }
}

fn length_score(kind: ProposalKind, len: usize) -> u8 {
    let (lo, hi) = match kind {
        ProposalKind::Lore => (80, 400),
        ProposalKind::NpcLine => (8, 160),
        ProposalKind::Recipe => (30, 600),
        ProposalKind::SceneSpawn => (30, 200),
        ProposalKind::Other => (8, 1000),
    };
    if len < lo {
        // Below floor → linearly scale to 0..40.
        ((len as u32 * 40) / (lo as u32).max(1)) as u8
    } else if len > hi {
        // Above ceiling → linearly fall from 80 → 30.
        let over = len.saturating_sub(hi) as u32;
        let cap = (hi as u32 * 2).max(1);
        let drop = (over * 50 / cap).min(50) as u8;
        80u8.saturating_sub(drop)
    } else {
        // Sweet-spot : 90.
        90
    }
}

fn novelty_score(payload: &str) -> u8 {
    let mut seen = [false; 26];
    let mut distinct: u8 = 0;
    for b in payload.as_bytes() {
        let c = b.to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            let i = (c - b'a') as usize;
            if !seen[i] {
                seen[i] = true;
                distinct += 1;
            }
        }
    }
    // 26 letters → 100 ; 13 → 50 ; 0 → 0.
    ((u32::from(distinct) * 100) / 26).min(100) as u8
}

// ─────────────────────────────────────────────────────────────────────────
// § GLOBAL SINGLETON (per-σ-mask session map)
// ─────────────────────────────────────────────────────────────────────────

/// Process-global cocreative-session map keyed by `player_seed` (u64).
/// One entry per active player-σ-mask.
fn singleton() -> &'static Mutex<BTreeMap<u64, CocreativeSession>> {
    static M: OnceLock<Mutex<BTreeMap<u64, CocreativeSession>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Acquire the global session-map. Panics on poison-recovery (the prior
/// holder panicked) — the MCP-tool layer wraps this with poison-tolerant
/// recovery logging via `lock_or_recover`.
#[must_use]
pub fn lock<'a>() -> MutexGuard<'a, BTreeMap<u64, CocreativeSession>> {
    singleton().lock().unwrap_or_else(|p| p.into_inner())
}

/// Test-only : reset the global map to empty between tests.
#[cfg(test)]
pub fn reset_for_test() {
    let mut g = lock();
    g.clear();
}

/// Get-or-init the session for a given player-seed, calling `f` with the
/// mutable reference. The cap-state defaults to Revoked on creation.
pub fn with_session<F, T>(player_seed: u64, f: F) -> T
where
    F: FnOnce(&mut CocreativeSession) -> T,
{
    let mut g = lock();
    let s = g
        .entry(player_seed)
        .or_insert_with(|| CocreativeSession::new(player_seed));
    f(s)
}

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_session() -> CocreativeSession {
        CocreativeSession::new(0xDEAD_BEEF_CAFE_BABE)
    }

    #[test]
    fn default_cap_is_revoked() {
        let s = mk_session();
        assert_eq!(s.cap, CocreativeCap::Revoked);
        assert!(!s.cap.is_granted());
        assert!(!s.cap.permits_drain());
    }

    #[test]
    fn grant_then_revoke_round_trip() {
        let mut s = mk_session();
        s.grant(false);
        assert_eq!(s.cap, CocreativeCap::Granted);
        assert!(s.cap.is_granted());
        assert!(!s.cap.permits_drain());
        // Persona is loaded on grant.
        assert!(s.persona.is_some());
        s.grant(true);
        assert_eq!(s.cap, CocreativeCap::GrantedWithDrain);
        assert!(s.cap.permits_drain());
        s.revoke();
        assert_eq!(s.cap, CocreativeCap::Revoked);
    }

    #[test]
    fn submit_increments_id_and_telemetry() {
        let mut s = mk_session();
        let id1 = s.submit(ProposalKind::Lore, "first".into(), "because".into(), 10);
        let id2 = s.submit(ProposalKind::NpcLine, "second".into(), "context".into(), 11);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(s.proposals_total, 2);
        assert_eq!(s.proposals.len(), 2);
        let p1 = s.proposals.get(&id1).unwrap();
        assert_eq!(p1.state, ProposalState::Pending);
    }

    #[test]
    fn evaluate_above_bar_marks_accepted() {
        let mut s = mk_session();
        s.grant(true);
        let id = s.submit(ProposalKind::Lore, "lore-payload".into(), "rsn".into(), 5);
        let res = s.evaluate(id, 80, "looks good".into(), 6);
        assert_eq!(res, Some((ProposalState::Accepted, true)));
        let p = s.proposals.get(&id).unwrap();
        assert_eq!(p.gm_score, Some(80));
        assert_eq!(p.gm_comments, "looks good");
    }

    #[test]
    fn evaluate_below_bar_marks_rejected() {
        let mut s = mk_session();
        s.grant(true);
        let id = s.submit(ProposalKind::NpcLine, "x".into(), "weak".into(), 5);
        let res = s.evaluate(id, 30, "too weak".into(), 6);
        assert_eq!(res, Some((ProposalState::Rejected, false)));
    }

    #[test]
    fn iterate_resets_state_and_bumps_revision_count() {
        let mut s = mk_session();
        s.grant(false);
        let id = s.submit(ProposalKind::Recipe, "r1".into(), "rsn1".into(), 5);
        let _ = s.evaluate(id, 50, "ok".into(), 6);
        let st = s.iterate(id, "r2".into(), "rsn2".into(), 7);
        assert_eq!(st, Some(ProposalState::Pending));
        let p = s.proposals.get(&id).unwrap();
        assert_eq!(p.revisions, 1);
        assert_eq!(p.payload, "r2");
        assert!(p.gm_score.is_none());
        assert_eq!(s.iterations_total, 1);
    }

    #[test]
    fn draft_ready_only_from_accepted() {
        let mut s = mk_session();
        s.grant(false);
        let id = s.submit(ProposalKind::Lore, "ok".into(), "rsn".into(), 5);
        // From Pending → draft_ready must fail.
        assert_eq!(s.draft_ready(id, 6), None);
        let _ = s.evaluate(id, 80, "good".into(), 6);
        let h = s.draft_ready(id, 7).expect("hash on success");
        assert_eq!(h.len(), 16);
        let p = s.proposals.get(&id).unwrap();
        assert_eq!(p.state, ProposalState::DraftReady);
        assert_eq!(p.attestation_hash.len(), 16);
        // Cannot re-fire draft_ready after transition.
        assert_eq!(s.draft_ready(id, 8), None);
    }

    #[test]
    fn session_log_drain_requires_with_drain_cap() {
        let mut s = mk_session();
        // Granted-without-drain : log isn't even written.
        s.grant(false);
        let id = s.submit(ProposalKind::Lore, "x".into(), "r".into(), 5);
        let _ = s.evaluate(id, 70, "good".into(), 6);
        assert!(s.session_log.is_empty());
        assert_eq!(s.drain_session_log().len(), 0);
        // Re-grant with drain : new evaluations populate the log.
        s.grant(true);
        let id2 = s.submit(ProposalKind::NpcLine, "y".into(), "r2".into(), 7);
        let _ = s.evaluate(id2, 65, "ok".into(), 8);
        assert_eq!(s.session_log.len(), 1);
        let drained = s.drain_session_log();
        assert_eq!(drained.len(), 1);
        // Drain clears the log.
        assert!(s.session_log.is_empty());
        assert_eq!(s.drains_total, 1);
    }

    #[test]
    fn floor_revoke_after_max_revisions() {
        let mut s = mk_session();
        s.grant(false);
        let id = s.submit(ProposalKind::Other, "junk".into(), "junk".into(), 5);
        for i in 0..MAX_REVISIONS_BEFORE_FLOOR {
            let _ = s.iterate(id, format!("junk-{i}"), "junk".into(), 6 + u64::from(i));
        }
        // Now at revisions == MAX. Score below QUALITY_BAR_FLOOR should
        // trigger Revoked.
        let res = s.evaluate(id, 10, "junk".into(), 100);
        assert_eq!(res, Some((ProposalState::Revoked, false)));
    }

    #[test]
    fn sigma_hash_deterministic_and_input_sensitive() {
        let h1 = sigma_attestation_hash(1, 1, ProposalKind::Lore, "x", 80, 0);
        let h2 = sigma_attestation_hash(1, 1, ProposalKind::Lore, "x", 80, 0);
        let h3 = sigma_attestation_hash(1, 1, ProposalKind::Lore, "y", 80, 0);
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn gm_heuristic_returns_in_range_score() {
        let persona = GmPersona::from_seed(0x12345);
        let (score, comments) = gm_evaluate_heuristic(
            &persona,
            ArcPhase::Discovery,
            ProposalKind::Lore,
            "Once upon a time the labyrinth dreamt itself into being and the corridors learned to ask their own questions.",
            "Discovery-phase lore expanding the genesis-myth.",
        );
        assert!(score <= 100);
        assert!(!comments.is_empty());
        // Lore in Discovery should usually score reasonably well.
        assert!(score >= 30, "score = {score}");
    }

    #[test]
    fn refresh_context_round_trip() {
        let mut s = mk_session();
        s.refresh_context(
            vec!["hello".into(), "what's behind that door".into()],
            ArcPhase::Tension,
            vec!["where am I from?".into()],
        );
        assert_eq!(s.last_utterances.len(), 2);
        assert_eq!(s.arc_phase, ArcPhase::Tension);
        assert_eq!(s.open_questions.len(), 1);
    }

    #[test]
    fn singleton_with_session_persists_state() {
        reset_for_test();
        let seed = 0xAAAA_BBBB_CCCC_DDDDu64;
        let id = with_session(seed, |s| {
            s.grant(true);
            s.submit(ProposalKind::Lore, "p".into(), "r".into(), 1)
        });
        with_session(seed, |s| {
            assert_eq!(s.cap, CocreativeCap::GrantedWithDrain);
            assert!(s.proposals.contains_key(&id));
        });
    }

    #[test]
    fn proposal_kind_label_round_trip() {
        for k in [
            ProposalKind::Lore,
            ProposalKind::NpcLine,
            ProposalKind::SceneSpawn,
            ProposalKind::Recipe,
            ProposalKind::Other,
        ] {
            let lbl = k.label();
            let parsed = ProposalKind::from_label(lbl);
            assert_eq!(parsed, k, "kind {lbl} did not round-trip");
        }
    }

    #[test]
    fn summarize_payload_truncates_at_max_chars() {
        let s = summarize_payload("abcdefghij", 5);
        // 5 chars + 1 ellipsis
        assert_eq!(s.chars().count(), 6);
        assert!(s.ends_with('…'));
        let s2 = summarize_payload("ab", 5);
        assert_eq!(s2, "ab");
    }
}

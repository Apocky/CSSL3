//! § pattern — `ChatPattern` bit-packed anonymized signal
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   `ChatPattern` ⊑ smallest-fully-anonymous-signal that can move across
//!   the mycelial-network. ¬ raw-chat-text. Pure-shape : (intent-kind ×
//!   response-shape × arc-phase × confidence). 32-byte fixed-size record.
//!
//! § BIT-PACK LAYOUT (LE · 32 bytes total)
//!   ┌───────────┬────────┬────────────────────────────────────────────────┐
//!   │ offset    │ bytes  │ field                                          │
//!   ├───────────┼────────┼────────────────────────────────────────────────┤
//!   │  0..4     │   4    │ pattern_id            (BLAKE3-trunc · u32 LE)  │
//!   │  4        │   1    │ intent_kind           (IntentKind disc · u8)   │
//!   │  5        │   1    │ response_shape        (ResponseShape disc · u8)│
//!   │  6        │   1    │ arc_phase             (ArcPhase disc · u8)     │
//!   │  7        │   1    │ confidence_q8         (0..=255 ↦ 0.0..=1.0)    │
//!   │  8..12    │   4    │ ts_bucketed           (epoch/60s · u32 LE)     │
//!   │ 12..14    │   2    │ region_tag            (u16 LE)                 │
//!   │ 14        │   1    │ opt_in_tier           (0..=3 · u8)             │
//!   │ 15        │   1    │ cap_flags             (Σ-mask gates · u8)      │
//!   │ 16..24    │   8    │ emitter_handle        (BLAKE3-trunc · u64 LE)  │
//!   │ 24..32    │   8    │ co_signer_set_hash    (BLAKE3-trunc · u64 LE)  │
//!   └───────────┴────────┴────────────────────────────────────────────────┘
//!
//! § PRIVACY DERIVATION RULES
//!   ─ `pattern_id` = blake3(intent_kind ‖ response_shape ‖ arc_phase) ⟶ trunc
//!     ⊑ same-shape across-emitters collide ⟶ enables k-anon counting
//!   ─ `emitter_handle` = blake3("chat-sync\0emitter\0v1" ‖ pubkey) ⟶ trunc
//!     ⊑ ¬ recoverable to-pubkey · used for distinct-emitter counting only
//!   ─ `co_signer_set_hash` ⟶ tracks which-set-of-emitters contributed
//!     ⊑ enables federation-side k-anon-enforcement at-ingestion
//!
//! § CAP_FLAGS BITS (Σ-mask)
//!   bit 0 : CAP_EMIT_ALLOWED        — set = emitter is opted-in
//!   bit 1 : CAP_FEDERATION_INGEST   — set = pattern allowed into federation
//!   bit 2 : CAP_PURGE_ON_REVOKE     — set = revoke-on-purge propagation
//!   bit 3 : CAP_REPLAY_DETERMINISTIC— set = use in deterministic-modulation
//!   bits 4..7 : reserved (must be 0)

use serde::{Deserialize, Serialize};

/// § ChatPattern — fixed-32-byte anonymized signal.
///
/// Wire-stable representation : `[u8; 32]`. Field-access via accessors which
/// decode the bit-pack. Constructor + accessors live here ; serde wraps the
/// raw bytes as a hex-string for JSON-compat over Supabase / disk-snapshots.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatPattern {
    /// § raw — the 32-byte canonical packed form. Field-public for in-crate
    /// callers that need byte-level access (federation hash · transport
    /// blob). External callers should prefer the accessors.
    pub raw: [u8; 32],
}

impl std::fmt::Debug for ChatPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatPattern")
            .field("pattern_id", &self.pattern_id())
            .field("intent_kind", &self.intent_kind())
            .field("response_shape", &self.response_shape())
            .field("arc_phase", &self.arc_phase())
            .field("confidence", &self.confidence())
            .field("ts_bucketed", &self.ts_bucketed())
            .field("region_tag", &self.region_tag())
            .field("opt_in_tier", &self.opt_in_tier_raw())
            .field("cap_flags", &format_args!("0b{:08b}", self.cap_flags()))
            .field("emitter_handle", &format_args!("{:016x}", self.emitter_handle()))
            .field(
                "co_signer_set_hash",
                &format_args!("{:016x}", self.co_signer_set_hash()),
            )
            .finish()
    }
}

/// § IntentKind — narrow taxonomy of chat-intent shapes.
///
/// Adding a variant requires PRIME_DIRECTIVE-review. The enum is `repr(u8)`
/// so the discriminant is wire-stable.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum IntentKind {
    /// § Question — open-ended ask.
    Question = 0,
    /// § Command — directive / "do X" intent.
    Command = 1,
    /// § Reflection — emotional / introspective beat.
    Reflection = 2,
    /// § Worldbuilding — lore / setting expansion.
    Worldbuilding = 3,
    /// § Combat — tactical / encounter directive.
    Combat = 4,
    /// § Exploration — "where can I go" / scout intent.
    Exploration = 5,
    /// § Crafting — recipe / item-construction intent.
    Crafting = 6,
    /// § Social — NPC-interaction / dialogue beat.
    Social = 7,
    /// § Meta — out-of-character / system-control beat.
    Meta = 8,
    /// § Unknown — fallback ; classifier did not match.
    Unknown = 255,
}

impl IntentKind {
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Question,
            1 => Self::Command,
            2 => Self::Reflection,
            3 => Self::Worldbuilding,
            4 => Self::Combat,
            5 => Self::Exploration,
            6 => Self::Crafting,
            7 => Self::Social,
            8 => Self::Meta,
            _ => Self::Unknown,
        }
    }
}

/// § ResponseShape — the SHAPE of a good GM/DM response, never the content.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum ResponseShape {
    /// § ShortDirect — ≤ 50 tokens · no flourish · just answer.
    ShortDirect = 0,
    /// § ScenicNarrative — flowing description · scene-setting.
    ScenicNarrative = 1,
    /// § DialogueDriven — NPC voice + line-level back-and-forth.
    DialogueDriven = 2,
    /// § BulletedOptions — present-choices ; "you could …".
    BulletedOptions = 3,
    /// § QuestionBack — Socratic ; ask-clarifier-before-acting.
    QuestionBack = 4,
    /// § AmbientHint — soft-environmental nudge ; ¬ explicit.
    AmbientHint = 5,
    /// § MechanicalReadout — dice/stats/numbers-foreground.
    MechanicalReadout = 6,
    /// § StorybeatPunch — short-dramatic-punctuation.
    StorybeatPunch = 7,
    /// § Unknown — fallback.
    Unknown = 255,
}

impl ResponseShape {
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::ShortDirect,
            1 => Self::ScenicNarrative,
            2 => Self::DialogueDriven,
            3 => Self::BulletedOptions,
            4 => Self::QuestionBack,
            5 => Self::AmbientHint,
            6 => Self::MechanicalReadout,
            7 => Self::StorybeatPunch,
            _ => Self::Unknown,
        }
    }
}

/// § ArcPhase — narrative-arc position discriminator.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum ArcPhase {
    /// § Setup — opening-beats · onboarding · intro.
    Setup = 0,
    /// § RisingAction — building-tension · complications.
    RisingAction = 1,
    /// § Climax — peak-stakes · key-decision-moment.
    Climax = 2,
    /// § Falling — release · resolution-approach.
    Falling = 3,
    /// § Denouement — wrap-up · reflection · transition.
    Denouement = 4,
    /// § Interlude — between-arcs · downtime · explore.
    Interlude = 5,
    /// § Unknown — fallback.
    Unknown = 255,
}

impl ArcPhase {
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Setup,
            1 => Self::RisingAction,
            2 => Self::Climax,
            3 => Self::Falling,
            4 => Self::Denouement,
            5 => Self::Interlude,
            _ => Self::Unknown,
        }
    }
}

// ─── cap_flags Σ-mask bits ──────────────────────────────────────────────────

/// § CAP_EMIT_ALLOWED — emitter has opted-in to chat-sync federation.
pub const CAP_EMIT_ALLOWED: u8 = 0b0000_0001;
/// § CAP_FEDERATION_INGEST — pattern allowed into the federated shared-state.
pub const CAP_FEDERATION_INGEST: u8 = 0b0000_0010;
/// § CAP_PURGE_ON_REVOKE — emitter consents to revoke-on-purge propagation.
pub const CAP_PURGE_ON_REVOKE: u8 = 0b0000_0100;
/// § CAP_REPLAY_DETERMINISTIC — pattern eligible for replay-safe modulation.
pub const CAP_REPLAY_DETERMINISTIC: u8 = 0b0000_1000;

/// § CAP_FLAGS_ALL — full set ; emitter must hold ALL bits to federate.
pub const CAP_FLAGS_ALL: u8 =
    CAP_EMIT_ALLOWED | CAP_FEDERATION_INGEST | CAP_PURGE_ON_REVOKE | CAP_REPLAY_DETERMINISTIC;

/// § CAP_FLAGS_RESERVED_MASK — bits 4..=7 must be 0.
pub const CAP_FLAGS_RESERVED_MASK: u8 = 0b1111_0000;

// ─── ChatPatternBuilder ─────────────────────────────────────────────────────

/// § ChatPatternBuilder — typed-builder for `ChatPattern`. Performs the
/// bit-pack + derives the `pattern_id` + `emitter_handle` BLAKE3-truncs.
#[derive(Debug, Clone)]
pub struct ChatPatternBuilder {
    pub intent_kind: IntentKind,
    pub response_shape: ResponseShape,
    pub arc_phase: ArcPhase,
    /// 0.0..=1.0 — clamped to range, then quantized to u8.
    pub confidence: f32,
    /// Wall-clock unix-seconds. Will be bucketed to /60 in the packed form.
    pub ts_unix: u64,
    pub region_tag: u16,
    /// 0 (LocalOnly) → 3 (Public).
    pub opt_in_tier: u8,
    /// Σ-mask bits the emitter holds. See CAP_* constants.
    pub cap_flags: u8,
    /// Raw 32-byte emitter pubkey ; truncated to 8-byte handle.
    pub emitter_pubkey: [u8; 32],
    /// Set of emitter pubkeys that co-contributed (for k-anon ingestion).
    /// Empty ⟹ singleton-pattern (will fail k-anon at federation).
    pub co_signers: Vec<[u8; 32]>,
}

impl ChatPatternBuilder {
    /// § build — derive ids + bit-pack into `ChatPattern`.
    ///
    /// Validates : `confidence` ∈ [0, 1] · `opt_in_tier` ∈ 0..=3 ·
    /// `cap_flags` reserved-bits = 0. Returns `Err` on violation.
    pub fn build(self) -> Result<ChatPattern, PatternError> {
        if !(0.0..=1.0).contains(&self.confidence) || self.confidence.is_nan() {
            return Err(PatternError::ConfidenceOutOfRange(self.confidence));
        }
        if self.opt_in_tier > 3 {
            return Err(PatternError::OptInTierOutOfRange(self.opt_in_tier));
        }
        if self.cap_flags & CAP_FLAGS_RESERVED_MASK != 0 {
            return Err(PatternError::ReservedCapFlagsSet(self.cap_flags));
        }

        let confidence_q8 = (self.confidence * 255.0).round().clamp(0.0, 255.0) as u8;
        let ts_bucketed: u32 = ((self.ts_unix / 60) & 0xFFFF_FFFF) as u32;

        let pattern_id = derive_pattern_id(self.intent_kind, self.response_shape, self.arc_phase);
        let emitter_handle = derive_emitter_handle(&self.emitter_pubkey);
        let co_signer_set_hash = derive_co_signer_set_hash(&self.co_signers);

        let mut raw = [0_u8; 32];
        raw[0..4].copy_from_slice(&pattern_id.to_le_bytes());
        raw[4] = self.intent_kind as u8;
        raw[5] = self.response_shape as u8;
        raw[6] = self.arc_phase as u8;
        raw[7] = confidence_q8;
        raw[8..12].copy_from_slice(&ts_bucketed.to_le_bytes());
        raw[12..14].copy_from_slice(&self.region_tag.to_le_bytes());
        raw[14] = self.opt_in_tier;
        raw[15] = self.cap_flags;
        raw[16..24].copy_from_slice(&emitter_handle.to_le_bytes());
        raw[24..32].copy_from_slice(&co_signer_set_hash.to_le_bytes());

        Ok(ChatPattern { raw })
    }
}

// ─── ChatPattern accessors ──────────────────────────────────────────────────

impl ChatPattern {
    /// § from_raw — wrap raw 32 bytes (assumes external validation).
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self { raw }
    }

    /// § as_bytes — borrow the canonical 32-byte form (transport blob).
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.raw
    }

    /// § pattern_id — BLAKE3-truncated id of (intent · shape · phase).
    /// Patterns with same shape collide ; enables k-anon counting.
    #[must_use]
    pub fn pattern_id(&self) -> u32 {
        u32::from_le_bytes([self.raw[0], self.raw[1], self.raw[2], self.raw[3]])
    }

    #[must_use]
    pub fn intent_kind(&self) -> IntentKind {
        IntentKind::from_u8(self.raw[4])
    }

    #[must_use]
    pub fn response_shape(&self) -> ResponseShape {
        ResponseShape::from_u8(self.raw[5])
    }

    #[must_use]
    pub fn arc_phase(&self) -> ArcPhase {
        ArcPhase::from_u8(self.raw[6])
    }

    /// § confidence — quantized back to f32 in [0, 1].
    #[must_use]
    pub fn confidence(&self) -> f32 {
        f32::from(self.raw[7]) / 255.0
    }

    #[must_use]
    pub fn confidence_q8(&self) -> u8 {
        self.raw[7]
    }

    #[must_use]
    pub fn ts_bucketed(&self) -> u32 {
        u32::from_le_bytes([self.raw[8], self.raw[9], self.raw[10], self.raw[11]])
    }

    #[must_use]
    pub fn region_tag(&self) -> u16 {
        u16::from_le_bytes([self.raw[12], self.raw[13]])
    }

    #[must_use]
    pub fn opt_in_tier_raw(&self) -> u8 {
        self.raw[14]
    }

    #[must_use]
    pub fn cap_flags(&self) -> u8 {
        self.raw[15]
    }

    /// § emitter_handle — non-recoverable BLAKE3-trunc of emitter pubkey.
    #[must_use]
    pub fn emitter_handle(&self) -> u64 {
        let mut buf = [0_u8; 8];
        buf.copy_from_slice(&self.raw[16..24]);
        u64::from_le_bytes(buf)
    }

    #[must_use]
    pub fn co_signer_set_hash(&self) -> u64 {
        let mut buf = [0_u8; 8];
        buf.copy_from_slice(&self.raw[24..32]);
        u64::from_le_bytes(buf)
    }

    /// § cap_check — Σ-mask gate at ingest. Returns `true` iff ALL required
    /// bits are set AND no reserved bits are present.
    #[must_use]
    pub fn cap_check(&self, required: u8) -> bool {
        let f = self.cap_flags();
        (f & CAP_FLAGS_RESERVED_MASK) == 0 && (f & required) == required
    }

    /// § validate — structural-validity check called at ingest.
    pub fn validate(&self) -> Result<(), PatternError> {
        if !matches!(self.intent_kind(), IntentKind::Unknown) || self.raw[4] == 255 {
            // ok : either matched or sentinel-Unknown
        }
        if self.opt_in_tier_raw() > 3 {
            return Err(PatternError::OptInTierOutOfRange(self.opt_in_tier_raw()));
        }
        if self.cap_flags() & CAP_FLAGS_RESERVED_MASK != 0 {
            return Err(PatternError::ReservedCapFlagsSet(self.cap_flags()));
        }
        Ok(())
    }
}

// ─── derivation helpers ─────────────────────────────────────────────────────

fn derive_pattern_id(intent: IntentKind, shape: ResponseShape, phase: ArcPhase) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-mycelium-chat-sync\0pattern_id\0v1");
    h.update(&[intent as u8, shape as u8, phase as u8]);
    let bytes = h.finalize();
    u32::from_le_bytes([
        bytes.as_bytes()[0],
        bytes.as_bytes()[1],
        bytes.as_bytes()[2],
        bytes.as_bytes()[3],
    ])
}

fn derive_emitter_handle(pubkey: &[u8; 32]) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-mycelium-chat-sync\0emitter_handle\0v1");
    h.update(pubkey);
    let bytes = h.finalize();
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(&bytes.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

fn derive_co_signer_set_hash(co_signers: &[[u8; 32]]) -> u64 {
    if co_signers.is_empty() {
        return 0;
    }
    // Sort first so set-equivalence collides regardless of insertion-order.
    let mut sorted: Vec<&[u8; 32]> = co_signers.iter().collect();
    sorted.sort();
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-mycelium-chat-sync\0co_signer_set\0v1");
    h.update(&(sorted.len() as u32).to_le_bytes());
    for k in sorted {
        h.update(k);
    }
    let bytes = h.finalize();
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(&bytes.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

// ─── errors ─────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum PatternError {
    #[error("confidence {0} out of range [0.0, 1.0]")]
    ConfidenceOutOfRange(f32),
    #[error("opt_in_tier {0} out of range [0, 3]")]
    OptInTierOutOfRange(u8),
    #[error("reserved cap_flags bits set : 0b{0:08b}")]
    ReservedCapFlagsSet(u8),
    #[error("cap_check failed : required 0b{required:08b}, had 0b{had:08b}")]
    CapDenied { required: u8, had: u8 },
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_builder() -> ChatPatternBuilder {
        ChatPatternBuilder {
            intent_kind: IntentKind::Question,
            response_shape: ResponseShape::ScenicNarrative,
            arc_phase: ArcPhase::RisingAction,
            confidence: 0.75,
            ts_unix: 60_000,
            region_tag: 7,
            opt_in_tier: 1,
            cap_flags: CAP_FLAGS_ALL,
            emitter_pubkey: [9_u8; 32],
            co_signers: vec![[1_u8; 32], [2_u8; 32], [3_u8; 32]],
        }
    }

    #[test]
    fn pack_size_is_32_bytes() {
        let p = mk_builder().build().unwrap();
        assert_eq!(p.as_bytes().len(), 32);
        assert_eq!(std::mem::size_of::<ChatPattern>(), 32);
    }

    #[test]
    fn round_trip_accessors() {
        let p = mk_builder().build().unwrap();
        assert_eq!(p.intent_kind(), IntentKind::Question);
        assert_eq!(p.response_shape(), ResponseShape::ScenicNarrative);
        assert_eq!(p.arc_phase(), ArcPhase::RisingAction);
        assert!((p.confidence() - 0.75).abs() < 0.005);
        assert_eq!(p.ts_bucketed(), 1000); // 60_000 / 60
        assert_eq!(p.region_tag(), 7);
        assert_eq!(p.opt_in_tier_raw(), 1);
        assert_eq!(p.cap_flags(), CAP_FLAGS_ALL);
    }

    #[test]
    fn pattern_id_collides_for_same_shape_diff_emitter() {
        let mut a = mk_builder();
        let mut b = mk_builder();
        a.emitter_pubkey = [0xAA; 32];
        b.emitter_pubkey = [0xBB; 32];
        let pa = a.build().unwrap();
        let pb = b.build().unwrap();
        assert_eq!(pa.pattern_id(), pb.pattern_id());
        assert_ne!(pa.emitter_handle(), pb.emitter_handle());
    }

    #[test]
    fn pattern_id_differs_for_different_shape() {
        let mut a = mk_builder();
        let mut b = mk_builder();
        a.intent_kind = IntentKind::Question;
        b.intent_kind = IntentKind::Combat;
        let pa = a.build().unwrap();
        let pb = b.build().unwrap();
        assert_ne!(pa.pattern_id(), pb.pattern_id());
    }

    #[test]
    fn confidence_out_of_range_rejected() {
        let mut b = mk_builder();
        b.confidence = 1.5;
        assert!(b.build().is_err());
        let mut b = mk_builder();
        b.confidence = -0.1;
        assert!(b.build().is_err());
        let mut b = mk_builder();
        b.confidence = f32::NAN;
        assert!(b.build().is_err());
    }

    #[test]
    fn opt_in_tier_out_of_range_rejected() {
        let mut b = mk_builder();
        b.opt_in_tier = 4;
        assert!(b.build().is_err());
    }

    #[test]
    fn reserved_cap_flags_rejected() {
        let mut b = mk_builder();
        b.cap_flags = 0b0001_0000;
        assert!(b.build().is_err());
    }

    #[test]
    fn cap_check_emit_allowed() {
        let p = mk_builder().build().unwrap();
        assert!(p.cap_check(CAP_EMIT_ALLOWED));
        assert!(p.cap_check(CAP_FEDERATION_INGEST));
        assert!(p.cap_check(CAP_FLAGS_ALL));
    }

    #[test]
    fn cap_check_deny_when_missing_bit() {
        let mut b = mk_builder();
        b.cap_flags = CAP_EMIT_ALLOWED; // missing FEDERATION_INGEST
        let p = b.build().unwrap();
        assert!(p.cap_check(CAP_EMIT_ALLOWED));
        assert!(!p.cap_check(CAP_FEDERATION_INGEST));
        assert!(!p.cap_check(CAP_FLAGS_ALL));
    }

    #[test]
    fn cap_check_default_deny() {
        let mut b = mk_builder();
        b.cap_flags = 0;
        let p = b.build().unwrap();
        assert!(!p.cap_check(CAP_EMIT_ALLOWED));
        assert!(!p.cap_check(CAP_FEDERATION_INGEST));
    }

    #[test]
    fn co_signer_set_hash_order_independent() {
        let mut a = mk_builder();
        let mut b = mk_builder();
        a.co_signers = vec![[1_u8; 32], [2_u8; 32], [3_u8; 32]];
        b.co_signers = vec![[3_u8; 32], [1_u8; 32], [2_u8; 32]];
        let pa = a.build().unwrap();
        let pb = b.build().unwrap();
        assert_eq!(pa.co_signer_set_hash(), pb.co_signer_set_hash());
    }

    #[test]
    fn co_signer_set_hash_differs_with_size() {
        let mut a = mk_builder();
        let mut b = mk_builder();
        a.co_signers = vec![[1_u8; 32]];
        b.co_signers = vec![[1_u8; 32], [2_u8; 32]];
        let pa = a.build().unwrap();
        let pb = b.build().unwrap();
        assert_ne!(pa.co_signer_set_hash(), pb.co_signer_set_hash());
    }

    #[test]
    fn co_signer_empty_yields_zero_hash() {
        let mut b = mk_builder();
        b.co_signers = vec![];
        let p = b.build().unwrap();
        assert_eq!(p.co_signer_set_hash(), 0);
    }

    #[test]
    fn round_trip_byte_serialize() {
        let p = mk_builder().build().unwrap();
        let bytes = p.as_bytes();
        let p2 = ChatPattern::from_raw(*bytes);
        assert_eq!(p, p2);
    }

    #[test]
    fn ts_bucketed_truncates_to_minutes() {
        let mut b = mk_builder();
        b.ts_unix = 60 * 100 + 37; // 100min + 37s — drops the 37s
        let p = b.build().unwrap();
        assert_eq!(p.ts_bucketed(), 100);
    }

    #[test]
    fn validate_passes_well_formed() {
        let p = mk_builder().build().unwrap();
        assert!(p.validate().is_ok());
    }
}

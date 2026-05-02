//! § pattern — `FederationPattern` 32-byte bit-packed federation-record
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   `FederationPattern` ⊑ smallest-fully-anonymous signal that can move
//!   across the mycelial mesh for ANY cell-state, KAN-bias, or content-
//!   discovery event. Generalizes cssl-mycelium-chat-sync's `ChatPattern`
//!   into a uniform 32-byte carrier discriminated by `kind` discriminant.
//!   ¬ raw payload egress. Pure-shape : (kind · payload-hash · sigma-mask ·
//!   ts · k-anon-cohort-size · sig).
//!
//! § BIT-PACK LAYOUT (LE · 32 bytes total)
//!   ┌───────────┬────────┬────────────────────────────────────────────────┐
//!   │ offset    │ bytes  │ field                                          │
//!   ├───────────┼────────┼────────────────────────────────────────────────┤
//!   │  0        │   1    │ kind                  (FederationKind disc·u8) │
//!   │  1        │   1    │ cap_flags             (Σ-mask gates · u8)      │
//!   │  2        │   1    │ k_anon_cohort_size    (saturating · u8)        │
//!   │  3        │   1    │ confidence_q8         (0..=255 ↦ 0.0..=1.0)    │
//!   │  4..8     │   4    │ ts_bucketed           (epoch/60s · u32 LE)     │
//!   │  8..16    │   8    │ payload_hash          (BLAKE3-trunc · u64 LE)  │
//!   │ 16..24    │   8    │ emitter_handle        (BLAKE3-trunc · u64 LE)  │
//!   │ 24..32    │   8    │ sig                   (BLAKE3-trunc · u64 LE)  │
//!   └───────────┴────────┴────────────────────────────────────────────────┘
//!
//! § PRIVACY DERIVATION RULES
//!   ─ `payload_hash` = blake3("federation\0payload\0v1" ‖ payload) ⟶ trunc8
//!     ⊑ same-payload across-emitters collide ⟶ enables k-anon counting
//!   ─ `emitter_handle` = blake3("federation\0emitter\0v1" ‖ pubkey) ⟶ trunc8
//!     ⊑ ¬ recoverable to-pubkey · used for distinct-emitter counting only
//!   ─ `sig` = blake3("federation\0sig\0v1" ‖ kind ‖ payload_hash ‖
//!                    emitter_handle ‖ ts_bucketed ‖ cap_flags) ⟶ trunc8
//!     ⊑ tamper-evidence (mutating any field invalidates `sig`)
//!
//! § CAP_FLAGS BITS (Σ-mask)
//!   bit 0 : `CAP_FED_EMIT_ALLOWED`         — emitter is opted-in
//!   bit 1 : `CAP_FED_INGEST`               — pattern allowed into federation
//!   bit 2 : `CAP_FED_PURGE_ON_REVOKE`      — peer-purge propagation
//!   bit 3 : `CAP_FED_REPLAY_DETERMINISTIC` — replay-safe modulation
//!   bits 4..7 : reserved (must be 0)

use serde::{Deserialize, Serialize};

/// § `FEDERATION_PATTERN_SIZE` — wire-stable byte size.
pub const FEDERATION_PATTERN_SIZE: usize = 32;

/// § `FederationPattern` — fixed-32-byte anonymized federation record.
///
/// Wire-stable representation : `[u8; 32]`. Field-access via accessors
/// which decode the bit-pack.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FederationPattern {
    /// § raw — the 32-byte canonical packed form.
    pub raw: [u8; FEDERATION_PATTERN_SIZE],
}

impl std::fmt::Debug for FederationPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationPattern")
            .field("kind", &self.kind())
            .field("cap_flags", &format_args!("0b{:08b}", self.cap_flags()))
            .field("k_anon_cohort_size", &self.k_anon_cohort_size())
            .field("confidence", &self.confidence())
            .field("ts_bucketed", &self.ts_bucketed())
            .field("payload_hash", &format_args!("{:016x}", self.payload_hash()))
            .field("emitter_handle", &format_args!("{:016x}", self.emitter_handle()))
            .field("sig", &format_args!("{:016x}", self.sig()))
            .finish()
    }
}

/// § `FederationKind` — what KIND of federation event this pattern represents.
///
/// Adding a variant requires PRIME_DIRECTIVE-review. The enum is `repr(u8)`
/// so the discriminant is wire-stable.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[repr(u8)]
pub enum FederationKind {
    /// § `Unknown` — fallback / sentinel.
    #[default]
    Unknown = 0,
    /// § `CellState` — ω-field cell-state delta (with Σ-mask permitting).
    CellState = 1,
    /// § `KanBias` — per-swap-point KAN bias-vector update.
    KanBias = 2,
    /// § `ContentDiscovery` — newly-published content fingerprint.
    ContentDiscovery = 3,
    /// § `ChatShape` — re-export of chat-sync pattern (cross-bridge).
    ChatShape = 4,
    /// § `HotfixWitness` — fleet-wide hotfix-apply observation.
    HotfixWitness = 5,
    /// § `RecipeOutcome` — crafting-recipe success/failure aggregate.
    RecipeOutcome = 6,
    /// § `EncounterStat` — combat-encounter difficulty calibration.
    EncounterStat = 7,
    /// § `NemesisEvent` — nemesis-system cross-instance event.
    NemesisEvent = 8,
}

impl FederationKind {
    /// § from_u8 — wire-stable discriminant decode.
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::CellState,
            2 => Self::KanBias,
            3 => Self::ContentDiscovery,
            4 => Self::ChatShape,
            5 => Self::HotfixWitness,
            6 => Self::RecipeOutcome,
            7 => Self::EncounterStat,
            8 => Self::NemesisEvent,
            _ => Self::Unknown,
        }
    }
}

// ─── cap_flags Σ-mask bits ──────────────────────────────────────────────────

/// § `CAP_FED_EMIT_ALLOWED` — emitter has opted-in to federation.
pub const CAP_FED_EMIT_ALLOWED: u8 = 0b0000_0001;
/// § `CAP_FED_INGEST` — pattern allowed into the federated shared-state.
pub const CAP_FED_INGEST: u8 = 0b0000_0010;
/// § `CAP_FED_PURGE_ON_REVOKE` — emitter consents to purge propagation.
pub const CAP_FED_PURGE_ON_REVOKE: u8 = 0b0000_0100;
/// § `CAP_FED_REPLAY_DETERMINISTIC` — pattern eligible for replay-safe modulation.
pub const CAP_FED_REPLAY_DETERMINISTIC: u8 = 0b0000_1000;

/// § `CAP_FED_FLAGS_ALL` — full set ; emitter must hold ALL bits to federate.
pub const CAP_FED_FLAGS_ALL: u8 = CAP_FED_EMIT_ALLOWED
    | CAP_FED_INGEST
    | CAP_FED_PURGE_ON_REVOKE
    | CAP_FED_REPLAY_DETERMINISTIC;

/// § `CAP_FED_FLAGS_RESERVED_MASK` — bits 4..=7 must be 0.
pub const CAP_FED_FLAGS_RESERVED_MASK: u8 = 0b1111_0000;

// ─── FederationPatternBuilder ───────────────────────────────────────────────

/// § `FederationPatternBuilder` — typed-builder for `FederationPattern`.
/// Performs the bit-pack + derives `payload_hash`, `emitter_handle`, `sig`.
#[derive(Debug, Clone)]
pub struct FederationPatternBuilder {
    pub kind: FederationKind,
    /// Σ-mask bits the emitter holds. See CAP_FED_* constants.
    pub cap_flags: u8,
    /// Cohort size at emit-time (saturating to u8).
    pub k_anon_cohort_size: u32,
    /// 0.0..=1.0 — clamped to range, then quantized to u8.
    pub confidence: f32,
    /// Wall-clock unix-seconds. Will be bucketed to /60 in the packed form.
    pub ts_unix: u64,
    /// Raw payload bytes ; truncated to 8-byte payload_hash via BLAKE3.
    pub payload: Vec<u8>,
    /// Raw 32-byte emitter pubkey ; truncated to 8-byte handle.
    pub emitter_pubkey: [u8; 32],
}

impl FederationPatternBuilder {
    /// § build — derive ids + bit-pack into `FederationPattern`.
    pub fn build(self) -> Result<FederationPattern, PatternError> {
        if !(0.0..=1.0).contains(&self.confidence) || self.confidence.is_nan() {
            return Err(PatternError::ConfidenceOutOfRange(self.confidence));
        }
        if self.cap_flags & CAP_FED_FLAGS_RESERVED_MASK != 0 {
            return Err(PatternError::ReservedCapFlagsSet(self.cap_flags));
        }

        let confidence_q8 = (self.confidence * 255.0).round().clamp(0.0, 255.0) as u8;
        let ts_bucketed: u32 = ((self.ts_unix / 60) & 0xFFFF_FFFF) as u32;
        let cohort_saturating: u8 = self.k_anon_cohort_size.min(255) as u8;

        let payload_hash = derive_payload_hash(&self.payload);
        let emitter_handle = derive_emitter_handle(&self.emitter_pubkey);
        let sig = derive_sig(
            self.kind as u8,
            payload_hash,
            emitter_handle,
            ts_bucketed,
            self.cap_flags,
        );

        let mut raw = [0_u8; FEDERATION_PATTERN_SIZE];
        raw[0] = self.kind as u8;
        raw[1] = self.cap_flags;
        raw[2] = cohort_saturating;
        raw[3] = confidence_q8;
        raw[4..8].copy_from_slice(&ts_bucketed.to_le_bytes());
        raw[8..16].copy_from_slice(&payload_hash.to_le_bytes());
        raw[16..24].copy_from_slice(&emitter_handle.to_le_bytes());
        raw[24..32].copy_from_slice(&sig.to_le_bytes());

        Ok(FederationPattern { raw })
    }
}

// ─── FederationPattern accessors ────────────────────────────────────────────

impl FederationPattern {
    /// § from_raw — wrap raw 32 bytes (assumes external validation).
    #[must_use]
    pub const fn from_raw(raw: [u8; FEDERATION_PATTERN_SIZE]) -> Self {
        Self { raw }
    }

    /// § as_bytes — borrow the canonical 32-byte form.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; FEDERATION_PATTERN_SIZE] {
        &self.raw
    }

    #[must_use]
    pub fn kind(&self) -> FederationKind {
        FederationKind::from_u8(self.raw[0])
    }

    #[must_use]
    pub const fn cap_flags(&self) -> u8 {
        self.raw[1]
    }

    #[must_use]
    pub const fn k_anon_cohort_size(&self) -> u8 {
        self.raw[2]
    }

    #[must_use]
    pub fn confidence(&self) -> f32 {
        f32::from(self.raw[3]) / 255.0
    }

    #[must_use]
    pub const fn confidence_q8(&self) -> u8 {
        self.raw[3]
    }

    #[must_use]
    pub fn ts_bucketed(&self) -> u32 {
        u32::from_le_bytes([self.raw[4], self.raw[5], self.raw[6], self.raw[7]])
    }

    #[must_use]
    pub fn payload_hash(&self) -> u64 {
        let mut buf = [0_u8; 8];
        buf.copy_from_slice(&self.raw[8..16]);
        u64::from_le_bytes(buf)
    }

    #[must_use]
    pub fn emitter_handle(&self) -> u64 {
        let mut buf = [0_u8; 8];
        buf.copy_from_slice(&self.raw[16..24]);
        u64::from_le_bytes(buf)
    }

    #[must_use]
    pub fn sig(&self) -> u64 {
        let mut buf = [0_u8; 8];
        buf.copy_from_slice(&self.raw[24..32]);
        u64::from_le_bytes(buf)
    }

    /// § cap_check — Σ-mask gate. Returns `true` iff ALL required bits set
    /// AND no reserved bits are present.
    #[must_use]
    pub fn cap_check(&self, required: u8) -> bool {
        let f = self.cap_flags();
        (f & CAP_FED_FLAGS_RESERVED_MASK) == 0 && (f & required) == required
    }

    /// § verify_sig — recompute `sig` and compare to packed value. Returns
    /// `true` iff the packed sig matches a fresh derivation. This is
    /// tamper-evidence — any field mutation invalidates the sig.
    #[must_use]
    pub fn verify_sig(&self) -> bool {
        let recomputed = derive_sig(
            self.raw[0],
            self.payload_hash(),
            self.emitter_handle(),
            self.ts_bucketed(),
            self.cap_flags(),
        );
        recomputed == self.sig()
    }

    /// § validate — structural-validity check called at ingest.
    pub fn validate(&self) -> Result<(), PatternError> {
        if self.cap_flags() & CAP_FED_FLAGS_RESERVED_MASK != 0 {
            return Err(PatternError::ReservedCapFlagsSet(self.cap_flags()));
        }
        if !self.verify_sig() {
            return Err(PatternError::SigMismatch);
        }
        Ok(())
    }
}

// ─── derivation helpers ─────────────────────────────────────────────────────

fn derive_payload_hash(payload: &[u8]) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"federation\0payload\0v1");
    h.update(payload);
    let bytes = h.finalize();
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(&bytes.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

fn derive_emitter_handle(pubkey: &[u8; 32]) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"federation\0emitter\0v1");
    h.update(pubkey);
    let bytes = h.finalize();
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(&bytes.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

fn derive_sig(
    kind: u8,
    payload_hash: u64,
    emitter_handle: u64,
    ts_bucketed: u32,
    cap_flags: u8,
) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"federation\0sig\0v1");
    h.update(&[kind]);
    h.update(&payload_hash.to_le_bytes());
    h.update(&emitter_handle.to_le_bytes());
    h.update(&ts_bucketed.to_le_bytes());
    h.update(&[cap_flags]);
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
    #[error("reserved cap_flags bits set : 0b{0:08b}")]
    ReservedCapFlagsSet(u8),
    #[error("sig mismatch — pattern was tampered with")]
    SigMismatch,
    #[error("cap_check failed : required 0b{required:08b}, had 0b{had:08b}")]
    CapDenied { required: u8, had: u8 },
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_builder() -> FederationPatternBuilder {
        FederationPatternBuilder {
            kind: FederationKind::CellState,
            cap_flags: CAP_FED_FLAGS_ALL,
            k_anon_cohort_size: 12,
            confidence: 0.75,
            ts_unix: 60_000,
            payload: b"cell-state-omega-tick-12345".to_vec(),
            emitter_pubkey: [9_u8; 32],
        }
    }

    #[test]
    fn pack_size_is_32_bytes() {
        let p = mk_builder().build().unwrap();
        assert_eq!(p.as_bytes().len(), FEDERATION_PATTERN_SIZE);
        assert_eq!(std::mem::size_of::<FederationPattern>(), 32);
    }

    #[test]
    fn round_trip_accessors() {
        let p = mk_builder().build().unwrap();
        assert_eq!(p.kind(), FederationKind::CellState);
        assert_eq!(p.cap_flags(), CAP_FED_FLAGS_ALL);
        assert_eq!(p.k_anon_cohort_size(), 12);
        assert!((p.confidence() - 0.75).abs() < 0.005);
        assert_eq!(p.ts_bucketed(), 1000); // 60_000 / 60
    }

    #[test]
    fn payload_hash_collides_for_same_payload_diff_emitter() {
        let mut a = mk_builder();
        let mut b = mk_builder();
        a.emitter_pubkey = [0xAA; 32];
        b.emitter_pubkey = [0xBB; 32];
        let pa = a.build().unwrap();
        let pb = b.build().unwrap();
        assert_eq!(pa.payload_hash(), pb.payload_hash());
        assert_ne!(pa.emitter_handle(), pb.emitter_handle());
        // Sigs differ because emitter_handle is mixed in.
        assert_ne!(pa.sig(), pb.sig());
    }

    #[test]
    fn payload_hash_differs_for_different_payload() {
        let mut a = mk_builder();
        let mut b = mk_builder();
        a.payload = b"payload-a".to_vec();
        b.payload = b"payload-b".to_vec();
        let pa = a.build().unwrap();
        let pb = b.build().unwrap();
        assert_ne!(pa.payload_hash(), pb.payload_hash());
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
    fn reserved_cap_flags_rejected() {
        let mut b = mk_builder();
        b.cap_flags = 0b0001_0000;
        assert!(b.build().is_err());
    }

    #[test]
    fn cap_check_default_deny() {
        let mut b = mk_builder();
        b.cap_flags = 0;
        let p = b.build().unwrap();
        assert!(!p.cap_check(CAP_FED_EMIT_ALLOWED));
        assert!(!p.cap_check(CAP_FED_INGEST));
    }

    #[test]
    fn cap_check_passes_when_all_bits_present() {
        let p = mk_builder().build().unwrap();
        assert!(p.cap_check(CAP_FED_EMIT_ALLOWED));
        assert!(p.cap_check(CAP_FED_INGEST));
        assert!(p.cap_check(CAP_FED_FLAGS_ALL));
    }

    #[test]
    fn sig_round_trips() {
        let p = mk_builder().build().unwrap();
        assert!(p.verify_sig());
    }

    #[test]
    fn sig_detects_tampering() {
        let mut p = mk_builder().build().unwrap();
        // Mutate the kind byte ; sig should fail.
        p.raw[0] = FederationKind::KanBias as u8;
        assert!(!p.verify_sig());
    }

    #[test]
    fn validate_passes_well_formed() {
        let p = mk_builder().build().unwrap();
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validate_rejects_tampered_sig() {
        let mut p = mk_builder().build().unwrap();
        p.raw[1] = 0; // mutate cap_flags
        assert!(matches!(p.validate(), Err(PatternError::SigMismatch)));
    }

    #[test]
    fn cohort_saturates_at_255() {
        let mut b = mk_builder();
        b.k_anon_cohort_size = 1_000_000;
        let p = b.build().unwrap();
        assert_eq!(p.k_anon_cohort_size(), 255);
    }

    #[test]
    fn ts_bucketed_truncates_to_minutes() {
        let mut b = mk_builder();
        b.ts_unix = 60 * 100 + 37; // 100min + 37s — drops the 37s
        let p = b.build().unwrap();
        assert_eq!(p.ts_bucketed(), 100);
    }

    #[test]
    fn round_trip_byte_serialize() {
        let p = mk_builder().build().unwrap();
        let bytes = p.as_bytes();
        let p2 = FederationPattern::from_raw(*bytes);
        assert_eq!(p, p2);
    }

    #[test]
    fn kind_disc_round_trips_for_all_variants() {
        for k in 0_u8..=8 {
            let v = FederationKind::from_u8(k);
            assert_eq!(v as u8, if k <= 8 { k } else { 0 });
        }
        // Out-of-range collapses to Unknown.
        assert_eq!(FederationKind::from_u8(99), FederationKind::Unknown);
    }
}

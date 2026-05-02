//! § mask.rs — Σ-mask runtime bit-layout (19 B packed) + BLAKE3-128 checksum.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § BIT-LAYOUT (19 bytes packed · little-endian)
//!
//! ```text
//!  offset | bytes | field            | semantic
//!  -------+-------+------------------+-----------------------------------------
//!    0    |   2   | audience_class   | u16 bitset · 16 audience-class slots
//!    2    |   4   | effect_caps      | u32 bitset · 32 effect-bit slots
//!    6    |   1   | k_anon_thresh    | u8 · 0 = no-aggregate · else floor
//!    7    |   4   | ttl_seconds      | u32 · 0 = no-TTL · else auto-revoke
//!   11    |   4   | revoked_at       | u32 · 0 = active · else revocation-ts
//!   15    |   1   | flags            | u8  · PROPAGATE / INHERIT / OVERRIDE / ATTESTED
//!   16    |   3   | checksum_low24   | u24 · BLAKE3-128 truncated lowest 24 bits
//!  -------+-------+------------------+-----------------------------------------
//!                  19 B total
//! ```
//!
//! § DESIGN (per memory_sawyer_pokemon_efficiency)
//!   Bit-packed records · pre-alloc · NO Vec inside SigmaMask · checksum is
//!   the lowest 24 bits of a BLAKE3-128 keyed-hash so tamper-detect is one
//!   blake3 invocation + a u32 compare. The 24-bit truncation gives a
//!   ≈1-in-16M false-positive ceiling on raw bit-flips — sufficient for
//!   in-memory tamper-detection (the canonical full-strength check is
//!   the Σ-Chain on-disk record, not this in-memory cache).
//!
//! § FLAGS (1 byte · 8 bits)
//!
//! ```text
//!   bit 0 : PROPAGATE   — child cells inherit a strictly-tighter mask
//!   bit 1 : INHERIT     — this mask was derived from a parent (not original)
//!   bit 2 : OVERRIDE    — sovereign-explicit override of inherited rules
//!   bit 3 : ATTESTED    — mask carries a sovereign-cap attestation
//!   bit 4 : RESERVED
//!   bit 5 : RESERVED
//!   bit 6 : RESERVED
//!   bit 7 : RESERVED
//! ```

use core::time::Duration;

/// Total packed byte-size of [`SigmaMask`] (19 B).
///
/// § STABILITY : ABI-frozen. Adding fields = spec-amendment + DECISIONS entry.
pub const MASK_PACKED_BYTES: usize = 19;

// ── audience-class bits (u16 · 16 slots) ────────────────────────────────────

/// Audience-class : holder-only (the sovereign-self).
pub const AUDIENCE_SELF: u16 = 1 << 0;
/// Audience-class : trusted circle (e.g. friend-list / consented-peers).
pub const AUDIENCE_CIRCLE: u16 = 1 << 1;
/// Audience-class : public (anyone).
pub const AUDIENCE_PUBLIC: u16 = 1 << 2;
/// Audience-class : admin (operator / moderator / sovereign-delegate).
pub const AUDIENCE_ADMIN: u16 = 1 << 3;
/// Audience-class : system (substrate-level processes · non-human).
pub const AUDIENCE_SYSTEM: u16 = 1 << 4;
/// Audience-class : derived (analytics / aggregator output · k-anon-required).
pub const AUDIENCE_DERIVED: u16 = 1 << 5;
// bits 6..=15 are RESERVED-FOR-EXTENSION.

/// Convenience enum view of the 16 canonical audience-class bits.
///
/// § STABILITY : variant-positions are FROZEN. Adding a slot = spec-amendment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum AudienceBit {
    SelfOnly = AUDIENCE_SELF,
    Circle = AUDIENCE_CIRCLE,
    Public = AUDIENCE_PUBLIC,
    Admin = AUDIENCE_ADMIN,
    System = AUDIENCE_SYSTEM,
    Derived = AUDIENCE_DERIVED,
}

// ── effect-cap bits (u32 · 32 slots) ────────────────────────────────────────

/// Effect : observe / read scalar value.
pub const EFFECT_READ: u32 = 1 << 0;
/// Effect : mutate / write scalar value.
pub const EFFECT_WRITE: u32 = 1 << 1;
/// Effect : derive / aggregate (analytics·summary).
pub const EFFECT_DERIVE: u32 = 1 << 2;
/// Effect : broadcast / publish to audience.
pub const EFFECT_BROADCAST: u32 = 1 << 3;
/// Effect : purge / GDPR-forget.
pub const EFFECT_PURGE: u32 = 1 << 4;
/// Effect : log / append to audit-trail (separate from runtime audit-ring).
pub const EFFECT_LOG: u32 = 1 << 5;
// bits 6..=31 are RESERVED-FOR-EXTENSION.

/// Convenience enum view of the canonical effect-cap bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum EffectCap {
    Read = EFFECT_READ,
    Write = EFFECT_WRITE,
    Derive = EFFECT_DERIVE,
    Broadcast = EFFECT_BROADCAST,
    Purge = EFFECT_PURGE,
    Log = EFFECT_LOG,
}

// ── flag bits (u8 · 8 slots) ────────────────────────────────────────────────

/// Flag : children inherit a strictly-tighter mask via [`crate::propagation::compose_parent_child`].
pub const FLAG_PROPAGATE: u8 = 1 << 0;
/// Flag : this mask was derived from a parent · cf. INHERIT vs ORIGINAL.
pub const FLAG_INHERIT: u8 = 1 << 1;
/// Flag : sovereign-explicit override of inherited rules (parent loses).
pub const FLAG_OVERRIDE: u8 = 1 << 2;
/// Flag : mask carries an attached SovereignCap attestation.
pub const FLAG_ATTESTED: u8 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MaskFlag {
    Propagate = FLAG_PROPAGATE,
    Inherit = FLAG_INHERIT,
    Override = FLAG_OVERRIDE,
    Attested = FLAG_ATTESTED,
}

// ───────────────────────────────────────────────────────────────────────────
// § SigmaMask — runtime gate-record
// ───────────────────────────────────────────────────────────────────────────

/// Runtime Σ-mask record · 19 B packed · gate-fn input.
///
/// Distinct from [`cssl-substrate-prime-directive`]'s `SigmaMaskPacked` (16 B
/// per-cell). This record represents the AUDIENCE+EFFECT+TTL+REVOCATION shape
/// used by aggregator / chat-sync / hotfix / akashic crates.
///
/// § INVARIANTS
///   - `checksum_low24` reflects BLAKE3-128 of bytes 0..16 keyed by
///     [`SIGMA_MASK_CHECKSUM_KEY`] · validated by [`SigmaMask::verify_checksum`].
///   - `revoked_at == 0` means active ; any non-zero value means revoked at
///     that wall-clock-second timestamp.
///   - `ttl_seconds == 0` means no-TTL ; the mask never auto-expires.
///   - Public mutation surface : [`SigmaMask::new`] · [`SigmaMask::revoke`].
///     There is NO `set_field` API ; rebuild the mask from scratch + rehash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigmaMask {
    audience_class: u16,
    effect_caps: u32,
    k_anon_thresh: u8,
    ttl_seconds: u32,
    revoked_at: u32,
    flags: u8,
    /// Lowest-24-bits of BLAKE3-128 keyed-hash over bytes 0..16.
    /// Stored as u32 (u24-effective) to avoid an unaligned-3-byte field.
    checksum_low24: u32,
    /// Wall-clock-second timestamp the mask was created at. NOT part of the
    /// 19 B canonical packed-form ; carried alongside for TTL evaluation.
    /// Hashed-into checksum so that two masks with identical bits but
    /// different birth-times tamper-mismatch.
    created_at: u64,
}

/// Domain-separated BLAKE3 keyed-hash key for SigmaMask checksums.
///
/// § DESIGN : keyed-hash prevents collision-attacks where a caller crafts a
/// SigmaMask that hashes to the same checksum as a known-good one.
const SIGMA_MASK_CHECKSUM_KEY: [u8; 32] = *b"cssl.sigma-runtime.checksum.k01\0";

impl SigmaMask {
    /// Construct a fresh runtime Σ-mask.
    ///
    /// § ARGS
    ///   - `audience_class` : OR-combined [`AudienceBit`] values.
    ///   - `effect_caps`    : OR-combined [`EffectCap`] values.
    ///   - `k_anon_thresh`  : k-anonymity floor for `Derived` audience.
    ///                        0 = no aggregation required.
    ///   - `ttl_seconds`    : 0 = no-TTL ; else seconds-after-`created_at`
    ///                        the mask auto-expires.
    ///   - `flags`          : OR-combined [`MaskFlag`] values.
    ///   - `created_at`     : wall-clock-second timestamp · caller-supplied
    ///                        for deterministic-replay.
    pub fn new(
        audience_class: u16,
        effect_caps: u32,
        k_anon_thresh: u8,
        ttl_seconds: u32,
        flags: u8,
        created_at: u64,
    ) -> Self {
        let mut m = Self {
            audience_class,
            effect_caps,
            k_anon_thresh,
            ttl_seconds,
            revoked_at: 0,
            flags,
            checksum_low24: 0,
            created_at,
        };
        m.checksum_low24 = m.compute_checksum();
        m
    }

    // ── accessors (read-only · no setters) ─────────────────────────────

    pub const fn audience_class(&self) -> u16 {
        self.audience_class
    }
    pub const fn effect_caps(&self) -> u32 {
        self.effect_caps
    }
    pub const fn k_anon_thresh(&self) -> u8 {
        self.k_anon_thresh
    }
    pub const fn ttl_seconds(&self) -> u32 {
        self.ttl_seconds
    }
    pub const fn revoked_at(&self) -> u32 {
        self.revoked_at
    }
    pub const fn flags(&self) -> u8 {
        self.flags
    }
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// Test whether a flag bit is set.
    pub const fn has_flag(&self, flag: u8) -> bool {
        (self.flags & flag) == flag
    }

    /// Test whether an audience bit is set.
    pub const fn allows_audience(&self, audience: u16) -> bool {
        // ¬ subset-narrow : we only require AT-LEAST-ONE of the requested
        // audience-bits to be in the mask's audience-class. Callers asking
        // for AUDIENCE_PUBLIC alone match a mask that lists Public.
        (self.audience_class & audience) != 0
    }

    /// Test whether an effect-cap bit is set.
    pub const fn permits_effect(&self, effect: u32) -> bool {
        (self.effect_caps & effect) == effect
    }

    /// Test whether the mask has been explicitly revoked.
    pub const fn is_revoked(&self) -> bool {
        self.revoked_at != 0
    }

    /// Test whether the mask has expired given a wall-clock-second now-ts.
    pub const fn is_expired(&self, now_seconds: u64) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }
        // saturating arith : never wrap into a fake-active state.
        let expires_at = self.created_at.saturating_add(self.ttl_seconds as u64);
        now_seconds >= expires_at
    }

    /// Wall-clock-second the TTL elapses ; 0 = no TTL.
    pub const fn expires_at(&self) -> u64 {
        if self.ttl_seconds == 0 {
            0
        } else {
            self.created_at.saturating_add(self.ttl_seconds as u64)
        }
    }

    // ── revocation (mutates `revoked_at` + rehashes) ──────────────────

    /// Revoke the mask · sets `revoked_at` to the supplied wall-clock second.
    ///
    /// § PRIME_DIRECTIVE § 5 revocability : revocation is per-mask, monotone-
    /// not-reversible (re-grant requires a fresh mask + new SovereignCap).
    pub fn revoke(&mut self, revoked_at_seconds: u32) {
        // 0 is reserved for active ; coerce 0 to 1 to preserve the invariant.
        self.revoked_at = revoked_at_seconds.max(1);
        self.checksum_low24 = self.compute_checksum();
    }

    // ── tamper-detection ──────────────────────────────────────────────

    /// Recompute the checksum + compare against the stored value.
    ///
    /// § INVARIANT : `verify_checksum() == true` iff the in-memory bytes
    /// match the BLAKE3-128 keyed-hash recorded at construction-time.
    pub fn verify_checksum(&self) -> bool {
        self.compute_checksum() == self.checksum_low24
    }

    /// Compute the canonical BLAKE3-128 keyed-hash low-24-bits over the
    /// canonical-byte-form of the mask (excluding the checksum field itself).
    fn compute_checksum(&self) -> u32 {
        let mut buf = [0u8; 16];
        // Canonical packing : little-endian per std430.
        buf[0..2].copy_from_slice(&self.audience_class.to_le_bytes());
        buf[2..6].copy_from_slice(&self.effect_caps.to_le_bytes());
        buf[6] = self.k_anon_thresh;
        buf[7..11].copy_from_slice(&self.ttl_seconds.to_le_bytes());
        buf[11..15].copy_from_slice(&self.revoked_at.to_le_bytes());
        buf[15] = self.flags;
        let mut hasher = blake3::Hasher::new_keyed(&SIGMA_MASK_CHECKSUM_KEY);
        hasher.update(&buf);
        hasher.update(&self.created_at.to_le_bytes());
        let h = hasher.finalize();
        let bytes = h.as_bytes();
        // Low 24 bits = bytes[0..3]
        u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
    }

    // ── canonical byte-form ───────────────────────────────────────────

    /// Pack into canonical 19 B little-endian byte-form.
    ///
    /// § DESIGN : used by Σ-Chain on-disk encoding · NOT by hot-path
    /// evaluator (which reads fields directly).
    pub fn pack(&self) -> [u8; MASK_PACKED_BYTES] {
        let mut out = [0u8; MASK_PACKED_BYTES];
        out[0..2].copy_from_slice(&self.audience_class.to_le_bytes());
        out[2..6].copy_from_slice(&self.effect_caps.to_le_bytes());
        out[6] = self.k_anon_thresh;
        out[7..11].copy_from_slice(&self.ttl_seconds.to_le_bytes());
        out[11..15].copy_from_slice(&self.revoked_at.to_le_bytes());
        out[15] = self.flags;
        out[16] = (self.checksum_low24 & 0xFF) as u8;
        out[17] = ((self.checksum_low24 >> 8) & 0xFF) as u8;
        out[18] = ((self.checksum_low24 >> 16) & 0xFF) as u8;
        out
    }

    /// Convenience constructor for a TTL given as a [`Duration`] · seconds-only
    /// (sub-second is truncated). Used by host crates that already hold
    /// a `Duration`.
    pub fn with_ttl_duration(
        audience_class: u16,
        effect_caps: u32,
        k_anon_thresh: u8,
        ttl: Duration,
        flags: u8,
        created_at: u64,
    ) -> Self {
        let ttl_seconds = u32::try_from(ttl.as_secs()).unwrap_or(u32::MAX);
        Self::new(
            audience_class,
            effect_caps,
            k_anon_thresh,
            ttl_seconds,
            flags,
            created_at,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests · unit-level (composition + audit + evaluator suites in own files)
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_pack_size_is_19_bytes() {
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let bytes = m.pack();
        assert_eq!(bytes.len(), MASK_PACKED_BYTES);
    }

    #[test]
    fn t02_checksum_validates_on_fresh_mask() {
        let m = SigmaMask::new(
            AUDIENCE_SELF | AUDIENCE_CIRCLE,
            EFFECT_READ | EFFECT_DERIVE,
            5,
            3600,
            FLAG_PROPAGATE,
            1_000,
        );
        assert!(m.verify_checksum(), "fresh mask must hash-validate");
    }

    #[test]
    fn t03_checksum_detects_field_tamper() {
        let m = SigmaMask::new(AUDIENCE_PUBLIC, EFFECT_READ, 0, 0, 0, 1_000);
        let mut tampered = m;
        // simulate adversarial in-memory tamper :
        tampered.audience_class = AUDIENCE_PUBLIC | AUDIENCE_ADMIN;
        // checksum stale ⇒ verify must FAIL.
        assert!(!tampered.verify_checksum());
    }

    #[test]
    fn t04_revoke_sets_revoked_at_and_rehashes() {
        let mut m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        assert!(!m.is_revoked());
        m.revoke(2_000);
        assert!(m.is_revoked());
        assert!(m.verify_checksum(), "revoke rehashes");
        assert_eq!(m.revoked_at(), 2_000);
    }

    #[test]
    fn t05_revoke_zero_coerces_to_one() {
        let mut m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        m.revoke(0);
        assert_eq!(m.revoked_at(), 1, "revoke(0) → 1 to preserve active=0 invariant");
        assert!(m.is_revoked());
    }

    #[test]
    fn t06_ttl_zero_means_no_expiry() {
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        assert!(!m.is_expired(u64::MAX), "TTL 0 ⇒ never expires");
        assert_eq!(m.expires_at(), 0);
    }

    #[test]
    fn t07_ttl_nonzero_expires_when_now_geq_expiry() {
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 60, 0, 1_000);
        assert!(!m.is_expired(1_000));
        assert!(!m.is_expired(1_059));
        assert!(m.is_expired(1_060));
        assert!(m.is_expired(2_000));
        assert_eq!(m.expires_at(), 1_060);
    }

    #[test]
    fn t08_audience_and_effect_query() {
        let m = SigmaMask::new(
            AUDIENCE_SELF | AUDIENCE_CIRCLE,
            EFFECT_READ | EFFECT_WRITE | EFFECT_DERIVE,
            0,
            0,
            FLAG_PROPAGATE | FLAG_ATTESTED,
            0,
        );
        assert!(m.allows_audience(AUDIENCE_SELF));
        assert!(m.allows_audience(AUDIENCE_CIRCLE));
        assert!(!m.allows_audience(AUDIENCE_PUBLIC));
        assert!(m.permits_effect(EFFECT_READ));
        assert!(m.permits_effect(EFFECT_READ | EFFECT_WRITE));
        assert!(!m.permits_effect(EFFECT_PURGE));
        assert!(m.has_flag(FLAG_PROPAGATE));
        assert!(m.has_flag(FLAG_ATTESTED));
        assert!(!m.has_flag(FLAG_OVERRIDE));
    }

    #[test]
    fn t09_with_ttl_duration_truncates_subsecond() {
        let m = SigmaMask::with_ttl_duration(
            AUDIENCE_SELF,
            EFFECT_READ,
            0,
            Duration::from_millis(60_500),
            0,
            1_000,
        );
        assert_eq!(m.ttl_seconds(), 60);
    }

    #[test]
    fn t10_pack_roundtrip_preserves_fields() {
        let m = SigmaMask::new(
            AUDIENCE_DERIVED,
            EFFECT_DERIVE | EFFECT_LOG,
            10,
            7200,
            FLAG_PROPAGATE,
            42_000,
        );
        let packed = m.pack();
        // We don't expose unpack(); re-build + compare canonical bytes.
        let m2 = SigmaMask::new(
            AUDIENCE_DERIVED,
            EFFECT_DERIVE | EFFECT_LOG,
            10,
            7200,
            FLAG_PROPAGATE,
            42_000,
        );
        assert_eq!(packed, m2.pack(), "deterministic pack across constructions");
    }
}

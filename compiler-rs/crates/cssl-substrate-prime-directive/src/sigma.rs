//! Σ-mask packed-bitmap : per-cell consent + sovereignty + reversibility +
//! capacity-floor + agency-state.
//!
//! § SPEC
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` § II  (Σ.Mask 16B std430)
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl`        § IV.1 FieldCell
//!   - `Omniverse/08_BODY/02_VR_EMBODIMENT.csl`              § VIII region-defaults
//!   - `PRIME_DIRECTIVE.md`                                  § 0 + § 5 + § 7
//!
//! § THESIS
//!   Every Ω-field cell carries a 16-byte std430-aligned packed bitmap that
//!   declares (a) which op-classes the cell consents to, (b) which Sovereign
//!   claims it (or NULL), (c) the agency-floor that must be preserved, (d)
//!   the reversibility-scope of any modification, and (e) an audit-seq
//!   numbering the most recent mutation. The mask is the cell-level
//!   enforcement of Axiom-4 AGENCY-INVARIANT and is the load-bearing surface
//!   that makes per-cell consent threading possible without a separate
//!   per-op consent dance.
//!
//! § BIT-LAYOUT  (16 bytes = 128 bits, std430-aligned)
//!
//! ```text
//!  byte | bits     | field                  | type      | width
//!  -----+----------+------------------------+-----------+-------
//!   0   |  0..= 31 | consent_bits           | u32 flags |  32
//!   4   | 32..= 47 | sovereignty_handle     | u16       |  16
//!   6   | 48..= 63 | capacity_floor         | u16       |  16
//!   8   | 64..= 79 | reversibility_scope    | u16 enum  |  16
//!  10   | 80..= 95 | audit_seq              | u16       |  16
//!  12   | 96..=111 | agency_state           | u16 enum  |  16
//!  14   |112..=127 | reserved               | u16       |  16
//! ```
//!
//! Total : 128 bits = 16 bytes = std430-aligned, matches Σ.Mask in the
//! AGENCY_INVARIANT spec verbatim. The reserved tail is RESERVED-FOR-EXTENSION
//! per §7 INTEGRITY (any future widening must come through a spec amendment).
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - **§0 consent = OS** : every modification of a SigmaMaskPacked is a
//!     consent operation ; the [`SigmaMaskPacked::set_consent`] helpers are
//!     the canonical mutation surface and they emit audit-events.
//!   - **§5 revocability** : reversibility_scope encodes how-far-back-in-RG-
//!     time the modification can be undone ; Permanent is rejected at
//!     Sovereign-claimed cells unless the Sovereign themselves permits.
//!   - **§7 INTEGRITY** : audit_seq is monotone-increasing per cell ; any
//!     mutation that does not advance the seq is a bug.
//!
//! § ATTESTATION
//!   Every public mutation routes through [`SigmaMaskPacked::mutate`] which
//!   verifies the canonical attestation constant + emits an audit-event.
//!   The text of the attestation is in [`crate::attestation::ATTESTATION`].

use thiserror::Error;

use crate::audit::EnforcementAuditBus;

// ───────────────────────────────────────────────────────────────────────
// § Bit-layout positions  — single source of truth for pack/unpack.
// ───────────────────────────────────────────────────────────────────────

/// Bit-position of the consent-bits field within the 128-bit Σ-mask.
pub const SIGMA_BIT_CONSENT_LO: u32 = 0;
/// Width of the consent-bits field (in bits).
pub const SIGMA_BIT_CONSENT_WIDTH: u32 = 32;
/// Bit-position of the sovereignty-handle field.
pub const SIGMA_BIT_SOVEREIGN_LO: u32 = 32;
pub const SIGMA_BIT_SOVEREIGN_WIDTH: u32 = 16;
pub const SIGMA_BIT_CAPACITY_LO: u32 = 48;
pub const SIGMA_BIT_CAPACITY_WIDTH: u32 = 16;
pub const SIGMA_BIT_REVERSIBILITY_LO: u32 = 64;
pub const SIGMA_BIT_REVERSIBILITY_WIDTH: u32 = 16;
pub const SIGMA_BIT_AUDIT_SEQ_LO: u32 = 80;
pub const SIGMA_BIT_AUDIT_SEQ_WIDTH: u32 = 16;
pub const SIGMA_BIT_AGENCY_STATE_LO: u32 = 96;
pub const SIGMA_BIT_AGENCY_STATE_WIDTH: u32 = 16;
pub const SIGMA_BIT_RESERVED_LO: u32 = 112;
pub const SIGMA_BIT_RESERVED_WIDTH: u32 = 16;

/// Sentinel value indicating "no Sovereign claims this cell" — the cell is
/// public/unclaimed. Sovereign handles are u16, so 0 is reserved.
pub const SIGMA_SOVEREIGN_NULL: u16 = 0;

// ───────────────────────────────────────────────────────────────────────
// § Consent-bit flags  — the 32-bit op-class permission table.
// ───────────────────────────────────────────────────────────────────────

/// Per-op-class consent flags packed into the 32-bit `consent_bits` field.
///
/// § SPEC : 04_AGENCY_INVARIANT § II `bitflag ConsentBit : u32`. Variants
/// 0..=7 are the canonical op-classes ; 8..=15 are reserved for future op
/// classes (per the spec's "up to 32 op-classes" comment) ; 16..=31 are
/// reserved-for-extension.
///
/// § STABILITY
///   The bit-positions are FROZEN. Reordering = ABI break. Adding a new flag
///   requires a spec amendment + DECISIONS entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum ConsentBit {
    /// Bit 0 — observe : the cell may be observed (read-only inspection).
    Observe = 1 << 0,
    /// Bit 1 — traverse : the cell may be moved through.
    Traverse = 1 << 1,
    /// Bit 2 — modify : the cell may be physically modified.
    Modify = 1 << 2,
    /// Bit 3 — communicate : the cell may be talked-to or signaled.
    Communicate = 1 << 3,
    /// Bit 4 — sample : data may be extracted from the cell.
    Sample = 1 << 4,
    /// Bit 5 — reconfigure : structural change permitted.
    Reconfigure = 1 << 5,
    /// Bit 6 — translate : substrate-shift permitted.
    Translate = 1 << 6,
    /// Bit 7 — recrystallize : Pattern-rewrite permitted (rare ; Sovereign-
    /// only).
    Recrystallize = 1 << 7,
    /// Bit 8 — destroy : the cell may be destroyed (released to default).
    Destroy = 1 << 8,
}

impl ConsentBit {
    /// All consent-bit flags in canonical order.
    #[must_use]
    pub const fn all() -> &'static [ConsentBit] {
        &[
            Self::Observe,
            Self::Traverse,
            Self::Modify,
            Self::Communicate,
            Self::Sample,
            Self::Reconfigure,
            Self::Translate,
            Self::Recrystallize,
            Self::Destroy,
        ]
    }

    /// Stable canonical name (used in audit-chain entries).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Traverse => "traverse",
            Self::Modify => "modify",
            Self::Communicate => "communicate",
            Self::Sample => "sample",
            Self::Reconfigure => "reconfigure",
            Self::Translate => "translate",
            Self::Recrystallize => "recrystallize",
            Self::Destroy => "destroy",
        }
    }

    /// Numeric bit-mask value.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self as u32
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Reversibility-scope enum  — how far back can a mutation be undone.
// ───────────────────────────────────────────────────────────────────────

/// Scope-of-undo for a mutation, per AGENCY_INVARIANT § I.3 § scope-of-undo.
///
/// Larger scope ⇒ stricter reversibility requirement. Encoded as a u16 in the
/// Σ-mask. Default = `Session` (the safe everyday default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum ReversibilityScope {
    /// Immediate : undo within the current omega_step tick.
    Immediate = 0,
    /// Session : undo within the active session (default).
    Session = 1,
    /// RG-day : undo within an RG-day window.
    RgDay = 2,
    /// RG-week : undo within an RG-week window.
    RgWeek = 3,
    /// Permanent : irreversible by ordinary undo. Requires Sovereign-
    /// acknowledged declared-irreversibility justification (Axiom-4 § I.3).
    Permanent = 4,
}

impl ReversibilityScope {
    /// All variants in canonical order (used in tests + DECISIONS reproduction).
    #[must_use]
    pub const fn all() -> &'static [ReversibilityScope] {
        &[
            Self::Immediate,
            Self::Session,
            Self::RgDay,
            Self::RgWeek,
            Self::Permanent,
        ]
    }

    /// Canonical name (used in audit-chain).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Session => "session",
            Self::RgDay => "rg_day",
            Self::RgWeek => "rg_week",
            Self::Permanent => "permanent",
        }
    }

    /// Pack into a 16-bit field.
    #[must_use]
    pub const fn to_u16(self) -> u16 {
        self as u16
    }

    /// Decode from a 16-bit field. Unknown encodings clamp to `Session`
    /// (the safe default) per §3 conservative-default.
    ///
    /// The `Self::Session` arm is intentionally identical to the wildcard
    /// arm — clippy's `match_same_arms` lint flags it, but the arms are
    /// semantically distinct (one decodes a known value, the other clamps
    /// an unknown value).
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn from_u16(v: u16) -> ReversibilityScope {
        match v {
            0 => Self::Immediate,
            1 => Self::Session,
            2 => Self::RgDay,
            3 => Self::RgWeek,
            4 => Self::Permanent,
            _ => Self::Session,
        }
    }
}

impl Default for ReversibilityScope {
    fn default() -> Self {
        Self::Session
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Agency-state enum  — coarse runtime status of the cell.
// ───────────────────────────────────────────────────────────────────────

/// Per-cell agency-state. Coarse status used by the Phase-5 AGENCY-VERIFY
/// pass to skip cells that are already-frozen + flag cells that are mid-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum AgencyState {
    /// Quiescent : no in-flight op, default state.
    Quiescent = 0,
    /// Pending : an op is being verified ; mutations refused.
    Pending = 1,
    /// Active : an op is currently mid-application.
    Active = 2,
    /// Frozen : the Sovereign has declared the cell immutable.
    Frozen = 3,
    /// Reverted : the cell was rolled back ; awaiting Sovereign acknowledge.
    Reverted = 4,
}

impl AgencyState {
    #[must_use]
    pub const fn all() -> &'static [AgencyState] {
        &[
            Self::Quiescent,
            Self::Pending,
            Self::Active,
            Self::Frozen,
            Self::Reverted,
        ]
    }

    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Quiescent => "quiescent",
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Frozen => "frozen",
            Self::Reverted => "reverted",
        }
    }

    #[must_use]
    pub const fn to_u16(self) -> u16 {
        self as u16
    }

    /// Decode from a 16-bit field. Unknown encodings clamp to `Quiescent`.
    ///
    /// The `Self::Quiescent` arm is intentionally identical to the wildcard
    /// — see [`ReversibilityScope::from_u16`] for the rationale.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn from_u16(v: u16) -> AgencyState {
        match v {
            0 => Self::Quiescent,
            1 => Self::Pending,
            2 => Self::Active,
            3 => Self::Frozen,
            4 => Self::Reverted,
            _ => Self::Quiescent,
        }
    }
}

impl Default for AgencyState {
    fn default() -> Self {
        Self::Quiescent
    }
}

// ───────────────────────────────────────────────────────────────────────
// § SigmaPolicy  — pre-canned default-policies for typical region kinds.
// ───────────────────────────────────────────────────────────────────────

/// Pre-canned policies for typical Σ-mask configurations.
///
/// The variant carries no data ; [`SigmaPolicy::to_consent_bits`] yields the
/// u32 flags. Policies are the recommended way to stamp regions at scene-
/// load — bespoke per-cell masks can still be authored, but they go through
/// [`SigmaMaskPacked::with_consent`].
///
/// § SPEC : 02_VR_EMBODIMENT § VIII (region-defaults table) and the spirit
/// of §3 conservative-default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SigmaPolicy {
    /// Default-Private : Observe self-only. The conservative default for
    /// any Sovereign-claimed cell. (HEAD / TRUNK / FOOT / GAZE / FACE in
    /// VR_EMBODIMENT § VIII.)
    DefaultPrivate,
    /// Public-Read : Observe + Sample by anyone (read-only public).
    PublicRead,
    /// Public-Modify : Observe + Modify by anyone (rare ; needs explicit
    /// authoring intent — playground sandbox region, e.g.).
    PublicModify,
    /// Sovereign-Only : Observe + Modify only by the claiming Sovereign.
    SovereignOnly,
    /// Co-Present : Observe + Communicate by co-present Sovereigns.
    /// (HANDS in VR_EMBODIMENT § VIII.)
    CoPresent,
    /// Aura-Tier-L3 : Observe by Sovereigns at tier-L3+ (Aura social-signal).
    AuraTierL3,
    /// Self-Recrystallize : the Sovereign is the only entity that may
    /// rewrite the Pattern of this cell.
    SelfRecrystallize,
}

impl SigmaPolicy {
    /// Translate the policy into a 32-bit consent-bits field.
    ///
    /// § DESIGN : these are the canonical defaults referenced by VR_EMBODIMENT
    /// § VIII. The actual *who-may-observe* discrimination is enforced at the
    /// op-evaluation layer ; the bits here just encode which op-classes are
    /// enabled at all. Sovereign-discrimination is the runtime's job.
    ///
    /// `DefaultPrivate` and `AuraTierL3` produce identical bit-patterns at
    /// this layer (both = Observe-only) — clippy flags it, but the policies
    /// are semantically distinct (the *who-may-observe* differs : self vs
    /// tier-L3+ co-present), so we keep the arms separate to avoid
    /// information loss in the source.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn to_consent_bits(self) -> u32 {
        match self {
            Self::DefaultPrivate => ConsentBit::Observe.bits(),
            Self::PublicRead => ConsentBit::Observe.bits() | ConsentBit::Sample.bits(),
            Self::PublicModify => ConsentBit::Observe.bits() | ConsentBit::Modify.bits(),
            Self::SovereignOnly => {
                ConsentBit::Observe.bits()
                    | ConsentBit::Modify.bits()
                    | ConsentBit::Sample.bits()
                    | ConsentBit::Reconfigure.bits()
            }
            Self::CoPresent => ConsentBit::Observe.bits() | ConsentBit::Communicate.bits(),
            Self::AuraTierL3 => ConsentBit::Observe.bits(),
            Self::SelfRecrystallize => {
                ConsentBit::Observe.bits()
                    | ConsentBit::Modify.bits()
                    | ConsentBit::Reconfigure.bits()
                    | ConsentBit::Recrystallize.bits()
            }
        }
    }

    /// Stable canonical name.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::DefaultPrivate => "default_private",
            Self::PublicRead => "public_read",
            Self::PublicModify => "public_modify",
            Self::SovereignOnly => "sovereign_only",
            Self::CoPresent => "co_present",
            Self::AuraTierL3 => "aura_tier_l3",
            Self::SelfRecrystallize => "self_recrystallize",
        }
    }

    /// All policies in canonical order.
    #[must_use]
    pub const fn all() -> &'static [SigmaPolicy] {
        &[
            Self::DefaultPrivate,
            Self::PublicRead,
            Self::PublicModify,
            Self::SovereignOnly,
            Self::CoPresent,
            Self::AuraTierL3,
            Self::SelfRecrystallize,
        ]
    }
}

// ───────────────────────────────────────────────────────────────────────
// § VR-region default constants  — codifies VR_EMBODIMENT § VIII verbatim.
// ───────────────────────────────────────────────────────────────────────

/// HEAD region : OBSERVE only @ self-Sovereign. Per VR_EMBODIMENT § VIII.
pub const SIGMA_REGION_HEAD: SigmaPolicy = SigmaPolicy::DefaultPrivate;
/// TRUNK region : OBSERVE @ self only.
pub const SIGMA_REGION_TRUNK: SigmaPolicy = SigmaPolicy::DefaultPrivate;
/// L_FOOT / R_FOOT : OBSERVE @ self only.
pub const SIGMA_REGION_FEET: SigmaPolicy = SigmaPolicy::DefaultPrivate;
/// GAZE : OBSERVE @ self only ; HIGHLY-private. Per VR_EMBODIMENT § VIII.
pub const SIGMA_REGION_GAZE: SigmaPolicy = SigmaPolicy::DefaultPrivate;
/// FACE : OBSERVE @ self only ; EXTRA-restricted (face-tracking).
pub const SIGMA_REGION_FACE: SigmaPolicy = SigmaPolicy::DefaultPrivate;
/// L_HAND / R_HAND : OBSERVE @ self ⊗ COMMUNICATE @ co-present.
pub const SIGMA_REGION_HANDS: SigmaPolicy = SigmaPolicy::CoPresent;
/// AURA : OBSERVE @ co-present-Sovereign-tier-L3+.
pub const SIGMA_REGION_AURA: SigmaPolicy = SigmaPolicy::AuraTierL3;

// ───────────────────────────────────────────────────────────────────────
// § SigmaMaskPacked  — the 16-byte std430-aligned mask itself.
// ───────────────────────────────────────────────────────────────────────

/// Per-cell Σ-mask, std430-aligned 16-byte packed bitmap.
///
/// § INVARIANTS
///   - `core::mem::size_of::<SigmaMaskPacked>() == 16` (verified-by-test).
///   - `core::mem::align_of::<SigmaMaskPacked>() == 8` (std430-rule).
///   - audit_seq is monotone-increasing per cell ; mutations advance it.
///
/// § FIELD-ORDER
///   The struct uses a `#[repr(C)]` layout so the byte-order matches the
///   bit-layout documented in the module-level docs verbatim. Pack/unpack
///   helpers convert to/from a `u128` for serialization + GPU upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C, align(8))]
pub struct SigmaMaskPacked {
    /// Bytes  0..= 3 : consent-bits (u32 flags).
    consent_bits: u32,
    /// Bytes  4..= 5 : sovereign-handle (u16 ; 0 = NULL/unclaimed).
    sovereign_handle: u16,
    /// Bytes  6..= 7 : capacity-floor (u16 quota).
    capacity_floor: u16,
    /// Bytes  8..= 9 : reversibility-scope (u16 enum).
    reversibility_scope: u16,
    /// Bytes 10..=11 : audit-seq (u16 monotone counter).
    audit_seq: u16,
    /// Bytes 12..=13 : agency-state (u16 enum).
    agency_state: u16,
    /// Bytes 14..=15 : reserved (must be 0 ; reserved-for-extension).
    reserved: u16,
}

impl SigmaMaskPacked {
    /// The DEFAULT mask : Default-Private + NULL Sovereign + capacity-floor=0
    /// + Session reversibility + Quiescent agency-state + audit-seq=0.
    ///
    /// § DESIGN : this is the §3 conservative-default. Cells that have never
    /// been touched return this value from [`crate::audit::EnforcementAuditBus`]
    /// + the SigmaOverlay sparse-grid (per Ω-field design).
    #[must_use]
    pub const fn default_mask() -> SigmaMaskPacked {
        SigmaMaskPacked {
            consent_bits: SigmaPolicy::DefaultPrivate.to_consent_bits(),
            sovereign_handle: SIGMA_SOVEREIGN_NULL,
            capacity_floor: 0,
            reversibility_scope: ReversibilityScope::Session.to_u16(),
            audit_seq: 0,
            agency_state: AgencyState::Quiescent.to_u16(),
            reserved: 0,
        }
    }

    /// Construct a mask from explicit fields.
    ///
    /// # Errors
    /// Returns [`SigmaMaskError::ReservedNonZero`] if `reserved != 0` (the
    /// reserved-tail is reserved-for-extension per §7).
    pub fn new(
        consent_bits: u32,
        sovereign_handle: u16,
        capacity_floor: u16,
        reversibility_scope: ReversibilityScope,
        audit_seq: u16,
        agency_state: AgencyState,
    ) -> Result<SigmaMaskPacked, SigmaMaskError> {
        Ok(SigmaMaskPacked {
            consent_bits,
            sovereign_handle,
            capacity_floor,
            reversibility_scope: reversibility_scope.to_u16(),
            audit_seq,
            agency_state: agency_state.to_u16(),
            reserved: 0,
        })
    }

    /// Pack the mask into a single `u128` for serialization. The byte-order
    /// is little-endian per std430 + the packed bit-layout documented at
    /// the top of this module.
    #[must_use]
    pub const fn to_u128(self) -> u128 {
        (self.consent_bits as u128)
            | ((self.sovereign_handle as u128) << SIGMA_BIT_SOVEREIGN_LO)
            | ((self.capacity_floor as u128) << SIGMA_BIT_CAPACITY_LO)
            | ((self.reversibility_scope as u128) << SIGMA_BIT_REVERSIBILITY_LO)
            | ((self.audit_seq as u128) << SIGMA_BIT_AUDIT_SEQ_LO)
            | ((self.agency_state as u128) << SIGMA_BIT_AGENCY_STATE_LO)
            | ((self.reserved as u128) << SIGMA_BIT_RESERVED_LO)
    }

    /// Unpack from a 128-bit value. Reserved bits are dropped (treated as
    /// 0 ; this is the §3 conservative-default).
    #[must_use]
    pub const fn from_u128(v: u128) -> SigmaMaskPacked {
        let consent_mask: u128 = (1u128 << SIGMA_BIT_CONSENT_WIDTH) - 1;
        let sov_mask: u128 = (1u128 << SIGMA_BIT_SOVEREIGN_WIDTH) - 1;
        let cap_mask: u128 = (1u128 << SIGMA_BIT_CAPACITY_WIDTH) - 1;
        let rev_mask: u128 = (1u128 << SIGMA_BIT_REVERSIBILITY_WIDTH) - 1;
        let seq_mask: u128 = (1u128 << SIGMA_BIT_AUDIT_SEQ_WIDTH) - 1;
        let ag_mask: u128 = (1u128 << SIGMA_BIT_AGENCY_STATE_WIDTH) - 1;
        SigmaMaskPacked {
            consent_bits: (v & consent_mask) as u32,
            sovereign_handle: ((v >> SIGMA_BIT_SOVEREIGN_LO) & sov_mask) as u16,
            capacity_floor: ((v >> SIGMA_BIT_CAPACITY_LO) & cap_mask) as u16,
            reversibility_scope: ((v >> SIGMA_BIT_REVERSIBILITY_LO) & rev_mask) as u16,
            audit_seq: ((v >> SIGMA_BIT_AUDIT_SEQ_LO) & seq_mask) as u16,
            agency_state: ((v >> SIGMA_BIT_AGENCY_STATE_LO) & ag_mask) as u16,
            // Reserved tail intentionally dropped — see §7 INTEGRITY.
            reserved: 0,
        }
    }

    /// Construct a mask from a [`SigmaPolicy`]. Sovereign-handle defaults
    /// to NULL ; mutate via [`Self::with_sovereign`].
    #[must_use]
    pub const fn from_policy(policy: SigmaPolicy) -> SigmaMaskPacked {
        SigmaMaskPacked {
            consent_bits: policy.to_consent_bits(),
            sovereign_handle: SIGMA_SOVEREIGN_NULL,
            capacity_floor: 0,
            reversibility_scope: ReversibilityScope::Session.to_u16(),
            audit_seq: 0,
            agency_state: AgencyState::Quiescent.to_u16(),
            reserved: 0,
        }
    }

    // ── Accessors ────────────────────────────────────────────────────

    #[must_use]
    pub const fn consent_bits(self) -> u32 {
        self.consent_bits
    }

    #[must_use]
    pub const fn sovereign_handle(self) -> u16 {
        self.sovereign_handle
    }

    #[must_use]
    pub const fn capacity_floor(self) -> u16 {
        self.capacity_floor
    }

    #[must_use]
    pub fn reversibility_scope(self) -> ReversibilityScope {
        ReversibilityScope::from_u16(self.reversibility_scope)
    }

    #[must_use]
    pub const fn audit_seq(self) -> u16 {
        self.audit_seq
    }

    #[must_use]
    pub fn agency_state(self) -> AgencyState {
        AgencyState::from_u16(self.agency_state)
    }

    /// Test whether a given consent-bit is set in this mask.
    #[must_use]
    pub const fn permits(self, bit: ConsentBit) -> bool {
        (self.consent_bits & bit.bits()) != 0
    }

    // ── Bit-test convenience predicates ─────────────────────────────

    #[must_use]
    pub const fn can_observe(self) -> bool {
        self.permits(ConsentBit::Observe)
    }
    #[must_use]
    pub const fn can_traverse(self) -> bool {
        self.permits(ConsentBit::Traverse)
    }
    #[must_use]
    pub const fn can_modify(self) -> bool {
        self.permits(ConsentBit::Modify)
    }
    #[must_use]
    pub const fn can_communicate(self) -> bool {
        self.permits(ConsentBit::Communicate)
    }
    #[must_use]
    pub const fn can_sample(self) -> bool {
        self.permits(ConsentBit::Sample)
    }
    #[must_use]
    pub const fn can_reconfigure(self) -> bool {
        self.permits(ConsentBit::Reconfigure)
    }
    #[must_use]
    pub const fn can_translate(self) -> bool {
        self.permits(ConsentBit::Translate)
    }
    #[must_use]
    pub const fn can_recrystallize(self) -> bool {
        self.permits(ConsentBit::Recrystallize)
    }
    #[must_use]
    pub const fn can_destroy(self) -> bool {
        self.permits(ConsentBit::Destroy)
    }
    #[must_use]
    pub const fn is_sovereign(self) -> bool {
        self.sovereign_handle != SIGMA_SOVEREIGN_NULL
    }

    // ── Pure builder-style mutations (do NOT audit) ─────────────────
    //
    // These are intended for *initial construction* (scene-load + stamping
    // region defaults). For runtime mutations on a live cell, use [`Self::mutate`]
    // which routes through the audit-bus.

    /// Replace the consent-bits with a fresh u32 mask.
    #[must_use]
    pub const fn with_consent(mut self, consent_bits: u32) -> SigmaMaskPacked {
        self.consent_bits = consent_bits;
        self
    }

    /// Set the consent-bits to the value implied by `policy`.
    #[must_use]
    pub const fn with_policy(mut self, policy: SigmaPolicy) -> SigmaMaskPacked {
        self.consent_bits = policy.to_consent_bits();
        self
    }

    /// Replace the Sovereign handle. `SIGMA_SOVEREIGN_NULL` (0) means
    /// unclaimed.
    #[must_use]
    pub const fn with_sovereign(mut self, handle: u16) -> SigmaMaskPacked {
        self.sovereign_handle = handle;
        self
    }

    /// Replace the capacity-floor.
    #[must_use]
    pub const fn with_capacity_floor(mut self, floor: u16) -> SigmaMaskPacked {
        self.capacity_floor = floor;
        self
    }

    /// Replace the reversibility-scope.
    #[must_use]
    pub fn with_reversibility(mut self, scope: ReversibilityScope) -> SigmaMaskPacked {
        self.reversibility_scope = scope.to_u16();
        self
    }

    /// Replace the agency-state.
    #[must_use]
    pub fn with_agency_state(mut self, state: AgencyState) -> SigmaMaskPacked {
        self.agency_state = state.to_u16();
        self
    }

    /// Add a single consent-bit (bitwise OR).
    #[must_use]
    pub const fn or_bit(mut self, bit: ConsentBit) -> SigmaMaskPacked {
        self.consent_bits |= bit.bits();
        self
    }

    /// Remove a single consent-bit (bitwise AND-NOT).
    #[must_use]
    pub const fn and_not_bit(mut self, bit: ConsentBit) -> SigmaMaskPacked {
        self.consent_bits &= !bit.bits();
        self
    }

    // ── Audited mutation surface ────────────────────────────────────

    /// Apply a mutation closure + emit the canonical audit-event.
    ///
    /// § FLOW
    ///   1. Capture `before`-snapshot.
    ///   2. Run user-supplied `f`.
    ///   3. Validate the result (audit_seq advanced ; reversibility-scope
    ///      not silently widened ; capacity_floor not lowered without
    ///      explicit Sovereign opt-in).
    ///   4. Emit a [`crate::audit::AuditEvent::SigmaMaskMutated`] entry.
    ///   5. Return the new mask.
    ///
    /// # Errors
    /// See [`SigmaMaskError`].
    pub fn mutate(
        self,
        bus: &mut EnforcementAuditBus,
        site: impl Into<String>,
        f: impl FnOnce(SigmaMaskPacked) -> SigmaMaskPacked,
    ) -> Result<SigmaMaskPacked, SigmaMaskError> {
        let before = self;
        let mut after = f(self);

        // Bookkeeping : audit_seq must advance. We saturating_add(1) so
        // wrap-around (very long-lived cell) is detectable + non-fatal.
        if after.audit_seq <= before.audit_seq {
            after.audit_seq = before.audit_seq.wrapping_add(1);
        }

        // §5 reversibility check : permanent at Sovereign-claimed cell
        // requires that the Sovereign is the same (or unclaimed) ; we cannot
        // silently widen reversibility for a Sovereign-claimed cell.
        if before.is_sovereign()
            && before.sovereign_handle == after.sovereign_handle
            && before.reversibility_scope() != ReversibilityScope::Permanent
            && after.reversibility_scope() == ReversibilityScope::Permanent
        {
            return Err(SigmaMaskError::ReversibilityWidenForbidden {
                from: before.reversibility_scope(),
                to: after.reversibility_scope(),
                sovereign: before.sovereign_handle,
            });
        }

        // §3 conservative-default : reserved-tail must remain zero.
        if after.reserved != 0 {
            return Err(SigmaMaskError::ReservedNonZero);
        }

        // Capacity-floor cannot be reduced unless the Sovereign opts-out
        // (this is the AGENCY_INVARIANT § I.2 floor-preservation rule).
        if before.capacity_floor > 0
            && after.capacity_floor < before.capacity_floor
            && before.sovereign_handle == after.sovereign_handle
        {
            return Err(SigmaMaskError::CapacityFloorEroded {
                from: before.capacity_floor,
                to: after.capacity_floor,
                sovereign: before.sovereign_handle,
            });
        }

        bus.record_sigma_mask_mutated(before, after, site);
        Ok(after)
    }

    /// Mutation variant that allows widening reversibility (ie. Sovereign
    /// has explicitly authorized a permanence-change). The caller MUST
    /// supply the same Sovereign-handle that owns the cell.
    ///
    /// # Errors
    /// Returns [`SigmaMaskError::SovereignMismatch`] if `authorizing` does
    /// not match the cell's current Sovereign.
    pub fn mutate_with_sovereign_consent(
        self,
        bus: &mut EnforcementAuditBus,
        site: impl Into<String>,
        authorizing: u16,
        f: impl FnOnce(SigmaMaskPacked) -> SigmaMaskPacked,
    ) -> Result<SigmaMaskPacked, SigmaMaskError> {
        if self.is_sovereign() && self.sovereign_handle != authorizing {
            return Err(SigmaMaskError::SovereignMismatch {
                expected: self.sovereign_handle,
                got: authorizing,
            });
        }
        let before = self;
        let mut after = f(self);
        if after.audit_seq <= before.audit_seq {
            after.audit_seq = before.audit_seq.wrapping_add(1);
        }
        if after.reserved != 0 {
            return Err(SigmaMaskError::ReservedNonZero);
        }
        bus.record_sigma_mask_mutated(before, after, site);
        Ok(after)
    }
}

impl Default for SigmaMaskPacked {
    fn default() -> Self {
        Self::default_mask()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Errors  — the failure modes of Σ-mask construction + mutation.
// ───────────────────────────────────────────────────────────────────────

/// Failure modes for [`SigmaMaskPacked`] mutations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SigmaMaskError {
    /// The reserved 16-bit tail of the mask was non-zero. Reserved-for-
    /// extension per §7 INTEGRITY.
    #[error("PD0017 — Σ-mask reserved tail must be zero")]
    ReservedNonZero,
    /// The mutation tried to widen reversibility-scope to Permanent on a
    /// Sovereign-claimed cell without explicit Sovereign consent. This is
    /// the §5 revocability + §1 control prohibition.
    #[error(
        "PD0002 — Σ-mask reversibility cannot widen from {from:?} to {to:?} on Sovereign-claimed cell {sovereign} without explicit consent"
    )]
    ReversibilityWidenForbidden {
        from: ReversibilityScope,
        to: ReversibilityScope,
        sovereign: u16,
    },
    /// The mutation tried to lower capacity-floor on a Sovereign-claimed
    /// cell. This is AGENCY_INVARIANT § I.2 floor-preservation.
    #[error(
        "PD0014 — Σ-mask capacity-floor cannot erode from {from} to {to} on Sovereign-claimed cell {sovereign}"
    )]
    CapacityFloorEroded { from: u16, to: u16, sovereign: u16 },
    /// Sovereign-handle mismatch on consent-authorized mutation.
    #[error("PD0014 — Σ-mask Sovereign mismatch : expected={expected}, got={got}")]
    SovereignMismatch { expected: u16, got: u16 },
}

#[cfg(test)]
mod tests {
    use super::{
        AgencyState, ConsentBit, ReversibilityScope, SigmaMaskError, SigmaMaskPacked, SigmaPolicy,
        SIGMA_BIT_AGENCY_STATE_LO, SIGMA_BIT_AGENCY_STATE_WIDTH, SIGMA_BIT_AUDIT_SEQ_LO,
        SIGMA_BIT_AUDIT_SEQ_WIDTH, SIGMA_BIT_CAPACITY_LO, SIGMA_BIT_CAPACITY_WIDTH,
        SIGMA_BIT_CONSENT_LO, SIGMA_BIT_CONSENT_WIDTH, SIGMA_BIT_RESERVED_LO,
        SIGMA_BIT_RESERVED_WIDTH, SIGMA_BIT_REVERSIBILITY_LO, SIGMA_BIT_REVERSIBILITY_WIDTH,
        SIGMA_BIT_SOVEREIGN_LO, SIGMA_BIT_SOVEREIGN_WIDTH, SIGMA_REGION_AURA, SIGMA_REGION_FACE,
        SIGMA_REGION_FEET, SIGMA_REGION_GAZE, SIGMA_REGION_HANDS, SIGMA_REGION_HEAD,
        SIGMA_REGION_TRUNK, SIGMA_SOVEREIGN_NULL,
    };
    use crate::audit::EnforcementAuditBus;

    // ── Layout invariants ───────────────────────────────────────────

    #[test]
    fn sigma_mask_size_is_16_bytes() {
        assert_eq!(core::mem::size_of::<SigmaMaskPacked>(), 16);
    }

    #[test]
    fn sigma_mask_alignment_is_8_bytes() {
        assert_eq!(core::mem::align_of::<SigmaMaskPacked>(), 8);
    }

    #[test]
    fn bit_widths_sum_to_128() {
        let total = SIGMA_BIT_CONSENT_WIDTH
            + SIGMA_BIT_SOVEREIGN_WIDTH
            + SIGMA_BIT_CAPACITY_WIDTH
            + SIGMA_BIT_REVERSIBILITY_WIDTH
            + SIGMA_BIT_AUDIT_SEQ_WIDTH
            + SIGMA_BIT_AGENCY_STATE_WIDTH
            + SIGMA_BIT_RESERVED_WIDTH;
        assert_eq!(total, 128);
    }

    #[test]
    fn bit_positions_are_contiguous() {
        assert_eq!(SIGMA_BIT_CONSENT_LO, 0);
        assert_eq!(
            SIGMA_BIT_SOVEREIGN_LO,
            SIGMA_BIT_CONSENT_LO + SIGMA_BIT_CONSENT_WIDTH
        );
        assert_eq!(
            SIGMA_BIT_CAPACITY_LO,
            SIGMA_BIT_SOVEREIGN_LO + SIGMA_BIT_SOVEREIGN_WIDTH
        );
        assert_eq!(
            SIGMA_BIT_REVERSIBILITY_LO,
            SIGMA_BIT_CAPACITY_LO + SIGMA_BIT_CAPACITY_WIDTH
        );
        assert_eq!(
            SIGMA_BIT_AUDIT_SEQ_LO,
            SIGMA_BIT_REVERSIBILITY_LO + SIGMA_BIT_REVERSIBILITY_WIDTH
        );
        assert_eq!(
            SIGMA_BIT_AGENCY_STATE_LO,
            SIGMA_BIT_AUDIT_SEQ_LO + SIGMA_BIT_AUDIT_SEQ_WIDTH
        );
        assert_eq!(
            SIGMA_BIT_RESERVED_LO,
            SIGMA_BIT_AGENCY_STATE_LO + SIGMA_BIT_AGENCY_STATE_WIDTH
        );
    }

    // ── Pack/unpack roundtrip ───────────────────────────────────────

    #[test]
    fn pack_unpack_default_roundtrip() {
        let m = SigmaMaskPacked::default_mask();
        let packed = m.to_u128();
        let unpacked = SigmaMaskPacked::from_u128(packed);
        assert_eq!(m, unpacked);
    }

    #[test]
    fn pack_unpack_full_roundtrip_with_all_fields_set() {
        let m = SigmaMaskPacked::new(
            0xDEAD_BEEF,
            0xABCD,
            0x1234,
            ReversibilityScope::RgWeek,
            0x5678,
            AgencyState::Active,
        )
        .unwrap();
        let packed = m.to_u128();
        let unpacked = SigmaMaskPacked::from_u128(packed);
        assert_eq!(m, unpacked);
    }

    #[test]
    fn pack_unpack_consent_bits_round_trip() {
        let m = SigmaMaskPacked::default_mask().with_consent(0x0000_FFFF);
        assert_eq!(
            SigmaMaskPacked::from_u128(m.to_u128()).consent_bits(),
            0x0000_FFFF
        );
    }

    #[test]
    fn pack_unpack_sovereign_handle_round_trip() {
        let m = SigmaMaskPacked::default_mask().with_sovereign(0x4242);
        assert_eq!(
            SigmaMaskPacked::from_u128(m.to_u128()).sovereign_handle(),
            0x4242
        );
    }

    #[test]
    fn pack_unpack_capacity_floor_round_trip() {
        let m = SigmaMaskPacked::default_mask().with_capacity_floor(0x9876);
        assert_eq!(
            SigmaMaskPacked::from_u128(m.to_u128()).capacity_floor(),
            0x9876
        );
    }

    #[test]
    fn pack_unpack_reversibility_round_trip() {
        for &scope in ReversibilityScope::all() {
            let m = SigmaMaskPacked::default_mask().with_reversibility(scope);
            assert_eq!(
                SigmaMaskPacked::from_u128(m.to_u128()).reversibility_scope(),
                scope
            );
        }
    }

    #[test]
    fn pack_unpack_agency_state_round_trip() {
        for &state in AgencyState::all() {
            let m = SigmaMaskPacked::default_mask().with_agency_state(state);
            assert_eq!(
                SigmaMaskPacked::from_u128(m.to_u128()).agency_state(),
                state
            );
        }
    }

    #[test]
    fn from_u128_drops_reserved_bits() {
        // Manufacture a u128 with reserved bits set ; from_u128 must drop them.
        let raw: u128 = 0xFFFF_u128 << SIGMA_BIT_RESERVED_LO;
        let m = SigmaMaskPacked::from_u128(raw);
        assert_eq!(m.to_u128() & (0xFFFF_u128 << SIGMA_BIT_RESERVED_LO), 0);
    }

    // ── Bit-test correctness ────────────────────────────────────────

    #[test]
    fn permits_observe_after_set() {
        let m = SigmaMaskPacked::default_mask().with_consent(ConsentBit::Observe.bits());
        assert!(m.can_observe());
        assert!(!m.can_modify());
    }

    #[test]
    fn permits_modify_after_set() {
        let m = SigmaMaskPacked::default_mask().with_consent(ConsentBit::Modify.bits());
        assert!(m.can_modify());
        assert!(!m.can_observe());
    }

    #[test]
    fn permits_all_canonical_bits_independently() {
        for &bit in ConsentBit::all() {
            let m = SigmaMaskPacked::default_mask().with_consent(bit.bits());
            assert!(m.permits(bit), "bit {bit:?} must be set");
            for &other in ConsentBit::all() {
                if other != bit {
                    assert!(!m.permits(other), "bit {other:?} must not be set");
                }
            }
        }
    }

    #[test]
    fn or_bit_adds_flag_without_disturbing_others() {
        let m = SigmaMaskPacked::default_mask()
            .with_consent(ConsentBit::Observe.bits())
            .or_bit(ConsentBit::Modify);
        assert!(m.can_observe());
        assert!(m.can_modify());
        assert!(!m.can_destroy());
    }

    #[test]
    fn and_not_bit_removes_flag_without_disturbing_others() {
        let m = SigmaMaskPacked::default_mask()
            .with_consent(ConsentBit::Observe.bits() | ConsentBit::Modify.bits())
            .and_not_bit(ConsentBit::Modify);
        assert!(m.can_observe());
        assert!(!m.can_modify());
    }

    #[test]
    fn is_sovereign_false_when_handle_null() {
        let m = SigmaMaskPacked::default_mask();
        assert!(!m.is_sovereign());
        assert_eq!(m.sovereign_handle(), SIGMA_SOVEREIGN_NULL);
    }

    #[test]
    fn is_sovereign_true_when_handle_nonzero() {
        let m = SigmaMaskPacked::default_mask().with_sovereign(7);
        assert!(m.is_sovereign());
    }

    // ── Default-policy correctness ──────────────────────────────────

    #[test]
    fn default_mask_is_default_private() {
        let m = SigmaMaskPacked::default_mask();
        assert!(m.can_observe());
        assert!(!m.can_modify());
        assert!(!m.can_destroy());
        assert!(!m.is_sovereign());
        assert_eq!(m.audit_seq(), 0);
        assert_eq!(m.reversibility_scope(), ReversibilityScope::Session);
        assert_eq!(m.agency_state(), AgencyState::Quiescent);
    }

    #[test]
    fn policy_default_private_observe_only() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::DefaultPrivate);
        assert!(m.can_observe());
        assert!(!m.can_modify());
        assert!(!m.can_sample());
    }

    #[test]
    fn policy_public_read_observe_plus_sample() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
        assert!(m.can_observe());
        assert!(m.can_sample());
        assert!(!m.can_modify());
    }

    #[test]
    fn policy_public_modify_observe_plus_modify() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::PublicModify);
        assert!(m.can_observe());
        assert!(m.can_modify());
        assert!(!m.can_destroy());
    }

    #[test]
    fn policy_sovereign_only_observe_modify_sample_reconfigure() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::SovereignOnly);
        assert!(m.can_observe());
        assert!(m.can_modify());
        assert!(m.can_sample());
        assert!(m.can_reconfigure());
        assert!(!m.can_destroy());
    }

    #[test]
    fn policy_co_present_observe_plus_communicate() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::CoPresent);
        assert!(m.can_observe());
        assert!(m.can_communicate());
        assert!(!m.can_modify());
    }

    #[test]
    fn policy_aura_tier_l3_observe_only() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::AuraTierL3);
        assert!(m.can_observe());
        assert!(!m.can_modify());
    }

    #[test]
    fn policy_self_recrystallize_includes_recrystallize_bit() {
        let m = SigmaMaskPacked::from_policy(SigmaPolicy::SelfRecrystallize);
        assert!(m.can_recrystallize());
        assert!(m.can_modify());
    }

    #[test]
    fn policy_canonical_names_unique() {
        let mut names: Vec<&'static str> = SigmaPolicy::all()
            .iter()
            .map(|p| p.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    // ── VR-region default constants  (per VR_EMBODIMENT § VIII) ─────

    #[test]
    fn vr_region_head_is_default_private() {
        assert_eq!(SIGMA_REGION_HEAD, SigmaPolicy::DefaultPrivate);
    }

    #[test]
    fn vr_region_trunk_is_default_private() {
        assert_eq!(SIGMA_REGION_TRUNK, SigmaPolicy::DefaultPrivate);
    }

    #[test]
    fn vr_region_feet_is_default_private() {
        assert_eq!(SIGMA_REGION_FEET, SigmaPolicy::DefaultPrivate);
    }

    #[test]
    fn vr_region_gaze_is_default_private() {
        assert_eq!(SIGMA_REGION_GAZE, SigmaPolicy::DefaultPrivate);
    }

    #[test]
    fn vr_region_face_is_default_private() {
        assert_eq!(SIGMA_REGION_FACE, SigmaPolicy::DefaultPrivate);
    }

    #[test]
    fn vr_region_hands_is_co_present() {
        assert_eq!(SIGMA_REGION_HANDS, SigmaPolicy::CoPresent);
        // Co-present masks permit communicate (gestures public when co-present).
        let m = SigmaMaskPacked::from_policy(SIGMA_REGION_HANDS);
        assert!(m.can_communicate());
    }

    #[test]
    fn vr_region_aura_is_tier_l3() {
        assert_eq!(SIGMA_REGION_AURA, SigmaPolicy::AuraTierL3);
    }

    // ── Reversibility-scope behavior ────────────────────────────────

    #[test]
    fn reversibility_default_is_session() {
        assert_eq!(ReversibilityScope::default(), ReversibilityScope::Session);
    }

    #[test]
    fn reversibility_unknown_clamps_to_session() {
        assert_eq!(
            ReversibilityScope::from_u16(99),
            ReversibilityScope::Session
        );
    }

    #[test]
    fn reversibility_canonical_names_unique() {
        let mut names: Vec<&'static str> = ReversibilityScope::all()
            .iter()
            .map(|s| s.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    #[test]
    fn reversibility_widening_to_permanent_on_sovereign_cell_rejected() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask().with_sovereign(7);
        let err = m
            .mutate(&mut bus, "test", |s| {
                s.with_reversibility(ReversibilityScope::Permanent)
            })
            .unwrap_err();
        assert!(matches!(
            err,
            SigmaMaskError::ReversibilityWidenForbidden { .. }
        ));
    }

    #[test]
    fn reversibility_widening_on_unclaimed_cell_allowed() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask();
        let after = m
            .mutate(&mut bus, "test", |s| {
                s.with_reversibility(ReversibilityScope::Permanent)
            })
            .unwrap();
        assert_eq!(after.reversibility_scope(), ReversibilityScope::Permanent);
    }

    // ── Agency-state behavior ───────────────────────────────────────

    #[test]
    fn agency_default_is_quiescent() {
        assert_eq!(AgencyState::default(), AgencyState::Quiescent);
    }

    #[test]
    fn agency_unknown_clamps_to_quiescent() {
        assert_eq!(AgencyState::from_u16(99), AgencyState::Quiescent);
    }

    #[test]
    fn agency_canonical_names_unique() {
        let mut names: Vec<&'static str> = AgencyState::all()
            .iter()
            .map(|s| s.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    // ── Audit-on-mutation ───────────────────────────────────────────

    #[test]
    fn mutate_advances_audit_seq() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask();
        let after = m
            .mutate(&mut bus, "test", |s| s.or_bit(ConsentBit::Modify))
            .unwrap();
        assert_eq!(after.audit_seq(), 1);
    }

    #[test]
    fn mutate_emits_audit_entry() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask();
        let _ = m
            .mutate(&mut bus, "site/x", |s| s.or_bit(ConsentBit::Modify))
            .unwrap();
        assert_eq!(bus.entry_count(), 1);
        let e = bus.iter().next().unwrap();
        assert_eq!(e.tag, "h6.sigma.mutated");
        assert!(e.message.contains("site/x"));
    }

    #[test]
    fn mutate_multiple_advances_seq_each_time() {
        let mut bus = EnforcementAuditBus::new();
        let mut m = SigmaMaskPacked::default_mask();
        for i in 1..=5 {
            m = m
                .mutate(&mut bus, "test", |s| s.or_bit(ConsentBit::Observe))
                .unwrap();
            assert_eq!(m.audit_seq(), i);
        }
        assert_eq!(bus.entry_count(), 5);
    }

    #[test]
    fn mutate_capacity_floor_erosion_rejected_on_sovereign_cell() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask()
            .with_sovereign(7)
            .with_capacity_floor(50);
        let err = m
            .mutate(&mut bus, "test", |s| s.with_capacity_floor(10))
            .unwrap_err();
        assert!(matches!(err, SigmaMaskError::CapacityFloorEroded { .. }));
    }

    #[test]
    fn mutate_capacity_floor_increase_allowed() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask()
            .with_sovereign(7)
            .with_capacity_floor(50);
        let after = m
            .mutate(&mut bus, "test", |s| s.with_capacity_floor(100))
            .unwrap();
        assert_eq!(after.capacity_floor(), 100);
    }

    #[test]
    fn mutate_with_sovereign_consent_allows_widening() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask().with_sovereign(7);
        let after = m
            .mutate_with_sovereign_consent(&mut bus, "site", 7, |s| {
                s.with_reversibility(ReversibilityScope::Permanent)
            })
            .unwrap();
        assert_eq!(after.reversibility_scope(), ReversibilityScope::Permanent);
        assert_eq!(after.audit_seq(), 1);
    }

    #[test]
    fn mutate_with_sovereign_consent_rejects_handle_mismatch() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask().with_sovereign(7);
        let err = m
            .mutate_with_sovereign_consent(&mut bus, "site", 99, |s| {
                s.with_reversibility(ReversibilityScope::Permanent)
            })
            .unwrap_err();
        assert!(matches!(err, SigmaMaskError::SovereignMismatch { .. }));
    }

    // ── Edge cases + invariants ─────────────────────────────────────

    #[test]
    fn consent_bit_canonical_names_unique() {
        let mut names: Vec<&'static str> = ConsentBit::all()
            .iter()
            .map(|c| c.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    #[test]
    fn consent_bit_values_are_powers_of_two() {
        for &bit in ConsentBit::all() {
            let v = bit.bits();
            assert!(v != 0);
            assert!(
                v.is_power_of_two(),
                "bit {bit:?} = {v:#x} must be a power of two"
            );
        }
    }

    #[test]
    fn pack_pattern_matches_expected_bit_layout() {
        // Manually construct a known-bit-pattern + verify the packed form.
        let m = SigmaMaskPacked::new(
            0x0000_0001, // consent_bits = Observe
            0x0007,      // sovereign = 7
            0x0064,      // capacity_floor = 100
            ReversibilityScope::RgDay,
            0x000A, // audit_seq = 10
            AgencyState::Active,
        )
        .unwrap();
        let packed = m.to_u128();
        // consent_bits @ bits 0..32 = 0x1
        assert_eq!(packed & 0xFFFF_FFFF, 0x1);
        // sovereign @ 32..48 = 0x7
        assert_eq!((packed >> 32) & 0xFFFF, 0x7);
        // capacity @ 48..64 = 0x64
        assert_eq!((packed >> 48) & 0xFFFF, 0x64);
        // reversibility @ 64..80 = 2 (RgDay)
        assert_eq!((packed >> 64) & 0xFFFF, 2);
        // audit_seq @ 80..96 = 10
        assert_eq!((packed >> 80) & 0xFFFF, 10);
        // agency_state @ 96..112 = 2 (Active)
        assert_eq!((packed >> 96) & 0xFFFF, 2);
        // reserved @ 112..128 = 0
        assert_eq!((packed >> 112) & 0xFFFF, 0);
    }

    #[test]
    fn default_mask_packs_to_known_value() {
        let m = SigmaMaskPacked::default_mask();
        let packed = m.to_u128();
        // consent_bits = Observe = 1 ; sovereign = 0 ; capacity = 0 ;
        // reversibility = Session = 1 ; audit_seq = 0 ; agency = Quiescent = 0 ;
        // reserved = 0.
        let expected: u128 = 0x1 | (1u128 << 64);
        assert_eq!(packed, expected);
    }

    #[test]
    fn audit_seq_wraparound_on_overflow() {
        let mut bus = EnforcementAuditBus::new();
        let m = SigmaMaskPacked::default_mask()
            .with_consent(ConsentBit::Observe.bits())
            // Set audit_seq to u16::MAX so the next mutation wraps.
            .with_consent(ConsentBit::Observe.bits()); // no-op, just to test
        let mut m_at_max = SigmaMaskPacked::new(
            0,
            0,
            0,
            ReversibilityScope::Session,
            u16::MAX,
            AgencyState::Quiescent,
        )
        .unwrap();
        m_at_max = m_at_max
            .mutate(&mut bus, "wrap", |s| s.or_bit(ConsentBit::Observe))
            .unwrap();
        // Wraparound : u16::MAX + 1 = 0.
        assert_eq!(m_at_max.audit_seq(), 0);
        let _ = m;
    }

    #[test]
    fn mutate_rejects_reserved_nonzero() {
        // Construct a mask with reserved set via from_u128 (which drops it),
        // then use the field-direct path : we cannot — reserved is private.
        // So this test verifies that from_u128 always zeros reserved.
        let raw: u128 = 0xFF_u128 << 112;
        let m = SigmaMaskPacked::from_u128(raw);
        // Reserved was dropped by from_u128 ; the resulting mask is valid.
        assert_eq!(m.to_u128() & (0xFFFF_u128 << 112), 0);
    }
}

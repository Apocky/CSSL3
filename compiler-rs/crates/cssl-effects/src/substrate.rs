//! Substrate effect-rows + composition discipline.
//!
//! § SPEC :
//!   - `specs/30_SUBSTRATE.csl § EFFECT-ROWS` (S8-H0 / T11-D79) — canonical effect-list
//!     and forbidden-composition table.
//!   - `specs/04_EFFECTS.csl § SUBSTRATE-EFFECT-ROWS` (S8-H4 / T11-D92) — extends the
//!     existing `{IO}` effect-pattern (S6-B5 / T11-D76) to the Substrate effect set.
//!   - `PRIME_DIRECTIVE.md` § 1 PROHIBITIONS + § 5 CONSENT-ARCHITECTURE — the
//!     forbidden-composition table is the structural encoding of the Substrate's
//!     consent + harm-prevention discipline.
//!
//! § THESIS
//!   The Substrate (engine-plumbing layer between cssl-rt + LoA, per `specs/30`) needs
//!   a small **fixed** vocabulary of effect-row labels covering its six concerns :
//!
//!   ```text
//!     Render    GPU-render-graph context (omega_step phase-7..8)
//!     Sim       simulation-tick context (phase-4)
//!     Audio     audio-DSP callback (phase-6)
//!     Net       network IO (phase-2 + phase-11)
//!     Save      save-journal-append (phase-12)
//!     Telemetry observability ring (phase-9)
//!   ```
//!
//!   Each can be present-or-absent in a fn's effect-row independently. The
//!   composition-table (below) encodes which pairs / combinations are **legal**
//!   versus which are **compile-errors** versus which **require a `caps_grant(...)`**
//!   token. The encoding is structural — a program that violates the table is
//!   rejected at type-check time.
//!
//! § SHAPE — STAGE-0 vs FULL
//!   Stage-0 surface (this module) :
//!     - [`SubstrateEffect`] — dense enum of the six labels.
//!     - [`SubstrateEffectRow`] — small bit-set over `SubstrateEffect`. Ordered
//!       insertion + iteration ; no allocation ; no row-polymorphism (open-rows
//!       live one layer up in `cssl-hir`).
//!     - [`try_compose`] — combine two rows, returning either the unioned row or
//!       a [`ConflictReason`] with a stable diagnostic code in
//!       [`EFR0001`..`EFR0010`].
//!     - [`RowContext`] — caller-context bits used by the composition table
//!       (e.g., `Sim ⊎ Net` legality depends on whether the caller already
//!       holds `caps_grant(net_send_state)`, which the HIR layer materializes
//!       as a privilege-token). Builder API : `RowContext::with_caps_grant_net_send_state` /
//!       `with_pure_det` / `with_audit_companion` / `with_kernel_privilege`.
//!
//!   Full surface (deferred to T11-D9X) :
//!     - HIR + MIR threading of `(effect_row, "{Sim, Render}")` per-fn attribute.
//!       The IO marker pattern from S6-B5 (`(io_effect, "true")` per-op) is the
//!       precedent ; the Substrate effect-row attribute is per-fn structural.
//!     - Const-evaluation of caps-grant tokens (e.g., is the caller actually
//!       inside an `unsafe_caps_grant(net_send_state) { ... }` block?). Stage-0
//!       takes the `RowContext::has_caps_grant` flag at face value.
//!     - User-defined Substrate effects beyond the six. The dispatch plan
//!       lists this as a future-axis (`{Mod}` for modding-sandbox, `{VR}` for
//!       VR-projections, etc.).
//!
//! § STABILITY (per § 3 escalation #4 stable-block convention)
//!   The diagnostic codes [`EFR0001`..`EFR0010`] are allocated as a single block
//!   in this slice (T11-D92). Reordering or repurposing any of them is a
//!   major-version-bump event ; new diagnostic codes go in the next-block
//!   `EFR0011`+ in a future slice. The [`SubstrateEffect`] enum order is **also**
//!   stable — the discriminant values feed into the [`SubstrateEffectRow`]
//!   bit-set encoding ; reordering would silently corrupt existing rows.
//!
//! § PRIME-DIRECTIVE STRUCTURAL ENCODING
//!   The forbidden-composition table is **not** a runtime policy. It is a
//!   compile-time property of the type system. No `cfg`, no env-var, no
//!   command-line flag, no runtime condition can disable it. This mirrors
//!   the existing `banned_composition` checker in `banned.rs` (which encodes
//!   `Sensitive<>` × `IO` combinations) — `try_compose` is the same pattern
//!   applied to the Substrate axis.
//!
//!   Specifically :
//!     - `Net ⊎ Sim` without `caps_grant(net_send_state)` ⇒ `EFR0001`
//!       rationale : un-gated Sim → Net composition could exfiltrate game-state
//!       (consent-violation per PRIME_DIRECTIVE § 5).
//!     - `PureDet ⊎ Render` ⇒ `EFR0002` (rendering touches output devices ;
//!       not pure-deterministic).
//!     - `Save ⊎ anything` requires audit ⇒ recorded as `EFR0003` (advisory ;
//!       not a hard error — Save merely *requires* an `Audit<>` companion).
//!     - The other codes cover the remaining table cells (see below).
//!
//!   The `Telemetry` effect is **universal-additive** : it composes with
//!   anything (per `specs/30 § COMPOSITION-RULES` — "Telemetry participates
//!   here ; no-changes"). This reflects the design that telemetry is observation,
//!   not action ; observation never violates consent provided the
//!   `ConsentToken<"telemetry-egress">` is held when telemetry-egress is
//!   triggered (which is enforced one layer up).
//!
//! § REFERENCES
//!   - `specs/30_SUBSTRATE.csl` § EFFECT-ROWS § COMPOSITION-RULES + § FORBIDDEN-COMPOSITIONS
//!   - `specs/30_SUBSTRATE.csl` § OMEGA-STEP § PHASES (where each effect occurs)
//!   - `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING (Sensitive-domain interaction)
//!   - `specs/04_EFFECTS.csl § IO-EFFECT` (S6-B5 — the precedent for fn-level
//!     effect-row attribute pattern)
//!   - `PRIME_DIRECTIVE.md` § 0 AXIOM + § 1 PROHIBITIONS + § 5 CONSENT-ARCH

// `RowContext` carries four independent gating booleans that map 1:1 to the
// caller-context dimensions named by the spec ; refactoring into a state-machine
// or two-variant enums would obscure the spec → impl correspondence rather than
// clarify it. `try_compose` / `compose_with_advisories` take `&SubstrateEffectRow`
// + `&RowContext` to match the existing `EffectRef<'_>` convention in
// `discipline.rs` (which clippy also exempts) and to leave room for future
// field-expansion of these structs without API churn at every call-site.
#![allow(
    clippy::struct_excessive_bools,
    clippy::trivially_copy_pass_by_ref,
    clippy::redundant_closure_for_method_calls
)]

use std::fmt;

use thiserror::Error;

// ─ SubstrateEffect ───────────────────────────────────────────────────────────

/// One of the six canonical Substrate effect labels.
///
/// § STABLE-ORDER — the discriminant values feed the [`SubstrateEffectRow`]
/// bit-set encoding. Do not reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SubstrateEffect {
    /// GPU-render-graph context. Implies `{GPU, Region<'frame>}` at the BuiltinEffect
    /// layer (see `specs/30 § EFFECT-ROWS § SUBSTRATE-EFFECTS`).
    Render = 0,
    /// Simulation-tick context. Implies `{DetRNG, Reversible}` at the BuiltinEffect
    /// layer.
    Sim = 1,
    /// Audio-DSP callback. Implies `{NoAlloc, NoUnbounded, Deadline<1ms>,
    /// Realtime<Crit>, PureDet, DetRNG}`. Real-time critical.
    Audio = 2,
    /// Network IO. Implies `{IO, Sensitive<"net-egress">}` ; gated by
    /// `ConsentToken<"net">`.
    Net = 3,
    /// Save-journal-append. Implies `{IO, Audit<"save-journal">}` ; gated by
    /// `ConsentToken<"fs">`.
    Save = 4,
    /// Telemetry / observability ring. Universal-additive — composes with anything
    /// at this layer ; egress gated separately by `ConsentToken<"telemetry-egress">`.
    Telemetry = 5,
}

impl SubstrateEffect {
    /// Canonical source-form name (matches how the effect appears in
    /// `/ {Sim, Render, ...}` row annotations).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Render => "Render",
            Self::Sim => "Sim",
            Self::Audio => "Audio",
            Self::Net => "Net",
            Self::Save => "Save",
            Self::Telemetry => "Telemetry",
        }
    }

    /// All six effects, in stable-order. Useful for iteration tests.
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Render,
            Self::Sim,
            Self::Audio,
            Self::Net,
            Self::Save,
            Self::Telemetry,
        ]
    }

    /// Bit-mask for this effect in the [`SubstrateEffectRow`] encoding.
    #[must_use]
    const fn bit(self) -> u8 {
        1u8 << (self as u8)
    }
}

impl fmt::Display for SubstrateEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ─ SubstrateEffectRow ────────────────────────────────────────────────────────

/// A small bit-set over [`SubstrateEffect`].
///
/// § ENCODING : `bits & (1 << e as u8)` for each `e`. Six effects fit in one byte.
/// § ORDERING : iteration follows the stable enum-order of [`SubstrateEffect`].
/// § PURITY : `SubstrateEffectRow::EMPTY` represents `{}` — the canonical pure
///   marker at this layer. (Backwards-compat : a fn without a Substrate effect-row
///   annotation defaults to `EMPTY`, matching the existing pure-fn convention from
///   `specs/04 § EFFECT-ROW TYPES § ⟨⟩ ≡ pure`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SubstrateEffectRow {
    bits: u8,
}

impl SubstrateEffectRow {
    /// The empty row — `{}` ≡ pure at this layer.
    pub const EMPTY: Self = Self { bits: 0 };

    /// Build from a slice of effects. Duplicates are absorbed (set semantics).
    #[must_use]
    pub fn from_effects(effects: &[SubstrateEffect]) -> Self {
        let mut bits: u8 = 0;
        for e in effects {
            bits |= e.bit();
        }
        Self { bits }
    }

    /// Build from a single effect.
    #[must_use]
    pub const fn singleton(effect: SubstrateEffect) -> Self {
        Self { bits: effect.bit() }
    }

    /// Add an effect (idempotent).
    pub fn insert(&mut self, effect: SubstrateEffect) {
        self.bits |= effect.bit();
    }

    /// Remove an effect.
    pub fn remove(&mut self, effect: SubstrateEffect) {
        self.bits &= !effect.bit();
    }

    /// `true` iff `effect` is in the row.
    #[must_use]
    pub const fn contains(&self, effect: SubstrateEffect) -> bool {
        self.bits & effect.bit() != 0
    }

    /// `true` iff the row is empty (≡ pure).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Number of effects in the row.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bits.count_ones() as usize
    }

    /// Iterate over effects in stable-order.
    pub fn iter(&self) -> impl Iterator<Item = SubstrateEffect> + '_ {
        SubstrateEffect::all()
            .into_iter()
            .filter(|e| self.contains(*e))
    }

    /// Set-union of two rows, ignoring composition rules (use [`try_compose`]
    /// for the rule-aware version).
    #[must_use]
    pub const fn union(&self, other: &Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Set-intersection of two rows.
    #[must_use]
    pub const fn intersection(&self, other: &Self) -> Self {
        Self {
            bits: self.bits & other.bits,
        }
    }

    /// `true` iff every effect of `other` is also in `self` (sub-row check).
    #[must_use]
    pub const fn contains_row(&self, other: &Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    /// Raw bits (for serialization or testing). Stage-0 stable.
    #[must_use]
    pub const fn bits(&self) -> u8 {
        self.bits
    }
}

impl fmt::Display for SubstrateEffectRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        let mut first = true;
        for e in self.iter() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{e}")?;
            first = false;
        }
        write!(f, "}}")
    }
}

// ─ RowContext ────────────────────────────────────────────────────────────────

/// Caller-context bits that gate certain composition rules.
///
/// § RATIONALE
///   Some compositions are legal **iff** the caller holds a privilege or
///   capability-grant. For example, `Sim ⊎ Net` is normally `EFR0001` (potential
///   exfil), but is **legal** if the caller is inside an
///   `unsafe_caps_grant(net_send_state) { ... }` block (which the HIR layer
///   tracks as `has_caps_grant_net_send_state = true`).
///
///   The `RowContext` struct captures these gating bits at the call-site of
///   [`try_compose`]. The HIR / type-checker materializes the bits from the
///   surrounding lexical scope ; this crate just consumes them.
///
/// § DEFAULT
///   `RowContext::default()` has all gating bits cleared — the strictest
///   interpretation. `try_compose(a, b, &RowContext::default())` is the
///   "is this composition legal in **any** context?" query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowContext {
    /// `true` iff the caller is inside an `unsafe_caps_grant(net_send_state)`
    /// block — gates `Sim ⊎ Net` past `EFR0001`.
    pub has_caps_grant_net_send_state: bool,
    /// `true` iff the caller declares the `PureDet` BuiltinEffect at the same
    /// level — gates `Render` to `EFR0002` (compile-error : Render is not pure).
    pub has_pure_det: bool,
    /// `true` iff the caller declares the `Audit<dom>` BuiltinEffect with
    /// matching domain — used by the `Save ⊎ *` advisory rule (not a hard
    /// error : `Save` *requires* an audit companion at the BuiltinEffect layer,
    /// which this flag confirms).
    pub has_audit_companion: bool,
    /// `true` iff the caller is at `Privilege<Kernel>` or higher — used by
    /// future hardened compositions (currently advisory).
    pub has_kernel_privilege: bool,
}

impl RowContext {
    /// Builder : caller is inside `caps_grant(net_send_state)`.
    #[must_use]
    pub const fn with_caps_grant_net_send_state(mut self) -> Self {
        self.has_caps_grant_net_send_state = true;
        self
    }

    /// Builder : caller has `PureDet` at the same level.
    #[must_use]
    pub const fn with_pure_det(mut self) -> Self {
        self.has_pure_det = true;
        self
    }

    /// Builder : caller has `Audit<dom>` companion.
    #[must_use]
    pub const fn with_audit_companion(mut self) -> Self {
        self.has_audit_companion = true;
        self
    }

    /// Builder : caller has `Privilege<Kernel>`.
    #[must_use]
    pub const fn with_kernel_privilege(mut self) -> Self {
        self.has_kernel_privilege = true;
        self
    }
}

// ─ ConflictReason / EFR codes ────────────────────────────────────────────────

/// Stable diagnostic codes for Substrate effect-row composition conflicts.
///
/// § STABLE BLOCK : EFR0001..EFR0010 allocated in T11-D92 (S8-H4). Reordering or
/// repurposing any code is a major-version-bump event. Future codes start at
/// EFR0011 in a separate block.
///
/// § ACTIONABLE-MESSAGE-CONVENTION : every variant carries enough context to
/// produce a diagnostic of the shape :
///
/// ```text
/// error[EFR0001]: composition `{Sim} ⊎ {Net}` requires caps_grant(net_send_state)
///   = note: see specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING for rationale
///   = help: wrap the call in `unsafe_caps_grant(net_send_state) { ... }`,
///           or move the network call to a non-Sim fiber
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConflictReason {
    /// EFR0001 — `Sim ⊎ Net` without `caps_grant(net_send_state)`.
    /// Rationale : un-gated Sim → Net composition could exfiltrate simulation state
    /// (consent-violation per PRIME_DIRECTIVE § 5 + `specs/11_IFC § PRIME-DIRECTIVE
    /// ENCODING`).
    #[error(
        "[EFR0001] composition `{{Sim}} ⊎ {{Net}}` requires caps_grant(net_send_state) \
         — un-gated Sim→Net could exfiltrate game-state \
         (see specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING ; \
         help: wrap the call in `unsafe_caps_grant(net_send_state) {{ ... }}`)"
    )]
    SimPlusNetNeedsCapsGrant,

    /// EFR0002 — `PureDet ⊎ Render` is a compile-error : rendering touches output
    /// devices and is not pure-deterministic.
    /// Rationale : per `specs/30 § OMEGA-STEP § DETERMINISTIC-REPLAY-INVARIANTS`,
    /// Render is intentionally outside the PureDet contract.
    #[error(
        "[EFR0002] composition `{{PureDet}} ⊎ {{Render}}` is forbidden — \
         rendering touches output devices and is not bit-exact reproducible \
         (see specs/30_SUBSTRATE.csl § OMEGA-STEP § DETERMINISTIC-REPLAY-INVARIANTS ; \
         help: drop {{PureDet}} from the row, or move rendering to a non-PureDet caller)"
    )]
    PureDetPlusRenderForbidden,

    /// EFR0003 — `Save` requires an `Audit<>` companion.
    /// Rationale : per `specs/30 § EFFECT-ROWS § SUBSTRATE-EFFECTS`, every Save
    /// composes with `{IO, Audit<"save-journal">}`. The Substrate-row layer flags
    /// the missing audit companion ; the BuiltinEffect layer enforces the actual
    /// presence.
    #[error(
        "[EFR0003] effect `{{Save}}` requires an `Audit<>` companion at the \
         BuiltinEffect layer (RowContext.has_audit_companion was false) \
         (see specs/30_SUBSTRATE.csl § EFFECT-ROWS § SUBSTRATE-EFFECTS ; \
         help: add `Audit<\"save-journal\">` to the BuiltinEffect row)"
    )]
    SaveRequiresAuditCompanion,

    /// EFR0004 — `Net ⊎ PureDet` is a compile-error : real network IO is
    /// non-deterministic. Replay-mode (which records traces) is the only legal
    /// PureDet-Net composition, and is gated separately.
    #[error(
        "[EFR0004] composition `{{Net}} ⊎ {{PureDet}}` is forbidden — \
         live network IO is non-deterministic between runs \
         (see specs/30_SUBSTRATE.csl § EFFECT-ROWS § COMPOSITION-RULES ; \
         help: use replay-mode (recorded-trace) for deterministic network behavior, \
         or drop {{PureDet}} from the row)"
    )]
    NetPlusPureDetForbidden,

    /// EFR0005 — `Audio ⊎ Sim` in the same fiber is forbidden.
    /// Rationale : per `specs/30 § EFFECT-ROWS § COMPOSITION-RULES`, Audio runs
    /// on a dedicated RT-thread reading frozen-sim-val. Same-fiber composition
    /// would either (a) starve the RT-thread or (b) tear the sim state mid-step.
    #[error(
        "[EFR0005] composition `{{Audio}} ⊎ {{Sim}}` in the same fiber is forbidden — \
         Audio runs on a dedicated RT-thread that reads frozen-sim-val \
         (see specs/30_SUBSTRATE.csl § EFFECT-ROWS § COMPOSITION-RULES ; \
         help: split into two fns — one /{{Sim}} (sim-fiber) and one /{{Audio}} (audio-callback))"
    )]
    AudioPlusSimSameFiberForbidden,

    /// EFR0006 — `Audio` requires `Realtime<Crit>` budget at the BuiltinEffect
    /// layer. Stage-0 surfaces this as advisory (the BuiltinEffect layer
    /// enforces the actual budget).
    #[error(
        "[EFR0006] effect `{{Audio}}` requires a `Realtime<Crit>` budget + \
         `Deadline<1ms>` at the BuiltinEffect layer \
         (see specs/30_SUBSTRATE.csl § EFFECT-ROWS § SUBSTRATE-EFFECTS ; \
         help: add `Realtime<Crit>, Deadline<1ms>, NoAlloc, NoUnbounded` to the row)"
    )]
    AudioRequiresRealtimeCrit,

    /// EFR0007 — `Render ⊎ Sim` without `Sim` being frozen-val is forbidden.
    /// Rationale : per `specs/30 § OMEGA-STEP § PHASES` (phase-7 frozen-sim
    /// reads), render reads val-frozen-sim, never trn-mutating-sim. Stage-0
    /// surfaces this when the row presents both effects without a frozen-marker
    /// in the RowContext.
    #[error(
        "[EFR0007] composition `{{Render}} ⊎ {{Sim}}` requires Sim to be frozen-val \
         at the BuiltinEffect / capability layer \
         (see specs/30_SUBSTRATE.csl § OMEGA-STEP § PHASES — phase-7 reads frozen-sim ; \
         help: phase Sim mutation into a sim-fiber that produces a frozen-val by phase-7)"
    )]
    RenderPlusSimNeedsFrozen,

    /// EFR0008 — Empty row composed with anything is identity ; this code is
    /// reserved for an internal-error path where the bit-set encoding rejects
    /// a malformed row.
    /// Rationale : defense-in-depth — if a future SubstrateEffect variant is
    /// added without updating the bit-encoding, this code surfaces the bug.
    #[error(
        "[EFR0008] internal: SubstrateEffectRow has bits outside the valid range \
         {{Render, Sim, Audio, Net, Save, Telemetry}} (raw bits = 0b{bits:08b}) \
         (this is a compiler bug ; please file an issue with the offending row)"
    )]
    InvalidRowBits {
        /// The raw bit-pattern that includes invalid bits.
        bits: u8,
    },

    /// EFR0009 — `Net` requires `ConsentToken<"net">` to be active.
    /// Stage-0 surfaces this as advisory ; the runtime + HIR layer enforces
    /// the actual token-check.
    #[error(
        "[EFR0009] effect `{{Net}}` requires `ConsentToken<\"net\">` \
         (see PRIME_DIRECTIVE.md § 5 CONSENT-ARCHITECTURE + \
         specs/30_SUBSTRATE.csl § Ω-TENSOR § OmegaConsent ; \
         help: ensure the call-site is within a scope where the consent-token \
         has been granted-and-not-revoked)"
    )]
    NetRequiresConsentToken,

    /// EFR0010 — `Save` requires `ConsentToken<"fs">` to be active.
    /// Stage-0 surfaces this as advisory ; same enforcement-pattern as EFR0009.
    #[error(
        "[EFR0010] effect `{{Save}}` requires `ConsentToken<\"fs\">` \
         (see PRIME_DIRECTIVE.md § 5 CONSENT-ARCHITECTURE + \
         specs/30_SUBSTRATE.csl § Ω-TENSOR § OmegaConsent ; \
         help: ensure the call-site is within a scope where the consent-token \
         has been granted-and-not-revoked)"
    )]
    SaveRequiresConsentToken,
}

impl ConflictReason {
    /// Stable diagnostic-code as a `&'static str` (e.g., `"EFR0001"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SimPlusNetNeedsCapsGrant => "EFR0001",
            Self::PureDetPlusRenderForbidden => "EFR0002",
            Self::SaveRequiresAuditCompanion => "EFR0003",
            Self::NetPlusPureDetForbidden => "EFR0004",
            Self::AudioPlusSimSameFiberForbidden => "EFR0005",
            Self::AudioRequiresRealtimeCrit => "EFR0006",
            Self::RenderPlusSimNeedsFrozen => "EFR0007",
            Self::InvalidRowBits { .. } => "EFR0008",
            Self::NetRequiresConsentToken => "EFR0009",
            Self::SaveRequiresConsentToken => "EFR0010",
        }
    }

    /// `true` iff this is a hard compile-error (vs an advisory that requires a
    /// companion / context-bit one-layer-up).
    ///
    /// § STAGE-0
    ///   Hard errors : EFR0001 (without grant), EFR0002, EFR0004, EFR0005,
    ///                 EFR0008.
    ///   Advisories  : EFR0003, EFR0006, EFR0007, EFR0009, EFR0010 (these
    ///                 require a companion at the BuiltinEffect layer ; this
    ///                 layer just flags them).
    #[must_use]
    pub const fn is_hard_error(&self) -> bool {
        matches!(
            self,
            Self::SimPlusNetNeedsCapsGrant
                | Self::PureDetPlusRenderForbidden
                | Self::NetPlusPureDetForbidden
                | Self::AudioPlusSimSameFiberForbidden
                | Self::InvalidRowBits { .. }
        )
    }
}

// ─ try_compose ───────────────────────────────────────────────────────────────

/// Attempt to compose two Substrate effect-rows under the canonical composition
/// table.
///
/// § ALGORITHM
///   1. Validate both rows have only valid bits (defensive ; should always
///      hold by construction). Otherwise → `EFR0008`.
///   2. Compute the union of `a` and `b`.
///   3. For each rule in the composition table (below), check the union +
///      [`RowContext`]. Any violation is appended to the error-list.
///   4. If the error-list is non-empty AND contains a hard-error,
///      return `Err`. Otherwise return `Ok(union_row)` (advisories are
///      filtered into a separate channel — see [`compose_with_advisories`]).
///
/// § COMPOSITION TABLE (full 6×6 ; `_` = identity ; `×` = empty-row check)
///
/// ```text
///                 Render   Sim      Audio    Net      Save     Telemetry
///   Render        ✓ id     ✓ R7?    ✓        ✓ +grant ✓        ✓
///   Sim           ✓ R7?    ✓ id     ✗ EFR05  ✓ EFR01  ✓        ✓
///   Audio         ✓        ✗ EFR05  ✓ id     ✓ +grant ✓        ✓
///   Net           ✓ +grant ✓ EFR01  ✓ +grant ✓ id     ✓        ✓
///   Save          ✓        ✓        ✓        ✓        ✓ id     ✓
///   Telemetry     ✓        ✓        ✓        ✓        ✓        ✓ id
///
///   PureDet       ✗ EFR02  ✓        ✓ ‼      ✗ EFR04  ✓        ✓
/// ```
///
///   - `id`            = composition with self ≡ identity (no-op union)
///   - `EFR0X`         = hard-error code
///   - `R7?`           = advisory `EFR0007` (requires frozen-sim marker)
///   - `+grant`        = legal iff `RowContext::has_caps_grant_*` set
///   - `‼`             = `Audio` is *defined* PureDet at BuiltinEffect layer ;
///                       the row-level composition is fine.
///   - `PureDet`       = NOT a SubstrateEffect, but a BuiltinEffect bit fed via
///                       [`RowContext::has_pure_det`].
pub fn try_compose(
    a: &SubstrateEffectRow,
    b: &SubstrateEffectRow,
    ctx: &RowContext,
) -> Result<SubstrateEffectRow, Vec<ConflictReason>> {
    let advisories = check_advisories(a, b, ctx);
    let hard = check_hard_errors(a, b, ctx);

    // Defensive bit-validation. The full SubstrateEffect set covers bits 0..6,
    // so the valid mask is 0b0011_1111. Bits outside this range are EFR0008.
    let invalid_bits_a = a.bits & !VALID_BITS_MASK;
    let invalid_bits_b = b.bits & !VALID_BITS_MASK;
    let mut errors: Vec<ConflictReason> = Vec::new();
    if invalid_bits_a != 0 {
        errors.push(ConflictReason::InvalidRowBits { bits: a.bits });
    }
    if invalid_bits_b != 0 {
        errors.push(ConflictReason::InvalidRowBits { bits: b.bits });
    }
    errors.extend(hard);

    if !errors.is_empty() {
        // Any hard-error fails the compose. Advisories ride along for diagnostic
        // completeness — tools can grep for `EFR00*` codes.
        errors.extend(advisories);
        return Err(errors);
    }
    if !advisories.is_empty() {
        // No hard errors but advisories present — return Ok with the union but
        // surface advisories via a separate path. Callers that want strict
        // checking should use `compose_with_advisories` instead.
        // (Deliberate design : try_compose is the "is this ok in any reading?"
        // entry-point ; compose_with_advisories is the strict-mode entry.)
    }
    Ok(a.union(b))
}

/// Like [`try_compose`] but advisories are also reported as `Err` (strict mode).
///
/// § USE-CASE
///   The HIR layer when running under `--strict-effects` should call this. The
///   editor / IDE-layer (squiggle-on-warning) should also call this — advisories
///   are real warnings, just non-blocking.
pub fn compose_with_advisories(
    a: &SubstrateEffectRow,
    b: &SubstrateEffectRow,
    ctx: &RowContext,
) -> Result<SubstrateEffectRow, Vec<ConflictReason>> {
    let mut errors: Vec<ConflictReason> = Vec::new();

    // Bit validation (EFR0008).
    if a.bits & !VALID_BITS_MASK != 0 {
        errors.push(ConflictReason::InvalidRowBits { bits: a.bits });
    }
    if b.bits & !VALID_BITS_MASK != 0 {
        errors.push(ConflictReason::InvalidRowBits { bits: b.bits });
    }

    errors.extend(check_hard_errors(a, b, ctx));
    errors.extend(check_advisories(a, b, ctx));

    if errors.is_empty() {
        Ok(a.union(b))
    } else {
        Err(errors)
    }
}

/// Bit-mask covering the six valid SubstrateEffect bits (0b0011_1111).
const VALID_BITS_MASK: u8 = 0b0011_1111;

/// Check the hard-error rules (those that fail compilation regardless of
/// BuiltinEffect-layer companions).
fn check_hard_errors(
    a: &SubstrateEffectRow,
    b: &SubstrateEffectRow,
    ctx: &RowContext,
) -> Vec<ConflictReason> {
    let union = a.union(b);
    let mut errors: Vec<ConflictReason> = Vec::new();

    // EFR0001 : Sim ⊎ Net without caps_grant(net_send_state)
    if union.contains(SubstrateEffect::Sim)
        && union.contains(SubstrateEffect::Net)
        && !ctx.has_caps_grant_net_send_state
    {
        errors.push(ConflictReason::SimPlusNetNeedsCapsGrant);
    }

    // EFR0002 : PureDet ⊎ Render
    if ctx.has_pure_det && union.contains(SubstrateEffect::Render) {
        errors.push(ConflictReason::PureDetPlusRenderForbidden);
    }

    // EFR0004 : Net ⊎ PureDet
    if ctx.has_pure_det && union.contains(SubstrateEffect::Net) {
        errors.push(ConflictReason::NetPlusPureDetForbidden);
    }

    // EFR0005 : Audio ⊎ Sim same-fiber. Both rows-having-both-effects ⇒ same-fiber
    // co-presence (compose at row-level).
    if union.contains(SubstrateEffect::Audio) && union.contains(SubstrateEffect::Sim) {
        errors.push(ConflictReason::AudioPlusSimSameFiberForbidden);
    }

    errors
}

/// Check the advisory rules (those that require a companion at the BuiltinEffect
/// layer ; this crate flags them so the HIR layer can report them up the
/// diagnostic chain).
fn check_advisories(
    a: &SubstrateEffectRow,
    b: &SubstrateEffectRow,
    ctx: &RowContext,
) -> Vec<ConflictReason> {
    let union = a.union(b);
    let mut advisories: Vec<ConflictReason> = Vec::new();

    // EFR0003 : Save requires Audit<> companion.
    if union.contains(SubstrateEffect::Save) && !ctx.has_audit_companion {
        advisories.push(ConflictReason::SaveRequiresAuditCompanion);
    }

    // EFR0006 : Audio requires Realtime<Crit> + Deadline<1ms>. Stage-0 has no
    // BuiltinEffect-layer access from this crate, so we surface the advisory
    // unconditionally if Audio is present.
    if union.contains(SubstrateEffect::Audio) {
        advisories.push(ConflictReason::AudioRequiresRealtimeCrit);
    }

    // EFR0007 : Render + Sim requires frozen-sim marker. We can't see the marker
    // from this crate — surfaced as advisory whenever both effects are present
    // and the EFR0005 hard-error did NOT already fire (Audio+Sim same-fiber
    // takes precedence diagnostic-wise).
    if union.contains(SubstrateEffect::Render)
        && union.contains(SubstrateEffect::Sim)
        && !union.contains(SubstrateEffect::Audio)
    {
        advisories.push(ConflictReason::RenderPlusSimNeedsFrozen);
    }

    // EFR0009 : Net requires ConsentToken<"net">.
    if union.contains(SubstrateEffect::Net) {
        advisories.push(ConflictReason::NetRequiresConsentToken);
    }

    // EFR0010 : Save requires ConsentToken<"fs">.
    if union.contains(SubstrateEffect::Save) {
        advisories.push(ConflictReason::SaveRequiresConsentToken);
    }

    advisories
}

// ─ Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── SubstrateEffect basic tests ──────────────────────────────────────

    #[test]
    fn substrate_effect_names_are_canonical() {
        assert_eq!(SubstrateEffect::Render.name(), "Render");
        assert_eq!(SubstrateEffect::Sim.name(), "Sim");
        assert_eq!(SubstrateEffect::Audio.name(), "Audio");
        assert_eq!(SubstrateEffect::Net.name(), "Net");
        assert_eq!(SubstrateEffect::Save.name(), "Save");
        assert_eq!(SubstrateEffect::Telemetry.name(), "Telemetry");
    }

    #[test]
    fn substrate_effect_all_returns_six_in_stable_order() {
        let all = SubstrateEffect::all();
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], SubstrateEffect::Render);
        assert_eq!(all[5], SubstrateEffect::Telemetry);
    }

    #[test]
    fn substrate_effect_bits_distinct() {
        let mut seen: u8 = 0;
        for e in SubstrateEffect::all() {
            assert_eq!(seen & e.bit(), 0, "duplicate bit for {e}");
            seen |= e.bit();
        }
        assert_eq!(seen, VALID_BITS_MASK);
    }

    // ─── SubstrateEffectRow tests ─────────────────────────────────────────

    #[test]
    fn empty_row_is_pure() {
        let row = SubstrateEffectRow::EMPTY;
        assert!(row.is_empty());
        assert_eq!(row.len(), 0);
        assert!(!row.contains(SubstrateEffect::Render));
    }

    #[test]
    fn singleton_contains_only_that_effect() {
        let row = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        assert!(row.contains(SubstrateEffect::Render));
        assert!(!row.contains(SubstrateEffect::Sim));
        assert_eq!(row.len(), 1);
    }

    #[test]
    fn from_effects_builds_row() {
        let row = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Render,
            SubstrateEffect::Telemetry,
        ]);
        assert!(row.contains(SubstrateEffect::Render));
        assert!(row.contains(SubstrateEffect::Telemetry));
        assert!(!row.contains(SubstrateEffect::Sim));
        assert_eq!(row.len(), 2);
    }

    #[test]
    fn from_effects_dedups_duplicates() {
        let row = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Render,
            SubstrateEffect::Render,
            SubstrateEffect::Render,
        ]);
        assert_eq!(row.len(), 1);
    }

    #[test]
    fn insert_and_remove_round_trip() {
        let mut row = SubstrateEffectRow::EMPTY;
        row.insert(SubstrateEffect::Sim);
        row.insert(SubstrateEffect::Render);
        assert_eq!(row.len(), 2);
        row.remove(SubstrateEffect::Sim);
        assert_eq!(row.len(), 1);
        assert!(row.contains(SubstrateEffect::Render));
        assert!(!row.contains(SubstrateEffect::Sim));
    }

    #[test]
    fn iter_returns_stable_order() {
        let row = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Telemetry,
            SubstrateEffect::Render,
            SubstrateEffect::Audio,
        ]);
        let collected: Vec<_> = row.iter().collect();
        assert_eq!(
            collected,
            vec![
                SubstrateEffect::Render,
                SubstrateEffect::Audio,
                SubstrateEffect::Telemetry,
            ]
        );
    }

    #[test]
    fn union_combines_rows() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Telemetry);
        let u = a.union(&b);
        assert_eq!(u.len(), 2);
        assert!(u.contains(SubstrateEffect::Render));
        assert!(u.contains(SubstrateEffect::Telemetry));
    }

    #[test]
    fn intersection_yields_common() {
        let a = SubstrateEffectRow::from_effects(&[SubstrateEffect::Render, SubstrateEffect::Sim]);
        let b = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Render,
            SubstrateEffect::Telemetry,
        ]);
        let i = a.intersection(&b);
        assert_eq!(i.len(), 1);
        assert!(i.contains(SubstrateEffect::Render));
    }

    #[test]
    fn contains_row_is_subset_check() {
        let small = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        let big = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Render,
            SubstrateEffect::Telemetry,
        ]);
        assert!(big.contains_row(&small));
        assert!(!small.contains_row(&big));
        // Self-containment
        assert!(big.contains_row(&big));
    }

    #[test]
    fn display_format_is_canonical() {
        let row = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Render,
            SubstrateEffect::Telemetry,
        ]);
        // Stable-order : Render before Telemetry.
        assert_eq!(format!("{row}"), "{Render, Telemetry}");

        let empty = SubstrateEffectRow::EMPTY;
        assert_eq!(format!("{empty}"), "{}");
    }

    #[test]
    fn bits_round_trip_via_constructor() {
        let row =
            SubstrateEffectRow::from_effects(&[SubstrateEffect::Render, SubstrateEffect::Save]);
        let bits = row.bits();
        let reconstructed = SubstrateEffectRow { bits };
        assert_eq!(row, reconstructed);
    }

    // ─── Composition rule tests ───────────────────────────────────────────

    #[test]
    fn telemetry_universal_additive() {
        // Telemetry composes with everything in any order.
        let ctx = RowContext::default();
        for e in SubstrateEffect::all() {
            let a = SubstrateEffectRow::singleton(e);
            let b = SubstrateEffectRow::singleton(SubstrateEffect::Telemetry);
            let result = compose_with_advisories(&a, &b, &ctx);
            // We may surface advisories (e.g., Audio→EFR0006) but not for Telemetry-alone.
            // For this universal-additive test we only require no EFR0008
            // (invalid-bits) and no hard-error from Telemetry composition itself.
            match result {
                Ok(_) => {}
                Err(reasons) => {
                    // The advisories present must not include any code introduced by the
                    // Telemetry composition (Telemetry has none).
                    for r in &reasons {
                        assert_ne!(
                            r.code(),
                            "EFR0008",
                            "Telemetry composition triggered EFR0008"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn render_plus_telemetry_clean() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Telemetry);
        let ctx = RowContext::default();
        let result = compose_with_advisories(&a, &b, &ctx);
        assert!(result.is_ok(), "Render + Telemetry should be clean");
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn sim_plus_net_without_grant_is_efr0001() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Sim);
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Net);
        let ctx = RowContext::default();
        let result = try_compose(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0001"));
    }

    #[test]
    fn sim_plus_net_with_grant_succeeds() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Sim);
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Net);
        let ctx = RowContext::default().with_caps_grant_net_send_state();
        let result = try_compose(&a, &b, &ctx);
        // Hard errors absent ; advisories may surface (EFR0009 ConsentToken<"net">).
        assert!(
            result.is_ok(),
            "Sim + Net + grant should succeed at try_compose level"
        );
        let row = result.unwrap();
        assert!(row.contains(SubstrateEffect::Sim));
        assert!(row.contains(SubstrateEffect::Net));
    }

    #[test]
    fn pure_det_plus_render_is_efr0002() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default().with_pure_det();
        let result = try_compose(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0002"));
    }

    #[test]
    fn pure_det_without_render_clean() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Sim);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default().with_pure_det();
        let result = try_compose(&a, &b, &ctx);
        // PureDet + Sim is fine — Sim is intended PureDet at the BuiltinEffect layer.
        assert!(result.is_ok());
    }

    #[test]
    fn save_advisory_efr0003_when_no_audit() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Save);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default();
        let result = compose_with_advisories(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0003"));
        assert!(errs.iter().any(|e| e.code() == "EFR0010"));
    }

    #[test]
    fn save_with_audit_companion_no_efr0003() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Save);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default().with_audit_companion();
        let result = compose_with_advisories(&a, &b, &ctx);
        // EFR0003 should be cleared, EFR0010 (consent-token) still surfaces.
        let errs = result.err().unwrap_or_default();
        assert!(!errs.iter().any(|e| e.code() == "EFR0003"));
        // EFR0010 is the consent-token advisory ; remains until ConsentToken state is checked.
        assert!(errs.iter().any(|e| e.code() == "EFR0010"));
    }

    #[test]
    fn net_plus_pure_det_is_efr0004() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Net);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default()
            .with_pure_det()
            .with_caps_grant_net_send_state();
        let result = try_compose(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0004"));
    }

    #[test]
    fn audio_plus_sim_same_fiber_is_efr0005() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Audio);
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Sim);
        let ctx = RowContext::default();
        let result = try_compose(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0005"));
    }

    #[test]
    fn audio_alone_advisory_efr0006() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Audio);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default();
        let result = compose_with_advisories(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0006"));
    }

    #[test]
    fn render_plus_sim_advisory_efr0007() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Sim);
        let ctx = RowContext::default();
        let result = compose_with_advisories(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0007"));
    }

    #[test]
    fn invalid_bits_efr0008() {
        // Construct a row directly with bits outside the valid mask.
        let bad = SubstrateEffectRow { bits: 0b1100_0000 };
        let good = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default();
        let result = try_compose(&bad, &good, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0008"));
    }

    #[test]
    fn net_advisory_efr0009() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Net);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default().with_caps_grant_net_send_state();
        let result = compose_with_advisories(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0009"));
    }

    #[test]
    fn save_advisory_efr0010() {
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Save);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default().with_audit_companion();
        let result = compose_with_advisories(&a, &b, &ctx);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.code() == "EFR0010"));
    }

    #[test]
    fn empty_plus_empty_is_pure() {
        let a = SubstrateEffectRow::EMPTY;
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default();
        let result = try_compose(&a, &b, &ctx).expect("empty + empty is pure");
        assert!(result.is_empty());
    }

    #[test]
    fn empty_plus_render_yields_render() {
        let a = SubstrateEffectRow::EMPTY;
        let b = SubstrateEffectRow::singleton(SubstrateEffect::Render);
        let ctx = RowContext::default();
        let result = try_compose(&a, &b, &ctx).expect("empty + render is render");
        assert!(result.contains(SubstrateEffect::Render));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn idempotent_self_composition() {
        // Composing a row with itself yields the same row (provided no rule fires).
        let row = SubstrateEffectRow::from_effects(&[
            SubstrateEffect::Render,
            SubstrateEffect::Telemetry,
        ]);
        let ctx = RowContext::default();
        let result = try_compose(&row, &row, &ctx).expect("idempotent self-compose");
        assert_eq!(result, row);
    }

    #[test]
    fn all_efr_codes_distinct_and_stable() {
        // Stability test : the EFR0001..EFR0010 block has exactly 10 distinct codes.
        let reasons = [
            ConflictReason::SimPlusNetNeedsCapsGrant,
            ConflictReason::PureDetPlusRenderForbidden,
            ConflictReason::SaveRequiresAuditCompanion,
            ConflictReason::NetPlusPureDetForbidden,
            ConflictReason::AudioPlusSimSameFiberForbidden,
            ConflictReason::AudioRequiresRealtimeCrit,
            ConflictReason::RenderPlusSimNeedsFrozen,
            ConflictReason::InvalidRowBits { bits: 0xFF },
            ConflictReason::NetRequiresConsentToken,
            ConflictReason::SaveRequiresConsentToken,
        ];
        let codes: Vec<&str> = reasons.iter().map(|r| r.code()).collect();
        assert_eq!(codes.len(), 10);
        // Distinctness
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 10, "codes are distinct");
        // Stability — the canonical block
        assert_eq!(
            codes,
            vec![
                "EFR0001", "EFR0002", "EFR0003", "EFR0004", "EFR0005", "EFR0006", "EFR0007",
                "EFR0008", "EFR0009", "EFR0010",
            ]
        );
    }

    #[test]
    fn hard_vs_advisory_classification() {
        // Hard errors
        assert!(ConflictReason::SimPlusNetNeedsCapsGrant.is_hard_error());
        assert!(ConflictReason::PureDetPlusRenderForbidden.is_hard_error());
        assert!(ConflictReason::NetPlusPureDetForbidden.is_hard_error());
        assert!(ConflictReason::AudioPlusSimSameFiberForbidden.is_hard_error());
        assert!(ConflictReason::InvalidRowBits { bits: 0 }.is_hard_error());
        // Advisories
        assert!(!ConflictReason::SaveRequiresAuditCompanion.is_hard_error());
        assert!(!ConflictReason::AudioRequiresRealtimeCrit.is_hard_error());
        assert!(!ConflictReason::RenderPlusSimNeedsFrozen.is_hard_error());
        assert!(!ConflictReason::NetRequiresConsentToken.is_hard_error());
        assert!(!ConflictReason::SaveRequiresConsentToken.is_hard_error());
    }

    // ─── Diagnostic-message tests ─────────────────────────────────────────

    #[test]
    fn diagnostic_message_contains_actionable_hint() {
        // EFR0001 message should include caps_grant + spec link.
        let msg = format!("{}", ConflictReason::SimPlusNetNeedsCapsGrant);
        assert!(msg.contains("EFR0001"));
        assert!(msg.contains("caps_grant(net_send_state)"));
        assert!(msg.contains("specs/11_IFC.csl") || msg.contains("PRIME-DIRECTIVE"));
        assert!(msg.contains("help:"));
    }

    #[test]
    fn diagnostic_message_efr0002_actionable() {
        let msg = format!("{}", ConflictReason::PureDetPlusRenderForbidden);
        assert!(msg.contains("EFR0002"));
        assert!(msg.contains("PureDet"));
        assert!(msg.contains("Render"));
        assert!(msg.contains("help:"));
    }

    // ─── Composition table sweep ─────────────────────────────────────────

    #[test]
    fn full_table_sweep_telemetry_universal_additive() {
        // Every (Telemetry, X) and (X, Telemetry) pair returns Ok at try_compose
        // level, regardless of context bits. (Advisories may still surface but
        // not from the Telemetry side.)
        let ctx = RowContext::default()
            .with_caps_grant_net_send_state()
            .with_audit_companion();
        for e in SubstrateEffect::all() {
            let a = SubstrateEffectRow::singleton(SubstrateEffect::Telemetry);
            let b = SubstrateEffectRow::singleton(e);
            assert!(
                try_compose(&a, &b, &ctx).is_ok(),
                "Telemetry + {e:?} should compose (try_compose level)"
            );
            assert!(
                try_compose(&b, &a, &ctx).is_ok(),
                "{e:?} + Telemetry should compose (try_compose level)"
            );
        }
    }

    #[test]
    fn pure_det_with_audio_alone_clean() {
        // Audio is intended PureDet at BuiltinEffect layer ; PureDet + Audio is fine.
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Audio);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default().with_pure_det();
        let result = try_compose(&a, &b, &ctx);
        assert!(
            result.is_ok(),
            "PureDet + Audio is the canonical audio-callback shape"
        );
    }

    #[test]
    fn defaults_reject_strict_save() {
        // RowContext::default() with Save triggers EFR0003 + EFR0010 advisories.
        let a = SubstrateEffectRow::singleton(SubstrateEffect::Save);
        let b = SubstrateEffectRow::EMPTY;
        let ctx = RowContext::default();
        let result = compose_with_advisories(&a, &b, &ctx);
        let errs = result.err().unwrap_or_default();
        let codes: std::collections::HashSet<_> = errs.iter().map(|e| e.code()).collect();
        assert!(codes.contains("EFR0003"), "missing EFR0003");
        assert!(codes.contains("EFR0010"), "missing EFR0010");
    }
}

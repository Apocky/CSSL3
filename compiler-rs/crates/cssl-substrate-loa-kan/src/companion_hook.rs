//! § CompanionAiHook — opt-in companion-AI scene-glue hook.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Substrate-side dual of Stage-8 companion-perspective rendering. Lets
//!   a companion-AI register a perspective-shift on a Sovereign-claimed
//!   cell. The renderer-side (cssl-render-companion-perspective) consumes
//!   this hook to drive the optional Stage-8 semantic overlay.
//!
//! § PRIME-DIRECTIVE
//!   - Default-deny : a cell without explicit Companion-consent refuses
//!     hook registration. The Sovereign of the cell MUST authorize
//!     companion presence via [`CompanionConsent::Granted`].
//!   - No surveillance-mirror : the hook does NOT carry observer-state
//!     data outside the cell's scope. Per spec § STAGE-8 the hook is
//!     consent-protected-rendering only.
//!   - Mutual-witness : when a hook fires, both the cell's Sovereign and
//!     the companion's identity are recorded in the audit-chain.
//!
//! § SPEC
//!   - `specs/32_SIGNATURE_RENDERING.csl` § STAGE-8 (CompanionSemantic).
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT § II.G` (mutual-witness).

/// § Discriminator for companion-AI hook kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CompanionAiKind {
    /// § No companion registered. Cells default to this.
    #[default]
    None = 0,
    /// § Creature-companion : a non-player creature with a perspective
    ///   on this cell (e.g. labyrinth-creature-companion-scene).
    Creature = 1,
    /// § NPC-companion : a story-bound NPC.
    Npc = 2,
    /// § Spirit-companion : a non-corporeal entity (ψ-resonance only).
    Spirit = 3,
    /// § Witness-companion : a recursive-witness Φ-tagged entity per
    ///   Stage-9 mise-en-abyme.
    Witness = 4,
}

impl CompanionAiKind {
    /// § All variants in canonical order.
    #[must_use]
    pub const fn all() -> [CompanionAiKind; 5] {
        [
            Self::None,
            Self::Creature,
            Self::Npc,
            Self::Spirit,
            Self::Witness,
        ]
    }

    /// § Stable canonical name for telemetry.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Creature => "creature",
            Self::Npc => "npc",
            Self::Spirit => "spirit",
            Self::Witness => "witness",
        }
    }
}

/// § Companion-consent status for a cell. The Sovereign of the cell
///   declares whether companion presence is authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum CompanionConsent {
    /// § Default — no companion permitted on this cell.
    #[default]
    Refused = 0,
    /// § Companion permitted — Sovereign has explicitly authorized.
    Granted = 1,
    /// § Mutual-witness required — Sovereign authorized but mandates
    ///   that BOTH parties' presence be audit-logged.
    MutualWitness = 2,
}

impl CompanionConsent {
    /// § True iff companion presence is permitted under this consent.
    #[must_use]
    pub const fn is_permitted(self) -> bool {
        matches!(self, Self::Granted | Self::MutualWitness)
    }

    /// § True iff mutual-witness audit is required.
    #[must_use]
    pub const fn requires_mutual_witness(self) -> bool {
        matches!(self, Self::MutualWitness)
    }
}

/// § Per-cell companion-AI hook : kind + consent + Sovereign + companion
///   identity. Registered by the cell's Sovereign ; consumed by Stage-8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompanionAiHook {
    /// § Kind of companion (None ⇒ no hook).
    pub kind: CompanionAiKind,
    /// § Consent status declared by cell-Sovereign.
    pub consent: CompanionConsent,
    /// § Cell-Sovereign handle that declared consent.
    pub sovereign_handle: u16,
    /// § Companion identity (handle into a per-substrate companion-table).
    pub companion_handle: u32,
    /// § Audit-seq stamp at registration. Monotone-increasing.
    pub audit_seq: u16,
    /// § Reserved-for-extension (must be 0).
    pub reserved: u8,
}

impl CompanionAiHook {
    /// § Construct a no-op hook : no companion registered. The default
    ///   for unclaimed cells.
    #[must_use]
    pub const fn none() -> CompanionAiHook {
        CompanionAiHook {
            kind: CompanionAiKind::None,
            consent: CompanionConsent::Refused,
            sovereign_handle: 0,
            companion_handle: 0,
            audit_seq: 0,
            reserved: 0,
        }
    }

    /// § Register a companion-AI hook on a cell.
    ///
    /// # Errors
    /// - [`HookError::ConsentRefused`] when consent is Refused.
    /// - [`HookError::SovereignNull`] when cell-Sovereign is unclaimed.
    /// - [`HookError::CompanionNullForActiveKind`] when kind is non-None
    ///   but companion_handle is 0.
    pub fn register(
        kind: CompanionAiKind,
        consent: CompanionConsent,
        sovereign_handle: u16,
        companion_handle: u32,
        audit_seq: u16,
    ) -> Result<CompanionAiHook, HookError> {
        if !consent.is_permitted() {
            return Err(HookError::ConsentRefused);
        }
        if sovereign_handle == 0 {
            return Err(HookError::SovereignNull);
        }
        if !matches!(kind, CompanionAiKind::None) && companion_handle == 0 {
            return Err(HookError::CompanionNullForActiveKind { kind });
        }
        Ok(CompanionAiHook {
            kind,
            consent,
            sovereign_handle,
            companion_handle,
            audit_seq,
            reserved: 0,
        })
    }

    /// § True iff the hook is active (kind ≠ None and consent permits).
    #[must_use]
    pub fn is_active(&self) -> bool {
        !matches!(self.kind, CompanionAiKind::None) && self.consent.is_permitted()
    }

    /// § True iff the hook requires mutual-witness audit logging.
    #[must_use]
    pub const fn requires_audit(&self) -> bool {
        self.consent.requires_mutual_witness()
    }

    /// § Bump the audit-seq counter by 1 (monotone). Used after audit
    ///   write completes successfully.
    pub fn bump_audit_seq(&mut self) {
        self.audit_seq = self.audit_seq.wrapping_add(1);
    }
}

impl Default for CompanionAiHook {
    fn default() -> Self {
        Self::none()
    }
}

/// § Failure modes for companion-hook registration.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// § Cell-Sovereign declared Refused consent for companion presence.
    #[error("LK0020 — companion-hook registration refused : Σ-mask consent declines companion")]
    ConsentRefused,
    /// § Cell has no Sovereign-handle ; cannot register a companion-hook
    ///   without an authorizing actor.
    #[error("LK0021 — companion-hook on unclaimed cell : Sovereign-handle is NULL")]
    SovereignNull,
    /// § Kind declared non-None but companion_handle is 0 (incoherent).
    #[error(
        "LK0022 — companion-hook with active kind={kind:?} but companion_handle=0 (incoherent)"
    )]
    CompanionNullForActiveKind { kind: CompanionAiKind },
    /// § Audit-write failed during mutual-witness enforcement.
    #[error("LK0023 — mutual-witness audit-write failure")]
    AuditWriteFailure,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Kind tags ──────────────────────────────────────────────────

    #[test]
    fn companion_kind_all_count() {
        assert_eq!(CompanionAiKind::all().len(), 5);
    }

    #[test]
    fn companion_kind_canonical_names_unique() {
        let names: Vec<&'static str> = CompanionAiKind::all()
            .iter()
            .map(|k| k.canonical_name())
            .collect();
        let mut s = names.clone();
        s.sort_unstable();
        let original = s.len();
        s.dedup();
        assert_eq!(s.len(), original);
    }

    // ── Consent ────────────────────────────────────────────────────

    #[test]
    fn consent_refused_not_permitted() {
        assert!(!CompanionConsent::Refused.is_permitted());
    }

    #[test]
    fn consent_granted_permitted_no_audit() {
        assert!(CompanionConsent::Granted.is_permitted());
        assert!(!CompanionConsent::Granted.requires_mutual_witness());
    }

    #[test]
    fn consent_mutual_witness_permitted_with_audit() {
        assert!(CompanionConsent::MutualWitness.is_permitted());
        assert!(CompanionConsent::MutualWitness.requires_mutual_witness());
    }

    // ── Hook registration ──────────────────────────────────────────

    #[test]
    fn none_hook_inactive() {
        let h = CompanionAiHook::none();
        assert!(!h.is_active());
    }

    #[test]
    fn register_with_refused_consent_fails() {
        let err = CompanionAiHook::register(
            CompanionAiKind::Creature,
            CompanionConsent::Refused,
            42,
            7,
            0,
        )
        .unwrap_err();
        assert!(matches!(err, HookError::ConsentRefused));
    }

    #[test]
    fn register_on_unclaimed_cell_fails() {
        let err = CompanionAiHook::register(
            CompanionAiKind::Creature,
            CompanionConsent::Granted,
            0,
            7,
            0,
        )
        .unwrap_err();
        assert!(matches!(err, HookError::SovereignNull));
    }

    #[test]
    fn register_active_kind_with_null_companion_fails() {
        let err =
            CompanionAiHook::register(CompanionAiKind::Spirit, CompanionConsent::Granted, 42, 0, 0)
                .unwrap_err();
        assert!(matches!(err, HookError::CompanionNullForActiveKind { .. }));
    }

    #[test]
    fn register_creature_with_consent_succeeds() {
        let h = CompanionAiHook::register(
            CompanionAiKind::Creature,
            CompanionConsent::Granted,
            42,
            7,
            0,
        )
        .unwrap();
        assert!(h.is_active());
        assert_eq!(h.sovereign_handle, 42);
        assert_eq!(h.companion_handle, 7);
        assert!(!h.requires_audit());
    }

    #[test]
    fn register_with_mutual_witness_requires_audit() {
        let h = CompanionAiHook::register(
            CompanionAiKind::Witness,
            CompanionConsent::MutualWitness,
            99,
            12,
            0,
        )
        .unwrap();
        assert!(h.is_active());
        assert!(h.requires_audit());
    }

    // ── Audit-seq bump ─────────────────────────────────────────────

    #[test]
    fn audit_seq_bump_increments() {
        let mut h = CompanionAiHook::register(
            CompanionAiKind::Creature,
            CompanionConsent::Granted,
            42,
            7,
            0,
        )
        .unwrap();
        assert_eq!(h.audit_seq, 0);
        h.bump_audit_seq();
        assert_eq!(h.audit_seq, 1);
        h.bump_audit_seq();
        assert_eq!(h.audit_seq, 2);
    }

    #[test]
    fn audit_seq_wraps_at_u16_max() {
        let mut h = CompanionAiHook::register(
            CompanionAiKind::Creature,
            CompanionConsent::Granted,
            42,
            7,
            u16::MAX,
        )
        .unwrap();
        h.bump_audit_seq();
        assert_eq!(h.audit_seq, 0);
    }
}

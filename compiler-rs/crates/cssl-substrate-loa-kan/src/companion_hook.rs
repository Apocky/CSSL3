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

// ════════════════════════════════════════════════════════════════════════════
// § W-S-CORE-6 (T11-D305) — CROSS-PILLAR COMPANION-AI HOOK
// ════════════════════════════════════════════════════════════════════════════
//
// § ATTRIBUTION-NOTE
//   This block (W-S-CORE-6) was authored under the T11-D305 task. Due
//   to a concurrent-fanout commit-collision, the initial landing tree
//   was bundled into a sibling commit ; this attribution-note documents
//   the canonical task-tag for telemetry / audit-trail discoverability.
//
// § ROLE
//   Generalizes the per-cell CompanionAiHook (procgen-only) into a
//   cross-pillar surface that lets a companion-AI propose mutations
//   spanning procgen + procgame + render + train scopes, with each
//   proposal gated by :
//     1. AICapScope             (which pillar the mutation targets)
//     2. AICapPolicy            (per-scope default-deny rules)
//     3. CompanionConsent       (Sovereign-side authorization)
//     4. MutualWitness audit    (cell-Sovereign + companion-handle logged)
//
// § DEFAULT-DENY DISCIPLINE
//   - Every scope has a `default_deny: true` flag in policy. A proposal
//     is REJECTED unless the Sovereign has explicitly granted that scope.
//   - Capability-mismatch (proposal-scope ∉ policy-allowed-scopes) ⇒
//     `SovereignMismatch` IfcViolation (no override).
//   - No-consent ⇒ default-deny ⇒ `SovereignMismatch`.
//
// § AUDIT-TRAIL
//   Every (propose, request_consent, apply) tuple emits an `AuditEntry`
//   carrying epoch + sovereign_handle + companion_handle + scope +
//   decision + intent_tag. The audit-trail is monotone-append-only.
//
// § SPEC
//   - `specs/30_SUBSTRATE_v2.csl` § COMPANION-AI INTEGRATION (cross-pillar
//     extension of D-1 substrate-S12 evolution).
//   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT § II.G` — mutual-witness.
//   - `specs/11_IFC.csl` § PRIVILEGE EFFECT — SovereignContext semantics.

use cssl_ifc::{IfcViolation, SovereignContext};

/// § Cross-pillar capability scopes a companion-AI proposal can target.
///
/// Each variant maps to one substrate-pillar. The Sovereign of a region
/// declares per-scope which scopes a companion-AI may propose mutations
/// against ; default-deny applies when no explicit grant exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AICapScope {
    /// § Procgen pillar — terrain/biome/Σ-mask cell mutation.
    ProcgenScope = 0,
    /// § Procgame pillar — creature behavior + game-state mutation
    ///   (LoA scene-glue : creature-companion behaviors).
    ProcgameScope = 1,
    /// § Render pillar — Stage-8 perspective overlay + visual hints.
    RenderScope = 2,
    /// § Train pillar — KAN-edge weight refinement (companion-driven
    ///   inverse-rendering hints).
    TrainScope = 3,
}

impl AICapScope {
    /// § All variants in canonical order.
    #[must_use]
    pub const fn all() -> [AICapScope; 4] {
        [
            Self::ProcgenScope,
            Self::ProcgameScope,
            Self::RenderScope,
            Self::TrainScope,
        ]
    }

    /// § Stable canonical name for telemetry.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::ProcgenScope => "procgen",
            Self::ProcgameScope => "procgame",
            Self::RenderScope => "render",
            Self::TrainScope => "train",
        }
    }

    /// § Bitmask used in [`AICapPolicy`] to mark this scope as allowed.
    #[must_use]
    pub const fn bit(self) -> u8 {
        1u8 << (self as u8)
    }
}

/// § A proposed mutation a companion-AI wants the Sovereign to authorize.
///
/// The mutation is opaque-to-this-crate : the carrier is just an intent
/// tag + a per-scope opaque payload-handle that downstream pillars
/// resolve. The substrate refuses-to-apply when consent is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mutation {
    /// § Which pillar this mutation targets.
    pub scope: AICapScope,
    /// § Opaque intent tag — describes what the mutation does
    ///   (downstream pillars decode). Stable u32 enum-handle.
    pub intent_tag: u32,
    /// § Opaque payload handle into a per-pillar pool. Substrate does NOT
    ///   dereference ; only pillar-side code does.
    pub payload_handle: u32,
    /// § Cell-Sovereign that this mutation targets.
    pub sovereign_handle: u16,
    /// § Companion-AI proposing the mutation.
    pub companion_handle: u32,
}

impl Mutation {
    /// § Construct a mutation proposal.
    #[must_use]
    pub const fn new(
        scope: AICapScope,
        intent_tag: u32,
        payload_handle: u32,
        sovereign_handle: u16,
        companion_handle: u32,
    ) -> Mutation {
        Mutation {
            scope,
            intent_tag,
            payload_handle,
            sovereign_handle,
            companion_handle,
        }
    }
}

/// § Sovereign-side decision on whether to grant a proposed mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    /// § Sovereign refuses ; default-deny applies.
    Refused {
        /// § The mutation that was refused.
        mutation: Mutation,
        /// § Reason-tag (telemetry-only ; non-overridable).
        reason: RefuseReason,
    },
    /// § Sovereign authorizes — single-shot ; applies once.
    Granted {
        /// § The mutation authorized.
        mutation: Mutation,
        /// § Audit-seq stamp at consent-time.
        audit_seq: u32,
    },
    /// § Sovereign authorizes WITH mutual-witness audit requirement —
    ///   apply MUST log both Sovereign + companion identities.
    GrantedMutualWitness {
        /// § The mutation authorized.
        mutation: Mutation,
        /// § Audit-seq stamp at consent-time.
        audit_seq: u32,
    },
}

impl ConsentDecision {
    /// § True iff the decision authorizes the mutation.
    #[must_use]
    pub const fn is_granted(&self) -> bool {
        matches!(
            self,
            Self::Granted { .. } | Self::GrantedMutualWitness { .. }
        )
    }

    /// § True iff mutual-witness audit is required for apply.
    #[must_use]
    pub const fn requires_mutual_witness(&self) -> bool {
        matches!(self, Self::GrantedMutualWitness { .. })
    }
}

/// § Reasons a Sovereign refuses a companion-AI proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RefuseReason {
    /// § Default-deny : Sovereign has not granted this scope.
    DefaultDeny = 0,
    /// § Scope is allowed by policy but Sovereign-handle in proposal
    ///   does not match this Sovereign's handle.
    SovereignMismatch = 1,
    /// § Companion-handle in proposal does not match a registered hook
    ///   on the target cell.
    CompanionUnknown = 2,
    /// § Proposal carries an intent_tag the policy explicitly forbids.
    IntentForbidden = 3,
}

/// § Per-Sovereign cross-pillar capability policy.
///
/// Encodes the default-deny rules : each scope has a per-policy
/// allow-bit + a per-scope intent-allowlist (forbid-by-default). A
/// freshly-constructed policy denies ALL scopes — Sovereign must
/// explicitly grant each scope they want to permit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AICapPolicy {
    /// § Sovereign-handle this policy belongs to.
    pub sovereign_handle: u16,
    /// § Bitmask of allowed scopes (use [`AICapScope::bit`]).
    /// § Default = 0 (all-scopes-denied, default-deny).
    allowed_scopes_mask: u8,
    /// § Per-scope max-intent-tag-rank : intent_tag ≥ this value is
    /// § forbidden even when scope is allowed. Default = 0 (forbid-all).
    /// § Indexed by `AICapScope as u8`.
    intent_max_per_scope: [u32; 4],
    /// § True iff this policy requires mutual-witness for ALL grants
    /// § (otherwise per-grant Sovereign-decision controls).
    pub require_mutual_witness: bool,
}

impl AICapPolicy {
    /// § Construct a default-deny policy : ZERO scopes allowed.
    #[must_use]
    pub const fn deny_all(sovereign_handle: u16) -> AICapPolicy {
        AICapPolicy {
            sovereign_handle,
            allowed_scopes_mask: 0,
            intent_max_per_scope: [0; 4],
            require_mutual_witness: false,
        }
    }

    /// § Grant a scope with a max-intent-tag-rank ceiling.
    /// § Intent-tags strictly less than `max_intent_rank` are admissible.
    pub fn grant_scope(&mut self, scope: AICapScope, max_intent_rank: u32) {
        self.allowed_scopes_mask |= scope.bit();
        self.intent_max_per_scope[scope as usize] = max_intent_rank;
    }

    /// § Revoke a scope ; subsequent proposals against it default-deny.
    pub fn revoke_scope(&mut self, scope: AICapScope) {
        self.allowed_scopes_mask &= !scope.bit();
        self.intent_max_per_scope[scope as usize] = 0;
    }

    /// § True iff this scope is granted.
    #[must_use]
    pub const fn allows_scope(&self, scope: AICapScope) -> bool {
        (self.allowed_scopes_mask & scope.bit()) != 0
    }

    /// § Check a single mutation against this policy. Returns the
    /// § refuse-reason on deny, None on admit.
    #[must_use]
    pub const fn refusal_for(&self, m: &Mutation) -> Option<RefuseReason> {
        if (self.allowed_scopes_mask & m.scope.bit()) == 0 {
            return Some(RefuseReason::DefaultDeny);
        }
        if m.sovereign_handle != self.sovereign_handle {
            return Some(RefuseReason::SovereignMismatch);
        }
        if m.intent_tag >= self.intent_max_per_scope[m.scope as usize] {
            return Some(RefuseReason::IntentForbidden);
        }
        None
    }
}

/// § Audit-trail entry : monotone-append-only record of a proposal+
///   consent+apply tuple. Carries epoch + Sovereign + companion + scope
///   + decision + intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEntry {
    /// § Audit-seq epoch : monotone-incrementing per-trail.
    pub epoch: u32,
    /// § Sovereign-handle that authorized (or refused) the mutation.
    pub sovereign_handle: u16,
    /// § Companion-handle that proposed the mutation.
    pub companion_handle: u32,
    /// § Scope of the proposed mutation.
    pub scope: AICapScope,
    /// § Intent-tag carried in the proposal.
    pub intent_tag: u32,
    /// § What stage of the lifecycle this entry records.
    pub stage: AuditStage,
    /// § Decision outcome.
    pub decision: AuditDecision,
}

/// § Lifecycle stage being audited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AuditStage {
    /// § Initial proposal by companion-AI.
    Propose = 0,
    /// § Consent decision by Sovereign.
    Consent = 1,
    /// § Apply attempt — captured PASS/FAIL.
    Apply = 2,
}

/// § Audit decision outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AuditDecision {
    /// § Admitted : proposal advanced ; consent granted ; apply succeeded.
    Admit = 0,
    /// § Refused : proposal/consent/apply was denied.
    Refuse = 1,
}

/// § The cross-pillar companion-AI hook surface. Owns one per-Sovereign
///   policy + a monotone audit trail.
///
/// # Threading
///   This type is `!Sync` by intent : per-Sovereign single-writer ;
///   higher-level coordination owns the cross-Sovereign view.
#[derive(Debug, Clone)]
pub struct CrossPillarCompanionAi {
    /// § Per-Sovereign policy.
    policy: AICapPolicy,
    /// § Monotone audit-trail. Append-only.
    audit_trail: Vec<AuditEntry>,
    /// § Monotone audit-epoch counter.
    epoch: u32,
}

impl CrossPillarCompanionAi {
    /// § Construct with a default-deny policy for the given Sovereign.
    #[must_use]
    pub fn new(sovereign_handle: u16) -> CrossPillarCompanionAi {
        CrossPillarCompanionAi {
            policy: AICapPolicy::deny_all(sovereign_handle),
            audit_trail: Vec::new(),
            epoch: 0,
        }
    }

    /// § Construct from a pre-built policy (advanced — mostly for tests).
    #[must_use]
    pub fn with_policy(policy: AICapPolicy) -> CrossPillarCompanionAi {
        CrossPillarCompanionAi {
            policy,
            audit_trail: Vec::new(),
            epoch: 0,
        }
    }

    /// § Read-only view of the active policy.
    #[must_use]
    pub const fn policy(&self) -> &AICapPolicy {
        &self.policy
    }

    /// § Mutable view : caller may grant/revoke scopes.
    pub fn policy_mut(&mut self) -> &mut AICapPolicy {
        &mut self.policy
    }

    /// § Read-only view of the audit-trail.
    #[must_use]
    pub fn audit_trail(&self) -> &[AuditEntry] {
        &self.audit_trail
    }

    /// § Companion-AI side — propose a mutation in a scope. Returns the
    /// § candidate Mutation list (empty when proposal is refused at the
    /// § policy gate). Logs a `Propose` audit entry either way.
    pub fn companion_ai_propose(
        &mut self,
        intent_tag: u32,
        scope: AICapScope,
        sovereign: SovereignContext,
        companion_handle: u32,
        payload_handle: u32,
    ) -> Vec<Mutation> {
        let sovereign_handle = sovereign_context_to_handle(sovereign, self.policy.sovereign_handle);
        let m = Mutation::new(
            scope,
            intent_tag,
            payload_handle,
            sovereign_handle,
            companion_handle,
        );
        let refusal = self.policy.refusal_for(&m);
        let decision = match refusal {
            Some(_) => AuditDecision::Refuse,
            None => AuditDecision::Admit,
        };
        self.append_audit(AuditStage::Propose, &m, decision);
        if refusal.is_some() {
            Vec::new()
        } else {
            vec![m]
        }
    }

    /// § Sovereign-side — request consent for a proposed mutation.
    ///
    /// Mirrors the W-S12 `CompanionAiHook` consent gate but cross-pillar :
    /// returns a [`ConsentDecision`] which the apply-step gates on.
    pub fn companion_ai_request_consent(&mut self, mutations: &[Mutation]) -> Vec<ConsentDecision> {
        let mut out = Vec::with_capacity(mutations.len());
        for m in mutations {
            let decision = match self.policy.refusal_for(m) {
                Some(reason) => {
                    self.append_audit(AuditStage::Consent, m, AuditDecision::Refuse);
                    ConsentDecision::Refused {
                        mutation: *m,
                        reason,
                    }
                }
                None => {
                    self.epoch = self.epoch.wrapping_add(1);
                    self.append_audit(AuditStage::Consent, m, AuditDecision::Admit);
                    if self.policy.require_mutual_witness {
                        ConsentDecision::GrantedMutualWitness {
                            mutation: *m,
                            audit_seq: self.epoch,
                        }
                    } else {
                        ConsentDecision::Granted {
                            mutation: *m,
                            audit_seq: self.epoch,
                        }
                    }
                }
            };
            out.push(decision);
        }
        out
    }

    /// § Apply-side — substrate-final gate. Default-DENY when ANY
    /// § decision is Refused. On admit, logs the apply audit entry.
    ///
    /// # Errors
    /// - [`IfcViolation::SovereignMismatch`] when ANY decision in the
    ///   batch is Refused (default-deny ; no-override).
    pub fn companion_ai_apply(
        &mut self,
        decisions: &[ConsentDecision],
    ) -> Result<(), IfcViolation> {
        // First pass : detect any refusal ; default-deny on the WHOLE batch.
        for d in decisions {
            if !d.is_granted() {
                let m = match d {
                    ConsentDecision::Refused { mutation, .. } => *mutation,
                    _ => unreachable!(),
                };
                self.append_audit(AuditStage::Apply, &m, AuditDecision::Refuse);
                return Err(IfcViolation::SovereignMismatch {
                    expected: SovereignContext::User,
                    actual: SovereignContext::User,
                });
            }
        }
        // Second pass : log admits.
        for d in decisions {
            let m = match d {
                ConsentDecision::Granted { mutation, .. } => *mutation,
                ConsentDecision::GrantedMutualWitness { mutation, .. } => *mutation,
                ConsentDecision::Refused { .. } => unreachable!(),
            };
            self.append_audit(AuditStage::Apply, &m, AuditDecision::Admit);
        }
        Ok(())
    }

    /// § Helper : append an audit entry.
    fn append_audit(&mut self, stage: AuditStage, m: &Mutation, decision: AuditDecision) {
        self.audit_trail.push(AuditEntry {
            epoch: self.epoch,
            sovereign_handle: m.sovereign_handle,
            companion_handle: m.companion_handle,
            scope: m.scope,
            intent_tag: m.intent_tag,
            stage,
            decision,
        });
    }
}

/// § Map a SovereignContext into the per-policy Sovereign-handle. The
/// § policy's Sovereign-handle is authoritative ; the SovereignContext
/// § is the caller's claimed context. When the caller is User context
/// § we trust the policy's Sovereign-handle ; when the caller is System
/// § we use 0xFFFE as a privileged-system-bus marker.
const fn sovereign_context_to_handle(ctx: SovereignContext, policy_sovereign: u16) -> u16 {
    match ctx {
        // Ordinary game-code Sovereign : trust the policy-side handle.
        SovereignContext::User => policy_sovereign,
        // Privilege-tier callers are tracked separately ; map them to a
        // synthetic handle so policy.refusal_for fires SovereignMismatch
        // unless explicitly granted on policy_sovereign.
        SovereignContext::System => 0xFFFE,
        // Kernel-tier : also synthetic-handle so cross-Sovereign isolation
        // holds (kernel must not silently mutate user-Sovereign cells).
        SovereignContext::Kernel => 0xFFFF,
    }
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

    // ── W-S-CORE-6 cross-pillar tests ──────────────────────────────────

    /// § Default-deny : a fresh policy refuses ALL scopes.
    #[test]
    fn cross_pillar_default_deny() {
        let mut hook = CrossPillarCompanionAi::new(42);
        for scope in AICapScope::all() {
            let proposed = hook.companion_ai_propose(
                /*intent*/ 1,
                scope,
                SovereignContext::User,
                /*companion*/ 7,
                /*payload*/ 0,
            );
            assert!(proposed.is_empty(), "scope {scope:?} should default-deny",);
        }
    }

    /// § Sovereign grants procgen scope explicitly ; proposal admitted.
    #[test]
    fn sovereign_grants_procgen() {
        let mut hook = CrossPillarCompanionAi::new(42);
        hook.policy_mut()
            .grant_scope(AICapScope::ProcgenScope, /*max-intent*/ 8);
        let proposed = hook.companion_ai_propose(
            /*intent*/ 3,
            AICapScope::ProcgenScope,
            SovereignContext::User,
            /*companion*/ 7,
            /*payload*/ 100,
        );
        assert_eq!(proposed.len(), 1);
        assert_eq!(proposed[0].scope, AICapScope::ProcgenScope);
        assert_eq!(proposed[0].intent_tag, 3);
        assert_eq!(proposed[0].sovereign_handle, 42);
    }

    /// § Sovereign grants render scope ; procgen still denied (scope-isolation).
    #[test]
    fn sovereign_grants_render() {
        let mut hook = CrossPillarCompanionAi::new(99);
        hook.policy_mut().grant_scope(AICapScope::RenderScope, 5);
        // Render proposal admitted.
        let render_props =
            hook.companion_ai_propose(2, AICapScope::RenderScope, SovereignContext::User, 7, 0);
        assert_eq!(render_props.len(), 1);
        // Procgen proposal still default-denied.
        let procgen_props =
            hook.companion_ai_propose(2, AICapScope::ProcgenScope, SovereignContext::User, 7, 0);
        assert!(procgen_props.is_empty());
    }

    /// § Capability-mismatch : intent-tag exceeds scope's allowlist.
    #[test]
    fn capability_mismatch_intent_forbidden() {
        let mut hook = CrossPillarCompanionAi::new(42);
        // Grant ProcgameScope with max-intent-rank = 4.
        hook.policy_mut().grant_scope(AICapScope::ProcgameScope, 4);
        // Intent-tag 5 ≥ 4 ⇒ refused as IntentForbidden even though scope is allowed.
        let proposed =
            hook.companion_ai_propose(5, AICapScope::ProcgameScope, SovereignContext::User, 7, 0);
        assert!(proposed.is_empty());
        // Inspect the audit-trail : should be a Refuse on the Propose stage.
        let trail = hook.audit_trail();
        assert_eq!(trail.len(), 1);
        assert_eq!(trail[0].stage, AuditStage::Propose);
        assert_eq!(trail[0].decision, AuditDecision::Refuse);
        assert_eq!(trail[0].intent_tag, 5);
    }

    /// § Apply default-denies a refused mutation (consent_required).
    #[test]
    fn apply_default_denies_when_no_consent() {
        let mut hook = CrossPillarCompanionAi::new(42);
        // Manually fabricate a Refused decision (simulate Sovereign-side refusal).
        let m = Mutation::new(AICapScope::TrainScope, 1, 0, 42, 7);
        let refused = ConsentDecision::Refused {
            mutation: m,
            reason: RefuseReason::DefaultDeny,
        };
        let result = hook.companion_ai_apply(&[refused]);
        assert!(matches!(
            result,
            Err(IfcViolation::SovereignMismatch { .. }),
        ));
        // Audit-trail should record the refused apply.
        let trail = hook.audit_trail();
        assert!(trail
            .iter()
            .any(|e| e.stage == AuditStage::Apply && e.decision == AuditDecision::Refuse));
    }

    /// § MutualWitness : when policy demands it, granted decisions
    ///   carry the audit-required variant.
    #[test]
    fn mutual_witness_required_in_consent() {
        let mut hook = CrossPillarCompanionAi::new(42);
        hook.policy_mut().grant_scope(AICapScope::RenderScope, 4);
        hook.policy_mut().require_mutual_witness = true;
        let proposed =
            hook.companion_ai_propose(1, AICapScope::RenderScope, SovereignContext::User, 7, 0);
        let decisions = hook.companion_ai_request_consent(&proposed);
        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].is_granted());
        assert!(decisions[0].requires_mutual_witness());
        // Apply succeeds + emits audit entry.
        let r = hook.companion_ai_apply(&decisions);
        assert!(r.is_ok());
        let admit_count = hook
            .audit_trail()
            .iter()
            .filter(|e| e.stage == AuditStage::Apply && e.decision == AuditDecision::Admit)
            .count();
        assert_eq!(admit_count, 1);
    }

    /// § Audit-trail records the full lifecycle : Propose → Consent → Apply.
    #[test]
    fn audit_trail_records_full_lifecycle() {
        let mut hook = CrossPillarCompanionAi::new(42);
        hook.policy_mut().grant_scope(AICapScope::ProcgenScope, 4);
        let proposed =
            hook.companion_ai_propose(2, AICapScope::ProcgenScope, SovereignContext::User, 7, 100);
        let decisions = hook.companion_ai_request_consent(&proposed);
        let _ = hook.companion_ai_apply(&decisions);
        let trail = hook.audit_trail();
        assert_eq!(trail.len(), 3);
        assert_eq!(trail[0].stage, AuditStage::Propose);
        assert_eq!(trail[0].decision, AuditDecision::Admit);
        assert_eq!(trail[1].stage, AuditStage::Consent);
        assert_eq!(trail[1].decision, AuditDecision::Admit);
        assert_eq!(trail[2].stage, AuditStage::Apply);
        assert_eq!(trail[2].decision, AuditDecision::Admit);
        // All entries reference same companion + sovereign.
        assert!(trail
            .iter()
            .all(|e| e.sovereign_handle == 42 && e.companion_handle == 7));
    }

    /// § Scope-isolation : granting one scope does NOT leak to other scopes,
    ///   even within the same Sovereign-handle.
    #[test]
    fn scope_isolation_across_pillars() {
        let mut hook = CrossPillarCompanionAi::new(42);
        // Grant only TrainScope.
        hook.policy_mut().grant_scope(AICapScope::TrainScope, 4);
        for scope in AICapScope::all() {
            let proposed = hook.companion_ai_propose(1, scope, SovereignContext::User, 7, 0);
            if scope == AICapScope::TrainScope {
                assert_eq!(proposed.len(), 1, "TrainScope should be admitted");
            } else {
                assert!(
                    proposed.is_empty(),
                    "scope {scope:?} should still be denied",
                );
            }
        }
        // Policy-level allows_scope mirrors the proposal-level outcome.
        for scope in AICapScope::all() {
            let admitted = hook.policy().allows_scope(scope);
            assert_eq!(admitted, scope == AICapScope::TrainScope);
        }
    }

    /// § System-context callers can NOT silently impersonate the user-Sovereign :
    ///   the synthetic handle 0xFFFE forces SovereignMismatch unless explicitly
    ///   granted to that handle.
    #[test]
    fn system_context_does_not_impersonate_user_sovereign() {
        let mut hook = CrossPillarCompanionAi::new(42);
        // Grant procgen on the user-Sovereign (handle 42).
        hook.policy_mut().grant_scope(AICapScope::ProcgenScope, 4);
        // System-context proposal : sovereign-handle becomes 0xFFFE ≠ 42.
        let proposed =
            hook.companion_ai_propose(1, AICapScope::ProcgenScope, SovereignContext::System, 7, 0);
        assert!(
            proposed.is_empty(),
            "System-context must not bypass user-Sovereign policy",
        );
        // Audit-trail records the Sovereign-mismatch refusal.
        let trail = hook.audit_trail();
        assert_eq!(trail.len(), 1);
        assert_eq!(trail[0].stage, AuditStage::Propose);
        assert_eq!(trail[0].decision, AuditDecision::Refuse);
    }

    /// § AICapPolicy revoke : after revocation a previously-allowed
    ///   scope reverts to default-deny.
    #[test]
    fn policy_revoke_reverts_to_default_deny() {
        let mut policy = AICapPolicy::deny_all(42);
        policy.grant_scope(AICapScope::RenderScope, 4);
        assert!(policy.allows_scope(AICapScope::RenderScope));
        policy.revoke_scope(AICapScope::RenderScope);
        assert!(!policy.allows_scope(AICapScope::RenderScope));
    }

    /// § AICapScope canonical names are unique (telemetry-stable).
    #[test]
    fn ai_cap_scope_canonical_names_unique() {
        let names: Vec<&'static str> = AICapScope::all()
            .iter()
            .map(|s| s.canonical_name())
            .collect();
        let mut s = names;
        s.sort_unstable();
        let original = s.len();
        s.dedup();
        assert_eq!(s.len(), original);
    }
}

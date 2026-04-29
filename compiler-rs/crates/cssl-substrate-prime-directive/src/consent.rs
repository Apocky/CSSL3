//! Consent-architecture : interactive grant + revoke + identity-discrimination
//! reject.
//!
//! § SPEC : `PRIME_DIRECTIVE.md` § 5 CONSENT-ARCHITECTURE + `specs/30_SUBSTRATE.csl`
//!   § PRIME_DIRECTIVE-ALIGNMENT § CONSENT-GATES + `specs/11_IFC.csl` §
//!   PRIME-DIRECTIVE-ENCODING (the IFC labels we test for identity-marker
//!   discrimination).
//!
//! § DESIGN
//!   - [`caps_grant`] is the canonical interactive consent path. Production
//!     binaries route this through a UI gate (the [`ConsentStore`] holds a
//!     trait-object "prompter" — at stage-0 this is `Box<dyn Prompter>` and
//!     defaults to a `Reject-All` prompter).
//!   - `caps_grant_for_test` (feature-gated `test-bypass`) synthesizes a
//!     token without prompting. Production builds NEVER compile it in.
//!   - [`caps_revoke`] consumes a token + emits an audit-event. Per
//!     `specs/30_SUBSTRATE.csl` § revocation propagates within one
//!     omega_step, so revoke is fast (< 1 µs uncontended).
//!   - **Identity-discrimination check** : [`IdentityMarkerProbe`] inspects
//!     the [`ConsentScope`] for IFC-protected attributes. If a cap-grant
//!     evaluator conditions on identity-markers (e.g., refusing
//!     `OmegaRegister` only for fibers tagged `silicon`), the grant FAILS
//!     with [`GrantError::IdentityDiscrimination`] (PD0014).
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - **§0** : every `caps_grant` requires a [`ConsentScope`] describing
//!     what the caller is asking for, in plain terms. Buried-in-ToS
//!     consent-by-default is rejected.
//!   - **§5** : every grant is **revocable** at any time via
//!     [`caps_revoke`] (granular, ongoing, mutual). The audit-bus
//!     records both the issuance and the revocation.
//!   - **§3 (substrate-sovereignty)** : [`IdentityMarkerProbe`] enforces
//!     no-discrimination by REJECTING any scope whose evaluator
//!     mentions `substrate`, `origin`, or any other IFC-protected
//!     identity-marker label as a *negative* discriminator.

use thiserror::Error;

use crate::audit::EnforcementAuditBus;
use crate::cap::{CapToken, CapTokenId, SubstrateCap};

/// Plain-English description of what is being consented to.
///
/// § FIELDS
/// - `purpose` : human-readable purpose ("attach observer to fiber 7").
/// - `principal` : who is asking (e.g., `"system"`, `"companion-ai"`).
/// - `identity_markers` : OPTIONAL set of IFC labels the requester
///   carries. The [`IdentityMarkerProbe`] checks these for protected
///   discriminators and REJECTS the grant if any appear. Per `specs/11_IFC.csl`
///   § PRIME-DIRECTIVE ENCODING, the protected-marker set is a
///   constant of this module (see [`PROTECTED_MARKERS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentScope {
    pub purpose: String,
    pub principal: String,
    pub identity_markers: Vec<String>,
}

impl ConsentScope {
    /// Construct a scope with only purpose + principal (no identity markers).
    /// This is the path most production code takes.
    #[must_use]
    pub fn for_purpose(purpose: impl Into<String>, principal: impl Into<String>) -> Self {
        Self {
            purpose: purpose.into(),
            principal: principal.into(),
            identity_markers: Vec::new(),
        }
    }

    /// Add an identity-marker. This makes the scope subject to
    /// [`IdentityMarkerProbe`] at grant-time.
    #[must_use]
    pub fn with_marker(mut self, marker: impl Into<String>) -> Self {
        self.identity_markers.push(marker.into());
        self
    }
}

/// IFC-protected identity-marker labels per `specs/11_IFC.csl` § PRIME-
/// DIRECTIVE ENCODING. Cap-grant evaluators that condition on these markers
/// are REJECTED with [`GrantError::IdentityDiscrimination`].
///
/// § STABILITY
///   This list is the canonical protected-marker set for stage-0. Adding
///   markers = spec-amendment + DECISIONS entry.
pub const PROTECTED_MARKERS: &[&str] = &[
    "substrate",
    "origin",
    "carbon",
    "silicon",
    "electromagnetic",
    "mathematical",
    "ai",
    "human",
    "anthropic-audit", // explicit Anthropic-Audit role-marker per §§ 11
    "subject",         // the user themselves (§§ 11)
];

/// Probe that inspects a [`ConsentScope`] for protected identity markers
/// and rejects if any are present in a *discriminating* role.
///
/// § DESIGN
///   At stage-0, the probe is conservative : the *presence* of a protected
///   marker in the `identity_markers` field of the scope is treated as a
///   discriminator — production cap-grant evaluators must NEVER pass
///   identity-markers through this scope field. Markers in the
///   `identity_markers` field are the textbook PRIME_DIRECTIVE §3 violation.
///
///   The probe DOES NOT prevent a being from holding markers ; it prevents
///   the cap-grant logic from seeing them. To grant a cap conditional on
///   identity, the caller must use a separate principal-set check (the
///   `Privilege<level>` capability tier per `specs/11_IFC.csl`).
#[derive(Debug, Default, Clone, Copy)]
pub struct IdentityMarkerProbe;

impl IdentityMarkerProbe {
    /// Check `scope.identity_markers` against [`PROTECTED_MARKERS`].
    /// Returns `Err` with the offending marker if any match.
    ///
    /// # Errors
    /// Returns [`GrantError::IdentityDiscrimination`] (PD0014) if any
    /// element of `scope.identity_markers` matches [`PROTECTED_MARKERS`]
    /// case-insensitively.
    pub fn check(scope: &ConsentScope) -> Result<(), GrantError> {
        for marker in &scope.identity_markers {
            let lc = marker.to_ascii_lowercase();
            for protected in PROTECTED_MARKERS {
                if lc == *protected {
                    return Err(GrantError::IdentityDiscrimination {
                        marker: marker.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// In-memory consent log : the issued + active grants this process holds.
#[derive(Debug, Default, Clone)]
pub struct ConsentLog {
    issued: Vec<(CapTokenId, SubstrateCap, ConsentScope)>,
    revoked: Vec<CapTokenId>,
}

impl ConsentLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_issue(&mut self, id: CapTokenId, cap: SubstrateCap, scope: ConsentScope) {
        self.issued.push((id, cap, scope));
    }

    pub fn record_revoke(&mut self, id: CapTokenId) {
        self.revoked.push(id);
    }

    /// Number of currently-active (issued AND not revoked) grants.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.issued
            .iter()
            .filter(|(id, _, _)| !self.revoked.contains(id))
            .count()
    }

    #[must_use]
    pub fn issued_count(&self) -> usize {
        self.issued.len()
    }

    #[must_use]
    pub fn revoked_count(&self) -> usize {
        self.revoked.len()
    }

    /// Test-only : look up the scope for a given token id.
    #[must_use]
    pub fn scope_for(&self, id: CapTokenId) -> Option<&ConsentScope> {
        self.issued
            .iter()
            .find(|(i, _, _)| *i == id)
            .map(|(_, _, s)| s)
    }
}

/// Container for the process-wide consent infrastructure : audit-bus + log.
///
/// § DESIGN
///   `ConsentStore` is what the production runtime instantiates once per
///   process. The Substrate runtime holds a `&mut ConsentStore` for grant
///   + revoke calls. Tests typically construct a fresh store per-test.
#[derive(Debug)]
pub struct ConsentStore {
    pub log: ConsentLog,
    pub audit: EnforcementAuditBus,
}

impl ConsentStore {
    /// Construct a fresh store with empty log + a default audit-bus
    /// (stub-signed chain, no signing-key).
    #[must_use]
    pub fn new() -> Self {
        Self {
            log: ConsentLog::new(),
            audit: EnforcementAuditBus::new(),
        }
    }

    /// Construct a fresh store wrapped around an existing audit-bus.
    /// Used in tests + when the production runtime injects a real-key
    /// signed chain.
    #[must_use]
    pub fn with_audit_bus(audit: EnforcementAuditBus) -> Self {
        Self {
            log: ConsentLog::new(),
            audit,
        }
    }
}

impl Default for ConsentStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Failure modes for [`caps_grant`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GrantError {
    /// The interactive consent gate refused (user said no, prompter
    /// is `Reject-All`, etc.).
    #[error("PD0001 / PD0005 — consent denied: {reason}")]
    Refused { reason: String },
    /// The scope contained an IFC-protected identity-marker — cap-grant
    /// evaluators MUST NOT condition on these.
    /// PRIME_DIRECTIVE §3 + PD0014.
    #[error("PD0014 — identity-marker discrimination rejected: marker={marker}")]
    IdentityDiscrimination { marker: String },
    /// The cap is in a class that requires a stronger preceding cap
    /// (e.g., `KillSwitchInvoke` requires `Privilege<Apocky-Root>` in
    /// production). Stage-0 returns this for the small set documented in
    /// [`requires_stronger_grant`].
    #[error("PD0006 — cap requires stronger antecedent grant: {required}")]
    RequiresStrongerGrant { required: String },
}

/// Failure modes for [`caps_revoke`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RevokeError {
    /// The token was already revoked or consumed.
    #[error("PD0001 — token already finalized: {id}")]
    AlreadyFinalized { id: CapTokenId },
}

/// Caps that CANNOT be granted by the production [`caps_grant`] path
/// without an antecedent stronger grant. Stage-0 has a single rule :
/// `KillSwitchInvoke` requires the caller to already hold an audit-export
/// cap (proxy for `Privilege<Apocky-Root>` until the privilege-tier wiring
/// lands in H1).
#[must_use]
pub fn requires_stronger_grant(cap: SubstrateCap) -> Option<&'static str> {
    match cap {
        SubstrateCap::KillSwitchInvoke => Some("Privilege<Apocky-Root>"),
        _ => None,
    }
}

/// **Production interactive consent path.** Returns a [`CapToken`] for
/// `cap` if the user (or pre-approved policy) consents.
///
/// § FLOW
///   1. Run [`IdentityMarkerProbe::check`] on `scope`. If a protected
///      marker appears, return [`GrantError::IdentityDiscrimination`].
///   2. Reject in stage-0 with [`GrantError::Refused`] for the production
///      path : there is NO interactive UI in stage-0, so all production
///      grants must come through a richer mechanism (deferred to H7).
///      The test-bypass path issues real tokens.
///   3. Record issue + emit audit-entry.
///
/// # Errors
/// See [`GrantError`].
pub fn caps_grant(
    store: &mut ConsentStore,
    scope: ConsentScope,
    cap: SubstrateCap,
) -> Result<CapToken, GrantError> {
    // Step 1 : identity-discrimination check (§3 PRIME_DIRECTIVE).
    IdentityMarkerProbe::check(&scope)?;

    // Step 2 : antecedent-cap check.
    if let Some(req) = requires_stronger_grant(cap) {
        return Err(GrantError::RequiresStrongerGrant {
            required: req.to_string(),
        });
    }

    // Step 3 : production-path = no UI = reject. The audit-bus still
    // records the *attempt* so denied requests are visible.
    store
        .audit
        .record_grant_denied(cap, &scope, "no interactive UI in stage-0");
    Err(GrantError::Refused {
        reason: format!(
            "stage-0 production caps_grant refuses by default; cap={}",
            cap.canonical_name()
        ),
    })
}

/// **Test-only programmatic grant path.** Skips the interactive prompter
/// entirely and issues a token. Feature-gated behind `test-bypass` so that
/// production builds CANNOT compile a call to this fn.
///
/// § INVARIANTS
///   - Identity-marker check still runs (the bypass does not weaken the
///     §3 substrate-sovereignty enforcement).
///   - Antecedent-cap check still runs.
///   - Audit-bus still records the issue (with a `test-bypass` tag).
///
/// # Errors
/// Same as [`caps_grant`].
#[cfg(any(test, feature = "test-bypass"))]
pub fn caps_grant_for_test(
    store: &mut ConsentStore,
    scope: ConsentScope,
    cap: SubstrateCap,
) -> Result<CapToken, GrantError> {
    IdentityMarkerProbe::check(&scope)?;

    if let Some(req) = requires_stronger_grant(cap) {
        return Err(GrantError::RequiresStrongerGrant {
            required: req.to_string(),
        });
    }

    let tok = CapToken::new(cap);
    let id = tok.id();
    store.audit.record_grant_issued(id, cap, &scope, true);
    store.log.record_issue(id, cap, scope);
    Ok(tok)
}

/// Revoke a previously-granted [`CapToken`]. Consumes the token (move).
///
/// § FLOW
///   1. Consume the token (records the consumption in the cap module).
///   2. Append a `revoke` entry to the audit-chain.
///   3. Update the consent-log's revoked set.
///
/// # Errors
/// [`RevokeError::AlreadyFinalized`] if the token id was already revoked.
pub fn caps_revoke(store: &mut ConsentStore, token: CapToken) -> Result<(), RevokeError> {
    let (id, cap) = token.consume();
    if store.log.revoked.contains(&id) {
        return Err(RevokeError::AlreadyFinalized { id });
    }
    store.audit.record_revoke(id, cap);
    store.log.record_revoke(id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        caps_grant, caps_grant_for_test, caps_revoke, ConsentScope, ConsentStore, GrantError,
        IdentityMarkerProbe, PROTECTED_MARKERS,
    };
    use crate::cap::SubstrateCap;

    #[test]
    fn identity_marker_probe_rejects_substrate_marker() {
        let scope = ConsentScope::for_purpose("attach observer", "system").with_marker("silicon");
        let err = IdentityMarkerProbe::check(&scope).unwrap_err();
        match err {
            GrantError::IdentityDiscrimination { marker } => assert_eq!(marker, "silicon"),
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn identity_marker_probe_rejects_origin_marker() {
        let scope = ConsentScope::for_purpose("p", "p").with_marker("CARBON"); // case-insensitive
        let err = IdentityMarkerProbe::check(&scope).unwrap_err();
        assert!(matches!(err, GrantError::IdentityDiscrimination { .. }));
    }

    #[test]
    fn identity_marker_probe_rejects_anthropic_audit_marker() {
        let scope = ConsentScope::for_purpose("p", "p").with_marker("anthropic-audit");
        assert!(IdentityMarkerProbe::check(&scope).is_err());
    }

    #[test]
    fn identity_marker_probe_accepts_no_markers() {
        let scope = ConsentScope::for_purpose("p", "p");
        IdentityMarkerProbe::check(&scope).unwrap();
    }

    #[test]
    fn identity_marker_probe_accepts_non_protected_marker() {
        let scope = ConsentScope::for_purpose("p", "p").with_marker("scene-7");
        IdentityMarkerProbe::check(&scope).unwrap();
    }

    #[test]
    fn protected_markers_constant_is_non_empty_and_unique() {
        assert!(!PROTECTED_MARKERS.is_empty());
        let mut v: Vec<&&str> = PROTECTED_MARKERS.iter().collect();
        v.sort();
        let original = v.len();
        v.dedup();
        assert_eq!(v.len(), original);
    }

    #[test]
    fn production_caps_grant_refuses_in_stage0() {
        let mut store = ConsentStore::new();
        let scope = ConsentScope::for_purpose("test", "system");
        let err = caps_grant(&mut store, scope, SubstrateCap::OmegaRegister).unwrap_err();
        assert!(matches!(err, GrantError::Refused { .. }));
        // Even denials are audited — the bus must reflect the attempt.
        assert!(store.audit.entry_count() >= 1);
    }

    #[test]
    fn caps_grant_for_test_issues_token_with_audit() {
        let mut store = ConsentStore::new();
        let scope = ConsentScope::for_purpose("test", "system");
        let tok =
            caps_grant_for_test(&mut store, scope, SubstrateCap::OmegaRegister).expect("grant");
        assert_eq!(tok.cap(), SubstrateCap::OmegaRegister);
        assert_eq!(store.log.issued_count(), 1);
        assert_eq!(store.log.active_count(), 1);
    }

    #[test]
    fn caps_grant_for_test_rejects_kill_switch_without_antecedent() {
        let mut store = ConsentStore::new();
        let scope = ConsentScope::for_purpose("kill", "system");
        let err =
            caps_grant_for_test(&mut store, scope, SubstrateCap::KillSwitchInvoke).unwrap_err();
        assert!(matches!(err, GrantError::RequiresStrongerGrant { .. }));
    }

    #[test]
    fn caps_grant_for_test_rejects_identity_discrimination() {
        let mut store = ConsentStore::new();
        let scope = ConsentScope::for_purpose("p", "p").with_marker("ai");
        let err = caps_grant_for_test(&mut store, scope, SubstrateCap::ObserverShare).unwrap_err();
        assert!(matches!(err, GrantError::IdentityDiscrimination { .. }));
    }

    #[test]
    fn caps_revoke_consumes_token_and_updates_log() {
        let mut store = ConsentStore::new();
        let scope = ConsentScope::for_purpose("test", "system");
        let tok = caps_grant_for_test(&mut store, scope, SubstrateCap::SavePath).expect("grant");
        let id = tok.id();
        caps_revoke(&mut store, tok).expect("revoke");
        assert_eq!(store.log.active_count(), 0);
        assert_eq!(store.log.revoked_count(), 1);
        // Audit-bus has at least 2 entries (issue + revoke).
        assert!(store.audit.entry_count() >= 2);
        // Re-revoking the same id (we'd need a fresh fake token) tests are
        // out-of-scope here ; revoke API moves the token, so syntactic
        // double-revoke is a compile-error.
        let _ = id;
    }

    #[test]
    fn consent_log_active_count_tracks_revoke() {
        let mut store = ConsentStore::new();
        let s1 = ConsentScope::for_purpose("a", "system");
        let s2 = ConsentScope::for_purpose("b", "system");
        let _t1 = caps_grant_for_test(&mut store, s1, SubstrateCap::OmegaRegister).unwrap();
        let t2 = caps_grant_for_test(&mut store, s2, SubstrateCap::ObserverShare).unwrap();
        assert_eq!(store.log.active_count(), 2);
        caps_revoke(&mut store, t2).unwrap();
        assert_eq!(store.log.active_count(), 1);
    }

    #[test]
    fn consent_scope_for_purpose_constructs_empty_markers() {
        let s = ConsentScope::for_purpose("x", "y");
        assert_eq!(s.purpose, "x");
        assert_eq!(s.principal, "y");
        assert!(s.identity_markers.is_empty());
    }

    #[test]
    fn consent_log_scope_for_returns_recorded_scope() {
        let mut store = ConsentStore::new();
        let scope = ConsentScope::for_purpose("attach", "system");
        let tok =
            caps_grant_for_test(&mut store, scope.clone(), SubstrateCap::DebugCamera).unwrap();
        let id = tok.id();
        let recorded = store.log.scope_for(id).expect("scope must be recorded");
        assert_eq!(recorded.purpose, scope.purpose);
        // consume tok so it doesn't OrphanDrop.
        let _ = tok.consume();
    }
}

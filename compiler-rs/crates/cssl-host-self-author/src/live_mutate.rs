// live_mutate.rs — LiveMutateGate : Σ-cap REQUIRED for live-mutate
// ══════════════════════════════════════════════════════════════════
// § ROLE
//   The ONLY surface that promotes a sandbox-validated CSSL-source into an
//   ACTUAL file-write on disk. Every promotion goes through `decide_mutate`
//   which consults a SovereignCap and the forbidden-target list. Successful
//   mutations are RECORDED on the Σ-Chain so the audit history is immutable.
//
//   ¬ live-mutate without :
//     - SovereignCap with EFFECT_WRITE bit
//     - cap.preflight() OK (signature verifies + not expired + not revoked)
//     - target_path NOT in FORBIDDEN_TARGETS
//     - sandbox-score ≥ threshold
//
// § REVOKE-CASCADE
//   When the sovereign revokes the SelfAuthorMutateCap, the gate transitions
//   every PendingApply → PendingRollback. Pending mutations on the host are
//   THEN reverted via the existing cssl-host-coder-runtime revert-window.
// ══════════════════════════════════════════════════════════════════

use crate::forbidden::is_forbidden_target;
use crate::sandbox_csslc::SandboxReport;
use cssl_substrate_sigma_chain::{record_attestation, Attestation, EntryKind, SigmaChain};
use cssl_substrate_sigma_runtime::{SovereignCap, EFFECT_WRITE};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

/// Cap-class discriminator. `SelfAuthorMutateCap` is the canonical name
/// surfaced through the public API ; structurally it's a thin wrapper around
/// the underlying [`SovereignCap`] that constrains the cap to carry the
/// `EFFECT_WRITE` bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfAuthorMutateCap {
    inner: SovereignCap,
}

impl SelfAuthorMutateCap {
    /// Wrap a SovereignCap. Returns `None` if the cap does not bear `EFFECT_WRITE`.
    /// This structural-wrapper enforces "self-author cap is necessarily a write-cap".
    #[must_use]
    pub fn from_cap(cap: SovereignCap) -> Option<Self> {
        if cap.permits_effect(EFFECT_WRITE) {
            Some(Self { inner: cap })
        } else {
            None
        }
    }

    /// Borrow the underlying cap (e.g. for preflight check).
    #[must_use]
    pub fn inner(&self) -> &SovereignCap {
        &self.inner
    }
}

/// Reasons the gate denied a mutate-attempt. All variants log + Σ-Chain-anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveMutateDecision {
    /// Approved : caller MAY proceed with file-write + revert-window arming.
    Allow,
    /// Sandbox quality-score below the configured threshold.
    DenyScoreBelowThreshold {
        /// Actual score.
        score: u8,
        /// Required threshold.
        threshold: u8,
    },
    /// Sandbox produced Fail / Rejected outcome.
    DenySandboxFailed,
    /// Target-path matches forbidden-target list.
    DenyForbiddenTarget(String),
    /// SovereignCap preflight failed (expired · revoked · sig invalid · malformed).
    DenyCapInvalid(&'static str),
    /// Cap was revoked during the apply window — caller must transition to PendingRollback.
    DenyCapRevoked,
    /// Cap is held but missing the sovereign-bit required by this kind (e.g. `System`).
    DenySovereignBitMissing,
}

impl LiveMutateDecision {
    /// Returns `true` iff this decision is `Allow`.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Stable string-tag for audit-log + Σ-Chain payload.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::DenyScoreBelowThreshold { .. } => "deny_score_below_threshold",
            Self::DenySandboxFailed => "deny_sandbox_failed",
            Self::DenyForbiddenTarget(_) => "deny_forbidden_target",
            Self::DenyCapInvalid(_) => "deny_cap_invalid",
            Self::DenyCapRevoked => "deny_cap_revoked",
            Self::DenySovereignBitMissing => "deny_sovereign_bit_missing",
        }
    }
}

/// Outcome of a complete mutate-cycle. Includes the decision plus, on Allow,
/// the seq_no of the Σ-Chain anchor entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutateOutcome {
    /// Gate decision.
    pub decision: LiveMutateDecision,
    /// Σ-Chain seq_no of the anchored attempt-record (every attempt anchored).
    pub sigma_chain_seq: Option<u64>,
}

/// Arguments to [`LiveMutateGate::decide_mutate`]. Passing as a struct keeps the
/// call-site readable — the security-critical fields are first-class named.
#[derive(Debug, Clone)]
pub struct MutateContext<'a> {
    /// Target file-path.
    pub target_path: &'a str,
    /// Sandbox quality-score 0..100.
    pub score: u8,
    /// Sandbox report (used for forbidden-effect propagation).
    pub report: &'a SandboxReport,
    /// Score threshold required for Allow.
    pub threshold: u8,
    /// Whether this kind requires sovereign-bit (e.g. `SelfAuthorKind::System`).
    pub requires_sovereign: bool,
    /// Whether the caller has presented the sovereign-bit (see CoderRuntime cap-system).
    pub sovereign_bit_held: bool,
    /// Wall-clock unix-second.
    pub now_unix: u64,
    /// Issuing-sovereign Ed25519 pubkey to verify the cap against.
    pub issuing_sovereign_pk: [u8; 32],
}

/// Live-mutate gate. Owns a non-shared reference to the Σ-Chain for anchoring.
pub struct LiveMutateGate<'a> {
    chain: &'a SigmaChain,
    chain_signer: &'a SigningKey,
    /// Set of cap-pubkeys currently revoked. The gate AND-narrows on this set.
    revoked_pubkeys: Vec<[u8; 32]>,
}

impl<'a> LiveMutateGate<'a> {
    /// Construct a fresh gate.
    #[must_use]
    pub fn new(chain: &'a SigmaChain, chain_signer: &'a SigningKey) -> Self {
        Self {
            chain,
            chain_signer,
            revoked_pubkeys: Vec::new(),
        }
    }

    /// Mark a cap-holder pubkey as revoked. Subsequent `decide_mutate` calls
    /// presenting a cap with this `holder_pubkey` are denied with `DenyCapRevoked`.
    /// Records a `CapRevoke` entry on the Σ-Chain.
    pub fn revoke_cap(&mut self, holder_pubkey: [u8; 32], now_unix: u64) {
        self.revoked_pubkeys.push(holder_pubkey);
        let _ = record_attestation(
            self.chain,
            self.chain_signer,
            &Attestation::new(EntryKind::CapRevoke, &holder_pubkey, now_unix),
        );
    }

    /// Returns `true` iff `holder_pubkey` has been revoked via this gate.
    #[must_use]
    pub fn is_revoked(&self, holder_pubkey: &[u8; 32]) -> bool {
        self.revoked_pubkeys.iter().any(|p| p == holder_pubkey)
    }

    /// Decide whether a candidate mutate is permitted. ALWAYS records a
    /// `AttestationAnchor` (Allow or Deny) entry on the Σ-Chain so the
    /// history is immutable.
    pub fn decide_mutate(
        &self,
        cap: &SelfAuthorMutateCap,
        ctx: &MutateContext<'_>,
    ) -> MutateOutcome {
        // 1. Forbidden-target gate (cheap · structural).
        if is_forbidden_target(ctx.target_path) {
            return self.anchor_and_finish(
                LiveMutateDecision::DenyForbiddenTarget(ctx.target_path.to_string()),
                ctx,
            );
        }
        // 2. Revoke-cascade : per-gate revoke-set.
        if self.is_revoked(&cap.inner().holder_pubkey) {
            return self.anchor_and_finish(LiveMutateDecision::DenyCapRevoked, ctx);
        }
        // 3. Cap preflight : signature + expiry + on-cap revocation_ref.
        if let Err(e) = cap.inner().preflight(&ctx.issuing_sovereign_pk, ctx.now_unix) {
            let tag: &'static str = match e {
                cssl_substrate_sigma_runtime::CapError::MalformedPublicKey => "malformed_pk",
                cssl_substrate_sigma_runtime::CapError::MalformedSignature => "malformed_sig",
                cssl_substrate_sigma_runtime::CapError::SignatureVerifyFailed => "sig_verify_failed",
                cssl_substrate_sigma_runtime::CapError::Expired { .. } => "expired",
                cssl_substrate_sigma_runtime::CapError::Revoked => "revoked",
            };
            return self.anchor_and_finish(LiveMutateDecision::DenyCapInvalid(tag), ctx);
        }
        // 4. Sandbox score-floor.
        if !matches!(
            ctx.report.compile,
            crate::sandbox_csslc::CompileOutcome::Pass { .. }
        ) {
            return self.anchor_and_finish(LiveMutateDecision::DenySandboxFailed, ctx);
        }
        if ctx.score < ctx.threshold {
            return self.anchor_and_finish(
                LiveMutateDecision::DenyScoreBelowThreshold {
                    score: ctx.score,
                    threshold: ctx.threshold,
                },
                ctx,
            );
        }
        // 5. Sovereign-bit requirement (e.g. for `System` kind).
        if ctx.requires_sovereign && !ctx.sovereign_bit_held {
            return self.anchor_and_finish(LiveMutateDecision::DenySovereignBitMissing, ctx);
        }
        // 6. Allow.
        self.anchor_and_finish(LiveMutateDecision::Allow, ctx)
    }

    fn anchor_and_finish(
        &self,
        decision: LiveMutateDecision,
        ctx: &MutateContext<'_>,
    ) -> MutateOutcome {
        let payload = AnchorPayload::from_decision(&decision, ctx);
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        let kind = if matches!(decision, LiveMutateDecision::Allow) {
            EntryKind::AttestationAnchor
        } else {
            EntryKind::CapRevoke
        };
        let res = record_attestation(
            self.chain,
            self.chain_signer,
            &Attestation::new(kind, &bytes, ctx.now_unix),
        );
        MutateOutcome {
            decision,
            sigma_chain_seq: res.ok().map(|r| r.seq_no),
        }
    }
}

/// Payload anchored on the Σ-Chain for each decision. JSON for forward-compat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AnchorPayload {
    target_path: String,
    score: u8,
    threshold: u8,
    decision: String,
    source_blake3_hex: String,
}

impl AnchorPayload {
    fn from_decision(decision: &LiveMutateDecision, ctx: &MutateContext<'_>) -> Self {
        let mut hex = String::with_capacity(64);
        for b in ctx.report.source_blake3 {
            hex.push_str(&format!("{b:02x}"));
        }
        Self {
            target_path: ctx.target_path.to_string(),
            score: ctx.score,
            threshold: ctx.threshold,
            decision: decision.as_str().to_string(),
            source_blake3_hex: hex,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox_csslc::{CompileOutcome, InlineTestReport, SandboxReport};
    use cssl_substrate_sigma_runtime::{AUDIENCE_DERIVED, EFFECT_READ};
    use ed25519_dalek::Signer;
    use rand::rngs::OsRng;

    fn make_signed_cap(
        sovereign: &SigningKey,
        holder_pk: [u8; 32],
        grants: u32,
        expires: u64,
    ) -> SovereignCap {
        let mut cap = SovereignCap::from_raw(
            holder_pk,
            grants,
            AUDIENCE_DERIVED,
            expires,
            None,
            [0u8; 64],
        );
        let sig = sovereign.sign(&cap.canonical_signing_bytes());
        cap.signature = sig.to_bytes();
        cap
    }

    fn pass_report() -> SandboxReport {
        SandboxReport {
            compile: CompileOutcome::Pass { warnings: 0 },
            tests: InlineTestReport {
                test_count: 1,
                assert_count: 1,
                forbidden_effect_matched: false,
            },
            source_blake3: [0xAB; 32],
        }
    }

    fn fail_report() -> SandboxReport {
        SandboxReport {
            compile: CompileOutcome::Fail {
                errors: vec!["E1".into()],
            },
            tests: InlineTestReport {
                test_count: 0,
                assert_count: 0,
                forbidden_effect_matched: false,
            },
            source_blake3: [0xAB; 32],
        }
    }

    fn ctx<'a>(
        report: &'a SandboxReport,
        sovereign_pk: [u8; 32],
        target: &'a str,
        score: u8,
    ) -> MutateContext<'a> {
        MutateContext {
            target_path: target,
            score,
            report,
            threshold: 75,
            requires_sovereign: false,
            sovereign_bit_held: false,
            now_unix: 1_700_000_000,
            issuing_sovereign_pk: sovereign_pk,
        }
    }

    #[test]
    fn t01_self_author_mutate_cap_requires_write_bit() {
        let sov = SigningKey::generate(&mut OsRng);
        let read_only = make_signed_cap(&sov, [1; 32], EFFECT_READ, 0);
        assert!(SelfAuthorMutateCap::from_cap(read_only).is_none());
        let writeable = make_signed_cap(&sov, [1; 32], EFFECT_WRITE, 0);
        assert!(SelfAuthorMutateCap::from_cap(writeable).is_some());
    }

    #[test]
    fn t02_allow_happy_path() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [1; 32], EFFECT_WRITE, 0)).unwrap();
        let report = pass_report();
        let outcome = gate.decide_mutate(
            &cap,
            &ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/scenes/forest.cssl", 100),
        );
        assert_eq!(outcome.decision, LiveMutateDecision::Allow);
        assert!(outcome.sigma_chain_seq.is_some());
    }

    #[test]
    fn t03_forbidden_target_denied() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [1; 32], EFFECT_WRITE, 0)).unwrap();
        let report = pass_report();
        let outcome = gate.decide_mutate(
            &cap,
            &ctx(&report, sov.verifying_key().to_bytes(), "compiler-rs/crates/csslc/src/lib.rs", 100),
        );
        assert!(matches!(outcome.decision, LiveMutateDecision::DenyForbiddenTarget(_)));
    }

    #[test]
    fn t04_score_below_threshold_denied() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [1; 32], EFFECT_WRITE, 0)).unwrap();
        let report = pass_report();
        let outcome = gate.decide_mutate(
            &cap,
            &ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/scenes/x.cssl", 50),
        );
        assert!(matches!(
            outcome.decision,
            LiveMutateDecision::DenyScoreBelowThreshold { score: 50, threshold: 75 }
        ));
    }

    #[test]
    fn t05_sandbox_failed_denied() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [1; 32], EFFECT_WRITE, 0)).unwrap();
        let report = fail_report();
        let outcome = gate.decide_mutate(
            &cap,
            &ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/scenes/x.cssl", 100),
        );
        assert_eq!(outcome.decision, LiveMutateDecision::DenySandboxFailed);
    }

    #[test]
    fn t06_revoke_cascades_subsequent_calls() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let mut gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [9; 32], EFFECT_WRITE, 0)).unwrap();
        let report = pass_report();
        // First call : Allow.
        let pre = gate.decide_mutate(
            &cap,
            &ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/scenes/x.cssl", 100),
        );
        assert_eq!(pre.decision, LiveMutateDecision::Allow);
        // Revoke.
        gate.revoke_cap([9; 32], 1_700_000_001);
        // Subsequent call : DenyCapRevoked.
        let post = gate.decide_mutate(
            &cap,
            &ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/scenes/x.cssl", 100),
        );
        assert_eq!(post.decision, LiveMutateDecision::DenyCapRevoked);
    }

    #[test]
    fn t07_invalid_signature_denied() {
        let real_sov = SigningKey::generate(&mut OsRng);
        let imposter = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&imposter, [1; 32], EFFECT_WRITE, 0)).unwrap();
        let report = pass_report();
        let outcome = gate.decide_mutate(
            &cap,
            &ctx(&report, real_sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/scenes/x.cssl", 100),
        );
        assert!(matches!(
            outcome.decision,
            LiveMutateDecision::DenyCapInvalid("sig_verify_failed")
        ));
    }

    #[test]
    fn t08_sovereign_bit_required_for_system_kind() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let chain_signer = SigningKey::generate(&mut OsRng);
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [1; 32], EFFECT_WRITE, 0)).unwrap();
        let report = pass_report();
        let mut c = ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/systems/economy.csl", 100);
        c.requires_sovereign = true;
        c.sovereign_bit_held = false;
        let outcome = gate.decide_mutate(&cap, &c);
        assert_eq!(outcome.decision, LiveMutateDecision::DenySovereignBitMissing);
        // With sovereign-bit held → Allow.
        let mut c = ctx(&report, sov.verifying_key().to_bytes(), "Labyrinth of Apocalypse/systems/economy.csl", 100);
        c.requires_sovereign = true;
        c.sovereign_bit_held = true;
        let outcome = gate.decide_mutate(&cap, &c);
        assert_eq!(outcome.decision, LiveMutateDecision::Allow);
    }
}

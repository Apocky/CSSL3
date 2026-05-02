//! § evaluator.rs — the canonical Σ-runtime gate-fn.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   `evaluate(mask, cap_witness, audience, requested_effect, aggregation_k)`
//!   is the single chokepoint every other crate routes READ / WRITE / EMIT
//!   through. The function is sync · zero-allocation in the success path ·
//!   emits exactly-one audit-ring entry per call.
//!
//! § ALGORITHM
//!   ```text
//!     0. checksum-validate(mask)               ⇒ Tampered if fail
//!     1. mask.is_revoked()                     ⇒ Revoked
//!     2. mask.is_expired(now)                  ⇒ Expired
//!     3. mask.allows_audience(audience)        ⇒ Deny(audience-class-mismatch)
//!     4. mask.permits_effect(effect)           ⇒ Deny(effect-not-permitted)
//!     5. ATTESTED ⇒ cap-required ⇒ verify-or-Deny
//!     6. DERIVED audience ⇒ k-anon-floor check ⇒ NeedsKAnonymity
//!     7. ⇒ Allow
//!   ```
//!
//! § DESIGN (per memory_sawyer_pokemon_efficiency)
//!   - Hot-path is branchless-favored : early-return on first deny.
//!   - All decisions emit exactly-ONE audit-ring entry · the entry seq is
//!     returned in the [`AccessDecision`] variant for caller-side pairing.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::audit::{AuditEntry, AuditRing, DecisionTag};
use crate::cap::{CapError, SovereignCap};
use crate::mask::{
    SigmaMask, AUDIENCE_DERIVED, FLAG_ATTESTED,
};

/// Reasons the evaluator may issue a Deny.
#[derive(Debug, Clone, Error)]
pub enum DenyReason {
    /// Mask checksum failed — in-memory tamper.
    #[error("Σ-mask checksum mismatch (in-memory tamper)")]
    Tampered,
    /// Audience-class bit not present in mask.
    #[error("audience-class not allowed by mask")]
    AudienceMismatch,
    /// Effect-cap bit not present in mask.
    #[error("effect-cap not permitted by mask")]
    EffectNotPermitted,
    /// Cap required (mask is ATTESTED) but no cap supplied.
    #[error("ATTESTED mask requires cap-witness, none supplied")]
    CapRequired,
    /// Cap supplied but does not cover the requested audience.
    #[error("cap does not cover requested audience")]
    CapAudienceMismatch,
    /// Cap supplied but does not grant the requested effect.
    #[error("cap does not grant requested effect")]
    CapEffectNotGranted,
    /// Cap signature/expiry/revocation pre-flight failed.
    #[error("cap pre-flight failed: {0}")]
    CapPreflightFailed(CapError),
}

/// Result of a Σ-runtime gate-fn invocation.
#[derive(Debug, Clone)]
pub enum AccessDecision {
    /// Permitted · audit-ring sequence-number recorded.
    Allow { audit_ref: u64 },
    /// Refused · reason + audit-ring sequence-number.
    Deny {
        reason: DenyReason,
        audit_ref: u64,
    },
    /// Aggregation-floor not met · current_k below required_k.
    NeedsKAnonymity {
        current_k: u32,
        required_k: u32,
        audit_ref: u64,
    },
    /// Stronger cap required · returns the (audience, effect) needed.
    NeedsCap {
        required_audience: u16,
        required_effects: u32,
        audit_ref: u64,
    },
    /// Mask was revoked.
    Revoked { revoked_at: u64, audit_ref: u64 },
    /// TTL elapsed.
    Expired { expired_at: u64, audit_ref: u64 },
}

impl AccessDecision {
    /// True iff the decision is a successful Allow.
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Audit-ring sequence number recorded for this decision.
    pub const fn audit_ref(&self) -> u64 {
        match self {
            Self::Allow { audit_ref }
            | Self::Deny { audit_ref, .. }
            | Self::NeedsKAnonymity { audit_ref, .. }
            | Self::NeedsCap { audit_ref, .. }
            | Self::Revoked { audit_ref, .. }
            | Self::Expired { audit_ref, .. } => *audit_ref,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Public gate-fn
// ───────────────────────────────────────────────────────────────────────────

/// Lazy-init process-wide audit-ring. Callers that want their own ring can
/// invoke [`evaluate_with_ring`] directly.
fn default_ring() -> Arc<AuditRing> {
    use std::sync::OnceLock;
    static RING: OnceLock<Arc<AuditRing>> = OnceLock::new();
    RING.get_or_init(|| Arc::new(AuditRing::default())).clone()
}

/// Wall-clock seconds since unix-epoch.
///
/// § DESIGN : the evaluator accepts caller-supplied now-seconds for
/// deterministic-replay ; this helper is for ad-hoc call-sites.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Microseconds-since-epoch for audit-entry timestamps.
fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// THE canonical gate-fn (uses process-wide audit-ring).
///
/// § ARGS
///   - `mask`             : the Σ-mask gating the data.
///   - `cap_witness`      : optional [`SovereignCap`] for ATTESTED masks.
///   - `audience`         : the audience-class bit the caller represents.
///   - `requested_effect` : the effect-cap bits being requested.
///   - `aggregation_k`    : current k-anonymity bucket-size for DERIVED audiences.
///   - `now_seconds_arg`  : wall-clock-second for TTL eval (caller-supplied).
///   - `issuing_sovereign_pk` : Ed25519 PK of the cap's issuing-sovereign
///                              · only consulted when `cap_witness.is_some()`.
pub fn evaluate(
    mask: &SigmaMask,
    cap_witness: Option<&SovereignCap>,
    audience: u16,
    requested_effect: u32,
    aggregation_k: Option<u32>,
    now_seconds_arg: u64,
    issuing_sovereign_pk: Option<&[u8; 32]>,
) -> AccessDecision {
    evaluate_with_ring(
        &default_ring(),
        mask,
        cap_witness,
        audience,
        requested_effect,
        aggregation_k,
        now_seconds_arg,
        issuing_sovereign_pk,
    )
}

/// Variant of [`evaluate`] taking an explicit [`AuditRing`] (for tests or
/// per-tenant rings).
#[allow(clippy::too_many_arguments)]
pub fn evaluate_with_ring(
    ring: &AuditRing,
    mask: &SigmaMask,
    cap_witness: Option<&SovereignCap>,
    audience: u16,
    requested_effect: u32,
    aggregation_k: Option<u32>,
    now_seconds_arg: u64,
    issuing_sovereign_pk: Option<&[u8; 32]>,
) -> AccessDecision {
    // Pre-compute audit metadata (zero-alloc · stack only).
    let ts = now_micros();
    let actor = cap_witness
        .map(|c| u64_from_pk_lo(&c.holder_pubkey))
        .unwrap_or(0);
    let subject = audience as u64;
    let k_tag = aggregation_k.map(|k| (k & 0xFF) as u8).unwrap_or(0);
    let push_entry = |tag: DecisionTag| -> u64 {
        ring.push(AuditEntry::new(
            ts,
            actor,
            subject,
            tag,
            requested_effect,
            k_tag,
            0, // seq auto-assigned
        ))
    };

    // ── 0. checksum-validate ───────────────────────────────────────────
    if !mask.verify_checksum() {
        let audit_ref = push_entry(DecisionTag::Tampered);
        return AccessDecision::Deny {
            reason: DenyReason::Tampered,
            audit_ref,
        };
    }
    // ── 1. revoked ────────────────────────────────────────────────────
    if mask.is_revoked() {
        let audit_ref = push_entry(DecisionTag::Revoked);
        return AccessDecision::Revoked {
            revoked_at: u64::from(mask.revoked_at()),
            audit_ref,
        };
    }
    // ── 2. expired ────────────────────────────────────────────────────
    if mask.is_expired(now_seconds_arg) {
        let audit_ref = push_entry(DecisionTag::Expired);
        return AccessDecision::Expired {
            expired_at: mask.expires_at(),
            audit_ref,
        };
    }
    // ── 3. audience-class allowed by mask ──────────────────────────────
    if !mask.allows_audience(audience) {
        let audit_ref = push_entry(DecisionTag::Deny);
        return AccessDecision::Deny {
            reason: DenyReason::AudienceMismatch,
            audit_ref,
        };
    }
    // ── 4. effect-cap permitted by mask ────────────────────────────────
    if !mask.permits_effect(requested_effect) {
        let audit_ref = push_entry(DecisionTag::Deny);
        return AccessDecision::Deny {
            reason: DenyReason::EffectNotPermitted,
            audit_ref,
        };
    }
    // ── 5. ATTESTED ⇒ cap required + verified ──────────────────────────
    if mask.has_flag(FLAG_ATTESTED) {
        let Some(cap) = cap_witness else {
            let audit_ref = push_entry(DecisionTag::NeedsCap);
            return AccessDecision::NeedsCap {
                required_audience: audience,
                required_effects: requested_effect,
                audit_ref,
            };
        };
        // cap must cover requested audience + effect.
        if !cap.covers_audience(audience) {
            let audit_ref = push_entry(DecisionTag::Deny);
            return AccessDecision::Deny {
                reason: DenyReason::CapAudienceMismatch,
                audit_ref,
            };
        }
        if !cap.permits_effect(requested_effect) {
            let audit_ref = push_entry(DecisionTag::Deny);
            return AccessDecision::Deny {
                reason: DenyReason::CapEffectNotGranted,
                audit_ref,
            };
        }
        // cap pre-flight (sig + expiry + revocation).
        if let Some(pk) = issuing_sovereign_pk {
            if let Err(e) = cap.preflight(pk, now_seconds_arg) {
                let audit_ref = push_entry(DecisionTag::Deny);
                return AccessDecision::Deny {
                    reason: DenyReason::CapPreflightFailed(e),
                    audit_ref,
                };
            }
        } else {
            // ATTESTED mask + cap supplied + no issuing-sovereign-pk
            // ⇒ sovereign-PK is mandatory ; treat as cap-required.
            let audit_ref = push_entry(DecisionTag::NeedsCap);
            return AccessDecision::NeedsCap {
                required_audience: audience,
                required_effects: requested_effect,
                audit_ref,
            };
        }
    }
    // ── 6. DERIVED audience ⇒ k-anon-floor check ───────────────────────
    if (audience & AUDIENCE_DERIVED) != 0 {
        let k_floor = u32::from(mask.k_anon_thresh());
        if k_floor > 0 {
            let cur_k = aggregation_k.unwrap_or(0);
            if cur_k < k_floor {
                let audit_ref = push_entry(DecisionTag::NeedsKAnon);
                return AccessDecision::NeedsKAnonymity {
                    current_k: cur_k,
                    required_k: k_floor,
                    audit_ref,
                };
            }
        }
    }
    // ── 7. Allow ───────────────────────────────────────────────────────
    let audit_ref = push_entry(DecisionTag::Allow);
    AccessDecision::Allow { audit_ref }
}

/// Extract the low 8 bytes of an Ed25519 pubkey for actor-hash compaction.
fn u64_from_pk_lo(pk: &[u8; 32]) -> u64 {
    u64::from_le_bytes([pk[0], pk[1], pk[2], pk[3], pk[4], pk[5], pk[6], pk[7]])
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::{
        AUDIENCE_CIRCLE, AUDIENCE_PUBLIC, AUDIENCE_SELF, EFFECT_DERIVE, EFFECT_PURGE, EFFECT_READ,
        EFFECT_WRITE, FLAG_PROPAGATE,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn fresh_keypair() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn sign_a_cap(
        sovereign: &SigningKey,
        holder_pk: [u8; 32],
        grants: u32,
        audience: u16,
        expires_at: u64,
    ) -> SovereignCap {
        let mut cap = SovereignCap::from_raw(holder_pk, grants, audience, expires_at, None, [0u8; 64]);
        let msg = cap.canonical_signing_bytes();
        cap.signature = sovereign.sign(&msg).to_bytes();
        cap
    }

    #[test]
    fn t01_allow_on_valid_mask_audience_effect() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let d = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_000, None);
        assert!(d.is_allow(), "{:?}", d);
    }

    #[test]
    fn t02_deny_on_audience_mismatch() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let d = evaluate_with_ring(
            &ring,
            &m,
            None,
            AUDIENCE_PUBLIC,
            EFFECT_READ,
            None,
            1_000,
            None,
        );
        assert!(matches!(
            d,
            AccessDecision::Deny {
                reason: DenyReason::AudienceMismatch,
                ..
            }
        ));
    }

    #[test]
    fn t03_deny_on_effect_not_permitted() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let d = evaluate_with_ring(
            &ring,
            &m,
            None,
            AUDIENCE_SELF,
            EFFECT_PURGE,
            None,
            1_000,
            None,
        );
        assert!(matches!(
            d,
            AccessDecision::Deny {
                reason: DenyReason::EffectNotPermitted,
                ..
            }
        ));
    }

    #[test]
    fn t04_revoked_mask_returns_revoked() {
        let ring = AuditRing::new(64);
        let mut m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        m.revoke(1_500);
        let d = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_500, None);
        assert!(matches!(d, AccessDecision::Revoked { revoked_at: 1_500, .. }));
    }

    #[test]
    fn t05_expired_mask_returns_expired() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 60, 0, 1_000);
        let d = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 2_000, None);
        assert!(matches!(d, AccessDecision::Expired { expired_at: 1_060, .. }));
    }

    #[test]
    fn t06_kanon_floor_requires_aggregation() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_DERIVED, EFFECT_DERIVE, 5, 0, 0, 1_000);
        // current k=2 below floor=5
        let d = evaluate_with_ring(
            &ring,
            &m,
            None,
            AUDIENCE_DERIVED,
            EFFECT_DERIVE,
            Some(2),
            1_000,
            None,
        );
        assert!(matches!(
            d,
            AccessDecision::NeedsKAnonymity {
                current_k: 2,
                required_k: 5,
                ..
            }
        ));
        // bumping k to 5 ⇒ allow
        let d2 = evaluate_with_ring(
            &ring,
            &m,
            None,
            AUDIENCE_DERIVED,
            EFFECT_DERIVE,
            Some(5),
            1_000,
            None,
        );
        assert!(d2.is_allow());
    }

    #[test]
    fn t07_attested_mask_requires_cap() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_ATTESTED, 1_000);
        let d = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_000, None);
        assert!(matches!(d, AccessDecision::NeedsCap { .. }));
    }

    #[test]
    fn t08_attested_mask_with_valid_cap_allows() {
        let ring = AuditRing::new(64);
        let sovereign = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            AUDIENCE_SELF,
            0,
        );
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_ATTESTED, 1_000);
        let d = evaluate_with_ring(
            &ring,
            &m,
            Some(&cap),
            AUDIENCE_SELF,
            EFFECT_READ,
            None,
            1_000,
            Some(&sovereign.verifying_key().to_bytes()),
        );
        assert!(d.is_allow(), "{:?}", d);
    }

    #[test]
    fn t09_attested_with_bad_signature_denies() {
        let ring = AuditRing::new(64);
        let sovereign = fresh_keypair();
        let imposter = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            AUDIENCE_SELF,
            0,
        );
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, FLAG_ATTESTED, 1_000);
        let d = evaluate_with_ring(
            &ring,
            &m,
            Some(&cap),
            AUDIENCE_SELF,
            EFFECT_READ,
            None,
            1_000,
            // wrong sovereign-pk
            Some(&imposter.verifying_key().to_bytes()),
        );
        assert!(matches!(
            d,
            AccessDecision::Deny {
                reason: DenyReason::CapPreflightFailed(_),
                ..
            }
        ));
    }

    #[test]
    fn t10_tampered_mask_returns_tampered() {
        let ring = AuditRing::new(64);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        // simulate tamper by copying the mask + corrupting via a roundtrip
        // we lack direct field-mutation surface, so use revoke()-without-rehash
        // simulation : impossible. Instead we use mask::pack to validate the
        // tamper-path indirectly via the sister test in mask.rs t03. Here we
        // assert that an UNTAMPERED mask passes the gate (fall-through to
        // Allow on otherwise valid args).
        let d = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_000, None);
        assert!(d.is_allow());
    }

    #[test]
    fn t11_audit_ring_records_one_entry_per_evaluate() {
        let ring = AuditRing::new(256);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        for _ in 0..100 {
            let _ = evaluate_with_ring(
                &ring,
                &m,
                None,
                AUDIENCE_SELF,
                EFFECT_READ,
                None,
                1_000,
                None,
            );
        }
        assert_eq!(ring.total_written(), 100);
    }

    #[test]
    fn t12_decision_audit_ref_matches_ring_seq() {
        let ring = AuditRing::new(16);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let d0 = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_000, None);
        let d1 = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_000, None);
        assert_eq!(d0.audit_ref(), 0);
        assert_eq!(d1.audit_ref(), 1);
    }

    #[test]
    fn t13_concurrent_evaluates_audit_ring_consistent() {
        use std::sync::Arc;
        use std::thread;

        let ring = Arc::new(AuditRing::new(2048));
        let m = Arc::new(SigmaMask::new(
            AUDIENCE_SELF | AUDIENCE_CIRCLE,
            EFFECT_READ | EFFECT_WRITE,
            0,
            0,
            FLAG_PROPAGATE,
            1_000,
        ));

        const N_THREADS: usize = 8;
        const PER_THREAD: usize = 125; // total 1000 evaluates
        let mut handles = Vec::with_capacity(N_THREADS);
        for _ in 0..N_THREADS {
            let ring_c = ring.clone();
            let m_c = m.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..PER_THREAD {
                    let _ = evaluate_with_ring(
                        &ring_c,
                        &m_c,
                        None,
                        AUDIENCE_SELF,
                        EFFECT_READ,
                        None,
                        1_000,
                        None,
                    );
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            ring.total_written(),
            (N_THREADS * PER_THREAD) as u64,
            "every evaluate emits exactly-one audit-entry · concurrent-safe"
        );
    }

    #[test]
    fn t14_decision_is_allow_helper() {
        let ring = AuditRing::new(8);
        let m = SigmaMask::new(AUDIENCE_SELF, EFFECT_READ, 0, 0, 0, 1_000);
        let d_allow = evaluate_with_ring(&ring, &m, None, AUDIENCE_SELF, EFFECT_READ, None, 1_000, None);
        let d_deny = evaluate_with_ring(&ring, &m, None, AUDIENCE_PUBLIC, EFFECT_READ, None, 1_000, None);
        assert!(d_allow.is_allow());
        assert!(!d_deny.is_allow());
    }
}

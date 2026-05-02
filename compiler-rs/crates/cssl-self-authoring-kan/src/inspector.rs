//! § inspector.rs — Σ-mask-gated read-only API for admin-dashboard.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § BiasInspector
//!   Default-deny per-row read API. Every read goes through the Σ-runtime
//!   gate-fn ([`cssl_substrate_sigma_runtime::evaluate`]) requiring :
//!     - audience-class includes [`AUDIENCE_ADMIN`]
//!     - effect-cap includes [`EFFECT_READ`]
//!     - if mask is `Derived`-class : k-anon-floor must be ≥ inspector's
//!       view of the cell's distinct-player-count.
//!
//! § INTEGRATION
//!   The admin-dashboard (analytics-pipeline downstream) holds a SovereignCap
//!   issued by the operator's wallet-PK. It calls
//!   [`BiasInspector::read_cell`] / [`BiasInspector::read_top_for_archetype`]
//!   which gate the read through Σ-runtime, emit an audit-ring entry, and
//!   return either the bias-row or a typed [`InspectorError`].

use thiserror::Error;

use cssl_substrate_sigma_runtime::{
    evaluate, AccessDecision, DenyReason, SigmaMask, SovereignCap, AUDIENCE_ADMIN, EFFECT_READ,
};

use crate::bias_map::{ArchetypeId, TemplateBiasMap, TemplateId};
use crate::reservoir::Reservoir;

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InspectorError {
    /// Σ-runtime gate-fn returned Deny.
    #[error("Σ-runtime denied : {0:?}")]
    SigmaDenied(String),
    /// Σ-runtime gate-fn returned NeedsKAnonymity ; cell does not have
    /// enough distinct contributors to surface.
    #[error("k-anonymity floor not met : current_k={current_k}, required_k={required_k}")]
    NeedsKAnonymity { current_k: u32, required_k: u32 },
    /// Σ-runtime gate-fn returned NeedsCap ; caller must present a cap.
    #[error("attested mask requires SovereignCap, none supplied")]
    NeedsCap,
    /// Σ-runtime gate-fn returned Revoked.
    #[error("Σ-mask is revoked : revoked_at={0}")]
    Revoked(u64),
    /// Σ-runtime gate-fn returned Expired.
    #[error("Σ-mask is expired : expired_at={0}")]
    Expired(u64),
}

impl InspectorError {
    fn from_decision(d: &AccessDecision) -> Self {
        match d {
            AccessDecision::Deny { reason, .. } => match reason {
                DenyReason::AudienceMismatch => Self::SigmaDenied("AudienceMismatch".to_string()),
                DenyReason::EffectNotPermitted => Self::SigmaDenied("EffectNotPermitted".to_string()),
                DenyReason::CapAudienceMismatch => Self::SigmaDenied("CapAudienceMismatch".to_string()),
                DenyReason::CapEffectNotGranted => Self::SigmaDenied("CapEffectNotGranted".to_string()),
                DenyReason::CapPreflightFailed(_) => Self::SigmaDenied("CapPreflightFailed".to_string()),
                DenyReason::CapRequired => Self::SigmaDenied("CapRequired".to_string()),
                DenyReason::Tampered => Self::SigmaDenied("Tampered".to_string()),
            },
            AccessDecision::NeedsKAnonymity {
                current_k,
                required_k,
                ..
            } => Self::NeedsKAnonymity {
                current_k: *current_k,
                required_k: *required_k,
            },
            AccessDecision::NeedsCap { .. } => Self::NeedsCap,
            AccessDecision::Revoked { revoked_at, .. } => Self::Revoked(*revoked_at),
            AccessDecision::Expired { expired_at, .. } => Self::Expired(*expired_at),
            AccessDecision::Allow { .. } => Self::SigmaDenied("UNREACHABLE-allow".to_string()),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § BiasInspector
// ───────────────────────────────────────────────────────────────────────────

/// Read-only Σ-mask-gated view onto a `TemplateBiasMap`.
///
/// § DESIGN
///   The inspector borrows the bias-map + reservoir immutably ; every read
///   routes through `cssl_substrate_sigma_runtime::evaluate` which emits an
///   audit-ring-entry. Failed-gate calls return `InspectorError` ; the audit
///   trail records the deny independent of the caller's error-handling.
pub struct BiasInspector<'a> {
    bias_map: &'a TemplateBiasMap,
    reservoir: &'a Reservoir,
}

impl<'a> BiasInspector<'a> {
    pub fn new(bias_map: &'a TemplateBiasMap, reservoir: &'a Reservoir) -> Self {
        Self {
            bias_map,
            reservoir,
        }
    }

    /// Read a single (template · archetype) bias-row through the Σ-runtime gate.
    ///
    /// § ARGS
    ///   - `mask` : Σ-mask gating the inspector. Must allow AUDIENCE_ADMIN +
    ///              EFFECT_READ.
    ///   - `cap` : optional SovereignCap (required if mask is ATTESTED).
    ///   - `now_seconds` : caller-supplied wall-clock-second for TTL eval.
    ///   - `issuing_pk` : Ed25519 PK of the cap's issuing-sovereign (if cap supplied).
    ///   - `template`, `archetype` : the cell to read.
    pub fn read_cell(
        &self,
        mask: &SigmaMask,
        cap: Option<&SovereignCap>,
        now_seconds: u64,
        issuing_pk: Option<&[u8; 32]>,
        template: TemplateId,
        archetype: ArchetypeId,
    ) -> Result<i16, InspectorError> {
        // The cell's k-anon current-count is the distinct-player-count in the
        // reservoir for the cell ; passed to evaluate() so it can gate-derived
        // audiences against the mask's k_anon_thresh.
        let current_k = self.reservoir.distinct_players_for_cell(template, archetype);
        let decision = evaluate(
            mask,
            cap,
            AUDIENCE_ADMIN,
            EFFECT_READ,
            Some(current_k),
            now_seconds,
            issuing_pk,
        );
        match decision {
            AccessDecision::Allow { .. } => Ok(self.bias_map.priority_shift(template, archetype)),
            other => Err(InspectorError::from_decision(&other)),
        }
    }

    /// Read top-N templates for an archetype through the Σ-runtime gate.
    pub fn read_top_for_archetype(
        &self,
        mask: &SigmaMask,
        cap: Option<&SovereignCap>,
        now_seconds: u64,
        issuing_pk: Option<&[u8; 32]>,
        archetype: ArchetypeId,
        top_n: usize,
    ) -> Result<Vec<(TemplateId, i16)>, InspectorError> {
        // Aggregate-read uses a single Σ-runtime gate-call ; current_k is the
        // MAX distinct-player-count across all cells for the archetype.
        let mut max_k = 0u32;
        for r in self.reservoir.iter_records() {
            if r.archetype_id == archetype {
                let k = self.reservoir.distinct_players_for_cell(r.template_id, archetype);
                if k > max_k {
                    max_k = k;
                }
            }
        }
        let decision = evaluate(
            mask,
            cap,
            AUDIENCE_ADMIN,
            EFFECT_READ,
            Some(max_k),
            now_seconds,
            issuing_pk,
        );
        match decision {
            AccessDecision::Allow { .. } => Ok(self.bias_map.top_for_archetype(archetype, top_n)),
            other => Err(InspectorError::from_decision(&other)),
        }
    }

    /// Total cell-count : doesn't expose per-row data, so always allowed
    /// without a Σ-mask (aggregate-only-statistic).
    pub fn cell_count(&self) -> usize {
        self.bias_map.cell_count()
    }

    /// Snapshot of the bias-map's update-counter. Aggregate-only-statistic.
    pub fn update_count(&self) -> u64 {
        self.bias_map.update_count()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bias_map::KanBiasUpdate;
    use crate::reservoir::PlayerHandle;
    use crate::signal::QualitySignal;
    use cssl_substrate_sigma_runtime::{
        AUDIENCE_PUBLIC, EFFECT_WRITE,
    };

    fn admin_read_mask(now_seconds: u64) -> SigmaMask {
        // audience = ADMIN, effect = READ, k_anon = 0 (no k-anon gate for this mask),
        // ttl = 0 (no expiry), flags = 0 (not attested), created_at = now_seconds.
        SigmaMask::new(AUDIENCE_ADMIN, EFFECT_READ, 0, 0, 0, now_seconds)
    }

    fn public_read_mask(now_seconds: u64) -> SigmaMask {
        SigmaMask::new(AUDIENCE_PUBLIC, EFFECT_READ, 0, 0, 0, now_seconds)
    }

    fn admin_write_mask(now_seconds: u64) -> SigmaMask {
        // permits write but NOT read ⇒ read should be denied.
        SigmaMask::new(AUDIENCE_ADMIN, EFFECT_WRITE, 0, 0, 0, now_seconds)
    }

    #[test]
    fn admin_with_read_mask_can_read_cell() {
        let mut bm = TemplateBiasMap::new();
        bm.apply_update(KanBiasUpdate::new(TemplateId(5), ArchetypeId(0), 1234, 0))
            .unwrap();
        let rsv = Reservoir::new(0);
        let insp = BiasInspector::new(&bm, &rsv);
        let mask = admin_read_mask(1_000_000);
        let v = insp
            .read_cell(&mask, None, 1_000_000, None, TemplateId(5), ArchetypeId(0))
            .unwrap();
        assert_eq!(v, 1234);
    }

    #[test]
    fn public_audience_cannot_read_admin_cell() {
        let bm = TemplateBiasMap::new();
        let rsv = Reservoir::new(0);
        let insp = BiasInspector::new(&bm, &rsv);
        let mask = public_read_mask(1_000_000);
        let r = insp.read_cell(&mask, None, 1_000_000, None, TemplateId(0), ArchetypeId(0));
        assert!(
            matches!(r, Err(InspectorError::SigmaDenied(_))),
            "public mask must be denied at admin-audience read : {:?}",
            r
        );
    }

    #[test]
    fn write_only_mask_denies_read() {
        let bm = TemplateBiasMap::new();
        let rsv = Reservoir::new(0);
        let insp = BiasInspector::new(&bm, &rsv);
        let mask = admin_write_mask(1_000_000);
        let r = insp.read_cell(&mask, None, 1_000_000, None, TemplateId(0), ArchetypeId(0));
        assert!(matches!(r, Err(InspectorError::SigmaDenied(_))));
    }

    #[test]
    fn aggregate_only_stats_bypass_sigma_gate() {
        let mut bm = TemplateBiasMap::new();
        bm.apply_update(KanBiasUpdate::new(TemplateId(0), ArchetypeId(0), 100, 0)).unwrap();
        let rsv = Reservoir::new(0);
        let insp = BiasInspector::new(&bm, &rsv);
        // No mask required for aggregate-only stats.
        assert_eq!(insp.cell_count(), 1);
        assert_eq!(insp.update_count(), 1);
    }

    #[test]
    fn inspector_top_for_archetype_through_admin_mask() {
        let mut bm = TemplateBiasMap::new();
        for (t, v) in [(TemplateId(1), 100), (TemplateId(2), 500), (TemplateId(3), 250)] {
            bm.apply_update(KanBiasUpdate::new(t, ArchetypeId(0), v, 0)).unwrap();
        }
        let mut rsv = Reservoir::new(0);
        // populate reservoir with a few signals for max-k computation.
        for p in 0..3u32 {
            rsv.ingest(crate::reservoir::QualitySignalRecord::new(
                TemplateId(1),
                ArchetypeId(0),
                QualitySignal::SandboxPass,
                PlayerHandle(p),
                0,
            ))
            .unwrap();
        }
        let insp = BiasInspector::new(&bm, &rsv);
        let mask = admin_read_mask(1_000_000);
        let top = insp
            .read_top_for_archetype(&mask, None, 1_000_000, None, ArchetypeId(0), 3)
            .unwrap();
        assert_eq!(top[0], (TemplateId(2), 500));
        assert_eq!(top[1], (TemplateId(3), 250));
        assert_eq!(top[2], (TemplateId(1), 100));
    }
}

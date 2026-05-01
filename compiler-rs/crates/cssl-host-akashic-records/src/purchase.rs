// § purchase.rs · AkashicLedger · purchase-flow + audit-emit
// § FREE auto-imprint @ Basic · paid imprint @ HighFid/Commissioned/Eternal/Tour
// § Eternal one-time-per (author · scene-name) · audit-emit-every-deduction

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::attribution::{AttributionLedger, AuthorPubkey};
use crate::fidelity::{FidelityTier, ShardCostConfig};
use crate::imprint::{
    AkashicError, Imprint, ImprintId, ImprintState, RevokedReason, SceneMeta, TtlToken,
};
use crate::shards::AethericShards;

/// Caller-supplied request to imprint an event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PurchaseRequest {
    pub author: AuthorPubkey,
    pub fidelity: FidelityTier,
    pub ts: u64,
    pub scene_metadata: SceneMeta,
    /// Required iff `fidelity == Commissioned` ; ignored otherwise.
    pub commissioned_narration: Option<String>,
}

/// Outcome of a purchase / imprint attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PurchaseOutcome {
    /// Imprint granted · contains the persisted imprint + new balance.
    Granted {
        imprint: Imprint,
        new_balance: AethericShards,
    },
    /// Insufficient shards (no state-change).
    InsufficientShards { have: u64, need: u64 },
    /// Eternal-attribution already-claimed for (author, scene-name).
    AlreadyOwnedEternal { original: ImprintId },
    /// Cosmetic-axiom violation rejected at-purchase-gate.
    CosmeticAxiomViolation { field: &'static str, reason: &'static str },
    /// Commissioned tier missing required narration.
    MissingNarration,
}

/// Audit-trail entry · audit-emit-every-deduction (per task-spec § 5).
///
/// Mock for `cssl-host-attestation` consumer · stub-flag for sibling crate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditEvent {
    BasicImprintFree {
        imprint_id: ImprintId,
        author: AuthorPubkey,
    },
    ShardsDeducted {
        imprint_id: ImprintId,
        author: AuthorPubkey,
        amount: u32,
        new_balance: u64,
        tier: FidelityTier,
    },
    EternalAttributionClaimed {
        imprint_id: ImprintId,
        author: AuthorPubkey,
        scene_name: String,
    },
    HistoricalTourTokenIssued {
        imprint_id: ImprintId,
        author: AuthorPubkey,
        ttl_secs: u64,
    },
    ImprintRevoked {
        imprint_id: ImprintId,
        reason: RevokedReason,
    },
    PurchaseRejected {
        author: AuthorPubkey,
        reason_kind: String,
    },
}

mod ledger_serde {
    //! Serde-shim : convert `BTreeMap<ImprintId, _>` to `Vec<_>` for JSON-compat.
    //! ImprintId is a transparent-newtype-u64 ; JSON-spec forbids number-map-keys.
    //! Converting to a value-array (since `id` is already inside [`Imprint`]) keeps
    //! deterministic iteration-order via `BTreeMap` at-rest while passing JSON.
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{AethericShards, AuthorPubkey, Imprint, ImprintId};

    pub(super) fn ser_imprints<S: Serializer>(
        m: &BTreeMap<ImprintId, Imprint>,
        ser: S,
    ) -> Result<S::Ok, S::Error> {
        let v: Vec<&Imprint> = m.values().collect();
        v.serialize(ser)
    }

    pub(super) fn de_imprints<'de, D: Deserializer<'de>>(
        de: D,
    ) -> Result<BTreeMap<ImprintId, Imprint>, D::Error> {
        let v: Vec<Imprint> = Vec::deserialize(de)?;
        Ok(v.into_iter().map(|i| (i.id, i)).collect())
    }

    pub(super) fn ser_balances<S: Serializer>(
        m: &BTreeMap<AuthorPubkey, AethericShards>,
        ser: S,
    ) -> Result<S::Ok, S::Error> {
        // Hex-string keys : AuthorPubkey serializes to hex-string, so a wrapped
        // BTreeMap<String, _> survives JSON.
        let mut as_str: BTreeMap<String, AethericShards> = BTreeMap::new();
        for (k, v) in m {
            as_str.insert(k.to_hex(), *v);
        }
        as_str.serialize(ser)
    }

    pub(super) fn de_balances<'de, D: Deserializer<'de>>(
        de: D,
    ) -> Result<BTreeMap<AuthorPubkey, AethericShards>, D::Error> {
        let as_str: BTreeMap<String, AethericShards> = BTreeMap::deserialize(de)?;
        let mut out = BTreeMap::new();
        for (k, v) in as_str {
            let pk = AuthorPubkey::from_hex(&k).map_err(serde::de::Error::custom)?;
            out.insert(pk, v);
        }
        Ok(out)
    }
}

/// Per-author balance + imprint storage + attribution ledger + audit-trail.
///
/// All maps are `BTreeMap` (deterministic-iteration · per task-spec hard-cap).
/// Maps are serialized via shim to remain JSON-compat (non-string keys).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AkashicLedger {
    next_id: u64,
    #[serde(serialize_with = "ledger_serde::ser_balances", deserialize_with = "ledger_serde::de_balances")]
    balances: BTreeMap<AuthorPubkey, AethericShards>,
    #[serde(serialize_with = "ledger_serde::ser_imprints", deserialize_with = "ledger_serde::de_imprints")]
    imprints: BTreeMap<ImprintId, Imprint>,
    attribution: AttributionLedger,
    config: ShardCostConfig,
    audit: Vec<AuditEvent>,
}

impl AkashicLedger {
    #[must_use]
    pub fn new(config: ShardCostConfig) -> Self {
        Self {
            next_id: 1,
            config,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn config(&self) -> &ShardCostConfig {
        &self.config
    }

    /// Set / replace per-author balance.
    pub fn set_balance(&mut self, author: AuthorPubkey, balance: AethericShards) {
        self.balances.insert(author, balance);
    }

    /// Top-up · checked-add · returns `BalanceOverflow` on overflow.
    ///
    /// # Errors
    /// Returns [`AkashicError::BalanceOverflow`] on `u64` overflow.
    pub fn credit(
        &mut self,
        author: AuthorPubkey,
        amount: AethericShards,
    ) -> Result<AethericShards, AkashicError> {
        let cur = self.balances.get(&author).copied().unwrap_or_default();
        let next = cur.checked_add(amount)?;
        self.balances.insert(author, next);
        Ok(next)
    }

    #[must_use]
    pub fn balance(&self, author: &AuthorPubkey) -> AethericShards {
        self.balances.get(author).copied().unwrap_or_default()
    }

    /// Read-only audit-trail.
    #[must_use]
    pub fn audit_trail(&self) -> &[AuditEvent] {
        &self.audit
    }

    /// Issue an imprint per the request · returns `PurchaseOutcome`.
    pub fn imprint(&mut self, req: PurchaseRequest) -> PurchaseOutcome {
        // 1. structural-prevalidate scene-metadata
        if let Err(AkashicError::CosmeticAxiomViolation { field, reason }) = req.scene_metadata.validate() {
            self.audit.push(AuditEvent::PurchaseRejected {
                author: req.author,
                reason_kind: "cosmetic-axiom".to_owned(),
            });
            return PurchaseOutcome::CosmeticAxiomViolation { field, reason };
        }

        // 2. Commissioned must include narration
        if matches!(req.fidelity, FidelityTier::Commissioned) && req.commissioned_narration.is_none()
        {
            self.audit.push(AuditEvent::PurchaseRejected {
                author: req.author,
                reason_kind: "missing-narration".to_owned(),
            });
            return PurchaseOutcome::MissingNarration;
        }
        if !matches!(req.fidelity, FidelityTier::Commissioned) && req.commissioned_narration.is_some() {
            self.audit.push(AuditEvent::PurchaseRejected {
                author: req.author,
                reason_kind: "narration-on-non-commissioned".to_owned(),
            });
            return PurchaseOutcome::CosmeticAxiomViolation {
                field: "commissioned_narration",
                reason: "narration provided on non-Commissioned tier",
            };
        }

        // 3. Eternal one-time gate — checked BEFORE balance-deduct
        if matches!(req.fidelity, FidelityTier::EternalAttribution) {
            if let Some(orig) = self
                .attribution
                .original_claim(&req.author, &req.scene_metadata.scene_name)
            {
                self.audit.push(AuditEvent::PurchaseRejected {
                    author: req.author,
                    reason_kind: "eternal-already-owned".to_owned(),
                });
                return PurchaseOutcome::AlreadyOwnedEternal { original: orig };
            }
        }

        // 4. Balance check
        let cost = self.config.cost_for(req.fidelity);
        let cur_balance = self.balances.get(&req.author).copied().unwrap_or_default();
        if cost > 0 && !cur_balance.covers(cost) {
            self.audit.push(AuditEvent::PurchaseRejected {
                author: req.author,
                reason_kind: "insufficient-shards".to_owned(),
            });
            return PurchaseOutcome::InsufficientShards {
                have: cur_balance.amount(),
                need: u64::from(cost),
            };
        }

        // 5. Deduct (if any) · checked-arithmetic
        let new_balance = if cost > 0 {
            match cur_balance.checked_sub(AethericShards::from(cost)) {
                Ok(b) => {
                    self.balances.insert(req.author, b);
                    b
                }
                Err(_) => {
                    // theoretically unreachable after `covers` check
                    self.audit.push(AuditEvent::PurchaseRejected {
                        author: req.author,
                        reason_kind: "balance-arithmetic".to_owned(),
                    });
                    return PurchaseOutcome::InsufficientShards {
                        have: cur_balance.amount(),
                        need: u64::from(cost),
                    };
                }
            }
        } else {
            cur_balance
        };

        // 6. Mint imprint
        let id = ImprintId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);

        let content_blake3 =
            Imprint::compute_content_hash(&req.scene_metadata, &req.author, req.ts);

        let ttl_token = if matches!(req.fidelity, FidelityTier::HistoricalReconstructionTour) {
            Some(TtlToken::new(req.ts, self.config.historical_tour_ttl_secs))
        } else {
            None
        };

        let imprint = Imprint {
            id,
            fidelity: req.fidelity,
            content_blake3,
            author_pubkey: req.author,
            ts: req.ts,
            shard_cost: cost,
            eternal: req.fidelity.is_eternal(),
            scene_metadata: req.scene_metadata.clone(),
            // Basic auto-Permanent · paid → Permanent (Pending/Verified are
            // upstream Σ-Chain confirmation states · stub-Permanent for stage-0)
            state: ImprintState::Permanent,
            commissioned_narration: req.commissioned_narration.clone(),
            ttl_token,
        };

        // 7. Defense-in-depth structural-guard
        if let Err(err) = imprint.assert_cosmetic_only() {
            // rollback balance (if we deducted)
            if cost > 0 {
                self.balances.insert(req.author, cur_balance);
            }
            self.audit.push(AuditEvent::PurchaseRejected {
                author: req.author,
                reason_kind: "post-mint-cosmetic-guard".to_owned(),
            });
            return match err {
                AkashicError::CosmeticAxiomViolation { field, reason } => {
                    PurchaseOutcome::CosmeticAxiomViolation { field, reason }
                }
                _ => PurchaseOutcome::CosmeticAxiomViolation {
                    field: "imprint",
                    reason: "post-mint guard failed",
                },
            };
        }

        // 8. Audit-emit
        if cost == 0 {
            self.audit.push(AuditEvent::BasicImprintFree {
                imprint_id: id,
                author: req.author,
            });
        } else {
            self.audit.push(AuditEvent::ShardsDeducted {
                imprint_id: id,
                author: req.author,
                amount: cost,
                new_balance: new_balance.amount(),
                tier: req.fidelity,
            });
        }
        if matches!(req.fidelity, FidelityTier::EternalAttribution) {
            self.attribution
                .try_claim(req.author, &req.scene_metadata.scene_name, id);
            self.audit.push(AuditEvent::EternalAttributionClaimed {
                imprint_id: id,
                author: req.author,
                scene_name: req.scene_metadata.scene_name.clone(),
            });
        }
        if matches!(req.fidelity, FidelityTier::HistoricalReconstructionTour) {
            self.audit.push(AuditEvent::HistoricalTourTokenIssued {
                imprint_id: id,
                author: req.author,
                ttl_secs: self.config.historical_tour_ttl_secs,
            });
        }

        // 9. Persist
        self.imprints.insert(id, imprint.clone());
        PurchaseOutcome::Granted { imprint, new_balance }
    }

    /// Revoke a non-eternal imprint.
    ///
    /// # Errors
    /// - `UnknownImprint` if `id` not present
    /// - `InvariantViolation` if attempting to revoke an eternal-attribution
    pub fn revoke(&mut self, id: ImprintId, reason: RevokedReason) -> Result<(), AkashicError> {
        let entry = self
            .imprints
            .get_mut(&id)
            .ok_or(AkashicError::UnknownImprint(id))?;
        if entry.eternal {
            return Err(AkashicError::InvariantViolation(
                "EternalAttribution can NEVER be revoked",
            ));
        }
        entry.state = ImprintState::Revoked(reason.clone());
        self.audit.push(AuditEvent::ImprintRevoked {
            imprint_id: id,
            reason,
        });
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: ImprintId) -> Option<&Imprint> {
        self.imprints.get(&id)
    }

    /// Iterate-all imprints · deterministic-order (BTreeMap).
    pub fn iter_imprints(&self) -> impl Iterator<Item = &Imprint> {
        self.imprints.values()
    }

    /// Number of imprints currently stored (any state).
    #[must_use]
    pub fn imprint_count(&self) -> usize {
        self.imprints.len()
    }
}

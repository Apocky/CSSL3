// § cap.rs — sovereign-cap matrix · DEFAULT-DENY · per-cycle.
//
// § PRIME-DIRECTIVE § 0
//   "consent = OS · default-deny" — the orchestrator MUST refuse to perform
//   any mutation, network egress, or cap-elevation unless Apocky has
//   explicitly granted the corresponding cap-bit. The matrix below makes
//   the intended grants explicit + auditable + machine-checkable in tests.

use serde::{Deserialize, Serialize};

use crate::cycles::CycleKind;

/// Fine-grained cap-bits.
///
/// `MutateContent` and `MutateBias` are intentionally distinct so an Apocky
/// who wants to allow KAN bias-updates but block CSSL-source mutation can
/// grant the latter without the former.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum CapKind {
    /// Permit the daemon to enqueue self-author CSSL drafts.
    AuthorDraft,
    /// Permit a self-authored draft to be live-mutated into the active
    /// substrate (otherwise the draft is recorded-only).
    MutateContent,
    /// Permit KAN bias-updates to be applied to the live template-bias-map.
    MutateBias,
    /// Permit the playtest-driver to consume CPU running auto-playtests.
    Playtest,
    /// Permit the daemon to federate mycelium pattern deltas to peers.
    NetworkEgress,
    /// Permit the daemon to anchor cycle-events on Σ-Chain.
    SigmaAnchor,
    /// Permit the daemon to operate at elevated-priority in idle mode.
    IdleEscalate,
}

impl CapKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::AuthorDraft => "author_draft",
            Self::MutateContent => "mutate_content",
            Self::MutateBias => "mutate_bias",
            Self::Playtest => "playtest",
            Self::NetworkEgress => "network_egress",
            Self::SigmaAnchor => "sigma_anchor",
            Self::IdleEscalate => "idle_escalate",
        }
    }
}

/// Default-deny matrix. Each cap-bit is independently grant-able + revoke-able.
///
/// In production the matrix is loaded from `~/.loa-secrets/orchestrator-caps.toml`
/// (see W14 runbook). In tests Apocky's grant-pattern is supplied programmatically.
#[derive(Debug, Clone, Default)]
pub struct SovereignCapMatrix {
    grants: [bool; 7],
}

impl SovereignCapMatrix {
    /// Construct an all-deny matrix. This is the ONLY default ; PRIME-DIRECTIVE-aligned.
    pub fn default_deny() -> Self {
        Self { grants: [false; 7] }
    }

    /// Apocky-only escape-hatch : grant ALL caps. Used by integration-tests +
    /// Apocky's "yes I trust this run" override.
    pub fn grant_all() -> Self {
        Self { grants: [true; 7] }
    }

    pub fn grant(&mut self, cap: CapKind) {
        self.grants[Self::idx(cap)] = true;
    }

    pub fn revoke(&mut self, cap: CapKind) {
        self.grants[Self::idx(cap)] = false;
    }

    pub fn is_granted(&self, cap: CapKind) -> bool {
        self.grants[Self::idx(cap)]
    }

    /// Which caps are required for a cycle of this kind ? Returns the FIRST
    /// missing cap (ordered : drafting comes before mutation comes before egress)
    /// so cap-deny error messages are deterministic + diff-stable in tests.
    pub fn check(&self, kind: CycleKind) -> CapDecision {
        let required = match kind {
            // Self-author cycle : drafting requires AuthorDraft. We DO NOT require
            // MutateContent at the cycle-level — drafts are recorded-only by
            // default ; mutate is a per-draft sub-cap-check inside the driver.
            CycleKind::SelfAuthor => &[CapKind::AuthorDraft, CapKind::SigmaAnchor][..],
            CycleKind::Playtest => &[CapKind::Playtest, CapKind::SigmaAnchor][..],
            CycleKind::KanTick => &[CapKind::MutateBias, CapKind::SigmaAnchor][..],
            CycleKind::MyceliumSync => &[CapKind::NetworkEgress, CapKind::SigmaAnchor][..],
            CycleKind::IdleDeepProcgen => &[CapKind::IdleEscalate, CapKind::SigmaAnchor][..],
        };
        for cap in required {
            if !self.is_granted(*cap) {
                return CapDecision::Deny {
                    cycle: kind,
                    missing: *cap,
                };
            }
        }
        CapDecision::Allow { cycle: kind }
    }

    fn idx(cap: CapKind) -> usize {
        match cap {
            CapKind::AuthorDraft => 0,
            CapKind::MutateContent => 1,
            CapKind::MutateBias => 2,
            CapKind::Playtest => 3,
            CapKind::NetworkEgress => 4,
            CapKind::SigmaAnchor => 5,
            CapKind::IdleEscalate => 6,
        }
    }
}

/// Result of a per-cycle cap-check.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CapDecision {
    Allow { cycle: CycleKind },
    Deny { cycle: CycleKind, missing: CapKind },
}

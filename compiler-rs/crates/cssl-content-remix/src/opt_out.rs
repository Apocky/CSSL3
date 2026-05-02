//! § opt_out — original-creator can block FUTURE remixes of their content.
//!
//! § INVARIANTS (PRIME-DIRECTIVE)
//!   ─ opt-out blocks NEW remixes only · existing remixes preserved
//!   ─ creator-pubkey-keyed (revocable identity)
//!   ─ irreversible-future-block ; can be reset ONLY by the creator
//!   ─ never retroactively-revokes-attribution (sovereignty-axiom :
//!     existing creators of past remixes have inherent rights to their
//!     attribution-record)

use crate::link::ContentId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// § Sovereignty axiom embedded in code for transparency-audit.
pub const OPT_OUT_AXIOM: &str =
    "creator-blocks-FUTURE-remixes-only · existing-remixes-PRESERVED · sovereignty-irrevocable-for-past-attributions";

/// Decision returned by `OptOutRegistry::check_remix_allowed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptOutDecision {
    /// Remix is allowed to proceed.
    Allowed,
    /// Remix is blocked because the parent's creator opted-out.
    BlockedByOptOut {
        parent_id: ContentId,
    },
}

/// In-memory opt-out registry. Production-deployment uses
/// `content_creator_opt_out` table in 0030 SQL ; this is the pure-Rust
/// deterministic mirror for unit-testing + offline-tools.
#[derive(Debug, Default, Clone)]
pub struct OptOutRegistry {
    opted_out_creators: BTreeSet<String>,
}

impl OptOutRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            opted_out_creators: BTreeSet::new(),
        }
    }

    /// Mark a creator as opted-out. Idempotent.
    pub fn opt_out(&mut self, creator_pubkey: String) {
        self.opted_out_creators.insert(creator_pubkey);
    }

    /// Reset opt-out for a creator (sovereign-decision). Returns true if a
    /// previous opt-out was lifted.
    pub fn opt_in(&mut self, creator_pubkey: &str) -> bool {
        self.opted_out_creators.remove(creator_pubkey)
    }

    /// True if the creator currently opted-out (FUTURE-remixes-blocked).
    #[must_use]
    pub fn is_opted_out(&self, creator_pubkey: &str) -> bool {
        self.opted_out_creators.contains(creator_pubkey)
    }

    /// Decide whether a NEW remix-link may be created. The caller passes the
    /// parent-content's author-pubkey (looked-up in the W12-4 packages
    /// table) plus the parent-id for diagnostic-emit.
    pub fn check_remix_allowed(
        &self,
        parent_creator_pubkey: &str,
        parent_id: &ContentId,
    ) -> OptOutDecision {
        if self.is_opted_out(parent_creator_pubkey) {
            OptOutDecision::BlockedByOptOut {
                parent_id: parent_id.clone(),
            }
        } else {
            OptOutDecision::Allowed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_out_blocks_new_remix() {
        let mut r = OptOutRegistry::new();
        r.opt_out("creator-1".to_string());
        let d = r.check_remix_allowed("creator-1", &"parent-x".to_string());
        matches!(d, OptOutDecision::BlockedByOptOut { .. });
    }

    #[test]
    fn opt_in_lifts_block() {
        let mut r = OptOutRegistry::new();
        r.opt_out("creator-1".to_string());
        assert!(r.is_opted_out("creator-1"));
        assert!(r.opt_in("creator-1"));
        assert!(!r.is_opted_out("creator-1"));
        let d = r.check_remix_allowed("creator-1", &"parent-x".to_string());
        assert_eq!(d, OptOutDecision::Allowed);
    }

    #[test]
    fn allowed_when_creator_not_opted_out() {
        let r = OptOutRegistry::new();
        let d = r.check_remix_allowed("creator-2", &"parent".to_string());
        assert_eq!(d, OptOutDecision::Allowed);
    }
}

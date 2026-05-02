//! § RemixChain — in-memory store keyed by ContentId. The DB-side store is
//! `content_remix_links` in `0030_content_remix.sql`. This crate keeps the
//! pure-Rust deterministic in-memory variant for testing + offline-tools.

use crate::link::{ContentId, RemixLink};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RemixChainError {
    #[error("duplicate remix-link for content {0}")]
    Duplicate(ContentId),
    #[error("attribution chain cycle detected starting at {0}")]
    Cycle(ContentId),
    #[error("attribution chain depth exceeded {0}")]
    DepthExceeded(usize),
}

/// Maximum depth of an attribution-chain walk before we abort. Bounded to
/// keep traversal O(depth) deterministic against pathological inputs.
pub const MAX_CHAIN_DEPTH: usize = 256;

/// In-memory remix-link store. Keyed by `remixed_id` (the child).
#[derive(Debug, Default, Clone)]
pub struct RemixChain {
    by_child: BTreeMap<ContentId, RemixLink>,
}

impl RemixChain {
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_child: BTreeMap::new(),
        }
    }

    /// Insert a link. Rejects duplicates (immutable post-anchor).
    pub fn insert(&mut self, link: RemixLink) -> Result<(), RemixChainError> {
        if self.by_child.contains_key(&link.remixed_id) {
            return Err(RemixChainError::Duplicate(link.remixed_id.clone()));
        }
        self.by_child.insert(link.remixed_id.clone(), link);
        Ok(())
    }

    /// Lookup a single link by child-id.
    #[must_use]
    pub fn get(&self, child_id: &ContentId) -> Option<&RemixLink> {
        self.by_child.get(child_id)
    }

    /// Number of links stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_child.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_child.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::RemixKind;
    use crate::royalty::RoyaltyShareGift;
    use crate::sign::sign_remix_link;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn link_for(child: &str, parent: &str) -> RemixLink {
        let signer = SigningKey::generate(&mut OsRng);
        let pubkey_hex = crate::hex_lower(&signer.verifying_key().to_bytes());
        let mut l = RemixLink::new_draft(
            child.to_string(),
            parent.to_string(),
            "1.0.0".to_string(),
            RemixKind::Extension,
            String::new(),
            42,
            pubkey_hex,
            RoyaltyShareGift::none(),
        )
        .unwrap();
        sign_remix_link(&signer, &mut l).unwrap();
        l
    }

    #[test]
    fn insert_and_lookup() {
        let mut c = RemixChain::new();
        c.insert(link_for("child", "parent")).unwrap();
        assert_eq!(c.len(), 1);
        assert!(c.get(&"child".to_string()).is_some());
    }

    #[test]
    fn duplicate_insert_rejected() {
        let mut c = RemixChain::new();
        c.insert(link_for("child", "parent")).unwrap();
        let err = c.insert(link_for("child", "other-parent")).unwrap_err();
        matches!(err, RemixChainError::Duplicate(_));
    }
}

//! § attribution — walk a remix-chain from leaf-id up to genesis.
//!
//! Genesis = a content-id that has NO entry in the chain (i.e., it was
//! authored from-scratch). The walk stops there.
//!
//! Complexity O(depth) with cycle-detection via visited-set. Verifies each
//! link's signature + Σ-Chain anchor as it walks ; refuses to emit on any
//! invalid link (return `BadLinkAt`).

use crate::chain::{RemixChain, MAX_CHAIN_DEPTH};
use crate::link::{ContentId, RemixLink};
use crate::verify::{verify_remix_link, VerifyError};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AttributionError {
    #[error("attribution-chain cycle detected at {0}")]
    Cycle(ContentId),
    #[error("max chain-depth {MAX_CHAIN_DEPTH} exceeded at {0}")]
    DepthExceeded(ContentId),
    #[error("link verification failed at {0} : {1}")]
    BadLinkAt(ContentId, VerifyError),
}

/// Result of walking the attribution-chain : ordered child→genesis-direction
/// list of links, plus the genesis-id (the topmost ancestor with no link).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributionWalk<'a> {
    pub links: Vec<&'a RemixLink>,
    pub genesis_id: ContentId,
}

/// Walk attribution chain from `start_id` up-to-genesis. Verifies each link
/// (Σ-Chain-anchor + Ed25519-sig). Cycle-detect via BTreeSet visited.
pub fn get_attribution_chain<'a>(
    chain: &'a RemixChain,
    start_id: &ContentId,
) -> Result<AttributionWalk<'a>, AttributionError> {
    let mut links: Vec<&RemixLink> = Vec::new();
    let mut visited: BTreeSet<ContentId> = BTreeSet::new();
    let mut cur = start_id.clone();

    for _depth in 0..=MAX_CHAIN_DEPTH {
        if !visited.insert(cur.clone()) {
            return Err(AttributionError::Cycle(cur));
        }
        let Some(link) = chain.get(&cur) else {
            // genesis : no link for this id → it is the top-most ancestor.
            return Ok(AttributionWalk {
                links,
                genesis_id: cur,
            });
        };
        verify_remix_link(link).map_err(|e| AttributionError::BadLinkAt(cur.clone(), e))?;
        links.push(link);
        cur = link.parent_id.clone();
    }
    Err(AttributionError::DepthExceeded(cur))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::RemixChain;
    use crate::kind::RemixKind;
    use crate::link::RemixLink;
    use crate::royalty::RoyaltyShareGift;
    use crate::sign::sign_remix_link;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn fresh_link(child: &str, parent: &str, kind: RemixKind) -> RemixLink {
        let signer = SigningKey::generate(&mut OsRng);
        let pubkey_hex = crate::hex_lower(&signer.verifying_key().to_bytes());
        let mut l = RemixLink::new_draft(
            child.to_string(),
            parent.to_string(),
            "1.0.0".to_string(),
            kind,
            format!("{kind:?} of {parent}"),
            7,
            pubkey_hex,
            RoyaltyShareGift::pledged(10).unwrap(),
        )
        .unwrap();
        sign_remix_link(&signer, &mut l).unwrap();
        l
    }

    #[test]
    fn walk_three_levels_to_genesis() {
        let mut c = RemixChain::new();
        c.insert(fresh_link("c", "b", RemixKind::Improvement)).unwrap();
        c.insert(fresh_link("b", "a", RemixKind::Translation)).unwrap();
        // "a" has no entry → it is genesis.

        let walk = get_attribution_chain(&c, &"c".to_string()).unwrap();
        assert_eq!(walk.links.len(), 2);
        assert_eq!(walk.genesis_id, "a");
        assert_eq!(walk.links[0].remixed_id, "c");
        assert_eq!(walk.links[1].remixed_id, "b");
    }

    #[test]
    fn cycle_detected_clean() {
        // Construct an artificial cycle a→b→a by inserting both.
        let mut c = RemixChain::new();
        c.insert(fresh_link("a", "b", RemixKind::Fork)).unwrap();
        c.insert(fresh_link("b", "a", RemixKind::Fork)).unwrap();
        let err = get_attribution_chain(&c, &"a".to_string()).unwrap_err();
        matches!(err, AttributionError::Cycle(_));
    }

    #[test]
    fn genesis_id_is_start_when_no_links_present() {
        let c = RemixChain::new();
        let walk = get_attribution_chain(&c, &"orphan".to_string()).unwrap();
        assert_eq!(walk.genesis_id, "orphan");
        assert!(walk.links.is_empty());
    }
}

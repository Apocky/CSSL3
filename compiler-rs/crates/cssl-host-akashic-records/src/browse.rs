// § browse.rs · BrowseQuery + browse-by-{scene · author · fidelity-min}
// § Permanent-only · Revoked filtered-out (per task-spec § 8)

use serde::{Deserialize, Serialize};

use crate::attribution::AuthorPubkey;
use crate::fidelity::FidelityTier;
use crate::imprint::{Imprint, ImprintState};
use crate::purchase::AkashicLedger;

/// Browse query · all-fields-optional · AND-conjunction.
///
/// Per spec/18 § FREE-TIER : `search by tag · biome · element · class`.
/// This stage-0 implementation supports scene-name + author + fidelity-min ;
/// extended faceting deferred to W8-C4 (sibling crate per spec/18 line 120).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowseQuery {
    pub scene_name: Option<String>,
    pub author: Option<AuthorPubkey>,
    pub fidelity_min: Option<FidelityTier>,
}

impl BrowseQuery {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_scene_name(mut self, name: impl Into<String>) -> Self {
        self.scene_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn with_author(mut self, author: AuthorPubkey) -> Self {
        self.author = Some(author);
        self
    }

    #[must_use]
    pub fn with_fidelity_min(mut self, tier: FidelityTier) -> Self {
        self.fidelity_min = Some(tier);
        self
    }

    pub(crate) fn matches(&self, imprint: &Imprint) -> bool {
        // Permanent-only · Revoked filtered-out (per § 8).
        if !matches!(imprint.state, ImprintState::Permanent) {
            return false;
        }
        if let Some(name) = &self.scene_name {
            if &imprint.scene_metadata.scene_name != name {
                return false;
            }
        }
        if let Some(author) = &self.author {
            if &imprint.author_pubkey != author {
                return false;
            }
        }
        if let Some(min) = self.fidelity_min {
            if imprint.fidelity < min {
                return false;
            }
        }
        true
    }
}

/// Browse result wrapper · count + matched-imprints.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct BrowseResult<'a> {
    pub matched: Vec<&'a Imprint>,
}

impl<'a> BrowseResult<'a> {
    #[must_use]
    pub fn count(&self) -> usize {
        self.matched.len()
    }
}

/// Run a browse-query against a ledger · deterministic-iteration order.
#[must_use]
pub fn browse<'a>(ledger: &'a AkashicLedger, query: &BrowseQuery) -> BrowseResult<'a> {
    let matched: Vec<&Imprint> = ledger
        .iter_imprints()
        .filter(|i| query.matches(i))
        .collect();
    BrowseResult { matched }
}

//! § sigma — Σ-mask consent gate per channel.
//!
//! The Σ-mask is the user-controlled opt-in matrix : for each of the 9
//! channels, the player has a yes/no toggle. By default ONLY
//! `Channel::SecurityPatch` is on (per `Channel::default_consent_on()`) ;
//! every other channel requires explicit consent.
//!
//! The policy is consulted by `cssl-hotfix-client::poll_loop` *before*
//! any download or apply. A revoked consent at any later moment causes
//! pending bundles to be dropped, and applied bundles to be rolled-back
//! on next quiescent boundary.

use crate::channel::{Channel, CHANNELS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § Per-channel consent map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigmaPolicy {
    consent: BTreeMap<Channel, UpdateConsent>,
}

/// § Consent state for a single channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateConsent {
    /// Auto-download, auto-apply.
    AutoApply,
    /// Auto-download, prompt before apply.
    PromptBeforeApply,
    /// Pin to current version, never check for updates.
    PinnedNoUpdates,
    /// Default-deny : do nothing for this channel.
    Off,
}

impl Default for SigmaPolicy {
    fn default() -> Self {
        let mut consent = BTreeMap::new();
        for ch in CHANNELS {
            consent.insert(
                ch,
                if ch.default_consent_on() {
                    UpdateConsent::AutoApply
                } else {
                    UpdateConsent::Off
                },
            );
        }
        Self { consent }
    }
}

impl SigmaPolicy {
    /// Read the current consent for a channel.
    #[must_use]
    pub fn get(&self, ch: Channel) -> UpdateConsent {
        self.consent.get(&ch).copied().unwrap_or(UpdateConsent::Off)
    }

    /// Set / update consent. Returns the previous value.
    pub fn set(&mut self, ch: Channel, c: UpdateConsent) -> UpdateConsent {
        self.consent.insert(ch, c).unwrap_or(UpdateConsent::Off)
    }

    /// Should the client even *consider* polling/downloading for this
    /// channel ? `true` for AutoApply / PromptBeforeApply.
    #[must_use]
    pub fn allows_download(&self, ch: Channel) -> bool {
        matches!(
            self.get(ch),
            UpdateConsent::AutoApply | UpdateConsent::PromptBeforeApply
        )
    }

    /// Should the client apply silently without prompting ?
    #[must_use]
    pub fn allows_silent_apply(&self, ch: Channel) -> bool {
        matches!(self.get(ch), UpdateConsent::AutoApply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_security_only() {
        let p = SigmaPolicy::default();
        assert_eq!(p.get(Channel::SecurityPatch), UpdateConsent::AutoApply);
        for ch in CHANNELS {
            if ch != Channel::SecurityPatch {
                assert_eq!(
                    p.get(ch),
                    UpdateConsent::Off,
                    "{} default must be Off",
                    ch.name()
                );
            }
        }
    }

    #[test]
    fn allows_download_matrix() {
        let mut p = SigmaPolicy::default();
        // Default Off = no download.
        assert!(!p.allows_download(Channel::CsslBundle));
        p.set(Channel::CsslBundle, UpdateConsent::AutoApply);
        assert!(p.allows_download(Channel::CsslBundle));
        p.set(Channel::CsslBundle, UpdateConsent::PromptBeforeApply);
        assert!(p.allows_download(Channel::CsslBundle));
        p.set(Channel::CsslBundle, UpdateConsent::PinnedNoUpdates);
        assert!(!p.allows_download(Channel::CsslBundle));
    }

    #[test]
    fn silent_apply_only_for_auto() {
        let mut p = SigmaPolicy::default();
        assert!(p.allows_silent_apply(Channel::SecurityPatch));
        p.set(Channel::SecurityPatch, UpdateConsent::PromptBeforeApply);
        assert!(!p.allows_silent_apply(Channel::SecurityPatch));
    }

    #[test]
    fn set_returns_prior() {
        let mut p = SigmaPolicy::default();
        let prior = p.set(Channel::CsslBundle, UpdateConsent::AutoApply);
        assert_eq!(prior, UpdateConsent::Off);
    }

    #[test]
    fn serde_roundtrip() {
        let p = SigmaPolicy::default();
        let s = serde_json::to_string(&p).unwrap();
        let back: SigmaPolicy = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }
}

//! § sigma_mask — Σ-private gating for the amplifier
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.3 (d) sovereignty` :
//!   "amplifier ¬ blurs-other-Sovereign's body @ Σ-private-region". The
//!   amplifier MUST consult a privacy classification before evaluating its
//!   KAN-network and emit `AmplifiedFragment::ZERO` (effectively "no detail
//!   emerged here") for any fragment classified Σ-private.
//!
//!   The crate is a sibling to `cssl-substrate-prime-directive` ; the full
//!   IFC-label machinery (multi-tier sovereignty, capability-bounded
//!   surfaces, audit-chain witnessing) lives there. This module's surface
//!   is the minimal classification carrier the amplifier needs : a
//!   two-state enum `{ Public, Private }` plus a check trait that lets
//!   callers attach their own classifier (e.g. D116 raymarch's RayHit
//!   Σ-mask field).

use core::fmt;

/// § Σ-privacy classification for a fragment. Two states :
///
///   - `Public` — the fragment is in a region the amplifier may evaluate
///     freely. The KAN-network produces sub-pixel detail.
///   - `Private` — the fragment is in another Sovereign's Σ-private
///     region. The amplifier emits `ZERO` and the recursion driver
///     immediately truncates, regardless of confidence.
///
///   The Σ-mask check is performed BEFORE the KAN-network is evaluated, so
///   a Σ-private fragment never sees its `world_pos` enter a KAN-input
///   vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SigmaPrivacy {
    /// § Fragment is in a public region ; amplifier may evaluate freely.
    #[default]
    Public,
    /// § Fragment is in a Σ-private region of another Sovereign ;
    ///   amplifier MUST refuse evaluation and emit ZERO.
    Private,
}

impl SigmaPrivacy {
    /// § True iff this fragment is public (i.e. the amplifier may
    ///   evaluate). The standard early-out check at the top of
    ///   [`crate::FractalAmplifier::amplify`].
    #[must_use]
    pub const fn is_public(self) -> bool {
        matches!(self, Self::Public)
    }

    /// § True iff this fragment is private (i.e. the amplifier MUST
    ///   refuse and emit ZERO).
    #[must_use]
    pub const fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

impl fmt::Display for SigmaPrivacy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Private => write!(f, "Σ-private"),
        }
    }
}

/// § Trait the caller implements to attach a Σ-classifier to its hit
///   type. The amplifier invokes this on every fragment before any
///   KAN-network is touched.
///
/// `cssl-render-v2` (D116) implements this for its `RayHit` type by
/// delegating to the cell's Σ-mask field. Mock implementations in the
/// test suite use a constant classification for unit tests.
pub trait SigmaMaskCheck {
    /// § Classify the fragment. Returns `SigmaPrivacy::Public` to allow
    ///   amplification, `SigmaPrivacy::Private` to refuse.
    fn classify_privacy(&self) -> SigmaPrivacy;
}

/// § A constant-Public classifier that always allows amplification.
///   Used by the simplest unit tests where Σ-privacy is irrelevant.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysPublic;

impl SigmaMaskCheck for AlwaysPublic {
    fn classify_privacy(&self) -> SigmaPrivacy {
        SigmaPrivacy::Public
    }
}

/// § A constant-Private classifier that always refuses amplification.
///   Used in tests that pin the "Σ-private fragment emits ZERO" property.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysPrivate;

impl SigmaMaskCheck for AlwaysPrivate {
    fn classify_privacy(&self) -> SigmaPrivacy {
        SigmaPrivacy::Private
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § default is Public.
    #[test]
    fn default_is_public() {
        let p = SigmaPrivacy::default();
        assert!(p.is_public());
        assert!(!p.is_private());
    }

    /// § Public classifies as public.
    #[test]
    fn public_is_public() {
        assert!(SigmaPrivacy::Public.is_public());
    }

    /// § Private classifies as private.
    #[test]
    fn private_is_private() {
        assert!(SigmaPrivacy::Private.is_private());
    }

    /// § AlwaysPublic returns Public.
    #[test]
    fn always_public_returns_public() {
        let c = AlwaysPublic;
        assert_eq!(c.classify_privacy(), SigmaPrivacy::Public);
    }

    /// § AlwaysPrivate returns Private.
    #[test]
    fn always_private_returns_private() {
        let c = AlwaysPrivate;
        assert_eq!(c.classify_privacy(), SigmaPrivacy::Private);
    }

    /// § Display formats as expected.
    #[test]
    fn display_renders_correctly() {
        assert_eq!(format!("{}", SigmaPrivacy::Public), "public");
        assert_eq!(format!("{}", SigmaPrivacy::Private), "Σ-private");
    }
}

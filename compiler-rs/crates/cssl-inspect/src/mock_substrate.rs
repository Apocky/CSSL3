//! § Mock substrate surface for D162 MVP.
//!
//! This module supplies the minimum trait-shape that the real
//! cssl-substrate-omega-field + cssl-substrate-prime-directive crates will
//! eventually provide. When those crates land in the workspace, the swap is
//! ONE line in `lib.rs` :
//!
//! ```text
//! pub use mock_substrate::{Cap, ConsentBit, ...};
//! ```
//!
//! becomes
//!
//! ```text
//! pub use cssl_substrate_prime_directive::{Cap, ConsentBit, ...};
//! pub use cssl_substrate_omega_field::{MortonKey, SigmaOverlay, ...};
//! ```
//!
//! The Σ-mask logic implemented here is intentionally simple : a tag-string
//! discriminator. Any tag containing the substring `"biometric"` refuses
//! Observe ; any tag containing `"private"` requires explicit dev-mode AND
//! Observe consent ; everything else permits Observe. The real overlay is a
//! 16-byte packed bitfield ; this is a stand-in.
//!
//! § PRIME_DIRECTIVE compliance : the mock is intentionally MORE strict than
//! a permissive default — refusal is the safe option when the real overlay
//! is not yet wired. Tests exercise both refusal and permission paths.

use std::marker::PhantomData;

/// Marker type for the "dev-mode" capability class.
#[derive(Debug, Clone, Copy)]
pub struct DevMode;

/// Marker type for the "telemetry-egress" capability class.
#[derive(Debug, Clone, Copy)]
pub struct TelemetryEgress;

/// Capability token. Production-impl is opaque + non-forgeable ; this mock
/// is an enum so tests can construct degenerate variants.
#[derive(Debug, Clone)]
pub struct Cap<K> {
    kind: CapKind,
    _phantom: PhantomData<K>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CapKind {
    DevMode,
    TelemetryEgress,
    NondevSynthetic,
}

impl Cap<DevMode> {
    /// Construct a dev-mode capability for tests.
    #[must_use]
    pub fn dev_for_tests() -> Self {
        Self {
            kind: CapKind::DevMode,
            _phantom: PhantomData,
        }
    }

    /// Construct a degenerate non-dev cap for tests.
    #[must_use]
    pub fn synthetic_nondev_for_tests() -> Self {
        Self {
            kind: CapKind::NondevSynthetic,
            _phantom: PhantomData,
        }
    }

    /// Whether this cap actually grants dev-mode.
    #[must_use]
    pub fn permits_dev_mode(&self) -> bool {
        self.kind == CapKind::DevMode
    }
}

impl Cap<TelemetryEgress> {
    /// Construct a telemetry-egress cap for tests.
    #[must_use]
    pub fn egress_for_tests() -> Self {
        Self {
            kind: CapKind::TelemetryEgress,
            _phantom: PhantomData,
        }
    }

    /// Construct a degenerate non-egress cap for tests.
    #[must_use]
    pub fn synthetic_nonegress_for_tests() -> Self {
        Self {
            kind: CapKind::NondevSynthetic,
            _phantom: PhantomData,
        }
    }

    /// Whether this cap grants telemetry-egress.
    #[must_use]
    pub fn permits_egress(&self) -> bool {
        self.kind == CapKind::TelemetryEgress
    }
}

/// A morton key — index into the sparse field-cell grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MortonKey(u64);

impl MortonKey {
    /// Construct a morton-key from a raw u64.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw u64 backing value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// The 32-bit cached low half of the Σ-overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SigmaConsentBits(u32);

impl SigmaConsentBits {
    /// All-permissive — Observe + Sample granted.
    #[must_use]
    pub fn open() -> Self {
        Self((1 << ConsentBit::Observe as u32) | (1 << ConsentBit::Sample as u32))
    }

    /// All-refused.
    #[must_use]
    pub fn closed() -> Self {
        Self(0)
    }

    /// Whether the requested consent bit is set.
    #[must_use]
    pub fn permits(self, bit: ConsentBit) -> bool {
        (self.0 & (1 << bit as u32)) != 0
    }

    /// The raw bits.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// Consent-bit kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ConsentBit {
    /// Single-cell read consent.
    Observe = 0,
    /// Bulk-region read consent.
    Sample = 1,
}

/// Σ-overlay surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigmaOverlay(SigmaConsentBits);

impl SigmaOverlay {
    /// Look up the overlay for a given tag string.
    #[must_use]
    pub fn at(tag: &str) -> Self {
        if tag.contains("biometric") || tag.contains("private") {
            Self(SigmaConsentBits::closed())
        } else {
            Self(SigmaConsentBits::open())
        }
    }

    /// Whether Observe is permitted.
    #[must_use]
    pub fn permits(self, bit: ConsentBit) -> bool {
        self.0.permits(bit)
    }

    /// The exposed cached low-half (32-bit) overlay value.
    #[must_use]
    pub fn cached_bits(self) -> SigmaConsentBits {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_cap_permits() {
        assert!(Cap::<DevMode>::dev_for_tests().permits_dev_mode());
    }

    #[test]
    fn nondev_cap_refuses() {
        assert!(!Cap::<DevMode>::synthetic_nondev_for_tests().permits_dev_mode());
    }

    #[test]
    fn egress_cap_permits() {
        assert!(Cap::<TelemetryEgress>::egress_for_tests().permits_egress());
    }

    #[test]
    fn nonegress_cap_refuses() {
        assert!(!Cap::<TelemetryEgress>::synthetic_nonegress_for_tests().permits_egress());
    }

    #[test]
    fn morton_round_trip() {
        let k = MortonKey::new(0xdead_beef);
        assert_eq!(k.raw(), 0xdead_beef);
    }

    #[test]
    fn morton_ord_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(MortonKey::new(1));
        set.insert(MortonKey::new(2));
        assert_eq!(set.len(), 2);
        assert!(MortonKey::new(1) < MortonKey::new(2));
    }

    #[test]
    fn open_bits_permit_observe_and_sample() {
        let bits = SigmaConsentBits::open();
        assert!(bits.permits(ConsentBit::Observe));
        assert!(bits.permits(ConsentBit::Sample));
    }

    #[test]
    fn closed_bits_refuse() {
        let bits = SigmaConsentBits::closed();
        assert!(!bits.permits(ConsentBit::Observe));
        assert!(!bits.permits(ConsentBit::Sample));
    }

    #[test]
    fn closed_bits_raw_is_zero() {
        assert_eq!(SigmaConsentBits::closed().raw(), 0);
    }

    #[test]
    fn open_bits_raw_nonzero() {
        assert_ne!(SigmaConsentBits::open().raw(), 0);
    }

    #[test]
    fn overlay_refuses_biometric() {
        let sigma = SigmaOverlay::at("biometric:face_geometry");
        assert!(!sigma.permits(ConsentBit::Observe));
    }

    #[test]
    fn overlay_refuses_private() {
        let sigma = SigmaOverlay::at("private:reproductive_state");
        assert!(!sigma.permits(ConsentBit::Observe));
    }

    #[test]
    fn overlay_permits_default() {
        let sigma = SigmaOverlay::at("ground/wood/oak");
        assert!(sigma.permits(ConsentBit::Observe));
        assert!(sigma.permits(ConsentBit::Sample));
    }

    #[test]
    fn overlay_substring_match_biometric() {
        let sigma = SigmaOverlay::at("metadata.biometric.disabled");
        assert!(!sigma.permits(ConsentBit::Observe));
    }

    #[test]
    fn cap_kind_clone_and_debug() {
        let c = Cap::<DevMode>::dev_for_tests();
        let c2: Cap<DevMode> = Clone::clone(&c);
        assert!(c2.permits_dev_mode());
        assert!(c.permits_dev_mode());
        let s = format!("{c:?}");
        assert!(s.contains("Cap"));
    }

    #[test]
    fn consent_bit_observe_zero_index() {
        assert_eq!(ConsentBit::Observe as u32, 0);
        assert_eq!(ConsentBit::Sample as u32, 1);
    }

    #[test]
    fn cached_bits_matches_open() {
        let sigma = SigmaOverlay::at("safe/tag");
        assert!(sigma.cached_bits().permits(ConsentBit::Observe));
    }
}

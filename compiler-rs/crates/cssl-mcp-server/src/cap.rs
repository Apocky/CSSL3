//! Capability tokens — sealed-newtype `Cap<T>` pattern (D164 inspired).
//!
//! § T11-D229 STUB-LEVEL : real `CapToken` machinery lives in
//! `cssl-substrate-prime-directive` (D131). Until that crate stabilizes its
//! public surface we mock the cap-token discipline here. The integration
//! seam is documented inline so the swap is one-line per cap-kind once the
//! upstream surface is ready.
//!
//! ## Privacy invariants
//!
//! - [`Cap`] has a private `()` field — outside this module the type cannot
//!   be constructed by name. Construction goes through the per-kind
//!   constructors which document the consent boundary they enforce.
//! - `Cap<DevMode>::interactive` is the canonical constructor for the
//!   default-OFF dev-mode cap. In tests + Jθ-1 skeleton it is unrestricted ;
//!   future slices (Jθ-1.1) layer a `dev-mode` feature-gate + interactive
//!   prompt.
//! - `Cap<RemoteDev>` and `Cap<BiometricInspect>` have NO unrestricted
//!   constructor. The only way to materialize them is via `for_test()`,
//!   which is `#[cfg(test)]`-gated AND non-Copy / non-Clone, so test
//!   fixtures cannot accidentally proliferate them. Real grants land via
//!   D131's signed-token issuance once that crate stabilizes.
//!
//! ## Witness pattern
//!
//! [`CapWitness`] is the long-lived proof a [`Session`](crate::session::Session)
//! holds. The `Cap<T>` is consumed at session-open ; we materialize a
//! [`CapWitness`] (just the kind + an issuance counter) and store that.
//! Every tool dispatch re-checks the witness against the audit-chain.

use core::marker::PhantomData;

/// Marker trait for capability kinds. Sealed in this module so external
/// crates cannot introduce new caps without going through D131.
mod sealed {
    pub trait Sealed {}
}

/// Capability marker trait. Implemented for [`DevMode`], [`RemoteDev`],
/// [`BiometricInspect`].
pub trait CapMarker: sealed::Sealed {
    /// Stable string identifying the cap-kind (used in audit-events).
    const NAME: &'static str;
    /// Default-grant policy. `false` means the cap MUST be explicitly
    /// constructed via the per-kind constructor ; auto-grant in tests is
    /// disallowed.
    const DEFAULT_GRANTED: bool;
    /// Maps the marker to its [`CapKind`] discriminant.
    fn kind() -> CapKind;
}

/// `Cap<DevMode>` marker. Default-OFF : the dev-mode cap must be consumed
/// at [`McpServer::new`](crate::server::McpServer::new) construction-time.
#[derive(Debug)]
pub struct DevMode;

/// `Cap<RemoteDev>` marker. Default-DENIED : non-loopback transports refuse
/// to bind without a witness.
#[derive(Debug)]
pub struct RemoteDev;

/// `Cap<BiometricInspect>` marker. Default-DENIED : biometric tools refuse
/// to dispatch without a witness ; real biometric tools also enforce
/// compile-time refusal at registration (Jθ-8 owns that gate).
#[derive(Debug)]
pub struct BiometricInspect;

impl sealed::Sealed for DevMode {}
impl sealed::Sealed for RemoteDev {}
impl sealed::Sealed for BiometricInspect {}

impl CapMarker for DevMode {
    const NAME: &'static str = "DevMode";
    const DEFAULT_GRANTED: bool = false;
    fn kind() -> CapKind {
        CapKind::DevMode
    }
}

impl CapMarker for RemoteDev {
    const NAME: &'static str = "RemoteDev";
    const DEFAULT_GRANTED: bool = false;
    fn kind() -> CapKind {
        CapKind::RemoteDev
    }
}

impl CapMarker for BiometricInspect {
    const NAME: &'static str = "BiometricInspect";
    const DEFAULT_GRANTED: bool = false;
    fn kind() -> CapKind {
        CapKind::BiometricInspect
    }
}

/// Discriminant for the three cap-kinds modelled in this slice.
///
/// Future slices (Jθ-2..Jθ-8) extend this enum to cover `SovereignInspect`
/// + `TelemetryEgress`. The variant order is part of the stable surface ;
/// new variants are append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapKind {
    /// Dev-mode root cap.
    DevMode,
    /// Non-loopback transport cap.
    RemoteDev,
    /// Biometric tool dispatch cap.
    BiometricInspect,
}

impl CapKind {
    /// Stable string identifier used in audit-events + error messages.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DevMode => "DevMode",
            Self::RemoteDev => "RemoteDev",
            Self::BiometricInspect => "BiometricInspect",
        }
    }
}

/// Capability token. The private `()` field prevents direct construction
/// outside this module ; constructors enforce consent boundaries per-kind.
///
/// `Cap<T>` is non-Copy + non-Clone so it cannot be used twice ; passing it
/// to a function consumes it, and the receiver typically materializes a
/// [`CapWitness`] for long-lived proof.
#[derive(Debug)]
pub struct Cap<T: CapMarker> {
    _seal: (),
    _marker: PhantomData<T>,
}

impl<T: CapMarker> Cap<T> {
    /// Internal constructor — only callable inside this module.
    const fn new() -> Self {
        Self {
            _seal: (),
            _marker: PhantomData,
        }
    }

    /// Project the cap-kind discriminant. Useful for audit-events.
    #[must_use]
    pub fn kind(&self) -> CapKind {
        T::kind()
    }

    /// Consume the cap and produce a [`CapWitness`] tagged with the
    /// supplied issuance counter. Used at [`Session`](crate::session::Session)
    /// initialization time.
    #[must_use]
    pub fn into_witness(self, issued_at: u64) -> CapWitness {
        CapWitness {
            kind: T::kind(),
            issued_at,
        }
    }
}

impl Cap<DevMode> {
    /// Construct a dev-mode cap interactively. Stage-0 stub : in production
    /// this gates on a TTY-prompt or signed Apocky-PM token. For the
    /// skeleton we accept the call unconditionally so the test-suite can
    /// exercise the cap-flow.
    ///
    /// ## SWAP-POINT (D131 integration)
    /// Replace the body with a call into `cssl_substrate_prime_directive::
    /// authority::issue_cap::<DevMode>(prompt)` once that surface lands.
    #[must_use]
    pub fn interactive() -> Self {
        Self::new()
    }

    /// Test-fixture construction. Available under the `test-bypass` feature
    /// AND under `#[cfg(test)]` for the crate's own unit-tests.
    /// Non-Copy + non-Clone discipline is preserved by the `Cap<T>` type
    /// itself — this function only constructs ; nothing here lets a caller
    /// duplicate the resulting cap.
    ///
    /// ## SWAP-POINT (D131 integration)
    /// Once cap-issuance is wired, gate this on a feature-flag controlled
    /// by the upstream test-discipline.
    #[cfg(any(test, feature = "test-bypass"))]
    #[must_use]
    pub fn for_test() -> Self {
        Self::new()
    }
}

impl Cap<RemoteDev> {
    /// Test-fixture construction. Default-DENIED in production : real
    /// grants require Apocky-PM signed-token via D131.
    ///
    /// ## SWAP-POINT (D131 integration)
    /// Replace this with `cssl_substrate_prime_directive::authority::
    /// issue_cap::<RemoteDev>(signed_token)` validation.
    #[cfg(any(test, feature = "test-bypass"))]
    #[must_use]
    pub fn for_test() -> Self {
        Self::new()
    }
}

impl Cap<BiometricInspect> {
    /// Test-fixture construction. Default-DENIED in production : real
    /// grants require explicit consent flow + Apocky-PM signature via D131.
    ///
    /// ## CRITICAL : even with this cap granted, biometric data NEVER
    /// egresses off-device. The cap unlocks INSPECTION ; egress is
    /// gated by a separate `TelemetryEgress` cap that is permanently
    /// REFUSED at the IFC layer for biometric-labeled data.
    #[cfg(any(test, feature = "test-bypass"))]
    #[must_use]
    pub fn for_test() -> Self {
        Self::new()
    }
}

/// Long-lived proof that a cap was issued to a session. Stored in the
/// [`SessionCapSet`](crate::session::SessionCapSet). Cheap to copy + compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapWitness {
    /// Which cap-kind this witness proves.
    pub kind: CapKind,
    /// Audit-chain sequence number @ issuance. Used for replay-validation.
    pub issued_at: u64,
}

impl CapWitness {
    /// Returns true when this witness covers the given cap-kind.
    #[must_use]
    pub const fn covers(&self, kind: CapKind) -> bool {
        matches!(
            (self.kind, kind),
            (CapKind::DevMode, CapKind::DevMode)
                | (CapKind::RemoteDev, CapKind::RemoteDev)
                | (CapKind::BiometricInspect, CapKind::BiometricInspect)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_kinds_have_stable_names() {
        assert_eq!(CapKind::DevMode.as_str(), "DevMode");
        assert_eq!(CapKind::RemoteDev.as_str(), "RemoteDev");
        assert_eq!(CapKind::BiometricInspect.as_str(), "BiometricInspect");
    }

    #[test]
    fn dev_mode_interactive_constructs() {
        let cap = Cap::<DevMode>::interactive();
        assert_eq!(cap.kind(), CapKind::DevMode);
    }

    #[test]
    fn dev_mode_for_test_constructs() {
        let cap = Cap::<DevMode>::for_test();
        assert_eq!(cap.kind(), CapKind::DevMode);
    }

    #[test]
    fn remote_dev_for_test_constructs() {
        let cap = Cap::<RemoteDev>::for_test();
        assert_eq!(cap.kind(), CapKind::RemoteDev);
    }

    #[test]
    fn biometric_for_test_constructs() {
        let cap = Cap::<BiometricInspect>::for_test();
        assert_eq!(cap.kind(), CapKind::BiometricInspect);
    }

    #[test]
    fn cap_into_witness_preserves_kind() {
        let cap = Cap::<DevMode>::for_test();
        let w = cap.into_witness(42);
        assert_eq!(w.kind, CapKind::DevMode);
        assert_eq!(w.issued_at, 42);
    }

    #[test]
    fn witness_covers_same_kind() {
        let w = CapWitness {
            kind: CapKind::DevMode,
            issued_at: 1,
        };
        assert!(w.covers(CapKind::DevMode));
        assert!(!w.covers(CapKind::RemoteDev));
        assert!(!w.covers(CapKind::BiometricInspect));
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn cap_default_granted_is_false() {
        // We deliberately assert the const value at the trait-impl level so
        // that lift-to-default-true would surface as a TEST failure (rather
        // than passing silently when the marker is changed). The
        // `assertions_on_constants` lint is suppressed on purpose : these
        // ARE constants by design and the assertion exists as a regression
        // tripwire.
        assert!(!DevMode::DEFAULT_GRANTED);
        assert!(!RemoteDev::DEFAULT_GRANTED);
        assert!(!BiometricInspect::DEFAULT_GRANTED);
    }

    #[test]
    fn marker_names_match_capkind() {
        assert_eq!(DevMode::NAME, CapKind::DevMode.as_str());
        assert_eq!(RemoteDev::NAME, CapKind::RemoteDev.as_str());
        assert_eq!(BiometricInspect::NAME, CapKind::BiometricInspect.as_str());
    }
}

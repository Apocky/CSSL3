//! CSSLv3 Substrate — PRIME_DIRECTIVE enforcement layer.
//!
//! § SPEC : `PRIME_DIRECTIVE.md` (root-of-trust) + `specs/30_SUBSTRATE.csl` §
//!   PRIME_DIRECTIVE-ALIGNMENT + `specs/11_IFC.csl` § PRIME-DIRECTIVE-ENCODING
//!   + `specs/22_TELEMETRY.csl` § AUDIT-CHAIN.
//!
//! § THESIS
//!
//! Every Substrate operation passes through three gates :
//!
//! 1. **Capability check** — does the caller hold a valid
//!    [`CapToken`] for the requested [`SubstrateCap`] ?
//! 2. **Harm-prevention check** — does the operation pass the 17
//!    canonical prohibitions encoded as [`Prohibition::all`] ?
//! 3. **Audit-chain attestation** — every grant, every revoke, every
//!    gated op emits an R18 audit-entry signed into the immutable
//!    append-only chain (`cssl_telemetry::AuditChain`).
//!
//! This crate is the cross-cutting concern that sibling slices H1..H5
//! reference but do not implement directly. It establishes the canonical
//! surface for every Substrate cap that can ever exist.
//!
//! § INVARIANTS  (PRIME_DIRECTIVE §0 + §7)
//!   - Consent is the OS. Every cap-grant requires an interactive consent
//!     path ; the test-bypass `caps_grant_for_test` is feature-gated
//!     (`feature = "test-bypass"`) and ABSENT from production builds (no
//!     `#[cfg]`-able runtime-flag exists).
//!   - Tokens are non-copyable + non-cloneable at the type level. Every
//!     consumer takes the token by-value (move). This enforces single-use.
//!   - The 17 prohibitions are **not** redefined here — each
//!     [`Prohibition`] variant references `PRIME_DIRECTIVE.md` § 1 verbatim.
//!   - Kill-switch [`substrate_halt`] completes within one omega_step tick
//!     (verified in tests against a 1ms-budget oracle).
//!   - Audit-append failure ⇒ panic. Per spec §§ 22 § AUDIT-CHAIN, the
//!     chain CANNOT be silently skipped.
//!
//! § PRIME_DIRECTIVE  (immutable root-of-trust this crate inherits)
//!   `consent = OS • sovereignty = substrate-invariant • violation = bug
//!   • ¬override ∃`
//!
//! § ATTESTATION
//!   The canonical attestation propagates through every public fn via
//!   [`ATTESTATION`] :
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody." Every Substrate fn that lowers through this enforcement
//!   layer carries this constant ; integrity-check is performed at fn-entry.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod attestation;
pub mod audit;
pub mod cap;
pub mod consent;
pub mod diag;
pub mod halt;
pub mod harm;

pub use attestation::{
    attestation_check, attestation_constant_text, path_hash_discipline_attestation_constant_hash,
    AttestationError, ATTESTATION, ATTESTATION_HASH, PATH_HASH_DISCIPLINE_ATTESTATION,
    PATH_HASH_DISCIPLINE_ATTESTATION_HASH,
};
#[cfg(any(test, feature = "test-bypass"))]
pub use audit::audit_chain_for_test;
pub use audit::{AuditEvent, EnforcementAuditBus, PathOpKind};
pub use cap::{CapToken, CapTokenId, SubstrateCap};
pub use consent::{
    caps_grant, caps_revoke, ConsentLog, ConsentScope, ConsentStore, GrantError,
    IdentityMarkerProbe, RevokeError,
};
pub use diag::{prohibition_for_code, DiagnosticCode, ProhibitionCodeTable, PD_TABLE};
pub use halt::{
    substrate_halt, CountingHaltSink, HaltOutcome, HaltReason, HaltSink, HaltStats, KillSwitch,
};
pub use harm::{
    HarmCheckError, HarmPrevention, Prohibition, ProhibitionCheck, ProhibitionViolation,
};

#[cfg(any(test, feature = "test-bypass"))]
pub use consent::caps_grant_for_test;

/// Crate version string ; mirrors the `cssl-*` scaffold convention.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}

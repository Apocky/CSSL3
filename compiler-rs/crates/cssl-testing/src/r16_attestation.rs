//! R16 C99-anchor reproducibility-attestation hook.
//!
//! § SPEC : `specs/01_BOOTSTRAP.csl` § REPRODUCIBILITY + §§ SYNTHESIS_V2 R16.
//! § GATE : T30 (OG10) ship-gate — C99-compiled stage3 ≡ CSSLv3-compiled stage1 bit-exact.
//! § CHAIN: attestation signed by Apocky-key + CI-key chain per `specs/01_BOOTSTRAP.csl`.
//! § STATUS : stage3 stub — infrastructure wired; real attestation at stage3 entry.

/// Single attestation record, Ed25519-signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    /// Semver of the compiler build that produced this attestation.
    pub compiler_version: String,
    /// Git SHA of the source commit (empty in stage0 / non-git contexts).
    pub source_commit: String,
    /// BLAKE3 hash of the emitted C99 tarball.
    pub c99_tarball_blake3: String,
    /// BLAKE3 hash of the stage1 (self-hosted) compiler binary.
    pub stage1_blake3: String,
    /// Ed25519 signature over the canonical serialization of the above fields.
    pub signature: Vec<u8>,
}

/// Outcome of running the R16 attestation pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — stage3 infrastructure pending.
    Stage0Unimplemented,
    /// C99 rebuild produced byte-exact stage1 binary; attestation signed.
    Attested { record: Attestation },
    /// Rebuild diverged; attestation refused.
    Diverged {
        expected_blake3: String,
        actual_blake3: String,
    },
    /// Attestation key material unavailable in this context (dev workstation sans keys).
    NoSigningKey,
}

/// Attester trait — stage3 implementation drives the rebuild + signing pipeline.
pub trait Attester {
    /// Emit a C99 tarball, rebuild stage1, compare byte-for-byte, sign the attestation.
    fn attest(&self) -> Outcome;
}

/// Stage0 stub attester — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Attester for Stage0Stub {
    fn attest(&self) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

#[cfg(test)]
mod tests {
    use super::{Attester, Outcome, Stage0Stub};

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(Stage0Stub.attest(), Outcome::Stage0Unimplemented);
    }
}

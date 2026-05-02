//! § ffi — extern "C" surface for loa-host registration.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Status codes (i32) :
//!    0 : ok
//!   -1 : null pointer
//!   -2 : invalid record
//!   -3 : cap denied
//!   -4 : not found
//!   -5 : window violated
//!
//! The host wires these via `extern "C"` declarations matching the
//! `Labyrinth of Apocalypse/systems/content_moderation.csl` module.

use crate::record::{FlagKind, FlagRecord};

pub const FFI_OK: i32 = 0;
pub const FFI_ERR_NULL: i32 = -1;
pub const FFI_ERR_INVALID: i32 = -2;
pub const FFI_ERR_CAP_DENIED: i32 = -3;
pub const FFI_ERR_NOT_FOUND: i32 = -4;
pub const FFI_ERR_WINDOW: i32 = -5;

/// Safe-Rust callable that returns the packed bytes — the canonical
/// pack-fn for non-FFI callers + integration-tests. The loa-host wires
/// this into its FFI-registry alongside its safe-Rust wrappers ; we do
/// NOT export raw extern-C symbols from this crate (forbid-unsafe-code).
pub fn flag_pack_safe(
    flagger_pubkey_hash: u64,
    content_id: u32,
    kind: u8,
    severity: u8,
    sigma_mask: u8,
    ts: u32,
    rationale_short: u64,
    sig_trunc: u32,
) -> Result<[u8; 32], i32> {
    let kind = FlagKind::from_u8(kind).ok_or(FFI_ERR_INVALID)?;
    let r = FlagRecord::pack(
        flagger_pubkey_hash,
        content_id,
        kind,
        severity,
        sigma_mask,
        ts,
        rationale_short,
        sig_trunc,
    )
    .map_err(|_| FFI_ERR_INVALID)?;
    Ok(r.raw)
}

/// Compute pubkey-handle (BLAKE3-trunc) from raw pubkey bytes.
pub fn pubkey_handle_safe(pubkey: &[u8]) -> u64 {
    FlagRecord::pubkey_handle(pubkey)
}

/// PRIME-DIRECTIVE attestation surface — deterministic + queryable from FFI.
pub fn prime_directive_attestation_safe() -> &'static str {
    crate::prime_directive_attestation()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_pack_safe_roundtrip() {
        let raw = flag_pack_safe(0xCAFE, 1, 0, 50, 0x01, 1_700_000_000, 0, 0).unwrap();
        let r = FlagRecord::from_raw_validated(raw).unwrap();
        assert_eq!(r.content_id(), 1);
        assert_eq!(r.severity(), 50);
        assert_eq!(r.flag_kind(), FlagKind::PrimeDirectiveViolation);
    }

    #[test]
    fn invalid_flag_kind_rejected() {
        let err = flag_pack_safe(0, 0, 99, 0, 0, 0, 0, 0).unwrap_err();
        assert_eq!(err, FFI_ERR_INVALID);
    }

    #[test]
    fn attestation_string_contains_invariants() {
        let s = prime_directive_attestation_safe();
        assert!(s.contains("NO-shadowban"));
        assert!(s.contains("Sigma-Chain-anchor"));
        assert!(s.contains("sovereign-revoke"));
        assert!(s.contains("author-transparent"));
        assert!(s.contains("flagger-revocable"));
        assert!(s.contains("30d-appeal-window"));
        assert!(s.contains("7d-auto-restore"));
    }
}

//! ID newtypes : `HomeId` / `Pubkey` / `Timestamp`.
//!
//! All collection-keys are `Copy + Ord + Hash` and `serde`-roundtrip safe.

use serde::{Deserialize, Serialize};

/// Unique 64-bit identifier of a Home pocket-dimension.
///
/// Allocation policy is `cssl-host-home-dimension`-internal and not exposed —
/// callers receive a `HomeId` from [`crate::Home::new`] and pass it back in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HomeId(pub u64);

impl HomeId {
    /// Returns the raw `u64` for FFI/serialization callers.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Ed25519-style public-key handle (32 bytes ; opaque to this crate).
///
/// Used as owner-key + visitor-key + companion-key + memorial-author-key.
/// This crate does **not** verify signatures — that lives in
/// `cssl-host-attestation` / `cssl-host-mycelium`. Here it is merely a
/// deterministic-orderable key for cap-gated access maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pubkey(pub [u8; 32]);

impl Pubkey {
    /// All-zero key, useful as a sentinel in tests + default-construction.
    #[must_use]
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Deterministic small-domain test-helper : produce a pubkey whose first
    /// byte is `seed` and the rest are zero. **Do not** use outside tests.
    #[must_use]
    pub const fn from_seed(seed: u8) -> Self {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        Self(bytes)
    }
}

/// Monotonic millisecond timestamp (caller-supplied for determinism).
///
/// This crate never reads the wall-clock — every API that needs a timestamp
/// takes one as an argument. That keeps tests deterministic and lets the
/// host-runtime advance time however it sees fit (frame-tick / replay / etc).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Time-zero sentinel.
    #[must_use]
    pub const fn zero() -> Self {
        Self(0)
    }
}

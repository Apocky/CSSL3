//! § cssl-host-mycelium — mycelial-network protocol layer
//!
//! § T11-W8-B2 : substrate-as-mycelium ↔ cross-user federated-learning
//!
//! ## Purpose
//!
//! Implements the cross-user signaling layer described by
//! `specs/grand-vision/16_MYCELIAL_NETWORK.csl`. The substrate is modelled as
//! mycelium: ω-field = hyphae, Σ-mask = membrane, KAN = nutrient-pathway,
//! HDC = signaling. This crate provides the *signaling* sub-layer:
//!
//! - **Spore-emit** — opt-in, capability-gated cross-user events.
//! - **Nutrient-poll** — region-scoped query for spores at-or-below caller's
//!   tier.
//! - **Federated-aggregate** — DP-noised, sample-floored bias aggregation.
//!
//! ## PRIME-DIRECTIVE invariants
//!
//! - `#![forbid(unsafe_code)]` — no `unsafe` anywhere in this crate.
//! - `Sensitive<biometric|gaze|face|body>` is structurally stripped *at emit*
//!   so it cannot leave the local sovereign boundary.
//! - DP-aggregate refuses to return data when sample count < 100.
//! - Tier escalation is forbidden: a poll at tier `T` only returns spores
//!   whose authors emitted at tier ≤ `T`.
//! - DP-noise is deterministic (seeded from region + time-bucket) so the
//!   same query at the same epoch returns the same answer for replay.
//!
//! ## Module map
//!
//! - [`spore`]     — Spore + SporeKind + SporeId + emit-pipeline.
//! - [`nutrient`]  — NutrientQuery + filter pipeline.
//! - [`aggregate`] — AggregatedBias + DP-floor + Gaussian-noise.
//! - [`privacy`]   — OptInTier ordering + sensitive-strip helpers + DP RNG.
//! - [`transport_adapter`] — pluggable transport trait (mocks if upstream
//!   `cssl-host-mp-transport-real` is unavailable in the build graph).

#![forbid(unsafe_code)]
#![doc(html_no_source)]

pub mod aggregate;
pub mod nutrient;
pub mod privacy;
pub mod spore;
pub mod transport_adapter;

pub use aggregate::{AggregateError, AggregatedBias, DP_SAMPLE_FLOOR};
pub use nutrient::{NutrientQuery, NutrientResponse};
pub use privacy::{OptInTier, RegionTag, SensitiveField};
pub use spore::{Spore, SporeId, SporeKind, SporePayload};
pub use transport_adapter::{
    InMemoryTransport, MyceliumError, MyceliumTransport, TransportAdapter,
};

/// Crate-wide version stamp emitted into audit lines.
pub const MYCELIUM_PROTOCOL_VERSION: u32 = 1;

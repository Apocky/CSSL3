//! Asset-reference trait + opaque concrete type.
//!
//! `cssl-host-home-dimension` does **not** depend on `cssl-host-asset-bundle`
//! directly — that would couple the Home crate to LAB-v1 plumbing and force
//! all consumers (tests, scenes, edge-routes) to bring in the bundle crate.
//! Instead, this crate exposes [`AssetRef`] as a lightweight handle trait and
//! [`OpaqueAsset`] as a concrete `(u64, String)` impl that downstream callers
//! can use directly or wrap their own bundle-handle in.
//!
//! When a consumer is also using `cssl-host-asset-bundle`, they implement
//! [`AssetRef`] for `BundleManifest`-keyed handles ; the impl lives in their
//! crate, which keeps the dependency direction acyclic.

use serde::{Deserialize, Serialize};

/// Lightweight asset-reference handle.
///
/// Decorations / trophies / portals all hold an `AssetRef` so consumers can
/// resolve to whatever asset-system they prefer (LAB v1 bundle id, GLTF path,
/// runtime-procgen-blueprint hash, etc).
pub trait AssetRef: core::fmt::Debug {
    /// Stable 64-bit asset id (FNV-1a-128 prefix / bundle-id / etc).
    fn id(&self) -> u64;
    /// Short human-tag (filename / kind / blueprint-name) for logs + tests.
    fn tag(&self) -> &str;
}

/// Default opaque concrete impl — `(id, tag)` pair.
///
/// Suitable for tests + simple host-runtime callers that don't yet wire
/// to a richer asset-bundle. Holds owned `String` for the tag so the
/// struct is `Send + Sync + Serialize` without lifetime gymnastics.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OpaqueAsset {
    /// Stable 64-bit asset identifier.
    pub id: u64,
    /// Short human-readable tag.
    pub tag: String,
}

impl OpaqueAsset {
    /// Build a new opaque-asset handle.
    #[must_use]
    pub fn new(id: u64, tag: impl Into<String>) -> Self {
        Self {
            id,
            tag: tag.into(),
        }
    }
}

impl AssetRef for OpaqueAsset {
    fn id(&self) -> u64 {
        self.id
    }
    fn tag(&self) -> &str {
        &self.tag
    }
}

//! § sources — pluggable manifest + bundle fetchers.
//!
//! Production : HTTP-backed adapters live in a sibling out-of-tree crate.
//! Mycelium-peer fallback is implemented via a chain-of-`BundleSource`s.
//! Tests : `MockManifestSource` + `MockBundleSource` simulate the wire
//! end-to-end without touching the network.

use cssl_hotfix::manifest::Manifest;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("network error : {0}")]
    Network(String),
    #[error("manifest parse error : {0}")]
    Parse(String),
    #[error("bundle not found : channel={channel} version={version}")]
    BundleNotFound { channel: String, version: String },
    #[error("rate limited : retry after {0} ms")]
    RateLimited(u64),
}

/// § A source of signed manifests (apocky.com `/api/hotfix/manifest`).
pub trait ManifestSource: Send + Sync {
    fn fetch_manifest(&self) -> Result<Manifest, SourceError>;
}

/// § A source of bundle bytes. Production : HTTP first, then mycelium-peers.
pub trait BundleSource: Send + Sync {
    fn fetch_bundle(&self, channel: &str, version: &str) -> Result<Vec<u8>, SourceError>;
}

/// § Mock manifest source : returns the same manifest each call.
pub struct MockManifestSource {
    manifest: std::sync::Mutex<Result<Manifest, SourceError>>,
}

impl MockManifestSource {
    #[must_use]
    pub fn new(m: Manifest) -> Self {
        Self {
            manifest: std::sync::Mutex::new(Ok(m)),
        }
    }
    pub fn set(&self, m: Manifest) {
        *self.manifest.lock().unwrap() = Ok(m);
    }
    pub fn set_error(&self, e: SourceError) {
        *self.manifest.lock().unwrap() = Err(e);
    }
}

impl ManifestSource for MockManifestSource {
    fn fetch_manifest(&self) -> Result<Manifest, SourceError> {
        self.manifest
            .lock()
            .unwrap()
            .as_ref()
            .map(Clone::clone)
            .map_err(|e| SourceError::Network(e.to_string()))
    }
}

/// § Mock bundle source : in-memory `(channel, version) → bytes` map.
#[derive(Default)]
pub struct MockBundleSource {
    map: std::sync::Mutex<BTreeMap<(String, String), Vec<u8>>>,
}

impl MockBundleSource {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn put(&self, channel: &str, version: &str, bytes: Vec<u8>) {
        self.map
            .lock()
            .unwrap()
            .insert((channel.to_string(), version.to_string()), bytes);
    }
}

impl BundleSource for MockBundleSource {
    fn fetch_bundle(&self, channel: &str, version: &str) -> Result<Vec<u8>, SourceError> {
        self.map
            .lock()
            .unwrap()
            .get(&(channel.to_string(), version.to_string()))
            .cloned()
            .ok_or_else(|| SourceError::BundleNotFound {
                channel: channel.to_string(),
                version: version.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_hotfix::cap::CapRole;
    use cssl_hotfix::manifest::Manifest;

    fn empty_manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            generated_at_ns: 0,
            signed_by: CapRole::CapA,
            channels: Default::default(),
            revocations: vec![],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn mock_manifest_source_returns_set_value() {
        let m = empty_manifest();
        let src = MockManifestSource::new(m.clone());
        assert_eq!(src.fetch_manifest().unwrap(), m);
    }

    #[test]
    fn mock_manifest_source_can_error() {
        let src = MockManifestSource::new(empty_manifest());
        src.set_error(SourceError::RateLimited(500));
        assert!(src.fetch_manifest().is_err());
    }

    #[test]
    fn mock_bundle_source_returns_put_value() {
        let src = MockBundleSource::new();
        src.put("cssl.bundle", "1.0.0", vec![1, 2, 3]);
        assert_eq!(
            src.fetch_bundle("cssl.bundle", "1.0.0").unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn mock_bundle_source_missing_errors() {
        let src = MockBundleSource::new();
        let r = src.fetch_bundle("cssl.bundle", "1.0.0");
        assert!(matches!(r, Err(SourceError::BundleNotFound { .. })));
    }
}

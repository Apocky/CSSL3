//! Persistence-backend trait + in-memory reference impl.

use std::collections::HashMap;

use thiserror::Error;

use crate::image::{ImageRecord, PersistenceImage};
use crate::schema::SchemaVersion;

/// Persistence failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PersistError {
    /// Record with the given key was not found.
    #[error("record `{key}` not found")]
    NotFound { key: String },
    /// Schema mismatch during read.
    #[error("schema mismatch : record has version {found} but caller expects {expected}")]
    SchemaMismatch {
        found: SchemaVersion,
        expected: SchemaVersion,
    },
    /// WAL / LMDB backend not yet implemented.
    #[error("backend `{backend}` not wired at stage-0 (T11-phase-2 delivers WAL + LMDB)")]
    BackendNotWired { backend: &'static str },
}

/// Persistence-backend trait.
pub trait PersistenceBackend {
    /// Backend short-name.
    fn name(&self) -> &'static str;
    /// Write a record (overwrites on key-collision).
    ///
    /// # Errors
    /// Returns [`PersistError::BackendNotWired`] for stub backends.
    fn put(&mut self, record: ImageRecord) -> Result<(), PersistError>;
    /// Read a record by key.
    ///
    /// # Errors
    /// Returns [`PersistError::NotFound`] if the key is absent.
    fn get(&self, key: &str) -> Result<ImageRecord, PersistError>;
    /// Snapshot the whole image.
    ///
    /// # Errors
    /// Returns [`PersistError::BackendNotWired`] for stub backends that can't snapshot.
    fn snapshot(
        &self,
        timestamp_s: u64,
        schema: SchemaVersion,
    ) -> Result<PersistenceImage, PersistError>;
    /// Record count currently in the store.
    fn len(&self) -> usize;
    /// Empty check.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Stage-0 reference impl : `HashMap<String, ImageRecord>`.
#[derive(Debug, Clone, Default)]
pub struct InMemoryBackend {
    records: HashMap<String, ImageRecord>,
    insertion_order: Vec<String>,
}

impl InMemoryBackend {
    /// New empty backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl PersistenceBackend for InMemoryBackend {
    fn name(&self) -> &'static str {
        "in-memory"
    }

    fn put(&mut self, record: ImageRecord) -> Result<(), PersistError> {
        let key = record.key.clone();
        if !self.records.contains_key(&key) {
            self.insertion_order.push(key.clone());
        }
        self.records.insert(key, record);
        Ok(())
    }

    fn get(&self, key: &str) -> Result<ImageRecord, PersistError> {
        self.records
            .get(key)
            .cloned()
            .ok_or_else(|| PersistError::NotFound {
                key: key.to_string(),
            })
    }

    fn snapshot(
        &self,
        timestamp_s: u64,
        schema: SchemaVersion,
    ) -> Result<PersistenceImage, PersistError> {
        let mut img = PersistenceImage::new(schema, timestamp_s);
        for key in &self.insertion_order {
            if let Some(r) = self.records.get(key) {
                img.push_record(r.clone());
            }
        }
        Ok(img)
    }

    fn len(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{InMemoryBackend, PersistError, PersistenceBackend};
    use crate::image::ImageRecord;
    use crate::schema::SchemaVersion;

    fn rec(key: &str, payload: &[u8]) -> ImageRecord {
        ImageRecord::new(key, SchemaVersion::genesis(), payload.to_vec())
    }

    #[test]
    fn new_backend_is_empty() {
        let b = InMemoryBackend::new();
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
        assert_eq!(b.name(), "in-memory");
    }

    #[test]
    fn put_and_get() {
        let mut b = InMemoryBackend::new();
        b.put(rec("k", b"v")).unwrap();
        let r = b.get("k").unwrap();
        assert_eq!(r.payload, b"v");
    }

    #[test]
    fn get_missing_returns_not_found() {
        let b = InMemoryBackend::new();
        let err = b.get("missing").unwrap_err();
        assert!(matches!(err, PersistError::NotFound { .. }));
    }

    #[test]
    fn put_overwrites_same_key_once_in_order() {
        let mut b = InMemoryBackend::new();
        b.put(rec("k", b"v1")).unwrap();
        b.put(rec("k", b"v2")).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b.get("k").unwrap().payload, b"v2");
    }

    #[test]
    fn snapshot_preserves_insertion_order() {
        let mut b = InMemoryBackend::new();
        b.put(rec("a", b"1")).unwrap();
        b.put(rec("b", b"2")).unwrap();
        b.put(rec("c", b"3")).unwrap();
        let img = b.snapshot(1_000, SchemaVersion::genesis()).unwrap();
        let keys: Vec<_> = img.records.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn snapshot_record_count_correct() {
        let mut b = InMemoryBackend::new();
        for i in 0..5 {
            b.put(rec(&format!("k{i}"), &[i as u8])).unwrap();
        }
        let img = b.snapshot(0, SchemaVersion::genesis()).unwrap();
        assert_eq!(img.records.len(), 5);
        assert_eq!(img.header.record_count, 5);
    }
}

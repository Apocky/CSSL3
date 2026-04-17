//! Persistence-image header + record + in-memory representation.

use crate::schema::SchemaVersion;

/// Persistence-image header (embedded in a WAL-file or LMDB meta-record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageHeader {
    /// Magic bytes (ASCII `"CSSLPRS1"`).
    pub magic: [u8; 8],
    /// Format version (bumped on header-layout change).
    pub format_version: u32,
    /// Schema version of the records stored.
    pub schema: SchemaVersion,
    /// Unix timestamp (seconds) when the image was written.
    pub timestamp_s: u64,
    /// Record count.
    pub record_count: u32,
    /// 32-byte BLAKE3 digest of all records concatenated (stage-0 stubbed).
    pub content_digest: [u8; 32],
}

impl ImageHeader {
    /// Canonical magic (ASCII "CSSLPRS1").
    pub const MAGIC: [u8; 8] = *b"CSSLPRS1";

    /// New header with `MAGIC` + `format_version = 1` + zero digest.
    #[must_use]
    pub fn new(schema: SchemaVersion, timestamp_s: u64) -> Self {
        Self {
            magic: Self::MAGIC,
            format_version: 1,
            schema,
            timestamp_s,
            record_count: 0,
            content_digest: [0u8; 32],
        }
    }
}

/// One record in the persistence image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRecord {
    /// Stable key (UTF-8 identifier).
    pub key: String,
    /// Schema version of this record's serialized-form.
    pub schema: SchemaVersion,
    /// Serialized payload (opaque bytes).
    pub payload: Vec<u8>,
}

impl ImageRecord {
    /// New record.
    #[must_use]
    pub fn new(key: impl Into<String>, schema: SchemaVersion, payload: Vec<u8>) -> Self {
        Self {
            key: key.into(),
            schema,
            payload,
        }
    }

    /// Payload-size accessor.
    #[must_use]
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }
}

/// In-memory persistence-image : header + records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceImage {
    /// Header block.
    pub header: ImageHeader,
    /// Records in insertion-order.
    pub records: Vec<ImageRecord>,
}

impl PersistenceImage {
    /// Build an empty image with the given schema + timestamp.
    #[must_use]
    pub fn new(schema: SchemaVersion, timestamp_s: u64) -> Self {
        Self {
            header: ImageHeader::new(schema, timestamp_s),
            records: Vec::new(),
        }
    }

    /// Append a record + refresh header.record_count. The content-digest is
    /// stage-0 stub (phase-2 swaps for BLAKE3 over a canonical form).
    pub fn push_record(&mut self, r: ImageRecord) {
        self.records.push(r);
        self.header.record_count = u32::try_from(self.records.len()).unwrap_or(u32::MAX);
        self.header.content_digest = stub_content_digest(&self.records);
    }

    /// Find a record by key.
    #[must_use]
    pub fn find(&self, key: &str) -> Option<&ImageRecord> {
        self.records.iter().find(|r| r.key == key)
    }

    /// Total payload-size across records.
    #[must_use]
    pub fn total_payload_size(&self) -> usize {
        self.records.iter().map(ImageRecord::payload_size).sum()
    }
}

fn stub_content_digest(records: &[ImageRecord]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, r) in records.iter().enumerate() {
        for (j, b) in r.key.as_bytes().iter().enumerate() {
            out[(i + j) % 32] ^= b.rotate_left(u32::try_from((i + j) % 8).unwrap_or(0));
        }
        for (j, b) in r.payload.iter().enumerate() {
            out[(i * 7 + j) % 32] ^= b.rotate_left(u32::try_from(j % 8).unwrap_or(0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{ImageHeader, ImageRecord, PersistenceImage};
    use crate::schema::SchemaVersion;

    #[test]
    fn header_has_canonical_magic() {
        let h = ImageHeader::new(SchemaVersion::genesis(), 1_000);
        assert_eq!(h.magic, *b"CSSLPRS1");
        assert_eq!(h.format_version, 1);
    }

    #[test]
    fn new_image_is_empty() {
        let img = PersistenceImage::new(SchemaVersion::genesis(), 0);
        assert!(img.records.is_empty());
        assert_eq!(img.header.record_count, 0);
        assert_eq!(img.total_payload_size(), 0);
    }

    #[test]
    fn push_record_updates_count_and_digest() {
        let mut img = PersistenceImage::new(SchemaVersion::genesis(), 0);
        img.push_record(ImageRecord::new(
            "user/1",
            SchemaVersion::genesis(),
            b"bytes-1".to_vec(),
        ));
        img.push_record(ImageRecord::new(
            "user/2",
            SchemaVersion::genesis(),
            b"bytes-22".to_vec(),
        ));
        assert_eq!(img.header.record_count, 2);
        assert_ne!(img.header.content_digest, [0u8; 32]);
        assert_eq!(img.total_payload_size(), 7 + 8);
    }

    #[test]
    fn find_locates_record_by_key() {
        let mut img = PersistenceImage::new(SchemaVersion::genesis(), 0);
        img.push_record(ImageRecord::new(
            "alpha",
            SchemaVersion::genesis(),
            b"a".to_vec(),
        ));
        img.push_record(ImageRecord::new(
            "beta",
            SchemaVersion::genesis(),
            b"b".to_vec(),
        ));
        let r = img.find("beta").unwrap();
        assert_eq!(r.payload, b"b");
        assert!(img.find("gamma").is_none());
    }

    #[test]
    fn digest_stable_for_same_records() {
        let mut a = PersistenceImage::new(SchemaVersion::genesis(), 0);
        let mut b = PersistenceImage::new(SchemaVersion::genesis(), 0);
        a.push_record(ImageRecord::new(
            "k",
            SchemaVersion::genesis(),
            b"v".to_vec(),
        ));
        b.push_record(ImageRecord::new(
            "k",
            SchemaVersion::genesis(),
            b"v".to_vec(),
        ));
        assert_eq!(a.header.content_digest, b.header.content_digest);
    }

    #[test]
    fn record_payload_size() {
        let r = ImageRecord::new("x", SchemaVersion::genesis(), vec![1, 2, 3, 4, 5]);
        assert_eq!(r.payload_size(), 5);
    }
}

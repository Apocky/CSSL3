// § registry.rs — LicenseRegistry : keyed map + reports + filters
// I> register-idempotent on identical-license · reject conflict
// I> filter_compatible / filter_requires_attribution = HUD ingredients
// I> report_attribution_text = multiline-HUD scroll · report_attribution_jsonl = log-stream

use crate::asset::AssetLicenseRecord;
use crate::license::License;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Errors that may arise while mutating the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegErr {
    /// Two different licenses claimed for the same `asset_id`.
    DuplicateConflict {
        /// The offending asset id.
        asset_id: String,
        /// License already on file.
        existing: License,
        /// License attempted to be inserted.
        new: License,
    },
}

impl fmt::Display for RegErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegErr::DuplicateConflict {
                asset_id,
                existing,
                new,
            } => write!(
                f,
                "duplicate-conflict for asset_id='{asset_id}': existing={existing:?} new={new:?}"
            ),
        }
    }
}

impl std::error::Error for RegErr {}

/// In-memory registry of asset license records, keyed by `asset_id`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LicenseRegistry {
    records: HashMap<String, AssetLicenseRecord>,
}

impl LicenseRegistry {
    /// New empty registry.
    pub fn new() -> Self {
        LicenseRegistry {
            records: HashMap::new(),
        }
    }

    /// Number of records currently stored.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True iff the registry has no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Insert (or update-if-same-license) a record.
    ///
    /// Behavior :
    ///   - asset_id absent → insert
    ///   - asset_id present, same license → overwrite (idempotent metadata refresh)
    ///   - asset_id present, different license → `Err(DuplicateConflict)`
    pub fn register(&mut self, record: AssetLicenseRecord) -> Result<(), RegErr> {
        if let Some(existing) = self.records.get(&record.asset_id) {
            if existing.license != record.license {
                return Err(RegErr::DuplicateConflict {
                    asset_id: record.asset_id.clone(),
                    existing: existing.license.clone(),
                    new: record.license.clone(),
                });
            }
        }
        self.records.insert(record.asset_id.clone(), record);
        Ok(())
    }

    /// Lookup by asset id.
    pub fn get(&self, asset_id: &str) -> Option<&AssetLicenseRecord> {
        self.records.get(asset_id)
    }

    /// Iterator over records whose license is LoA-compatible.
    pub fn filter_compatible(&self) -> impl Iterator<Item = &AssetLicenseRecord> {
        self.records.values().filter(|r| r.is_loa_compatible())
    }

    /// Iterator over records whose license requires attribution.
    pub fn filter_requires_attribution(&self) -> impl Iterator<Item = &AssetLicenseRecord> {
        self.records
            .values()
            .filter(|r| r.license.requires_attribution())
    }

    /// Multi-line plain-text HUD report, sorted by asset_id for determinism.
    pub fn report_attribution_text(&self) -> String {
        let mut keys: Vec<&String> = self.records.keys().collect();
        keys.sort();
        let mut out = String::new();
        for k in keys {
            if let Some(rec) = self.records.get(k) {
                out.push_str(&rec.attribution_text());
                out.push('\n');
            }
        }
        out
    }

    /// JSON-lines stream, sorted by asset_id for determinism.
    /// Each line is a serialized `AssetLicenseRecord`.
    pub fn report_attribution_jsonl(&self) -> String {
        let mut keys: Vec<&String> = self.records.keys().collect();
        keys.sort();
        let mut out = String::new();
        for k in keys {
            if let Some(rec) = self.records.get(k) {
                if let Ok(json) = serde_json::to_string(rec) {
                    out.push_str(&json);
                    out.push('\n');
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, lic: License, author: Option<&str>) -> AssetLicenseRecord {
        AssetLicenseRecord {
            asset_id: id.into(),
            source: "test".into(),
            license: lic,
            author: author.map(Into::into),
            source_url: Some(format!("https://x.test/{id}")),
            fetched_at_iso: "2026-04-30T12:00:00Z".into(),
            sha256: None,
        }
    }

    #[test]
    fn empty_registry() {
        let r = LicenseRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert!(r.get("nope").is_none());
        assert_eq!(r.filter_compatible().count(), 0);
        assert_eq!(r.report_attribution_text(), "");
        assert_eq!(r.report_attribution_jsonl(), "");
    }

    #[test]
    fn register_idempotent_on_same_license() {
        let mut r = LicenseRegistry::new();
        r.register(rec("a1", License::CC0, None)).unwrap();
        // re-register same id + same license → ok
        r.register(rec("a1", License::CC0, Some("Author")))
            .unwrap();
        assert_eq!(r.len(), 1);
        // updated author should be visible after the second register.
        assert_eq!(r.get("a1").unwrap().author.as_deref(), Some("Author"));
    }

    #[test]
    fn register_rejects_conflict() {
        let mut r = LicenseRegistry::new();
        r.register(rec("a1", License::CC0, None)).unwrap();
        let err = r
            .register(rec("a1", License::CCBYNC40, None))
            .expect_err("conflict expected");
        match err {
            RegErr::DuplicateConflict {
                asset_id,
                existing,
                new,
            } => {
                assert_eq!(asset_id, "a1");
                assert_eq!(existing, License::CC0);
                assert_eq!(new, License::CCBYNC40);
            }
        }
    }

    #[test]
    fn filter_compatible_excludes_cc_nc() {
        let mut r = LicenseRegistry::new();
        r.register(rec("ok1", License::CC0, None)).unwrap();
        r.register(rec("ok2", License::CCBY40, Some("A"))).unwrap();
        r.register(rec("nope", License::CCBYNC40, Some("B")))
            .unwrap();
        r.register(rec("prop", License::ProprietaryUnlicensed, None))
            .unwrap();
        let ids: Vec<&str> = r.filter_compatible().map(|r| r.asset_id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"ok1"));
        assert!(ids.contains(&"ok2"));
        assert!(!ids.contains(&"nope"));
        assert!(!ids.contains(&"prop"));
    }

    #[test]
    fn filter_attribution_includes_cc_by() {
        let mut r = LicenseRegistry::new();
        r.register(rec("free1", License::CC0, None)).unwrap();
        r.register(rec("attr1", License::CCBY40, Some("X"))).unwrap();
        r.register(rec("attr2", License::MITLike, Some("Y"))).unwrap();
        let ids: Vec<&str> = r
            .filter_requires_attribution()
            .map(|r| r.asset_id.as_str())
            .collect();
        assert!(ids.contains(&"attr1"));
        assert!(ids.contains(&"attr2"));
        assert!(!ids.contains(&"free1"));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn text_report_multiline() {
        let mut r = LicenseRegistry::new();
        r.register(rec("z_last", License::CC0, None)).unwrap();
        r.register(rec("a_first", License::CCBY40, Some("Author")))
            .unwrap();
        let text = r.report_attribution_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        // sorted by asset_id → a_first before z_last
        assert!(lines[0].contains("a_first"));
        assert!(lines[1].contains("z_last"));
        assert!(lines[0].contains("CC-BY-4.0"));
        assert!(lines[1].contains("CC0"));
    }

    #[test]
    fn jsonl_roundtrip() {
        let mut r = LicenseRegistry::new();
        r.register(rec("k1", License::CC0, None)).unwrap();
        r.register(rec("k2", License::CCBY40, Some("Auth"))).unwrap();
        let jsonl = r.report_attribution_jsonl();
        let lines: Vec<&str> = jsonl.lines().collect();
        assert_eq!(lines.len(), 2);
        // each line is valid JSON of an AssetLicenseRecord
        let mut got = LicenseRegistry::new();
        for line in lines {
            let rec: AssetLicenseRecord = serde_json::from_str(line).expect("valid jsonl line");
            got.register(rec).unwrap();
        }
        assert_eq!(got.len(), 2);
        assert!(got.get("k1").is_some());
        assert!(got.get("k2").is_some());
        assert_eq!(got.get("k2").unwrap().author.as_deref(), Some("Auth"));
    }
}

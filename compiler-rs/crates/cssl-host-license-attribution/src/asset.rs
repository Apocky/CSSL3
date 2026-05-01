// § asset.rs — AssetLicenseRecord : per-asset metadata + attribution-text
// I> attribution_text() = HUD-line · attribution_html() = anchor-tags
// I> missing_required_attribution() = "license-requires-author + author=None"

use crate::license::License;
use serde::{Deserialize, Serialize};

/// One asset's license + provenance record.
///
/// Stored in the `LicenseRegistry`, keyed by `asset_id`.
/// `fetched_at_iso` is an ISO-8601 timestamp of when the asset was downloaded.
/// `sha256` is optional content hash for cache-validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLicenseRecord {
    /// Stable identifier for the asset within LoA's content addressing.
    pub asset_id: String,
    /// Origin host or platform (e.g. "polyhaven", "kenney", "sketchfab").
    pub source: String,
    /// Recognized license.
    pub license: License,
    /// Author / creator name when known; required for CC-BY-style licenses.
    pub author: Option<String>,
    /// Original asset page URL.
    pub source_url: Option<String>,
    /// ISO-8601 fetch timestamp.
    pub fetched_at_iso: String,
    /// Optional SHA-256 of the downloaded bytes.
    pub sha256: Option<String>,
}

impl AssetLicenseRecord {
    /// Minimal constructor — useful for tests + ingest pipeline.
    pub fn new(asset_id: impl Into<String>, source: impl Into<String>, license: License) -> Self {
        AssetLicenseRecord {
            asset_id: asset_id.into(),
            source: source.into(),
            license,
            author: None,
            source_url: None,
            fetched_at_iso: String::new(),
            sha256: None,
        }
    }

    /// Plain-text HUD line. Format: `<asset_id> by <author> · <license> · <source_url>`
    /// Missing fields are gracefully elided.
    pub fn attribution_text(&self) -> String {
        let mut parts = vec![self.asset_id.clone()];
        if let Some(author) = &self.author {
            parts.push(format!("by {author}"));
        }
        parts.push(self.license.short_label().to_string());
        if let Some(url) = &self.source_url {
            parts.push(url.clone());
        }
        parts.join(" \u{00B7} ") // middle-dot separator
    }

    /// HTML attribution with anchor tags around URLs.
    /// Used by web-HUD or browser-overlay attribution panels.
    pub fn attribution_html(&self) -> String {
        let mut parts = vec![html_escape(&self.asset_id)];
        if let Some(author) = &self.author {
            parts.push(format!("by {}", html_escape(author)));
        }
        let lic_label = html_escape(self.license.short_label());
        if let Some(url) = self.license.full_url() {
            parts.push(format!(
                "<a href=\"{}\" rel=\"license noopener\">{}</a>",
                html_escape(url),
                lic_label
            ));
        } else {
            parts.push(lic_label);
        }
        if let Some(url) = &self.source_url {
            parts.push(format!(
                "<a href=\"{0}\" rel=\"noopener\">{0}</a>",
                html_escape(url)
            ));
        }
        parts.join(" \u{00B7} ")
    }

    /// Delegates to license-level LoA compatibility.
    pub fn is_loa_compatible(&self) -> bool {
        self.license.is_loa_compatible()
    }

    /// True iff the license requires attribution AND author is missing.
    /// Indicates a record-quality bug that must be fixed before shipping.
    pub fn missing_required_attribution(&self) -> bool {
        self.license.requires_attribution() && self.author.is_none()
    }
}

/// Minimal HTML-attribute / text escaper — handles the 5 reserved chars.
/// Sufficient for attribution-HUD strings; not a general-purpose sanitizer.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc_by_record() -> AssetLicenseRecord {
        AssetLicenseRecord {
            asset_id: "tree_oak_01".into(),
            source: "polyhaven".into(),
            license: License::CCBY40,
            author: Some("Rico Cilliers".into()),
            source_url: Some("https://polyhaven.com/a/tree_oak_01".into()),
            fetched_at_iso: "2026-04-30T12:00:00Z".into(),
            sha256: Some("deadbeef".into()),
        }
    }

    #[test]
    fn record_roundtrip() {
        let rec = cc_by_record();
        let json = serde_json::to_string(&rec).expect("serialize");
        let back: AssetLicenseRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, back);
    }

    #[test]
    fn attribution_text_format() {
        let rec = cc_by_record();
        let text = rec.attribution_text();
        assert!(text.contains("tree_oak_01"));
        assert!(text.contains("Rico Cilliers"));
        assert!(text.contains("CC-BY-4.0"));
        assert!(text.contains("polyhaven.com"));
        // Each segment separated by middle-dot.
        assert_eq!(text.split(" \u{00B7} ").count(), 4);
    }

    #[test]
    fn attribution_html_anchors() {
        let rec = cc_by_record();
        let html = rec.attribution_html();
        // Two anchor tags: one for the license URL, one for the source URL.
        let anchor_count = html.matches("<a ").count();
        assert_eq!(anchor_count, 2, "html={html}");
        assert!(html.contains("https://creativecommons.org/licenses/by/4.0/"));
        assert!(html.contains("CC-BY-4.0"));
        assert!(html.contains("rel=\"license noopener\""));
    }

    #[test]
    fn missing_attribution_detected() {
        let mut rec = cc_by_record();
        rec.author = None;
        assert!(rec.missing_required_attribution());
    }

    #[test]
    fn cc0_no_attribution_need_when_author_absent() {
        let rec = AssetLicenseRecord {
            asset_id: "rock_03".into(),
            source: "kenney".into(),
            license: License::CC0,
            author: None,
            source_url: None,
            fetched_at_iso: "2026-04-30T12:01:00Z".into(),
            sha256: None,
        };
        // CC0 does not require attribution, so author=None is fine.
        assert!(!rec.missing_required_attribution());
        // Still LoA-compatible.
        assert!(rec.is_loa_compatible());
    }

    #[test]
    fn cc_by_flags_missing_author() {
        let rec = AssetLicenseRecord {
            asset_id: "stairs_metal".into(),
            source: "opengameart".into(),
            license: License::CCBY40,
            author: None,
            source_url: Some("https://opengameart.org/x".into()),
            fetched_at_iso: "2026-04-30T12:02:00Z".into(),
            sha256: None,
        };
        assert!(rec.missing_required_attribution());
        // attribution_text still emits the license + url even without author.
        let txt = rec.attribution_text();
        assert!(txt.contains("CC-BY-4.0"));
        assert!(!txt.contains(" by "));
    }

    #[test]
    fn html_escape_handles_special_chars() {
        let rec = AssetLicenseRecord {
            asset_id: "<script>".into(),
            source: "test".into(),
            license: License::MITLike,
            author: Some("a&b".into()),
            source_url: Some("https://x.test/?q=1&r=\"2\"".into()),
            fetched_at_iso: "2026-04-30T12:03:00Z".into(),
            sha256: None,
        };
        let html = rec.attribution_html();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("a&amp;b"));
        assert!(html.contains("&quot;2&quot;"));
    }
}

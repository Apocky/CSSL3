//! § SpecCoverageReport — exportable report consumed by MCP `read_spec_coverage`
//!
//! The matrix in [`crate::matrix`] is a flat table ; this module produces
//! the **report** flavour : a denormalized, human-friendly version with
//! curated buckets (gaps / pending / deferred), grouped by spec-file.
//!
//! Output formats :
//!   - [`ExportFormat::Json`] — compact JSON (no extra deps)
//!   - [`ExportFormat::Markdown`] — pasteable to DECISIONS / SESSION reports
//!   - [`ExportFormat::Plain`] — plain-text terminal diagnostic

use crate::anchor::{ImplStatus, SpecAnchor};
use crate::matrix::CoverageMatrix;

/// One report row : a single anchor, projected for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportEntry {
    pub key: String,
    pub spec_root: String,
    pub spec_file: String,
    pub section: String,
    pub criterion: Option<String>,
    pub impl_status: String,
    pub test_status: String,
    pub crate_path: Option<String>,
    pub metric_count: usize,
    pub gaps: Vec<String>,
}

impl ReportEntry {
    pub fn from_anchor(anchor: &SpecAnchor) -> Self {
        let gaps = match &anchor.impl_status {
            ImplStatus::Partial { gaps, .. } => gaps.clone(),
            _ => Vec::new(),
        };
        Self {
            key: anchor.key(),
            spec_root: anchor.spec_root.as_str().to_string(),
            spec_file: anchor.spec_file.clone(),
            section: anchor.section.clone(),
            criterion: anchor.criterion.clone(),
            impl_status: anchor.impl_status.discriminant().to_string(),
            test_status: anchor.test_status.discriminant().to_string(),
            crate_path: anchor.impl_status.crate_path().map(String::from),
            metric_count: anchor.citing_metrics.len(),
            gaps,
        }
    }
}

/// The full coverage report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpecCoverageReport {
    pub total_anchors: usize,
    pub full_count: usize,
    pub partial_count: usize,
    pub missing_count: usize,
    pub coverage_percent: f64,
    pub gaps: Vec<ReportEntry>,
    pub partial: Vec<ReportEntry>,
    pub stale: Vec<ReportEntry>,
    pub pending_todos: Vec<ReportEntry>,
    pub deferred_items: Vec<ReportEntry>,
}

impl SpecCoverageReport {
    /// Build a report from a list of anchors.
    pub fn build<'a>(anchors: impl IntoIterator<Item = &'a SpecAnchor> + Clone) -> Self {
        let matrix = CoverageMatrix::from_anchors(anchors.clone().into_iter());
        let mut gaps = Vec::new();
        let mut partial = Vec::new();
        let mut stale = Vec::new();
        let mut pending = Vec::new();
        let mut deferred = Vec::new();
        for a in anchors.into_iter() {
            let entry = ReportEntry::from_anchor(a);
            if a.is_gap() {
                gaps.push(entry.clone());
                pending.push(entry.clone());
            }
            if matches!(a.impl_status, ImplStatus::Partial { .. }) {
                partial.push(entry.clone());
                if matches!(a.test_status, crate::anchor::TestStatus::Untested) {
                    pending.push(entry.clone());
                }
            }
            if a.is_stale() {
                stale.push(entry.clone());
            }
            if a.criterion.as_deref().map(|s| s.contains("deferred")).unwrap_or(false) {
                deferred.push(entry.clone());
            }
        }
        // Stable ordering by key for deterministic output.
        gaps.sort_by(|a, b| a.key.cmp(&b.key));
        partial.sort_by(|a, b| a.key.cmp(&b.key));
        stale.sort_by(|a, b| a.key.cmp(&b.key));
        pending.sort_by(|a, b| a.key.cmp(&b.key));
        deferred.sort_by(|a, b| a.key.cmp(&b.key));
        Self {
            total_anchors: matrix.total_anchors,
            full_count: matrix.full_count,
            partial_count: matrix.partial_count,
            missing_count: matrix.missing_count,
            coverage_percent: matrix.coverage_percent(),
            gaps,
            partial,
            stale,
            pending_todos: pending,
            deferred_items: deferred,
        }
    }

    /// Render in the requested format.
    pub fn export(&self, format: ExportFormat) -> String {
        match format {
            ExportFormat::Json => self.to_json(),
            ExportFormat::Markdown => self.to_markdown(),
            ExportFormat::Plain => self.to_plain(),
        }
    }

    fn to_json(&self) -> String {
        let mut s = String::new();
        s.push('{');
        s.push_str(&format!("\"total\":{},", self.total_anchors));
        s.push_str(&format!("\"full\":{},", self.full_count));
        s.push_str(&format!("\"partial\":{},", self.partial_count));
        s.push_str(&format!("\"missing\":{},", self.missing_count));
        s.push_str(&format!("\"coverage\":{:.4},", self.coverage_percent));
        s.push_str(&format!(
            "\"gap_count\":{},",
            self.gaps.len()
        ));
        s.push_str(&format!(
            "\"pending_count\":{},",
            self.pending_todos.len()
        ));
        s.push_str(&format!(
            "\"stale_count\":{},",
            self.stale.len()
        ));
        s.push_str(&format!(
            "\"deferred_count\":{}",
            self.deferred_items.len()
        ));
        s.push('}');
        s
    }

    fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("# CSSLv3 Spec-Coverage Report\n\n");
        s.push_str(&format!(
            "Total anchors : **{}**  |  Coverage : **{:.1}%**\n\n",
            self.total_anchors, self.coverage_percent
        ));
        s.push_str(&format!(
            "- Full     : {}\n- Partial  : {}\n- Missing  : {}\n\n",
            self.full_count, self.partial_count, self.missing_count
        ));
        s.push_str(&format!(
            "## GAP-LIST ({} anchors)\n\n",
            self.gaps.len()
        ));
        for entry in &self.gaps {
            s.push_str(&format!(
                "- `{}` :: `{}` :: `{}`\n",
                entry.spec_root, entry.spec_file, entry.section
            ));
        }
        s.push_str(&format!(
            "\n## PENDING TODOs ({} anchors)\n\n",
            self.pending_todos.len()
        ));
        for entry in &self.pending_todos {
            s.push_str(&format!(
                "- `{}` :: `{}` (impl={}, test={})\n",
                entry.spec_file, entry.section, entry.impl_status, entry.test_status
            ));
        }
        s.push_str(&format!(
            "\n## DEFERRED ITEMS ({} anchors)\n\n",
            self.deferred_items.len()
        ));
        for entry in &self.deferred_items {
            s.push_str(&format!(
                "- `{}` :: `{}` :: rationale={:?}\n",
                entry.spec_file,
                entry.section,
                entry.criterion.as_deref().unwrap_or("")
            ));
        }
        s.push_str(&format!(
            "\n## STALE ANCHORS ({} anchors)\n\n",
            self.stale.len()
        ));
        for entry in &self.stale {
            s.push_str(&format!(
                "- `{}` :: `{}`\n",
                entry.spec_file, entry.section
            ));
        }
        s
    }

    fn to_plain(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "[spec-coverage] total={} full={} partial={} missing={} cov={:.1}% gaps={} pending={} stale={}\n",
            self.total_anchors,
            self.full_count,
            self.partial_count,
            self.missing_count,
            self.coverage_percent,
            self.gaps.len(),
            self.pending_todos.len(),
            self.stale.len()
        ));
        for entry in &self.gaps {
            s.push_str(&format!(
                "  GAP   {}::{} ({})\n",
                entry.spec_root, entry.section, entry.impl_status
            ));
        }
        s
    }
}

/// Export format selectors for [`SpecCoverageReport::export`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Markdown,
    Plain,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{
        ImplConfidence, ImplStatus, SpecAnchorBuilder, SpecRoot, TestStatus,
    };

    fn full_anchor(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::Omniverse)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Implemented {
                crate_path: "cssl-foo".into(),
                primary_module: "crate::bar".into(),
                confidence: ImplConfidence::High,
                impl_date: "2026-04-29".into(),
            })
            .test_status(TestStatus::Tested {
                test_paths: vec!["t".into()],
                last_pass_date: "2026-04-29".into(),
            })
            .build()
    }

    fn missing_anchor(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Missing)
            .test_status(TestStatus::Untested)
            .build()
    }

    fn deferred_anchor(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .criterion("deferred to Wave-Jζ-final")
            .impl_status(ImplStatus::Missing)
            .test_status(TestStatus::Untested)
            .build()
    }

    #[test]
    fn report_classifies_buckets() {
        let anchors = vec![
            full_anchor("a", "§ I"),
            missing_anchor("b", "§ I"),
            deferred_anchor("c", "§ I"),
        ];
        let report = SpecCoverageReport::build(anchors.iter());
        assert_eq!(report.total_anchors, 3);
        assert_eq!(report.gaps.len(), 2); // missing + deferred (Missing impl)
        assert_eq!(report.deferred_items.len(), 1);
    }

    #[test]
    fn report_export_json_minimal() {
        let anchors = vec![full_anchor("a", "§ I")];
        let report = SpecCoverageReport::build(anchors.iter());
        let json = report.export(ExportFormat::Json);
        assert!(json.contains("\"total\":1"));
        assert!(json.contains("\"coverage\":100"));
    }

    #[test]
    fn report_export_markdown_contains_sections() {
        let anchors = vec![full_anchor("a", "§ I"), missing_anchor("b", "§ I")];
        let report = SpecCoverageReport::build(anchors.iter());
        let md = report.export(ExportFormat::Markdown);
        assert!(md.contains("# CSSLv3 Spec-Coverage Report"));
        assert!(md.contains("## GAP-LIST"));
        assert!(md.contains("## PENDING TODOs"));
        assert!(md.contains("## DEFERRED ITEMS"));
        assert!(md.contains("## STALE ANCHORS"));
    }

    #[test]
    fn report_export_plain_summary() {
        let anchors = vec![full_anchor("a", "§ I"), missing_anchor("b", "§ I")];
        let report = SpecCoverageReport::build(anchors.iter());
        let txt = report.export(ExportFormat::Plain);
        assert!(txt.starts_with("[spec-coverage]"));
        assert!(txt.contains("gaps=1"));
    }

    #[test]
    fn report_pending_todos_includes_partial_untested() {
        let mut p = full_anchor("a", "§ I");
        p.impl_status = ImplStatus::Partial {
            crate_path: "cssl-foo".into(),
            gaps: vec!["foo".into()],
        };
        p.test_status = TestStatus::Untested;
        let report = SpecCoverageReport::build([&p]);
        assert!(!report.pending_todos.is_empty());
    }

    #[test]
    fn report_export_format_variants_distinct() {
        let anchors = vec![full_anchor("a", "§ I")];
        let r = SpecCoverageReport::build(anchors.iter());
        let j = r.export(ExportFormat::Json);
        let m = r.export(ExportFormat::Markdown);
        let p = r.export(ExportFormat::Plain);
        assert_ne!(j, m);
        assert_ne!(j, p);
        assert_ne!(m, p);
    }

    #[test]
    fn report_entry_from_partial_carries_gaps() {
        let a = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::Omniverse)
            .spec_file("a")
            .section("§ I")
            .impl_status(ImplStatus::Partial {
                crate_path: "cssl-foo".into(),
                gaps: vec!["bar".into(), "baz".into()],
            })
            .build();
        let entry = ReportEntry::from_anchor(&a);
        assert_eq!(entry.gaps.len(), 2);
    }

    #[test]
    fn report_default_zero_anchors() {
        let r = SpecCoverageReport::default();
        assert_eq!(r.total_anchors, 0);
        let json = r.export(ExportFormat::Json);
        assert!(json.contains("\"total\":0"));
    }

    #[test]
    fn report_stale_anchor_detection() {
        let stale = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("a")
            .section("§ I")
            .spec_mtime("2026-04-29")
            .impl_mtime("2026-01-01")
            .impl_status(ImplStatus::Implemented {
                crate_path: "cssl-foo".into(),
                primary_module: "crate::bar".into(),
                confidence: ImplConfidence::Medium,
                impl_date: "stage0".into(),
            })
            .test_status(TestStatus::Tested {
                test_paths: vec!["t".into()],
                last_pass_date: "2026-04-29".into(),
            })
            .build();
        let r = SpecCoverageReport::build([&stale]);
        assert_eq!(r.stale.len(), 1);
    }
}

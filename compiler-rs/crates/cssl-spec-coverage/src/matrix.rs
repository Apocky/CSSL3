//! § CoverageMatrix — the most-important diagnostic artifact
//!
//! Per `06_l2_telemetry_spec.md` § IV.5 :
//!
//!   rows    = spec-§ entries (sorted by spec-file then section)
//!   columns = (ImplStatus, TestStatus, MetricCount, LastUpdate, Confidence)
//!   serializable : JSON, Markdown, Perfetto-overlay-track (Wave-J L3)
//!
//! In stage-0 we emit JSON and Markdown directly ; the Perfetto track
//! variant is deferred to Wave-Jθ-3 (perfetto bridge).

use crate::anchor::{ImplConfidence, ImplStatus, SpecAnchor, TestStatus};

/// One cell of the coverage matrix : a coarse classification suitable
/// for color-coding (green=full / yellow=partial / red=missing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoverageStatus {
    /// Implemented + Tested.
    Full,
    /// Has impl OR has tests, but not both ; or Partial impl status.
    Partial,
    /// Stub or Missing impl ; no test backing.
    Missing,
}

impl CoverageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CoverageStatus::Full => "Full",
            CoverageStatus::Partial => "Partial",
            CoverageStatus::Missing => "Missing",
        }
    }

    pub fn ansi_marker(&self) -> &'static str {
        match self {
            CoverageStatus::Full => "[+]",
            CoverageStatus::Partial => "[~]",
            CoverageStatus::Missing => "[ ]",
        }
    }

    /// Reduce an (impl, test) pair into a single matrix cell.
    pub fn from_anchor(anchor: &SpecAnchor) -> Self {
        let impl_ok = anchor.impl_status.is_implemented();
        let test_ok = anchor.test_status.is_tested();
        match (impl_ok, test_ok) {
            (true, true) => CoverageStatus::Full,
            (false, false) => match (&anchor.impl_status, &anchor.test_status) {
                (ImplStatus::Missing, _) => CoverageStatus::Missing,
                (ImplStatus::Stub { .. }, _) => CoverageStatus::Missing,
                (_, TestStatus::Untested) => CoverageStatus::Partial,
                _ => CoverageStatus::Partial,
            },
            _ => CoverageStatus::Partial,
        }
    }
}

/// One row of the coverage matrix — a denormalized projection of a
/// SpecAnchor for table rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRow {
    pub spec_root: String,
    pub spec_file: String,
    pub section: String,
    pub impl_status: String,
    pub test_status: String,
    pub metric_count: usize,
    pub last_update: Option<String>,
    pub confidence: Option<String>,
    pub status: CoverageStatus,
    pub crate_path: Option<String>,
    pub gaps: Vec<String>,
    pub criterion: Option<String>,
}

impl CoverageRow {
    pub fn from_anchor(anchor: &SpecAnchor) -> Self {
        let confidence = match &anchor.impl_status {
            ImplStatus::Implemented { confidence, .. } => Some(confidence.as_str().to_string()),
            _ => None,
        };
        let gaps = match &anchor.impl_status {
            ImplStatus::Partial { gaps, .. } => gaps.clone(),
            _ => Vec::new(),
        };
        let crate_path = anchor.impl_status.crate_path().map(String::from);
        Self {
            spec_root: anchor.spec_root.as_str().to_string(),
            spec_file: anchor.spec_file.clone(),
            section: anchor.section.clone(),
            impl_status: anchor.impl_status.discriminant().to_string(),
            test_status: anchor.test_status.discriminant().to_string(),
            metric_count: anchor.citing_metrics.len(),
            last_update: anchor.last_verified.clone(),
            confidence,
            status: CoverageStatus::from_anchor(anchor),
            crate_path,
            gaps,
            criterion: anchor.criterion.clone(),
        }
    }
}

/// One cell-level breakdown of a CoverageRow — used by the Perfetto
/// export (Wave-Jθ-3) to render per-(impl, test, metrics) sub-bars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageCell {
    pub label: String,
    pub status: CoverageStatus,
    pub detail: String,
}

impl CoverageCell {
    pub fn impl_cell(row: &CoverageRow) -> Self {
        let s = match row.impl_status.as_str() {
            "Implemented" => CoverageStatus::Full,
            "Partial" => CoverageStatus::Partial,
            _ => CoverageStatus::Missing,
        };
        Self {
            label: "impl".to_string(),
            status: s,
            detail: row.impl_status.clone(),
        }
    }

    pub fn test_cell(row: &CoverageRow) -> Self {
        let s = match row.test_status.as_str() {
            "Tested" => CoverageStatus::Full,
            "Partial" => CoverageStatus::Partial,
            "NoTests" => CoverageStatus::Full,
            _ => CoverageStatus::Missing,
        };
        Self {
            label: "test".to_string(),
            status: s,
            detail: row.test_status.clone(),
        }
    }

    pub fn metric_cell(row: &CoverageRow) -> Self {
        let s = if row.metric_count > 0 {
            CoverageStatus::Full
        } else {
            CoverageStatus::Missing
        };
        Self {
            label: "metric".to_string(),
            status: s,
            detail: format!("{} metrics", row.metric_count),
        }
    }
}

/// The full coverage matrix : rows, plus aggregate counts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CoverageMatrix {
    pub rows: Vec<CoverageRow>,
    pub total_anchors: usize,
    pub full_count: usize,
    pub partial_count: usize,
    pub missing_count: usize,
}

impl CoverageMatrix {
    pub fn from_anchors<'a>(anchors: impl IntoIterator<Item = &'a SpecAnchor>) -> Self {
        let mut rows: Vec<CoverageRow> = anchors.into_iter().map(CoverageRow::from_anchor).collect();
        rows.sort_by(|a, b| {
            (a.spec_root.as_str(), a.spec_file.as_str(), a.section.as_str()).cmp(&(
                b.spec_root.as_str(),
                b.spec_file.as_str(),
                b.section.as_str(),
            ))
        });
        let mut full = 0usize;
        let mut partial = 0usize;
        let mut missing = 0usize;
        for r in &rows {
            match r.status {
                CoverageStatus::Full => full += 1,
                CoverageStatus::Partial => partial += 1,
                CoverageStatus::Missing => missing += 1,
            }
        }
        Self {
            total_anchors: rows.len(),
            full_count: full,
            partial_count: partial,
            missing_count: missing,
            rows,
        }
    }

    /// Coverage percent (full / total) in the inclusive 0..=100 range.
    pub fn coverage_percent(&self) -> f64 {
        if self.total_anchors == 0 {
            0.0
        } else {
            100.0 * (self.full_count as f64) / (self.total_anchors as f64)
        }
    }

    /// Render the matrix as a Markdown table (suitable for paste into
    /// DECISIONS / SESSION reports).
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("# CSSLv3 spec-coverage matrix\n\n");
        s.push_str(&format!(
            "Total anchors : **{}** ; Full : **{}** ; Partial : **{}** ; Missing : **{}** ; Coverage : **{:.1}%**\n\n",
            self.total_anchors, self.full_count, self.partial_count, self.missing_count, self.coverage_percent()
        ));
        s.push_str("| Status | Root | Spec file | Section | Impl | Test | Metrics | Crate |\n");
        s.push_str("|---|---|---|---|---|---|---|---|\n");
        for row in &self.rows {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                row.status.ansi_marker(),
                row.spec_root,
                row.spec_file,
                row.section,
                row.impl_status,
                row.test_status,
                row.metric_count,
                row.crate_path.clone().unwrap_or_default()
            ));
        }
        s
    }

    /// Render as compact, hand-formatted JSON (no extra deps).
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!("\"total_anchors\":{},", self.total_anchors));
        out.push_str(&format!("\"full_count\":{},", self.full_count));
        out.push_str(&format!("\"partial_count\":{},", self.partial_count));
        out.push_str(&format!("\"missing_count\":{},", self.missing_count));
        out.push_str(&format!(
            "\"coverage_percent\":{:.4},",
            self.coverage_percent()
        ));
        out.push_str("\"rows\":[");
        let mut first = true;
        for row in &self.rows {
            if !first {
                out.push(',');
            }
            first = false;
            out.push('{');
            out.push_str(&format!(
                "\"spec_root\":{},\"spec_file\":{},\"section\":{},\"impl\":{},\"test\":{},\"metrics\":{},\"status\":{},\"crate\":{}",
                json_str(&row.spec_root),
                json_str(&row.spec_file),
                json_str(&row.section),
                json_str(&row.impl_status),
                json_str(&row.test_status),
                row.metric_count,
                json_str(row.status.as_str()),
                json_str(row.crate_path.as_deref().unwrap_or(""))
            ));
            out.push('}');
        }
        out.push(']');
        out.push('}');
        out
    }

    /// Filter rows where status == Missing.
    pub fn missing_rows(&self) -> Vec<&CoverageRow> {
        self.rows
            .iter()
            .filter(|r| r.status == CoverageStatus::Missing)
            .collect()
    }

    /// Filter rows where status == Partial.
    pub fn partial_rows(&self) -> Vec<&CoverageRow> {
        self.rows
            .iter()
            .filter(|r| r.status == CoverageStatus::Partial)
            .collect()
    }

    /// Filter rows where status == Full.
    pub fn full_rows(&self) -> Vec<&CoverageRow> {
        self.rows
            .iter()
            .filter(|r| r.status == CoverageStatus::Full)
            .collect()
    }

    /// Per-confidence-tier counts for Implemented anchors.
    pub fn confidence_breakdown(&self) -> (usize, usize, usize) {
        let mut low = 0;
        let mut medium = 0;
        let mut high = 0;
        for row in &self.rows {
            match row.confidence.as_deref() {
                Some(c) if c == ImplConfidence::Low.as_str() => low += 1,
                Some(c) if c == ImplConfidence::Medium.as_str() => medium += 1,
                Some(c) if c == ImplConfidence::High.as_str() => high += 1,
                _ => {}
            }
        }
        (low, medium, high)
    }

    /// Per-spec-root row counts.
    pub fn root_breakdown(&self) -> (usize, usize, usize) {
        let mut omn = 0;
        let mut spc = 0;
        let mut dec = 0;
        for row in &self.rows {
            match row.spec_root.as_str() {
                "Omniverse" => omn += 1,
                "CssLv3" => spc += 1,
                "DecisionsLog" => dec += 1,
                _ => {}
            }
        }
        (omn, spc, dec)
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{ImplConfidence, ImplStatus, SpecAnchorBuilder, SpecRoot, TestStatus};

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

    fn partial_anchor(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Partial {
                crate_path: "cssl-bar".into(),
                gaps: vec!["foo".into()],
            })
            .test_status(TestStatus::Untested)
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

    #[test]
    fn coverage_status_classification() {
        assert_eq!(
            CoverageStatus::from_anchor(&full_anchor("a", "§ A")),
            CoverageStatus::Full
        );
        assert_eq!(
            CoverageStatus::from_anchor(&missing_anchor("a", "§ A")),
            CoverageStatus::Missing
        );
        assert_eq!(
            CoverageStatus::from_anchor(&partial_anchor("a", "§ A")),
            CoverageStatus::Partial
        );
    }

    #[test]
    fn coverage_status_str_round_trip() {
        for s in [
            CoverageStatus::Full,
            CoverageStatus::Partial,
            CoverageStatus::Missing,
        ] {
            assert!(!s.as_str().is_empty());
            assert!(!s.ansi_marker().is_empty());
        }
    }

    #[test]
    fn matrix_aggregate_counts() {
        let anchors = vec![
            full_anchor("a", "§ I"),
            full_anchor("a", "§ II"),
            partial_anchor("b", "§ I"),
            missing_anchor("c", "§ I"),
        ];
        let matrix = CoverageMatrix::from_anchors(anchors.iter());
        assert_eq!(matrix.total_anchors, 4);
        assert_eq!(matrix.full_count, 2);
        assert_eq!(matrix.partial_count, 1);
        assert_eq!(matrix.missing_count, 1);
        assert!((matrix.coverage_percent() - 50.0).abs() < 0.001);
    }

    #[test]
    fn matrix_empty_returns_zero_percent() {
        let m = CoverageMatrix::default();
        assert_eq!(m.coverage_percent(), 0.0);
    }

    #[test]
    fn matrix_sorts_rows_by_root_then_file_then_section() {
        let anchors = vec![
            partial_anchor("specs/zz.csl", "§ I"),
            full_anchor("Omniverse/aa", "§ II"),
            full_anchor("Omniverse/aa", "§ I"),
            missing_anchor("specs/aa.csl", "§ I"),
        ];
        let matrix = CoverageMatrix::from_anchors(anchors.iter());
        assert_eq!(matrix.rows[0].spec_file, "specs/aa.csl");
        // Note : sorted alphabetically on root string ; "CssLv3" < "Omniverse"
        assert_eq!(matrix.rows[0].spec_root, "CssLv3");
    }

    #[test]
    fn matrix_to_markdown_contains_header_and_rows() {
        let anchors = vec![full_anchor("a", "§ I")];
        let m = CoverageMatrix::from_anchors(anchors.iter());
        let md = m.to_markdown();
        assert!(md.starts_with("# CSSLv3 spec-coverage matrix"));
        assert!(md.contains("Coverage : **100.0%**"));
        assert!(md.contains("[+]"));
    }

    #[test]
    fn matrix_to_json_is_valid_shape() {
        let anchors = vec![full_anchor("a", "§ I"), missing_anchor("b", "§ I")];
        let m = CoverageMatrix::from_anchors(anchors.iter());
        let json = m.to_json();
        assert!(json.starts_with('{'));
        assert!(json.ends_with('}'));
        assert!(json.contains("\"total_anchors\":2"));
        assert!(json.contains("\"rows\":["));
    }

    #[test]
    fn json_str_escapes_special_characters() {
        let escaped = json_str("foo\"bar\nbaz\\");
        assert_eq!(escaped, "\"foo\\\"bar\\nbaz\\\\\"");
    }

    #[test]
    fn confidence_breakdown_counts_implemented() {
        let mut a1 = full_anchor("a", "§ I");
        if let ImplStatus::Implemented { confidence, .. } = &mut a1.impl_status {
            *confidence = ImplConfidence::Low;
        }
        let mut a2 = full_anchor("b", "§ I");
        if let ImplStatus::Implemented { confidence, .. } = &mut a2.impl_status {
            *confidence = ImplConfidence::Medium;
        }
        let a3 = full_anchor("c", "§ I"); // High by default
        let m = CoverageMatrix::from_anchors([&a1, &a2, &a3]);
        let (low, med, high) = m.confidence_breakdown();
        assert_eq!((low, med, high), (1, 1, 1));
    }

    #[test]
    fn root_breakdown_counts() {
        let omn = full_anchor("a", "§ I"); // Omniverse via builder default
        let spc = partial_anchor("b", "§ I");
        let m = CoverageMatrix::from_anchors([&omn, &spc]);
        let (omn_n, spc_n, dec_n) = m.root_breakdown();
        assert_eq!(omn_n, 1);
        assert_eq!(spc_n, 1);
        assert_eq!(dec_n, 0);
    }

    #[test]
    fn missing_partial_full_filters() {
        let anchors = vec![
            full_anchor("a", "§ I"),
            partial_anchor("b", "§ I"),
            missing_anchor("c", "§ I"),
        ];
        let m = CoverageMatrix::from_anchors(anchors.iter());
        assert_eq!(m.full_rows().len(), 1);
        assert_eq!(m.partial_rows().len(), 1);
        assert_eq!(m.missing_rows().len(), 1);
    }

    #[test]
    fn coverage_cell_emit_three_breakdowns() {
        let row = CoverageRow::from_anchor(&full_anchor("a", "§ I"));
        let imp = CoverageCell::impl_cell(&row);
        let tst = CoverageCell::test_cell(&row);
        let met = CoverageCell::metric_cell(&row);
        assert_eq!(imp.label, "impl");
        assert_eq!(tst.label, "test");
        assert_eq!(met.label, "metric");
        assert_eq!(imp.status, CoverageStatus::Full);
        assert_eq!(tst.status, CoverageStatus::Full);
        assert_eq!(met.status, CoverageStatus::Missing); // no citing metrics
    }
}

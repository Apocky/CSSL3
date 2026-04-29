//! § SpecAnchor — the atomic unit of spec-coverage tracking
//!
//! Mirrors the contract laid out in `_drafts/phase_j/06_l2_telemetry_spec.md`
//! § IV.3. Every recorded SpecAnchor binds (a) a spec-§ from the
//! Omniverse / CSSLv3 / DECISIONS corpora, (b) the implementation
//! status, (c) the test status, and (d) optionally a list of metric
//! names that validate the anchor at runtime.
//!
//! § DESIGN NOTE
//!   The original spec uses `&'static str` slices throughout to make
//!   anchors directly embeddable in `static`/`const` declarations.
//!   In stage-0 we use owned `String`s so the registry can be populated
//!   dynamically from extraction. A static-emit path can layer on top
//!   later by codegen-ing equivalent `&'static`-shaped tuples.

use std::fmt;

/// Three-axis taxonomy of spec corpora a coverage anchor can cite.
///
/// A single physical spec-§ is uniquely identified by the pair
/// `(SpecRoot, spec_file)` ; the `section` field then narrows further.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpecRoot {
    /// `Omniverse/...` — semantic axiom corpus.
    Omniverse,
    /// `specs/...` — CSSLv3 compiler/runtime specs.
    CssLv3,
    /// `DECISIONS.md` — per-slice design rationale anchors.
    DecisionsLog,
}

impl SpecRoot {
    /// Canonical short-name (used in JSON / Markdown export).
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecRoot::Omniverse => "Omniverse",
            SpecRoot::CssLv3 => "CssLv3",
            SpecRoot::DecisionsLog => "DecisionsLog",
        }
    }

    /// Parse a short-name back into a SpecRoot. Used when deserializing
    /// extracted markers.
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "Omniverse" => Some(SpecRoot::Omniverse),
            "CssLv3" | "specs" => Some(SpecRoot::CssLv3),
            "DecisionsLog" | "DECISIONS" => Some(SpecRoot::DecisionsLog),
            _ => None,
        }
    }
}

impl fmt::Display for SpecRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Confidence tier attached to an `Implemented` anchor.
///
/// - `Low` : freshly written or recently changed ; not yet bench-validated.
/// - `Medium` : bench-validated under happy-path conditions.
/// - `High` : full M7-floor compliance (all acceptance criteria met).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImplConfidence {
    Low,
    Medium,
    High,
}

impl ImplConfidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImplConfidence::Low => "Low",
            ImplConfidence::Medium => "Medium",
            ImplConfidence::High => "High",
        }
    }
}

impl fmt::Display for ImplConfidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Implementation status of a tracked spec-§.
///
/// Mirrors `06_l2_telemetry_spec.md § IV.1`. The discriminants are
/// strict-equality only — see anti-pattern table : a Stub MUST NOT
/// silently coerce to Implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplStatus {
    /// Production-grade ; meets spec ; reviewed.
    Implemented {
        /// Crate path, e.g. `compiler-rs/crates/cssl-render-v2`.
        crate_path: String,
        /// Primary module, e.g. `crate::pipeline::stage_5`.
        primary_module: String,
        /// Confidence tier.
        confidence: ImplConfidence,
        /// ISO-8601 date when impl was first marked.
        impl_date: String,
    },
    /// Shape-correct ; behavior-incomplete.
    Partial {
        /// Crate path.
        crate_path: String,
        /// Human-readable gap descriptions.
        gaps: Vec<String>,
    },
    /// Type exists, body is `todo!()` / `unimplemented!()` / placeholder.
    Stub {
        /// Crate path.
        crate_path: String,
    },
    /// No impl reference exists.
    Missing,
}

impl ImplStatus {
    /// Human-readable single-word status (used by reports + matrix).
    pub fn discriminant(&self) -> &'static str {
        match self {
            ImplStatus::Implemented { .. } => "Implemented",
            ImplStatus::Partial { .. } => "Partial",
            ImplStatus::Stub { .. } => "Stub",
            ImplStatus::Missing => "Missing",
        }
    }

    /// True if this anchor counts as "shipped" for gap-list purposes.
    /// Stub and Missing are GAPS ; Partial and Implemented are NOT.
    pub fn is_gap(&self) -> bool {
        matches!(self, ImplStatus::Stub { .. } | ImplStatus::Missing)
    }

    /// True for full impl only (excludes Partial). Used by attestation.
    pub fn is_implemented(&self) -> bool {
        matches!(self, ImplStatus::Implemented { .. })
    }

    /// True if there is at least a stub or partial — i.e., NOT Missing.
    pub fn has_some_code(&self) -> bool {
        !matches!(self, ImplStatus::Missing)
    }

    /// Crate path if the status carries one.
    pub fn crate_path(&self) -> Option<&str> {
        match self {
            ImplStatus::Implemented { crate_path, .. } => Some(crate_path),
            ImplStatus::Partial { crate_path, .. } => Some(crate_path),
            ImplStatus::Stub { crate_path } => Some(crate_path),
            ImplStatus::Missing => None,
        }
    }

    /// Confidence tier if implemented.
    pub fn confidence(&self) -> Option<ImplConfidence> {
        match self {
            ImplStatus::Implemented { confidence, .. } => Some(*confidence),
            _ => None,
        }
    }
}

/// Test-coverage status of a tracked spec-§.
///
/// Mirrors `06_l2_telemetry_spec.md § IV.2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestStatus {
    /// At least one test cites this anchor and passes.
    Tested {
        /// Test paths, e.g. `module::test_name`.
        test_paths: Vec<String>,
        /// ISO-8601 date of the last passing run.
        last_pass_date: String,
    },
    /// Some tests exist but coverage is incomplete.
    Partial {
        test_paths: Vec<String>,
        uncovered_criteria: Vec<String>,
    },
    /// Spec exists ; no tests cite it.
    Untested,
    /// Spec is intentionally not testable.
    NoTests {
        /// Why this anchor is exempt.
        rationale: String,
    },
}

impl TestStatus {
    pub fn discriminant(&self) -> &'static str {
        match self {
            TestStatus::Tested { .. } => "Tested",
            TestStatus::Partial { .. } => "Partial",
            TestStatus::Untested => "Untested",
            TestStatus::NoTests { .. } => "NoTests",
        }
    }

    pub fn is_tested(&self) -> bool {
        matches!(self, TestStatus::Tested { .. })
    }

    pub fn test_paths(&self) -> &[String] {
        match self {
            TestStatus::Tested { test_paths, .. } | TestStatus::Partial { test_paths, .. } => {
                test_paths
            }
            _ => &[],
        }
    }
}

/// The atomic spec-coverage entry.
///
/// One `SpecAnchor` ↔ one (spec-file, §) pair. Multiple anchors per
/// spec-file are normal when the file has multiple numbered sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecAnchor {
    /// Which corpus this anchor lives in.
    pub spec_root: SpecRoot,
    /// File path within the corpus
    /// (e.g. `04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md`).
    pub spec_file: String,
    /// Section header within the file (e.g. `§ V`).
    pub section: String,
    /// Optional acceptance criterion (e.g. `phase-COLLAPSE ≤ 4ms`).
    pub criterion: Option<String>,
    /// Implementation status.
    pub impl_status: ImplStatus,
    /// Test status.
    pub test_status: TestStatus,
    /// Names of metrics that validate this anchor at runtime
    /// (populated by the cssl-metrics integration when present).
    pub citing_metrics: Vec<String>,
    /// Optional Rust path of the symbol owning this anchor
    /// (e.g. `cssl_render_v2::pipeline::stage_5`).
    pub rust_symbol: Option<String>,
    /// Optional ISO-8601 date the anchor was last verified.
    pub last_verified: Option<String>,
    /// Optional ISO-8601 date of the source spec-file.
    pub spec_mtime: Option<String>,
    /// Optional ISO-8601 date of the impl source-file.
    pub impl_mtime: Option<String>,
}

impl SpecAnchor {
    /// Convenience: stable identifier for matrix sorting / dedup.
    /// Combines (root, file, section) into a normalized string.
    pub fn key(&self) -> String {
        format!("{}::{}::{}", self.spec_root, self.spec_file, self.section)
    }

    /// True if the anchor is currently a "should-but-doesn't" gap.
    pub fn is_gap(&self) -> bool {
        self.impl_status.is_gap()
    }

    /// True if the anchor lacks any test backing.
    pub fn lacks_tests(&self) -> bool {
        matches!(self.test_status, TestStatus::Untested)
    }

    /// True if spec-mtime > impl-mtime ⇒ stale per § IV.7.
    pub fn is_stale(&self) -> bool {
        match (&self.spec_mtime, &self.impl_mtime) {
            (Some(sm), Some(im)) => sm > im,
            _ => false,
        }
    }
}

/// Builder for [`SpecAnchor`] to keep call-sites readable.
#[derive(Debug, Default)]
pub struct SpecAnchorBuilder {
    spec_root: Option<SpecRoot>,
    spec_file: Option<String>,
    section: Option<String>,
    criterion: Option<String>,
    impl_status: Option<ImplStatus>,
    test_status: Option<TestStatus>,
    citing_metrics: Vec<String>,
    rust_symbol: Option<String>,
    last_verified: Option<String>,
    spec_mtime: Option<String>,
    impl_mtime: Option<String>,
}

impl SpecAnchorBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spec_root(mut self, root: SpecRoot) -> Self {
        self.spec_root = Some(root);
        self
    }

    pub fn spec_file(mut self, file: impl Into<String>) -> Self {
        self.spec_file = Some(file.into());
        self
    }

    pub fn section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    pub fn criterion(mut self, criterion: impl Into<String>) -> Self {
        self.criterion = Some(criterion.into());
        self
    }

    pub fn impl_status(mut self, status: ImplStatus) -> Self {
        self.impl_status = Some(status);
        self
    }

    pub fn test_status(mut self, status: TestStatus) -> Self {
        self.test_status = Some(status);
        self
    }

    pub fn add_citing_metric(mut self, metric: impl Into<String>) -> Self {
        self.citing_metrics.push(metric.into());
        self
    }

    pub fn rust_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.rust_symbol = Some(symbol.into());
        self
    }

    pub fn last_verified(mut self, date: impl Into<String>) -> Self {
        self.last_verified = Some(date.into());
        self
    }

    pub fn spec_mtime(mut self, date: impl Into<String>) -> Self {
        self.spec_mtime = Some(date.into());
        self
    }

    pub fn impl_mtime(mut self, date: impl Into<String>) -> Self {
        self.impl_mtime = Some(date.into());
        self
    }

    pub fn build(self) -> SpecAnchor {
        SpecAnchor {
            spec_root: self.spec_root.unwrap_or(SpecRoot::CssLv3),
            spec_file: self.spec_file.unwrap_or_default(),
            section: self.section.unwrap_or_default(),
            criterion: self.criterion,
            impl_status: self.impl_status.unwrap_or(ImplStatus::Missing),
            test_status: self.test_status.unwrap_or(TestStatus::Untested),
            citing_metrics: self.citing_metrics,
            rust_symbol: self.rust_symbol,
            last_verified: self.last_verified,
            spec_mtime: self.spec_mtime,
            impl_mtime: self.impl_mtime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_root_roundtrip() {
        for root in [SpecRoot::Omniverse, SpecRoot::CssLv3, SpecRoot::DecisionsLog] {
            let s = root.as_str();
            assert_eq!(SpecRoot::parse(s), Some(root));
        }
    }

    #[test]
    fn spec_root_parse_short_aliases() {
        assert_eq!(SpecRoot::parse("specs"), Some(SpecRoot::CssLv3));
        assert_eq!(SpecRoot::parse("DECISIONS"), Some(SpecRoot::DecisionsLog));
        assert_eq!(SpecRoot::parse("nonsense"), None);
    }

    #[test]
    fn impl_status_gap_classification() {
        let stub = ImplStatus::Stub {
            crate_path: "cssl-foo".into(),
        };
        assert!(stub.is_gap());
        assert!(!stub.is_implemented());

        let missing = ImplStatus::Missing;
        assert!(missing.is_gap());

        let impld = ImplStatus::Implemented {
            crate_path: "cssl-foo".into(),
            primary_module: "crate::bar".into(),
            confidence: ImplConfidence::High,
            impl_date: "2026-04-29".into(),
        };
        assert!(!impld.is_gap());
        assert!(impld.is_implemented());

        let partial = ImplStatus::Partial {
            crate_path: "cssl-foo".into(),
            gaps: vec!["foo not yet implemented".into()],
        };
        assert!(!partial.is_gap()); // Partial is NOT a gap per § IV.5
        assert!(!partial.is_implemented());
    }

    #[test]
    fn test_status_discriminants() {
        assert_eq!(TestStatus::Untested.discriminant(), "Untested");
        assert_eq!(
            TestStatus::Tested {
                test_paths: vec!["a::b".into()],
                last_pass_date: "2026-04-29".into(),
            }
            .discriminant(),
            "Tested"
        );
        assert_eq!(
            TestStatus::Partial {
                test_paths: vec![],
                uncovered_criteria: vec![],
            }
            .discriminant(),
            "Partial"
        );
        assert_eq!(
            TestStatus::NoTests {
                rationale: "attestation-only".into(),
            }
            .discriminant(),
            "NoTests"
        );
    }

    #[test]
    fn anchor_builder_minimal() {
        let a = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::Omniverse)
            .spec_file("04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md")
            .section("§ V")
            .criterion("phase-COLLAPSE p99 <= 4ms")
            .impl_status(ImplStatus::Implemented {
                crate_path: "cssl-substrate-omega-step".into(),
                primary_module: "crate::collapse".into(),
                confidence: ImplConfidence::Medium,
                impl_date: "2026-04-29".into(),
            })
            .test_status(TestStatus::Tested {
                test_paths: vec!["collapse_phase_per_spec_05_density_budget".into()],
                last_pass_date: "2026-04-29".into(),
            })
            .build();
        assert_eq!(a.spec_root, SpecRoot::Omniverse);
        assert_eq!(a.section, "§ V");
        assert!(a.impl_status.is_implemented());
        assert!(a.test_status.is_tested());
    }

    #[test]
    fn anchor_key_stable() {
        let a = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("specs/08_MIR.csl")
            .section("§ Lowering")
            .build();
        assert_eq!(a.key(), "CssLv3::specs/08_MIR.csl::§ Lowering");
    }

    #[test]
    fn anchor_stale_detection() {
        let stale = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("specs/08_MIR.csl")
            .section("§ Lowering")
            .spec_mtime("2026-04-29")
            .impl_mtime("2026-03-01")
            .build();
        assert!(stale.is_stale());

        let fresh = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("specs/08_MIR.csl")
            .section("§ Lowering")
            .spec_mtime("2026-03-01")
            .impl_mtime("2026-04-29")
            .build();
        assert!(!fresh.is_stale());
    }

    #[test]
    fn anchor_gap_and_lacks_tests() {
        let gap = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("specs/08_MIR.csl")
            .section("§ Lowering")
            .impl_status(ImplStatus::Missing)
            .build();
        assert!(gap.is_gap());
        assert!(gap.lacks_tests());
    }

    #[test]
    fn impl_confidence_ordering() {
        assert!(ImplConfidence::Low < ImplConfidence::Medium);
        assert!(ImplConfidence::Medium < ImplConfidence::High);
    }

    #[test]
    fn impl_status_crate_path_extraction() {
        let s = ImplStatus::Stub {
            crate_path: "cssl-foo".into(),
        };
        assert_eq!(s.crate_path(), Some("cssl-foo"));
        assert_eq!(ImplStatus::Missing.crate_path(), None);
    }

    #[test]
    fn impl_status_has_some_code() {
        assert!(!ImplStatus::Missing.has_some_code());
        assert!(ImplStatus::Stub {
            crate_path: "x".into()
        }
        .has_some_code());
    }
}

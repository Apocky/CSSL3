//! § SpecCoverageRegistry — runtime store of all known [`SpecAnchor`]s
//!
//! The query surface mirrors `06_l2_telemetry_spec.md` § IV.5 :
//!
//!   - [`SpecCoverageRegistry::gap_list`] — anchors w/ Stub or Missing impl
//!   - [`SpecCoverageRegistry::coverage_for_crate`] — per-crate anchors
//!   - [`SpecCoverageRegistry::impl_of_section`] — per-spec-§ anchors
//!   - [`SpecCoverageRegistry::tests_of_section`] — citing tests for a §
//!   - [`SpecCoverageRegistry::impl_without_metrics`] — implemented but unmeasured
//!   - [`SpecCoverageRegistry::metric_to_spec_anchor`] — reverse lookup
//!   - [`SpecCoverageRegistry::coverage_matrix`] — full matrix
//!   - [`SpecCoverageRegistry::stale_anchors`] — drift detection
//!
//! The registry is **append-only** at construction time : extraction
//! produces a Vec of anchors, and the registry is built once. Updates
//! happen by rebuilding (cheap : O(N) over a few thousand anchors).
//! Stage-0 keeps everything in memory ; mmap'd persistence is deferred.

use crate::anchor::{ImplStatus, SpecAnchor, TestStatus};
use crate::error::SpecCoverageError;
use crate::extract::{ExtractedAnchor, TestNameMatch};
use crate::matrix::CoverageMatrix;
use crate::paradigm::AnchorParadigm;
use crate::report::SpecCoverageReport;

use std::collections::{BTreeMap, BTreeSet};

/// Runtime store of spec-coverage anchors.
#[derive(Debug, Clone, Default)]
pub struct SpecCoverageRegistry {
    /// All anchors, keyed by canonical `(root::file::section)` triple.
    anchors: BTreeMap<String, SpecAnchor>,
    /// For each anchor key, which paradigm contributed it (multi-source
    /// merging is allowed — the set tracks every contributor).
    provenance: BTreeMap<String, BTreeSet<AnchorParadigm>>,
    /// Spec-files known to the registry, indexing into the anchor map
    /// for fast per-file aggregate queries.
    by_file: BTreeMap<String, Vec<String>>,
    /// Per-crate index : crate_path → anchor keys.
    by_crate: BTreeMap<String, Vec<String>>,
    /// Metric-name → anchor key (set after extraction merges).
    by_metric: BTreeMap<String, String>,
}

impl SpecCoverageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of registered anchors.
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// All anchors, in stable key order.
    pub fn anchors(&self) -> impl Iterator<Item = &SpecAnchor> {
        self.anchors.values()
    }

    /// Insert (or merge with) a single SpecAnchor.
    ///
    /// Merge rules :
    ///   - duplicate `(root, file, section)` triples : the existing
    ///     entry's impl_status is kept unless the new one has a
    ///     **higher-quality** discriminant (Implemented > Partial >
    ///     Stub > Missing).
    ///   - test_paths : union'd
    ///   - citing_metrics : union'd
    ///   - criterion : new wins if existing is None
    pub fn insert(&mut self, anchor: SpecAnchor) {
        let key = anchor.key();
        let crate_path_opt = anchor.impl_status.crate_path().map(String::from);
        let file_key = anchor.spec_file.clone();
        let metric_keys: Vec<String> = anchor.citing_metrics.clone();
        match self.anchors.get_mut(&key) {
            Some(existing) => {
                merge_anchor(existing, anchor);
            }
            None => {
                self.anchors.insert(key.clone(), anchor);
                self.by_file.entry(file_key).or_default().push(key.clone());
                if let Some(cp) = crate_path_opt {
                    self.by_crate.entry(cp).or_default().push(key.clone());
                }
                for m in metric_keys {
                    self.by_metric.insert(m, key.clone());
                }
            }
        }
    }

    /// Insert with provenance tag.
    pub fn insert_with_provenance(&mut self, anchor: SpecAnchor, paradigm: AnchorParadigm) {
        let key = anchor.key();
        self.insert(anchor);
        self.provenance.entry(key).or_default().insert(paradigm);
    }

    /// Insert a batch of [`ExtractedAnchor`]s in one pass.
    pub fn extend_from_extracted<I: IntoIterator<Item = ExtractedAnchor>>(&mut self, batch: I) {
        for e in batch {
            let paradigm = e.paradigm;
            self.insert_with_provenance(e.anchor, paradigm);
        }
    }

    /// Merge a list of [`TestNameMatch`]s : for each match, attempt to
    /// find an anchor whose section / spec_file substring overlaps the
    /// `anchor_part` token, and upgrade its TestStatus.
    pub fn merge_test_matches(&mut self, matches: &[TestNameMatch]) -> usize {
        let mut hits = 0usize;
        for m in matches {
            let key_opt = self.lookup_anchor_key_by_part(&m.anchor_part);
            if let Some(key) = key_opt {
                if let Some(anchor) = self.anchors.get_mut(&key) {
                    upgrade_test_status(anchor, &m.fn_part);
                    hits += 1;
                }
            }
        }
        hits
    }

    /// "what is spec'd but not implemented? = the should-but-doesn't-work list"
    pub fn gap_list(&self) -> Vec<&SpecAnchor> {
        self.anchors.values().filter(|a| a.is_gap()).collect()
    }

    /// "what spec-sections does this crate cover?"
    pub fn coverage_for_crate(&self, crate_path: &str) -> Vec<&SpecAnchor> {
        match self.by_crate.get(crate_path) {
            Some(keys) => keys.iter().filter_map(|k| self.anchors.get(k)).collect(),
            None => Vec::new(),
        }
    }

    /// "what crates implement this spec-§?"
    pub fn impl_of_section(&self, spec_file: &str, section: &str) -> Vec<&SpecAnchor> {
        self.anchors
            .values()
            .filter(|a| a.spec_file == spec_file && a.section == section)
            .collect()
    }

    /// "what tests validate this spec-§?"
    pub fn tests_of_section(&self, spec_file: &str, section: &str) -> Vec<String> {
        let mut out = Vec::new();
        for anchor in self.impl_of_section(spec_file, section) {
            for t in anchor.test_status.test_paths() {
                out.push(t.clone());
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// "of the impl_status=Implemented anchors, which lack metrics?"
    pub fn impl_without_metrics(&self) -> Vec<&SpecAnchor> {
        self.anchors
            .values()
            .filter(|a| a.impl_status.is_implemented() && a.citing_metrics.is_empty())
            .collect()
    }

    /// "of the registered metrics, which spec-anchor do they cite?"
    pub fn metric_to_spec_anchor(&self, metric_name: &str) -> Option<&SpecAnchor> {
        let key = self.by_metric.get(metric_name)?;
        self.anchors.get(key)
    }

    /// Coverage report : 3-axis matrix (spec-§, impl, test).
    pub fn coverage_matrix(&self) -> CoverageMatrix {
        CoverageMatrix::from_anchors(self.anchors.values())
    }

    /// Build a curated report (gaps / pending / deferred / stale).
    pub fn report(&self) -> SpecCoverageReport {
        SpecCoverageReport::build(self.anchors.values())
    }

    /// Spec-update detection : spec-file mtime > impl-mtime.
    pub fn stale_anchors(&self) -> Vec<&SpecAnchor> {
        self.anchors.values().filter(|a| a.is_stale()).collect()
    }

    /// All known unique spec files, alphabetically sorted.
    pub fn spec_files(&self) -> Vec<String> {
        self.by_file.keys().cloned().collect()
    }

    /// All known unique crate paths, alphabetically sorted.
    pub fn crate_paths(&self) -> Vec<String> {
        self.by_crate.keys().cloned().collect()
    }

    /// All paradigms recorded for a given anchor key.
    pub fn provenance_of(&self, anchor_key: &str) -> Vec<AnchorParadigm> {
        match self.provenance.get(anchor_key) {
            Some(set) => set.iter().copied().collect(),
            None => Vec::new(),
        }
    }

    /// Anchors whose impl_status is Partial. Distinct from gap_list().
    pub fn partial_anchors(&self) -> Vec<&SpecAnchor> {
        self.anchors
            .values()
            .filter(|a| matches!(a.impl_status, ImplStatus::Partial { .. }))
            .collect()
    }

    /// Anchors whose tests are Untested.
    pub fn untested_anchors(&self) -> Vec<&SpecAnchor> {
        self.anchors
            .values()
            .filter(|a| a.lacks_tests())
            .collect()
    }

    /// Coverage percent (full / total).
    pub fn coverage_percent(&self) -> f64 {
        self.coverage_matrix().coverage_percent()
    }

    /// Quick "register-completeness" check : true if every anchor has
    /// at least one source-of-truth attribution.
    pub fn anchors_have_provenance(&self) -> bool {
        self.anchors
            .keys()
            .all(|k| self.provenance.contains_key(k))
    }

    /// All anchors lacking provenance (caller can build-warn on these).
    pub fn anchors_without_provenance(&self) -> Vec<&SpecAnchor> {
        self.anchors
            .iter()
            .filter(|(k, _)| !self.provenance.contains_key(*k))
            .map(|(_, a)| a)
            .collect()
    }

    /// Verify every anchor has at least one source-of-truth citation.
    /// Used at build-time to enforce the § XI anti-pattern row "spec-
    /// anchor without source-of-truth".
    pub fn validate_source_of_truth(&self) -> crate::Result<()> {
        for (key, _anchor) in self.anchors.iter() {
            if !self.provenance.contains_key(key) {
                return Err(SpecCoverageError::Invariant(format!(
                    "anchor {key} lacks any source-of-truth citation"
                )));
            }
        }
        Ok(())
    }

    /// Internal : look up anchor key by anchor_part substring.
    fn lookup_anchor_key_by_part(&self, anchor_part: &str) -> Option<String> {
        // Stage-0 strategy : search for any anchor whose spec_file or
        // section contains the anchor_part token. Try both raw and
        // underscore-to-space normalized forms so the matcher is
        // resilient to test-name vs spec-file conventions.
        let raw = anchor_part.to_ascii_lowercase();
        let normalized = anchor_part.replace('_', " ").to_ascii_lowercase();
        for (key, anchor) in self.anchors.iter() {
            let f_lower = anchor.spec_file.to_ascii_lowercase();
            let s_lower = anchor.section.to_ascii_lowercase();
            if f_lower.contains(&raw)
                || s_lower.contains(&raw)
                || f_lower.contains(&normalized)
                || s_lower.contains(&normalized)
            {
                return Some(key.clone());
            }
        }
        None
    }
}

/// Merge `incoming` into `existing` per the rules above.
fn merge_anchor(existing: &mut SpecAnchor, incoming: SpecAnchor) {
    // Impl status : take the better of the two.
    if impl_quality(&incoming.impl_status) > impl_quality(&existing.impl_status) {
        existing.impl_status = incoming.impl_status;
    }
    // Test status : Tested > Partial > Untested ; NoTests stays sticky.
    let existing_q = test_quality(&existing.test_status);
    let incoming_q = test_quality(&incoming.test_status);
    if incoming_q > existing_q {
        existing.test_status = incoming.test_status;
    } else if existing_q == incoming_q {
        // Same quality : union test_paths if both Tested or both Partial.
        merge_test_paths(&mut existing.test_status, &incoming.test_status);
    }
    // Citing metrics : union (preserving order ; dedup).
    for m in incoming.citing_metrics {
        if !existing.citing_metrics.contains(&m) {
            existing.citing_metrics.push(m);
        }
    }
    // Criterion : adopt incoming if existing has none.
    if existing.criterion.is_none() {
        existing.criterion = incoming.criterion;
    }
    if existing.rust_symbol.is_none() {
        existing.rust_symbol = incoming.rust_symbol;
    }
    if existing.last_verified.is_none() {
        existing.last_verified = incoming.last_verified;
    }
    if existing.spec_mtime.is_none() {
        existing.spec_mtime = incoming.spec_mtime;
    }
    if existing.impl_mtime.is_none() {
        existing.impl_mtime = incoming.impl_mtime;
    }
}

fn impl_quality(s: &ImplStatus) -> u8 {
    match s {
        ImplStatus::Implemented { .. } => 4,
        ImplStatus::Partial { .. } => 3,
        ImplStatus::Stub { .. } => 2,
        ImplStatus::Missing => 1,
    }
}

fn test_quality(s: &TestStatus) -> u8 {
    match s {
        TestStatus::Tested { .. } => 4,
        TestStatus::Partial { .. } => 3,
        TestStatus::NoTests { .. } => 2,
        TestStatus::Untested => 1,
    }
}

fn merge_test_paths(existing: &mut TestStatus, incoming: &TestStatus) {
    let new_paths = incoming.test_paths();
    match existing {
        TestStatus::Tested { test_paths, .. } | TestStatus::Partial { test_paths, .. } => {
            for p in new_paths {
                if !test_paths.contains(p) {
                    test_paths.push(p.clone());
                }
            }
        }
        _ => {}
    }
}

fn upgrade_test_status(anchor: &mut SpecAnchor, fn_part: &str) {
    let test_path = format!("{fn_part}_per_spec");
    match &mut anchor.test_status {
        TestStatus::Untested => {
            anchor.test_status = TestStatus::Tested {
                test_paths: vec![test_path],
                last_pass_date: "stage0".to_string(),
            };
        }
        TestStatus::Tested { test_paths, .. } | TestStatus::Partial { test_paths, .. } => {
            if !test_paths.contains(&test_path) {
                test_paths.push(test_path);
            }
        }
        TestStatus::NoTests { .. } => {} // sticky
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{ImplConfidence, ImplStatus, SpecAnchorBuilder, SpecRoot, TestStatus};
    use crate::extract::TestNameMatch;
    use crate::paradigm::AnchorParadigm;

    fn fa(file: &str, sec: &str) -> SpecAnchor {
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

    fn missing(file: &str, sec: &str) -> SpecAnchor {
        SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file(file)
            .section(sec)
            .impl_status(ImplStatus::Missing)
            .test_status(TestStatus::Untested)
            .build()
    }

    #[test]
    fn registry_insert_and_lookup_by_section() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("Omniverse/04.csl", "§ I"));
        let hits = reg.impl_of_section("Omniverse/04.csl", "§ I");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn registry_gap_list_filters_missing() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        reg.insert(missing("b", "§ I"));
        assert_eq!(reg.gap_list().len(), 1);
        assert_eq!(reg.gap_list()[0].spec_file, "b");
    }

    #[test]
    fn registry_coverage_for_crate() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        reg.insert(fa("a", "§ II"));
        let hits = reg.coverage_for_crate("cssl-foo");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn registry_coverage_for_unknown_crate_empty() {
        let reg = SpecCoverageRegistry::new();
        assert!(reg.coverage_for_crate("cssl-nonexistent").is_empty());
    }

    #[test]
    fn registry_tests_of_section_dedupes() {
        let mut reg = SpecCoverageRegistry::new();
        let mut a = fa("a", "§ I");
        a.test_status = TestStatus::Tested {
            test_paths: vec!["t1".into(), "t2".into()],
            last_pass_date: "2026-04-29".into(),
        };
        let mut b = fa("a", "§ I");
        b.test_status = TestStatus::Tested {
            test_paths: vec!["t2".into(), "t3".into()],
            last_pass_date: "2026-04-29".into(),
        };
        reg.insert(a);
        reg.insert(b);
        let tests = reg.tests_of_section("a", "§ I");
        assert_eq!(tests, vec!["t1", "t2", "t3"]);
    }

    #[test]
    fn registry_merge_better_impl_wins() {
        let mut reg = SpecCoverageRegistry::new();
        let stub = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::Omniverse)
            .spec_file("a")
            .section("§ I")
            .impl_status(ImplStatus::Stub {
                crate_path: "cssl-foo".into(),
            })
            .build();
        reg.insert(stub);
        reg.insert(fa("a", "§ I"));
        let hits = reg.impl_of_section("a", "§ I");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].impl_status.is_implemented());
    }

    #[test]
    fn registry_merge_test_paths_unioned() {
        let mut reg = SpecCoverageRegistry::new();
        let mut a = fa("a", "§ I");
        a.test_status = TestStatus::Tested {
            test_paths: vec!["t1".into()],
            last_pass_date: "stage0".into(),
        };
        let mut b = fa("a", "§ I");
        b.test_status = TestStatus::Tested {
            test_paths: vec!["t2".into()],
            last_pass_date: "stage0".into(),
        };
        reg.insert(a);
        reg.insert(b);
        let result = reg.impl_of_section("a", "§ I");
        let paths = result[0].test_status.test_paths();
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn registry_metric_to_spec_anchor() {
        let mut reg = SpecCoverageRegistry::new();
        let mut a = fa("a", "§ I");
        a.citing_metrics = vec!["omega_step.phase_time_ns".into()];
        reg.insert(a);
        let hit = reg.metric_to_spec_anchor("omega_step.phase_time_ns");
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().spec_file, "a");
    }

    #[test]
    fn registry_impl_without_metrics() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        let mut b = fa("b", "§ I");
        b.citing_metrics = vec!["foo".into()];
        reg.insert(b);
        let hits = reg.impl_without_metrics();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].spec_file, "a");
    }

    #[test]
    fn registry_provenance_recorded() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert_with_provenance(fa("a", "§ I"), AnchorParadigm::InlineSectionMarker);
        reg.insert_with_provenance(fa("a", "§ I"), AnchorParadigm::DecisionsLog);
        let prov = reg.provenance_of(&fa("a", "§ I").key());
        assert!(prov.contains(&AnchorParadigm::InlineSectionMarker));
        assert!(prov.contains(&AnchorParadigm::DecisionsLog));
    }

    #[test]
    fn registry_validate_source_of_truth_passes() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert_with_provenance(fa("a", "§ I"), AnchorParadigm::InlineSectionMarker);
        assert!(reg.validate_source_of_truth().is_ok());
    }

    #[test]
    fn registry_validate_source_of_truth_fails_without_provenance() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        let r = reg.validate_source_of_truth();
        assert!(r.is_err());
    }

    #[test]
    fn registry_extend_from_extracted() {
        let mut reg = SpecCoverageRegistry::new();
        let extracted = vec![
            ExtractedAnchor {
                paradigm: AnchorParadigm::InlineSectionMarker,
                anchor: fa("a", "§ I"),
                source_file: "src/a.rs".into(),
                source_line: 12,
            },
            ExtractedAnchor {
                paradigm: AnchorParadigm::DecisionsLog,
                anchor: missing("b", "§ I"),
                source_file: "DECISIONS.md".into(),
                source_line: 99,
            },
        ];
        reg.extend_from_extracted(extracted);
        assert_eq!(reg.len(), 2);
        assert!(!reg.gap_list().is_empty());
    }

    #[test]
    fn registry_merge_test_matches_upgrades_status() {
        let mut reg = SpecCoverageRegistry::new();
        let mut a = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("specs/06_substrate_evolution.csl")
            .section("§ I")
            .impl_status(ImplStatus::Stub {
                crate_path: "cssl-foo".into(),
            })
            .build();
        a.test_status = TestStatus::Untested;
        reg.insert(a);
        let matches = vec![TestNameMatch {
            raw: "omega_field_per_spec_06_substrate_evolution".into(),
            crate_part: String::new(),
            fn_part: "omega_field".into(),
            anchor_part: "06_substrate_evolution".into(),
        }];
        let hits = reg.merge_test_matches(&matches);
        assert_eq!(hits, 1);
        let updated = reg
            .impl_of_section("specs/06_substrate_evolution.csl", "§ I")
            .pop()
            .unwrap();
        assert!(updated.test_status.is_tested());
    }

    #[test]
    fn registry_stale_anchors() {
        let mut reg = SpecCoverageRegistry::new();
        let stale = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::Omniverse)
            .spec_file("a")
            .section("§ I")
            .spec_mtime("2026-04-29")
            .impl_mtime("2026-01-01")
            .impl_status(ImplStatus::Implemented {
                crate_path: "cssl-foo".into(),
                primary_module: "crate::bar".into(),
                confidence: ImplConfidence::High,
                impl_date: "stage0".into(),
            })
            .build();
        reg.insert(stale);
        assert_eq!(reg.stale_anchors().len(), 1);
    }

    #[test]
    fn registry_partial_anchors_filter() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        let p = SpecAnchorBuilder::new()
            .spec_root(SpecRoot::CssLv3)
            .spec_file("b")
            .section("§ I")
            .impl_status(ImplStatus::Partial {
                crate_path: "cssl-bar".into(),
                gaps: vec!["foo".into()],
            })
            .build();
        reg.insert(p);
        assert_eq!(reg.partial_anchors().len(), 1);
    }

    #[test]
    fn registry_untested_anchors() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(missing("a", "§ I"));
        reg.insert(fa("b", "§ I"));
        assert_eq!(reg.untested_anchors().len(), 1);
    }

    #[test]
    fn registry_spec_files_listing() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("Omniverse/a.csl", "§ I"));
        reg.insert(fa("Omniverse/a.csl", "§ II"));
        reg.insert(fa("Omniverse/b.csl", "§ I"));
        let files = reg.spec_files();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn registry_crate_paths_listing() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        let mut x = fa("b", "§ I");
        x.impl_status = ImplStatus::Implemented {
            crate_path: "cssl-bar".into(),
            primary_module: "crate".into(),
            confidence: ImplConfidence::High,
            impl_date: "stage0".into(),
        };
        reg.insert(x);
        let crates = reg.crate_paths();
        assert!(crates.contains(&"cssl-foo".to_string()));
        assert!(crates.contains(&"cssl-bar".to_string()));
    }

    #[test]
    fn registry_coverage_percent_with_mix() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        reg.insert(fa("a", "§ II"));
        reg.insert(missing("b", "§ I"));
        let pct = reg.coverage_percent();
        assert!((pct - 66.66).abs() < 1.0);
    }

    #[test]
    fn registry_anchors_have_provenance_check() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I")); // no provenance
        assert!(!reg.anchors_have_provenance());
        let mut reg2 = SpecCoverageRegistry::new();
        reg2.insert_with_provenance(fa("a", "§ I"), AnchorParadigm::InlineSectionMarker);
        assert!(reg2.anchors_have_provenance());
    }

    #[test]
    fn registry_anchors_without_provenance() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        reg.insert_with_provenance(fa("b", "§ I"), AnchorParadigm::InlineSectionMarker);
        let orphans = reg.anchors_without_provenance();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].spec_file, "a");
    }

    #[test]
    fn registry_test_quality_ordering() {
        assert!(test_quality(&TestStatus::Untested) < test_quality(&TestStatus::NoTests {
            rationale: String::new()
        }));
        assert!(test_quality(&TestStatus::NoTests {
            rationale: String::new()
        }) < test_quality(&TestStatus::Partial {
            test_paths: vec![],
            uncovered_criteria: vec![]
        }));
        assert!(test_quality(&TestStatus::Partial {
            test_paths: vec![],
            uncovered_criteria: vec![]
        }) < test_quality(&TestStatus::Tested {
            test_paths: vec![],
            last_pass_date: String::new()
        }));
    }

    #[test]
    fn registry_impl_quality_ordering() {
        assert!(
            impl_quality(&ImplStatus::Missing)
                < impl_quality(&ImplStatus::Stub {
                    crate_path: "x".into()
                })
        );
        assert!(
            impl_quality(&ImplStatus::Stub {
                crate_path: "x".into()
            }) < impl_quality(&ImplStatus::Partial {
                crate_path: "x".into(),
                gaps: vec![]
            })
        );
        assert!(
            impl_quality(&ImplStatus::Partial {
                crate_path: "x".into(),
                gaps: vec![]
            }) < impl_quality(&ImplStatus::Implemented {
                crate_path: "x".into(),
                primary_module: "y".into(),
                confidence: ImplConfidence::Low,
                impl_date: "x".into(),
            })
        );
    }

    #[test]
    fn registry_no_double_insert_into_indexes() {
        let mut reg = SpecCoverageRegistry::new();
        reg.insert(fa("a", "§ I"));
        reg.insert(fa("a", "§ I"));
        let by_crate = reg.coverage_for_crate("cssl-foo");
        assert_eq!(by_crate.len(), 1);
    }
}

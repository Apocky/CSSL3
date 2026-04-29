//! § Three anchor paradigms (per spec-anchor audit recommendations)
//!
//! The Wave-Jζ-4 audit (`_drafts/phase_j/spec_anchor_audit.md`) identified
//! three patterns of spec-anchoring observed in production crates :
//!
//!   1. **Centralized citations** — a top-level `SPEC_CITATIONS` array
//!      (cssl-render-v2 style ; 60 references). One module-level source of
//!      truth for all the spec-§s that module realizes.
//!   2. **Inline section markers** — terse `§ SPEC : …` markers on each
//!      type / function (cssl-cgen-cpu-x64 style ; 52 references, 32 of
//!      them inline markers).
//!   3. **Multi-axis** — a single attribute citing all three axes
//!      (`omniverse`, `spec`, `decision`) at once (cssl-mir style ; 45
//!      references balanced across families).
//!
//! This module is the **type surface** for all three paradigms. The
//! [`extract`](crate::extract) module is responsible for parsing each
//! shape out of source files.

use crate::anchor::SpecAnchor;

/// Tag identifying which extraction paradigm produced an anchor. Used
/// by the registry to attribute anchors to a source-of-truth and to
/// enable provenance debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnchorParadigm {
    /// `pub const SPEC_CITATIONS: &[&str] = &[...]` block.
    CentralizedCitations,
    /// `/// § SPEC : …` doc-comment line attached to an item.
    InlineSectionMarker,
    /// `#[spec_anchor(omniverse=…, spec=…, decision=…)]` attribute.
    MultiAxis,
    /// Test-name regex `[crate]_[fn]_per_spec_[anchor]`.
    TestName,
    /// `DECISIONS.md` `spec-anchors:` block.
    DecisionsLog,
}

impl AnchorParadigm {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnchorParadigm::CentralizedCitations => "CentralizedCitations",
            AnchorParadigm::InlineSectionMarker => "InlineSectionMarker",
            AnchorParadigm::MultiAxis => "MultiAxis",
            AnchorParadigm::TestName => "TestName",
            AnchorParadigm::DecisionsLog => "DecisionsLog",
        }
    }
}

/// Paradigm-1 : a centralized `SPEC_CITATIONS` block for a crate or module.
///
/// The block is owned by a single Rust path (typically a public
/// constant) and carries a list of cited spec-§ strings that the
/// extractor decomposes into individual [`SpecAnchor`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationsBlock {
    /// Crate the block lives in (e.g. `cssl-render-v2`).
    pub crate_path: String,
    /// Owning Rust path (e.g. `cssl_render_v2::attestation::SPEC_CITATIONS`).
    pub owner_path: String,
    /// Raw citation strings as they appear in the array.
    pub citations: Vec<String>,
}

impl CitationsBlock {
    /// Decompose into per-§ [`SpecAnchor`]s. Each citation string is
    /// split on the canonical "§" delimiter ; the first token is taken
    /// as the spec_file and the remainder as the section header.
    pub fn into_anchors(&self) -> Vec<SpecAnchor> {
        self.citations
            .iter()
            .filter_map(|c| split_citation(c))
            .map(|(file, section)| {
                crate::anchor::SpecAnchorBuilder::new()
                    .spec_root(infer_root(&file))
                    .spec_file(file)
                    .section(section)
                    .impl_status(crate::anchor::ImplStatus::Implemented {
                        crate_path: self.crate_path.clone(),
                        primary_module: self.owner_path.clone(),
                        confidence: crate::anchor::ImplConfidence::Medium,
                        impl_date: chrono_default(),
                    })
                    .build()
            })
            .collect()
    }
}

/// Paradigm-2 : a single inline section marker pulled out of a
/// doc-comment or regular comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineMarker {
    /// Raw marker line, exactly as it appears in source.
    pub raw: String,
    /// Normalized spec-file portion.
    pub spec_file: String,
    /// Normalized section portion.
    pub section: String,
    /// Crate that owns the marker.
    pub crate_path: String,
    /// Optional Rust symbol path the marker is attached to.
    pub rust_symbol: Option<String>,
}

impl InlineMarker {
    pub fn into_anchor(&self) -> SpecAnchor {
        let mut builder = crate::anchor::SpecAnchorBuilder::new()
            .spec_root(infer_root(&self.spec_file))
            .spec_file(self.spec_file.clone())
            .section(self.section.clone())
            .impl_status(crate::anchor::ImplStatus::Implemented {
                crate_path: self.crate_path.clone(),
                primary_module: self.rust_symbol.clone().unwrap_or_default(),
                confidence: crate::anchor::ImplConfidence::Medium,
                impl_date: chrono_default(),
            });
        if let Some(s) = &self.rust_symbol {
            builder = builder.rust_symbol(s.clone());
        }
        builder.build()
    }
}

/// Paradigm-3 : a multi-axis attribute citing all three corpora at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiAxisAnchor {
    pub crate_path: String,
    pub rust_symbol: Option<String>,
    pub omniverse: Option<String>,
    pub spec: Option<String>,
    pub decision: Option<String>,
    pub criterion: Option<String>,
}

impl MultiAxisAnchor {
    /// Decompose into one anchor per axis present (Omniverse, CssLv3,
    /// DecisionsLog). Each axis spawns its own [`SpecAnchor`] entry so
    /// the matrix can score them independently.
    pub fn into_anchors(&self) -> Vec<SpecAnchor> {
        let mut out = Vec::new();
        let impl_status = crate::anchor::ImplStatus::Implemented {
            crate_path: self.crate_path.clone(),
            primary_module: self.rust_symbol.clone().unwrap_or_default(),
            confidence: crate::anchor::ImplConfidence::Medium,
            impl_date: chrono_default(),
        };
        if let Some(om) = &self.omniverse {
            let (file, section) = split_citation(om).unwrap_or_else(|| (om.clone(), String::new()));
            let mut builder = crate::anchor::SpecAnchorBuilder::new()
                .spec_root(crate::anchor::SpecRoot::Omniverse)
                .spec_file(file)
                .section(section)
                .impl_status(impl_status.clone());
            if let Some(c) = &self.criterion {
                builder = builder.criterion(c.clone());
            }
            if let Some(s) = &self.rust_symbol {
                builder = builder.rust_symbol(s.clone());
            }
            out.push(builder.build());
        }
        if let Some(sp) = &self.spec {
            let (file, section) = split_citation(sp).unwrap_or_else(|| (sp.clone(), String::new()));
            let mut builder = crate::anchor::SpecAnchorBuilder::new()
                .spec_root(crate::anchor::SpecRoot::CssLv3)
                .spec_file(file)
                .section(section)
                .impl_status(impl_status.clone());
            if let Some(c) = &self.criterion {
                builder = builder.criterion(c.clone());
            }
            if let Some(s) = &self.rust_symbol {
                builder = builder.rust_symbol(s.clone());
            }
            out.push(builder.build());
        }
        if let Some(dec) = &self.decision {
            let mut builder = crate::anchor::SpecAnchorBuilder::new()
                .spec_root(crate::anchor::SpecRoot::DecisionsLog)
                .spec_file("DECISIONS.md".to_string())
                .section(dec.clone())
                .impl_status(impl_status.clone());
            if let Some(c) = &self.criterion {
                builder = builder.criterion(c.clone());
            }
            if let Some(s) = &self.rust_symbol {
                builder = builder.rust_symbol(s.clone());
            }
            out.push(builder.build());
        }
        out
    }
}

/// Split a citation like `"Omniverse/04_X.csl § V foo"` into
/// `(spec_file, section)`. The first occurrence of "§ " is the cut point.
/// Returns None for empty input.
pub fn split_citation(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(idx) = trimmed.find("§") {
        let file = trimmed[..idx].trim_end_matches(' ').trim().to_string();
        let section_raw = trimmed[idx..].trim();
        Some((file, section_raw.to_string()))
    } else {
        Some((trimmed.to_string(), String::new()))
    }
}

/// Best-effort root inference from a citation file path. `Omniverse/...`
/// → Omniverse, `specs/...` → CssLv3, anything else → CssLv3 default.
pub fn infer_root(path: &str) -> crate::anchor::SpecRoot {
    let trimmed = path.trim();
    if trimmed.starts_with("Omniverse/") || trimmed.starts_with("Omniverse ") {
        crate::anchor::SpecRoot::Omniverse
    } else if trimmed.starts_with("specs/") || trimmed.starts_with("specs ") {
        crate::anchor::SpecRoot::CssLv3
    } else if trimmed.starts_with("DECISIONS") {
        crate::anchor::SpecRoot::DecisionsLog
    } else {
        crate::anchor::SpecRoot::CssLv3
    }
}

/// Stand-in for the build-date when no real timestamp is available.
/// Stage-0 returns a fixed string ; the real build-script will inject
/// the actual ISO-8601 date through env-vars.
fn chrono_default() -> String {
    "stage0".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{ImplStatus, SpecRoot};

    #[test]
    fn split_citation_parses_basic() {
        let (f, s) =
            split_citation("Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md § V phase").unwrap();
        assert_eq!(f, "Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md");
        assert!(s.starts_with("§ V"));
    }

    #[test]
    fn split_citation_no_section() {
        let (f, s) =
            split_citation("Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md").unwrap();
        assert_eq!(f, "Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md");
        assert_eq!(s, "");
    }

    #[test]
    fn split_citation_empty_returns_none() {
        assert!(split_citation("").is_none());
        assert!(split_citation("   ").is_none());
    }

    #[test]
    fn infer_root_omniverse() {
        assert_eq!(
            infer_root("Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md"),
            SpecRoot::Omniverse
        );
    }

    #[test]
    fn infer_root_specs() {
        assert_eq!(
            infer_root("specs/08_MIR.csl"),
            SpecRoot::CssLv3
        );
    }

    #[test]
    fn infer_root_decisions() {
        assert_eq!(infer_root("DECISIONS/T11-D042"), SpecRoot::DecisionsLog);
    }

    #[test]
    fn citations_block_into_anchors() {
        let block = CitationsBlock {
            crate_path: "cssl-render-v2".into(),
            owner_path: "cssl_render_v2::attestation::SPEC_CITATIONS".into(),
            citations: vec![
                "Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md".into(),
                "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-5".into(),
            ],
        };
        let anchors = block.into_anchors();
        assert_eq!(anchors.len(), 2);
        assert_eq!(anchors[0].spec_root, SpecRoot::Omniverse);
        assert!(anchors[0].impl_status.is_implemented());
    }

    #[test]
    fn inline_marker_into_anchor() {
        let m = InlineMarker {
            raw: "// § SPEC : specs/07_CODEGEN.csl § CPU BACKEND § ABI".into(),
            spec_file: "specs/07_CODEGEN.csl".into(),
            section: "§ CPU BACKEND § ABI".into(),
            crate_path: "cssl-cgen-cpu-x64".into(),
            rust_symbol: Some("cssl_cgen_cpu_x64::abi::X64Abi".into()),
        };
        let a = m.into_anchor();
        assert_eq!(a.spec_root, SpecRoot::CssLv3);
        assert_eq!(a.section, "§ CPU BACKEND § ABI");
        assert!(matches!(a.impl_status, ImplStatus::Implemented { .. }));
        assert_eq!(
            a.rust_symbol.as_deref(),
            Some("cssl_cgen_cpu_x64::abi::X64Abi")
        );
    }

    #[test]
    fn multiaxis_into_anchors_three() {
        let m = MultiAxisAnchor {
            crate_path: "cssl-mir".into(),
            rust_symbol: Some("cssl_mir::lower::lower".into()),
            omniverse: Some("Omniverse/03_INTERMEDIATION/IR.csl § Lowering".into()),
            spec: Some("specs/08_MIR.csl § Lowering".into()),
            decision: Some("DECISIONS/T11-D042".into()),
            criterion: Some("preserves total ordering".into()),
        };
        let anchors = m.into_anchors();
        assert_eq!(anchors.len(), 3);
        let kinds: Vec<_> = anchors.iter().map(|a| a.spec_root).collect();
        assert!(kinds.contains(&SpecRoot::Omniverse));
        assert!(kinds.contains(&SpecRoot::CssLv3));
        assert!(kinds.contains(&SpecRoot::DecisionsLog));
        assert!(anchors.iter().all(|a| a.criterion.is_some()));
    }

    #[test]
    fn multiaxis_partial_omniverse_only() {
        let m = MultiAxisAnchor {
            crate_path: "cssl-omega".into(),
            rust_symbol: None,
            omniverse: Some("Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md § V".into()),
            spec: None,
            decision: None,
            criterion: None,
        };
        let anchors = m.into_anchors();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].spec_root, SpecRoot::Omniverse);
    }

    #[test]
    fn paradigm_str_round_trip() {
        for p in [
            AnchorParadigm::CentralizedCitations,
            AnchorParadigm::InlineSectionMarker,
            AnchorParadigm::MultiAxis,
            AnchorParadigm::TestName,
            AnchorParadigm::DecisionsLog,
        ] {
            let s = p.as_str();
            assert!(!s.is_empty());
        }
    }
}

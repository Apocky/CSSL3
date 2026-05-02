// request.rs — SelfAuthorRequest value-object + variants + Constraints
// ══════════════════════════════════════════════════════════════════
// § REQ-FIELDS
//   - prompt      : the GM/DM-language description of what to author
//   - kind        : Scene | NpcLine | Recipe | Lore | System (5-variants)
//   - examples    : optional few-shot examples ; LLM is steered toward similar-shape
//   - constraints : token-budget · max-lines · forbid-effects · target-file-glob
//   - target_path : where in the LoA tree the authored CSSL should land (validated)
// § VALIDATION (cheap-pre-LLM)
//   - target_path MUST NOT match any forbidden-target glob
//   - prompt non-empty + ≤ MAX_PROMPT_BYTES (16 KiB)
//   - examples ≤ MAX_EXAMPLES (8) and each ≤ MAX_EXAMPLE_BYTES (4 KiB)
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Maximum bytes accepted in `prompt` field. Overflow = `RequestError::PromptTooLarge`.
pub const MAX_PROMPT_BYTES: usize = 16 * 1024;
/// Maximum number of few-shot example strings.
pub const MAX_EXAMPLES: usize = 8;
/// Maximum bytes per example.
pub const MAX_EXAMPLE_BYTES: usize = 4 * 1024;

/// Authoring-kind discriminator. Drives the LLM system-prompt template + the
/// sandbox-execute test-shape.
///
/// § Kinds (5)
///   - `Scene`   : `.cssl` scene-file (entities · triggers · lighting · camera)
///   - `NpcLine` : single NPC dialogue-snippet (composable into a scene)
///   - `Recipe`  : crafting recipe (inputs · outputs · station · time-units)
///   - `Lore`    : world-lore prose-block (no executable code, gated by `Constraints::lore_only`)
///   - `System`  : a CSSL-systems-file (rules · invariants · effects ; high-impact)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SelfAuthorKind {
    /// Scene-file authoring.
    Scene,
    /// Single NPC dialogue line.
    NpcLine,
    /// Crafting recipe.
    Recipe,
    /// World-lore prose-block.
    Lore,
    /// Systems-file (high-impact; sovereign-required at LiveMutateGate).
    System,
}

impl SelfAuthorKind {
    /// Stable string-tag used in audit logs + Σ-Chain payloads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::NpcLine => "npc_line",
            Self::Recipe => "recipe",
            Self::Lore => "lore",
            Self::System => "system",
        }
    }

    /// Returns `true` for kinds that REQUIRE the sovereign-bit in addition to the
    /// EFFECT_WRITE cap. Currently only `System` (high-impact rules-edits).
    #[must_use]
    pub const fn requires_sovereign(self) -> bool {
        matches!(self, Self::System)
    }
}

/// Constraints that bound an author-request's output-shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    /// Maximum lines of CSSL output the LLM is asked to produce. Default 200.
    pub max_lines: u32,
    /// Maximum tokens for the LLM call. Default 4096.
    pub max_tokens: u32,
    /// Effect-bits the authored CSSL is FORBIDDEN to introduce.
    /// Encoded as a string-set for serde stability; the orchestrator treats
    /// each entry as a substring-search on the generated source.
    pub forbid_effect_strings: Vec<String>,
    /// If `true`, the kind must be `Lore` (no executable code surfaces in output).
    pub lore_only: bool,
    /// Quality-score floor for auto-mutate (overrides crate default).
    pub score_threshold: u8,
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            max_lines: 200,
            max_tokens: 4096,
            forbid_effect_strings: vec![
                // Default-forbid surveillance / network / FS-mutation effects.
                "Effect::NetworkEgress".to_string(),
                "Effect::FsWrite".to_string(),
                "Effect::Surveil".to_string(),
            ],
            lore_only: false,
            score_threshold: super::DEFAULT_SCORE_THRESHOLD,
        }
    }
}

/// Errors surfaced from `SelfAuthorRequest::validate`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    /// Prompt was empty.
    #[error("prompt is empty")]
    PromptEmpty,
    /// Prompt exceeded `MAX_PROMPT_BYTES`.
    #[error("prompt too large: {0} > {MAX_PROMPT_BYTES}")]
    PromptTooLarge(usize),
    /// Too many few-shot examples (> `MAX_EXAMPLES`).
    #[error("too many examples: {0} > {MAX_EXAMPLES}")]
    TooManyExamples(usize),
    /// One example exceeded `MAX_EXAMPLE_BYTES`.
    #[error("example {idx} too large: {len} > {MAX_EXAMPLE_BYTES}")]
    ExampleTooLarge {
        /// Zero-based index of the offending example.
        idx: usize,
        /// Actual byte length.
        len: usize,
    },
    /// Lore-only constraint but kind ≠ `Lore`.
    #[error("constraints.lore_only = true but kind ≠ Lore (kind = {0})")]
    LoreOnlyConstraintViolated(&'static str),
    /// Target path matched a forbidden-glob.
    #[error("forbidden target path: {0}")]
    ForbiddenTarget(String),
}

/// Self-author request value-object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfAuthorRequest {
    /// Free-form GM/DM-language description of the author-task.
    pub prompt: String,
    /// Authoring kind (5-variants).
    pub kind: SelfAuthorKind,
    /// Few-shot examples (≤ 8).
    pub examples: Vec<String>,
    /// Output-shape constraints.
    pub constraints: Constraints,
    /// Target file-path the authored CSSL would replace/insert. Validated against
    /// the forbidden-target list at `validate` time AND again at LiveMutateGate.
    pub target_path: String,
}

impl SelfAuthorRequest {
    /// Convenience constructor with empty target_path. Caller MUST set
    /// `target_path` before passing to the orchestrator if a live-mutate is
    /// desired ; the orchestrator will reject empty target_path at mutate-time.
    #[must_use]
    pub fn new(
        prompt: impl Into<String>,
        kind: SelfAuthorKind,
        examples: Vec<String>,
        constraints: Constraints,
    ) -> Self {
        Self {
            prompt: prompt.into(),
            kind,
            examples,
            constraints,
            target_path: String::new(),
        }
    }

    /// Builder-style mutator for `target_path`.
    #[must_use]
    pub fn with_target_path(mut self, target_path: impl Into<String>) -> Self {
        self.target_path = target_path.into();
        self
    }

    /// Validate the request shape. Returns `Ok(())` on pass. Cheap (no LLM call).
    pub fn validate(&self) -> Result<(), RequestError> {
        if self.prompt.is_empty() {
            return Err(RequestError::PromptEmpty);
        }
        if self.prompt.len() > MAX_PROMPT_BYTES {
            return Err(RequestError::PromptTooLarge(self.prompt.len()));
        }
        if self.examples.len() > MAX_EXAMPLES {
            return Err(RequestError::TooManyExamples(self.examples.len()));
        }
        for (idx, ex) in self.examples.iter().enumerate() {
            if ex.len() > MAX_EXAMPLE_BYTES {
                return Err(RequestError::ExampleTooLarge {
                    idx,
                    len: ex.len(),
                });
            }
        }
        if self.constraints.lore_only && self.kind != SelfAuthorKind::Lore {
            return Err(RequestError::LoreOnlyConstraintViolated(self.kind.as_str()));
        }
        if !self.target_path.is_empty() && super::is_forbidden_target(&self.target_path) {
            return Err(RequestError::ForbiddenTarget(self.target_path.clone()));
        }
        Ok(())
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_kind_as_str_stable() {
        assert_eq!(SelfAuthorKind::Scene.as_str(), "scene");
        assert_eq!(SelfAuthorKind::NpcLine.as_str(), "npc_line");
        assert_eq!(SelfAuthorKind::Recipe.as_str(), "recipe");
        assert_eq!(SelfAuthorKind::Lore.as_str(), "lore");
        assert_eq!(SelfAuthorKind::System.as_str(), "system");
    }

    #[test]
    fn t02_system_requires_sovereign() {
        assert!(SelfAuthorKind::System.requires_sovereign());
        assert!(!SelfAuthorKind::Scene.requires_sovereign());
        assert!(!SelfAuthorKind::Lore.requires_sovereign());
    }

    #[test]
    fn t03_validate_empty_prompt_rejected() {
        let req = SelfAuthorRequest::new("", SelfAuthorKind::Scene, vec![], Constraints::default());
        assert_eq!(req.validate(), Err(RequestError::PromptEmpty));
    }

    #[test]
    fn t04_validate_too_large_prompt_rejected() {
        let huge = "x".repeat(MAX_PROMPT_BYTES + 1);
        let req = SelfAuthorRequest::new(huge.clone(), SelfAuthorKind::Scene, vec![], Constraints::default());
        assert_eq!(req.validate(), Err(RequestError::PromptTooLarge(huge.len())));
    }

    #[test]
    fn t05_validate_too_many_examples_rejected() {
        let exes = vec!["e".to_string(); MAX_EXAMPLES + 1];
        let req = SelfAuthorRequest::new("p", SelfAuthorKind::Scene, exes, Constraints::default());
        assert_eq!(req.validate(), Err(RequestError::TooManyExamples(MAX_EXAMPLES + 1)));
    }

    #[test]
    fn t06_validate_lore_only_violation() {
        let cs = Constraints {
            lore_only: true,
            ..Constraints::default()
        };
        let req = SelfAuthorRequest::new("p", SelfAuthorKind::Scene, vec![], cs);
        assert!(matches!(req.validate(), Err(RequestError::LoreOnlyConstraintViolated(_))));
    }

    #[test]
    fn t07_validate_happy_path() {
        let req = SelfAuthorRequest::new("p", SelfAuthorKind::Scene, vec![], Constraints::default());
        assert!(req.validate().is_ok());
    }
}

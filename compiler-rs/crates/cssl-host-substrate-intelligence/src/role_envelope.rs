//! § role_envelope — per-role syntactic envelope (clause-shape templates).
//!
//! Each role + kind combination has a canonical "shape" the composer
//! follows. The shape selects clause counts, word counts per clause,
//! and which parts-of-speech go where. The composer fills in the
//! morphology procedurally per clause.

use crate::{ComposeKind, Role};

/// Sentence-shape descriptor — a vector of clause-roles.
#[derive(Debug, Clone, Copy)]
pub struct Envelope {
    /// Up to 4 clauses per utterance. Each entry is a `ClauseShape`.
    pub clauses: [ClauseShape; 4],
    /// How many of `clauses` are active.
    pub clause_count: u8,
    /// Whether the closing punctuation is forced declarative (false →
    /// axis-weighted choice from PUNCT_ENDINGS).
    pub force_declarative: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ClauseShape {
    /// `the ADJ NOUN VERB`        — 4 stems
    Statement,
    /// `the NOUN VERB ADV`        — 4 stems, mid-clause
    Continuation,
    /// `where ADV does NOUN VERB` — 5 stems, interrogative mood
    Question,
    /// `let NOUN VERB`            — 3 stems, hortative
    Hortative,
    /// `NOUN, like ADJ NOUN`      — 4 stems, simile
    Simile,
}

/// Pick the envelope appropriate to the `(role, kind)` pair.
pub fn pick(role: Role, kind: ComposeKind) -> Envelope {
    use ClauseShape::*;
    let (clauses, count, decl) = match (role, kind) {
        // GM dialogue : 1-2 statements with optional continuation.
        (Role::Gm, ComposeKind::DialogueLine) => {
            ([Statement, Continuation, Statement, Statement], 2, false)
        }
        // GM environment description : 2-3 layered clauses.
        (Role::Gm, ComposeKind::EnvironmentDescription) => {
            ([Statement, Continuation, Simile, Statement], 3, true)
        }
        // DM directive : terse hortative.
        (Role::Dm, ComposeKind::ArcDirective) => {
            ([Hortative, Statement, Statement, Statement], 1, true)
        }
        // Collaborator remix : longer reflective.
        (Role::Collaborator, ComposeKind::RemixDraft) => {
            ([Statement, Continuation, Simile, Continuation], 4, false)
        }
        // Coder proposal : technical statement.
        (Role::Coder, ComposeKind::EngineProposal) => {
            ([Statement, Continuation, Statement, Statement], 2, true)
        }
        // KAN bias update : single statement.
        (Role::Coder, ComposeKind::KanBiasUpdate) => {
            ([Statement, Statement, Statement, Statement], 1, true)
        }
        // Default : single statement.
        _ => ([Statement, Statement, Statement, Statement], 1, true),
    };
    Envelope { clauses, clause_count: count, force_declarative: decl }
}

impl ClauseShape {
    /// Number of stem-words this clause contains.
    pub fn stem_count(self) -> u8 {
        match self {
            ClauseShape::Statement => 4,
            ClauseShape::Continuation => 4,
            ClauseShape::Question => 5,
            ClauseShape::Hortative => 3,
            ClauseShape::Simile => 4,
        }
    }
}

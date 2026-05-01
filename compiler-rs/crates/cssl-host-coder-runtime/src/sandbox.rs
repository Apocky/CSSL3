// sandbox.rs — staging-area for in-flight edits ; NEVER touches real files
// ══════════════════════════════════════════════════════════════════
// § sandbox holds Vec-style records keyed by CoderEditId in BTreeMap (deterministic)
// § state-transition is the ONLY mutation surface ; insert is one-shot
// § apply-to-real-file is performed by caller-supplied writer in CoderRuntime::apply
// ══════════════════════════════════════════════════════════════════

use crate::edit::{CoderEditId, EditState, StagedEdit};
use std::collections::BTreeMap;

/// In-memory sandbox store. Deterministic ordering via [`BTreeMap`].
#[derive(Debug, Default)]
pub struct SandboxStore {
    edits: BTreeMap<CoderEditId, StagedEdit>,
}

impl SandboxStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a new edit record.
    pub fn insert(&mut self, edit: StagedEdit) {
        self.edits.insert(edit.id, edit);
    }

    /// Borrow an edit by id.
    pub fn get(&self, id: CoderEditId) -> Option<&StagedEdit> {
        self.edits.get(&id)
    }

    /// Transition an edit's state. No-op if id is unknown.
    pub fn transition(&mut self, id: CoderEditId, next: EditState) {
        if let Some(edit) = self.edits.get_mut(&id) {
            edit.state = next;
        }
    }

    /// Number of edits currently in the sandbox.
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    /// Returns true iff sandbox is empty.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Iterate all sandbox-resident edits in id-order (deterministic).
    pub fn iter(&self) -> impl Iterator<Item = (&CoderEditId, &StagedEdit)> {
        self.edits.iter()
    }
}

/// Errors from the apply-to-real-file path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxApplyError {
    /// Edit-id not present in sandbox.
    UnknownEdit,
    /// Edit was not in `Approved` state.
    NotApproved(EditState),
    /// Caller-supplied writer failed.
    WriterFailed(String),
}

impl core::fmt::Display for SandboxApplyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownEdit => write!(f, "unknown edit-id"),
            Self::NotApproved(s) => write!(f, "edit not approved (state = {s:?})"),
            Self::WriterFailed(msg) => write!(f, "writer failed: {msg}"),
        }
    }
}

impl std::error::Error for SandboxApplyError {}

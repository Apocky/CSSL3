//! § apply — class-specific apply handlers + pluggable registry.
//!
//! The pipeline does NOT itself know how to mutate KAN weights /
//! procgen biases / shader uniforms — that is the host's
//! responsibility. The pipeline calls a registered
//! [`ApplyHandler`] per class ; if no handler is registered,
//! `NoopApplyHandler` runs (audit-only).

use crate::class::{HotfixClass, HotfixId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § Outcome the host returns from a handler.
///
/// The `pre_apply_snapshot` field is opaque bytes the host can use
/// to roll back state. Pipeline stores it in `StagedHotfix`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    /// Bytes the rollback path will hand back to the same handler.
    /// Empty `Vec` is fine for handlers that have no rollback need.
    pub pre_apply_snapshot: Vec<u8>,
    /// Free-form host-side note (e.g. `"applied 12 weight deltas"`).
    pub note: String,
}

impl ApplyOutcome {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            pre_apply_snapshot: Vec::new(),
            note: String::new(),
        }
    }
}

/// § A class-specific apply handler.
///
/// Object-safe trait (no generics) so a registry can hold
/// `Box<dyn ApplyHandler>`. Handlers MUST be deterministic for a
/// given payload + host-state ; non-determinism breaks rollback.
pub trait ApplyHandler: Send + Sync + 'static {
    /// Apply the hotfix payload. Return a snapshot used for rollback.
    fn apply(&self, payload: &[u8]) -> ApplyOutcome;

    /// Restore the host's prior state given the snapshot returned
    /// by a previous `apply`. Default impl is a no-op for handlers
    /// that don't need an active rollback step.
    fn rollback(&self, _snapshot: &[u8]) {}
}

/// § Trivial handler used when the host hasn't registered a
/// class-specific one (e.g. tests, early integration).
#[derive(Debug, Default, Copy, Clone)]
pub struct NoopApplyHandler;

impl ApplyHandler for NoopApplyHandler {
    fn apply(&self, _payload: &[u8]) -> ApplyOutcome {
        ApplyOutcome {
            pre_apply_snapshot: Vec::new(),
            note: "noop-apply".into(),
        }
    }
}

/// § Registry mapping `HotfixClass` → `Box<dyn ApplyHandler>`.
///
/// Backed by `BTreeMap` for deterministic iteration in tests.
#[derive(Default)]
pub struct ApplyRegistry {
    handlers: BTreeMap<HotfixClass, Box<dyn ApplyHandler>>,
}

impl std::fmt::Debug for ApplyRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys: Vec<_> = self.handlers.keys().collect();
        f.debug_struct("ApplyRegistry")
            .field("classes", &keys)
            .finish()
    }
}

impl ApplyRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for a class. Replaces any prior handler.
    pub fn register<H: ApplyHandler>(&mut self, class: HotfixClass, handler: H) {
        self.handlers.insert(class, Box::new(handler));
    }

    /// Look up a handler ; returns `None` if unregistered.
    #[must_use]
    pub fn get(&self, class: HotfixClass) -> Option<&dyn ApplyHandler> {
        self.handlers.get(&class).map(std::convert::AsRef::as_ref)
    }

    /// Convenience : get-or-noop.
    #[must_use]
    pub fn get_or_noop<'a>(&'a self, class: HotfixClass) -> &'a dyn ApplyHandler {
        self.handlers
            .get(&class)
            .map_or(&NoopApplyHandler as &dyn ApplyHandler, std::convert::AsRef::as_ref)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    #[must_use]
    pub fn registered_classes(&self) -> Vec<HotfixClass> {
        self.handlers.keys().copied().collect()
    }
}

/// Suppress unused-warning : `HotfixId` re-exported through `lib.rs`
/// for ergonomic top-level re-exports.
#[allow(dead_code)]
fn _id_typecheck(_: HotfixId) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test-only handler that records its calls into a shared cell.
    #[derive(Default)]
    struct CountingHandler {
        calls: std::sync::Mutex<Vec<Vec<u8>>>,
    }

    impl ApplyHandler for CountingHandler {
        fn apply(&self, payload: &[u8]) -> ApplyOutcome {
            self.calls.lock().unwrap().push(payload.to_vec());
            ApplyOutcome {
                pre_apply_snapshot: vec![0xFE, 0xED],
                note: "counted".into(),
            }
        }
    }

    #[test]
    fn registry_dispatches_to_registered_handler() {
        let mut reg = ApplyRegistry::new();
        reg.register(HotfixClass::KanWeightUpdate, CountingHandler::default());
        let h = reg.get(HotfixClass::KanWeightUpdate).unwrap();
        let outcome = h.apply(&[1, 2, 3]);
        assert_eq!(outcome.pre_apply_snapshot, vec![0xFE, 0xED]);
        assert_eq!(outcome.note, "counted");
    }

    #[test]
    fn registry_falls_back_to_noop_when_unregistered() {
        let reg = ApplyRegistry::new();
        let h = reg.get_or_noop(HotfixClass::RenderPipelineParam);
        let outcome = h.apply(&[]);
        assert_eq!(outcome.note, "noop-apply");
    }
}

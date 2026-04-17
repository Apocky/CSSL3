//! Linear-value use-count tracker for `iso` capabilities.
//!
//! § RULE (`specs/12` § LINEAR × HANDLER R8)
//!   An iso value must be used *exactly once* in its lexical scope : either consumed
//!   (moved into a function call / returned / re-bound) or explicitly dropped. The
//!   tracker detects :
//!
//!   - **Leak**       : scope exits without the iso being used.
//!   - **Duplicate**  : iso is referenced more than once.
//!   - **Multi-shot** : iso flows through a handler whose resume is not one-shot.
//!
//! § STAGE-0 SHAPE
//!   The tracker is a per-scope `BTreeMap<BindingId, LinearUse>` where `BindingId`
//!   is a newtype over `u32`. Callers in `cssl-hir` map their HIR-level binding
//!   identifiers (from `HirPatternKind::Binding`) onto `BindingId`s. The tracker
//!   then watches for use events and emits `LinearViolation`s at scope-close.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::cap::CapKind;

/// Identifier for a specific linear binding being tracked. `cssl-hir` maps its
/// `HirId` or `Symbol` onto this opaque `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct BindingId(pub u32);

/// What kind of use event occurred on a linear binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    /// Value consumed : moved into a call, returned, or re-bound.
    Consume,
    /// Explicit `drop(x)` call.
    Drop,
    /// Read without consume — should never happen for iso (it's an error to surface).
    Read,
    /// Passed through a handler with one-shot resume.
    ResumeOnce,
    /// Passed through a handler with multi-shot resume.
    ResumeMultiShot,
}

/// Bookkeeping record per linear binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinearUse {
    /// The cap declared for this binding — only `iso` is linear.
    pub cap: CapKind,
    /// Number of consume / resume events observed.
    pub consume_count: u32,
    /// Number of read events observed (should be 0 for iso).
    pub read_count: u32,
    /// Whether a drop was issued explicitly.
    pub dropped: bool,
    /// Whether the binding is still in-scope (becomes `false` after `close_scope`).
    pub in_scope: bool,
}

impl LinearUse {
    /// Build a fresh record for a new binding.
    #[must_use]
    pub const fn new(cap: CapKind) -> Self {
        Self {
            cap,
            consume_count: 0,
            read_count: 0,
            dropped: false,
            in_scope: true,
        }
    }

    /// `true` iff this binding has been used exactly once (consume or drop).
    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        self.consume_count == 1 || self.dropped
    }

    /// `true` iff this binding leaked (scope closed without resolution).
    #[must_use]
    pub const fn is_leak(&self) -> bool {
        !self.in_scope && self.consume_count == 0 && !self.dropped
    }

    /// `true` iff this binding was consumed more than once.
    #[must_use]
    pub const fn is_duplicate(&self) -> bool {
        self.consume_count > 1
    }
}

/// Violations emitted by [`LinearTracker::close_scope`] and [`LinearTracker::use_binding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LinearViolation {
    /// `iso` value consumed more than once in its scope.
    #[error("linear value consumed more than once (binding {0:?}) (§§ 12 iso discipline)")]
    DuplicateConsume(BindingId),
    /// `iso` value scope-closed without consume-or-drop.
    #[error("linear value leaked — scope closed without consume-or-drop (binding {0:?}) (§§ 12 iso discipline)")]
    Leak(BindingId),
    /// `iso` value passed through multi-shot resume (R8 violation).
    #[error("linear value passed through multi-shot resume (binding {0:?}) (§§ 12 R8)")]
    MultiShotResume(BindingId),
    /// `iso` value read without consume (unusual ; tracker can flag for diagnosis).
    #[error("linear value read without consume (binding {0:?}) (§§ 12 iso discipline)")]
    ReadWithoutConsume(BindingId),
    /// Use on a binding that's already out-of-scope.
    #[error("use after scope-exit (binding {0:?})")]
    UseAfterScope(BindingId),
}

/// Per-scope linear tracker.
#[derive(Debug, Default, Clone)]
pub struct LinearTracker {
    bindings: BTreeMap<BindingId, LinearUse>,
}

impl LinearTracker {
    /// Build an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin tracking a new binding. Overwrites any prior binding with the same id.
    pub fn introduce(&mut self, id: BindingId, cap: CapKind) {
        self.bindings.insert(id, LinearUse::new(cap));
    }

    /// Record a use event. For non-linear caps this is a no-op (they're unlimited).
    /// For iso bindings, updates counters and returns any violation surfaced.
    pub fn use_binding(&mut self, id: BindingId, kind: UseKind) -> Result<(), LinearViolation> {
        let rec = self
            .bindings
            .get_mut(&id)
            .ok_or(LinearViolation::UseAfterScope(id))?;
        if !rec.cap.is_linear() {
            // Non-linear caps need no tracking.
            return Ok(());
        }
        if !rec.in_scope {
            return Err(LinearViolation::UseAfterScope(id));
        }
        match kind {
            UseKind::Consume | UseKind::ResumeOnce => {
                rec.consume_count = rec.consume_count.saturating_add(1);
                if rec.consume_count > 1 {
                    return Err(LinearViolation::DuplicateConsume(id));
                }
            }
            UseKind::Drop => {
                if rec.dropped {
                    return Err(LinearViolation::DuplicateConsume(id));
                }
                rec.dropped = true;
            }
            UseKind::Read => {
                rec.read_count = rec.read_count.saturating_add(1);
                return Err(LinearViolation::ReadWithoutConsume(id));
            }
            UseKind::ResumeMultiShot => {
                return Err(LinearViolation::MultiShotResume(id));
            }
        }
        Ok(())
    }

    /// Close the tracker's scope — every iso-binding must be resolved, else a
    /// `Leak` is surfaced. The returned vector is in binding-insertion order.
    pub fn close_scope(&mut self) -> Vec<LinearViolation> {
        let mut out = Vec::new();
        for (id, rec) in &mut self.bindings {
            rec.in_scope = false;
            if rec.cap.is_linear() && !rec.is_resolved() {
                out.push(LinearViolation::Leak(*id));
            }
        }
        out
    }

    /// Lookup the current record for a binding.
    #[must_use]
    pub fn get(&self, id: BindingId) -> Option<&LinearUse> {
        self.bindings.get(&id)
    }

    /// Number of bindings currently tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// `true` iff nothing is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{BindingId, LinearTracker, LinearViolation, UseKind};
    use crate::cap::CapKind;

    #[test]
    fn iso_consumed_once_resolves() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        assert!(t.use_binding(id, UseKind::Consume).is_ok());
        let violations = t.close_scope();
        assert!(violations.is_empty());
    }

    #[test]
    fn iso_leaked_detected() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        let violations = t.close_scope();
        assert_eq!(violations.len(), 1);
        assert!(matches!(violations[0], LinearViolation::Leak(_)));
    }

    #[test]
    fn iso_duplicate_consume_detected() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        assert!(t.use_binding(id, UseKind::Consume).is_ok());
        let second = t.use_binding(id, UseKind::Consume);
        assert!(matches!(second, Err(LinearViolation::DuplicateConsume(_))));
    }

    #[test]
    fn iso_dropped_resolves() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        assert!(t.use_binding(id, UseKind::Drop).is_ok());
        let violations = t.close_scope();
        assert!(violations.is_empty());
    }

    #[test]
    fn non_linear_cap_unrestricted() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Val);
        for _ in 0..5 {
            assert!(t.use_binding(id, UseKind::Read).is_ok());
        }
        let violations = t.close_scope();
        assert!(violations.is_empty());
    }

    #[test]
    fn multi_shot_resume_blocked_for_iso() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        let res = t.use_binding(id, UseKind::ResumeMultiShot);
        assert!(matches!(res, Err(LinearViolation::MultiShotResume(_))));
    }

    #[test]
    fn resume_once_counts_as_consume() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        assert!(t.use_binding(id, UseKind::ResumeOnce).is_ok());
        let violations = t.close_scope();
        assert!(violations.is_empty());
    }

    #[test]
    fn iso_read_without_consume_flagged() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        let res = t.use_binding(id, UseKind::Read);
        assert!(matches!(res, Err(LinearViolation::ReadWithoutConsume(_))));
    }

    #[test]
    fn use_after_scope_is_error() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        let _ = t.use_binding(id, UseKind::Consume);
        let _ = t.close_scope();
        let res = t.use_binding(id, UseKind::Consume);
        assert!(matches!(res, Err(LinearViolation::UseAfterScope(_))));
    }

    #[test]
    fn multi_binding_tracking() {
        let mut t = LinearTracker::new();
        t.introduce(BindingId(1), CapKind::Iso);
        t.introduce(BindingId(2), CapKind::Iso);
        t.introduce(BindingId(3), CapKind::Val);
        assert!(t.use_binding(BindingId(1), UseKind::Consume).is_ok());
        // 2 is leaked, 3 is non-linear so ignored.
        let violations = t.close_scope();
        assert_eq!(violations.len(), 1);
        assert!(matches!(violations[0], LinearViolation::Leak(BindingId(2))));
    }

    #[test]
    fn get_returns_current_record() {
        let mut t = LinearTracker::new();
        let id = BindingId(1);
        t.introduce(id, CapKind::Iso);
        let r = t.get(id).unwrap();
        assert_eq!(r.cap, CapKind::Iso);
        assert_eq!(r.consume_count, 0);
    }
}

// § lineage.rs · ordered-list of SigmaEventLike events
// ══════════════════════════════════════════════════════════════════════════════
// § I> sort-key : (ts asc · id asc) — STABLE deterministic order
// § I> ts-equal events tie-break on id-bytes lexicographic-asc
// § I> empty lineage permitted ; recompute-empty → snapshot-empty
// ══════════════════════════════════════════════════════════════════════════════
use std::cmp::Ordering;

use thiserror::Error;

use crate::event::{EventId, SigmaEventLike};

/// Errors detected during lineage-construction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LineageError {
    /// Two events share the same id — chain is malformed.
    #[error("lineage contains duplicate event-id")]
    DuplicateId,
    /// A parent_id reference does not appear earlier in the lineage.
    #[error("parent_id references unknown ancestor")]
    DanglingParent,
}

/// Ordered, validated lineage of events.
///
/// Stored sorted by (`ts` asc · `id` asc). Cloning preserves order.
#[derive(Debug, Clone)]
pub struct Lineage<E: SigmaEventLike + Clone> {
    events: Vec<E>,
}

impl<E: SigmaEventLike + Clone> Default for Lineage<E> {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl<E: SigmaEventLike + Clone> Lineage<E> {
    /// Empty lineage.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build by sorting + validating an unordered collection.
    ///
    /// - Sorts stable by (`ts` asc · `id` asc)
    /// - Rejects duplicate ids
    /// - Allows missing parents (callers may relax DAG constraint)
    pub fn from_unsorted(mut events: Vec<E>) -> Result<Self, LineageError> {
        events.sort_by(|a, b| match a.ts().cmp(&b.ts()) {
            Ordering::Equal => a.id().cmp(&b.id()),
            ord => ord,
        });
        // Duplicate-id check.
        for window in events.windows(2) {
            if window[0].id() == window[1].id() {
                return Err(LineageError::DuplicateId);
            }
        }
        Ok(Self { events })
    }

    /// Strict variant : also validates parent_id references appear earlier.
    pub fn from_unsorted_strict(events: Vec<E>) -> Result<Self, LineageError> {
        let chain = Self::from_unsorted(events)?;
        let mut seen: std::collections::BTreeSet<EventId> = std::collections::BTreeSet::new();
        for e in &chain.events {
            if let Some(p) = e.parent_id() {
                if !seen.contains(&p) {
                    return Err(LineageError::DanglingParent);
                }
            }
            seen.insert(e.id());
        }
        Ok(chain)
    }

    /// Borrow events in sorted-order.
    pub fn events(&self) -> &[E] {
        &self.events
    }

    /// Number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True iff empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MockSigmaEvent;

    #[test]
    fn empty_lineage_construct() {
        let l: Lineage<MockSigmaEvent> = Lineage::empty();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn sort_stable_by_ts_then_id() {
        let a = MockSigmaEvent::seeded(0x05, 10, None); // id starts 0x05..
        let b = MockSigmaEvent::seeded(0x01, 5, None); // earlier ts
        let c = MockSigmaEvent::seeded(0x09, 10, None); // same ts as a, larger id
        let l = Lineage::from_unsorted(vec![a.clone(), b.clone(), c.clone()]).unwrap();
        let ev = l.events();
        // expected order : b (ts=5) ; a (ts=10, id=05..) ; c (ts=10, id=09..)
        assert_eq!(ev[0].id(), b.id());
        assert_eq!(ev[1].id(), a.id());
        assert_eq!(ev[2].id(), c.id());
    }

    #[test]
    fn duplicate_id_rejected() {
        let a = MockSigmaEvent::seeded(0x05, 10, None);
        let b = a.clone(); // duplicate id
        let err = Lineage::from_unsorted(vec![a, b]).unwrap_err();
        assert_eq!(err, LineageError::DuplicateId);
    }
}

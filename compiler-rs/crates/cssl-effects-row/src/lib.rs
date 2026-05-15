#![forbid(unsafe_code)]
#![doc = "cssl-effects-row — Koka-style row-typed effects.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-effects-row. \
An `EffectRow` is a (sorted) set of effect labels plus a tail (`Closed` or `Open` \
on a row-variable). `union` joins rows ; `discharge` removes a label (handler \
discharge). A row is `pure` iff its label-set is empty and its tail is `Closed`."]

use cssl_cas::{cid_of_bytes, CanonicalEncode, Cid};
use std::collections::BTreeSet;

/// Effect label (string-interned externally if needed).
pub type EffectLabel = String;

/// Identifier for a row-tail variable in `Open` rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RowVar(pub u32);

/// Tail of a row : closed (no extension allowed) or open on a row variable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RowTail {
    Closed,
    Open(RowVar),
}

/// A row-typed effect signature : sorted label-set + tail.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectRow {
    pub labels: BTreeSet<EffectLabel>,
    pub tail: RowTail,
}

impl EffectRow {
    /// The empty closed row (= pure).
    #[must_use]
    pub fn empty() -> Self {
        Self { labels: BTreeSet::new(), tail: RowTail::Closed }
    }

    /// A closed row containing exactly one label.
    #[must_use]
    pub fn singleton(label: EffectLabel) -> Self {
        let mut labels = BTreeSet::new();
        labels.insert(label);
        Self { labels, tail: RowTail::Closed }
    }

    /// Open the row on a row variable.
    #[must_use]
    pub fn open(self, var: RowVar) -> Self {
        Self { labels: self.labels, tail: RowTail::Open(var) }
    }

    /// Add a label to the row.
    #[must_use]
    pub fn extend(mut self, label: EffectLabel) -> Self {
        self.labels.insert(label);
        self
    }

    /// Remove a label (handler discharge). No-op if label is not present.
    #[must_use]
    pub fn discharge(mut self, label: &str) -> Self {
        self.labels.remove(label);
        self
    }

    /// Union two rows : label-sets union ; tails must agree on closed-ness
    /// (open rows widen if both open with same var ; otherwise the result is
    /// closed unless either is open on a unifiable variable — full unification
    /// is deferred to the elaborator wave).
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let mut labels = self.labels;
        labels.extend(other.labels);
        let tail = match (self.tail, other.tail) {
            (RowTail::Closed, RowTail::Closed) => RowTail::Closed,
            (RowTail::Open(a), RowTail::Open(b)) if a == b => RowTail::Open(a),
            // Mixed / non-matching open tails : conservative-close.
            _ => RowTail::Closed,
        };
        Self { labels, tail }
    }

    /// `true` iff the row carries no effects and is closed.
    #[must_use]
    pub fn is_pure(&self) -> bool {
        self.labels.is_empty() && matches!(self.tail, RowTail::Closed)
    }

    /// Whether `label` is present.
    #[must_use]
    pub fn contains(&self, label: &str) -> bool {
        self.labels.contains(label)
    }

    /// Canonical content-Cid.
    #[must_use]
    pub fn cid(&self) -> Cid {
        let mut buf = Vec::new();
        self.encode(&mut buf);
        cid_of_bytes(&buf)
    }
}

impl CanonicalEncode for EffectRow {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.labels.len() as u64).to_le_bytes());
        for l in &self.labels {
            l.encode(out);
        }
        match &self.tail {
            RowTail::Closed => out.push(0),
            RowTail::Open(v) => {
                out.push(1);
                out.extend_from_slice(&v.0.to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_row_is_pure() {
        assert!(EffectRow::empty().is_pure());
    }

    #[test]
    fn singleton_row_contains_label_and_is_not_pure() {
        let r = EffectRow::singleton("io".into());
        assert!(r.contains("io"));
        assert!(!r.is_pure());
    }

    #[test]
    fn discharge_removes_label() {
        let r = EffectRow::singleton("io".into()).discharge("io");
        assert!(r.is_pure(), "discharge of sole label must yield pure row");
    }

    #[test]
    fn discharge_missing_label_is_noop() {
        let r = EffectRow::singleton("io".into()).discharge("missing");
        assert!(r.contains("io"));
    }

    #[test]
    fn union_idempotent_on_same_labels() {
        let r = EffectRow::singleton("io".into()).extend("state".into());
        let u = r.clone().union(r.clone());
        assert_eq!(u, r);
    }

    #[test]
    fn union_combines_distinct_labels() {
        let a = EffectRow::singleton("io".into());
        let b = EffectRow::singleton("state".into());
        let u = a.union(b);
        assert!(u.contains("io"));
        assert!(u.contains("state"));
    }

    #[test]
    fn row_cid_label_set_invariant() {
        // BTreeSet ordering means insertion-order independence is automatic.
        let r1 = EffectRow::empty().extend("io".into()).extend("state".into());
        let r2 = EffectRow::empty().extend("state".into()).extend("io".into());
        assert_eq!(r1.cid(), r2.cid());
    }

    #[test]
    fn open_row_tail_distinguishes_cid() {
        let closed = EffectRow::singleton("io".into());
        let opened = EffectRow::singleton("io".into()).open(RowVar(0));
        assert_ne!(closed.cid(), opened.cid());
    }
}

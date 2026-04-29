//! BlackBoard — shared deterministic state-store for FSM / BT / UtilityAI nodes.
//!
//! § THESIS
//!   In game-AI literature a "blackboard" is a key-value scratch-space
//!   that decision nodes read+write between ticks. Sequence/Selector/
//!   Parallel BT nodes all need a place to stash transient state ; FSMs
//!   need predicate inputs ; UtilityAI considerations read sensor outputs.
//!   This is that shared store.
//!
//! § DETERMINISM
//!   - `BTreeMap`-backed storage so iteration order is reproducible
//!     (matches the omega_step replay-determinism contract per
//!     `cssl-substrate-omega-step::omega_stub`).
//!   - Float values stored as f64 ; bit-equality compare is the canonical
//!     replay-test (NaN-payload sensitive).
//!   - No internal RNG — all randomness comes from the caller's `DetRng`.
//!   - No clock reads.
//!
//! § VALUE TYPES (`BbValue`)
//!   - `Int(i64)`   : counters, bucket-ids, integer state
//!   - `Float(f64)` : utility-scores, sensor outputs, position scalars
//!   - `Bool(bool)` : predicate flags (alert? alive?)
//!   - `Vec2([f64; 2])` : 2D positions / velocities / facings
//!   - `Text(String)`   : labels (current-state-name, target-tag)
//!
//!   Stage-0 STABLE — adding a variant is non-breaking.
//!
//! § SOVEREIGNTY
//!   The BlackBoard does NOT carry a `CompanionView` projection ; if a
//!   caller stuffs Companion-state into a BlackBoard they are bypassing
//!   the read-only-projection discipline. The `Companion` doc-block on
//!   `lib.rs` flags this. The runtime guard is in `brain::AiBrain` —
//!   `AiBrain::new(ActorKind::Companion, ...)` returns
//!   `CompanionGuardError`.
//!
//! § INTEROP-WITH-OMEGA-STEP
//!   The BlackBoard is the AI brain's local state ; the canonical
//!   Ω-tensor (`OmegaSnapshot`) is the world state. The brain reads
//!   from `OmegaSnapshot` (perception), writes to `BlackBoard` (decision
//!   memory), and writes to `OmegaSnapshot` (action effect). Stage-0
//!   keeps these separate so the BlackBoard can be reset without
//!   disturbing world state.

use std::collections::BTreeMap;

use thiserror::Error;

/// A typed scalar value stored on a BlackBoard. Stage-0 stable.
///
/// § DESIGN
///   We chose a closed-enum over `Box<dyn Any>` so `bit_eq` is meaningful
///   (replay-determinism) + so audit-walkers can render BlackBoard state
///   with no type-registry lookup. Trade-off : type extensions are spec-
///   amendments. Acceptable at stage-0.
#[derive(Debug, Clone, PartialEq)]
pub enum BbValue {
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit IEEE-754 float.
    Float(f64),
    /// Boolean flag.
    Bool(bool),
    /// 2D vector — positions, velocities, facings, etc.
    Vec2([f64; 2]),
    /// UTF-8 text — labels, state-names, target-tags.
    Text(String),
}

impl BbValue {
    /// Bit-equality compare — float-aware (`NaN` payload distinguishes).
    /// Used by replay-tests + the brain's snapshot oracle.
    #[must_use]
    pub fn bit_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Vec2(a), Self::Vec2(b)) => {
                a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
            }
            (Self::Text(a), Self::Text(b)) => a == b,
            _ => false,
        }
    }

    /// Canonical type-name for diagnostic + audit-log rendering.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::Bool(_) => "Bool",
            Self::Vec2(_) => "Vec2",
            Self::Text(_) => "Text",
        }
    }

    /// Read as `i64` — returns `None` on type mismatch.
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        if let Self::Int(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Read as `f64` — returns `None` on type mismatch.
    #[must_use]
    pub const fn as_float(&self) -> Option<f64> {
        if let Self::Float(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Read as `bool` — returns `None` on type mismatch.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Read as `[f64; 2]` — returns `None` on type mismatch.
    #[must_use]
    pub const fn as_vec2(&self) -> Option<[f64; 2]> {
        if let Self::Vec2(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    /// Read as `&str` — returns `None` on type mismatch.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        if let Self::Text(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

/// Errors the BlackBoard surfaces.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum BlackBoardError {
    /// Get-or-fail returned with no entry.
    #[error("AIBEHAV0010 — BlackBoard key '{key}' not found")]
    KeyNotFound { key: String },
    /// Get-typed observed a different type than requested.
    #[error("AIBEHAV0011 — BlackBoard key '{key}' has type {actual} but requested {requested}")]
    TypeMismatch {
        key: String,
        requested: &'static str,
        actual: &'static str,
    },
}

impl BlackBoardError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::KeyNotFound { .. } => "AIBEHAV0010",
            Self::TypeMismatch { .. } => "AIBEHAV0011",
        }
    }
}

/// Shared key-value state-store for AI decision nodes.
///
/// § Determinism : `BTreeMap` chosen over `HashMap` so iteration order is
///   reproducible across runs (matches omega_step replay-determinism contract).
#[derive(Debug, Clone, Default)]
pub struct BlackBoard {
    entries: BTreeMap<String, BbValue>,
}

impl BlackBoard {
    /// Construct a fresh empty BlackBoard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a value at `key`. Overwrites the previous entry if present.
    pub fn set(&mut self, key: impl Into<String>, value: BbValue) {
        self.entries.insert(key.into(), value);
    }

    /// Convenience setter for an integer.
    pub fn set_int(&mut self, key: impl Into<String>, value: i64) {
        self.set(key, BbValue::Int(value));
    }

    /// Convenience setter for a float.
    pub fn set_float(&mut self, key: impl Into<String>, value: f64) {
        self.set(key, BbValue::Float(value));
    }

    /// Convenience setter for a bool.
    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) {
        self.set(key, BbValue::Bool(value));
    }

    /// Convenience setter for a 2D vector.
    pub fn set_vec2(&mut self, key: impl Into<String>, value: [f64; 2]) {
        self.set(key, BbValue::Vec2(value));
    }

    /// Convenience setter for text.
    pub fn set_text(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.set(key, BbValue::Text(value.into()));
    }

    /// Get a value by key. Returns `None` if absent.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&BbValue> {
        self.entries.get(key)
    }

    /// Get-or-error. Surfaces `KeyNotFound` so audit logs see the failure.
    pub fn require(&self, key: &str) -> Result<&BbValue, BlackBoardError> {
        self.entries
            .get(key)
            .ok_or_else(|| BlackBoardError::KeyNotFound {
                key: key.to_string(),
            })
    }

    /// Typed get for `i64` ; surfaces `TypeMismatch` if present-but-wrong-type.
    pub fn get_int(&self, key: &str) -> Result<i64, BlackBoardError> {
        let v = self.require(key)?;
        v.as_int().ok_or_else(|| BlackBoardError::TypeMismatch {
            key: key.to_string(),
            requested: "Int",
            actual: v.type_name(),
        })
    }

    /// Typed get for `f64`.
    pub fn get_float(&self, key: &str) -> Result<f64, BlackBoardError> {
        let v = self.require(key)?;
        v.as_float().ok_or_else(|| BlackBoardError::TypeMismatch {
            key: key.to_string(),
            requested: "Float",
            actual: v.type_name(),
        })
    }

    /// Typed get for `bool`.
    pub fn get_bool(&self, key: &str) -> Result<bool, BlackBoardError> {
        let v = self.require(key)?;
        v.as_bool().ok_or_else(|| BlackBoardError::TypeMismatch {
            key: key.to_string(),
            requested: "Bool",
            actual: v.type_name(),
        })
    }

    /// Typed get for `[f64; 2]`.
    pub fn get_vec2(&self, key: &str) -> Result<[f64; 2], BlackBoardError> {
        let v = self.require(key)?;
        v.as_vec2().ok_or_else(|| BlackBoardError::TypeMismatch {
            key: key.to_string(),
            requested: "Vec2",
            actual: v.type_name(),
        })
    }

    /// Typed get for `&str`.
    pub fn get_text(&self, key: &str) -> Result<&str, BlackBoardError> {
        let v = self.require(key)?;
        v.as_text().ok_or_else(|| BlackBoardError::TypeMismatch {
            key: key.to_string(),
            requested: "Text",
            actual: v.type_name(),
        })
    }

    /// Remove an entry by key. Returns the removed value if present.
    pub fn remove(&mut self, key: &str) -> Option<BbValue> {
        self.entries.remove(key)
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the board is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check if a key is set.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Iterate entries in deterministic (sorted-by-key) order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &BbValue)> {
        self.entries.iter()
    }

    /// Bit-equality compare. NaN-payload-aware via `BbValue::bit_eq`.
    /// Used by replay-tests to assert two ticks produced identical state.
    #[must_use]
    pub fn bit_eq(&self, other: &Self) -> bool {
        if self.entries.len() != other.entries.len() {
            return false;
        }
        self.entries
            .iter()
            .zip(other.entries.iter())
            .all(|((ka, va), (kb, vb))| ka == kb && va.bit_eq(vb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_blackboard() {
        let bb = BlackBoard::new();
        assert!(bb.is_empty());
        assert_eq!(bb.len(), 0);
    }

    #[test]
    fn set_and_get_int() {
        let mut bb = BlackBoard::new();
        bb.set_int("hp", 42);
        assert_eq!(bb.get_int("hp").unwrap(), 42);
    }

    #[test]
    fn set_and_get_float() {
        let mut bb = BlackBoard::new();
        bb.set_float("dist", 12.5);
        assert!((bb.get_float("dist").unwrap() - 12.5).abs() < 1e-9);
    }

    #[test]
    fn set_and_get_bool() {
        let mut bb = BlackBoard::new();
        bb.set_bool("alert", true);
        assert!(bb.get_bool("alert").unwrap());
    }

    #[test]
    fn set_and_get_vec2() {
        let mut bb = BlackBoard::new();
        bb.set_vec2("pos", [1.0, 2.0]);
        assert_eq!(bb.get_vec2("pos").unwrap(), [1.0, 2.0]);
    }

    #[test]
    fn set_and_get_text() {
        let mut bb = BlackBoard::new();
        bb.set_text("state", "patrolling");
        assert_eq!(bb.get_text("state").unwrap(), "patrolling");
    }

    #[test]
    fn key_not_found_error() {
        let bb = BlackBoard::new();
        let err = bb.get_int("missing").unwrap_err();
        assert!(matches!(err, BlackBoardError::KeyNotFound { .. }));
        assert_eq!(err.code(), "AIBEHAV0010");
    }

    #[test]
    fn type_mismatch_error() {
        let mut bb = BlackBoard::new();
        bb.set_int("hp", 5);
        let err = bb.get_float("hp").unwrap_err();
        assert!(matches!(
            err,
            BlackBoardError::TypeMismatch {
                requested: "Float",
                actual: "Int",
                ..
            }
        ));
        assert_eq!(err.code(), "AIBEHAV0011");
    }

    #[test]
    fn overwrite_replaces() {
        let mut bb = BlackBoard::new();
        bb.set_int("x", 1);
        bb.set_int("x", 2);
        assert_eq!(bb.get_int("x").unwrap(), 2);
    }

    #[test]
    fn remove_removes() {
        let mut bb = BlackBoard::new();
        bb.set_int("x", 1);
        let v = bb.remove("x").unwrap();
        assert!(matches!(v, BbValue::Int(1)));
        assert!(!bb.contains("x"));
    }

    #[test]
    fn clear_resets() {
        let mut bb = BlackBoard::new();
        bb.set_int("a", 1);
        bb.set_int("b", 2);
        bb.clear();
        assert!(bb.is_empty());
    }

    #[test]
    fn iter_is_sorted() {
        let mut bb = BlackBoard::new();
        bb.set_int("z", 3);
        bb.set_int("a", 1);
        bb.set_int("m", 2);
        let keys: Vec<&String> = bb.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, [&"a".to_string(), &"m".to_string(), &"z".to_string()]);
    }

    #[test]
    fn bb_value_bit_eq_int() {
        let a = BbValue::Int(7);
        let b = BbValue::Int(7);
        let c = BbValue::Int(8);
        assert!(a.bit_eq(&b));
        assert!(!a.bit_eq(&c));
    }

    #[test]
    fn bb_value_bit_eq_float_distinguishes_nan_payload() {
        let a = BbValue::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let b = BbValue::Float(f64::from_bits(0x7ff8_0000_0000_0002));
        assert!(!a.bit_eq(&b));
    }

    #[test]
    fn bb_value_bit_eq_vec2_float_aware() {
        let a = BbValue::Vec2([0.0, 1.0]);
        let b = BbValue::Vec2([0.0, 1.0]);
        let c = BbValue::Vec2([0.0, 2.0]);
        assert!(a.bit_eq(&b));
        assert!(!a.bit_eq(&c));
    }

    #[test]
    fn bb_value_bit_eq_type_mismatch_false() {
        let a = BbValue::Int(0);
        let b = BbValue::Bool(false);
        assert!(!a.bit_eq(&b));
    }

    #[test]
    fn type_name_strings() {
        assert_eq!(BbValue::Int(0).type_name(), "Int");
        assert_eq!(BbValue::Float(0.0).type_name(), "Float");
        assert_eq!(BbValue::Bool(false).type_name(), "Bool");
        assert_eq!(BbValue::Vec2([0.0, 0.0]).type_name(), "Vec2");
        assert_eq!(BbValue::Text(String::new()).type_name(), "Text");
    }

    #[test]
    fn bb_bit_eq_self() {
        let mut bb = BlackBoard::new();
        bb.set_int("x", 1);
        assert!(bb.bit_eq(&bb.clone()));
    }

    #[test]
    fn bb_bit_eq_detects_drift() {
        let mut a = BlackBoard::new();
        let mut b = BlackBoard::new();
        a.set_int("x", 1);
        b.set_int("x", 2);
        assert!(!a.bit_eq(&b));
    }

    #[test]
    fn bb_bit_eq_size_mismatch() {
        let mut a = BlackBoard::new();
        let mut b = BlackBoard::new();
        a.set_int("x", 1);
        a.set_int("y", 2);
        b.set_int("x", 1);
        assert!(!a.bit_eq(&b));
    }
}

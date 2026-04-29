//! UtilityAi — score-based action selection for NPC behavior.
//!
//! § THESIS
//!   Where FSMs encode "if X then state-Y" + BTs encode "try-this-tree-of-
//!   priorities", utility-AI encodes "score every option ; pick the
//!   argmax". Each action is scored by combining several **considerations**
//!   (a consideration = an input + a curve fn that maps input to a [0..1]
//!   utility). The product of consideration-scores becomes the action's
//!   total utility ; the action with the highest total wins.
//!
//!   Reference : Mark Lewis, "Building a Better Centaur" (GDC 2015).
//!
//! § DESIGN
//!   - [`Consideration`] : { input-fn(&BlackBoard) -> f64, curve : CurveKind }
//!     The input-fn is a pure read of BlackBoard ; the curve maps that
//!     value to a unit-interval utility. Stage-0 supports 4 canonical
//!     curves : Linear / Quadratic / Sigmoid / Inverse.
//!   - [`UtilityAction`] : { name, considerations : `Vec<ConsiderationId>` }
//!     The score is `min_consideration * product_of_considerations`,
//!     which is the standard "compensation factor" form (Mark Lewis 2015).
//!   - [`UtilityAi::pick`] : evaluates all actions ; returns the [`ActionId`]
//!     with the highest score. **Tie-break by ActionId ascending** —
//!     deterministic + stable across runs.
//!
//! § DETERMINISM (‼ load-bearing)
//!   - Curves are pure fns of `f64` ; same input ⇒ same output.
//!   - Consideration evaluation order = declaration order (stored in Vec).
//!   - Tie-break by `ActionId` ascending is the canonical rule ; without
//!     it, two actions scoring identically would pick non-deterministically.
//!   - No internal RNG.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - Every action + consideration carries a `name()` ; full audit trail.
//!   - Companion-archetype rejected at `UtilityAi::new`.
//!   - Score range is [0..1] for considerations ; out-of-range is clamped
//!     so a malformed input-fn cannot poison the score-space.

use std::fmt;

use thiserror::Error;

use crate::blackboard::BlackBoard;
use crate::companion_guard::{assert_not_companion, ActorKind, CompanionGuardError};

/// Score type alias for clarity. Always in [0.0, 1.0] after curve eval.
pub type UtilityScore = f64;

/// Identifier for an action in the UtilityAi's action list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(pub u32);

/// Identifier for a consideration in the UtilityAi's consideration list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConsiderationId(pub u32);

/// Curve shapes for mapping consideration-input to [0..1] utility.
///
/// § PARAMETER NOTE
///   All curves take their input as `f64` and clamp to [0..1] before
///   returning. Out-of-range input is **clamped**, not rejected — this
///   keeps `pick()` total ; a buggy input-fn produces a 0 or 1 score
///   rather than a panic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveKind {
    /// Identity-on-`[0,1]` : `f(x) = clamp(x, 0, 1)`.
    Linear,
    /// Quadratic : `f(x) = clamp(x, 0, 1)^2`. Penalizes mid-range values.
    Quadratic,
    /// Logistic-sigmoid : `f(x) = 1 / (1 + exp(-k*(x - x0)))` ; clamped.
    /// Standard parameters chosen for stability in stage-0.
    Sigmoid { k: f64, x0: f64 },
    /// Inverse : `f(x) = 1 - clamp(x, 0, 1)`. Larger input ⇒ smaller utility.
    Inverse,
}

impl CurveKind {
    /// Evaluate the curve on input `x` ; returns [0..1].
    #[must_use]
    pub fn eval(&self, x: f64) -> UtilityScore {
        let clamp = |v: f64| -> f64 {
            if v.is_nan() {
                0.0
            } else {
                v.clamp(0.0, 1.0)
            }
        };
        match self {
            Self::Linear => clamp(x),
            Self::Quadratic => {
                let c = clamp(x);
                c * c
            }
            Self::Sigmoid { k, x0 } => {
                // f(x) = 1 / (1 + exp(-k*(x - x0)))
                let t = -k * (x - x0);
                // Guard against overflow : if t very large, exp(t) → ∞ → f → 0.
                if t > 700.0 {
                    0.0
                } else if t < -700.0 {
                    1.0
                } else {
                    let v = 1.0 / (1.0 + t.exp());
                    clamp(v)
                }
            }
            Self::Inverse => 1.0 - clamp(x),
        }
    }
}

/// A consideration : an input fn + a curve that turns the input into a
/// utility score in [0..1].
pub struct Consideration {
    name: String,
    input_fn: Box<dyn Fn(&BlackBoard) -> f64 + Send + Sync>,
    curve: CurveKind,
}

impl Consideration {
    /// Construct a consideration from a name, input fn, and curve.
    pub fn new<F>(name: impl Into<String>, curve: CurveKind, input_fn: F) -> Self
    where
        F: Fn(&BlackBoard) -> f64 + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            input_fn: Box::new(input_fn),
            curve,
        }
    }

    /// Evaluate this consideration against the BlackBoard.
    #[must_use]
    pub fn eval(&self, bb: &BlackBoard) -> UtilityScore {
        let input = (self.input_fn)(bb);
        self.curve.eval(input)
    }

    /// Audit-readable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The curve this consideration uses.
    #[must_use]
    pub fn curve(&self) -> CurveKind {
        self.curve
    }
}

impl fmt::Debug for Consideration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Consideration")
            .field("name", &self.name)
            .field("curve", &self.curve)
            .field("input_fn", &"<closure>")
            .finish()
    }
}

/// A utility-AI action — a name + the considerations that score it.
#[derive(Debug)]
pub struct UtilityAction {
    name: String,
    considerations: Vec<ConsiderationId>,
}

impl UtilityAction {
    /// Construct an action.
    #[must_use]
    pub fn new(name: impl Into<String>, considerations: Vec<ConsiderationId>) -> Self {
        Self {
            name: name.into(),
            considerations,
        }
    }

    /// Audit-readable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Number of considerations contributing to this action's score.
    #[must_use]
    pub fn consideration_count(&self) -> usize {
        self.considerations.len()
    }
}

/// Errors the UtilityAi surfaces.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum UtilityAiError {
    /// Caller attempted to drive a Companion via UtilityAi.
    #[error("AIBEHAV0040 — UtilityAi rejects Companion-archetype: {0}")]
    Companion(#[from] CompanionGuardError),

    /// Action references a `ConsiderationId` not in the considerations list.
    #[error("AIBEHAV0041 — consideration id {id} out of bounds (have {count} considerations)")]
    ConsiderationOutOfBounds { id: u32, count: u32 },

    /// `pick()` called on a UtilityAi with no actions.
    #[error("AIBEHAV0042 — UtilityAi has no actions to pick from")]
    NoActions,
}

impl UtilityAiError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Companion(_) => "AIBEHAV0040",
            Self::ConsiderationOutOfBounds { .. } => "AIBEHAV0041",
            Self::NoActions => "AIBEHAV0042",
        }
    }
}

/// Score-based action selection over a set of considerations.
pub struct UtilityAi {
    considerations: Vec<Consideration>,
    actions: Vec<UtilityAction>,
}

impl UtilityAi {
    /// Construct a fresh UtilityAi for an Npc actor.
    pub fn new(kind: ActorKind) -> Result<Self, UtilityAiError> {
        assert_not_companion(kind)?;
        Ok(Self {
            considerations: Vec::new(),
            actions: Vec::new(),
        })
    }

    /// Register a consideration ; returns its id for use in actions.
    pub fn add_consideration(&mut self, c: Consideration) -> ConsiderationId {
        let id = ConsiderationId(self.considerations.len() as u32);
        self.considerations.push(c);
        id
    }

    /// Register an action ; validates that all referenced considerations exist.
    pub fn add_action(&mut self, action: UtilityAction) -> Result<ActionId, UtilityAiError> {
        let count = self.considerations.len() as u32;
        for c in &action.considerations {
            if c.0 >= count {
                return Err(UtilityAiError::ConsiderationOutOfBounds { id: c.0, count });
            }
        }
        let id = ActionId(self.actions.len() as u32);
        self.actions.push(action);
        Ok(id)
    }

    /// Evaluate `action.score = product(consideration.eval(bb))` —
    /// standard utility-AI multiplicative form.
    ///
    /// § DESIGN-NOTE
    ///   Multiplicative scoring means any consideration scoring 0 zeros
    ///   the action — this lets considerations act as gates ("must have
    ///   ammo > 0 to fire"). Additive scoring is sometimes preferred ;
    ///   stage-0 commits to multiplicative because gating is the more
    ///   common need.
    #[must_use]
    pub fn score_action(&self, action_id: ActionId, bb: &BlackBoard) -> UtilityScore {
        let Some(action) = self.actions.get(action_id.0 as usize) else {
            return 0.0;
        };
        if action.considerations.is_empty() {
            // Action with no considerations gets a fixed 0.5 score —
            // breaks ties toward "neutral" rather than "always pick".
            return 0.5;
        }
        let mut score: UtilityScore = 1.0;
        for cid in &action.considerations {
            let c = &self.considerations[cid.0 as usize];
            score *= c.eval(bb);
        }
        score.clamp(0.0, 1.0)
    }

    /// Pick the best action — argmax of `score_action`.
    /// **Tie-break by ActionId ascending.**
    pub fn pick(&self, bb: &BlackBoard) -> Result<ActionId, UtilityAiError> {
        if self.actions.is_empty() {
            return Err(UtilityAiError::NoActions);
        }
        let mut best_id = ActionId(0);
        let mut best_score = self.score_action(ActionId(0), bb);
        for i in 1..self.actions.len() {
            let id = ActionId(i as u32);
            let s = self.score_action(id, bb);
            // Strict > for tie-break by ascending id : in case of tie,
            // we keep the lower id (already in best_id).
            if s > best_score {
                best_score = s;
                best_id = id;
            }
        }
        Ok(best_id)
    }

    /// Number of registered considerations.
    #[must_use]
    pub fn consideration_count(&self) -> usize {
        self.considerations.len()
    }

    /// Number of registered actions.
    #[must_use]
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    /// Look up an action by id (for audit-log / debug).
    #[must_use]
    pub fn action(&self, id: ActionId) -> Option<&UtilityAction> {
        self.actions.get(id.0 as usize)
    }

    /// Look up a consideration by id (for audit-log / debug).
    #[must_use]
    pub fn consideration(&self, id: ConsiderationId) -> Option<&Consideration> {
        self.considerations.get(id.0 as usize)
    }
}

impl fmt::Debug for UtilityAi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UtilityAi")
            .field("consideration_count", &self.considerations.len())
            .field("action_count", &self.actions.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_curve() {
        assert!((CurveKind::Linear.eval(0.5) - 0.5).abs() < 1e-9);
        assert!((CurveKind::Linear.eval(0.0)).abs() < 1e-9);
        assert!((CurveKind::Linear.eval(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn linear_curve_clamps() {
        assert!((CurveKind::Linear.eval(2.0) - 1.0).abs() < 1e-9);
        assert!((CurveKind::Linear.eval(-1.0)).abs() < 1e-9);
    }

    #[test]
    fn linear_curve_nan_to_zero() {
        assert_eq!(CurveKind::Linear.eval(f64::NAN), 0.0);
    }

    #[test]
    fn quadratic_curve() {
        assert!((CurveKind::Quadratic.eval(0.5) - 0.25).abs() < 1e-9);
        assert!((CurveKind::Quadratic.eval(1.0) - 1.0).abs() < 1e-9);
        assert_eq!(CurveKind::Quadratic.eval(0.0), 0.0);
    }

    #[test]
    fn sigmoid_curve_midpoint() {
        let s = CurveKind::Sigmoid { k: 4.0, x0: 0.5 };
        // f(0.5) = 0.5
        assert!((s.eval(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn sigmoid_curve_extreme_high() {
        let s = CurveKind::Sigmoid { k: 4.0, x0: 0.5 };
        assert!(s.eval(10.0) > 0.99);
    }

    #[test]
    fn sigmoid_curve_extreme_low() {
        let s = CurveKind::Sigmoid { k: 4.0, x0: 0.5 };
        assert!(s.eval(-10.0) < 0.01);
    }

    #[test]
    fn sigmoid_curve_overflow_safe() {
        let s = CurveKind::Sigmoid { k: 1000.0, x0: 0.0 };
        assert_eq!(s.eval(-1000.0), 0.0);
        assert_eq!(s.eval(1000.0), 1.0);
    }

    #[test]
    fn inverse_curve() {
        assert!((CurveKind::Inverse.eval(0.0) - 1.0).abs() < 1e-9);
        assert!((CurveKind::Inverse.eval(1.0)).abs() < 1e-9);
        assert!((CurveKind::Inverse.eval(0.25) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn util_companion_rejected() {
        let err = UtilityAi::new(ActorKind::Companion).unwrap_err();
        assert!(matches!(err, UtilityAiError::Companion(_)));
        assert_eq!(err.code(), "AIBEHAV0040");
    }

    #[test]
    fn util_pick_no_actions_errors() {
        let u = UtilityAi::new(ActorKind::Npc).unwrap();
        let bb = BlackBoard::new();
        let err = u.pick(&bb).unwrap_err();
        assert_eq!(err.code(), "AIBEHAV0042");
    }

    #[test]
    fn util_consideration_id_validation() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let err = u
            .add_action(UtilityAction::new("a", vec![ConsiderationId(99)]))
            .unwrap_err();
        assert_eq!(err.code(), "AIBEHAV0041");
    }

    #[test]
    fn util_single_action_picks_it() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let cid = u.add_consideration(Consideration::new("c0", CurveKind::Linear, |bb| {
            bb.get_float("x").unwrap_or(0.0)
        }));
        let aid = u.add_action(UtilityAction::new("only", vec![cid])).unwrap();
        let mut bb = BlackBoard::new();
        bb.set_float("x", 0.7);
        assert_eq!(u.pick(&bb).unwrap(), aid);
    }

    #[test]
    fn util_higher_score_wins() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let c_hp = u.add_consideration(Consideration::new("hp", CurveKind::Linear, |bb| {
            bb.get_float("hp_norm").unwrap_or(0.0)
        }));
        let c_dist = u.add_consideration(Consideration::new("dist", CurveKind::Inverse, |bb| {
            bb.get_float("dist_norm").unwrap_or(1.0)
        }));
        let attack = u
            .add_action(UtilityAction::new("attack", vec![c_dist])) // close=high
            .unwrap();
        let _heal = u
            .add_action(UtilityAction::new("heal", vec![c_hp])) // hp_norm directly
            .unwrap();
        let mut bb = BlackBoard::new();
        // hp_norm = 0.9 (full health), dist_norm = 0.1 (close enemy) → attack score
        // attack-score = 1 - 0.1 = 0.9 ; heal-score = 0.9
        // tie : ActionId(0) is attack, picks attack (lower id).
        bb.set_float("hp_norm", 0.9);
        bb.set_float("dist_norm", 0.1);
        assert_eq!(u.pick(&bb).unwrap(), attack);
    }

    #[test]
    fn util_tie_break_lower_id_wins() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        // Two actions with no considerations both score 0.5 — tie.
        let a = u.add_action(UtilityAction::new("a", vec![])).unwrap();
        let _b = u.add_action(UtilityAction::new("b", vec![])).unwrap();
        let bb = BlackBoard::new();
        let pick = u.pick(&bb).unwrap();
        assert_eq!(pick, a, "lower-id breaks tie");
    }

    #[test]
    fn util_zero_score_consideration_zeros_action() {
        // A consideration that returns 0 zeros the whole action — gating.
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let always_one = u.add_consideration(Consideration::new("one", CurveKind::Linear, |_| 1.0));
        let always_zero =
            u.add_consideration(Consideration::new("zero", CurveKind::Linear, |_| 0.0));
        let _gate = u
            .add_action(UtilityAction::new("gated", vec![always_one, always_zero]))
            .unwrap();
        let other = u
            .add_action(UtilityAction::new("ungated", vec![always_one]))
            .unwrap();
        let bb = BlackBoard::new();
        // gated scores 1*0=0 ; ungated scores 1.0 ; ungated wins.
        assert_eq!(u.pick(&bb).unwrap(), other);
    }

    #[test]
    fn util_score_action_clamped() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let c = u.add_consideration(Consideration::new(
            "c",
            CurveKind::Linear,
            |_| 5.0, // out-of-range
        ));
        let aid = u.add_action(UtilityAction::new("a", vec![c])).unwrap();
        let bb = BlackBoard::new();
        // Curve-clamps to 1.0 ; product is 1.0.
        assert!((u.score_action(aid, &bb) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn util_consideration_count_and_action_count() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let c = u.add_consideration(Consideration::new("c", CurveKind::Linear, |_| 0.5));
        let _ = u.add_action(UtilityAction::new("a", vec![c])).unwrap();
        assert_eq!(u.consideration_count(), 1);
        assert_eq!(u.action_count(), 1);
    }

    #[test]
    fn util_consideration_named() {
        let c = Consideration::new("my-c", CurveKind::Linear, |_| 0.5);
        assert_eq!(c.name(), "my-c");
        assert_eq!(c.curve(), CurveKind::Linear);
    }

    #[test]
    fn util_action_named() {
        let a = UtilityAction::new("my-a", vec![]);
        assert_eq!(a.name(), "my-a");
        assert_eq!(a.consideration_count(), 0);
    }

    #[test]
    fn util_lookup_methods() {
        let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
        let c = u.add_consideration(Consideration::new("c", CurveKind::Linear, |_| 0.5));
        let a = u.add_action(UtilityAction::new("a", vec![c])).unwrap();
        assert!(u.action(a).is_some());
        assert!(u.consideration(c).is_some());
        assert!(u.action(ActionId(99)).is_none());
        assert!(u.consideration(ConsiderationId(99)).is_none());
    }

    #[test]
    fn util_determinism_across_runs() {
        let make = || -> UtilityAi {
            let mut u = UtilityAi::new(ActorKind::Npc).unwrap();
            let c = u.add_consideration(Consideration::new("c", CurveKind::Quadratic, |bb| {
                bb.get_float("x").unwrap_or(0.0)
            }));
            let _ = u.add_action(UtilityAction::new("a", vec![c])).unwrap();
            u
        };
        let a = make();
        let b = make();
        let mut bb = BlackBoard::new();
        bb.set_float("x", 0.7);
        assert_eq!(a.pick(&bb).unwrap(), b.pick(&bb).unwrap());
        let sa = a.score_action(ActionId(0), &bb);
        let sb = b.score_action(ActionId(0), &bb);
        assert_eq!(sa.to_bits(), sb.to_bits());
    }
}

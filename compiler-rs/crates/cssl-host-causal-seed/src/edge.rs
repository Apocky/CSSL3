// § T11-W4-CAUSAL : edge ← causal-DAG directed edge
// ════════════════════════════════════════════════════════════════════
// § I> CausalEdge = ⟨src, dst, kind, weight⟩ ; src→dst
// § I> EdgeKind ∈ ⟪Causes Enables Blocks Implies Follows Contradicts⟫
// § I> validate : ¬self-loop ∧ finite-weight ∧ Causes/Enables ⇒ weight>0
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Variant of causal relationship between two nodes.
///
/// § I> Causes / Enables : positive force ; weight must be > 0
/// § I> Blocks / Contradicts : negative force ; weight may be any finite f32
/// § I> Implies / Follows : structural / temporal ordering ; weight = strength
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Causes,
    Enables,
    Blocks,
    Implies,
    Follows,
    Contradicts,
}

/// Directed weighted edge ← src→dst in causal-DAG.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CausalEdge {
    pub src: u64,
    pub dst: u64,
    pub kind: EdgeKind,
    pub weight: f32,
}

/// Validation failures on edge construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeErr {
    /// src == dst — self-loops forbidden in DAG.
    SelfLoop,
    /// weight = NaN or ±∞.
    NaN,
    /// kind ∈ {Causes,Enables} but weight ≤ 0.
    NegativeCausalWeight,
}

impl CausalEdge {
    /// Construct edge ; does NOT validate — call `.validate()` to check.
    #[must_use]
    pub fn new(src: u64, dst: u64, kind: EdgeKind, weight: f32) -> Self {
        Self { src, dst, kind, weight }
    }

    /// Check well-formedness : non-self-loop · finite weight · positive for causal-positive kinds.
    pub fn validate(&self) -> Result<(), EdgeErr> {
        if self.src == self.dst {
            return Err(EdgeErr::SelfLoop);
        }
        if !self.weight.is_finite() {
            return Err(EdgeErr::NaN);
        }
        if matches!(self.kind, EdgeKind::Causes | EdgeKind::Enables) && self.weight <= 0.0 {
            return Err(EdgeErr::NegativeCausalWeight);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_edge_passes_validate() {
        let e = CausalEdge::new(1, 2, EdgeKind::Causes, 0.7);
        assert_eq!(e.validate(), Ok(()));

        let f = CausalEdge::new(3, 4, EdgeKind::Blocks, -0.4);
        assert_eq!(f.validate(), Ok(()));

        let g = CausalEdge::new(5, 6, EdgeKind::Implies, 0.0);
        assert_eq!(g.validate(), Ok(())); // weight=0 OK for non-Causes/Enables
    }

    #[test]
    fn self_loop_rejected() {
        let e = CausalEdge::new(7, 7, EdgeKind::Causes, 1.0);
        assert_eq!(e.validate(), Err(EdgeErr::SelfLoop));
    }

    #[test]
    fn nan_and_inf_weight_rejected() {
        let e1 = CausalEdge::new(1, 2, EdgeKind::Implies, f32::NAN);
        assert_eq!(e1.validate(), Err(EdgeErr::NaN));

        let e2 = CausalEdge::new(1, 2, EdgeKind::Implies, f32::INFINITY);
        assert_eq!(e2.validate(), Err(EdgeErr::NaN));

        let e3 = CausalEdge::new(1, 2, EdgeKind::Implies, f32::NEG_INFINITY);
        assert_eq!(e3.validate(), Err(EdgeErr::NaN));
    }

    #[test]
    fn negative_weight_for_causes_or_enables_rejected() {
        let e1 = CausalEdge::new(1, 2, EdgeKind::Causes, -0.1);
        assert_eq!(e1.validate(), Err(EdgeErr::NegativeCausalWeight));

        let e2 = CausalEdge::new(1, 2, EdgeKind::Enables, 0.0);
        assert_eq!(e2.validate(), Err(EdgeErr::NegativeCausalWeight));

        let e3 = CausalEdge::new(1, 2, EdgeKind::Enables, -5.0);
        assert_eq!(e3.validate(), Err(EdgeErr::NegativeCausalWeight));
    }

    #[test]
    fn serde_roundtrip_preserves_all_fields() {
        let e = CausalEdge::new(11, 22, EdgeKind::Contradicts, -0.123);
        let json = serde_json::to_string(&e).expect("serialize");
        let back: CausalEdge = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(e, back);
        assert_eq!(back.src, 11);
        assert_eq!(back.dst, 22);
        assert_eq!(back.kind, EdgeKind::Contradicts);
        assert!((back.weight - -0.123).abs() < 1e-6);
    }
}

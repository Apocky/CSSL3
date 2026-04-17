//! CSSLv3 Jet<T, N> — higher-order automatic differentiation via Taylor-series.
//!
//! § SPEC : `specs/17_JETS.csl` + `specs/05_AUTODIFF.csl` § JET-EXPANSION.
//!
//! § DESIGN
//!   A `Jet<T, N>` represents a value plus its N-th order Taylor coefficients :
//!
//!   ```text
//!   Jet<T, N> = (value : T, d1 : T, d2 : T, …, dN : T)
//!   ```
//!
//!   Common operations (construct, project, add, mul, sin, …) propagate tangents
//!   up to order N automatically. Stage-0 provides the type machinery + op
//!   specifications ; actual code-generation happens via `@staged`-per-N
//!   specialization in `cssl-staging` (T8).
//!
//! § OPERATIONS (cssl.jet.*)
//!   - `construct` : build a Jet from N+1 coefficients.
//!   - `project`   : extract the k-th coefficient (0 = primal).
//!   - `add`       : `Jet<T,N> + Jet<T,N>` — elementwise.
//!   - `mul`       : Leibniz product rule up to order N.
//!   - `apply`     : apply a scalar fn with Taylor-series expansion.
//!
//! § NOT-YET (T7-phase-2 / T17-extensions)
//!   - Runtime representation (tuple vs array vs struct-of-arrays).
//!   - Jet<T, ∞> via co-inductive lazy-stream.
//!   - SMT-discharge of Jet composition invariants.
//!   - Full HIR-level `Jet` type-constructor recognition.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]

use thiserror::Error;

/// Taylor-series order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct JetOrder(pub u32);

impl JetOrder {
    /// A 1st-order Jet (primal + first derivative) — most common use-case.
    pub const FIRST: Self = Self(1);
    /// A 2nd-order Jet (primal + first + second derivative).
    pub const SECOND: Self = Self(2);
    /// Number of coefficients in a Jet of this order (`N + 1`).
    #[must_use]
    pub const fn coefficient_count(self) -> u32 {
        self.0.saturating_add(1)
    }
}

/// Jet operations recognized by the staging pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JetOp {
    /// Construct a Jet from primal + N tangent coefficients.
    Construct,
    /// Project the k-th coefficient.
    Project,
    /// Elementwise addition.
    Add,
    /// Leibniz product rule.
    Mul,
    /// Apply a scalar fn with Taylor-series expansion.
    Apply,
}

impl JetOp {
    /// Canonical source-form name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Construct => "cssl.jet.construct",
            Self::Project => "cssl.jet.project",
            Self::Add => "cssl.jet.add",
            Self::Mul => "cssl.jet.mul",
            Self::Apply => "cssl.jet.apply",
        }
    }

    /// All 5 operations in canonical order.
    pub const ALL: [Self; 5] = [
        Self::Construct,
        Self::Project,
        Self::Add,
        Self::Mul,
        Self::Apply,
    ];
}

/// Signature of a `JetOp` : operand-arity + result-arity + order-dependence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JetSignature {
    /// Number of Jet operands.
    pub jet_operands: u32,
    /// Number of scalar operands.
    pub scalar_operands: u32,
    /// Number of Jet results.
    pub jet_results: u32,
    /// Whether the op's behavior depends on `JetOrder`.
    pub order_dependent: bool,
}

impl JetOp {
    /// Signature for this op at a given order.
    #[must_use]
    pub const fn signature(self) -> JetSignature {
        match self {
            // construct : 1+N scalar operands → 1 Jet result (order-dependent).
            Self::Construct => JetSignature {
                jet_operands: 0,
                scalar_operands: 0, // variadic — caller enforces 1+N by order
                jet_results: 1,
                order_dependent: true,
            },
            // project : 1 Jet + 1 scalar index → 1 scalar result.
            Self::Project => JetSignature {
                jet_operands: 1,
                scalar_operands: 1,
                jet_results: 0,
                order_dependent: false,
            },
            // add : 2 Jets → 1 Jet (order preserved).
            Self::Add => JetSignature {
                jet_operands: 2,
                scalar_operands: 0,
                jet_results: 1,
                order_dependent: true,
            },
            // mul : 2 Jets → 1 Jet (order preserved).
            Self::Mul => JetSignature {
                jet_operands: 2,
                scalar_operands: 0,
                jet_results: 1,
                order_dependent: true,
            },
            // apply : 1 scalar-fn + 1 Jet → 1 Jet.
            Self::Apply => JetSignature {
                jet_operands: 1,
                scalar_operands: 1, // the fn reference counts as 1 scalar-operand
                jet_results: 1,
                order_dependent: true,
            },
        }
    }
}

/// Failure modes for Jet operations.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum JetError {
    /// A `Project` call accessed an index ≥ `coefficient_count(order)`.
    #[error("jet-project out-of-bounds : index {index} ≥ count {count} (order {order})")]
    ProjectOutOfBounds { index: u32, count: u32, order: u32 },
    /// Order mismatch between operand Jets.
    #[error("jet-order mismatch : lhs order {lhs} vs rhs order {rhs}")]
    OrderMismatch { lhs: u32, rhs: u32 },
    /// Construct was called with the wrong number of coefficients.
    #[error("jet-construct arity mismatch : expected {expected} coefficients, got {actual}")]
    ArityMismatch { expected: u32, actual: u32 },
}

/// Validate that a `construct` call has the right number of coefficients for an order.
pub fn validate_construct(order: JetOrder, coefficient_count: u32) -> Result<(), JetError> {
    let expected = order.coefficient_count();
    if coefficient_count != expected {
        return Err(JetError::ArityMismatch {
            expected,
            actual: coefficient_count,
        });
    }
    Ok(())
}

/// Validate that a `project` call's index is in-range for the order.
pub fn validate_project(order: JetOrder, index: u32) -> Result<(), JetError> {
    let count = order.coefficient_count();
    if index >= count {
        return Err(JetError::ProjectOutOfBounds {
            index,
            count,
            order: order.0,
        });
    }
    Ok(())
}

/// Validate that two Jet operands have matching orders.
pub fn validate_binary_order(lhs: JetOrder, rhs: JetOrder) -> Result<(), JetError> {
    if lhs != rhs {
        return Err(JetError::OrderMismatch {
            lhs: lhs.0,
            rhs: rhs.0,
        });
    }
    Ok(())
}

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{
        validate_binary_order, validate_construct, validate_project, JetError, JetOp, JetOrder,
        STAGE0_SCAFFOLD,
    };

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn first_order_has_two_coefficients() {
        assert_eq!(JetOrder::FIRST.coefficient_count(), 2);
    }

    #[test]
    fn second_order_has_three_coefficients() {
        assert_eq!(JetOrder::SECOND.coefficient_count(), 3);
    }

    #[test]
    fn all_five_jet_ops() {
        assert_eq!(JetOp::ALL.len(), 5);
    }

    #[test]
    fn op_names_match_dialect_prefix() {
        for op in JetOp::ALL {
            assert!(op.name().starts_with("cssl.jet."));
        }
    }

    #[test]
    fn project_signature_takes_one_jet_one_scalar() {
        let sig = JetOp::Project.signature();
        assert_eq!(sig.jet_operands, 1);
        assert_eq!(sig.scalar_operands, 1);
        assert_eq!(sig.jet_results, 0);
    }

    #[test]
    fn add_signature_takes_two_jets() {
        let sig = JetOp::Add.signature();
        assert_eq!(sig.jet_operands, 2);
        assert_eq!(sig.jet_results, 1);
        assert!(sig.order_dependent);
    }

    #[test]
    fn construct_arity_check_passes_for_correct_count() {
        assert!(validate_construct(JetOrder::FIRST, 2).is_ok());
        assert!(validate_construct(JetOrder::SECOND, 3).is_ok());
    }

    #[test]
    fn construct_arity_check_rejects_wrong_count() {
        let res = validate_construct(JetOrder::SECOND, 2);
        assert!(matches!(res, Err(JetError::ArityMismatch { .. })));
    }

    #[test]
    fn project_index_in_range_ok() {
        assert!(validate_project(JetOrder::SECOND, 0).is_ok());
        assert!(validate_project(JetOrder::SECOND, 1).is_ok());
        assert!(validate_project(JetOrder::SECOND, 2).is_ok());
    }

    #[test]
    fn project_index_oob_rejected() {
        let res = validate_project(JetOrder::SECOND, 3);
        assert!(matches!(res, Err(JetError::ProjectOutOfBounds { .. })));
    }

    #[test]
    fn binary_order_match_ok() {
        assert!(validate_binary_order(JetOrder::FIRST, JetOrder::FIRST).is_ok());
    }

    #[test]
    fn binary_order_mismatch_rejected() {
        let res = validate_binary_order(JetOrder::FIRST, JetOrder::SECOND);
        assert!(matches!(res, Err(JetError::OrderMismatch { .. })));
    }

    #[test]
    fn order_ordering_total() {
        let a = JetOrder(1);
        let b = JetOrder(2);
        assert!(a < b);
    }
}

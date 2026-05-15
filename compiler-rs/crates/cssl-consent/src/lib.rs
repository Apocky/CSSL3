#![forbid(unsafe_code)]
#![doc = "cssl-consent — consent-grade lattice.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-consent. \
Lattice: `Denied < Implicit < Explicit` ; `Revoked` is a side-state reachable from \
any non-Denied level and acts as a sticky-deny until re-grant. `permits(required)` \
returns true iff the current level is `≥ required` AND not `Revoked`."]

/// Consent level. Total order on the active spectrum (`Denied < Implicit < Explicit`).
/// `Revoked` is a separate state that does NOT permit anything until reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Consent {
    Denied,
    Implicit,
    Explicit,
    Revoked,
}

impl Consent {
    /// Numeric rank for non-Revoked levels (Revoked yields 0 — same as Denied for permits).
    fn rank(self) -> u8 {
        match self {
            Self::Denied | Self::Revoked => 0,
            Self::Implicit => 1,
            Self::Explicit => 2,
        }
    }

    /// Greatest-lower-bound on the active spectrum. `Revoked` propagates.
    #[must_use]
    pub fn meet(self, other: Self) -> Self {
        if matches!(self, Self::Revoked) || matches!(other, Self::Revoked) {
            return Self::Revoked;
        }
        match self.rank().min(other.rank()) {
            0 => Self::Denied,
            1 => Self::Implicit,
            _ => Self::Explicit,
        }
    }

    /// Least-upper-bound on the active spectrum. `Revoked` is absorbing for join too
    /// (you cannot raise consent over a Revoked state without explicit re-grant).
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        if matches!(self, Self::Revoked) || matches!(other, Self::Revoked) {
            return Self::Revoked;
        }
        match self.rank().max(other.rank()) {
            0 => Self::Denied,
            1 => Self::Implicit,
            _ => Self::Explicit,
        }
    }

    /// `true` iff this level permits an operation requiring `required`.
    /// `Revoked` permits nothing.
    #[must_use]
    pub fn permits(self, required: Self) -> bool {
        if matches!(self, Self::Revoked) || matches!(required, Self::Revoked) {
            return false;
        }
        self.rank() >= required.rank()
    }
}

/// A scoped consent grant : tagged operation + level + optional expiry timestamp.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConsentScope {
    pub op: String,
    pub level: Consent,
    /// Absolute expiry timestamp (caller's clock). `None` = no expiry.
    pub expires: Option<u64>,
}

impl ConsentScope {
    /// `true` iff still valid at `now` (i.e., not expired AND level permits `required`).
    #[must_use]
    pub fn permits_at(&self, required: Consent, now: u64) -> bool {
        if let Some(t) = self.expires {
            if now >= t { return false; }
        }
        self.level.permits(required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lattice_idempotent() {
        for c in [Consent::Denied, Consent::Implicit, Consent::Explicit] {
            assert_eq!(c.meet(c), c);
            assert_eq!(c.join(c), c);
        }
    }

    #[test]
    fn permits_respects_lattice_order() {
        assert!(Consent::Explicit.permits(Consent::Implicit));
        assert!(Consent::Explicit.permits(Consent::Denied));
        assert!(Consent::Implicit.permits(Consent::Denied));
        assert!(!Consent::Denied.permits(Consent::Implicit));
        assert!(!Consent::Implicit.permits(Consent::Explicit));
    }

    #[test]
    fn revoked_permits_nothing() {
        assert!(!Consent::Revoked.permits(Consent::Denied));
        assert!(!Consent::Revoked.permits(Consent::Explicit));
    }

    #[test]
    fn revoked_is_absorbing_under_meet_and_join() {
        assert_eq!(Consent::Revoked.meet(Consent::Explicit), Consent::Revoked);
        assert_eq!(Consent::Explicit.join(Consent::Revoked), Consent::Revoked);
    }

    #[test]
    fn consent_scope_expires_check() {
        let s = ConsentScope { op: "read".into(), level: Consent::Explicit, expires: Some(100) };
        assert!(s.permits_at(Consent::Implicit, 50));
        assert!(!s.permits_at(Consent::Implicit, 150));
    }

    #[test]
    fn explicit_required_for_user_facing_op() {
        let implicit = ConsentScope { op: "telemetry".into(), level: Consent::Implicit, expires: None };
        assert!(!implicit.permits_at(Consent::Explicit, 0),
            "PD : implicit must not satisfy explicit-required ops");
    }
}

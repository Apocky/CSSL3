// § cap.rs — capability bits
// Each public method on `StripeClient` requires the caller's granted-bits
// to include the method's required-bit. CapDenied is fatal AND audit-emitted.

/// `STRIPE_CAP_CHECKOUT` — required for `create_checkout_session`.
pub const STRIPE_CAP_CHECKOUT: u32 = 1 << 0;

/// `STRIPE_CAP_WEBHOOK` — required for `verify_webhook`.
pub const STRIPE_CAP_WEBHOOK: u32 = 1 << 1;

/// `STRIPE_CAP_REFUND` — required for `create_refund`.
pub const STRIPE_CAP_REFUND: u32 = 1 << 2;

/// `STRIPE_CAP_SUBSCRIPTION` — required for any `subscription_*` method.
pub const STRIPE_CAP_SUBSCRIPTION: u32 = 1 << 3;

/// § StripeCap — newtype wrapper to keep call-sites self-documenting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StripeCap(pub u32);

impl StripeCap {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(
        STRIPE_CAP_CHECKOUT | STRIPE_CAP_WEBHOOK | STRIPE_CAP_REFUND | STRIPE_CAP_SUBSCRIPTION,
    );

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_bits_distinct() {
        assert_ne!(STRIPE_CAP_CHECKOUT, STRIPE_CAP_WEBHOOK);
        assert_ne!(STRIPE_CAP_REFUND, STRIPE_CAP_SUBSCRIPTION);
        assert_eq!(
            STRIPE_CAP_CHECKOUT | STRIPE_CAP_WEBHOOK | STRIPE_CAP_REFUND | STRIPE_CAP_SUBSCRIPTION,
            0b1111
        );
    }

    #[test]
    fn cap_union_idempotent() {
        let c1 = StripeCap(STRIPE_CAP_CHECKOUT);
        let c2 = StripeCap(STRIPE_CAP_CHECKOUT | STRIPE_CAP_REFUND);
        assert!(c2.contains(c1));
        assert!(!c1.contains(c2));
    }
}

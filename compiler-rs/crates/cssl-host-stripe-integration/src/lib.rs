// § cssl-host-stripe-integration — T11-W8-D1
// ══════════════════════════════════════════════════════════════════
// § ROLE : Stripe REST-client wrapper · cap-gated · idempotent · webhook-verified
// § PRIME-DIRECTIVE : ¬ pay-for-power · cosmetic-channel-only-axiom
//   ⊑ HIGH-FIDELITY-IMPRINT (50-200 shards) · ETERNAL-ATTRIBUTION (1000)
//   ⊑ HISTORICAL-RECONSTRUCTIONS (50) · COMMISSIONED-IMPRINTS (200-500)
//   ⊑ NEVER : pay-to-win · stat-buffs · gameplay-advantage · power-progression-shortcut
// § STRUCTURAL : NEVER-store-raw-card-numbers · types-have-no-card-fields ; only opaque
//   payment_method_ids + price_lookup_keys cross-the-boundary
// § STAGE-0 : ureq + hmac + sha2 + subtle ABSENT-from-workspace.dependencies
//   M! mock-via-trait `HttpTransport` (G1-wires-real-ureq) ; flagged-in-doc
//   M! `HmacSha256Verifier` trait + blake3-keyed-MAC fallback (G1-wires-real-HMAC-SHA256)
//   M! constant-time-compare manual-volatile-loop (G1-wires-`subtle::ConstantTimeEq`)
// § HARD-CAPS :
//   - #![forbid(unsafe_code)]
//   - cap-gating @ every public-method
//   - audit-emit on-success AND-failure (no-silent-bypass)
//   - BTreeMap-only (deterministic-iteration)
//   - NO real-Stripe-HTTPS in-tests (HttpTransport-mock-only)

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
// Lint-allowances : design-deliberate tradeoffs we accept in this crate.
//   - significant_drop_tightening : we intentionally keep the Mutex held
//     for the duration of cache-coherent compound operations (read + maybe-
//     remove · read + insert) ; tightening would require restructuring that
//     opens TOCTOU windows.
//   - option_if_let_else : the `if let` form reads cleaner than `map_or`
//     when the branch performs a side effect (s.remove(key)) on the Some-arm.
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::option_if_let_else)]

// ══════════════════════════════════════════════════════════════════
// § README inline-fallback (Cargo will-error on missing-README · use cfg)
// ══════════════════════════════════════════════════════════════════

pub mod cap;
pub mod checkout;
pub mod client;
pub mod idempotency;
pub mod refund;
pub mod subscription;
pub mod transport;
pub mod webhook;

pub use cap::{StripeCap, STRIPE_CAP_CHECKOUT, STRIPE_CAP_REFUND, STRIPE_CAP_SUBSCRIPTION, STRIPE_CAP_WEBHOOK};
pub use checkout::{CheckoutSession, CheckoutSessionRequest};
pub use client::{StripeClient, StripeClientConfig};
pub use idempotency::{IdempotencyEntry, IdempotencyKey, IdempotencyStore};
pub use refund::{RefundReason, RefundRequest, RefundResponse};
pub use subscription::{Subscription, SubscriptionRequest, SubscriptionStatus};
pub use transport::{HttpRequest, HttpResponse, HttpTransport, MockHttpTransport};
pub use webhook::{HmacSha256Verifier, MockHmacVerifier, WebhookEvent, WebhookEventType};

// ══════════════════════════════════════════════════════════════════
// § StripeError — single-error-enum for-public-API
// ══════════════════════════════════════════════════════════════════

/// § StripeResult<T> ⊑ Result<T, StripeError>
pub type StripeResult<T> = Result<T, StripeError>;

/// § StripeError — public error-enum
///
/// ⊑ Network    : transport-layer failure (HTTP-5xx · connection-reset · timeout)
/// ⊑ Auth       : Stripe rejected secret-key (HTTP-401)
/// ⊑ Idempotency Conflict : reused-key with-different-payload
/// ⊑ WebhookSig Invalid : HMAC-MAC mismatch OR-malformed-header
/// ⊑ CapDenied  : caller-cap-bits do-NOT-include required-bit ; audit-emitted
/// ⊑ Unsupported : fallback-stub for-G1-not-yet-implemented
/// ⊑ SerdeError : JSON-payload malformed
/// ⊑ ApiError   : Stripe-side validation rejected (HTTP-4xx · code surfaced)
#[derive(Debug, thiserror::Error)]
pub enum StripeError {
    #[error("network transport failure: {0}")]
    Network(String),
    #[error("authentication failure (HTTP {status}): {message}")]
    Auth { status: u16, message: String },
    #[error("idempotency-key conflict: key reused with divergent payload")]
    IdempotencyConflict,
    #[error("webhook signature invalid: {reason}")]
    WebhookSigInvalid { reason: String },
    #[error("capability denied: required {required}, granted {granted}")]
    CapDenied { required: u32, granted: u32 },
    #[error("unsupported in stage-0 (G1-pending): {0}")]
    Unsupported(&'static str),
    #[error("serde error: {0}")]
    SerdeError(String),
    #[error("Stripe API error (HTTP {status}): code={code} message={message}")]
    ApiError {
        status: u16,
        code: String,
        message: String,
    },
}

impl From<serde_json::Error> for StripeError {
    fn from(e: serde_json::Error) -> Self {
        StripeError::SerdeError(e.to_string())
    }
}

// ══════════════════════════════════════════════════════════════════
// § SecretString — non-Debug · zeroize-on-drop · opaque-Display
// ══════════════════════════════════════════════════════════════════

/// § SecretString — wraps Stripe secret-key (`sk_live_*` or `sk_test_*`)
///
/// ⊑ NO-`Debug` impl (manual `<SecretString redacted>`)
/// ⊑ NO-`Display` impl (use `.expose_secret()` only-when-needed-for-HTTP-header)
/// ⊑ Drop zeroes-bytes (best-effort · stage-0 manual fill ; G1 may-add zeroize crate)
/// ⊑ NO-`Clone` impl (callers must-construct-explicitly)
pub struct SecretString {
    inner: String,
}

impl SecretString {
    /// Construct from raw secret-key string. Caller is responsible for sourcing
    /// the secret from a secure-store (env-var · OS-keyring · sealed-config).
    #[must_use]
    pub fn new(secret: String) -> Self {
        Self { inner: secret }
    }

    /// Expose the raw secret. Use ONLY when constructing the
    /// `Authorization: Bearer …` header for outgoing HTTP. Never log.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.inner
    }

    /// Length of the underlying secret bytes (useful for cap-precondition checks
    /// without exposing the secret itself).
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the secret is empty (e.g. mis-configured environment).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl core::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("<SecretString redacted>")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        // § best-effort zeroize — overwrite each byte with 0u8 via volatile-write
        // (G1 : replace with `zeroize::Zeroize::zeroize` when crate is added)
        let bytes = unsafe_free_zeroize(&mut self.inner);
        let _ = bytes;
    }
}

/// § zeroize without `unsafe` — overwrite the String's contents with NUL bytes
/// before drop. We can do this safely because `String::as_mut_vec` is unsafe but
/// we instead replace the string with a zeroed buffer of the same length.
fn unsafe_free_zeroize(s: &mut String) -> usize {
    let n = s.len();
    s.clear();
    // Touch capacity-bytes by re-pushing NULs of the same length (best-effort ;
    // the original allocation may have already been freed by `clear()` if the
    // allocator chose to · but for the visible-buffer case this defends against
    // `Debug { inner }` style accidental-leaks.)
    for _ in 0..n {
        s.push('\0');
    }
    s.clear();
    n
}

// ══════════════════════════════════════════════════════════════════
// § AuditSink — pluggable audit-emit (every public-method calls-this)
// ══════════════════════════════════════════════════════════════════

/// § AuditEvent — emitted on-every public-method call (success AND-failure)
///
/// G1 : wire-into cssl-host-attestation OR-cssl-telemetry tracing-subscriber
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub method: &'static str,
    pub outcome: AuditOutcome,
    pub cap_required: u32,
    pub cap_granted: u32,
    pub idempotency_key: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditOutcome {
    Success,
    CapDenied,
    Failure,
}

/// § AuditSink — pluggable trait
///
/// stage-0 default `MockAuditSink` records-events into BTreeMap-of-Vec ;
/// G1 wires-real subscriber via `cssl-telemetry`.
pub trait AuditSink: Send + Sync {
    fn emit(&self, event: AuditEvent);
}

/// § MockAuditSink — test-helper · records all-events
#[derive(Debug, Default)]
pub struct MockAuditSink {
    events: std::sync::Mutex<Vec<AuditEvent>>,
}

impl MockAuditSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot all-events recorded so-far (test-only ; takes lock).
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Count events matching the given outcome.
    #[must_use]
    pub fn count_outcome(&self, outcome: &AuditOutcome) -> usize {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|ev| &ev.outcome == outcome)
            .count()
    }
}

impl AuditSink for MockAuditSink {
    fn emit(&self, event: AuditEvent) {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }
}

// ══════════════════════════════════════════════════════════════════
// § cap-check helper — used-by every-public-method
// ══════════════════════════════════════════════════════════════════

/// § cap_check — single shared cap-gate. Returns CapDenied + audit-emits
/// `AuditOutcome::CapDenied` if caller's `granted` lacks `required` bits.
pub fn cap_check(
    method: &'static str,
    required: u32,
    granted: u32,
    audit: &dyn AuditSink,
) -> StripeResult<()> {
    if granted & required == required {
        Ok(())
    } else {
        audit.emit(AuditEvent {
            method,
            outcome: AuditOutcome::CapDenied,
            cap_required: required,
            cap_granted: granted,
            idempotency_key: None,
            note: format!("cap-denied: required {required:#x} granted {granted:#x}"),
        });
        Err(StripeError::CapDenied { required, granted })
    }
}

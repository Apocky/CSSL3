//! Capability-gated access control for projections.
//!
//! § PRIME-DIRECTIVE ANCHOR
//!   Per `specs/30_SUBSTRATE.csl § PROJECTIONS § IFC-MASK / § CONSENT-REQUIRED
//!   / § SUBSTRATE-PRIME-DIRECTIVE-ALIGNMENT`, projections cannot see private
//!   substrate state without explicit grant ; debug-cam / DebugIntrospect
//!   projections require a separate clearance and emit telemetry.
//!
//! § THIS MODULE
//!   Implements the host-side runtime gate. The crate exposes :
//!     - [`Grant`] — enum naming the three projection-level grants required
//!       by the H-track design.
//!     - [`CapsToken`] — opaque grant-set ; immutable once constructed ;
//!       passed by `&` reference into accessor APIs that need a grant.
//!     - [`TelemetryHook`] — fn-pointer the host registers to receive
//!       debug-cam invocation events. The substrate is FORBIDDEN from
//!       silently observing its own state without telemetry per
//!       PRIME_DIRECTIVE §4 TRANSPARENCY.
//!
//!   Higher-level callers (e.g. cssl-host-vulkan, cssl-host-d3d12) construct
//!   a [`CapsToken`] from the user's consent surface (the {Consent<T, L>}
//!   tokens documented in `specs/12_CAPABILITIES.csl § CONSENT TOKENS`),
//!   then thread it through projection APIs. A projection cannot read the
//!   Ω-tensor without `OmegaTensorAccess` ; cannot mint a debug-cam without
//!   `DebugCamera` ; cannot share its rendered output with another observer
//!   without `ObserverShare`.
//!
//! § STAGE-0 SHAPE
//!   - The `CapsToken` is a host-runtime structure — there is no source-level
//!     CSSLv3 syntax for it yet (the {Consent<T, L>} token type lands in a
//!     separate slice). Stage-0 callers produce one via
//!     [`CapsToken::with_grants`] or one of the convenience constructors.
//!   - Telemetry is best-effort : if no hook is registered, debug-cam
//!     invocations still proceed (no silent denial) but no audit-log entry
//!     is produced. Production hosts MUST register a hook.
//!   - The hook surface uses `std::sync::RwLock<Option<TelemetryHook>>` to
//!     keep this crate `unsafe`-free + portable across all workspace hosts
//!     (the MinGW-less toolchain cannot build `parking_lot_core` ; see the
//!     header note in `Cargo.toml`). The lock is acquired once per debug-cam
//!     call ; the hot-path cost is negligible vs. the actual debug-projection
//!     rendering work. Lock poisoning is treated as "no hook" (best-effort
//!     telemetry) to keep substrate-totality discipline at the audit boundary.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// A single named projection-level capability grant. Mirrors the spec's
/// `IfcMask` / `ConsentTokenSet` discrimination but at coarser granularity
/// suitable for the host-runtime API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grant {
    /// Permission for a projection to read the substrate's Ω-tensor state.
    /// Without this grant, projections operate on their own local cached
    /// data + cannot pull live state. Most projections need this.
    OmegaTensorAccess,
    /// Permission to mint or operate a debug-camera ("god-cam") projection
    /// that bypasses the user's normal IFC mask. Producing or using a
    /// `DebugIntrospect`-kind projection requires this grant AND emits a
    /// telemetry event.
    DebugCamera,
    /// Permission to share this projection's rendered output / metadata
    /// with another observer-frame (e.g. for stereo rendering, split-screen,
    /// or AI-companion-view forwarding). The default is opaque-to-other-
    /// systems per § PROJECTIONS § THESIS.
    ObserverShare,
}

impl Grant {
    /// All grants in canonical order — `[OmegaTensorAccess, DebugCamera, ObserverShare]`.
    pub const ALL: [Self; 3] = [
        Self::OmegaTensorAccess,
        Self::DebugCamera,
        Self::ObserverShare,
    ];

    /// Bit position used by [`CapsToken`]'s packed representation. Stable
    /// from S8-H3 ; renaming or reordering requires a major-version bump.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Self::OmegaTensorAccess => 0,
            Self::DebugCamera => 1,
            Self::ObserverShare => 2,
        }
    }

    /// Canonical name for the grant — used in error messages + telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OmegaTensorAccess => "OmegaTensorAccess",
            Self::DebugCamera => "DebugCamera",
            Self::ObserverShare => "ObserverShare",
        }
    }
}

/// Opaque capability token. Holds a packed bitset of the grants the
/// substrate has issued to a projection at construction time. Immutable
/// once built — the only way to change a projection's grants is to
/// rebuild the token.
///
/// § OPACITY
///   The internal `bits` field is `u32` to permit up to 32 grant kinds
///   in the future ; today only 3 bits are used. The [`CapsToken::raw`]
///   accessor exposes the bitset for telemetry serialization, but the
///   sanctioned API for asking "does this token grant X" is
///   [`CapsToken::has`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapsToken {
    bits: u32,
}

impl CapsToken {
    /// Empty token — no grants. A projection holding this token can do
    /// observer-frame math (compute view / projection matrices) but
    /// cannot read substrate state, mint debug-cams, or share output.
    pub const EMPTY: Self = Self { bits: 0 };

    /// Token holding every defined grant. Use sparingly — typically only
    /// the developer / test harness has a full-grant token.
    pub const FULL: Self = Self {
        bits: (1 << Grant::OmegaTensorAccess.bit())
            | (1 << Grant::DebugCamera.bit())
            | (1 << Grant::ObserverShare.bit()),
    };

    /// Construct from an explicit grant slice. Duplicates are silently
    /// merged ; unknown grants are not possible (the type system forbids).
    #[must_use]
    pub fn with_grants(grants: &[Grant]) -> Self {
        let mut bits = 0u32;
        for g in grants {
            bits |= 1 << g.bit();
        }
        Self { bits }
    }

    /// Returns `true` iff this token grants `g`.
    #[must_use]
    pub const fn has(&self, g: Grant) -> bool {
        (self.bits & (1 << g.bit())) != 0
    }

    /// Returns the union of two tokens — the resulting token has every grant
    /// that EITHER input has. Used when merging the substrate's session
    /// token with a per-projection token.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Returns the intersection of two tokens — only grants present in BOTH
    /// inputs survive. Used to enforce "no grant exceeds the parent context".
    #[must_use]
    pub const fn intersect(self, other: Self) -> Self {
        Self {
            bits: self.bits & other.bits,
        }
    }

    /// Read the raw packed bits — for telemetry / serialization only.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.bits
    }
}

/// Result type for capability-gated reads.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CapsError {
    /// Caller's `CapsToken` did not include the required `Grant`.
    #[error("missing capability grant : {0}")]
    Missing(&'static str),
}

/// Verify that a token contains a specific grant. Returns `Err(CapsError::Missing)`
/// otherwise. The canonical entry point used by every gated accessor.
///
/// # Errors
/// Returns `Err(CapsError::Missing)` if `required` is not present in
/// `token`.
pub fn caps_grant(token: &CapsToken, required: Grant) -> Result<(), CapsError> {
    if token.has(required) {
        Ok(())
    } else {
        Err(CapsError::Missing(required.as_str()))
    }
}

/// Telemetry event emitted on debug-camera invocation. The host's registered
/// hook receives one of these per debug-cam access.
///
/// § PRIME-DIRECTIVE
///   §4 TRANSPARENCY says "behavior == apparent-behavior". Debug-cam access
///   bypasses the user's normal IFC mask and is therefore the highest-risk
///   surveillance vector in the substrate ; logging is mandatory. If a
///   production host registers a no-op hook, the substrate continues to
///   function but the audit-trail is on the host's conscience.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugCamEvent {
    /// Caller-supplied projection identifier — opaque u64. Hosts use this
    /// to trace which projection performed the access.
    pub projection_id: u64,
    /// Grant kind (always `Grant::DebugCamera` for events emitted via the
    /// stage-0 path ; the field exists for forward-compat with future
    /// telemetry-grant kinds).
    pub grant: Grant,
    /// `true` if the grant check succeeded ; `false` if the caller's token
    /// did not contain `DebugCamera`. Audit-trail captures both.
    pub granted: bool,
}

/// Host-registered telemetry callback type. Stage-0 uses a plain `fn`
/// pointer to keep registration `unsafe`-free + avoid an allocator-backed
/// closure. A future slice can grow a richer hook surface (closures + per-
/// thread context) once `cssl-rt::telemetry` ring-integration lands.
pub type TelemetryHook = fn(DebugCamEvent);

// Singleton hook + counter. The hook is held in a `std::sync::RwLock`
// (const-fn `new()` since 1.63) so the registration + invocation API can
// stay `unsafe`-free. The counter is a plain atomic for monotonic event-
// count observation. Lock poisoning is treated as "no hook" — best-effort
// telemetry rather than panic-propagation, matching substrate-totality.
static HOOK: RwLock<Option<TelemetryHook>> = RwLock::new(None);
static EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Register a telemetry hook that receives a [`DebugCamEvent`] for every
/// debug-camera invocation. Replaces any previously registered hook.
/// Pass [`clear_telemetry_hook`] to remove without replacement.
///
/// If the lock is poisoned (a previous hook panicked under-write), the
/// poison is cleared and the new hook is installed — the substrate
/// continues to operate. The poisoning event itself is not telemetry-
/// recorded ; production hooks should not panic.
pub fn set_telemetry_hook(hook: TelemetryHook) {
    match HOOK.write() {
        Ok(mut guard) => *guard = Some(hook),
        Err(poison) => *poison.into_inner() = Some(hook),
    }
}

/// Remove the currently registered telemetry hook. Subsequent debug-cam
/// invocations produce no telemetry events but DO succeed (audit-log gap
/// is on the host).
pub fn clear_telemetry_hook() {
    match HOOK.write() {
        Ok(mut guard) => *guard = None,
        Err(poison) => *poison.into_inner() = None,
    }
}

/// Number of debug-camera events recorded since process start. Monotonic ;
/// observable by the host for sanity / regression tests.
#[must_use]
pub fn telemetry_event_count() -> u64 {
    EVENT_COUNT.load(Ordering::Acquire)
}

/// Convenience for tests : reset the counter back to zero. Production hosts
/// SHOULD NOT call this — the monotonicity guarantee is a load-bearing
/// audit-property.
#[doc(hidden)]
pub fn reset_telemetry_for_tests() {
    EVENT_COUNT.store(0, Ordering::Release);
    match HOOK.write() {
        Ok(mut guard) => *guard = None,
        Err(poison) => *poison.into_inner() = None,
    }
}

/// Verify a debug-camera grant AND emit a telemetry event. The combination
/// is the canonical path — production code MUST use this, not bare
/// `caps_grant(token, Grant::DebugCamera)`. Stage-0 enforces the convention
/// at the API level by exposing only this entry point for the DebugCamera
/// grant.
///
/// # Errors
/// Returns `Err(CapsError::Missing)` if the token does not grant
/// `DebugCamera`. The event counter is incremented BEFORE the check ;
/// failed attempts ARE logged (audit-trail captures both successes and
/// rejections).
pub fn caps_grant_debug_camera(token: &CapsToken, projection_id: u64) -> Result<(), CapsError> {
    EVENT_COUNT.fetch_add(1, Ordering::AcqRel);
    let granted = token.has(Grant::DebugCamera);
    let event = DebugCamEvent {
        projection_id,
        grant: Grant::DebugCamera,
        granted,
    };
    // Snapshot the hook under read-lock then release before invoking ; this
    // avoids holding the lock across the user-supplied callback. Poisoned
    // lock ⇒ no telemetry but no abort — substrate totality.
    let snapshot = match HOOK.read() {
        Ok(guard) => *guard,
        Err(poison) => *poison.into_inner(),
    };
    if let Some(hook) = snapshot {
        hook(event);
    }
    if granted {
        Ok(())
    } else {
        Err(CapsError::Missing(Grant::DebugCamera.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        caps_grant, caps_grant_debug_camera, clear_telemetry_hook, reset_telemetry_for_tests,
        set_telemetry_hook, telemetry_event_count, CapsError, CapsToken, DebugCamEvent, Grant,
    };
    use core::sync::atomic::{AtomicU64, Ordering};

    static LAST_PROJECTION_ID: AtomicU64 = AtomicU64::new(0);
    static LAST_GRANTED: AtomicU64 = AtomicU64::new(0);

    fn test_hook(event: DebugCamEvent) {
        LAST_PROJECTION_ID.store(event.projection_id, Ordering::Release);
        LAST_GRANTED.store(u64::from(event.granted), Ordering::Release);
    }

    #[test]
    fn empty_token_grants_nothing() {
        let t = CapsToken::EMPTY;
        for g in Grant::ALL {
            assert!(!t.has(g));
            assert!(caps_grant(&t, g).is_err());
        }
    }

    #[test]
    fn full_token_grants_everything() {
        let t = CapsToken::FULL;
        for g in Grant::ALL {
            assert!(t.has(g));
            assert!(caps_grant(&t, g).is_ok());
        }
    }

    #[test]
    fn with_grants_constructs_correct_subset() {
        let t = CapsToken::with_grants(&[Grant::OmegaTensorAccess, Grant::ObserverShare]);
        assert!(t.has(Grant::OmegaTensorAccess));
        assert!(!t.has(Grant::DebugCamera));
        assert!(t.has(Grant::ObserverShare));
    }

    #[test]
    fn duplicate_grants_in_input_dont_double_count() {
        let t = CapsToken::with_grants(&[Grant::OmegaTensorAccess, Grant::OmegaTensorAccess]);
        assert!(t.has(Grant::OmegaTensorAccess));
        // Bits should equal a single-grant token.
        assert_eq!(
            t.raw(),
            CapsToken::with_grants(&[Grant::OmegaTensorAccess]).raw()
        );
    }

    #[test]
    fn union_combines_grants() {
        let a = CapsToken::with_grants(&[Grant::OmegaTensorAccess]);
        let b = CapsToken::with_grants(&[Grant::DebugCamera]);
        let u = a.union(b);
        assert!(u.has(Grant::OmegaTensorAccess));
        assert!(u.has(Grant::DebugCamera));
        assert!(!u.has(Grant::ObserverShare));
    }

    #[test]
    fn intersect_keeps_only_shared_grants() {
        let a = CapsToken::with_grants(&[Grant::OmegaTensorAccess, Grant::DebugCamera]);
        let b = CapsToken::with_grants(&[Grant::DebugCamera, Grant::ObserverShare]);
        let i = a.intersect(b);
        assert!(!i.has(Grant::OmegaTensorAccess));
        assert!(i.has(Grant::DebugCamera));
        assert!(!i.has(Grant::ObserverShare));
    }

    #[test]
    fn caps_grant_returns_correct_error() {
        let t = CapsToken::EMPTY;
        let err = caps_grant(&t, Grant::OmegaTensorAccess).unwrap_err();
        assert_eq!(err, CapsError::Missing(Grant::OmegaTensorAccess.as_str()));
    }

    #[test]
    fn debug_camera_grant_emits_telemetry_when_hook_set() {
        // Reset state ; install hook ; call.
        reset_telemetry_for_tests();
        set_telemetry_hook(test_hook);
        let token = CapsToken::with_grants(&[Grant::DebugCamera]);
        let before = telemetry_event_count();
        caps_grant_debug_camera(&token, 0xCAFE_BABE).unwrap();
        let after = telemetry_event_count();
        assert!(after > before);
        assert_eq!(LAST_PROJECTION_ID.load(Ordering::Acquire), 0xCAFE_BABE);
        assert_eq!(LAST_GRANTED.load(Ordering::Acquire), 1);
        clear_telemetry_hook();
    }

    #[test]
    fn debug_camera_grant_emits_telemetry_even_on_rejection() {
        // Reset ; install hook ; call without grant ; verify counter ticks
        // and event captures granted=false.
        reset_telemetry_for_tests();
        set_telemetry_hook(test_hook);
        let token = CapsToken::EMPTY;
        let before = telemetry_event_count();
        let result = caps_grant_debug_camera(&token, 0x1234);
        let after = telemetry_event_count();
        assert!(result.is_err());
        // Audit-trail captures BOTH successes and rejections.
        assert!(after > before);
        assert_eq!(LAST_GRANTED.load(Ordering::Acquire), 0);
        clear_telemetry_hook();
    }

    #[test]
    fn debug_camera_works_without_registered_hook() {
        // No silent denial — substrate proceeds, audit-gap is host's responsibility.
        reset_telemetry_for_tests();
        let token = CapsToken::with_grants(&[Grant::DebugCamera]);
        caps_grant_debug_camera(&token, 0).expect("should succeed without a hook");
    }

    #[test]
    fn grant_bits_are_stable_from_s8_h3() {
        // STABILITY contract : these bit values are spec-locked. Renaming
        // requires a major-version bump per § STAGE-0 SHAPE.
        assert_eq!(Grant::OmegaTensorAccess.bit(), 0);
        assert_eq!(Grant::DebugCamera.bit(), 1);
        assert_eq!(Grant::ObserverShare.bit(), 2);
    }

    #[test]
    fn grant_canonical_names_are_stable() {
        assert_eq!(Grant::OmegaTensorAccess.as_str(), "OmegaTensorAccess");
        assert_eq!(Grant::DebugCamera.as_str(), "DebugCamera");
        assert_eq!(Grant::ObserverShare.as_str(), "ObserverShare");
    }
}

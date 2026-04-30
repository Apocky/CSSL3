//! Host-FFI capability witnesses — `Cap<Window>` / `Cap<Gpu>` / `Cap<Input>` /
//! `Cap<Audio>` / `Cap<Thread>` / `Cap<Time>` / `Cap<XR>`.
//!
//! § SPEC : `specs/24_HOST_FFI.csl` § CAPABILITY-MATRIX +
//!          `specs/12_CAPABILITIES.csl` § ISO-OWNERSHIP.
//!
//! § WAVE : Wave-D7 (host-FFI surface).
//!
//! § THESIS
//!   Every `__cssl_*` host-FFI symbol consumes a `Cap<Kind>` witness on the
//!   CSSL side. The `Cap<T>` is a sealed newtype with a private `()` field +
//!   a `PhantomData<T>` marker — outside this module + outside the
//!   per-kind `for_test()` constructor it cannot be materialised. This
//!   mirrors the `cssl-mcp-server::cap` pattern (D164 inspired) so that
//!   D131 (`cssl-substrate-prime-directive`) can later swap each
//!   constructor body for a signed-token issuance call without touching
//!   any caller.
//!
//! § PRIME-DIRECTIVE
//!   - `Cap<Input>`, `Cap<Audio>`, `Cap<XR>` are all default-DENIED. The only
//!     materialisation path is `for_test()` (cfg-gated) until D131 ships.
//!   - `Cap<Window>`, `Cap<Gpu>`, `Cap<Thread>`, `Cap<Time>` mirror the
//!     same discipline (no production constructor here ; awaiting D131).
//!   - The witness layout is bit-packed (`HostCapKind` = `u8` + `issued_at`
//!     = `u64`) → `HostCapWitness` is exactly 16 bytes aligned.
//!
//! § INTEGRATION-NOTE @ end-of-file describes the one-line `pub mod`
//!   declaration to add to `cssl-caps::lib.rs` once Wave-D7's stitch-up
//!   merge fires.

use core::marker::PhantomData;

// § SEALED MARKER TRAIT ≡ closed extension-set @ this-module
mod sealed {
    pub trait Sealed {}
}

/// Marker trait for host-FFI capability kinds. Sealed in this module so no
/// downstream crate can introduce new host-cap-kinds without going through
/// D131 (`cssl-substrate-prime-directive`).
pub trait HostCapMarker: sealed::Sealed {
    /// Stable string identifying the kind ; appears in audit-events.
    const NAME: &'static str;
    /// Default-grant policy. ALL host-caps are default-DENIED per
    /// `specs/24` § CAPABILITY-MATRIX.
    const DEFAULT_GRANTED: bool = false;
    /// Discriminant projection.
    fn kind() -> HostCapKind;
}

// ════ Marker types — one per host-FFI domain ════════════════════════════════

/// `Cap<Window>` marker. Granted-by Apocky-PM signed-token OR `--dev-mode`.
/// `specs/24` § CAPABILITY-MATRIX : ¬biometric · ¬surveillance · close-
/// disposition required.
#[derive(Debug)]
pub struct Window;

/// `Cap<Gpu>` marker. Granted-by Apocky-PM OR `--dev-mode`.
/// `specs/24` § CAPABILITY-MATRIX : ¬compute-secret-extract · ¬cross-
/// process-resource-share.
#[derive(Debug)]
pub struct Gpu;

/// `Cap<Input>` marker. Granted-by Apocky-PM signed-token.
/// `specs/24` § CAPABILITY-MATRIX : mouse-delta = `Sensitive<Behavioral>` ·
/// keyboard-events = `Sensitive<Behavioral>` if-non-game-key · NEVER egresses.
#[derive(Debug)]
pub struct Input;

/// `Cap<Audio>` marker. Granted-by Apocky-PM ; microphone =
/// `Sensitive<Voice>` · default-DENIED.
#[derive(Debug)]
pub struct Audio;

/// `Cap<Thread>` marker. Granted-by Apocky-PM (default for engine-internal).
#[derive(Debug)]
pub struct Thread;

/// `Cap<Time>` marker. Granted-by Apocky-PM ; carries `NonDeterministic`
/// effect-row implication.
#[derive(Debug)]
pub struct Time;

/// `Cap<XR>` marker. Granted-by Apocky-PM signed-token ; pose =
/// `Sensitive<Spatial>` · NEVER egresses.
#[derive(Debug)]
pub struct XR;

impl sealed::Sealed for Window {}
impl sealed::Sealed for Gpu {}
impl sealed::Sealed for Input {}
impl sealed::Sealed for Audio {}
impl sealed::Sealed for Thread {}
impl sealed::Sealed for Time {}
impl sealed::Sealed for XR {}

// ════ HostCapKind discriminant — packed u8 ══════════════════════════════════

/// Discriminant for the seven host-FFI capability kinds modelled in this
/// slice. Variant order is part of the stable surface ; new variants are
/// append-only per `specs/24` § ABI-STABLE-SYMBOLS.
///
/// Repr `u8` keeps `HostCapWitness` to 16 bytes (1+7-pad+8) under
/// natural alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum HostCapKind {
    /// `Cap<Window>` discriminant.
    Window = 0,
    /// `Cap<Gpu>` discriminant.
    Gpu = 1,
    /// `Cap<Input>` discriminant.
    Input = 2,
    /// `Cap<Audio>` discriminant.
    Audio = 3,
    /// `Cap<Thread>` discriminant.
    Thread = 4,
    /// `Cap<Time>` discriminant.
    Time = 5,
    /// `Cap<XR>` discriminant.
    XR = 6,
}

impl HostCapKind {
    /// All seven host-cap-kinds in canonical order, for table-driven walks.
    pub const ALL: [Self; 7] = [
        Self::Window,
        Self::Gpu,
        Self::Input,
        Self::Audio,
        Self::Thread,
        Self::Time,
        Self::XR,
    ];

    /// Stable string identifier — used in audit-events + error messages +
    /// the `__cssl_<domain>_*` symbol-name root per `specs/24` § P1.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Window => "Window",
            Self::Gpu => "Gpu",
            Self::Input => "Input",
            Self::Audio => "Audio",
            Self::Thread => "Thread",
            Self::Time => "Time",
            Self::XR => "XR",
        }
    }

    /// `true` iff this cap-kind admits Sensitive-labeled data flow per
    /// `specs/24` § CAPABILITY-MATRIX. Used by the runtime-emit hook
    /// `verify_cap` to short-circuit IFC-cross-checks for purely-Public
    /// surfaces (Time, Thread).
    #[must_use]
    pub const fn touches_sensitive(self) -> bool {
        matches!(
            self,
            Self::Window | Self::Gpu | Self::Input | Self::Audio | Self::XR,
        )
    }

    /// `true` iff this cap-kind is *default-DENIED* per
    /// `specs/24` § CAPABILITY-MATRIX. Currently every host-cap is
    /// default-DENIED ; the predicate exists for forward-compat.
    #[must_use]
    pub const fn is_default_denied(self) -> bool {
        // ∀ host-cap : default-DENIED per spec.
        true
    }
}

impl HostCapMarker for Window {
    const NAME: &'static str = "Window";
    fn kind() -> HostCapKind {
        HostCapKind::Window
    }
}

impl HostCapMarker for Gpu {
    const NAME: &'static str = "Gpu";
    fn kind() -> HostCapKind {
        HostCapKind::Gpu
    }
}

impl HostCapMarker for Input {
    const NAME: &'static str = "Input";
    fn kind() -> HostCapKind {
        HostCapKind::Input
    }
}

impl HostCapMarker for Audio {
    const NAME: &'static str = "Audio";
    fn kind() -> HostCapKind {
        HostCapKind::Audio
    }
}

impl HostCapMarker for Thread {
    const NAME: &'static str = "Thread";
    fn kind() -> HostCapKind {
        HostCapKind::Thread
    }
}

impl HostCapMarker for Time {
    const NAME: &'static str = "Time";
    fn kind() -> HostCapKind {
        HostCapKind::Time
    }
}

impl HostCapMarker for XR {
    const NAME: &'static str = "XR";
    fn kind() -> HostCapKind {
        HostCapKind::XR
    }
}

// ════ Cap<T> sealed newtype ═════════════════════════════════════════════════

/// Host-FFI capability token. The private `()` field prevents direct
/// construction outside this module ; constructors (currently only
/// `for_test()`) enforce the consent boundary per kind.
///
/// `Cap<T>` is non-`Copy` + non-`Clone` so it cannot be used twice ; passing
/// it to a host-FFI shim consumes it, and the receiver typically materialises
/// a [`HostCapWitness`] for long-lived proof.
#[derive(Debug)]
pub struct Cap<T: HostCapMarker> {
    _seal: (),
    _marker: PhantomData<T>,
}

impl<T: HostCapMarker> Cap<T> {
    /// Internal constructor — only callable inside this module. Per the
    /// sealed-newtype pattern, this is the *only* construction path ;
    /// outside callers use `for_test()` (under `#[cfg(test)]` or the
    /// `test-bypass` feature).
    ///
    /// `#[allow(dead_code)]` is justified : in the non-test, non-`test-
    /// bypass` build configuration the only caller of `new()` is
    /// `for_test()` which is itself cfg-gated away. Once D131 ships its
    /// production `issue_host_cap` constructor, `new()` becomes the
    /// only callable from that swap-point, and the lint goes away.
    #[allow(dead_code)]
    const fn new() -> Self {
        Self {
            _seal: (),
            _marker: PhantomData,
        }
    }

    /// Project the cap-kind discriminant. Useful for audit-events.
    #[must_use]
    pub fn kind(&self) -> HostCapKind {
        T::kind()
    }

    /// Consume the cap and produce a [`HostCapWitness`] tagged with the
    /// supplied issuance counter. Used at host-FFI shim entry-time.
    ///
    /// § Sawyer-efficiency : witness is exactly 16 bytes (kind:u8 +
    /// 7 pad + issued_at:u64) ; trivially `Copy`.
    #[must_use]
    pub fn into_witness(self, issued_at: u64) -> HostCapWitness {
        HostCapWitness {
            kind: T::kind(),
            issued_at,
        }
    }

    /// Test-fixture construction. Bounded to `#[cfg(test)]` so production
    /// callers cannot reach this path. Default-DENIED in production ; real
    /// grants land via D131 (`cssl-substrate-prime-directive::authority::
    /// issue_cap`) once that surface stabilises.
    ///
    /// § Wave-D7 stage : `#[cfg(test)]` only — the `test-bypass` feature
    /// described in the INTEGRATION-NOTE is added in the stitch-up commit
    /// alongside the `pub mod host_caps;` line. Until then this fixture
    /// is reachable only by THIS module's own tests.
    ///
    /// ## SWAP-POINT (D131 integration)
    /// Replace the body with a call into
    /// `cssl_substrate_prime_directive::authority::issue_cap::<T>(token)`.
    #[cfg(test)]
    #[must_use]
    pub fn for_test() -> Self {
        Self::new()
    }
}

// ════ HostCapWitness ════════════════════════════════════════════════════════

/// Long-lived proof that a host-FFI cap was issued. Cheap to copy + compare.
///
/// Layout (Sawyer-efficiency) :
/// ```text
///   field      bytes   note
///   kind       1       HostCapKind, repr(u8)
///   <padding>  7       align-up for u64
///   issued_at  8       audit-chain seq#
///   ────────  ───
///   total      16      cache-line-friendly
/// ```
///
/// Implementations of `__cssl_*` shims persist this in their per-thread
/// cap-set + cross-reference it on every FFI symbol invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct HostCapWitness {
    /// Which cap-kind this witness proves.
    pub kind: HostCapKind,
    /// Audit-chain sequence number @ issuance. Used for replay-validation.
    pub issued_at: u64,
}

impl HostCapWitness {
    /// `true` iff this witness covers the given cap-kind.
    ///
    /// § soundness : witnesses are kind-specific ; cross-kind aliasing is
    /// rejected. Compare-by-discriminant is constant-time + side-channel-
    /// free per `specs/11` § COVERT-CHANNEL MITIGATIONS.
    #[must_use]
    pub const fn covers(&self, kind: HostCapKind) -> bool {
        // Manually expand the match for `const fn` compatibility ; an
        // `==` over `HostCapKind` is not yet `const fn` on stable.
        matches!(
            (self.kind, kind),
            (HostCapKind::Window, HostCapKind::Window)
                | (HostCapKind::Gpu, HostCapKind::Gpu)
                | (HostCapKind::Input, HostCapKind::Input)
                | (HostCapKind::Audio, HostCapKind::Audio)
                | (HostCapKind::Thread, HostCapKind::Thread)
                | (HostCapKind::Time, HostCapKind::Time)
                | (HostCapKind::XR, HostCapKind::XR),
        )
    }
}

// ════ Runtime-emit hooks ════════════════════════════════════════════════════

/// Reasons a cap-verification fails. Used by `verify_cap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapDenied {
    /// Witness's `kind` does not match the requested `expected` kind.
    KindMismatch {
        /// The kind the witness proves.
        witness_kind: HostCapKind,
        /// The kind the call-site requires.
        expected: HostCapKind,
    },
    /// Witness's `issued_at` is older than the audit-chain's current
    /// minimum (revocation occurred since issuance).
    Revoked {
        /// The witness's stale `issued_at`.
        witness_issued_at: u64,
        /// The currently-required minimum `issued_at`.
        required_minimum: u64,
    },
}

/// Runtime-emit hook called by every `__cssl_<domain>_*` shim before its
/// body executes. Verifies that the supplied [`HostCapWitness`] covers the
/// `expected` cap-kind AND has not been revoked since issuance.
///
/// § SPEC : `specs/24` § P3 cap-gated.
/// § PRIME-DIRECTIVE : revocation is non-overridable per `specs/11`
///   § PRIME-DIRECTIVE ENCODING.
///
/// # Errors
/// Returns [`CapDenied`] if the witness does not cover the expected kind
/// or has been revoked.
pub fn verify_cap(
    witness: &HostCapWitness,
    expected: HostCapKind,
    revocation_floor: u64,
) -> Result<(), CapDenied> {
    if !witness.covers(expected) {
        return Err(CapDenied::KindMismatch {
            witness_kind: witness.kind,
            expected,
        });
    }
    if witness.issued_at < revocation_floor {
        return Err(CapDenied::Revoked {
            witness_issued_at: witness.issued_at,
            required_minimum: revocation_floor,
        });
    }
    Ok(())
}

// ════ Tests ═════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    // 1) layout-tripwire : witness must remain 16 bytes
    #[test]
    fn host_cap_witness_is_sixteen_bytes() {
        assert_eq!(size_of::<HostCapWitness>(), 16);
    }

    // 2) HostCapKind has 7 variants
    #[test]
    fn host_cap_kind_has_seven_variants() {
        assert_eq!(HostCapKind::ALL.len(), 7);
    }

    // 3) every variant has a stable as_str
    #[test]
    fn host_cap_kind_as_str_table() {
        assert_eq!(HostCapKind::Window.as_str(), "Window");
        assert_eq!(HostCapKind::Gpu.as_str(), "Gpu");
        assert_eq!(HostCapKind::Input.as_str(), "Input");
        assert_eq!(HostCapKind::Audio.as_str(), "Audio");
        assert_eq!(HostCapKind::Thread.as_str(), "Thread");
        assert_eq!(HostCapKind::Time.as_str(), "Time");
        assert_eq!(HostCapKind::XR.as_str(), "XR");
    }

    // 4) Cap<X> is non-Copy / non-Clone — compile-time : we assert via
    //    presence of a static-fn with `T: !Copy + !Clone` (we can't write
    //    that bound directly, so we approximate via trait-object size).
    //    Concrete check : if Cap<T> were Copy, this `fn` would not need
    //    `&` ; we exercise the move-discipline at runtime to confirm.
    #[test]
    fn cap_is_consumed_on_into_witness() {
        let cap = Cap::<Window>::for_test();
        let _ = cap.into_witness(0);
        // `cap` is now moved ; using `cap` again would be a compile-error.
        // We can't write the violating line in a passing-test, so the
        // discipline is enforced structurally by the type-system.
    }

    // 5) for_test() is bounded — only reachable under #[cfg(test)] or
    //    the `test-bypass` feature ; this very test demonstrates that.
    #[test]
    fn cap_for_test_constructs_each_kind() {
        let _ = Cap::<Window>::for_test();
        let _ = Cap::<Gpu>::for_test();
        let _ = Cap::<Input>::for_test();
        let _ = Cap::<Audio>::for_test();
        let _ = Cap::<Thread>::for_test();
        let _ = Cap::<Time>::for_test();
        let _ = Cap::<XR>::for_test();
    }

    // 6) into_witness round-trip per cap : every kind round-trips
    #[test]
    fn into_witness_roundtrip_every_kind() {
        let cases: [(HostCapKind, fn() -> HostCapWitness); 7] = [
            (HostCapKind::Window, || {
                Cap::<Window>::for_test().into_witness(1)
            }),
            (HostCapKind::Gpu, || Cap::<Gpu>::for_test().into_witness(2)),
            (HostCapKind::Input, || {
                Cap::<Input>::for_test().into_witness(3)
            }),
            (HostCapKind::Audio, || {
                Cap::<Audio>::for_test().into_witness(4)
            }),
            (HostCapKind::Thread, || {
                Cap::<Thread>::for_test().into_witness(5)
            }),
            (HostCapKind::Time, || {
                Cap::<Time>::for_test().into_witness(6)
            }),
            (HostCapKind::XR, || Cap::<XR>::for_test().into_witness(7)),
        ];
        for (i, (expected_kind, mk)) in cases.iter().enumerate() {
            let w = mk();
            assert_eq!(w.kind, *expected_kind, "case #{i}");
            assert_eq!(w.issued_at as usize, i + 1, "case #{i}");
        }
    }

    // 7) HostCapWitness::covers — same-kind hits, cross-kind misses
    #[test]
    fn witness_covers_same_kind_only() {
        for k in HostCapKind::ALL {
            let w = HostCapWitness {
                kind: k,
                issued_at: 0,
            };
            assert!(w.covers(k), "{k:?} should cover itself");
            for other in HostCapKind::ALL {
                if other != k {
                    assert!(
                        !w.covers(other),
                        "{k:?} witness should not cover {other:?}"
                    );
                }
            }
        }
    }

    // 8) verify_cap : kind-match + non-revoked = Ok
    #[test]
    fn verify_cap_ok_path() {
        let w = Cap::<Input>::for_test().into_witness(100);
        assert_eq!(verify_cap(&w, HostCapKind::Input, 100), Ok(()));
    }

    // 9) verify_cap : kind-mismatch = KindMismatch
    #[test]
    fn verify_cap_kind_mismatch() {
        let w = Cap::<Window>::for_test().into_witness(0);
        let r = verify_cap(&w, HostCapKind::Audio, 0);
        assert_eq!(
            r,
            Err(CapDenied::KindMismatch {
                witness_kind: HostCapKind::Window,
                expected: HostCapKind::Audio,
            })
        );
    }

    // 10) verify_cap : stale-witness = Revoked
    #[test]
    fn verify_cap_revoked_when_issued_before_floor() {
        let w = Cap::<Time>::for_test().into_witness(5);
        let r = verify_cap(&w, HostCapKind::Time, 100);
        assert_eq!(
            r,
            Err(CapDenied::Revoked {
                witness_issued_at: 5,
                required_minimum: 100,
            })
        );
    }

    // 11) Sensitive-touch predicate : Window/Gpu/Input/Audio/XR = true ;
    //     Thread/Time = false (Public)
    #[test]
    fn touches_sensitive_partition() {
        for k in [
            HostCapKind::Window,
            HostCapKind::Gpu,
            HostCapKind::Input,
            HostCapKind::Audio,
            HostCapKind::XR,
        ] {
            assert!(k.touches_sensitive(), "{k:?} touches sensitive");
        }
        for k in [HostCapKind::Thread, HostCapKind::Time] {
            assert!(
                !k.touches_sensitive(),
                "{k:?} is Public per specs/24"
            );
        }
    }

    // 12) every kind is default-DENIED per spec
    #[test]
    fn every_host_cap_is_default_denied() {
        for k in HostCapKind::ALL {
            assert!(k.is_default_denied(), "{k:?} should be default-DENIED");
        }
    }

    // 13) marker NAME matches kind().as_str()
    #[test]
    fn marker_name_matches_kind_as_str() {
        assert_eq!(Window::NAME, HostCapKind::Window.as_str());
        assert_eq!(Gpu::NAME, HostCapKind::Gpu.as_str());
        assert_eq!(Input::NAME, HostCapKind::Input.as_str());
        assert_eq!(Audio::NAME, HostCapKind::Audio.as_str());
        assert_eq!(Thread::NAME, HostCapKind::Thread.as_str());
        assert_eq!(Time::NAME, HostCapKind::Time.as_str());
        assert_eq!(XR::NAME, HostCapKind::XR.as_str());
    }

    // 14) trait DEFAULT_GRANTED == false for every marker
    #[test]
    fn trait_default_granted_is_false() {
        // Lift each marker into its trait, then assert the const.
        // (At trait-level we can't iterate over types ; one assertion
        // per marker keeps the regression-tripwire explicit.)
        assert!(!Window::DEFAULT_GRANTED);
        assert!(!Gpu::DEFAULT_GRANTED);
        assert!(!Input::DEFAULT_GRANTED);
        assert!(!Audio::DEFAULT_GRANTED);
        assert!(!Thread::DEFAULT_GRANTED);
        assert!(!Time::DEFAULT_GRANTED);
        assert!(!XR::DEFAULT_GRANTED);
    }

    // 15) Cap<T>::kind() projects to the same HostCapKind as the marker
    #[test]
    fn cap_kind_projection_matches_marker() {
        assert_eq!(Cap::<Window>::for_test().kind(), HostCapKind::Window);
        assert_eq!(Cap::<Gpu>::for_test().kind(), HostCapKind::Gpu);
        assert_eq!(Cap::<Input>::for_test().kind(), HostCapKind::Input);
        assert_eq!(Cap::<Audio>::for_test().kind(), HostCapKind::Audio);
        assert_eq!(Cap::<Thread>::for_test().kind(), HostCapKind::Thread);
        assert_eq!(Cap::<Time>::for_test().kind(), HostCapKind::Time);
        assert_eq!(Cap::<XR>::for_test().kind(), HostCapKind::XR);
    }

    // 16) HostCapKind variant ordering matches ABI-stable assignment
    //     (variant repr-u8 values 0..6 are part of the stable surface).
    #[test]
    fn host_cap_kind_repr_u8_stable() {
        assert_eq!(HostCapKind::Window as u8, 0);
        assert_eq!(HostCapKind::Gpu as u8, 1);
        assert_eq!(HostCapKind::Input as u8, 2);
        assert_eq!(HostCapKind::Audio as u8, 3);
        assert_eq!(HostCapKind::Thread as u8, 4);
        assert_eq!(HostCapKind::Time as u8, 5);
        assert_eq!(HostCapKind::XR as u8, 6);
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § INTEGRATION-NOTE  (Wave-D7 stitch-up — separate commit, do NOT do here)
// ════════════════════════════════════════════════════════════════════════════
//
// To activate this module, add ONE LINE to `cssl-caps/src/lib.rs` :
//
//     pub mod host_caps;
//
// And optionally re-export the public surface :
//
//     pub use host_caps::{
//         Cap as HostCap,
//         CapDenied,
//         HostCapKind,
//         HostCapMarker,
//         HostCapWitness,
//         verify_cap,
//         // marker types :
//         Window, Gpu, Input, Audio, Thread, Time, XR,
//     };
//
// `Cargo.toml` requires an OPTIONAL feature `test-bypass` to allow
// `for_test()` outside `#[cfg(test)]` :
//
//     [features]
//     test-bypass = []
//
// When D131 (`cssl-substrate-prime-directive`) stabilises, replace the
// body of `Cap::<T>::for_test()` with :
//
//     cssl_substrate_prime_directive::authority::issue_host_cap::<T>(
//         signed_token,
//     )
//
// per the SWAP-POINT comment on `for_test()`.
//
// Downstream `host_*` modules (Wave-D1..D6/D8 in `cssl-rt`) call
// `verify_cap(&witness, expected, revocation_floor)` at the top of every
// `__cssl_<domain>_*` shim ; on `Err(CapDenied::*)` the shim returns the
// negative-errno `-EPERM` immediately AND emits a `{Audit<*>}` event
// per `specs/24` § P3.
//
// ∎ host-caps

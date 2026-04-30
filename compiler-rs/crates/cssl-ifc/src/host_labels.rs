//! Host-FFI IFC labels — `Sensitive<Behavioral>` / `Sensitive<Voice>` /
//! `Sensitive<Spatial>` / `Sensitive<NetData>` / `Sensitive<Frame>`.
//!
//! § SPEC : `specs/24_HOST_FFI.csl` § IFC-LABELS +
//!          `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING.
//!
//! § WAVE : Wave-D7 (host-FFI surface).
//!
//! § THESIS
//!   Every byte that crosses a `__cssl_*` host-FFI shim carries an IFC
//!   label per `specs/11` § TYPE-LEVEL LABELS. The five host-FFI label
//!   tags are bit-packed into a single `u8` — 4 bits of tag (16 possible
//!   tags ; 5 used currently + room to grow) + 4 bits of scope ( device-
//!   local / process-local / cross-process / network ). String-allocations
//!   are forbidden in this surface per Sawyer-efficiency.
//!
//! § PRIME-DIRECTIVE
//!   - `Sensitive<Behavioral>` / `Sensitive<Voice>` / `Sensitive<Spatial>`
//!     all REFUSE cross-process + network scope at the `verify_label`
//!     hook. There is no Privilege escape-hatch — these are absolute
//!     bans per `specs/11` § PRIME-DIRECTIVE ENCODING.
//!   - `Sensitive<NetData>` is admissible cross-process + network *only*
//!     when the value's confidentiality-set permits ; `verify_label`
//!     defers to the existing `Label::flows_to` lattice-check.
//!   - `Sensitive<Frame>` (XR-headset render-output) is REFUSED at any
//!     scope > device-local ; egress would reveal body-pose per
//!     `specs/24` § IFC-LABELS.
//!
//! § COMPATIBILITY WITH cssl-ifc::SensitiveDomain
//!   This module's [`HostSensitiveTag`] is the *host-FFI* projection of
//!   the IFC `SensitiveDomain` enum. The mapping is total :
//!     `Behavioral` → `SensitiveDomain::Privacy` family + `Behavioral`
//!     `Voice`      → `SensitiveDomain::Voice` (TBD in W3β-08)
//!     `Spatial`    → `SensitiveDomain::Body` (biometric-family)
//!     `NetData`    → routed through `cssl-rt::net` cap-bitset
//!     `Frame`      → `SensitiveDomain::Body` (biometric-family ; render-out)
//!   The cross-walk is structural ; both layers reject the same set of
//!   compositions, so the order of W3β-04 vs W3β-07 integration is
//!   irrelevant.

use core::fmt;

// ════ Tag enum — packed u8 ══════════════════════════════════════════════════

/// Sensitive-tag for host-FFI labels. Matches `specs/24` § IFC-LABELS.
///
/// `repr(u8)` ; only 4 bits used so the upper nibble is reserved for
/// future tags. Five tags currently allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum HostSensitiveTag {
    /// `Sensitive<Behavioral>` : mouse-deltas + keyboard-events +
    /// XR-controller-pose. NEVER egresses cross-process per
    /// `specs/24` § IFC-LABELS.
    Behavioral = 0x1,
    /// `Sensitive<Voice>` : microphone stream. Default-DENIED capability ;
    /// NEVER egresses without explicit `Cap<Audio>` consent.
    Voice = 0x2,
    /// `Sensitive<Spatial>` : XR head-pose. NEVER egresses ;
    /// reveals body-pose ⇒ biometric-family per `specs/11`.
    Spatial = 0x3,
    /// `Sensitive<NetData>` : per Wave-C4 `cssl-rt::net` cap-bitset.
    /// Admissible cross-process when confidentiality-set permits.
    NetData = 0x4,
    /// `Sensitive<Frame>` : XR-headset render-output. NEVER egresses ;
    /// frame-content reveals body-pose via reverse-rendering.
    Frame = 0x5,
}

impl HostSensitiveTag {
    /// All five host-FFI sensitive-tags in canonical order.
    pub const ALL: [Self; 5] = [
        Self::Behavioral,
        Self::Voice,
        Self::Spatial,
        Self::NetData,
        Self::Frame,
    ];

    /// Stable string identifier used in audit-events + diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Behavioral => "Behavioral",
            Self::Voice => "Voice",
            Self::Spatial => "Spatial",
            Self::NetData => "NetData",
            Self::Frame => "Frame",
        }
    }

    /// `true` iff this tag is *absolutely-egress-banned* at any scope
    /// greater than `Scope::DeviceLocal` per `specs/24` § IFC-LABELS.
    /// No `Privilege<*>` capability can override.
    #[must_use]
    pub const fn is_absolutely_egress_banned(self) -> bool {
        matches!(
            self,
            Self::Behavioral | Self::Voice | Self::Spatial | Self::Frame,
        )
    }

    /// `true` iff this tag corresponds to a biometric-family domain
    /// (per `specs/11` § PRIME-DIRECTIVE ENCODING : Body / Spatial /
    /// Frame all reveal body-pose).
    #[must_use]
    pub const fn is_biometric_family(self) -> bool {
        matches!(self, Self::Spatial | Self::Frame)
    }
}

impl fmt::Display for HostSensitiveTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ════ Scope enum — packed u8 ════════════════════════════════════════════════

/// Egress-scope qualifier for a host-FFI label. Matches the granularity
/// of `specs/24` § IFC-LABELS + `cssl-rt::net` `NET_CAP_*` bits.
///
/// `repr(u8)` ; only 4 bits used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum HostLabelScope {
    /// Stays inside the device kernel-buffer + GPU-VRAM only. Default.
    DeviceLocal = 0x1,
    /// Crosses thread-boundaries inside the same process.
    ProcessLocal = 0x2,
    /// Crosses process-boundaries on the same device (IPC).
    CrossProcess = 0x3,
    /// Egresses off the device via `cssl-rt::net` (loopback or non-loopback).
    Network = 0x4,
}

impl HostLabelScope {
    /// All four scopes in canonical order, narrow → broad.
    pub const ALL: [Self; 4] = [
        Self::DeviceLocal,
        Self::ProcessLocal,
        Self::CrossProcess,
        Self::Network,
    ];

    /// Stable string identifier used in audit-events + diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeviceLocal => "DeviceLocal",
            Self::ProcessLocal => "ProcessLocal",
            Self::CrossProcess => "CrossProcess",
            Self::Network => "Network",
        }
    }

    /// `true` iff this scope is more-or-equally-restrictive than `other`.
    /// `DeviceLocal ⊑ ProcessLocal ⊑ CrossProcess ⊑ Network`.
    #[must_use]
    pub const fn flows_to(self, other: Self) -> bool {
        (self as u8) <= (other as u8)
    }
}

impl fmt::Display for HostLabelScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ════ HostLabel — packed u8 bitfield ═══════════════════════════════════════

/// Host-FFI IFC label : packed bitfield carrying a [`HostSensitiveTag`]
/// (4 bits) + [`HostLabelScope`] (4 bits) in a single `u8`.
///
/// § Sawyer-efficiency : entire label = 1 byte. No String, no heap.
///
/// Layout :
/// ```text
///   bit:   7 6 5 4   3 2 1 0
///          └─tag─┘   └scope┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct HostLabel(u8);

impl HostLabel {
    const TAG_SHIFT: u8 = 4;
    const TAG_MASK: u8 = 0xF0;
    const SCOPE_MASK: u8 = 0x0F;

    /// Build a label from tag + scope.
    #[must_use]
    pub const fn new(tag: HostSensitiveTag, scope: HostLabelScope) -> Self {
        let packed = ((tag as u8) << Self::TAG_SHIFT) | (scope as u8);
        Self(packed)
    }

    /// Construct from raw `u8` (provenance-checked). Returns `None` if the
    /// nibbles do not decode to a valid `(tag, scope)` pair.
    #[must_use]
    pub const fn from_raw(byte: u8) -> Option<Self> {
        // Decode without dispatch.
        let tag_bits = (byte & Self::TAG_MASK) >> Self::TAG_SHIFT;
        let scope_bits = byte & Self::SCOPE_MASK;
        let tag_ok = matches!(tag_bits, 0x1..=0x5);
        let scope_ok = matches!(scope_bits, 0x1..=0x4);
        if tag_ok && scope_ok {
            Some(Self(byte))
        } else {
            None
        }
    }

    /// Project the underlying `u8` representation. Stable on-wire form.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Project the [`HostSensitiveTag`].
    #[must_use]
    pub const fn tag(self) -> HostSensitiveTag {
        // Safe : constructor invariant guarantees tag-bits ∈ {1..5}.
        match (self.0 & Self::TAG_MASK) >> Self::TAG_SHIFT {
            0x1 => HostSensitiveTag::Behavioral,
            0x2 => HostSensitiveTag::Voice,
            0x3 => HostSensitiveTag::Spatial,
            0x4 => HostSensitiveTag::NetData,
            0x5 => HostSensitiveTag::Frame,
            // Unreachable under constructor invariant ; default keeps the
            // function `const`-and-total without a panic in stable-const.
            _ => HostSensitiveTag::Behavioral,
        }
    }

    /// Project the [`HostLabelScope`].
    #[must_use]
    pub const fn scope(self) -> HostLabelScope {
        match self.0 & Self::SCOPE_MASK {
            0x1 => HostLabelScope::DeviceLocal,
            0x2 => HostLabelScope::ProcessLocal,
            0x3 => HostLabelScope::CrossProcess,
            0x4 => HostLabelScope::Network,
            // Unreachable under constructor invariant.
            _ => HostLabelScope::DeviceLocal,
        }
    }

    /// `true` iff this label is admissible at the given scope.
    ///
    /// § PRIME-DIRECTIVE : Behavioral / Voice / Spatial / Frame are
    /// rejected at any scope > DeviceLocal. NetData defers to scope-
    /// flows-to comparison.
    #[must_use]
    pub fn admissible_at(self, target_scope: HostLabelScope) -> bool {
        if self.tag().is_absolutely_egress_banned() {
            return matches!(target_scope, HostLabelScope::DeviceLocal);
        }
        // NetData : structural-flow-check.
        self.scope().flows_to(target_scope)
    }
}

impl fmt::Display for HostLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sensitive<{}>@{}", self.tag(), self.scope())
    }
}

// ════ Runtime-emit hook ═════════════════════════════════════════════════════

/// Reasons a label-verification fails. Used by `verify_label`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfcViolation {
    /// Label is absolutely-egress-banned at the requested scope (no
    /// `Privilege<*>` override exists).
    AbsoluteEgressBan {
        /// The offending tag.
        tag: HostSensitiveTag,
        /// The scope the call-site requested.
        attempted_scope: HostLabelScope,
    },
    /// Label's source-scope does not flow-to the target-scope.
    ScopeMismatch {
        /// The label's source scope.
        source_scope: HostLabelScope,
        /// The scope the call-site requested.
        target_scope: HostLabelScope,
        /// The label's tag (informational).
        tag: HostSensitiveTag,
    },
    /// Sovereign-mismatch : the label asserts a sovereign-domain that
    /// does not match the caller's sovereign-context. Used when a
    /// non-sovereign caller attempts to dispatch into a sovereign-only
    /// host-shim per `specs/11` § PRIVILEGE EFFECT.
    SovereignMismatch {
        /// The expected sovereign-context (per `specs/11` PrivilegeLevel).
        expected: SovereignContext,
        /// The caller's actual context.
        actual: SovereignContext,
    },
}

/// Sovereign-context tag — narrowed projection of `cssl-ifc::PrivilegeLevel`
/// for host-FFI cross-checks. Three values map directly to the three
/// host-FFI privilege-tiers per `specs/11` § PRIVILEGE EFFECT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SovereignContext {
    /// Ordinary game-code.
    User = 0x1,
    /// Engine-internal scheduler / allocator.
    System = 0x2,
    /// OS-interop / kernel-grade host-shim.
    Kernel = 0x3,
}

impl SovereignContext {
    /// Stable string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::System => "System",
            Self::Kernel => "Kernel",
        }
    }

    /// `true` iff `self` ≥ `required`. Used for kernel-shim gating.
    #[must_use]
    pub const fn covers(self, required: Self) -> bool {
        (self as u8) >= (required as u8)
    }
}

/// Runtime-emit hook called by every `__cssl_<domain>_*` shim before
/// any Sensitive-labeled byte crosses an FFI boundary. Verifies that
/// the label is admissible at the target scope AND that the caller's
/// sovereign-context covers the required level.
///
/// § SPEC : `specs/24` § P5 IFC-label.
/// § PRIME-DIRECTIVE : the absolutely-egress-banned set is non-overridable
/// per `specs/11` § PRIME-DIRECTIVE ENCODING.
///
/// # Errors
/// Returns [`IfcViolation`] when any of the three structural checks fails.
pub fn verify_label(
    label: HostLabel,
    target_scope: HostLabelScope,
    caller_context: SovereignContext,
    required_context: SovereignContext,
) -> Result<(), IfcViolation> {
    // 1) sovereign-context cross-check
    if !caller_context.covers(required_context) {
        return Err(IfcViolation::SovereignMismatch {
            expected: required_context,
            actual: caller_context,
        });
    }
    // 2) absolute-egress-ban for biometric / behavioral / voice / frame
    if label.tag().is_absolutely_egress_banned()
        && !matches!(target_scope, HostLabelScope::DeviceLocal)
    {
        return Err(IfcViolation::AbsoluteEgressBan {
            tag: label.tag(),
            attempted_scope: target_scope,
        });
    }
    // 3) scope-flows-to check for non-banned labels (NetData)
    if !label.scope().flows_to(target_scope) {
        return Err(IfcViolation::ScopeMismatch {
            source_scope: label.scope(),
            target_scope,
            tag: label.tag(),
        });
    }
    Ok(())
}

// ════ Tests ═════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    // 1) Sawyer-efficiency : HostLabel is exactly 1 byte
    #[test]
    fn host_label_is_one_byte() {
        assert_eq!(size_of::<HostLabel>(), 1);
    }

    // 2) every tag round-trips through (new ⇄ tag)
    #[test]
    fn tag_roundtrip() {
        for t in HostSensitiveTag::ALL {
            let lbl = HostLabel::new(t, HostLabelScope::DeviceLocal);
            assert_eq!(lbl.tag(), t, "{t:?}");
        }
    }

    // 3) every scope round-trips
    #[test]
    fn scope_roundtrip() {
        for s in HostLabelScope::ALL {
            let lbl = HostLabel::new(HostSensitiveTag::NetData, s);
            assert_eq!(lbl.scope(), s, "{s:?}");
        }
    }

    // 4) raw / from_raw round-trip on every (tag, scope) combination
    #[test]
    fn raw_roundtrip_all_combinations() {
        for t in HostSensitiveTag::ALL {
            for s in HostLabelScope::ALL {
                let lbl = HostLabel::new(t, s);
                let raw = lbl.raw();
                let back = HostLabel::from_raw(raw).expect("valid raw");
                assert_eq!(back, lbl, "{t:?}+{s:?}");
            }
        }
    }

    // 5) from_raw rejects malformed nibbles
    #[test]
    fn from_raw_rejects_unknown_tag() {
        // tag-nibble 0x0 (out-of-range) + scope-nibble 0x1
        assert!(HostLabel::from_raw(0x01).is_none());
        // tag-nibble 0xF (reserved) + scope-nibble 0x1
        assert!(HostLabel::from_raw(0xF1).is_none());
        // tag-nibble 0x1 + scope-nibble 0x0 (out-of-range)
        assert!(HostLabel::from_raw(0x10).is_none());
        // tag-nibble 0x1 + scope-nibble 0xA (reserved)
        assert!(HostLabel::from_raw(0x1A).is_none());
    }

    // 6) absolute-egress-ban predicate covers Behavioral/Voice/Spatial/Frame
    #[test]
    fn absolute_egress_ban_partition() {
        for t in [
            HostSensitiveTag::Behavioral,
            HostSensitiveTag::Voice,
            HostSensitiveTag::Spatial,
            HostSensitiveTag::Frame,
        ] {
            assert!(t.is_absolutely_egress_banned(), "{t:?}");
        }
        assert!(!HostSensitiveTag::NetData.is_absolutely_egress_banned());
    }

    // 7) biometric-family includes Spatial + Frame
    #[test]
    fn biometric_family_includes_spatial_and_frame() {
        assert!(HostSensitiveTag::Spatial.is_biometric_family());
        assert!(HostSensitiveTag::Frame.is_biometric_family());
        assert!(!HostSensitiveTag::Behavioral.is_biometric_family());
        assert!(!HostSensitiveTag::Voice.is_biometric_family());
        assert!(!HostSensitiveTag::NetData.is_biometric_family());
    }

    // 8) scope flows-to is reflexive + monotone
    #[test]
    fn scope_flows_to_lattice() {
        for s in HostLabelScope::ALL {
            assert!(s.flows_to(s));
        }
        assert!(HostLabelScope::DeviceLocal.flows_to(HostLabelScope::Network));
        assert!(!HostLabelScope::Network.flows_to(HostLabelScope::DeviceLocal));
        assert!(HostLabelScope::ProcessLocal.flows_to(HostLabelScope::CrossProcess));
    }

    // 9) admissible_at respects absolute-egress-ban
    #[test]
    fn admissible_at_blocks_behavioral_off_device() {
        let lbl = HostLabel::new(
            HostSensitiveTag::Behavioral,
            HostLabelScope::DeviceLocal,
        );
        assert!(lbl.admissible_at(HostLabelScope::DeviceLocal));
        assert!(!lbl.admissible_at(HostLabelScope::ProcessLocal));
        assert!(!lbl.admissible_at(HostLabelScope::CrossProcess));
        assert!(!lbl.admissible_at(HostLabelScope::Network));
    }

    // 10) admissible_at allows NetData per scope-flows-to
    #[test]
    fn admissible_at_allows_netdata_within_scope() {
        let lbl = HostLabel::new(HostSensitiveTag::NetData, HostLabelScope::ProcessLocal);
        assert!(lbl.admissible_at(HostLabelScope::ProcessLocal));
        assert!(lbl.admissible_at(HostLabelScope::CrossProcess));
        assert!(lbl.admissible_at(HostLabelScope::Network));
        assert!(!lbl.admissible_at(HostLabelScope::DeviceLocal));
    }

    // 11) verify_label OK path
    #[test]
    fn verify_label_ok_netdata_user_to_user() {
        let lbl = HostLabel::new(HostSensitiveTag::NetData, HostLabelScope::ProcessLocal);
        assert_eq!(
            verify_label(
                lbl,
                HostLabelScope::Network,
                SovereignContext::User,
                SovereignContext::User,
            ),
            Ok(())
        );
    }

    // 12) verify_label : Behavioral cross-process = AbsoluteEgressBan
    #[test]
    fn verify_label_behavioral_cross_process_is_banned() {
        let lbl = HostLabel::new(HostSensitiveTag::Behavioral, HostLabelScope::DeviceLocal);
        let r = verify_label(
            lbl,
            HostLabelScope::CrossProcess,
            SovereignContext::Kernel,
            SovereignContext::Kernel,
        );
        assert_eq!(
            r,
            Err(IfcViolation::AbsoluteEgressBan {
                tag: HostSensitiveTag::Behavioral,
                attempted_scope: HostLabelScope::CrossProcess,
            })
        );
    }

    // 13) verify_label : Voice → Network rejected even with Kernel context
    //     (proves there is NO Privilege override)
    #[test]
    fn verify_label_voice_network_rejected_even_with_kernel() {
        let lbl = HostLabel::new(HostSensitiveTag::Voice, HostLabelScope::DeviceLocal);
        let r = verify_label(
            lbl,
            HostLabelScope::Network,
            SovereignContext::Kernel,
            SovereignContext::Kernel,
        );
        assert!(matches!(
            r,
            Err(IfcViolation::AbsoluteEgressBan {
                tag: HostSensitiveTag::Voice,
                ..
            })
        ));
    }

    // 14) verify_label : Spatial → CrossProcess rejected (XR head-pose)
    #[test]
    fn verify_label_spatial_cross_process_rejected() {
        let lbl = HostLabel::new(HostSensitiveTag::Spatial, HostLabelScope::DeviceLocal);
        let r = verify_label(
            lbl,
            HostLabelScope::CrossProcess,
            SovereignContext::User,
            SovereignContext::User,
        );
        assert!(matches!(
            r,
            Err(IfcViolation::AbsoluteEgressBan {
                tag: HostSensitiveTag::Spatial,
                ..
            })
        ));
    }

    // 15) verify_label : Frame → Network rejected (XR render-out)
    #[test]
    fn verify_label_frame_network_rejected() {
        let lbl = HostLabel::new(HostSensitiveTag::Frame, HostLabelScope::DeviceLocal);
        let r = verify_label(
            lbl,
            HostLabelScope::Network,
            SovereignContext::User,
            SovereignContext::User,
        );
        assert!(matches!(
            r,
            Err(IfcViolation::AbsoluteEgressBan {
                tag: HostSensitiveTag::Frame,
                ..
            })
        ));
    }

    // 16) verify_label : sovereign-mismatch detected
    #[test]
    fn verify_label_sovereign_mismatch() {
        let lbl = HostLabel::new(HostSensitiveTag::NetData, HostLabelScope::DeviceLocal);
        let r = verify_label(
            lbl,
            HostLabelScope::DeviceLocal,
            SovereignContext::User,
            SovereignContext::Kernel,
        );
        assert_eq!(
            r,
            Err(IfcViolation::SovereignMismatch {
                expected: SovereignContext::Kernel,
                actual: SovereignContext::User,
            })
        );
    }

    // 17) verify_label : NetData scope-mismatch (cannot widen via flows-to)
    #[test]
    fn verify_label_netdata_scope_mismatch_when_narrowing() {
        // Label says scope=Network ; target=DeviceLocal — should reject.
        let lbl = HostLabel::new(HostSensitiveTag::NetData, HostLabelScope::Network);
        let r = verify_label(
            lbl,
            HostLabelScope::DeviceLocal,
            SovereignContext::User,
            SovereignContext::User,
        );
        assert_eq!(
            r,
            Err(IfcViolation::ScopeMismatch {
                source_scope: HostLabelScope::Network,
                target_scope: HostLabelScope::DeviceLocal,
                tag: HostSensitiveTag::NetData,
            })
        );
    }

    // 18) display formatting is stable
    #[test]
    fn display_format_is_stable() {
        let lbl = HostLabel::new(HostSensitiveTag::Voice, HostLabelScope::DeviceLocal);
        assert_eq!(format!("{lbl}"), "Sensitive<Voice>@DeviceLocal");
    }

    // 19) sovereign-context covers is monotone
    #[test]
    fn sovereign_context_covers_is_monotone() {
        assert!(SovereignContext::Kernel.covers(SovereignContext::User));
        assert!(SovereignContext::Kernel.covers(SovereignContext::System));
        assert!(SovereignContext::Kernel.covers(SovereignContext::Kernel));
        assert!(SovereignContext::System.covers(SovereignContext::User));
        assert!(!SovereignContext::User.covers(SovereignContext::Kernel));
        assert!(!SovereignContext::User.covers(SovereignContext::System));
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § INTEGRATION-NOTE  (Wave-D7 stitch-up — separate commit)
// ════════════════════════════════════════════════════════════════════════════
//
// To activate this module, add ONE LINE to `cssl-ifc/src/lib.rs` :
//
//     pub mod host_labels;
//
// And optionally re-export the public surface :
//
//     pub use host_labels::{
//         HostLabel,
//         HostLabelScope,
//         HostSensitiveTag,
//         IfcViolation,
//         SovereignContext,
//         verify_label,
//     };
//
// Downstream `host_*` modules (Wave-D1..D6/D8 in `cssl-rt`) call
// `verify_label(label, target_scope, caller_ctx, required_ctx)` BEFORE
// any byte tagged with a `HostSensitiveTag` crosses an FFI boundary.
// On `Err(IfcViolation::*)` the shim returns `-EPERM` immediately AND
// emits a `{Audit<*>}` event per `specs/11` § AUDIT EFFECT.
//
// W3β-08 alignment : when `cssl-ifc::SensitiveDomain` adds `Voice` +
// `Spatial` + `Frame` first-class variants, this module's `HostSensitiveTag`
// becomes the host-side projection. The cross-walk is structural ; the
// `is_absolutely_egress_banned` predicate matches
// `SensitiveDomain::is_telemetry_egress_absolutely_banned`.
//
// ∎ host-labels

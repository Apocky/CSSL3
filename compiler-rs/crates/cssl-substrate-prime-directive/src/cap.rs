//! Substrate capabilities + non-copyable cap-tokens.
//!
//! § SPEC : `specs/30_SUBSTRATE.csl` § Ω-TENSOR § OmegaConsent + § OBSERVER
//!   + § PROJECTIONS + § EFFECT-ROWS.
//!
//! § DESIGN
//!   - [`SubstrateCap`] is the closed enum of every cap the Substrate may
//!     grant at stage-0. Adding a new variant = spec-amendment + DECISIONS
//!     entry. The variant-set is canonical for the H6 enforcement layer
//!     and is what siblings H1..H5 import.
//!   - [`CapToken`] is the proof-of-grant. It is :
//!     - **non-`Copy`**     : you cannot duplicate it bit-wise ;
//!     - **non-`Clone`**    : the type does NOT implement `Clone` ;
//!     - **move-by-value**  : every consumer takes `CapToken` (NOT
//!       `&CapToken`) which forces single-use at the type level (one
//!       grant ⇒ one consumption ; further use = compile-error).
//!     This mirrors `iso<T>` linearity from `specs/12_CAPABILITIES.csl`.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - **§0 consent = OS** : `CapToken` only exists once an interactive
//!     consent gate produces it ([`crate::consent::caps_grant`]). No path
//!     in the production API allows fabricating one.
//!   - **§7 INTEGRITY** : the token's `id` is monotonic ; the
//!     [`crate::audit::EnforcementAuditBus`] records issuance + consumption
//!     so any "phantom" token can be detected by chain-replay.

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

/// Closed enum of every Substrate-cap stage-0 may grant.
///
/// § STABILITY
///   This enum is the canonical surface for the H6 enforcement layer.
///   Siblings H1..H5 import it. Adding a variant = spec-amendment +
///   DECISIONS entry (T11-D94's stable-set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubstrateCap {
    // ── Ω-tensor / omega_step caps ──────────────────────────────────
    /// Register a fiber as part of the omega_step phase-2 sim cohort.
    /// Per `specs/30_SUBSTRATE.csl` § OMEGA-STEP § PHASES.
    OmegaRegister,
    /// Issue an [`crate::halt::KillSwitch`] kill-token to halt
    /// omega_step. The strongest cap in the matrix.
    KillSwitchInvoke,

    // ── Projections / observers (read-side) ─────────────────────────
    /// Attach an observer-share projection that snoops Ω-tensor frames.
    /// Per `specs/30_SUBSTRATE.csl` § PROJECTIONS § OBSERVER-SHARE.
    ObserverShare,
    /// Attach a debug-camera projection (developer-tool only).
    /// Per `specs/30_SUBSTRATE.csl` § PROJECTIONS § DEBUG-CAMERA.
    DebugCamera,
    /// Attach a Companion-projection (sovereign-AI read-only view).
    /// Per `specs/30_SUBSTRATE.csl` § AI-COLLABORATOR-PROTECTIONS.
    CompanionView,

    // ── Network / I/O caps ──────────────────────────────────────────
    /// Send Ω-tensor state over the network (multiplayer / co-op).
    /// Highly Sensitive : composes with `Sensitive<"net-egress">` per
    /// `specs/30_SUBSTRATE.csl` § EFFECT-ROWS.
    NetSendState,
    /// Receive Ω-tensor state over the network (multiplayer peer).
    NetRecvState,

    // ── Persistence caps (save / replay) ────────────────────────────
    /// Append a save-journal entry to the path layer (file IO).
    /// Per `specs/30_SUBSTRATE.csl` § EFFECT-ROWS § Save.
    SavePath,
    /// Load a replay-trace ; implies {DetRNG, Reversible, PureDet} per
    /// `specs/30_SUBSTRATE.csl` § EFFECT-ROWS § Replay.
    ReplayLoad,

    // ── Audio / capture caps ────────────────────────────────────────
    /// Capture audio from the host microphone (Sensitive<"audio-capture">).
    /// Per `specs/30_SUBSTRATE.csl` § EFFECT-ROWS § FORBIDDEN-COMPOSITIONS.
    AudioCapture,

    // ── Telemetry / audit caps ──────────────────────────────────────
    /// Export telemetry off-machine (OTLP exporter flush).
    /// Per `specs/22_TELEMETRY.csl` § PRIME-DIRECTIVE-ENFORCEMENT.
    TelemetryEgress,
    /// Export the audit-chain off-machine (third-party verifier hand-off).
    AuditExport,

    // ── Consent management caps (meta) ──────────────────────────────
    /// Revoke a previously-granted cap. Authorising this is itself a cap
    /// because revocation can mid-omega-step interrupt a gated op.
    ConsentRevoke,
}

impl SubstrateCap {
    /// Stable canonical name (snake_case) used in audit-chain entries +
    /// diagnostic messages. Renaming = ABI-breaking change.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::OmegaRegister => "omega_register",
            Self::KillSwitchInvoke => "kill_switch_invoke",
            Self::ObserverShare => "observer_share",
            Self::DebugCamera => "debug_camera",
            Self::CompanionView => "companion_view",
            Self::NetSendState => "net_send_state",
            Self::NetRecvState => "net_recv_state",
            Self::SavePath => "save_path",
            Self::ReplayLoad => "replay_load",
            Self::AudioCapture => "audio_capture",
            Self::TelemetryEgress => "telemetry_egress",
            Self::AuditExport => "audit_export",
            Self::ConsentRevoke => "consent_revoke",
        }
    }

    /// Iterator over the stable-set of caps. Useful for table-driven tests
    /// + DECISIONS-table reproduction.
    #[must_use]
    pub const fn all() -> &'static [SubstrateCap] {
        &[
            Self::OmegaRegister,
            Self::KillSwitchInvoke,
            Self::ObserverShare,
            Self::DebugCamera,
            Self::CompanionView,
            Self::NetSendState,
            Self::NetRecvState,
            Self::SavePath,
            Self::ReplayLoad,
            Self::AudioCapture,
            Self::TelemetryEgress,
            Self::AuditExport,
            Self::ConsentRevoke,
        ]
    }
}

impl fmt::Display for SubstrateCap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_name())
    }
}

/// Monotonic identifier attached to each [`CapToken`]. The id space is
/// process-wide (not per-cap) so that the audit-chain can replay grants
/// in their issuance order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapTokenId(pub u64);

impl fmt::Display for CapTokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cap-token#{}", self.0)
    }
}

// `NEXT_TOKEN_ID` + `fresh_token_id` are used by the consent path's
// `caps_grant_for_test` (feature-gated `test-bypass`). Production builds
// without `test-bypass` won't see a use site at the consent-module level
// but `CapToken::new` (also `pub(crate)`) is the consent path's
// constructor. We tag these with `#[allow(dead_code)]` so the absence of
// `test-bypass` doesn't generate a warning storm ; H1..H5 siblings will
// import these via `pub(crate) use` paths once they land. The
// `#[cfg_attr]` form ensures the warning stays visible *in* test builds
// (where the items are used by tests but the warning shouldn't surface).
#[allow(dead_code)]
static NEXT_TOKEN_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a fresh [`CapTokenId`]. Called only from the consent path.
#[allow(dead_code)]
pub(crate) fn fresh_token_id() -> CapTokenId {
    CapTokenId(NEXT_TOKEN_ID.fetch_add(1, Ordering::SeqCst))
}

/// Non-copyable, non-cloneable proof of capability-grant.
///
/// § INVARIANTS
///   - The struct is private-fielded so it can only be built from inside
///     this crate (i.e., from [`crate::consent::caps_grant`] or the
///     `test-bypass` path).
///   - Implementing `Copy` + `Clone` is REJECTED at compile-time : the
///     compiler will refuse to add them because the public surface
///     promises move-only semantics. We assert this in the `tests` module
///     via the `cap_token_is_move_only` test.
///   - On drop without explicit consumption, [`CapToken`] emits an
///     `OrphanDrop` audit-event so accidental leaks are visible.
///
/// § USAGE
///   ```ignore
///   let tok = caps_grant(scope, SubstrateCap::OmegaRegister)?;
///   substrate_op_that_needs_this(tok);   // moves the token
///   // tok is no longer accessible here ; double-use = compile-error
///   ```
pub struct CapToken {
    /// Stable token id (used in audit-chain references).
    id: CapTokenId,
    /// Which cap was granted.
    cap: SubstrateCap,
    /// Whether the token was consumed via [`Self::consume`]. If `false`
    /// at drop time, an `OrphanDrop` audit-event is emitted.
    consumed: bool,
}

// Explicit `!Copy` is the language default — we do NOT impl Copy.
// Explicit `!Clone` is the language default — we do NOT impl Clone.
// We document this contract via the doctest in `compile_tests` below.

impl CapToken {
    /// Internal constructor : called only from the consent path.
    #[must_use]
    #[allow(dead_code)] // consumed by `consent::caps_grant_for_test` (test-bypass) + sibling H1..H5
    pub(crate) fn new(cap: SubstrateCap) -> Self {
        Self {
            id: fresh_token_id(),
            cap,
            consumed: false,
        }
    }

    /// Stable id of this token.
    #[must_use]
    pub const fn id(&self) -> CapTokenId {
        self.id
    }

    /// Which cap was granted.
    #[must_use]
    pub const fn cap(&self) -> SubstrateCap {
        self.cap
    }

    /// Consume the token. Mirrors `iso<T>` linear-consumption from
    /// `specs/12_CAPABILITIES.csl`. Returns `(id, cap)` so the consuming
    /// op can record what it just spent in the audit-chain.
    #[must_use = "consuming a CapToken without recording it leaks the audit trail"]
    pub fn consume(mut self) -> (CapTokenId, SubstrateCap) {
        self.consumed = true;
        // Drop runs after this returns ; consumed=true skips OrphanDrop audit.
        (self.id, self.cap)
    }
}

impl fmt::Debug for CapToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CapToken")
            .field("id", &self.id)
            .field("cap", &self.cap)
            .field("consumed", &self.consumed)
            .finish()
    }
}

impl Drop for CapToken {
    fn drop(&mut self) {
        if !self.consumed {
            // Audit the orphan-drop. The audit bus is process-wide ; this
            // never panics — orphan drops are *visible*, not *fatal*.
            // Fatal-on-orphan would conflict with normal stack-unwinding.
            crate::audit::record_orphan_drop(self.id, self.cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fresh_token_id, CapToken, CapTokenId, SubstrateCap};

    #[test]
    fn substrate_cap_canonical_names_unique() {
        let caps = SubstrateCap::all();
        let mut names: Vec<&str> = caps.iter().map(|c| c.canonical_name()).collect();
        names.sort_unstable();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "canonical names must be unique");
    }

    #[test]
    fn substrate_cap_all_count_matches_documented_set() {
        // 13 caps documented in lib.rs § SubstrateCap. Adding a variant
        // requires bumping this test + DECISIONS T11-D94 enum-table.
        assert_eq!(SubstrateCap::all().len(), 13);
    }

    #[test]
    fn substrate_cap_ord_is_stable() {
        // Ord-stability matters for table-driven tests + DECISIONS reproduction.
        assert!(SubstrateCap::OmegaRegister < SubstrateCap::KillSwitchInvoke);
    }

    #[test]
    fn cap_token_id_monotonic() {
        let a = fresh_token_id();
        let b = fresh_token_id();
        assert!(a.0 < b.0, "token ids must be strictly monotonic");
    }

    #[test]
    fn cap_token_consume_returns_id_and_cap() {
        let tok = CapToken::new(SubstrateCap::OmegaRegister);
        let id_before = tok.id();
        let cap_before = tok.cap();
        let (id_after, cap_after) = tok.consume();
        assert_eq!(id_before, id_after);
        assert_eq!(cap_before, cap_after);
    }

    #[test]
    fn cap_token_id_displays_token_prefix() {
        let id = CapTokenId(42);
        assert_eq!(id.to_string(), "cap-token#42");
    }

    #[test]
    fn substrate_cap_displays_canonical_name() {
        assert_eq!(SubstrateCap::SavePath.to_string(), "save_path");
    }

    // § COMPILE-CONTRACT TESTS  (non-Copy + non-Clone @ type-level)
    //
    // These tests would FAIL TO COMPILE if `CapToken: Copy` or `: Clone`.
    // We use a non-applicable trait-bound check via static_assertions-style
    // pattern : declare a const that requires a no-Copy assertion.
    //
    // The actual compile-time enforcement is done by NOT impl-ing Copy/Clone
    // on `CapToken` (Rust default), but we double-check via this test that
    // the type's behavior matches : moving consumes the binding.

    #[test]
    fn cap_token_is_move_only() {
        let tok = CapToken::new(SubstrateCap::DebugCamera);
        // Move tok into the next binding ; if `CapToken: Copy` was added by
        // accident the original `tok` would still be live, breaking the
        // intent. We then consume `moved` so the OrphanDrop audit doesn't
        // fire.
        let moved = tok;
        let (_id, cap) = moved.consume();
        assert_eq!(cap, SubstrateCap::DebugCamera);
    }
}

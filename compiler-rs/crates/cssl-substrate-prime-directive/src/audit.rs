//! Audit-chain integration for the H6 enforcement layer.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § AUDIT-CHAIN + `specs/30_SUBSTRATE.csl`
//!   § PRIME_DIRECTIVE-ALIGNMENT § AUDIT-CHAIN-INTEGRATION + R18 telemetry-ring
//!   invariant.
//!
//! § DESIGN
//!   - [`EnforcementAuditBus`] wraps a [`cssl_telemetry::AuditChain`] and
//!     exposes typed `record_*` methods that produce stable tag-strings +
//!     deterministic message bodies.
//!   - Every cap-grant + revoke + violation emits an entry. Per `specs/22_TELEMETRY.csl`
//!     the chain is APPEND-ONLY ; we never mutate or remove entries.
//!   - The bus also receives orphan-drop events from
//!     [`crate::cap::CapToken`]'s `Drop` impl via a private process-wide
//!     channel. Tests can intercept by grabbing the
//!     `audit_chain_for_test` handle (feature-gated `test-bypass`).
//!
//! § INVARIANTS
//!   - Audit-append failure ⇒ panic (`record_*` panics on internal allocation
//!     failures so the chain is never silently dropped). Per `specs/22_TELEMETRY.csl`
//!     § PRIME-DIRECTIVE : `audit-append-failure → process-abort`.
//!   - Tag strings are stable across builds (we test for them).
//!   - Entry order matches issuance order (chain seq is monotonic).

use std::sync::Mutex;

use cssl_telemetry::audit::AuditChain;
use cssl_telemetry::audit::AuditEntry;
use cssl_telemetry::path_hash::PathHash;

use crate::cap::{CapTokenId, SubstrateCap};
use crate::consent::ConsentScope;
use crate::halt::HaltReason;
use crate::harm::Prohibition;

/// Op-kind code for [`AuditEvent::PathOp`] entries (T11-D130).
///
/// § STABILITY
///   Renaming a variant = ABI bump (audit-chain replays pin this name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathOpKind {
    /// `__cssl_fs_open` (any flags).
    Open,
    /// `__cssl_fs_read` call.
    Read,
    /// `__cssl_fs_write` call.
    Write,
    /// `__cssl_fs_close`.
    Close,
    /// `SavePath::append_hashed` — substrate-level save journal append.
    SaveAppend,
}

impl PathOpKind {
    /// Stable canonical name in audit messages.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Open => "fs-open",
            Self::Read => "fs-read",
            Self::Write => "fs-write",
            Self::Close => "fs-close",
            Self::SaveAppend => "save-append",
        }
    }
}

/// Kinds of enforcement events that can land on the audit-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// A cap-grant was issued (interactive or test-bypass).
    GrantIssued {
        token_id: CapTokenId,
        cap: SubstrateCap,
        principal: String,
        purpose: String,
        test_bypass: bool,
    },
    /// A cap-grant attempt was denied.
    GrantDenied {
        cap: SubstrateCap,
        principal: String,
        purpose: String,
        reason: String,
    },
    /// A cap was revoked (token consumed by `caps_revoke`).
    Revoked {
        token_id: CapTokenId,
        cap: SubstrateCap,
    },
    /// A [`crate::cap::CapToken`] was dropped without being consumed. Visible
    /// (not fatal) — the bus records the leak so it can be inspected.
    OrphanDrop {
        token_id: CapTokenId,
        cap: SubstrateCap,
    },
    /// A harm-prevention check fired and rejected a Substrate op.
    HarmCheckFailed {
        prohibition: Prohibition,
        site: String,
    },
    /// The kill-switch [`crate::halt::substrate_halt`] was invoked.
    Halted {
        reason: HaltReason,
        outstanding_steps: u32,
    },
    /// Attestation drift was detected (the per-fn ATTESTATION constant
    /// did not match the canonical hash).
    AttestationDrift { site: String },
    /// A path-touching op was recorded with hash-only discipline (T11-D130).
    /// The path itself is NEVER stored — only the 32-byte
    /// installation-salted BLAKE3 hash. Per `specs/22 § FS-OPS` and
    /// `PRIME_DIRECTIVE.md § 1` (no surveillance).
    PathOp {
        /// 32-byte BLAKE3-salted hash of the path (never the raw path).
        path_hash: PathHash,
        /// Which fs-op the entry records.
        op: PathOpKind,
        /// Optional structured extra (e.g., `bytes=42`). Validated free of
        /// raw-path patterns by [`cssl_telemetry::audit_path_op_check_raw_path_rejected`].
        extra: String,
    },
}

impl AuditEvent {
    /// The stable tag string used for the chain entry. Renaming = ABI bump.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::GrantIssued { .. } => "h6.grant.issued",
            Self::GrantDenied { .. } => "h6.grant.denied",
            Self::Revoked { .. } => "h6.revoke",
            Self::OrphanDrop { .. } => "h6.orphan-drop",
            Self::HarmCheckFailed { .. } => "h6.harm.failed",
            Self::Halted { .. } => "h6.halt",
            Self::AttestationDrift { .. } => "h6.attestation.drift",
            Self::PathOp { .. } => "h6.fs.path-op",
        }
    }

    /// Render the event as a deterministic UTF-8 message body. The body
    /// content is what the BLAKE3 chain-hash covers ; we intentionally
    /// keep it short + structured.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::GrantIssued {
                token_id,
                cap,
                principal,
                purpose,
                test_bypass,
            } => format!(
                "issued {token_id} {cap} principal={principal} purpose={purpose} bypass={test_bypass}"
            ),
            Self::GrantDenied {
                cap,
                principal,
                purpose,
                reason,
            } => format!(
                "denied {cap} principal={principal} purpose={purpose} reason={reason}"
            ),
            Self::Revoked { token_id, cap } => format!("revoked {token_id} {cap}"),
            Self::OrphanDrop { token_id, cap } => format!("orphan-drop {token_id} {cap}"),
            Self::HarmCheckFailed { prohibition, site } => {
                format!("harm-check {} site={site}", prohibition.code())
            }
            Self::Halted {
                reason,
                outstanding_steps,
            } => format!(
                "halt reason={} outstanding_steps={outstanding_steps}",
                reason.canonical_name()
            ),
            Self::AttestationDrift { site } => format!("attestation-drift site={site}"),
            Self::PathOp {
                path_hash,
                op,
                extra,
            } => {
                if extra.is_empty() {
                    format!("path-op kind={} path_hash={path_hash}", op.canonical_name())
                } else {
                    format!(
                        "path-op kind={} path_hash={path_hash} {extra}",
                        op.canonical_name()
                    )
                }
            }
        }
    }
}

/// Audit-bus owned by a [`crate::consent::ConsentStore`].
///
/// § DESIGN
///   - Wraps an [`AuditChain`] (from `cssl-telemetry`).
///   - Every event is appended with a deterministic timestamp (caller-supplied
///     or `0`) so unit-tests can produce byte-identical chains.
#[derive(Debug)]
pub struct EnforcementAuditBus {
    chain: AuditChain,
    /// Monotonic synthetic timestamp used when no explicit timestamp is
    /// passed. Real production builds use OS-clock per `specs/22_TELEMETRY.csl`
    /// § ring-implementation.
    next_ts: u64,
}

impl EnforcementAuditBus {
    /// Construct a bus with a stub-signed empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain: AuditChain::new(),
            next_ts: 0,
        }
    }

    /// Construct a bus around an existing (possibly signed) chain.
    #[must_use]
    pub fn from_chain(chain: AuditChain) -> Self {
        Self { chain, next_ts: 0 }
    }

    /// Inspect the inner chain (read-only).
    #[must_use]
    pub const fn chain(&self) -> &AuditChain {
        &self.chain
    }

    /// Number of entries currently in the chain.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.chain.len()
    }

    /// Iterate entries in chain order. Borrowed from the underlying chain.
    pub fn iter(&self) -> impl Iterator<Item = &AuditEntry> {
        self.chain.iter()
    }

    /// Append `event` to the chain. Internal — every public `record_*`
    /// method routes here.
    fn append(&mut self, event: &AuditEvent) {
        let ts = self.next_ts;
        self.next_ts = self.next_ts.saturating_add(1);
        self.chain
            .append(event.tag().to_string(), event.message(), ts);
    }

    // ── Public typed recorders ──────────────────────────────────────────

    pub fn record_grant_issued(
        &mut self,
        token_id: CapTokenId,
        cap: SubstrateCap,
        scope: &ConsentScope,
        test_bypass: bool,
    ) {
        let event = AuditEvent::GrantIssued {
            token_id,
            cap,
            principal: scope.principal.clone(),
            purpose: scope.purpose.clone(),
            test_bypass,
        };
        self.append(&event);
    }

    pub fn record_grant_denied(
        &mut self,
        cap: SubstrateCap,
        scope: &ConsentScope,
        reason: impl Into<String>,
    ) {
        let event = AuditEvent::GrantDenied {
            cap,
            principal: scope.principal.clone(),
            purpose: scope.purpose.clone(),
            reason: reason.into(),
        };
        self.append(&event);
    }

    pub fn record_revoke(&mut self, token_id: CapTokenId, cap: SubstrateCap) {
        let event = AuditEvent::Revoked { token_id, cap };
        self.append(&event);
    }

    pub fn record_orphan_drop(&mut self, token_id: CapTokenId, cap: SubstrateCap) {
        let event = AuditEvent::OrphanDrop { token_id, cap };
        self.append(&event);
    }

    pub fn record_harm_check_failed(&mut self, prohibition: Prohibition, site: impl Into<String>) {
        let event = AuditEvent::HarmCheckFailed {
            prohibition,
            site: site.into(),
        };
        self.append(&event);
    }

    pub fn record_halted(&mut self, reason: HaltReason, outstanding_steps: u32) {
        let event = AuditEvent::Halted {
            reason,
            outstanding_steps,
        };
        self.append(&event);
    }

    pub fn record_attestation_drift(&mut self, site: impl Into<String>) {
        let event = AuditEvent::AttestationDrift { site: site.into() };
        self.append(&event);
    }

    /// Record a path-touching op with **hash-only discipline** (T11-D130).
    ///
    /// § DISCIPLINE
    ///   `path_hash` is the 32-byte BLAKE3-salted hash of the path ;
    ///   the raw path is NEVER passed to this method. `extra` is scanned
    ///   for raw-path patterns by
    ///   [`cssl_telemetry::audit_path_op_check_raw_path_rejected`] and
    ///   rejected if it contains `/`, `\`, or a Windows drive prefix.
    ///
    /// # Errors
    /// Returns
    /// [`cssl_telemetry::PathLogError::RawPathInField`]
    /// if `extra` contains a raw-path leak.
    pub fn record_path_op(
        &mut self,
        path_hash: PathHash,
        op: PathOpKind,
        extra: &str,
    ) -> Result<(), cssl_telemetry::PathLogError> {
        cssl_telemetry::audit_path_op_check_raw_path_rejected(extra)?;
        let event = AuditEvent::PathOp {
            path_hash,
            op,
            extra: extra.to_string(),
        };
        self.append(&event);
        Ok(())
    }
}

impl Default for EnforcementAuditBus {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § PROCESS-BUS — orphan-drop sink.
//
// `CapToken::drop` does not have access to a `&mut ConsentStore`. We give
// it a process-wide bus that aggregates orphan-drops. Tests can swap this
// out via [`audit_chain_for_test`].
// ───────────────────────────────────────────────────────────────────────

static PROCESS_BUS: Mutex<Option<EnforcementAuditBus>> = Mutex::new(None);

/// Record an orphan-drop on the process-wide bus. Called from
/// [`crate::cap::CapToken::drop`]. Lazy-initializes the bus on first use.
pub(crate) fn record_orphan_drop(token_id: CapTokenId, cap: SubstrateCap) {
    let mut guard = PROCESS_BUS.lock().expect("process audit bus lock poisoned");
    if guard.is_none() {
        *guard = Some(EnforcementAuditBus::new());
    }
    if let Some(bus) = guard.as_mut() {
        bus.record_orphan_drop(token_id, cap);
    }
}

/// Test-only handle to inspect the process-wide audit-bus.
///
/// Returns the entry count + a vector of (tag, message) pairs since the
/// last call. The bus retains entries (it is append-only) ; this helper
/// exposes them for assertion. Tests using this should use
/// `--test-threads=1` to avoid cross-test interference.
#[cfg(any(test, feature = "test-bypass"))]
#[must_use]
pub fn audit_chain_for_test() -> Vec<(String, String)> {
    let guard = PROCESS_BUS.lock().expect("process audit bus lock poisoned");
    guard.as_ref().map_or_else(Vec::new, |bus| {
        bus.iter()
            .map(|e| (e.tag.clone(), e.message.clone()))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{AuditEvent, EnforcementAuditBus};
    use crate::cap::{CapTokenId, SubstrateCap};
    use crate::consent::ConsentScope;
    use crate::halt::HaltReason;
    use crate::harm::Prohibition;

    #[test]
    fn audit_event_tags_are_stable_strings() {
        // Pin tag strings ; downstream consumers (telemetry exporters) rely
        // on these. Renaming = ABI break per `specs/22_TELEMETRY.csl`.
        assert_eq!(
            AuditEvent::GrantIssued {
                token_id: CapTokenId(0),
                cap: SubstrateCap::OmegaRegister,
                principal: "p".into(),
                purpose: "u".into(),
                test_bypass: false,
            }
            .tag(),
            "h6.grant.issued"
        );
        assert_eq!(
            AuditEvent::GrantDenied {
                cap: SubstrateCap::OmegaRegister,
                principal: "p".into(),
                purpose: "u".into(),
                reason: "r".into(),
            }
            .tag(),
            "h6.grant.denied"
        );
        assert_eq!(
            AuditEvent::Revoked {
                token_id: CapTokenId(0),
                cap: SubstrateCap::OmegaRegister
            }
            .tag(),
            "h6.revoke"
        );
        assert_eq!(
            AuditEvent::OrphanDrop {
                token_id: CapTokenId(0),
                cap: SubstrateCap::OmegaRegister
            }
            .tag(),
            "h6.orphan-drop"
        );
        assert_eq!(
            AuditEvent::HarmCheckFailed {
                prohibition: Prohibition::Harm,
                site: "x".into()
            }
            .tag(),
            "h6.harm.failed"
        );
        assert_eq!(
            AuditEvent::Halted {
                reason: HaltReason::User,
                outstanding_steps: 0
            }
            .tag(),
            "h6.halt"
        );
        assert_eq!(
            AuditEvent::AttestationDrift { site: "y".into() }.tag(),
            "h6.attestation.drift"
        );
    }

    #[test]
    fn bus_record_grant_issued_appends_one_entry() {
        let mut bus = EnforcementAuditBus::new();
        let scope = ConsentScope::for_purpose("test", "system");
        bus.record_grant_issued(CapTokenId(7), SubstrateCap::ObserverShare, &scope, true);
        assert_eq!(bus.entry_count(), 1);
        let entries: Vec<_> = bus.iter().collect();
        assert_eq!(entries[0].tag, "h6.grant.issued");
        assert!(entries[0].message.contains("cap-token#7"));
        assert!(entries[0].message.contains("observer_share"));
    }

    #[test]
    fn bus_record_grant_denied_appends_with_reason() {
        let mut bus = EnforcementAuditBus::new();
        let scope = ConsentScope::for_purpose("p", "system");
        bus.record_grant_denied(SubstrateCap::SavePath, &scope, "denied-by-policy");
        assert_eq!(bus.entry_count(), 1);
        let e = bus.iter().next().unwrap();
        assert!(e.message.contains("denied-by-policy"));
    }

    #[test]
    fn bus_record_revoke_appends_with_token() {
        let mut bus = EnforcementAuditBus::new();
        bus.record_revoke(CapTokenId(99), SubstrateCap::DebugCamera);
        assert_eq!(bus.entry_count(), 1);
        let e = bus.iter().next().unwrap();
        assert!(e.message.contains("cap-token#99"));
        assert!(e.message.contains("debug_camera"));
    }

    #[test]
    fn bus_chain_verifies_after_many_appends() {
        let mut bus = EnforcementAuditBus::new();
        let scope = ConsentScope::for_purpose("p", "system");
        for i in 0..50 {
            bus.record_grant_issued(CapTokenId(i), SubstrateCap::OmegaRegister, &scope, false);
        }
        bus.chain().verify_chain().expect("chain must verify");
        assert_eq!(bus.entry_count(), 50);
    }

    #[test]
    fn bus_records_halt_event() {
        let mut bus = EnforcementAuditBus::new();
        bus.record_halted(HaltReason::User, 3);
        let e = bus.iter().next().unwrap();
        assert_eq!(e.tag, "h6.halt");
        assert!(e.message.contains("user"));
        assert!(e.message.contains("outstanding_steps=3"));
    }

    #[test]
    fn bus_records_harm_check_with_pd_code() {
        let mut bus = EnforcementAuditBus::new();
        bus.record_harm_check_failed(Prohibition::Surveillance, "omega_step.read_sensor");
        let e = bus.iter().next().unwrap();
        assert!(e.message.contains("PD0004"));
        assert!(e.message.contains("omega_step.read_sensor"));
    }

    #[test]
    fn audit_event_messages_are_deterministic() {
        let event_a = AuditEvent::Revoked {
            token_id: CapTokenId(5),
            cap: SubstrateCap::SavePath,
        };
        let event_b = AuditEvent::Revoked {
            token_id: CapTokenId(5),
            cap: SubstrateCap::SavePath,
        };
        assert_eq!(event_a.message(), event_b.message());
    }

    // § T11-D130 — path-hash-only audit-event tests

    #[test]
    fn path_op_event_tag_is_stable() {
        use super::PathOpKind;
        use cssl_telemetry::path_hash::PathHash;
        let event = AuditEvent::PathOp {
            path_hash: PathHash::zero(),
            op: PathOpKind::Open,
            extra: String::new(),
        };
        assert_eq!(event.tag(), "h6.fs.path-op");
    }

    #[test]
    fn path_op_event_message_contains_hash_short_form_and_op_kind() {
        use super::PathOpKind;
        use cssl_telemetry::path_hash::PathHasher;
        let h = PathHasher::from_seed([1u8; 32]).hash_str("/etc/hosts");
        let event = AuditEvent::PathOp {
            path_hash: h,
            op: PathOpKind::Open,
            extra: "bytes=0".to_string(),
        };
        let msg = event.message();
        assert!(msg.contains("path_hash="));
        assert!(msg.contains("..."));
        assert!(msg.contains("kind=fs-open"));
        assert!(msg.contains("bytes=0"));
        // Critical : raw path bytes never appear.
        assert!(!msg.contains("/etc"));
    }

    #[test]
    fn bus_record_path_op_appends_hash_only_entry() {
        use super::PathOpKind;
        use cssl_telemetry::path_hash::PathHasher;
        let mut bus = EnforcementAuditBus::new();
        let h = PathHasher::from_seed([2u8; 32]).hash_str("/var/log/cssl.log");
        bus.record_path_op(h, PathOpKind::Write, "bytes=42")
            .expect("hash-form accepted");
        let e = bus.iter().next().unwrap();
        assert_eq!(e.tag, "h6.fs.path-op");
        assert!(e.message.contains("path_hash="));
        assert!(e.message.contains("kind=fs-write"));
        assert!(!e.message.contains("/var"));
    }

    #[test]
    fn bus_record_path_op_rejects_raw_path_in_extra() {
        use super::PathOpKind;
        use cssl_telemetry::path_hash::PathHasher;
        let mut bus = EnforcementAuditBus::new();
        let h = PathHasher::from_seed([2u8; 32]).hash_str("/foo");
        // Try to leak a raw path through the extra field.
        let r = bus.record_path_op(h, PathOpKind::Open, "leaked=/etc/passwd");
        assert!(r.is_err());
        // Bus is unchanged on rejection.
        assert_eq!(bus.entry_count(), 0);
    }

    #[test]
    fn bus_record_path_op_chain_verifies() {
        use super::PathOpKind;
        use cssl_telemetry::path_hash::PathHasher;
        let mut bus = EnforcementAuditBus::new();
        let hasher = PathHasher::from_seed([3u8; 32]);
        for p in &["/a", "/b", "/c"] {
            let h = hasher.hash_str(p);
            bus.record_path_op(h, PathOpKind::Read, "bytes=4").unwrap();
        }
        bus.chain()
            .verify_chain()
            .expect("path-op chain must verify");
        assert_eq!(bus.entry_count(), 3);
    }

    #[test]
    fn path_op_event_empty_extra_omits_field() {
        use super::PathOpKind;
        use cssl_telemetry::path_hash::PathHasher;
        let h = PathHasher::from_seed([4u8; 32]).hash_str("/tmp/x");
        let event = AuditEvent::PathOp {
            path_hash: h,
            op: PathOpKind::Close,
            extra: String::new(),
        };
        let msg = event.message();
        // Format : `path-op kind=fs-close path_hash=<short>`
        assert!(msg.starts_with("path-op kind=fs-close path_hash="));
    }

    #[test]
    fn path_op_kind_canonical_names_are_stable() {
        use super::PathOpKind;
        assert_eq!(PathOpKind::Open.canonical_name(), "fs-open");
        assert_eq!(PathOpKind::Read.canonical_name(), "fs-read");
        assert_eq!(PathOpKind::Write.canonical_name(), "fs-write");
        assert_eq!(PathOpKind::Close.canonical_name(), "fs-close");
        assert_eq!(PathOpKind::SaveAppend.canonical_name(), "save-append");
    }
}

//! Session — cap-token-bound + handshake protocol per spec § 6.
//!
//! A [`Session`] is one-per-client : the [`McpServer`](crate::server::McpServer)
//! creates a session at `initialize` handshake, binds the client's caps as
//! [`CapWitness`](crate::cap::CapWitness)es, and stores it for the lifetime
//! of the connection. Per spec § 6 the session struct has 7 canonical fields ;
//! we model all 7 here.
//!
//! ## Handshake protocol
//!
//! 1. Client sends `initialize` with optional cap-claims.
//! 2. Server validates principal (transport-derived).
//! 3. Server materializes [`SessionCapSet`] from the claimed caps.
//! 4. Server returns `initialize` response with the session-id +
//!    [`MCP_PROTOCOL_VERSION`](crate::MCP_PROTOCOL_VERSION).
//! 5. Subsequent calls reference this session by id.
//!
//! ## Anti-pattern guard
//!
//! Sessions DO NOT store the original [`Cap<T>`](crate::cap::Cap) — that
//! type is non-Copy + non-Clone and is consumed at session-open. Long-lived
//! proof is a [`CapWitness`] which is Copy + cheap to compare.

use crate::cap::{BiometricInspect, Cap, CapKind, CapWitness, DevMode, RemoteDev};
use crate::error::{McpError, McpResult};

/// Stable identifier for a session. BLAKE3-hashed in production ; here we
/// model a u64 for the skeleton. Drift to BLAKE3 lands when D131 audit-chain
/// is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub u64);

impl SessionId {
    /// Construct from a raw u64. Tests use small constants ; production
    /// derives via the audit-chain counter.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Principal (the entity behind the session). Per spec § 6 the principal
/// is derived from the transport :
///   - stdio  → `DevModeChild` (parent process spawned the engine)
///   - unix   → uid-derived `LocalDev`
///   - ws-loopback → `LocalDev`
///   - ws-non-loopback → `RemoteDev` (only with `Cap<RemoteDev>`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// Engine was spawned as a child process by the dev-tool ;
    /// caps inherit from parent.
    DevModeChild,
    /// Local dev session via unix-socket or ws-loopback.
    LocalDev {
        /// Audit-friendly identifier (e.g. uid-derived label).
        label: String,
    },
    /// Remote dev session ; only valid with `Cap<RemoteDev>` witness.
    RemoteDev {
        /// Audit-friendly bind label (e.g. `"203.0.113.1:443"`).
        bind_label: String,
    },
    /// Apocky-PM authority (signed-token verified). Used for cap-issuance
    /// in production ; stage-0 stub for tests.
    ApockyPM,
}

impl Principal {
    /// Audit-friendly stable label.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::DevModeChild => "DevModeChild",
            Self::LocalDev { label } | Self::RemoteDev { bind_label: label } => label.as_str(),
            Self::ApockyPM => "ApockyPM",
        }
    }
}

/// Cap-witnesses bound to a session. Per spec § 6 we model the 3 caps
/// covered by D229 ; sovereign / telemetry-egress join in Jθ-2..Jθ-8.
///
/// `Option<CapWitness>` represents "claimed?" : `None` = no cap, `Some(w)` =
/// witness materialized at handshake-time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SessionCapSet {
    /// Dev-mode root cap.
    pub dev_mode: Option<CapWitness>,
    /// Non-loopback transport cap.
    pub remote_dev: Option<CapWitness>,
    /// Biometric tool-dispatch cap.
    pub biometric_inspect: Option<CapWitness>,
}

impl SessionCapSet {
    /// Returns true iff the set contains a witness for the given kind.
    #[must_use]
    pub fn has(&self, kind: CapKind) -> bool {
        match kind {
            CapKind::DevMode => self.dev_mode.is_some(),
            CapKind::RemoteDev => self.remote_dev.is_some(),
            CapKind::BiometricInspect => self.biometric_inspect.is_some(),
        }
    }

    /// Bind a `Cap<DevMode>` ; consumes the cap, materializes a witness.
    /// If a witness already exists we return [`McpError::SessionAlreadyInitialized`]
    /// to enforce the consume-once discipline.
    pub fn grant_dev_mode(&mut self, cap: Cap<DevMode>, issued_at: u64) -> McpResult<()> {
        if self.dev_mode.is_some() {
            return Err(McpError::SessionAlreadyInitialized);
        }
        self.dev_mode = Some(cap.into_witness(issued_at));
        Ok(())
    }

    /// Bind a `Cap<RemoteDev>` ; consumes the cap.
    pub fn grant_remote_dev(&mut self, cap: Cap<RemoteDev>, issued_at: u64) -> McpResult<()> {
        if self.remote_dev.is_some() {
            return Err(McpError::SessionAlreadyInitialized);
        }
        self.remote_dev = Some(cap.into_witness(issued_at));
        Ok(())
    }

    /// Bind a `Cap<BiometricInspect>` ; consumes the cap.
    pub fn grant_biometric_inspect(
        &mut self,
        cap: Cap<BiometricInspect>,
        issued_at: u64,
    ) -> McpResult<()> {
        if self.biometric_inspect.is_some() {
            return Err(McpError::SessionAlreadyInitialized);
        }
        self.biometric_inspect = Some(cap.into_witness(issued_at));
        Ok(())
    }

    /// Revoke a witness. Mid-session revocation per spec § 5 (REVOCABILITY).
    pub fn revoke(&mut self, kind: CapKind) {
        match kind {
            CapKind::DevMode => self.dev_mode = None,
            CapKind::RemoteDev => self.remote_dev = None,
            CapKind::BiometricInspect => self.biometric_inspect = None,
        }
    }

    /// Returns true if every kind in `needed` is present.
    #[must_use]
    pub fn covers_all(&self, needed: &[CapKind]) -> bool {
        needed.iter().all(|k| self.has(*k))
    }
}

/// Session struct — the 7 canonical fields per spec § 6.
#[derive(Debug, Clone)]
pub struct Session {
    /// Stable session identifier.
    pub session_id: SessionId,
    /// Principal derived from transport context.
    pub principal: Principal,
    /// Bound capability witnesses.
    pub caps: SessionCapSet,
    /// Per-session log filter level (0 = trace, 5 = error).
    pub log_filter: u8,
    /// Frame number @ session-open.
    pub created_at_frame: u64,
    /// Frame number @ last activity.
    pub last_activity_frame: u64,
    /// Monotonic per-session counter for audit ordering.
    pub audit_seq: u64,
    /// Initialized flag — transitions `false → true` at handshake completion.
    initialized: bool,
}

impl Session {
    /// Construct an un-initialized session. The caller must invoke
    /// [`Session::initialize`] before any tool dispatch.
    #[must_use]
    pub fn new(session_id: SessionId, principal: Principal, created_at_frame: u64) -> Self {
        Self {
            session_id,
            principal,
            caps: SessionCapSet::default(),
            log_filter: 2, // INFO default
            created_at_frame,
            last_activity_frame: created_at_frame,
            audit_seq: 0,
            initialized: false,
        }
    }

    /// Mark the session initialized. Returns
    /// [`McpError::SessionAlreadyInitialized`] on double-init.
    pub fn initialize(&mut self) -> McpResult<()> {
        if self.initialized {
            return Err(McpError::SessionAlreadyInitialized);
        }
        self.initialized = true;
        Ok(())
    }

    /// Returns true iff [`Session::initialize`] has been called.
    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Bump audit-seq + last-activity frame. Called by the dispatch path
    /// after every tool invocation.
    pub fn touch(&mut self, frame_n: u64) {
        self.last_activity_frame = frame_n;
        self.audit_seq = self.audit_seq.saturating_add(1);
    }

    /// Verify the session has every cap in `needed` ; otherwise return
    /// the first missing one as [`McpError::CapDenied`].
    pub fn require_caps(&self, needed: &[CapKind]) -> McpResult<()> {
        for kind in needed {
            if !self.caps.has(*kind) {
                return Err(McpError::CapDenied { needed: *kind });
            }
        }
        Ok(())
    }

    /// Verify the session is initialized + has the needed caps. The
    /// canonical pre-flight for a tool dispatch.
    pub fn require_active(&self, needed: &[CapKind]) -> McpResult<()> {
        if !self.initialized {
            return Err(McpError::SessionNotInitialized);
        }
        self.require_caps(needed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_starts_uninitialized() {
        let s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        assert!(!s.is_initialized());
        assert_eq!(s.session_id, SessionId::new(1));
        assert_eq!(s.audit_seq, 0);
    }

    #[test]
    fn session_initialize_flips_flag() {
        let mut s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        s.initialize().expect("init");
        assert!(s.is_initialized());
    }

    #[test]
    fn session_double_initialize_refused() {
        let mut s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        s.initialize().expect("init");
        let err = s.initialize().unwrap_err();
        assert!(matches!(err, McpError::SessionAlreadyInitialized));
    }

    #[test]
    fn session_touch_bumps_seq() {
        let mut s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        s.touch(42);
        assert_eq!(s.audit_seq, 1);
        assert_eq!(s.last_activity_frame, 42);
        s.touch(100);
        assert_eq!(s.audit_seq, 2);
        assert_eq!(s.last_activity_frame, 100);
    }

    #[test]
    fn cap_set_grant_consumes_cap() {
        let mut caps = SessionCapSet::default();
        let cap = Cap::<DevMode>::for_test();
        caps.grant_dev_mode(cap, 7).expect("grant");
        assert!(caps.has(CapKind::DevMode));
        assert!(!caps.has(CapKind::RemoteDev));
    }

    #[test]
    fn cap_set_double_grant_refused() {
        let mut caps = SessionCapSet::default();
        caps.grant_dev_mode(Cap::<DevMode>::for_test(), 1)
            .expect("first");
        let err = caps
            .grant_dev_mode(Cap::<DevMode>::for_test(), 2)
            .unwrap_err();
        assert!(matches!(err, McpError::SessionAlreadyInitialized));
    }

    #[test]
    fn cap_set_revoke() {
        let mut caps = SessionCapSet::default();
        caps.grant_dev_mode(Cap::<DevMode>::for_test(), 1)
            .expect("grant");
        caps.revoke(CapKind::DevMode);
        assert!(!caps.has(CapKind::DevMode));
    }

    #[test]
    fn cap_set_covers_all() {
        let mut caps = SessionCapSet::default();
        caps.grant_dev_mode(Cap::<DevMode>::for_test(), 1)
            .expect("grant-dm");
        caps.grant_remote_dev(Cap::<RemoteDev>::for_test(), 2)
            .expect("grant-rd");
        assert!(caps.covers_all(&[CapKind::DevMode, CapKind::RemoteDev]));
        assert!(!caps.covers_all(&[CapKind::DevMode, CapKind::BiometricInspect]));
    }

    #[test]
    fn session_require_caps_denies_missing() {
        let s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        let err = s.require_caps(&[CapKind::DevMode]).unwrap_err();
        match err {
            McpError::CapDenied { needed } => assert_eq!(needed, CapKind::DevMode),
            _ => panic!("expected CapDenied"),
        }
    }

    #[test]
    fn session_require_active_denies_uninitialized() {
        let mut s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        s.caps
            .grant_dev_mode(Cap::<DevMode>::for_test(), 1)
            .expect("grant");
        let err = s.require_active(&[CapKind::DevMode]).unwrap_err();
        assert!(matches!(err, McpError::SessionNotInitialized));
    }

    #[test]
    fn principal_label_is_stable() {
        assert_eq!(Principal::DevModeChild.label(), "DevModeChild");
        assert_eq!(Principal::ApockyPM.label(), "ApockyPM");
        assert_eq!(
            Principal::LocalDev {
                label: "uid-1000".to_string()
            }
            .label(),
            "uid-1000"
        );
    }
}

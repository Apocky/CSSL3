//! § cssl-host-config :: sovereignty + safety policy parameters
//!
//! § I> PolicyConfig wires the LoA host's safety + sovereignty knobs :
//! sovereign-cap hex string, audit + telemetry log directories, log-rotation
//! threshold, max in-memory audit-row buffer size, and license-policy choice
//! (Default / AllowUnknown / AllowProprietary / Strict).
//!
//! § validate-rules
//!   sovereign_cap_hex     : non-empty + valid hex (∈ {0-9, a-f, A-F})
//!   audit_log_dir         : non-empty
//!   telemetry_log_dir     : non-empty
//!   autorotate_threshold_mb : > 0
//!   max_in_memory_audit_rows: > 0
//!   license_policy        : enum — no validation (pure choice)
//!
//! § license_policy mapping → cssl-host-license-attribution::LicensePolicy
//! Default            : reject Unknown ; allow ProprietaryRoyaltyFree (LoA-default)
//! AllowUnknown       : permissive — allow Unknown licenses (dev-only)
//! AllowProprietary   : explicitly allow ProprietaryRoyaltyFree (alias of Default)
//! Strict             : OSS-only ; reject Proprietary + Unknown
//!
//! Mapping itself lives in the consumer crate ; this enum is a serializable
//! choice-marker only — keeps cssl-host-config FILE-DISJOINT (no upstream dep
//! on cssl-host-license-attribution).

use serde::{Deserialize, Serialize};

use crate::loader::ConfigErr;

/// § LicensePolicyChoice — config-level enum corresponding to the runtime
/// `LicensePolicy` in cssl-host-license-attribution. Mapped at host wire-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicensePolicyChoice {
    /// § default LoA policy : reject Unknown, allow Proprietary + OSS
    Default,
    /// § dev-only : allow Unknown licenses (NOT for shipped builds)
    AllowUnknown,
    /// § alias of Default — explicit Proprietary opt-in marker
    AllowProprietary,
    /// § OSS-only : reject Proprietary + Unknown
    Strict,
}

impl Default for LicensePolicyChoice {
    fn default() -> Self {
        Self::Default
    }
}

/// § PolicyConfig — typed sovereignty + safety parameters from `loa.config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// § sovereign-capability hex string. 32-byte hex (64 chars) is canonical
    /// LoA shape, but any non-empty hex passes parse-validation here ;
    /// length-checks live in cssl-caps if needed.
    pub sovereign_cap_hex: String,
    /// § directory for structured audit-event logs (JSON-Lines).
    pub audit_log_dir: String,
    /// § directory for telemetry / metrics logs (separate from audit).
    pub telemetry_log_dir: String,
    /// § log-file size threshold (MiB) above which auto-rotate kicks in.
    pub autorotate_threshold_mb: u32,
    /// § max in-memory audit-row buffer before forced flush-to-disk.
    pub max_in_memory_audit_rows: u32,
    /// § license-policy enum — passed to cssl-asset-fetcher policy-gate.
    pub license_policy: LicensePolicyChoice,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            // § placeholder cap — host wire-up replaces with caller-specific
            // sovereign-cap on first run. 64-char zero-hex is structurally
            // valid but caller-must-derive a real one for production.
            sovereign_cap_hex:
                "0000000000000000000000000000000000000000000000000000000000000000".into(),
            audit_log_dir: "./logs/audit".into(),
            telemetry_log_dir: "./logs/telemetry".into(),
            autorotate_threshold_mb: 10,
            max_in_memory_audit_rows: 10_000,
            license_policy: LicensePolicyChoice::Default,
        }
    }
}

impl PolicyConfig {
    /// § validate — returns `ConfigErr::Policy(reason)` on first failing rule.
    pub fn validate(&self) -> Result<(), ConfigErr> {
        if self.sovereign_cap_hex.is_empty() {
            return Err(ConfigErr::Policy("sovereign_cap_hex must be non-empty".into()));
        }
        if !self
            .sovereign_cap_hex
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        {
            return Err(ConfigErr::Policy(format!(
                "sovereign_cap_hex must be hex (0-9 + a-f) ; got {}",
                self.sovereign_cap_hex
            )));
        }
        if self.audit_log_dir.is_empty() {
            return Err(ConfigErr::Policy("audit_log_dir must be non-empty".into()));
        }
        if self.telemetry_log_dir.is_empty() {
            return Err(ConfigErr::Policy(
                "telemetry_log_dir must be non-empty".into(),
            ));
        }
        if self.autorotate_threshold_mb == 0 {
            return Err(ConfigErr::Policy(
                "autorotate_threshold_mb must be > 0".into(),
            ));
        }
        if self.max_in_memory_audit_rows == 0 {
            return Err(ConfigErr::Policy(
                "max_in_memory_audit_rows must be > 0".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // tests intentionally mutate
                                              // a default + re-validate to
                                              // exercise per-field rules.
mod tests {
    use super::*;

    #[test]
    fn default_validates() {
        let cfg = PolicyConfig::default();
        cfg.validate().expect("default PolicyConfig must validate");
        assert_eq!(cfg.sovereign_cap_hex.len(), 64);
        assert_eq!(cfg.audit_log_dir, "./logs/audit");
        assert_eq!(cfg.telemetry_log_dir, "./logs/telemetry");
        assert_eq!(cfg.autorotate_threshold_mb, 10);
        assert_eq!(cfg.max_in_memory_audit_rows, 10_000);
        assert_eq!(cfg.license_policy, LicensePolicyChoice::Default);
    }

    #[test]
    fn bad_hex_rejected() {
        let mut cfg = PolicyConfig::default();
        cfg.sovereign_cap_hex = "not-hex-at-all-zzzz".into();
        assert!(matches!(cfg.validate(), Err(ConfigErr::Policy(_))));

        cfg.sovereign_cap_hex = String::new();
        assert!(matches!(cfg.validate(), Err(ConfigErr::Policy(_))));

        // mixed case ok
        cfg.sovereign_cap_hex = "DeadBeefCafeBabe".into();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn zero_rotate_rejected() {
        let mut cfg = PolicyConfig::default();
        cfg.autorotate_threshold_mb = 0;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Policy(_))));

        cfg.autorotate_threshold_mb = 1;
        cfg.max_in_memory_audit_rows = 0;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Policy(_))));
    }

    #[test]
    fn strict_policy() {
        let mut cfg = PolicyConfig::default();
        cfg.license_policy = LicensePolicyChoice::Strict;
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.license_policy, LicensePolicyChoice::Strict);

        cfg.license_policy = LicensePolicyChoice::AllowUnknown;
        assert!(cfg.validate().is_ok());
        cfg.license_policy = LicensePolicyChoice::AllowProprietary;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = PolicyConfig {
            sovereign_cap_hex: "abcdef0123456789".into(),
            audit_log_dir: "/var/log/loa/audit".into(),
            telemetry_log_dir: "/var/log/loa/telemetry".into(),
            autorotate_threshold_mb: 100,
            max_in_memory_audit_rows: 50_000,
            license_policy: LicensePolicyChoice::Strict,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: PolicyConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back);
    }
}

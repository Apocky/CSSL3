//! § cssl-host-config :: network parameters
//!
//! § I> NetworkConfig holds runtime-tweakable network parameters : MCP server
//! port, localhost-bind-only flag, HTTP capability bitset, remote-companion
//! gate, and Supabase backend credentials. Defaults are DEFAULT-DENY :
//! localhost-only, http_caps = 0, remote-companion off, no Supabase.
//!
//! § validate-rules
//!   mcp_port              : > 1024 (avoid privileged ports)
//!   mcp_bind_localhost_only : bool — no validation
//!   http_caps             : u32 — bitset, no individual-bit validation
//!   allow_remote_companion: bool — no validation
//!   supabase_url + key    : both Some OR both None (consistency rule)
//!
//! § supabase-credentials
//! `supabase_anon_key` is the public-anon JWT — safe to ship in config since
//! Row-Level-Security policies enforce data access. NOT a secret. The
//! service-role key (which IS a secret) is never loaded from this config ;
//! it's read from `LOA_SUPABASE_SERVICE_KEY` env-var by the host crate
//! (defense-in-depth : config-file scrutiny separated from secret-handling).

use serde::{Deserialize, Serialize};

use crate::loader::ConfigErr;

/// § NetworkConfig — typed network parameters loaded from `loa.config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// § MCP server port. Must be > 1024 to avoid privileged-port collision.
    pub mcp_port: u16,
    /// § when true, MCP server binds 127.0.0.1 only ; false enables LAN bind.
    pub mcp_bind_localhost_only: bool,
    /// § HTTP capability bitset — DEFAULT-DENY ; cssl-rt http GET/POST gates
    /// reads its bits via `cssl-caps`. Default 0 forbids all outbound HTTP.
    pub http_caps: u32,
    /// § remote-companion AI gate — when false, companion runs local-only.
    pub allow_remote_companion: bool,
    /// § Supabase project URL (e.g. "https://<ref>.supabase.co") — Option so
    /// LoA-host can run fully-offline when None.
    pub supabase_url: Option<String>,
    /// § Supabase anon-key (public JWT) ; pairs with `supabase_url`.
    /// Both Some or both None — half-set is a config-error.
    pub supabase_anon_key: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mcp_port: 3001,
            mcp_bind_localhost_only: true,
            http_caps: 0,
            allow_remote_companion: false,
            supabase_url: None,
            supabase_anon_key: None,
        }
    }
}

impl NetworkConfig {
    /// § validate — returns `ConfigErr::Network(reason)` on first failing rule.
    pub fn validate(&self) -> Result<(), ConfigErr> {
        if self.mcp_port <= 1024 {
            return Err(ConfigErr::Network(format!(
                "mcp_port must be > 1024 ; got {}",
                self.mcp_port
            )));
        }
        match (&self.supabase_url, &self.supabase_anon_key) {
            (Some(_), Some(_)) | (None, None) => {}
            (Some(_), None) => {
                return Err(ConfigErr::Network(
                    "supabase_url set but supabase_anon_key missing".into(),
                ));
            }
            (None, Some(_)) => {
                return Err(ConfigErr::Network(
                    "supabase_anon_key set but supabase_url missing".into(),
                ));
            }
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
        let cfg = NetworkConfig::default();
        cfg.validate().expect("default NetworkConfig must validate");
        assert_eq!(cfg.mcp_port, 3001);
        assert!(cfg.mcp_bind_localhost_only);
        assert_eq!(cfg.http_caps, 0);
        assert!(!cfg.allow_remote_companion);
        assert!(cfg.supabase_url.is_none());
        assert!(cfg.supabase_anon_key.is_none());
    }

    #[test]
    fn port_in_range() {
        let mut cfg = NetworkConfig::default();
        cfg.mcp_port = 1025;
        assert!(cfg.validate().is_ok());
        cfg.mcp_port = u16::MAX;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn supabase_mismatch_rejected() {
        let mut cfg = NetworkConfig::default();
        cfg.supabase_url = Some("https://x.supabase.co".into());
        cfg.supabase_anon_key = None;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Network(_))));

        cfg.supabase_url = None;
        cfg.supabase_anon_key = Some("anon.jwt.token".into());
        assert!(matches!(cfg.validate(), Err(ConfigErr::Network(_))));

        // both set : ok
        cfg.supabase_url = Some("https://x.supabase.co".into());
        cfg.supabase_anon_key = Some("anon.jwt.token".into());
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn port_zero_rejected() {
        let mut cfg = NetworkConfig::default();
        cfg.mcp_port = 0;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Network(_))));
        // boundary : 1024 still rejected (privileged)
        cfg.mcp_port = 1024;
        assert!(matches!(cfg.validate(), Err(ConfigErr::Network(_))));
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = NetworkConfig {
            mcp_port: 4242,
            mcp_bind_localhost_only: false,
            http_caps: 0b0011,
            allow_remote_companion: true,
            supabase_url: Some("https://foo.supabase.co".into()),
            supabase_anon_key: Some("eyJ.fake.jwt".into()),
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: NetworkConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back);
    }
}

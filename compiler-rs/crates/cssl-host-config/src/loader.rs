//! § cssl-host-config :: top-level loader
//!
//! § I> LoaConfig is the runtime aggregate of {render, network, policy}
//! sub-configs. Loader entrypoints :
//!   - `LoaConfig::default()`         : in-memory defaults
//!   - `LoaConfig::load_from_str()`   : parse JSON string
//!   - `LoaConfig::load_from_file()`  : read + parse `loa.config.json`
//!   - `LoaConfig::save_to_file()`    : write pretty-printed JSON
//!   - `LoaConfig::apply_env_overrides()` : LOA_* env-var overrides
//!   - `LoaConfig::validate()`        : aggregate per-section validation
//!
//! § env-override surface (read at apply_env_overrides()) :
//!   LOA_MCP_PORT             u16  → network.mcp_port
//!   LOA_HDR_EXPOSURE         f32  → render.hdr_exposure
//!   LOA_TARGET_FPS           u32  → render.target_fps
//!   LOA_VSYNC                bool → render.vsync (parses 1/0/true/false)
//!   LOA_HTTP_CAPS            u32  → network.http_caps
//!   LOA_AUDIT_LOG_DIR        str  → policy.audit_log_dir
//!   LOA_SUPABASE_URL         str  → network.supabase_url
//!   LOA_SUPABASE_ANON_KEY    str  → network.supabase_anon_key
//!
//! § validation strategy
//! `validate()` returns `Result<(), Vec<ConfigErr>>` — one entry per failing
//! section. Multi-error display lets the host print all problems @ startup
//! rather than one-at-a-time.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::network::NetworkConfig;
use crate::policy::PolicyConfig;
use crate::render::RenderConfig;

/// § ConfigErr — sectioned error variants. Library does NOT panic ; loader
/// converts io::Error / serde_json::Error into stringified `ConfigErr::Io`
/// / `ConfigErr::JsonParse` so callers can pattern-match without pulling
/// upstream error types into their match-arms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigErr {
    /// § JSON parse-error from serde_json (line/col preserved in inner string).
    JsonParse(String),
    /// § render-section validation failure ; reason is human-readable.
    Render(String),
    /// § network-section validation failure ; reason is human-readable.
    Network(String),
    /// § policy-section validation failure ; reason is human-readable.
    Policy(String),
    /// § io error — file not found, permission denied, etc.
    Io(String),
}

impl std::fmt::Display for ConfigErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JsonParse(s) => write!(f, "JSON parse error : {s}"),
            Self::Render(s) => write!(f, "render config error : {s}"),
            Self::Network(s) => write!(f, "network config error : {s}"),
            Self::Policy(s) => write!(f, "policy config error : {s}"),
            Self::Io(s) => write!(f, "io error : {s}"),
        }
    }
}

impl std::error::Error for ConfigErr {}

/// § LoaConfig — top-level runtime config aggregate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LoaConfig {
    /// § render parameters (window, MSAA, exposure, vsync, etc.)
    #[serde(default)]
    pub render: RenderConfig,
    /// § network parameters (MCP port, HTTP caps, Supabase, etc.)
    #[serde(default)]
    pub network: NetworkConfig,
    /// § sovereignty + safety policy parameters
    #[serde(default)]
    pub policy: PolicyConfig,
}

impl LoaConfig {
    /// § validate — aggregates per-section errors into a `Vec<ConfigErr>`.
    pub fn validate(&self) -> Result<(), Vec<ConfigErr>> {
        let mut errors = Vec::new();
        if let Err(e) = self.render.validate() {
            errors.push(e);
        }
        if let Err(e) = self.network.validate() {
            errors.push(e);
        }
        if let Err(e) = self.policy.validate() {
            errors.push(e);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// § load_from_str — parse a JSON config string. Returns `JsonParse`
    /// error variant on serde_json failure ; does NOT call `validate()`
    /// (caller decides when to validate).
    pub fn load_from_str(json: &str) -> Result<Self, ConfigErr> {
        serde_json::from_str(json).map_err(|e| ConfigErr::JsonParse(e.to_string()))
    }

    /// § load_from_file — read JSON from disk + parse. Returns `Io` on
    /// read-failure or `JsonParse` on parse-failure.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigErr> {
        let bytes = fs::read_to_string(path).map_err(|e| ConfigErr::Io(e.to_string()))?;
        Self::load_from_str(&bytes)
    }

    /// § save_to_file — pretty-print JSON to disk. Standard io::Result so
    /// callers can use `?` directly.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("serde_json : {e}"))
        })?;
        let mut f = fs::File::create(path)?;
        f.write_all(json.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    }

    /// § apply_env_overrides — read `LOA_*` env-vars + override matching fields.
    /// Malformed env-var values are silently ignored (host can re-validate
    /// after override). Caller invokes `validate()` afterward to catch any
    /// resulting inconsistency.
    pub fn apply_env_overrides(&mut self) {
        if let Ok(s) = std::env::var("LOA_MCP_PORT") {
            if let Ok(p) = s.parse::<u16>() {
                self.network.mcp_port = p;
            }
        }
        if let Ok(s) = std::env::var("LOA_HDR_EXPOSURE") {
            if let Ok(v) = s.parse::<f32>() {
                self.render.hdr_exposure = v;
            }
        }
        if let Ok(s) = std::env::var("LOA_TARGET_FPS") {
            if let Ok(v) = s.parse::<u32>() {
                self.render.target_fps = v;
            }
        }
        if let Ok(s) = std::env::var("LOA_VSYNC") {
            self.render.vsync = match s.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => true,
                "0" | "false" | "no" | "off" => false,
                _ => self.render.vsync,
            };
        }
        if let Ok(s) = std::env::var("LOA_HTTP_CAPS") {
            if let Ok(v) = s.parse::<u32>() {
                self.network.http_caps = v;
            }
        }
        if let Ok(s) = std::env::var("LOA_AUDIT_LOG_DIR") {
            if !s.is_empty() {
                self.policy.audit_log_dir = s;
            }
        }
        if let Ok(s) = std::env::var("LOA_SUPABASE_URL") {
            if !s.is_empty() {
                self.network.supabase_url = Some(s);
            }
        }
        if let Ok(s) = std::env::var("LOA_SUPABASE_ANON_KEY") {
            if !s.is_empty() {
                self.network.supabase_anon_key = Some(s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § test-helper : generate a unique temp-file path for round-trip tests
    /// without pulling in `tempfile` crate. Uses process-id + nanosecond clock
    /// + sequence-counter for collision-resistance under parallel `cargo test`.
    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let mut p = std::env::temp_dir();
        p.push(format!("cssl-host-config-{label}-{pid}-{nanos}-{seq}.json"));
        p
    }

    #[test]
    fn default_loads() {
        let cfg = LoaConfig::default();
        cfg.validate().expect("default LoaConfig must validate");
        assert_eq!(cfg.render.resolution, (1920, 1080));
        assert_eq!(cfg.network.mcp_port, 3001);
        assert_eq!(cfg.policy.autorotate_threshold_mb, 10);
    }

    #[test]
    fn valid_json_parses() {
        let cfg = LoaConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back = LoaConfig::load_from_str(&json).expect("parse");
        assert_eq!(cfg, back);
        back.validate().expect("loaded default must validate");
    }

    #[test]
    fn invalid_json_rejected() {
        let bad = "{ this is not json at all";
        let res = LoaConfig::load_from_str(bad);
        assert!(matches!(res, Err(ConfigErr::JsonParse(_))));
    }

    #[test]
    fn file_roundtrip() {
        let path = unique_temp_path("file_roundtrip");
        let cfg = LoaConfig {
            render: RenderConfig {
                resolution: (2560, 1440),
                msaa_samples: 8,
                ..RenderConfig::default()
            },
            network: NetworkConfig {
                mcp_port: 4242,
                ..NetworkConfig::default()
            },
            ..LoaConfig::default()
        };
        cfg.save_to_file(&path).expect("save");
        let back = LoaConfig::load_from_file(&path).expect("load");
        assert_eq!(cfg, back);
        // cleanup ; tolerate failure on locked-fs
        let _ = std::fs::remove_file(&path);
    }

    /// § test-helper : guard env-var mutation so parallel tests don't race.
    /// std::env::{set_var, remove_var} is process-global → tests that mutate
    /// LOA_* vars hold this mutex so they observe a consistent snapshot.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, PoisonError};
        static GUARD: Mutex<()> = Mutex::new(());
        GUARD.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn env_override_applies() {
        let _g = env_guard();
        let saved_port = std::env::var("LOA_MCP_PORT").ok();
        let saved_fps = std::env::var("LOA_TARGET_FPS").ok();
        let saved_vsync = std::env::var("LOA_VSYNC").ok();

        // SAFETY : env mutation is process-global ; env_guard() serializes
        // the test ; std::env::set_var is unsafe in 2024-edition but our MSRV
        // is 2021-edition where it is safe.
        std::env::set_var("LOA_MCP_PORT", "5555");
        std::env::set_var("LOA_TARGET_FPS", "120");
        std::env::set_var("LOA_VSYNC", "false");

        let mut cfg = LoaConfig::default();
        cfg.apply_env_overrides();
        assert_eq!(cfg.network.mcp_port, 5555);
        assert_eq!(cfg.render.target_fps, 120);
        assert!(!cfg.render.vsync);
        cfg.validate().expect("post-override must validate");

        // restore
        match saved_port {
            Some(v) => std::env::set_var("LOA_MCP_PORT", v),
            None => std::env::remove_var("LOA_MCP_PORT"),
        }
        match saved_fps {
            Some(v) => std::env::set_var("LOA_TARGET_FPS", v),
            None => std::env::remove_var("LOA_TARGET_FPS"),
        }
        match saved_vsync {
            Some(v) => std::env::set_var("LOA_VSYNC", v),
            None => std::env::remove_var("LOA_VSYNC"),
        }
    }

    #[test]
    fn validation_multi_errors() {
        let cfg = LoaConfig {
            render: RenderConfig {
                resolution: (0, 0),
                ..RenderConfig::default()
            },
            network: NetworkConfig {
                mcp_port: 0,
                ..NetworkConfig::default()
            },
            policy: PolicyConfig {
                sovereign_cap_hex: String::new(),
                ..PolicyConfig::default()
            },
        };
        let res = cfg.validate();
        match res {
            Err(errs) => {
                assert_eq!(errs.len(), 3, "expected 3 errors ; got {}", errs.len());
                assert!(matches!(errs[0], ConfigErr::Render(_)));
                assert!(matches!(errs[1], ConfigErr::Network(_)));
                assert!(matches!(errs[2], ConfigErr::Policy(_)));
            }
            Ok(()) => panic!("expected multi-error validation failure"),
        }
    }

    #[test]
    fn save_pretty_format() {
        let path = unique_temp_path("save_pretty");
        let cfg = LoaConfig::default();
        cfg.save_to_file(&path).expect("save");
        let raw = std::fs::read_to_string(&path).expect("read");
        // pretty-print = multi-line + 2-space indent on inner fields
        assert!(raw.contains('\n'), "pretty-print must contain newlines");
        assert!(
            raw.contains("  \"render\""),
            "pretty-print must indent top-level keys ; got :\n{raw}"
        );
        let _ = std::fs::remove_file(&path);
    }
}

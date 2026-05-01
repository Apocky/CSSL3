//! § wired_config — wrapper around `cssl-host-config`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the typed JSON config loader + sub-configs (render +
//!   network + policy) so MCP tools can surface the default LoaConfig as
//!   pretty JSON without each call-site reaching into the path-dep.
//!
//! § wrapped surface
//!   - [`LoaConfig`] / [`ConfigErr`] — top-level typed config + error.
//!   - [`RenderConfig`] / [`NetworkConfig`] / [`PolicyConfig`] /
//!     [`LicensePolicyChoice`] — sub-configs.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; reads-only on disk.

pub use cssl_host_config::{
    ConfigErr, LicensePolicyChoice, LoaConfig, NetworkConfig, PolicyConfig, RenderConfig,
};

/// Convenience : load `path` if it exists, otherwise return the default
/// config. Mirrors the bootstrap-step "either user-config or defaults" path.
pub fn load_or_default(path: impl AsRef<std::path::Path>) -> LoaConfig {
    LoaConfig::load_from_file(path).unwrap_or_default()
}

/// Convenience : default LoaConfig serialized as pretty JSON. Used by the
/// `config.default_json` MCP tool to surface the canonical defaults to
/// downstream tooling.
#[must_use]
pub fn default_pretty_json() -> String {
    let cfg = LoaConfig::default();
    serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pretty_json_is_well_formed_object() {
        let s = default_pretty_json();
        let parsed: serde_json::Value =
            serde_json::from_str(&s).expect("default_pretty_json must parse");
        assert!(parsed.is_object());
        // Must contain the three canonical sections.
        assert!(parsed.get("render").is_some(), "render section missing");
        assert!(parsed.get("network").is_some(), "network section missing");
        assert!(parsed.get("policy").is_some(), "policy section missing");
    }

    #[test]
    fn load_or_default_missing_path_returns_default() {
        let cfg = load_or_default("non-existent-loa-host-wired-config-path-xyz");
        // Default config validates by construction.
        cfg.validate().expect("default config validates");
    }
}

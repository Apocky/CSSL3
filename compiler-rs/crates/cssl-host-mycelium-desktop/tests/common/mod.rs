//! § shared test helpers — Mode-C (substrate-only) `MyceliumApp` factory.

#![allow(dead_code, unreachable_pub, clippy::redundant_pub_crate)]

use cssl_host_mycelium_desktop::{AppConfig, CapMode, LlmMode, MyceliumApp};

/// Construct an `AppConfig` shaped for fast offline tests : Mode-C bridge,
/// `simulate_delay = false`, sovereign-master cap-mode (so all 12 tools
/// are auto-approved without prompting).
pub(crate) fn fast_test_config() -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.llm.mode = LlmMode::SubstrateOnly;
    cfg.llm.simulate_delay = false;
    cfg.caps = CapMode::SovereignMaster;
    cfg.knowledge_top_k = 3;
    cfg.context_token_budget = 5_000;
    cfg
}

/// `AppConfig` shaped as the default user (mutating tools require approval).
pub(crate) fn default_user_config() -> AppConfig {
    let mut cfg = fast_test_config();
    cfg.caps = CapMode::Default;
    cfg
}

/// Construct a fully-stubbed `MyceliumApp` with sovereign-master caps.
pub(crate) fn make_app() -> MyceliumApp {
    MyceliumApp::new(fast_test_config()).expect("app construction")
}

/// Construct a fully-stubbed `MyceliumApp` with default-user caps.
pub(crate) fn make_default_user_app() -> MyceliumApp {
    MyceliumApp::new(default_user_config()).expect("app construction")
}

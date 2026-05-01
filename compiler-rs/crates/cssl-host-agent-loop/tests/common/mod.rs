//! § shared test helpers — substrate-only bridge wiring + standard handlers.
//!
//! § Note : each integration-test binary `#[path]`-includes this module
//! privately ; helpers used by some binaries but not others trigger
//! dead_code, and `pub(crate)` inside a private module triggers
//! `redundant_pub_crate`. Both are non-issues for shared-helpers.

#![allow(dead_code, unreachable_pub, clippy::redundant_pub_crate)]

use std::sync::Arc;

use cssl_host_agent_loop::{
    make_bridge, AgentLoop, AuditPort, CapBits, LlmBridge, LlmConfig, LlmMode, ToolCaps,
    ToolHandlers, VecAuditPort,
};

/// Construct a Mode-C `SubstrateBridge` with `simulate_delay = false` so
/// tests run in microseconds.
pub(crate) fn fast_substrate_bridge() -> Box<dyn LlmBridge> {
    let cfg = LlmConfig {
        mode: LlmMode::SubstrateOnly,
        simulate_delay: false,
        ..LlmConfig::default()
    };
    make_bridge(&cfg, CapBits::substrate_only()).expect("substrate bridge")
}

/// Construct a fully-stubbed agent loop : Mode-C bridge + sovereign-master
/// caps + null handlers + collecting audit-port.
pub(crate) fn make_loop_sovereign() -> (AgentLoop, Arc<VecAuditPort>) {
    let bridge = fast_substrate_bridge();
    let caps = ToolCaps::sovereign_master();
    let tools = ToolHandlers::null();
    let audit = Arc::new(VecAuditPort::new());
    let port: Arc<dyn AuditPort> = audit.clone();
    (AgentLoop::new(bridge, caps, tools, port), audit)
}

/// Construct a default-user agent loop : Mode-C bridge + default caps +
/// null handlers + collecting audit-port.
pub(crate) fn make_loop_default() -> (AgentLoop, Arc<VecAuditPort>) {
    let bridge = fast_substrate_bridge();
    let caps = ToolCaps::default_user();
    let tools = ToolHandlers::null();
    let audit = Arc::new(VecAuditPort::new());
    let port: Arc<dyn AuditPort> = audit.clone();
    (AgentLoop::new(bridge, caps, tools, port), audit)
}

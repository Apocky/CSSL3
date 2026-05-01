//! § caps tests — sovereign vs default vs paranoid + bypass-record.

use cssl_host_agent_loop::{CapDecision, CapMode, ToolCaps, ToolName};

#[test]
fn sovereign_master_allows_all() {
    let caps = ToolCaps::sovereign_master();
    assert_eq!(caps.mode, CapMode::SovereignMaster);
    for tool in ToolCaps::all_tools() {
        assert_eq!(
            caps.check(tool),
            CapDecision::Allow,
            "sovereign should allow {tool:?}"
        );
    }
}

#[test]
fn default_user_auto_approves_reads() {
    let caps = ToolCaps::default_user();
    for read in [
        ToolName::FileRead,
        ToolName::SpecQuery,
        ToolName::WebSearch,
        ToolName::OllamaChat,
        ToolName::AnthropicMessages,
    ] {
        assert_eq!(caps.check(read), CapDecision::Allow, "{read:?}");
    }
}

#[test]
fn default_user_requires_approval_for_writes() {
    let caps = ToolCaps::default_user();
    for w in [
        ToolName::FileWrite,
        ToolName::FileEdit,
        ToolName::Bash,
        ToolName::GitCommit,
        ToolName::McpCall,
        ToolName::VercelDeploy,
    ] {
        assert_eq!(caps.check(w), CapDecision::RequireApproval, "{w:?}");
    }
}

#[test]
fn paranoid_denies_writes() {
    let caps = ToolCaps::paranoid();
    assert_eq!(caps.mode, CapMode::Paranoid);
    for w in [
        ToolName::FileWrite,
        ToolName::FileEdit,
        ToolName::Bash,
        ToolName::GitCommit,
        ToolName::VercelDeploy,
    ] {
        assert_eq!(caps.check(w), CapDecision::Deny, "{w:?}");
    }
    // Reads still allow.
    assert_eq!(caps.check(ToolName::FileRead), CapDecision::Allow);
    assert_eq!(caps.check(ToolName::SpecQuery), CapDecision::Allow);
}

#[test]
fn paranoid_denies_git_push() {
    let caps = ToolCaps::paranoid();
    assert_eq!(caps.check(ToolName::GitPush), CapDecision::Deny);
    assert_eq!(caps.check(ToolName::GitCommit), CapDecision::Deny);
}

#[test]
fn sovereign_bypass_recorded() {
    let caps = ToolCaps::sovereign_master();
    let rec = caps.record_sovereign_bypass(ToolName::Bash);
    assert_eq!(rec.tool, ToolName::Bash);
    assert_eq!(rec.reason, "sovereign_master_default_allow");
    // Recorded timestamp should be in the past relative to "now".
    let now = cssl_host_agent_loop::now_unix();
    assert!(rec.recorded_unix <= now);

    // Default + paranoid produce different reason strings.
    let d = ToolCaps::default_user().record_sovereign_bypass(ToolName::FileWrite);
    assert_eq!(d.reason, "manual_user_override");
    let p = ToolCaps::paranoid().record_sovereign_bypass(ToolName::FileWrite);
    assert_eq!(p.reason, "explicit_paranoid_override");
}

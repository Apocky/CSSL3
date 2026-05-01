//! § tools tests — dispatch + cap-gate + port routing.

use std::sync::Arc;

use cssl_host_agent_loop::{
    dispatch, BashOutput, BashPort, FilePort, GitPort, McpPort, MemFilePort, NullBashPort,
    NullGitPort, NullMcpPort, NullVercelPort, NullWebSearchPort, ToolCaps, ToolError,
    ToolHandlers, ToolName, WebSearchHit, WebSearchPort,
};

#[path = "common/mod.rs"]
mod common;

// ── tracking ports ──────────────────────────────────────────────────

#[derive(Default)]
struct TrackBash {
    last: std::sync::Mutex<Option<String>>,
}

impl BashPort for TrackBash {
    fn run(&self, command: &str) -> Result<BashOutput, ToolError> {
        *self.last.lock().unwrap() = Some(command.into());
        Ok(BashOutput {
            stdout: format!("ran: {command}"),
            stderr: String::new(),
            exit_code: 0,
        })
    }
}

#[derive(Default)]
struct TrackGit {
    commits: std::sync::Mutex<Vec<String>>,
}

impl GitPort for TrackGit {
    fn commit(&self, message: &str, _paths: &[String]) -> Result<String, ToolError> {
        self.commits.lock().unwrap().push(message.into());
        Ok("deadbeef".into())
    }
    fn push(&self, _remote: &str, _branch: &str) -> Result<(), ToolError> {
        Ok(())
    }
}

#[derive(Default)]
struct TrackMcp;
impl McpPort for TrackMcp {
    fn call(
        &self,
        tool: &str,
        _input: &serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        Ok(serde_json::json!({ "echo": tool }))
    }
}

#[derive(Default)]
struct TrackWeb;
impl WebSearchPort for TrackWeb {
    fn search(&self, query: &str) -> Result<Vec<WebSearchHit>, ToolError> {
        Ok(vec![WebSearchHit {
            title: format!("hit:{query}"),
            url: "https://example.com".into(),
            snippet: "snippet".into(),
        }])
    }
}

fn handlers_with(file: Arc<dyn FilePort>, bash: Arc<dyn BashPort>, git: Arc<dyn GitPort>,
                 mcp: Arc<dyn McpPort>, web: Arc<dyn WebSearchPort>) -> ToolHandlers {
    ToolHandlers {
        file,
        bash,
        git,
        mcp,
        vercel: Arc::new(NullVercelPort),
        web,
    }
}

// ── tests ──────────────────────────────────────────────────────────

#[test]
fn file_read_via_mem_port() {
    let mem = Arc::new(MemFilePort::with("/a.txt", "hello"));
    let handlers = handlers_with(
        mem,
        Arc::new(NullBashPort),
        Arc::new(NullGitPort),
        Arc::new(NullMcpPort),
        Arc::new(NullWebSearchPort),
    );
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let out = dispatch(
        &handlers,
        &caps,
        ToolName::FileRead,
        &serde_json::json!({"path": "/a.txt"}),
        bridge.as_ref(),
    )
    .expect("file read");
    assert_eq!(out["content"], "hello");
}

#[test]
fn file_write_denied_under_paranoid() {
    let handlers = ToolHandlers::null();
    let caps = ToolCaps::paranoid();
    let bridge = common::fast_substrate_bridge();
    let r = dispatch(
        &handlers,
        &caps,
        ToolName::FileWrite,
        &serde_json::json!({"path": "/x", "content": "y"}),
        bridge.as_ref(),
    );
    assert!(matches!(r, Err(ToolError::Denied(_))));
}

#[test]
fn bash_dispatch_routes_to_port() {
    let bash = Arc::new(TrackBash::default());
    let bash_dyn: Arc<dyn BashPort> = bash.clone();
    let handlers = handlers_with(
        Arc::new(MemFilePort::new()),
        bash_dyn,
        Arc::new(NullGitPort),
        Arc::new(NullMcpPort),
        Arc::new(NullWebSearchPort),
    );
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let out = dispatch(
        &handlers,
        &caps,
        ToolName::Bash,
        &serde_json::json!({"command": "echo hi"}),
        bridge.as_ref(),
    )
    .unwrap();
    assert_eq!(out["exit_code"], 0);
    assert_eq!(out["stdout"], "ran: echo hi");
    assert_eq!(*bash.last.lock().unwrap(), Some("echo hi".into()));
}

#[test]
fn git_commit_dispatch() {
    let git = Arc::new(TrackGit::default());
    let git_dyn: Arc<dyn GitPort> = git.clone();
    let handlers = handlers_with(
        Arc::new(MemFilePort::new()),
        Arc::new(NullBashPort),
        git_dyn,
        Arc::new(NullMcpPort),
        Arc::new(NullWebSearchPort),
    );
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let out = dispatch(
        &handlers,
        &caps,
        ToolName::GitCommit,
        &serde_json::json!({"message": "test commit", "paths": ["a.txt"]}),
        bridge.as_ref(),
    )
    .unwrap();
    assert_eq!(out["sha"], "deadbeef");
    assert_eq!(git.commits.lock().unwrap().as_slice(), &["test commit"]);
}

#[test]
fn mcp_call_dispatch() {
    let handlers = handlers_with(
        Arc::new(MemFilePort::new()),
        Arc::new(NullBashPort),
        Arc::new(NullGitPort),
        Arc::new(TrackMcp),
        Arc::new(NullWebSearchPort),
    );
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let out = dispatch(
        &handlers,
        &caps,
        ToolName::McpCall,
        &serde_json::json!({"tool": "deploy_to_vercel", "input": {}}),
        bridge.as_ref(),
    )
    .unwrap();
    assert_eq!(out["echo"], "deploy_to_vercel");
}

#[test]
fn web_search_dispatch() {
    let handlers = handlers_with(
        Arc::new(MemFilePort::new()),
        Arc::new(NullBashPort),
        Arc::new(NullGitPort),
        Arc::new(NullMcpPort),
        Arc::new(TrackWeb),
    );
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let out = dispatch(
        &handlers,
        &caps,
        ToolName::WebSearch,
        &serde_json::json!({"query": "rust"}),
        bridge.as_ref(),
    )
    .unwrap();
    let arr = out.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["title"], "hit:rust");
}

#[test]
fn spec_query_uses_substrate_knowledge() {
    let handlers = ToolHandlers::null();
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let out = dispatch(
        &handlers,
        &caps,
        ToolName::SpecQuery,
        &serde_json::json!({"query": "mycelium desktop", "top_k": 3}),
        bridge.as_ref(),
    )
    .unwrap();
    // Hits may be empty if corpus is bare ; the call MUST succeed & return
    // a `hits` array.
    assert!(out["hits"].is_array());
}

#[test]
fn unknown_tool_returns_invalid() {
    // We can't pass an unknown ToolName (enum is closed) ; instead we
    // probe the InvalidInput path : missing required field.
    let handlers = ToolHandlers::null();
    let caps = ToolCaps::sovereign_master();
    let bridge = common::fast_substrate_bridge();
    let r = dispatch(
        &handlers,
        &caps,
        ToolName::FileRead,
        &serde_json::json!({}), // missing "path"
        bridge.as_ref(),
    );
    assert!(matches!(r, Err(ToolError::InvalidInput(_))));
}

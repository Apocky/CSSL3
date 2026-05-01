//! § tools — port traits + dispatch + 11-tool handlers.
//!
//! § Architecture
//!   The loop never touches the real filesystem / git / shell / network
//!   directly. Every effect goes through a port-trait owned by
//!   `ToolHandlers`, which the host wires with concrete impls
//!   (`std::fs`, `git2`, `std::process::Command`, `ureq`, etc.) at
//!   the wave-C integration layer.
//!
//! § In-memory + null ports
//!   For tests + Mode-C self-sufficient defaults we ship :
//!     - `MemFilePort`         — HashMap-backed read/write/edit
//!     - `NullBashPort`        — returns NotImplemented
//!     - `NullGitPort`         — returns Ok with stub-hash / no-op
//!     - `NullMcpPort`         — returns NotImplemented
//!     - `NullVercelPort`      — returns Ok with stub-url
//!     - `NullWebSearchPort`   — returns empty Vec
//!
//! § PRIME-DIRECTIVE
//!   Every dispatcher cap-checks BEFORE invoking the port.
//!   `dispatch` returns `ToolError::Denied` on cap-mismatch ;
//!   the loop runner translates that into an `ApprovalState::Denied`
//!   on the corresponding `ToolCall`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cssl_host_llm_bridge::{LlmBridge, LlmMessage, LlmRole};
use serde::{Deserialize, Serialize};

use crate::caps::{CapDecision, ToolCaps};
use crate::state::ToolName;

/// File-effect port — read / write / edit.
pub trait FilePort: Send + Sync {
    /// Read the entire file at `path`.
    fn read(&self, path: &str) -> Result<String, ToolError>;
    /// Write `content` to `path`, creating-or-truncating.
    fn write(&self, path: &str, content: &str) -> Result<(), ToolError>;
    /// Replace the first occurrence of `old` with `new` in the file at
    /// `path`. Returns the number of replacements made.
    fn edit(&self, path: &str, old: &str, new: &str) -> Result<usize, ToolError>;
}

/// Bash / shell port.
pub trait BashPort: Send + Sync {
    /// Run a command and capture stdout / stderr / exit-code.
    fn run(&self, command: &str) -> Result<BashOutput, ToolError>;
}

/// Bash command output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BashOutput {
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Process exit-code (`0` on success).
    pub exit_code: i32,
}

/// Git port.
pub trait GitPort: Send + Sync {
    /// Create a commit with the given message + paths. Returns the commit
    /// SHA on success.
    fn commit(&self, message: &str, paths: &[String]) -> Result<String, ToolError>;
    /// Push `branch` to `remote`.
    fn push(&self, remote: &str, branch: &str) -> Result<(), ToolError>;
}

/// Generic MCP-tool port.
pub trait McpPort: Send + Sync {
    /// Dispatch the named tool with the given JSON input.
    fn call(&self, tool: &str, input: &serde_json::Value)
        -> Result<serde_json::Value, ToolError>;
}

/// Vercel-deploy port — used by `apocky.com` deploy flow.
pub trait VercelPort: Send + Sync {
    /// Deploy the directory `dir` to the named project. Returns the
    /// deployment URL.
    fn deploy(&self, project: &str, dir: &str) -> Result<String, ToolError>;
}

/// Outbound web-search port.
pub trait WebSearchPort: Send + Sync {
    /// Search and return up to a port-defined number of hits.
    fn search(&self, query: &str) -> Result<Vec<WebSearchHit>, ToolError>;
}

/// A single web-search hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSearchHit {
    /// Page title.
    pub title: String,
    /// Page URL.
    pub url: String,
    /// Short snippet (provider-truncated).
    pub snippet: String,
}

/// Bundle of every port the loop can dispatch through. The host wires
/// concrete impls ; tests wire `Mem*` / `Null*` fakes.
#[derive(Clone)]
pub struct ToolHandlers {
    /// File port.
    pub file: Arc<dyn FilePort>,
    /// Bash port.
    pub bash: Arc<dyn BashPort>,
    /// Git port.
    pub git: Arc<dyn GitPort>,
    /// MCP port.
    pub mcp: Arc<dyn McpPort>,
    /// Vercel port.
    pub vercel: Arc<dyn VercelPort>,
    /// Web-search port.
    pub web: Arc<dyn WebSearchPort>,
}

impl ToolHandlers {
    /// Construct a fully-stubbed handler-set : `MemFilePort` + every other
    /// port as `Null*`. This is the Mode-C self-sufficient default and
    /// the test default.
    #[must_use]
    pub fn null() -> Self {
        Self {
            file: Arc::new(MemFilePort::new()),
            bash: Arc::new(NullBashPort),
            git: Arc::new(NullGitPort),
            mcp: Arc::new(NullMcpPort),
            vercel: Arc::new(NullVercelPort),
            web: Arc::new(NullWebSearchPort),
        }
    }
}

impl std::fmt::Debug for ToolHandlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolHandlers")
            .field("file", &"<dyn FilePort>")
            .field("bash", &"<dyn BashPort>")
            .field("git", &"<dyn GitPort>")
            .field("mcp", &"<dyn McpPort>")
            .field("vercel", &"<dyn VercelPort>")
            .field("web", &"<dyn WebSearchPort>")
            .finish()
    }
}

/// Tool-dispatch error type.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// I/O-layer failure (path-not-found, permission, etc.).
    #[error("io: {0}")]
    Io(String),
    /// Cap-policy denied the tool.
    #[error("denied: {0}")]
    Denied(&'static str),
    /// Input JSON failed schema-validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Concrete impl is not available in the current port.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Top-level dispatcher. Cap-checks first ; then routes to the appropriate
/// port. The bridge is threaded through for the LLM-tool variants
/// (`OllamaChat` / `AnthropicMessages`) which call the bridge's `chat`.
pub fn dispatch(
    handlers: &ToolHandlers,
    caps: &ToolCaps,
    tool: ToolName,
    input: &serde_json::Value,
    bridge: &dyn LlmBridge,
) -> Result<serde_json::Value, ToolError> {
    // ─ cap-gate FIRST. Default-deny posture per PRIME-DIRECTIVE.
    match caps.check(tool) {
        CapDecision::Deny => return Err(ToolError::Denied(tool.as_str())),
        CapDecision::RequireApproval | CapDecision::Allow => {}
    }

    match tool {
        ToolName::FileRead => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("path".into()))?;
            let body = handlers.file.read(path)?;
            Ok(serde_json::json!({ "content": body }))
        }
        ToolName::FileWrite => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("path".into()))?;
            let content = input
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("content".into()))?;
            handlers.file.write(path, content)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        ToolName::FileEdit => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("path".into()))?;
            let old = input
                .get("old")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("old".into()))?;
            let new = input
                .get("new")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("new".into()))?;
            let n = handlers.file.edit(path, old, new)?;
            Ok(serde_json::json!({ "replacements": n }))
        }
        ToolName::Bash => {
            let cmd = input
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("command".into()))?;
            let out = handlers.bash.run(cmd)?;
            Ok(serde_json::to_value(&out).map_err(|e| ToolError::Io(e.to_string()))?)
        }
        ToolName::GitCommit => {
            let msg = input
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("message".into()))?;
            let paths: Vec<String> = input
                .get("paths")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let sha = handlers.git.commit(msg, &paths)?;
            Ok(serde_json::json!({ "sha": sha }))
        }
        ToolName::GitPush => {
            let remote = input
                .get("remote")
                .and_then(|v| v.as_str())
                .unwrap_or("origin");
            let branch = input
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("branch".into()))?;
            handlers.git.push(remote, branch)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        ToolName::McpCall => {
            let name = input
                .get("tool")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("tool".into()))?;
            let inner = input.get("input").cloned().unwrap_or(serde_json::Value::Null);
            handlers.mcp.call(name, &inner)
        }
        ToolName::VercelDeploy => {
            let project = input
                .get("project")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("project".into()))?;
            let dir = input
                .get("dir")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("dir".into()))?;
            let url = handlers.vercel.deploy(project, dir)?;
            Ok(serde_json::json!({ "url": url }))
        }
        ToolName::OllamaChat | ToolName::AnthropicMessages => {
            let prompt = input
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("prompt".into()))?;
            let msgs = vec![LlmMessage::new(LlmRole::User, prompt)];
            let reply = bridge
                .chat(&msgs)
                .map_err(|e| ToolError::Io(format!("llm: {e}")))?;
            Ok(serde_json::json!({ "reply": reply, "mode": bridge.mode().as_str() }))
        }
        ToolName::WebSearch => {
            let query = input
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("query".into()))?;
            let hits = handlers.web.search(query)?;
            Ok(serde_json::to_value(&hits).map_err(|e| ToolError::Io(e.to_string()))?)
        }
        ToolName::SpecQuery => {
            let q = input
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("query".into()))?;
            let top_k = input
                .get("top_k")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(5) as usize;
            let hits = cssl_host_substrate_knowledge::query_relevant(q, top_k);
            let payload: Vec<_> = hits
                .into_iter()
                .map(|(name, score)| serde_json::json!({ "name": name, "score": score }))
                .collect();
            Ok(serde_json::json!({ "hits": payload }))
        }
    }
}

// ── In-memory + null port impls ─────────────────────────────────────────

/// In-memory `FilePort` for tests + Mode-C defaults.
#[derive(Debug, Default)]
pub struct MemFilePort {
    inner: Mutex<HashMap<String, String>>,
}

impl MemFilePort {
    /// Empty filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populate with a single file.
    #[must_use]
    pub fn with(path: &str, content: &str) -> Self {
        let port = Self::new();
        port.inner
            .lock()
            .expect("mem-file mutex poisoned")
            .insert(path.into(), content.into());
        port
    }

    /// Number of files currently held.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("mem-file mutex poisoned").len()
    }

    /// True iff empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl FilePort for MemFilePort {
    fn read(&self, path: &str) -> Result<String, ToolError> {
        self.inner
            .lock()
            .expect("mem-file mutex poisoned")
            .get(path)
            .cloned()
            .ok_or_else(|| ToolError::Io(format!("not found: {path}")))
    }

    fn write(&self, path: &str, content: &str) -> Result<(), ToolError> {
        self.inner
            .lock()
            .expect("mem-file mutex poisoned")
            .insert(path.into(), content.into());
        Ok(())
    }

    // We deliberately hold the lock for the whole edit so the
    // read-modify-write is atomic ; clippy's significant-drop tightening
    // lint flags this — allow at function-level.
    #[allow(clippy::significant_drop_tightening)]
    fn edit(&self, path: &str, old: &str, new: &str) -> Result<usize, ToolError> {
        let mut map = self.inner.lock().expect("mem-file mutex poisoned");
        let entry = map
            .get_mut(path)
            .ok_or_else(|| ToolError::Io(format!("not found: {path}")))?;
        entry.find(old).map_or(Ok(0), |idx| {
            entry.replace_range(idx..idx + old.len(), new);
            Ok(1)
        })
    }
}

/// `BashPort` that always returns `NotImplemented`. Default for tests
/// + Mode-C — wave-C wires real `std::process::Command`.
#[derive(Debug, Default)]
pub struct NullBashPort;

impl BashPort for NullBashPort {
    fn run(&self, _command: &str) -> Result<BashOutput, ToolError> {
        Err(ToolError::NotImplemented("bash"))
    }
}

/// `GitPort` that returns a deterministic stub-sha for commits and a
/// no-op success for pushes. Used by tests + Mode-C.
#[derive(Debug, Default)]
pub struct NullGitPort;

impl GitPort for NullGitPort {
    fn commit(&self, _message: &str, _paths: &[String]) -> Result<String, ToolError> {
        Ok("0000000000000000000000000000000000000000".into())
    }
    fn push(&self, _remote: &str, _branch: &str) -> Result<(), ToolError> {
        Ok(())
    }
}

/// `McpPort` that always returns `NotImplemented`. Wave-C wires the
/// concrete MCP-client.
#[derive(Debug, Default)]
pub struct NullMcpPort;

impl McpPort for NullMcpPort {
    fn call(
        &self,
        _tool: &str,
        _input: &serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::NotImplemented("mcp"))
    }
}

/// `VercelPort` that returns a deterministic stub-URL.
#[derive(Debug, Default)]
pub struct NullVercelPort;

impl VercelPort for NullVercelPort {
    fn deploy(&self, project: &str, _dir: &str) -> Result<String, ToolError> {
        Ok(format!("https://{project}.vercel.app"))
    }
}

/// `WebSearchPort` that returns an empty hit-list. Used in tests +
/// privacy-respecting Mode-C defaults.
#[derive(Debug, Default)]
pub struct NullWebSearchPort;

impl WebSearchPort for NullWebSearchPort {
    fn search(&self, _query: &str) -> Result<Vec<WebSearchHit>, ToolError> {
        Ok(Vec::new())
    }
}

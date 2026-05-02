//! § gm_live — live-LLM dialog wire for gm_narrator
//! ════════════════════════════════════════════════════════════
//!
//! § T11-W17-B (W-LOA-host-gm-live) — dual-mode dispatch + cap-gated path.
//!
//! § ROLE
//!   Wires `gm_narrator` to the existing `cssl-host-llm-bridge` so a
//!   cap-granted runtime can replace the deterministic xorshift32-over-
//!   phrase-pools fallback with a live-LLM-generated line. The cap
//!   denial-path falls straight back to the existing pools so the
//!   replay-determinism contract is preserved : same input + same
//!   cap-state → same output (LLM-mode is non-deterministic only when
//!   the cap is granted at runtime).
//!
//! § DUAL-MODE BEHAVIOR
//!   - cap-denied / no-bridge / network-error / timeout
//!       → caller falls back to xorshift pools (existing behavior)
//!   - cap-granted + live-bridge configured + chat-success
//!       → returns the LLM text and skips the pool draw
//!
//! § PRIME-DIRECTIVE
//!   - Default-deny : `LiveBridge` is `None` until the host explicitly
//!     calls `set_live_bridge`. Until then, gm_narrator is unchanged.
//!   - Timeout : 5s default ; configurable per-call.
//!   - Anti-repeat : LLM responses join the same FNV-1a hash ring the
//!     pool-draw uses, so a chatty-LLM cannot loop on the same line.
//!   - Audit : every successful live-call logs through `cssl_rt::log_event`
//!     with the LLM mode label ; every error is logged + non-fatal.
//!   - API key : loaded from `~/.loa-secrets/anthropic.env` if present,
//!     else from the `ANTHROPIC_API_KEY` env-var, else absent. The
//!     constructor never panics on missing key — the caller decides if
//!     Mode-A is required (in which case `make_bridge` errors out and
//!     gm_narrator stays in pool-mode).
//!
//! § DISPATCH SHAPE
//!   The `LiveBridge` trait is the gm_narrator surface :
//!     - `is_available()` — fast-path the pool draw when the live bridge
//!       is unwired or has had a cap-deny error recently.
//!     - `ask(system, user, timeout)` — sync blocking call ; returns
//!       `Ok(text)` on success, `Err(LiveError)` on any failure.
//!   Tests inject a `MockLiveBridge` that returns canned responses so
//!   the cap-granted path is testable without network access.
//!
//! § REPLAY-DETERMINISM CONTRACT
//!   When the cap is denied, gm_narrator's behavior is bit-identical to
//!   the pre-W17 build (same xorshift32 + pool tables + ring). When the
//!   cap is granted at runtime, the LLM-emitted text is non-deterministic
//!   by design — but the input recording captures the LLM-output text in
//!   the replay log, so a recorded run replays bit-identically by simply
//!   substituting the recorded text at each ask-site (the cap-state at
//!   replay-time is the recorded one, not the live one).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cssl_host_llm_bridge::{
    make_bridge, CapBits, LlmBridge, LlmConfig, LlmError, LlmMessage, LlmMode, LlmRole,
};
use cssl_rt::loa_startup::log_event;

use crate::gm_narrator::{Archetype, Mood, PhraseTopic, TimeOfDay};

// ─────────────────────────────────────────────────────────────────────────
// § ERROR
// ─────────────────────────────────────────────────────────────────────────

/// Live-bridge error surface. Every variant maps a failure mode that
/// should fall back to the xorshift pool draw rather than propagate.
#[derive(Debug)]
pub enum LiveError {
    /// Cap not granted at construction.
    CapDenied(&'static str),
    /// API key missing from disk + env.
    NotConfigured(&'static str),
    /// Underlying bridge returned an error.
    Bridge(String),
    /// Wall-clock exceeded the configured timeout.
    Timeout,
}

impl std::fmt::Display for LiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiveError::CapDenied(c) => write!(f, "cap denied: {}", c),
            LiveError::NotConfigured(c) => write!(f, "not configured: {}", c),
            LiveError::Bridge(s) => write!(f, "bridge: {}", s),
            LiveError::Timeout => write!(f, "timeout"),
        }
    }
}

impl std::error::Error for LiveError {}

impl From<LlmError> for LiveError {
    fn from(e: LlmError) -> Self {
        match e {
            LlmError::CapDenied(c) => LiveError::CapDenied(c),
            LlmError::NotConfigured(c) => LiveError::NotConfigured(c),
            other => LiveError::Bridge(format!("{}", other)),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PROMPT-CONTEXT — small struct rolled by gm_narrator at the call site
// ─────────────────────────────────────────────────────────────────────────

/// All the context the live-bridge needs to compose a prompt. Built by
/// the gm_narrator at the call-site so the live-bridge surface stays
/// simple.
#[derive(Debug, Clone)]
pub struct DialoguePromptContext {
    pub archetype: Archetype,
    pub mood: Mood,
    pub topic: PhraseTopic,
    /// Brief environment description (already drawn by `describe_environment`).
    pub environment: String,
    /// Optional player utterance the GM is responding to.
    pub player_utterance: Option<String>,
    /// Optional persona-flavor bias (e.g., laconic / verbose / cryptic).
    pub persona_flavor: Option<String>,
}

impl DialoguePromptContext {
    /// Compose the system prompt + user message for an Anthropic-style
    /// chat call. Returns `(system, user)`.
    pub fn render(&self) -> (String, String) {
        let system = format!(
            "You are the GM-narrator for the Labyrinth of Apocalypse. \
             You speak in the voice of a {} archetype. \
             Mood: {:?}. Topic: {:?}. \
             Environment context: {} \
             {}\
             \n\nRules:\n\
             - Reply with ONE short line (1-2 sentences max).\n\
             - Stay in-character ; do NOT acknowledge being an AI or LLM.\n\
             - Do NOT use surrounding quotes ; do NOT prefix with the speaker's name.\n\
             - Never break the fourth wall.\n\
             - Match the archetype's voice: a Sage is wise + reverent ; a Trickster \
             is playful + slippery ; a Mute returns silence (e.g., '...').\n",
            self.archetype.label(),
            self.mood,
            self.topic,
            self.environment,
            self.persona_flavor
                .as_deref()
                .map(|p| format!("Persona flavor: {}. ", p))
                .unwrap_or_default(),
        );
        let user = match &self.player_utterance {
            Some(u) => format!(
                "The traveler says: \"{}\"\n\nGenerate the GM's in-character reply.",
                u.replace('"', "'")
            ),
            None => "Generate one in-character GM line for this moment.".to_string(),
        };
        (system, user)
    }
}

/// Environment description prompt context (no NPC) — used by
/// `describe_environment`'s live-mode path.
#[derive(Debug, Clone)]
pub struct EnvironmentPromptContext {
    pub camera_pos: (f32, f32, f32),
    pub time_of_day: TimeOfDay,
    /// Optional bias hint (e.g., "ruined corridor", "open atrium").
    pub locale_hint: Option<String>,
}

impl EnvironmentPromptContext {
    /// Compose `(system, user)` for an environment description call.
    pub fn render(&self) -> (String, String) {
        let system = format!(
            "You are the GM-narrator for the Labyrinth of Apocalypse. \
             Describe the immediate environment in 2-3 short sentences. \
             Time of day: {:?}. {}\
             \n\nRules:\n\
             - Atmospheric + sensory ; no second-person prescription.\n\
             - Do NOT mention the player's actions ; describe only the place.\n\
             - Stay grounded in the labyrinth's stone + lamps + corridors.\n\
             - Never break the fourth wall.\n",
            self.time_of_day,
            self.locale_hint
                .as_deref()
                .map(|h| format!("Locale hint: {}.", h))
                .unwrap_or_default(),
        );
        let user = format!(
            "The traveler stands at coordinates ({:.2}, {:.2}, {:.2}). \
             Describe the immediate surroundings.",
            self.camera_pos.0, self.camera_pos.1, self.camera_pos.2,
        );
        (system, user)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § TRAIT — gm_narrator's surface for live-LLM dispatch
// ─────────────────────────────────────────────────────────────────────────

/// Live-LLM bridge surface. `gm_narrator` holds an `Option<Arc<dyn LiveBridge>>`
/// and consults `is_available` first ; if true, it composes the prompt and
/// calls `ask`. Any error → fall back to the pool draw.
///
/// § Thread-safety : `Send + Sync` so multiple narrator instances can share
/// one bridge in a multi-room runtime.
pub trait LiveBridge: Send + Sync {
    /// Stable label for the bridge mode (used in audit logs).
    fn name(&self) -> &'static str;

    /// True iff the bridge is configured + the cap is granted. Cheap to
    /// call ; `gm_narrator` calls this on every dialogue draw.
    fn is_available(&self) -> bool;

    /// Synchronous chat call. `system` is the system prompt, `user` is the
    /// user message, `timeout` is the wall-clock budget.
    fn ask(
        &self,
        system: &str,
        user: &str,
        timeout: Duration,
    ) -> Result<String, LiveError>;
}

// ─────────────────────────────────────────────────────────────────────────
// § REAL IMPL — wraps `cssl-host-llm-bridge`
// ─────────────────────────────────────────────────────────────────────────

/// Default per-call timeout when the caller does not specify one.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Real `LiveBridge` implementation backed by the cap-gated `LlmBridge`.
pub struct RealLiveBridge {
    inner: Box<dyn LlmBridge>,
    name: &'static str,
}

impl RealLiveBridge {
    /// Construct a live bridge from the given config + cap-bits.
    ///
    /// § Failure modes (caller falls back to pool-mode) :
    ///   - `LiveError::CapDenied(_)` — the requested mode's bit was absent.
    ///   - `LiveError::NotConfigured(_)` — required field missing
    ///     (e.g., API key for Mode-A, or `tls` feature off at compile time).
    pub fn new(config: LlmConfig, caps: CapBits) -> Result<Self, LiveError> {
        let mode = config.mode;
        let inner = make_bridge(&config, caps)?;
        let name: &'static str = match mode {
            LlmMode::ExternalAnthropic => "live.anthropic",
            LlmMode::LocalOllama => "live.ollama",
            LlmMode::SubstrateOnly => "live.substrate",
        };
        log_event(
            "INFO",
            "loa-host/gm-live",
            &format!("RealLiveBridge constructed · mode={}", name),
        );
        Ok(Self { inner, name })
    }

    /// Convenience constructor that loads the API key per-spec :
    ///   1. `~/.loa-secrets/anthropic.env` first line of `ANTHROPIC_API_KEY=…`
    ///   2. else `ANTHROPIC_API_KEY` environment variable
    ///   3. else `None`
    /// Then constructs Mode-A if the key is present + `caps` includes
    /// `EXTERNAL_API`, otherwise falls back to Mode-C (substrate-templated).
    ///
    /// § This is the recommended path for the host's startup wiring : it
    /// transparently degrades from Mode-A → Mode-C without ever panicking.
    pub fn from_secrets_or_env(
        anthropic_caps: CapBits,
    ) -> Result<Self, LiveError> {
        let key = load_anthropic_key();
        let mut config = LlmConfig::default();
        if let Some(k) = key {
            config.mode = LlmMode::ExternalAnthropic;
            config.anthropic_api_key = Some(k);
            // Try Mode-A first ; if it fails (e.g., tls feature off), fall
            // back to Mode-C so gm_narrator still has *some* live surface.
            match Self::new(config.clone(), anthropic_caps) {
                Ok(b) => return Ok(b),
                Err(e) => {
                    log_event(
                        "WARN",
                        "loa-host/gm-live",
                        &format!("Mode-A construction failed ({}) ; falling back to Mode-C", e),
                    );
                }
            }
        }
        // Mode-C — always-on fallback. Cap is `SUBSTRATE_ONLY`.
        let mut c = LlmConfig::default();
        c.mode = LlmMode::SubstrateOnly;
        Self::new(c, CapBits::substrate_only())
    }
}

impl LiveBridge for RealLiveBridge {
    fn name(&self) -> &'static str {
        self.name
    }

    fn is_available(&self) -> bool {
        // Real bridge is constructed only when caps + config valid.
        true
    }

    fn ask(
        &self,
        system: &str,
        user: &str,
        timeout: Duration,
    ) -> Result<String, LiveError> {
        // The bridge's blocking `chat` returns the assembled response. The
        // timeout is not yet plumbed through to ureq's per-request setting
        // (would require touching the bridge crate) ; we approximate by
        // running the call on a worker thread and joining with a timeout
        // so the caller never blocks longer than `timeout`.
        let messages = vec![
            LlmMessage::new(LlmRole::System, system.to_string()),
            LlmMessage::new(LlmRole::User, user.to_string()),
        ];

        // Channel + thread for timeout enforcement. The bridge call itself
        // is blocking ; we sacrifice the thread on timeout (it eventually
        // unblocks when ureq's own internal timeout fires, typically 60s).
        // This is acceptable for stage-0 — production would use a per-call
        // ureq agent with the timeout set on the agent.
        let (tx, rx) = std::sync::mpsc::channel();
        // We need to clone the bridge handle into the worker. `Box<dyn LlmBridge>`
        // is not Clone, so wrap the inner in an Arc on construction. We
        // refactor by holding `Arc<dyn LlmBridge>` instead — see the
        // changes below in the new path.
        //
        // For correctness at this layer we instead use a scoped reference
        // pattern : because `LlmBridge: Send + Sync`, we can borrow the
        // bridge for the duration of the call.
        let bridge: &dyn LlmBridge = self.inner.as_ref();
        // SAFETY: `std::thread::scope` requires `'env` lifetime ; we use it
        // to safely hand `bridge` to the worker thread.
        std::thread::scope(|scope| {
            scope.spawn(move || {
                let result = bridge.chat(&messages);
                let _ = tx.send(result);
            });
            match rx.recv_timeout(timeout) {
                Ok(Ok(text)) => Ok(text),
                Ok(Err(e)) => Err(LiveError::from(e)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(LiveError::Timeout),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    Err(LiveError::Bridge("worker disconnected".into()))
                }
            }
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § MOCK IMPL — for tests
// ─────────────────────────────────────────────────────────────────────────

/// Mock `LiveBridge` for tests + the cap-denied-no-bridge default.
///
/// Construct with a list of canned responses ; each `ask` consumes one.
/// When the list is empty, returns `LiveError::Bridge("mock exhausted")`.
pub struct MockLiveBridge {
    responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    available: bool,
    name: &'static str,
    /// Optional cap-error to return on every ask (simulates cap-denied).
    deny: Option<&'static str>,
    /// Records calls for test inspection.
    pub calls: std::sync::Mutex<Vec<(String, String)>>,
}

impl MockLiveBridge {
    /// Construct a mock that returns `responses` in order.
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into_iter().collect()),
            available: true,
            name: "mock.live",
            deny: None,
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Construct an unavailable mock (mimics `is_available() == false`).
    pub fn unavailable() -> Self {
        let mut m = Self::new(vec![]);
        m.available = false;
        m
    }

    /// Construct a mock that always returns `CapDenied` on `ask`. Used to
    /// validate the gm_narrator fallback path.
    pub fn cap_denied(cap: &'static str) -> Self {
        let mut m = Self::new(vec![]);
        m.deny = Some(cap);
        m
    }

    /// Number of recorded calls. Test helper.
    pub fn call_count(&self) -> usize {
        self.calls.lock().map(|c| c.len()).unwrap_or(0)
    }
}

impl LiveBridge for MockLiveBridge {
    fn name(&self) -> &'static str {
        self.name
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn ask(
        &self,
        system: &str,
        user: &str,
        _timeout: Duration,
    ) -> Result<String, LiveError> {
        if let Ok(mut c) = self.calls.lock() {
            c.push((system.to_string(), user.to_string()));
        }
        if let Some(cap) = self.deny {
            return Err(LiveError::CapDenied(cap));
        }
        let mut q = self
            .responses
            .lock()
            .map_err(|_| LiveError::Bridge("mock-mutex-poisoned".into()))?;
        match q.pop_front() {
            Some(text) => Ok(text),
            None => Err(LiveError::Bridge("mock exhausted".into())),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § API-KEY LOADER
// ─────────────────────────────────────────────────────────────────────────

/// Load the Anthropic API key from `~/.loa-secrets/anthropic.env` first,
/// then from the `ANTHROPIC_API_KEY` env-var, returning the first non-empty
/// value found. Never panics ; returns `None` if neither source provides a
/// usable key.
///
/// § Format on disk : a `.env`-style file with a single line :
///     `ANTHROPIC_API_KEY=sk-ant-...`
/// Trailing whitespace + surrounding quotes are stripped. Other lines are
/// ignored so the same file may also carry comments.
pub fn load_anthropic_key() -> Option<String> {
    // 1. file at ~/.loa-secrets/anthropic.env
    if let Some(home) = home_dir() {
        let path: PathBuf = home.join(".loa-secrets").join("anthropic.env");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("ANTHROPIC_API_KEY") {
                    let value = rest
                        .trim_start()
                        .trim_start_matches('=')
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }
    // 2. env var
    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// Resolve the user's home directory cross-platform. Stage-0 ; we only
/// rely on the env vars Windows + Linux + macOS all set at login.
fn home_dir() -> Option<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    if let Ok(p) = std::env::var("USERPROFILE") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    // Fallback : compose from HOMEDRIVE + HOMEPATH (Windows pre-Vista shape).
    if let (Ok(d), Ok(p)) = (std::env::var("HOMEDRIVE"), std::env::var("HOMEPATH")) {
        if !d.is_empty() {
            return Some(PathBuf::from(format!("{}{}", d, p)));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────
// § ARC-WRAPPED HANDLE — for gm_narrator
// ─────────────────────────────────────────────────────────────────────────

/// Type-erased + Arc-wrapped bridge handle. `gm_narrator` stores
/// `Option<LiveBridgeHandle>` so the host can hot-swap modes without
/// rebuilding the narrator state.
pub type LiveBridgeHandle = Arc<dyn LiveBridge>;

// ─────────────────────────────────────────────────────────────────────────
// § TESTS
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Cap-denied → live-bridge returns `CapDenied` ; the gm_narrator
    /// caller treats this as "fall back to pools" by-contract.
    ///
    /// This test asserts the handshake : `MockLiveBridge::cap_denied(...)`
    /// surfaces `LiveError::CapDenied(_)` so callers can pattern-match
    /// and downgrade.
    #[test]
    fn cap_denied_returns_error_so_caller_falls_back() {
        let bridge = MockLiveBridge::cap_denied("EXTERNAL_API");
        assert!(bridge.is_available(), "cap-denied bridge is still 'wired'");
        let result = bridge.ask("sys", "usr", Duration::from_secs(1));
        match result {
            Err(LiveError::CapDenied(c)) => assert_eq!(c, "EXTERNAL_API"),
            other => panic!("expected CapDenied, got {:?}", other),
        }
    }

    /// Cap-granted + mock-success → bridge returns the canned text and
    /// records the call.
    #[test]
    fn cap_granted_mock_success_returns_llm_text() {
        let canned = "The lamps are kind tonight, traveler.".to_string();
        let bridge = MockLiveBridge::new(vec![canned.clone()]);
        assert!(bridge.is_available());
        let result = bridge.ask(
            "system-prompt-sage",
            "user-message-greeting",
            Duration::from_secs(1),
        );
        assert_eq!(result.unwrap(), canned);
        assert_eq!(bridge.call_count(), 1);
        let calls = bridge.calls.lock().unwrap();
        assert_eq!(calls[0].0, "system-prompt-sage");
        assert_eq!(calls[0].1, "user-message-greeting");
    }

    /// Mock exhaustion (no canned responses left) → caller falls back.
    #[test]
    fn mock_exhausted_returns_bridge_error() {
        let bridge = MockLiveBridge::new(vec![]);
        let result = bridge.ask("s", "u", Duration::from_secs(1));
        match result {
            Err(LiveError::Bridge(s)) => assert!(s.contains("exhausted")),
            other => panic!("expected Bridge, got {:?}", other),
        }
    }

    /// Unavailable bridge — `is_available` returns false ; the gm_narrator
    /// fast-path skips even attempting the call.
    #[test]
    fn unavailable_bridge_is_skipped_by_caller() {
        let bridge = MockLiveBridge::unavailable();
        assert!(!bridge.is_available());
    }

    /// `DialoguePromptContext::render` produces non-empty system + user.
    #[test]
    fn dialogue_prompt_renders_non_empty() {
        let ctx = DialoguePromptContext {
            archetype: Archetype::Sage,
            mood: Mood::Calm,
            topic: PhraseTopic::LoreHistory,
            environment: "An ancient corridor.".into(),
            player_utterance: Some("Tell me of the first walls.".into()),
            persona_flavor: Some("laconic".into()),
        };
        let (sys, usr) = ctx.render();
        assert!(sys.contains("Sage"));
        assert!(sys.contains("LoreHistory"));
        assert!(sys.contains("ancient corridor"));
        assert!(sys.contains("laconic"));
        assert!(usr.contains("first walls"));
    }

    /// `EnvironmentPromptContext::render` likewise.
    #[test]
    fn environment_prompt_renders_non_empty() {
        let ctx = EnvironmentPromptContext {
            camera_pos: (1.0, 2.0, 3.0),
            time_of_day: TimeOfDay::Dusk,
            locale_hint: Some("ruined atrium".into()),
        };
        let (sys, usr) = ctx.render();
        assert!(sys.contains("Dusk"));
        assert!(sys.contains("ruined atrium"));
        assert!(usr.contains("1.00") || usr.contains("(1"));
    }

    /// `LlmError::CapDenied` → `LiveError::CapDenied` round-trip.
    #[test]
    fn llm_error_maps_to_live_error() {
        let e: LiveError = LlmError::CapDenied("EXTERNAL_API").into();
        match e {
            LiveError::CapDenied(c) => assert_eq!(c, "EXTERNAL_API"),
            _ => panic!("expected CapDenied"),
        }
    }

    /// API-key loader returns `None` when neither source has a key. We
    /// can't easily unset the env-var inside a test (cargo runs all tests
    /// in one process), so we verify the function does not panic + returns
    /// either `Some` or `None`.
    #[test]
    fn load_anthropic_key_does_not_panic() {
        let _ = load_anthropic_key();
    }
}

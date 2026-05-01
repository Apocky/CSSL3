//! § loop_runner — `AgentLoop` orchestrator.
//!
//! § Responsibilities
//!   - Drive a single turn through `TurnPhase` → `Done` (or `Aborted`).
//!   - Classify intent (keyword heuristic ; replaceable by DI).
//!   - Fetch top-K substrate-docs + always-loaded canon for context.
//!   - Build the LLM message-list ; enforce token-budget.
//!   - Call the bridge ; capture the reply.
//!   - Emit audit events at every phase boundary.
//!
//! § PRIME-DIRECTIVE
//!   - Every mutation is preceded by a `caps.check` cap-gate.
//!   - Sovereign-master bypasses are RECORDED via
//!     `caps.record_sovereign_bypass` + `audit.emit(AuditAxis::CapBypass)`.
//!   - Token-budget enforcement truncates over-budget docs by
//!     proportional share rather than dropping silently.

use std::sync::Arc;

use cssl_host_llm_bridge::{LlmBridge, LlmError, LlmMessage, LlmRole};

use crate::audit::{AuditAxis, AuditEvent, AuditPort};
use crate::caps::ToolCaps;
use crate::now_unix;
use crate::state::{Handoff, TurnPhase, TurnState};
use crate::tools::{ToolError, ToolHandlers};

/// Top-level orchestrator. Owns the bridge + caps + ports + audit.
pub struct AgentLoop {
    /// LLM bridge used for the turn's primary call.
    pub bridge: Box<dyn LlmBridge>,
    /// Cap-policy active for the session.
    pub caps: ToolCaps,
    /// Tool-port bundle.
    pub tools: ToolHandlers,
    /// Audit-port — every phase boundary emits here.
    pub audit: Arc<dyn AuditPort>,
    /// Top-K docs to fetch from `cssl-host-substrate-knowledge`.
    pub knowledge_top_k: usize,
    /// Coarse total-token budget for the assembled message-list.
    pub context_token_budget: usize,
    next_turn_id: u64,
}

impl AgentLoop {
    /// Construct a fresh loop with sensible defaults : `top_k = 5`,
    /// `context_token_budget = 50_000`, `next_turn_id = 1`.
    pub fn new(
        bridge: Box<dyn LlmBridge>,
        caps: ToolCaps,
        tools: ToolHandlers,
        audit: Arc<dyn AuditPort>,
    ) -> Self {
        Self {
            bridge,
            caps,
            tools,
            audit,
            knowledge_top_k: 5,
            context_token_budget: 50_000,
            next_turn_id: 1,
        }
    }

    /// Drive a single turn end-to-end. Returns the final `TurnState`.
    pub fn run_turn(&mut self, user_input: &str) -> Result<TurnState, LoopError> {
        let turn_id = self.next_turn_id;
        self.next_turn_id += 1;
        let mut state = TurnState::new(turn_id, user_input, now_unix());

        // ── Phase: ReceiveInput → emit transparency event.
        self.emit(
            &state,
            AuditAxis::Transparency,
            serde_json::json!({ "event": "input_received", "len": user_input.len() }),
        );
        state.advance(); // → Classify

        // ── Phase: Classify
        let class = self.classify(user_input);
        state.classification = Some(class);
        self.emit(
            &state,
            AuditAxis::Transparency,
            serde_json::json!({ "event": "classified", "handoff": class.as_str() }),
        );
        if matches!(class, Handoff::Collaborator) {
            self.emit(
                &state,
                AuditAxis::Cocreative,
                serde_json::json!({ "event": "collaborator_handoff" }),
            );
        }
        state.advance(); // → FetchContext

        // ── Phase: FetchContext
        state.fetched_docs = self.fetch_context(user_input);
        self.emit(
            &state,
            AuditAxis::Transparency,
            serde_json::json!({ "event": "context_fetched", "count": state.fetched_docs.len() }),
        );
        state.advance(); // → LlmCall

        // ── Phase: LlmCall — assemble messages + call bridge.
        state.llm_messages = self.build_messages(&state);
        let reply = self.bridge.chat(&state.llm_messages)?;
        state.final_reply = Some(reply);
        self.emit(
            &state,
            AuditAxis::ImplementationTransparency,
            serde_json::json!({
                "event": "llm_reply",
                "mode": self.bridge.mode().as_str(),
                "name": self.bridge.name(),
            }),
        );
        state.advance(); // → ToolUse

        // ── Phase: ToolUse — stage-0 stub. Real tool-loop is in wave-C
        // when the upstream LLM (Mode-A/B) emits `ToolCall` events.
        // Here we emit a transparency record so the audit-row is complete.
        self.emit(
            &state,
            AuditAxis::Transparency,
            serde_json::json!({ "event": "tool_use_phase", "calls": state.tool_calls.len() }),
        );
        state.advance(); // → AuditEmit

        // ── Phase: AuditEmit — final sovereignty event for the turn.
        self.emit(
            &state,
            AuditAxis::Sovereignty,
            serde_json::json!({
                "event": "turn_complete",
                "turn_id": turn_id,
                "handoff": class.as_str(),
                "tool_calls": state.tool_calls.len(),
            }),
        );
        state.advance(); // → Reply

        // ── Phase: Reply — passthrough ; final_reply is already set.
        state.advance(); // → Done

        Ok(state)
    }

    /// Keyword-based intent classification per spec/grand-vision/23
    /// § AGENT-LOOP. Stage-0 heuristic — a Mode-A/B richer classifier is
    /// the wave-C drop-in.
    pub fn classify(&self, input: &str) -> Handoff {
        let lower = input.to_ascii_lowercase();
        if contains_any(
            &lower,
            &["compile", "build", "refactor", "fix", "bug"],
        ) {
            Handoff::Coder
        } else if contains_any(&lower, &["narrate", "scene", "describe", "world"]) {
            Handoff::Gm
        } else if contains_any(&lower, &["orchestrate", "plan", "design", "system"]) {
            Handoff::Dm
        } else if contains_any(&lower, &["co-author", "collaborate", "polish"]) {
            Handoff::Collaborator
        } else {
            Handoff::Generic
        }
    }

    /// Top-K relevant docs from the substrate-knowledge corpus. Returns
    /// `(name, score)` pairs ; deterministic on ties (ascending name).
    pub fn fetch_context(&self, query: &str) -> Vec<(String, f32)> {
        cssl_host_substrate_knowledge::query_relevant(query, self.knowledge_top_k)
            .into_iter()
            .map(|(n, s)| (n.to_string(), s))
            .collect()
    }

    /// Build the LLM message-list. The system message folds in :
    ///   1. Always-loaded canon (`PRIME_DIRECTIVE.md` / `CLAUDE.md` /
    ///      `MEMORY.md`).
    ///   2. Top-K relevant substrate docs from `state.fetched_docs`.
    /// Token budget enforcement truncates docs by proportional share if
    /// the total exceeds `context_token_budget`.
    pub fn build_messages(&self, state: &TurnState) -> Vec<LlmMessage> {
        let canon = cssl_host_substrate_knowledge::always_loaded();
        // Coarse token-count via byte-length / 4 (matches the substrate-knowledge
        // crate's `estimated_tokens` heuristic).
        let estimate = |s: &str| s.len() / 4;
        let canon_tokens: usize = canon.iter().map(|(_, body)| estimate(body)).sum();

        // Pull bodies for the fetched docs ; skip ones that aren't embedded.
        let fetched: Vec<(String, String)> = state
            .fetched_docs
            .iter()
            .filter_map(|(name, _)| {
                cssl_host_substrate_knowledge::get_doc(name).map(|b| (name.clone(), b.to_string()))
            })
            .collect();
        let fetched_tokens: usize = fetched.iter().map(|(_, b)| estimate(b)).sum();

        let total = canon_tokens + fetched_tokens;
        let budget = self.context_token_budget.max(1);

        // Build canon + fetched sections, truncating fetched proportionally
        // if total > budget. Canon is sacrosanct ; we never truncate it
        // (PRIME-DIRECTIVE invariant — drop fetched docs first).
        let mut system = String::new();
        system.push_str("[PRIME-DIRECTIVE + CANON]\n");
        for (name, body) in &canon {
            system.push_str("--- ");
            system.push_str(name);
            system.push('\n');
            system.push_str(body);
            system.push_str("\n\n");
        }

        if !fetched.is_empty() {
            system.push_str("[SUBSTRATE-KNOWLEDGE-TOP-K]\n");
            // Compute per-doc share-numerator if over-budget. We work in
            // integer-arithmetic via mul-then-div to avoid lossy casts to
            // f64 (clippy::cast_precision_loss). `share_num / fetched_tokens`
            // is the conceptual scale ; we apply it inside the loop as
            // `(estimate(body) * share_num) / fetched_tokens`.
            let share_num = if total > budget && fetched_tokens > 0 {
                budget.saturating_sub(canon_tokens.min(budget))
            } else {
                fetched_tokens
            };
            for (name, body) in &fetched {
                system.push_str("--- ");
                system.push_str(name);
                system.push('\n');
                let est = estimate(body);
                let take_tokens = if fetched_tokens == 0 {
                    est
                } else {
                    est.saturating_mul(share_num) / fetched_tokens
                };
                let take_bytes = take_tokens.saturating_mul(4).min(body.len());
                // Snap to char boundary.
                let mut cut = take_bytes;
                while cut > 0 && !body.is_char_boundary(cut) {
                    cut -= 1;
                }
                system.push_str(&body[..cut]);
                system.push_str("\n\n");
            }
        }

        vec![
            LlmMessage::new(LlmRole::System, system),
            LlmMessage::new(LlmRole::User, &state.user_input),
        ]
    }

    /// Cancel any in-flight bridge call + abort the current turn record.
    pub fn abort(&mut self, reason: &str) {
        self.bridge.cancel();
        // The current turn is held by the caller of `run_turn` ; we
        // surface the abort by emitting an audit event. The caller can
        // mutate its `TurnState` via `TurnState::abort` directly.
        self.audit.emit(AuditEvent {
            turn_id: self.next_turn_id.saturating_sub(1),
            phase: TurnPhase::Aborted(String::new()).as_str(),
            axis: AuditAxis::Sovereignty,
            payload: serde_json::json!({ "event": "abort", "reason": reason }),
            timestamp_unix: now_unix(),
        });
    }

    /// Internal — emit a phase-tagged audit event.
    fn emit(&self, state: &TurnState, axis: AuditAxis, payload: serde_json::Value) {
        self.audit.emit(AuditEvent {
            turn_id: state.turn_id,
            phase: state.phase.as_str(),
            axis,
            payload,
            timestamp_unix: now_unix(),
        });
    }
}

/// Loop-level error type. Combines bridge / tool / abort / budget cases.
#[derive(Debug, thiserror::Error)]
pub enum LoopError {
    /// LLM bridge surfaced an error (network / cap / config / etc.).
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
    /// Tool dispatch returned an error.
    #[error("tool: {0}")]
    Tool(#[from] ToolError),
    /// Caller invoked `abort` mid-turn.
    #[error("aborted: {0}")]
    Aborted(String),
    /// Token budget exceeded with no recoverable truncation.
    #[error("budget exceeded: {0}")]
    BudgetExceeded(&'static str),
}

/// Test : does any keyword in `needles` appear in `haystack`?
fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

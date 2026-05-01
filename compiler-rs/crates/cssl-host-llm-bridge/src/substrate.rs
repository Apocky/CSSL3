//! § cssl-host-llm-bridge::substrate — Mode-C templated bridge.
//!
//! § ROLE
//!   The truly-self-sufficient fallback. No HTTP. No external dependency.
//!   Returns templated responses keyed off simple keyword classification of
//!   the most-recent user message.
//!
//! § DISCIPLINE
//!   - Cap-bit `SUBSTRATE_ONLY` required (always-on by default but the gate
//!     is structural — `make_bridge` rejects construction if absent).
//!   - Streaming emits one `TextDelta` per word (split on ASCII whitespace)
//!     so the UI animates a typewriter effect identical to Mode-A/B.
//!   - `simulate_delay = false` skips the per-word sleep so tests finish in
//!     microseconds.
//!
//! § INTENT CLASSIFICATION
//!   Lowercased keyword search against the last user message :
//!     - "code" / "rust"  → "I cannot generate code without an LLM bridge…"
//!     - "spec"           → "Use spec_query tool…"
//!     - "git"            → "I can run git commands via the bash tool…"
//!     - else             → "[Mycelium · Mode-C · stage-0-templated] …"
//!
//!   This is intentionally simple. Future revisions wire `cssl-host-kan-real`
//!   for richer classification per spec/grand-vision/23 § MODE-C.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::types::{
    CapBits, LlmAuditEvent, LlmConfig, LlmError, LlmEvent, LlmMessage, LlmMode, LlmRole,
};
use crate::LlmBridge;

/// Mode-C bridge handle. Holds the config and a cancel-flag.
pub struct SubstrateBridge {
    cfg: LlmConfig,
    cancel: AtomicBool,
}

impl SubstrateBridge {
    /// Construct a Mode-C bridge after verifying the cap-bit.
    pub fn new(cfg: LlmConfig, caps: CapBits) -> Result<Self, LlmError> {
        if !caps.has(CapBits::SUBSTRATE_ONLY) {
            return Err(LlmError::CapDenied("SUBSTRATE_ONLY"));
        }
        Ok(Self {
            cfg,
            cancel: AtomicBool::new(false),
        })
    }

    /// Build the templated reply for the given user input.
    fn template_for(message: &str) -> String {
        let lower = message.to_ascii_lowercase();
        if lower.contains("code") || lower.contains("rust") {
            "I cannot generate code without an LLM bridge enabled. \
             Use Mode-A (Anthropic) or Mode-B (Ollama). \
             I can still execute substrate-tools you-explicitly-invoke."
                .into()
        } else if lower.contains("spec") {
            "Use spec_query tool to retrieve substrate-knowledge. I can route the query.".into()
        } else if lower.contains("git") {
            "I can run git commands via the bash tool with your-explicit-approval.".into()
        } else {
            "[Mycelium · Mode-C · stage-0-templated] Your message is acknowledged. \
             To enable generative responses, configure Mode-A (Anthropic-API-key) \
             or Mode-B (Ollama-running)."
                .into()
        }
    }

    /// Pull the most-recent user-role message ; falls back to empty-string.
    fn last_user_content(messages: &[LlmMessage]) -> &str {
        messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, LlmRole::User))
            .map_or("", |m| m.content.as_str())
    }

    /// Approximate-token-count : whitespace-split count. Good enough for the
    /// audit row when the real provider is offline.
    fn approximate_tokens(text: &str) -> u32 {
        let count = text.split_ascii_whitespace().count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}

impl LlmBridge for SubstrateBridge {
    fn name(&self) -> &'static str {
        "mycelium-substrate-mode-c"
    }

    fn mode(&self) -> LlmMode {
        LlmMode::SubstrateOnly
    }

    fn chat(&self, messages: &[LlmMessage]) -> Result<String, LlmError> {
        let user = Self::last_user_content(messages);
        Ok(Self::template_for(user))
    }

    fn chat_stream(
        &self,
        messages: &[LlmMessage],
        on_event: &mut dyn FnMut(LlmEvent),
    ) -> Result<LlmAuditEvent, LlmError> {
        // Reset any prior cancel.
        self.cancel.store(false, Ordering::SeqCst);

        let user = Self::last_user_content(messages);
        let reply = Self::template_for(user);

        let input_tokens = messages
            .iter()
            .map(|m| Self::approximate_tokens(&m.content))
            .sum::<u32>();
        let output_tokens = Self::approximate_tokens(&reply);

        // Word-by-word streaming. Re-attach a single space ahead of every
        // word except the first so the emitted-deltas reconstruct the
        // original whitespace exactly.
        let mut first = true;
        for word in reply.split_ascii_whitespace() {
            if self.cancel.load(Ordering::SeqCst) {
                on_event(LlmEvent::Error("cancelled".into()));
                break;
            }
            let chunk = if first {
                first = false;
                word.to_string()
            } else {
                format!(" {word}")
            };
            on_event(LlmEvent::TextDelta(chunk));
            if self.cfg.simulate_delay {
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        on_event(LlmEvent::Done {
            input_tokens,
            output_tokens,
            stop_reason: "end_turn".into(),
        });

        Ok(LlmAuditEvent {
            mode: LlmMode::SubstrateOnly,
            model: "mycelium-substrate".into(),
            input_tokens,
            output_tokens,
            estimated_cost_usd: 0.0,
            timestamp_unix: now_unix(),
        })
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Wall-clock unix seconds. Falls back to `0` on the (impossible) clock-skew
/// pre-1970 case so the function is total.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Re-export of `template_for` for unit-tests in sibling modules. Keeping it
/// at module-public scope rather than re-exposing the constructor lets tests
/// validate the classifier without needing to instantiate a full bridge.
#[doc(hidden)]
#[must_use]
pub fn classify_template(message: &str) -> String {
    SubstrateBridge::template_for(message)
}

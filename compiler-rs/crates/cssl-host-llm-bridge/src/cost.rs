//! § cssl-host-llm-bridge::cost — token-usage → USD estimate.
//!
//! § Pricing reference (per million tokens, USD)
//!   - Anthropic Claude Opus 4.7 : $15 input · $75 output
//!   - Local Ollama              : $0  (electricity excluded — out-of-scope)
//!   - Mode-C substrate-only     : $0  (no LLM call)
//!
//! § Discipline
//!   Pricing is a moving target. The constants live HERE so a single edit
//!   updates every audit-event downstream. Future revisions can swap to a
//!   per-model table when the bridge gains Sonnet / Haiku tiers.

use crate::types::LlmMode;

/// Anthropic Opus 4.7 input price per token (USD).
pub const ANTHROPIC_OPUS_INPUT_USD_PER_TOKEN: f64 = 15.0 / 1_000_000.0;
/// Anthropic Opus 4.7 output price per token (USD).
pub const ANTHROPIC_OPUS_OUTPUT_USD_PER_TOKEN: f64 = 75.0 / 1_000_000.0;

/// Estimate the USD cost of a single LLM call.
///
/// Mode-B and Mode-C return `0.0` ; Mode-A applies the Opus 4.7 schedule.
/// `model` is currently informational — it is recorded in the audit row but
/// the price table is single-tier (Opus 4.7) until the bridge surfaces
/// model-discrimination upstream.
#[must_use]
pub fn estimate_usd(mode: LlmMode, _model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    match mode {
        LlmMode::ExternalAnthropic => f64::from(input_tokens).mul_add(
            ANTHROPIC_OPUS_INPUT_USD_PER_TOKEN,
            f64::from(output_tokens) * ANTHROPIC_OPUS_OUTPUT_USD_PER_TOKEN,
        ),
        LlmMode::LocalOllama | LlmMode::SubstrateOnly => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_opus_pricing() {
        // 1M input + 1M output = $15 + $75 = $90.
        let usd = estimate_usd(LlmMode::ExternalAnthropic, "claude-opus-4-7", 1_000_000, 1_000_000);
        assert!((usd - 90.0).abs() < 1e-6);
    }

    #[test]
    fn ollama_zero() {
        let usd = estimate_usd(LlmMode::LocalOllama, "qwen2.5-coder:32b", 1_000, 1_000);
        assert!(usd.abs() < 1e-9);
    }

    #[test]
    fn substrate_zero() {
        let usd = estimate_usd(LlmMode::SubstrateOnly, "n/a", 1_000, 1_000);
        assert!(usd.abs() < 1e-9);
    }

    #[test]
    fn anthropic_typical_call() {
        // 1k input + 500 output ≈ $0.015 + $0.0375 = $0.0525
        let usd = estimate_usd(LlmMode::ExternalAnthropic, "claude-opus-4-7", 1_000, 500);
        assert!((usd - 0.0525).abs() < 1e-6, "got {usd}");
    }
}

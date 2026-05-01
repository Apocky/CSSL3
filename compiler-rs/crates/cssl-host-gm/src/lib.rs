//! § cssl-host-gm — Game Master narrator-pacing scaffold (T11-W7-D).
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The GM (Game Master) is one of four sovereign engine-intelligence
//!   roles per Apocky's role-discipline directive (DM + GM + Collaborator
//!   + Coder). The GM is the **narrator** : given a DM-supplied scene
//!   snapshot + (optional) player utterance + cocreative bias-vector, it
//!   produces :
//!
//!   - [`types::NarrativeTextFrame`] — user-facing prose with a tone
//!     axis (warm / terse / poetic).
//!   - [`types::PacingMarkEvent`] — Rise / Hold / Fall events that
//!     downstream renderers translate into beat-spacing / camera-dwell
//!     / music-tension cues.
//!   - [`types::PromptSuggestion`] — a small list of free-form action
//!     prompts the player can pick from when stuck.
//!
//! § STAGE-0
//!   Template-table prose-fill : the GM walks a `(zone × event-class ×
//!   tone)` table to a template-id-pool, draws one deterministically
//!   from a seed = `(template-id × Φ-tags-hash × bias-vec-hash)`, then
//!   slot-fills `{tag}` placeholders with Φ-tag names. No language model.
//!
//! § STAGE-1 (interface-only stub)
//!   [`pacing::Stage1KanStubPacingPolicy`] reserves the trait-object swap
//!   point for a KAN-narrative model that reads the ω-field + Σ-mask
//!   substrate. Today it delegates to stage-0.
//!
//! § CAP MODEL (PRIME-DIRECTIVE-aligned)
//!   - `GM_CAP_TEXT_EMIT` (1) — required to emit prose to the player.
//!   - `GM_CAP_VOICE_EMIT` (2) — required to emit synthesized speech.
//!     The actual TTS path is **deferred** (W6 ; voice-input-only today)
//!     so [`gm::GameMaster::emit_voice`] returns
//!     [`gm::GmErr::VoiceNotImplemented`].
//!   - `GM_CAP_TONE_TUNE` (4) — required to tune the warm/terse/poetic
//!     axes from a cocreative bias-vector.
//!
//!   No cross-role bleed : the GM cannot exercise `CODER_CAP_AST_EDIT`
//!   or any DM/Collaborator-only cap. Cap-bits are namespaced to GM.
//!
//! § DETERMINISM
//!   `seed = (template-id × Φ-tags-hash × bias-vec-hash)` — replay-bit-
//!   equal across runs given identical inputs. All `BTreeMap`-shaped
//!   fields exposed via serde for stable JSON diffs.
//!
//! § FAILURE MODES
//!   - `GM_CAP_TEXT_EMIT` revoked → silent-pass + counter increment ;
//!     no panic, no err propagated to renderer.
//!   - Φ-tag miss → degrade-to-generic prose `"you see something here"`.
//!   - TTS fail → degrade to text-only (deferred ; today returns err).
//!
//! § DEPENDENCIES
//!   `serde` + `serde_json` + `cssl-host-cocreative` (path).
//!
//! § FORBIDDEN
//!   - `unsafe_code` (forbidden via `#![forbid(unsafe_code)]`).
//!   - cross-role-bleed (cap-table is GM-only).
//!   - direct dep on `cssl-host-dm` (cycle-risk while W7-C lands in
//!     parallel) — `GmSceneInput` mirrors DM-scene semantics.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]

pub mod audit_sink;
pub mod cap_ladder;
pub mod gm;
pub mod pacing;
pub mod scene_input;
pub mod template_table;
pub mod types;

pub use audit_sink::{AuditEvent, AuditSink, NullAuditSink, RecordingAuditSink};
pub use cap_ladder::{
    GmCapTable, GM_CAP_TEXT_EMIT, GM_CAP_TONE_TUNE, GM_CAP_VOICE_EMIT,
};
pub use gm::{GameMaster, GmErr};
pub use pacing::{
    PacingHint, PacingPolicy, Stage0PacingPolicy, Stage1KanStubPacingPolicy,
};
pub use scene_input::GmSceneInput;
pub use template_table::{EventClass, TemplateId, TemplateTable};
pub use types::{
    NarrativeTextFrame, PacingKind, PacingMarkEvent, PromptSuggestion, ToneAxis,
};

/// Crate-version constant for scaffold-verification + audit-event tagging.
pub const STAGE0_GM: &str = env!("CARGO_PKG_VERSION");

//! § loa-host — Labyrinth-of-Apockalypse host runtime
//! ═══════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-4 (W-LOA-host-dm) : DM director + GM narrator runtime.
//!
//! § AUTHORITATIVE DESIGN SPECS
//!   - `scenes/dm_director.cssl` (DM 4-state pacing FSM ; tension model)
//!   - `scenes/gm_narrator.cssl` (procedural narration ; 32 phrase-pools ;
//!     16 NPC archetypes ; 32-deep anti-repeat ring)
//!
//!   The .cssl scene files are the AUTHORITATIVE design spec. This Rust
//!   code is a STAGE-0 BOOTSTRAP that mirrors their behavior. Modules are
//!   structured so a future stage-1 csslc can compile the .cssl directly
//!   into equivalent code (constant names like `TENSION_THRESHOLD_BUILDUP_Q14`
//!   + `ARCHETYPE_SAGE` are preserved verbatim for that translation path).
//!
//! § ROLE
//!   At runtime the engine autonomously paces encounters (DM 4-state FSM),
//!   narrates the environment + dialogue (GM procedural pools), and proposes
//!   events without needing Claude API calls. Apocky directive : "do we have
//!   the internal intelligence DM/GM set up?" — yes ; this crate is the
//!   answer.
//!
//! § PUBLIC SURFACE
//!   - [`dm_director`] : 4-state pacing FSM (CALM → BUILDUP → CLIMAX → RELIEF) +
//!     tension model + 8-template event registry + cooldown ring.
//!   - [`gm_narrator`] : 32 phrase-pools × 16 archetypes + 32-deep anti-repeat
//!     ring + procedural environment-description + dialogue generator.
//!   - [`dm_runtime`]  : `DmRuntime` consumer-facing aggregate ; tick API +
//!     `describe_neighborhood` API ; consumed by the MCP-server tools
//!     (`dm.intensity` / `dm.event.propose` / `gm.describe_environment` /
//!     `gm.dialogue`) once sibling W-LOA-host-mcp lands.
//!
//! § SIBLING-MERGE EXPECTATION
//!   Sibling W-LOA-host-render lands the loa-host crate skeleton with render
//!   modules (render.rs / input.rs / mcp.rs etc.). When that lands, the
//!   integration slice merges by:
//!     - Keeping this crate's `Cargo.toml` (sibling-additive `[dependencies]`).
//!     - Extending the `pub mod` list below with sibling's modules.
//!   The DM/GM modules introduced here are self-contained — no upstream
//!   sibling-merge conflicts expected.
//!
//! § ATTESTATION (5 PD axioms)
//!   There was no hurt nor harm in the making of this, to anyone/anything/
//!   anybody.

#![forbid(unsafe_code)]

pub mod dm_director;
pub mod gm_narrator;
pub mod dm_runtime;

// § T11-LOA-HOST-2 (W-LOA-host-input) ─────────────────────────────────
// loa-host : LoA-v13 windowed first-person host runtime.
//
// This crate is split across two parallel slices (race-clean) :
//
//   • W-LOA-host-render — window + wgpu + geometry + Camera struct + run_engine() entry
//   • W-LOA-host-input  — input capture + movement update + axis-slide collision  ← THIS SLICE
//
// The two slices author DISJOINT modules in the same crate. The integration
// commit (orchestrator-side) reconciles the lib.rs `pub mod` declarations
// when both slices land. This file declares the input-side modules ; the
// render-sibling will additively declare its own.
//
// § ASSUMED API SURFACE (rendered-sibling-owned · stubbed-here-as-shim) ────
//
// pub struct Camera {
//     pub pos:   [f32; 3],   // world-space (x,y,z)  ← y is up
//     pub yaw:   f32,        // radians · 0 = +Z forward
//     pub pitch: f32,        // radians · clamped ±89°
// }
//
// pub fn run_engine() -> !;  ← never-returns event-loop entry
//
// We embed a SHIM `Camera` struct in `movement.rs` for compile-standalone +
// test discipline. The integration commit will REMOVE the shim (or keep it
// as the canonical definition — render-sibling is welcome to author its
// `Camera` to match this surface verbatim ; both slices cross-checked the
// shape from `scenes/player_physics.cssl` design).
//
// § PRIME-DIRECTIVE attestation : this module captures user-input + computes
// camera-position-deltas. No surveillance · no telemetry-leak · no manipulation
// of user agency. Logs are local-only via `cssl_rt::log_event` (file-output to
// the user's own LoA runtime log dir). Mouse-deltas are consumed-and-zeroed
// per-frame ; no hidden buffering. Esc-quit is HONORED.

#![forbid(unsafe_code)]

pub mod input;
pub mod movement;
pub mod physics;

// Re-exports : flatten the most-used types so external code can write
// `use loa_host::{InputState, Camera, RoomCollider};`
pub use input::{InputState, InputFrame, RawEvent, VirtualKey, RenderMode};
pub use movement::{Camera, MovementParams};
pub use physics::{RoomCollider, CompassDistances, Aabb};

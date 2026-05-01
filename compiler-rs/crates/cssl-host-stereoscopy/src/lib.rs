//! CSSLv3 stage0 — Stereoscopic camera-pair geometry + IPD-aware projection +
//! side-by-side / top-bottom / anaglyph image composition.
//!
//! § T11-WAVE3-STEREO (cssl/session-15/W-WAVE3-stereo)
//!
//! § PURPOSE
//!   LoA's MCP `render.snapshot_png` currently captures a single mono frame.
//!   To let Apocky "see the game as a human would", we need stereoscopic
//!   capture : left-eye + right-eye images at human IPD (63 mm default) →
//!   composed into anaglyph or side-by-side.
//!
//!   This crate ships the GEOMETRY + COMPOSITION half of the stereoscopic
//!   pipeline. The loa-host wiring (rendering each eye-pose then handing the
//!   bytes here for composition) lands in wave-4. By design this crate is
//!   FILE-DISJOINT from any in-flight merge : pure-math / pure-bytes only.
//!
//! § MODULE LAYOUT
//!   - [`config`]      — `StereoConfig` + `StereoErr` (IPD / convergence /
//!                       eye-separation-direction validation).
//!   - [`geometry`]    — `EyePose` + `EyePair` + `eye_pair_from_mono` (toed-in
//!                       camera-pair derivation from a monocular pose).
//!   - [`composition`] — `compose_side_by_side` / `compose_top_bottom` /
//!                       `compose_anaglyph_red_cyan` (raw RGBA byte composition).
//!   - [`manifest`]    — `StereoCaptureManifest` + `ComposeFormat`
//!                       (serializable record of a stereo-capture event).
//!
//! § GUARANTEES
//!   - `#![forbid(unsafe_code)]` ; no `unsafe` blocks anywhere.
//!   - No panics on malformed input : every public fn returns `Result`.
//!   - NaN-propagation defended-against by validating all f32 inputs first.
//!   - `serde` round-trip stable for `StereoConfig` + `EyePose` + `EyePair` +
//!     `StereoCaptureManifest` + `ComposeFormat`.

#![forbid(unsafe_code)]

pub mod composition;
pub mod config;
pub mod geometry;
pub mod manifest;

pub use composition::{ComposeErr, compose_anaglyph_red_cyan, compose_side_by_side, compose_top_bottom};
pub use config::{StereoConfig, StereoErr};
pub use geometry::{EyePair, EyePose, eye_pair_from_mono, left_eye_forward, left_eye_position, right_eye_forward, right_eye_position};
pub use manifest::{ComposeFormat, StereoCaptureManifest};

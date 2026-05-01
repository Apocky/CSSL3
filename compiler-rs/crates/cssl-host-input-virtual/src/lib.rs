//! # cssl-host-input-virtual
//!
//! Deterministic synthetic input event generators for the LoA host —
//! a test/dev apparatus that produces `cssl-host-replay::ReplayEvent`
//! streams without requiring a live human at the keyboard or mouse.
//!
//! ## Why a Virtual-Input Crate?
//!
//! `cssl-host-replay` records and replays input streams, but to validate
//! the replay machinery (or to seed the future MCP test harness) we need
//! **synthetic** event streams that are :
//!
//! - **Deterministic** : same seed → bit-identical event stream, so a
//!   golden-image test can pin a stream and detect drift.
//! - **Schema-correct** : every produced event is a valid `ReplayEvent`
//!   (i.e. matches what `cssl-host-replay::Recorder` writes), so the
//!   stream can be fed straight into a `Replayer` for visual-regression
//!   on the renderer side.
//! - **Compositional** : low-level generators (typing, circles, random
//!   walks, Lissajous) compose into higher-level scenarios (room tour,
//!   intent typing, multi-intent Q&A session).
//!
//! ## Module Map
//!
//! - [`rng`] — pure-stdlib PCG-32 RNG, hand-rolled to avoid the `rand`
//!   dependency-graph and to keep determinism stable across rustc versions.
//! - [`keystrokes`] — typing-session generators : random WPM bursts +
//!   literal-text-to-keystroke sequencing.
//! - [`mouse_paths`] — parametric cursor-trajectory generators : circle,
//!   Lissajous, Gaussian random walk, drag gesture.
//! - [`scenarios`] — high-level multi-event compositions for common test
//!   shapes (room navigation · intent phrase · full Q&A session).
//!
//! ## Forbidden Patterns
//!
//! - `unsafe` is forbidden crate-wide via `#![forbid(unsafe_code)]`.
//! - Library code never panics ; every generator returns a `Vec<ReplayEvent>`
//!   that is empty when the inputs imply zero events (rather than panicking).
//! - No I/O ; this is a pure-CPU generator crate.
//!
//! ## Determinism Contract
//!
//! For any generator `f(seed: u64, ...) -> Vec<ReplayEvent>`, two invocations
//! with identical arguments **must** return `Vec`s that compare equal under
//! `PartialEq` (which on `ReplayEvent` is structural over `ts_micros + kind`).
//! The unit tests in each module enforce this for every public generator.

#![forbid(unsafe_code)]
// § Local clippy allowances : workspace lints inherit pedantic+nursery at warn,
// and the pure-math float code in this crate trips a handful of stylistic-only
// lints (FMA suggestions on hot paths, hypotenuse-form on test-asserts, items-
// after-statements for module-private consts, single-char trig bindings, and
// the documented sub-23-bit precision loss in the standard u32→f32 division
// for [0,1) draws).  None affect correctness ; explicit allow keeps the gate
// at zero warnings without weakening workspace defaults.
#![allow(
    clippy::cast_precision_loss,
    clippy::imprecise_flops,
    clippy::suboptimal_flops,
    clippy::items_after_statements,
    clippy::many_single_char_names,
)]

pub mod keystrokes;
pub mod mouse_paths;
pub mod rng;
pub mod scenarios;

pub use cssl_host_replay::{ReplayEvent, ReplayEventKind};
pub use keystrokes::{ascii_text_to_keystrokes, random_typing_session};
pub use mouse_paths::{circle_path, drag, lissajous, random_walk};
pub use rng::Pcg32;
pub use scenarios::{full_qa_session, navigate_test_room, type_intent_phrase};

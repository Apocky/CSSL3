// § T11-WAVE3-PROCGEN-ROOMS : crate root
// ══════════════════════════════════════════════════════════════════
//! # cssl-host-procgen-rooms
//!
//! § Runtime procedural-generation of LoA-v13 room recipes.
//!
//! Per Apocky's engine-directive : LoA rooms are generated at runtime, NOT
//! authored as static scenes. This crate produces deterministic [`RoomRecipe`]
//! structures from a `(seed, dims, kind, constraints)` input tuple. Wave-4
//! wires these recipes into the loa-host runtime ; this crate is self-contained
//! and has no dependency on engine subsystems.
//!
//! ## Determinism
//! Same input → bit-identical output. Backed by a pure-stdlib PCG-32 RNG
//! ([`rng::Pcg32`]) — no `rand` crate dependency. Round-trip
//! serialize-deserialize-serialize equality is guaranteed for any generated
//! recipe.
//!
//! ## Recipe kinds
//! - [`recipe::RoomKind::CalibrationGrid`] — grid-aligned reference tiles
//! - [`recipe::RoomKind::MaterialShowcase`] — 16-patch material grid
//! - [`recipe::RoomKind::ScaleHall`] — long corridor with receding tiles
//! - [`recipe::RoomKind::ColorWheel`] — 12-segment circular floor
//! - [`recipe::RoomKind::PatternMaze`] — cellular-automata maze
//! - [`recipe::RoomKind::NoiseField`] — value-noise material field
//! - [`recipe::RoomKind::VoronoiPlazas`] — 8-site voronoi tiling
//!
//! ## Forbidden
//! - `unsafe` — `#![forbid(unsafe_code)]`
//! - `panic!` / `unwrap` outside tests — recipe generation is infallible by
//!   construction (input validation is the caller's job ; out-of-range
//!   constraints clamp silently).

#![forbid(unsafe_code)]
// § Pedantic/nursery noise that's purely cosmetic in this scaffold crate.
// Per workspace policy, scaffold-crates are allowed to defer these to a
// later docstring/style polish slice.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::single_char_lifetime_names)]
#![allow(clippy::min_ident_chars)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::tuple_array_conversions)]
#![allow(clippy::float_cmp)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::float_arithmetic)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::while_float)]

pub mod constraints;
pub mod recipe;
pub mod recipes;
pub mod rng;

pub use constraints::{validate_recipe, ConstraintErr, RoomConstraints};
pub use recipe::{Doorway, LightHint, RoomDims, RoomKind, RoomRecipe, TileLayer, TilePlacement, WallSide};
pub use recipes::generate;
pub use rng::Pcg32;

//! # cssl-host-rt-trace — bounded-memory runtime trace ring
//!
//! § T11-W5-RT-TRACE : fixed-size ring of [`RtEvent`] records for hot-path
//! instrumentation in render-loops + game-tick threads. Lock-free
//! single-producer / multi-consumer model :
//!
//! - **O(1) push** via `AtomicU64::fetch_add` on `write_idx` — no mutex
//! - **O(N) snapshot** copies out `write_idx − read_idx` entries
//! - **Drop-oldest** on overrun (writer wins ; reader catches up or loses tail)
//! - **Pre-allocated `Vec<RtEvent>`** never grows after [`RtRing::new`]
//! - **32-byte cache-line-friendly** [`RtEvent`] layout
//!
//! ## Use case (LoA)
//!
//! ```ignore
//! let ring = RtRing::new(4096);
//! let mut interner = LabelInterner::default();
//! let frame_idx = interner.intern("frame");
//!
//! // Inside render loop :
//! {
//!     let _scope = scoped_mark(&ring, frame_idx);
//!     // ... render work ...
//! } // Drop pushes MarkEnd with elapsed-micros
//!
//! // Out-of-line drain thread :
//! let events = ring.drain();
//! ```
//!
//! ## Why not crossbeam / tokio ?
//!
//! Per spec : std-only. Render-loops run pre-async ; trace ring must not
//! pull a tokio runtime onto the engine. Crossbeam's `ArrayQueue` is
//! similar but adds dep weight ; we own the bounded ring so we can tune
//! drop-oldest semantics + serde-roundtrip the snapshot.
//!
//! ## Unsafe ?
//!
//! No `unsafe` blocks. The ring uses `AtomicU64` indices + `Vec<UnsafeCell<…>>`
//! avoidance via `parking_lot`-free design : we accept the one-mutex on
//! `Vec` cell-writes (`std::sync::Mutex<Vec<RtEvent>>`) for stage-0
//! correctness. Future stage-1 may upgrade to `UnsafeCell` + atomic-cell
//! when benchmarked.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod event;
pub mod macros;
pub mod ring;
pub mod scope;

pub use event::{LabelInterner, RtEvent, RtEventKind};
pub use ring::RtRing;
pub use scope::{scoped_mark, ScopedMark};

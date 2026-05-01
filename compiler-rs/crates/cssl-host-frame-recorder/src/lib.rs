//! § cssl-host-frame-recorder
//! ══════════════════════════════════════════════════════════════════
//! § T11-W5b-FRAME-RECORDER : pure-rust frame-buffer accumulator +
//! lossless on-disk format ("LFRC" = LoA Frame Recording Container)
//! suitable for recording the engine's render output without bringing
//! in `ffmpeg` / `mp4` / `flate2` dependencies.
//!
//! § scope
//!   • `Frame`     — RGBA8 framebuffer + width/height/timestamp + kind tag
//!   • `FrameRecorder` — bounded-ring accumulator with drop counters
//!   • LFRC encoder/decoder — magic-bytes + 40-byte header + per-frame
//!     records + 8-byte footer (4-byte sentinel + 4-byte CRC32). Round-
//!     trips byte-exact via stdlib-only CRC32 polynomial 0xEDB88320.
//!   • `LfrcStore` — disk-backed save / load / list of recordings
//!
//! § non-goals (stage-0)
//!   • compression — every frame stored RAW ; flate2 deferred wave-6
//!   • inter-frame deltas — `FrameKind::DeltaFromPrevious` reserved but
//!     unused in stage-0 ; encoder always emits `KeyFrame`
//!   • lossy codecs — LFRC is bit-exact ; round-trip verified by tests
//!
//! § wire-up
//! Workspace glob @ `compiler-rs/Cargo.toml` auto-discovers this crate.
//! `loa-host` integration (record-hotkey wiring) lands wave-6.
//!
//! § safety
//! `#![forbid(unsafe_code)]` ; library never panics on user input.

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-host-frame-recorder/0.1.0")]

pub mod frame;
pub mod lfrc;
pub mod recorder;
pub mod storage;

pub use frame::{Frame, FrameErr, FrameKind};
pub use lfrc::{decode_from_bytes, encode_to_bytes, LfrcErr, LFRC_MAGIC, LFRC_VERSION};
pub use recorder::FrameRecorder;
pub use storage::LfrcStore;

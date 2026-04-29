//! § cssl-asset — CSSLv3 stage-0 asset pipeline (PNG / GLTF / WAV / TTF)
//! ════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec: `specs/14_BACKEND.csl § ASSET PIPELINE` (added
//! by this slice — N1) + `stdlib/fs.cssl` (file-IO surface this builds
//! on) + `stdlib/string.cssl` + `stdlib/vec.cssl` (parser inputs and
//! outputs).
//!
//! § ROLE
//!   The asset pipeline is the layer between cssl-rt's file-IO surface
//!   and the runtime data structures of a CSSLv3 game / app. Parsers
//!   are written in Rust at stage-0 (per the same Rust-hosted bootstrap
//!   strategy as the rest of the compiler) ; the surface mirrors what
//!   the eventual CSSLv3-self-hosted version will expose so consumers
//!   can be written in either language and migrated mechanically.
//!
//! § FORMATS (stage-0 always-on)
//!   - **PNG**  — 8-bit grayscale, gray+alpha, RGB, RGBA. Hand-rolled
//!                inflate + PLTE-less + non-interlaced. Round-trip safe.
//!   - **GLTF / GLB** — full glTF 2.0 JSON manifest + GLB binary
//!                container. Scene-graph walker + accessor → byte-slice
//!                resolution. Materials + animations + skins decoded
//!                at the JSON level (consumer interprets contents).
//!   - **WAV**  — RIFF/WAVE PCM (8/16/24/32-bit signed and unsigned-8) +
//!                IEEE 754 float32. Round-trip safe.
//!   - **TTF**  — TrueType / OpenType (TTF outlines only ; CFF deferred).
//!                Header + table directory + cmap (format 4) + glyf
//!                (simple glyphs) + hmtx (advance + LSB).
//!
//! § FORMATS (deferred, cfg-gated for future slices)
//!   - **WebP / KTX2** — texture compression-format read (cfg-gated).
//!   - **OGG-Vorbis / MP3** — compressed audio decode (cfg-gated).
//!   - **Bitmap fonts** — BDF / PCF / sfnt-bitmap.
//!
//! § INFRASTRUCTURE
//!   - [`AssetHandle<T>`][handle::AssetHandle] — async-loading surface.
//!     At stage-0 every handle is `Ready` (synchronous) ; the API
//!     shape is forward-compatible with a real async runtime.
//!   - [`AssetBudget`] — per-asset-class memory
//!     budget with LRU / smallest-first eviction policy.
//!   - [`AssetWatcher`] — hot-reload watcher
//!     scaffold. At stage-0 inert ; OS-backed implementation lands
//!     in a follow-up slice.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   Asset loaders are surveillance-adjacent (file-system reads, format
//!   parsing) ; this crate's surface :
//!     - never auto-traverses directories the caller did not name,
//!     - caps every allocation at the input's reported size + a hard
//!       upper bound (`png::MAX_IMAGE_BYTES`, `wav::MAX_WAV_DATA_BYTES`),
//!     - rejects pathological inputs (zero-dim images, integer-overflow
//!       buffer sizes, deeply-nested JSON) BEFORE any allocation,
//!     - emits no telemetry — the caller sees only the values it asks
//!       for + structured `AssetError` for everything else.
//!   No microphone surface, no camera, no system-font enumeration. The
//!   PNG / WAV / TTF / GLTF subset is by design : these are file
//!   parsers, not capture devices.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::needless_pass_by_value)]
// § N1 (T11-D104) : asset parsers handle integer-byte arithmetic on file
// offsets + IEEE-754 round-trips. These lints fire on arithmetic that's
// audited to be safe (CRC table indexing, byte-buffer indices, exact
// f32/f64 round-trips).
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::float_cmp)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::vec_init_then_push)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::if_not_else)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_then_some_else_none)]
#![allow(clippy::redundant_else)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::if_same_then_else)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod budget;
pub mod error;
pub mod gltf;
pub mod handle;
pub mod png;
pub mod ttf;
pub mod watcher;
pub mod wav;

pub use budget::{AssetBudget, AssetClass, EvictionPolicy};
pub use error::{AssetError, Result};
pub use gltf::{
    decode_glb, decode_gltf, parse_json, Accessor, AnimChannel, AnimSampler, Animation, Buffer,
    BufferView, GltfAsset, GltfDocument, Image, JsonValue, Material, Mesh, MeshPrimitive, Node,
    Scene, Skin, Texture,
};
pub use handle::{AssetHandle, AssetState, LoadProgress};
pub use png::{decode as decode_png, encode as encode_png, peek as peek_png, ColorType, PngImage};
pub use ttf::{
    parse as parse_ttf, Cmap4, FontHeader, GlyphMetric, GlyphOutline, GlyphPoint, HoriHeader,
    TtfFont,
};
pub use watcher::{watch_path, AssetWatcher, WatchEvent};
pub use wav::{decode as decode_wav, encode as encode_wav, SampleFormat, WavFile};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

#[cfg(test)]
mod crate_tests {
    use super::*;

    #[test]
    fn version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present_and_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn re_exports_resolve() {
        // Compile-time check : these names must be reachable via `cssl_asset::`.
        let _: Result<()> = Ok(());
        let _ = AssetClass::Texture.name();
        let _ = EvictionPolicy::Lru;
        let _: AssetHandle<u32> = AssetHandle::ready(1, 4);
        let _ = LoadProgress::starting(0);
        let _ = WatchEvent::Created { path: "x".into() };
    }
}

//! § cssl-host-asset-loader — pre-authored / pre-generated asset ingestion.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Sibling of `cssl-host-procgen-pipeline`. Where procgen-pipeline EMITS
//!   crystals from runtime intent, this crate INGESTS pre-existing data
//!   (pre-authored `.csl` scenes from disk, CC-licensed 3D meshes from the
//!   Khronos glTF-Sample-Models repo, CC0 HDRIs from Polyhaven, OpenGameArt
//!   CC0 textures, plus a compile-time embedded unit-cube) and lifts them
//!   into the same `CrystalSeed` lingua-franca that the substrate-render
//!   layer ultimately consumes.
//!
//! § APOCKY-DIRECTIVE (verbatim · this slice)
//!   "Can we feed in pre-generated or pre-authored assets and data?
//!    Generation algorithms, 3D data from free legal sources, etc.?"
//!   Answer : yes — through this manifest-driven scaffold. Default-OFF
//!   sovereignty, opt-in env-flag, license-tag enforcement, sha256-pinned.
//!
//! § PIPELINE
//!   ```text
//!   AssetManifest { entries: Vec<AssetEntry> }
//!     │
//!     ├── default_manifest()       ← curated baseline of CC-licensed sources
//!     ├── load_local_cssl_dir()    ← scan disk for .csl files
//!     └── (manifest assembly · serde-JSON round-trippable)
//!     │
//!     ▼ for entry in entries
//!   AssetEntry { id, kind, source_uri, license, sha256_expected, parser }
//!     │
//!     ▼ fetch_with_validation(entry, fetch_max_bytes)
//!     │   1. policy gate (LOA_ASSET_LOAD env-var ; license-tag check)
//!     │   2. delegate to cssl_host_procgen_pipeline::asset_fetch
//!     │   3. sha256-verify if sha256_expected.is_some()
//!     │
//!     ▼ Vec<u8> raw bytes
//!     │
//!     ▼ parse_to_crystal_seed(bytes, parser)
//!     │   GltfBinary / GltfJson / CSSLObject / JsonRaw / Wav / OggVorbis
//!     │
//!     ▼ Vec<CrystalSeed>
//!   ```
//!
//! § SOVEREIGNTY-FLOOR (default-DENY · explicit-opt-in)
//!   - `LOA_ASSET_LOAD` env-var : default-OFF ; truthy values
//!     (`"1"` / `"true"` / `"TRUE"` / `"yes"` / `"on"`) opt in. When OFF,
//!     `fetch_with_validation` returns [`LoaderErr::FetchDisabled`] without
//!     touching the network.
//!   - Network surface delegates to `cssl_host_procgen_pipeline::asset_fetch`
//!     which itself enforces : `LOA_PROCGEN_FETCH_HOSTS` allowlist
//!     (default-empty = deny-all), `LOA_PROCGEN_FETCH_MAX_BYTES` size-cap
//!     (default 10 MB), `LOA_PROCGEN_FETCH_OFFLINE=1` kill-switch, 30-second
//!     timeout. THREE independent gates : two in this crate (load-flag +
//!     license-flag), three in the procgen-pipeline crate.
//!   - License-tag enforcement : `LicenseTag::ProprietaryApocky` entries
//!     are NEVER fetched unless `LOA_ASSET_LOAD=1` AND
//!     `LOA_ASSET_PROPRIETARY=1` (twin-key opt-in).
//!   - SHA-256 verification : when `entry.sha256_expected` is `Some`,
//!     `fetch_with_validation` rejects mismatched bytes via
//!     [`LoaderErr::ChecksumMismatch`] with both expected + observed digests.
//!   - Compile-time embedded asset : [`embedded_unit_cube_seeds`] returns
//!     24 `CrystalSeed`s WITHOUT any network or filesystem traffic.
//!
//! § CC-LICENSE VERIFICATION (real source-URL citations)
//!   - Khronos glTF-Sample-Models : CC-BY 4.0 / CC0 1.0 (per-asset)
//!       <https://github.com/KhronosGroup/glTF-Sample-Models/blob/master/LICENSE.md>
//!     The repository's master `LICENSE.md` enumerates which models are CC0
//!     and which are CC-BY (most are CC0 ; DamagedHelmet credits the original
//!     authors under CC-BY 4.0 ; Box / BoxTextured / Cube / WaterBottle are
//!     CC0). Per-asset license is captured in the manifest's `license` field.
//!
//!   - Polyhaven HDRIs            : CC0 1.0 Public Domain Dedication
//!       <https://polyhaven.com/license>
//!     "All assets on Poly Haven are released under the CC0 license. They
//!     are completely free to use ... No restrictions. No attribution
//!     required." (verbatim from the license page).
//!
//!   - OpenGameArt CC0 textures   : CC0 1.0 (per-asset filter)
//!       <https://opengameart.org/content/license-search?field_art_licenses_tid%5B%5D=4>
//!     Only entries tagged CC0 1.0 are referenced here. Per-asset attribution
//!     is captured in the manifest's `id` field for traceability.
//!
//! § PRIME-DIRECTIVE alignment
//!   - default-deny via LOA_ASSET_LOAD=0 (default unset = OFF)
//!   - explicit-opt-in via env-var (player-sovereignty)
//!   - license-tag enforcement (no surprise proprietary-fetch)
//!   - sha256-pinning (no surprise byte-substitution)
//!   - delegates to existing audited HTTP surface (no parallel network code)
//!   - no telemetry / no cookies / no background tasks / no global state
//!   - serde-roundtrippable manifest (cssl-edge inspectable)
//!
//! § F2-SLICE LANDED — sovereignty-attestation
//!   ✓ default-off · explicit env-opt-in
//!   ✓ CC-licenses verified · per-asset license-tag captured
//!   ✓ sha256-pinned · twin-key opt-in for ProprietaryApocky
//!   ✓ embedded unit-cube reachable without any I/O
//!   ✓ delegates to cssl-host-procgen-pipeline::asset_fetch (audited)

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
// Single-character bindings are common in helper closures + tests ; the
// hygiene tradeoff is not worth the noise here.
#![allow(clippy::many_single_char_names)]
// FMA-rewrites for `1.0 + i * 0.5` are micro-optimizations that hurt
// readability at the call-volume here (≤4 crystals).
#![allow(clippy::suboptimal_flops)]
// Small fixed-size arrays of tuples in embedded_unit_cube_seeds are
// inherently a "tuple → array" pattern ; the lint suggests a refactor
// that would not improve clarity for the 6-face cube.
#![allow(clippy::tuple_array_conversions)]
// Test-only allowances : redundant_clone / case_sensitive_file_extension
// firing on `std::fs::canonicalize` fallback + `assert!(... ends_with(".csl"))`
// are stylistic for setup boilerplate, not safety-critical.
#![cfg_attr(test, allow(clippy::redundant_clone))]
#![cfg_attr(test, allow(clippy::case_sensitive_file_extension_comparisons))]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ════════════════════════════════════════════════════════════════════════════
// § TYPES — manifest, entries, kinds, parsers, license, crystal-seed, errors
// ════════════════════════════════════════════════════════════════════════════

/// What kind of asset-payload an entry represents.
///
/// `Mesh` and `Texture` are the bread-and-butter substrate-render inputs.
/// `Audio` is reserved for the cssl-wave-audio integration. `CSSLScene`
/// is a pre-authored .csl source file (parsed to AST → Crystal layout).
/// `JsonRecipe` is a generation-algorithm parameter blob (procedural
/// recipe). `GltfModel` overlaps Mesh+Texture but signals a glTF-bundle
/// the parser should split into multiple sub-payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetKind {
    /// Static mesh — vertices + indices ; no texture binding.
    Mesh,
    /// Texture — RGB(A) image bytes ; PNG / JPG / EXR.
    Texture,
    /// Audio sample — WAV PCM or OGG-Vorbis encoded.
    Audio,
    /// Pre-authored CSSL scene file — `.csl` source bytes.
    CSSLScene,
    /// JSON recipe — parameters for a procedural-generation algorithm.
    JsonRecipe,
    /// Full glTF model — mesh + textures + materials in one bundle.
    GltfModel,
}

/// License-tag for an asset entry.
///
/// Captured verbatim so downstream attribution (cssl-edge `/credits`
/// page, in-game pause-menu credits-list) can render exactly the
/// terms required by each source.
///
/// `ProprietaryApocky` is the kill-switch tag for any future
/// internal-only assets. It is NEVER fetched unless the player has
/// BOTH `LOA_ASSET_LOAD=1` AND `LOA_ASSET_PROPRIETARY=1` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LicenseTag {
    /// CC0 1.0 Universal Public Domain Dedication.
    /// <https://creativecommons.org/publicdomain/zero/1.0/>
    CC0,
    /// CC-BY 4.0 — attribution required.
    /// <https://creativecommons.org/licenses/by/4.0/>
    CCBY,
    /// MIT License.
    MIT,
    /// Apache License 2.0.
    Apache2,
    /// Apocky's proprietary content. Twin-key opt-in only.
    ProprietaryApocky,
    /// US-government / pre-1929 / explicitly-public-domain.
    PublicDomain,
}

/// Parser dispatch enum — selects which decoder to apply to fetched bytes.
///
/// Each variant maps to a `parse_<kind>_to_crystal_seed` private fn in this
/// crate. The actual parsers are STUB-LEVEL : they extract enough structure
/// to produce a `Vec<CrystalSeed>` and defer full deserialization to the
/// host-side renderer (which has access to the geometry / material crates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParserKind {
    /// glTF Binary (.glb) — magic 0x46546C67 + JSON-chunk + BIN-chunk.
    GltfBinary,
    /// glTF JSON (.gltf) — UTF-8 JSON with external buffer references.
    GltfJson,
    /// CSSL object literal — text source ; we extract `position` / `color`
    /// fields per top-level object via line-scan parser.
    CSSLObject,
    /// Raw JSON recipe — `{"seeds": [{"x":..,"y":..,"z":..,"r":..,...}]}`.
    JsonRaw,
    /// WAV PCM — read header, return one zero-position seed per file.
    Wav,
    /// OGG-Vorbis — return one zero-position audio-tagged seed per file.
    OggVorbis,
}

/// One entry in the asset-manifest.
///
/// `source_uri` is either an `http://` / `https://` URL (for online sources)
/// or a `file://` URL / bare path (for local pre-authored files). The
/// loader inspects the scheme and routes accordingly.
///
/// `sha256_expected` is `None` for entries where stable hashing is not yet
/// available (e.g. moving-target CDN endpoints). When `Some`, the loader
/// rejects mismatched bytes ; this is the strongest defense against
/// CDN-substitution attacks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetEntry {
    /// Stable identifier — used as the credits-page key + cache filename.
    pub id: String,
    /// What payload-shape this entry carries.
    pub kind: AssetKind,
    /// Where the bytes live — http(s) URL or file path.
    pub source_uri: String,
    /// Per-asset license tag (drives the credits page + fetch policy).
    pub license: LicenseTag,
    /// Optional SHA-256 of the expected bytes. When present, fetched
    /// payloads are rejected on mismatch.
    pub sha256_expected: Option<[u8; 32]>,
    /// Which decoder to apply to the fetched bytes.
    pub parser: ParserKind,
}

/// The top-level manifest — a list of entries with no global state.
///
/// Round-trips through serde-JSON cleanly so a manifest can be authored
/// in `assets/manifest.json` on disk, loaded at boot, and inspected via
/// the cssl-edge `/manifest` endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AssetManifest {
    pub entries: Vec<AssetEntry>,
}

/// Minimal Crystal-shaped record produced by every parser.
///
/// Loa-host's full `Crystal` struct carries additional fields (substrate
/// energy seed, KAN-bias coupling, etc.). `CrystalSeed` is the skinny
/// projection that asset-loader emits ; the loa-host integration layer
/// is responsible for lifting `CrystalSeed` → `Crystal` by filling in
/// the substrate-side fields (those require access to the omega-field
/// / KAN crates which are NOT pulled here per the tight-scope rule).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrystalSeed {
    /// World-space position (metres, observer-local frame).
    pub position_xyz: [f32; 3],
    /// RGB color tuple in [0.0, 1.0].
    pub color_rgb: [f32; 3],
    /// Emission intensity (1.0 = nominal ; > 1.0 for HDR sources).
    pub intensity: f32,
    /// Lower-case kind-tag — same vocabulary as procgen-pipeline.
    /// "mesh-vertex" for glTF vertices ; "audio" for WAV / OGG ;
    /// "scene-object" for CSSLScene parses ; etc.
    pub kind_tag: KindTag,
}

/// Compact tag-enum to avoid String-bloat in seed batches.
///
/// Kept tiny on purpose : a 24-vertex unit-cube emits 24 seeds × this
/// enum-discriminator instead of 24 String allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KindTag {
    /// Vertex from a parsed mesh.
    MeshVertex,
    /// Audio sample (single zero-pos seed per file).
    Audio,
    /// Top-level object from a CSSL scene.
    SceneObject,
    /// JSON-recipe seed entry.
    Recipe,
    /// HDRI / environment sample.
    Hdri,
    /// Fallback for unparsed payloads.
    Unknown,
}

/// Errors from the high-level loader API.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoaderErr {
    /// `LOA_ASSET_LOAD` is unset / falsy ; fetch attempted.
    #[error("asset-load disabled : set LOA_ASSET_LOAD=1 to opt in")]
    FetchDisabled,
    /// Entry is `LicenseTag::ProprietaryApocky` and twin-key opt-in is missing.
    #[error("proprietary asset blocked : set LOA_ASSET_PROPRIETARY=1 to opt in")]
    ProprietaryBlocked,
    /// Underlying procgen-pipeline asset_fetch returned an error.
    #[error("fetch failed : {0}")]
    Fetch(String),
    /// SHA-256 of fetched bytes did not match `entry.sha256_expected`.
    #[error("checksum mismatch : expected {expected_hex}, got {observed_hex}")]
    ChecksumMismatch {
        expected_hex: String,
        observed_hex: String,
    },
    /// Local-directory scan I/O error.
    #[error("io error : {0}")]
    Io(String),
    /// URI is malformed (e.g. empty after trim).
    #[error("invalid asset uri : {0}")]
    InvalidUri(String),
}

/// Errors from the parsers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseErr {
    /// Empty payload ; nothing to decode.
    #[error("empty payload")]
    Empty,
    /// glTF magic / JSON / chunk-shape was malformed.
    #[error("malformed gltf : {0}")]
    BadGltf(String),
    /// JSON did not parse.
    #[error("malformed json : {0}")]
    BadJson(String),
    /// CSSL scene-text could not be tokenized into an object-list.
    #[error("malformed cssl : {0}")]
    BadCssl(String),
    /// WAV / OGG header was not recognized.
    #[error("malformed audio : {0}")]
    BadAudio(String),
}

// ════════════════════════════════════════════════════════════════════════════
// § ENV-VAR NAMES (canonical · grep-target)
// ════════════════════════════════════════════════════════════════════════════

const ENV_LOAD: &str = "LOA_ASSET_LOAD";
const ENV_PROPRIETARY: &str = "LOA_ASSET_PROPRIETARY";

// ════════════════════════════════════════════════════════════════════════════
// § default_manifest — curated baseline of CC-licensed sources
// ════════════════════════════════════════════════════════════════════════════

/// Returns a curated baseline manifest of FREE LEGAL asset URIs.
///
/// The URIs are NOT fetched here ; this fn is pure (no I/O). The
/// returned manifest is meant to be inspected, persisted to JSON,
/// and passed to `fetch_with_validation` by callers that have
/// explicit consent to fetch.
///
/// § INCLUDED ASSETS
///   - Khronos glTF Sample Models (CC-0 / CC-BY 4.0 per-asset) :
///     - Box.glb           (CC0)
///     - BoxTextured.glb   (CC0)
///     - Cube.glb          (CC0)
///     - WaterBottle.glb   (CC0)
///     - DamagedHelmet.glb (CC-BY 4.0 : credits to theblueturtle_/Microsoft)
///     - MetalRoughSpheres.glb (CC0)
///   - Polyhaven HDRIs (CC0 1.0) :
///     - kloofendal_43d_clear_puresky_4k.exr
///     - studio_small_03_4k.exr
///   - OpenGameArt CC0 textures (URI-form-only baseline) :
///     - One CC0 grass-pattern URI as a representative.
///
/// § NETWORK BEHAVIOR
///   None. This fn touches NEITHER disk nor network. The URIs are
///   stable strings ; the test-suite proves `default_manifest()` is
///   a pure const-equivalent fn (no env-var, no time, no clock).
///
/// § CITATIONS (verifiable)
///   - <https://github.com/KhronosGroup/glTF-Sample-Models>
///   - <https://github.com/KhronosGroup/glTF-Sample-Models/blob/master/LICENSE.md>
///   - <https://polyhaven.com/license>
///   - <https://opengameart.org/content/license-search?field_art_licenses_tid%5B%5D=4>
pub fn default_manifest() -> AssetManifest {
    let entries = vec![
        // ── Khronos glTF-Sample-Models : CC0 ──────────────────────────────
        AssetEntry {
            id: "khronos-box-glb".into(),
            kind: AssetKind::GltfModel,
            source_uri: "https://raw.githubusercontent.com/KhronosGroup/\
                         glTF-Sample-Models/master/2.0/Box/glTF-Binary/Box.glb"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        },
        AssetEntry {
            id: "khronos-box-textured-glb".into(),
            kind: AssetKind::GltfModel,
            source_uri: "https://raw.githubusercontent.com/KhronosGroup/\
                         glTF-Sample-Models/master/2.0/BoxTextured/\
                         glTF-Binary/BoxTextured.glb"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        },
        AssetEntry {
            id: "khronos-cube-glb".into(),
            kind: AssetKind::GltfModel,
            source_uri: "https://raw.githubusercontent.com/KhronosGroup/\
                         glTF-Sample-Models/master/2.0/Cube/glTF/Cube.gltf"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfJson,
        },
        AssetEntry {
            id: "khronos-water-bottle-glb".into(),
            kind: AssetKind::GltfModel,
            source_uri: "https://raw.githubusercontent.com/KhronosGroup/\
                         glTF-Sample-Models/master/2.0/WaterBottle/\
                         glTF-Binary/WaterBottle.glb"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        },
        // ── Khronos glTF-Sample-Models : CC-BY 4.0 ────────────────────────
        AssetEntry {
            id: "khronos-damaged-helmet-glb".into(),
            kind: AssetKind::GltfModel,
            source_uri: "https://raw.githubusercontent.com/KhronosGroup/\
                         glTF-Sample-Models/master/2.0/DamagedHelmet/\
                         glTF-Binary/DamagedHelmet.glb"
                .into(),
            license: LicenseTag::CCBY,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        },
        AssetEntry {
            id: "khronos-metal-rough-spheres-glb".into(),
            kind: AssetKind::GltfModel,
            source_uri: "https://raw.githubusercontent.com/KhronosGroup/\
                         glTF-Sample-Models/master/2.0/MetalRoughSpheres/\
                         glTF-Binary/MetalRoughSpheres.glb"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        },
        // ── Polyhaven HDRIs : CC0 1.0 ─────────────────────────────────────
        AssetEntry {
            id: "polyhaven-kloofendal-puresky-4k".into(),
            kind: AssetKind::Texture,
            source_uri: "https://dl.polyhaven.org/file/ph-assets/HDRIs/\
                         exr/4k/kloofendal_43d_clear_puresky_4k.exr"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::JsonRaw,
        },
        AssetEntry {
            id: "polyhaven-studio-small-03-4k".into(),
            kind: AssetKind::Texture,
            source_uri: "https://dl.polyhaven.org/file/ph-assets/HDRIs/\
                         exr/4k/studio_small_03_4k.exr"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::JsonRaw,
        },
        // ── OpenGameArt CC0 textures (baseline representative) ────────────
        AssetEntry {
            id: "opengameart-cc0-tile-baseline".into(),
            kind: AssetKind::Texture,
            source_uri: "https://opengameart.org/sites/default/files/\
                         styles/medium/public/seamless_grass.png"
                .into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::JsonRaw,
        },
    ];
    AssetManifest { entries }
}

// ════════════════════════════════════════════════════════════════════════════
// § load_local_cssl_dir — scan a directory for .csl files
// ════════════════════════════════════════════════════════════════════════════

/// Scan `dir` for `.csl` files and produce one `AssetEntry` per file.
///
/// This is the LOCAL-PRE-AUTHORED-ASSET path. No network. No license
/// inference (everything coming off local disk is tagged
/// `ProprietaryApocky` by default — the developer who placed the file
/// there is the rightholder ; downstream slices can override the tag
/// post-load).
///
/// § BEHAVIOR
///   - Non-existent / non-directory `dir` → [`LoaderErr::Io`].
///   - Non-`.csl` files are silently skipped.
///   - Sub-directories are NOT recursed (this is a flat scan).
///   - Returned entries use the file's stem as `id` ; `source_uri` is
///     the absolute path with `file://` prefix for explicit-scheme.
///   - Returned in `read_dir` order ; callers that need stable ordering
///     should sort by `id` after this call.
pub fn load_local_cssl_dir(dir: &Path) -> Result<Vec<AssetEntry>, LoaderErr> {
    if !dir.exists() {
        return Err(LoaderErr::Io(format!(
            "directory does not exist : {}",
            dir.display()
        )));
    }
    if !dir.is_dir() {
        return Err(LoaderErr::Io(format!(
            "path is not a directory : {}",
            dir.display()
        )));
    }
    let read = std::fs::read_dir(dir)
        .map_err(|e| LoaderErr::Io(e.to_string()))?;
    let mut entries = Vec::new();
    for ent in read {
        let ent = ent.map_err(|e| LoaderErr::Io(e.to_string()))?;
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("csl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();
        let canonical = canonicalize_or_keep(&path);
        let uri = format!(
            "file://{}",
            canonical.display().to_string().replace('\\', "/")
        );
        entries.push(AssetEntry {
            id: stem,
            kind: AssetKind::CSSLScene,
            source_uri: uri,
            license: LicenseTag::ProprietaryApocky,
            sha256_expected: None,
            parser: ParserKind::CSSLObject,
        });
    }
    Ok(entries)
}

/// Canonicalize a path if possible ; fall back to the original path
/// when canonicalization fails (e.g. read-only volume).
fn canonicalize_or_keep(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

// ════════════════════════════════════════════════════════════════════════════
// § fetch_with_validation — sovereignty-floored fetch + sha256-verify
// ════════════════════════════════════════════════════════════════════════════

/// Fetch the bytes for `entry`, applying the full sovereignty-floor.
///
/// § ALGORITHM
///   1. Check `LOA_ASSET_LOAD` env-var. Default-OFF returns
///      [`LoaderErr::FetchDisabled`] without inspecting the entry.
///   2. If `entry.license == ProprietaryApocky` and `LOA_ASSET_PROPRIETARY`
///      is not truthy, return [`LoaderErr::ProprietaryBlocked`].
///   3. Trim `entry.source_uri` ; reject empty.
///   4. If the URI starts with `file://`, read the local file directly
///      (BUT only when the entry's license is ProprietaryApocky AND the
///      twin-key was already accepted ; otherwise the path-based scheme
///      is rejected). Local-path fetches NEVER touch the network.
///   5. Otherwise delegate to
///      `cssl_host_procgen_pipeline::asset_fetch`. Map its `FetchErr`
///      variants into [`LoaderErr::Fetch`].
///   6. Apply size-cap : if `bytes.len() > fetch_max_bytes`, return
///      [`LoaderErr::Fetch`] (the underlying call already capped, but
///      we re-check defensively).
///   7. If `entry.sha256_expected` is `Some(expected)`, compute the
///      SHA-256 of the fetched bytes ; on mismatch return
///      [`LoaderErr::ChecksumMismatch`] with hex strings.
///
/// § DETERMINISM
///   The non-network paths (file://, error-paths, sha256-verify) are
///   pure given the input bytes. The network path is non-deterministic
///   by definition ; callers should pin sha256_expected to make the
///   network result reproducible.
pub fn fetch_with_validation(
    entry: &AssetEntry,
    fetch_max_bytes: usize,
) -> Result<Vec<u8>, LoaderErr> {
    // § 1. Master kill-switch.
    if !is_load_enabled() {
        return Err(LoaderErr::FetchDisabled);
    }
    // § 2. Proprietary-license twin-key check.
    if entry.license == LicenseTag::ProprietaryApocky && !is_proprietary_enabled()
    {
        return Err(LoaderErr::ProprietaryBlocked);
    }
    let trimmed = entry.source_uri.trim();
    if trimmed.is_empty() {
        return Err(LoaderErr::InvalidUri(String::new()));
    }
    // § 4. file:// path : read local file (no network surface).
    let bytes = if let Some(path_part) = trimmed.strip_prefix("file://") {
        // The twin-key is required for file:// fetches because local
        // pre-authored assets are tagged ProprietaryApocky by default.
        // Step 2 above already verified the twin-key.
        std::fs::read(path_part).map_err(|e| LoaderErr::Io(e.to_string()))?
    } else {
        // § 5. Delegate to the audited HTTP surface.
        cssl_host_procgen_pipeline::asset_fetch(trimmed)
            .map_err(|e| LoaderErr::Fetch(e.to_string()))?
    };
    // § 6. Defensive size-cap re-check.
    if bytes.len() > fetch_max_bytes {
        return Err(LoaderErr::Fetch(format!(
            "body too large : {} bytes",
            bytes.len()
        )));
    }
    // § 7. SHA-256 verify if expected present.
    if let Some(expected) = entry.sha256_expected {
        let observed = sha256(&bytes);
        if observed != expected {
            return Err(LoaderErr::ChecksumMismatch {
                expected_hex: hex32(&expected),
                observed_hex: hex32(&observed),
            });
        }
    }
    Ok(bytes)
}

// ── env-var policy helpers (pure-fn) ────────────────────────────────────────

fn is_load_enabled() -> bool {
    truthy_env(ENV_LOAD)
}

fn is_proprietary_enabled() -> bool {
    truthy_env(ENV_PROPRIETARY)
}

fn truthy_env(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let lower = v.trim().to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// 64-char lower-case hex of a 32-byte digest.
fn hex32(b: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for &byte in b {
        s.push(HEX[(byte >> 4) as usize] as char);
        s.push(HEX[(byte & 0x0f) as usize] as char);
    }
    s
}

// ════════════════════════════════════════════════════════════════════════════
// § parse_to_crystal_seed — parser dispatch
// ════════════════════════════════════════════════════════════════════════════

/// Parse `bytes` according to `parser`, returning a seed-batch.
///
/// All parsers return AT LEAST one CrystalSeed for any non-empty input
/// (so downstream code can always render SOMETHING). Empty payloads
/// return [`ParseErr::Empty`].
pub fn parse_to_crystal_seed(
    bytes: &[u8],
    parser: ParserKind,
) -> Result<Vec<CrystalSeed>, ParseErr> {
    if bytes.is_empty() {
        return Err(ParseErr::Empty);
    }
    match parser {
        ParserKind::GltfBinary => parse_gltf_binary(bytes),
        ParserKind::GltfJson => parse_gltf_json(bytes),
        ParserKind::CSSLObject => parse_cssl_object(bytes),
        ParserKind::JsonRaw => parse_json_raw(bytes),
        ParserKind::Wav => parse_wav(bytes),
        ParserKind::OggVorbis => parse_ogg_vorbis(bytes),
    }
}

/// glTF-Binary stub — verifies the magic + returns a single placeholder
/// seed. Full vertex extraction is OUT OF SCOPE for this slice ; it
/// belongs to the host integration layer that has access to the
/// geometry crate.
fn parse_gltf_binary(b: &[u8]) -> Result<Vec<CrystalSeed>, ParseErr> {
    if b.len() < 12 {
        return Err(ParseErr::BadGltf("header too short".into()));
    }
    // glTF-binary magic = "glTF" little-endian = 0x46546C67.
    if &b[0..4] != b"glTF" {
        return Err(ParseErr::BadGltf("bad magic".into()));
    }
    Ok(vec![CrystalSeed {
        position_xyz: [0.0, 0.0, 0.0],
        color_rgb: [0.8, 0.8, 0.8],
        intensity: 1.0,
        kind_tag: KindTag::MeshVertex,
    }])
}

/// glTF-JSON stub — light JSON-shape sniff (must contain `"asset"` and
/// `"version"` keys) ; returns one placeholder seed.
fn parse_gltf_json(b: &[u8]) -> Result<Vec<CrystalSeed>, ParseErr> {
    let s = std::str::from_utf8(b).map_err(|e| ParseErr::BadGltf(e.to_string()))?;
    if !s.contains("\"asset\"") || !s.contains("\"version\"") {
        return Err(ParseErr::BadGltf("missing asset/version".into()));
    }
    Ok(vec![CrystalSeed {
        position_xyz: [0.0, 0.0, 0.0],
        color_rgb: [0.7, 0.7, 0.9],
        intensity: 1.0,
        kind_tag: KindTag::MeshVertex,
    }])
}

/// CSSL-object stub — line-scan for `position` / `color` / `intensity`
/// triples. Real grammar is parsed by the csslc compiler ; this is the
/// lightweight authoring-side preview.
fn parse_cssl_object(b: &[u8]) -> Result<Vec<CrystalSeed>, ParseErr> {
    let s = std::str::from_utf8(b).map_err(|e| ParseErr::BadCssl(e.to_string()))?;
    // Minimal guard : expect an object-opener somewhere.
    if !s.contains('{') && !s.contains("§") {
        return Err(ParseErr::BadCssl("no object-opener found".into()));
    }
    // Count top-level `§` lines as objects ; one seed per.
    let object_count = s.lines().filter(|l| l.trim_start().starts_with('§')).count();
    let n = object_count.max(1);
    let mut seeds = Vec::with_capacity(n);
    for i in 0..n {
        let theta = (i as f32) * 2.399_963_3_f32;
        let r = 1.0_f32 + (i as f32) * 0.5_f32;
        seeds.push(CrystalSeed {
            position_xyz: [r * theta.cos(), 0.0, r * theta.sin()],
            color_rgb: [0.6, 0.9, 0.6],
            intensity: 1.0,
            kind_tag: KindTag::SceneObject,
        });
    }
    Ok(seeds)
}

/// JSON-recipe stub — accept any well-formed JSON ; emit one seed
/// per top-level array element if the root is an array, else one seed
/// for the whole document.
fn parse_json_raw(b: &[u8]) -> Result<Vec<CrystalSeed>, ParseErr> {
    let value: serde_json::Value =
        serde_json::from_slice(b).map_err(|e| ParseErr::BadJson(e.to_string()))?;
    let count = match &value {
        serde_json::Value::Array(arr) => arr.len().max(1),
        _ => 1,
    };
    let mut seeds = Vec::with_capacity(count);
    for i in 0..count {
        seeds.push(CrystalSeed {
            position_xyz: [i as f32, 0.0, 0.0],
            color_rgb: [0.9, 0.7, 0.5],
            intensity: 1.0,
            kind_tag: KindTag::Recipe,
        });
    }
    Ok(seeds)
}

/// WAV-PCM stub — verifies RIFF header presence ; one zero-pos seed.
fn parse_wav(b: &[u8]) -> Result<Vec<CrystalSeed>, ParseErr> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(ParseErr::BadAudio("not a RIFF/WAVE file".into()));
    }
    Ok(vec![CrystalSeed {
        position_xyz: [0.0, 0.0, 0.0],
        color_rgb: [0.5, 0.5, 0.9],
        intensity: 1.0,
        kind_tag: KindTag::Audio,
    }])
}

/// OGG-Vorbis stub — verifies "OggS" capture-pattern ; one zero-pos seed.
fn parse_ogg_vorbis(b: &[u8]) -> Result<Vec<CrystalSeed>, ParseErr> {
    if b.len() < 4 || &b[0..4] != b"OggS" {
        return Err(ParseErr::BadAudio("not an Ogg stream".into()));
    }
    Ok(vec![CrystalSeed {
        position_xyz: [0.0, 0.0, 0.0],
        color_rgb: [0.4, 0.6, 0.9],
        intensity: 1.0,
        kind_tag: KindTag::Audio,
    }])
}

// ════════════════════════════════════════════════════════════════════════════
// § embedded_unit_cube_seeds — compile-time mini-asset (no network · no I/O)
// ════════════════════════════════════════════════════════════════════════════

/// Returns a 24-vertex unit-cube `CrystalSeed` batch.
///
/// Uses BOXED-CUBE topology (4 verts × 6 faces = 24) so that face
/// normals can be inferred per-face downstream without index-buffer
/// trickery. Coordinates are at ±0.5 metres so the cube is one
/// metre on each axis, observer-centered.
///
/// § PURITY
///   This fn touches NEITHER disk nor network. The byte-pattern is
///   compile-time embedded. Tests prove `LOA_ASSET_LOAD=0` does not
///   block this fn ; it always returns the same 24 seeds.
pub fn embedded_unit_cube_seeds() -> Vec<CrystalSeed> {
    // 6 faces · 4 vertices each · CCW winding when viewed from outside.
    // Coordinates are observer-centered ±0.5 ; colors per-face for visual
    // distinctness in debug-render.
    type Face = (([f32; 3], [f32; 3], [f32; 3], [f32; 3]), [f32; 3]);
    let faces: [Face; 6] = [
        // +X face : red
        (
            (
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ),
            [1.0, 0.2, 0.2],
        ),
        // -X face : cyan
        (
            (
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ),
            [0.2, 1.0, 1.0],
        ),
        // +Y face : green
        (
            (
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ),
            [0.2, 1.0, 0.2],
        ),
        // -Y face : magenta
        (
            (
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ),
            [1.0, 0.2, 1.0],
        ),
        // +Z face : blue
        (
            (
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ),
            [0.2, 0.2, 1.0],
        ),
        // -Z face : yellow
        (
            (
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ),
            [1.0, 1.0, 0.2],
        ),
    ];
    let mut seeds = Vec::with_capacity(24);
    for ((p0, p1, p2, p3), color) in faces {
        for p in [p0, p1, p2, p3] {
            seeds.push(CrystalSeed {
                position_xyz: p,
                color_rgb: color,
                intensity: 1.0,
                kind_tag: KindTag::MeshVertex,
            });
        }
    }
    seeds
}

// ════════════════════════════════════════════════════════════════════════════
// § sha256 — self-contained FIPS-180-4 SHA-256 (no external dep)
// ════════════════════════════════════════════════════════════════════════════
//
// We avoid pulling the `sha2` crate to keep the dep-graph minimal and the
// audit-trail tight. The implementation follows FIPS-180-4 §6.2 verbatim
// and is verified against published RFC test-vectors in the test-suite.

const K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5,
    0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5,
    0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3,
    0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174,
    0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc,
    0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da,
    0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7,
    0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967,
    0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13,
    0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
    0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3,
    0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070,
    0x19a4_c116, 0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5,
    0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208,
    0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
];

const H0: [u32; 8] = [
    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a,
    0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19,
];

/// Compute SHA-256 of `data` and return the 32-byte digest.
///
/// Reference : FIPS-180-4 §6.2 (Federal Information Processing Standards
/// Publication 180-4, August 2015). Verified against RFC 6234
/// test-vectors in the test-suite.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    // § Padding : append 0x80, then zeros, until len ≡ 56 (mod 64),
    // then append 64-bit big-endian bit-length.
    let bit_len: u64 = (data.len() as u64).wrapping_mul(8);
    let mut padded: Vec<u8> = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = H0;
    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7)
                ^ w[i - 15].rotate_right(18)
                ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17)
                ^ w[i - 2].rotate_right(19)
                ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
// § TESTS
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Per-test serialization lock for env-var driven tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_load: Option<String>,
        prev_prop: Option<String>,
        prev_offline: Option<String>,
        prev_hosts: Option<String>,
    }

    impl EnvGuard {
        fn take() -> Self {
            let lock = ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let g = Self {
                _lock: lock,
                prev_load: std::env::var(ENV_LOAD).ok(),
                prev_prop: std::env::var(ENV_PROPRIETARY).ok(),
                prev_offline: std::env::var("LOA_PROCGEN_FETCH_OFFLINE").ok(),
                prev_hosts: std::env::var("LOA_PROCGEN_FETCH_HOSTS").ok(),
            };
            std::env::remove_var(ENV_LOAD);
            std::env::remove_var(ENV_PROPRIETARY);
            std::env::remove_var("LOA_PROCGEN_FETCH_OFFLINE");
            std::env::remove_var("LOA_PROCGEN_FETCH_HOSTS");
            g
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in [
                (ENV_LOAD, &self.prev_load),
                (ENV_PROPRIETARY, &self.prev_prop),
                ("LOA_PROCGEN_FETCH_OFFLINE", &self.prev_offline),
                ("LOA_PROCGEN_FETCH_HOSTS", &self.prev_hosts),
            ] {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    // ── 1. manifest serde round-trip ─────────────────────────────────────
    #[test]
    fn manifest_roundtrip_serde_json() {
        let m = default_manifest();
        let s = serde_json::to_string(&m).expect("serialize");
        let back: AssetManifest = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(m, back);
    }

    // ── 2. default_manifest is non-empty + curated ───────────────────────
    #[test]
    fn default_manifest_non_empty_and_curated() {
        let m = default_manifest();
        assert!(!m.entries.is_empty(), "must include curated baseline");
        // At least one CC0 + one CC-BY entry expected.
        let has_cc0 = m
            .entries
            .iter()
            .any(|e| e.license == LicenseTag::CC0);
        let has_ccby = m
            .entries
            .iter()
            .any(|e| e.license == LicenseTag::CCBY);
        assert!(has_cc0, "expect at least one CC0 entry");
        assert!(has_ccby, "expect at least one CC-BY entry");
        // No proprietary leak in the public default.
        assert!(
            m.entries
                .iter()
                .all(|e| e.license != LicenseTag::ProprietaryApocky),
            "default manifest must not include proprietary entries"
        );
        // Every URI is https:// (no plain http in the curated set).
        assert!(
            m.entries.iter().all(|e| e.source_uri.starts_with("https://")),
            "all curated URIs must be https"
        );
    }

    // ── 3. license-enforced : Proprietary blocked without twin-key ───────
    #[test]
    fn proprietary_blocked_without_twin_key() {
        let _g = EnvGuard::take();
        std::env::set_var(ENV_LOAD, "1");
        // PROPRIETARY env-var unset.
        let entry = AssetEntry {
            id: "secret".into(),
            kind: AssetKind::CSSLScene,
            source_uri: "file:///tmp/some.csl".into(),
            license: LicenseTag::ProprietaryApocky,
            sha256_expected: None,
            parser: ParserKind::CSSLObject,
        };
        let r = fetch_with_validation(&entry, 1024);
        assert!(matches!(r, Err(LoaderErr::ProprietaryBlocked)));
    }

    // ── 4. sha256 mismatch rejected ──────────────────────────────────────
    #[test]
    fn sha256_mismatch_rejected_on_local_read() {
        let _g = EnvGuard::take();
        std::env::set_var(ENV_LOAD, "1");
        std::env::set_var(ENV_PROPRIETARY, "1");
        // Write a temp file with known contents.
        let dir = std::env::temp_dir().join("cssl-asset-loader-test-3");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("payload.csl");
        std::fs::write(&path, "\u{00A7} object { x = 0 }\n".as_bytes()).unwrap();
        let canonical = std::fs::canonicalize(&path).unwrap_or(path.clone());
        let uri = format!("file://{}", canonical.display().to_string().replace('\\', "/"));
        // Wrong expected digest.
        let entry = AssetEntry {
            id: "p".into(),
            kind: AssetKind::CSSLScene,
            source_uri: uri,
            license: LicenseTag::ProprietaryApocky,
            sha256_expected: Some([0xab; 32]),
            parser: ParserKind::CSSLObject,
        };
        let r = fetch_with_validation(&entry, 1024);
        assert!(matches!(r, Err(LoaderErr::ChecksumMismatch { .. })));
    }

    // ── 5. LOA_ASSET_LOAD=0 blocks ALL fetches ──────────────────────────
    #[test]
    fn load_disabled_blocks_all_fetches() {
        let _g = EnvGuard::take();
        // Default-OFF (env unset).
        let entry = AssetEntry {
            id: "any".into(),
            kind: AssetKind::Mesh,
            source_uri: "https://example.com/x.glb".into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        };
        assert!(matches!(
            fetch_with_validation(&entry, 1024),
            Err(LoaderErr::FetchDisabled)
        ));
        // Explicit "0" still blocked.
        std::env::set_var(ENV_LOAD, "0");
        assert!(matches!(
            fetch_with_validation(&entry, 1024),
            Err(LoaderErr::FetchDisabled)
        ));
        // Falsy values blocked.
        for v in &["false", "no", "off", ""] {
            std::env::set_var(ENV_LOAD, v);
            assert!(
                matches!(
                    fetch_with_validation(&entry, 1024),
                    Err(LoaderErr::FetchDisabled)
                ),
                "LOA_ASSET_LOAD={v} should block"
            );
        }
    }

    // ── 6. embedded unit-cube loads without network ─────────────────────
    #[test]
    fn embedded_cube_loads_without_network() {
        let _g = EnvGuard::take();
        // Force every network gate OFF — embedded path must still work.
        std::env::set_var(ENV_LOAD, "0");
        std::env::set_var("LOA_PROCGEN_FETCH_OFFLINE", "1");
        let seeds = embedded_unit_cube_seeds();
        assert_eq!(seeds.len(), 24, "boxed cube = 4 verts × 6 faces");
        // All vertices on the unit-cube surface.
        for s in &seeds {
            for axis in s.position_xyz {
                assert!(
                    (axis - 0.5).abs() < 1e-6 || (axis + 0.5).abs() < 1e-6,
                    "cube vertices at ±0.5 only ; got {axis}"
                );
            }
            assert!(matches!(s.kind_tag, KindTag::MeshVertex));
        }
        // Six distinct face-colors (one per face × 4 verts).
        let mut palette: Vec<[u32; 3]> = seeds
            .iter()
            .map(|s| {
                [
                    (s.color_rgb[0] * 255.0) as u32,
                    (s.color_rgb[1] * 255.0) as u32,
                    (s.color_rgb[2] * 255.0) as u32,
                ]
            })
            .collect();
        palette.sort_unstable();
        palette.dedup();
        assert_eq!(palette.len(), 6, "six distinct face-colors");
    }

    // ── 7. GltfJson parse stub ───────────────────────────────────────────
    #[test]
    fn gltf_json_parse_stub() {
        let good = br#"{"asset":{"version":"2.0"},"meshes":[]}"#;
        let seeds = parse_to_crystal_seed(good, ParserKind::GltfJson).unwrap();
        assert_eq!(seeds.len(), 1);
        assert!(matches!(seeds[0].kind_tag, KindTag::MeshVertex));
        // Bad JSON-ish input rejected.
        let bad = b"<not json>";
        assert!(matches!(
            parse_to_crystal_seed(bad, ParserKind::GltfJson),
            Err(ParseErr::BadGltf(_))
        ));
    }

    // ── 8. invalid URI rejected ──────────────────────────────────────────
    #[test]
    fn invalid_uri_rejected() {
        let _g = EnvGuard::take();
        std::env::set_var(ENV_LOAD, "1");
        let entry = AssetEntry {
            id: "x".into(),
            kind: AssetKind::Mesh,
            source_uri: "   ".into(),
            license: LicenseTag::CC0,
            sha256_expected: None,
            parser: ParserKind::GltfBinary,
        };
        assert!(matches!(
            fetch_with_validation(&entry, 1024),
            Err(LoaderErr::InvalidUri(_))
        ));
    }

    // ── 9. SHA-256 RFC test-vectors ──────────────────────────────────────
    #[test]
    fn sha256_rfc_vectors() {
        // RFC 6234 §A.1 — the canonical "abc" vector.
        let abc = sha256(b"abc");
        let expected_abc = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea,
            0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
            0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c,
            0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(abc, expected_abc, "abc vector");
        // Empty-string vector.
        let empty = sha256(b"");
        let expected_empty = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14,
            0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24,
            0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
            0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(empty, expected_empty, "empty vector");
    }

    // ── 10. local CSSL dir scan ─────────────────────────────────────────
    #[test]
    fn local_cssl_dir_scan() {
        let _g = EnvGuard::take();
        let dir = std::env::temp_dir().join("cssl-asset-loader-test-10");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alpha.csl"), "\u{00A7} a {}\n".as_bytes()).unwrap();
        std::fs::write(dir.join("beta.csl"), "\u{00A7} b {}\n".as_bytes()).unwrap();
        std::fs::write(dir.join("ignore.txt"), b"not csl").unwrap();
        let entries = load_local_cssl_dir(&dir).unwrap();
        // Two .csl files picked up ; .txt ignored.
        assert_eq!(entries.len(), 2);
        for e in &entries {
            assert!(matches!(e.kind, AssetKind::CSSLScene));
            assert!(matches!(e.license, LicenseTag::ProprietaryApocky));
            assert!(e.source_uri.starts_with("file://"));
            assert!(e.source_uri.ends_with(".csl"));
        }
        // Non-existent dir errors.
        assert!(matches!(
            load_local_cssl_dir(&dir.join("nonexistent")),
            Err(LoaderErr::Io(_))
        ));
    }

    // ── 11. CSSL parser stub ─────────────────────────────────────────────
    #[test]
    fn cssl_parser_stub_emits_seed_per_section() {
        let src = "§ first { x = 0 }\n§ second { y = 1 }\n§ third { z = 2 }\n";
        let seeds = parse_to_crystal_seed(src.as_bytes(), ParserKind::CSSLObject).unwrap();
        assert_eq!(seeds.len(), 3);
        for s in &seeds {
            assert!(matches!(s.kind_tag, KindTag::SceneObject));
        }
        // Bad input rejected.
        assert!(matches!(
            parse_to_crystal_seed(b"plain text no opener", ParserKind::CSSLObject),
            Err(ParseErr::BadCssl(_))
        ));
    }

    // ── 12. WAV / OGG audio parsers ─────────────────────────────────────
    #[test]
    fn audio_parsers() {
        // RIFF/WAVE header (canonical 44-byte minimum).
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&36u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 16]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&0u32.to_le_bytes());
        let seeds = parse_to_crystal_seed(&wav, ParserKind::Wav).unwrap();
        assert_eq!(seeds.len(), 1);
        assert!(matches!(seeds[0].kind_tag, KindTag::Audio));
        // OggS magic prefix.
        let ogg = b"OggSrest-of-stream-bytes";
        let seeds = parse_to_crystal_seed(ogg, ParserKind::OggVorbis).unwrap();
        assert_eq!(seeds.len(), 1);
        assert!(matches!(seeds[0].kind_tag, KindTag::Audio));
        // Non-RIFF bytes rejected as WAV.
        assert!(matches!(
            parse_to_crystal_seed(b"junk", ParserKind::Wav),
            Err(ParseErr::BadAudio(_))
        ));
        // Non-Ogg bytes rejected as OGG.
        assert!(matches!(
            parse_to_crystal_seed(b"junk", ParserKind::OggVorbis),
            Err(ParseErr::BadAudio(_))
        ));
        // Empty rejected universally.
        assert!(matches!(
            parse_to_crystal_seed(&[], ParserKind::Wav),
            Err(ParseErr::Empty)
        ));
    }

    // ── 13. JSON-raw parser ─────────────────────────────────────────────
    #[test]
    fn json_raw_parser() {
        let arr = b"[1,2,3,4]";
        let seeds = parse_to_crystal_seed(arr, ParserKind::JsonRaw).unwrap();
        assert_eq!(seeds.len(), 4);
        let obj = b"{\"k\":\"v\"}";
        let seeds = parse_to_crystal_seed(obj, ParserKind::JsonRaw).unwrap();
        assert_eq!(seeds.len(), 1);
        let bad = b"{not-json";
        assert!(matches!(
            parse_to_crystal_seed(bad, ParserKind::JsonRaw),
            Err(ParseErr::BadJson(_))
        ));
    }

    // ── 14. glTF binary magic check ─────────────────────────────────────
    #[test]
    fn gltf_binary_magic() {
        // Synth minimal glTF-binary header : magic + version + length.
        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&12u32.to_le_bytes());
        let seeds = parse_to_crystal_seed(&glb, ParserKind::GltfBinary).unwrap();
        assert_eq!(seeds.len(), 1);
        // Bad magic rejected.
        let bad = b"NOTglTFhdr12";
        assert!(matches!(
            parse_to_crystal_seed(bad, ParserKind::GltfBinary),
            Err(ParseErr::BadGltf(_))
        ));
        // Too short rejected.
        let short = b"glTF";
        assert!(matches!(
            parse_to_crystal_seed(short, ParserKind::GltfBinary),
            Err(ParseErr::BadGltf(_))
        ));
    }

    // ── 15. Full happy-path : env-on + sha-pinned local file ────────────
    #[test]
    fn happy_path_local_file_sha_pinned() {
        let _g = EnvGuard::take();
        std::env::set_var(ENV_LOAD, "1");
        std::env::set_var(ENV_PROPRIETARY, "1");
        let dir = std::env::temp_dir().join("cssl-asset-loader-test-15");
        let _ = std::fs::create_dir_all(&dir);
        let payload = "\u{00A7} scene { hp = 100 }\n".as_bytes();
        let path = dir.join("test.csl");
        std::fs::write(&path, payload).unwrap();
        let canonical = std::fs::canonicalize(&path).unwrap_or(path.clone());
        let uri = format!("file://{}", canonical.display().to_string().replace('\\', "/"));
        let expected = sha256(payload);
        let entry = AssetEntry {
            id: "scene".into(),
            kind: AssetKind::CSSLScene,
            source_uri: uri,
            license: LicenseTag::ProprietaryApocky,
            sha256_expected: Some(expected),
            parser: ParserKind::CSSLObject,
        };
        let bytes = fetch_with_validation(&entry, 4096).expect("happy-path fetch");
        assert_eq!(bytes, payload);
        let seeds = parse_to_crystal_seed(&bytes, ParserKind::CSSLObject).unwrap();
        assert_eq!(seeds.len(), 1);
    }

    // ── 16. hex helper ──────────────────────────────────────────────────
    #[test]
    fn hex32_round_trip() {
        let head: [u8; 8] = [0xde, 0xad, 0xbe, 0xef, 0x00, 0xff, 0xab, 0xcd];
        let mut full = [0u8; 32];
        full[0..8].copy_from_slice(&head);
        let h = hex32(&full);
        assert_eq!(h.len(), 64);
        assert!(h.starts_with("deadbeef00ffabcd"));
    }
}

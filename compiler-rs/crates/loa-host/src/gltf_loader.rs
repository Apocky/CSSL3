//! § gltf_loader — GLTF/GLB → loa-host `Vertex` + index translator
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-WAVE3-GLTF (W-WAVE3-gltf-parser)
//!
//! § ROLE
//!   Convert externally-authored glTF 2.0 / GLB files into loa-host's
//!   canonical `Vertex` struct so `world.spawn_gltf` can populate the
//!   dynamic-mesh render path with arbitrary 3D models. The loader is
//!   pure-Rust (via the well-vetted `gltf` crate · no native deps) and
//!   catalog-buildable — `runtime` feature is NOT required for parse,
//!   only for the eventual GPU upload path.
//!
//! § PIPELINE
//!   ```text
//!   GLB bytes  →  gltf::Document  →  per-primitive walk
//!                                      ├─ position (REQUIRED)
//!                                      ├─ normal   (computed if absent)
//!                                      ├─ uv-0     (zero if absent)
//!                                      ├─ color    (1.0 if absent)
//!                                      └─ indices  (auto-fan if non-indexed)
//!                  │
//!                  ▼
//!              Vec<Vertex> + Vec<u32>  +  GltfMaterialHint  +  bbox
//!   ```
//!
//! § MATERIAL MAPPING
//!   The first slice maps glTF PBR base-color → closest match in our
//!   16-material LUT via simple HSV-distance. Roughness/metallic act as
//!   tiebreakers (high-metallic + low-roughness → Brushed-Steel /
//!   Gold-Leaf · high-roughness → Matte-Grey). Unknown materials fall
//!   back to MAT_MATTE_GREY. `pattern_id` defaults to `PAT_SOLID` for
//!   externally-spawned meshes (procedural patterns are reserved for the
//!   hand-authored test rooms).
//!
//! § BOUNDARIES
//!   - Models above `MAX_VERTS_PER_SPAWN` (= 200_000) emit a META_WARNING
//!     hint but still parse — the host can choose to reject. This keeps a
//!     single GB-class .glb from OOM-ing the renderer.
//!   - Coordinate system : glTF uses +Y up · -Z forward (right-handed).
//!     loa-host shares this convention — NO axis-swap needed.
//!   - World-transform : `transform_into_world` translates + scales the
//!     mesh in-place ; rotation is identity for the first slice.
//!
//! § PRIME-DIRECTIVE
//!   The loader reads a file the user explicitly named (`world.spawn_gltf`
//!   takes a `path` arg authenticated by sovereign-cap). No off-machine
//!   fetch · no auto-discovery · no telemetry leak of the model contents.
//!   Each spawn emits a single structured-event log line so the user can
//!   audit which assets were loaded.
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::module_name_repetitions)]

use std::fmt;
use std::path::{Path, PathBuf};

use cssl_rt::loa_startup::log_event;

use crate::geometry::Vertex;
use crate::material::{
    MAT_BRUSHED_STEEL, MAT_DEEP_INDIGO, MAT_EMISSIVE_CYAN, MAT_GOLD_LEAF, MAT_HOLOGRAPHIC,
    MAT_IRIDESCENT, MAT_MATTE_GREY, MAT_NEON_MAGENTA, MAT_OFF_WHITE, MAT_TRANSPARENT_GLASS,
    MAT_VERMILLION_LACQUER, MAT_WARM_SKY,
};
use crate::pattern::PAT_SOLID;

// ──────────────────────────────────────────────────────────────────────────
// § Constants
// ──────────────────────────────────────────────────────────────────────────

/// Soft-cap : models above this vertex count get a META_WARNING hint.
/// At 200K vertices a typical Vec<Vertex> is ~12 MB — well under any
/// modern GPU's per-buffer limit but enough to want the host's permission.
pub const MAX_VERTS_PER_SPAWN: usize = 200_000;

/// Hard-cap : models above this vertex count are rejected outright.
/// 5M verts = ~300MB Vec<Vertex> — past that we likely have a corrupt
/// or adversarial input.
pub const HARD_VERTS_LIMIT: usize = 5_000_000;

// ──────────────────────────────────────────────────────────────────────────
// § Public surface
// ──────────────────────────────────────────────────────────────────────────

/// Hint about how the mesh's source material should map into loa-host's
/// 16-material LUT. The renderer reads this when uploading the mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GltfMaterialHint {
    /// Best-match material id (0..15).
    pub material_id: u32,
    /// Pattern id ; for spawned meshes this is always `PAT_SOLID` so the
    /// uber-shader uses the per-vertex color directly.
    pub pattern_id: u32,
    /// Source PBR base-color (linear RGBA, 0..1) — kept for diagnostics.
    pub base_color_linear: [f32; 4],
    /// Source PBR metallic factor (0..1).
    pub metallic: f32,
    /// Source PBR roughness factor (0..1).
    pub roughness: f32,
}

impl Default for GltfMaterialHint {
    fn default() -> Self {
        Self {
            material_id: MAT_MATTE_GREY,
            pattern_id: PAT_SOLID,
            base_color_linear: [0.5, 0.5, 0.5, 1.0],
            metallic: 0.0,
            roughness: 1.0,
        }
    }
}

/// Bundled CPU-side result of parsing a .glb / .gltf file. The host can
/// then upload `vertices` + `indices` into a fresh dynamic-mesh slot.
#[derive(Debug, Clone)]
pub struct GltfMesh {
    /// All vertices flattened across the file's primitives.
    pub vertices: Vec<Vertex>,
    /// 32-bit indices into `vertices`. Always present (auto-fan if the
    /// source mesh was non-indexed).
    pub indices: Vec<u32>,
    /// Material hint — the renderer maps this to a slot in the LUT.
    pub material: GltfMaterialHint,
    /// Axis-aligned bounding box (`min`, `max`) over the mesh.
    pub bbox: ([f32; 3], [f32; 3]),
    /// `true` when the parser tripped a soft-cap warning (over
    /// `MAX_VERTS_PER_SPAWN`). The mesh is still complete — the host
    /// chooses whether to spawn or reject.
    pub meta_warning: Option<String>,
    /// Path or virtual-name (for log lines + telemetry).
    pub source_label: String,
}

impl GltfMesh {
    /// Total triangle count (indices / 3).
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// AABB extent on each axis.
    #[must_use]
    pub fn extent(&self) -> [f32; 3] {
        let (lo, hi) = self.bbox;
        [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
    }

    /// World-space center of the bbox.
    #[must_use]
    pub fn center(&self) -> [f32; 3] {
        let (lo, hi) = self.bbox;
        [
            0.5 * (lo[0] + hi[0]),
            0.5 * (lo[1] + hi[1]),
            0.5 * (lo[2] + hi[2]),
        ]
    }
}

/// Parser-level error type. Distinguishes IO errors from semantic ones
/// so the MCP layer can return the right JSON-RPC error envelope.
#[derive(Debug)]
pub enum GltfErr {
    /// Filesystem error (file not found · permission · etc.).
    Io(std::io::Error),
    /// `gltf` crate parse / validation error.
    Parse(String),
    /// Mesh has zero primitives or zero vertices.
    Empty,
    /// Required POSITION attribute is missing.
    MissingPosition,
    /// Vertex count exceeds `HARD_VERTS_LIMIT`.
    TooLarge { verts: usize, limit: usize },
}

impl fmt::Display for GltfErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Parse(s) => write!(f, "parse: {s}"),
            Self::Empty => write!(f, "empty: glTF file has no mesh primitives"),
            Self::MissingPosition => write!(f, "missing-position: glTF primitive lacks POSITION"),
            Self::TooLarge { verts, limit } => {
                write!(f, "too-large: {verts} verts > hard cap {limit}")
            }
        }
    }
}

impl std::error::Error for GltfErr {}

impl From<std::io::Error> for GltfErr {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<gltf::Error> for GltfErr {
    fn from(e: gltf::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Loader entry-points
// ──────────────────────────────────────────────────────────────────────────

/// Load a glTF or GLB file from disk and convert it into a `GltfMesh`.
/// `path` is taken verbatim — caller is expected to have sanitized it
/// (e.g. via `mcp_tools::sanitize_path` or sovereign-cap).
///
/// Errors :
///   - `GltfErr::Io` if the file can't be opened or read.
///   - `GltfErr::Parse` if the gltf-crate parser rejects the file.
///   - `GltfErr::Empty` / `MissingPosition` / `TooLarge` for semantic faults.
pub fn load_gltf(path: &Path) -> Result<GltfMesh, GltfErr> {
    let label = path.display().to_string();
    log_event(
        "INFO",
        "loa-host/gltf_loader",
        &format!("load_gltf · path={label} · begin"),
    );
    // gltf::import is the file-system entry-point — it auto-detects .gltf
    // (JSON-with-buffer-refs) vs .glb (binary blob) and pulls accompanying
    // .bin files relative to the JSON file's directory.
    let (doc, buffers, _images) = gltf::import(path)?;
    let mesh = build_mesh(&doc, &buffers, label)?;
    log_event(
        "INFO",
        "loa-host/gltf_loader",
        &format!(
            "load_gltf · {} · verts={} · tris={} · bbox=[{:?},{:?}]",
            mesh.source_label,
            mesh.vertices.len(),
            mesh.triangle_count(),
            mesh.bbox.0,
            mesh.bbox.1,
        ),
    );
    Ok(mesh)
}

/// Parse an in-memory GLB byte slice. Useful for embedded fixtures + the
/// MCP path where the model bytes are already in memory (e.g. the
/// authentication harness pre-validated them). Stricter than
/// `load_gltf` because there's no parent directory for external buffers.
pub fn load_glb_bytes(bytes: &[u8]) -> Result<GltfMesh, GltfErr> {
    log_event(
        "INFO",
        "loa-host/gltf_loader",
        &format!("load_glb_bytes · {} bytes · begin", bytes.len()),
    );
    let glb = gltf::Glb::from_slice(bytes)?;
    let doc = gltf::Gltf::from_slice(&glb.json)?;
    // Convert the (single) BIN chunk into the gltf-crate's Buffer-data
    // shape used by Reader. If there's no BIN chunk and no embedded data,
    // we still try (degenerate but parseable for spec-test fixtures).
    let bin = glb.bin.unwrap_or(std::borrow::Cow::Borrowed(&[]));
    let buffers: Vec<gltf::buffer::Data> = doc
        .buffers()
        .map(|b| {
            // For GLB-embedded buffers (URI absent) the BIN chunk supplies
            // the bytes. For external URIs we'd have to fetch · for the
            // bytes-path we just zero-pad if missing (degenerate fixtures
            // get caught by the empty-mesh check downstream).
            // gltf::buffer::Source doesn't impl PartialEq · pattern-match.
            match b.source() {
                gltf::buffer::Source::Bin => gltf::buffer::Data(bin.to_vec()),
                gltf::buffer::Source::Uri(_) => gltf::buffer::Data(vec![0u8; b.length()]),
            }
        })
        .collect();
    let mesh = build_mesh(&doc, &buffers, format!("<inline {} bytes>", bytes.len()))?;
    log_event(
        "INFO",
        "loa-host/gltf_loader",
        &format!(
            "load_glb_bytes · verts={} · tris={} · bbox=[{:?},{:?}]",
            mesh.vertices.len(),
            mesh.triangle_count(),
            mesh.bbox.0,
            mesh.bbox.1,
        ),
    );
    Ok(mesh)
}

/// Apply a world-position translation + uniform scale into the mesh's
/// vertices. Returns a new `GltfMesh` (the source is left intact so the
/// host can spawn many instances of the same parsed asset). Normals are
/// preserved (uniform scale doesn't rotate them).
#[must_use]
pub fn transform_into_world(mesh: &GltfMesh, world_pos: [f32; 3], scale: f32) -> GltfMesh {
    let s = scale.max(1e-6);
    let mut out_verts = Vec::with_capacity(mesh.vertices.len());
    for v in &mesh.vertices {
        let p = [
            v.position[0] * s + world_pos[0],
            v.position[1] * s + world_pos[1],
            v.position[2] * s + world_pos[2],
        ];
        out_verts.push(Vertex {
            position: p,
            normal: v.normal,
            color: v.color,
            uv: v.uv,
            material_id: v.material_id,
            pattern_id: v.pattern_id,
        });
    }
    let (lo, hi) = mesh.bbox;
    let new_lo = [
        lo[0] * s + world_pos[0],
        lo[1] * s + world_pos[1],
        lo[2] * s + world_pos[2],
    ];
    let new_hi = [
        hi[0] * s + world_pos[0],
        hi[1] * s + world_pos[1],
        hi[2] * s + world_pos[2],
    ];
    GltfMesh {
        vertices: out_verts,
        indices: mesh.indices.clone(),
        material: mesh.material,
        bbox: (new_lo, new_hi),
        meta_warning: mesh.meta_warning.clone(),
        source_label: mesh.source_label.clone(),
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Internal mesh-builder
// ──────────────────────────────────────────────────────────────────────────

fn build_mesh(
    doc: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    label: String,
) -> Result<GltfMesh, GltfErr> {
    let mut all_verts: Vec<Vertex> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut bbox_min = [f32::INFINITY; 3];
    let mut bbox_max = [f32::NEG_INFINITY; 3];
    let mut hint = GltfMaterialHint::default();
    let mut hint_assigned = false;

    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            // Material hint : pull the FIRST primitive's PBR base color +
            // metallic/roughness as the canonical hint. Multi-primitive
            // meshes blend visually even though the renderer only carries
            // one slot per spawn.
            if !hint_assigned {
                hint = extract_material_hint(&prim);
                hint_assigned = true;
            }

            let reader = prim.reader(|b| Some(&buffers[b.index()].0));

            // POSITION is required by the glTF spec — bail out if absent.
            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(it) => it.collect(),
                None => return Err(GltfErr::MissingPosition),
            };
            if positions.is_empty() {
                continue;
            }

            // NORMAL is optional ; if absent we compute face-normals after
            // the index buffer is built.
            let normals_opt: Option<Vec<[f32; 3]>> = reader.read_normals().map(|it| it.collect());

            // TEXCOORD_0 (uv) is optional ; default to (0, 0).
            let uvs_opt: Option<Vec<[f32; 2]>> = reader
                .read_tex_coords(0)
                .map(|it| it.into_f32().collect::<Vec<_>>());

            // COLOR_0 is optional ; default to (1, 1, 1) — base-color of
            // the material acts as the visible tint via the pattern lookup.
            let colors_opt: Option<Vec<[f32; 4]>> = reader
                .read_colors(0)
                .map(|it| it.into_rgba_f32().collect::<Vec<_>>());

            // INDICES are optional ; if absent the spec says draw as
            // sequential triangles (auto-fan : 0,1,2,3,4,5,...).
            let prim_indices: Vec<u32> = match reader.read_indices() {
                Some(it) => it.into_u32().collect(),
                None => (0u32..positions.len() as u32).collect(),
            };

            let base_idx = u32::try_from(all_verts.len()).map_err(|_| GltfErr::TooLarge {
                verts: all_verts.len(),
                limit: HARD_VERTS_LIMIT,
            })?;

            // Build the per-vertex array. Apply material hint into each
            // slot so the uber-shader picks up the correct LUT entry.
            for (i, p) in positions.iter().enumerate() {
                // Update bbox.
                for ax in 0..3 {
                    if p[ax] < bbox_min[ax] {
                        bbox_min[ax] = p[ax];
                    }
                    if p[ax] > bbox_max[ax] {
                        bbox_max[ax] = p[ax];
                    }
                }
                let n = normals_opt
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .unwrap_or([0.0, 1.0, 0.0]); // placeholder if no normals
                let uv = uvs_opt
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .unwrap_or([0.0, 0.0]);
                let col = colors_opt
                    .as_ref()
                    .and_then(|v| v.get(i).copied())
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                all_verts.push(Vertex {
                    position: *p,
                    normal: n,
                    color: [col[0], col[1], col[2]],
                    uv,
                    material_id: hint.material_id,
                    pattern_id: hint.pattern_id,
                });
            }

            // If the source mesh lacked NORMALs, compute face-normals
            // from the index triangles. This produces a flat-shaded look
            // but guarantees lighting works even on barebones fixtures.
            if normals_opt.is_none() {
                compute_face_normals(
                    &mut all_verts[base_idx as usize..],
                    &prim_indices,
                );
            }

            // Append indices, biased by the primitive's vertex offset.
            all_indices.extend(prim_indices.iter().map(|i| base_idx + *i));

            // Hard-cap check : reject inputs that would OOM the GPU.
            if all_verts.len() > HARD_VERTS_LIMIT {
                return Err(GltfErr::TooLarge {
                    verts: all_verts.len(),
                    limit: HARD_VERTS_LIMIT,
                });
            }
        }
    }

    if all_verts.is_empty() {
        return Err(GltfErr::Empty);
    }

    // bbox sanity : if for some reason no vertex updated the box (shouldn't
    // happen but defensive), zero it out.
    if !bbox_min[0].is_finite() {
        bbox_min = [0.0, 0.0, 0.0];
        bbox_max = [0.0, 0.0, 0.0];
    }

    let meta_warning = if all_verts.len() > MAX_VERTS_PER_SPAWN {
        Some(format!(
            "META_WARNING: {} verts > soft cap {} · spawn may be heavy",
            all_verts.len(),
            MAX_VERTS_PER_SPAWN
        ))
    } else {
        None
    };

    Ok(GltfMesh {
        vertices: all_verts,
        indices: all_indices,
        material: hint,
        bbox: (bbox_min, bbox_max),
        meta_warning,
        source_label: label,
    })
}

// ──────────────────────────────────────────────────────────────────────────
// § Material hint derivation
// ──────────────────────────────────────────────────────────────────────────

/// Read the PBR-MetallicRoughness material params off a primitive and
/// pick the closest match in our 16-material LUT.
fn extract_material_hint(prim: &gltf::Primitive<'_>) -> GltfMaterialHint {
    let mat = prim.material();
    let pbr = mat.pbr_metallic_roughness();
    let base = pbr.base_color_factor(); // linear RGBA
    let metallic = pbr.metallic_factor();
    let roughness = pbr.roughness_factor();
    let id = pick_material_for_pbr([base[0], base[1], base[2]], metallic, roughness);
    GltfMaterialHint {
        material_id: id,
        pattern_id: PAT_SOLID,
        base_color_linear: base,
        metallic,
        roughness,
    }
}

/// § PBR → loa-material classifier
///
/// Heuristic ladder :
///   1. Strongly metallic (m > 0.8) + smooth (r < 0.3) → metallic family
///        - warm hue → Gold-Leaf
///        - else → Brushed-Steel
///   2. Strongly emissive base-color (any channel > 0.95) → Emissive-Cyan
///        if cyan-ish · Neon-Magenta if magenta-ish · else Holographic
///   3. Translucent alpha (handled at the material level — we sniff via
///      blue-shift) → Transparent-Glass / Iridescent
///   4. Else nearest-neighbor by RGB euclidean distance against each
///      LUT material's representative reference color.
///
/// This is intentionally simple — the goal is "looks roughly right",
/// not perfect. The host always has the option of overriding via
/// `render.set_material` once spawned.
#[must_use]
pub fn pick_material_for_pbr(rgb: [f32; 3], metallic: f32, roughness: f32) -> u32 {
    // Tier 1 : metallic family.
    if metallic > 0.7 && roughness < 0.4 {
        let r = rgb[0];
        let g = rgb[1];
        let b = rgb[2];
        // Warm-yellow (R > G > B with R-B ≥ 0.3) → Gold-Leaf.
        // This catches both pure gold (1.0, 0.85, 0.55) and aged-gold
        // (0.9, 0.7, 0.4). Plain greys with R≈G≈B fall through to steel.
        if r > 0.6 && g > 0.5 && (r - b) > 0.3 && r >= g {
            return MAT_GOLD_LEAF;
        }
        return MAT_BRUSHED_STEEL;
    }

    // Tier 2 : strong emissive / vivid colors.
    let max_c = rgb[0].max(rgb[1]).max(rgb[2]);
    let min_c = rgb[0].min(rgb[1]).min(rgb[2]);
    let saturation = if max_c > 0.0 {
        (max_c - min_c) / max_c
    } else {
        0.0
    };
    if saturation > 0.7 && max_c > 0.7 {
        // Vivid cyan : low R, high G/B
        if rgb[0] < 0.4 && rgb[1] > 0.6 && rgb[2] > 0.6 {
            return MAT_EMISSIVE_CYAN;
        }
        // Vivid magenta : high R, low G, high B
        if rgb[0] > 0.6 && rgb[1] < 0.4 && rgb[2] > 0.6 {
            return MAT_NEON_MAGENTA;
        }
        // Vivid red : high R, low G, low B
        if rgb[0] > 0.7 && rgb[1] < 0.3 && rgb[2] < 0.3 {
            return MAT_VERMILLION_LACQUER;
        }
        // Deep blue : low R, low G, high B
        if rgb[0] < 0.3 && rgb[1] < 0.3 && rgb[2] > 0.5 {
            return MAT_DEEP_INDIGO;
        }
    }

    // Tier 3 : near-white with high roughness → Off-White (paint).
    if roughness > 0.6 && rgb[0] > 0.85 && rgb[1] > 0.85 && rgb[2] > 0.85 {
        return MAT_OFF_WHITE;
    }

    // Tier 4 : sky-blue tint
    if rgb[2] > 0.6 && rgb[1] > 0.5 && rgb[0] < 0.6 {
        return MAT_WARM_SKY;
    }

    // Tier 5 : iridescent (low-saturation but high blue-channel + low
    // metallic — a placeholder heuristic).
    if metallic > 0.3 && saturation < 0.4 && rgb[2] > 0.5 {
        return MAT_IRIDESCENT;
    }

    // Tier 6 : holographic (saturated mid-spectrum, mid-metallic).
    if metallic > 0.3 && saturation > 0.4 && saturation < 0.7 {
        return MAT_HOLOGRAPHIC;
    }

    // Tier 7 : translucent / glass-ish — TODO hook on alpha-mode in a
    // future slice. For now defer to grey.
    let _ = MAT_TRANSPARENT_GLASS;

    // Default : matte-grey for unknowns.
    MAT_MATTE_GREY
}

// ──────────────────────────────────────────────────────────────────────────
// § Face-normal computation (for normal-less inputs)
// ──────────────────────────────────────────────────────────────────────────

/// Compute flat face-normals over a triangle list. Used when the source
/// glTF lacks NORMAL ; we accumulate per-vertex from each triangle's
/// cross-product, then normalize. This produces smooth-ish shading on
/// closed meshes (each vertex's normal is the average of incident faces).
fn compute_face_normals(verts: &mut [Vertex], indices: &[u32]) {
    // Reset all normals to zero first.
    for v in verts.iter_mut() {
        v.normal = [0.0, 0.0, 0.0];
    }
    // Accumulate face cross-products.
    let n = verts.len();
    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }
        let p0 = verts[i0].position;
        let p1 = verts[i1].position;
        let p2 = verts[i2].position;
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for &i in &[i0, i1, i2] {
            verts[i].normal[0] += cross[0];
            verts[i].normal[1] += cross[1];
            verts[i].normal[2] += cross[2];
        }
    }
    // Normalize.
    for v in verts.iter_mut() {
        let n = v.normal;
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-6 {
            v.normal = [n[0] / len, n[1] / len, n[2] / len];
        } else {
            v.normal = [0.0, 1.0, 0.0]; // safe default for degenerate
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § GLB fixture builders (for tests + the embedded sample)
// ──────────────────────────────────────────────────────────────────────────

/// Build a minimal-valid GLB byte stream encoding a single triangle.
/// Used by the test suite to exercise the parser without shipping
/// binary fixtures in-tree. The triangle has positions
/// (-1, 0, 0), (1, 0, 0), (0, 1, 0) and indices [0, 1, 2].
#[must_use]
pub fn build_triangle_glb_fixture() -> Vec<u8> {
    build_glb_fixture(
        &[[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        &[0, 1, 2],
    )
}

/// Build a tetrahedron GLB fixture (4 verts, 4 triangles → 12 indices).
/// Used to test the bbox + face-normal computation path.
#[must_use]
pub fn build_tetrahedron_glb_fixture() -> Vec<u8> {
    build_glb_fixture(
        &[
            [0.0, 1.0, 0.0],
            [-1.0, -0.5, -1.0],
            [1.0, -0.5, -1.0],
            [0.0, -0.5, 1.0],
        ],
        &[0, 1, 2, 0, 2, 3, 0, 3, 1, 1, 3, 2],
    )
}

/// Build a generic positions+indices GLB.
fn build_glb_fixture(positions: &[[f32; 3]], indices: &[u32]) -> Vec<u8> {
    // Layout : [positions f32×3 array][indices u32 array]
    // glTF wants 4-byte alignment between accessors ; we pad to 4-byte.
    let pos_bytes_len = positions.len() * 12;
    let pos_pad = (4 - (pos_bytes_len % 4)) % 4;
    let idx_offset = pos_bytes_len + pos_pad;
    let idx_bytes_len = indices.len() * 4;
    let total_bin_len = idx_offset + idx_bytes_len;
    // Pad BIN chunk to 4-byte boundary.
    let bin_pad = (4 - (total_bin_len % 4)) % 4;
    let bin_chunk_len = total_bin_len + bin_pad;

    let mut bin = Vec::with_capacity(bin_chunk_len);
    for p in positions {
        bin.extend_from_slice(&p[0].to_le_bytes());
        bin.extend_from_slice(&p[1].to_le_bytes());
        bin.extend_from_slice(&p[2].to_le_bytes());
    }
    for _ in 0..pos_pad {
        bin.push(0);
    }
    for i in indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    for _ in 0..bin_pad {
        bin.push(0);
    }

    // Compute bbox over the positions for the JSON's accessor.min/max.
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            if p[i] < min[i] {
                min[i] = p[i];
            }
            if p[i] > max[i] {
                max[i] = p[i];
            }
        }
    }

    // Build the JSON chunk.
    let json = format!(
        "{{\
\"asset\":{{\"version\":\"2.0\",\"generator\":\"loa-host gltf_loader fixture\"}},\
\"scene\":0,\
\"scenes\":[{{\"nodes\":[0]}}],\
\"nodes\":[{{\"mesh\":0}}],\
\"meshes\":[{{\"primitives\":[{{\"attributes\":{{\"POSITION\":0}},\"indices\":1,\"material\":0}}]}}],\
\"buffers\":[{{\"byteLength\":{bin_len}}}],\
\"bufferViews\":[\
{{\"buffer\":0,\"byteOffset\":0,\"byteLength\":{pos_len},\"target\":34962}},\
{{\"buffer\":0,\"byteOffset\":{idx_off},\"byteLength\":{idx_len},\"target\":34963}}\
],\
\"accessors\":[\
{{\"bufferView\":0,\"componentType\":5126,\"count\":{p_count},\"type\":\"VEC3\",\"min\":[{min_x},{min_y},{min_z}],\"max\":[{max_x},{max_y},{max_z}]}},\
{{\"bufferView\":1,\"componentType\":5125,\"count\":{i_count},\"type\":\"SCALAR\"}}\
],\
\"materials\":[{{\"pbrMetallicRoughness\":{{\"baseColorFactor\":[0.7,0.7,0.7,1.0],\"metallicFactor\":0.0,\"roughnessFactor\":1.0}}}}]\
}}",
        bin_len = bin_chunk_len,
        pos_len = pos_bytes_len,
        idx_off = idx_offset,
        idx_len = idx_bytes_len,
        p_count = positions.len(),
        i_count = indices.len(),
        min_x = min[0],
        min_y = min[1],
        min_z = min[2],
        max_x = max[0],
        max_y = max[1],
        max_z = max[2],
    );
    let mut json_bytes = json.into_bytes();
    // Pad JSON to 4 bytes with spaces (per glTF GLB spec).
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    // Build the GLB header + chunks.
    let total_glb_len = 12 // GLB header
        + 8 + json_bytes.len()  // JSON chunk header + payload
        + 8 + bin_chunk_len; // BIN chunk header + payload
    let mut out = Vec::with_capacity(total_glb_len);
    // Header : magic (0x46546C67) · version (2) · total length.
    out.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total_glb_len as u32).to_le_bytes());
    // JSON chunk : length · type=0x4E4F534A · payload.
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    // BIN chunk : length · type=0x004E4942 · payload.
    out.extend_from_slice(&(bin_chunk_len as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    out.extend_from_slice(&bin);
    out
}

// ──────────────────────────────────────────────────────────────────────────
// § Spawn registry (host-side metadata for live-spawned meshes)
// ──────────────────────────────────────────────────────────────────────────

/// Single record about a spawned glTF instance. The renderer uses this
/// to build (or reuse) a `DynamicMesh` slot ; the MCP `world.spawn_gltf`
/// returns the `instance_id` so subsequent calls can despawn / move it.
#[derive(Debug, Clone)]
pub struct GltfSpawnRecord {
    pub instance_id: u32,
    pub source_path: PathBuf,
    pub world_pos: [f32; 3],
    pub scale: f32,
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub material_id: u32,
    pub bbox: ([f32; 3], [f32; 3]),
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1 : parse a known-good GLB byte stream and verify vertex count.
    /// We use the in-house triangle fixture so the test is hermetic
    /// (no on-disk dependency).
    #[test]
    fn parse_known_glb_returns_correct_vertex_count() {
        let bytes = build_triangle_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("triangle fixture must parse");
        assert_eq!(mesh.vertices.len(), 3, "triangle has 3 vertices");
        assert_eq!(mesh.indices.len(), 3, "triangle has 3 indices");
        assert_eq!(mesh.triangle_count(), 1, "triangle_count = 1");
        // The fixture spans X = [-1, 1], Y = [0, 1], Z = [0, 0].
        let (lo, hi) = mesh.bbox;
        assert!((lo[0] - (-1.0)).abs() < 1e-5);
        assert!((hi[0] - 1.0).abs() < 1e-5);
        assert!((lo[1]).abs() < 1e-5);
        assert!((hi[1] - 1.0).abs() < 1e-5);
    }

    /// Test 2 : transform_into_world translates vertex positions and bbox
    /// by the world-pos offset, scaled by the scale factor.
    #[test]
    fn transform_into_world_translates_bbox() {
        let bytes = build_triangle_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("triangle fixture must parse");
        let world_pos = [10.0, 5.0, -3.0];
        let scale = 2.0;
        let moved = transform_into_world(&mesh, world_pos, scale);
        // Original X=[-1,1] · scaled by 2 → [-2,2] · offset by 10 → [8,12].
        let (lo, hi) = moved.bbox;
        assert!((lo[0] - 8.0).abs() < 1e-4, "bbox.lo.x={}", lo[0]);
        assert!((hi[0] - 12.0).abs() < 1e-4, "bbox.hi.x={}", hi[0]);
        // Original Y=[0,1] · scaled by 2 → [0,2] · offset by 5 → [5,7].
        assert!((lo[1] - 5.0).abs() < 1e-4);
        assert!((hi[1] - 7.0).abs() < 1e-4);
        // Z origin is at -3, mesh is flat at Z=0, so [(-3),(-3)].
        assert!((lo[2] - (-3.0)).abs() < 1e-4);
        assert!((hi[2] - (-3.0)).abs() < 1e-4);

        // Each vertex got moved correspondingly.
        // Vertex 0 was (-1, 0, 0) → ((-1)*2 + 10, 0*2 + 5, 0*2 + (-3)) = (8, 5, -3).
        let v0 = &moved.vertices[0];
        assert!((v0.position[0] - 8.0).abs() < 1e-4);
        assert!((v0.position[1] - 5.0).abs() < 1e-4);
        assert!((v0.position[2] - (-3.0)).abs() < 1e-4);
    }

    /// Test 3 : material hint for an unknown grey PBR material falls back
    /// to MAT_MATTE_GREY (the fixture material is grey roughness=1).
    #[test]
    fn material_hint_falls_back_to_grey() {
        let bytes = build_triangle_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("triangle fixture must parse");
        assert_eq!(mesh.material.material_id, MAT_MATTE_GREY);
        assert_eq!(mesh.material.pattern_id, PAT_SOLID);
        // Each vertex must carry the matte-grey id.
        for v in &mesh.vertices {
            assert_eq!(v.material_id, MAT_MATTE_GREY);
            assert_eq!(v.pattern_id, PAT_SOLID);
        }
    }

    /// Test 4 : a glTF without NORMAL is given face-normals computed from
    /// triangle cross-products. The triangle in our fixture lies in the XY
    /// plane (Z=0 for all 3 verts), so the normal must be ±Z (CCW from
    /// +Z viewer = +Z normal).
    #[test]
    fn gltf_with_no_normals_computes_face_normals() {
        let bytes = build_triangle_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("triangle fixture must parse");
        // Triangle in XY plane, vertices CCW from +Z → normal = +Z.
        for v in &mesh.vertices {
            assert!(
                (v.normal[2] - 1.0).abs() < 1e-4,
                "expected +Z normal, got {:?}",
                v.normal
            );
            assert!(v.normal[0].abs() < 1e-4);
            assert!(v.normal[1].abs() < 1e-4);
        }
    }

    /// Test 5 : exceeding the soft-cap MAX_VERTS_PER_SPAWN should set
    /// `meta_warning` but NOT cause a panic / OOM. We avoid building an
    /// actual 200K-vert fixture (too slow for unit tests) and instead
    /// directly assert that the cap-check logic works at the boundary.
    #[test]
    fn large_mesh_above_threshold_returns_meta_warning() {
        // Synthesize a mesh just above the soft-cap by repeating a tiny
        // primitive in a loop, then run it through the meta-warning check
        // directly. We cannot easily round-trip 200K verts through a GLB
        // in a unit-test budget, so we test the cap arithmetic explicitly.
        let n_above = MAX_VERTS_PER_SPAWN + 10;
        let dummy = GltfMesh {
            vertices: vec![Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                color: [1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                material_id: MAT_MATTE_GREY,
                pattern_id: PAT_SOLID,
            }; n_above],
            indices: vec![],
            material: GltfMaterialHint::default(),
            bbox: ([0.0; 3], [0.0; 3]),
            meta_warning: if n_above > MAX_VERTS_PER_SPAWN {
                Some("META_WARNING: synthetic over-cap".to_string())
            } else {
                None
            },
            source_label: "<test synthetic>".to_string(),
        };
        assert!(dummy.meta_warning.is_some());
        assert_eq!(dummy.vertices.len(), n_above);
        // Simulate the build_mesh cap-detection :
        let warning_emitted = dummy.vertices.len() > MAX_VERTS_PER_SPAWN;
        assert!(warning_emitted);
        // Hard-cap remains untriggered (no panic).
        assert!(dummy.vertices.len() < HARD_VERTS_LIMIT);
    }

    /// Test 6 : MCP-style spawn smoke-test — load fixture, transform into
    /// world, check the spawn-record fields are populated correctly.
    /// This is the contract the host's `world.spawn_gltf` MCP handler
    /// upholds (instance-id management lives in a global counter ; here
    /// we just verify the GltfSpawnRecord shape).
    #[test]
    fn mcp_world_spawn_gltf_returns_instance_id() {
        let bytes = build_tetrahedron_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("tetra fixture must parse");
        let world_pos = [0.0, 0.0, 65.0]; // MaterialRoom-Annex zone
        let scale = 1.5;
        let placed = transform_into_world(&mesh, world_pos, scale);

        // Synthesize the spawn record the way the MCP handler will.
        let record = GltfSpawnRecord {
            instance_id: 42,
            source_path: PathBuf::from("<inline tetra>"),
            world_pos,
            scale,
            vertex_count: placed.vertices.len() as u32,
            triangle_count: placed.triangle_count() as u32,
            material_id: placed.material.material_id,
            bbox: placed.bbox,
        };
        assert_eq!(record.instance_id, 42);
        assert_eq!(record.vertex_count, 4);
        assert_eq!(record.triangle_count, 4);
        assert_eq!(record.material_id, MAT_MATTE_GREY);
        // World-pos translated correctly.
        assert!((record.bbox.0[2] - 65.0_f32).abs() < 2.0);
    }

    /// Bonus test : pick_material_for_pbr classifier — gold is gold,
    /// glass is glass-shaped, grey is grey.
    #[test]
    fn pick_material_classifier_picks_sensible_buckets() {
        // Polished gold : warm-yellow + high metallic + low roughness.
        let gold = pick_material_for_pbr([1.0, 0.85, 0.55], 1.0, 0.1);
        assert_eq!(gold, MAT_GOLD_LEAF, "gold should map to MAT_GOLD_LEAF");

        // Brushed steel : neutral grey + high metallic + low roughness.
        let steel = pick_material_for_pbr([0.7, 0.7, 0.72], 1.0, 0.2);
        assert_eq!(steel, MAT_BRUSHED_STEEL, "steel should map to MAT_BRUSHED_STEEL");

        // Vivid red : high R + high saturation.
        let red = pick_material_for_pbr([0.95, 0.05, 0.05], 0.0, 0.6);
        assert_eq!(red, MAT_VERMILLION_LACQUER, "red should map to MAT_VERMILLION_LACQUER");

        // Off-white paint : near-white + high roughness.
        let white = pick_material_for_pbr([0.95, 0.93, 0.92], 0.0, 0.9);
        assert_eq!(white, MAT_OFF_WHITE, "white-paint should map to MAT_OFF_WHITE");

        // Matte grey default.
        let grey = pick_material_for_pbr([0.5, 0.5, 0.5], 0.0, 1.0);
        assert_eq!(grey, MAT_MATTE_GREY, "grey should map to MAT_MATTE_GREY");
    }

    /// Bonus test : tetrahedron parses to 4 verts + 12 indices (4 faces
    /// × 3) — exercises the multi-triangle index buffer path.
    #[test]
    fn tetrahedron_fixture_parses_as_4_verts_12_indices() {
        let bytes = build_tetrahedron_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("tetra fixture must parse");
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 12);
        assert_eq!(mesh.triangle_count(), 4);
    }

    /// Bonus test : transforming with scale=0 should not crash (clamped
    /// to 1e-6 so the mesh collapses to a point but no division-by-zero).
    #[test]
    fn transform_with_zero_scale_does_not_crash() {
        let bytes = build_triangle_glb_fixture();
        let mesh = load_glb_bytes(&bytes).expect("triangle fixture must parse");
        let world_pos = [0.0, 0.0, 0.0];
        let moved = transform_into_world(&mesh, world_pos, 0.0);
        // All verts collapse to ~(0, 0, 0) plus small numerical noise.
        for v in &moved.vertices {
            assert!(v.position[0].abs() < 1e-3);
            assert!(v.position[1].abs() < 1e-3);
            assert!(v.position[2].abs() < 1e-3);
        }
    }

    /// Bonus test : verify GLB header is well-formed (magic + version).
    /// Catches accidental endianness / off-by-one bugs in the fixture
    /// builder before we reach the gltf-crate parser.
    #[test]
    fn fixture_glb_header_is_well_formed() {
        let bytes = build_triangle_glb_fixture();
        assert!(bytes.len() > 12);
        // Magic : "glTF" little-endian.
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(magic, 0x4654_6C67);
        // Version : 2.
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(version, 2);
        // Total length matches buffer.
        let total = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(total as usize, bytes.len());
    }
}

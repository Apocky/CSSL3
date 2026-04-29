//! GLTF 2.0 parser — JSON manifest + binary `.glb` container.
//!
//! § SCOPE (stage-0)
//!   - DECODE : `.glb` (binary) container per the glTF 2.0 spec :
//!              12-byte header + JSON chunk + (optional) BIN chunk.
//!   - DECODE : `.gltf` (text) JSON manifest. External buffers /
//!              images are NOT auto-fetched at stage-0 ; the caller
//!              receives URI strings and resolves them.
//!   - WALK   : scene-graph traversal (nodes → meshes → primitives →
//!              accessors → bufferViews → buffers).
//!   - DECODE : animations, skins, materials, textures, images at
//!              the JSON-structure level (real content interpretation
//!              is the consumer's responsibility).
//!
//! § DELIBERATELY DEFERRED (stage-0)
//!   - Real GLB → buffer-view → typed-accessor decoding (we expose the
//!     metadata + raw byte slices ; consumer interprets them).
//!   - JSON-extension consumption (`KHR_lights_punctual`, etc. — exposed
//!     as raw key-value Maps).
//!   - URI-encoded data: scheme decoding (consumer fetches external
//!     bytes).
//!   - Validation against the schema (we trust the JSON's well-formedness
//!     ; structural mismatches surface during accessor lookup).
//!
//! § JSON PARSER
//!   We hand-roll a minimal JSON parser sufficient for glTF manifests :
//!   objects + arrays + strings + numbers + booleans + null. No support
//!   for streaming, extended numbers (NaN / Infinity), or Unicode
//!   escapes beyond `\uXXXX` BMP code points.
//!
//! § PRIME-DIRECTIVE
//!   The parser caps JSON depth at `MAX_JSON_DEPTH` and string lengths
//!   at the buffer's reported size. No URI auto-fetch (the consumer
//!   provides bytes for external resources). No surveillance.

use crate::error::{AssetError, Result};
use std::collections::BTreeMap;

/// Maximum nesting depth the JSON parser will descend into.
pub const MAX_JSON_DEPTH: usize = 64;

/// GLB magic bytes (`glTF`).
pub const GLB_MAGIC: u32 = 0x4654_6c67;
/// GLB JSON chunk type (`JSON`).
pub const CHUNK_JSON: u32 = 0x4e4f_534a;
/// GLB BIN chunk type (`BIN\0`).
pub const CHUNK_BIN: u32 = 0x004e_4942;

// ─────────────────────────────────────────────────────────────────────────
// § JSON value (minimal)
// ─────────────────────────────────────────────────────────────────────────

/// Minimal JSON value sufficient for glTF manifests.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// IEEE 754 double.
    Number(f64),
    /// UTF-8 string (already unescaped).
    String(String),
    /// Array of values.
    Array(Vec<JsonValue>),
    /// Object with string keys.
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    /// Borrow as object, or `None`.
    #[must_use]
    pub const fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        if let Self::Object(m) = self {
            Some(m)
        } else {
            None
        }
    }

    /// Borrow as array, or `None`.
    #[must_use]
    pub const fn as_array(&self) -> Option<&Vec<JsonValue>> {
        if let Self::Array(a) = self {
            Some(a)
        } else {
            None
        }
    }

    /// Borrow as string, or `None`.
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }

    /// Borrow as number, or `None`.
    #[must_use]
    pub const fn as_number(&self) -> Option<f64> {
        if let Self::Number(n) = self {
            Some(*n)
        } else {
            None
        }
    }

    /// Borrow as boolean, or `None`.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }
}

/// Parse JSON text into a `JsonValue`.
pub fn parse_json(input: &str) -> Result<JsonValue> {
    let mut p = JsonParser {
        bytes: input.as_bytes(),
        pos: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(AssetError::invalid(
            "GLTF",
            "json",
            format!("trailing bytes after value at pos {}", p.pos),
        ));
    }
    Ok(v)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> JsonParser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue> {
        if self.depth >= MAX_JSON_DEPTH {
            return Err(AssetError::invalid(
                "GLTF",
                "json",
                format!("max depth {MAX_JSON_DEPTH} exceeded"),
            ));
        }
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return Err(AssetError::truncated("GLTF/json-value", 1, 0));
        }
        match self.bytes[self.pos] {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => Ok(JsonValue::String(self.parse_string()?)),
            b't' | b'f' => self.parse_bool(),
            b'n' => self.parse_null(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            other => Err(AssetError::invalid(
                "GLTF",
                "json",
                format!("unexpected byte 0x{other:02x} at pos {}", self.pos),
            )),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue> {
        self.pos += 1; // consume '{'
        self.depth += 1;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'}' {
            self.pos += 1;
            self.depth -= 1;
            return Ok(JsonValue::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_ws();
            if self.pos < self.bytes.len() {
                match self.bytes[self.pos] {
                    b',' => {
                        self.pos += 1;
                    }
                    b'}' => {
                        self.pos += 1;
                        self.depth -= 1;
                        return Ok(JsonValue::Object(map));
                    }
                    other => {
                        return Err(AssetError::invalid(
                            "GLTF",
                            "json-object",
                            format!("expected , or }} after value, got 0x{other:02x}"),
                        ));
                    }
                }
            } else {
                return Err(AssetError::truncated("GLTF/json-object", 1, 0));
            }
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue> {
        self.pos += 1; // consume '['
        self.depth += 1;
        let mut arr = Vec::new();
        self.skip_ws();
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b']' {
            self.pos += 1;
            self.depth -= 1;
            return Ok(JsonValue::Array(arr));
        }
        loop {
            self.skip_ws();
            let v = self.parse_value()?;
            arr.push(v);
            self.skip_ws();
            if self.pos < self.bytes.len() {
                match self.bytes[self.pos] {
                    b',' => {
                        self.pos += 1;
                    }
                    b']' => {
                        self.pos += 1;
                        self.depth -= 1;
                        return Ok(JsonValue::Array(arr));
                    }
                    other => {
                        return Err(AssetError::invalid(
                            "GLTF",
                            "json-array",
                            format!("expected , or ] after value, got 0x{other:02x}"),
                        ));
                    }
                }
            } else {
                return Err(AssetError::truncated("GLTF/json-array", 1, 0));
            }
        }
    }

    fn parse_string(&mut self) -> Result<String> {
        self.expect(b'"')?;
        let mut s = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            match b {
                b'"' => return Ok(s),
                b'\\' => {
                    if self.pos >= self.bytes.len() {
                        return Err(AssetError::truncated("GLTF/json-string-escape", 1, 0));
                    }
                    let esc = self.bytes[self.pos];
                    self.pos += 1;
                    match esc {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{0008}'),
                        b'f' => s.push('\u{000c}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => {
                            if self.pos + 4 > self.bytes.len() {
                                return Err(AssetError::truncated(
                                    "GLTF/json-string-unicode",
                                    4,
                                    self.bytes.len() - self.pos,
                                ));
                            }
                            let hex = &self.bytes[self.pos..self.pos + 4];
                            self.pos += 4;
                            let mut code: u32 = 0;
                            for &h in hex {
                                let v = match h {
                                    b'0'..=b'9' => h - b'0',
                                    b'a'..=b'f' => 10 + (h - b'a'),
                                    b'A'..=b'F' => 10 + (h - b'A'),
                                    _ => {
                                        return Err(AssetError::invalid(
                                            "GLTF",
                                            "json-string-unicode",
                                            format!("non-hex byte 0x{h:02x}"),
                                        ));
                                    }
                                };
                                code = (code << 4) | u32::from(v);
                            }
                            // Stage-0 supports BMP code points only ; surrogate
                            // pairs surface as their high-surrogate code point
                            // (caller can re-pair if needed).
                            if let Some(c) = char::from_u32(code) {
                                s.push(c);
                            } else {
                                s.push('\u{fffd}');
                            }
                        }
                        other => {
                            return Err(AssetError::invalid(
                                "GLTF",
                                "json-string",
                                format!("unknown escape \\\\0x{other:02x}"),
                            ));
                        }
                    }
                }
                _ => {
                    s.push(b as char);
                }
            }
        }
        Err(AssetError::truncated("GLTF/json-string-end", 1, 0))
    }

    fn parse_bool(&mut self) -> Result<JsonValue> {
        if self.bytes[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(JsonValue::Bool(true))
        } else if self.bytes[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(JsonValue::Bool(false))
        } else {
            Err(AssetError::invalid(
                "GLTF",
                "json-bool",
                "expected true or false",
            ))
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue> {
        if self.bytes[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(JsonValue::Null)
        } else {
            Err(AssetError::invalid("GLTF", "json-null", "expected null"))
        }
    }

    fn parse_number(&mut self) -> Result<JsonValue> {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-' => self.pos += 1,
                _ => break,
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| AssetError::invalid("GLTF", "json-number", "non-utf8 number bytes"))?;
        let n: f64 = s.parse().map_err(|_| {
            AssetError::invalid("GLTF", "json-number", format!("invalid number `{s}`"))
        })?;
        Ok(JsonValue::Number(n))
    }

    fn expect(&mut self, b: u8) -> Result<()> {
        if self.pos >= self.bytes.len() || self.bytes[self.pos] != b {
            let saw = if self.pos < self.bytes.len() {
                format!("0x{:02x}", self.bytes[self.pos])
            } else {
                "EOF".into()
            };
            return Err(AssetError::invalid(
                "GLTF",
                "json",
                format!("expected 0x{b:02x}, saw {saw}"),
            ));
        }
        self.pos += 1;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § GLTF SCHEMA SUBSET
// ─────────────────────────────────────────────────────────────────────────

/// Parsed GLTF document. Stores the top-level arrays + raw binary buffer
/// (for `.glb`).
#[derive(Debug, Clone, PartialEq)]
pub struct GltfDocument {
    /// Asset metadata (`asset.version` etc.).
    pub asset: GltfAsset,
    /// Default scene index, or `None` if not specified.
    pub default_scene: Option<usize>,
    /// All scenes in the file.
    pub scenes: Vec<Scene>,
    /// All nodes (referenced by scenes).
    pub nodes: Vec<Node>,
    /// All meshes (referenced by nodes).
    pub meshes: Vec<Mesh>,
    /// All accessors.
    pub accessors: Vec<Accessor>,
    /// All bufferViews.
    pub buffer_views: Vec<BufferView>,
    /// All buffers (length only ; bytes in `binary_buffer`).
    pub buffers: Vec<Buffer>,
    /// All animations (structure only).
    pub animations: Vec<Animation>,
    /// All skins (structure only).
    pub skins: Vec<Skin>,
    /// All materials (structure only).
    pub materials: Vec<Material>,
    /// All images (URI / bufferView reference).
    pub images: Vec<Image>,
    /// All textures (sampler+source pair).
    pub textures: Vec<Texture>,
    /// Embedded binary buffer (from `.glb` BIN chunk). `None` for
    /// text-mode `.gltf` files.
    pub binary_buffer: Option<Vec<u8>>,
}

/// Asset metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfAsset {
    /// glTF version string (e.g. "2.0").
    pub version: String,
    /// Optional generator string.
    pub generator: Option<String>,
}

/// Scene (root of node graph).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    /// Optional human name.
    pub name: Option<String>,
    /// Root node indices.
    pub nodes: Vec<usize>,
}

/// Node (transform + optional mesh + children).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Optional human name.
    pub name: Option<String>,
    /// Mesh index (or `None`).
    pub mesh: Option<usize>,
    /// Skin index (or `None`).
    pub skin: Option<usize>,
    /// Translation `[x, y, z]`. Defaults to `[0, 0, 0]`.
    pub translation: [f32; 3],
    /// Rotation quaternion `[x, y, z, w]`. Defaults to `[0, 0, 0, 1]`.
    pub rotation: [f32; 4],
    /// Scale `[x, y, z]`. Defaults to `[1, 1, 1]`.
    pub scale: [f32; 3],
    /// Optional 4x4 matrix (column-major). Mutually exclusive with TRS.
    pub matrix: Option<[f32; 16]>,
    /// Child node indices.
    pub children: Vec<usize>,
}

/// Mesh (a list of primitives).
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Optional human name.
    pub name: Option<String>,
    /// One mesh primitive per draw call.
    pub primitives: Vec<MeshPrimitive>,
}

/// Mesh primitive (attribute → accessor map + indices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshPrimitive {
    /// Attribute → accessor-index map (POSITION, NORMAL, TANGENT,
    /// TEXCOORD_0, etc.).
    pub attributes: BTreeMap<String, usize>,
    /// Indices accessor (or `None` for non-indexed draw).
    pub indices: Option<usize>,
    /// Material index (or `None`).
    pub material: Option<usize>,
    /// Topology mode (5=TRIANGLE_STRIP etc. ; default 4=TRIANGLES).
    pub mode: u32,
}

/// Accessor (typed view into a bufferView).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accessor {
    /// Optional bufferView index.
    pub buffer_view: Option<usize>,
    /// Byte offset within the bufferView.
    pub byte_offset: usize,
    /// Component type (5120=BYTE, 5121=UBYTE, 5122=SHORT, 5123=USHORT,
    /// 5125=UINT, 5126=FLOAT).
    pub component_type: u32,
    /// Element count.
    pub count: usize,
    /// Type ("SCALAR", "VEC2", "VEC3", "VEC4", "MAT2", "MAT3", "MAT4").
    pub type_: String,
    /// Whether integer types are mapped to `[-1, 1]` / `[0, 1]` floats.
    pub normalized: bool,
}

impl Accessor {
    /// Bytes per component for this accessor's component type.
    #[must_use]
    pub const fn bytes_per_component(&self) -> usize {
        match self.component_type {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => 0,
        }
    }

    /// Components per element for this accessor's type string.
    #[must_use]
    pub fn components_per_element(&self) -> usize {
        match self.type_.as_str() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" | "MAT2" => 4,
            "MAT3" => 9,
            "MAT4" => 16,
            _ => 0,
        }
    }

    /// Total bytes this accessor describes (count * components * bpc).
    #[must_use]
    pub fn byte_length(&self) -> usize {
        self.count * self.components_per_element() * self.bytes_per_component()
    }
}

/// BufferView (slice into a Buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferView {
    /// Buffer index.
    pub buffer: usize,
    /// Byte offset within the buffer.
    pub byte_offset: usize,
    /// Byte length.
    pub byte_length: usize,
    /// Optional stride (interleaved attributes).
    pub byte_stride: Option<usize>,
    /// Optional usage hint (34962=ARRAY_BUFFER, 34963=ELEMENT_ARRAY_BUFFER).
    pub target: Option<u32>,
}

/// Buffer (size only ; raw bytes live in `GltfDocument::binary_buffer`
/// for embedded-buffer GLBs, or are fetched by the consumer for `uri`-
/// referenced text-mode glTF).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    /// URI for external buffers ("data:..." or relative path) ; `None`
    /// for the GLB-embedded buffer.
    pub uri: Option<String>,
    /// Byte length.
    pub byte_length: usize,
}

/// Animation (channels + samplers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    /// Optional name.
    pub name: Option<String>,
    /// Channels — each binds a sampler output to a node target property.
    pub channels: Vec<AnimChannel>,
    /// Samplers — each is an input/output accessor pair.
    pub samplers: Vec<AnimSampler>,
}

/// One animation channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimChannel {
    /// Sampler index (within the same animation).
    pub sampler: usize,
    /// Target node + property (translation / rotation / scale / weights).
    pub target_node: Option<usize>,
    /// Target property string.
    pub target_path: String,
}

/// Animation sampler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimSampler {
    /// Input accessor (time keyframes).
    pub input: usize,
    /// Output accessor (value keyframes).
    pub output: usize,
    /// Interpolation ("LINEAR", "STEP", "CUBICSPLINE"). Default "LINEAR".
    pub interpolation: String,
}

/// Skin (joint hierarchy + inverse-bind matrices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skin {
    /// Optional name.
    pub name: Option<String>,
    /// Inverse-bind-matrices accessor (matrix-per-joint).
    pub inverse_bind_matrices: Option<usize>,
    /// Joint node indices.
    pub joints: Vec<usize>,
    /// Optional skeleton root node.
    pub skeleton: Option<usize>,
}

/// Material (PBR-Metallic-Roughness subset).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Optional name.
    pub name: Option<String>,
    /// Base color factor [r, g, b, a].
    pub base_color_factor: [f32; 4],
    /// Optional base-color texture index.
    pub base_color_texture: Option<usize>,
    /// Metallic factor.
    pub metallic_factor: f32,
    /// Roughness factor.
    pub roughness_factor: f32,
    /// Optional normal-texture index.
    pub normal_texture: Option<usize>,
    /// Alpha mode ("OPAQUE", "MASK", "BLEND"). Default "OPAQUE".
    pub alpha_mode: String,
    /// Whether geometry is double-sided.
    pub double_sided: bool,
}

/// Image (URI or bufferView reference).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// URI for external images, or `None` if bufferView-referenced.
    pub uri: Option<String>,
    /// MIME type (e.g. "image/png").
    pub mime_type: Option<String>,
    /// BufferView index for embedded images.
    pub buffer_view: Option<usize>,
}

/// Texture (sampler + source image).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Texture {
    /// Optional sampler index.
    pub sampler: Option<usize>,
    /// Optional image source index.
    pub source: Option<usize>,
}

// ─────────────────────────────────────────────────────────────────────────
// § GLB CONTAINER
// ─────────────────────────────────────────────────────────────────────────

/// Decode a `.glb` byte stream.
pub fn decode_glb(bytes: &[u8]) -> Result<GltfDocument> {
    if bytes.len() < 12 {
        return Err(AssetError::truncated("GLB/header", 12, bytes.len()));
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != GLB_MAGIC {
        return Err(AssetError::bad_magic("GLB", &bytes[..4]));
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(AssetError::unsupported(
            "GLB",
            format!("version {version} (stage-0 supports 2)"),
        ));
    }
    let total_len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if total_len > bytes.len() {
        return Err(AssetError::truncated("GLB/length", total_len, bytes.len()));
    }
    let mut offset = 12usize;
    let mut json_chunk: Option<&[u8]> = None;
    let mut bin_chunk: Option<&[u8]> = None;
    while offset + 8 <= total_len {
        let chunk_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_type = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        let body_start = offset + 8;
        let body_end = body_start
            .checked_add(chunk_len)
            .ok_or_else(|| AssetError::invalid("GLB", "chunk", "len overflow"))?;
        if body_end > total_len {
            return Err(AssetError::truncated(
                "GLB/chunk-body",
                body_end - offset,
                total_len - offset,
            ));
        }
        match chunk_type {
            CHUNK_JSON => json_chunk = Some(&bytes[body_start..body_end]),
            CHUNK_BIN => bin_chunk = Some(&bytes[body_start..body_end]),
            _ => {} // unknown chunks per spec are skipped silently
        }
        offset = body_end;
    }
    let json_chunk = json_chunk
        .ok_or_else(|| AssetError::invalid("GLB", "chunks", "missing required JSON chunk"))?;
    // Trim padding ASCII spaces from JSON chunk (per spec).
    let json_str = std::str::from_utf8(json_chunk)
        .map_err(|_| AssetError::invalid("GLB", "JSON", "non-utf8 json chunk"))?
        .trim_end_matches([' ', '\0']);
    let value = parse_json(json_str)?;
    let mut doc = build_document(&value)?;
    if let Some(bin) = bin_chunk {
        // Trim trailing zero padding (per spec — BIN chunks may be padded
        // to 4 bytes with 0x00).
        let mut end = bin.len();
        while end > 0 && bin[end - 1] == 0 {
            end -= 1;
        }
        doc.binary_buffer = Some(bin[..end].to_vec());
    }
    Ok(doc)
}

/// Decode a `.gltf` text-mode JSON document. The caller resolves
/// external URIs separately.
pub fn decode_gltf(json_text: &str) -> Result<GltfDocument> {
    let value = parse_json(json_text)?;
    build_document(&value)
}

fn build_document(value: &JsonValue) -> Result<GltfDocument> {
    let root = value
        .as_object()
        .ok_or_else(|| AssetError::invalid("GLTF", "root", "must be an object"))?;
    let asset = parse_asset(root.get("asset"))?;
    let default_scene = root
        .get("scene")
        .and_then(JsonValue::as_number)
        .map(|n| n as usize);
    let scenes = parse_scenes(root.get("scenes"))?;
    let nodes = parse_nodes(root.get("nodes"))?;
    let meshes = parse_meshes(root.get("meshes"))?;
    let accessors = parse_accessors(root.get("accessors"))?;
    let buffer_views = parse_buffer_views(root.get("bufferViews"))?;
    let buffers = parse_buffers(root.get("buffers"))?;
    let animations = parse_animations(root.get("animations"))?;
    let skins = parse_skins(root.get("skins"))?;
    let materials = parse_materials(root.get("materials"))?;
    let images = parse_images(root.get("images"))?;
    let textures = parse_textures(root.get("textures"))?;
    Ok(GltfDocument {
        asset,
        default_scene,
        scenes,
        nodes,
        meshes,
        accessors,
        buffer_views,
        buffers,
        animations,
        skins,
        materials,
        images,
        textures,
        binary_buffer: None,
    })
}

fn parse_asset(value: Option<&JsonValue>) -> Result<GltfAsset> {
    let obj = value
        .and_then(JsonValue::as_object)
        .ok_or_else(|| AssetError::invalid("GLTF", "asset", "missing required asset object"))?;
    let version = obj
        .get("version")
        .and_then(JsonValue::as_string)
        .ok_or_else(|| AssetError::invalid("GLTF", "asset.version", "missing version string"))?
        .to_string();
    if !version.starts_with("2.") {
        return Err(AssetError::unsupported(
            "GLTF",
            format!("version `{version}` (stage-0 supports 2.x)"),
        ));
    }
    let generator = obj
        .get("generator")
        .and_then(JsonValue::as_string)
        .map(ToString::to_string);
    Ok(GltfAsset { version, generator })
}

fn each_object<F, T>(arr: Option<&JsonValue>, mut f: F) -> Result<Vec<T>>
where
    F: FnMut(&BTreeMap<String, JsonValue>) -> Result<T>,
{
    match arr {
        None => Ok(Vec::new()),
        Some(v) => {
            let a = v
                .as_array()
                .ok_or_else(|| AssetError::invalid("GLTF", "array", "expected array"))?;
            let mut out = Vec::with_capacity(a.len());
            for item in a {
                let obj = item
                    .as_object()
                    .ok_or_else(|| AssetError::invalid("GLTF", "array-item", "expected object"))?;
                out.push(f(obj)?);
            }
            Ok(out)
        }
    }
}

fn opt_index(obj: &BTreeMap<String, JsonValue>, key: &str) -> Option<usize> {
    obj.get(key)
        .and_then(JsonValue::as_number)
        .map(|n| n as usize)
}

fn opt_string(obj: &BTreeMap<String, JsonValue>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_string())
        .map(ToString::to_string)
}

fn opt_bool(obj: &BTreeMap<String, JsonValue>, key: &str) -> Option<bool> {
    obj.get(key).and_then(JsonValue::as_bool)
}

fn parse_index_array(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<Vec<usize>> {
    match obj.get(key) {
        None => Ok(Vec::new()),
        Some(v) => {
            let a = v
                .as_array()
                .ok_or_else(|| AssetError::invalid("GLTF", key, "expected array"))?;
            let mut out = Vec::with_capacity(a.len());
            for item in a {
                let n = item
                    .as_number()
                    .ok_or_else(|| AssetError::invalid("GLTF", key, "expected number array"))?;
                out.push(n as usize);
            }
            Ok(out)
        }
    }
}

fn parse_float_array<const N: usize>(
    obj: &BTreeMap<String, JsonValue>,
    key: &str,
    default: [f32; N],
) -> Result<[f32; N]> {
    match obj.get(key) {
        None => Ok(default),
        Some(v) => {
            let a = v
                .as_array()
                .ok_or_else(|| AssetError::invalid("GLTF", key, "expected array"))?;
            if a.len() != N {
                return Err(AssetError::invalid(
                    "GLTF",
                    key,
                    format!("expected {N} elements, got {}", a.len()),
                ));
            }
            let mut out = [0f32; N];
            for (i, item) in a.iter().enumerate() {
                let n = item
                    .as_number()
                    .ok_or_else(|| AssetError::invalid("GLTF", key, "expected number array"))?;
                out[i] = n as f32;
            }
            Ok(out)
        }
    }
}

fn parse_scenes(value: Option<&JsonValue>) -> Result<Vec<Scene>> {
    each_object(value, |obj| {
        Ok(Scene {
            name: opt_string(obj, "name"),
            nodes: parse_index_array(obj, "nodes")?,
        })
    })
}

fn parse_nodes(value: Option<&JsonValue>) -> Result<Vec<Node>> {
    each_object(value, |obj| {
        let matrix = match obj.get("matrix") {
            Some(v) => {
                let a = v
                    .as_array()
                    .ok_or_else(|| AssetError::invalid("GLTF", "node.matrix", "expected array"))?;
                if a.len() != 16 {
                    return Err(AssetError::invalid(
                        "GLTF",
                        "node.matrix",
                        format!("expected 16 elements, got {}", a.len()),
                    ));
                }
                let mut out = [0f32; 16];
                for (i, item) in a.iter().enumerate() {
                    let n = item.as_number().ok_or_else(|| {
                        AssetError::invalid("GLTF", "node.matrix", "expected numbers")
                    })?;
                    out[i] = n as f32;
                }
                Some(out)
            }
            None => None,
        };
        Ok(Node {
            name: opt_string(obj, "name"),
            mesh: opt_index(obj, "mesh"),
            skin: opt_index(obj, "skin"),
            translation: parse_float_array::<3>(obj, "translation", [0.0; 3])?,
            rotation: parse_float_array::<4>(obj, "rotation", [0.0, 0.0, 0.0, 1.0])?,
            scale: parse_float_array::<3>(obj, "scale", [1.0; 3])?,
            matrix,
            children: parse_index_array(obj, "children")?,
        })
    })
}

fn parse_meshes(value: Option<&JsonValue>) -> Result<Vec<Mesh>> {
    each_object(value, |obj| {
        let prims_value = obj
            .get("primitives")
            .ok_or_else(|| AssetError::invalid("GLTF", "mesh.primitives", "missing"))?;
        let prims_arr = prims_value
            .as_array()
            .ok_or_else(|| AssetError::invalid("GLTF", "mesh.primitives", "expected array"))?;
        let mut primitives = Vec::with_capacity(prims_arr.len());
        for p in prims_arr {
            let p_obj = p
                .as_object()
                .ok_or_else(|| AssetError::invalid("GLTF", "primitive", "expected object"))?;
            let mut attributes = BTreeMap::new();
            if let Some(JsonValue::Object(attrs)) = p_obj.get("attributes") {
                for (k, v) in attrs {
                    if let JsonValue::Number(n) = v {
                        attributes.insert(k.clone(), *n as usize);
                    }
                }
            }
            let mode = p_obj
                .get("mode")
                .and_then(JsonValue::as_number)
                .map(|n| n as u32)
                .unwrap_or(4);
            primitives.push(MeshPrimitive {
                attributes,
                indices: opt_index(p_obj, "indices"),
                material: opt_index(p_obj, "material"),
                mode,
            });
        }
        Ok(Mesh {
            name: opt_string(obj, "name"),
            primitives,
        })
    })
}

fn parse_accessors(value: Option<&JsonValue>) -> Result<Vec<Accessor>> {
    each_object(value, |obj| {
        let component_type = obj
            .get("componentType")
            .and_then(JsonValue::as_number)
            .ok_or_else(|| {
                AssetError::invalid(
                    "GLTF",
                    "accessor.componentType",
                    "missing required componentType",
                )
            })? as u32;
        let count = obj
            .get("count")
            .and_then(JsonValue::as_number)
            .ok_or_else(|| {
                AssetError::invalid("GLTF", "accessor.count", "missing required count")
            })? as usize;
        let type_ = obj
            .get("type")
            .and_then(JsonValue::as_string)
            .ok_or_else(|| AssetError::invalid("GLTF", "accessor.type", "missing required type"))?
            .to_string();
        Ok(Accessor {
            buffer_view: opt_index(obj, "bufferView"),
            byte_offset: obj
                .get("byteOffset")
                .and_then(JsonValue::as_number)
                .map(|n| n as usize)
                .unwrap_or(0),
            component_type,
            count,
            type_,
            normalized: opt_bool(obj, "normalized").unwrap_or(false),
        })
    })
}

fn parse_buffer_views(value: Option<&JsonValue>) -> Result<Vec<BufferView>> {
    each_object(value, |obj| {
        let buffer = opt_index(obj, "buffer").ok_or_else(|| {
            AssetError::invalid("GLTF", "bufferView.buffer", "missing required buffer index")
        })?;
        let byte_length = obj
            .get("byteLength")
            .and_then(JsonValue::as_number)
            .ok_or_else(|| {
                AssetError::invalid(
                    "GLTF",
                    "bufferView.byteLength",
                    "missing required byteLength",
                )
            })? as usize;
        Ok(BufferView {
            buffer,
            byte_offset: obj
                .get("byteOffset")
                .and_then(JsonValue::as_number)
                .map(|n| n as usize)
                .unwrap_or(0),
            byte_length,
            byte_stride: obj
                .get("byteStride")
                .and_then(JsonValue::as_number)
                .map(|n| n as usize),
            target: obj
                .get("target")
                .and_then(JsonValue::as_number)
                .map(|n| n as u32),
        })
    })
}

fn parse_buffers(value: Option<&JsonValue>) -> Result<Vec<Buffer>> {
    each_object(value, |obj| {
        let byte_length = obj
            .get("byteLength")
            .and_then(JsonValue::as_number)
            .ok_or_else(|| {
                AssetError::invalid("GLTF", "buffer.byteLength", "missing required byteLength")
            })? as usize;
        Ok(Buffer {
            uri: opt_string(obj, "uri"),
            byte_length,
        })
    })
}

fn parse_animations(value: Option<&JsonValue>) -> Result<Vec<Animation>> {
    each_object(value, |obj| {
        let channels = match obj.get("channels") {
            Some(JsonValue::Array(a)) => {
                let mut out = Vec::with_capacity(a.len());
                for c in a {
                    let c_obj = c.as_object().ok_or_else(|| {
                        AssetError::invalid("GLTF", "anim.channel", "expected object")
                    })?;
                    let sampler = opt_index(c_obj, "sampler").ok_or_else(|| {
                        AssetError::invalid("GLTF", "anim.channel.sampler", "missing sampler index")
                    })?;
                    let target_obj = c_obj
                        .get("target")
                        .and_then(JsonValue::as_object)
                        .ok_or_else(|| {
                            AssetError::invalid(
                                "GLTF",
                                "anim.channel.target",
                                "missing target object",
                            )
                        })?;
                    let target_path = target_obj
                        .get("path")
                        .and_then(JsonValue::as_string)
                        .ok_or_else(|| {
                            AssetError::invalid("GLTF", "anim.channel.target.path", "missing path")
                        })?
                        .to_string();
                    out.push(AnimChannel {
                        sampler,
                        target_node: opt_index(target_obj, "node"),
                        target_path,
                    });
                }
                out
            }
            _ => Vec::new(),
        };
        let samplers = match obj.get("samplers") {
            Some(JsonValue::Array(a)) => {
                let mut out = Vec::with_capacity(a.len());
                for s in a {
                    let s_obj = s.as_object().ok_or_else(|| {
                        AssetError::invalid("GLTF", "anim.sampler", "expected object")
                    })?;
                    let input = opt_index(s_obj, "input").ok_or_else(|| {
                        AssetError::invalid("GLTF", "anim.sampler.input", "missing")
                    })?;
                    let output = opt_index(s_obj, "output").ok_or_else(|| {
                        AssetError::invalid("GLTF", "anim.sampler.output", "missing")
                    })?;
                    let interpolation =
                        opt_string(s_obj, "interpolation").unwrap_or_else(|| "LINEAR".to_string());
                    out.push(AnimSampler {
                        input,
                        output,
                        interpolation,
                    });
                }
                out
            }
            _ => Vec::new(),
        };
        Ok(Animation {
            name: opt_string(obj, "name"),
            channels,
            samplers,
        })
    })
}

fn parse_skins(value: Option<&JsonValue>) -> Result<Vec<Skin>> {
    each_object(value, |obj| {
        Ok(Skin {
            name: opt_string(obj, "name"),
            inverse_bind_matrices: opt_index(obj, "inverseBindMatrices"),
            joints: parse_index_array(obj, "joints")?,
            skeleton: opt_index(obj, "skeleton"),
        })
    })
}

fn parse_materials(value: Option<&JsonValue>) -> Result<Vec<Material>> {
    each_object(value, |obj| {
        let pbr = obj
            .get("pbrMetallicRoughness")
            .and_then(JsonValue::as_object);
        let base_color_factor = match pbr.and_then(|p| p.get("baseColorFactor")) {
            Some(JsonValue::Array(a)) if a.len() == 4 => {
                let mut out = [1f32; 4];
                for (i, v) in a.iter().enumerate() {
                    out[i] = v.as_number().unwrap_or(1.0) as f32;
                }
                out
            }
            _ => [1.0, 1.0, 1.0, 1.0],
        };
        let base_color_texture = pbr
            .and_then(|p| p.get("baseColorTexture"))
            .and_then(JsonValue::as_object)
            .and_then(|t| t.get("index"))
            .and_then(JsonValue::as_number)
            .map(|n| n as usize);
        let metallic_factor = pbr
            .and_then(|p| p.get("metallicFactor"))
            .and_then(JsonValue::as_number)
            .map(|n| n as f32)
            .unwrap_or(1.0);
        let roughness_factor = pbr
            .and_then(|p| p.get("roughnessFactor"))
            .and_then(JsonValue::as_number)
            .map(|n| n as f32)
            .unwrap_or(1.0);
        let normal_texture = obj
            .get("normalTexture")
            .and_then(JsonValue::as_object)
            .and_then(|t| t.get("index"))
            .and_then(JsonValue::as_number)
            .map(|n| n as usize);
        Ok(Material {
            name: opt_string(obj, "name"),
            base_color_factor,
            base_color_texture,
            metallic_factor,
            roughness_factor,
            normal_texture,
            alpha_mode: opt_string(obj, "alphaMode").unwrap_or_else(|| "OPAQUE".to_string()),
            double_sided: opt_bool(obj, "doubleSided").unwrap_or(false),
        })
    })
}

fn parse_images(value: Option<&JsonValue>) -> Result<Vec<Image>> {
    each_object(value, |obj| {
        Ok(Image {
            uri: opt_string(obj, "uri"),
            mime_type: opt_string(obj, "mimeType"),
            buffer_view: opt_index(obj, "bufferView"),
        })
    })
}

fn parse_textures(value: Option<&JsonValue>) -> Result<Vec<Texture>> {
    each_object(value, |obj| {
        Ok(Texture {
            sampler: opt_index(obj, "sampler"),
            source: opt_index(obj, "source"),
        })
    })
}

// ─────────────────────────────────────────────────────────────────────────
// § SCENE-GRAPH WALK
// ─────────────────────────────────────────────────────────────────────────

impl GltfDocument {
    /// Walk the scene graph rooted at `scene` (or the default scene) in
    /// depth-first order, invoking `visit(node_index, depth)` for each
    /// node.
    pub fn walk_scene<F>(&self, scene_index: Option<usize>, mut visit: F) -> Result<()>
    where
        F: FnMut(usize, usize),
    {
        let scene_idx = scene_index.or(self.default_scene).ok_or_else(|| {
            AssetError::invalid("GLTF", "scene", "no default scene + none specified")
        })?;
        let scene = self.scenes.get(scene_idx).ok_or_else(|| {
            AssetError::invalid(
                "GLTF",
                "scene",
                format!("scene index {scene_idx} out of range"),
            )
        })?;
        for &root in &scene.nodes {
            self.walk_node(root, 0, &mut visit)?;
        }
        Ok(())
    }

    fn walk_node<F>(&self, node_idx: usize, depth: usize, visit: &mut F) -> Result<()>
    where
        F: FnMut(usize, usize),
    {
        if depth > 1024 {
            return Err(AssetError::invalid(
                "GLTF",
                "scene-graph",
                "max walk depth exceeded (cycle?)",
            ));
        }
        let node = self.nodes.get(node_idx).ok_or_else(|| {
            AssetError::invalid(
                "GLTF",
                "node",
                format!("node index {node_idx} out of range"),
            )
        })?;
        visit(node_idx, depth);
        for &child in &node.children {
            self.walk_node(child, depth + 1, visit)?;
        }
        Ok(())
    }

    /// Resolve an accessor's byte slice from the binary buffer.
    /// Returns `None` if the accessor is sparse-only / lacks a bufferView.
    pub fn accessor_bytes(&self, accessor_idx: usize) -> Result<Option<&[u8]>> {
        let acc = self.accessors.get(accessor_idx).ok_or_else(|| {
            AssetError::invalid(
                "GLTF",
                "accessor",
                format!("accessor index {accessor_idx} out of range"),
            )
        })?;
        let view_idx = match acc.buffer_view {
            Some(v) => v,
            None => return Ok(None),
        };
        let view = self.buffer_views.get(view_idx).ok_or_else(|| {
            AssetError::invalid(
                "GLTF",
                "bufferView",
                format!("bufferView index {view_idx} out of range"),
            )
        })?;
        if view.buffer != 0 {
            // Stage-0 only handles buffer 0 (the embedded GLB BIN).
            return Err(AssetError::unsupported(
                "GLTF",
                "external buffer (only buffer 0 supported at stage-0)",
            ));
        }
        let data = self.binary_buffer.as_deref().ok_or_else(|| {
            AssetError::invalid(
                "GLTF",
                "binary",
                "no binary buffer available (text-mode glTF)",
            )
        })?;
        let start = view.byte_offset + acc.byte_offset;
        let end = start + acc.byte_length();
        if end > data.len() {
            return Err(AssetError::truncated(
                "GLTF/accessor-bytes",
                end,
                data.len(),
            ));
        }
        Ok(Some(&data[start..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthesize_minimal_glb() -> Vec<u8> {
        // Construct a minimal GLB :
        //   - 1 buffer of 12 bytes (3 × VEC3 of f32 ... wait, that's
        //     36 bytes ; let's just use 12 bytes of a single VEC3
        //     position).
        // Actually let's build a simpler one : 1 buffer = 12 bytes,
        // 1 bufferView 0..12, 1 accessor (1 × VEC3 FLOAT), 1 mesh
        // primitive {POSITION = 0}, 1 node.mesh = 0, 1 scene.nodes = [0].
        let bin: Vec<u8> = {
            let mut v = Vec::new();
            // VEC3 of (1.0, 2.0, 3.0).
            v.extend_from_slice(&1.0f32.to_le_bytes());
            v.extend_from_slice(&2.0f32.to_le_bytes());
            v.extend_from_slice(&3.0f32.to_le_bytes());
            v
        };
        // Pad bin to 4-byte alignment.
        let mut padded_bin = bin.clone();
        while padded_bin.len() % 4 != 0 {
            padded_bin.push(0);
        }
        let json = format!(
            r#"{{
  "asset": {{ "version": "2.0", "generator": "cssl-asset/test" }},
  "scene": 0,
  "scenes": [ {{ "name": "root", "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0, "name": "n0" }} ],
  "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }} }} ] }} ],
  "accessors": [ {{ "bufferView": 0, "componentType": 5126, "count": 1, "type": "VEC3" }} ],
  "bufferViews": [ {{ "buffer": 0, "byteOffset": 0, "byteLength": {} }} ],
  "buffers": [ {{ "byteLength": {} }} ]
}}"#,
            bin.len(),
            bin.len()
        );
        // Pad JSON to 4-byte alignment with spaces (per spec).
        let mut json_bytes = json.into_bytes();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        // Total length = 12 (header) + 8 (json header) + json + 8 (bin header) + bin.
        let total = 12 + 8 + json_bytes.len() + 8 + padded_bin.len();
        let mut out = Vec::with_capacity(total);
        // GLB header.
        out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        // JSON chunk.
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        out.extend_from_slice(&json_bytes);
        // BIN chunk.
        out.extend_from_slice(&(padded_bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(&padded_bin);
        out
    }

    #[test]
    fn parse_simple_json_object() {
        let v = parse_json(r#"{"a": 1, "b": "two", "c": true, "d": null}"#).unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o.get("a").unwrap().as_number(), Some(1.0));
        assert_eq!(o.get("b").unwrap().as_string(), Some("two"));
        assert_eq!(o.get("c").unwrap().as_bool(), Some(true));
        assert!(matches!(o.get("d").unwrap(), JsonValue::Null));
    }

    #[test]
    fn parse_nested_array() {
        let v = parse_json("[1, [2, 3], [4, [5, 6]]]").unwrap();
        let a = v.as_array().unwrap();
        assert_eq!(a.len(), 3);
    }

    #[test]
    fn parse_string_with_escapes() {
        let v = parse_json(r#""hello\nworld\t!""#).unwrap();
        assert_eq!(v.as_string(), Some("hello\nworld\t!"));
    }

    #[test]
    fn parse_number_negative_and_decimal() {
        let v = parse_json("-1.5e2").unwrap();
        assert_eq!(v.as_number(), Some(-150.0));
    }

    #[test]
    fn parse_rejects_trailing_garbage() {
        let r = parse_json("123abc");
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn parse_rejects_unmatched_brace() {
        let r = parse_json("{");
        assert!(r.is_err());
    }

    #[test]
    fn parse_rejects_excessive_depth() {
        let mut s = String::new();
        for _ in 0..(MAX_JSON_DEPTH + 5) {
            s.push('[');
        }
        let r = parse_json(&s);
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn decode_glb_minimal_succeeds() {
        let bytes = synthesize_minimal_glb();
        let doc = decode_glb(&bytes).unwrap();
        assert_eq!(doc.asset.version, "2.0");
        assert_eq!(doc.scenes.len(), 1);
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.meshes.len(), 1);
        assert_eq!(doc.accessors.len(), 1);
        assert_eq!(doc.buffer_views.len(), 1);
        assert_eq!(doc.buffers.len(), 1);
        assert!(doc.binary_buffer.is_some());
        assert_eq!(doc.binary_buffer.as_ref().unwrap().len(), 12);
    }

    #[test]
    fn decode_glb_walk_scene_visits_root_node() {
        let bytes = synthesize_minimal_glb();
        let doc = decode_glb(&bytes).unwrap();
        let mut visited = Vec::new();
        doc.walk_scene(None, |idx, depth| visited.push((idx, depth)))
            .unwrap();
        assert_eq!(visited, vec![(0, 0)]);
    }

    #[test]
    fn decode_glb_accessor_bytes_returns_correct_slice() {
        let bytes = synthesize_minimal_glb();
        let doc = decode_glb(&bytes).unwrap();
        let slice = doc.accessor_bytes(0).unwrap().expect("accessor bytes");
        assert_eq!(slice.len(), 12);
        assert_eq!(
            f32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]),
            1.0
        );
    }

    #[test]
    fn decode_glb_rejects_short_input() {
        let r = decode_glb(&[0u8; 4]);
        assert!(matches!(r, Err(AssetError::Truncated { .. })));
    }

    #[test]
    fn decode_glb_rejects_bad_magic() {
        let mut bytes = vec![0u8; 16];
        bytes[0..4].copy_from_slice(b"BADX");
        let r = decode_glb(&bytes);
        assert!(matches!(r, Err(AssetError::BadMagic { .. })));
    }

    #[test]
    fn decode_glb_rejects_unsupported_version() {
        let mut bytes = synthesize_minimal_glb();
        // Set version field to 3 (offset 4..8).
        bytes[4..8].copy_from_slice(&3u32.to_le_bytes());
        let r = decode_glb(&bytes);
        assert!(matches!(r, Err(AssetError::UnsupportedKind { .. })));
    }

    #[test]
    fn decode_gltf_text_mode() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "scenes": [{"nodes": [0]}],
            "nodes": [{}],
            "scene": 0
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.asset.version, "2.0");
        assert_eq!(doc.default_scene, Some(0));
        assert_eq!(doc.nodes.len(), 1);
        assert!(doc.binary_buffer.is_none());
    }

    #[test]
    fn decode_gltf_rejects_old_version() {
        let json = r#"{ "asset": {"version": "1.0"} }"#;
        let r = decode_gltf(json);
        assert!(matches!(r, Err(AssetError::UnsupportedKind { .. })));
    }

    #[test]
    fn decode_gltf_default_node_trs() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "nodes": [{}]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.nodes[0].translation, [0.0; 3]);
        assert_eq!(doc.nodes[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(doc.nodes[0].scale, [1.0; 3]);
    }

    #[test]
    fn decode_gltf_walks_nested_node_graph() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [
                {"children": [1]},
                {"children": [2, 3]},
                {},
                {}
            ]
        }"#;
        let doc = decode_gltf(json).unwrap();
        let mut visited = Vec::new();
        doc.walk_scene(None, |idx, depth| visited.push((idx, depth)))
            .unwrap();
        assert_eq!(visited, vec![(0, 0), (1, 1), (2, 2), (3, 2)]);
    }

    #[test]
    fn accessor_bytes_per_component_table() {
        let mut a = Accessor {
            buffer_view: None,
            byte_offset: 0,
            component_type: 5120, // BYTE
            count: 1,
            type_: "SCALAR".into(),
            normalized: false,
        };
        assert_eq!(a.bytes_per_component(), 1);
        a.component_type = 5126; // FLOAT
        assert_eq!(a.bytes_per_component(), 4);
        a.component_type = 5123; // USHORT
        assert_eq!(a.bytes_per_component(), 2);
    }

    #[test]
    fn accessor_components_per_element_table() {
        let mut a = Accessor {
            buffer_view: None,
            byte_offset: 0,
            component_type: 5126,
            count: 1,
            type_: "VEC3".into(),
            normalized: false,
        };
        assert_eq!(a.components_per_element(), 3);
        assert_eq!(a.byte_length(), 12);
        a.type_ = "MAT4".into();
        assert_eq!(a.components_per_element(), 16);
        assert_eq!(a.byte_length(), 64);
    }

    #[test]
    fn json_value_as_methods_on_wrong_variant_return_none() {
        let v = JsonValue::Number(1.0);
        assert!(v.as_object().is_none());
        assert!(v.as_array().is_none());
        assert!(v.as_string().is_none());
        assert!(v.as_bool().is_none());
        assert_eq!(v.as_number(), Some(1.0));
    }

    #[test]
    fn parse_animation_with_channels() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "animations": [{
                "channels": [{
                    "sampler": 0,
                    "target": {"node": 0, "path": "translation"}
                }],
                "samplers": [{
                    "input": 0,
                    "output": 1,
                    "interpolation": "LINEAR"
                }]
            }]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.animations.len(), 1);
        assert_eq!(doc.animations[0].channels.len(), 1);
        assert_eq!(doc.animations[0].channels[0].target_path, "translation");
        assert_eq!(doc.animations[0].samplers[0].interpolation, "LINEAR");
    }

    #[test]
    fn parse_skin_with_joints() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "skins": [{
                "joints": [1, 2, 3],
                "skeleton": 1
            }]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.skins.len(), 1);
        assert_eq!(doc.skins[0].joints, vec![1, 2, 3]);
        assert_eq!(doc.skins[0].skeleton, Some(1));
    }

    #[test]
    fn parse_material_with_pbr() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "materials": [{
                "name": "red",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [1.0, 0.0, 0.0, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.5
                }
            }]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.materials.len(), 1);
        assert_eq!(doc.materials[0].name.as_deref(), Some("red"));
        assert_eq!(doc.materials[0].base_color_factor, [1.0, 0.0, 0.0, 1.0]);
        assert!((doc.materials[0].metallic_factor - 0.0).abs() < f32::EPSILON);
        assert!((doc.materials[0].roughness_factor - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_image_with_buffer_view() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "images": [{
                "bufferView": 0,
                "mimeType": "image/png"
            }]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.images.len(), 1);
        assert_eq!(doc.images[0].buffer_view, Some(0));
        assert_eq!(doc.images[0].mime_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn parse_texture_pair() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "textures": [{"sampler": 0, "source": 0}]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert_eq!(doc.textures.len(), 1);
        assert_eq!(doc.textures[0].sampler, Some(0));
        assert_eq!(doc.textures[0].source, Some(0));
    }

    #[test]
    fn rejects_glb_missing_json() {
        let mut out = Vec::new();
        out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&12u32.to_le_bytes()); // total = 12 (header only)
        let r = decode_glb(&out);
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn walk_scene_with_explicit_index() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "scenes": [
                {"nodes": [0]},
                {"nodes": [1]}
            ],
            "nodes": [{}, {}]
        }"#;
        let doc = decode_gltf(json).unwrap();
        let mut visited = Vec::new();
        doc.walk_scene(Some(1), |idx, _| visited.push(idx)).unwrap();
        assert_eq!(visited, vec![1]);
    }

    #[test]
    fn walk_scene_no_default_errors() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "scenes": [{"nodes": [0]}],
            "nodes": [{}]
        }"#;
        let doc = decode_gltf(json).unwrap();
        let r = doc.walk_scene(None, |_, _| {});
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn parse_node_matrix_when_present() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "nodes": [{
                "matrix": [1,0,0,0, 0,1,0,0, 0,0,1,0, 5,6,7,1]
            }]
        }"#;
        let doc = decode_gltf(json).unwrap();
        assert!(doc.nodes[0].matrix.is_some());
        let m = doc.nodes[0].matrix.unwrap();
        assert_eq!(m[12], 5.0);
        assert_eq!(m[13], 6.0);
        assert_eq!(m[14], 7.0);
    }
}

//! § cssl-asset round-trip integration tests
//! ═══════════════════════════════════════════
//!
//! These integration tests provide the report-back round-trip checks
//! per the slice charter :
//!   (a) PNG : load known-good PNG → re-encode → byte-equal
//!   (b) WAV : load known-good WAV → PCM equality
//!   (c) GLB : load known-good GLB → walk scene graph
//!
//! Stage-0 we generate the "known-good" inputs in-process from the
//! encoders themselves. This is the same strategy as cssl-host-audio's
//! integration tests : the encoder is the spec, and we verify the
//! decoder agrees.
//!
//! ‼ DXT-compression of PNG (mentioned in the report-back) is a
//! follow-up slice — the GPU-side compressor isn't wired at stage-0.
//! What we DO verify here is the harder property : encode → decode →
//! re-encode is byte-stable, which is the necessary precondition for
//! any lossless compression pipeline that wraps it.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::unnecessary_wraps)]

use cssl_asset::{
    decode_glb, decode_png, decode_wav, encode_png, encode_wav, png, ColorType, GltfDocument,
    PngImage, SampleFormat, WavFile,
};

/// (a) PNG round-trip : encode → decode → re-encode → byte-equal.
#[test]
fn png_round_trip_byte_equal() {
    let img = make_test_png();
    let bytes_a = encode_png(&img).expect("encode PNG");
    let decoded = decode_png(&bytes_a).expect("decode PNG");
    assert_eq!(decoded.width, img.width);
    assert_eq!(decoded.height, img.height);
    assert_eq!(decoded.color_type, img.color_type);
    assert_eq!(decoded.pixels, img.pixels);
    let bytes_b = encode_png(&decoded).expect("re-encode PNG");
    assert_eq!(bytes_a, bytes_b, "PNG round-trip is not byte-stable");
}

/// (a) PNG round-trip via larger pixel buffer.
#[test]
fn png_round_trip_large_buffer() {
    let w = 32u32;
    let h = 24u32;
    let pixels: Vec<u8> = (0..(w * h * 4))
        .map(|i| (i.wrapping_mul(13) & 0xff) as u8)
        .collect();
    let img = PngImage {
        width: w,
        height: h,
        color_type: ColorType::Rgba,
        pixels,
    };
    let bytes_a = encode_png(&img).unwrap();
    let decoded = decode_png(&bytes_a).unwrap();
    let bytes_b = encode_png(&decoded).unwrap();
    assert_eq!(bytes_a, bytes_b);
    assert_eq!(decoded.pixels, img.pixels);
}

/// (a) PNG : peek without full decode returns matching IHDR data.
#[test]
fn png_peek_matches_decode() {
    let img = make_test_png();
    let bytes = encode_png(&img).unwrap();
    let (w, h, ct) = png::peek(&bytes).unwrap();
    let decoded = decode_png(&bytes).unwrap();
    assert_eq!(w, decoded.width);
    assert_eq!(h, decoded.height);
    assert_eq!(ct, decoded.color_type);
}

/// (c) WAV round-trip : encode → decode → PCM equality.
#[test]
fn wav_round_trip_pcm_equality() {
    let wave = make_test_wav();
    let bytes_a = encode_wav(&wave).expect("encode WAV");
    let decoded = decode_wav(&bytes_a).expect("decode WAV");
    assert_eq!(decoded.channels, wave.channels);
    assert_eq!(decoded.sample_rate, wave.sample_rate);
    assert_eq!(decoded.format, wave.format);
    assert_eq!(decoded.pcm, wave.pcm, "PCM data mismatch after round-trip");
    let bytes_b = encode_wav(&decoded).unwrap();
    assert_eq!(bytes_a, bytes_b, "WAV bytes not stable across round-trip");
}

/// (c) WAV : multi-format round-trip.
#[test]
fn wav_round_trip_all_supported_formats() {
    for (fmt, sample_bytes) in [
        (SampleFormat::PcmU8, 1),
        (SampleFormat::PcmS16, 2),
        (SampleFormat::PcmS24, 3),
        (SampleFormat::PcmS32, 4),
        (SampleFormat::Float32, 4),
    ] {
        let frames: usize = 100;
        let channels: u16 = 2;
        let pcm: Vec<u8> = (0..(frames * (channels as usize) * sample_bytes))
            .map(|i| (i.wrapping_mul(7) & 0xff) as u8)
            .collect();
        let wave = WavFile {
            channels,
            sample_rate: 48_000,
            format: fmt,
            pcm: pcm.clone(),
        };
        let bytes = encode_wav(&wave).unwrap_or_else(|e| panic!("encode {fmt:?}: {e}"));
        let back = decode_wav(&bytes).unwrap_or_else(|e| panic!("decode {fmt:?}: {e}"));
        assert_eq!(back.pcm, pcm, "round-trip mismatch for {fmt:?}");
    }
}

/// (b) GLB round-trip : load known-good GLB → walk scene graph.
#[test]
fn glb_round_trip_walk_scene_graph() {
    let bytes = make_test_glb();
    let doc = decode_glb(&bytes).expect("decode GLB");
    assert_eq!(doc.asset.version, "2.0");
    assert!(doc.binary_buffer.is_some(), "GLB BIN chunk missing");
    let bin = doc.binary_buffer.as_ref().unwrap();
    assert_eq!(bin.len(), 12, "GLB BIN chunk wrong size");
    // Walk the default scene + collect node order.
    let mut visited = Vec::new();
    doc.walk_scene(None, |idx, depth| visited.push((idx, depth)))
        .expect("walk scene");
    assert!(!visited.is_empty(), "scene walk yielded no nodes");
    // Verify accessor → byte slice resolution.
    let acc_bytes = doc
        .accessor_bytes(0)
        .expect("accessor 0 resolves")
        .expect("accessor 0 returns bytes");
    assert_eq!(acc_bytes.len(), 12, "accessor 0 byte length mismatch");
    let x = f32::from_le_bytes([acc_bytes[0], acc_bytes[1], acc_bytes[2], acc_bytes[3]]);
    assert!((x - 1.0).abs() < f32::EPSILON);
}

/// (b) GLB : nested scene graph traversal yields depth-first order.
#[test]
fn glb_nested_scene_graph_depth_first() {
    let bytes = make_nested_glb();
    let doc = decode_glb(&bytes).expect("decode GLB");
    let mut visited = Vec::new();
    doc.walk_scene(None, |idx, depth| visited.push((idx, depth)))
        .expect("walk scene");
    assert_eq!(visited, vec![(0, 0), (1, 1), (2, 2), (3, 0)]);
}

/// Cross-format integration : encode + decode a handful of formats and
/// validate the unified `AssetError` surface for at least one failure
/// mode per format.
#[test]
fn cross_format_error_surface_is_unified() {
    use cssl_asset::AssetError;

    // PNG : bad magic.
    let r = decode_png(b"not-a-png-at-all");
    assert!(matches!(r, Err(AssetError::BadMagic { .. })));

    // WAV : bad RIFF.
    let r = decode_wav(&[0u8; 16]);
    assert!(matches!(r, Err(AssetError::BadMagic { .. })));

    // GLB : truncated header.
    let r = decode_glb(&[0u8; 4]);
    assert!(matches!(r, Err(AssetError::Truncated { .. })));
}

// ─────────────────────────────────────────────────────────────────────
// § Test fixtures (in-process generators)
// ─────────────────────────────────────────────────────────────────────

fn make_test_png() -> PngImage {
    PngImage {
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![
            // row 0
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255, // row 1
            128, 128, 128, 200, 64, 64, 64, 200, 32, 32, 32, 200, 16, 16, 16, 200,
            // row 2
            255, 100, 100, 128, 100, 255, 100, 128, 100, 100, 255, 128, 200, 200, 200, 128,
            // row 3
            10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160,
        ],
    }
}

fn make_test_wav() -> WavFile {
    // A short stereo s16 burst.
    let frames = 64usize;
    let mut pcm = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let l = (i.wrapping_mul(257) & 0x7fff) as i16;
        let r = (i.wrapping_mul(521) & 0x7fff) as i16;
        pcm.extend_from_slice(&l.to_le_bytes());
        pcm.extend_from_slice(&r.to_le_bytes());
    }
    WavFile {
        channels: 2,
        sample_rate: 44_100,
        format: SampleFormat::PcmS16,
        pcm,
    }
}

fn make_test_glb() -> Vec<u8> {
    // Single-node scene with 1 mesh + 1 accessor of 1 VEC3 float.
    let bin: Vec<u8> = {
        let mut v = Vec::new();
        v.extend_from_slice(&1.0f32.to_le_bytes());
        v.extend_from_slice(&2.0f32.to_le_bytes());
        v.extend_from_slice(&3.0f32.to_le_bytes());
        v
    };
    let mut padded_bin = bin.clone();
    while padded_bin.len() % 4 != 0 {
        padded_bin.push(0);
    }
    let json = format!(
        r#"{{
  "asset": {{ "version": "2.0", "generator": "cssl-asset/integration" }},
  "scene": 0,
  "scenes": [{{ "nodes": [0] }}],
  "nodes": [{{ "mesh": 0 }}],
  "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }} }}] }}],
  "accessors": [{{ "bufferView": 0, "componentType": 5126, "count": 1, "type": "VEC3" }}],
  "bufferViews": [{{ "buffer": 0, "byteOffset": 0, "byteLength": {0} }}],
  "buffers": [{{ "byteLength": {0} }}]
}}"#,
        bin.len()
    );
    let mut json_bytes = json.into_bytes();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len() + 8 + padded_bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6c67_u32.to_le_bytes()); // GLB_MAGIC
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes()); // CHUNK_JSON
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(padded_bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004e_4942_u32.to_le_bytes()); // CHUNK_BIN
    out.extend_from_slice(&padded_bin);
    out
}

fn make_nested_glb() -> Vec<u8> {
    let json = r#"{
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 3]}],
        "nodes": [
            {"children": [1]},
            {"children": [2]},
            {},
            {}
        ]
    }"#;
    let mut json_bytes = json.as_bytes().to_vec();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let total = 12 + 8 + json_bytes.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6c67_u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out
}

// Make the fixture types referenced from documentation.
#[allow(dead_code)]
fn _document_type_referenced(_: GltfDocument) {}

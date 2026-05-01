//! § visual_regression — T11-LOA-TEST-APP integration test.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Exercises the snapshot pipeline end-to-end at the catalog level
//!   (no GPU required) :
//!     1. Build a tiny synthetic RGBA8 image
//!     2. Encode it via `snapshot::encode_png`
//!     3. Decode it back
//!     4. Round-trip the bytes
//!     5. Compute MAE between identical and divergent images
//!     6. Verify all 5 tour-pose registries resolve correctly
//!
//! § WHY HERE
//!   The full GPU readback path is exercised at runtime via
//!   `nc localhost 3001` + the MCP `render.snapshot_png` tool. This
//!   integration test guards the catalog-level surface so a regression
//!   in path-sanitization, PNG encoding, or tour registries can be
//!   caught without spinning up wgpu (which requires DISPLAY/MSVC on
//!   Windows + can't run in CI).
//!
//! § INVOCATION
//!   cargo test -p loa-host --test visual_regression
//!
//!   For the full GPU path :
//!   cargo +stable-x86_64-pc-windows-msvc test -p loa-host \
//!     --features runtime --test visual_regression -- --include-ignored

#![allow(clippy::cast_possible_truncation)]

use loa_host::snapshot::{
    decode_png, encode_png, mae_bgra8, rgba8_to_bgra8_inplace, sanitize_snapshot_path,
    tour_by_id, GOLDEN_MAE_THRESHOLD, TOUR_IDS,
};

/// Build a 32×16 RGBA8 image with a horizontal red→blue gradient. Returns
/// `(rgba_bytes, width, height)`. Large enough to exercise the encoder
/// (multiple scanlines), small enough to run in tests <50ms.
fn synthetic_gradient(w: u32, h: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let t = (x as f32) / (w as f32 - 1.0);
            let r = ((1.0 - t) * 255.0) as u8;
            let g = (((y as f32) / (h as f32 - 1.0)) * 128.0) as u8;
            let b = (t * 255.0) as u8;
            buf.push(r);
            buf.push(g);
            buf.push(b);
            buf.push(255);
        }
    }
    buf
}

#[test]
fn integration_encode_decode_round_trip() {
    let dir = std::env::temp_dir().join("loa-test-app-vis-reg");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join("integration_round_trip.png");

    let w: u32 = 32;
    let h: u32 = 16;
    let rgba = synthetic_gradient(w, h);

    let bytes = encode_png(&rgba, w, h, &p).expect("encode");
    assert!(bytes > 0, "encoded file should have non-zero size");

    let (rgba2, w2, h2) = decode_png(&p).expect("decode");
    assert_eq!(w2, w);
    assert_eq!(h2, h);
    assert_eq!(rgba2.len(), rgba.len());
    assert_eq!(rgba2, rgba);

    let _ = std::fs::remove_file(&p);
}

#[test]
fn integration_mae_zero_for_identical_pngs() {
    let dir = std::env::temp_dir().join("loa-test-app-vis-reg");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p1 = dir.join("identical_a.png");
    let p2 = dir.join("identical_b.png");

    let rgba = synthetic_gradient(16, 16);
    encode_png(&rgba, 16, 16, &p1).unwrap();
    encode_png(&rgba, 16, 16, &p2).unwrap();

    let (mut a, _, _) = decode_png(&p1).unwrap();
    let (mut b, _, _) = decode_png(&p2).unwrap();
    rgba8_to_bgra8_inplace(&mut a);
    rgba8_to_bgra8_inplace(&mut b);
    let mae = mae_bgra8(&a, &b).expect("size match");
    assert!(mae.abs() < 1e-6, "identical PNGs must have MAE ≈ 0; got {mae}");

    let _ = std::fs::remove_file(&p1);
    let _ = std::fs::remove_file(&p2);
}

#[test]
fn integration_mae_above_threshold_for_divergent_pngs() {
    let dir = std::env::temp_dir().join("loa-test-app-vis-reg");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p1 = dir.join("diff_a.png");
    let p2 = dir.join("diff_b.png");

    let rgba_a = synthetic_gradient(16, 16);
    // B = inverted A (every pixel flipped) → MAE ≈ 0.5
    let rgba_b: Vec<u8> = rgba_a
        .chunks_exact(4)
        .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]])
        .collect();
    encode_png(&rgba_a, 16, 16, &p1).unwrap();
    encode_png(&rgba_b, 16, 16, &p2).unwrap();

    let (mut a, _, _) = decode_png(&p1).unwrap();
    let (mut b, _, _) = decode_png(&p2).unwrap();
    rgba8_to_bgra8_inplace(&mut a);
    rgba8_to_bgra8_inplace(&mut b);
    let mae = mae_bgra8(&a, &b).expect("size match");
    // The threshold is 0.02 ; the inverted-image MAE should be FAR above it.
    assert!(
        mae > GOLDEN_MAE_THRESHOLD * 5.0,
        "divergent PNGs should exceed threshold; got mae={mae}, threshold={GOLDEN_MAE_THRESHOLD}"
    );

    let _ = std::fs::remove_file(&p1);
    let _ = std::fs::remove_file(&p2);
}

#[test]
fn integration_all_tour_ids_resolve() {
    for id in TOUR_IDS {
        let poses = tour_by_id(id).expect("must resolve");
        assert!(!poses.is_empty(), "tour {id} should have ≥ 1 pose");
        // Names are unique within a tour
        let mut names: Vec<&str> = poses.iter().map(|p| p.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            poses.len(),
            "tour {id} has duplicate pose names"
        );
    }
}

#[test]
fn integration_sanitize_path_rejects_traversal() {
    use std::path::Path;
    let base = Path::new("logs/snapshots");
    assert!(sanitize_snapshot_path(base, "../../etc/passwd").is_none());
    assert!(sanitize_snapshot_path(base, "subdir/../.../escape").is_none());
    let ok = sanitize_snapshot_path(base, "subdir/snap.png");
    assert!(ok.is_some());
}

#[test]
fn integration_tour_count_summary() {
    // Total snapshots a full run-of-all-tours would produce.
    let total: usize = TOUR_IDS
        .iter()
        .map(|id| tour_by_id(id).unwrap().len())
        .sum();
    // 5 + 4 + 5 + 14 + 1 = 29
    assert_eq!(total, 29);
}

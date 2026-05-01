//! § sense_integration — integration test for the T11-LOA-SENSORY harness.
//!
//! This test verifies that ALL `sense.*` MCP tools dispatch successfully
//! through the JSON-RPC entry point, return non-empty results, and that
//! the per-axis aggregation logic doesn't panic on a fresh EngineState.
//!
//! § ROLE
//!   Catalog-buildable integration. The runtime feature is not required —
//!   the sense.* surface is read-only and aggregates from the EngineState
//!   mirror, so the test exercises the JSON contract without spinning up
//!   wgpu / winit / etc.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use loa_host::mcp_server::{
    dispatch, EngineState, JsonRpcRequest, JSON_RPC_VERSION,
};
use loa_host::mcp_tools::tool_registry;

fn shared_state() -> Arc<Mutex<EngineState>> {
    Arc::new(Mutex::new(EngineState::default()))
}

fn invoke(state: &Arc<Mutex<EngineState>>, method: &str, params: Value) -> Value {
    let reg = tool_registry();
    let req = JsonRpcRequest {
        jsonrpc: JSON_RPC_VERSION.to_string(),
        id: Value::from(1),
        method: method.to_string(),
        params,
    };
    let resp = dispatch(state, &reg, &req);
    if let Some(err) = resp.error {
        panic!("dispatch returned error : {} (code {})", err.message, err.code);
    }
    resp.result.expect("ok result")
}

#[test]
fn integration_all_25_sense_tools_dispatch_successfully() {
    let st = shared_state();
    // 32 sense.* tools across 9 axes + 1 combined = full taxonomy.
    let tools: &[&str] = &[
        // Visual
        "sense.framebuffer_thumbnail",
        "sense.center_pixel",
        "sense.viewport_summary",
        "sense.object_at_crosshair",
        // Audio (stubs)
        "sense.audio_levels",
        "sense.audio_recent",
        "sense.spatial_audio",
        // Spatial
        "sense.compass_8",
        "sense.body_pose",
        "sense.room_neighbors",
        // Interoception
        "sense.engine_load",
        "sense.frame_pacing",
        "sense.gpu_state",
        "sense.thermal",
        // Diagnostic
        "sense.recent_errors",
        "sense.recent_panics",
        "sense.validation_errors",
        "sense.test_status",
        // Temporal
        "sense.event_log",
        "sense.dm_history",
        "sense.input_history",
        // Causal
        "sense.dm_state",
        "sense.gm_recent_phrases",
        "sense.companion_proposals",
        // Network
        "sense.mcp_clients",
        "sense.recent_commands",
        // Environmental
        "sense.omega_field_at_camera",
        "sense.spectral_at_pixel",
        "sense.stokes_at_pixel",
        "sense.cfer_neighborhood",
        "sense.dgi_signal",
        // Combined
        "sense.snapshot",
    ];

    for t in tools {
        let v = invoke(&st, t, json!({}));
        // Each tool must return a JSON object (or array for some).
        assert!(
            v.is_object() || v.is_array(),
            "tool {t} returned non-object/array : {v}"
        );
    }

    // After 32 invocations, the per-state counter should reflect at least 32.
    let g = st.lock().unwrap();
    assert!(
        g.sense_invocations_total >= 32,
        "expected ≥ 32 sense invocations, got {}",
        g.sense_invocations_total
    );
}

#[test]
fn integration_sense_compass_8_returns_8_directions() {
    let st = shared_state();
    {
        let mut g = st.lock().unwrap();
        g.compass_distances_m = [10.0, 14.14, 10.0, 14.14, 10.0, 14.14, 10.0, 14.14];
    }
    let v = invoke(&st, "sense.compass_8", json!({}));
    let dists = v["distances_m"].as_array().expect("distances_m array");
    assert_eq!(dists.len(), 8);
    let dirs = v["directions"].as_array().expect("directions array");
    assert_eq!(dirs.len(), 8);
    assert_eq!(dirs[0].as_str().unwrap(), "N");
    assert_eq!(dirs[7].as_str().unwrap(), "NW");
}

#[test]
fn integration_sense_framebuffer_thumbnail_pending_when_no_capture() {
    let st = shared_state();
    let v = invoke(&st, "sense.framebuffer_thumbnail", json!({}));
    // Initially no capture has happened.
    assert_eq!(v["available"], false);
    // Capture should now be pending.
    assert_eq!(v["capture_pending"], true);
    // After this call, EngineState's capture_pending flag is set.
    let g = st.lock().unwrap();
    assert!(g.fb_thumb.capture_pending);
}

#[test]
fn integration_tools_list_reports_93_entries() {
    // § post-WAVE3-grand-merge : 84 (post-userfix) + 2 text-input + 3 gltf
    // + 2 spontaneous + 2 intent-router = 93.
    let st = shared_state();
    let v = invoke(&st, "tools.list", json!({}));
    assert_eq!(v["count"].as_u64().unwrap(), 93);
    let tools = v["tools"].as_array().expect("array");
    let sense_tools: Vec<&Value> = tools
        .iter()
        .filter(|t| {
            t["name"]
                .as_str()
                .map(|n| n.starts_with("sense."))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        sense_tools.len() >= 20,
        "expected ≥ 20 sense.* tools, got {}",
        sense_tools.len()
    );
}

#[test]
fn integration_sense_omega_field_at_camera_reports_facets() {
    let st = shared_state();
    {
        let mut g = st.lock().unwrap();
        g.camera.pos = loa_host::mcp_server::Vec3 {
            x: 0.0,
            y: 1.5,
            z: 0.0,
        };
    }
    let v = invoke(&st, "sense.omega_field_at_camera", json!({}));
    assert_eq!(v["available"].as_bool().unwrap(), true);
    let facets = v["facets"].as_object().expect("facets object");
    for k in [
        "density",
        "velocity",
        "vorticity",
        "enthalpy",
        "multivec_lo",
        "radiance_probe_lo",
        "pattern_handle",
        "sigma_low",
    ] {
        assert!(facets.contains_key(k), "missing facet {k}");
    }
}

#[test]
fn integration_sense_spectral_at_pixel_returns_16_bands_for_each_illuminant() {
    let st = shared_state();
    let v = invoke(&st, "sense.spectral_at_pixel", json!({}));
    assert_eq!(v["band_count"].as_u64().unwrap(), 16);
    assert_eq!(v["spd_16"].as_array().unwrap().len(), 16);
    assert_eq!(v["reflectance_16"].as_array().unwrap().len(), 16);
    assert_eq!(v["radiance_16"].as_array().unwrap().len(), 16);
    assert_eq!(v["illuminant"].as_str().unwrap(), "D65");
}

#[test]
fn integration_combined_snapshot_aggregates_seven_axes() {
    let st = shared_state();
    let v = invoke(&st, "sense.snapshot", json!({}));
    for k in [
        "center_pixel",
        "compass_8",
        "body_pose",
        "engine_load",
        "frame_pacing",
        "dm_state",
        "room_neighbors",
    ] {
        assert!(v.get(k).is_some(), "missing axis {k}");
    }
}

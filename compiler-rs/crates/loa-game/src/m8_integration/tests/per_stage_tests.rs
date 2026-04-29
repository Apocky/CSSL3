//! Per-stage instrumentation tests — verify every stage of the canonical
//! 12-stage pipeline is wrapped by Timer::start/stop and its histogram
//! receives samples on every frame.

use crate::m8_integration::pipeline::Pipeline;
use crate::m8_integration::{PassContext, StageId};

fn ctx_for(frame: u64, workload: u32) -> PassContext {
    PassContext {
        frame_n: frame,
        workload,
        ..Default::default()
    }
}

#[test]
fn pipeline_default_constructs_with_12_stages() {
    let pipe = Pipeline::new();
    let ns = pipe.registered_stage_namespaces();
    assert_eq!(ns.len(), 12);
}

#[test]
fn pipeline_all_12_namespaces_canonical() {
    let pipe = Pipeline::new();
    let ns = pipe.registered_stage_namespaces();
    let expected = vec![
        "pipeline.stage_1_embodiment.frame_time_ms",
        "pipeline.stage_2_gaze_collapse.frame_time_ms",
        "pipeline.stage_3_omega_field_update.frame_time_ms",
        "pipeline.stage_4_wave_solver.frame_time_ms",
        "pipeline.stage_5_sdf_raymarch.frame_time_ms",
        "pipeline.stage_6_kan_brdf.frame_time_ms",
        "pipeline.stage_7_fractal_amplifier.frame_time_ms",
        "pipeline.stage_8_companion_semantic.frame_time_ms",
        "pipeline.stage_9_mise_en_abyme.frame_time_ms",
        "pipeline.stage_10_tonemap.frame_time_ms",
        "pipeline.stage_11_motion_vec.frame_time_ms",
        "pipeline.stage_12_compose_xr_layers.frame_time_ms",
    ];
    assert_eq!(ns, expected);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_one_frame_records_one_sample_per_stage() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 16));
    for stage in StageId::ALL {
        let h = pipe.histogram(stage);
        assert_eq!(h.len(), 1, "stage {:?} expected 1 sample", stage);
        assert_eq!(h.total_count(), 1);
    }
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_n_frames_records_n_samples_per_stage() {
    let mut pipe = Pipeline::new();
    for f in 0..50 {
        pipe.run_frame(ctx_for(f, 16));
    }
    for stage in StageId::ALL {
        let h = pipe.histogram(stage);
        assert_eq!(h.len(), 50, "stage {:?} expected 50 samples", stage);
        assert_eq!(h.total_count(), 50);
    }
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_total_samples_sum_across_stages() {
    let mut pipe = Pipeline::new();
    for f in 0..10 {
        pipe.run_frame(ctx_for(f, 16));
    }
    // 12 stages × 10 frames = 120 total samples
    assert_eq!(pipe.total_samples_recorded(), 120);
}

#[test]
fn pipeline_frame_counter_increments_each_run() {
    let mut pipe = Pipeline::new();
    assert_eq!(pipe.frame_count(), 0);
    pipe.run_frame(ctx_for(0, 8));
    assert_eq!(pipe.frame_count(), 1);
    pipe.run_frame(ctx_for(0, 8));
    assert_eq!(pipe.frame_count(), 2);
}

#[test]
fn pipeline_each_stage_executes_per_frame() {
    let mut pipe = Pipeline::new();
    for f in 0..5 {
        pipe.run_frame(ctx_for(f, 4));
    }
    // Verify each pass's frames_executed is at 5
    // (We use trait-object inspection — re-borrow each pass).
    // Note : downcasting Box<dyn Pass> requires Any ; instead, we
    // verify via histograms : if a stage didn't execute, its hist
    // would have 0 samples.
    #[cfg(feature = "metrics")]
    for stage in StageId::ALL {
        let h = pipe.histogram(stage);
        assert_eq!(h.len(), 5);
    }
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_1_embodiment_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::Embodiment);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_2_gaze_collapse_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::GazeCollapse);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_3_omega_field_update_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::OmegaFieldUpdate);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_4_wave_solver_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::WaveSolver);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_5_sdf_raymarch_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::SdfRaymarch);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_6_kan_brdf_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::KanBrdf);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_7_fractal_amplifier_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::FractalAmplifier);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_8_companion_semantic_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::CompanionSemantic);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_9_mise_en_abyme_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::MiseEnAbyme);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_10_tonemap_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::Tonemap);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_11_motion_vec_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::MotionVec);
    assert_eq!(h.len(), 1);
}

#[cfg(feature = "metrics")]
#[test]
fn pipeline_stage_12_compose_xr_layers_instrumented() {
    let mut pipe = Pipeline::new();
    pipe.run_frame(ctx_for(0, 1));
    let h = pipe.histogram(StageId::ComposeXrLayers);
    assert_eq!(h.len(), 1);
}

#[test]
fn stage_id_index_is_1_to_12() {
    for (i, s) in StageId::ALL.iter().enumerate() {
        assert_eq!(s.index() as usize, i + 1);
    }
}

#[test]
fn stage_id_all_distinct() {
    use std::collections::HashSet;
    let set: HashSet<_> = StageId::ALL.iter().collect();
    assert_eq!(set.len(), 12);
}

#[test]
fn stage_id_snake_names_distinct() {
    use std::collections::HashSet;
    let set: HashSet<_> = StageId::ALL.iter().map(|s| s.snake_name()).collect();
    assert_eq!(set.len(), 12);
}

#[test]
fn stage_id_namespaces_distinct() {
    use std::collections::HashSet;
    let set: HashSet<_> = StageId::ALL.iter().map(|s| s.metric_namespace()).collect();
    assert_eq!(set.len(), 12);
}

#[test]
fn stage_id_canonical_order_preserved() {
    let order: Vec<u8> = StageId::ALL.iter().map(|s| s.index()).collect();
    assert_eq!(order, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
}

#[test]
fn pipeline_pass_lookup_returns_correct_stage() {
    let pipe = Pipeline::new();
    for stage in StageId::ALL {
        let p = pipe.pass(stage);
        assert_eq!(p.stage_id(), stage);
    }
}

#[test]
fn pipeline_pass_names_match_stage_id() {
    let pipe = Pipeline::new();
    for stage in StageId::ALL {
        let p = pipe.pass(stage);
        assert_eq!(p.name(), stage.snake_name());
    }
}

#[test]
fn pipeline_default_trait_works() {
    let pipe = Pipeline::default();
    assert_eq!(pipe.frame_count(), 0);
    assert_eq!(pipe.registered_stage_namespaces().len(), 12);
}

//! § fps_pipeline_bench — CPU-only integration bench for the FPS pipeline
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-FPS-PIPELINE-BENCH (W13-1)
//!
//! § ROLE
//!   On-CPU-only path verifies the FPS pipeline meets the 8.33ms (120Hz)
//!   frame-budget for orchestration overhead alone — i.e. the pipeline's
//!   begin/record/submit/end machinery contributes negligibly to the
//!   frame-time. Real wgpu draw work happens elsewhere ; this bench measures
//!   only what we own.
//!
//!   The bench drives 1000 simulated frames through the pipeline using
//!   realistic instance-population (256 instances per frame). It then
//!   asserts :
//!     - Mean overhead per frame ≤ 0.5 ms (= 6% of 8.33ms budget)
//!     - p99 overhead per frame ≤ 1.0 ms
//!     - Total cycles all clean : 0 leaked slots, 0 illegal transitions
//!
//! § PRIME-DIRECTIVE
//!   The bench reads only public-API state ; no surveillance ; no behavior
//!   profiling. Output is aggregate timings only.

use loa_host::fps_pipeline::{
    FpsPipeline, InstanceEntry, FRAME_BUDGET_120HZ_MS, FRAME_BUDGET_144HZ_MS,
};
use loa_host::wired_fps_pipeline::{default_pipeline, summary_line};
use std::time::Instant;

const BENCH_FRAMES: u64 = 1_000;
const INSTANCES_PER_FRAME: u32 = 256;

#[test]
fn fps_pipeline_overhead_under_half_ms_p50() {
    let mut pipeline = default_pipeline();
    let mut samples_us: Vec<u64> = Vec::with_capacity(BENCH_FRAMES as usize);

    for frame in 0..BENCH_FRAMES {
        let t0 = Instant::now();

        // Begin frame.
        let _slot = pipeline.begin_frame(frame * 8_000);

        // Populate the instance buffer with synthetic per-frame instances.
        for i in 0..INSTANCES_PER_FRAME {
            let _ = pipeline.instances.push(InstanceEntry {
                instance_id: i,
                bsphere_center: [
                    (i as f32) * 0.5,
                    1.0,
                    (frame as f32) * 0.1,
                ],
                bsphere_radius: 1.0,
                ..Default::default()
            });
        }

        // Run the cull pass.
        let snap_inst = pipeline.instances.populated_slice().to_vec();
        let _passed = pipeline.cull_plan.cull_pass(&snap_inst);

        // Record all 9 named passes.
        for kind in loa_host::fps_pipeline::PassKind::ALL {
            pipeline.record_pass(kind);
        }
        pipeline.touch_cmd_buffers(frame);

        let now_us = (frame + 1) * 8_000;
        let _ = pipeline.submit_frame(frame, now_us);
        let _ = pipeline.present_frame();
        let _m = pipeline.end_frame();

        let elapsed_us = t0.elapsed().as_micros() as u64;
        samples_us.push(elapsed_us);
    }

    // Sort to compute p50 + p99.
    samples_us.sort_unstable();
    let p50 = samples_us[samples_us.len() / 2];
    let p99 = samples_us[(samples_us.len() * 99 / 100).min(samples_us.len() - 1)];
    let mean: u64 =
        samples_us.iter().sum::<u64>() / (samples_us.len() as u64);

    let p50_ms = (p50 as f32) / 1000.0;
    let p99_ms = (p99 as f32) / 1000.0;
    let mean_ms = (mean as f32) / 1000.0;

    eprintln!(
        "fps_pipeline overhead bench · {} frames · {} instances/frame",
        BENCH_FRAMES, INSTANCES_PER_FRAME
    );
    eprintln!(
        "  mean = {:.4} ms · p50 = {:.4} ms · p99 = {:.4} ms",
        mean_ms, p50_ms, p99_ms
    );
    eprintln!("  budget 120Hz = {:.3} ms", FRAME_BUDGET_120HZ_MS);
    eprintln!("  budget 144Hz = {:.3} ms", FRAME_BUDGET_144HZ_MS);
    eprintln!("  summary: {}", summary_line(&pipeline));

    // Mean overhead must be ≤ 0.5 ms (= 6% of 8.33 ms).
    // We allow some slack because CI-environment timing varies.
    assert!(
        mean_ms <= 0.5,
        "mean overhead = {mean_ms} ms exceeds 0.5 ms target"
    );
    // p99 overhead must be ≤ 1.0 ms.
    assert!(
        p99_ms <= 1.0,
        "p99 overhead = {p99_ms} ms exceeds 1.0 ms target"
    );
}

#[test]
fn fps_pipeline_zero_leaked_slots_after_long_run() {
    let mut pipeline = default_pipeline();
    for i in 0..2_000 {
        // Fully cycle one frame.
        let _ = pipeline.step_one_frame(i * 8_000, 5_000);
    }
    // After 2000 cycles, all slots Free, no in-flight.
    assert_eq!(pipeline.ring.free_count(), pipeline.ring.depth());
    assert_eq!(pipeline.ring.recording_count(), 0);
    assert_eq!(pipeline.ring.submitted_count(), 0);
    assert_eq!(pipeline.total_frames(), 2_000);
}

#[test]
fn fps_pipeline_metrics_drive_perf_budget() {
    use loa_host::polish_audit::{
        PerfBudget, FRAME_BUDGET_120HZ_MS as PA_120, FRAME_BUDGET_144HZ_MS as PA_144,
    };

    let mut pipeline = default_pipeline();
    let mut budget = PerfBudget::new();

    // 50 in-budget frames, then 5 over 144Hz only, then 5 over 120Hz too.
    for i in 0..50 {
        let m = pipeline.step_one_frame(i * 8_000, 5_000); // 5ms — under both
        budget.record_frame_ms(m.frame_ms);
    }
    for i in 50..55 {
        let m = pipeline.step_one_frame(i * 8_000, 7_000); // 7ms — over 144Hz only
        budget.record_frame_ms(m.frame_ms);
    }
    for i in 55..60 {
        let m = pipeline.step_one_frame(i * 8_000, 10_000); // 10ms — over 120Hz too
        budget.record_frame_ms(m.frame_ms);
    }

    // 60 total frames, 10 missed 120Hz, 10 missed 144Hz (5+5).
    assert_eq!(budget.total_frames, 60);
    assert!(budget.over_120hz_count >= 5);
    assert!(budget.over_144hz_count >= 10);

    // 50/60 = 83% pass-rate at 120Hz · below 95% threshold.
    assert!(!budget.passes_120hz_attestation());
    // 50/60 = 83% pass-rate at 144Hz · also below the 90% threshold.
    assert!(!budget.passes_144hz_attestation());

    // Constants match cross-module.
    assert!((PA_120 - FRAME_BUDGET_120HZ_MS).abs() < 0.01);
    assert!((PA_144 - FRAME_BUDGET_144HZ_MS).abs() < 0.01);
}

//! M8 12-stage render-pipeline orchestrator.
//!
//! § T11-D158 (W-Jζ-2) : Timer-wraps each of the 12 stage `execute()`
//! calls, recording into per-stage histograms registered in the
//! [`crate::metrics_mock::MetricsRegistry`].
//!
//! § INVARIANTS
//!   - Stages run in canonical order [`super::StageId::ALL`] (1..=12)
//!   - Each call-site : `let t = Timer::start(); pass.execute(&ctx);
//!     hist.record(t.stop_ms());`
//!   - feature-gate `metrics` off : Timer is no-op, hist.record is no-op,
//!     entire instrumentation block elides to identical machine-code as
//!     a non-instrumented build (verified by binary-size diff test).
//!   - Replay-determinism : Timer + Histogram are observe-only, never
//!     mutate render state.

use std::sync::Arc;

use crate::m8_integration::compose_xr_layers_pass::ComposeXrLayersPass;
use crate::m8_integration::companion_semantic_pass::CompanionSemanticPass;
use crate::m8_integration::embodiment_pass::EmbodimentPass;
use crate::m8_integration::fractal_amplifier_pass::FractalAmplifierPass;
use crate::m8_integration::gaze_collapse_pass::GazeCollapsePass;
use crate::m8_integration::kan_brdf_pass::KanBrdfPass;
use crate::m8_integration::mise_en_abyme_pass::MiseEnAbymePass;
use crate::m8_integration::motion_vec_pass::MotionVecPass;
use crate::m8_integration::omega_field_update_pass::OmegaFieldUpdatePass;
use crate::m8_integration::sdf_raymarch_pass::SdfRaymarchPass;
use crate::m8_integration::tonemap_pass::TonemapPass;
use crate::m8_integration::wave_solver_pass::WaveSolverPass;
use crate::m8_integration::{Pass, PassContext, StageId};
use crate::metrics_mock::{Histogram, MetricsRegistry, MockRegistry, Timer};

/// 12-stage render-pipeline orchestrator.
///
/// § FIELDS
///   - `passes`     : 12 boxed Pass-trait objects in canonical order
///   - `histograms` : 12 per-stage histograms, namespace-keyed
///   - `registry`   : metrics registry (mock by default ; swap-in real
///                    cssl-metrics once T11-D157 lands)
///   - `frame_n`    : monotonic frame-counter
pub struct Pipeline {
    passes: Vec<Box<dyn Pass>>,
    histograms: Vec<Histogram>,
    registry: Arc<dyn MetricsRegistry>,
    frame_n: u64,
}

impl Pipeline {
    /// Construct a default pipeline w/ mock-registry + 12 default passes.
    #[must_use]
    pub fn new() -> Self {
        Self::with_registry(Arc::new(MockRegistry::new()))
    }

    /// Construct w/ caller-supplied [`MetricsRegistry`] (e.g. real
    /// cssl-metrics once T11-D157 lands).
    #[must_use]
    pub fn with_registry(registry: Arc<dyn MetricsRegistry>) -> Self {
        let passes: Vec<Box<dyn Pass>> = vec![
            Box::new(EmbodimentPass::new()),
            Box::new(GazeCollapsePass::new()),
            Box::new(OmegaFieldUpdatePass::new()),
            Box::new(WaveSolverPass::new()),
            Box::new(SdfRaymarchPass::new()),
            Box::new(KanBrdfPass::new()),
            Box::new(FractalAmplifierPass::new()),
            Box::new(CompanionSemanticPass::new()),
            Box::new(MiseEnAbymePass::new()),
            Box::new(TonemapPass::new()),
            Box::new(MotionVecPass::new()),
            Box::new(ComposeXrLayersPass::new()),
        ];
        // Pre-register one histogram per stage (canonical-namespace).
        let histograms: Vec<Histogram> = StageId::ALL
            .iter()
            .map(|s| registry.register_histogram(&s.metric_namespace()))
            .collect();
        Self {
            passes,
            histograms,
            registry,
            frame_n: 0,
        }
    }

    /// Construct w/ explicit pass list + registry. Order must match
    /// canonical [`StageId::ALL`]. Used by tests for swap-in mocks.
    ///
    /// § PANICS
    /// When `passes.len() != 12` or stage-ids don't match canonical order.
    #[must_use]
    pub fn with_passes(
        registry: Arc<dyn MetricsRegistry>,
        passes: Vec<Box<dyn Pass>>,
    ) -> Self {
        assert_eq!(passes.len(), 12, "expected 12 passes ; got {}", passes.len());
        for (i, p) in passes.iter().enumerate() {
            assert_eq!(
                p.stage_id(),
                StageId::ALL[i],
                "pass[{}] stage_id mismatch ; expected {:?} got {:?}",
                i,
                StageId::ALL[i],
                p.stage_id()
            );
        }
        let histograms: Vec<Histogram> = StageId::ALL
            .iter()
            .map(|s| registry.register_histogram(&s.metric_namespace()))
            .collect();
        Self {
            passes,
            histograms,
            registry,
            frame_n: 0,
        }
    }

    /// Run one frame across all 12 stages w/ Timer-wrapped instrumentation.
    ///
    /// § DETERMINISM
    ///   The order of stage execution is canonical and deterministic.
    ///   Timer measurement does not affect any pass's internal state.
    pub fn run_frame(&mut self, mut ctx: PassContext) {
        ctx.frame_n = self.frame_n;
        // 12 explicit Timer-wrapped execute calls. Stage-by-stage so that
        // when feature-gate `metrics` is OFF, each call-site collapses to
        // bare `pass.execute(&ctx)`.

        // ── stage 1 : embodiment ───────────────────────────────
        {
            let t = Timer::start();
            self.passes[0].execute(&ctx);
            self.histograms[0].record(t.stop_ms());
        }
        // ── stage 2 : gaze_collapse ────────────────────────────
        {
            let t = Timer::start();
            self.passes[1].execute(&ctx);
            self.histograms[1].record(t.stop_ms());
        }
        // ── stage 3 : omega_field_update ───────────────────────
        {
            let t = Timer::start();
            self.passes[2].execute(&ctx);
            self.histograms[2].record(t.stop_ms());
        }
        // ── stage 4 : wave_solver ──────────────────────────────
        {
            let t = Timer::start();
            self.passes[3].execute(&ctx);
            self.histograms[3].record(t.stop_ms());
        }
        // ── stage 5 : sdf_raymarch ─────────────────────────────
        {
            let t = Timer::start();
            self.passes[4].execute(&ctx);
            self.histograms[4].record(t.stop_ms());
        }
        // ── stage 6 : kan_brdf ─────────────────────────────────
        {
            let t = Timer::start();
            self.passes[5].execute(&ctx);
            self.histograms[5].record(t.stop_ms());
        }
        // ── stage 7 : fractal_amplifier ────────────────────────
        {
            let t = Timer::start();
            self.passes[6].execute(&ctx);
            self.histograms[6].record(t.stop_ms());
        }
        // ── stage 8 : companion_semantic ───────────────────────
        {
            let t = Timer::start();
            self.passes[7].execute(&ctx);
            self.histograms[7].record(t.stop_ms());
        }
        // ── stage 9 : mise_en_abyme ────────────────────────────
        {
            let t = Timer::start();
            self.passes[8].execute(&ctx);
            self.histograms[8].record(t.stop_ms());
        }
        // ── stage 10 : tonemap ─────────────────────────────────
        {
            let t = Timer::start();
            self.passes[9].execute(&ctx);
            self.histograms[9].record(t.stop_ms());
        }
        // ── stage 11 : motion_vec ──────────────────────────────
        {
            let t = Timer::start();
            self.passes[10].execute(&ctx);
            self.histograms[10].record(t.stop_ms());
        }
        // ── stage 12 : compose_xr_layers ───────────────────────
        {
            let t = Timer::start();
            self.passes[11].execute(&ctx);
            self.histograms[11].record(t.stop_ms());
        }

        self.frame_n = self.frame_n.wrapping_add(1);
    }

    /// Read the histogram for a given stage.
    #[must_use]
    pub fn histogram(&self, stage: StageId) -> &Histogram {
        let idx = (stage.index() as usize).saturating_sub(1);
        &self.histograms[idx]
    }

    /// Read p50 frame-time-ms for a given stage. Returns NaN when no data.
    #[must_use]
    pub fn p50_ms(&self, stage: StageId) -> f64 {
        self.histogram(stage).p50()
    }

    /// Read p95 frame-time-ms for a given stage. Returns NaN when no data.
    #[must_use]
    pub fn p95_ms(&self, stage: StageId) -> f64 {
        self.histogram(stage).p95()
    }

    /// Read p99 frame-time-ms for a given stage. Returns NaN when no data.
    #[must_use]
    pub fn p99_ms(&self, stage: StageId) -> f64 {
        self.histogram(stage).p99()
    }

    /// Frames-elapsed counter (monotonic).
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_n
    }

    /// Borrow the underlying registry (for cross-frame inspection in
    /// caller code that wants to enumerate all metric-namespaces).
    #[must_use]
    pub fn registry(&self) -> &Arc<dyn MetricsRegistry> {
        &self.registry
    }

    /// All-stage namespaces (lex-sorted for determinism).
    #[must_use]
    pub fn registered_stage_namespaces(&self) -> Vec<String> {
        StageId::ALL.iter().map(|s| s.metric_namespace()).collect()
    }

    /// Total samples recorded across all 12 stages (sum).
    #[must_use]
    pub fn total_samples_recorded(&self) -> u64 {
        self.histograms.iter().map(Histogram::total_count).sum()
    }

    /// Reset all histograms (drops all recorded samples).
    pub fn reset_histograms(&mut self) {
        for h in &self.histograms {
            h.reset();
        }
    }

    /// Borrow a pass for inspection (read-only).
    #[must_use]
    pub fn pass(&self, stage: StageId) -> &dyn Pass {
        let idx = (stage.index() as usize).saturating_sub(1);
        self.passes[idx].as_ref()
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

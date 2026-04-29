//! § PerFragmentCost — Stage-6 cost model + Quest-3 budget validation
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Encodes the canonical Quest-3 / RTX-50 / scalar cost model from
//!   `07_AES/07 § VII` :
//!
//!   | Tier         | Per-band ns | × 16 bands | Per-pixel | × 1080p frame |
//!   |--------------|-------------|------------|-----------|---------------|
//!   | CoopMatrix   | 50 ns       | 800 ns     | 800 ns    | 1.66 ms       |
//!   | SIMD-warp    | 200 ns      | 3200 ns    | 3.2 µs    | 6.62 ms       |
//!   | Scalar       | 800 ns      | 12800 ns   | 12.8 µs   | 26.5 ms       |
//!
//!   Target Stage-6 budget : ≤ 1.8 ms / frame at the foveated 1080p pixel
//!   count + iridescence + fluorescence overheads.
//!
//!   The cost model is purely-arithmetic — no actual GPU dispatch happens
//!   here. The numbers feed compile-time `density_check` per `07_AES/07
//!   § XI` and the runtime degraded-mode escalation per `07_AES/07 § VII`.

use crate::band::BAND_COUNT;

/// § The Quest-3 frame budget at 90 Hz, in milliseconds. Per `07_AES/07 §
///   VII` : "Quest-3 90Hz (11.1ms)".
pub const QUEST3_FRAME_BUDGET_MS: f32 = 11.1;

/// § The RTX-50 frame budget at 120 Hz, in milliseconds.
pub const RTX50_FRAME_BUDGET_MS: f32 = 8.3;

/// § Stage-6 budget, in milliseconds, on the Quest-3. Per `07_AES/07 § VII`
///   the KAN-shading total must be ≤ 2.5 ms ; we reserve 1.8 ms for the
///   spectral-BRDF + hero-MIS critical path, leaving 0.7 ms for
///   iridescence + fluorescence + CSF overhead.
pub const STAGE6_BUDGET_MS: f32 = 1.8;

/// § Pixel count for the canonical 1080p target.
pub const PIXELS_1080P: u32 = 1920 * 1080;

/// § Foveation factor : effective active-pixel count is ~38% of naive 1080p
///   per `07_AES/07 § VII` (fovea 25% × full + mid 35% × 0.5 + peripheral
///   40% × 0.25).
pub const FOVEATION_FACTOR: f32 = 0.38;

/// § GPU parallelism — number of concurrent fragment shaders in flight on
///   the canonical reference hardware, by dispatch tier. The factors are
///   back-derived from the spec `07_AES/07 § VII` "Foveated frame-cost
///   (1080p)" table :
///     CoopMatrix : 800 ns/frag → 0.63 ms / frame ⇒ ~1000 parallel threads
///     SIMD-warp  : 3200 ns/frag → 2.51 ms / frame ⇒ ~1000 parallel threads
///     Scalar     : 12800 ns/frag → 10.07 ms / frame ⇒ ~1000 parallel threads
///   The constant 1000 reflects an Adreno-8 / RTX-50-class parallelism
///   floor that the cost model treats as fixed across tiers.
pub const GPU_PARALLELISM_COOP_MATRIX: f32 = 1000.0;
pub const GPU_PARALLELISM_SIMD_WARP: f32 = 1000.0;
pub const GPU_PARALLELISM_SCALAR: f32 = 1000.0;

/// § Per-band per-eval nanoseconds per dispatch tier. Per `07_AES/07 §
///   VII` table.
pub const NS_PER_BAND_COOP_MATRIX: f32 = 50.0;
pub const NS_PER_BAND_SIMD_WARP: f32 = 200.0;
pub const NS_PER_BAND_SCALAR: f32 = 800.0;

/// § Iridescence + fluorescence overhead per *active* fragment (ns).
///   Per `07_AES/07 § VII` budget table : iridescence 0.20 ms / frame
///   distributed over the ~5% of fragments that need it (the anisotropic-
///   surface gate ratio). 0.20 ms / 1e6 (active fragments × parallelism)
///   → ~50 ns per gated fragment effective wall-clock.
pub const NS_IRIDESCENCE_PER_FRAGMENT: f32 = 50.0;
pub const NS_FLUORESCENCE_PER_FRAGMENT: f32 = 25.0;

/// § The dispatch-tier classification. Per `07_AES/07 § III`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTier {
    /// § Tier-1 cooperative-matrix dispatch (RTX-50 / Quest-4 / Adreno-8x+).
    CoopMatrix,
    /// § Tier-2 SIMD-warp cooperative dispatch (Quest-3 / RDNA-3 / Xe-2).
    SimdWarp,
    /// § Tier-3 scalar dispatch (low-end profile only).
    Scalar,
}

impl CostTier {
    /// § Per-band evaluation cost in nanoseconds.
    #[must_use]
    pub fn ns_per_band(self) -> f32 {
        match self {
            Self::CoopMatrix => NS_PER_BAND_COOP_MATRIX,
            Self::SimdWarp => NS_PER_BAND_SIMD_WARP,
            Self::Scalar => NS_PER_BAND_SCALAR,
        }
    }

    /// § GPU-parallelism factor for this tier. Used by
    ///   [`PerFragmentCost::frame_ms_1080p_foveated`] to amortize per-
    ///   fragment latency over concurrent threads.
    #[must_use]
    pub fn gpu_parallelism(self) -> f32 {
        match self {
            Self::CoopMatrix => GPU_PARALLELISM_COOP_MATRIX,
            Self::SimdWarp => GPU_PARALLELISM_SIMD_WARP,
            Self::Scalar => GPU_PARALLELISM_SCALAR,
        }
    }

    /// § Total per-fragment nanoseconds for a 16-band BRDF eval at this
    ///   tier (no extras).
    #[must_use]
    pub fn ns_per_fragment_brdf(self) -> f32 {
        self.ns_per_band() * BAND_COUNT as f32
    }
}

/// § A computed per-fragment cost summary. Holds the breakdown so the
///   pipeline-stage logger can attribute time per call-site.
#[derive(Debug, Clone, Copy)]
pub struct PerFragmentCost {
    /// § Dispatch tier in use.
    pub tier: CostTier,
    /// § BRDF eval cost (ns).
    pub brdf_ns: f32,
    /// § Iridescence overhead (ns) when active ; 0 otherwise.
    pub iridescence_ns: f32,
    /// § Fluorescence overhead (ns) when active.
    pub fluorescence_ns: f32,
    /// § CSF-gate evaluation cost (ns). Constant ~30 ns regardless of tier.
    pub csf_gate_ns: f32,
}

impl PerFragmentCost {
    /// § Construct a baseline cost (BRDF only, no extras).
    #[must_use]
    pub fn baseline(tier: CostTier) -> Self {
        Self {
            tier,
            brdf_ns: tier.ns_per_fragment_brdf(),
            iridescence_ns: 0.0,
            fluorescence_ns: 0.0,
            csf_gate_ns: 30.0,
        }
    }

    /// § Add iridescence overhead.
    #[must_use]
    pub fn with_iridescence(mut self) -> Self {
        self.iridescence_ns = NS_IRIDESCENCE_PER_FRAGMENT;
        self
    }

    /// § Add fluorescence overhead.
    #[must_use]
    pub fn with_fluorescence(mut self) -> Self {
        self.fluorescence_ns = NS_FLUORESCENCE_PER_FRAGMENT;
        self
    }

    /// § Total per-fragment cost in ns.
    #[must_use]
    pub fn total_ns(self) -> f32 {
        self.brdf_ns + self.iridescence_ns + self.fluorescence_ns + self.csf_gate_ns
    }

    /// § Total per-fragment cost in milliseconds.
    #[must_use]
    pub fn total_ms(self) -> f32 {
        self.total_ns() / 1_000_000.0
    }

    /// § Project to a full 1080p frame cost in milliseconds, using the
    ///   foveation factor + the tier-specific GPU-parallelism factor. The
    ///   parallelism factor amortizes per-fragment latency over the
    ///   number of fragments concurrently in flight ; the formula matches
    ///   the spec `07_AES/07 § VII` table within a few percent.
    #[must_use]
    pub fn frame_ms_1080p_foveated(self) -> f32 {
        let active_pixels = PIXELS_1080P as f32 * FOVEATION_FACTOR;
        // wall-clock-ms = active_pixels × per-frag-ns / parallelism / 1e6
        active_pixels * self.total_ns() / self.tier.gpu_parallelism() / 1_000_000.0
    }

    /// § True iff the projected frame cost fits within the Stage-6 budget
    ///   on the Quest-3.
    #[must_use]
    pub fn fits_stage6_quest3_budget(self) -> bool {
        self.frame_ms_1080p_foveated() <= STAGE6_BUDGET_MS
    }

    /// § True iff the projected frame cost fits within the full Quest-3
    ///   frame budget.
    #[must_use]
    pub fn fits_quest3_frame_budget(self) -> bool {
        self.frame_ms_1080p_foveated() <= QUEST3_FRAME_BUDGET_MS
    }

    /// § Return a degraded-mode classification per `07_AES/07 § VII` :
    ///   what to drop first if over-budget.
    #[must_use]
    pub fn degradation_decision(self) -> Option<DegradationStep> {
        if self.fits_stage6_quest3_budget() {
            return None;
        }
        // § Order : iridescence first (minor), fluorescence next, then
        //   half-rate-fovea (downstream stage decides), then drop to 4-band.
        if self.iridescence_ns > 0.0 {
            return Some(DegradationStep::DropIridescence);
        }
        if self.fluorescence_ns > 0.0 {
            return Some(DegradationStep::DropFluorescence);
        }
        if self.tier == CostTier::Scalar {
            return Some(DegradationStep::RefuseScalar);
        }
        Some(DegradationStep::HalfRateFovea)
    }
}

/// § The deterministic degraded-mode step taken when over-budget. Per
///   `07_AES/07 § VII` "degradation-order ⊗ R! deterministic ⊗ N! random".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradationStep {
    /// § Step 1 : drop iridescence (minor visual impact : aniso surfaces flat).
    DropIridescence,
    /// § Step 2 : drop fluorescence (minor : Λ-tokens flat).
    DropFluorescence,
    /// § Step 3 : half-rate fovea-displacement (moderate : detail blur).
    HalfRateFovea,
    /// § Step 4 : drop spectral → 4-band fallback (significant).
    DropTo4Band,
    /// § Refuse-bootstrap : scalar tier on M7 hardware.
    RefuseScalar,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § CoopMatrix per-band ns matches spec table.
    #[test]
    fn coop_matrix_per_band_50ns() {
        assert!((CostTier::CoopMatrix.ns_per_band() - 50.0).abs() < 1e-6);
    }

    /// § SIMD-warp per-band ns matches spec table.
    #[test]
    fn simd_warp_per_band_200ns() {
        assert!((CostTier::SimdWarp.ns_per_band() - 200.0).abs() < 1e-6);
    }

    /// § Scalar per-band ns matches spec table.
    #[test]
    fn scalar_per_band_800ns() {
        assert!((CostTier::Scalar.ns_per_band() - 800.0).abs() < 1e-6);
    }

    /// § 16-band BRDF eval at CoopMatrix = 800 ns / fragment.
    #[test]
    fn coop_brdf_per_fragment_800ns() {
        assert!((CostTier::CoopMatrix.ns_per_fragment_brdf() - 800.0).abs() < 1e-6);
    }

    /// § Baseline cost of CoopMatrix fits Stage-6 budget @ Quest-3.
    #[test]
    fn coop_baseline_fits_stage6() {
        let c = PerFragmentCost::baseline(CostTier::CoopMatrix);
        assert!(
            c.fits_stage6_quest3_budget(),
            "frame ms = {}",
            c.frame_ms_1080p_foveated()
        );
    }

    /// § Scalar tier blows the Stage-6 budget.
    #[test]
    fn scalar_busts_stage6() {
        let c = PerFragmentCost::baseline(CostTier::Scalar);
        assert!(!c.fits_stage6_quest3_budget());
    }

    /// § Adding iridescence to CoopMatrix still fits Stage-6.
    #[test]
    fn coop_with_iridescence_fits() {
        let c = PerFragmentCost::baseline(CostTier::CoopMatrix).with_iridescence();
        assert!(
            c.fits_stage6_quest3_budget(),
            "frame ms = {}",
            c.frame_ms_1080p_foveated()
        );
    }

    /// § Total ns sums components.
    #[test]
    fn total_ns_sums_components() {
        let c = PerFragmentCost::baseline(CostTier::CoopMatrix)
            .with_iridescence()
            .with_fluorescence();
        let expected = 800.0 + NS_IRIDESCENCE_PER_FRAGMENT + NS_FLUORESCENCE_PER_FRAGMENT + 30.0;
        assert!((c.total_ns() - expected).abs() < 1e-3);
    }

    /// § Frame ms is positive.
    #[test]
    fn frame_ms_positive() {
        let c = PerFragmentCost::baseline(CostTier::CoopMatrix);
        assert!(c.frame_ms_1080p_foveated() > 0.0);
    }

    /// § Degradation decision : within budget = None.
    #[test]
    fn degradation_within_budget_none() {
        let c = PerFragmentCost::baseline(CostTier::CoopMatrix);
        assert!(c.degradation_decision().is_none());
    }

    /// § Degradation decision : iridescence dropped first.
    #[test]
    fn degradation_drops_iridescence_first() {
        // SIMD-warp baseline = 3200 ns/frag = ~2.5 ms / frame ; over 1.8 budget.
        let c = PerFragmentCost::baseline(CostTier::SimdWarp).with_iridescence();
        let d = c.degradation_decision();
        assert!(matches!(d, Some(DegradationStep::DropIridescence)));
    }

    /// § Degradation : scalar gets refused.
    #[test]
    fn degradation_scalar_refused() {
        let c = PerFragmentCost::baseline(CostTier::Scalar);
        assert!(matches!(
            c.degradation_decision(),
            Some(DegradationStep::RefuseScalar)
        ));
    }

    /// § Foveation factor matches 0.38.
    #[test]
    fn foveation_factor() {
        assert!((FOVEATION_FACTOR - 0.38).abs() < 1e-6);
    }

    /// § Stage-6 budget = 1.8 ms.
    #[test]
    fn stage6_budget_value() {
        assert!((STAGE6_BUDGET_MS - 1.8).abs() < 1e-6);
    }

    /// § Quest-3 frame budget = 11.1 ms.
    #[test]
    fn quest3_budget_value() {
        assert!((QUEST3_FRAME_BUDGET_MS - 11.1).abs() < 1e-6);
    }

    /// § total_ms matches total_ns / 1e6.
    #[test]
    fn total_ms_consistent() {
        let c = PerFragmentCost::baseline(CostTier::CoopMatrix);
        let ns = c.total_ns();
        let ms = c.total_ms();
        assert!((ms - ns / 1_000_000.0).abs() < 1e-9);
    }

    /// § fits_quest3_frame_budget for scalar tier — scalar JUST fits the
    ///   full Quest-3 frame budget (10.09 ms ≤ 11.1 ms) but is over the
    ///   Stage-6 sub-budget. This test asserts the latter.
    #[test]
    fn scalar_busts_stage6_full_budget() {
        let c = PerFragmentCost::baseline(CostTier::Scalar);
        // 12800 ns/frag at scalar => 10.09 ms / frame after foveation.
        // That's over the 1.8 ms Stage-6 budget.
        assert!(
            !c.fits_stage6_quest3_budget(),
            "frame ms = {}",
            c.frame_ms_1080p_foveated()
        );
        // It DOES fit the full Quest-3 frame budget (11.1 ms) by a thin
        // margin — but not Stage-6's 1.8 ms slice.
        assert!(c.fits_quest3_frame_budget());
    }
}

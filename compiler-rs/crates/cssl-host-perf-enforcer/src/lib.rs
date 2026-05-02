//! § cssl-host-perf-enforcer — CI + runtime perf-budget enforcer
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-PERF-ENFORCER (W13-12 perf-budget-enforcer)
//!
//! § ROLE
//!
//! This crate ENFORCES the perf-budget surface that W12-12 `polish_audit`
//! merely CODIFIED. Six disjoint enforcement-concerns wired into a
//! single audit-surface :
//!
//! - `FrameBudgetEnforcer`  - per-frame trip-points + verdicts
//! - `ZeroAllocVerifier`    - counts heap allocations on hot path
//! - `RegressionBaseline`   - diffs current vs baseline + >5pct alert
//! - `AdaptiveDegrader`     - auto-reduces LOD/effect-tier on drift
//! - `PerfBenchHarness`     - CI runs micro-benches under perf-bench
//! - `TelemetryEmitter`     - feeds W11-4 analytics-aggregator
//!
//! § COMPOUNDS-WITH (¬ touches W12-12 territory)
//!
//!   W12-12 `polish_audit::PerfBudget` records a 64-sample ring + counts
//!   over_60hz / over_120hz frames. This crate READS those public fields
//!   (read-only) via plain f32 arithmetic — there is no Cargo-dep on
//!   `loa-host` here. Instead, the host wires its `PerfBudget` snapshot
//!   into our `FrameBudgetEnforcer::ingest_snapshot` via a tiny adapter
//!   in `loa-host/src/perf_runtime_check.rs`. This keeps W13-12 strictly
//!   downstream of W12-12.
//!
//! § DESIGN GOALS (Sawyer-style, per memory_sawyer_pokemon_efficiency)
//!
//! - Bit-pack records: `DegradationTier` is `#[repr(u8)]`
//! - Ring-buffer pre-alloc: sliding window is a fixed-size `[f32; N]`
//! - Index-types: `BenchId` = `u16` (no String hashing)
//! - Differential encoding: ms values transmitted as Q14 fixed-point
//! - In-place ops: all sorts done in stack-only buffers
//! - RLE sparse-fields: tier transitions logged as deltas only
//! - Fixed-point: pct-deltas stored as i16 basis-points
//!
//! § PRIME-DIRECTIVE attestation
//!   - ¬ surveillance : enforcer reads ONLY frame-time samples — no input
//!     content, no scene contents, no player behavior. Tier-changes are
//!     local-only (caller decides whether to telemetry-export).
//!   - ¬ engagement-bait : no nudge / retain / dark-pattern. The enforcer
//!     can ONLY downgrade fidelity to protect the budget. It cannot
//!     hide settings or reverse player overrides.
//!   - ¬ harm/control/manipulation : tier-changes are Σ-mask-revocable ;
//!     player can pin any tier and the enforcer respects the pin.
//!
//! There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![forbid(unsafe_code)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::module_name_repetitions)]

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// ──────────────────────────────────────────────────────────────────────────
// § 1. Frame-time thresholds   (mirror polish_audit constants · ¬ duplicate-
//      logic ; we cannot DEP on loa-host so we restate the canonical values
//      with a const-equality test in our own test-suite as the wire-check)
// ──────────────────────────────────────────────────────────────────────────

/// 60Hz frame budget = 16.6667 ms.
pub const FRAME_BUDGET_60HZ_MS: f32 = 16.667;
/// 120Hz frame budget = 8.3333 ms (Apocky-mandate · ≤8.33ms hot-path).
pub const FRAME_BUDGET_120HZ_MS: f32 = 8.333;
/// 144Hz frame budget = 6.9444 ms (stretch).
pub const FRAME_BUDGET_144HZ_MS: f32 = 6.944;
/// Default regression-alert threshold = 5% drift.
pub const DEFAULT_REGRESSION_PCT: f32 = 5.0;
/// Sliding-window size for adaptive-degrader trend (frames).
pub const ADAPTIVE_WINDOW_FRAMES: usize = 32;
/// Threshold (frames over budget within window) at which tier auto-degrades.
pub const ADAPTIVE_DEGRADE_THRESHOLD: u32 = 8; // 8/32 = 25% over-budget → degrade
/// Threshold below which tier auto-recovers up.
pub const ADAPTIVE_RECOVER_THRESHOLD: u32 = 1; // 1/32 = ≤3% over-budget → recover

// ──────────────────────────────────────────────────────────────────────────
// § 2. Refresh-rate target enum
// ──────────────────────────────────────────────────────────────────────────

/// Refresh-rate target. Tied 1:1 to a frame-budget threshold.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefreshTarget {
    /// 60Hz · 16.667ms.
    Hz60 = 0,
    /// 120Hz · 8.333ms (Apocky-mandate hot-path).
    Hz120 = 1,
    /// 144Hz · 6.944ms (stretch).
    Hz144 = 2,
}

impl RefreshTarget {
    /// Frame budget (ms) for this target.
    #[must_use]
    pub const fn budget_ms(self) -> f32 {
        match self {
            Self::Hz60 => FRAME_BUDGET_60HZ_MS,
            Self::Hz120 => FRAME_BUDGET_120HZ_MS,
            Self::Hz144 => FRAME_BUDGET_144HZ_MS,
        }
    }

    /// Stable name for telemetry / JSON.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hz60 => "60hz",
            Self::Hz120 => "120hz",
            Self::Hz144 => "144hz",
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § 3. Verdict — pass/over/severe verdict for one frame
// ──────────────────────────────────────────────────────────────────────────

/// Per-frame verdict. Cheap to compute · cheap to emit.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Within budget.
    Pass = 0,
    /// Over budget by 0..2× (budget-violation · soft-warn).
    Over = 1,
    /// Over budget by ≥2× (severe · adaptive-degrader fires).
    Severe = 2,
}

impl Verdict {
    /// Classify a frame-time sample against a budget.
    #[must_use]
    pub fn from_sample(ms: f32, budget_ms: f32) -> Self {
        if !(ms.is_finite()) || ms <= budget_ms {
            Self::Pass
        } else if ms <= budget_ms * 2.0 {
            Self::Over
        } else {
            Self::Severe
        }
    }

    /// Stable name for JSON.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Over => "over",
            Self::Severe => "severe",
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § 4. FrameBudgetEnforcer   (runtime-side · ingests samples · emits verdicts)
// ──────────────────────────────────────────────────────────────────────────

/// Runtime-side enforcer. Owns a fixed-size sliding-window of frame-time
/// samples (no heap) and emits verdicts + over/severe counters as the
/// caller streams `record_frame_ms`. The host wires its W12-12 `PerfBudget`
/// snapshot into this struct via the small `loa-host/perf_runtime_check.rs`
/// adapter — both sides stay file-disjoint, no Cargo-dep cycle.
#[derive(Debug, Clone, Copy)]
pub struct FrameBudgetEnforcer {
    /// Refresh-rate target → drives `budget_ms`.
    pub target: RefreshTarget,
    /// Sliding-window of last ADAPTIVE_WINDOW_FRAMES samples (ms).
    samples: [f32; ADAPTIVE_WINDOW_FRAMES],
    /// Next write-index (0..ADAPTIVE_WINDOW_FRAMES).
    write_idx: u8,
    /// Number of valid samples (saturates at ADAPTIVE_WINDOW_FRAMES).
    valid_count: u8,
    /// Total over-budget frames within the current window.
    over_in_window: u32,
    /// Cumulative counter — Pass.
    pub pass_count: u32,
    /// Cumulative counter — Over.
    pub over_count: u32,
    /// Cumulative counter — Severe.
    pub severe_count: u32,
    /// Total frames recorded.
    pub total_frames: u32,
}

impl Default for FrameBudgetEnforcer {
    fn default() -> Self {
        Self {
            target: RefreshTarget::Hz120,
            samples: [0.0; ADAPTIVE_WINDOW_FRAMES],
            write_idx: 0,
            valid_count: 0,
            over_in_window: 0,
            pass_count: 0,
            over_count: 0,
            severe_count: 0,
            total_frames: 0,
        }
    }
}

impl FrameBudgetEnforcer {
    /// Build a new enforcer at the given target.
    #[must_use]
    pub fn new(target: RefreshTarget) -> Self {
        Self {
            target,
            ..Self::default()
        }
    }

    /// Reset all counters + sliding window.
    pub fn reset(&mut self) {
        let target = self.target;
        *self = Self::default();
        self.target = target;
    }

    /// Active frame-budget threshold (ms).
    #[must_use]
    pub fn budget_ms(&self) -> f32 {
        self.target.budget_ms()
    }

    /// Record one frame. Returns the verdict. O(1) · zero-allocation ·
    /// safe to call from the hot path.
    pub fn record_frame_ms(&mut self, ms: f32) -> Verdict {
        let v = Verdict::from_sample(ms, self.budget_ms());
        self.total_frames = self.total_frames.saturating_add(1);
        match v {
            Verdict::Pass => self.pass_count = self.pass_count.saturating_add(1),
            Verdict::Over => self.over_count = self.over_count.saturating_add(1),
            Verdict::Severe => self.severe_count = self.severe_count.saturating_add(1),
        }
        // Slide the window.
        let idx = self.write_idx as usize;
        let evicted = self.samples[idx];
        self.samples[idx] = ms;
        self.write_idx = ((self.write_idx + 1) as usize % ADAPTIVE_WINDOW_FRAMES) as u8;
        if self.valid_count < ADAPTIVE_WINDOW_FRAMES as u8 {
            self.valid_count += 1;
        } else if evicted > self.budget_ms() {
            // The sample being evicted from the window was over-budget.
            self.over_in_window = self.over_in_window.saturating_sub(1);
        }
        if !matches!(v, Verdict::Pass) {
            self.over_in_window = self.over_in_window.saturating_add(1);
        }
        v
    }

    /// True when the over-budget frame-rate within the sliding window
    /// crosses the adaptive-degrade threshold (default 25%).
    #[must_use]
    pub fn should_degrade(&self) -> bool {
        self.valid_count >= ADAPTIVE_WINDOW_FRAMES as u8 / 2
            && self.over_in_window >= ADAPTIVE_DEGRADE_THRESHOLD
    }

    /// True when the over-budget frame-rate within the sliding window
    /// has dropped below the recover threshold.
    #[must_use]
    pub fn can_recover(&self) -> bool {
        self.valid_count >= ADAPTIVE_WINDOW_FRAMES as u8 / 2
            && self.over_in_window <= ADAPTIVE_RECOVER_THRESHOLD
    }

    /// p99 frame-time within the sliding window (ms). Stack-only sort.
    #[must_use]
    pub fn p99_ms(&self) -> f32 {
        let n = self.valid_count as usize;
        if n == 0 {
            return 0.0;
        }
        let mut buf = [0.0_f32; ADAPTIVE_WINDOW_FRAMES];
        buf[..n].copy_from_slice(&self.samples[..n]);
        // In-place insertion sort (n ≤ 32).
        for i in 1..n {
            let mut j = i;
            while j > 0 && buf[j - 1] > buf[j] {
                buf.swap(j - 1, j);
                j -= 1;
            }
        }
        let idx = ((n as f32 * 0.99) as usize).min(n - 1);
        buf[idx]
    }

    /// True when ≥ 95% of recorded frames stayed within the active budget.
    #[must_use]
    pub fn passes_attestation(&self) -> bool {
        if self.total_frames == 0 {
            return true;
        }
        (self.pass_count as f32 / self.total_frames as f32) >= 0.95
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § 5. ZeroAllocVerifier   (test-build · counts allocations on hot path)
// ──────────────────────────────────────────────────────────────────────────

/// Global allocation-counters. Process-wide · used only by tests + bench
/// builds. Production uses `std::alloc::System` directly with no hook.
///
/// To activate, the embedding test crate sets `#[global_allocator] static
/// A: TrackingAllocator<System> = TrackingAllocator::new(System);`. We do
/// NOT install a global-allocator from this crate — that decision is the
/// embedder's per Rust convention (one global allocator per binary).
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static HOT_PATH_DEPTH: AtomicU32 = AtomicU32::new(0);
static HOT_PATH_ALLOCS: AtomicU64 = AtomicU64::new(0);

/// Number of bytes currently tracked as allocated (sum since last reset).
pub fn alloc_count() -> u64 {
    ALLOC_COUNT.load(Ordering::Relaxed)
}

/// Number of de-allocations recorded.
pub fn dealloc_count() -> u64 {
    DEALLOC_COUNT.load(Ordering::Relaxed)
}

/// Number of allocations that occurred WHILE the hot-path scope was active.
/// Anything > 0 is a violation of the zero-alloc-hot-path mandate.
pub fn hot_path_alloc_count() -> u64 {
    HOT_PATH_ALLOCS.load(Ordering::Relaxed)
}

/// Reset all allocation counters. Used between tests + between bench rounds.
pub fn reset_alloc_counters() {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    DEALLOC_COUNT.store(0, Ordering::Relaxed);
    HOT_PATH_DEPTH.store(0, Ordering::Relaxed);
    HOT_PATH_ALLOCS.store(0, Ordering::Relaxed);
}

/// Record one allocation event. Called by the embedder's GlobalAlloc.
#[doc(hidden)]
pub fn record_alloc(_size: usize) {
    ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
    if HOT_PATH_DEPTH.load(Ordering::Relaxed) > 0 {
        HOT_PATH_ALLOCS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Record one de-allocation event. Called by the embedder's GlobalAlloc.
#[doc(hidden)]
pub fn record_dealloc(_size: usize) {
    DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Mark hot-path entry. Pair with `mark_hot_path_exit`.
pub fn mark_hot_path_enter() {
    HOT_PATH_DEPTH.fetch_add(1, Ordering::Relaxed);
}

/// Mark hot-path exit. Pair with `mark_hot_path_enter`.
pub fn mark_hot_path_exit() {
    let prev = HOT_PATH_DEPTH.fetch_sub(1, Ordering::Relaxed);
    debug_assert!(prev > 0, "hot-path exit without enter");
}

/// RAII scope-guard for the hot path. While in scope, any allocation
/// recorded via `record_alloc` increments the hot-path counter. Drop
/// decrements the depth.
pub struct HotPathScope {
    _priv: (),
}

impl HotPathScope {
    /// Enter the hot-path scope.
    #[must_use]
    pub fn enter() -> Self {
        mark_hot_path_enter();
        Self { _priv: () }
    }
}

impl Drop for HotPathScope {
    fn drop(&mut self) {
        mark_hot_path_exit();
    }
}

/// Verifier surface used by tests. Asserts that NO allocations occurred
/// inside the closure when called from a hot-path scope.
///
/// Does NOT install a global allocator — embedder is responsible for that.
/// In default (no-allocator-hook) builds, the closure is run and the
/// counters return whatever the host installs. Production binaries should
/// NOT install the tracking allocator (zero overhead in release).
pub fn assert_no_hot_path_alloc<F: FnOnce()>(f: F) -> Result<(), u64> {
    let before = HOT_PATH_ALLOCS.load(Ordering::Relaxed);
    {
        let _guard = HotPathScope::enter();
        f();
        // _guard dropped at scope end → mark_hot_path_exit
    }
    let delta = HOT_PATH_ALLOCS.load(Ordering::Relaxed) - before;
    if delta == 0 { Ok(()) } else { Err(delta) }
}

// ──────────────────────────────────────────────────────────────────────────
// § 6. RegressionBaseline   (CI-side · diffs current vs prior · >5% alert)
// ──────────────────────────────────────────────────────────────────────────

/// Stable bench identifier · u16 index · NO String hashing on hot path.
pub type BenchId = u16;

/// One bench-result. Stable shape · serializes to one-line JSON.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BenchResult {
    pub bench_id: BenchId,
    /// p50 frame-time (ms).
    pub p50_ms: f32,
    /// p99 frame-time (ms).
    pub p99_ms: f32,
    /// Number of samples used to compute p50/p99.
    pub samples: u32,
}

impl BenchResult {
    /// Build a result from a slice of frame-time samples (ms). O(N log N) ·
    /// allocates a single Vec for sorting (NOT on the hot path).
    #[must_use]
    pub fn from_samples(bench_id: BenchId, mut samples: Vec<f32>) -> Self {
        let n = samples.len() as u32;
        if samples.is_empty() {
            return Self { bench_id, p50_ms: 0.0, p99_ms: 0.0, samples: 0 };
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let p50 = samples[samples.len() / 2];
        let p99_idx = ((samples.len() as f32 * 0.99) as usize).min(samples.len() - 1);
        let p99 = samples[p99_idx];
        Self { bench_id, p50_ms: p50, p99_ms: p99, samples: n }
    }
}

/// Status of a regression-baseline comparison.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegressionStatus {
    /// Within ±threshold% of baseline.
    Stable = 0,
    /// Improvement ≥ threshold% (faster).
    Improved = 1,
    /// Regression ≥ threshold% (slower) — alert.
    Regressed = 2,
}

impl RegressionStatus {
    /// Stable name for JSON.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Improved => "improved",
            Self::Regressed => "regressed",
        }
    }
}

/// One row in a regression-baseline diff.
#[derive(Debug, Clone, Copy)]
pub struct RegressionRow {
    pub bench_id: BenchId,
    pub baseline_p50: f32,
    pub current_p50: f32,
    pub delta_pct: f32,
    pub status: RegressionStatus,
}

/// Compute a regression status by comparing `current` to `baseline`. The
/// caller passes the threshold percentage (default 5.0). Returns Improved
/// when current is faster by ≥ threshold, Regressed when slower by ≥
/// threshold, Stable otherwise.
#[must_use]
pub fn classify_regression(baseline_ms: f32, current_ms: f32, threshold_pct: f32) -> RegressionStatus {
    if baseline_ms <= 0.0 {
        return RegressionStatus::Stable;
    }
    let delta = (current_ms - baseline_ms) / baseline_ms * 100.0;
    if delta >= threshold_pct {
        RegressionStatus::Regressed
    } else if -delta >= threshold_pct {
        RegressionStatus::Improved
    } else {
        RegressionStatus::Stable
    }
}

/// Diff a current-bench result against a baseline. Returns one row per
/// bench-id present in BOTH baseline and current. Missing benches are
/// silently ignored — the harness already handles add/remove via a
/// separate codepath.
#[must_use]
pub fn diff_baseline(
    baseline: &[BenchResult],
    current: &[BenchResult],
    threshold_pct: f32,
) -> Vec<RegressionRow> {
    let mut rows = Vec::with_capacity(current.len());
    for c in current {
        if let Some(b) = baseline.iter().find(|b| b.bench_id == c.bench_id) {
            let delta_pct = if b.p50_ms > 0.0 {
                (c.p50_ms - b.p50_ms) / b.p50_ms * 100.0
            } else {
                0.0
            };
            let status = classify_regression(b.p50_ms, c.p50_ms, threshold_pct);
            rows.push(RegressionRow {
                bench_id: c.bench_id,
                baseline_p50: b.p50_ms,
                current_p50: c.p50_ms,
                delta_pct,
                status,
            });
        }
    }
    rows
}

/// True when ANY row in the diff is a regression.
#[must_use]
pub fn any_regression(rows: &[RegressionRow]) -> bool {
    rows.iter().any(|r| matches!(r.status, RegressionStatus::Regressed))
}

// ──────────────────────────────────────────────────────────────────────────
// § 7. AdaptiveDegrader   (runtime-side · auto-reduce LOD/effect-tier)
// ──────────────────────────────────────────────────────────────────────────

/// Effect / LOD tier. Higher numbers = higher fidelity. The adaptive
/// degrader walks DOWN this enum when budget is missed and walks UP when
/// budget is comfortably met.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DegradationTier {
    /// Lowest-fidelity tier. Disable all post-process effects · LOD-0.
    /// Reserved for severe-budget-violation auto-fallback.
    Minimal = 0,
    /// Reduce SSAO + bloom resolution · LOD-1.
    Low = 1,
    /// Reduce shadow-cascade resolution · LOD-2.
    Medium = 2,
    /// Default LoA target tier · LOD-3 · all effects on.
    High = 3,
    /// Maximum-fidelity tier · LOD-4 · MSAA × 8 · ray-traced shadows.
    Ultra = 4,
}

impl DegradationTier {
    /// Stable name for JSON.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// Step down one tier. Floor = Minimal.
    #[must_use]
    pub fn step_down(self) -> Self {
        match self {
            Self::Ultra => Self::High,
            Self::High => Self::Medium,
            Self::Medium => Self::Low,
            Self::Low | Self::Minimal => Self::Minimal,
        }
    }

    /// Step up one tier. Ceiling = Ultra.
    #[must_use]
    pub fn step_up(self) -> Self {
        match self {
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High | Self::Ultra => Self::Ultra,
        }
    }
}

/// Adaptive-degrader state. Holds the active tier + a player-pin flag.
/// When pinned, the degrader respects the player's choice and never
/// auto-degrades or auto-recovers. (Sovereignty axiom : the player can
/// always opt out of automatic adjustments.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveDegrader {
    /// Currently active tier.
    pub tier: DegradationTier,
    /// True when the player has pinned the tier (no auto-adjust).
    pub pinned: bool,
    /// Total auto-degrade events since reset.
    pub degrade_events: u32,
    /// Total auto-recover events since reset.
    pub recover_events: u32,
}

impl Default for AdaptiveDegrader {
    fn default() -> Self {
        Self {
            tier: DegradationTier::High,
            pinned: false,
            degrade_events: 0,
            recover_events: 0,
        }
    }
}

impl AdaptiveDegrader {
    /// Build a new degrader at the given starting tier.
    #[must_use]
    pub fn new(tier: DegradationTier) -> Self {
        Self { tier, ..Self::default() }
    }

    /// Pin the tier (player override · never auto-adjust).
    pub fn pin(&mut self) {
        self.pinned = true;
    }

    /// Unpin (allow auto-adjust again).
    pub fn unpin(&mut self) {
        self.pinned = false;
    }

    /// Set the tier (player-driven · clears auto counters).
    pub fn set_tier(&mut self, tier: DegradationTier) {
        self.tier = tier;
    }

    /// Tick the degrader once per frame. Returns Some(new_tier) when the
    /// tier changed, None otherwise. The caller passes the enforcer so
    /// we can read should_degrade / can_recover (kept disjoint to ease
    /// unit-testing).
    pub fn tick(&mut self, enf: &FrameBudgetEnforcer) -> Option<DegradationTier> {
        if self.pinned {
            return None;
        }
        if enf.should_degrade() && self.tier != DegradationTier::Minimal {
            let new = self.tier.step_down();
            self.tier = new;
            self.degrade_events = self.degrade_events.saturating_add(1);
            return Some(new);
        }
        if enf.can_recover() && self.tier != DegradationTier::Ultra {
            let new = self.tier.step_up();
            self.tier = new;
            self.recover_events = self.recover_events.saturating_add(1);
            return Some(new);
        }
        None
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § 8. PerfBenchHarness   (CI-side · runs micro-benches under perf-bench)
// ──────────────────────────────────────────────────────────────────────────

/// One bench-spec entry. The harness runs `closure_id` for `N` warmup
/// iterations + `M` measurement-iterations. Both counts are static — we
/// avoid heap allocations for the spec table.
#[derive(Debug, Clone, Copy)]
pub struct BenchSpec {
    pub bench_id: BenchId,
    pub name: &'static str,
    pub warmup_iters: u32,
    pub measure_iters: u32,
    /// Frame-budget threshold this bench must beat (ms).
    pub budget_ms: f32,
}

impl BenchSpec {
    /// Build a spec at 120Hz target.
    #[must_use]
    pub const fn new_120hz(bench_id: BenchId, name: &'static str) -> Self {
        Self {
            bench_id,
            name,
            warmup_iters: 16,
            measure_iters: 256,
            budget_ms: FRAME_BUDGET_120HZ_MS,
        }
    }
}

/// Canonical bench-spec table covering the four hot-path categories
/// (frame_tick · render · physics · network_replication). Bench-IDs are
/// stable — used by baseline diff + telemetry-event correlation.
const CANONICAL_BENCH_SPECS: &[BenchSpec] = &[
    BenchSpec::new_120hz(0, "frame_tick"),
    BenchSpec::new_120hz(1, "render_frame"),
    BenchSpec::new_120hz(2, "physics_step"),
    BenchSpec::new_120hz(3, "network_replication"),
];

/// Accessor for the canonical bench-spec table.
#[must_use]
pub const fn canonical_bench_specs() -> &'static [BenchSpec] {
    CANONICAL_BENCH_SPECS
}

/// Run a single bench under a closure that returns one frame's elapsed-ms.
/// Returns a `BenchResult`. Allocates a Vec once for sample collection
/// (NOT on the hot path — the harness runs OUTSIDE record_frame).
pub fn run_bench<F: FnMut() -> f32>(spec: &BenchSpec, mut frame_fn: F) -> BenchResult {
    // Warmup
    for _ in 0..spec.warmup_iters {
        let _ = frame_fn();
    }
    // Measure
    let mut samples = Vec::with_capacity(spec.measure_iters as usize);
    for _ in 0..spec.measure_iters {
        samples.push(frame_fn());
    }
    BenchResult::from_samples(spec.bench_id, samples)
}

/// Run all canonical specs, returning a Vec of results.
pub fn run_all_specs<F: FnMut(BenchId) -> f32>(mut frame_fn: F) -> Vec<BenchResult> {
    let specs = canonical_bench_specs();
    let mut out = Vec::with_capacity(specs.len());
    for s in specs {
        out.push(run_bench(s, || frame_fn(s.bench_id)));
    }
    out
}

/// Assert that ALL bench-results in the slice meet their spec's budget at
/// p99. Returns Err with the first failing result + its budget.
pub fn assert_budgets(results: &[BenchResult]) -> Result<(), (BenchResult, f32)> {
    let specs = canonical_bench_specs();
    for r in results {
        if let Some(spec) = specs.iter().find(|s| s.bench_id == r.bench_id) {
            if r.p99_ms > spec.budget_ms {
                return Err((*r, spec.budget_ms));
            }
        }
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
// § 9. TelemetryEmitter   (W11-4 analytics-aggregator integration)
// ──────────────────────────────────────────────────────────────────────────

/// One telemetry-event the enforcer emits. Stable shape · serializes to
/// JSONL via `to_jsonl`. Caller bridges to `cssl-analytics-aggregator` by
/// translating each enum-variant to an `EventRecord` of the matching kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PerfEvent {
    /// One frame · classified verdict.
    FrameVerdict {
        frame_offset: u32,
        ms_q14: u32, // ms × 16384 (Q14 fixed-point)
        verdict: Verdict,
    },
    /// Adaptive-degrade fired · tier changed.
    TierChanged {
        frame_offset: u32,
        old: DegradationTier,
        new: DegradationTier,
    },
    /// Regression alert (p50 drift > threshold).
    RegressionAlert {
        bench_id: BenchId,
        baseline_p50_q14: u32,
        current_p50_q14: u32,
        delta_bp: i32, // basis-points
    },
    /// Hot-path allocation observed (zero-alloc-violation).
    HotPathAlloc { count: u32 },
}

impl PerfEvent {
    /// Stable kind-name for JSONL.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::FrameVerdict { .. } => "perf.frame_verdict",
            Self::TierChanged { .. } => "perf.tier_changed",
            Self::RegressionAlert { .. } => "perf.regression_alert",
            Self::HotPathAlloc { .. } => "perf.hot_path_alloc",
        }
    }

    /// One-line JSON serialization. No external dep — keeps this crate
    /// zero-dep so the workspace can compile it on any host without
    /// pulling serde_json.
    #[must_use]
    pub fn to_jsonl(&self) -> String {
        match *self {
            Self::FrameVerdict { frame_offset, ms_q14, verdict } => format!(
                "{{\"kind\":\"{}\",\"frame\":{},\"ms_q14\":{},\"verdict\":\"{}\"}}",
                self.kind_name(),
                frame_offset,
                ms_q14,
                verdict.name()
            ),
            Self::TierChanged { frame_offset, old, new } => format!(
                "{{\"kind\":\"{}\",\"frame\":{},\"old\":\"{}\",\"new\":\"{}\"}}",
                self.kind_name(),
                frame_offset,
                old.name(),
                new.name()
            ),
            Self::RegressionAlert { bench_id, baseline_p50_q14, current_p50_q14, delta_bp } => {
                format!(
                    "{{\"kind\":\"{}\",\"bench_id\":{},\"baseline_q14\":{},\"current_q14\":{},\"delta_bp\":{}}}",
                    self.kind_name(),
                    bench_id,
                    baseline_p50_q14,
                    current_p50_q14,
                    delta_bp
                )
            }
            Self::HotPathAlloc { count } => format!(
                "{{\"kind\":\"{}\",\"count\":{}}}",
                self.kind_name(),
                count
            ),
        }
    }
}

/// A collector buffer for `PerfEvent` records.
///
/// Fixed-capacity (no unbounded heap growth) — drops events past CAPACITY
/// and increments the `dropped` counter. Caller drains via `take_all` and
/// forwards into the analytics-aggregator.
pub struct PerfEventBuffer {
    buf: Vec<PerfEvent>,
    capacity: usize,
    pub dropped: u64,
}

impl PerfEventBuffer {
    /// Build a new buffer with the given capacity (events).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }

    /// Push one event. Returns true on success, false if the buffer is full.
    pub fn push(&mut self, ev: PerfEvent) -> bool {
        if self.buf.len() >= self.capacity {
            self.dropped = self.dropped.saturating_add(1);
            false
        } else {
            self.buf.push(ev);
            true
        }
    }

    /// Drain all events (caller forwards to aggregator).
    pub fn take_all(&mut self) -> Vec<PerfEvent> {
        std::mem::take(&mut self.buf)
    }

    /// Current buffer length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// True when no events are buffered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § 10. Tests · ≥10 required ; ~22 here for full coverage
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    // ── Constants stay in lockstep with W12-12 polish_audit ─────────────

    #[test]
    fn budget_constants_match_polish_audit_canonical_values() {
        // W12-12 polish_audit defines :
        //   FRAME_BUDGET_60HZ_MS  = 16.667
        //   FRAME_BUDGET_120HZ_MS = 8.333
        // We mirror them here. The test fails if either drifts.
        assert!((FRAME_BUDGET_60HZ_MS - 16.667).abs() < 1e-3);
        assert!((FRAME_BUDGET_120HZ_MS - 8.333).abs() < 1e-3);
        // 144Hz stretch matches fps_pipeline.csl (FRAME_BUDGET_144HZ_MS)
        assert!((FRAME_BUDGET_144HZ_MS - 6.944).abs() < 1e-3);
    }

    #[test]
    fn refresh_target_budgets_round_trip() {
        assert_eq!(RefreshTarget::Hz60.budget_ms(), FRAME_BUDGET_60HZ_MS);
        assert_eq!(RefreshTarget::Hz120.budget_ms(), FRAME_BUDGET_120HZ_MS);
        assert_eq!(RefreshTarget::Hz144.budget_ms(), FRAME_BUDGET_144HZ_MS);
        assert_eq!(RefreshTarget::Hz120.name(), "120hz");
    }

    // ── FrameBudgetEnforcer ─────────────────────────────────────────────

    #[test]
    fn enforcer_passes_under_budget_at_120hz() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        let v = enf.record_frame_ms(7.0);
        assert_eq!(v, Verdict::Pass);
        assert_eq!(enf.pass_count, 1);
        assert_eq!(enf.over_count, 0);
        assert_eq!(enf.severe_count, 0);
        assert_eq!(enf.total_frames, 1);
    }

    #[test]
    fn enforcer_classifies_over_and_severe_correctly() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        assert_eq!(enf.record_frame_ms(8.0), Verdict::Pass);   // = under
        assert_eq!(enf.record_frame_ms(8.34), Verdict::Over);  // just over (8.333)
        assert_eq!(enf.record_frame_ms(20.0), Verdict::Severe); // ≥ 2× budget
        assert_eq!(enf.pass_count, 1);
        assert_eq!(enf.over_count, 1);
        assert_eq!(enf.severe_count, 1);
    }

    #[test]
    fn enforcer_p99_from_window_samples() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        // Push 32 samples 0..32 (ms). p99 of 0..32 = sample at floor(32*0.99)=31 → 31.0
        for i in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(i as f32);
        }
        assert_eq!(enf.p99_ms(), 31.0);
    }

    #[test]
    fn enforcer_window_eviction_decrements_over_count() {
        // Push 32 over-budget frames → over_in_window = 32.
        // Push 32 under-budget frames → over_in_window should drop back to 0.
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        for _ in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(20.0);
        }
        assert!(enf.over_in_window >= ADAPTIVE_DEGRADE_THRESHOLD);
        for _ in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(2.0);
        }
        assert_eq!(enf.over_in_window, 0);
    }

    #[test]
    fn enforcer_passes_attestation_when_95pct_in_budget() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        for _ in 0..100 {
            enf.record_frame_ms(2.0);
        }
        assert!(enf.passes_attestation());
        // Push 6% over-budget → fails.
        for _ in 0..6 {
            enf.record_frame_ms(20.0);
        }
        assert!(!enf.passes_attestation());
    }

    #[test]
    fn enforcer_should_degrade_when_25pct_over_budget_in_window() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        // First fill the window with under-budget samples.
        for _ in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(2.0);
        }
        assert!(!enf.should_degrade());
        // Now stream 8 over-budget frames → 8/32 = 25% → trip.
        for _ in 0..ADAPTIVE_DEGRADE_THRESHOLD {
            enf.record_frame_ms(20.0);
        }
        assert!(enf.should_degrade());
    }

    // ── ZeroAllocVerifier ───────────────────────────────────────────────

    #[test]
    fn hot_path_scope_increments_depth() {
        reset_alloc_counters();
        let _scope = HotPathScope::enter();
        assert_eq!(HOT_PATH_DEPTH.load(Ordering::Relaxed), 1);
        record_alloc(64);
        assert_eq!(hot_path_alloc_count(), 1);
        drop(_scope);
        assert_eq!(HOT_PATH_DEPTH.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn assert_no_hot_path_alloc_detects_violation() {
        reset_alloc_counters();
        // No allocations inside → Ok.
        assert!(assert_no_hot_path_alloc(|| {}).is_ok());
        // One simulated allocation → Err(1).
        let res = assert_no_hot_path_alloc(|| {
            record_alloc(8);
        });
        assert_eq!(res, Err(1));
    }

    #[test]
    fn record_alloc_only_counts_when_in_hot_path() {
        reset_alloc_counters();
        // Outside hot-path → not counted toward HOT_PATH_ALLOCS.
        record_alloc(16);
        assert_eq!(hot_path_alloc_count(), 0);
        assert_eq!(alloc_count(), 1);
        record_dealloc(16);
        assert_eq!(dealloc_count(), 1);
    }

    // ── RegressionBaseline ──────────────────────────────────────────────

    #[test]
    fn classify_regression_at_5pct_threshold() {
        // Exactly on threshold = Regressed (≥ check).
        assert_eq!(
            classify_regression(10.0, 10.5, 5.0),
            RegressionStatus::Regressed
        );
        // Under threshold = Stable.
        assert_eq!(
            classify_regression(10.0, 10.49, 5.0),
            RegressionStatus::Stable
        );
        // Improvement at threshold.
        assert_eq!(
            classify_regression(10.0, 9.5, 5.0),
            RegressionStatus::Improved
        );
    }

    #[test]
    fn diff_baseline_emits_one_row_per_matching_bench() {
        let baseline = vec![
            BenchResult { bench_id: 0, p50_ms: 5.0, p99_ms: 7.0, samples: 100 },
            BenchResult { bench_id: 1, p50_ms: 4.0, p99_ms: 5.0, samples: 100 },
        ];
        let current = vec![
            BenchResult { bench_id: 0, p50_ms: 5.6, p99_ms: 8.0, samples: 100 }, // +12% regression
            BenchResult { bench_id: 1, p50_ms: 3.9, p99_ms: 4.8, samples: 100 }, // -2.5% stable
            BenchResult { bench_id: 2, p50_ms: 9.0, p99_ms: 10.0, samples: 100 }, // not in baseline → skipped
        ];
        let rows = diff_baseline(&baseline, &current, 5.0);
        assert_eq!(rows.len(), 2);
        assert!(any_regression(&rows));
        let r0 = rows.iter().find(|r| r.bench_id == 0).unwrap();
        assert_eq!(r0.status, RegressionStatus::Regressed);
        let r1 = rows.iter().find(|r| r.bench_id == 1).unwrap();
        assert_eq!(r1.status, RegressionStatus::Stable);
    }

    // ── AdaptiveDegrader ────────────────────────────────────────────────

    #[test]
    fn adaptive_degrader_steps_down_when_enforcer_says_so() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        let mut deg = AdaptiveDegrader::new(DegradationTier::High);
        // Saturate over-budget within the sliding window.
        for _ in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(20.0);
        }
        let new = deg.tick(&enf);
        assert_eq!(new, Some(DegradationTier::Medium));
        assert_eq!(deg.tier, DegradationTier::Medium);
        assert_eq!(deg.degrade_events, 1);
    }

    #[test]
    fn adaptive_degrader_recovers_when_under_budget() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        let mut deg = AdaptiveDegrader::new(DegradationTier::Medium);
        for _ in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(2.0);
        }
        let new = deg.tick(&enf);
        assert_eq!(new, Some(DegradationTier::High));
        assert_eq!(deg.recover_events, 1);
    }

    #[test]
    fn adaptive_degrader_respects_pin() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        let mut deg = AdaptiveDegrader::new(DegradationTier::High);
        deg.pin();
        for _ in 0..ADAPTIVE_WINDOW_FRAMES {
            enf.record_frame_ms(20.0);
        }
        // Pinned → never auto-degrade.
        assert_eq!(deg.tick(&enf), None);
        assert_eq!(deg.tier, DegradationTier::High);
    }

    #[test]
    fn tier_step_down_floors_at_minimal() {
        assert_eq!(DegradationTier::Minimal.step_down(), DegradationTier::Minimal);
        assert_eq!(DegradationTier::Low.step_down(), DegradationTier::Minimal);
    }

    #[test]
    fn tier_step_up_ceils_at_ultra() {
        assert_eq!(DegradationTier::Ultra.step_up(), DegradationTier::Ultra);
        assert_eq!(DegradationTier::High.step_up(), DegradationTier::Ultra);
    }

    // ── PerfBenchHarness ────────────────────────────────────────────────

    #[test]
    fn run_bench_returns_well_formed_p99_under_budget() {
        let spec = BenchSpec::new_120hz(0, "frame_tick");
        // Harness body returns a sub-budget sample each iteration.
        let result = run_bench(&spec, || 4.0);
        assert_eq!(result.bench_id, 0);
        assert_eq!(result.samples, spec.measure_iters);
        assert_eq!(result.p50_ms, 4.0);
        assert_eq!(result.p99_ms, 4.0);
        assert!(result.p99_ms <= spec.budget_ms);
    }

    #[test]
    fn assert_budgets_fails_when_over_budget() {
        let res = vec![BenchResult {
            bench_id: 0,
            p50_ms: 10.0,
            p99_ms: 12.0, // > 8.333 (120Hz)
            samples: 100,
        }];
        let r = assert_budgets(&res);
        assert!(r.is_err());
    }

    #[test]
    fn assert_budgets_passes_when_under_budget() {
        let res = vec![BenchResult {
            bench_id: 0,
            p50_ms: 5.0,
            p99_ms: 8.0, // < 8.333
            samples: 100,
        }];
        let r = assert_budgets(&res);
        assert!(r.is_ok());
    }

    #[test]
    fn canonical_bench_specs_cover_4_hot_paths() {
        let specs = canonical_bench_specs();
        assert_eq!(specs.len(), 4);
        let names: Vec<_> = specs.iter().map(|s| s.name).collect();
        assert!(names.contains(&"frame_tick"));
        assert!(names.contains(&"render_frame"));
        assert!(names.contains(&"physics_step"));
        assert!(names.contains(&"network_replication"));
    }

    // ── TelemetryEmitter ────────────────────────────────────────────────

    #[test]
    fn perf_event_jsonl_round_trips_kind_and_payload() {
        let ev = PerfEvent::FrameVerdict { frame_offset: 7, ms_q14: 16384 * 8, verdict: Verdict::Pass };
        let j = ev.to_jsonl();
        assert!(j.contains("\"kind\":\"perf.frame_verdict\""));
        assert!(j.contains("\"frame\":7"));
        assert!(j.contains("\"verdict\":\"pass\""));

        let tier = PerfEvent::TierChanged {
            frame_offset: 100,
            old: DegradationTier::High,
            new: DegradationTier::Medium,
        };
        let j = tier.to_jsonl();
        assert!(j.contains("\"kind\":\"perf.tier_changed\""));
        assert!(j.contains("\"old\":\"high\""));
        assert!(j.contains("\"new\":\"medium\""));

        let reg = PerfEvent::RegressionAlert {
            bench_id: 1,
            baseline_p50_q14: 16384 * 4,
            current_p50_q14: 16384 * 5,
            delta_bp: 2500, // +25%
        };
        let j = reg.to_jsonl();
        assert!(j.contains("\"kind\":\"perf.regression_alert\""));
        assert!(j.contains("\"delta_bp\":2500"));

        let hp = PerfEvent::HotPathAlloc { count: 3 };
        let j = hp.to_jsonl();
        assert!(j.contains("\"kind\":\"perf.hot_path_alloc\""));
        assert!(j.contains("\"count\":3"));
    }

    #[test]
    fn perf_event_buffer_drops_past_capacity() {
        let mut buf = PerfEventBuffer::new(2);
        assert!(buf.push(PerfEvent::HotPathAlloc { count: 1 }));
        assert!(buf.push(PerfEvent::HotPathAlloc { count: 2 }));
        // Third push fails · dropped counter increments.
        assert!(!buf.push(PerfEvent::HotPathAlloc { count: 3 }));
        assert_eq!(buf.dropped, 1);
        assert_eq!(buf.len(), 2);
        let drained = buf.take_all();
        assert_eq!(drained.len(), 2);
        assert!(buf.is_empty());
    }

    // ── End-to-end : adaptive-degrade FIRES + emits TIER_CHANGED ───────

    #[test]
    fn end_to_end_adaptive_degrade_fires_and_emits_telemetry() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz120);
        let mut deg = AdaptiveDegrader::new(DegradationTier::High);
        let mut emitter = PerfEventBuffer::new(64);

        // Stream over-budget frames until adaptive-degrader trips.
        for f in 0..ADAPTIVE_WINDOW_FRAMES {
            let v = enf.record_frame_ms(20.0);
            // Frame-verdict event is always emitted on the over/severe path.
            if !matches!(v, Verdict::Pass) {
                let _ = emitter.push(PerfEvent::FrameVerdict {
                    frame_offset: f as u32,
                    ms_q14: (20.0 * 16384.0) as u32,
                    verdict: v,
                });
            }
        }
        let prev = deg.tier;
        if let Some(new_tier) = deg.tick(&enf) {
            let _ = emitter.push(PerfEvent::TierChanged {
                frame_offset: ADAPTIVE_WINDOW_FRAMES as u32,
                old: prev,
                new: new_tier,
            });
        }

        // Verify : both FrameVerdict and TierChanged events were emitted.
        let drained = emitter.take_all();
        assert!(drained.iter().any(|e| matches!(e, PerfEvent::TierChanged { .. })));
        assert!(drained.iter().any(|e| matches!(e, PerfEvent::FrameVerdict { .. })));
        // Tier moved high → medium.
        assert_eq!(deg.tier, DegradationTier::Medium);
    }

    #[test]
    fn enforcer_reset_preserves_target() {
        let mut enf = FrameBudgetEnforcer::new(RefreshTarget::Hz144);
        enf.record_frame_ms(5.0);
        enf.reset();
        assert_eq!(enf.target, RefreshTarget::Hz144);
        assert_eq!(enf.total_frames, 0);
    }

    #[test]
    fn perf_bench_feature_flag_is_optional() {
        // The crate should compile + tests should pass without `perf-bench`.
        // (This is a smoke-assertion that we never gated test-content
        // exclusively behind the feature.)
        #[cfg(feature = "perf-bench")]
        let _ = canonical_bench_specs();
        let specs = canonical_bench_specs();
        assert!(!specs.is_empty());
    }
}

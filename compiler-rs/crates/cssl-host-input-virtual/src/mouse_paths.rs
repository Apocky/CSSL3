// § T11-W5b-INPUT-VIRTUAL : mouse-path event generators
// ══════════════════════════════════════════════════════════════════
//! Parametric mouse-cursor trajectory generators.
//!
//! Four public generators ; all emit `ReplayEventKind::MouseMove` events
//! sampled at `samples_per_sec` Hz over `duration_ms` (or a fixed sample
//! count for `drag`).  The `drag` generator additionally bookends the
//! Move sequence with `MouseClick` Down/Up events for the given button.
//!
//! ## Generator Inventory
//!
//! - [`circle_path`] : exact parametric circle, deterministic for a given
//!   seed (seed only affects the start-phase to allow ensemble testing).
//! - [`random_walk`] : Gaussian-step walk with per-step variance scaled
//!   by `step_size`.
//! - [`lissajous`] : `(amp_x sin(2π freq_x t + 0), amp_y sin(2π freq_y t + π/2))`
//!   ; classic two-frequency oscillator for visual-regression coverage.
//! - [`drag`] : MouseClick(btn) at start → linear interpolation over
//!   `samples` MouseMove events → MouseClick(btn) at end.  Used to
//!   simulate pointer-drag gestures.
//!
//! ## Determinism
//!
//! Identical args → identical Vec.  RNG-driven generators (`random_walk`,
//! `circle_path` start-phase) thread a `Pcg32` seeded from the `seed`
//! argument and never read process-global entropy.

use cssl_host_replay::{ReplayEvent, ReplayEventKind};

use crate::rng::Pcg32;

/// § Tau = 2π for circle / Lissajous parametrics.
const TAU: f32 = std::f32::consts::TAU;

/// Compute total sample count for a duration-driven generator.
///
/// Returns 0 if either `duration_ms == 0` or `samples_per_sec == 0` —
/// callers treat this as "empty Vec" without further branching.
fn sample_count(duration_ms: u32, samples_per_sec: u32) -> u32 {
    if duration_ms == 0 || samples_per_sec == 0 {
        return 0;
    }
    // (duration_ms / 1000) * samples_per_sec, computed in u64 to avoid overflow.
    ((u64::from(duration_ms) * u64::from(samples_per_sec)) / 1000) as u32
}

/// Generate MouseMove events tracing a circular path.
///
/// `seed` perturbs the starting phase ; `center_xy` + `radius` define the
/// circle ; `duration_ms` × `samples_per_sec` controls density.
#[must_use]
pub fn circle_path(
    seed: u64,
    center_xy: (f32, f32),
    radius: f32,
    duration_ms: u32,
    samples_per_sec: u32,
) -> Vec<ReplayEvent> {
    let n = sample_count(duration_ms, samples_per_sec);
    if n == 0 {
        return Vec::new();
    }
    let mut rng = Pcg32::new(seed);
    let phase0 = rng.range_f32(0.0, TAU);
    let dt_micros = 1_000_000_u64 / u64::from(samples_per_sec);
    let mut events = Vec::with_capacity(n as usize);
    for i in 0..n {
        let t = i as f32 / n as f32;
        let theta = phase0 + TAU * t;
        let x = center_xy.0 + radius * theta.cos();
        let y = center_xy.1 + radius * theta.sin();
        let ts = u64::from(i) * dt_micros;
        events.push(ReplayEvent::new(ts, ReplayEventKind::MouseMove { x, y }));
    }
    events
}

/// Generate MouseMove events along a Gaussian-step random walk.
///
/// At each sample the cursor takes a step `(dx, dy)` where `dx, dy` are
/// independent draws from `N(0, step_size²)`, accumulated from `start_xy`.
#[must_use]
pub fn random_walk(
    seed: u64,
    start_xy: (f32, f32),
    step_size: f32,
    duration_ms: u32,
    samples_per_sec: u32,
) -> Vec<ReplayEvent> {
    let n = sample_count(duration_ms, samples_per_sec);
    if n == 0 {
        return Vec::new();
    }
    let mut rng = Pcg32::new(seed);
    let dt_micros = 1_000_000_u64 / u64::from(samples_per_sec);
    let mut x = start_xy.0;
    let mut y = start_xy.1;
    let mut events = Vec::with_capacity(n as usize);
    for i in 0..n {
        let dx = rng.next_gaussian() * step_size;
        let dy = rng.next_gaussian() * step_size;
        x += dx;
        y += dy;
        let ts = u64::from(i) * dt_micros;
        events.push(ReplayEvent::new(ts, ReplayEventKind::MouseMove { x, y }));
    }
    events
}

/// Generate MouseMove events tracing a Lissajous curve.
///
/// `(x, y) = (cx + ax sin(2π fx t),  cy + ay cos(2π fy t))` over `duration_ms`.
/// Seed is currently unused (Lissajous is fully deterministic from
/// the geometric parameters) but reserved for future jitter-mode.
#[must_use]
pub fn lissajous(
    seed: u64,
    center: (f32, f32),
    amp: (f32, f32),
    freq: (f32, f32),
    duration_ms: u32,
    samples_per_sec: u32,
) -> Vec<ReplayEvent> {
    let _ = seed; // reserved for future phase-jitter mode
    let n = sample_count(duration_ms, samples_per_sec);
    if n == 0 {
        return Vec::new();
    }
    let dt_micros = 1_000_000_u64 / u64::from(samples_per_sec);
    let total_secs = duration_ms as f32 / 1000.0;
    let mut events = Vec::with_capacity(n as usize);
    for i in 0..n {
        let t = (i as f32 / n as f32) * total_secs;
        let x = center.0 + amp.0 * (TAU * freq.0 * t).sin();
        let y = center.1 + amp.1 * (TAU * freq.1 * t).cos();
        let ts = u64::from(i) * dt_micros;
        events.push(ReplayEvent::new(ts, ReplayEventKind::MouseMove { x, y }));
    }
    events
}

/// Generate a drag-gesture event sequence.
///
/// Emits :
/// 1. `MouseClick(btn, start.0, start.1)` at `start_ts_micros`.
/// 2. `samples` × `MouseMove` events linearly interpolated `start → end`.
/// 3. `MouseClick(btn, end.0, end.1)` at the final ts.
///
/// `samples == 0` yields just the two click events (no interpolation).
/// Sample spacing is 16 ms (≈ 60 Hz) — typical mouse-event rate.
#[must_use]
pub fn drag(
    start: (f32, f32),
    end: (f32, f32),
    btn: u8,
    start_ts_micros: u64,
    samples: u32,
) -> Vec<ReplayEvent> {
    let dt_micros: u64 = 16_000;
    let mut events = Vec::with_capacity(samples as usize + 2);
    // Down click at start.
    events.push(ReplayEvent::new(
        start_ts_micros,
        ReplayEventKind::MouseClick {
            btn,
            x: start.0,
            y: start.1,
        },
    ));
    // Interpolation.
    for i in 0..samples {
        let t = if samples == 1 {
            1.0
        } else {
            (i + 1) as f32 / (samples + 1) as f32
        };
        let x = start.0 + (end.0 - start.0) * t;
        let y = start.1 + (end.1 - start.1) * t;
        let ts = start_ts_micros + dt_micros * (i + 1) as u64;
        events.push(ReplayEvent::new(ts, ReplayEventKind::MouseMove { x, y }));
    }
    // Up click at end.
    let ts_end = start_ts_micros + dt_micros * (samples + 1) as u64;
    events.push(ReplayEvent::new(
        ts_end,
        ReplayEventKind::MouseClick {
            btn,
            x: end.0,
            y: end.1,
        },
    ));
    events
}

// ══════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    fn pos(ev: &ReplayEvent) -> (f32, f32) {
        if let ReplayEventKind::MouseMove { x, y } = ev.kind {
            (x, y)
        } else {
            panic!("expected MouseMove")
        }
    }

    /// § Same seed + params → bit-identical circle path.
    #[test]
    fn circle_deterministic() {
        let a = circle_path(7, (100.0, 100.0), 50.0, 1000, 60);
        let b = circle_path(7, (100.0, 100.0), 50.0, 1000, 60);
        assert_eq!(a, b);
        // Each sample should lie ≈ 50 from center (within float epsilon).
        for ev in &a {
            let (x, y) = pos(ev);
            let r = ((x - 100.0).powi(2) + (y - 100.0).powi(2)).sqrt();
            assert!((r - 50.0).abs() < 1e-3, "off-circle radius: {r}");
        }
    }

    /// § Random-walk samples stay finite for a reasonable duration.
    ///
    /// Gaussian-walk variance grows with step count ; for 1s @ 60Hz @
    /// step_size=2 the cursor wanders ≲ ±50 from start with very high
    /// probability — bound is permissive (±2000) just to catch NaN/inf.
    #[test]
    fn walk_bounded() {
        let evs = random_walk(123, (0.0, 0.0), 2.0, 1_000, 60);
        assert!(!evs.is_empty());
        for ev in &evs {
            let (x, y) = pos(ev);
            assert!(x.is_finite() && y.is_finite(), "non-finite walk position");
            assert!(x.abs() < 2000.0 && y.abs() < 2000.0, "walk drifted: ({x},{y})");
        }
    }

    /// § Lissajous covers both x and y excursions (non-degenerate shape).
    #[test]
    fn lissajous_shape() {
        let evs = lissajous(0, (0.0, 0.0), (10.0, 7.0), (1.0, 2.0), 2_000, 100);
        assert!(!evs.is_empty());
        let (mut max_x, mut max_y) = (0.0_f32, 0.0_f32);
        for ev in &evs {
            let (x, y) = pos(ev);
            max_x = max_x.max(x.abs());
            max_y = max_y.max(y.abs());
        }
        // Should reach near amplitude on both axes.
        assert!(max_x > 9.0, "x amplitude not reached: {max_x}");
        assert!(max_y > 6.0, "y amplitude not reached: {max_y}");
    }

    /// § Drag emits exactly samples + 2 events.
    #[test]
    fn drag_event_count() {
        let evs = drag((0.0, 0.0), (100.0, 100.0), 0, 5_000, 8);
        assert_eq!(evs.len(), 10, "8 samples + 2 click events = 10");
        // First + last are MouseClick.
        assert!(matches!(evs[0].kind, ReplayEventKind::MouseClick { btn: 0, .. }));
        assert!(matches!(evs[9].kind, ReplayEventKind::MouseClick { btn: 0, .. }));
        // Middle is MouseMove.
        assert!(matches!(evs[1].kind, ReplayEventKind::MouseMove { .. }));
    }

    /// § All duration-positive paths return non-empty Vecs.
    #[test]
    fn all_paths_return_non_empty_when_duration_positive() {
        assert!(!circle_path(0, (0.0, 0.0), 10.0, 100, 60).is_empty());
        assert!(!random_walk(0, (0.0, 0.0), 1.0, 100, 60).is_empty());
        assert!(!lissajous(0, (0.0, 0.0), (1.0, 1.0), (1.0, 1.0), 100, 60).is_empty());
        // Drag with samples=0 still emits the 2 click events.
        assert_eq!(drag((0.0, 0.0), (1.0, 1.0), 0, 0, 0).len(), 2);
        // Zero duration → empty.
        assert!(circle_path(0, (0.0, 0.0), 10.0, 0, 60).is_empty());
    }
}

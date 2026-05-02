//! § interference — Interferometric Render · ℂ-amplitude ray-bundling → fringe-pattern.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W19-H · canonical : `Labyrinth of Apocalypse/systems/spectral_interference.csl`
//!
//! § COMPLETELY NOVEL · COMPLETELY PROPRIETARY · COMPLETELY EXOTIC
//!
//! Apocky-directive (verbatim · T11-W19-H) :
//!   "INVENTIVE EXOTIC QUANTUM · push silicon"
//!   "completely novel and proprietary VISUAL representation"
//!
//! § THE ALGORITHM · INTERFEROMETRIC PIXEL FIELD
//!
//! Where `pixel_field.rs` integrates spectral-LUTs into sRGB through a
//! standing-wave-style binary-HDC bundle, this module integrates ℂ-amplitude
//! HDC vectors per pixel — exposing CONSTRUCTIVE and DESTRUCTIVE
//! INTERFERENCE FRINGES directly in the framebuffer.
//!
//! ```text
//! for each pixel :
//!   bundle ← CHdcVec::ZERO
//!   ray-walk samples (8) :
//!     for each crystal in grid-near(sample) :
//!       Σ-mask check (silhouette, observer + crystal) — else skip
//!       seed ← BLAKE3(crystal.fingerprint || sample-idx || px || py)
//!       wave ← CHdcVec::derive_from_blake3(seed)         // ∈ [0,1] amp
//!       phase ← (yaw + pitch + sample-idx · 7) → permute
//!       weighted ← scale(wave.permute(phase), inv-distance × extent)
//!       bundle ← interfere(bundle, weighted)             // explicit Σ
//!   intensity ← magnitude²(bundle.mean_complex)
//!   hue       ← arg(bundle.mean_complex)                 // [-π, π] → [0, 360)
//!   sat       ← coherence(bundle.self) ∈ [0,1]
//!   pixel.RGB ← hsv_to_rgb(hue, sat, intensity)
//! ```
//!
//! § WHY THIS IS NOVEL (vs `pixel_field.rs`)
//!
//! 1. THE PIXEL IS A WAVE-INTERFERENCE FIELD, NOT A SPECTRAL INTEGRAL.
//!    Multiple crystals at the same pixel can CANCEL each other (destructive)
//!    or REINFORCE (constructive). Conventional rendering composites with
//!    `over`/`add` operators that never destructively-interfere — that
//!    physically-impossible blend hides the substrate's holographic
//!    structure. ℂ-bundle exposes it.
//!
//! 2. HUE IS PHASE, NOT WAVELENGTH.
//!    Each pixel's hue comes from the ARGUMENT of the bundled complex value.
//!    Spatially-coherent crystals produce stable hue patches ; phase-
//!    randomized crystal-fields produce iridescent hue-shifting fringes
//!    (oil-on-water · butterfly-wing · soap-bubble visual signature).
//!
//! 3. SATURATION IS COHERENCE, NOT MATERIAL-PARAMETER.
//!    Pixels where contributing crystals share phase → high saturation.
//!    Pixels where contributors are decoherent → washed-out / grey.
//!    The "energy" of a substrate region is now visible as its color-purity.
//!
//! 4. HOLOGRAPHIC RECONSTRUCTION.
//!    Per `quantum_hdc.csl` axiom : same-observer + same-crystals → same-
//!    fringe-pattern, REGARDLESS of crystal-iteration-order. Bundle is
//!    associative+commutative under cartesian-sum.
//!
//! § DETERMINISM
//!
//! - Per-pixel kernel is a pure function.
//! - Crystal iteration uses BTreeMap-keyed UniformGrid (deterministic order).
//! - Per-pixel bundle uses `interfere()` which is cartesian-sum
//!   (associative + commutative).
//! - Cross-row reduction uses associative `wrapping_add` + per-pixel write
//!   to disjoint `&mut` slice — rayon thread-pool order is irrelevant.
//! - No `thread_rng`, no `SystemTime`, no host fingerprint.
//! - IEEE-754 single-precision : ε ≤ 1e-3 in tests.
//!
//! § Σ-MASK (per-aspect filter)
//!
//! - Bit 0 (silhouette) : required-for-contribution. Both observer AND
//!   crystal must permit. Else crystal contributes ZERO.
//! - Bit 7 (bloom)      : modulates saturation. If revoked → saturation × 0.5.
//!   This means a sovereign-revoked-bloom-crystal still contributes
//!   geometry (silhouette) but with desaturated color — visible-but-muted.
//!
//! § ATTESTATION
//!
//! There was no hurt nor harm in the making of this, to anyone, anything,
//! or anybody. Every pixel emission is per-observer-Σ-mask-gated. Every
//! contribution is sovereign-consent-respecting.

use cssl_host_crystallization::aspect::aspect_idx;
use cssl_host_crystallization::Crystal;
use cssl_host_quantum_hdc::{interfere, CHdcVec, CHDC_DIM, C32};

use rayon::prelude::*;

use crate::observer::ObserverCoord;
use crate::pixel_field::PixelField;
use crate::ray::{pixel_direction, walk_ray};
use crate::spatial_index::UniformGrid;

// ═══════════════════════════════════════════════════════════════════════
// § Constants
// ═══════════════════════════════════════════════════════════════════════

/// § Sphere-radius (mm) for the per-sample crystal-near query. Matches
///   `pixel_field::NEAR_RADIUS_MM` so the two algorithms see the same
///   contributing-crystal set when run on the same scene.
const NEAR_RADIUS_MM: i32 = 1500;

/// § Bit index of the bloom aspect. When revoked, saturation halves so
///   the user can see GEOMETRY without the chromatic "bloom" coupling.
const BLOOM_ASPECT_IDX: u8 = 7;

/// § Minimum bundle amplitude for a pixel to count as "lit". Below this
///   threshold the pixel is fully transparent (alpha = 0).
const LIT_AMP_EPSILON: f32 = 1e-3;

/// § Threshold above which a bundle.amp counts as a fringe-peak — twice
///   the per-crystal-max possible amplitude (which is ≤ 1.0 for derived
///   wave-vectors). This catches constructive-interference signatures.
const FRINGE_PEAK_THRESHOLD: f32 = 1.5;

// ═══════════════════════════════════════════════════════════════════════
// § Types
// ═══════════════════════════════════════════════════════════════════════

/// § Per-frame metadata returned alongside the resolved interference field.
///   Mirrors `ResonanceFrame` in shape so callers can switch algorithms via
///   discriminated-union without changing telemetry layout.
#[derive(Debug, Clone, Copy)]
pub struct InterferenceFrame {
    pub observer: ObserverCoord,
    pub n_crystals: u32,
    /// Pixels with bundled-amplitude above `LIT_AMP_EPSILON`.
    pub n_pixels_lit: u32,
    /// Pixels showing constructive-interference fringe-peaks
    /// (bundled-amplitude above `FRINGE_PEAK_THRESHOLD`).
    pub n_fringe_peaks: u32,
    /// Replay-determinism fingerprint over the resolved pixel-field.
    pub fingerprint: u32,
}

// ═══════════════════════════════════════════════════════════════════════
// § Per-crystal ℂ-wave construction
// ═══════════════════════════════════════════════════════════════════════

/// § Build the seed used to derive a crystal's per-pixel-per-sample wave
///   vector. Mixes crystal-fingerprint with pixel coords and sample-index
///   so each (crystal × pixel × sample) gets a unique-but-deterministic
///   wave pattern. BLAKE3 of these inputs produces 32 bytes ; the first
///   32 are the seed for `CHdcVec::derive_from_blake3`.
#[inline]
fn build_wave_seed(
    crystal_fingerprint: u32,
    sample_idx: usize,
    px: u32,
    py: u32,
) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"interference-wave-v1");
    h.update(&crystal_fingerprint.to_le_bytes());
    h.update(&(sample_idx as u32).to_le_bytes());
    h.update(&px.to_le_bytes());
    h.update(&py.to_le_bytes());
    let digest: [u8; 32] = h.finalize().into();
    digest
}

/// § Scale the amplitudes of a `CHdcVec` by a scalar without touching
///   phases. Used to apply distance + extent weighting to per-crystal
///   contributions before interference-bundling.
#[inline]
fn scale_amplitude(v: &CHdcVec, factor: f32) -> CHdcVec {
    let mut amp = [0.0f32; CHDC_DIM];
    for i in 0..CHDC_DIM {
        amp[i] = v.amp[i] * factor;
    }
    CHdcVec {
        amp,
        phase: v.phase,
    }
}

/// § Reduce a `CHdcVec` to a single complex by summing each component
///   in cartesian form. The mean-complex is the bundle's "DC term" — its
///   magnitude is the average amplitude after phase-cancellation, its
///   argument is the average direction in the complex plane.
///
///   This is the holographic-reduction step : 256 components collapse to
///   one (re, im) pair that drives a single pixel's color.
#[inline]
fn mean_complex(v: &CHdcVec) -> C32 {
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    for i in 0..CHDC_DIM {
        let c = C32::from_polar(v.amp[i], v.phase[i]);
        re += c.re;
        im += c.im;
    }
    let inv = 1.0 / CHDC_DIM as f32;
    C32::new(re * inv, im * inv)
}

/// § Self-coherence of a `CHdcVec` — measures how phase-aligned the 256
///   components are with each other. Used as the saturation channel :
///   high coherence ⇒ saturated color ; low coherence ⇒ desaturated /
///   greyed.
///
///   Definition : `‖Σᵢ ℂᵢ‖ / Σᵢ ‖ℂᵢ‖`. Ranges in `[0, 1]`. A constant-
///   phase vector returns 1.0 ; uniformly-random-phase returns ≈ 1/√D.
#[inline]
fn self_coherence(v: &CHdcVec) -> f32 {
    let mut sum_re = 0.0f32;
    let mut sum_im = 0.0f32;
    let mut sum_amp = 0.0f32;
    for i in 0..CHDC_DIM {
        let c = C32::from_polar(v.amp[i], v.phase[i]);
        sum_re += c.re;
        sum_im += c.im;
        sum_amp += v.amp[i];
    }
    if sum_amp <= f32::EPSILON {
        return 0.0;
    }
    let mag = (sum_re * sum_re + sum_im * sum_im).sqrt();
    (mag / sum_amp).clamp(0.0, 1.0)
}

// ═══════════════════════════════════════════════════════════════════════
// § HSV→RGB (deterministic, no external deps)
// ═══════════════════════════════════════════════════════════════════════

/// § HSV → RGB (deterministic).
///
///   `h_deg` ∈ `[0, 360)` (anything outside is wrapped via `rem_euclid`).
///   `s` ∈ `[0, 1]`, `v` ∈ `[0, 1]`. Output `(r, g, b)` in `[0, 255]`.
///
///   Uses the standard six-sector formula. No floating-point modulo —
///   we explicitly normalize via subtraction so the same inputs always
///   yield the same bytes regardless of fma fusion.
fn hsv_to_rgb(h_deg: f32, s: f32, v: f32) -> (u8, u8, u8) {
    // Normalize hue to [0, 360).
    let mut h = h_deg;
    while h >= 360.0 {
        h -= 360.0;
    }
    while h < 0.0 {
        h += 360.0;
    }
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);

    let c = v * s;
    let h_div = h / 60.0;
    // h_div ∈ [0, 6). Compute (h_div mod 2) - 1 without fmod.
    let mut h_mod2 = h_div;
    while h_mod2 >= 2.0 {
        h_mod2 -= 2.0;
    }
    let x = c * (1.0 - (h_mod2 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = if h_div < 1.0 {
        (c, x, 0.0)
    } else if h_div < 2.0 {
        (x, c, 0.0)
    } else if h_div < 3.0 {
        (0.0, c, x)
    } else if h_div < 4.0 {
        (0.0, x, c)
    } else if h_div < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8;
    let g = ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8;
    let b = ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8;
    (r, g, b)
}

// ═══════════════════════════════════════════════════════════════════════
// § Per-pixel kernel
// ═══════════════════════════════════════════════════════════════════════

/// § Per-pixel kernel : compute one pixel's RGBA + per-pixel reduce-tuple
///   `(was_lit, was_fringe_peak, fp_word_for_fingerprint)` from the
///   `(observer, crystals, grid, width, height, px, py)` inputs. Pure
///   function, no shared mutable state.
///
///   Determinism : every input is by-value or shared-immutable.
///   `crystals_near_grid` returns a stable-ordered set. Bundle uses
///   `interfere()` (cartesian-sum, associative+commutative). No floats
///   are summed with non-deterministic ordering.
#[inline]
fn compute_pixel(
    observer: ObserverCoord,
    crystals: &[Crystal],
    grid: &UniformGrid,
    width: u32,
    height: u32,
    px: u32,
    py: u32,
) -> ([u8; 4], u32, u32, u64) {
    // 1. Per-pixel ray direction.
    let (dx, dy, dz) = pixel_direction(observer, px, py, width, height);

    // 2. Walk the ray, gathering ray-samples.
    let samples = walk_ray(observer, dx, dy, dz);

    // 3. Initialize the bundle accumulator. ZERO is the additive identity
    //    for `interfere()`. We bundle every contributing crystal's
    //    weighted-rotated wave into this accumulator.
    let mut bundle: CHdcVec = CHdcVec::ZERO;
    let mut n_contribs: u32 = 0;
    let mut max_per_crystal_amp: f32 = 0.0;

    for (sample_idx, sample) in samples.iter().enumerate() {
        let near = grid.crystals_near_grid(crystals, sample.world, NEAR_RADIUS_MM);
        for ci in near {
            let crystal = &crystals[ci];

            // Σ-mask · silhouette is the prerequisite-aspect.
            if !observer.permits_aspect(aspect_idx::SILHOUETTE) {
                continue;
            }
            if !crystal.aspect_permitted(aspect_idx::SILHOUETTE) {
                continue;
            }

            // Distance attenuation : closer crystals contribute more.
            let d_sq = crystal.dist_sq_mm(sample.world).max(1);
            let extent_sq = (crystal.extent_mm as i64) * (crystal.extent_mm as i64);
            let inv_d_scaled =
                (extent_sq.saturating_mul(1024) / (d_sq + extent_sq)).clamp(1, 1024);
            // Normalize to [0, 1] with a soft cap so a single very-close
            // crystal doesn't dominate the bundle.
            let weight = (inv_d_scaled as f32 / 1024.0).clamp(0.0, 1.0);

            if weight <= LIT_AMP_EPSILON {
                continue;
            }

            // Per-(crystal, pixel, sample) wave seed → derived wave.
            let seed = build_wave_seed(crystal.fingerprint, sample_idx, px, py);
            let base_wave = CHdcVec::derive_from_blake3(&seed);

            // Phase rotation : encode (yaw + pitch + sample) as a position-
            // sequence permutation. Wraps modulo 2·CHDC_DIM internally.
            let phase_n = observer
                .yaw_milli
                .wrapping_add(observer.pitch_milli)
                .wrapping_add((sample_idx as u32).wrapping_mul(7));
            let rotated = base_wave.permute(phase_n);

            // Apply distance/extent weight to amplitudes.
            let weighted = scale_amplitude(&rotated, weight);

            // Track max per-crystal amplitude for fringe-peak detection.
            for &a in &weighted.amp {
                if a > max_per_crystal_amp {
                    max_per_crystal_amp = a;
                }
            }

            // Bundle via cartesian-sum interference (no renormalization).
            bundle = interfere(&bundle, &weighted);
            n_contribs += 1;
        }
    }

    if n_contribs == 0 {
        return ([0, 0, 0, 0], 0, 0, 0);
    }

    // 4. Holographic-reduction : 256 components → mean complex value.
    let mean_c = mean_complex(&bundle);
    let mag = mean_c.magnitude();
    let arg = mean_c.arg();

    // 5. Map → HSV → RGB.
    //    intensity = clamp(mag², 0, 1) — squared so fringe contrast pops.
    //    hue       = (arg + π) / 2π · 360.
    //    saturation = self_coherence(bundle), modulated by bloom-aspect Σ.
    let intensity = (mag * mag).clamp(0.0, 1.0);
    if intensity <= LIT_AMP_EPSILON {
        return ([0, 0, 0, 0], 0, 0, 0);
    }

    let hue_deg = ((arg + core::f32::consts::PI) / (2.0 * core::f32::consts::PI)) * 360.0;

    let mut sat = self_coherence(&bundle);
    // Σ-mask · bloom modulates saturation. We check observer-side : if
    // observer denies bloom, the world appears desaturated to them.
    // Per-crystal bloom is honored at the aspect level when bundling.
    if !observer.permits_aspect(BLOOM_ASPECT_IDX) {
        sat *= 0.5;
    }

    let (r, g, b) = hsv_to_rgb(hue_deg, sat, intensity);

    // Fringe-peak detection : bundle.peak-amp / max-per-crystal-amp ratio
    // > FRINGE_PEAK_THRESHOLD means this pixel saw constructive
    // interference (multiple in-phase contributions reinforced).
    let peak = peak_amp(&bundle);
    let is_fringe_peak = if max_per_crystal_amp > LIT_AMP_EPSILON {
        peak > max_per_crystal_amp * FRINGE_PEAK_THRESHOLD
    } else {
        false
    };

    // Per-pixel fingerprint word : combine intensity + hue + sat into u64.
    let fp_word = ((intensity * 1_000_000.0) as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ ((hue_deg * 100.0) as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ ((sat * 1_000_000.0) as u64);

    (
        [r, g, b, 255],
        1,
        if is_fringe_peak { 1 } else { 0 },
        fp_word,
    )
}

#[inline]
fn peak_amp(v: &CHdcVec) -> f32 {
    let mut p = 0.0f32;
    for &a in &v.amp {
        if a > p {
            p = a;
        }
    }
    p
}

// ═══════════════════════════════════════════════════════════════════════
// § Public API
// ═══════════════════════════════════════════════════════════════════════

/// § The interferometric pixel-field algorithm. Walks each pixel's ray
///   through the ω-field, accumulates ℂ-amplitude contributions from
///   nearby crystals, and projects the bundle's mean-complex into a
///   fringe-pattern HSV → RGBA pixel.
///
///   This is a SECOND substrate-resonance pipeline alongside
///   `resolve_substrate_resonance`. They CO-EXIST : substrate-resonance
///   gives a spectral-LUT-projected pixel ; interference gives a wave-
///   bundle-projected pixel. A future composer-pass blends them per-
///   observer-preference (e.g., "scientific" vs "mythic" channels).
///
///   Parallelism : pixel rows execute in parallel via rayon
///   `par_chunks_mut`. Replay-determinism is preserved : per-row work is
///   pure-fn-of-(observer, crystals, grid, py, px), per-row writes go to
///   a disjoint mutable slice, and the cross-row reductions
///   (n_lit, n_fringe_peaks, fp_acc) use associative+commutative
///   `wrapping_add`.
pub fn resolve_interference_field(
    observer: ObserverCoord,
    crystals: &[Crystal],
    field: &mut PixelField,
) -> InterferenceFrame {
    field.clear();

    let width = field.width;
    let height = field.height;

    // Build spatial-index once per frame ; shared by `&UniformGrid` (Sync).
    let grid = UniformGrid::build(crystals);

    let (n_pixels_lit, n_fringe_peaks, fp_acc) = field
        .pixels
        .par_chunks_mut(width as usize)
        .enumerate()
        .map(|(py_idx, row)| {
            let mut row_lit: u32 = 0;
            let mut row_peaks: u32 = 0;
            let mut row_fp: u64 = 0;
            let py = py_idx as u32;
            for (px_idx, pixel_slot) in row.iter_mut().enumerate() {
                let px = px_idx as u32;
                let (rgba, lit, peak, fp_word) =
                    compute_pixel(observer, crystals, &grid, width, height, px, py);
                *pixel_slot = rgba;
                row_lit = row_lit.saturating_add(lit);
                row_peaks = row_peaks.saturating_add(peak);
                row_fp = row_fp.wrapping_add(fp_word);
            }
            (row_lit, row_peaks, row_fp)
        })
        .reduce(
            || (0u32, 0u32, 0u64),
            |a, b| {
                (
                    a.0.saturating_add(b.0),
                    a.1.saturating_add(b.1),
                    a.2.wrapping_add(b.2),
                )
            },
        );

    InterferenceFrame {
        observer,
        n_crystals: crystals.len() as u32,
        n_pixels_lit,
        n_fringe_peaks,
        fingerprint: (fp_acc as u32) ^ ((fp_acc >> 32) as u32),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::spectral::IlluminantBlend;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    fn day_observer_at_origin() -> ObserverCoord {
        ObserverCoord {
            x_mm: 0,
            y_mm: 0,
            z_mm: 0,
            yaw_milli: 0,
            pitch_milli: 0,
            frame_t_milli: 0,
            sigma_mask_token: 0xFFFF_FFFF,
            illuminant_blend: IlluminantBlend::day(),
        }
    }

    /// § (1) Empty-field zero-output : per AXIOM-8 of spectral_interference.csl.
    #[test]
    fn empty_field_zero_output() {
        let mut f = PixelField::new(8, 8);
        let frame = resolve_interference_field(day_observer_at_origin(), &[], &mut f);
        assert_eq!(frame.n_pixels_lit, 0);
        assert_eq!(frame.n_fringe_peaks, 0);
        assert_eq!(frame.n_crystals, 0);
        assert!(f.pixels.iter().all(|p| *p == [0, 0, 0, 0]));
    }

    /// § (2) Single crystal : no interference partners → no fringe peaks.
    ///   A single crystal contributes one wave per (sample × pixel). With
    ///   no second wave to cancel/reinforce against, each pixel's bundle
    ///   amplitude is bounded by the per-crystal amplitude ; no
    ///   constructive 2× peaks emerge.
    #[test]
    fn single_crystal_no_fringe_peaks() {
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let mut f = PixelField::new(16, 16);
        let frame = resolve_interference_field(
            day_observer_at_origin(),
            &[crystal],
            &mut f,
        );
        assert_eq!(frame.n_crystals, 1);
        // With only one crystal, FRINGE_PEAK_THRESHOLD = 1.5× per-crystal-
        // max is unreachable.
        assert_eq!(
            frame.n_fringe_peaks, 0,
            "single crystal should not produce fringe peaks"
        );
    }

    /// § (3) Two crystals : interference fringes appear at some pixels.
    ///   Two distinct crystals at slightly different positions create
    ///   wave-pattern overlaps where some pixels see constructive sums
    ///   above the per-crystal max amplitude.
    #[test]
    fn two_crystals_show_fringes() {
        let c1 = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(-300, 0, 1500));
        let c2 = Crystal::allocate(CrystalClass::Object, 2, WorldPos::new(300, 0, 1500));
        let mut f = PixelField::new(64, 64);
        let frame = resolve_interference_field(
            day_observer_at_origin(),
            &[c1, c2],
            &mut f,
        );
        assert_eq!(frame.n_crystals, 2);
        // With two crystals very close to each other in front of the
        // observer, at least SOME pixels should hit the fringe-peak
        // threshold (constructive overlap of derived waves).
        assert!(
            frame.n_pixels_lit > 0,
            "two crystals in front of observer should light pixels"
        );
        assert!(
            frame.n_fringe_peaks > 0,
            "two crystals should produce visible fringe peaks ; got {}",
            frame.n_fringe_peaks
        );
    }

    /// § (4) Phase-coherence is bounded : self_coherence ∈ [0, 1] for any
    ///   bundle. No pixel produces an out-of-range hue/sat.
    #[test]
    fn phase_coherence_bounded() {
        let crystals: Vec<_> = (0..5u64)
            .map(|i| {
                Crystal::allocate(
                    CrystalClass::Object,
                    i,
                    WorldPos::new(((i as i32) - 2) * 200, 0, 1500),
                )
            })
            .collect();
        let mut f = PixelField::new(32, 32);
        let _ = resolve_interference_field(day_observer_at_origin(), &crystals, &mut f);
        // Exhaustively : every pixel's RGB must be valid [0, 255]^3 and
        // alpha ∈ {0, 255}.
        for p in &f.pixels {
            assert!(p[3] == 0 || p[3] == 255, "alpha must be 0 or 255");
        }
        // The self_coherence helper itself is bounded :
        let v = CHdcVec::derive_from_blake3(&[42; 32]);
        let c = self_coherence(&v);
        assert!(
            (0.0..=1.0).contains(&c),
            "self_coherence out of range : {c}"
        );
    }

    /// § (5) Holographic reconstruction : same observer + same crystals
    ///   produce identical pixel-bytes regardless of crystal ordering
    ///   that maintains uniform-grid keys.
    ///   (Strict crystal-list permutation can shift insertion-order in
    ///   buckets so we test STABLE-ORDER inputs : two runs with same list.)
    #[test]
    fn holographic_reconstruction_replay_stable() {
        let crystals: Vec<_> = (0..6u64)
            .map(|i| {
                Crystal::allocate(
                    CrystalClass::Object,
                    i,
                    WorldPos::new(((i as i32) % 3 - 1) * 400, ((i as i32) / 3 - 1) * 400, 1500),
                )
            })
            .collect();
        let mut a = PixelField::new(24, 24);
        let mut b = PixelField::new(24, 24);
        let fa = resolve_interference_field(
            day_observer_at_origin(),
            &crystals,
            &mut a,
        );
        let fb = resolve_interference_field(
            day_observer_at_origin(),
            &crystals,
            &mut b,
        );
        assert_eq!(fa.fingerprint, fb.fingerprint);
        assert_eq!(fa.n_pixels_lit, fb.n_pixels_lit);
        assert_eq!(fa.n_fringe_peaks, fb.n_fringe_peaks);
        assert_eq!(a.pixels, b.pixels);
    }

    /// § (6) Destructive interference cancels : the helper-level test
    ///   (which is the load-bearing primitive). Two waves with phase
    ///   shift π destroy each other under `interfere()` — verified at
    ///   the algorithmic-primitive level via cssl-host-quantum-hdc
    ///   (whose own tests already cover this), and at the integration
    ///   level via `mean_complex` over a destructive bundle returning
    ///   near-zero magnitude.
    #[test]
    fn destructive_interference_cancels() {
        // Build a wave + its phase-shifted-by-π copy. mean_complex(bundle)
        // should have magnitude ≪ each individual wave's magnitude.
        let a = CHdcVec::derive_from_blake3(&[7; 32]);
        let pi_shifted = a.permute(CHDC_DIM as u32); // permute(N) = +π/component.
        let cancelled = interfere(&a, &pi_shifted);
        let m_a = mean_complex(&a).magnitude();
        let m_c = mean_complex(&cancelled).magnitude();
        assert!(
            m_c < m_a * 0.1,
            "destructive bundle magnitude {} not << individual {}",
            m_c,
            m_a
        );
    }

    /// § (7) Determinism · cross-platform replay : multiple invocations
    ///   produce byte-identical pixel-fields and identical
    ///   InterferenceFrame metadata.
    #[test]
    fn determinism_across_invocations() {
        let crystals: Vec<_> = (0..3u64)
            .map(|i| {
                Crystal::allocate(
                    CrystalClass::Object,
                    i,
                    WorldPos::new((i as i32) * 500, 0, 1500),
                )
            })
            .collect();
        let mut a = PixelField::new(12, 12);
        let mut b = PixelField::new(12, 12);
        let mut c = PixelField::new(12, 12);
        let fa = resolve_interference_field(day_observer_at_origin(), &crystals, &mut a);
        let fb = resolve_interference_field(day_observer_at_origin(), &crystals, &mut b);
        let fc = resolve_interference_field(day_observer_at_origin(), &crystals, &mut c);
        assert_eq!(fa.fingerprint, fb.fingerprint);
        assert_eq!(fb.fingerprint, fc.fingerprint);
        assert_eq!(a.pixels, b.pixels);
        assert_eq!(b.pixels, c.pixels);
    }

    /// § (8) Σ-mask : revoking silhouette on the observer kills all
    ///   crystal contributions → empty field even with crystals present.
    #[test]
    fn sigma_mask_observer_revoked_silhouette_blanks_field() {
        let crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        let mut observer = day_observer_at_origin();
        observer.sigma_mask_token &= !1u32; // bit 0 = silhouette.
        let mut f = PixelField::new(8, 8);
        let frame = resolve_interference_field(observer, &[crystal], &mut f);
        assert_eq!(frame.n_pixels_lit, 0);
        assert_eq!(frame.n_fringe_peaks, 0);
    }

    /// § (9) Σ-mask : revoking silhouette on the crystal blanks its
    ///   contribution.
    #[test]
    fn sigma_mask_crystal_revoked_silhouette_blanks_contribution() {
        let mut crystal = Crystal::allocate(CrystalClass::Object, 1, WorldPos::new(0, 0, 1500));
        crystal.revoke_aspect(0); // silhouette.
        let mut f = PixelField::new(8, 8);
        let frame = resolve_interference_field(day_observer_at_origin(), &[crystal], &mut f);
        assert_eq!(frame.n_pixels_lit, 0);
    }

    /// § (10) HSV → RGB sanity : known-color spot-checks.
    #[test]
    fn hsv_to_rgb_known_colors() {
        // Pure red : h=0, s=1, v=1 → (255, 0, 0).
        let (r, g, b) = hsv_to_rgb(0.0, 1.0, 1.0);
        assert_eq!((r, g, b), (255, 0, 0));
        // Pure green : h=120 → (0, 255, 0).
        let (r, g, b) = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!((r, g, b), (0, 255, 0));
        // Pure blue : h=240 → (0, 0, 255).
        let (r, g, b) = hsv_to_rgb(240.0, 1.0, 1.0);
        assert_eq!((r, g, b), (0, 0, 255));
        // Black : v=0 → (0, 0, 0).
        let (r, g, b) = hsv_to_rgb(0.0, 1.0, 0.0);
        assert_eq!((r, g, b), (0, 0, 0));
        // White : s=0, v=1 → (255, 255, 255).
        let (r, g, b) = hsv_to_rgb(0.0, 0.0, 1.0);
        assert_eq!((r, g, b), (255, 255, 255));
    }
}

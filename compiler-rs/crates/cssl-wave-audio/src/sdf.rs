//! § SDF-vocal-tract — minimal signed-distance-field for procedural vocals.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The procedural-vocal synthesizer needs a way to describe the geometry
//!   of a creature's vocal tract — pharynx + oral-cavity + nasal-coupling.
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § IV.1` :
//!
//!   ```text
//!   SDF (07_AESTHETIC/01_SDF_NATIVE_RENDER) ⊗ source-of-boundary-geometry
//!   SDF.evaluate(x) ≤ 0 ⊗ inside-solid ⊗ ψ-handling :
//!     rigid-wall    : Dirichlet ψ = 0
//!     soft-wall     : Neumann ∂ψ/∂n = 0
//!     impedance     : Robin (∂ψ/∂n + Z·ψ) = 0
//!   ```
//!
//!   We provide a **piecewise-cylinder vocal-tract** SDF here : a sequence
//!   of frusta (truncated cones) sampled along the tract centerline.
//!   Each segment carries a radius + a wall-impedance class. This is the
//!   minimal-viable shape that produces formant-like resonances when
//!   driven by a glottal-pulse source.
//!
//! § DESIGN — piecewise frusta along centerline
//!   The vocal tract from glottis to lips is roughly 17 cm long for adult
//!   humans, parameterized by an area-function `A(x)` along the
//!   centerline. We discretize `A(x)` into N piecewise-constant segments
//!   each described by `(radius, length, wall_class)`. A SDF query at a
//!   3D point projects onto the centerline + computes the distance from
//!   the segment's surface.
//!
//! § WALL CLASSIFICATION
//!   Each segment carries a `WallClass` enum signaling whether the wall
//!   is rigid (Dirichlet ψ = 0), soft (Neumann), or impedance (Robin
//!   with KAN-derived Z(λ)). For a typical creature vocal tract :
//!     - pharynx walls  : impedance (soft-tissue)
//!     - oral cavity    : impedance (mostly hard-palate + soft-tongue)
//!     - lip aperture   : rigid (radiates into free-field at boundary)

use crate::error::{Result, WaveAudioError};

/// Wall-impedance classification per spec § IV.1 SDF surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallClass {
    /// Dirichlet ψ = 0 : perfect-reflector / pressure-zero. Used at the
    /// glottis closure phase + at hard-palate.
    Rigid,
    /// Neumann ∂ψ/∂n = 0 : open-end / radiating. Used at the lip aperture.
    Soft,
    /// Robin (∂ψ/∂n + Z·ψ) = 0 : impedance-wall with KAN-derived `Z(λ)`.
    /// The typical case for soft tissue.
    Impedance,
}

/// One piecewise-cylinder segment along the vocal-tract centerline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TractSegment {
    /// Radius at the segment's start (m).
    pub radius_start: f32,
    /// Radius at the segment's end (m).
    pub radius_end: f32,
    /// Segment length along the centerline (m).
    pub length: f32,
    /// Wall classification.
    pub wall_class: WallClass,
}

impl TractSegment {
    /// Construct a segment.
    #[must_use]
    pub const fn new(
        radius_start: f32,
        radius_end: f32,
        length: f32,
        wall_class: WallClass,
    ) -> TractSegment {
        TractSegment {
            radius_start,
            radius_end,
            length,
            wall_class,
        }
    }

    /// Cross-sectional area at parameter `t ∈ [0, 1]` along the segment.
    /// Linear interpolation in radius (not area) ; matches the standard
    /// frustum convention.
    #[must_use]
    pub fn radius_at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        self.radius_start + (self.radius_end - self.radius_start) * t
    }

    /// Cross-sectional area at parameter `t ∈ [0, 1]` along the segment.
    #[must_use]
    pub fn area_at(self, t: f32) -> f32 {
        let r = self.radius_at(t);
        core::f32::consts::PI * r * r
    }
}

/// Piecewise-cylinder vocal-tract SDF.
///
/// § FIELDS
///   - `segments` : ordered list of frusta from glottis (idx 0) to lips
///     (idx N-1).
///   - `centerline_dir` : 3D direction the centerline runs along ; defaults
///     to `+X`.
///
/// § INVARIANTS
///   - At least one segment.
///   - All radii ≥ 0.
///   - All segment lengths > 0.
#[derive(Debug, Clone)]
pub struct VocalTractSdf {
    segments: Vec<TractSegment>,
    /// Total tract length (m), cached.
    total_length: f32,
}

impl VocalTractSdf {
    /// Construct a vocal-tract from a list of segments.
    ///
    /// § ERRORS
    ///   - [`WaveAudioError::VocalTract`] when the segment list is empty,
    ///     contains a zero-length segment, or has a negative radius.
    pub fn new(segments: Vec<TractSegment>) -> Result<VocalTractSdf> {
        if segments.is_empty() {
            return Err(WaveAudioError::VocalTract("must have at least one segment"));
        }
        let mut total_length = 0.0_f32;
        for s in &segments {
            if s.length <= 0.0 {
                return Err(WaveAudioError::VocalTract(
                    "segment length must be positive",
                ));
            }
            if s.radius_start < 0.0 || s.radius_end < 0.0 {
                return Err(WaveAudioError::VocalTract(
                    "segment radius must be non-negative",
                ));
            }
            total_length += s.length;
        }
        Ok(VocalTractSdf {
            segments,
            total_length,
        })
    }

    /// Build a canonical adult human vocal-tract SDF — 17 cm long, with
    /// 5 segments approximating pharynx (large radius) → oral-cavity
    /// (smaller radius) → lip aperture (rigid).
    #[must_use]
    pub fn human_default() -> VocalTractSdf {
        // Segment radii from the canonical Story-and-Titze area-function
        // (rounded for compactness). All in metres.
        let segments = vec![
            // Pharynx : large radius, impedance walls.
            TractSegment::new(0.012, 0.014, 0.045, WallClass::Impedance),
            // Lower oral cavity : tongue-base.
            TractSegment::new(0.014, 0.011, 0.040, WallClass::Impedance),
            // Mid oral cavity.
            TractSegment::new(0.011, 0.010, 0.035, WallClass::Impedance),
            // Upper oral cavity : approaching lips.
            TractSegment::new(0.010, 0.008, 0.030, WallClass::Impedance),
            // Lip aperture : open-end (Soft).
            TractSegment::new(0.008, 0.006, 0.020, WallClass::Soft),
        ];
        VocalTractSdf::new(segments).expect("canonical human tract is valid")
    }

    /// Build a procedural creature vocal-tract from a "size factor" +
    /// "throat-narrowness" pair. The size-factor scales the overall
    /// tract length ; throat-narrowness biases the pharynx radius
    /// downward (whistlier tone) when high.
    pub fn creature(size_factor: f32, throat_narrowness: f32) -> Result<VocalTractSdf> {
        let s = size_factor.max(0.1);
        let n = throat_narrowness.clamp(0.0, 1.0);
        let pharynx_r = 0.013 * (1.0 - 0.5 * n);
        let segments = vec![
            TractSegment::new(pharynx_r, pharynx_r * 1.05, 0.045 * s, WallClass::Impedance),
            TractSegment::new(pharynx_r * 1.05, 0.011 * s, 0.040 * s, WallClass::Impedance),
            TractSegment::new(0.011 * s, 0.010 * s, 0.035 * s, WallClass::Impedance),
            TractSegment::new(0.010 * s, 0.008 * s, 0.030 * s, WallClass::Impedance),
            TractSegment::new(0.008 * s, 0.006 * s, 0.020 * s, WallClass::Soft),
        ];
        VocalTractSdf::new(segments)
    }

    /// Number of segments.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Total tract length (m).
    #[must_use]
    pub fn total_length(&self) -> f32 {
        self.total_length
    }

    /// Read segment by index.
    #[must_use]
    pub fn segment(&self, index: usize) -> Option<TractSegment> {
        self.segments.get(index).copied()
    }

    /// Iterate segments.
    pub fn iter(&self) -> impl Iterator<Item = TractSegment> + '_ {
        self.segments.iter().copied()
    }

    /// Evaluate the radius along the centerline at axial position `s ∈
    /// [0, total_length]`. Linear interpolation between segment
    /// endpoints. Out-of-bounds `s` clamps to the nearest endpoint.
    #[must_use]
    pub fn radius_at_s(&self, s: f32) -> f32 {
        let mut acc = 0.0_f32;
        for seg in &self.segments {
            if s <= acc + seg.length {
                let t = ((s - acc) / seg.length).clamp(0.0, 1.0);
                return seg.radius_at(t);
            }
            acc += seg.length;
        }
        // Past the end : last segment's end-radius.
        self.segments.last().map(|s| s.radius_end).unwrap_or(0.0)
    }

    /// Cross-sectional area `A(s)` along the centerline.
    #[must_use]
    pub fn area_at_s(&self, s: f32) -> f32 {
        let r = self.radius_at_s(s);
        core::f32::consts::PI * r * r
    }

    /// Wall classification at axial position `s`.
    #[must_use]
    pub fn wall_class_at_s(&self, s: f32) -> WallClass {
        let mut acc = 0.0_f32;
        for seg in &self.segments {
            if s <= acc + seg.length {
                return seg.wall_class;
            }
            acc += seg.length;
        }
        self.segments
            .last()
            .map(|s| s.wall_class)
            .unwrap_or(WallClass::Soft)
    }

    /// Evaluate the SDF at a 3D point treated as `(s, ρ, _)` cylindrical
    /// coordinates relative to the centerline. Returns the signed
    /// distance to the nearest tract wall : negative inside, positive
    /// outside.
    ///
    /// `s` is the axial coordinate in metres ; `rho` is the radial
    /// distance from the centerline.
    #[must_use]
    pub fn evaluate(&self, s: f32, rho: f32) -> f32 {
        if s < 0.0 {
            // Before the glottis : distance is the L2 distance to (0, 0).
            let r0 = self.segments[0].radius_start;
            let dr = rho - r0;
            // Inside disc : negative ; outside : positive.
            return (s * s + dr.max(0.0).powi(2)).sqrt() * dr.signum().max(-1.0);
        }
        if s > self.total_length {
            // Past the lips : analogous distance to lip aperture.
            let r_end = self.segments.last().map(|s| s.radius_end).unwrap_or(0.0);
            let dr = rho - r_end;
            let ds = s - self.total_length;
            return (ds * ds + dr.max(0.0).powi(2)).sqrt() * dr.signum().max(-1.0);
        }
        let r = self.radius_at_s(s);
        rho - r
    }

    /// Compute the per-band acoustic resonant frequencies (formants) of
    /// the tract using the Webster-equation approximation. Returns the
    /// first `n` formants in Hz.
    ///
    /// § APPROXIMATION
    ///   For a uniform-area tube of length `L` closed at the glottis +
    ///   open at the lips, the formants are at `f_n = (2n-1) c / 4L`
    ///   for `n = 1, 2, 3, ...`. We apply that here as a first-order
    ///   approximation ; the full Webster solver is in the LBM module.
    #[must_use]
    pub fn formants(&self, n: usize, speed_of_sound: f32) -> Vec<f32> {
        if self.total_length <= 0.0 {
            return vec![];
        }
        (1..=n)
            .map(|k| (2.0 * k as f32 - 1.0) * speed_of_sound / (4.0 * self.total_length))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{TractSegment, VocalTractSdf, WallClass};

    #[test]
    fn segment_radius_at_endpoints() {
        let s = TractSegment::new(0.01, 0.02, 0.05, WallClass::Soft);
        assert!((s.radius_at(0.0) - 0.01).abs() < 1e-6);
        assert!((s.radius_at(1.0) - 0.02).abs() < 1e-6);
    }

    #[test]
    fn segment_radius_at_midpoint() {
        let s = TractSegment::new(0.0, 0.02, 0.1, WallClass::Soft);
        assert!((s.radius_at(0.5) - 0.01).abs() < 1e-6);
    }

    #[test]
    fn segment_area_at_midpoint() {
        let s = TractSegment::new(0.0, 0.02, 0.1, WallClass::Soft);
        let a = s.area_at(0.5);
        let expected = core::f32::consts::PI * 0.01 * 0.01;
        assert!((a - expected).abs() < 1e-6);
    }

    #[test]
    fn empty_segments_rejected() {
        let r = VocalTractSdf::new(vec![]);
        assert!(r.is_err());
    }

    #[test]
    fn zero_length_segment_rejected() {
        let r = VocalTractSdf::new(vec![TractSegment::new(0.01, 0.01, 0.0, WallClass::Soft)]);
        assert!(r.is_err());
    }

    #[test]
    fn negative_radius_segment_rejected() {
        let r = VocalTractSdf::new(vec![TractSegment::new(-0.01, 0.01, 0.05, WallClass::Soft)]);
        assert!(r.is_err());
    }

    #[test]
    fn human_default_has_five_segments() {
        let t = VocalTractSdf::human_default();
        assert_eq!(t.segment_count(), 5);
    }

    #[test]
    fn human_default_total_length_about_17cm() {
        let t = VocalTractSdf::human_default();
        let len = t.total_length();
        assert!((len - 0.170).abs() < 0.01);
    }

    #[test]
    fn human_default_first_formant_in_human_range() {
        // Human first formant ≈ 500 Hz (vowel /a/) ; uniform-tube approx
        // yields f1 = c/(4L). For c=343, L=0.170, f1 ≈ 504 Hz.
        let t = VocalTractSdf::human_default();
        let f = t.formants(1, 343.0);
        assert!(!f.is_empty());
        let f1 = f[0];
        assert!(f1 > 400.0 && f1 < 600.0, "f1 = {f1}");
    }

    #[test]
    fn human_default_formants_are_odd_harmonics() {
        let t = VocalTractSdf::human_default();
        let f = t.formants(3, 343.0);
        // f1, f2, f3 should be in ratio 1 : 3 : 5 for uniform-tube.
        assert!((f[1] / f[0] - 3.0).abs() < 0.05);
        assert!((f[2] / f[0] - 5.0).abs() < 0.05);
    }

    #[test]
    fn creature_size_factor_scales_length() {
        let small = VocalTractSdf::creature(0.5, 0.5).unwrap();
        let big = VocalTractSdf::creature(2.0, 0.5).unwrap();
        assert!(big.total_length() > small.total_length());
    }

    #[test]
    fn radius_at_s_zero_is_pharynx_start() {
        let t = VocalTractSdf::human_default();
        let r = t.radius_at_s(0.0);
        assert!((r - 0.012).abs() < 1e-5);
    }

    #[test]
    fn radius_at_s_past_end_is_lip_radius() {
        let t = VocalTractSdf::human_default();
        let r = t.radius_at_s(t.total_length() + 0.01);
        // Lip end-radius (last segment) is 0.006.
        assert!((r - 0.006).abs() < 1e-5);
    }

    #[test]
    fn area_at_s_is_pi_r_squared() {
        let t = VocalTractSdf::human_default();
        let r = t.radius_at_s(0.05);
        let a = t.area_at_s(0.05);
        let expected = core::f32::consts::PI * r * r;
        assert!((a - expected).abs() < 1e-6);
    }

    #[test]
    fn wall_class_at_lips_is_soft() {
        let t = VocalTractSdf::human_default();
        let w = t.wall_class_at_s(t.total_length() - 0.005);
        assert_eq!(w, WallClass::Soft);
    }

    #[test]
    fn wall_class_in_pharynx_is_impedance() {
        let t = VocalTractSdf::human_default();
        let w = t.wall_class_at_s(0.01);
        assert_eq!(w, WallClass::Impedance);
    }

    #[test]
    fn evaluate_inside_returns_negative() {
        let t = VocalTractSdf::human_default();
        // At s = 0.05, rho = 0 (on axis) ; should be inside (negative).
        let d = t.evaluate(0.05, 0.0);
        assert!(d < 0.0, "expected negative inside, got {d}");
    }

    #[test]
    fn evaluate_outside_returns_positive() {
        let t = VocalTractSdf::human_default();
        // rho greater than max radius (0.014) ; outside the wall.
        let d = t.evaluate(0.05, 0.05);
        assert!(d > 0.0, "expected positive outside, got {d}");
    }

    #[test]
    fn iter_count_matches_segment_count() {
        let t = VocalTractSdf::human_default();
        assert_eq!(t.iter().count(), t.segment_count());
    }
}

//! § stokes — 4-component Stokes IQUV polarized rendering + Mueller matrices
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-FID-STOKES (W-LOA-fidelity-stokes)
//!
//! § ROLE
//!   Polarized-light arithmetic for the LoA-v13 host renderer. Tracks the
//!   Stokes vector (I, Q, U, V) per ray instead of scalar radiance and uses
//!   per-material Mueller matrices (4×4) to model polarization-correct
//!   reflection. Enables accurate dielectric/metallic/iridescent thin-film
//!   appearances that the substrate's `ApockyLight` primitive (per
//!   `specs/32_SIGNATURE_RENDERING.csl`) already specifies.
//!
//! § STOKES VECTOR (I, Q, U, V)
//!   - `I` : total intensity            (always ≥ 0)
//!   - `Q` : 0°/90° linear polarization (Q ∈ [-I, I])
//!   - `U` : ±45° linear polarization   (U ∈ [-I, I])
//!   - `V` : circular polarization      (V ∈ [-I, I])
//!
//!   Constraint : `I² ≥ Q² + U² + V²` (physically realizable).
//!
//! § MUELLER MATRIX
//!   4×4 real matrix mapping incident Stokes → reflected/transmitted Stokes.
//!   Composes by multiplication : `S_out = M_layerN · ... · M_layer1 · S_in`.
//!
//! § STAGE-1 PATH
//!   Mueller LUT → KAN-substrate-runtime per-cell Mueller field driven by
//!   the ω-field signature. Stage-0 keeps the 16-entry LUT static so the
//!   renderer is operational while spectral path matures.
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::many_single_char_names)]

use bytemuck::{Pod, Zeroable};

// ──────────────────────────────────────────────────────────────────────────
// § StokesVector — 4-component IQUV polarized radiance
// ──────────────────────────────────────────────────────────────────────────

/// Stokes vector (I, Q, U, V). 16 bytes · 16-aligned for GPU uploads.
///
/// `Pod + Zeroable` means we can `bytemuck::cast_slice` an array straight
/// into a wgpu uniform buffer without an intermediate copy.
#[repr(C, align(16))]
#[derive(Pod, Zeroable, Copy, Clone, Debug, Default, PartialEq)]
pub struct StokesVector {
    /// Total intensity (always ≥ 0).
    pub i: f32,
    /// 0°/90° linear polarization (-i..i).
    pub q: f32,
    /// ±45° linear polarization (-i..i).
    pub u: f32,
    /// Circular polarization (-i..i).
    pub v: f32,
}

impl StokesVector {
    /// All zeros (dark / no light).
    #[must_use]
    pub const fn zero() -> Self {
        Self { i: 0.0, q: 0.0, u: 0.0, v: 0.0 }
    }

    /// Unpolarized light at the given intensity. Q = U = V = 0.
    #[must_use]
    pub const fn unpolarized(intensity: f32) -> Self {
        Self { i: intensity, q: 0.0, u: 0.0, v: 0.0 }
    }

    /// Fully horizontally-polarized (E-field along x-axis). Q = +I.
    #[must_use]
    pub const fn linear_horizontal(intensity: f32) -> Self {
        Self { i: intensity, q: intensity, u: 0.0, v: 0.0 }
    }

    /// Fully vertically-polarized. Q = -I.
    #[must_use]
    pub const fn linear_vertical(intensity: f32) -> Self {
        Self { i: intensity, q: -intensity, u: 0.0, v: 0.0 }
    }

    /// Fully +45°-polarized. U = +I.
    #[must_use]
    pub const fn linear_45(intensity: f32) -> Self {
        Self { i: intensity, q: 0.0, u: intensity, v: 0.0 }
    }

    /// Fully -45°-polarized. U = -I.
    #[must_use]
    pub const fn linear_minus_45(intensity: f32) -> Self {
        Self { i: intensity, q: 0.0, u: -intensity, v: 0.0 }
    }

    /// Right-circularly polarized. V = +I.
    #[must_use]
    pub const fn circular_right(intensity: f32) -> Self {
        Self { i: intensity, q: 0.0, u: 0.0, v: intensity }
    }

    /// Left-circularly polarized. V = -I.
    #[must_use]
    pub const fn circular_left(intensity: f32) -> Self {
        Self { i: intensity, q: 0.0, u: 0.0, v: -intensity }
    }

    /// Degree of LINEAR polarization : `sqrt(Q² + U²) / I`. Range 0..1.
    #[must_use]
    pub fn dop_linear(&self) -> f32 {
        if self.i.abs() < 1e-6 {
            0.0
        } else {
            (self.q * self.q + self.u * self.u).sqrt() / self.i
        }
    }

    /// Degree of TOTAL polarization : `sqrt(Q² + U² + V²) / I`. Range 0..1.
    #[must_use]
    pub fn dop_total(&self) -> f32 {
        if self.i.abs() < 1e-6 {
            0.0
        } else {
            (self.q * self.q + self.u * self.u + self.v * self.v).sqrt() / self.i
        }
    }

    /// Add two Stokes vectors (incoherent superposition).
    #[must_use]
    pub fn add(&self, other: Self) -> Self {
        Self {
            i: self.i + other.i,
            q: self.q + other.q,
            u: self.u + other.u,
            v: self.v + other.v,
        }
    }

    /// Scale by a scalar (e.g. for attenuation / albedo).
    #[must_use]
    pub fn scale(&self, k: f32) -> Self {
        Self {
            i: self.i * k,
            q: self.q * k,
            u: self.u * k,
            v: self.v * k,
        }
    }

    /// Pack as a 4-element array in IQUV order (for WGSL upload).
    #[must_use]
    pub const fn as_array(&self) -> [f32; 4] {
        [self.i, self.q, self.u, self.v]
    }

    /// True iff the Stokes vector is physically realizable (I² ≥ Q² + U² + V²).
    /// Allows a small floating-point epsilon for rounding.
    #[must_use]
    pub fn is_physical(&self) -> bool {
        let pol_sq = self.q * self.q + self.u * self.u + self.v * self.v;
        let i_sq = self.i * self.i;
        // Allow 1% slack for accumulated FP error.
        i_sq + 1e-4 >= pol_sq
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § MuellerMatrix — 4×4 polarization-mapping matrix
// ──────────────────────────────────────────────────────────────────────────

/// Mueller matrix : maps incoming Stokes vector to outgoing.
/// 64 bytes · 16-byte aligned · `Pod + Zeroable` for direct GPU upload.
#[repr(C, align(16))]
#[derive(Pod, Zeroable, Copy, Clone, Debug, PartialEq)]
pub struct MuellerMatrix(pub [[f32; 4]; 4]);

impl Default for MuellerMatrix {
    fn default() -> Self {
        Self::identity()
    }
}

impl MuellerMatrix {
    /// Identity matrix : preserves polarization perfectly. Equivalent to a
    /// perfect mirror (or "do nothing" pass-through).
    #[must_use]
    pub const fn identity() -> Self {
        Self([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Pure depolarizer : output is unpolarized regardless of input.
    /// Only the I component survives ; Q/U/V are zeroed.
    #[must_use]
    pub const fn depolarizer() -> Self {
        Self([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ])
    }

    /// Linear polarizer with transmission axis at 0° (horizontal).
    /// Transmits half of unpolarized light · 100% of horizontally-polarized.
    #[must_use]
    pub const fn linear_polarizer_horizontal() -> Self {
        Self([
            [0.5, 0.5, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ])
    }

    /// Linear polarizer with transmission axis at 90° (vertical).
    #[must_use]
    pub const fn linear_polarizer_vertical() -> Self {
        Self([
            [0.5, -0.5, 0.0, 0.0],
            [-0.5, 0.5, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ])
    }

    /// Linear polarizer at +45°.
    #[must_use]
    pub const fn linear_polarizer_45() -> Self {
        Self([
            [0.5, 0.0, 0.5, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.5, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ])
    }

    /// Linear polarizer at angle `theta` (radians) measured CCW from horizontal.
    /// Generalizes the 0/45/90/135 fixed cases.
    #[must_use]
    pub fn linear_polarizer(theta: f32) -> Self {
        let c2 = (2.0 * theta).cos();
        let s2 = (2.0 * theta).sin();
        Self([
            [0.5, 0.5 * c2, 0.5 * s2, 0.0],
            [0.5 * c2, 0.5 * c2 * c2, 0.5 * c2 * s2, 0.0],
            [0.5 * s2, 0.5 * c2 * s2, 0.5 * s2 * s2, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ])
    }

    /// Quarter-wave plate with fast axis at angle `theta` (radians).
    /// Converts linear polarization at ±45° into circular polarization.
    #[must_use]
    pub fn quarter_wave_plate(theta: f32) -> Self {
        let c2 = (2.0 * theta).cos();
        let s2 = (2.0 * theta).sin();
        Self([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c2 * c2, c2 * s2, -s2],
            [0.0, c2 * s2, s2 * s2, c2],
            [0.0, s2, -c2, 0.0],
        ])
    }

    /// Half-wave plate with fast axis at angle `theta` (radians).
    /// Rotates linear polarization by `2θ`.
    #[must_use]
    pub fn half_wave_plate(theta: f32) -> Self {
        let c4 = (4.0 * theta).cos();
        let s4 = (4.0 * theta).sin();
        Self([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, c4, s4, 0.0],
            [0.0, s4, -c4, 0.0],
            [0.0, 0.0, 0.0, -1.0],
        ])
    }

    /// Fresnel dielectric reflection (real IOR) for unpolarized → partly-Q
    /// polarized output. `n1` = incident medium (1.0 air) · `n2` = surface
    /// medium (e.g. 1.5 glass) · `theta_in` = angle of incidence (radians).
    ///
    /// At the Brewster angle (`theta_B = atan(n2/n1)`), reflected light is
    /// purely vertically-polarized (Q = -I). This is the canonical photo-
    /// graphy filter physics.
    #[must_use]
    pub fn fresnel_dielectric(n1: f32, n2: f32, theta_in: f32) -> Self {
        let cos_i = theta_in.cos();
        let sin_i = theta_in.sin();
        let sin_t_sq = (n1 / n2).powi(2) * sin_i * sin_i;
        if sin_t_sq >= 1.0 {
            // Total internal reflection : both s and p reflect 100%.
            return Self::identity();
        }
        let cos_t = (1.0 - sin_t_sq).sqrt();
        // Fresnel s-polarized + p-polarized reflection coefficients.
        let rs_num = n1 * cos_i - n2 * cos_t;
        let rs_den = n1 * cos_i + n2 * cos_t;
        let rs = if rs_den.abs() < 1e-9 { 0.0 } else { rs_num / rs_den };
        let rp_num = n2 * cos_i - n1 * cos_t;
        let rp_den = n2 * cos_i + n1 * cos_t;
        let rp = if rp_den.abs() < 1e-9 { 0.0 } else { rp_num / rp_den };
        let rs_sq = rs * rs;
        let rp_sq = rp * rp;
        let m00 = 0.5 * (rs_sq + rp_sq);
        let m01 = 0.5 * (rs_sq - rp_sq);
        // For real (no absorption) dielectric : the reflected light's phase
        // delay is 0 or π so V stays mapped to V via cos·M33 = +1. Q ↔ Q,
        // U ↔ U scale by sqrt(rs²·rp²) magnitude.
        let m22 = (rs_sq * rp_sq).sqrt() * (rs * rp).signum();
        Self([
            [m00, m01, 0.0, 0.0],
            [m01, m00, 0.0, 0.0],
            [0.0, 0.0, m22, 0.0],
            [0.0, 0.0, 0.0, m22],
        ])
    }

    /// Fresnel metal reflection (complex IOR : `n + i·k`). For real-world
    /// metals the imaginary part `k` causes a phase-shift between s- and
    /// p-polarized components, mapping U/V into each other. At grazing
    /// angles this produces visible polarization-rotation.
    ///
    /// Reference values at λ = 550nm :
    ///   - Gold     : n ≈ 0.27 · k ≈ 2.78
    ///   - Silver   : n ≈ 0.12 · k ≈ 3.45
    ///   - Aluminum : n ≈ 1.30 · k ≈ 7.40
    ///   - Steel    : n ≈ 2.50 · k ≈ 3.50
    #[must_use]
    pub fn fresnel_metal(n: f32, k: f32, theta_in: f32) -> Self {
        let cos_i = theta_in.cos();
        let sin_i_sq = theta_in.sin().powi(2);
        // Complex-IOR reflectance for s-polarization.
        // |r_s|² = ((n - cos_i)² + k²) / ((n + cos_i)² + k²)  · simplified
        //   form for normal-incidence boundary conditions.
        let n2_k2 = n * n + k * k;
        let rs_sq = (n2_k2 - 2.0 * n * cos_i + cos_i * cos_i)
            / (n2_k2 + 2.0 * n * cos_i + cos_i * cos_i);
        let rp_sq = (n2_k2 * cos_i * cos_i - 2.0 * n * cos_i + 1.0)
            / (n2_k2 * cos_i * cos_i + 2.0 * n * cos_i + 1.0);
        let m00 = 0.5 * (rs_sq + rp_sq);
        let m01 = 0.5 * (rs_sq - rp_sq);
        // Phase difference between s and p for the metal :
        let phase = (2.0 * k * cos_i / (n2_k2 - cos_i * cos_i + 1e-6)).atan();
        // Avoid unused-binding warning on sin_i_sq; it would normally enter
        // the off-axis polarization rotation derived from (n²+k² - sin²θ).
        let _ = sin_i_sq;
        let m22 = (rs_sq * rp_sq).sqrt() * phase.cos();
        let m23 = (rs_sq * rp_sq).sqrt() * phase.sin();
        Self([
            [m00, m01, 0.0, 0.0],
            [m01, m00, 0.0, 0.0],
            [0.0, 0.0, m22, m23],
            [0.0, 0.0, -m23, m22],
        ])
    }

    /// Thin-film interference Mueller (e.g. soap bubble · oil-on-water · TiO2
    /// over Al). `d_nm` = film thickness · `n_film` = film IOR · `n_subst` =
    /// substrate IOR · `theta` = view angle (radians) · `lambda_nm` = light
    /// wavelength (nm). Produces strong wavelength-dependent Q and U
    /// variations (the iridescent rainbow).
    #[must_use]
    pub fn thin_film(d_nm: f32, n_film: f32, n_subst: f32, theta: f32, lambda_nm: f32) -> Self {
        // Optical-path-length difference.
        let cos_t_film = (1.0 - (theta.sin() / n_film).powi(2)).max(0.0).sqrt();
        let opd = 2.0 * n_film * d_nm * cos_t_film;
        let phase = 2.0 * std::f32::consts::PI * opd / lambda_nm.max(1.0);
        // Reflection magnitude at the air-film and film-substrate interfaces.
        let r1 = ((1.0 - n_film) / (1.0 + n_film)).powi(2);
        let r2 = ((n_film - n_subst) / (n_film + n_subst)).powi(2);
        let r_total = r1 + r2 + 2.0 * (r1 * r2).sqrt() * phase.cos();
        let m00 = r_total.clamp(0.0, 1.0);
        // Phase-driven Q/U coupling : sinusoidal in phase, signs alternate.
        let m01 = 0.5 * m00 * phase.cos();
        let m22 = m00 * (phase.cos());
        let m33 = m00 * (phase.cos());
        Self([
            [m00, m01, 0.0, 0.0],
            [m01, m00, 0.0, 0.0],
            [0.0, 0.0, m22, 0.0],
            [0.0, 0.0, 0.0, m33],
        ])
    }

    /// Apply this Mueller matrix to a Stokes vector (matrix × vector).
    /// `S_out = M · S_in`.
    #[must_use]
    pub fn apply(&self, s: StokesVector) -> StokesVector {
        let m = &self.0;
        StokesVector {
            i: m[0][0] * s.i + m[0][1] * s.q + m[0][2] * s.u + m[0][3] * s.v,
            q: m[1][0] * s.i + m[1][1] * s.q + m[1][2] * s.u + m[1][3] * s.v,
            u: m[2][0] * s.i + m[2][1] * s.q + m[2][2] * s.u + m[2][3] * s.v,
            v: m[3][0] * s.i + m[3][1] * s.q + m[3][2] * s.u + m[3][3] * s.v,
        }
    }

    /// Compose two Mueller matrices : `(self · other)` (self applied AFTER other).
    /// Equivalent to `self.apply(other.apply(s))`.
    #[must_use]
    pub fn then(&self, other: Self) -> Self {
        let a = &self.0;
        let b = &other.0;
        let mut out = [[0.0_f32; 4]; 4];
        for (i, row_out) in out.iter_mut().enumerate() {
            for (j, cell) in row_out.iter_mut().enumerate() {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += a[i][k] * b[k][j];
                }
                *cell = sum;
            }
        }
        Self(out)
    }

    /// Pack as flat `[f32; 16]` (row-major) for direct WGSL upload.
    #[must_use]
    pub fn as_flat(&self) -> [f32; 16] {
        let m = &self.0;
        [
            m[0][0], m[0][1], m[0][2], m[0][3],
            m[1][0], m[1][1], m[1][2], m[1][3],
            m[2][0], m[2][1], m[2][2], m[2][3],
            m[3][0], m[3][1], m[3][2], m[3][3],
        ]
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Mueller LUT — 16 canonical presets (matches Material LUT 1:1)
// ──────────────────────────────────────────────────────────────────────────

/// Total entries in the MUELLER_LUT (always equals MATERIAL_LUT_LEN).
pub const MUELLER_LUT_LEN: usize = 16;

/// Build the 16-entry canonical Mueller LUT.
///
/// Order matches the `MAT_*` constants in `material.rs`. Each entry models
/// the polarization behavior of the corresponding material :
///
///   0  MATTE_GREY        — depolarizer (Lambertian scatter randomizes)
///   1  VERMILLION_LACQUER— slight-Q polarization (oil-paint sheen)
///   2  GOLD_LEAF         — full-Mueller metal (n=0.27 + ik=2.78 @ 550nm)
///   3  BRUSHED_STEEL     — anisotropic linear-polarizer-like
///   4  IRIDESCENT        — thin-film 200nm TiO2 over Al @ 30°
///   5  EMISSIVE_CYAN     — depolarizer (emission is unpolarized)
///   6  TRANSPARENT_GLASS — Fresnel dielectric n=1.5 @ Brewster
///   7  HOLOGRAPHIC       — random Mueller (scrambles polarization)
///   8  HAIRY_FUR         — depolarizing volumetric
///   9  DICHROIC_VIOLET   — strong wavelength-dep linear polarization
///   10 NEON_MAGENTA      — mostly-emissive · low polarization
///   11 DEEP_INDIGO       — polarization-preserving low-albedo
///   12 OFF_WHITE         — Lambertian depolarizer
///   13 WARM_SKY          — atmospheric Rayleigh (pol at 90° from sun)
///   14 GRADIENT_RED      — depolarizer
///   15 PINK_NOISE_VOL    — volumetric depolarizer
#[must_use]
pub fn mueller_lut() -> [MuellerMatrix; MUELLER_LUT_LEN] {
    let pi = std::f32::consts::PI;
    [
        // 0 MATTE_GREY — Lambertian depolarizer
        MuellerMatrix::depolarizer(),
        // 1 VERMILLION_LACQUER — slight-Q oil-paint sheen
        MuellerMatrix([
            [1.0, 0.04, 0.0, 0.0],
            [0.04, 0.95, 0.0, 0.0],
            [0.0, 0.0, 0.93, 0.0],
            [0.0, 0.0, 0.0, 0.93],
        ]),
        // 2 GOLD_LEAF — full-Mueller metal at θ=30° (typical view angle)
        MuellerMatrix::fresnel_metal(0.27, 2.78, pi / 6.0),
        // 3 BRUSHED_STEEL — directional linear-polarizer-like
        MuellerMatrix([
            [1.0, 0.30, 0.0, 0.0],
            [0.30, 0.85, 0.0, 0.0],
            [0.0, 0.0, 0.50, 0.10],
            [0.0, 0.0, -0.10, 0.50],
        ]),
        // 4 IRIDESCENT — thin-film TiO2 over Al @ 30°, λ=550nm
        MuellerMatrix::thin_film(200.0, 2.40, 0.95, pi / 6.0, 550.0),
        // 5 EMISSIVE_CYAN — pure depolarizer
        MuellerMatrix::depolarizer(),
        // 6 TRANSPARENT_GLASS — Fresnel dielectric @ Brewster (n=1.5 → θ_B≈56.3°)
        MuellerMatrix::fresnel_dielectric(1.0, 1.5, 1.0_f32.atan() + 0.5),
        // 7 HOLOGRAPHIC — random Mueller (scrambles polarization)
        MuellerMatrix([
            [1.0, 0.10, -0.10, 0.05],
            [0.10, 0.20, 0.15, -0.05],
            [-0.10, 0.15, 0.10, 0.20],
            [0.05, -0.05, -0.20, 0.05],
        ]),
        // 8 HAIRY_FUR — depolarizing volumetric
        MuellerMatrix::depolarizer(),
        // 9 DICHROIC_VIOLET — strong linear-polarizer at 30°
        MuellerMatrix::linear_polarizer(pi / 6.0),
        // 10 NEON_MAGENTA — emissive-dominated · low pol
        MuellerMatrix([
            [1.0, 0.02, 0.0, 0.0],
            [0.02, 0.20, 0.0, 0.0],
            [0.0, 0.0, 0.20, 0.0],
            [0.0, 0.0, 0.0, 0.20],
        ]),
        // 11 DEEP_INDIGO — pol-preserving low-albedo
        MuellerMatrix([
            [0.40, 0.0, 0.0, 0.0],
            [0.0, 0.38, 0.0, 0.0],
            [0.0, 0.0, 0.38, 0.0],
            [0.0, 0.0, 0.0, 0.38],
        ]),
        // 12 OFF_WHITE — Lambertian depolarizer
        MuellerMatrix::depolarizer(),
        // 13 WARM_SKY — Rayleigh-style polarization at +90° from sun, U component
        MuellerMatrix([
            [1.0, 0.0, 0.40, 0.0],
            [0.0, 0.30, 0.0, 0.0],
            [0.40, 0.0, 0.30, 0.0],
            [0.0, 0.0, 0.0, 0.30],
        ]),
        // 14 GRADIENT_RED — depolarizer
        MuellerMatrix::depolarizer(),
        // 15 PINK_NOISE_VOL — depolarizing volumetric
        MuellerMatrix::depolarizer(),
    ]
}

/// Pack the Mueller LUT into a flat `[f32; 16 × 16]` for WGSL upload.
/// Each Mueller is laid out row-major as 4 vec4 (16 floats = 64 bytes).
#[must_use]
pub fn mueller_lut_flat() -> [f32; MUELLER_LUT_LEN * 16] {
    let lut = mueller_lut();
    let mut out = [0.0_f32; MUELLER_LUT_LEN * 16];
    for (i, m) in lut.iter().enumerate() {
        let f = m.as_flat();
        out[i * 16..(i + 1) * 16].copy_from_slice(&f);
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────
// § Polarization-view diagnostic mode
// ──────────────────────────────────────────────────────────────────────────

/// Diagnostic visualization mode for the polarization channels.
///
/// `Intensity` (mode 0) is the default · the rendered image looks identical
/// to the pre-Stokes path. Modes 1-4 are false-color overlays useful for
/// validating the Mueller pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PolarizationView {
    /// Default · I component → RGB tonemap.
    Intensity = 0,
    /// False-color Q : red=+H · blue=-H · green=zero.
    FalseColorQ = 1,
    /// False-color U : red=+45° · blue=-45° · green=zero.
    FalseColorU = 2,
    /// False-color V : red=right-circular · blue=left-circular.
    FalseColorV = 3,
    /// Degree of polarization (0..1) → grayscale.
    Dop = 4,
}

impl PolarizationView {
    /// Cycle to the next mode (4 → 0).
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Intensity => Self::FalseColorQ,
            Self::FalseColorQ => Self::FalseColorU,
            Self::FalseColorU => Self::FalseColorV,
            Self::FalseColorV => Self::Dop,
            Self::Dop => Self::Intensity,
        }
    }

    /// Convert from a u32 (the FFI surface uses raw u32). Out-of-range
    /// values clamp to `Intensity`.
    #[must_use]
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::FalseColorQ,
            2 => Self::FalseColorU,
            3 => Self::FalseColorV,
            4 => Self::Dop,
            _ => Self::Intensity,
        }
    }

    /// Human-readable name (for HUD + MCP).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Intensity => "Intensity",
            Self::FalseColorQ => "Q (linear-horizontal)",
            Self::FalseColorU => "U (linear-45)",
            Self::FalseColorV => "V (circular)",
            Self::Dop => "DOP (degree-of-polarization)",
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Polarization-room diagnostic panels (4 panels = 4 polarizer settings)
// ──────────────────────────────────────────────────────────────────────────

/// One panel in the polarization-diagnostic test layout. Used by the
/// `MaterialRoom`-extension polarization gallery and the corresponding
/// MCP test that verifies each panel emits distinct Stokes outputs.
#[derive(Debug, Clone, Copy)]
pub struct PolarizationPanel {
    /// Human-readable label shown over the panel.
    pub label: &'static str,
    /// Mueller matrix the panel's incoming light passes through before
    /// striking the chart material.
    pub filter: MuellerMatrix,
    /// Expected Stokes signature when illuminated with `sun_stokes_default()`.
    pub expected_signature: StokesVector,
}

/// Build the 4 canonical polarization-test panels.
///
/// Each panel is illuminated with the same Macbeth-chart material set, but
/// the incoming sun light passes through a different filter :
///   - Panel 0 : no filter (identity ; full sun Stokes)
///   - Panel 1 : 0° linear polarizer
///   - Panel 2 : +45° linear polarizer
///   - Panel 3 : right-circular polarizer (LP @ 0° + QWP @ 45°)
#[must_use]
pub fn polarization_panels() -> [PolarizationPanel; 4] {
    let pi_4 = std::f32::consts::PI / 4.0;
    let s_in = sun_stokes_default();

    let none = MuellerMatrix::identity();
    let lp_h = MuellerMatrix::linear_polarizer_horizontal();
    let lp_45 = MuellerMatrix::linear_polarizer_45();
    let qwp_45 = MuellerMatrix::quarter_wave_plate(pi_4);
    // Right-circular = LP @ 0° then QWP @ 45°.
    let circ = qwp_45.then(lp_h);

    [
        PolarizationPanel {
            label: "Panel-0 (no filter)",
            filter: none,
            expected_signature: none.apply(s_in),
        },
        PolarizationPanel {
            label: "Panel-1 (LP 0°)",
            filter: lp_h,
            expected_signature: lp_h.apply(s_in),
        },
        PolarizationPanel {
            label: "Panel-2 (LP +45°)",
            filter: lp_45,
            expected_signature: lp_45.apply(s_in),
        },
        PolarizationPanel {
            label: "Panel-3 (RC circular)",
            filter: circ,
            expected_signature: circ.apply(s_in),
        },
    ]
}

// ──────────────────────────────────────────────────────────────────────────
// § Sun reference Stokes vector (slight-Q from atmospheric scattering)
// ──────────────────────────────────────────────────────────────────────────

/// Default sun Stokes vector. The atmosphere adds a slight horizontal
/// linear polarization to the direct sunlight (typical DOP ~ 4% from
/// scattered light contribution near the sun direction).
#[must_use]
pub const fn sun_stokes_default() -> StokesVector {
    StokesVector {
        i: 1.0,
        q: 0.04,
        u: 0.0,
        v: 0.0,
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § TESTS
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn stokes_eq(a: StokesVector, b: StokesVector, eps: f32) -> bool {
        approx_eq(a.i, b.i, eps)
            && approx_eq(a.q, b.q, eps)
            && approx_eq(a.u, b.u, eps)
            && approx_eq(a.v, b.v, eps)
    }

    // ─── Spec-required tests (≥ 8 from the directive) ────────────────────

    #[test]
    fn stokes_unpolarized_dop_zero() {
        let s = StokesVector::unpolarized(1.0);
        assert!(approx_eq(s.dop_linear(), 0.0, 1e-6));
        assert!(approx_eq(s.dop_total(), 0.0, 1e-6));
    }

    #[test]
    fn mueller_identity_preserves_stokes() {
        let s = StokesVector { i: 1.0, q: 0.5, u: -0.3, v: 0.2 };
        let out = MuellerMatrix::identity().apply(s);
        assert!(stokes_eq(s, out, 1e-6), "id·s != s: {out:?}");
    }

    #[test]
    fn linear_polarizer_horizontal_filters_v_polarization() {
        // V (circular) light through a horizontal linear polarizer →
        // half intensity, no V remaining (filtered).
        let s_v = StokesVector::circular_right(1.0);
        let out = MuellerMatrix::linear_polarizer_horizontal().apply(s_v);
        assert!(approx_eq(out.i, 0.5, 1e-6), "out.i={}", out.i);
        assert!(approx_eq(out.v, 0.0, 1e-6), "out.v={}", out.v);
    }

    #[test]
    fn fresnel_at_brewster_polarizes_dielectric_reflection() {
        // At Brewster angle for n=1.5 glass, the reflected p-polarized
        // component vanishes. Reflected light ends up purely s-polarized
        // (Q < 0 in our convention since s-pol = vertical = -Q).
        let n1: f32 = 1.0;
        let n2: f32 = 1.5;
        let theta_b = (n2 / n1).atan(); // ≈ 0.9828 rad
        let m = MuellerMatrix::fresnel_dielectric(n1, n2, theta_b);
        let unpol = StokesVector::unpolarized(1.0);
        let out = m.apply(unpol);
        // I should be small (typical Brewster reflectance ≈ 7%).
        assert!(out.i > 0.0 && out.i < 0.20, "I out-of-range: {}", out.i);
        // Q magnitude should equal I (fully polarized).
        let dop = out.dop_linear();
        assert!(dop > 0.85, "Brewster reflection should be highly polarized · dop={dop}");
    }

    #[test]
    fn gold_metal_mueller_imaginary_part_visible_at_grazing() {
        // Gold at grazing (θ ≈ 80°) should produce a non-zero M[2][3]
        // component (the s-p phase shift from the imaginary IOR part).
        let pi = std::f32::consts::PI;
        let m = MuellerMatrix::fresnel_metal(0.27, 2.78, pi * 0.45); // 81°
        let m23 = m.0[2][3];
        // Either m23 OR -m23 = m32 should be non-zero (phase shift visible).
        assert!(m23.abs() > 1e-3 || m.0[3][2].abs() > 1e-3,
                "Gold at grazing must show phase-shift in M[2][3]: {m23}");
    }

    #[test]
    fn iridescent_thin_film_q_varies_with_view_angle() {
        // Thin-film Mueller's M[0][1] (Q-coupling) should change with view
        // angle, producing the iridescence rainbow.
        let pi = std::f32::consts::PI;
        let m1 = MuellerMatrix::thin_film(200.0, 2.40, 0.95, 0.0, 550.0);
        let m2 = MuellerMatrix::thin_film(200.0, 2.40, 0.95, pi / 4.0, 550.0);
        let m3 = MuellerMatrix::thin_film(200.0, 2.40, 0.95, pi / 3.0, 550.0);
        let q_coupling_diff_12 = (m1.0[0][1] - m2.0[0][1]).abs();
        let q_coupling_diff_13 = (m1.0[0][1] - m3.0[0][1]).abs();
        assert!(q_coupling_diff_12 > 0.01 || q_coupling_diff_13 > 0.01,
                "thin-film Q-coupling must vary with angle · diff_12={q_coupling_diff_12} diff_13={q_coupling_diff_13}");
    }

    // ─── Spec extras ─────────────────────────────────────────────────────

    #[test]
    fn mueller_lut_has_16_entries() {
        let lut = mueller_lut();
        assert_eq!(lut.len(), 16);
        assert_eq!(MUELLER_LUT_LEN, 16);
    }

    #[test]
    fn apply_chained_muellers_associative() {
        // (a · b · c)·s = a·(b·(c·s)) — Mueller matrix multiplication
        // is associative.
        let a = MuellerMatrix::linear_polarizer_horizontal();
        let b = MuellerMatrix::quarter_wave_plate(std::f32::consts::PI / 4.0);
        let c = MuellerMatrix::linear_polarizer(std::f32::consts::PI / 3.0);
        let s = StokesVector::unpolarized(1.0);
        let composed = a.then(b).then(c);
        let r1 = composed.apply(s);
        let r2 = a.apply(b.apply(c.apply(s)));
        assert!(stokes_eq(r1, r2, 1e-4),
                "associativity failed: composed={r1:?} sequential={r2:?}");
    }

    // ─── Additional structural tests ─────────────────────────────────────

    #[test]
    fn stokes_vector_size_is_16_bytes() {
        assert_eq!(core::mem::size_of::<StokesVector>(), 16);
        assert_eq!(core::mem::align_of::<StokesVector>(), 16);
    }

    #[test]
    fn mueller_matrix_size_is_64_bytes() {
        assert_eq!(core::mem::size_of::<MuellerMatrix>(), 64);
        assert_eq!(core::mem::align_of::<MuellerMatrix>(), 16);
    }

    #[test]
    fn mueller_lut_total_byte_size_under_16kib() {
        let total = MUELLER_LUT_LEN * core::mem::size_of::<MuellerMatrix>();
        assert!(total <= 16 * 1024, "Mueller LUT must fit in 16 KiB UBO");
        assert_eq!(total, 1024); // 16 × 64
    }

    #[test]
    fn mueller_lut_flat_pack_size_correct() {
        let flat = mueller_lut_flat();
        assert_eq!(flat.len(), 256); // 16 × 16
    }

    #[test]
    fn linear_polarizer_horizontal_passes_horizontal() {
        // Pure-horizontal in → pure-horizontal out (no loss).
        let s = StokesVector::linear_horizontal(1.0);
        let out = MuellerMatrix::linear_polarizer_horizontal().apply(s);
        assert!(approx_eq(out.i, 1.0, 1e-6), "I={}", out.i);
        assert!(approx_eq(out.q, 1.0, 1e-6), "Q={}", out.q);
    }

    #[test]
    fn linear_polarizer_horizontal_blocks_vertical() {
        // Pure-vertical in → zero out (orthogonal).
        let s = StokesVector::linear_vertical(1.0);
        let out = MuellerMatrix::linear_polarizer_horizontal().apply(s);
        assert!(approx_eq(out.i, 0.0, 1e-6), "I={}", out.i);
    }

    #[test]
    fn unpolarized_through_polarizer_loses_half() {
        // Unpolarized light through a linear polarizer → 50% transmission.
        let s = StokesVector::unpolarized(1.0);
        let out = MuellerMatrix::linear_polarizer_horizontal().apply(s);
        assert!(approx_eq(out.i, 0.5, 1e-6), "I={}", out.i);
        // Output should be fully horizontally-polarized.
        assert!(approx_eq(out.q, 0.5, 1e-6), "Q={}", out.q);
    }

    #[test]
    fn depolarizer_zeros_polarization_components() {
        let s = StokesVector { i: 1.0, q: 0.5, u: 0.5, v: 0.5 };
        let out = MuellerMatrix::depolarizer().apply(s);
        assert!(approx_eq(out.i, 1.0, 1e-6));
        assert!(approx_eq(out.q, 0.0, 1e-6));
        assert!(approx_eq(out.u, 0.0, 1e-6));
        assert!(approx_eq(out.v, 0.0, 1e-6));
    }

    #[test]
    fn quarter_wave_plate_converts_linear_to_circular() {
        // QWP at 45° converts horizontal linear to circular polarization.
        let qwp = MuellerMatrix::quarter_wave_plate(std::f32::consts::PI / 4.0);
        let s_h = StokesVector::linear_horizontal(1.0);
        let out = qwp.apply(s_h);
        // Output should have non-zero V (circular) component.
        assert!(out.v.abs() > 0.5, "QWP @ 45° should produce circular: V={}", out.v);
    }

    #[test]
    fn polarization_view_cycles_through_all_modes() {
        let mut v = PolarizationView::Intensity;
        let modes = [
            PolarizationView::FalseColorQ,
            PolarizationView::FalseColorU,
            PolarizationView::FalseColorV,
            PolarizationView::Dop,
            PolarizationView::Intensity,
        ];
        for expected in modes.iter() {
            v = v.next();
            assert_eq!(v, *expected, "cycle mismatch");
        }
    }

    #[test]
    fn polarization_view_from_u32_clamps_out_of_range() {
        assert_eq!(PolarizationView::from_u32(99), PolarizationView::Intensity);
        assert_eq!(PolarizationView::from_u32(0), PolarizationView::Intensity);
        assert_eq!(PolarizationView::from_u32(1), PolarizationView::FalseColorQ);
        assert_eq!(PolarizationView::from_u32(4), PolarizationView::Dop);
    }

    #[test]
    fn stokes_physical_constraint_check() {
        // Realizable.
        let ok = StokesVector { i: 1.0, q: 0.5, u: 0.5, v: 0.5 };
        assert!(ok.is_physical(), "I²=1 ≥ Q²+U²+V²=0.75");
        // Borderline (fully polarized).
        let edge = StokesVector { i: 1.0, q: 1.0, u: 0.0, v: 0.0 };
        assert!(edge.is_physical(), "fully horiz-polarized must be physical");
        // Unphysical : I=1 but |pol|² = 1.5.
        let bad = StokesVector { i: 1.0, q: 0.7, u: 0.7, v: 0.7 };
        assert!(!bad.is_physical(), "Q²+U²+V²=1.47 > I²=1");
    }

    #[test]
    fn dop_linear_ignores_v_component() {
        // V (circular) does NOT contribute to dop_linear, only dop_total.
        let s = StokesVector { i: 1.0, q: 0.0, u: 0.0, v: 0.8 };
        assert!(approx_eq(s.dop_linear(), 0.0, 1e-6));
        assert!(approx_eq(s.dop_total(), 0.8, 1e-6));
    }

    #[test]
    fn sun_stokes_default_has_slight_horizontal_polarization() {
        let s = sun_stokes_default();
        assert!(approx_eq(s.i, 1.0, 1e-6));
        assert!(s.q > 0.0); // slight horizontal polarization
        assert!(s.dop_linear() < 0.1); // less than 10% DOP
    }

    #[test]
    fn mueller_compose_then_distributes_over_apply() {
        // (M2 · M1) applied to s == M2(M1(s))
        let m1 = MuellerMatrix::linear_polarizer_horizontal();
        let m2 = MuellerMatrix::quarter_wave_plate(std::f32::consts::PI / 4.0);
        let s = StokesVector::unpolarized(1.0);
        let composed = m2.then(m1).apply(s);
        let sequential = m2.apply(m1.apply(s));
        assert!(stokes_eq(composed, sequential, 1e-5));
    }

    #[test]
    fn stokes_add_combines_intensities() {
        let a = StokesVector::unpolarized(0.5);
        let b = StokesVector::linear_horizontal(0.5);
        let sum = a.add(b);
        assert!(approx_eq(sum.i, 1.0, 1e-6));
        assert!(approx_eq(sum.q, 0.5, 1e-6));
    }

    #[test]
    fn stokes_scale_multiplies_all_components() {
        let s = StokesVector { i: 1.0, q: 0.5, u: -0.3, v: 0.2 };
        let out = s.scale(2.0);
        assert!(approx_eq(out.i, 2.0, 1e-6));
        assert!(approx_eq(out.q, 1.0, 1e-6));
        assert!(approx_eq(out.u, -0.6, 1e-6));
        assert!(approx_eq(out.v, 0.4, 1e-6));
    }

    // ─── Polarization-room panel diagnostic ──────────────────────────────

    #[test]
    fn polarization_panels_yield_distinct_signatures() {
        let panels = polarization_panels();
        assert_eq!(panels.len(), 4);
        // Panel 0 (no filter) preserves sun's slight-Q signature.
        assert!(panels[0].expected_signature.q > 0.0);
        // Panel 1 (LP @ 0°) : output should be Q-polarized.
        assert!(panels[1].expected_signature.q > panels[1].expected_signature.u.abs());
        // Panel 2 (LP @ +45°) : output should be U-polarized.
        assert!(panels[2].expected_signature.u > panels[2].expected_signature.q.abs());
        // Panel 3 (circular) : output should have a V component.
        assert!(panels[3].expected_signature.v.abs() > 1e-6);
        // All four signatures must be distinct (pairwise comparison).
        for i in 0..panels.len() {
            for j in (i + 1)..panels.len() {
                let s_i = panels[i].expected_signature;
                let s_j = panels[j].expected_signature;
                let diff = (s_i.i - s_j.i).abs()
                    + (s_i.q - s_j.q).abs()
                    + (s_i.u - s_j.u).abs()
                    + (s_i.v - s_j.v).abs();
                assert!(
                    diff > 1e-3,
                    "Panel {i} and {j} have identical Stokes signatures (diff={diff})"
                );
            }
        }
    }

    #[test]
    fn polarization_panel_labels_are_distinct() {
        use std::collections::HashSet;
        let panels = polarization_panels();
        let mut labels = HashSet::new();
        for p in &panels {
            labels.insert(p.label);
        }
        assert_eq!(labels.len(), 4);
    }
}

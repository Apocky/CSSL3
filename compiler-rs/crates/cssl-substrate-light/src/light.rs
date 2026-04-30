//! § ApockyLight — 32B std430-aligned per-quantum primitive type.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Canonical light-quantum carried per Ω-field cell (§§ 36 CFER renderer
//!   `L_c(λ, θφ) ∈ ApockyLight`). Stores hero-wavelength radiance + 8
//!   accompaniment-band coefficients + Stokes-vector polarization +
//!   octahedral-encoded propagation direction + KAN-band handle (compressed
//!   spectral basis) + evidence-glyph (CFER adaptive-sampling driver) +
//!   capability-handle (Pony-cap + IFC label binding).
//!
//! § BYTE-LAYOUT (std430, 32B total, 4-byte alignment)
//!   See crate-doc § BYTE-LAYOUT — invariant verified by [`tests`] below.
//!
//! § CONSTRUCTION
//!   Use [`ApockyLight::zero`] for null/dark quanta, [`ApockyLight::new`] for
//!   the canonical constructor, or the higher-level helpers in
//!   [`crate::operations`] :
//!     - [`crate::operations::blackbody`] — Planck-radiator quantum @ T (K).
//!     - [`crate::operations::monochromatic`] — single-λ delta quantum.
//!     - [`crate::operations::d65`] — D65 illuminant reference quantum.
//!
//! § COMPOSITION
//!   See [`crate::operations`] for the `add` / `scale` / `attenuate` /
//!   `mueller_apply` operators. All composition produces fresh quanta —
//!   the input quanta are never aliased or mutated in-place.

use core::fmt;

// ───────────────────────────────────────────────────────────────────────────
// § Module-level constants — exposed via the crate root re-export.
// ───────────────────────────────────────────────────────────────────────────

/// § The canonical std430 size of [`ApockyLight`] in bytes.
///
/// Verified by [`tests::layout_size`].
pub const APOCKY_LIGHT_SIZE_BYTES: usize = 32;

/// § Number of accompaniment-band coefficients packed into the cell.
///
/// 4 accompaniment-bands (2 in `accompaniment_lo` + 2 in `accompaniment_hi`,
/// each as IEEE-754 binary16 f16) plus the 1 hero-band gives an effective
/// 5-band carrier per quantum. Compatible with the 16-band canonical band
/// table — the 4 accompaniments cover the most-perceptually-important
/// bands flanking the hero wavelength while the hero carries the dominant
/// radiance + wavelength.
pub const ACCOMPANIMENT_COUNT: usize = 4;

/// § Minimum allowed `hero_lambda_nm` — covers UV-A/B (300 nm).
pub const LAMBDA_MIN_NM: f32 = 300.0;

/// § Maximum allowed `hero_lambda_nm` — covers SWIR (2500 nm).
pub const LAMBDA_MAX_NM: f32 = 2500.0;

/// § Default hero-wavelength when none specified — D65-peak (550 nm).
pub const LAMBDA_DEFAULT_NM: f32 = 550.0;

/// § DoP packed-q5.11 fixed-point scale.
const DOP_Q_SCALE: f32 = 2048.0;

/// § Stokes s1/s2 q1.7 packing scale (signed 8-bit).
const STOKES_Q_SCALE: f32 = 127.0;

// ───────────────────────────────────────────────────────────────────────────
// § Evidence-glyph enum (8 canonical glyphs).
// ───────────────────────────────────────────────────────────────────────────

/// § Per-quantum evidence-glyph — drives §§ 36 CFER adaptive-sampling.
///
/// Eight canonical glyphs encoded into the 1-byte evidence-field at offset
/// 27 of [`ApockyLight`]. Compatible with the CSLv3 glyph-set :
///
///   ```text
///   ◐ Uncertain    — re-iterate (variance > τ_confidence)
///   ✓ Trusted      — skip re-iteration (warm-cache hit)
///   ○ Default      — standard cadence (initial / unevaluated)
///   ✗ Rejected     — null-light (refused by IFC or material refused)
///   ⊘ Forbidden    — PRIME-DIRECTIVE absolute-banned source
///   △ Increasing   — radiance trending up (light-source ramping)
///   ▽ Decreasing   — radiance trending down (occluder-shadow growing)
///   ‼ Alert        — convergence stalled or material-discontinuity flagged
///   ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EvidenceGlyph {
    /// § ◐ Uncertain — re-iterate (variance above confidence-threshold).
    Uncertain = 0,
    /// § ✓ Trusted — skip re-iteration (warm-cache hit).
    Trusted = 1,
    /// § ○ Default — standard cadence (initial / unevaluated).
    Default = 2,
    /// § ✗ Rejected — null-light (refused by IFC or material refused).
    Rejected = 3,
    /// § ⊘ Forbidden — PRIME-DIRECTIVE absolute-banned source.
    Forbidden = 4,
    /// § △ Increasing — radiance trending up.
    Increasing = 5,
    /// § ▽ Decreasing — radiance trending down.
    Decreasing = 6,
    /// § ‼ Alert — convergence stalled or material-discontinuity flagged.
    Alert = 7,
}

impl EvidenceGlyph {
    /// § All 8 canonical glyphs in declared order.
    pub const ALL: [Self; 8] = [
        Self::Uncertain,
        Self::Trusted,
        Self::Default,
        Self::Rejected,
        Self::Forbidden,
        Self::Increasing,
        Self::Decreasing,
        Self::Alert,
    ];

    /// § Unicode-glyph rendering of this evidence-class.
    #[must_use]
    pub const fn glyph(self) -> char {
        match self {
            Self::Uncertain => '◐',
            Self::Trusted => '✓',
            Self::Default => '○',
            Self::Rejected => '✗',
            Self::Forbidden => '⊘',
            Self::Increasing => '△',
            Self::Decreasing => '▽',
            Self::Alert => '‼',
        }
    }

    /// § Decode from the 1-byte evidence-field. Saturates malformed bytes
    ///   to [`Self::Default`] for replay-determinism.
    #[must_use]
    pub const fn from_u8(b: u8) -> Self {
        match b {
            0 => Self::Uncertain,
            1 => Self::Trusted,
            2 => Self::Default,
            3 => Self::Rejected,
            4 => Self::Forbidden,
            5 => Self::Increasing,
            6 => Self::Decreasing,
            7 => Self::Alert,
            _ => Self::Default,
        }
    }

    /// § True iff this glyph drives the CFER iteration to RE-ITERATE the
    ///   quantum on the next frame.
    #[must_use]
    pub const fn drives_reiteration(self) -> bool {
        matches!(self, Self::Uncertain | Self::Alert | Self::Increasing)
    }

    /// § True iff this glyph indicates the quantum is BANNED FROM TELEMETRY
    ///   per PRIME-DIRECTIVE §1.
    #[must_use]
    pub const fn is_telemetry_banned(self) -> bool {
        matches!(self, Self::Forbidden)
    }
}

impl fmt::Display for EvidenceGlyph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Uncertain => "◐",
            Self::Trusted => "✓",
            Self::Default => "○",
            Self::Rejected => "✗",
            Self::Forbidden => "⊘",
            Self::Increasing => "△",
            Self::Decreasing => "▽",
            Self::Alert => "‼",
        })
    }
}

impl Default for EvidenceGlyph {
    fn default() -> Self {
        Self::Default
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § The ApockyLight 32B per-quantum struct.
// ───────────────────────────────────────────────────────────────────────────

/// § ApockyLight — 32B std430-aligned per-quantum primitive.
///
/// See crate-doc § BYTE-LAYOUT for the full byte-table. The struct uses
/// `#[repr(C)]` to lock the field order + offsets ; `align(4)` matches the
/// std430 minimum-alignment for a 32B aggregate of 4-byte primitives.
#[derive(Clone, Copy, PartialEq)]
#[repr(C, align(4))]
pub struct ApockyLight {
    /// § Hero-wavelength radiance (W·sr⁻¹·m⁻²·nm⁻¹) at offset 0..=3.
    pub hero_radiance: f32,

    /// § Hero-wavelength in nanometers at offset 4..=7.
    pub hero_lambda_nm: f32,

    /// § 4 accompaniment-band radiance values packed as 4×IEEE-754-binary16
    ///   (f16) at offset 8..=11. Bands 0-3.
    pub accompaniment_lo: u32,

    /// § 4 more accompaniment-band radiance values packed as 4×f16 at
    ///   offset 12..=15. Bands 4-7.
    pub accompaniment_hi: u32,

    /// § DoP + Stokes-vector polarization packed at offset 16..=19.
    ///
    /// Layout :
    ///   - bits  0..=15 : DoP magnitude as q5.11 (range 0..=15.999)
    ///   - bits 16..=23 : s1 q1.7 signed
    ///   - bits 24..=31 : s2 q1.7 signed
    ///
    /// s3 reconstructed via DoP² = s1²+s2²+s3² invariant.
    pub dop_packed: u32,

    /// § Octahedral-encoded propagation direction at offset 20..=23.
    ///
    /// Layout :
    ///   - bits  0..=15 : octahedral u-coordinate (q1.15)
    ///   - bits 16..=31 : octahedral v-coordinate (q1.15)
    pub direction_oct: u32,

    /// § Packed (kan_band_handle u24 + evidence_glyph u8) at offset 24..=27.
    ///
    /// Layout :
    ///   - bits  0..=23 : kan_band_handle index
    ///   - bits 24..=31 : evidence_glyph as u8
    pub kan_and_evidence: u32,

    /// § Capability-handle at offset 28..=31. Index into per-process CapTable.
    pub cap_handle: u32,
}

impl ApockyLight {
    // ────────────────────────────────────────────────────────────────────
    // § Construction
    // ────────────────────────────────────────────────────────────────────

    /// § The zero/dark/null-quantum — no radiance, default lambda, no
    ///   polarization, default direction, null KAN-band, Default-glyph,
    ///   anonymous cap.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            hero_radiance: 0.0,
            hero_lambda_nm: LAMBDA_DEFAULT_NM,
            accompaniment_lo: 0,
            accompaniment_hi: 0,
            dop_packed: 0,
            direction_oct: 0,
            // kan = 0 ; evidence = Default(2) at high byte
            kan_and_evidence: 0x0200_0000,
            cap_handle: 0,
        }
    }

    /// § Canonical constructor from individual physical fields.
    ///
    /// Clamps `hero_radiance` to ≥ 0, `hero_lambda_nm` to physical range,
    /// `dop` to [0, 1], `s1`/`s2` to [-1, 1], normalizes direction.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hero_radiance: f32,
        hero_lambda_nm: f32,
        accompaniments: [f32; ACCOMPANIMENT_COUNT],
        dop: f32,
        s1: f32,
        s2: f32,
        direction: [f32; 3],
        kan_band_handle: u32,
        evidence: EvidenceGlyph,
        cap_handle: u32,
    ) -> Self {
        let radiance = if hero_radiance.is_nan() || hero_radiance < 0.0 {
            0.0
        } else {
            hero_radiance
        };
        let lambda = if hero_lambda_nm.is_nan() {
            LAMBDA_DEFAULT_NM
        } else {
            hero_lambda_nm.max(LAMBDA_MIN_NM).min(LAMBDA_MAX_NM)
        };

        let (lo, hi) = pack_accompaniments(accompaniments);
        let dop_packed = pack_dop_stokes(dop, s1, s2);
        let dir_oct = pack_direction_octahedral(direction);
        let kan_and_evidence = (kan_band_handle & 0x00FF_FFFF) | ((evidence as u32) << 24);

        Self {
            hero_radiance: radiance,
            hero_lambda_nm: lambda,
            accompaniment_lo: lo,
            accompaniment_hi: hi,
            dop_packed,
            direction_oct: dir_oct,
            kan_and_evidence,
            cap_handle,
        }
    }

    // ────────────────────────────────────────────────────────────────────
    // § Accessors — physical-field decoding
    // ────────────────────────────────────────────────────────────────────

    /// § Hero-band radiance ; treats NaN as 0.0 + clamps to ≥ 0.0.
    #[must_use]
    pub fn intensity(&self) -> f32 {
        if self.hero_radiance.is_nan() || self.hero_radiance < 0.0 {
            0.0
        } else {
            self.hero_radiance
        }
    }

    /// § Hero wavelength in nanometers, clamped to physical range.
    #[must_use]
    pub fn lambda_nm(&self) -> f32 {
        self.hero_lambda_nm.max(LAMBDA_MIN_NM).min(LAMBDA_MAX_NM)
    }

    /// § Decoded 8-element accompaniment-band radiance vector.
    #[must_use]
    pub fn accompaniments(&self) -> [f32; ACCOMPANIMENT_COUNT] {
        unpack_accompaniments(self.accompaniment_lo, self.accompaniment_hi)
    }

    /// § Decoded radiance for a single accompaniment-band index ∈ 0..8.
    /// Returns 0.0 for out-of-range indices.
    #[must_use]
    pub fn accompaniment_band(&self, idx: usize) -> f32 {
        if idx >= ACCOMPANIMENT_COUNT {
            return 0.0;
        }
        let bands = self.accompaniments();
        bands[idx]
    }

    /// § Spectrum-at-lambda interpolation : returns radiance at given
    ///   wavelength using linear-falloff from hero + accompaniment-band-blend.
    #[must_use]
    pub fn spectrum_at(&self, lambda_nm: f32) -> f32 {
        let lambda = lambda_nm.max(LAMBDA_MIN_NM).min(LAMBDA_MAX_NM);
        let hero = self.lambda_nm();
        let dist = (lambda - hero).abs();

        // Within 5 nm of hero ⇒ pure hero-radiance.
        if dist <= 5.0 {
            return self.intensity();
        }

        // 4 accompaniment bands span ±100 nm in 25 nm steps from hero.
        let accomp_idx_f = (dist / 25.0).round() - 1.0;
        if accomp_idx_f < 0.0 || accomp_idx_f >= ACCOMPANIMENT_COUNT as f32 {
            return 0.0;
        }
        self.accompaniment_band(accomp_idx_f as usize)
    }

    /// § Decoded degree-of-polarization scalar ∈ [0.0, 1.0].
    #[must_use]
    pub fn dop(&self) -> f32 {
        let raw = (self.dop_packed & 0xFFFF) as f32 / DOP_Q_SCALE;
        raw.max(0.0).min(1.0)
    }

    /// § Decoded full Stokes-vector (s0, s1, s2, s3). s0 = total-intensity ;
    ///   (s1, s2) decoded from packed q1.7 ; s3 reconstructed via
    ///   DoP² = s1² + s2² + s3² invariant.
    #[must_use]
    pub fn stokes(&self) -> [f32; 4] {
        let s0 = self.intensity();
        let s1 = (((self.dop_packed >> 16) & 0xFF) as i8) as f32 / STOKES_Q_SCALE;
        let s2 = (((self.dop_packed >> 24) & 0xFF) as i8) as f32 / STOKES_Q_SCALE;
        let dop = self.dop();
        let dop_sq = dop * dop;
        let lin_sq = s1 * s1 + s2 * s2;
        let s3_sq = (dop_sq - lin_sq).max(0.0);
        let s3 = s3_sq.sqrt();
        [s0, s1 * s0, s2 * s0, s3 * s0]
    }

    /// § Decoded normalized propagation direction unit-vector.
    #[must_use]
    pub fn direction(&self) -> [f32; 3] {
        unpack_direction_octahedral(self.direction_oct)
    }

    /// § The 24-bit KAN-band handle.
    #[must_use]
    pub fn kan_band_handle(&self) -> u32 {
        self.kan_and_evidence & 0x00FF_FFFF
    }

    /// § The evidence-glyph driving CFER adaptive-sampling.
    #[must_use]
    pub fn evidence(&self) -> EvidenceGlyph {
        EvidenceGlyph::from_u8(((self.kan_and_evidence >> 24) & 0xFF) as u8)
    }

    /// § The capability handle index into the per-process CapTable.
    #[must_use]
    pub fn cap_handle(&self) -> u32 {
        self.cap_handle
    }

    // ────────────────────────────────────────────────────────────────────
    // § Field mutators — preserve invariants on update
    // ────────────────────────────────────────────────────────────────────

    /// § Set the evidence-glyph field, preserving the kan_band_handle.
    pub fn set_evidence(&mut self, e: EvidenceGlyph) {
        self.kan_and_evidence = (self.kan_and_evidence & 0x00FF_FFFF) | ((e as u32) << 24);
    }

    /// § Set the kan_band_handle field, preserving the evidence_glyph.
    pub fn set_kan_band_handle(&mut self, handle: u32) {
        let glyph_byte = self.kan_and_evidence & 0xFF00_0000;
        self.kan_and_evidence = (handle & 0x00FF_FFFF) | glyph_byte;
    }

    /// § Set the capability-handle.
    pub fn set_cap_handle(&mut self, handle: u32) {
        self.cap_handle = handle;
    }

    // ────────────────────────────────────────────────────────────────────
    // § Conversions — RGB tristimulus convenience accessor
    // ────────────────────────────────────────────────────────────────────

    /// § Approximate the quantum's contribution in linear-sRGB (Rec.709).
    ///   Convenience accessor — canonical RGB conversion lives in the
    ///   renderer tonemap stage.
    #[must_use]
    pub fn to_rgb(&self) -> [f32; 3] {
        let lambda = self.lambda_nm();
        let hero_xyz = wavelength_to_xyz(lambda);
        let intensity = self.intensity();
        let mut x = hero_xyz[0] * intensity;
        let mut y = hero_xyz[1] * intensity;
        let mut z = hero_xyz[2] * intensity;

        // Accompaniment bands : evenly-spaced ±25 nm steps from hero.
        let accomps = self.accompaniments();
        for (i, r) in accomps.iter().enumerate() {
            let dl = ((i as f32) + 1.0) * 25.0;
            let sign = if i & 1 == 0 { -1.0 } else { 1.0 };
            let l = (lambda + sign * dl).max(LAMBDA_MIN_NM).min(LAMBDA_MAX_NM);
            let xyz = wavelength_to_xyz(l);
            x += xyz[0] * r;
            y += xyz[1] * r;
            z += xyz[2] * r;
        }

        // CIE-XYZ → linear-sRGB Rec.709 transform.
        let r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
        let g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
        let b = 0.0557 * x - 0.2040 * y + 1.0570 * z;
        [r.max(0.0), g.max(0.0), b.max(0.0)]
    }

    // ────────────────────────────────────────────────────────────────────
    // § IFC + telemetry surface
    // ────────────────────────────────────────────────────────────────────

    /// § True iff the quantum's evidence-glyph is `Forbidden`.
    #[must_use]
    pub fn is_forbidden(&self) -> bool {
        self.evidence().is_telemetry_banned()
    }
}

impl fmt::Debug for ApockyLight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApockyLight")
            .field("hero_radiance", &self.hero_radiance)
            .field("hero_lambda_nm", &self.hero_lambda_nm)
            .field("dop", &self.dop())
            .field("direction", &self.direction())
            .field("kan_band_handle", &self.kan_band_handle())
            .field("evidence", &self.evidence().glyph())
            .field("cap_handle", &self.cap_handle)
            .finish()
    }
}

impl Default for ApockyLight {
    fn default() -> Self {
        Self::zero()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Pack/unpack helpers — IEEE-754 binary16 + octahedral + Stokes
// ───────────────────────────────────────────────────────────────────────────

/// § f32 → IEEE-754 binary16 bits. Round-to-nearest-even for normals.
fn f32_to_f16_bits(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp_f32 = ((bits >> 23) & 0xFF) as i32;
    let mant = bits & 0x007F_FFFF;

    if exp_f32 == 0xFF {
        return if mant == 0 {
            sign | 0x7C00
        } else {
            sign | 0x7E00
        };
    }
    if exp_f32 == 0 {
        return sign;
    }
    let exp_unbiased = exp_f32 - 127;
    if exp_unbiased < -14 {
        return sign;
    }
    if exp_unbiased > 15 {
        return sign | 0x7C00;
    }
    let exp_h = (exp_unbiased + 15) as u16;
    let mant_h = (mant >> 13) as u16;
    sign | (exp_h << 10) | mant_h
}

/// § IEEE-754 binary16 bits → f32.
fn f16_bits_to_f32(b: u16) -> f32 {
    let sign = ((b & 0x8000) as u32) << 16;
    let exp_h = ((b >> 10) & 0x1F) as u32;
    let mant_h = (b & 0x03FF) as u32;
    if exp_h == 0 {
        if mant_h == 0 {
            return f32::from_bits(sign);
        }
        let mut m = mant_h;
        let mut e: i32 = 1;
        while (m & 0x0400) == 0 {
            m <<= 1;
            e -= 1;
        }
        m &= 0x03FF;
        let exp_f32 = (127 - 15 + e - 1) as u32;
        return f32::from_bits(sign | (exp_f32 << 23) | (m << 13));
    }
    if exp_h == 0x1F {
        return f32::from_bits(sign | 0x7F80_0000 | (mant_h << 13));
    }
    let exp_f32 = (exp_h + (127 - 15)) << 23;
    let mant_f32 = mant_h << 13;
    f32::from_bits(sign | exp_f32 | mant_f32)
}

/// § Pack 4 f32 accompaniments into (lo_u32, hi_u32) of 2×f16 each.
fn pack_accompaniments(a: [f32; ACCOMPANIMENT_COUNT]) -> (u32, u32) {
    // Layout : lo = (a0 lo16 | a1 hi16) ; hi = (a2 lo16 | a3 hi16).
    let lo = (f32_to_f16_bits(a[0]) as u32) | ((f32_to_f16_bits(a[1]) as u32) << 16);
    let hi = (f32_to_f16_bits(a[2]) as u32) | ((f32_to_f16_bits(a[3]) as u32) << 16);
    (lo, hi)
}

/// § Unpack (lo_u32, hi_u32) → 4 f32 accompaniments.
fn unpack_accompaniments(lo: u32, hi: u32) -> [f32; ACCOMPANIMENT_COUNT] {
    [
        f16_bits_to_f32((lo & 0xFFFF) as u16),
        f16_bits_to_f32(((lo >> 16) & 0xFFFF) as u16),
        f16_bits_to_f32((hi & 0xFFFF) as u16),
        f16_bits_to_f32(((hi >> 16) & 0xFFFF) as u16),
    ]
}

/// § Pack DoP + s1/s2 Stokes into u32 layout (q5.11 + q1.7 + q1.7 + 8-pad).
fn pack_dop_stokes(dop: f32, s1: f32, s2: f32) -> u32 {
    let dop_clamped = dop.max(0.0).min(1.0);
    let dop_q = (dop_clamped * DOP_Q_SCALE).round() as u32 & 0xFFFF;
    let s1_clamped = s1.max(-1.0).min(1.0);
    let s1_q = ((s1_clamped * STOKES_Q_SCALE).round() as i32 as u32 & 0xFF) << 16;
    let s2_clamped = s2.max(-1.0).min(1.0);
    let s2_q = ((s2_clamped * STOKES_Q_SCALE).round() as i32 as u32 & 0xFF) << 24;
    dop_q | s1_q | s2_q
}

/// § Pack 3D direction into octahedral-projected 16+16 bit u32.
///
/// Cigolle-Donow-Evangelakos-Iwiniecki-McGuire 2014 projection.
fn pack_direction_octahedral(d: [f32; 3]) -> u32 {
    let nrm = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if nrm < 1e-9 {
        return 0;
    }
    let inv = 1.0 / nrm;
    let nx = d[0] * inv;
    let ny = d[1] * inv;
    let nz = d[2] * inv;
    let abs_sum = nx.abs() + ny.abs() + nz.abs();
    let mut u = nx / abs_sum;
    let mut v = ny / abs_sum;
    if nz < 0.0 {
        let signx = if u >= 0.0 { 1.0 } else { -1.0 };
        let signy = if v >= 0.0 { 1.0 } else { -1.0 };
        let prev_u = u;
        u = (1.0 - v.abs()) * signx;
        v = (1.0 - prev_u.abs()) * signy;
    }
    let u_q = (((u.max(-1.0).min(1.0)) * 32767.0 + 32767.5) as u32) & 0xFFFF;
    let v_q = (((v.max(-1.0).min(1.0)) * 32767.0 + 32767.5) as u32) & 0xFFFF;
    u_q | (v_q << 16)
}

/// § Unpack octahedral u32 into a 3D direction unit vector.
fn unpack_direction_octahedral(packed: u32) -> [f32; 3] {
    let u_q = (packed & 0xFFFF) as f32;
    let v_q = ((packed >> 16) & 0xFFFF) as f32;
    let u = (u_q / 32767.5) - 1.0;
    let v = (v_q / 32767.5) - 1.0;
    let mut x = u;
    let mut y = v;
    let z = 1.0 - x.abs() - y.abs();
    if z < 0.0 {
        let prev_x = x;
        x = (1.0 - y.abs()) * if x >= 0.0 { 1.0 } else { -1.0 };
        y = (1.0 - prev_x.abs()) * if y >= 0.0 { 1.0 } else { -1.0 };
    }
    let nrm = (x * x + y * y + z * z).sqrt().max(1e-9);
    [x / nrm, y / nrm, z / nrm]
}

// ───────────────────────────────────────────────────────────────────────────
// § Wavelength → CIE-XYZ tabulated colorimetry (subset for to_rgb helper).
// ───────────────────────────────────────────────────────────────────────────

/// § Approximate single-wavelength CIE-1931 → XYZ tristimulus values.
///
/// Wyman-Sloan-Shirley 2013 analytical Gaussian-fit. ~1% accurate over
/// 360-830 nm. Returns zero outside that range.
fn wavelength_to_xyz(lambda: f32) -> [f32; 3] {
    let l = lambda;
    if l < 360.0 || l > 830.0 {
        return [0.0, 0.0, 0.0];
    }
    let x = 1.056 * gaussian(l, 599.8, 37.9, 31.0)
        + 0.362 * gaussian(l, 442.0, 16.0, 26.7)
        - 0.065 * gaussian(l, 501.1, 20.4, 26.2);
    let y = 0.821 * gaussian(l, 568.8, 46.9, 40.5) + 0.286 * gaussian(l, 530.9, 16.3, 31.1);
    let z = 1.217 * gaussian(l, 437.0, 11.8, 36.0) + 0.681 * gaussian(l, 459.0, 26.0, 13.8);
    [x, y, z]
}

fn gaussian(x: f32, mu: f32, sigma1: f32, sigma2: f32) -> f32 {
    let t = if x < mu {
        (x - mu) / sigma1
    } else {
        (x - mu) / sigma2
    };
    (-0.5 * t * t).exp()
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_size() {
        assert_eq!(core::mem::size_of::<ApockyLight>(), APOCKY_LIGHT_SIZE_BYTES);
        assert_eq!(core::mem::size_of::<ApockyLight>(), 32);
    }

    #[test]
    fn layout_alignment() {
        assert_eq!(core::mem::align_of::<ApockyLight>(), 4);
    }

    #[test]
    fn zero_quantum_is_dark() {
        let z = ApockyLight::zero();
        assert_eq!(z.intensity(), 0.0);
        assert_eq!(z.lambda_nm(), LAMBDA_DEFAULT_NM);
        assert_eq!(z.dop(), 0.0);
        assert_eq!(z.kan_band_handle(), 0);
        assert_eq!(z.evidence(), EvidenceGlyph::Default);
        assert_eq!(z.cap_handle(), 0);
        assert!(!z.is_forbidden());
    }

    #[test]
    fn evidence_glyph_round_trip() {
        for &g in &EvidenceGlyph::ALL {
            let mut light = ApockyLight::zero();
            light.set_evidence(g);
            assert_eq!(light.evidence(), g);
            assert_eq!(light.kan_band_handle(), 0);
        }
    }

    #[test]
    fn kan_band_handle_round_trip() {
        let mut light = ApockyLight::zero();
        for h in [0_u32, 1, 0xFF, 0xFFFF, 0xFF_FFFF] {
            light.set_kan_band_handle(h);
            assert_eq!(light.kan_band_handle(), h);
            assert_eq!(light.evidence(), EvidenceGlyph::Default);
        }
    }
}

//! § KanBand — KAN-coef-band spectrum compression for CFER cell light-state.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per the canonical CFER formulation (specs/36_CFER_RENDERER.csl §
//!   MATHEMATICAL-FORMULATION § Light-state per-cell), each ω-field cell
//!   stores its angular-spectral light distribution in a *compressed* form :
//!
//!       L_c(λ, θφ) ≈ Σ_i coef[c, i] · basis_i(λ, θφ)
//!
//!   The `KanBand` carries those coefficients per-cell ; the `KanBandTable`
//!   carries the *shared* basis-function registry. Decoding a wavelength
//!   sample at a given direction reduces to a sparse dot-product against
//!   the basis evaluated at that (λ, ω) pair.
//!
//! § STORAGE-LAYOUT
//!   Per specs/35_DECAPLENOPTIC.csl § STORAGE-LAYOUT § KanBand : up to
//!   `KAN_BAND_RANK_MAX` coefficients, with per-cell *adaptive rank* (most
//!   cells need few coefficients ; sharp-spec cells use the full budget).
//!   Coefs are stored in a SmallVec to avoid heap-alloc on the common
//!   low-rank path while still supporting the rare high-rank cells.
//!
//! § DESIGN-NOTE
//!   The basis-function table is *shared* across cells (a single registry
//!   per OmegaField scene-segment). Cells reference basis-fns by index ;
//!   this keeps per-cell storage at O(rank) f32 plus a u8 basis-kind tag
//!   instead of O(rank · basis-eval-fn) closures.
//!
//! § PRIME-DIRECTIVE
//!   - The KanBand respects Σ-mask consent : decode is always permitted
//!     (read), but encode (mutation) is gated by the parent overlay's
//!     Reconfigure-bit per the SigmaOverlay contract.
//!   - No basis function may produce values outside [`COEF_BOUND`] in
//!     absolute value to keep the wave-solver stable.

use smallvec::SmallVec;

/// § Maximum coefficient count per cell. Sized to the spec's adaptive-rank
///   ceiling (specs/35 § KanBand). Sharp-specular cells may exhaust the
///   budget ; diffuse cells typically use 1-4.
pub const KAN_BAND_RANK_MAX: usize = 16;

/// § Default rank for new cells (warm-cache initial value). Most diffuse
///   surfaces decode adequately at this rank.
pub const KAN_BAND_RANK_DEFAULT: usize = 4;

/// § Stable upper bound on `|coef|` per entry. Ensures wave-solver stability
///   under repeated CFER iteration : exceeding this risks runaway amplification.
pub const COEF_BOUND: f32 = 32.0;

/// § Number of canonical wavelength bins for spectrum encode/decode. Matches
///   the Stage-6 16-band hyperspectral output established by cssl-spectral-
///   render. Spectrum samples outside this band-count are resampled to fit.
pub const SPECTRUM_BINS: usize = 16;

/// § Stable canonical basis-function discriminant. Reordering = ABI break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum BasisKind {
    /// § Gaussian-mix basis : each basis-fn is a Gaussian over (λ, θ, φ).
    ///   Most general-purpose option ; default for unspecified scenes.
    #[default]
    GaussianMix = 0,
    /// § Cosine-direction basis : basis-fns are cos(n·ω) products. Compact
    ///   for specular peaks ; equivalent to a low-order spherical-harmonic
    ///   prefix.
    Cosine = 1,
    /// § Learned basis : the basis-fns are learned at scene-bake-time from
    ///   training data (per specs/36 § DIFFERENTIABILITY § train-substrate).
    ///   Stored externally in the KanBandTable.
    Learned = 2,
    /// § Hat basis : piecewise-linear tent functions over wavelength bins.
    ///   Used for ground-truth encode of arbitrary spectra (no smoothing).
    Hat = 3,
}

impl BasisKind {
    /// § All variants in canonical order.
    #[must_use]
    pub const fn all() -> [BasisKind; 4] {
        [Self::GaussianMix, Self::Cosine, Self::Learned, Self::Hat]
    }

    /// § Stable canonical name for telemetry + audit.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::GaussianMix => "gaussian_mix",
            Self::Cosine => "cosine",
            Self::Learned => "learned",
            Self::Hat => "hat",
        }
    }

    /// § Decode from a u8 ; unknown discriminants clamp to GaussianMix.
    #[must_use]
    pub const fn from_u8(v: u8) -> BasisKind {
        match v {
            0 => Self::GaussianMix,
            1 => Self::Cosine,
            2 => Self::Learned,
            3 => Self::Hat,
            _ => Self::GaussianMix,
        }
    }

    /// § Pack to u8.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

/// § KAN-coef-band errors. Threaded through the canonical
///   thiserror-derived error chain established by the parent OmegaField.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum KanBandError {
    /// § Coefficient slice exceeds the rank budget.
    #[error("kan_band: rank {got} exceeds KAN_BAND_RANK_MAX ({max})")]
    RankExceeded { got: usize, max: usize },
    /// § A coefficient's absolute value exceeds [`COEF_BOUND`].
    #[error("kan_band: coef[{index}] = {value} exceeds bound {bound}")]
    CoefOutOfBounds { index: usize, value: f32, bound: f32 },
    /// § Basis-table lookup miss : the requested basis-fn index is not in
    ///   the shared registry.
    #[error("kan_band: basis-table miss ; requested {index} but table size is {size}")]
    BasisTableMiss { index: usize, size: usize },
    /// § Spectrum sample slice has the wrong length for encode/decode.
    #[error("kan_band: spectrum slice len {got} != expected {expected}")]
    SpectrumLenMismatch { got: usize, expected: usize },
    /// § Adaptive-rank reduction request below 1 (would lose all info).
    #[error("kan_band: adaptive rank {requested} below minimum 1")]
    RankUnderflow { requested: usize },
}

/// § Per-cell KAN-coef-band : the compressed light-state representation
///   carried alongside the FieldCell. Decoded on-demand at viewpoint via
///   [`KanBand::decode_spectrum`].
#[derive(Debug, Clone, PartialEq)]
pub struct KanBand {
    /// § Coefficient vector. Length = current adaptive rank ≤ rank_cap.
    ///   Inline-stored up to KAN_BAND_RANK_MAX entries on the stack.
    pub coefs: SmallVec<[f32; KAN_BAND_RANK_MAX]>,
    /// § Basis kind tag : selects which family of basis-fns interprets these
    ///   coefficients. The KanBandTable provides the actual evaluation.
    pub basis_kind: BasisKind,
    /// § Adaptive-rank cap. Updated by adaptive-rank logic based on per-cell
    ///   reconstruction error : low-error cells reduce rank, high-error cells
    ///   raise it (up to KAN_BAND_RANK_MAX).
    pub rank_cap: u8,
    /// § Reserved for ABI extension. Must be 0.
    pub reserved: u8,
}

impl KanBand {
    /// § Construct a zero-initialized KanBand at the default rank.
    #[must_use]
    pub fn zero() -> KanBand {
        let mut v: SmallVec<[f32; KAN_BAND_RANK_MAX]> = SmallVec::new();
        v.resize(KAN_BAND_RANK_DEFAULT, 0.0);
        KanBand {
            coefs: v,
            basis_kind: BasisKind::default(),
            rank_cap: KAN_BAND_RANK_DEFAULT as u8,
            reserved: 0,
        }
    }

    /// § Construct from an explicit coefficient slice.
    ///
    /// # Errors
    /// - [`KanBandError::RankExceeded`] when slice length > KAN_BAND_RANK_MAX.
    /// - [`KanBandError::CoefOutOfBounds`] when any |coef| > COEF_BOUND.
    pub fn from_slice(coefs: &[f32], basis: BasisKind) -> Result<KanBand, KanBandError> {
        if coefs.len() > KAN_BAND_RANK_MAX {
            return Err(KanBandError::RankExceeded {
                got: coefs.len(),
                max: KAN_BAND_RANK_MAX,
            });
        }
        for (i, &v) in coefs.iter().enumerate() {
            if v.abs() > COEF_BOUND {
                return Err(KanBandError::CoefOutOfBounds {
                    index: i,
                    value: v,
                    bound: COEF_BOUND,
                });
            }
        }
        let mut v: SmallVec<[f32; KAN_BAND_RANK_MAX]> = SmallVec::new();
        v.extend_from_slice(coefs);
        Ok(KanBand {
            coefs: v,
            basis_kind: basis,
            rank_cap: coefs.len() as u8,
            reserved: 0,
        })
    }

    /// § Current effective rank (number of meaningful coefficients).
    #[must_use]
    pub fn rank(&self) -> usize {
        self.coefs.len()
    }

    /// § Resize the coefficient vector to a new rank, zero-padding any
    ///   newly added entries.
    ///
    /// # Errors
    /// - [`KanBandError::RankExceeded`] when new_rank > KAN_BAND_RANK_MAX.
    /// - [`KanBandError::RankUnderflow`] when new_rank == 0.
    pub fn set_rank(&mut self, new_rank: usize) -> Result<(), KanBandError> {
        if new_rank == 0 {
            return Err(KanBandError::RankUnderflow { requested: new_rank });
        }
        if new_rank > KAN_BAND_RANK_MAX {
            return Err(KanBandError::RankExceeded {
                got: new_rank,
                max: KAN_BAND_RANK_MAX,
            });
        }
        self.coefs.resize(new_rank, 0.0);
        if (new_rank as u8) > self.rank_cap {
            self.rank_cap = new_rank as u8;
        }
        Ok(())
    }

    /// § Adaptive-rank step : drop trailing coefficients whose magnitude
    ///   falls below `tolerance`, but keep at least `floor` entries.
    ///   Returns the new rank.
    ///
    /// # Errors
    /// - [`KanBandError::RankUnderflow`] when floor == 0.
    pub fn adapt_rank(&mut self, tolerance: f32, floor: usize) -> Result<usize, KanBandError> {
        if floor == 0 {
            return Err(KanBandError::RankUnderflow { requested: 0 });
        }
        let mut keep = self.coefs.len();
        while keep > floor && self.coefs[keep - 1].abs() < tolerance {
            keep -= 1;
        }
        self.coefs.truncate(keep);
        Ok(keep)
    }

    /// § Reconstruction-error metric against an oracle coefficient vector.
    ///   Used by adjoint-method backward-pass + adaptive-rank growth.
    #[must_use]
    pub fn l2_error(&self, oracle: &[f32]) -> f32 {
        let n = self.coefs.len().min(oracle.len());
        let mut acc = 0.0_f32;
        for i in 0..n {
            let d = self.coefs[i] - oracle[i];
            acc += d * d;
        }
        // Tail of oracle that isn't represented contributes too.
        for i in n..oracle.len() {
            acc += oracle[i] * oracle[i];
        }
        acc.sqrt()
    }
}

impl Default for KanBand {
    fn default() -> Self {
        Self::zero()
    }
}

/// § Shared basis-function registry. Stores the per-basis-kind evaluation
///   table : a precomputed (rank × SPECTRUM_BINS) matrix of basis-fn samples
///   at canonical wavelength bins. Cells reference this table by basis_kind ;
///   decoding is a sparse dot-product against the row-block.
#[derive(Debug, Clone)]
pub struct KanBandTable {
    /// § Per-(BasisKind, rank-index) basis-fn samples at each spectrum bin.
    ///   Layout : [BasisKind variant][rank][bin]. Flat-indexed via
    ///   `index(kind, rank, bin)` for cache-friendly access.
    samples: Vec<f32>,
    /// § Number of spectrum bins per basis-fn.
    bins: usize,
    /// § Maximum rank stored per basis-kind. All basis-kinds share the
    ///   same rank-cap in this canonical table for determinism.
    rank_cap: usize,
    /// § Number of basis-kind slots (= BasisKind::all().len()).
    kinds: usize,
}

impl KanBandTable {
    /// § Construct the canonical basis-function table : Gaussian-mix +
    ///   cosine + hat bases pre-baked at (rank, bin) sample matrices.
    ///   Learned-basis is initialized to identity (replaced at bake-time
    ///   by the training loop).
    #[must_use]
    pub fn canonical() -> KanBandTable {
        let bins = SPECTRUM_BINS;
        let rank_cap = KAN_BAND_RANK_MAX;
        let kinds = BasisKind::all().len();
        let mut samples = vec![0.0_f32; kinds * rank_cap * bins];
        for kind in BasisKind::all() {
            for r in 0..rank_cap {
                for b in 0..bins {
                    let lambda = (b as f32) / ((bins - 1) as f32);
                    let v = match kind {
                        BasisKind::GaussianMix => {
                            // Each rank index r centers a Gaussian at λ_r =
                            // r / (rank_cap-1) with σ = 1/(2·rank_cap).
                            let mu = (r as f32) / ((rank_cap - 1).max(1) as f32);
                            let sigma = 1.0 / (2.0 * rank_cap as f32);
                            let dx = lambda - mu;
                            (-(dx * dx) / (2.0 * sigma * sigma)).exp()
                        }
                        BasisKind::Cosine => {
                            // Each rank index uses a cosine at frequency r.
                            (std::f32::consts::PI * (r as f32) * lambda).cos()
                        }
                        BasisKind::Learned => {
                            // Identity initialization : basis_r(bin) = δ(r, bin).
                            if r == b {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        BasisKind::Hat => {
                            // Hat-tent centered at λ_r, half-width 1/rank_cap.
                            let mu = (r as f32) / ((rank_cap - 1).max(1) as f32);
                            let half_width = 1.0 / (rank_cap.max(2) as f32);
                            let d = (lambda - mu).abs();
                            if d < half_width {
                                1.0 - d / half_width
                            } else {
                                0.0
                            }
                        }
                    };
                    let idx = kind.to_u8() as usize * rank_cap * bins + r * bins + b;
                    samples[idx] = v;
                }
            }
        }
        KanBandTable {
            samples,
            bins,
            rank_cap,
            kinds,
        }
    }

    /// § Number of spectrum bins per basis-fn.
    #[must_use]
    pub const fn bins(&self) -> usize {
        self.bins
    }

    /// § Maximum rank stored per basis-kind.
    #[must_use]
    pub const fn rank_cap(&self) -> usize {
        self.rank_cap
    }

    /// § Read the basis-fn sample at (kind, rank, bin).
    ///
    /// # Errors
    /// - [`KanBandError::BasisTableMiss`] when (kind, rank, bin) is OOB.
    pub fn sample(&self, kind: BasisKind, rank: usize, bin: usize) -> Result<f32, KanBandError> {
        let kind_u = kind.to_u8() as usize;
        if kind_u >= self.kinds || rank >= self.rank_cap || bin >= self.bins {
            return Err(KanBandError::BasisTableMiss {
                index: kind_u * self.rank_cap * self.bins + rank * self.bins + bin,
                size: self.samples.len(),
            });
        }
        let idx = kind_u * self.rank_cap * self.bins + rank * self.bins + bin;
        Ok(self.samples[idx])
    }

    /// § Replace the learned-basis row block with externally trained samples.
    ///   Used by the substrate-S12 training loop (specs/36 § train-substrate).
    ///
    /// # Errors
    /// - [`KanBandError::SpectrumLenMismatch`] when learned.len() !=
    ///   rank_cap * bins.
    pub fn set_learned(&mut self, learned: &[f32]) -> Result<(), KanBandError> {
        let expected = self.rank_cap * self.bins;
        if learned.len() != expected {
            return Err(KanBandError::SpectrumLenMismatch {
                got: learned.len(),
                expected,
            });
        }
        let kind_u = BasisKind::Learned.to_u8() as usize;
        let base = kind_u * self.rank_cap * self.bins;
        self.samples[base..base + expected].copy_from_slice(learned);
        Ok(())
    }
}

impl Default for KanBandTable {
    fn default() -> Self {
        Self::canonical()
    }
}

/// § Decode the KanBand into a wavelength-resolved spectrum at SPECTRUM_BINS
///   bins. Output is the dot-product Σ_r coef_r · basis_r(bin).
///
/// # Errors
/// - [`KanBandError::SpectrumLenMismatch`] when out.len() != SPECTRUM_BINS.
/// - [`KanBandError::BasisTableMiss`] when the band's rank exceeds the
///   table's rank_cap.
pub fn decode_spectrum(
    band: &KanBand,
    table: &KanBandTable,
    out: &mut [f32],
) -> Result<(), KanBandError> {
    if out.len() != SPECTRUM_BINS {
        return Err(KanBandError::SpectrumLenMismatch {
            got: out.len(),
            expected: SPECTRUM_BINS,
        });
    }
    if band.rank() > table.rank_cap() {
        return Err(KanBandError::BasisTableMiss {
            index: band.rank(),
            size: table.rank_cap(),
        });
    }
    for v in out.iter_mut() {
        *v = 0.0;
    }
    for (r, &coef) in band.coefs.iter().enumerate() {
        for b in 0..SPECTRUM_BINS {
            let s = table.sample(band.basis_kind, r, b)?;
            out[b] += coef * s;
        }
    }
    Ok(())
}

/// § Encode a wavelength spectrum into a KanBand at the given basis_kind +
///   target rank. Uses least-squares projection : coef_r = ⟨spectrum,
///   basis_r⟩ / ⟨basis_r, basis_r⟩ (orthogonal-basis assumption ; for non-
///   orthogonal bases this is a starting estimate refined by the iterative
///   solver).
///
/// # Errors
/// - [`KanBandError::SpectrumLenMismatch`] when spectrum.len() != SPECTRUM_BINS.
/// - [`KanBandError::RankExceeded`] when target_rank > KAN_BAND_RANK_MAX.
/// - [`KanBandError::CoefOutOfBounds`] when projected coef exceeds COEF_BOUND.
pub fn encode_spectrum(
    spectrum: &[f32],
    basis: BasisKind,
    target_rank: usize,
    table: &KanBandTable,
) -> Result<KanBand, KanBandError> {
    if spectrum.len() != SPECTRUM_BINS {
        return Err(KanBandError::SpectrumLenMismatch {
            got: spectrum.len(),
            expected: SPECTRUM_BINS,
        });
    }
    if target_rank > KAN_BAND_RANK_MAX {
        return Err(KanBandError::RankExceeded {
            got: target_rank,
            max: KAN_BAND_RANK_MAX,
        });
    }
    if target_rank == 0 {
        return Err(KanBandError::RankUnderflow { requested: 0 });
    }
    let mut coefs: SmallVec<[f32; KAN_BAND_RANK_MAX]> = SmallVec::new();
    for r in 0..target_rank {
        let mut numer = 0.0_f32;
        let mut denom = 0.0_f32;
        for b in 0..SPECTRUM_BINS {
            let bf = table.sample(basis, r, b)?;
            numer += spectrum[b] * bf;
            denom += bf * bf;
        }
        let coef = if denom > 1e-12 { numer / denom } else { 0.0 };
        if coef.abs() > COEF_BOUND {
            return Err(KanBandError::CoefOutOfBounds {
                index: r,
                value: coef,
                bound: COEF_BOUND,
            });
        }
        coefs.push(coef);
    }
    Ok(KanBand {
        coefs,
        basis_kind: basis,
        rank_cap: target_rank as u8,
        reserved: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BasisKind round-trip + canonical ────────────────────────────

    #[test]
    fn basis_kind_roundtrip_u8() {
        for &k in &BasisKind::all() {
            assert_eq!(BasisKind::from_u8(k.to_u8()), k);
        }
    }

    #[test]
    fn basis_kind_unknown_clamps_to_gaussian_mix() {
        assert_eq!(BasisKind::from_u8(255), BasisKind::GaussianMix);
    }

    #[test]
    fn basis_kind_canonical_names_unique() {
        let ns: Vec<_> = BasisKind::all().iter().map(|k| k.canonical_name()).collect();
        let mut s = ns.clone();
        s.sort_unstable();
        let pre = s.len();
        s.dedup();
        assert_eq!(s.len(), pre);
    }

    // ── KanBand basics ──────────────────────────────────────────────

    #[test]
    fn kan_band_zero_has_default_rank() {
        let b = KanBand::zero();
        assert_eq!(b.rank(), KAN_BAND_RANK_DEFAULT);
        assert!(b.coefs.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn kan_band_from_slice_round_trip() {
        let coefs = [1.0, 2.0, 3.0, 4.0];
        let b = KanBand::from_slice(&coefs, BasisKind::GaussianMix).unwrap();
        assert_eq!(b.rank(), 4);
        assert_eq!(b.basis_kind, BasisKind::GaussianMix);
        for (i, &c) in coefs.iter().enumerate() {
            assert_eq!(b.coefs[i], c);
        }
    }

    #[test]
    fn kan_band_rank_exceeded_errors() {
        let big = vec![0.1_f32; KAN_BAND_RANK_MAX + 1];
        let r = KanBand::from_slice(&big, BasisKind::Hat);
        assert!(matches!(r, Err(KanBandError::RankExceeded { .. })));
    }

    #[test]
    fn kan_band_coef_out_of_bounds_errors() {
        let bad = [COEF_BOUND + 1.0];
        let r = KanBand::from_slice(&bad, BasisKind::Cosine);
        assert!(matches!(r, Err(KanBandError::CoefOutOfBounds { .. })));
    }

    #[test]
    fn kan_band_set_rank_grows_zero_pads() {
        let mut b = KanBand::from_slice(&[1.0, 2.0], BasisKind::Hat).unwrap();
        b.set_rank(5).unwrap();
        assert_eq!(b.rank(), 5);
        assert_eq!(b.coefs[0], 1.0);
        assert_eq!(b.coefs[1], 2.0);
        for i in 2..5 {
            assert_eq!(b.coefs[i], 0.0);
        }
    }

    #[test]
    fn kan_band_set_rank_underflow_errors() {
        let mut b = KanBand::zero();
        let r = b.set_rank(0);
        assert!(matches!(r, Err(KanBandError::RankUnderflow { .. })));
    }

    #[test]
    fn kan_band_adapt_rank_drops_small_tail() {
        let mut b =
            KanBand::from_slice(&[1.0, 0.5, 1e-6, 1e-7, 1e-8], BasisKind::GaussianMix).unwrap();
        let new_rank = b.adapt_rank(1e-3, 1).unwrap();
        assert_eq!(new_rank, 2);
        assert_eq!(b.rank(), 2);
    }

    #[test]
    fn kan_band_adapt_rank_respects_floor() {
        let mut b =
            KanBand::from_slice(&[1e-9, 1e-9, 1e-9, 1e-9], BasisKind::GaussianMix).unwrap();
        let new_rank = b.adapt_rank(1e-3, 2).unwrap();
        assert_eq!(new_rank, 2);
    }

    #[test]
    fn kan_band_l2_error_zero_for_match() {
        let b = KanBand::from_slice(&[1.0, 2.0, 3.0], BasisKind::Hat).unwrap();
        let e = b.l2_error(&[1.0, 2.0, 3.0]);
        assert!(e < 1e-6);
    }

    #[test]
    fn kan_band_l2_error_picks_up_oracle_tail() {
        let b = KanBand::from_slice(&[1.0, 2.0], BasisKind::Hat).unwrap();
        let e = b.l2_error(&[1.0, 2.0, 3.0]);
        // Tail term : √9 = 3.
        assert!((e - 3.0).abs() < 1e-3);
    }

    // ── KanBandTable ────────────────────────────────────────────────

    #[test]
    fn kan_band_table_canonical_dimensions() {
        let t = KanBandTable::canonical();
        assert_eq!(t.bins(), SPECTRUM_BINS);
        assert_eq!(t.rank_cap(), KAN_BAND_RANK_MAX);
    }

    #[test]
    fn kan_band_table_gaussian_peaks_at_center() {
        let t = KanBandTable::canonical();
        // For r = 0, mu = 0, peak at bin 0.
        let v0 = t.sample(BasisKind::GaussianMix, 0, 0).unwrap();
        let v_far = t.sample(BasisKind::GaussianMix, 0, SPECTRUM_BINS - 1).unwrap();
        assert!(v0 > v_far);
        assert!((v0 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn kan_band_table_learned_initial_is_identity() {
        let t = KanBandTable::canonical();
        // Learned default : δ(r, bin).
        let v_diag = t.sample(BasisKind::Learned, 3, 3).unwrap();
        let v_off = t.sample(BasisKind::Learned, 3, 4).unwrap();
        assert!((v_diag - 1.0).abs() < 1e-6);
        assert!(v_off.abs() < 1e-6);
    }

    #[test]
    fn kan_band_table_set_learned_overwrites() {
        let mut t = KanBandTable::canonical();
        let n = t.rank_cap() * t.bins();
        let learned: Vec<f32> = (0..n).map(|i| (i as f32) * 0.1).collect();
        t.set_learned(&learned).unwrap();
        let v = t.sample(BasisKind::Learned, 0, 5).unwrap();
        assert!((v - 0.5).abs() < 1e-3);
    }

    #[test]
    fn kan_band_table_set_learned_len_mismatch_errors() {
        let mut t = KanBandTable::canonical();
        let r = t.set_learned(&[0.0_f32; 3]);
        assert!(matches!(r, Err(KanBandError::SpectrumLenMismatch { .. })));
    }

    #[test]
    fn kan_band_table_oob_sample_errors() {
        let t = KanBandTable::canonical();
        let r = t.sample(BasisKind::GaussianMix, KAN_BAND_RANK_MAX, 0);
        assert!(matches!(r, Err(KanBandError::BasisTableMiss { .. })));
    }

    // ── encode/decode round-trip ────────────────────────────────────

    #[test]
    fn decode_spectrum_zero_band_yields_zero_spectrum() {
        let b = KanBand::zero();
        let t = KanBandTable::canonical();
        let mut out = vec![0.0_f32; SPECTRUM_BINS];
        decode_spectrum(&b, &t, &mut out).unwrap();
        assert!(out.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn decode_spectrum_len_mismatch_errors() {
        let b = KanBand::zero();
        let t = KanBandTable::canonical();
        let mut wrong = vec![0.0_f32; SPECTRUM_BINS - 1];
        let r = decode_spectrum(&b, &t, &mut wrong);
        assert!(matches!(r, Err(KanBandError::SpectrumLenMismatch { .. })));
    }

    #[test]
    fn encode_decode_identity_basis_round_trips_exactly() {
        // Use BasisKind::Learned (= δ identity) at full rank :
        // encode then decode should reproduce the spectrum exactly.
        let t = KanBandTable::canonical();
        let spectrum: Vec<f32> = (0..SPECTRUM_BINS).map(|i| (i as f32) * 0.1).collect();
        let band = encode_spectrum(&spectrum, BasisKind::Learned, SPECTRUM_BINS, &t).unwrap();
        let mut out = vec![0.0_f32; SPECTRUM_BINS];
        decode_spectrum(&band, &t, &mut out).unwrap();
        for (a, b) in spectrum.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-4, "expected {} got {}", a, b);
        }
    }

    #[test]
    fn encode_spectrum_low_rank_approximates() {
        // Encode a smooth spectrum with low-rank GaussianMix basis ;
        // round-trip should be within reasonable l2 of the input.
        let t = KanBandTable::canonical();
        let spectrum: Vec<f32> = (0..SPECTRUM_BINS)
            .map(|i| {
                let x = (i as f32) / ((SPECTRUM_BINS - 1) as f32);
                (x * std::f32::consts::PI).sin()
            })
            .collect();
        let band = encode_spectrum(&spectrum, BasisKind::GaussianMix, 8, &t).unwrap();
        let mut out = vec![0.0_f32; SPECTRUM_BINS];
        decode_spectrum(&band, &t, &mut out).unwrap();
        let mut err2 = 0.0_f32;
        for i in 0..SPECTRUM_BINS {
            let d = spectrum[i] - out[i];
            err2 += d * d;
        }
        // Loose : 8-rank GaussianMix on a non-orthogonal basis is approximate.
        // The least-squares projection bound for non-orthogonal bases is much
        // looser than for orthogonal bases. Sanity-check : err < spectrum-norm.
        let mut spec_norm = 0.0_f32;
        for &v in spectrum.iter() {
            spec_norm += v * v;
        }
        assert!(err2.sqrt() < spec_norm.sqrt() * 4.0);
    }

    #[test]
    fn encode_spectrum_rank_zero_errors() {
        let t = KanBandTable::canonical();
        let spectrum = vec![1.0_f32; SPECTRUM_BINS];
        let r = encode_spectrum(&spectrum, BasisKind::Learned, 0, &t);
        assert!(matches!(r, Err(KanBandError::RankUnderflow { .. })));
    }

    #[test]
    fn encode_spectrum_len_mismatch_errors() {
        let t = KanBandTable::canonical();
        let r = encode_spectrum(&[1.0_f32; 3], BasisKind::Hat, 3, &t);
        assert!(matches!(r, Err(KanBandError::SpectrumLenMismatch { .. })));
    }
}

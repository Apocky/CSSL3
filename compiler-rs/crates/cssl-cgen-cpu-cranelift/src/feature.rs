//! SIMD-tier + individual CPU feature flags.
//!
//! § SPEC : `specs/10_HW.csl` § AVX2 + AVX-512 OPPORTUNISTIC + `specs/14_BACKEND.csl` § SIMD.

use core::fmt;

/// SIMD-ISA tier (monotonic : `Avx512` ⊇ `Avx2` ⊇ `Sse2` ⊇ `ScalarOnly`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimdTier {
    /// No SIMD ; scalar paths only.
    ScalarOnly,
    /// SSE2 (baseline x86-64).
    Sse2,
    /// AVX2 (x86-64-v3 + FMA + BMI2 ⇒ typical modern-CPU baseline).
    Avx2,
    /// AVX-512 (F + DQ + BW + VL minimum ; opportunistic per `specs/14`).
    Avx512,
}

impl SimdTier {
    /// Stable short-name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScalarOnly => "scalar",
            Self::Sse2 => "sse2",
            Self::Avx2 => "avx2",
            Self::Avx512 => "avx512",
        }
    }

    /// True iff `self` is at least `other` (monotonic lattice).
    #[must_use]
    pub const fn at_least(self, other: Self) -> bool {
        self.rank() >= other.rank()
    }

    /// Internal ordering : `ScalarOnly=0 < Sse2=1 < Avx2=2 < Avx512=3`.
    const fn rank(self) -> u8 {
        match self {
            Self::ScalarOnly => 0,
            Self::Sse2 => 1,
            Self::Avx2 => 2,
            Self::Avx512 => 3,
        }
    }
}

impl fmt::Display for SimdTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Individual CPU feature flag. Added on top of the SIMD-tier baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CpuFeature {
    /// Fused-Multiply-Add.
    Fma,
    /// Bit-manipulation instructions v1 (ANDN / BEXTR / BLSI / BLSMSK / BLSR / TZCNT).
    Bmi1,
    /// Bit-manipulation instructions v2 (BZHI / MULX / PDEP / PEXT / RORX / SARX / SHLX / SHRX).
    Bmi2,
    /// `popcnt` instruction.
    Popcnt,
    /// `lzcnt` instruction.
    Lzcnt,
    /// `movbe` byte-swap move.
    Movbe,
    /// AVX-512 Foundation.
    Avx512F,
    /// AVX-512 Doubleword + Quadword.
    Avx512Dq,
    /// AVX-512 Byte + Word.
    Avx512Bw,
    /// AVX-512 Vector-Length.
    Avx512Vl,
    /// AVX-512 VNNI (int8/int16 neural-network-inference).
    Avx512Vnni,
    /// AVX-512 BF16 (bfloat16 dot-product).
    Avx512Bf16,
    /// AES vector extensions.
    Vaes,
    /// `pclmulqdq` carry-less multiply.
    Pclmulqdq,
    /// SHA1 + SHA256 hash extensions.
    Sha,
    /// RDRAND hardware RNG.
    RdRand,
    /// RDSEED hardware RNG.
    RdSeed,
}

impl CpuFeature {
    /// Canonical short-name (matches the LLVM target-feature string).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fma => "fma",
            Self::Bmi1 => "bmi1",
            Self::Bmi2 => "bmi2",
            Self::Popcnt => "popcnt",
            Self::Lzcnt => "lzcnt",
            Self::Movbe => "movbe",
            Self::Avx512F => "avx512f",
            Self::Avx512Dq => "avx512dq",
            Self::Avx512Bw => "avx512bw",
            Self::Avx512Vl => "avx512vl",
            Self::Avx512Vnni => "avx512vnni",
            Self::Avx512Bf16 => "avx512bf16",
            Self::Vaes => "vaes",
            Self::Pclmulqdq => "pclmulqdq",
            Self::Sha => "sha",
            Self::RdRand => "rdrand",
            Self::RdSeed => "rdseed",
        }
    }
}

impl fmt::Display for CpuFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Set of feature-flags active for a target-profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuFeatureSet {
    features: std::collections::BTreeSet<CpuFeature>,
}

impl CpuFeatureSet {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a feature.
    pub fn add(&mut self, f: CpuFeature) {
        self.features.insert(f);
    }

    /// True iff `f` is in the set.
    #[must_use]
    pub fn contains(&self, f: CpuFeature) -> bool {
        self.features.contains(&f)
    }

    /// Iterate features in stable (sorted) order.
    pub fn iter(&self) -> impl Iterator<Item = CpuFeature> + '_ {
        self.features.iter().copied()
    }

    /// Number of features in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// True iff set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    /// Summary suffix for `CpuTargetProfile::summary()` — prefixed `+` if non-empty.
    #[must_use]
    pub fn summary_suffix(&self) -> String {
        if self.is_empty() {
            String::new()
        } else {
            let parts: Vec<&str> = self.iter().map(CpuFeature::as_str).collect();
            format!("+{}", parts.join("+"))
        }
    }

    /// Render the LLVM / cranelift `target-features` string (`"+fma,+bmi2,+popcnt"`).
    #[must_use]
    pub fn render_target_features(&self) -> String {
        self.iter()
            .map(|f| format!("+{}", f.as_str()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl FromIterator<CpuFeature> for CpuFeatureSet {
    fn from_iter<I: IntoIterator<Item = CpuFeature>>(iter: I) -> Self {
        let mut s = Self::new();
        for f in iter {
            s.add(f);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{CpuFeature, CpuFeatureSet, SimdTier};

    #[test]
    fn simd_tier_names() {
        assert_eq!(SimdTier::ScalarOnly.as_str(), "scalar");
        assert_eq!(SimdTier::Avx2.as_str(), "avx2");
        assert_eq!(SimdTier::Avx512.as_str(), "avx512");
    }

    #[test]
    fn simd_tier_monotonic() {
        assert!(SimdTier::Avx512.at_least(SimdTier::Avx2));
        assert!(SimdTier::Avx2.at_least(SimdTier::Sse2));
        assert!(!SimdTier::Sse2.at_least(SimdTier::Avx2));
    }

    #[test]
    fn feature_names() {
        assert_eq!(CpuFeature::Fma.as_str(), "fma");
        assert_eq!(CpuFeature::Avx512Vnni.as_str(), "avx512vnni");
    }

    #[test]
    fn feature_set_empty_summary() {
        let s = CpuFeatureSet::new();
        assert!(s.is_empty());
        assert_eq!(s.summary_suffix(), "");
    }

    #[test]
    fn feature_set_add_contains() {
        let mut s = CpuFeatureSet::new();
        s.add(CpuFeature::Fma);
        s.add(CpuFeature::Bmi2);
        assert!(s.contains(CpuFeature::Fma));
        assert!(s.contains(CpuFeature::Bmi2));
        assert!(!s.contains(CpuFeature::Avx512F));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn feature_set_iter_is_sorted() {
        let s = CpuFeatureSet::from_iter([
            CpuFeature::Bmi2,
            CpuFeature::Fma,
            CpuFeature::Popcnt,
            CpuFeature::Bmi1,
        ]);
        let order: Vec<_> = s.iter().collect();
        // Derived PartialOrd : enum-declaration order → Fma < Bmi1 < Bmi2 < Popcnt.
        assert_eq!(
            order,
            vec![
                CpuFeature::Fma,
                CpuFeature::Bmi1,
                CpuFeature::Bmi2,
                CpuFeature::Popcnt,
            ]
        );
    }

    #[test]
    fn feature_set_summary_suffix() {
        let s = CpuFeatureSet::from_iter([CpuFeature::Fma, CpuFeature::Bmi2]);
        assert_eq!(s.summary_suffix(), "+fma+bmi2");
    }

    #[test]
    fn feature_set_target_features_string() {
        let s = CpuFeatureSet::from_iter([CpuFeature::Fma, CpuFeature::Bmi2, CpuFeature::Popcnt]);
        assert_eq!(s.render_target_features(), "+fma,+bmi2,+popcnt");
    }
}

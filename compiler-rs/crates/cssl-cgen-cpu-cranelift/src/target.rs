//! CPU-target µarch enumeration + target-profile bundles.
//!
//! § SPEC : `specs/10_HW.csl` § PRIMARY-TARGET + `specs/14_BACKEND.csl` § µarchs supported.

use core::fmt;

use crate::abi::{Abi, ObjectFormat};
use crate::feature::{CpuFeature, CpuFeatureSet, SimdTier};

/// Canonical µarch identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpuTarget {
    /// Intel 12gen P-core (Alder Lake).
    IntelAlderLake,
    /// Intel 13/14gen P-core (Raptor Lake).
    IntelRaptorLake,
    /// Intel Core-Ultra-1 P-core (Meteor Lake).
    IntelMeteorLake,
    /// Intel Core-Ultra-2 (Arrow Lake).
    IntelArrowLake,
    /// AMD Zen-4.
    AmdZen4,
    /// AMD Zen-5.
    AmdZen5,
    /// Generic `x86-64-v3` baseline (AVX2 + FMA + BMI2 + POPCNT).
    GenericX86_64V3,
}

impl CpuTarget {
    /// Canonical triple-ish name used in profile diagnostics.
    #[must_use]
    pub const fn triple(self) -> &'static str {
        match self {
            Self::IntelAlderLake => "intel-alder-lake",
            Self::IntelRaptorLake => "intel-raptor-lake",
            Self::IntelMeteorLake => "intel-meteor-lake",
            Self::IntelArrowLake => "intel-arrow-lake",
            Self::AmdZen4 => "amd-zen4",
            Self::AmdZen5 => "amd-zen5",
            Self::GenericX86_64V3 => "x86-64-v3",
        }
    }

    /// Default SIMD tier this µarch supports.
    #[must_use]
    pub const fn default_simd_tier(self) -> SimdTier {
        match self {
            // Alder Lake : AVX-512 fused on P-core silicon but disabled in production microcode.
            Self::IntelAlderLake => SimdTier::Avx2,
            Self::IntelRaptorLake => SimdTier::Avx2,
            Self::IntelMeteorLake => SimdTier::Avx2,
            // Arrow Lake reintroduces AVX-512 on some SKUs via AVX10.1 (Granite Rapids alignment).
            Self::IntelArrowLake => SimdTier::Avx2,
            Self::AmdZen4 => SimdTier::Avx512,
            Self::AmdZen5 => SimdTier::Avx512,
            Self::GenericX86_64V3 => SimdTier::Avx2,
        }
    }

    /// All 7 supported µarchs.
    pub const ALL_TARGETS: [Self; 7] = [
        Self::IntelAlderLake,
        Self::IntelRaptorLake,
        Self::IntelMeteorLake,
        Self::IntelArrowLake,
        Self::AmdZen4,
        Self::AmdZen5,
        Self::GenericX86_64V3,
    ];
}

impl fmt::Display for CpuTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.triple())
    }
}

/// Debug-info emission format.
///
/// Matches `specs/07_CODEGEN.csl` § CPU BACKEND debug-info line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugFormat {
    /// DWARF-5 (Linux + Mac).
    Dwarf5,
    /// CodeView (Windows).
    CodeView,
    /// No debug-info emitted (release-stripped builds).
    None,
}

impl DebugFormat {
    /// Stable short-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dwarf5 => "dwarf5",
            Self::CodeView => "codeview",
            Self::None => "none",
        }
    }
}

/// Bundle of all codegen knobs for a given target machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTargetProfile {
    /// Canonical µarch.
    pub target: CpuTarget,
    /// SIMD tier (overrides `target.default_simd_tier()` when set below its default).
    pub simd_tier: SimdTier,
    /// Feature-set additive-over the tier's baseline (FMA, BMI2, POPCNT, VAES, etc.).
    pub features: CpuFeatureSet,
    /// Calling-convention ABI.
    pub abi: Abi,
    /// Object-file format.
    pub object_format: ObjectFormat,
    /// Debug-info emission format.
    pub debug_format: DebugFormat,
}

impl CpuTargetProfile {
    /// Default profile for the primary v1 target : Intel 12gen + Windows + COFF + CodeView.
    #[must_use]
    pub fn windows_default() -> Self {
        Self {
            target: CpuTarget::IntelAlderLake,
            simd_tier: SimdTier::Avx2,
            features: CpuFeatureSet::from_iter([
                CpuFeature::Fma,
                CpuFeature::Bmi1,
                CpuFeature::Bmi2,
                CpuFeature::Popcnt,
                CpuFeature::Lzcnt,
                CpuFeature::Movbe,
            ]),
            abi: Abi::WindowsX64,
            object_format: ObjectFormat::Coff,
            debug_format: DebugFormat::CodeView,
        }
    }

    /// Default profile for Linux : Intel 12gen + SysV + ELF + DWARF-5.
    #[must_use]
    pub fn linux_default() -> Self {
        Self {
            target: CpuTarget::IntelAlderLake,
            simd_tier: SimdTier::Avx2,
            features: CpuFeatureSet::from_iter([
                CpuFeature::Fma,
                CpuFeature::Bmi1,
                CpuFeature::Bmi2,
                CpuFeature::Popcnt,
                CpuFeature::Lzcnt,
                CpuFeature::Movbe,
            ]),
            abi: Abi::SysVAmd64,
            object_format: ObjectFormat::Elf,
            debug_format: DebugFormat::Dwarf5,
        }
    }

    /// Default profile for Mac-Intel : Darwin + `MachO` + DWARF-5.
    #[must_use]
    pub fn darwin_default() -> Self {
        Self {
            target: CpuTarget::GenericX86_64V3,
            simd_tier: SimdTier::Avx2,
            features: CpuFeatureSet::from_iter([
                CpuFeature::Fma,
                CpuFeature::Bmi1,
                CpuFeature::Bmi2,
                CpuFeature::Popcnt,
                CpuFeature::Lzcnt,
            ]),
            abi: Abi::DarwinAmd64,
            object_format: ObjectFormat::MachO,
            debug_format: DebugFormat::Dwarf5,
        }
    }

    /// Short diagnostic summary line (`"intel-alder-lake / avx2+fma+bmi2 / sysv / elf"`).
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} / {}{} / {} / {}",
            self.target.triple(),
            self.simd_tier.as_str(),
            self.features.summary_suffix(),
            self.abi.as_str(),
            self.object_format.as_str(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{CpuTarget, CpuTargetProfile, DebugFormat};
    use crate::abi::{Abi, ObjectFormat};
    use crate::feature::{CpuFeature, SimdTier};

    #[test]
    fn all_targets_unique() {
        let set: std::collections::HashSet<_> =
            CpuTarget::ALL_TARGETS.iter().map(|t| t.triple()).collect();
        assert_eq!(set.len(), CpuTarget::ALL_TARGETS.len());
    }

    #[test]
    fn default_simd_tier_intel_is_avx2() {
        assert_eq!(
            CpuTarget::IntelAlderLake.default_simd_tier(),
            SimdTier::Avx2
        );
    }

    #[test]
    fn default_simd_tier_zen_is_avx512() {
        assert_eq!(CpuTarget::AmdZen5.default_simd_tier(), SimdTier::Avx512);
    }

    #[test]
    fn windows_default_is_coff() {
        let p = CpuTargetProfile::windows_default();
        assert_eq!(p.object_format, ObjectFormat::Coff);
        assert_eq!(p.abi, Abi::WindowsX64);
        assert_eq!(p.debug_format, DebugFormat::CodeView);
    }

    #[test]
    fn linux_default_is_elf() {
        let p = CpuTargetProfile::linux_default();
        assert_eq!(p.object_format, ObjectFormat::Elf);
        assert_eq!(p.abi, Abi::SysVAmd64);
        assert_eq!(p.debug_format, DebugFormat::Dwarf5);
    }

    #[test]
    fn darwin_default_is_macho() {
        let p = CpuTargetProfile::darwin_default();
        assert_eq!(p.object_format, ObjectFormat::MachO);
    }

    #[test]
    fn summary_contains_target_and_simd() {
        let p = CpuTargetProfile::windows_default();
        let s = p.summary();
        assert!(s.contains("intel-alder-lake"));
        assert!(s.contains("avx2"));
        assert!(s.contains("fma"));
    }

    #[test]
    fn debug_format_names() {
        assert_eq!(DebugFormat::Dwarf5.as_str(), "dwarf5");
        assert_eq!(DebugFormat::CodeView.as_str(), "codeview");
        assert_eq!(DebugFormat::None.as_str(), "none");
    }

    #[test]
    fn profile_equality() {
        let a = CpuTargetProfile::windows_default();
        let b = CpuTargetProfile::windows_default();
        assert_eq!(a, b);
    }

    #[test]
    fn feature_set_accepts_additions() {
        let mut p = CpuTargetProfile::linux_default();
        p.features.add(CpuFeature::Avx512F);
        assert!(p.features.contains(CpuFeature::Avx512F));
    }
}

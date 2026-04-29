//! Cooperative-matrix preferred path for batched-Jacobian + KAN-batched
//! matmul.
//!
//! § SPEC : `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING § III` (canonical
//!         tile-shape table + dispatch tier) +
//!         `specs/05_AUTODIFF.csl § GPU AUTODIFF` :
//!   `{XMX} cooperative-matrix available for batched-Jacobians`.
//!
//! § DESIGN
//!   The KAN-runtime spec calls out three dispatch tiers :
//!     TIER-1 : cooperative-matrix (preferred — Vulkan KHR_cooperative_matrix
//!              / D3D12 SM6.9 / Metal-3 simdgroup-matrix)
//!     TIER-2 : SIMD-warp (32-wide / 64-wide cooperative groups)
//!     TIER-3 : per-thread scalar (low-end / fallback)
//!
//!   This module owns the TIER-1 path : detect availability + return the
//!   per-vendor `(M, N, K)` tile-shape so the Jacobian batching can compile
//!   to the matrix-engine. The TIER-2 fallback is handled by the existing
//!   subgroup-shuffle plumbing in `cssl-cgen-gpu-spirv` ; TIER-3 is the
//!   default scalar SPIR-V emission.
//!
//! § VENDOR-TILE-MAP
//!   The per-vendor tile-shape comes from the spec table. Renaming any of
//!   these names requires a lock-step update to the KAN-runtime
//!   specialization-by-shape recognizer.

/// Vendor / arch identifier — used to look up the matrix-engine tile-shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoopMatrixVendor {
    /// NVIDIA Tensor-Core 4th-gen (RTX-50 series).
    NvidiaTensorCore4,
    /// NVIDIA Tensor-Core 3rd-gen (RTX-40 series).
    NvidiaTensorCore3,
    /// AMD WMMA on RDNA-4 (32-wave preferred).
    AmdRdna4Wmma,
    /// AMD WMMA on RDNA-3 (W32-mode required).
    AmdRdna3Wmma,
    /// Intel Arc XMX (DG2 / Battlemage).
    IntelArcXmx,
    /// Apple-Silicon M3+ simdgroup-matrix.
    AppleM3SimdMatrix,
    /// Qualcomm Adreno SM6.9-cooperative-vectors backport.
    QualcommAdrenoCoopVec,
    /// Software fallback (TIER-3 marker).
    Scalar,
}

impl CoopMatrixVendor {
    /// All cataloged vendors (test surface).
    pub const ALL: [Self; 8] = [
        Self::NvidiaTensorCore4,
        Self::NvidiaTensorCore3,
        Self::AmdRdna4Wmma,
        Self::AmdRdna3Wmma,
        Self::IntelArcXmx,
        Self::AppleM3SimdMatrix,
        Self::QualcommAdrenoCoopVec,
        Self::Scalar,
    ];

    /// Stable text-form name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NvidiaTensorCore4 => "nvidia-tc4",
            Self::NvidiaTensorCore3 => "nvidia-tc3",
            Self::AmdRdna4Wmma => "amd-rdna4-wmma",
            Self::AmdRdna3Wmma => "amd-rdna3-wmma",
            Self::IntelArcXmx => "intel-arc-xmx",
            Self::AppleM3SimdMatrix => "apple-m3-simdmat",
            Self::QualcommAdrenoCoopVec => "adreno-coopvec",
            Self::Scalar => "scalar-fallback",
        }
    }

    /// True iff this vendor supports a cooperative-matrix path at all.
    #[must_use]
    pub const fn has_coop_matrix(self) -> bool {
        !matches!(self, Self::Scalar)
    }
}

/// A `(M, N, K)` tile-shape. `M` = rows of A / output, `N` = cols of B /
/// output, `K` = inner dimension. Names match the spec table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileShape {
    pub m: u16,
    pub n: u16,
    pub k: u16,
}

impl TileShape {
    /// Construct a tile-shape.
    #[must_use]
    pub const fn new(m: u16, n: u16, k: u16) -> Self {
        Self { m, n, k }
    }

    /// Total element count of an A-block (M × K).
    #[must_use]
    pub const fn a_elements(self) -> u32 {
        (self.m as u32) * (self.k as u32)
    }

    /// Total element count of a B-block (K × N).
    #[must_use]
    pub const fn b_elements(self) -> u32 {
        (self.k as u32) * (self.n as u32)
    }

    /// Total element count of a C / D-block (M × N).
    #[must_use]
    pub const fn c_elements(self) -> u32 {
        (self.m as u32) * (self.n as u32)
    }
}

/// Cooperative-matrix path descriptor — the result of the dispatch-decision
/// + the tile-shape the SPIR-V emitter should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CoopMatrixPath {
    /// Vendor / arch this path targets.
    pub vendor: CoopMatrixVendor,
    /// Tile shape (None for scalar fallback).
    pub tile: Option<TileShape>,
    /// Element format string ("f16" / "f32" / "i8" / "f8") — matches the
    /// spec's element-format column.
    pub element_format: &'static str,
}

impl CoopMatrixPath {
    /// Look up the canonical tile-shape for a vendor (per the KAN-runtime
    /// spec's vendor table § III).
    #[must_use]
    pub const fn tile_shape_for(vendor: CoopMatrixVendor) -> Option<TileShape> {
        match vendor {
            CoopMatrixVendor::NvidiaTensorCore4 | CoopMatrixVendor::NvidiaTensorCore3 => {
                Some(TileShape::new(16, 16, 16))
            }
            CoopMatrixVendor::AmdRdna4Wmma | CoopMatrixVendor::AmdRdna3Wmma => {
                Some(TileShape::new(16, 16, 8))
            }
            CoopMatrixVendor::IntelArcXmx => Some(TileShape::new(8, 16, 16)),
            CoopMatrixVendor::AppleM3SimdMatrix => Some(TileShape::new(8, 8, 8)),
            CoopMatrixVendor::QualcommAdrenoCoopVec => Some(TileShape::new(8, 8, 16)),
            CoopMatrixVendor::Scalar => None,
        }
    }

    /// Default element format for a vendor (most use FP16 for KAN ; Apple
    /// uses FP32 for compat ; Adreno uses FP16).
    #[must_use]
    pub const fn default_element_format(vendor: CoopMatrixVendor) -> &'static str {
        match vendor {
            CoopMatrixVendor::AppleM3SimdMatrix => "f32",
            _ => "f16",
        }
    }

    /// Construct a path for a vendor with default tile + element format.
    #[must_use]
    pub const fn for_vendor(vendor: CoopMatrixVendor) -> Self {
        Self {
            vendor,
            tile: Self::tile_shape_for(vendor),
            element_format: Self::default_element_format(vendor),
        }
    }

    /// True iff this path uses the matrix engine (not the scalar fallback).
    #[must_use]
    pub const fn uses_matrix_engine(self) -> bool {
        self.tile.is_some()
    }

    /// SPIR-V capability required for this path.
    #[must_use]
    pub const fn required_capability(self) -> Option<&'static str> {
        if self.uses_matrix_engine() {
            Some("CooperativeMatrixKHR")
        } else {
            None
        }
    }

    /// Number of FMA-units / cycle the tile delivers (informational ; used
    /// by the Jacobian-batch sizer).
    #[must_use]
    pub const fn fma_units_per_cycle(self) -> u32 {
        if let Some(t) = self.tile {
            (t.m as u32) * (t.n as u32)
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_tile_is_16_16_16() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::NvidiaTensorCore4);
        let t = p.tile.expect("tile present");
        assert_eq!((t.m, t.n, t.k), (16, 16, 16));
    }

    #[test]
    fn amd_rdna4_tile_is_16_16_8() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::AmdRdna4Wmma);
        let t = p.tile.expect("tile present");
        assert_eq!((t.m, t.n, t.k), (16, 16, 8));
    }

    #[test]
    fn intel_arc_tile_is_8_16_16() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::IntelArcXmx);
        let t = p.tile.expect("tile present");
        assert_eq!((t.m, t.n, t.k), (8, 16, 16));
    }

    #[test]
    fn apple_m3_tile_is_8_8_8() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::AppleM3SimdMatrix);
        let t = p.tile.expect("tile present");
        assert_eq!((t.m, t.n, t.k), (8, 8, 8));
    }

    #[test]
    fn scalar_fallback_has_no_tile() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::Scalar);
        assert!(p.tile.is_none());
        assert!(!p.uses_matrix_engine());
    }

    #[test]
    fn coop_matrix_capability_string() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::NvidiaTensorCore4);
        assert_eq!(p.required_capability(), Some("CooperativeMatrixKHR"));
    }

    #[test]
    fn scalar_path_no_capability_required() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::Scalar);
        assert_eq!(p.required_capability(), None);
    }

    #[test]
    fn apple_default_element_format_is_f32() {
        assert_eq!(
            CoopMatrixPath::default_element_format(CoopMatrixVendor::AppleM3SimdMatrix),
            "f32"
        );
    }

    #[test]
    fn nvidia_default_element_format_is_f16() {
        assert_eq!(
            CoopMatrixPath::default_element_format(CoopMatrixVendor::NvidiaTensorCore4),
            "f16"
        );
    }

    #[test]
    fn fma_units_per_cycle_for_nvidia() {
        let p = CoopMatrixPath::for_vendor(CoopMatrixVendor::NvidiaTensorCore4);
        assert_eq!(p.fma_units_per_cycle(), 256); // 16 * 16
    }

    #[test]
    fn vendor_names_are_unique() {
        use std::collections::HashSet;
        let names: HashSet<_> = CoopMatrixVendor::ALL.iter().map(|v| v.name()).collect();
        assert_eq!(names.len(), CoopMatrixVendor::ALL.len());
    }

    #[test]
    fn tile_a_b_c_element_counts_consistent() {
        let t = TileShape::new(16, 16, 16);
        assert_eq!(t.a_elements(), 16 * 16);
        assert_eq!(t.b_elements(), 16 * 16);
        assert_eq!(t.c_elements(), 16 * 16);
    }

    #[test]
    fn has_coop_matrix_excludes_scalar() {
        for v in CoopMatrixVendor::ALL {
            if matches!(v, CoopMatrixVendor::Scalar) {
                assert!(!v.has_coop_matrix());
            } else {
                assert!(v.has_coop_matrix());
            }
        }
    }
}

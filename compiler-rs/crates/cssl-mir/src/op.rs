//! CSSL-dialect operations — the ~25 custom `cssl.*` ops from `specs/15` § CSSL-DIALECT OPS.
//!
//! § DESIGN
//!   Each `CsslOp` variant represents one dialect op with its canonical source-form
//!   name. `OpSignature` records the expected operand / result arity for a given
//!   op variant ; the pretty-printer uses this to produce valid textual MLIR.
//!
//!   Non-custom ops (arith / scf / cf / func / memref / vector / linalg / affine /
//!   gpu / spirv / llvm) are represented as [`CsslOp::Std`] with a free-form name ;
//!   they're passed through the printer without schema validation at stage-0.

use core::fmt;

/// Canonical dialect-op variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CsslOp {
    // § AD + Jet (F1 + §§ 17)
    DiffPrimal,
    DiffFwd,
    DiffBwd,
    JetConstruct,
    JetProject,
    // § Effects + Handlers
    EffectPerform,
    EffectHandle,
    // § Regions + Handles (§§ 12)
    RegionEnter,
    RegionExit,
    HandlePack,
    HandleUnpack,
    HandleCheck,
    // § Staging + Macros (F4)
    StagedSplice,
    StagedQuote,
    StagedRun,
    MacroExpand,
    // § IFC + Verify (§§ 11 + §§ 20)
    IfcLabel,
    IfcDeclassify,
    VerifyAssert,
    // § Engine primitives
    SdfMarch,
    SdfNormal,
    // § GPU
    GpuBarrier,
    // § XMX cooperative matrix
    XmxCoopMatmul,
    // § Ray-tracing
    RtTraceRay,
    RtIntersect,
    // § Telemetry (R18)
    TelemetryProbe,
    /// Standard-dialect op — name stored separately. Used for `arith.*`, `scf.*`,
    /// `func.*`, `memref.*`, etc. that pass through without schema validation.
    Std,
}

impl CsslOp {
    /// Canonical source-form name (e.g., `"cssl.diff.primal"`). For [`Self::Std`],
    /// the caller supplies the name — this method returns `"cssl.std"` as a marker.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DiffPrimal => "cssl.diff.primal",
            Self::DiffFwd => "cssl.diff.fwd",
            Self::DiffBwd => "cssl.diff.bwd",
            Self::JetConstruct => "cssl.jet.construct",
            Self::JetProject => "cssl.jet.project",
            Self::EffectPerform => "cssl.effect.perform",
            Self::EffectHandle => "cssl.effect.handle",
            Self::RegionEnter => "cssl.region.enter",
            Self::RegionExit => "cssl.region.exit",
            Self::HandlePack => "cssl.handle.pack",
            Self::HandleUnpack => "cssl.handle.unpack",
            Self::HandleCheck => "cssl.handle.check",
            Self::StagedSplice => "cssl.staged.splice",
            Self::StagedQuote => "cssl.staged.quote",
            Self::StagedRun => "cssl.staged.run",
            Self::MacroExpand => "cssl.macro.expand",
            Self::IfcLabel => "cssl.ifc.label",
            Self::IfcDeclassify => "cssl.ifc.declassify",
            Self::VerifyAssert => "cssl.verify.assert",
            Self::SdfMarch => "cssl.sdf.march",
            Self::SdfNormal => "cssl.sdf.normal",
            Self::GpuBarrier => "cssl.gpu.barrier",
            Self::XmxCoopMatmul => "cssl.xmx.coop_matmul",
            Self::RtTraceRay => "cssl.rt.trace_ray",
            Self::RtIntersect => "cssl.rt.intersect",
            Self::TelemetryProbe => "cssl.telemetry.probe",
            Self::Std => "cssl.std",
        }
    }

    /// Category grouping from `specs/15`.
    #[must_use]
    pub const fn category(self) -> OpCategory {
        match self {
            Self::DiffPrimal | Self::DiffFwd | Self::DiffBwd => OpCategory::AutoDiff,
            Self::JetConstruct | Self::JetProject => OpCategory::Jet,
            Self::EffectPerform | Self::EffectHandle => OpCategory::Effect,
            Self::RegionEnter | Self::RegionExit => OpCategory::Region,
            Self::HandlePack | Self::HandleUnpack | Self::HandleCheck => OpCategory::Handle,
            Self::StagedSplice | Self::StagedQuote | Self::StagedRun => OpCategory::Staged,
            Self::MacroExpand => OpCategory::Macro,
            Self::IfcLabel | Self::IfcDeclassify => OpCategory::Ifc,
            Self::VerifyAssert => OpCategory::Verify,
            Self::SdfMarch | Self::SdfNormal => OpCategory::Sdf,
            Self::GpuBarrier => OpCategory::Gpu,
            Self::XmxCoopMatmul => OpCategory::Xmx,
            Self::RtTraceRay | Self::RtIntersect => OpCategory::Rt,
            Self::TelemetryProbe => OpCategory::Telemetry,
            Self::Std => OpCategory::Std,
        }
    }

    /// Canonical signature — expected operand / result counts.
    /// Variadic ops surface as `None` in the relevant slot.
    #[must_use]
    pub const fn signature(self) -> OpSignature {
        match self {
            // AD : primal takes N operands → N results ; fwd/bwd take primal + tangent(s).
            Self::DiffPrimal => OpSignature {
                operands: None,
                results: None,
            },
            Self::DiffFwd | Self::DiffBwd => OpSignature {
                operands: None,
                results: None,
            },
            // Jet : construct N-tangent → 1 Jet ; project Jet + index → 1 tangent.
            Self::JetConstruct => OpSignature {
                operands: None,
                results: Some(1),
            },
            Self::JetProject => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            // Effect : perform takes args → 1 result ; handle takes body → 1 result.
            Self::EffectPerform | Self::EffectHandle => OpSignature {
                operands: None,
                results: Some(1),
            },
            // Region : enter takes no operands → 1 token ; exit takes token → no result.
            Self::RegionEnter => OpSignature {
                operands: Some(0),
                results: Some(1),
            },
            Self::RegionExit => OpSignature {
                operands: Some(1),
                results: Some(0),
            },
            // Handle : pack (idx, gen) → u64 ; unpack u64 → (idx, gen) ; check u64 + gen → bool.
            Self::HandlePack => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            Self::HandleUnpack => OpSignature {
                operands: Some(1),
                results: Some(2),
            },
            Self::HandleCheck => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            // Staged : splice / quote / run all variadic.
            Self::StagedSplice | Self::StagedQuote | Self::StagedRun => OpSignature {
                operands: None,
                results: Some(1),
            },
            Self::MacroExpand => OpSignature {
                operands: None,
                results: Some(1),
            },
            // IFC : label (value, label-attr) → value ; declassify similar.
            Self::IfcLabel | Self::IfcDeclassify => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            // Verify : assert condition → no result.
            Self::VerifyAssert => OpSignature {
                operands: Some(1),
                results: Some(0),
            },
            // SDF : march (scene, ray) → hit ; normal (scene, point) → vec3.
            Self::SdfMarch | Self::SdfNormal => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            // GPU barrier : no operands, no results.
            Self::GpuBarrier => OpSignature {
                operands: Some(0),
                results: Some(0),
            },
            // XMX coop-matmul : (a, b, c) → d.
            Self::XmxCoopMatmul => OpSignature {
                operands: Some(3),
                results: Some(1),
            },
            // RT : trace_ray (ray, scene) → hit ; intersect custom op (variadic).
            Self::RtTraceRay => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            Self::RtIntersect => OpSignature {
                operands: None,
                results: Some(1),
            },
            // Telemetry probe : no operands, no results (side-effect only).
            Self::TelemetryProbe => OpSignature {
                operands: Some(0),
                results: Some(0),
            },
            // Std : free-form.
            Self::Std => OpSignature {
                operands: None,
                results: None,
            },
        }
    }

    /// All `cssl.*` dialect ops (excluding `Std`).
    pub const ALL_CSSL: [Self; 26] = [
        Self::DiffPrimal,
        Self::DiffFwd,
        Self::DiffBwd,
        Self::JetConstruct,
        Self::JetProject,
        Self::EffectPerform,
        Self::EffectHandle,
        Self::RegionEnter,
        Self::RegionExit,
        Self::HandlePack,
        Self::HandleUnpack,
        Self::HandleCheck,
        Self::StagedSplice,
        Self::StagedQuote,
        Self::StagedRun,
        Self::MacroExpand,
        Self::IfcLabel,
        Self::IfcDeclassify,
        Self::VerifyAssert,
        Self::SdfMarch,
        Self::SdfNormal,
        Self::GpuBarrier,
        Self::XmxCoopMatmul,
        Self::RtTraceRay,
        Self::RtIntersect,
        Self::TelemetryProbe,
    ];
}

impl fmt::Display for CsslOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Category grouping for dialect ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpCategory {
    AutoDiff,
    Jet,
    Effect,
    Region,
    Handle,
    Staged,
    Macro,
    Ifc,
    Verify,
    Sdf,
    Gpu,
    Xmx,
    Rt,
    Telemetry,
    Std,
}

/// Expected operand / result count for an op. `None` = variadic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpSignature {
    pub operands: Option<usize>,
    pub results: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::{CsslOp, OpCategory};

    #[test]
    fn all_ops_have_unique_names() {
        let mut names: Vec<&'static str> = CsslOp::ALL_CSSL.iter().map(|o| o.name()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "op names must be unique");
    }

    #[test]
    fn all_ops_start_with_cssl_prefix() {
        for op in CsslOp::ALL_CSSL {
            assert!(op.name().starts_with("cssl."), "{}", op.name());
        }
    }

    #[test]
    fn all_26_cssl_ops_tracked() {
        assert_eq!(CsslOp::ALL_CSSL.len(), 26);
    }

    #[test]
    fn category_mapping_is_exhaustive() {
        // Every ALL_CSSL variant should produce a non-Std category.
        for op in CsslOp::ALL_CSSL {
            assert_ne!(op.category(), OpCategory::Std, "{op:?}");
        }
    }

    #[test]
    fn handle_pack_signature_is_2_to_1() {
        let sig = CsslOp::HandlePack.signature();
        assert_eq!(sig.operands, Some(2));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn region_enter_returns_one_token() {
        let sig = CsslOp::RegionEnter.signature();
        assert_eq!(sig.operands, Some(0));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn telemetry_probe_is_void_in_void_out() {
        let sig = CsslOp::TelemetryProbe.signature();
        assert_eq!(sig.operands, Some(0));
        assert_eq!(sig.results, Some(0));
    }

    #[test]
    fn std_is_free_form() {
        let sig = CsslOp::Std.signature();
        assert!(sig.operands.is_none());
        assert!(sig.results.is_none());
    }

    #[test]
    fn display_matches_name() {
        assert_eq!(format!("{}", CsslOp::SdfMarch), "cssl.sdf.march");
        assert_eq!(format!("{}", CsslOp::GpuBarrier), "cssl.gpu.barrier");
    }
}

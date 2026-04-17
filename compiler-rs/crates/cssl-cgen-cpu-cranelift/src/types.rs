//! MIR-type → CLIF-type mapping + calling-convention lane-mapping.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU BACKEND § flow.

use cssl_mir::{FloatWidth, IntWidth, MirType};

/// Canonical CLIF-type name (stage-0 stores the textual form that Cranelift would accept ;
/// phase-2 swaps this for `cranelift_codegen::ir::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClifType {
    /// CLIF `i8`.
    I8,
    /// CLIF `i16`.
    I16,
    /// CLIF `i32`.
    I32,
    /// CLIF `i64`.
    I64,
    /// CLIF `b1` — boolean-width.
    B1,
    /// CLIF `f16` (not uniformly supported ; accepted as attribute at stage-0).
    F16,
    /// CLIF `f32`.
    F32,
    /// CLIF `f64`.
    F64,
    /// CLIF `r64` — reference-width on 64-bit targets.
    R64,
}

impl ClifType {
    /// CLIF-textual name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::B1 => "b1",
            Self::F16 => "f16",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::R64 => "r64",
        }
    }

    /// Width in bytes (B1 counts as 1 for lane-layout purposes).
    #[must_use]
    pub const fn byte_size(self) -> u8 {
        match self {
            Self::I8 | Self::B1 => 1,
            Self::I16 | Self::F16 => 2,
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 | Self::R64 => 8,
        }
    }
}

/// Map a MIR type to a CLIF type, if the MIR type is a scalar.
/// Aggregate / memref / opaque / none types return `None` ; the caller must spill
/// them into memory per calling-convention.
#[must_use]
pub fn clif_type_for(mir: &MirType) -> Option<ClifType> {
    match mir {
        MirType::Int(w) => Some(match w {
            IntWidth::I1 => ClifType::B1,
            IntWidth::I8 => ClifType::I8,
            IntWidth::I16 => ClifType::I16,
            IntWidth::I32 => ClifType::I32,
            IntWidth::I64 => ClifType::I64,
            IntWidth::Index => ClifType::I64,
        }),
        MirType::Float(w) => Some(match w {
            FloatWidth::F16 => ClifType::F16,
            FloatWidth::Bf16 => ClifType::F16, // stage-0 approximation
            FloatWidth::F32 => ClifType::F32,
            FloatWidth::F64 => ClifType::F64,
        }),
        MirType::Bool => Some(ClifType::B1),
        MirType::Handle => Some(ClifType::R64),
        MirType::None | MirType::Tuple(_) | MirType::Function { .. } | MirType::Memref { .. } => {
            None
        }
        MirType::Opaque(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{clif_type_for, ClifType};
    use cssl_mir::{FloatWidth, IntWidth, MirType};

    #[test]
    fn clif_type_names() {
        assert_eq!(ClifType::I32.as_str(), "i32");
        assert_eq!(ClifType::F32.as_str(), "f32");
        assert_eq!(ClifType::B1.as_str(), "b1");
    }

    #[test]
    fn clif_type_byte_size() {
        assert_eq!(ClifType::I8.byte_size(), 1);
        assert_eq!(ClifType::I32.byte_size(), 4);
        assert_eq!(ClifType::I64.byte_size(), 8);
        assert_eq!(ClifType::F64.byte_size(), 8);
    }

    #[test]
    fn mir_int_to_clif() {
        assert_eq!(
            clif_type_for(&MirType::Int(IntWidth::I32)),
            Some(ClifType::I32)
        );
        assert_eq!(
            clif_type_for(&MirType::Int(IntWidth::Index)),
            Some(ClifType::I64)
        );
        assert_eq!(
            clif_type_for(&MirType::Int(IntWidth::I1)),
            Some(ClifType::B1)
        );
    }

    #[test]
    fn mir_float_to_clif() {
        assert_eq!(
            clif_type_for(&MirType::Float(FloatWidth::F32)),
            Some(ClifType::F32)
        );
        assert_eq!(
            clif_type_for(&MirType::Float(FloatWidth::F64)),
            Some(ClifType::F64)
        );
        assert_eq!(
            clif_type_for(&MirType::Float(FloatWidth::Bf16)),
            Some(ClifType::F16)
        );
    }

    #[test]
    fn mir_bool_to_b1() {
        assert_eq!(clif_type_for(&MirType::Bool), Some(ClifType::B1));
    }

    #[test]
    fn mir_handle_to_r64() {
        assert_eq!(clif_type_for(&MirType::Handle), Some(ClifType::R64));
    }

    #[test]
    fn mir_aggregates_are_none() {
        assert_eq!(clif_type_for(&MirType::None), None);
        assert_eq!(clif_type_for(&MirType::Tuple(vec![])), None);
        assert_eq!(
            clif_type_for(&MirType::Function {
                params: vec![],
                results: vec![]
            }),
            None
        );
        assert_eq!(
            clif_type_for(&MirType::Memref {
                shape: vec![],
                elem: Box::new(MirType::Bool)
            }),
            None
        );
        assert_eq!(clif_type_for(&MirType::Opaque("x".into())), None);
    }
}

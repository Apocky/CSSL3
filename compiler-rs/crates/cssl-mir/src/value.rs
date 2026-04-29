//! MIR values + types.
//!
//! § DESIGN
//!   MIR uses SSA values identified by a monotonic `u32`. Types mirror the MLIR
//!   textual format : primitive types are canonical strings (`"i32"`, `"f32"`, `"index"`,
//!   `"!cssl.handle"`, …) and composite types carry an ordered list of inner types.

use core::fmt;

/// Monotonic SSA-value identifier within a `MirFunc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ValueId(pub u32);

impl fmt::Display for ValueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%{}", self.0)
    }
}

/// A typed SSA value reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MirValue {
    pub id: ValueId,
    pub ty: MirType,
}

impl MirValue {
    /// Build a new typed value.
    #[must_use]
    pub const fn new(id: ValueId, ty: MirType) -> Self {
        Self { id, ty }
    }
}

/// MIR type. For stage-0 we store a source-form string (e.g., `"f32"`, `"memref<16xf32>"`)
/// plus structured variants for common cases that need manipulation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MirType {
    /// Integer type : `"i8"`, `"i16"`, `"i32"`, `"i64"`, `"index"`, …
    Int(IntWidth),
    /// Float type : `"f16"`, `"f32"`, `"f64"`, `"bf16"`.
    Float(FloatWidth),
    /// `"i1"` boolean.
    Bool,
    /// `"none"` — absence of value.
    None,
    /// `!cssl.handle` — packed generational reference.
    Handle,
    /// Tuple type : `"tuple<T0, T1, ...>"`.
    Tuple(Vec<MirType>),
    /// Function type : `"(T0, T1, ...) -> (U0, U1, ...)"`.
    Function {
        params: Vec<MirType>,
        results: Vec<MirType>,
    },
    /// Memref : `"memref<shape x elem>"`.
    Memref {
        shape: Vec<Option<u64>>, // None = dynamic
        elem: Box<MirType>,
    },
    /// T11-D31 : fixed-size float vector (`vector<Nxf32>`). Unlocks real
    /// `length(p) - r` sphere-SDF + other vector-valued ops. Lane count is
    /// the u32 (typically 2/3/4/8/16) ; element type is the FloatWidth.
    /// Rendered as MLIR `vector<NxfM>` (e.g., `vector<3xf32>`).
    Vec(u32, FloatWidth),
    /// Opaque / uncategorized — name passed through verbatim.
    Opaque(String),
}

/// Integer bit-width + signedness hint (MLIR `i*`/`si*`/`ui*` forms are unified to
/// signless at stage-0 ; signed/unsigned distinction is an attribute when needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntWidth {
    I1,
    I8,
    I16,
    I32,
    I64,
    Index,
}

impl IntWidth {
    /// Canonical MLIR source-form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I1 => "i1",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::Index => "index",
        }
    }

    /// Natural alignment in bytes for CPU-host targets (T11-D59 / S6-C3).
    /// Used by `memref.load` / `memref.store` lowering to derive the default
    /// alignment when the op's `"alignment"` attribute is absent.
    #[must_use]
    pub const fn natural_alignment(self) -> u32 {
        match self {
            Self::I1 | Self::I8 => 1,
            Self::I16 => 2,
            Self::I32 => 4,
            Self::I64 | Self::Index => 8,
        }
    }
}

/// Float bit-width (MLIR supports `f16`, `bf16`, `f32`, `f64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatWidth {
    F16,
    Bf16,
    F32,
    F64,
}

impl FloatWidth {
    /// Canonical MLIR source-form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::F16 => "f16",
            Self::Bf16 => "bf16",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// Natural alignment in bytes for CPU-host targets (T11-D59 / S6-C3).
    /// Used by `memref.load` / `memref.store` lowering to derive the default
    /// alignment when the op's `"alignment"` attribute is absent.
    #[must_use]
    pub const fn natural_alignment(self) -> u32 {
        match self {
            Self::F16 | Self::Bf16 => 2,
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }
}

impl MirType {
    /// Natural alignment in bytes for CPU-host targets (T11-D59 / S6-C3).
    ///
    /// Returns the byte-alignment a value of this type requires for a regular
    /// (non-overaligned) load or store. Used by `memref.load` / `memref.store`
    /// codegen as the default when the op carries no explicit `"alignment"`
    /// attribute. Composite or non-scalar types return `None` ; callers must
    /// then either supply an explicit alignment attribute or reject the op
    /// as unsupported at stage-0.
    ///
    /// § INVARIANT — never under-align : per `specs/02_IR.csl § MEMORY-OPS`,
    /// an explicit `"alignment"` attribute MUST be ≥ this natural alignment.
    /// Codegen does not currently re-validate that invariant — the lowering
    /// uses whichever value the op carries and trusts the type-checker / pass
    /// pipeline to enforce it.
    #[must_use]
    pub const fn natural_alignment(&self) -> Option<u32> {
        match self {
            Self::Int(w) => Some(w.natural_alignment()),
            Self::Float(w) => Some(w.natural_alignment()),
            Self::Bool => Some(1),
            // Non-scalars : caller must specify explicitly.
            Self::None
            | Self::Handle
            | Self::Tuple(_)
            | Self::Function { .. }
            | Self::Memref { .. }
            | Self::Vec(_, _)
            | Self::Opaque(_) => None,
        }
    }
}

impl fmt::Display for MirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(w) => f.write_str(w.as_str()),
            Self::Float(w) => f.write_str(w.as_str()),
            Self::Bool => f.write_str("i1"),
            Self::None => f.write_str("none"),
            Self::Handle => f.write_str("!cssl.handle"),
            Self::Tuple(elems) => {
                f.write_str("tuple<")?;
                for (i, t) in elems.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{t}")?;
                }
                f.write_str(">")
            }
            Self::Function { params, results } => {
                f.write_str("(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{p}")?;
                }
                f.write_str(") -> ")?;
                if results.len() == 1 {
                    write!(f, "{}", results[0])
                } else {
                    f.write_str("(")?;
                    for (i, r) in results.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{r}")?;
                    }
                    f.write_str(")")
                }
            }
            Self::Memref { shape, elem } => {
                f.write_str("memref<")?;
                for dim in shape {
                    match dim {
                        Some(n) => write!(f, "{n}x")?,
                        None => f.write_str("?x")?,
                    }
                }
                write!(f, "{elem}>")
            }
            Self::Vec(lanes, w) => write!(f, "vector<{lanes}x{}>", w.as_str()),
            Self::Opaque(s) => f.write_str(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FloatWidth, IntWidth, MirType, MirValue, ValueId};

    #[test]
    fn value_id_display() {
        assert_eq!(format!("{}", ValueId(7)), "%7");
    }

    #[test]
    fn int_width_names() {
        assert_eq!(IntWidth::I32.as_str(), "i32");
        assert_eq!(IntWidth::Index.as_str(), "index");
    }

    #[test]
    fn float_width_names() {
        assert_eq!(FloatWidth::F32.as_str(), "f32");
        assert_eq!(FloatWidth::Bf16.as_str(), "bf16");
    }

    #[test]
    fn mir_type_display_primitives() {
        assert_eq!(format!("{}", MirType::Int(IntWidth::I32)), "i32");
        assert_eq!(format!("{}", MirType::Float(FloatWidth::F32)), "f32");
        assert_eq!(format!("{}", MirType::Bool), "i1");
        assert_eq!(format!("{}", MirType::None), "none");
        assert_eq!(format!("{}", MirType::Handle), "!cssl.handle");
    }

    #[test]
    fn mir_type_display_tuple() {
        let t = MirType::Tuple(vec![
            MirType::Int(IntWidth::I32),
            MirType::Float(FloatWidth::F32),
        ]);
        assert_eq!(format!("{t}"), "tuple<i32, f32>");
    }

    #[test]
    fn mir_type_display_function() {
        let t = MirType::Function {
            params: vec![MirType::Int(IntWidth::I32)],
            results: vec![MirType::Bool],
        };
        assert_eq!(format!("{t}"), "(i32) -> i1");
    }

    #[test]
    fn mir_type_display_function_multi_result() {
        let t = MirType::Function {
            params: vec![],
            results: vec![MirType::Int(IntWidth::I32), MirType::Bool],
        };
        assert_eq!(format!("{t}"), "() -> (i32, i1)");
    }

    #[test]
    fn mir_type_display_memref() {
        let t = MirType::Memref {
            shape: vec![Some(4), Some(4)],
            elem: Box::new(MirType::Float(FloatWidth::F32)),
        };
        assert_eq!(format!("{t}"), "memref<4x4xf32>");
    }

    #[test]
    fn mir_type_display_memref_dynamic() {
        let t = MirType::Memref {
            shape: vec![None, Some(3)],
            elem: Box::new(MirType::Float(FloatWidth::F32)),
        };
        assert_eq!(format!("{t}"), "memref<?x3xf32>");
    }

    #[test]
    fn mir_value_carries_type() {
        let v = MirValue::new(ValueId(3), MirType::Int(IntWidth::I32));
        assert_eq!(v.id, ValueId(3));
        assert_eq!(v.ty, MirType::Int(IntWidth::I32));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D31 : MirType::Vec vector types.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn mir_type_display_vec3_f32() {
        let t = MirType::Vec(3, FloatWidth::F32);
        assert_eq!(format!("{t}"), "vector<3xf32>");
    }

    #[test]
    fn mir_type_display_vec4_f32() {
        let t = MirType::Vec(4, FloatWidth::F32);
        assert_eq!(format!("{t}"), "vector<4xf32>");
    }

    #[test]
    fn mir_type_display_vec2_f64() {
        let t = MirType::Vec(2, FloatWidth::F64);
        assert_eq!(format!("{t}"), "vector<2xf64>");
    }

    #[test]
    fn mir_type_display_vec_equality() {
        let a = MirType::Vec(3, FloatWidth::F32);
        let b = MirType::Vec(3, FloatWidth::F32);
        let c = MirType::Vec(4, FloatWidth::F32);
        let d = MirType::Vec(3, FloatWidth::F64);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn mir_type_vec_as_fn_param() {
        // Confirm MirType::Vec can be used as a MirValue type without panicking.
        let v = MirValue::new(ValueId(7), MirType::Vec(3, FloatWidth::F32));
        assert_eq!(v.ty, MirType::Vec(3, FloatWidth::F32));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D59 / S6-C3 : natural-alignment helpers used by memref.load /
    // memref.store lowering.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn int_width_natural_alignment_canonical() {
        assert_eq!(IntWidth::I1.natural_alignment(), 1);
        assert_eq!(IntWidth::I8.natural_alignment(), 1);
        assert_eq!(IntWidth::I16.natural_alignment(), 2);
        assert_eq!(IntWidth::I32.natural_alignment(), 4);
        assert_eq!(IntWidth::I64.natural_alignment(), 8);
        assert_eq!(IntWidth::Index.natural_alignment(), 8);
    }

    #[test]
    fn float_width_natural_alignment_canonical() {
        assert_eq!(FloatWidth::F16.natural_alignment(), 2);
        assert_eq!(FloatWidth::Bf16.natural_alignment(), 2);
        assert_eq!(FloatWidth::F32.natural_alignment(), 4);
        assert_eq!(FloatWidth::F64.natural_alignment(), 8);
    }

    #[test]
    fn mir_type_natural_alignment_scalars() {
        assert_eq!(MirType::Int(IntWidth::I32).natural_alignment(), Some(4));
        assert_eq!(MirType::Int(IntWidth::I64).natural_alignment(), Some(8));
        assert_eq!(MirType::Float(FloatWidth::F32).natural_alignment(), Some(4));
        assert_eq!(MirType::Float(FloatWidth::F64).natural_alignment(), Some(8));
        assert_eq!(MirType::Bool.natural_alignment(), Some(1));
    }

    #[test]
    fn mir_type_natural_alignment_non_scalars_is_none() {
        assert_eq!(MirType::None.natural_alignment(), None);
        assert_eq!(MirType::Handle.natural_alignment(), None);
        assert_eq!(MirType::Tuple(vec![]).natural_alignment(), None);
        assert_eq!(MirType::Vec(3, FloatWidth::F32).natural_alignment(), None);
    }
}

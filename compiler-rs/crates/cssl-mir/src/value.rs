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
}

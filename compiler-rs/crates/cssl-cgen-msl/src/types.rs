//! MSL type-system : scalar / vector / matrix / texture / sampler.
//!
//! § SPEC : `specs/14_BACKEND.csl` § OWNED MSL EMITTER + Apple Metal
//!         Shading Language Specification 3.2 § 2 (Data Types) + § 6 (Functions).
//!
//! § ROLE
//!   Direct Apple-native MSL backend (W-G3 · T11-D269) for the LoA-v13
//!   macOS / iOS path. Distinct from `cssl-cgen-gpu-msl` (spirv-cross-shim
//!   variant) ; this crate emits MSL source-strings without any external
//!   transpiler. Apple's Metal driver compiles source-strings online via
//!   `MTLDevice.makeLibrary(source:options:)`.
//!
//! § COVERAGE
//!   - Scalar types : bool / char / uchar / short / ushort / int / uint /
//!     long / ulong / half / float.
//!   - Vector types : `<scalar><N>` for N ∈ {2,3,4} (e.g. `float4`).
//!   - Matrix types : `<float|half><M>x<N>` for M,N ∈ {2,3,4}.
//!   - Texture types : 1D / 2D / 2DArray / 3D / Cube / CubeArray plus access
//!     qualifiers (sample / read / write / read_write).
//!   - Sampler : opaque `sampler` type.
//!   - Address spaces : device / constant / threadgroup / thread.
//!   - Buffer attributes : `[[buffer(N)]]` / `[[texture(N)]]` /
//!     `[[sampler(N)]]` / `[[stage_in]]` / `[[position]]` /
//!     `[[thread_position_in_grid]]`.

use core::fmt;

/// MSL scalar type ; covers the canonical Metal-Shading-Language § 2.1 set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MslScalar {
    /// `bool` (1 byte logical).
    Bool,
    /// `char` (i8).
    Char,
    /// `uchar` (u8).
    UChar,
    /// `short` (i16).
    Short,
    /// `ushort` (u16).
    UShort,
    /// `int` (i32).
    Int,
    /// `uint` (u32).
    UInt,
    /// `long` (i64).
    Long,
    /// `ulong` (u64).
    ULong,
    /// `half` (16-bit float).
    Half,
    /// `float` (32-bit float).
    Float,
}

impl MslScalar {
    /// Canonical Metal source-form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Char => "char",
            Self::UChar => "uchar",
            Self::Short => "short",
            Self::UShort => "ushort",
            Self::Int => "int",
            Self::UInt => "uint",
            Self::Long => "long",
            Self::ULong => "ulong",
            Self::Half => "half",
            Self::Float => "float",
        }
    }

    /// Width in bytes of the scalar (Metal § 2.1 packed sizes).
    #[must_use]
    pub const fn byte_width(self) -> u32 {
        match self {
            Self::Bool | Self::Char | Self::UChar => 1,
            Self::Short | Self::UShort | Self::Half => 2,
            Self::Int | Self::UInt | Self::Float => 4,
            Self::Long | Self::ULong => 8,
        }
    }

    /// `true` iff this scalar can occupy a vector lane (matrices require
    /// half / float per Metal § 2.3).
    #[must_use]
    pub const fn is_floating(self) -> bool {
        matches!(self, Self::Half | Self::Float)
    }
}

impl fmt::Display for MslScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// MSL composite type. Covers all forms accepted in entry-function signatures
/// or buffer / texture bindings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MslType {
    /// Scalar (e.g. `float`).
    Scalar(MslScalar),
    /// Vector with 2..4 lanes (e.g. `float3`).
    Vector(MslScalar, u32),
    /// Matrix `<scalar><M>x<N>` — only `half` / `float` are legal Metal
    /// matrix-element types.
    Matrix(MslScalar, u32, u32),
    /// Texture variant + sampled element + access qualifier.
    Texture {
        kind: TextureKind,
        elem: MslScalar,
        access: TextureAccess,
    },
    /// Opaque `sampler`.
    Sampler,
    /// Pointer-to type with explicit address space (e.g. `device float*`).
    Pointer {
        address_space: AddressSpace,
        pointee: Box<MslType>,
    },
    /// Named user struct (forward-reference into module struct table).
    Struct(String),
    /// `void` — only legal in function-return position.
    Void,
}

impl MslType {
    /// Convenience constructor for a `device <pointee>*` pointer.
    #[must_use]
    pub fn device_ptr(pointee: MslType) -> Self {
        Self::Pointer {
            address_space: AddressSpace::Device,
            pointee: Box::new(pointee),
        }
    }

    /// Convenience constructor for a `constant <pointee>*` pointer.
    #[must_use]
    pub fn constant_ptr(pointee: MslType) -> Self {
        Self::Pointer {
            address_space: AddressSpace::Constant,
            pointee: Box::new(pointee),
        }
    }

    /// Convenience constructor for a `threadgroup <pointee>*` pointer.
    #[must_use]
    pub fn threadgroup_ptr(pointee: MslType) -> Self {
        Self::Pointer {
            address_space: AddressSpace::Threadgroup,
            pointee: Box::new(pointee),
        }
    }
}

impl fmt::Display for MslType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(s) => write!(f, "{s}"),
            Self::Vector(s, n) => write!(f, "{s}{n}"),
            Self::Matrix(s, m, n) => write!(f, "{s}{m}x{n}"),
            Self::Texture { kind, elem, access } => {
                write!(f, "{}<{elem}, access::{}>", kind.as_str(), access.as_str())
            }
            Self::Sampler => f.write_str("sampler"),
            Self::Pointer {
                address_space,
                pointee,
            } => write!(f, "{} {pointee}*", address_space.as_str()),
            Self::Struct(name) => f.write_str(name),
            Self::Void => f.write_str("void"),
        }
    }
}

/// Texture variant per Metal § 2.9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureKind {
    /// `texture1d`.
    D1,
    /// `texture2d`.
    D2,
    /// `texture2d_array`.
    D2Array,
    /// `texture3d`.
    D3,
    /// `texturecube`.
    Cube,
    /// `texturecube_array`.
    CubeArray,
}

impl TextureKind {
    /// Metal source-form name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::D1 => "texture1d",
            Self::D2 => "texture2d",
            Self::D2Array => "texture2d_array",
            Self::D3 => "texture3d",
            Self::Cube => "texturecube",
            Self::CubeArray => "texturecube_array",
        }
    }
}

/// Texture access qualifier per Metal § 2.9.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureAccess {
    /// `access::sample` — default ; supports filtered sampling.
    Sample,
    /// `access::read` — load-only without filtering.
    Read,
    /// `access::write` — store-only.
    Write,
    /// `access::read_write` — Metal-2.4+.
    ReadWrite,
}

impl TextureAccess {
    /// Metal source-form name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sample => "sample",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read_write",
        }
    }
}

/// Metal address-space qualifier per Metal § 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    /// `device` — read-write device-memory buffer.
    Device,
    /// `constant` — read-only device-memory buffer (small / cached).
    Constant,
    /// `threadgroup` — workgroup-shared memory.
    Threadgroup,
    /// `thread` — per-thread private (default for locals).
    Thread,
}

impl AddressSpace {
    /// Metal source-form keyword.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::Constant => "constant",
            Self::Threadgroup => "threadgroup",
            Self::Thread => "thread",
        }
    }
}

/// MSL parameter binding attribute.
///
/// Encapsulates the `[[buffer(N)]]` / `[[texture(N)]]` / `[[sampler(N)]]`
/// stage-input / builtin attributes attached to entry-function parameters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindAttr {
    /// `[[buffer(N)]]`.
    Buffer(u32),
    /// `[[texture(N)]]`.
    Texture(u32),
    /// `[[sampler(N)]]`.
    Sampler(u32),
    /// `[[stage_in]]` — vertex / fragment input struct.
    StageIn,
    /// `[[position]]` — vertex output / fragment input clip-space position.
    Position,
    /// `[[thread_position_in_grid]]` — kernel global thread index.
    ThreadPositionInGrid,
    /// `[[thread_position_in_threadgroup]]` — kernel local thread index.
    ThreadPositionInThreadgroup,
    /// `[[threadgroup_position_in_grid]]` — kernel workgroup index.
    ThreadgroupPositionInGrid,
    /// `[[vertex_id]]` — vertex stage builtin index.
    VertexId,
    /// `[[instance_id]]` — vertex stage instance index.
    InstanceId,
    /// `[[color(N)]]` — fragment output target.
    Color(u32),
    /// `[[point_size]]` — vertex point-size builtin.
    PointSize,
    /// `[[user(name)]]` — passthrough custom attribute.
    User(String),
}

impl BindAttr {
    /// Render this attribute as a Metal `[[…]]` bracketed string.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Buffer(n) => format!("[[buffer({n})]]"),
            Self::Texture(n) => format!("[[texture({n})]]"),
            Self::Sampler(n) => format!("[[sampler({n})]]"),
            Self::StageIn => "[[stage_in]]".into(),
            Self::Position => "[[position]]".into(),
            Self::ThreadPositionInGrid => "[[thread_position_in_grid]]".into(),
            Self::ThreadPositionInThreadgroup => "[[thread_position_in_threadgroup]]".into(),
            Self::ThreadgroupPositionInGrid => "[[threadgroup_position_in_grid]]".into(),
            Self::VertexId => "[[vertex_id]]".into(),
            Self::InstanceId => "[[instance_id]]".into(),
            Self::Color(n) => format!("[[color({n})]]"),
            Self::PointSize => "[[point_size]]".into(),
            Self::User(s) => format!("[[user({s})]]"),
        }
    }
}

/// Map a `cssl_mir::FloatWidth` to its MSL scalar.
///
/// `bf16` is not natively representable in MSL ; it folds to `half` (the
/// closest available 16-bit float). Callers requiring strict bf16 should
/// emit a comptime error before reaching codegen.
#[must_use]
pub const fn float_to_msl(w: cssl_mir::FloatWidth) -> MslScalar {
    match w {
        cssl_mir::FloatWidth::F16 | cssl_mir::FloatWidth::Bf16 => MslScalar::Half,
        // F32 + F64 collapse to MSL `float` ; Metal § 2.1 explicitly disallows
        // double, so F64 is downgraded to F32 with a documented caveat.
        cssl_mir::FloatWidth::F32 | cssl_mir::FloatWidth::F64 => MslScalar::Float,
    }
}

/// Map a `cssl_mir::IntWidth` to its MSL scalar.
///
/// `index` is treated as `uint` (Metal kernels typically index with `uint`).
#[must_use]
pub const fn int_to_msl(w: cssl_mir::IntWidth) -> MslScalar {
    match w {
        cssl_mir::IntWidth::I1 => MslScalar::Bool,
        cssl_mir::IntWidth::I8 => MslScalar::Char,
        cssl_mir::IntWidth::I16 => MslScalar::Short,
        cssl_mir::IntWidth::I32 | cssl_mir::IntWidth::Index => MslScalar::Int,
        cssl_mir::IntWidth::I64 => MslScalar::Long,
    }
}

/// Map a `cssl_mir::MirType` to its MSL representation.
///
/// Fallible : `MirType::Tuple` / `MirType::Function` / `MirType::Memref` /
/// `MirType::Handle` are not Metal-emittable as parameter types and surface
/// as `None`. The driver lifts them to errors at the lower level.
#[must_use]
pub fn mir_type_to_msl(t: &cssl_mir::MirType) -> Option<MslType> {
    match t {
        cssl_mir::MirType::Int(w) => Some(MslType::Scalar(int_to_msl(*w))),
        cssl_mir::MirType::Float(w) => Some(MslType::Scalar(float_to_msl(*w))),
        cssl_mir::MirType::Bool => Some(MslType::Scalar(MslScalar::Bool)),
        cssl_mir::MirType::None => Some(MslType::Void),
        cssl_mir::MirType::Vec(lanes, w) => {
            // Metal vectors only legal at 2/3/4 lanes.
            if matches!(lanes, 2..=4) {
                Some(MslType::Vector(float_to_msl(*w), *lanes))
            } else {
                None
            }
        }
        cssl_mir::MirType::Ptr => Some(MslType::device_ptr(MslType::Scalar(MslScalar::UChar))),
        // Unsupported in stage-0 cgen-msl — caller handles.
        cssl_mir::MirType::Handle
        | cssl_mir::MirType::Tuple(_)
        | cssl_mir::MirType::Function { .. }
        | cssl_mir::MirType::Memref { .. }
        | cssl_mir::MirType::Opaque(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        float_to_msl, int_to_msl, mir_type_to_msl, AddressSpace, BindAttr, MslScalar, MslType,
        TextureAccess, TextureKind,
    };
    use cssl_mir::{FloatWidth, IntWidth, MirType};

    #[test]
    fn scalar_canonical_strings() {
        assert_eq!(MslScalar::Float.as_str(), "float");
        assert_eq!(MslScalar::Half.as_str(), "half");
        assert_eq!(MslScalar::UInt.as_str(), "uint");
        assert_eq!(MslScalar::UChar.as_str(), "uchar");
    }

    #[test]
    fn scalar_byte_widths() {
        assert_eq!(MslScalar::UChar.byte_width(), 1);
        assert_eq!(MslScalar::Half.byte_width(), 2);
        assert_eq!(MslScalar::Float.byte_width(), 4);
        assert_eq!(MslScalar::Long.byte_width(), 8);
    }

    #[test]
    fn vector_renders_with_lane_count() {
        let v = MslType::Vector(MslScalar::Float, 4);
        assert_eq!(format!("{v}"), "float4");
    }

    #[test]
    fn matrix_renders_with_dims() {
        let m = MslType::Matrix(MslScalar::Float, 4, 4);
        assert_eq!(format!("{m}"), "float4x4");
    }

    #[test]
    fn texture_with_access_renders() {
        let t = MslType::Texture {
            kind: TextureKind::D2,
            elem: MslScalar::Float,
            access: TextureAccess::Sample,
        };
        assert_eq!(format!("{t}"), "texture2d<float, access::sample>");
    }

    #[test]
    fn pointer_with_address_space_renders() {
        let p = MslType::device_ptr(MslType::Scalar(MslScalar::Float));
        assert_eq!(format!("{p}"), "device float*");

        let c = MslType::constant_ptr(MslType::Scalar(MslScalar::UInt));
        assert_eq!(format!("{c}"), "constant uint*");

        let g = MslType::threadgroup_ptr(MslType::Scalar(MslScalar::Half));
        assert_eq!(format!("{g}"), "threadgroup half*");
    }

    #[test]
    fn bind_attr_buffer_renders() {
        assert_eq!(BindAttr::Buffer(0).render(), "[[buffer(0)]]");
        assert_eq!(BindAttr::Texture(2).render(), "[[texture(2)]]");
        assert_eq!(BindAttr::Sampler(1).render(), "[[sampler(1)]]");
    }

    #[test]
    fn bind_attr_builtins_render() {
        assert_eq!(BindAttr::StageIn.render(), "[[stage_in]]");
        assert_eq!(BindAttr::Position.render(), "[[position]]");
        assert_eq!(
            BindAttr::ThreadPositionInGrid.render(),
            "[[thread_position_in_grid]]"
        );
        assert_eq!(BindAttr::VertexId.render(), "[[vertex_id]]");
        assert_eq!(BindAttr::Color(3).render(), "[[color(3)]]");
    }

    #[test]
    fn address_space_keywords() {
        assert_eq!(AddressSpace::Device.as_str(), "device");
        assert_eq!(AddressSpace::Constant.as_str(), "constant");
        assert_eq!(AddressSpace::Threadgroup.as_str(), "threadgroup");
        assert_eq!(AddressSpace::Thread.as_str(), "thread");
    }

    #[test]
    fn float_width_mapping() {
        assert_eq!(float_to_msl(FloatWidth::F32), MslScalar::Float);
        assert_eq!(float_to_msl(FloatWidth::F16), MslScalar::Half);
        assert_eq!(float_to_msl(FloatWidth::Bf16), MslScalar::Half);
        // F64 is downgraded to Float with documented caveat.
        assert_eq!(float_to_msl(FloatWidth::F64), MslScalar::Float);
    }

    #[test]
    fn int_width_mapping() {
        assert_eq!(int_to_msl(IntWidth::I1), MslScalar::Bool);
        assert_eq!(int_to_msl(IntWidth::I8), MslScalar::Char);
        assert_eq!(int_to_msl(IntWidth::I16), MslScalar::Short);
        assert_eq!(int_to_msl(IntWidth::I32), MslScalar::Int);
        assert_eq!(int_to_msl(IntWidth::Index), MslScalar::Int);
        assert_eq!(int_to_msl(IntWidth::I64), MslScalar::Long);
    }

    #[test]
    fn mir_type_scalars_map_through() {
        let f = mir_type_to_msl(&MirType::Float(FloatWidth::F32)).unwrap();
        assert_eq!(format!("{f}"), "float");

        let i = mir_type_to_msl(&MirType::Int(IntWidth::I32)).unwrap();
        assert_eq!(format!("{i}"), "int");

        let b = mir_type_to_msl(&MirType::Bool).unwrap();
        assert_eq!(format!("{b}"), "bool");
    }

    #[test]
    fn mir_type_vectors_map_through() {
        let v3 = mir_type_to_msl(&MirType::Vec(3, FloatWidth::F32)).unwrap();
        assert_eq!(format!("{v3}"), "float3");

        let v4 = mir_type_to_msl(&MirType::Vec(4, FloatWidth::F16)).unwrap();
        assert_eq!(format!("{v4}"), "half4");
    }

    #[test]
    fn mir_type_invalid_vector_lanes_rejected() {
        // Metal disallows 5/8/16-lane vectors at the source-text level.
        assert!(mir_type_to_msl(&MirType::Vec(5, FloatWidth::F32)).is_none());
        assert!(mir_type_to_msl(&MirType::Vec(8, FloatWidth::F32)).is_none());
    }

    #[test]
    fn mir_type_handle_unsupported() {
        assert!(mir_type_to_msl(&MirType::Handle).is_none());
        assert!(mir_type_to_msl(&MirType::Memref {
            shape: vec![Some(4)],
            elem: Box::new(MirType::Float(FloatWidth::F32))
        })
        .is_none());
    }
}

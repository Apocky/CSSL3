//! WGSL type-system + `MirType → WgslType` mapping.
//!
//! § SPEC : W3C "WebGPU Shading Language" §§ Types — scalars, vectors,
//!         matrices, arrays, textures, samplers.
//!
//! § STAGE-0 NARROWING
//!   - i64 / f64 narrowed to i32 / f32 (WGSL has no native 64-bit scalars).
//!   - bf16 rendered as `f16` (WGSL gates `f16` behind the `shader-f16`
//!     feature ; downstream consumers must check feature-bits).
//!   - `index` rendered as `u32` (WebGPU dispatch indices are u32).
//!   - `MirType::Handle` / `MirType::Ptr` are *not legal* in WGSL — they
//!     are CPU-side concepts. The mapping rejects them with
//!     [`WgslTypeError::Unsupported`].

use core::fmt;
use cssl_mir::value::{FloatWidth, IntWidth, MirType};
use thiserror::Error;

/// A WGSL surface-level type. Maps 1:1 to a WGSL type-spelling fragment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WgslType {
    /// 32-bit signed integer (`i32`).
    I32,
    /// 32-bit unsigned integer (`u32`).
    U32,
    /// 32-bit IEEE float (`f32`).
    F32,
    /// 16-bit IEEE float (`f16` — gated on `shader-f16` feature).
    F16,
    /// Boolean scalar (`bool`).
    Bool,
    /// Fixed-size vector of N f32 lanes (`vec2<f32>` / `vec3<f32>` / `vec4<f32>`).
    /// Lane count must be 2, 3, or 4 per WGSL spec.
    VecF32(u32),
    /// Fixed-size vector of N i32 lanes.
    VecI32(u32),
    /// Fixed-size vector of N u32 lanes.
    VecU32(u32),
    /// Fixed-size matrix `matRxC<f32>` (row × col both 2..=4).
    MatF32 { rows: u32, cols: u32 },
    /// Sized array `array<T, N>` (must wrap in storage / uniform addr-space at
    /// the binding boundary ; here we just encode the spelling).
    Array { elem: Box<WgslType>, len: Option<u64> },
    /// 2D sampled texture (`texture_2d<f32>`).
    Texture2dF32,
    /// 2D storage texture (`texture_storage_2d<rgba8unorm, write>`).
    StorageTexture2dRgba8,
    /// `sampler` — opaque sampler binding.
    Sampler,
    /// Atomic u32 (`atomic<u32>`) — used in storage-buffer counters.
    AtomicU32,
}

impl WgslType {
    /// `true` iff this type is a *resource* type (texture / sampler / atomic /
    /// runtime-sized array) and therefore must appear at a binding-decorator
    /// boundary rather than as a plain function parameter.
    #[must_use]
    pub const fn is_resource(&self) -> bool {
        matches!(
            self,
            Self::Texture2dF32
                | Self::StorageTexture2dRgba8
                | Self::Sampler
                | Self::AtomicU32
        )
    }
}

impl fmt::Display for WgslType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I32 => f.write_str("i32"),
            Self::U32 => f.write_str("u32"),
            Self::F32 => f.write_str("f32"),
            Self::F16 => f.write_str("f16"),
            Self::Bool => f.write_str("bool"),
            Self::VecF32(n) => write!(f, "vec{n}<f32>"),
            Self::VecI32(n) => write!(f, "vec{n}<i32>"),
            Self::VecU32(n) => write!(f, "vec{n}<u32>"),
            Self::MatF32 { rows, cols } => write!(f, "mat{rows}x{cols}<f32>"),
            Self::Array { elem, len } => match len {
                Some(n) => write!(f, "array<{elem}, {n}>"),
                None => write!(f, "array<{elem}>"),
            },
            Self::Texture2dF32 => f.write_str("texture_2d<f32>"),
            Self::StorageTexture2dRgba8 => {
                f.write_str("texture_storage_2d<rgba8unorm, write>")
            }
            Self::Sampler => f.write_str("sampler"),
            Self::AtomicU32 => f.write_str("atomic<u32>"),
        }
    }
}

/// Error returned by [`wgsl_type_for`] when the input `MirType` has no WGSL
/// surface representation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WgslTypeError {
    /// Type cannot appear in WGSL source (e.g., `!cssl.handle`, `!cssl.ptr`,
    /// `none`, function-types as values).
    #[error("type `{0}` cannot be represented in WGSL")]
    Unsupported(String),
    /// Vector / matrix lane-count out of WGSL's 2..=4 range.
    #[error("invalid lane count {0} (WGSL requires 2..=4)")]
    InvalidLaneCount(u32),
}

/// Map a `MirType` to its WGSL surface-level form.
///
/// § ERRORS · returns [`WgslTypeError::Unsupported`] for handles, pointers,
/// none-types, and function-types as values ; returns
/// [`WgslTypeError::InvalidLaneCount`] for out-of-range vector lanes.
pub fn wgsl_type_for(ty: &MirType) -> Result<WgslType, WgslTypeError> {
    match ty {
        MirType::Bool => Ok(WgslType::Bool),
        MirType::Int(IntWidth::I1) => Ok(WgslType::Bool),
        MirType::Int(IntWidth::I8 | IntWidth::I16 | IntWidth::I32) => Ok(WgslType::I32),
        // i64 narrowed to i32 at stage-0 per §§ 14.
        MirType::Int(IntWidth::I64) => Ok(WgslType::I32),
        MirType::Int(IntWidth::Index) => Ok(WgslType::U32),
        MirType::Float(FloatWidth::F32) => Ok(WgslType::F32),
        MirType::Float(FloatWidth::F64) => Ok(WgslType::F32), // narrowed.
        MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => Ok(WgslType::F16),
        MirType::Vec(lanes, FloatWidth::F32) => {
            if matches!(lanes, 2..=4) {
                Ok(WgslType::VecF32(*lanes))
            } else {
                Err(WgslTypeError::InvalidLaneCount(*lanes))
            }
        }
        MirType::Vec(lanes, FloatWidth::F16 | FloatWidth::Bf16) => {
            // Render f16 vectors as f32 vectors at stage-0 unless `shader-f16`
            // is active. Keep the lane-count check.
            if matches!(lanes, 2..=4) {
                Ok(WgslType::VecF32(*lanes))
            } else {
                Err(WgslTypeError::InvalidLaneCount(*lanes))
            }
        }
        MirType::Vec(lanes, FloatWidth::F64) => {
            if matches!(lanes, 2..=4) {
                Ok(WgslType::VecF32(*lanes))
            } else {
                Err(WgslTypeError::InvalidLaneCount(*lanes))
            }
        }
        MirType::Memref { shape, elem } => {
            let elem_w = wgsl_type_for(elem)?;
            // 1D shapes : single dim ; runtime-size if None.
            if shape.len() == 1 {
                Ok(WgslType::Array {
                    elem: Box::new(elem_w),
                    len: shape[0],
                })
            } else if shape.is_empty() {
                Ok(elem_w)
            } else {
                // Multi-dim : flatten as length-product if all known, else runtime.
                let total = shape.iter().try_fold(1u64, |acc, dim| dim.map(|d| acc * d));
                Ok(WgslType::Array {
                    elem: Box::new(elem_w),
                    len: total,
                })
            }
        }
        MirType::Opaque(name) => match name.as_str() {
            "u32" => Ok(WgslType::U32),
            "texture_2d" => Ok(WgslType::Texture2dF32),
            "texture_storage_2d" => Ok(WgslType::StorageTexture2dRgba8),
            "sampler" => Ok(WgslType::Sampler),
            "atomic_u32" => Ok(WgslType::AtomicU32),
            other => Err(WgslTypeError::Unsupported(other.to_string())),
        },
        MirType::Handle => Err(WgslTypeError::Unsupported("!cssl.handle".into())),
        MirType::Ptr => Err(WgslTypeError::Unsupported("!cssl.ptr".into())),
        MirType::None => Err(WgslTypeError::Unsupported("none".into())),
        MirType::Tuple(_) => Err(WgslTypeError::Unsupported("tuple".into())),
        MirType::Function { .. } => {
            Err(WgslTypeError::Unsupported("function-as-value".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_mappings_render_correctly() {
        assert_eq!(WgslType::I32.to_string(), "i32");
        assert_eq!(WgslType::U32.to_string(), "u32");
        assert_eq!(WgslType::F32.to_string(), "f32");
        assert_eq!(WgslType::F16.to_string(), "f16");
        assert_eq!(WgslType::Bool.to_string(), "bool");
    }

    #[test]
    fn vec_and_mat_render_correctly() {
        assert_eq!(WgslType::VecF32(3).to_string(), "vec3<f32>");
        assert_eq!(WgslType::VecI32(4).to_string(), "vec4<i32>");
        assert_eq!(
            WgslType::MatF32 { rows: 4, cols: 4 }.to_string(),
            "mat4x4<f32>"
        );
    }

    #[test]
    fn array_runtime_and_sized_render_correctly() {
        let runtime = WgslType::Array { elem: Box::new(WgslType::F32), len: None };
        assert_eq!(runtime.to_string(), "array<f32>");
        let sized = WgslType::Array { elem: Box::new(WgslType::U32), len: Some(64) };
        assert_eq!(sized.to_string(), "array<u32, 64>");
    }

    #[test]
    fn mir_to_wgsl_scalar_mapping() {
        assert_eq!(
            wgsl_type_for(&MirType::Float(FloatWidth::F32)).unwrap(),
            WgslType::F32
        );
        assert_eq!(
            wgsl_type_for(&MirType::Int(IntWidth::I32)).unwrap(),
            WgslType::I32
        );
        // i64 narrows to i32.
        assert_eq!(
            wgsl_type_for(&MirType::Int(IntWidth::I64)).unwrap(),
            WgslType::I32
        );
        // index → u32.
        assert_eq!(
            wgsl_type_for(&MirType::Int(IntWidth::Index)).unwrap(),
            WgslType::U32
        );
    }

    #[test]
    fn mir_to_wgsl_vec_mapping() {
        let v3 = MirType::Vec(3, FloatWidth::F32);
        assert_eq!(wgsl_type_for(&v3).unwrap(), WgslType::VecF32(3));
        let bad = MirType::Vec(8, FloatWidth::F32);
        assert!(matches!(
            wgsl_type_for(&bad),
            Err(WgslTypeError::InvalidLaneCount(8))
        ));
    }

    #[test]
    fn mir_to_wgsl_memref_mapping() {
        let buf = MirType::Memref {
            shape: vec![Some(64)],
            elem: Box::new(MirType::Float(FloatWidth::F32)),
        };
        let w = wgsl_type_for(&buf).unwrap();
        assert_eq!(w.to_string(), "array<f32, 64>");
        let runtime = MirType::Memref {
            shape: vec![None],
            elem: Box::new(MirType::Float(FloatWidth::F32)),
        };
        assert_eq!(wgsl_type_for(&runtime).unwrap().to_string(), "array<f32>");
    }

    #[test]
    fn handle_and_ptr_rejected() {
        assert!(matches!(
            wgsl_type_for(&MirType::Handle),
            Err(WgslTypeError::Unsupported(_))
        ));
        assert!(matches!(
            wgsl_type_for(&MirType::Ptr),
            Err(WgslTypeError::Unsupported(_))
        ));
        assert!(matches!(
            wgsl_type_for(&MirType::None),
            Err(WgslTypeError::Unsupported(_))
        ));
    }

    #[test]
    fn opaque_resource_types() {
        assert_eq!(
            wgsl_type_for(&MirType::Opaque("texture_2d".into())).unwrap(),
            WgslType::Texture2dF32
        );
        assert_eq!(
            wgsl_type_for(&MirType::Opaque("sampler".into())).unwrap(),
            WgslType::Sampler
        );
        assert!(WgslType::Sampler.is_resource());
        assert!(WgslType::Texture2dF32.is_resource());
        assert!(!WgslType::F32.is_resource());
    }
}

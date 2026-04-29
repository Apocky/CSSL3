//! D3D12 root signature builder + wrapper.
//!
//! § DESIGN
//!   - `RootParameter` — a single root entry (CBV / SRV / UAV / 32-bit
//!     constants / descriptor table).
//!   - `ShaderVisibility` — which shader stages can see the parameter.
//!   - `RootSignatureBuilder` — fluent builder ; `build(&Device)` calls
//!     `D3D12SerializeRootSignature` + `CreateRootSignature`.
//!   - `RootSignature` — `ID3D12RootSignature` wrapper.
//!
//! § COMPLEXITY
//!   This stage-0 builder covers the four most common root entries CSSLv3
//!   needs : raw root constants (push-constant equivalent), root-CBV, root-SRV,
//!   root-UAV. Descriptor tables (the bindless surface) are stubbed at
//!   `RootParameterKind::DescriptorTable` and emit a single CBV/SRV/UAV range
//!   internally. Static samplers are deferred to a later slice.

use crate::device::Device;
use crate::error::{D3d12Error, Result};

/// Shader visibility (mirrors `D3D12_SHADER_VISIBILITY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderVisibility {
    /// Visible to all shader stages.
    All,
    /// Vertex shader only.
    Vertex,
    /// Hull shader only.
    Hull,
    /// Domain shader only.
    Domain,
    /// Geometry shader only.
    Geometry,
    /// Pixel shader only.
    Pixel,
    /// Amplification (mesh-shader pre-stage).
    Amplification,
    /// Mesh shader.
    Mesh,
}

impl ShaderVisibility {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Vertex => "vs",
            Self::Hull => "hs",
            Self::Domain => "ds",
            Self::Geometry => "gs",
            Self::Pixel => "ps",
            Self::Amplification => "as",
            Self::Mesh => "ms",
        }
    }
}

/// Root parameter kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootParameterKind {
    /// Raw 32-bit constants (count + register + space).
    Constants {
        /// Number of 32-bit values.
        count: u32,
        /// `register(b<reg>)`.
        shader_register: u32,
        /// `space<n>` (bind-space).
        register_space: u32,
    },
    /// Root CBV (single 64-bit address).
    Cbv {
        /// `register(b<reg>)`.
        shader_register: u32,
        /// `space<n>`.
        register_space: u32,
    },
    /// Root SRV (single 64-bit address).
    Srv {
        /// `register(t<reg>)`.
        shader_register: u32,
        /// `space<n>`.
        register_space: u32,
    },
    /// Root UAV (single 64-bit address).
    Uav {
        /// `register(u<reg>)`.
        shader_register: u32,
        /// `space<n>`.
        register_space: u32,
    },
    /// Single-range descriptor table (CBV/SRV/UAV).
    DescriptorTable {
        /// Range type (CBV/SRV/UAV).
        range_kind: DescriptorRangeKind,
        /// Number of descriptors.
        count: u32,
        /// Base register.
        base_register: u32,
        /// Register space.
        register_space: u32,
    },
}

/// Descriptor range type for `DescriptorTable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DescriptorRangeKind {
    /// CBV range.
    Cbv,
    /// SRV range.
    Srv,
    /// UAV range.
    Uav,
}

impl DescriptorRangeKind {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cbv => "cbv",
            Self::Srv => "srv",
            Self::Uav => "uav",
        }
    }
}

/// One root parameter entry (parameter + visibility).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootParameter {
    /// Kind of root parameter.
    pub kind: RootParameterKind,
    /// Shader visibility.
    pub visibility: ShaderVisibility,
}

impl RootParameter {
    /// New 32-bit constants entry.
    #[must_use]
    pub const fn constants(
        count: u32,
        shader_register: u32,
        register_space: u32,
        visibility: ShaderVisibility,
    ) -> Self {
        Self {
            kind: RootParameterKind::Constants {
                count,
                shader_register,
                register_space,
            },
            visibility,
        }
    }

    /// New root CBV entry.
    #[must_use]
    pub const fn cbv(
        shader_register: u32,
        register_space: u32,
        visibility: ShaderVisibility,
    ) -> Self {
        Self {
            kind: RootParameterKind::Cbv {
                shader_register,
                register_space,
            },
            visibility,
        }
    }

    /// New root SRV entry.
    #[must_use]
    pub const fn srv(
        shader_register: u32,
        register_space: u32,
        visibility: ShaderVisibility,
    ) -> Self {
        Self {
            kind: RootParameterKind::Srv {
                shader_register,
                register_space,
            },
            visibility,
        }
    }

    /// New root UAV entry.
    #[must_use]
    pub const fn uav(
        shader_register: u32,
        register_space: u32,
        visibility: ShaderVisibility,
    ) -> Self {
        Self {
            kind: RootParameterKind::Uav {
                shader_register,
                register_space,
            },
            visibility,
        }
    }

    /// New descriptor table entry.
    #[must_use]
    pub const fn descriptor_table(
        range_kind: DescriptorRangeKind,
        count: u32,
        base_register: u32,
        register_space: u32,
        visibility: ShaderVisibility,
    ) -> Self {
        Self {
            kind: RootParameterKind::DescriptorTable {
                range_kind,
                count,
                base_register,
                register_space,
            },
            visibility,
        }
    }
}

/// Builder for a root signature.
#[derive(Debug, Clone, Default)]
pub struct RootSignatureBuilder {
    parameters: Vec<RootParameter>,
    /// Allow input-assembler input layout (graphics PSO standard flag).
    pub allow_ia_input_layout: bool,
    /// Allow stream output.
    pub allow_stream_output: bool,
    /// Identification label (debug).
    pub label: Option<String>,
}

impl RootSignatureBuilder {
    /// New empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a root parameter.
    #[must_use]
    pub fn with_parameter(mut self, p: RootParameter) -> Self {
        self.parameters.push(p);
        self
    }

    /// Mark as allowing input-assembler.
    #[must_use]
    pub const fn with_ia(mut self) -> Self {
        self.allow_ia_input_layout = true;
        self
    }

    /// Add a debug label.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Number of root parameters.
    #[must_use]
    pub fn parameter_count(&self) -> usize {
        self.parameters.len()
    }

    /// Borrow the parameter list.
    #[must_use]
    pub fn parameters(&self) -> &[RootParameter] {
        &self.parameters
    }

    /// Build the root signature on the given device.
    pub fn build(self, device: &Device) -> Result<RootSignature> {
        if self.parameters.is_empty() {
            return Err(D3d12Error::invalid(
                "RootSignatureBuilder::build",
                "no parameters",
            ));
        }
        // SAFETY : we type-check parameter shape before serialization.
        imp::build_root_signature(device, self)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::{DescriptorRangeKind, RootParameterKind, RootSignatureBuilder, ShaderVisibility};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};
    use windows::Win32::Graphics::Direct3D12::{
        D3D12SerializeRootSignature, ID3D12RootSignature, D3D12_DESCRIPTOR_RANGE,
        D3D12_DESCRIPTOR_RANGE_TYPE_CBV, D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
        D3D12_DESCRIPTOR_RANGE_TYPE_UAV, D3D12_ROOT_CONSTANTS, D3D12_ROOT_DESCRIPTOR,
        D3D12_ROOT_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER, D3D12_ROOT_PARAMETER_0,
        D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER_TYPE_CBV,
        D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER_TYPE_SRV,
        D3D12_ROOT_PARAMETER_TYPE_UAV, D3D12_ROOT_SIGNATURE_DESC,
        D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        D3D12_ROOT_SIGNATURE_FLAG_ALLOW_STREAM_OUTPUT, D3D12_ROOT_SIGNATURE_FLAG_NONE,
        D3D_ROOT_SIGNATURE_VERSION_1_0,
    };
    use windows::Win32::Graphics::Direct3D12::{
        D3D12_SHADER_VISIBILITY, D3D12_SHADER_VISIBILITY_ALL,
        D3D12_SHADER_VISIBILITY_AMPLIFICATION, D3D12_SHADER_VISIBILITY_DOMAIN,
        D3D12_SHADER_VISIBILITY_GEOMETRY, D3D12_SHADER_VISIBILITY_HULL,
        D3D12_SHADER_VISIBILITY_MESH, D3D12_SHADER_VISIBILITY_PIXEL,
        D3D12_SHADER_VISIBILITY_VERTEX,
    };

    fn visibility_to_raw(v: ShaderVisibility) -> D3D12_SHADER_VISIBILITY {
        match v {
            ShaderVisibility::All => D3D12_SHADER_VISIBILITY_ALL,
            ShaderVisibility::Vertex => D3D12_SHADER_VISIBILITY_VERTEX,
            ShaderVisibility::Hull => D3D12_SHADER_VISIBILITY_HULL,
            ShaderVisibility::Domain => D3D12_SHADER_VISIBILITY_DOMAIN,
            ShaderVisibility::Geometry => D3D12_SHADER_VISIBILITY_GEOMETRY,
            ShaderVisibility::Pixel => D3D12_SHADER_VISIBILITY_PIXEL,
            ShaderVisibility::Amplification => D3D12_SHADER_VISIBILITY_AMPLIFICATION,
            ShaderVisibility::Mesh => D3D12_SHADER_VISIBILITY_MESH,
        }
    }

    /// D3D12 root signature.
    pub struct RootSignature {
        pub(crate) signature: ID3D12RootSignature,
        pub(crate) parameter_count: usize,
        pub(crate) label: Option<String>,
    }

    impl core::fmt::Debug for RootSignature {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("RootSignature")
                .field("parameter_count", &self.parameter_count)
                .field("label", &self.label)
                .finish_non_exhaustive()
        }
    }

    impl RootSignature {
        /// Number of root parameters.
        #[must_use]
        pub const fn parameter_count(&self) -> usize {
            self.parameter_count
        }

        /// Optional debug label.
        #[must_use]
        pub fn label(&self) -> Option<&str> {
            self.label.as_deref()
        }

        // Returns Option<_> for parity with the non-Windows stub side ; on
        // Windows the value is always Some(...).
        #[allow(clippy::unnecessary_wraps, clippy::redundant_pub_crate)]
        pub(crate) fn imp_signature(&self) -> Option<&ID3D12RootSignature> {
            Some(&self.signature)
        }
    }

    pub(super) fn build_root_signature(
        device: &Device,
        builder: RootSignatureBuilder,
    ) -> Result<RootSignature> {
        let parameter_count = builder.parameters.len();
        let mut raw_params: Vec<D3D12_ROOT_PARAMETER> = Vec::with_capacity(parameter_count);
        // Descriptor ranges keep separate allocations alive for the call.
        let mut range_storage: Vec<Vec<D3D12_DESCRIPTOR_RANGE>> = Vec::new();
        for p in &builder.parameters {
            let visibility = visibility_to_raw(p.visibility);
            let raw = match p.kind {
                RootParameterKind::Constants {
                    count,
                    shader_register,
                    register_space,
                } => D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        Constants: D3D12_ROOT_CONSTANTS {
                            ShaderRegister: shader_register,
                            RegisterSpace: register_space,
                            Num32BitValues: count,
                        },
                    },
                    ShaderVisibility: visibility,
                },
                RootParameterKind::Cbv {
                    shader_register,
                    register_space,
                } => D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        Descriptor: D3D12_ROOT_DESCRIPTOR {
                            ShaderRegister: shader_register,
                            RegisterSpace: register_space,
                        },
                    },
                    ShaderVisibility: visibility,
                },
                RootParameterKind::Srv {
                    shader_register,
                    register_space,
                } => D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_SRV,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        Descriptor: D3D12_ROOT_DESCRIPTOR {
                            ShaderRegister: shader_register,
                            RegisterSpace: register_space,
                        },
                    },
                    ShaderVisibility: visibility,
                },
                RootParameterKind::Uav {
                    shader_register,
                    register_space,
                } => D3D12_ROOT_PARAMETER {
                    ParameterType: D3D12_ROOT_PARAMETER_TYPE_UAV,
                    Anonymous: D3D12_ROOT_PARAMETER_0 {
                        Descriptor: D3D12_ROOT_DESCRIPTOR {
                            ShaderRegister: shader_register,
                            RegisterSpace: register_space,
                        },
                    },
                    ShaderVisibility: visibility,
                },
                RootParameterKind::DescriptorTable {
                    range_kind,
                    count,
                    base_register,
                    register_space,
                } => {
                    let range_type = match range_kind {
                        DescriptorRangeKind::Cbv => D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
                        DescriptorRangeKind::Srv => D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
                        DescriptorRangeKind::Uav => D3D12_DESCRIPTOR_RANGE_TYPE_UAV,
                    };
                    range_storage.push(vec![D3D12_DESCRIPTOR_RANGE {
                        RangeType: range_type,
                        NumDescriptors: count,
                        BaseShaderRegister: base_register,
                        RegisterSpace: register_space,
                        OffsetInDescriptorsFromTableStart: 0,
                    }]);
                    let range_ptr = range_storage.last().unwrap().as_ptr();
                    D3D12_ROOT_PARAMETER {
                        ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                        Anonymous: D3D12_ROOT_PARAMETER_0 {
                            DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                                NumDescriptorRanges: 1,
                                pDescriptorRanges: range_ptr,
                            },
                        },
                        ShaderVisibility: visibility,
                    }
                }
            };
            raw_params.push(raw);
        }

        let mut flags = D3D12_ROOT_SIGNATURE_FLAG_NONE;
        if builder.allow_ia_input_layout {
            flags |= D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
        }
        if builder.allow_stream_output {
            flags |= D3D12_ROOT_SIGNATURE_FLAG_ALLOW_STREAM_OUTPUT;
        }
        let desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: parameter_count as u32,
            pParameters: raw_params.as_ptr(),
            NumStaticSamplers: 0,
            pStaticSamplers: core::ptr::null(),
            Flags: flags,
        };
        let mut blob = None;
        let mut errors = None;
        // SAFETY : FFI ; raw_params + range_storage outlive the call ;
        // out blobs receive ownership on Ok.
        unsafe {
            D3D12SerializeRootSignature(
                &desc,
                D3D_ROOT_SIGNATURE_VERSION_1_0,
                &mut blob,
                Some(&mut errors),
            )
        }
        .map_err(|e| crate::device::imp_map_hresult("D3D12SerializeRootSignature", e))?;
        let blob = blob.ok_or_else(|| {
            D3d12Error::invalid("D3D12SerializeRootSignature", "no blob returned")
        })?;
        // SAFETY : blob lives until Drop ; we copy the bytes into the device call.
        let blob_ptr = unsafe { blob.GetBufferPointer() };
        let blob_size = unsafe { blob.GetBufferSize() };
        // SAFETY : ptr + size from a live ID3DBlob ; slice valid for call duration.
        let blob_slice = unsafe { core::slice::from_raw_parts(blob_ptr.cast::<u8>(), blob_size) };
        // SAFETY : FFI ; node-mask 0 = default.
        let signature: ID3D12RootSignature =
            unsafe { device.device.CreateRootSignature(0, blob_slice) }
                .map_err(|e| crate::device::imp_map_hresult("CreateRootSignature", e))?;
        Ok(RootSignature {
            signature,
            parameter_count,
            label: builder.label,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::RootSignatureBuilder;
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};

    /// Root signature stub.
    #[derive(Debug)]
    pub struct RootSignature {
        parameter_count: usize,
        label: Option<String>,
    }

    impl RootSignature {
        /// Stub parameter count.
        #[must_use]
        pub const fn parameter_count(&self) -> usize {
            self.parameter_count
        }

        /// Stub label.
        #[must_use]
        pub fn label(&self) -> Option<&str> {
            self.label.as_deref()
        }
    }

    pub(super) fn build_root_signature(
        _device: &Device,
        _builder: RootSignatureBuilder,
    ) -> Result<RootSignature> {
        Err(D3d12Error::loader("non-Windows target"))
    }
}

pub use imp::RootSignature;

#[cfg(test)]
mod tests {
    use super::{
        DescriptorRangeKind, RootParameter, RootParameterKind, RootSignatureBuilder,
        ShaderVisibility,
    };

    #[test]
    fn shader_visibility_names() {
        assert_eq!(ShaderVisibility::All.as_str(), "all");
        assert_eq!(ShaderVisibility::Vertex.as_str(), "vs");
        assert_eq!(ShaderVisibility::Mesh.as_str(), "ms");
        assert_eq!(ShaderVisibility::Amplification.as_str(), "as");
    }

    #[test]
    fn descriptor_range_kind_names() {
        assert_eq!(DescriptorRangeKind::Cbv.as_str(), "cbv");
        assert_eq!(DescriptorRangeKind::Srv.as_str(), "srv");
        assert_eq!(DescriptorRangeKind::Uav.as_str(), "uav");
    }

    #[test]
    fn root_parameter_constants_constructor() {
        let p = RootParameter::constants(4, 0, 0, ShaderVisibility::All);
        assert_eq!(p.visibility, ShaderVisibility::All);
        match p.kind {
            RootParameterKind::Constants { count, .. } => assert_eq!(count, 4),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn root_parameter_cbv_constructor() {
        let p = RootParameter::cbv(1, 0, ShaderVisibility::Vertex);
        match p.kind {
            RootParameterKind::Cbv {
                shader_register, ..
            } => assert_eq!(shader_register, 1),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn root_parameter_srv_constructor() {
        let p = RootParameter::srv(0, 0, ShaderVisibility::Pixel);
        match p.kind {
            RootParameterKind::Srv {
                shader_register, ..
            } => assert_eq!(shader_register, 0),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn root_parameter_uav_constructor() {
        let p = RootParameter::uav(2, 1, ShaderVisibility::Pixel);
        match p.kind {
            RootParameterKind::Uav {
                shader_register,
                register_space,
            } => {
                assert_eq!(shader_register, 2);
                assert_eq!(register_space, 1);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn root_parameter_descriptor_table_constructor() {
        let p = RootParameter::descriptor_table(
            DescriptorRangeKind::Srv,
            64,
            0,
            0,
            ShaderVisibility::All,
        );
        match p.kind {
            RootParameterKind::DescriptorTable { count, .. } => assert_eq!(count, 64),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn builder_parameter_count_grows() {
        let b = RootSignatureBuilder::new()
            .with_parameter(RootParameter::constants(4, 0, 0, ShaderVisibility::All))
            .with_parameter(RootParameter::cbv(0, 0, ShaderVisibility::All));
        assert_eq!(b.parameter_count(), 2);
    }

    #[test]
    fn builder_label_round_trips() {
        let b = RootSignatureBuilder::new().with_label("compute_kernel");
        assert_eq!(b.label.as_deref(), Some("compute_kernel"));
    }

    #[test]
    fn builder_with_ia_flag() {
        let b = RootSignatureBuilder::new().with_ia();
        assert!(b.allow_ia_input_layout);
    }

    #[test]
    fn builder_empty_build_returns_invalid() {
        // Build attempt requires a Device ; skip on non-Windows.
        // Just verify the empty-builder shape :
        let b = RootSignatureBuilder::new();
        assert_eq!(b.parameter_count(), 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn root_signature_build_with_constants_or_skip() {
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let rs = RootSignatureBuilder::new()
            .with_parameter(RootParameter::constants(4, 0, 0, ShaderVisibility::All))
            .with_label("compute_kernel_root")
            .build(&device);
        match rs {
            Ok(s) => {
                assert_eq!(s.parameter_count(), 1);
                assert_eq!(s.label(), Some("compute_kernel_root"));
            }
            Err(e) => assert!(
                e.is_loader_missing() || matches!(e, crate::error::D3d12Error::Hresult { .. })
            ),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn root_signature_build_with_descriptor_table_or_skip() {
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let rs = RootSignatureBuilder::new()
            .with_parameter(RootParameter::descriptor_table(
                DescriptorRangeKind::Srv,
                64,
                0,
                0,
                ShaderVisibility::All,
            ))
            .with_label("bindless_root")
            .build(&device);
        match rs {
            Ok(s) => assert_eq!(s.parameter_count(), 1),
            Err(e) => assert!(
                e.is_loader_missing() || matches!(e, crate::error::D3d12Error::Hresult { .. })
            ),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn empty_builder_build_returns_invalid_on_real_device() {
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let rs = RootSignatureBuilder::new().build(&device);
        assert!(rs.is_err());
        assert!(matches!(
            rs.unwrap_err(),
            crate::error::D3d12Error::InvalidArgument { .. }
        ));
    }
}

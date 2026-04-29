//! D3D12 pipeline state object (PSO).
//!
//! § DESIGN
//!   At S6-E2 we have NO real CSSLv3 DXIL emitter (D2 is deferred via dxc
//!   subprocess per the dispatch plan). The PSO surface accepts an arbitrary
//!   DXIL byte blob from the caller.
//!
//!   - `ComputePsoDesc` — root-signature + compute-shader DXIL bytes.
//!   - `GraphicsPsoDesc` — root-signature + VS + PS DXIL bytes (minimal ; full
//!     IA + RTV + DSV + sample-state defaults to a sensible fallback).
//!   - `PipelineState` — wraps `ID3D12PipelineState`.
//!
//! § STAGE-0 SCOPE
//!   The graphics path here is intentionally narrow : single render target,
//!   no depth, R8G8B8A8_UNORM, default rasterizer + blend + sampler. Full
//!   PSO descriptors land alongside the killer-app demo in a later slice.

use crate::root_signature::RootSignature;
// (Device + error types re-imported inside cfg-gated `imp` modules)

/// Compute pipeline state description.
#[derive(Debug, Clone)]
pub struct ComputePsoDesc<'a> {
    /// Root signature.
    pub root_signature: &'a RootSignature,
    /// DXIL bytecode for the compute shader.
    pub compute_shader_dxil: Vec<u8>,
    /// Optional debug label.
    pub label: Option<String>,
}

/// Graphics pipeline state description (minimal).
#[derive(Debug, Clone)]
pub struct GraphicsPsoDesc<'a> {
    /// Root signature.
    pub root_signature: &'a RootSignature,
    /// DXIL bytecode for the vertex shader.
    pub vertex_shader_dxil: Vec<u8>,
    /// DXIL bytecode for the pixel shader.
    pub pixel_shader_dxil: Vec<u8>,
    /// Optional debug label.
    pub label: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::{ComputePsoDesc, GraphicsPsoDesc};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};
    use windows::Win32::Graphics::Direct3D12::{
        ID3D12PipelineState, D3D12_BLEND_DESC, D3D12_BLEND_ONE, D3D12_BLEND_OP_ADD,
        D3D12_BLEND_ZERO, D3D12_COLOR_WRITE_ENABLE_ALL, D3D12_COMPUTE_PIPELINE_STATE_DESC,
        D3D12_CULL_MODE_BACK, D3D12_DEPTH_STENCIL_DESC, D3D12_DEPTH_WRITE_MASK_ZERO,
        D3D12_FILL_MODE_SOLID, D3D12_GRAPHICS_PIPELINE_STATE_DESC, D3D12_INPUT_LAYOUT_DESC,
        D3D12_LOGIC_OP_NOOP, D3D12_PIPELINE_STATE_FLAG_NONE,
        D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE, D3D12_RASTERIZER_DESC,
        D3D12_RENDER_TARGET_BLEND_DESC, D3D12_SHADER_BYTECODE, D3D12_STREAM_OUTPUT_DESC,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};

    /// Wrapper for `ID3D12PipelineState`.
    pub struct PipelineState {
        pub(crate) pso: ID3D12PipelineState,
        pub(crate) is_compute: bool,
        pub(crate) label: Option<String>,
    }

    impl core::fmt::Debug for PipelineState {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("PipelineState")
                .field("is_compute", &self.is_compute)
                .field("label", &self.label)
                .finish_non_exhaustive()
        }
    }

    impl PipelineState {
        /// Create a compute PSO.
        pub fn new_compute(device: &Device, desc: ComputePsoDesc<'_>) -> Result<Self> {
            if desc.compute_shader_dxil.is_empty() {
                return Err(D3d12Error::invalid(
                    "PipelineState::new_compute",
                    "empty compute DXIL",
                ));
            }
            let raw_rs = desc
                .root_signature
                .imp_signature()
                .ok_or_else(|| D3d12Error::invalid("PipelineState::new_compute", "rs unwired"))?;
            let mut raw_desc = D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: core::mem::ManuallyDrop::new(Some(raw_rs.clone())),
                CS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: desc.compute_shader_dxil.as_ptr().cast(),
                    BytecodeLength: desc.compute_shader_dxil.len(),
                },
                NodeMask: 0,
                CachedPSO: windows::Win32::Graphics::Direct3D12::D3D12_CACHED_PIPELINE_STATE {
                    pCachedBlob: core::ptr::null(),
                    CachedBlobSizeInBytes: 0,
                },
                Flags: D3D12_PIPELINE_STATE_FLAG_NONE,
            };
            // SAFETY : FFI ; desc fields owned in scope ; root_sig + dxil
            // bytes outlive the call. The ManuallyDrop wrapper holds a
            // bumped refcount on the rs ; we drop it explicitly post-call
            // to release the ref without double-drop.
            let pso_result: windows::core::Result<ID3D12PipelineState> =
                unsafe { device.device.CreateComputePipelineState(&raw_desc) };
            // SAFETY : we constructed the ManuallyDrop above ; take it now
            // so the cloned rs ref count is released.
            unsafe { core::mem::ManuallyDrop::drop(&mut raw_desc.pRootSignature) };
            let pso: ID3D12PipelineState = pso_result
                .map_err(|e| crate::device::imp_map_hresult("CreateComputePipelineState", e))?;
            Ok(Self {
                pso,
                is_compute: true,
                label: desc.label,
            })
        }

        /// Create a graphics PSO with the minimal default state (1 RTV
        /// R8G8B8A8_UNORM, no depth, triangle, default raster + blend).
        pub fn new_graphics(device: &Device, desc: GraphicsPsoDesc<'_>) -> Result<Self> {
            if desc.vertex_shader_dxil.is_empty() {
                return Err(D3d12Error::invalid(
                    "PipelineState::new_graphics",
                    "empty VS DXIL",
                ));
            }
            if desc.pixel_shader_dxil.is_empty() {
                return Err(D3d12Error::invalid(
                    "PipelineState::new_graphics",
                    "empty PS DXIL",
                ));
            }
            let raw_rs = desc
                .root_signature
                .imp_signature()
                .ok_or_else(|| D3d12Error::invalid("PipelineState::new_graphics", "rs unwired"))?;
            let mut rt_blend = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];
            rt_blend[0] = D3D12_RENDER_TARGET_BLEND_DESC {
                BlendEnable: false.into(),
                LogicOpEnable: false.into(),
                SrcBlend: D3D12_BLEND_ONE,
                DestBlend: D3D12_BLEND_ZERO,
                BlendOp: D3D12_BLEND_OP_ADD,
                SrcBlendAlpha: D3D12_BLEND_ONE,
                DestBlendAlpha: D3D12_BLEND_ZERO,
                BlendOpAlpha: D3D12_BLEND_OP_ADD,
                LogicOp: D3D12_LOGIC_OP_NOOP,
                RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let blend_desc = D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: rt_blend,
            };
            let raster_desc = D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_BACK,
                FrontCounterClockwise: false.into(),
                DepthBias: 0,
                DepthBiasClamp: 0.0,
                SlopeScaledDepthBias: 0.0,
                DepthClipEnable: true.into(),
                MultisampleEnable: false.into(),
                AntialiasedLineEnable: false.into(),
                ForcedSampleCount: 0,
                ConservativeRaster:
                    windows::Win32::Graphics::Direct3D12::D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
            };
            let mut rtv_formats = [windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT(0); 8];
            rtv_formats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
            let mut raw_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: core::mem::ManuallyDrop::new(Some(raw_rs.clone())),
                VS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: desc.vertex_shader_dxil.as_ptr().cast(),
                    BytecodeLength: desc.vertex_shader_dxil.len(),
                },
                PS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: desc.pixel_shader_dxil.as_ptr().cast(),
                    BytecodeLength: desc.pixel_shader_dxil.len(),
                },
                DS: D3D12_SHADER_BYTECODE::default(),
                HS: D3D12_SHADER_BYTECODE::default(),
                GS: D3D12_SHADER_BYTECODE::default(),
                StreamOutput: D3D12_STREAM_OUTPUT_DESC::default(),
                BlendState: blend_desc,
                SampleMask: u32::MAX,
                RasterizerState: raster_desc,
                DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                    DepthEnable: false.into(),
                    DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ZERO,
                    ..Default::default()
                },
                InputLayout: D3D12_INPUT_LAYOUT_DESC::default(),
                IBStripCutValue:
                    windows::Win32::Graphics::Direct3D12::D3D12_INDEX_BUFFER_STRIP_CUT_VALUE_DISABLED,
                PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                NumRenderTargets: 1,
                RTVFormats: rtv_formats,
                DSVFormat: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                NodeMask: 0,
                CachedPSO: windows::Win32::Graphics::Direct3D12::D3D12_CACHED_PIPELINE_STATE {
                    pCachedBlob: core::ptr::null(),
                    CachedBlobSizeInBytes: 0,
                },
                Flags: D3D12_PIPELINE_STATE_FLAG_NONE,
            };
            // SAFETY : FFI ; all fields valid for call duration.
            // SAFETY : FFI ; raw_desc fields are valid for the call duration ;
            // the rs ref is held inside ManuallyDrop and released below.
            let pso_result: windows::core::Result<ID3D12PipelineState> =
                unsafe { device.device.CreateGraphicsPipelineState(&raw_desc) };
            // SAFETY : ManuallyDrop holds the cloned rs ref ; release it now.
            unsafe { core::mem::ManuallyDrop::drop(&mut raw_desc.pRootSignature) };
            let pso: ID3D12PipelineState = pso_result
                .map_err(|e| crate::device::imp_map_hresult("CreateGraphicsPipelineState", e))?;
            Ok(Self {
                pso,
                is_compute: false,
                label: desc.label,
            })
        }

        /// Is this a compute PSO?
        #[must_use]
        pub const fn is_compute(&self) -> bool {
            self.is_compute
        }

        /// Optional debug label.
        #[must_use]
        pub fn label(&self) -> Option<&str> {
            self.label.as_deref()
        }

        // Returns Option<_> for parity with the non-Windows stub side ; on
        // Windows the value is always Some(...).
        #[allow(clippy::unnecessary_wraps, clippy::redundant_pub_crate)]
        pub(crate) fn imp_pso(&self) -> Option<&ID3D12PipelineState> {
            Some(&self.pso)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{ComputePsoDesc, GraphicsPsoDesc};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};

    /// Pipeline state stub.
    #[derive(Debug)]
    pub struct PipelineState {
        is_compute: bool,
        label: Option<String>,
    }

    impl PipelineState {
        /// Always returns `LoaderMissing`.
        pub fn new_compute(_device: &Device, _desc: ComputePsoDesc<'_>) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn new_graphics(_device: &Device, _desc: GraphicsPsoDesc<'_>) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub.
        #[must_use]
        pub const fn is_compute(&self) -> bool {
            self.is_compute
        }

        /// Stub.
        #[must_use]
        pub fn label(&self) -> Option<&str> {
            self.label.as_deref()
        }
    }
}

pub use imp::PipelineState;

#[cfg(test)]
mod tests {
    use super::{ComputePsoDesc, GraphicsPsoDesc};
    use crate::root_signature::RootSignatureBuilder;

    #[test]
    fn compute_pso_desc_can_be_constructed() {
        let dummy_rs_builder = RootSignatureBuilder::new();
        // No actual signature on non-Windows ; just check that the desc shape holds.
        let _ = (
            dummy_rs_builder.parameter_count(),
            ComputePsoDesc::<'static> {
                root_signature: unsafe {
                    // SAFETY : we never call .build / .new_compute on this fixture ;
                    // the address is never dereferenced. This is purely a shape test.
                    &*core::ptr::NonNull::<crate::root_signature::RootSignature>::dangling()
                        .as_ptr()
                },
                compute_shader_dxil: vec![0u8; 256],
                label: Some("test_kernel".into()),
            }
            .compute_shader_dxil
            .len(),
        );
    }

    #[test]
    fn graphics_pso_desc_holds_two_blobs() {
        let _ = GraphicsPsoDesc::<'static> {
            root_signature: unsafe {
                // SAFETY : address never dereferenced ; shape-only test.
                &*core::ptr::NonNull::<crate::root_signature::RootSignature>::dangling().as_ptr()
            },
            vertex_shader_dxil: vec![0u8; 64],
            pixel_shader_dxil: vec![0u8; 64],
            label: None,
        };
    }
}

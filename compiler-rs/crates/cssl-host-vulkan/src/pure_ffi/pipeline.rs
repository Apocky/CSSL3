//! § pure_ffi::pipeline — `VkPipeline` compile from SPIR-V.
//!
//! § ROLE
//!   From-scratch FFI declarations for the pipeline-layer Vulkan
//!   surface : `vkCreateShaderModule` + `vkDestroyShaderModule` +
//!   `vkCreatePipelineLayout` + `vkCreateComputePipelines` +
//!   `vkDestroyPipelineLayout` + `vkDestroyPipeline`.
//!
//! § SCOPE
//!   Stage A focuses on the COMPUTE pipeline path (the surface
//!   `cssl-cgen-gpu-spirv` emits) ; graphics-pipeline structs land in
//!   a follow-up slice.

#![allow(unsafe_code)]

use core::ffi::c_char;

use super::{
    PVkAllocationCallbacks, VkDevice, VkPipeline, VkPipelineCache, VkPipelineLayout, VkResult,
    VkShaderModule, VkStructureType, VulkanLoader, VK_NULL_HANDLE_NDISP,
};

// ───────────────────────────────────────────────────────────────────
// § Pipeline-layer enums.
// ───────────────────────────────────────────────────────────────────

/// `VkShaderStageFlagBits` — selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkShaderStageFlag {
    /// `VK_SHADER_STAGE_VERTEX_BIT`.
    Vertex = 0x0000_0001,
    /// `VK_SHADER_STAGE_TESSELLATION_CONTROL_BIT`.
    TessellationControl = 0x0000_0002,
    /// `VK_SHADER_STAGE_TESSELLATION_EVALUATION_BIT`.
    TessellationEvaluation = 0x0000_0004,
    /// `VK_SHADER_STAGE_GEOMETRY_BIT`.
    Geometry = 0x0000_0008,
    /// `VK_SHADER_STAGE_FRAGMENT_BIT`.
    Fragment = 0x0000_0010,
    /// `VK_SHADER_STAGE_COMPUTE_BIT`.
    Compute = 0x0000_0020,
    /// `VK_SHADER_STAGE_ALL_GRAPHICS`.
    AllGraphics = 0x0000_001F,
}

/// SPIR-V magic-number (`VK_SPIRV_HEADER_MAGIC`).
pub const SPIRV_MAGIC: u32 = 0x0723_0203;

// ───────────────────────────────────────────────────────────────────
// § Pipeline-layer structures.
// ───────────────────────────────────────────────────────────────────

/// `VkShaderModuleCreateInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkShaderModuleCreateInfo {
    /// Must be [`VkStructureType::ShaderModuleCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Reserved (must be 0).
    pub flags: u32,
    /// Size of the SPIR-V code in bytes (must be a multiple of 4).
    pub code_size: usize,
    /// Pointer to the SPIR-V code (u32-aligned).
    pub p_code: *const u32,
}

impl Default for VkShaderModuleCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::ShaderModuleCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            code_size: 0,
            p_code: core::ptr::null(),
        }
    }
}

/// `VkPipelineLayoutCreateInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkPipelineLayoutCreateInfo {
    /// Must be [`VkStructureType::PipelineLayoutCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Reserved (must be 0).
    pub flags: u32,
    /// Number of descriptor-set-layout entries.
    pub set_layout_count: u32,
    /// Pointer to descriptor-set-layout array.
    pub p_set_layouts: *const u64,
    /// Number of push-constant-range entries.
    pub push_constant_range_count: u32,
    /// Pointer to push-constant-range array.
    pub p_push_constant_ranges: *const core::ffi::c_void,
}

impl Default for VkPipelineLayoutCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::PipelineLayoutCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            set_layout_count: 0,
            p_set_layouts: core::ptr::null(),
            push_constant_range_count: 0,
            p_push_constant_ranges: core::ptr::null(),
        }
    }
}

/// `VkPipelineShaderStageCreateInfo` — entry for shader-stage in compute / graphics pipeline.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkPipelineShaderStageCreateInfo {
    /// Must be [`VkStructureType::PipelineShaderStageCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Reserved (must be 0).
    pub flags: u32,
    /// Bitmask of [`VkShaderStageFlag`] (single-bit for compute).
    pub stage: u32,
    /// Shader-module handle.
    pub module: VkShaderModule,
    /// NUL-terminated entry-point name (e.g. `"main"`).
    pub p_name: *const c_char,
    /// Pointer to specialization-info (or null).
    pub p_specialization_info: *const core::ffi::c_void,
}

impl Default for VkPipelineShaderStageCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::PipelineShaderStageCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            stage: VkShaderStageFlag::Compute as u32,
            module: VK_NULL_HANDLE_NDISP,
            p_name: core::ptr::null(),
            p_specialization_info: core::ptr::null(),
        }
    }
}

/// `VkComputePipelineCreateInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkComputePipelineCreateInfo {
    /// Must be [`VkStructureType::ComputePipelineCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Bitmask of `VkPipelineCreateFlagBits` (rarely used in stage-0).
    pub flags: u32,
    /// Compute shader stage.
    pub stage: VkPipelineShaderStageCreateInfo,
    /// Pipeline-layout handle.
    pub layout: VkPipelineLayout,
    /// Base pipeline handle (for derivatives ; null in stage-0).
    pub base_pipeline_handle: VkPipeline,
    /// Base pipeline index (-1 if no base).
    pub base_pipeline_index: i32,
}

impl Default for VkComputePipelineCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::ComputePipelineCreateInfo,
            p_next: core::ptr::null(),
            flags: 0,
            stage: VkPipelineShaderStageCreateInfo::default(),
            layout: VK_NULL_HANDLE_NDISP,
            base_pipeline_handle: VK_NULL_HANDLE_NDISP,
            base_pipeline_index: -1,
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// § C signature declarations.
// ───────────────────────────────────────────────────────────────────

/// `vkCreateShaderModule` C signature.
pub type PfnVkCreateShaderModule = unsafe extern "C" fn(
    device: VkDevice,
    p_create_info: *const VkShaderModuleCreateInfo,
    p_allocator: PVkAllocationCallbacks,
    p_shader_module: *mut VkShaderModule,
) -> i32;

/// `vkDestroyShaderModule` C signature.
pub type PfnVkDestroyShaderModule = unsafe extern "C" fn(
    device: VkDevice,
    shader_module: VkShaderModule,
    p_allocator: PVkAllocationCallbacks,
);

/// `vkCreatePipelineLayout` C signature.
pub type PfnVkCreatePipelineLayout = unsafe extern "C" fn(
    device: VkDevice,
    p_create_info: *const VkPipelineLayoutCreateInfo,
    p_allocator: PVkAllocationCallbacks,
    p_pipeline_layout: *mut VkPipelineLayout,
) -> i32;

/// `vkCreateComputePipelines` C signature.
pub type PfnVkCreateComputePipelines = unsafe extern "C" fn(
    device: VkDevice,
    pipeline_cache: VkPipelineCache,
    create_info_count: u32,
    p_create_infos: *const VkComputePipelineCreateInfo,
    p_allocator: PVkAllocationCallbacks,
    p_pipelines: *mut VkPipeline,
) -> i32;

/// `vkDestroyPipeline` C signature.
pub type PfnVkDestroyPipeline = unsafe extern "C" fn(
    device: VkDevice,
    pipeline: VkPipeline,
    p_allocator: PVkAllocationCallbacks,
);

/// `vkDestroyPipelineLayout` C signature.
pub type PfnVkDestroyPipelineLayout = unsafe extern "C" fn(
    device: VkDevice,
    pipeline_layout: VkPipelineLayout,
    p_allocator: PVkAllocationCallbacks,
);

// ───────────────────────────────────────────────────────────────────
// § Rust-side wrapper.
// ───────────────────────────────────────────────────────────────────

/// Errors surfaced by [`ComputePipelineCompile::compile_with_loader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineCompileError {
    /// Loader returned NULL for a pipeline entry-point.
    LoaderMissingSymbol(String),
    /// Stage A : real loaders not yet wired.
    StubLoaderUnsupported,
    /// SPIR-V validation rejected the blob.
    SpirvValidation(SpirvValidationError),
    /// Driver returned a non-success VkResult.
    Vk(VkResult),
}

/// SPIR-V structural validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpirvValidationError {
    /// Blob length is not a multiple of 4 bytes (SPIR-V is 32-bit words).
    BlobNotWordAligned(usize),
    /// First word is not the SPIR-V magic number.
    BadMagic(u32),
    /// Blob is shorter than the minimum 5-word header.
    HeaderTooShort(usize),
}

/// Validate a SPIR-V blob's structural header without dispatching the
/// driver. This is the front-line check before [`ComputePipelineCompile::compile_with_loader`]
/// hands the blob to `vkCreateShaderModule`.
///
/// # Errors
/// See [`SpirvValidationError`].
pub fn validate_spirv_header(blob: &[u8]) -> Result<(), SpirvValidationError> {
    if blob.len() % 4 != 0 {
        return Err(SpirvValidationError::BlobNotWordAligned(blob.len()));
    }
    if blob.len() < 5 * 4 {
        return Err(SpirvValidationError::HeaderTooShort(blob.len()));
    }
    let magic = u32::from_ne_bytes([blob[0], blob[1], blob[2], blob[3]]);
    // SPIR-V magic is endian-detectable : if the byte-order matches our
    // host the first word equals SPIRV_MAGIC ; if it's byte-swapped, the
    // word is the byte-reverse. Both are valid blobs.
    let swapped = u32::from_be_bytes([blob[0], blob[1], blob[2], blob[3]]);
    let swapped_match = swapped == SPIRV_MAGIC;
    if magic != SPIRV_MAGIC && !swapped_match {
        return Err(SpirvValidationError::BadMagic(magic));
    }
    Ok(())
}

/// Compute-pipeline compile request.
#[derive(Debug, Clone)]
pub struct ComputePipelineCompile {
    /// SPIR-V code to feed `vkCreateShaderModule`.
    pub spirv: Vec<u8>,
    /// Entry-point name (defaults to `"main"`).
    pub entry_point: String,
    /// Pipeline-layout to bind (must outlive the pipeline).
    pub layout: VkPipelineLayout,
}

impl ComputePipelineCompile {
    /// New compile request.
    #[must_use]
    pub fn new(spirv: Vec<u8>, layout: VkPipelineLayout) -> Self {
        Self {
            spirv,
            entry_point: "main".to_string(),
            layout,
        }
    }

    /// Override the entry-point name.
    #[must_use]
    pub fn with_entry_point(mut self, name: impl Into<String>) -> Self {
        self.entry_point = name.into();
        self
    }

    /// Validate the SPIR-V blob and resolve the pipeline-create symbol
    /// chain via the supplied loader.
    ///
    /// # Errors
    /// See [`PipelineCompileError`].
    pub fn compile_with_loader<L: VulkanLoader>(
        &self,
        loader: &L,
    ) -> Result<VkPipeline, PipelineCompileError> {
        validate_spirv_header(&self.spirv).map_err(PipelineCompileError::SpirvValidation)?;

        // The compile-flow resolves three symbols : the shader-module,
        // pipeline-layout, and compute-pipeline create-fns. Stage A
        // surfaces the resolution-shape so unit-tests can verify the
        // call layering without invoking real FFI.
        let needed = [
            "vkCreateShaderModule",
            "vkCreatePipelineLayout",
            "vkCreateComputePipelines",
        ];
        for sym in needed {
            match loader.resolve(core::ptr::null_mut(), sym) {
                None => return Err(PipelineCompileError::LoaderMissingSymbol(sym.to_string())),
                Some(_addr) if !loader.is_real() => {
                    // Mock loader records but we still bail at end of loop ;
                    // continue so we record every symbol the call-flow needs.
                }
                Some(_addr) => {}
            }
        }
        if !loader.is_real() {
            return Err(PipelineCompileError::StubLoaderUnsupported);
        }
        Ok(VK_NULL_HANDLE_NDISP)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_spirv_header, ComputePipelineCompile, PipelineCompileError, SpirvValidationError,
        VkComputePipelineCreateInfo, VkShaderStageFlag, SPIRV_MAGIC,
    };
    use crate::pure_ffi::{MockLoader, StubLoader, VK_NULL_HANDLE_NDISP};

    fn fake_spirv_blob() -> Vec<u8> {
        // 5-word SPIR-V header : magic | version | generator | bound | reserved.
        let mut v = Vec::with_capacity(20);
        v.extend_from_slice(&SPIRV_MAGIC.to_ne_bytes());
        v.extend_from_slice(&0x0001_0000_u32.to_ne_bytes()); // SPIR-V 1.0.
        v.extend_from_slice(&0u32.to_ne_bytes()); // generator.
        v.extend_from_slice(&1u32.to_ne_bytes()); // bound.
        v.extend_from_slice(&0u32.to_ne_bytes()); // reserved.
        v
    }

    #[test]
    fn validate_spirv_header_accepts_minimal_header() {
        let v = fake_spirv_blob();
        assert!(validate_spirv_header(&v).is_ok());
    }

    #[test]
    fn validate_spirv_header_rejects_bad_alignment() {
        let v = vec![0x03, 0x02, 0x23, 0x07, 0x42]; // 5 bytes
        let r = validate_spirv_header(&v);
        assert!(matches!(r, Err(SpirvValidationError::BlobNotWordAligned(5))));
    }

    #[test]
    fn validate_spirv_header_rejects_too_short() {
        let v = vec![0x03, 0x02, 0x23, 0x07]; // 4 bytes ; word-aligned but no header.
        let r = validate_spirv_header(&v);
        assert!(matches!(r, Err(SpirvValidationError::HeaderTooShort(4))));
    }

    #[test]
    fn validate_spirv_header_rejects_bad_magic() {
        let mut v = vec![0u8; 20];
        v[0..4].copy_from_slice(&0xDEAD_BEEF_u32.to_ne_bytes());
        let r = validate_spirv_header(&v);
        assert!(matches!(r, Err(SpirvValidationError::BadMagic(_))));
    }

    #[test]
    fn compile_with_stub_loader_errors_with_missing_symbol() {
        let l = StubLoader;
        let c = ComputePipelineCompile::new(fake_spirv_blob(), VK_NULL_HANDLE_NDISP);
        let r = c.compile_with_loader(&l);
        // First missing symbol = vkCreateShaderModule (symbol-chain order).
        assert!(matches!(r, Err(PipelineCompileError::LoaderMissingSymbol(ref n)) if n == "vkCreateShaderModule"));
    }

    #[test]
    fn compile_with_mock_loader_records_three_symbols() {
        let l = MockLoader::new();
        let c = ComputePipelineCompile::new(fake_spirv_blob(), VK_NULL_HANDLE_NDISP);
        let r = c.compile_with_loader(&l);
        assert!(matches!(r, Err(PipelineCompileError::StubLoaderUnsupported)));
        assert_eq!(l.resolve_count(), 3);
        let names = l.resolved_names();
        assert_eq!(names[0], "vkCreateShaderModule");
        assert_eq!(names[1], "vkCreatePipelineLayout");
        assert_eq!(names[2], "vkCreateComputePipelines");
    }

    #[test]
    fn compile_rejects_bad_spirv_before_loader() {
        let l = MockLoader::new();
        let bad = vec![0u8; 20]; // word-aligned, ≥20 bytes, but magic = 0.
        let c = ComputePipelineCompile::new(bad, VK_NULL_HANDLE_NDISP);
        let r = c.compile_with_loader(&l);
        assert!(matches!(r, Err(PipelineCompileError::SpirvValidation(_))));
        // Loader was NOT invoked because validation failed first.
        assert_eq!(l.resolve_count(), 0);
    }

    #[test]
    fn compute_pipeline_create_info_default_uses_compute_stage() {
        let info = VkComputePipelineCreateInfo::default();
        assert_eq!(info.stage.stage, VkShaderStageFlag::Compute as u32);
        assert_eq!(info.base_pipeline_index, -1);
    }
}

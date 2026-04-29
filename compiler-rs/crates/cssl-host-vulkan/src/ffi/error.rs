//! § ffi/error : unified ash-backed-FFI error type (T11-D65, S6-E1).
//!
//! § ROLE
//!   Single error enum every public ffi-fn returns. Carries Vulkan
//!   `vk::Result` codes verbatim where applicable + crate-specific
//!   variants for higher-level failure modes (loader missing / no
//!   suitable physical device / SPIR-V too short / etc.).
//!
//! § GATE-SKIP PATTERN
//!   `LoaderError::Loading(_)` is the canonical "loader missing" signal
//!   integration tests check before asserting hard-failures. Mirrors the
//!   `BinaryMissing` pattern from `csslc::linker` (T11-D55).
//!
//! § PRIME-DIRECTIVE
//!   No driver-source error string is hidden, escalated, or silently
//!   swallowed — every `ash` error carries through with its raw `vk`
//!   code so the ring's auditability stays intact.

use ash::vk;
use thiserror::Error;

/// Top-level ffi-fn error. Every public fn under `ffi::*` returns
/// `Result<T, AshError>` so callers can match without juggling multiple
/// error types.
#[derive(Debug, Error)]
pub enum AshError {
    /// Vulkan loader was unreachable.
    #[error("Vulkan loader missing : {0}")]
    Loader(#[from] LoaderError),

    /// `vkCreateInstance` failed.
    #[error("vkCreateInstance failed : {0}")]
    InstanceCreate(VkResultDisplay),

    /// `vkEnumeratePhysicalDevices` failed.
    #[error("vkEnumeratePhysicalDevices failed : {0}")]
    EnumeratePhysical(VkResultDisplay),

    /// No physical device satisfied the predicate.
    #[error("no suitable physical device : {0}")]
    NoSuitableDevice(String),

    /// `vkCreateDevice` failed.
    #[error("vkCreateDevice failed : {0}")]
    DeviceCreate(VkResultDisplay),

    /// Logical-device queue-family lookup failed.
    #[error("queue family `{family}` unavailable on device `{device}`")]
    QueueFamilyMissing {
        /// "graphics" / "compute" / "transfer" etc.
        family: String,
        /// Diagnostic name for the device.
        device: String,
    },

    /// `vkCreateBuffer` failed.
    #[error("vkCreateBuffer failed : {0}")]
    BufferCreate(VkResultDisplay),

    /// `vkAllocateMemory` failed.
    #[error("vkAllocateMemory failed : {0}")]
    MemoryAllocate(VkResultDisplay),

    /// Couldn't find a memory-type index satisfying the requested mask.
    #[error("no memory type satisfying type-bits {type_bits:#x} + flags {flags:?}")]
    NoMatchingMemoryType {
        /// `VkMemoryRequirements::memoryTypeBits`.
        type_bits: u32,
        /// `VkMemoryPropertyFlags` requested.
        flags: vk::MemoryPropertyFlags,
    },

    /// `vkBindBufferMemory` failed.
    #[error("vkBindBufferMemory failed : {0}")]
    BindBufferMemory(VkResultDisplay),

    /// SPIR-V blob is the wrong shape (must be a multiple of 4 bytes
    /// and start with the SPIR-V magic word).
    #[error(
        "SPIR-V blob malformed : len={len} bytes (must be multiple of 4) magic={magic:#x} \
         (expected {expected:#x})"
    )]
    SpirVMalformed {
        /// Byte length.
        len: usize,
        /// First u32 word.
        magic: u32,
        /// Expected magic.
        expected: u32,
    },

    /// `vkCreateShaderModule` failed.
    #[error("vkCreateShaderModule failed : {0}")]
    ShaderModuleCreate(VkResultDisplay),

    /// `vkCreatePipelineLayout` failed.
    #[error("vkCreatePipelineLayout failed : {0}")]
    PipelineLayoutCreate(VkResultDisplay),

    /// `vkCreateDescriptorSetLayout` failed.
    #[error("vkCreateDescriptorSetLayout failed : {0}")]
    DescriptorLayoutCreate(VkResultDisplay),

    /// `vkCreateComputePipelines` failed.
    #[error("vkCreateComputePipelines failed : {0}")]
    ComputePipelineCreate(VkResultDisplay),

    /// `vkCreateCommandPool` failed.
    #[error("vkCreateCommandPool failed : {0}")]
    CommandPoolCreate(VkResultDisplay),

    /// `vkAllocateCommandBuffers` failed.
    #[error("vkAllocateCommandBuffers failed : {0}")]
    CommandBufferAllocate(VkResultDisplay),

    /// `vkBeginCommandBuffer` failed.
    #[error("vkBeginCommandBuffer failed : {0}")]
    CommandBufferBegin(VkResultDisplay),

    /// `vkEndCommandBuffer` failed.
    #[error("vkEndCommandBuffer failed : {0}")]
    CommandBufferEnd(VkResultDisplay),

    /// `vkQueueSubmit` failed.
    #[error("vkQueueSubmit failed : {0}")]
    QueueSubmit(VkResultDisplay),

    /// `vkWaitForFences` failed.
    #[error("vkWaitForFences failed : {0}")]
    FenceWait(VkResultDisplay),

    /// `vkCreateFence` failed.
    #[error("vkCreateFence failed : {0}")]
    FenceCreate(VkResultDisplay),

    /// `vkResetFences` failed.
    #[error("vkResetFences failed : {0}")]
    FenceReset(VkResultDisplay),

    /// `vkMapMemory` failed.
    #[error("vkMapMemory failed : {0}")]
    MapMemory(VkResultDisplay),

    /// Generic driver error not specifically modelled above ; carries
    /// the raw `vk::Result` so the caller can match downstream.
    #[error("Vulkan driver error during `{stage}` : {result}")]
    Driver {
        /// Free-form diagnostic stage name.
        stage: String,
        /// Raw VK result code.
        result: VkResultDisplay,
    },
}

/// Loader-init failure. Distinct from [`AshError`] so tests gate-skip
/// on the loader-missing case without matching deep into AshError.
#[derive(Debug, Error)]
pub enum LoaderError {
    /// `Entry::load()` failed (vulkan-1.dll / libvulkan.so.1 absent).
    #[error("ash::Entry::load() failed : {detail}")]
    Loading {
        /// String form of `ash::LoadingError`.
        detail: String,
    },
}

/// Wrapper around `ash::vk::Result` so it implements `Display` /
/// `std::error::Error` cleanly via `thiserror`. The wrapper keeps the
/// raw integer + the canonical name (e.g., `"VK_ERROR_OUT_OF_HOST_MEMORY"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VkResultDisplay(pub vk::Result);

impl std::fmt::Display for VkResultDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // ash prints VkResult with the canonical name + raw int.
        write!(f, "{:?} ({})", self.0, self.0.as_raw())
    }
}

impl From<vk::Result> for VkResultDisplay {
    fn from(r: vk::Result) -> Self {
        Self(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_result_display_renders_canonical_name() {
        let v = VkResultDisplay::from(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
        let s = format!("{v}");
        assert!(s.contains("ERROR_OUT_OF_HOST_MEMORY"));
        assert!(s.contains("-1")); // raw int for OOM
    }

    #[test]
    fn loader_error_display_has_actionable_detail() {
        let e = LoaderError::Loading {
            detail: "vulkan-1.dll not found".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("Entry::load"));
        assert!(s.contains("vulkan-1.dll"));
    }

    #[test]
    fn ash_error_loader_round_trips_via_from() {
        let inner = LoaderError::Loading {
            detail: "stub".into(),
        };
        let wrapped: AshError = inner.into();
        let s = format!("{wrapped}");
        assert!(s.contains("loader missing"));
    }

    #[test]
    fn ash_error_no_suitable_device_carries_predicate() {
        let e = AshError::NoSuitableDevice("vendor=Intel device-id=0x56A0".into());
        let s = format!("{e}");
        assert!(s.contains("no suitable physical device"));
        assert!(s.contains("0x56A0"));
    }

    #[test]
    fn ash_error_queue_family_missing_lists_device_and_family() {
        let e = AshError::QueueFamilyMissing {
            family: "compute".into(),
            device: "Intel(R) Arc(TM) A770 Graphics".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("compute"));
        assert!(s.contains("Arc"));
    }

    #[test]
    fn ash_error_spirv_malformed_carries_diagnostic_fields() {
        let e = AshError::SpirVMalformed {
            len: 7,
            magic: 0xDEAD_BEEF,
            expected: 0x07230203,
        };
        let s = format!("{e}");
        assert!(s.contains("len=7"));
        assert!(s.contains("0xdeadbeef"));
        assert!(s.contains("0x7230203"));
    }
}

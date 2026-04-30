//! § pure_ffi::swapchain — `VkSwapchainKHR` acquire/present.
//!
//! § ROLE
//!   From-scratch FFI declarations for the swapchain-layer Vulkan
//!   surface : `vkCreateSwapchainKHR` + `vkDestroySwapchainKHR` +
//!   `vkGetSwapchainImagesKHR` + `vkAcquireNextImageKHR` +
//!   `vkQueuePresentKHR`.
//!
//! § SCOPE
//!   Every entry-point comes from `VK_KHR_swapchain` — the device must
//!   have requested that extension at `vkCreateDevice` time. cssl-rt
//!   `__cssl_gpu_swapchain_*` symbols delegate here.

#![allow(unsafe_code)]

use super::{
    PVkAllocationCallbacks, VkDevice, VkFence, VkImage, VkQueue, VkSemaphore, VkStructureType,
    VkSurfaceKHR, VkSwapchainKHR, VulkanLoader, VK_NULL_HANDLE_NDISP,
};

// ───────────────────────────────────────────────────────────────────
// § Swapchain-layer enums.
// ───────────────────────────────────────────────────────────────────

/// `VkFormat` — selected entries (full list is ~250 ; stage-0 surfaces
/// the swapchain-canonical set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkFormat {
    /// `VK_FORMAT_UNDEFINED` (sentinel for "use default").
    Undefined = 0,
    /// `VK_FORMAT_B8G8R8A8_UNORM` (most-common swapchain format on Windows/Linux).
    B8g8r8a8Unorm = 44,
    /// `VK_FORMAT_B8G8R8A8_SRGB`.
    B8g8r8a8Srgb = 50,
    /// `VK_FORMAT_R8G8B8A8_UNORM`.
    R8g8b8a8Unorm = 37,
    /// `VK_FORMAT_R8G8B8A8_SRGB`.
    R8g8b8a8Srgb = 43,
    /// `VK_FORMAT_R16G16B16A16_SFLOAT` (HDR).
    R16g16b16a16Sfloat = 97,
}

/// `VkColorSpaceKHR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkColorSpaceKHR {
    /// `VK_COLOR_SPACE_SRGB_NONLINEAR_KHR`.
    SrgbNonlinear = 0,
}

/// `VkPresentModeKHR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkPresentModeKHR {
    /// `VK_PRESENT_MODE_IMMEDIATE_KHR`.
    Immediate = 0,
    /// `VK_PRESENT_MODE_MAILBOX_KHR` (low-latency triple-buffer).
    Mailbox = 1,
    /// `VK_PRESENT_MODE_FIFO_KHR` (vsync ; required to be supported).
    Fifo = 2,
    /// `VK_PRESENT_MODE_FIFO_RELAXED_KHR`.
    FifoRelaxed = 3,
}

/// `VkSurfaceTransformFlagBitsKHR` — selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkSurfaceTransformFlagKHR {
    /// `VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR`.
    Identity = 0x0000_0001,
    /// `VK_SURFACE_TRANSFORM_INHERIT_BIT_KHR`.
    Inherit = 0x0000_0100,
}

/// `VkCompositeAlphaFlagBitsKHR` — selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkCompositeAlphaFlagKHR {
    /// `VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR`.
    Opaque = 0x0000_0001,
    /// `VK_COMPOSITE_ALPHA_PRE_MULTIPLIED_BIT_KHR`.
    PreMultiplied = 0x0000_0002,
    /// `VK_COMPOSITE_ALPHA_POST_MULTIPLIED_BIT_KHR`.
    PostMultiplied = 0x0000_0004,
    /// `VK_COMPOSITE_ALPHA_INHERIT_BIT_KHR`.
    Inherit = 0x0000_0008,
}

/// `VkSharingMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkSharingMode {
    /// `VK_SHARING_MODE_EXCLUSIVE` (single queue-family ; default).
    Exclusive = 0,
    /// `VK_SHARING_MODE_CONCURRENT` (multiple queue-families).
    Concurrent = 1,
}

/// `VkImageUsageFlagBits` — bitmask. Selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkImageUsageFlag {
    /// `VK_IMAGE_USAGE_TRANSFER_SRC_BIT`.
    TransferSrc = 0x0000_0001,
    /// `VK_IMAGE_USAGE_TRANSFER_DST_BIT`.
    TransferDst = 0x0000_0002,
    /// `VK_IMAGE_USAGE_SAMPLED_BIT`.
    Sampled = 0x0000_0004,
    /// `VK_IMAGE_USAGE_STORAGE_BIT`.
    Storage = 0x0000_0008,
    /// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT`.
    ColorAttachment = 0x0000_0010,
}

/// `VkImageUsageFlags` raw bitmask.
pub type VkImageUsageFlags = u32;

// ───────────────────────────────────────────────────────────────────
// § Swapchain-create-info.
// ───────────────────────────────────────────────────────────────────

/// `VkExtent2D`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VkExtent2D {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// `VkSwapchainCreateInfoKHR` — argument for `vkCreateSwapchainKHR`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkSwapchainCreateInfoKHR {
    /// Must be [`VkStructureType::SwapchainCreateInfoKhr`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Bitmask of `VkSwapchainCreateFlagBitsKHR` (rarely used in stage-0).
    pub flags: u32,
    /// Surface to present to.
    pub surface: VkSurfaceKHR,
    /// Minimum number of swap-chain images.
    pub min_image_count: u32,
    /// Image format.
    pub image_format: VkFormat,
    /// Image color-space.
    pub image_color_space: VkColorSpaceKHR,
    /// Image extent (width/height).
    pub image_extent: VkExtent2D,
    /// Number of array layers (typically 1 ; >1 only for stereo).
    pub image_array_layers: u32,
    /// Bitmask of [`VkImageUsageFlag`] for the swap-chain images.
    pub image_usage: VkImageUsageFlags,
    /// Sharing mode for the images across queue families.
    pub image_sharing_mode: VkSharingMode,
    /// Number of queue-family indices (only for concurrent sharing).
    pub queue_family_index_count: u32,
    /// Pointer to queue-family-indices array.
    pub p_queue_family_indices: *const u32,
    /// Pre-transform applied by the platform compositor.
    pub pre_transform: VkSurfaceTransformFlagKHR,
    /// Composite-alpha mode.
    pub composite_alpha: VkCompositeAlphaFlagKHR,
    /// Present mode (FIFO required to be supported).
    pub present_mode: VkPresentModeKHR,
    /// Whether the platform may discard pixels covered by other surfaces.
    pub clipped: u32,
    /// Old swapchain (for resize) or `VK_NULL_HANDLE`.
    pub old_swapchain: VkSwapchainKHR,
}

impl Default for VkSwapchainCreateInfoKHR {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::SwapchainCreateInfoKhr,
            p_next: core::ptr::null(),
            flags: 0,
            surface: VK_NULL_HANDLE_NDISP,
            min_image_count: 2,
            image_format: VkFormat::B8g8r8a8Unorm,
            image_color_space: VkColorSpaceKHR::SrgbNonlinear,
            image_extent: VkExtent2D::default(),
            image_array_layers: 1,
            image_usage: VkImageUsageFlag::ColorAttachment as u32,
            image_sharing_mode: VkSharingMode::Exclusive,
            queue_family_index_count: 0,
            p_queue_family_indices: core::ptr::null(),
            pre_transform: VkSurfaceTransformFlagKHR::Identity,
            composite_alpha: VkCompositeAlphaFlagKHR::Opaque,
            present_mode: VkPresentModeKHR::Fifo,
            clipped: 1,
            old_swapchain: VK_NULL_HANDLE_NDISP,
        }
    }
}

/// `VkPresentInfoKHR` — argument for `vkQueuePresentKHR`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkPresentInfoKHR {
    /// Must be [`VkStructureType::PresentInfoKhr`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Number of wait-semaphores.
    pub wait_semaphore_count: u32,
    /// Pointer to wait-semaphore array (length `wait_semaphore_count`).
    pub p_wait_semaphores: *const VkSemaphore,
    /// Number of swapchains being presented.
    pub swapchain_count: u32,
    /// Pointer to `VkSwapchainKHR` array.
    pub p_swapchains: *const VkSwapchainKHR,
    /// Pointer to image-index array (length `swapchain_count`).
    pub p_image_indices: *const u32,
    /// Pointer to per-swapchain VkResult array (or null).
    pub p_results: *mut i32,
}

impl Default for VkPresentInfoKHR {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::PresentInfoKhr,
            p_next: core::ptr::null(),
            wait_semaphore_count: 0,
            p_wait_semaphores: core::ptr::null(),
            swapchain_count: 0,
            p_swapchains: core::ptr::null(),
            p_image_indices: core::ptr::null(),
            p_results: core::ptr::null_mut(),
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// § C signature declarations.
// ───────────────────────────────────────────────────────────────────

/// `vkCreateSwapchainKHR` C signature.
pub type PfnVkCreateSwapchainKHR = unsafe extern "C" fn(
    device: VkDevice,
    p_create_info: *const VkSwapchainCreateInfoKHR,
    p_allocator: PVkAllocationCallbacks,
    p_swapchain: *mut VkSwapchainKHR,
) -> i32;

/// `vkDestroySwapchainKHR` C signature.
pub type PfnVkDestroySwapchainKHR = unsafe extern "C" fn(
    device: VkDevice,
    swapchain: VkSwapchainKHR,
    p_allocator: PVkAllocationCallbacks,
);

/// `vkGetSwapchainImagesKHR` C signature.
pub type PfnVkGetSwapchainImagesKHR = unsafe extern "C" fn(
    device: VkDevice,
    swapchain: VkSwapchainKHR,
    p_swapchain_image_count: *mut u32,
    p_swapchain_images: *mut VkImage,
) -> i32;

/// `vkAcquireNextImageKHR` C signature.
pub type PfnVkAcquireNextImageKHR = unsafe extern "C" fn(
    device: VkDevice,
    swapchain: VkSwapchainKHR,
    timeout: u64,
    semaphore: VkSemaphore,
    fence: VkFence,
    p_image_index: *mut u32,
) -> i32;

/// `vkQueuePresentKHR` C signature.
pub type PfnVkQueuePresentKHR = unsafe extern "C" fn(
    queue: VkQueue,
    p_present_info: *const VkPresentInfoKHR,
) -> i32;

// ───────────────────────────────────────────────────────────────────
// § Rust-side wrappers.
// ───────────────────────────────────────────────────────────────────

/// Errors surfaced by the swapchain layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapchainError {
    /// Loader returned NULL for a swapchain entry-point.
    LoaderMissingSymbol(String),
    /// Stage A : real loaders not yet wired.
    StubLoaderUnsupported,
    /// `vkAcquireNextImageKHR` returned VK_TIMEOUT (no image ready).
    AcquireTimeout,
    /// Surface handle was null.
    NullSurface,
}

/// Sentinel "image-not-acquired" index (matches the `0xFFFFFFFF`
/// `cssl-rt` swapchain-acquire timeout sentinel from
/// `__cssl_gpu_swapchain_acquire`).
pub const SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL: u32 = 0xFFFF_FFFF;

/// Owned-storage builder for [`VkSwapchainCreateInfoKHR`].
#[derive(Debug, Clone)]
pub struct SwapchainBuilder {
    surface: VkSurfaceKHR,
    extent: VkExtent2D,
    format: VkFormat,
    color_space: VkColorSpaceKHR,
    present_mode: VkPresentModeKHR,
    min_image_count: u32,
    image_usage: VkImageUsageFlags,
}

impl Default for SwapchainBuilder {
    fn default() -> Self {
        Self {
            surface: VK_NULL_HANDLE_NDISP,
            extent: VkExtent2D::default(),
            format: VkFormat::B8g8r8a8Unorm,
            color_space: VkColorSpaceKHR::SrgbNonlinear,
            present_mode: VkPresentModeKHR::Fifo,
            min_image_count: 2,
            image_usage: VkImageUsageFlag::ColorAttachment as u32,
        }
    }
}

impl SwapchainBuilder {
    /// Begin building a new swapchain-create-info.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind the target [`VkSurfaceKHR`].
    #[must_use]
    pub fn with_surface(mut self, surface: VkSurfaceKHR) -> Self {
        self.surface = surface;
        self
    }

    /// Set the surface extent.
    #[must_use]
    pub fn with_extent(mut self, w: u32, h: u32) -> Self {
        self.extent = VkExtent2D {
            width: w,
            height: h,
        };
        self
    }

    /// Set the image format.
    #[must_use]
    pub fn with_format(mut self, fmt: VkFormat) -> Self {
        self.format = fmt;
        self
    }

    /// Set the color-space.
    #[must_use]
    pub fn with_color_space(mut self, cs: VkColorSpaceKHR) -> Self {
        self.color_space = cs;
        self
    }

    /// Set the present-mode.
    #[must_use]
    pub fn with_present_mode(mut self, pm: VkPresentModeKHR) -> Self {
        self.present_mode = pm;
        self
    }

    /// Set the minimum-image-count.
    #[must_use]
    pub fn with_min_image_count(mut self, n: u32) -> Self {
        self.min_image_count = n;
        self
    }

    /// Set the image-usage bitmask.
    #[must_use]
    pub fn with_image_usage(mut self, usage: VkImageUsageFlags) -> Self {
        self.image_usage = usage;
        self
    }

    /// Build the FFI-shape struct (purely by-value ; no owned storage
    /// needed since swap-chain-create-info doesn't carry strings).
    #[must_use]
    pub fn build(&self) -> VkSwapchainCreateInfoKHR {
        VkSwapchainCreateInfoKHR {
            s_type: VkStructureType::SwapchainCreateInfoKhr,
            p_next: core::ptr::null(),
            flags: 0,
            surface: self.surface,
            min_image_count: self.min_image_count,
            image_format: self.format,
            image_color_space: self.color_space,
            image_extent: self.extent,
            image_array_layers: 1,
            image_usage: self.image_usage,
            image_sharing_mode: VkSharingMode::Exclusive,
            queue_family_index_count: 0,
            p_queue_family_indices: core::ptr::null(),
            pre_transform: VkSurfaceTransformFlagKHR::Identity,
            composite_alpha: VkCompositeAlphaFlagKHR::Opaque,
            present_mode: self.present_mode,
            clipped: 1,
            old_swapchain: VK_NULL_HANDLE_NDISP,
        }
    }

    /// Resolve `vkCreateSwapchainKHR` via the supplied loader and
    /// (Stage A) return the canned response.
    ///
    /// # Errors
    /// See [`SwapchainError`].
    pub fn create_with_loader<L: VulkanLoader>(
        &self,
        loader: &L,
    ) -> Result<VkSwapchainKHR, SwapchainError> {
        if self.surface == VK_NULL_HANDLE_NDISP {
            return Err(SwapchainError::NullSurface);
        }
        match loader.resolve(core::ptr::null_mut(), "vkCreateSwapchainKHR") {
            None => Err(SwapchainError::LoaderMissingSymbol(
                "vkCreateSwapchainKHR".to_string(),
            )),
            Some(_addr) if !loader.is_real() => Err(SwapchainError::StubLoaderUnsupported),
            Some(_addr) => Ok(VK_NULL_HANDLE_NDISP),
        }
    }
}

/// Acquire-call shape (mock for unit tests + cssl-rt host_gpu STUB).
///
/// # Errors
/// See [`SwapchainError`].
pub fn acquire_next_image_with_loader<L: VulkanLoader>(
    loader: &L,
    _swapchain: VkSwapchainKHR,
    timeout_ns: u64,
) -> Result<u32, SwapchainError> {
    match loader.resolve(core::ptr::null_mut(), "vkAcquireNextImageKHR") {
        None => Err(SwapchainError::LoaderMissingSymbol(
            "vkAcquireNextImageKHR".to_string(),
        )),
        Some(_addr) if !loader.is_real() => {
            // Match the cssl-rt sentinel : timeout_ns == 0 → fast-fail with timeout.
            if timeout_ns == 0 {
                return Err(SwapchainError::AcquireTimeout);
            }
            Err(SwapchainError::StubLoaderUnsupported)
        }
        Some(_addr) => Ok(0),
    }
}

/// Present-call shape.
///
/// # Errors
/// See [`SwapchainError`].
pub fn queue_present_with_loader<L: VulkanLoader>(
    loader: &L,
    _queue: VkQueue,
    _swapchain: VkSwapchainKHR,
    _image_index: u32,
) -> Result<(), SwapchainError> {
    match loader.resolve(core::ptr::null_mut(), "vkQueuePresentKHR") {
        None => Err(SwapchainError::LoaderMissingSymbol(
            "vkQueuePresentKHR".to_string(),
        )),
        Some(_addr) if !loader.is_real() => Err(SwapchainError::StubLoaderUnsupported),
        Some(_addr) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_next_image_with_loader, queue_present_with_loader, SwapchainBuilder,
        SwapchainError, VkColorSpaceKHR, VkFormat, VkImageUsageFlag, VkPresentModeKHR,
        SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL,
    };
    use crate::pure_ffi::{MockLoader, StubLoader, VK_NULL_HANDLE_NDISP};

    #[test]
    fn builder_records_extent_and_format() {
        let info = SwapchainBuilder::new()
            .with_surface(0xDEAD_BEEF)
            .with_extent(1920, 1080)
            .with_format(VkFormat::B8g8r8a8Srgb)
            .with_color_space(VkColorSpaceKHR::SrgbNonlinear)
            .with_present_mode(VkPresentModeKHR::Mailbox)
            .with_min_image_count(3)
            .with_image_usage(VkImageUsageFlag::ColorAttachment as u32)
            .build();
        assert_eq!(info.image_extent.width, 1920);
        assert_eq!(info.image_extent.height, 1080);
        assert_eq!(info.image_format, VkFormat::B8g8r8a8Srgb);
        assert_eq!(info.present_mode, VkPresentModeKHR::Mailbox);
        assert_eq!(info.min_image_count, 3);
        assert_eq!(info.surface, 0xDEAD_BEEF);
    }

    #[test]
    fn create_without_surface_errors() {
        let l = MockLoader::new();
        let r = SwapchainBuilder::new().create_with_loader(&l);
        assert!(matches!(r, Err(SwapchainError::NullSurface)));
        // Loader was NOT called because surface was rejected first.
        assert_eq!(l.resolve_count(), 0);
    }

    #[test]
    fn create_with_stub_loader_errors_with_missing_symbol() {
        let l = StubLoader;
        let r = SwapchainBuilder::new()
            .with_surface(0x1234)
            .create_with_loader(&l);
        assert!(matches!(r, Err(SwapchainError::LoaderMissingSymbol(ref n)) if n == "vkCreateSwapchainKHR"));
    }

    #[test]
    fn create_with_mock_loader_errors_with_stub_unsupported() {
        let l = MockLoader::new();
        let r = SwapchainBuilder::new()
            .with_surface(0x1234)
            .create_with_loader(&l);
        assert!(matches!(r, Err(SwapchainError::StubLoaderUnsupported)));
        assert_eq!(l.resolve_count(), 1);
    }

    #[test]
    fn acquire_with_timeout_zero_on_mock_loader_returns_timeout() {
        let l = MockLoader::new();
        let r = acquire_next_image_with_loader(&l, VK_NULL_HANDLE_NDISP, 0);
        assert!(matches!(r, Err(SwapchainError::AcquireTimeout)));
    }

    #[test]
    fn acquire_with_stub_loader_missing_symbol() {
        let l = StubLoader;
        let r = acquire_next_image_with_loader(&l, VK_NULL_HANDLE_NDISP, u64::MAX);
        assert!(matches!(r, Err(SwapchainError::LoaderMissingSymbol(_))));
    }

    #[test]
    fn present_with_stub_loader_missing_symbol() {
        let l = StubLoader;
        let r = queue_present_with_loader(&l, core::ptr::null_mut(), VK_NULL_HANDLE_NDISP, 0);
        assert!(matches!(r, Err(SwapchainError::LoaderMissingSymbol(_))));
    }

    #[test]
    fn timeout_sentinel_matches_cssl_rt() {
        // cssl-rt host_gpu uses 0xFFFF_FFFF as the swapchain-acquire-timeout sentinel ;
        // pure_ffi mirrors that constant for parity at the symbol seam.
        assert_eq!(SWAPCHAIN_ACQUIRE_TIMEOUT_SENTINEL, 0xFFFF_FFFF);
    }
}

//! § ffi::swapchain : `XrSwapchain` stereo + acquire/release/wait FFI.
//!
//! § SPEC : OpenXR 1.0 § 10 (Swapchains). A swapchain is a ring of
//!          GPU-resident images the runtime composites. Stereo = two
//!          swapchains (one per eye) OR a single array-2 swapchain
//!          (preferred — see `array_size = 2`).
//!
//! § STEREO STRATEGY
//!   The Quest-3s runtime supports both layouts ; the canonical CSSLv3
//!   path uses array-2 (multiview). `array_size = 2` + `face_count = 1`
//!   + `mip_count = 1` + `sample_count = 1` is the canonical config.

use super::result::XrResult;
use super::types::StructureType;
use bitflags::bitflags;

/// FFI handle for `XrSwapchain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct SwapchainHandle(pub u64);

impl SwapchainHandle {
    pub const NULL: Self = Self(0);

    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

bitflags! {
    /// `XrSwapchainUsageFlags`. § 10.1 spec.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[repr(transparent)]
    pub struct SwapchainUsageFlags: u64 {
        const COLOR_ATTACHMENT          = 0x0000_0001;
        const DEPTH_STENCIL_ATTACHMENT  = 0x0000_0002;
        const UNORDERED_ACCESS          = 0x0000_0004;
        const TRANSFER_SRC              = 0x0000_0008;
        const TRANSFER_DST              = 0x0000_0010;
        const SAMPLED                   = 0x0000_0020;
        const MUTABLE_FORMAT            = 0x0000_0040;
        const INPUT_ATTACHMENT          = 0x0000_0080;
    }
}

/// `XrSwapchainCreateInfo` ; FFI struct. The `format` is opaque (the
/// runtime accepts a graphics-API-specific value — e.g. Vulkan's
/// `VK_FORMAT_R8G8B8A8_SRGB` numeric value 43).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SwapchainCreateInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub create_flags: u64,
    pub usage_flags: SwapchainUsageFlags,
    pub format: i64,
    pub sample_count: u32,
    pub width: u32,
    pub height: u32,
    pub face_count: u32,
    pub array_size: u32,
    pub mip_count: u32,
}

impl SwapchainCreateInfo {
    /// Quest-3s canonical stereo color swapchain.
    #[must_use]
    pub const fn quest_3s_stereo_color(width: u32, height: u32) -> Self {
        Self {
            ty: StructureType::SwapchainCreateInfo,
            next: core::ptr::null(),
            create_flags: 0,
            usage_flags: SwapchainUsageFlags::COLOR_ATTACHMENT,
            // 43 = VK_FORMAT_R8G8B8A8_SRGB (canonical sRGB color).
            format: 43,
            sample_count: 1,
            width,
            height,
            face_count: 1,
            // Array-2 multiview ; 2 layers = stereo.
            array_size: 2,
            mip_count: 1,
        }
    }

    /// Quest-3s canonical stereo depth swapchain (D24-S8 = VK_FORMAT 129).
    #[must_use]
    pub const fn quest_3s_stereo_depth(width: u32, height: u32) -> Self {
        Self {
            ty: StructureType::SwapchainCreateInfo,
            next: core::ptr::null(),
            create_flags: 0,
            usage_flags: SwapchainUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            format: 129,
            sample_count: 1,
            width,
            height,
            face_count: 1,
            array_size: 2,
            mip_count: 1,
        }
    }

    /// `true` iff this is a stereo (array-2) swapchain.
    #[must_use]
    pub const fn is_stereo(&self) -> bool {
        self.array_size == 2
    }
}

/// `XrSwapchainImageAcquireInfo`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SwapchainAcquireInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
}

impl SwapchainAcquireInfo {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            ty: StructureType::Unknown,
            next: core::ptr::null(),
        }
    }
}

/// `XrSwapchainImageWaitInfo` ; nanosecond timeout.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SwapchainImageWaitInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
    pub timeout: i64,
}

impl SwapchainImageWaitInfo {
    /// Standard 100-ms timeout that matches the Quest-3s runtime expectation.
    #[must_use]
    pub const fn with_timeout_ms(ms: i64) -> Self {
        Self {
            ty: StructureType::Unknown,
            next: core::ptr::null(),
            timeout: ms.saturating_mul(1_000_000),
        }
    }
}

/// `XrSwapchainImageReleaseInfo`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SwapchainImageReleaseInfo {
    pub ty: StructureType,
    pub next: *const core::ffi::c_void,
}

impl SwapchainImageReleaseInfo {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            ty: StructureType::Unknown,
            next: core::ptr::null(),
        }
    }
}

/// In-memory mock swapchain. Tracks acquire / release / wait calls and
/// rotates a ring-counter to mimic the GPU image-cycle.
#[derive(Debug, Clone)]
pub struct MockSwapchain {
    pub handle: SwapchainHandle,
    pub create_info: SwapchainCreateInfo,
    pub image_count: u32,
    pub acquired_index: Option<u32>,
    pub next_index: u32,
    pub acquire_calls: u64,
    pub wait_calls: u64,
    pub release_calls: u64,
}

impl MockSwapchain {
    /// `xrCreateSwapchain` mock. Validates that array-size > 0 and image
    /// dimensions are non-zero.
    pub fn create(create_info: SwapchainCreateInfo) -> Result<Self, XrResult> {
        if create_info.width == 0 || create_info.height == 0 {
            return Err(XrResult::ERROR_VALIDATION_FAILURE);
        }
        if create_info.array_size == 0 {
            return Err(XrResult::ERROR_VALIDATION_FAILURE);
        }
        // Quest-3s default ring depth = 3 ; matches the typical
        // double-buffered + 1 in-flight scheme.
        Ok(Self {
            handle: SwapchainHandle(0xC551_5C00),
            create_info,
            image_count: 3,
            acquired_index: None,
            next_index: 0,
            acquire_calls: 0,
            wait_calls: 0,
            release_calls: 0,
        })
    }

    /// `xrAcquireSwapchainImage` mock : returns the next image index in the
    /// ring. Refuses if there is already an outstanding acquire (matches
    /// the OpenXR spec that mandates one outstanding-acquire at a time).
    pub fn acquire(&mut self) -> Result<u32, XrResult> {
        if self.acquired_index.is_some() {
            return Err(XrResult::ERROR_CALL_ORDER_INVALID);
        }
        let idx = self.next_index % self.image_count;
        self.acquired_index = Some(idx);
        self.next_index = self.next_index.wrapping_add(1);
        self.acquire_calls = self.acquire_calls.saturating_add(1);
        Ok(idx)
    }

    /// `xrWaitSwapchainImage` mock : succeeds immediately. Real impl waits
    /// for GPU presentation completion.
    pub fn wait(&mut self, _info: &SwapchainImageWaitInfo) -> XrResult {
        if self.acquired_index.is_none() {
            return XrResult::ERROR_CALL_ORDER_INVALID;
        }
        self.wait_calls = self.wait_calls.saturating_add(1);
        XrResult::SUCCESS
    }

    /// `xrReleaseSwapchainImage` mock : releases the outstanding image so
    /// the next acquire may succeed.
    pub fn release(&mut self, _info: &SwapchainImageReleaseInfo) -> XrResult {
        if self.acquired_index.is_none() {
            return XrResult::ERROR_CALL_ORDER_INVALID;
        }
        self.acquired_index = None;
        self.release_calls = self.release_calls.saturating_add(1);
        XrResult::SUCCESS
    }

    /// Drive a full acquire → wait → release round-trip. Returns the
    /// acquired image-index ; useful as a smoke-test in higher layers.
    pub fn round_trip(&mut self) -> Result<u32, XrResult> {
        let idx = self.acquire()?;
        let r = self.wait(&SwapchainImageWaitInfo::with_timeout_ms(100));
        if r.is_failure() {
            return Err(r);
        }
        let r = self.release(&SwapchainImageReleaseInfo::empty());
        if r.is_failure() {
            return Err(r);
        }
        Ok(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::{MockSwapchain, SwapchainCreateInfo, XrResult};

    #[test]
    fn quest_3s_stereo_color_is_array_2() {
        let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
        assert!(info.is_stereo());
        assert_eq!(info.array_size, 2);
        assert_eq!(info.face_count, 1);
        assert_eq!(info.mip_count, 1);
        assert_eq!(info.sample_count, 1);
    }

    #[test]
    fn create_with_zero_extent_is_validation_failure() {
        let mut info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
        info.width = 0;
        let r = MockSwapchain::create(info);
        assert_eq!(r.unwrap_err(), XrResult::ERROR_VALIDATION_FAILURE);
    }

    #[test]
    fn round_trip_advances_ring_index() {
        let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
        let mut sc = MockSwapchain::create(info).expect("create");
        let i0 = sc.round_trip().expect("rt0");
        let i1 = sc.round_trip().expect("rt1");
        let i2 = sc.round_trip().expect("rt2");
        let i3 = sc.round_trip().expect("rt3");
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert_eq!(i2, 2);
        // Ring wraps after image_count = 3.
        assert_eq!(i3, 0);
        assert_eq!(sc.acquire_calls, 4);
        assert_eq!(sc.release_calls, 4);
    }

    #[test]
    fn double_acquire_is_call_order_invalid() {
        let info = SwapchainCreateInfo::quest_3s_stereo_color(2064, 2208);
        let mut sc = MockSwapchain::create(info).expect("create");
        let _ = sc.acquire().expect("first");
        let r = sc.acquire();
        assert_eq!(r.unwrap_err(), XrResult::ERROR_CALL_ORDER_INVALID);
    }
}

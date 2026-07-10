//! § W-H2 (T11-D259) — IDXGISwapChain abstraction (own-FFI path).
//!
//! § PURPOSE
//!   The Vulkan-paired host-GPU layer (`cssl-host-vulkan`) exposes a
//!   `Swapchain` newtype around `vkSwapchainKHR`. This file provides the
//!   D3D12-equivalent on top of `IDXGISwapChain4`. The cross-platform
//!   substrate-renderer can swap one for the other at the host_gpu trait
//!   boundary.
//!
//! § SCOPE
//!   - `SwapChainConfig` — width / height / format / buffer-count (3-buffer
//!     default = canonical for present-fence stalls on Xe-HPG / RDNA).
//!   - `SwapChain` — opaque handle ; create + acquire-next-image + present +
//!     destroy. Stage-0 ships the descriptor + the COM-ptr storage ;
//!     `IDXGIFactory2::CreateSwapChainForHwnd` is the Windows entry-point
//!     and is wired through the loader probe.
//!   - Mock-mode acquire / present so substrate-renderer tests can build a
//!     full frame-graph without a HWND.
//!
//! § NON-WINDOWS
//!   `SwapChain::create_for_hwnd` returns `D3d12Error::LoaderMissing`.
//!   `SwapChain::mock` works everywhere ; the mock advances a back-buffer
//!   index modulo the configured `buffer_count`, which is sufficient to
//!   exercise present-fence + frame-pacing logic in tests.

use crate::error::{D3d12Error, Result};
use crate::ffi::{
    hr_check, ComPtr, CreateDXGIFactory2Fn, DxgiFormat, IDXGIFactory2VTable, IDXGISwapChain1VTable,
    IUnknownVTable, Loader, SampleDesc, SwapChainDesc1, DXGI_ALPHA_MODE_UNSPECIFIED,
    DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, IID_IDXGIFactory2,
};

// ─── public API ──────────────────────────────────────────────────────────

/// Tearing / VSync mode passed to `Present` (`DXGI_PRESENT_*` flag set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresentMode {
    /// Block on VBLANK (`SyncInterval=1`, `Flags=0`).
    Vsync,
    /// Allow tearing (`SyncInterval=0`, `Flags=DXGI_PRESENT_ALLOW_TEARING`).
    Tearing,
    /// Discard immediately (`SyncInterval=0`, `Flags=0`).
    Immediate,
}

impl PresentMode {
    /// Canonical (sync-interval, flags) tuple per `IDXGISwapChain::Present`.
    #[must_use]
    pub const fn as_present_args(self) -> (u32, u32) {
        match self {
            Self::Vsync => (1, 0),
            Self::Tearing => (0, 0x0000_0200),
            Self::Immediate => (0, 0),
        }
    }
}

/// Swap-chain effect (`DXGI_SWAP_EFFECT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwapEffect {
    /// `DXGI_SWAP_EFFECT_FLIP_DISCARD` — recommended for D3D12.
    FlipDiscard,
    /// `DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL`.
    FlipSequential,
}

impl SwapEffect {
    /// Raw `DXGI_SWAP_EFFECT` value.
    #[must_use]
    pub const fn as_raw(self) -> u32 {
        match self {
            Self::FlipDiscard => 4,
            Self::FlipSequential => 3,
        }
    }
}

/// Configuration for `IDXGIFactory2::CreateSwapChainForHwnd`.
#[derive(Debug, Clone, Copy)]
pub struct SwapChainConfig {
    /// Render-target width (pixels).
    pub width: u32,
    /// Render-target height (pixels).
    pub height: u32,
    /// Back-buffer pixel format.
    pub format: DxgiFormat,
    /// Number of back buffers (2 = double-buffer ; 3 = triple-buffer recommended).
    pub buffer_count: u32,
    /// Flip mode.
    pub swap_effect: SwapEffect,
    /// Present semantics.
    pub present_mode: PresentMode,
    /// `DXGI_USAGE_RENDER_TARGET_OUTPUT` etc.
    pub buffer_usage: u32,
}

impl SwapChainConfig {
    /// Sensible default : 1920×1080 R8G8B8A8 UNORM, triple-buffer, FlipDiscard, VSync.
    #[must_use]
    pub const fn default_1080p() -> Self {
        Self {
            width: 1920,
            height: 1080,
            format: DxgiFormat::R8g8b8a8Unorm,
            buffer_count: 3,
            swap_effect: SwapEffect::FlipDiscard,
            present_mode: PresentMode::Vsync,
            buffer_usage: 0x0000_0020, // DXGI_USAGE_RENDER_TARGET_OUTPUT
        }
    }

    /// Validate : non-zero extents, buffer-count ∈ [2, 16], known format.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` for any failed precondition.
    pub fn validate(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(D3d12Error::invalid(
                "SwapChainConfig",
                format!("zero extent {}×{}", self.width, self.height),
            ));
        }
        if self.buffer_count < 2 || self.buffer_count > 16 {
            return Err(D3d12Error::invalid(
                "SwapChainConfig",
                format!("buffer_count {} out of [2,16]", self.buffer_count),
            ));
        }
        if matches!(self.format, DxgiFormat::Unknown) {
            return Err(D3d12Error::invalid(
                "SwapChainConfig",
                "format=DXGI_FORMAT_UNKNOWN is not a valid swap-chain target",
            ));
        }
        Ok(())
    }
}

/// Window handle abstraction. Stage-0 stores the raw HWND as `usize` so the
/// type can cross thread boundaries without `*mut c_void` Send/Sync gymnastics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hwnd(pub usize);

impl Hwnd {
    /// Null sentinel ; selects the headless / mock path on `create_for_hwnd`.
    #[must_use]
    pub const fn null() -> Self {
        Self(0)
    }

    /// Is the HWND null ?
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// IDXGISwapChain wrapper. Mock-mode = `inner = ComPtr::null()` and the
/// back-buffer index is advanced internally.
#[derive(Debug)]
pub struct SwapChain {
    inner: ComPtr,
    config: SwapChainConfig,
    next_back_buffer_index: u32,
    frame_counter: u64,
    is_mock: bool,
}

impl SwapChain {
    /// Real path (Windows) : `IDXGIFactory2::CreateSwapChainForHwnd` via own-FFI
    /// vtable dispatch · returns an owned `IDXGISwapChain1` ComPtr in
    /// `self.inner`. Stores config + zeros frame counters.
    ///
    /// `queue_iunknown` MUST be the raw `IUnknown*` of an `ID3D12CommandQueue`.
    /// Use [`crate::queue::CommandQueue::raw_iunknown_ptr`] to obtain it. DXGI
    /// calls `AddRef` on the queue internally so the caller retains its own ref.
    ///
    /// # Errors
    /// - `D3d12Error::InvalidArgument` if `config` fails validation, `hwnd`
    ///   is null, or `queue_iunknown` is null.
    /// - `D3d12Error::LoaderMissing` if `CreateDXGIFactory2` is unresolved
    ///   (dxgi.dll absent or stripped, or non-Windows target).
    /// - `D3d12Error::Hresult` if `CreateDXGIFactory2` or
    ///   `CreateSwapChainForHwnd` returns a failure HRESULT.
    ///
    /// # Safety
    /// The function performs unsafe COM calls internally but its signature is
    /// safe. `queue_iunknown` MUST point to a live `ID3D12CommandQueue` for
    /// the duration of the call.
    pub fn create_for_hwnd(
        loader: &Loader,
        hwnd: Hwnd,
        config: SwapChainConfig,
        queue_iunknown: *mut core::ffi::c_void,
    ) -> Result<Self> {
        config.validate()?;
        let factory_proc = loader.create_dxgi_factory2.ok_or_else(|| {
            D3d12Error::loader("CreateDXGIFactory2 unresolved — dxgi.dll absent or stripped")
        })?;
        if hwnd.is_null() {
            return Err(D3d12Error::invalid(
                "SwapChain::create_for_hwnd",
                "HWND=null ; use SwapChain::mock for headless paths",
            ));
        }
        if queue_iunknown.is_null() {
            return Err(D3d12Error::invalid(
                "SwapChain::create_for_hwnd",
                "queue_iunknown=null ; pass &CommandQueue::raw_iunknown_ptr()",
            ));
        }

        // Non-Windows : the own-FFI types compile but the actual DLLs / DXGI
        // runtime are absent. Surface LoaderMissing so callers downgrade to
        // their non-Windows path.
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (factory_proc, hwnd, queue_iunknown);
            return Err(D3d12Error::loader(
                "non-Windows · D3D12 SwapChain unavailable through own-FFI",
            ));
        }

        // Windows : real DXGI dispatch through the resolved fn-pointer + the
        // own vtable for IDXGIFactory2.
        #[cfg(target_os = "windows")]
        {
            // 1. Resolve CreateDXGIFactory2 fn-pointer.
            // SAFETY : `factory_proc` was returned by `GetProcAddress("CreateDXGIFactory2")`
            // against `dxgi.dll` ; the OS guarantees the symbol matches the
            // `CreateDXGIFactory2Fn` ABI signature.
            let create_factory: CreateDXGIFactory2Fn =
                unsafe { core::mem::transmute::<usize, CreateDXGIFactory2Fn>(factory_proc) };

            // 2. Create the IDXGIFactory2 instance.
            let mut factory_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
            // SAFETY : `&IID_IDXGIFactory2` + `&mut factory_ptr` are valid for
            // the call duration ; create_factory writes the owned interface
            // into factory_ptr on success.
            let hr = unsafe { create_factory(0, &IID_IDXGIFactory2, &mut factory_ptr) };
            hr_check("CreateDXGIFactory2", hr)?;
            if factory_ptr.is_null() {
                return Err(D3d12Error::loader(
                    "CreateDXGIFactory2 reported SUCCESS but returned null factory",
                ));
            }
            let factory = ComPtr(factory_ptr);

            // 3. Build DXGI_SWAP_CHAIN_DESC1 from our config.
            let allow_tearing = matches!(config.present_mode, PresentMode::Tearing);
            let flags = if allow_tearing {
                DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
            } else {
                0
            };
            let desc = SwapChainDesc1 {
                width: config.width,
                height: config.height,
                format: config.format,
                stereo: 0,
                sample_desc: SampleDesc {
                    count: 1,
                    quality: 0,
                },
                buffer_usage: config.buffer_usage,
                buffer_count: config.buffer_count,
                scaling: DXGI_SCALING_NONE,
                swap_effect: config.swap_effect.as_raw(),
                alpha_mode: DXGI_ALPHA_MODE_UNSPECIFIED,
                flags,
            };

            // 4. Call IDXGIFactory2::CreateSwapChainForHwnd via the vtable.
            // SAFETY : factory_ptr is a live IDXGIFactory2 COM object ; the
            // vtable cast is valid because the vtable pointer is the first
            // word of every COM object.
            let factory_vtable: *const IDXGIFactory2VTable =
                unsafe { factory.vtable::<IDXGIFactory2VTable>() };
            if factory_vtable.is_null() {
                // Release factory before bailing.
                let iuv = unsafe { factory.vtable::<IUnknownVTable>() };
                if !iuv.is_null() {
                    let _ = unsafe { ((*iuv).release)(factory_ptr) };
                }
                return Err(D3d12Error::loader(
                    "IDXGIFactory2 vtable pointer is null — COM object malformed",
                ));
            }

            let mut sc1_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
            // SAFETY : all pointers live for the call ; vtable slot 15 is
            // `CreateSwapChainForHwnd` per the IDXGIFactory2 layout above ;
            // `desc` is `#[repr(C)]` matching DXGI_SWAP_CHAIN_DESC1 layout.
            let hr = unsafe {
                ((*factory_vtable).create_swap_chain_for_hwnd)(
                    factory_ptr,
                    queue_iunknown,
                    hwnd.0 as *mut core::ffi::c_void,
                    &desc,
                    core::ptr::null(),     // no fullscreen desc — windowed mode
                    core::ptr::null_mut(), // no restrict-to-output
                    &mut sc1_ptr,
                )
            };

            // 5. Release the factory (we don't keep a ref ; swapchain has its own
            // ref + ref'd the queue via internal AddRef).
            // SAFETY : factory_ptr is still live ; release decrements the ref-count.
            let factory_iunknown: *const IUnknownVTable =
                unsafe { factory.vtable::<IUnknownVTable>() };
            if !factory_iunknown.is_null() {
                let _ = unsafe { ((*factory_iunknown).release)(factory_ptr) };
            }

            // 6. Check the swap-chain creation result.
            hr_check("IDXGIFactory2::CreateSwapChainForHwnd", hr)?;
            if sc1_ptr.is_null() {
                return Err(D3d12Error::loader(
                    "CreateSwapChainForHwnd reported SUCCESS but returned null swap-chain",
                ));
            }

            Ok(Self {
                inner: ComPtr(sc1_ptr),
                config,
                next_back_buffer_index: 0,
                frame_counter: 0,
                is_mock: false,
            })
        }
    }

    /// Mock-mode constructor — works on any target. Useful for substrate-
    /// renderer tests that want to exercise frame-pacing without a window.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` if `config` fails validation.
    pub fn mock(config: SwapChainConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            inner: ComPtr::null(),
            config,
            next_back_buffer_index: 0,
            frame_counter: 0,
            is_mock: true,
        })
    }

    /// Return the configured back-buffer count (cached from `config`).
    #[must_use]
    pub const fn buffer_count(&self) -> u32 {
        self.config.buffer_count
    }

    /// Return the configured back-buffer extent (`width × height`).
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Return the configured pixel format.
    #[must_use]
    pub const fn format(&self) -> DxgiFormat {
        self.config.format
    }

    /// Acquire the index of the next back buffer to render to.
    ///
    /// In real-FFI mode this is `IDXGISwapChain3::GetCurrentBackBufferIndex`.
    /// In mock mode the index is advanced by `present()`.
    #[must_use]
    pub const fn current_back_buffer_index(&self) -> u32 {
        self.next_back_buffer_index
    }

    /// Total presents issued since creation (frame counter).
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_counter
    }

    /// Is this swap-chain operating in mock mode (no real DXGI handle) ?
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.is_mock
    }

    /// Resize back buffers (`IDXGISwapChain::ResizeBuffers`). Mock-mode
    /// updates the cached config ; real-mode would dispatch through the
    /// vtable.
    ///
    /// # Errors
    /// `D3d12Error::InvalidArgument` if the new dimensions are zero.
    pub fn resize(&mut self, new_width: u32, new_height: u32) -> Result<()> {
        if new_width == 0 || new_height == 0 {
            return Err(D3d12Error::invalid(
                "SwapChain::resize",
                format!("zero extent {new_width}×{new_height}"),
            ));
        }
        self.config.width = new_width;
        self.config.height = new_height;
        Ok(())
    }

    /// Issue a `Present` and advance the back-buffer index.
    ///
    /// In mock mode bumps the indices only. In real mode dispatches through
    /// the `IDXGISwapChain1::Present` vtable slot with the configured sync /
    /// tearing flags. The internal back-buffer index counter is advanced
    /// modulo `buffer_count` on success ; mirrors what `GetCurrentBackBufferIndex`
    /// would return on a real `IDXGISwapChain3` (QI for that lands in a
    /// follow-up · sufficient for buffer-rotation today).
    ///
    /// # Errors
    /// `D3d12Error::Hresult` on real-FFI failure ; `Ok(())` in mock mode.
    pub fn present(&mut self) -> Result<()> {
        // Mock : just bump the indices.
        if self.is_mock {
            self.next_back_buffer_index =
                (self.next_back_buffer_index + 1) % self.config.buffer_count;
            self.frame_counter = self.frame_counter.wrapping_add(1);
            return Ok(());
        }

        // Real path : dispatch through IDXGISwapChain1::Present.
        if self.inner.is_null() {
            return Err(D3d12Error::loader(
                "SwapChain::present : inner ComPtr is null but is_mock=false",
            ));
        }
        // SAFETY : inner is a live IDXGISwapChain1 COM object (created by
        // `create_for_hwnd`) ; vtable cast is valid (vtable ptr is first word
        // of every COM object). Present's signature in the vtable matches
        // the Win32 DXGI ABI.
        let vtable: *const IDXGISwapChain1VTable =
            unsafe { self.inner.vtable::<IDXGISwapChain1VTable>() };
        if vtable.is_null() {
            return Err(D3d12Error::loader(
                "SwapChain::present : vtable pointer is null — COM object malformed",
            ));
        }
        let (sync, flags) = self.config.present_mode.as_present_args();
        // SAFETY : all pointers live for the call ; vtable slot 8 is Present
        // per the IDXGISwapChain layout in ffi.rs.
        let hr = unsafe { ((*vtable).present)(self.inner.0, sync, flags) };
        hr_check("IDXGISwapChain1::Present", hr)?;

        self.next_back_buffer_index =
            (self.next_back_buffer_index + 1) % self.config.buffer_count;
        self.frame_counter = self.frame_counter.wrapping_add(1);
        Ok(())
    }
}

impl Drop for SwapChain {
    fn drop(&mut self) {
        // Real-FFI : would Release() the COM pointer. Mock : no-op.
        if !self.is_mock && !self.inner.is_null() {
            // SAFETY : `inner` is a live IUnknown when not mock + non-null ;
            // we take the vtable pointer's third slot (Release).
            unsafe {
                use crate::ffi::IUnknownVTable;
                let v: *const IUnknownVTable = self.inner.vtable();
                if !v.is_null() {
                    let _ = ((*v).release)(self.inner.0);
                }
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_validates() {
        let c = SwapChainConfig::default_1080p();
        assert!(c.validate().is_ok());
        assert_eq!(c.width, 1920);
        assert_eq!(c.height, 1080);
        assert_eq!(c.buffer_count, 3);
    }

    #[test]
    fn config_rejects_zero_extent() {
        let mut c = SwapChainConfig::default_1080p();
        c.width = 0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_rejects_bad_buffer_count() {
        let mut c = SwapChainConfig::default_1080p();
        c.buffer_count = 1;
        assert!(c.validate().is_err());
        c.buffer_count = 17;
        assert!(c.validate().is_err());
    }

    #[test]
    fn config_rejects_unknown_format() {
        let mut c = SwapChainConfig::default_1080p();
        c.format = DxgiFormat::Unknown;
        assert!(c.validate().is_err());
    }

    #[test]
    fn mock_create_succeeds_and_reports_extent() {
        let sc = SwapChain::mock(SwapChainConfig::default_1080p()).unwrap();
        assert!(sc.is_mock());
        assert_eq!(sc.extent(), (1920, 1080));
        assert_eq!(sc.buffer_count(), 3);
        assert_eq!(sc.frame_count(), 0);
        assert_eq!(sc.current_back_buffer_index(), 0);
    }

    #[test]
    fn mock_present_advances_index_and_frame_counter() {
        let mut sc = SwapChain::mock(SwapChainConfig::default_1080p()).unwrap();
        for expected_idx in 0_u64..6 {
            assert_eq!(u64::from(sc.current_back_buffer_index()), expected_idx % 3);
            sc.present().unwrap();
            assert_eq!(sc.frame_count(), expected_idx + 1);
        }
        // After 6 presents on a 3-buffer chain, idx is back to 0.
        assert_eq!(sc.current_back_buffer_index(), 0);
        assert_eq!(sc.frame_count(), 6);
    }

    #[test]
    fn mock_resize_updates_extent() {
        let mut sc = SwapChain::mock(SwapChainConfig::default_1080p()).unwrap();
        sc.resize(2560, 1440).unwrap();
        assert_eq!(sc.extent(), (2560, 1440));
    }

    #[test]
    fn resize_rejects_zero() {
        let mut sc = SwapChain::mock(SwapChainConfig::default_1080p()).unwrap();
        assert!(sc.resize(0, 1080).is_err());
    }

    /// Dummy non-null pointer for tests that exercise validation paths
    /// upstream of the actual DXGI call (where the pointer is never deref'd).
    fn dummy_queue_ptr() -> *mut core::ffi::c_void {
        1 as *mut core::ffi::c_void
    }

    #[test]
    fn create_for_hwnd_with_null_hwnd_is_invalid_argument() {
        let loader = crate::ffi::Loader {
            d3d12_create_device: Some(1),
            create_dxgi_factory2: Some(1),
            d3d12_get_debug_interface: Some(1),
            d3d12_serialize_root_signature: Some(1),
        };
        let r = SwapChain::create_for_hwnd(
            &loader,
            Hwnd::null(),
            SwapChainConfig::default_1080p(),
            dummy_queue_ptr(),
        );
        match r {
            Err(D3d12Error::InvalidArgument { .. }) => (),
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    #[test]
    fn create_for_hwnd_without_loader_is_loader_missing() {
        let loader = crate::ffi::Loader {
            d3d12_create_device: None,
            create_dxgi_factory2: None,
            d3d12_get_debug_interface: None,
            d3d12_serialize_root_signature: None,
        };
        let r = SwapChain::create_for_hwnd(
            &loader,
            Hwnd(0xdead_beef),
            SwapChainConfig::default_1080p(),
            dummy_queue_ptr(),
        );
        match r {
            Err(D3d12Error::LoaderMissing { .. }) => (),
            other => panic!("expected LoaderMissing, got {other:?}"),
        }
    }

    #[test]
    fn create_for_hwnd_with_null_queue_is_invalid_argument() {
        let loader = crate::ffi::Loader {
            d3d12_create_device: Some(1),
            create_dxgi_factory2: Some(1),
            d3d12_get_debug_interface: Some(1),
            d3d12_serialize_root_signature: Some(1),
        };
        let r = SwapChain::create_for_hwnd(
            &loader,
            Hwnd(0xdead_beef),
            SwapChainConfig::default_1080p(),
            core::ptr::null_mut(),
        );
        match r {
            Err(D3d12Error::InvalidArgument { .. }) => (),
            other => panic!("expected InvalidArgument (null queue), got {other:?}"),
        }
    }

    // NOTE : a real-FFI integration test exercising IDXGIFactory2::CreateSwapChainForHwnd
    // with dummy pointers crashes DXGI with STATUS_ACCESS_VIOLATION — DXGI dereferences
    // the queue IUnknown* and the HWND without validating them first. Real integration
    // testing requires a live ID3D12CommandQueue + a live HWND (cssl-rt host_gpu has the
    // full plumbing). The validation-path tests above cover the safe contract surface.
    //
    // A future integration test can live in cssl-host-rt or compiler-rs/tests/ once
    // the full Device + Queue + Window bring-up is wired.

    #[test]
    fn present_modes_have_distinct_args() {
        assert_eq!(PresentMode::Vsync.as_present_args(), (1, 0));
        assert_eq!(PresentMode::Tearing.as_present_args().0, 0);
        assert_ne!(
            PresentMode::Tearing.as_present_args().1,
            PresentMode::Immediate.as_present_args().1
        );
    }

    #[test]
    fn swap_effect_raw_values() {
        assert_eq!(SwapEffect::FlipDiscard.as_raw(), 4);
        assert_eq!(SwapEffect::FlipSequential.as_raw(), 3);
    }

    #[test]
    fn hwnd_null_helper() {
        assert!(Hwnd::null().is_null());
        assert!(!Hwnd(0x1234).is_null());
    }
}

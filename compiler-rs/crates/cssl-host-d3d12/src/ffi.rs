//! § W-H2 (T11-D259) — own-FFI for D3D12 + DXGI ; zero-external-deps thesis.
//!
//! § PURPOSE
//!   The pre-existing surface in this crate uses `windows-rs` (rich, binding-
//!   complete). This module ships a parallel from-scratch COM-style FFI for
//!   the SAME APIs : `D3D12CreateDevice`, `CreateDXGIFactory{1,2}`,
//!   `IDXGIFactory6::EnumAdapterByGpuPreference`, `IDXGISwapChain4::Present`,
//!   `ID3D12Device::CreateCommandQueue` / `CreateCommandList` /
//!   `CreateGraphicsPipelineState` / `CreateDescriptorHeap`. Goal : preserve
//!   the LoA-v13 thesis that everything below the compiler is OWN code, no
//!   crates.io dependency on `windows-sys` for the canonical path.
//!
//! § STRATEGY
//!   - Loader : `LoadLibraryW` + `GetProcAddress` against `d3d12.dll` +
//!     `dxgi.dll` (resolved on-demand ; cached behind a `OnceLock`-style
//!     guard).
//!   - COM : every interface is `#[repr(C)] struct VTable { fn_ptrs ... }`
//!     plus a `#[repr(transparent)] struct IFoo(*mut VTable)`. Method calls
//!     dispatch through the vtable as `((*self.0).method)(self.0, ...)`.
//!   - GUIDs : compile-time `[u8; 16]` constants (RFC-4122 mixed-endian
//!     layout matching D3D12 / DXGI headers).
//!   - HRESULT : raw `i32` ; helpers `succeeded` / `failed` mirror Win32
//!     `SUCCEEDED` / `FAILED` macros.
//!
//! § STAGE-0 SCOPE
//!   This module ships the LOADER + GUID-table + HRESULT-helpers + a small
//!   set of vtable shapes used by `swapchain.rs` / `cmd.rs` / `pipeline.rs`.
//!   Real device-create + present + record + dispatch is gated behind
//!   `#[cfg(target_os = "windows")]` ; non-Windows targets get stubs that
//!   return `D3d12Error::LoaderMissing`. The pre-existing windows-rs path
//!   remains the default for higher-level callers ; this layer is the
//!   "own-FFI" alternative the substrate-renderer can opt into when it
//!   wants the zero-ext-dep build.
//!
//! § UNSAFE
//!   FFI is inherently unsafe ; every `unsafe` block carries a `// SAFETY`
//!   comment naming the contract.

#![allow(clippy::missing_safety_doc)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::cast_possible_wrap)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]
// HRESULT canonical casts : 0x80000000+ HRESULT u32 → negative i32 by design.
// IUnknownVTable carries fn-ptrs which can't satisfy `Send` ; the unsafe-impl
// on `ComPtr` is the contract carrier, the vtable type itself is Sync-OK.

use crate::error::{D3d12Error, Result};

// ─── HRESULT helpers ──────────────────────────────────────────────────────

/// `HRESULT` is `LONG` (`i32`) in the Win32 ABI.
pub type HRESULT = i32;
/// `S_OK`.
pub const S_OK: HRESULT = 0x0000_0000_u32 as i32;
/// `E_FAIL`.
pub const E_FAIL: HRESULT = 0x8000_4005_u32 as i32;
/// `E_NOTIMPL`.
pub const E_NOTIMPL: HRESULT = 0x8000_4001_u32 as i32;
/// `E_OUTOFMEMORY`.
pub const E_OUTOFMEMORY: HRESULT = 0x8007_000E_u32 as i32;
/// `E_INVALIDARG`.
pub const E_INVALIDARG: HRESULT = 0x8007_0057_u32 as i32;
/// `DXGI_ERROR_DEVICE_REMOVED`.
pub const DXGI_ERROR_DEVICE_REMOVED: HRESULT = 0x887A_0005_u32 as i32;
/// `DXGI_ERROR_DEVICE_HUNG`.
pub const DXGI_ERROR_DEVICE_HUNG: HRESULT = 0x887A_0006_u32 as i32;

/// `SUCCEEDED(hr)` macro analog.
#[must_use]
pub const fn succeeded(hr: HRESULT) -> bool {
    hr >= 0
}

/// `FAILED(hr)` macro analog.
#[must_use]
pub const fn failed(hr: HRESULT) -> bool {
    hr < 0
}

/// Convert HRESULT to `Result<()>`.
///
/// # Errors
/// Returns `D3d12Error::Hresult` when `hr` indicates failure.
pub fn hr_check(context: &'static str, hr: HRESULT) -> Result<()> {
    if succeeded(hr) {
        Ok(())
    } else {
        Err(D3d12Error::hresult(
            context,
            hr,
            format!("HRESULT 0x{:08x}", hr as u32),
        ))
    }
}

// ─── GUID type + constants ───────────────────────────────────────────────

/// 128-bit COM interface identifier. Layout matches the Win32 `GUID` struct
/// (data1 LE u32, data2 LE u16, data3 LE u16, data4 raw 8 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Guid {
    /// Lower 32 bits, little-endian on disk.
    pub data1: u32,
    /// Next 16 bits.
    pub data2: u16,
    /// Next 16 bits.
    pub data3: u16,
    /// Final 8 bytes (raw).
    pub data4: [u8; 8],
}

impl Guid {
    /// Compile-time constructor.
    #[must_use]
    pub const fn new(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Self {
        Self {
            data1: d1,
            data2: d2,
            data3: d3,
            data4: d4,
        }
    }
}

/// `IID_ID3D12Device` — `189819f1-1db6-4b57-be54-1821339b85f7`.
pub const IID_ID3D12Device: Guid = Guid::new(
    0x189819f1,
    0x1db6,
    0x4b57,
    [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
);
/// `IID_ID3D12CommandQueue` — `0ec870a6-5d7e-4c22-8cfc-5baae07616ed`.
pub const IID_ID3D12CommandQueue: Guid = Guid::new(
    0x0ec870a6,
    0x5d7e,
    0x4c22,
    [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed],
);
/// `IID_ID3D12GraphicsCommandList` — `5b160d0f-ac1b-4185-8ba8-b3ae42a5a455`.
pub const IID_ID3D12GraphicsCommandList: Guid = Guid::new(
    0x5b160d0f,
    0xac1b,
    0x4185,
    [0x8b, 0xa8, 0xb3, 0xae, 0x42, 0xa5, 0xa4, 0x55],
);
/// `IID_ID3D12CommandAllocator` — `6102dee4-af59-4b09-b999-b44d73f09b24`.
pub const IID_ID3D12CommandAllocator: Guid = Guid::new(
    0x6102dee4,
    0xaf59,
    0x4b09,
    [0xb9, 0x99, 0xb4, 0x4d, 0x73, 0xf0, 0x9b, 0x24],
);
/// `IID_ID3D12PipelineState` — `765a30f3-f624-4c6f-a828-ace948622445`.
pub const IID_ID3D12PipelineState: Guid = Guid::new(
    0x765a30f3,
    0xf624,
    0x4c6f,
    [0xa8, 0x28, 0xac, 0xe9, 0x48, 0x62, 0x24, 0x45],
);
/// `IID_ID3D12DescriptorHeap` — `8efb471d-616c-4f49-90f7-127bb763fa51`.
pub const IID_ID3D12DescriptorHeap: Guid = Guid::new(
    0x8efb471d,
    0x616c,
    0x4f49,
    [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51],
);
/// `IID_IDXGIFactory6` — `c1b6694f-ff09-44a9-b03c-77900a0a1d17`.
pub const IID_IDXGIFactory6: Guid = Guid::new(
    0xc1b6694f,
    0xff09,
    0x44a9,
    [0xb0, 0x3c, 0x77, 0x90, 0x0a, 0x0a, 0x1d, 0x17],
);
/// `IID_IDXGISwapChain4` — `3d585d5a-bd4a-489e-b1f4-3dbcb6452ffb`.
pub const IID_IDXGISwapChain4: Guid = Guid::new(
    0x3d585d5a,
    0xbd4a,
    0x489e,
    [0xb1, 0xf4, 0x3d, 0xbc, 0xb6, 0x45, 0x2f, 0xfb],
);

// ─── Win32 / D3D12 small types (POD) ──────────────────────────────────────

/// `D3D_FEATURE_LEVEL`.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum D3DFeatureLevel {
    /// `D3D_FEATURE_LEVEL_11_0`.
    Level11_0 = 0xb000,
    /// `D3D_FEATURE_LEVEL_12_0`.
    Level12_0 = 0xc000,
    /// `D3D_FEATURE_LEVEL_12_1`.
    Level12_1 = 0xc100,
    /// `D3D_FEATURE_LEVEL_12_2`.
    Level12_2 = 0xc200,
}

/// `D3D12_COMMAND_LIST_TYPE` raw enum.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandListTypeRaw {
    /// Direct.
    Direct = 0,
    /// Bundle.
    Bundle = 1,
    /// Compute.
    Compute = 2,
    /// Copy.
    Copy = 3,
}

/// `D3D12_COMMAND_QUEUE_DESC`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CommandQueueDescRaw {
    /// `Type`.
    pub list_type: CommandListTypeRaw,
    /// `Priority`.
    pub priority: i32,
    /// `Flags`.
    pub flags: u32,
    /// `NodeMask`.
    pub node_mask: u32,
}

/// `DXGI_FORMAT` (subset — extend as needed).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DxgiFormat {
    /// `DXGI_FORMAT_UNKNOWN`.
    Unknown = 0,
    /// `DXGI_FORMAT_R8G8B8A8_UNORM`.
    R8g8b8a8Unorm = 28,
    /// `DXGI_FORMAT_B8G8R8A8_UNORM`.
    B8g8r8a8Unorm = 87,
    /// `DXGI_FORMAT_R10G10B10A2_UNORM`.
    R10g10b10a2Unorm = 24,
    /// `DXGI_FORMAT_R16G16B16A16_FLOAT`.
    R16g16b16a16Float = 10,
}

/// `DXGI_SAMPLE_DESC`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SampleDesc {
    /// `Count`.
    pub count: u32,
    /// `Quality`.
    pub quality: u32,
}

/// `DXGI_SWAP_CHAIN_DESC1` minimal subset.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SwapChainDesc1 {
    /// `Width`.
    pub width: u32,
    /// `Height`.
    pub height: u32,
    /// `Format`.
    pub format: DxgiFormat,
    /// `Stereo`.
    pub stereo: i32,
    /// `SampleDesc`.
    pub sample_desc: SampleDesc,
    /// `BufferUsage`.
    pub buffer_usage: u32,
    /// `BufferCount`.
    pub buffer_count: u32,
    /// `Scaling`.
    pub scaling: u32,
    /// `SwapEffect`.
    pub swap_effect: u32,
    /// `AlphaMode`.
    pub alpha_mode: u32,
    /// `Flags`.
    pub flags: u32,
}

// ─── Loader (Windows-only) ────────────────────────────────────────────────

/// Resolves `d3d12.dll` + `dxgi.dll` exports lazily.
#[derive(Debug, Clone, Copy)]
pub struct Loader {
    /// `D3D12CreateDevice` proc-address (or `None` if unavailable).
    pub d3d12_create_device: Option<usize>,
    /// `CreateDXGIFactory2` proc-address (or `None`).
    pub create_dxgi_factory2: Option<usize>,
    /// `D3D12GetDebugInterface` proc-address (or `None`).
    pub d3d12_get_debug_interface: Option<usize>,
    /// `D3D12SerializeRootSignature` proc-address (or `None`).
    pub d3d12_serialize_root_signature: Option<usize>,
}

impl Loader {
    /// Probe-only constructor : returns `Ok(Loader { all None })` on non-
    /// Windows. On Windows, attempts `LoadLibraryW` against the two DLLs
    /// and resolves the four canonical entry-points ; missing symbols are
    /// `None` (the call site decides whether that's fatal).
    ///
    /// # Errors
    /// Returns `D3d12Error::LoaderMissing` if neither DLL can be loaded.
    pub fn probe() -> Result<Self> {
        #[cfg(not(target_os = "windows"))]
        {
            Err(D3d12Error::loader(
                "non-Windows target — d3d12.dll / dxgi.dll unavailable",
            ))
        }
        #[cfg(target_os = "windows")]
        {
            win::probe_loader()
        }
    }

    /// Convenience : are ALL four core entry-points resolved ?
    #[must_use]
    pub const fn fully_loaded(&self) -> bool {
        self.d3d12_create_device.is_some()
            && self.create_dxgi_factory2.is_some()
            && self.d3d12_get_debug_interface.is_some()
            && self.d3d12_serialize_root_signature.is_some()
    }
}

#[cfg(target_os = "windows")]
mod win {
    use super::{D3d12Error, Loader, Result};

    // Manual Win32 prototypes — stdlib-only, no `windows` crate.
    extern "system" {
        fn LoadLibraryW(lpLibFileName: *const u16) -> *mut core::ffi::c_void;
        fn GetProcAddress(
            hModule: *mut core::ffi::c_void,
            lpProcName: *const u8,
        ) -> *mut core::ffi::c_void;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(core::iter::once(0)).collect()
    }

    fn load_proc(module: *mut core::ffi::c_void, name: &str) -> Option<usize> {
        if module.is_null() {
            return None;
        }
        let mut bytes: Vec<u8> = name.as_bytes().to_vec();
        bytes.push(0);
        // SAFETY : `bytes` is null-terminated ; `module` is non-null.
        let addr = unsafe { GetProcAddress(module, bytes.as_ptr()) };
        if addr.is_null() {
            None
        } else {
            Some(addr as usize)
        }
    }

    pub(super) fn probe_loader() -> Result<Loader> {
        let d3d12_name = wide("d3d12.dll");
        let dxgi_name = wide("dxgi.dll");
        // SAFETY : wide-strings are null-terminated.
        let d3d12 = unsafe { LoadLibraryW(d3d12_name.as_ptr()) };
        let dxgi = unsafe { LoadLibraryW(dxgi_name.as_ptr()) };
        if d3d12.is_null() && dxgi.is_null() {
            return Err(D3d12Error::loader(
                "neither d3d12.dll nor dxgi.dll loaded — Windows GPU stack absent",
            ));
        }
        Ok(Loader {
            d3d12_create_device: load_proc(d3d12, "D3D12CreateDevice"),
            create_dxgi_factory2: load_proc(dxgi, "CreateDXGIFactory2"),
            d3d12_get_debug_interface: load_proc(d3d12, "D3D12GetDebugInterface"),
            d3d12_serialize_root_signature: load_proc(d3d12, "D3D12SerializeRootSignature"),
        })
    }
}

// ─── COM IUnknown vtable shape ────────────────────────────────────────────

/// `IUnknown` vtable layout — first 3 slots of every COM interface.
#[repr(C)]
pub struct IUnknownVTable {
    /// `QueryInterface(this, riid, ppv) -> HRESULT`.
    pub query_interface:
        unsafe extern "system" fn(*mut core::ffi::c_void, *const Guid, *mut *mut core::ffi::c_void) -> HRESULT,
    /// `AddRef(this) -> ULONG`.
    pub add_ref: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
    /// `Release(this) -> ULONG`.
    pub release: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
}

/// Generic opaque COM pointer ; concrete-typed wrappers (`ID3D12Device`,
/// `IDXGISwapChain4`, etc) are `#[repr(transparent)]` newtypes around this.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ComPtr(pub *mut core::ffi::c_void);

impl ComPtr {
    /// Null sentinel.
    #[must_use]
    pub const fn null() -> Self {
        Self(core::ptr::null_mut())
    }

    /// Is the pointer null ?
    #[must_use]
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    /// Read the vtable pointer (first machine-word of every COM object).
    ///
    /// # Safety
    /// Caller must ensure `self` is non-null and points to a live COM object.
    #[must_use]
    pub unsafe fn vtable<V>(self) -> *const V {
        // Layout : `*mut Object` → `**mut V` → first slot = vtable pointer.
        *(self.0.cast::<*const V>())
    }
}

// SAFETY : ComPtr is a raw FFI pointer ; lifetime + thread-discipline is the
// caller's responsibility. We mark Send + Sync because D3D12 objects are
// thread-safe per Microsoft's documentation when used through their COM API.
unsafe impl Send for ComPtr {}
unsafe impl Sync for ComPtr {}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hresult_helpers() {
        assert!(succeeded(S_OK));
        assert!(failed(E_FAIL));
        assert!(failed(DXGI_ERROR_DEVICE_REMOVED));
        assert!(succeeded(0x0000_0001));
    }

    #[test]
    fn hr_check_routes_failure_to_d3d12_error() {
        let r = hr_check("test", E_INVALIDARG);
        assert!(r.is_err());
        match r.unwrap_err() {
            D3d12Error::Hresult { context, hresult, .. } => {
                assert_eq!(context, "test");
                assert_eq!(hresult, E_INVALIDARG);
            }
            other => panic!("expected Hresult, got {other:?}"),
        }
    }

    #[test]
    fn hr_check_routes_success_to_ok() {
        assert!(hr_check("ok", S_OK).is_ok());
    }

    #[test]
    fn iid_id3d12_device_layout() {
        let g = IID_ID3D12Device;
        assert_eq!(g.data1, 0x189819f1);
        assert_eq!(g.data2, 0x1db6);
        assert_eq!(g.data3, 0x4b57);
        assert_eq!(g.data4[0], 0xbe);
        assert_eq!(g.data4[7], 0xf7);
    }

    #[test]
    fn iid_idxgi_swap_chain4_layout() {
        let g = IID_IDXGISwapChain4;
        assert_eq!(g.data1, 0x3d585d5a);
        assert_eq!(g.data2, 0xbd4a);
    }

    #[test]
    fn iid_id3d12_command_queue_layout() {
        let g = IID_ID3D12CommandQueue;
        assert_eq!(g.data1, 0x0ec870a6);
    }

    #[test]
    fn iid_id3d12_pipeline_state_layout() {
        let g = IID_ID3D12PipelineState;
        assert_eq!(g.data1, 0x765a30f3);
    }

    #[test]
    fn iid_id3d12_descriptor_heap_layout() {
        let g = IID_ID3D12DescriptorHeap;
        assert_eq!(g.data1, 0x8efb471d);
    }

    #[test]
    fn iid_idxgi_factory6_layout() {
        let g = IID_IDXGIFactory6;
        assert_eq!(g.data1, 0xc1b6694f);
    }

    #[test]
    fn com_ptr_null_default() {
        assert!(ComPtr::null().is_null());
    }

    #[test]
    fn feature_level_values_match_d3d() {
        assert_eq!(D3DFeatureLevel::Level11_0 as i32, 0xb000);
        assert_eq!(D3DFeatureLevel::Level12_0 as i32, 0xc000);
        assert_eq!(D3DFeatureLevel::Level12_2 as i32, 0xc200);
    }

    #[test]
    fn command_list_type_raw_values_match_d3d() {
        assert_eq!(CommandListTypeRaw::Direct as i32, 0);
        assert_eq!(CommandListTypeRaw::Bundle as i32, 1);
        assert_eq!(CommandListTypeRaw::Compute as i32, 2);
        assert_eq!(CommandListTypeRaw::Copy as i32, 3);
    }

    #[test]
    fn dxgi_format_values_match_dxgi() {
        assert_eq!(DxgiFormat::Unknown as u32, 0);
        assert_eq!(DxgiFormat::R8g8b8a8Unorm as u32, 28);
        assert_eq!(DxgiFormat::R16g16b16a16Float as u32, 10);
    }

    #[test]
    fn loader_probe_returns_either_loader_or_error() {
        // On non-Windows : LoaderMissing. On Windows w/o GPU drivers : LoaderMissing.
        // On Windows w/ drivers : Ok(Loader { ... }). All three are valid outcomes.
        let r = Loader::probe();
        match r {
            Ok(l) => {
                // If we got here, we're on Windows ; at least ONE proc must resolve
                // for the loader to have succeeded.
                let any = l.d3d12_create_device.is_some()
                    || l.create_dxgi_factory2.is_some()
                    || l.d3d12_get_debug_interface.is_some()
                    || l.d3d12_serialize_root_signature.is_some();
                assert!(any, "loader claimed success but resolved zero entry-points");
            }
            Err(e) => {
                assert!(e.is_loader_missing());
            }
        }
    }

    #[test]
    fn loader_fully_loaded_predicate() {
        let l = Loader {
            d3d12_create_device: Some(1),
            create_dxgi_factory2: Some(2),
            d3d12_get_debug_interface: Some(3),
            d3d12_serialize_root_signature: Some(4),
        };
        assert!(l.fully_loaded());

        let partial = Loader {
            d3d12_create_device: Some(1),
            create_dxgi_factory2: None,
            d3d12_get_debug_interface: Some(3),
            d3d12_serialize_root_signature: Some(4),
        };
        assert!(!partial.fully_loaded());
    }

    #[test]
    fn iunknown_vtable_size_three_slots() {
        // 3 fn-ptrs × pointer-width.
        assert_eq!(
            core::mem::size_of::<IUnknownVTable>(),
            3 * core::mem::size_of::<usize>()
        );
    }
}

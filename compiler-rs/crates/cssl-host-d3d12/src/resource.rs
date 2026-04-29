//! D3D12 resource + heap allocation.
//!
//! § DESIGN
//!   - `ResourceDesc` — value type describing a `D3D12_RESOURCE_DESC` for a
//!     buffer (1D row-major). Texture descs are deferred to a later slice.
//!   - `Resource` — `ID3D12Resource` wrapper for a default heap allocation.
//!   - `UploadBuffer` — CPU-write / GPU-read staging buffer (D3D12_HEAP_TYPE_UPLOAD),
//!     persistently mapped.
//!   - `DescriptorHeap` — `ID3D12DescriptorHeap` wrapper.
//!   - `GpuBufferIso<'a>` — a borrow-form-of `Resource` carrying the
//!     `iso<gpu-buffer>` capability discipline from `specs/12_CAPABILITIES`.
//!
//! § STATE
//!   `ResourceState` mirrors `D3D12_RESOURCE_STATES` (subset). New buffers
//!   start in `Common` ; transitions are explicit via barriers.

use crate::heap::{DescriptorHeapType, HeapType};
// (Device + error types re-imported inside cfg-gated `imp` modules)

/// Resource state (subset of `D3D12_RESOURCE_STATES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceState {
    /// `D3D12_RESOURCE_STATE_COMMON`.
    Common,
    /// `D3D12_RESOURCE_STATE_GENERIC_READ`.
    GenericRead,
    /// `D3D12_RESOURCE_STATE_UNORDERED_ACCESS`.
    UnorderedAccess,
    /// `D3D12_RESOURCE_STATE_COPY_SOURCE`.
    CopySource,
    /// `D3D12_RESOURCE_STATE_COPY_DEST`.
    CopyDest,
    /// `D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE`.
    NonPixelShaderResource,
    /// `D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE`.
    PixelShaderResource,
}

impl ResourceState {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::GenericRead => "generic-read",
            Self::UnorderedAccess => "unordered-access",
            Self::CopySource => "copy-source",
            Self::CopyDest => "copy-dest",
            Self::NonPixelShaderResource => "non-pixel-shader-resource",
            Self::PixelShaderResource => "pixel-shader-resource",
        }
    }

    /// Raw `D3D12_RESOURCE_STATES` integer.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::Common => 0,
            Self::GenericRead => 0x0AC3,
            Self::UnorderedAccess => 0x0008,
            Self::CopySource => 0x0800,
            Self::CopyDest => 0x0400,
            Self::NonPixelShaderResource => 0x0040,
            Self::PixelShaderResource => 0x0080,
        }
    }
}

/// Buffer-specific resource description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceDesc {
    /// Size in bytes.
    pub size_in_bytes: u64,
    /// Required alignment (0 for D3D12 default of 64 KB).
    pub alignment: u64,
    /// Allow this buffer to be used as a UAV.
    pub allow_uav: bool,
    /// Allow this buffer to be used as cross-process / shared.
    pub allow_shared: bool,
}

impl ResourceDesc {
    /// New buffer desc with defaults (no UAV, no sharing).
    #[must_use]
    pub const fn buffer(size_in_bytes: u64) -> Self {
        Self {
            size_in_bytes,
            alignment: 0,
            allow_uav: false,
            allow_shared: false,
        }
    }

    /// Mark this buffer as UAV-capable.
    #[must_use]
    pub const fn with_uav(mut self) -> Self {
        self.allow_uav = true;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::{DescriptorHeapType, HeapType, ResourceDesc, ResourceState};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};
    use windows::Win32::Graphics::Direct3D12::{
        ID3D12DescriptorHeap, ID3D12Resource, D3D12_DESCRIPTOR_HEAP_DESC,
        D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        D3D12_DESCRIPTOR_HEAP_TYPE, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
        D3D12_DESCRIPTOR_HEAP_TYPE_DSV, D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES,
        D3D12_HEAP_TYPE, D3D12_HEAP_TYPE_CUSTOM, D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_READBACK,
        D3D12_HEAP_TYPE_UPLOAD, D3D12_MEMORY_POOL_UNKNOWN, D3D12_RESOURCE_DESC,
        D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATES, D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};

    fn heap_type_to_raw(t: HeapType) -> D3D12_HEAP_TYPE {
        match t {
            HeapType::Default => D3D12_HEAP_TYPE_DEFAULT,
            HeapType::Upload => D3D12_HEAP_TYPE_UPLOAD,
            HeapType::Readback => D3D12_HEAP_TYPE_READBACK,
            HeapType::Custom => D3D12_HEAP_TYPE_CUSTOM,
        }
    }

    fn descriptor_heap_type_to_raw(t: DescriptorHeapType) -> D3D12_DESCRIPTOR_HEAP_TYPE {
        match t {
            DescriptorHeapType::CbvSrvUav => D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            DescriptorHeapType::Sampler => D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
            DescriptorHeapType::Rtv => D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            DescriptorHeapType::Dsv => D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
        }
    }

    fn build_buffer_desc(d: &ResourceDesc) -> D3D12_RESOURCE_DESC {
        D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: d.alignment,
            Width: d.size_in_bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: if d.allow_uav {
                D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS
            } else {
                D3D12_RESOURCE_FLAG_NONE
            },
        }
    }

    /// Default-heap (GPU-local) resource.
    pub struct Resource {
        pub(crate) resource: ID3D12Resource,
        pub(crate) desc: ResourceDesc,
        pub(crate) heap_type: HeapType,
        pub(crate) state: core::cell::Cell<ResourceState>,
    }

    impl core::fmt::Debug for Resource {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Resource")
                .field("desc", &self.desc)
                .field("heap_type", &self.heap_type)
                .field("state", &self.state.get())
                .finish_non_exhaustive()
        }
    }

    impl Resource {
        /// Create a buffer in the default heap (GPU-local) at the given size.
        pub fn new_default_buffer(device: &Device, desc: ResourceDesc) -> Result<Self> {
            Self::new_committed_buffer(device, desc, HeapType::Default, ResourceState::Common)
        }

        /// Create a buffer in any heap with explicit initial state.
        pub fn new_committed_buffer(
            device: &Device,
            desc: ResourceDesc,
            heap_type: HeapType,
            initial_state: ResourceState,
        ) -> Result<Self> {
            if desc.size_in_bytes == 0 {
                return Err(D3d12Error::invalid(
                    "Resource::new_committed_buffer",
                    "size=0",
                ));
            }
            let raw_desc = build_buffer_desc(&desc);
            let props = D3D12_HEAP_PROPERTIES {
                Type: heap_type_to_raw(heap_type),
                CPUPageProperty:
                    windows::Win32::Graphics::Direct3D12::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                CreationNodeMask: 1,
                VisibleNodeMask: 1,
            };
            let mut out: Option<ID3D12Resource> = None;
            // D3D12_RESOURCE_STATES is `i32` ; we go through i32::from_ne_bytes
            // to avoid clippy::cast_possible_wrap on the u32→i32 step.
            let initial_raw =
                D3D12_RESOURCE_STATES(i32::from_ne_bytes(initial_state.as_u32().to_ne_bytes()));
            // SAFETY : FFI ; props + desc + nullable optimized clear value all valid.
            unsafe {
                device.device.CreateCommittedResource(
                    &props,
                    D3D12_HEAP_FLAG_NONE,
                    &raw_desc,
                    initial_raw,
                    None,
                    &mut out,
                )
            }
            .map_err(|e| crate::device::imp_map_hresult("CreateCommittedResource", e))?;
            let resource = out.ok_or_else(|| {
                D3d12Error::invalid("CreateCommittedResource", "returned null resource")
            })?;
            Ok(Self {
                resource,
                desc,
                heap_type,
                state: core::cell::Cell::new(initial_state),
            })
        }

        /// Get current logical state.
        #[must_use]
        pub fn state(&self) -> ResourceState {
            self.state.get()
        }

        /// Update logical state (call after issuing a barrier).
        pub fn set_state(&self, state: ResourceState) {
            self.state.set(state);
        }

        /// Resource description.
        #[must_use]
        pub const fn desc(&self) -> &ResourceDesc {
            &self.desc
        }

        /// Heap type.
        #[must_use]
        pub const fn heap_type(&self) -> HeapType {
            self.heap_type
        }

        /// Get the GPU virtual address.
        #[must_use]
        pub fn gpu_virtual_address(&self) -> u64 {
            // SAFETY : resource lives.
            unsafe { self.resource.GetGPUVirtualAddress() }
        }

        #[allow(dead_code)]
        pub(crate) fn raw(&self) -> &ID3D12Resource {
            &self.resource
        }
    }

    /// Upload buffer (CPU-write / GPU-read staging). Persistently mapped.
    pub struct UploadBuffer {
        pub(crate) resource: Resource,
        pub(crate) mapped: *mut u8,
    }

    // SAFETY : `mapped` is only used while `Self` is alive ; `Drop` unmaps it ;
    // no thread crosses this raw pointer because the wrapper is `!Sync`.
    impl core::fmt::Debug for UploadBuffer {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("UploadBuffer")
                .field("resource", &self.resource)
                .field("mapped_present", &!self.mapped.is_null())
                .finish_non_exhaustive()
        }
    }

    impl UploadBuffer {
        /// Create an upload buffer of the given size.
        pub fn new(device: &Device, size_in_bytes: u64) -> Result<Self> {
            let resource = Resource::new_committed_buffer(
                device,
                ResourceDesc::buffer(size_in_bytes),
                HeapType::Upload,
                ResourceState::GenericRead,
            )?;
            let mut mapped: *mut core::ffi::c_void = core::ptr::null_mut();
            // SAFETY : Map(0, NULL_RANGE, &out_ptr) : range NULL = full read+write.
            unsafe { resource.resource.Map(0, None, Some(&mut mapped)) }
                .map_err(|e| crate::device::imp_map_hresult("Resource::Map", e))?;
            Ok(Self {
                resource,
                mapped: mapped.cast::<u8>(),
            })
        }

        /// Write `bytes` at offset `offset` into the buffer.
        pub fn write_at(&self, offset: usize, bytes: &[u8]) -> Result<()> {
            let end = offset
                .checked_add(bytes.len())
                .ok_or_else(|| D3d12Error::invalid("UploadBuffer::write_at", "size overflow"))?;
            if end as u64 > self.resource.desc.size_in_bytes {
                return Err(D3d12Error::invalid(
                    "UploadBuffer::write_at",
                    "out of bounds",
                ));
            }
            // SAFETY : `mapped` is valid for the buffer's lifetime ; `bytes`
            // is a Rust slice ; we copy non-overlapping ; len bounds checked.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    self.mapped.add(offset),
                    bytes.len(),
                );
            }
            Ok(())
        }

        /// Get the underlying [`Resource`] for binding / barriers.
        #[must_use]
        pub const fn resource(&self) -> &Resource {
            &self.resource
        }
    }

    impl Drop for UploadBuffer {
        fn drop(&mut self) {
            if !self.mapped.is_null() {
                // SAFETY : Map was called once at construction ; Unmap must mirror it.
                unsafe { self.resource.resource.Unmap(0, None) };
            }
        }
    }

    /// Descriptor heap (CBV/SRV/UAV/Sampler/RTV/DSV).
    pub struct DescriptorHeap {
        #[allow(dead_code)]
        pub(crate) heap: ID3D12DescriptorHeap,
        pub(crate) ty: DescriptorHeapType,
        pub(crate) capacity: u32,
        pub(crate) shader_visible: bool,
    }

    impl core::fmt::Debug for DescriptorHeap {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("DescriptorHeap")
                .field("ty", &self.ty)
                .field("capacity", &self.capacity)
                .field("shader_visible", &self.shader_visible)
                .finish_non_exhaustive()
        }
    }

    impl DescriptorHeap {
        /// Create a descriptor heap.
        pub fn new(
            device: &Device,
            ty: DescriptorHeapType,
            capacity: u32,
            shader_visible: bool,
        ) -> Result<Self> {
            if capacity == 0 {
                return Err(D3d12Error::invalid("DescriptorHeap::new", "capacity=0"));
            }
            // RTV + DSV cannot be shader-visible per D3D12 spec.
            let shader_visible_eff =
                shader_visible && !matches!(ty, DescriptorHeapType::Rtv | DescriptorHeapType::Dsv);
            let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: descriptor_heap_type_to_raw(ty),
                NumDescriptors: capacity,
                Flags: if shader_visible_eff {
                    D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
                } else {
                    D3D12_DESCRIPTOR_HEAP_FLAG_NONE
                },
                NodeMask: 0,
            };
            // SAFETY : FFI ; desc valid for call duration.
            let heap: ID3D12DescriptorHeap =
                unsafe { device.device.CreateDescriptorHeap(&desc) }
                    .map_err(|e| crate::device::imp_map_hresult("CreateDescriptorHeap", e))?;
            Ok(Self {
                heap,
                ty,
                capacity,
                shader_visible: shader_visible_eff,
            })
        }

        /// Heap kind.
        #[must_use]
        pub const fn kind(&self) -> DescriptorHeapType {
            self.ty
        }

        /// Number of descriptors this heap can hold.
        #[must_use]
        pub const fn capacity(&self) -> u32 {
            self.capacity
        }

        /// Whether this heap is shader-visible.
        #[must_use]
        pub const fn is_shader_visible(&self) -> bool {
            self.shader_visible
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{DescriptorHeapType, HeapType, ResourceDesc, ResourceState};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};

    /// Resource stub.
    #[derive(Debug)]
    pub struct Resource;

    impl Resource {
        /// Always returns `LoaderMissing`.
        pub fn new_default_buffer(_device: &Device, _desc: ResourceDesc) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn new_committed_buffer(
            _device: &Device,
            _desc: ResourceDesc,
            _heap_type: HeapType,
            _initial_state: ResourceState,
        ) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub state.
        #[must_use]
        pub const fn state(&self) -> ResourceState {
            ResourceState::Common
        }

        /// No-op.
        pub fn set_state(&self, _state: ResourceState) {}

        /// Stub desc.
        #[must_use]
        pub const fn desc(&self) -> &ResourceDesc {
            &ResourceDesc {
                size_in_bytes: 0,
                alignment: 0,
                allow_uav: false,
                allow_shared: false,
            }
        }

        /// Stub heap type.
        #[must_use]
        pub const fn heap_type(&self) -> HeapType {
            HeapType::Default
        }

        /// Stub GPU VA.
        #[must_use]
        pub const fn gpu_virtual_address(&self) -> u64 {
            0
        }
    }

    /// Upload buffer stub.
    #[derive(Debug)]
    pub struct UploadBuffer;

    impl UploadBuffer {
        /// Always returns `LoaderMissing`.
        pub fn new(_device: &Device, _size_in_bytes: u64) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn write_at(&self, _offset: usize, _bytes: &[u8]) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub resource.
        #[must_use]
        pub fn resource(&self) -> &Resource {
            // Construct a const Resource ref via a private static — but we
            // can't construct Resource on non-Windows. Use an unreachable
            // pattern : caller must check `LoaderMissing` first.
            panic!("UploadBuffer::resource on non-Windows: should never be reached")
        }
    }

    /// Descriptor heap stub.
    #[derive(Debug)]
    pub struct DescriptorHeap;

    impl DescriptorHeap {
        /// Always returns `LoaderMissing`.
        pub fn new(
            _device: &Device,
            _ty: DescriptorHeapType,
            _capacity: u32,
            _shader_visible: bool,
        ) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub kind.
        #[must_use]
        pub const fn kind(&self) -> DescriptorHeapType {
            DescriptorHeapType::CbvSrvUav
        }

        /// Stub capacity.
        #[must_use]
        pub const fn capacity(&self) -> u32 {
            0
        }

        /// Stub visibility.
        #[must_use]
        pub const fn is_shader_visible(&self) -> bool {
            false
        }
    }
}

pub use imp::{DescriptorHeap, Resource, UploadBuffer};

/// Borrow form of a [`Resource`] carrying the `iso<gpu-buffer>` capability per
/// `specs/12_CAPABILITIES.csl § ISO-OWNERSHIP`.
///
/// This is a phantom type used by callers that want to encode capability
/// linearity at the Rust borrow level. The wrapper is `!Clone` and `!Copy` ;
/// passing it consumes it. Passing a `&'a GpuBufferIso<'a>` into a downstream
/// API is the equivalent of a `box<gpu-buffer>` borrow.
#[derive(Debug)]
pub struct GpuBufferIso<'a> {
    /// Borrowed underlying resource.
    pub resource: &'a Resource,
    /// Phantom marker so `'a` is exposed.
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> GpuBufferIso<'a> {
    /// Create a new `iso<gpu-buffer>` borrow over a resource.
    #[must_use]
    pub const fn new(resource: &'a Resource) -> Self {
        Self {
            resource,
            _marker: core::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ResourceDesc, ResourceState};

    #[test]
    fn resource_state_names() {
        assert_eq!(ResourceState::Common.as_str(), "common");
        assert_eq!(ResourceState::GenericRead.as_str(), "generic-read");
        assert_eq!(ResourceState::UnorderedAccess.as_str(), "unordered-access");
    }

    #[test]
    fn resource_state_integer_codes_are_distinct() {
        let states = [
            ResourceState::Common,
            ResourceState::GenericRead,
            ResourceState::UnorderedAccess,
            ResourceState::CopySource,
            ResourceState::CopyDest,
            ResourceState::PixelShaderResource,
            ResourceState::NonPixelShaderResource,
        ];
        let mut codes: Vec<u32> = states.iter().map(|s| s.as_u32()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), states.len());
    }

    #[test]
    fn resource_desc_buffer_defaults() {
        let d = ResourceDesc::buffer(4096);
        assert_eq!(d.size_in_bytes, 4096);
        assert_eq!(d.alignment, 0);
        assert!(!d.allow_uav);
        assert!(!d.allow_shared);
    }

    #[test]
    fn resource_desc_with_uav() {
        let d = ResourceDesc::buffer(4096).with_uav();
        assert!(d.allow_uav);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn upload_buffer_round_trip_or_skip() {
        use super::UploadBuffer;
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let buf = match UploadBuffer::new(&device, 256) {
            Ok(b) => b,
            Err(e) => {
                // Some headless runners lack a discrete GPU but DXGI exists.
                assert!(
                    e.is_loader_missing() || matches!(e, crate::error::D3d12Error::Hresult { .. })
                );
                return;
            }
        };
        let bytes = b"hello d3d12";
        buf.write_at(0, bytes).expect("write");
        // Out-of-bounds rejected.
        let oob = buf.write_at(250, &[0u8; 64]);
        assert!(oob.is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn default_buffer_zero_size_rejected() {
        use super::{Resource, ResourceDesc};
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let r = Resource::new_default_buffer(&device, ResourceDesc::buffer(0));
        assert!(r.is_err());
        assert!(matches!(
            r.unwrap_err(),
            crate::error::D3d12Error::InvalidArgument { .. }
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn descriptor_heap_zero_capacity_rejected() {
        use super::DescriptorHeap;
        use crate::device::{AdapterPreference, Device, Factory};
        use crate::heap::DescriptorHeapType;
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let r = DescriptorHeap::new(&device, DescriptorHeapType::CbvSrvUav, 0, false);
        assert!(r.is_err());
        assert!(matches!(
            r.unwrap_err(),
            crate::error::D3d12Error::InvalidArgument { .. }
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn descriptor_heap_rtv_cannot_be_shader_visible() {
        use super::DescriptorHeap;
        use crate::device::{AdapterPreference, Device, Factory};
        use crate::heap::DescriptorHeapType;
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        // shader_visible=true on RTV is silently downgraded ; verify not panicking.
        match DescriptorHeap::new(&device, DescriptorHeapType::Rtv, 4, true) {
            Ok(h) => assert!(!h.is_shader_visible(), "RTV heap not shader-visible"),
            Err(e) => assert!(
                e.is_loader_missing() || matches!(e, crate::error::D3d12Error::Hresult { .. })
            ),
        }
    }
}

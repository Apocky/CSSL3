//! § ffi/buffer : `VkBuffer` + `VkDeviceMemory` allocation (T11-D65, S6-E1).
//!
//! § ROLE
//!   `VkBufferHandle` is the RAII pair owning a `VkBuffer` + the
//!   `VkDeviceMemory` it's bound to. Drop unbinds + destroys + frees
//!   in the correct order.
//!
//! § CAP-FLOW (cssl-cap-system, see specs/12_CAPABILITIES.csl)
//!   A freshly allocated VkBuffer is `iso<gpu-buffer>` semantically —
//!   a unique linear capability that can be moved through the dispatch
//!   pipeline but not copied. The R18 telemetry-ring records the
//!   creation event so the audit-chain can correlate buffer-id with
//!   subsequent `vkCmdCopyBuffer` / dispatch / teardown.
//!
//! § BUFFER KINDS
//!   At stage-0 we expose : Storage (UAV), Uniform, TransferSrc,
//!   TransferDst — the minimal set the compute pipeline tests need.
//!   Later slices add Vertex / Index / IndirectArgs / etc.

#![allow(unsafe_code)]

use ash::vk;

use crate::ffi::device::LogicalDevice;
use crate::ffi::error::{AshError, VkResultDisplay};

/// Stage-0 buffer-kind taxonomy. Each maps to a `vk::BufferUsageFlags`
/// + memory-property recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferKind {
    /// Read/write SSBO. Memory : DEVICE_LOCAL preferred ; HOST_VISIBLE
    /// only for stage-0 testability.
    Storage,
    /// Uniform buffer (UBO).
    Uniform,
    /// Transfer-source staging buffer (HOST_VISIBLE).
    TransferSrc,
    /// Transfer-dest readback buffer (HOST_VISIBLE + HOST_COHERENT).
    TransferDst,
}

impl BufferKind {
    /// `VkBufferUsageFlags` requested.
    ///
    /// (Not `const fn` — ash's `BufferUsageFlags::or` is non-const.)
    #[must_use]
    pub fn usage_flags(self) -> vk::BufferUsageFlags {
        match self {
            Self::Storage => {
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST
            }
            Self::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
            Self::TransferSrc => vk::BufferUsageFlags::TRANSFER_SRC,
            Self::TransferDst => vk::BufferUsageFlags::TRANSFER_DST,
        }
    }

    /// `VkMemoryPropertyFlags` requested. Stage-0 keeps it host-visible
    /// for testability ; real graphics workflow uses DEVICE_LOCAL +
    /// staging.
    ///
    /// (Not `const fn` — ash's `MemoryPropertyFlags::or` is non-const.)
    #[must_use]
    pub fn memory_flags(self) -> vk::MemoryPropertyFlags {
        match self {
            Self::Storage | Self::TransferSrc | Self::TransferDst => {
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
            }
            Self::Uniform => {
                vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL
            }
        }
    }
}

/// RAII pair (VkBuffer + VkDeviceMemory).
///
/// The struct holds a `*const LogicalDevice` ; the `PhantomData` field
/// suppresses `Send`/`Sync` auto-impls so accidental cross-thread
/// sharing is rejected at compile time.
pub struct VkBufferHandle {
    /// Underlying buffer handle.
    buffer: vk::Buffer,
    /// Memory backing the buffer.
    memory: vk::DeviceMemory,
    /// Size in bytes (mirrors the create-info to avoid re-querying).
    size: u64,
    /// Buffer kind (carries through for diagnostics + cap-flow).
    pub kind: BufferKind,
    /// Borrowed device handle. Saved as a raw pointer because the
    /// caller's `LogicalDevice` outlives the buffer.
    device: *const LogicalDevice,
    /// Whether the handle was already destroyed (Drop guard).
    destroyed: bool,
    /// `PhantomData<*const ()>` makes the struct neither `Send` nor `Sync`,
    /// matching the cap-flow semantic that a `VkBufferHandle` is single-
    /// owner per-thread (linear `iso<gpu-buffer>`).
    _marker: std::marker::PhantomData<*const ()>,
}

impl VkBufferHandle {
    /// Create a buffer of `size` bytes with the supplied `kind`.
    /// Allocates + binds device-memory in one shot.
    ///
    /// # Errors
    /// - [`AshError::BufferCreate`] from `vkCreateBuffer`.
    /// - [`AshError::MemoryAllocate`] from `vkAllocateMemory`.
    /// - [`AshError::NoMatchingMemoryType`] when no compatible memory
    ///   type exists.
    /// - [`AshError::BindBufferMemory`] from `vkBindBufferMemory`.
    pub fn create(device: &LogicalDevice, kind: BufferKind, size: u64) -> Result<Self, AshError> {
        if size == 0 {
            return Err(AshError::BufferCreate(VkResultDisplay::from(
                vk::Result::ERROR_VALIDATION_FAILED_EXT,
            )));
        }

        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(kind.usage_flags())
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY : `device.raw()` alive ; create-info pointers all
        // outlive this call.
        let buffer = unsafe { device.raw().create_buffer(&info, None) }
            .map_err(|r| AshError::BufferCreate(VkResultDisplay::from(r)))?;

        // SAFETY : matching `vkGetBufferMemoryRequirements` for a freshly
        // created buffer is safe.
        let req = unsafe { device.raw().get_buffer_memory_requirements(buffer) };

        let mem_type = device.find_memory_type(req.memory_type_bits, kind.memory_flags())?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(mem_type);

        // SAFETY : same precondition as create_buffer.
        let memory = unsafe { device.raw().allocate_memory(&alloc_info, None) }.map_err(|r| {
            // Tear down the buffer before propagating.
            unsafe { device.raw().destroy_buffer(buffer, None) };
            AshError::MemoryAllocate(VkResultDisplay::from(r))
        })?;

        // SAFETY : binding a freshly allocated `VkDeviceMemory` to a
        // freshly created buffer with offset 0.
        unsafe { device.raw().bind_buffer_memory(buffer, memory, 0) }.map_err(|r| {
            unsafe {
                device.raw().free_memory(memory, None);
                device.raw().destroy_buffer(buffer, None);
            }
            AshError::BindBufferMemory(VkResultDisplay::from(r))
        })?;

        Ok(Self {
            buffer,
            memory,
            size: req.size,
            kind,
            device: device as *const LogicalDevice,
            destroyed: false,
            _marker: std::marker::PhantomData,
        })
    }

    /// Borrow the underlying buffer handle.
    #[must_use]
    pub const fn raw(&self) -> vk::Buffer {
        self.buffer
    }

    /// Borrow the underlying memory handle.
    #[must_use]
    pub const fn raw_memory(&self) -> vk::DeviceMemory {
        self.memory
    }

    /// Size (in bytes) of the bound memory.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Map the entire bound memory + return a writable slice. Caller
    /// must drop the returned `MemoryMap` before issuing further
    /// device-side operations on the buffer.
    ///
    /// # Errors
    /// [`AshError::MapMemory`] from `vkMapMemory`.
    pub fn map_full(&self) -> Result<MemoryMap<'_>, AshError> {
        // SAFETY : `device` raw-pointer was set on create() and is valid
        // for the lifetime of self.
        let device_ref = unsafe { &*self.device };
        let ptr = unsafe {
            device_ref
                .raw()
                .map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())
        }
        .map_err(|r| AshError::MapMemory(VkResultDisplay::from(r)))?;
        Ok(MemoryMap {
            ptr: ptr.cast::<u8>(),
            len: self.size as usize,
            buffer: self,
        })
    }
}

impl Drop for VkBufferHandle {
    fn drop(&mut self) {
        if self.destroyed {
            return;
        }
        // SAFETY : `device` raw-pointer is valid as long as the caller
        // upholds the contract that LogicalDevice outlives the buffer.
        let device_ref = unsafe { &*self.device };
        unsafe {
            device_ref.raw().destroy_buffer(self.buffer, None);
            device_ref.raw().free_memory(self.memory, None);
        }
        self.destroyed = true;
    }
}

/// RAII map-handle. Drops via `vkUnmapMemory`.
pub struct MemoryMap<'a> {
    ptr: *mut u8,
    len: usize,
    buffer: &'a VkBufferHandle,
}

impl<'a> MemoryMap<'a> {
    /// Pointer to the mapped region.
    #[must_use]
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Length of the mapped region.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True iff the mapped region has zero bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrow as a slice.
    ///
    /// # Safety
    /// Caller asserts no concurrent device-side access to the same
    /// memory while the slice is alive.
    pub unsafe fn as_slice(&self) -> &[u8] {
        // SAFETY : caller's contract.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Mutably borrow as a slice.
    ///
    /// # Safety
    /// Same contract as [`Self::as_slice`].
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY : caller's contract.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl<'a> Drop for MemoryMap<'a> {
    fn drop(&mut self) {
        // SAFETY : matched `vkMapMemory` from `map_full`.
        let device_ref = unsafe { &*self.buffer.device };
        unsafe { device_ref.raw().unmap_memory(self.buffer.memory) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_kind_storage_usage_includes_storage_bit() {
        let f = BufferKind::Storage.usage_flags();
        assert!(f.contains(vk::BufferUsageFlags::STORAGE_BUFFER));
        assert!(f.contains(vk::BufferUsageFlags::TRANSFER_SRC));
        assert!(f.contains(vk::BufferUsageFlags::TRANSFER_DST));
    }

    #[test]
    fn buffer_kind_transfer_src_excludes_storage_bit() {
        let f = BufferKind::TransferSrc.usage_flags();
        assert!(f.contains(vk::BufferUsageFlags::TRANSFER_SRC));
        assert!(!f.contains(vk::BufferUsageFlags::STORAGE_BUFFER));
    }

    #[test]
    fn buffer_kind_transfer_dst_memory_is_host_coherent() {
        let f = BufferKind::TransferDst.memory_flags();
        assert!(f.contains(vk::MemoryPropertyFlags::HOST_COHERENT));
        assert!(f.contains(vk::MemoryPropertyFlags::HOST_VISIBLE));
    }

    #[test]
    fn buffer_kind_uniform_memory_includes_device_local() {
        let f = BufferKind::Uniform.memory_flags();
        assert!(f.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL));
    }
}

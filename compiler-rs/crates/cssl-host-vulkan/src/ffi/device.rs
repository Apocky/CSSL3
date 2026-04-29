//! § ffi/device : logical `VkDevice` + queue creation (T11-D65, S6-E1).
//!
//! § ROLE
//!   `LogicalDevice` is the RAII wrapper owning the `ash::Device` +
//!   the queue handles for graphics/compute. Drop calls `vkDestroyDevice`.
//!
//! § QUEUE STRATEGY
//!   Stage-0 picks ONE queue family that supports both graphics+compute
//!   (Arc A770 / NV / AMD all expose this — see Vulkan spec §4.1
//!   "Queues") and creates a single queue from it. Multi-queue work is
//!   a later slice (separate compute-only queue for async dispatch).

#![allow(unsafe_code)]

use ash::vk;

use crate::ffi::error::{AshError, VkResultDisplay};
use crate::ffi::instance::VkInstanceHandle;
use crate::ffi::physical_device::PhysicalDevicePick;

/// RAII-wrapped logical device.
pub struct LogicalDevice {
    /// Underlying ash::Device. `Option` so Drop can `take()`.
    device: Option<ash::Device>,
    /// Queue used for both graphics + compute (single-queue stage-0).
    queue: vk::Queue,
    /// Queue-family index used.
    queue_family_index: u32,
    /// Physical-device PCI vendor for diagnostics.
    pub vendor_id: u32,
    /// Physical-device PCI id for diagnostics.
    pub device_id: u32,
    /// Resolved device name (carry-through from the physical-device pick).
    pub device_name: String,
    /// Memory-types snapshot — pulled from `vkGetPhysicalDeviceMemoryProperties`
    /// once at create-time and reused by buffer alloc.
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
}

impl LogicalDevice {
    /// Create a logical device from a [`PhysicalDevicePick`]. Requests
    /// no special device-extensions at stage-0 ; the caller can add
    /// them via `extra_extensions` for later slices (descriptor-indexing,
    /// ray-tracing, etc.).
    ///
    /// # Errors
    /// [`AshError::DeviceCreate`] propagated from `vkCreateDevice`.
    pub fn create(
        instance: &VkInstanceHandle,
        pick: &PhysicalDevicePick,
        extra_extensions: &[*const i8],
    ) -> Result<Self, AshError> {
        // Request a single queue.
        let priorities = [1.0_f32];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(pick.graphics_compute_family)
            .queue_priorities(&priorities);
        let queue_infos = [queue_info];

        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(extra_extensions);

        // SAFETY : `instance`+`pick` both alive ; `create_info`'s pointer
        // fields outlive the call.
        let device = unsafe {
            instance
                .raw()
                .create_device(pick.device.raw, &create_info, None)
        }
        .map_err(|r| AshError::DeviceCreate(VkResultDisplay::from(r)))?;

        // SAFETY : pick.graphics_compute_family came from queue_families[i].index,
        // and we requested 1 queue (index 0).
        let queue = unsafe { device.get_device_queue(pick.graphics_compute_family, 0) };

        // SAFETY : the underlying physical-device handle is valid.
        let memory_properties = unsafe {
            instance
                .raw()
                .get_physical_device_memory_properties(pick.device.raw)
        };

        Ok(Self {
            device: Some(device),
            queue,
            queue_family_index: pick.graphics_compute_family,
            vendor_id: pick.device.vendor_id,
            device_id: pick.device.device_id,
            device_name: pick.device.name.clone(),
            memory_properties,
        })
    }

    /// Borrow underlying ash::Device.
    #[must_use]
    pub fn raw(&self) -> &ash::Device {
        self.device.as_ref().expect("device present until drop")
    }

    /// Queue used for graphics + compute submissions.
    #[must_use]
    pub const fn queue(&self) -> vk::Queue {
        self.queue
    }

    /// Queue-family index.
    #[must_use]
    pub const fn queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    /// Find a memory-type index that satisfies the requested
    /// `memoryTypeBits` mask + `MemoryPropertyFlags`.
    ///
    /// # Errors
    /// [`AshError::NoMatchingMemoryType`] if nothing matches.
    pub fn find_memory_type(
        &self,
        type_bits: u32,
        flags: vk::MemoryPropertyFlags,
    ) -> Result<u32, AshError> {
        let count = self.memory_properties.memory_type_count;
        for i in 0..count {
            let mask = 1u32 << i;
            if type_bits & mask == 0 {
                continue;
            }
            let mt = self.memory_properties.memory_types[i as usize];
            if mt.property_flags.contains(flags) {
                return Ok(i);
            }
        }
        Err(AshError::NoMatchingMemoryType { type_bits, flags })
    }
}

impl Drop for LogicalDevice {
    fn drop(&mut self) {
        if let Some(device) = self.device.take() {
            // SAFETY : we matched `vkCreateDevice` with `vkDestroyDevice`.
            unsafe {
                let _ = device.device_wait_idle();
                device.destroy_device(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a stub `PhysicalDeviceMemoryProperties` with a single
    /// memory type carrying the provided flags. Used by find_memory_type
    /// unit tests that don't go through the real driver.
    fn stub_memory_props_with_one_type(flags: vk::MemoryPropertyFlags) -> LogicalDevice {
        let mut mp = vk::PhysicalDeviceMemoryProperties {
            memory_type_count: 1,
            memory_heap_count: 1,
            ..Default::default()
        };
        mp.memory_types[0] = vk::MemoryType {
            property_flags: flags,
            heap_index: 0,
        };
        // Heap-0 entry to keep heap-array shape valid.
        mp.memory_heaps[0] = vk::MemoryHeap {
            size: 1024 * 1024,
            flags: vk::MemoryHeapFlags::DEVICE_LOCAL,
        };
        LogicalDevice {
            device: None,
            queue: vk::Queue::null(),
            queue_family_index: 0,
            vendor_id: 0x8086,
            device_id: 0x56A0,
            device_name: "stub Arc A770".into(),
            memory_properties: mp,
        }
    }

    #[test]
    fn find_memory_type_returns_matching_index() {
        let d = stub_memory_props_with_one_type(
            vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE,
        );
        let i = d
            .find_memory_type(0b1, vk::MemoryPropertyFlags::DEVICE_LOCAL)
            .unwrap();
        assert_eq!(i, 0);
    }

    #[test]
    fn find_memory_type_rejects_unmatched_flags() {
        let d = stub_memory_props_with_one_type(vk::MemoryPropertyFlags::DEVICE_LOCAL);
        let err = d
            .find_memory_type(0b1, vk::MemoryPropertyFlags::HOST_VISIBLE)
            .unwrap_err();
        assert!(matches!(err, AshError::NoMatchingMemoryType { .. }));
    }

    #[test]
    fn find_memory_type_rejects_unmatched_bits() {
        let d = stub_memory_props_with_one_type(vk::MemoryPropertyFlags::DEVICE_LOCAL);
        // Type-bit 1 (only the bit-1 type was populated) ; bit-2 here is
        // unsatisfiable.
        let err = d
            .find_memory_type(0b10, vk::MemoryPropertyFlags::DEVICE_LOCAL)
            .unwrap_err();
        assert!(matches!(err, AshError::NoMatchingMemoryType { .. }));
    }

    #[test]
    fn find_memory_type_picks_first_among_compatible() {
        let mut d = stub_memory_props_with_one_type(vk::MemoryPropertyFlags::DEVICE_LOCAL);
        d.memory_properties.memory_type_count = 2;
        d.memory_properties.memory_types[1] = vk::MemoryType {
            property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL
                | vk::MemoryPropertyFlags::HOST_VISIBLE,
            heap_index: 0,
        };
        // Both types match the bits ; require HOST_VISIBLE so only
        // type-1 qualifies.
        let i = d
            .find_memory_type(0b11, vk::MemoryPropertyFlags::HOST_VISIBLE)
            .unwrap();
        assert_eq!(i, 1);
    }

    #[test]
    fn drop_with_none_device_is_noop() {
        // Ensure the stub helper can be dropped without panicking even
        // when `device` is None.
        let d = stub_memory_props_with_one_type(vk::MemoryPropertyFlags::DEVICE_LOCAL);
        drop(d); // implicit no-op.
    }
}

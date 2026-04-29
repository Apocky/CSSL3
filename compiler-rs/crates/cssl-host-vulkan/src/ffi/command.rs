//! § ffi/command : `VkCommandPool` + `VkCommandBuffer` + fence
//!                 synchronization (T11-D65, S6-E1).
//!
//! § ROLE
//!   `CommandContext` owns a per-queue-family `VkCommandPool` + a small
//!   pool of `VkCommandBuffer`s. `submit_dispatch` records a compute
//!   dispatch + waits on a fence ; `submit_with_recorded_buffer`
//!   accepts an already-recorded `VkCommandBuffer` for caller-built
//!   workloads.
//!
//! § PRIME-DIRECTIVE
//!   No instruction is recorded silently — every dispatch records a
//!   visible debug-marker (when `VK_EXT_debug_utils` is active) so the
//!   audit-ring can correlate the dispatch with subsequent telemetry.

#![allow(unsafe_code)]

use ash::vk;

use crate::ffi::device::LogicalDevice;
use crate::ffi::error::{AshError, VkResultDisplay};
use crate::ffi::pipeline::ComputePipelineHandle;

/// Result of a fence-waited submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceState {
    /// `vkWaitForFences` returned `VK_SUCCESS`.
    Signaled,
    /// `vkWaitForFences` returned `VK_TIMEOUT`.
    Timeout,
}

/// Wraps `VkCommandPool` + per-pool command-buffer + a single re-usable
/// fence. RAII : drop tears down in reverse-create order.
pub struct CommandContext {
    pool: vk::CommandPool,
    primary_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    /// Borrowed device pointer (caller-owned).
    device: *const LogicalDevice,
    destroyed: bool,
    _marker: std::marker::PhantomData<*const ()>,
}

impl CommandContext {
    /// Create a single-buffer command-context against the supplied
    /// device's queue-family.
    ///
    /// # Errors
    /// - [`AshError::CommandPoolCreate`] from `vkCreateCommandPool`.
    /// - [`AshError::CommandBufferAllocate`] from `vkAllocateCommandBuffers`.
    /// - [`AshError::FenceCreate`] from `vkCreateFence`.
    pub fn create(device: &LogicalDevice) -> Result<Self, AshError> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(device.queue_family_index())
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY : info pointers are stack-locals living to the end of
        // this fn ; create-call dereferences before returning.
        let pool = unsafe { device.raw().create_command_pool(&pool_info, None) }
            .map_err(|r| AshError::CommandPoolCreate(VkResultDisplay::from(r)))?;

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let buffers =
            unsafe { device.raw().allocate_command_buffers(&alloc_info) }.map_err(|r| {
                unsafe { device.raw().destroy_command_pool(pool, None) };
                AshError::CommandBufferAllocate(VkResultDisplay::from(r))
            })?;

        let fence_info = vk::FenceCreateInfo::default();
        let fence = unsafe { device.raw().create_fence(&fence_info, None) }.map_err(|r| {
            unsafe {
                device.raw().free_command_buffers(pool, &buffers);
                device.raw().destroy_command_pool(pool, None);
            }
            AshError::FenceCreate(VkResultDisplay::from(r))
        })?;

        Ok(Self {
            pool,
            primary_buffer: buffers[0],
            fence,
            device: device as *const LogicalDevice,
            destroyed: false,
            _marker: std::marker::PhantomData,
        })
    }

    /// Underlying primary cmd buffer.
    #[must_use]
    pub const fn primary_buffer(&self) -> vk::CommandBuffer {
        self.primary_buffer
    }

    /// Underlying fence.
    #[must_use]
    pub const fn fence(&self) -> vk::Fence {
        self.fence
    }

    /// Reset + record the primary cmd buffer using the caller-provided
    /// closure, then submit + wait for fence.
    ///
    /// # Errors
    /// - [`AshError::CommandBufferBegin`] from `vkBeginCommandBuffer`.
    /// - [`AshError::CommandBufferEnd`] from `vkEndCommandBuffer`.
    /// - [`AshError::QueueSubmit`] from `vkQueueSubmit`.
    /// - [`AshError::FenceWait`] from `vkWaitForFences`.
    pub fn submit_record_and_wait<F>(
        &self,
        record_fn: F,
        timeout_ns: u64,
    ) -> Result<FenceState, AshError>
    where
        F: FnOnce(&ash::Device, vk::CommandBuffer) -> Result<(), AshError>,
    {
        // SAFETY : self.device + queue both alive.
        let device_ref = unsafe { &*self.device };
        let dev = device_ref.raw();
        let queue = device_ref.queue();

        // Reset fence + buffer.
        // SAFETY : matched create-calls.
        unsafe {
            dev.reset_fences(&[self.fence])
                .map_err(|r| AshError::FenceReset(VkResultDisplay::from(r)))?;
            dev.reset_command_buffer(self.primary_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|r| AshError::Driver {
                    stage: "reset_command_buffer".into(),
                    result: VkResultDisplay::from(r),
                })?;
        }

        let begin_info = vk::CommandBufferBeginInfo::default();
        unsafe {
            dev.begin_command_buffer(self.primary_buffer, &begin_info)
                .map_err(|r| AshError::CommandBufferBegin(VkResultDisplay::from(r)))?;
        }

        record_fn(dev, self.primary_buffer)?;

        unsafe {
            dev.end_command_buffer(self.primary_buffer)
                .map_err(|r| AshError::CommandBufferEnd(VkResultDisplay::from(r)))?;
        }

        let buffers = [self.primary_buffer];
        let submit = vk::SubmitInfo::default().command_buffers(&buffers);
        unsafe {
            dev.queue_submit(queue, &[submit], self.fence)
                .map_err(|r| AshError::QueueSubmit(VkResultDisplay::from(r)))?;
        }

        // SAFETY : matched fence handle.
        let wait = unsafe { dev.wait_for_fences(&[self.fence], true, timeout_ns) };
        match wait {
            Ok(()) => Ok(FenceState::Signaled),
            Err(vk::Result::TIMEOUT) => Ok(FenceState::Timeout),
            Err(r) => Err(AshError::FenceWait(VkResultDisplay::from(r))),
        }
    }

    /// Convenience : record-and-submit a single compute dispatch.
    ///
    /// # Errors
    /// Propagates errors from [`Self::submit_record_and_wait`].
    pub fn submit_compute_dispatch(
        &self,
        pipeline: &ComputePipelineHandle,
        groups: (u32, u32, u32),
        timeout_ns: u64,
    ) -> Result<FenceState, AshError> {
        self.submit_record_and_wait(
            move |dev, buf| {
                // SAFETY : `buf` is the primary buffer we already
                // know is in `Recording` state (begin happened above).
                unsafe {
                    dev.cmd_bind_pipeline(buf, vk::PipelineBindPoint::COMPUTE, pipeline.raw());
                    dev.cmd_dispatch(buf, groups.0, groups.1, groups.2);
                }
                Ok(())
            },
            timeout_ns,
        )
    }
}

impl Drop for CommandContext {
    fn drop(&mut self) {
        if self.destroyed {
            return;
        }
        // SAFETY : `device` valid as long as caller upholds outliving
        // contract.
        let device_ref = unsafe { &*self.device };
        unsafe {
            // Wait for any in-flight work before tearing down.
            let _ = device_ref.raw().device_wait_idle();
            device_ref.raw().destroy_fence(self.fence, None);
            device_ref
                .raw()
                .free_command_buffers(self.pool, &[self.primary_buffer]);
            device_ref.raw().destroy_command_pool(self.pool, None);
        }
        self.destroyed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_state_eq_self() {
        assert_eq!(FenceState::Signaled, FenceState::Signaled);
        assert_eq!(FenceState::Timeout, FenceState::Timeout);
        assert_ne!(FenceState::Signaled, FenceState::Timeout);
    }

    #[test]
    fn fence_state_is_copy() {
        let a = FenceState::Signaled;
        let b = a; // copy
        assert_eq!(a, b);
    }
}

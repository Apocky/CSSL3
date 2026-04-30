//! § pure_ffi::cmd — command-buffer record + submit.
//!
//! § ROLE
//!   From-scratch FFI declarations for the command-layer Vulkan
//!   surface : `vkCreateCommandPool` + `vkAllocateCommandBuffers` +
//!   `vkBeginCommandBuffer` + `vkEndCommandBuffer` + `vkQueueSubmit` +
//!   `vkCmdBindPipeline` + `vkCmdDispatch`.
//!
//! § SCOPE
//!   Stage A surfaces the COMPUTE-dispatch + transfer cmd path. Render-
//!   pass + draw-call cmds land alongside graphics-pipeline support in
//!   a follow-up slice.

#![allow(unsafe_code)]

use super::{
    PVkAllocationCallbacks, VkCommandBuffer, VkCommandPool, VkDevice, VkFence, VkPipeline,
    VkQueue, VkSemaphore, VkStructureType, VulkanLoader, VK_NULL_HANDLE_NDISP,
};

// ───────────────────────────────────────────────────────────────────
// § Command-layer enums.
// ───────────────────────────────────────────────────────────────────

/// `VkCommandPoolCreateFlagBits` — selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkCommandPoolCreateFlag {
    /// `VK_COMMAND_POOL_CREATE_TRANSIENT_BIT`.
    Transient = 0x0000_0001,
    /// `VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT`.
    ResetCommandBuffer = 0x0000_0002,
    /// `VK_COMMAND_POOL_CREATE_PROTECTED_BIT`.
    Protected = 0x0000_0004,
}

/// `VkCommandBufferLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkCommandBufferLevel {
    /// `VK_COMMAND_BUFFER_LEVEL_PRIMARY`.
    Primary = 0,
    /// `VK_COMMAND_BUFFER_LEVEL_SECONDARY`.
    Secondary = 1,
}

/// `VkCommandBufferUsageFlagBits` — selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkCommandBufferUsageFlag {
    /// `VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT`.
    OneTimeSubmit = 0x0000_0001,
    /// `VK_COMMAND_BUFFER_USAGE_RENDER_PASS_CONTINUE_BIT`.
    RenderPassContinue = 0x0000_0002,
    /// `VK_COMMAND_BUFFER_USAGE_SIMULTANEOUS_USE_BIT`.
    SimultaneousUse = 0x0000_0004,
}

/// `VkPipelineBindPoint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VkPipelineBindPoint {
    /// `VK_PIPELINE_BIND_POINT_GRAPHICS`.
    Graphics = 0,
    /// `VK_PIPELINE_BIND_POINT_COMPUTE`.
    Compute = 1,
}

/// `VkPipelineStageFlagBits` — selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VkPipelineStageFlag {
    /// `VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT`.
    TopOfPipe = 0x0000_0001,
    /// `VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT`.
    ComputeShader = 0x0000_0800,
    /// `VK_PIPELINE_STAGE_TRANSFER_BIT`.
    Transfer = 0x0000_1000,
    /// `VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT`.
    BottomOfPipe = 0x0000_2000,
    /// `VK_PIPELINE_STAGE_ALL_GRAPHICS_BIT`.
    AllGraphics = 0x0000_8000,
    /// `VK_PIPELINE_STAGE_ALL_COMMANDS_BIT`.
    AllCommands = 0x0001_0000,
}

// ───────────────────────────────────────────────────────────────────
// § Command-layer structures.
// ───────────────────────────────────────────────────────────────────

/// `VkCommandPoolCreateInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkCommandPoolCreateInfo {
    /// Must be [`VkStructureType::CommandPoolCreateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Bitmask of [`VkCommandPoolCreateFlag`].
    pub flags: u32,
    /// Queue-family index that the pool's buffers will be submitted to.
    pub queue_family_index: u32,
}

impl Default for VkCommandPoolCreateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::CommandPoolCreateInfo,
            p_next: core::ptr::null(),
            flags: VkCommandPoolCreateFlag::ResetCommandBuffer as u32,
            queue_family_index: 0,
        }
    }
}

/// `VkCommandBufferAllocateInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkCommandBufferAllocateInfo {
    /// Must be [`VkStructureType::CommandBufferAllocateInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Source command-pool.
    pub command_pool: VkCommandPool,
    /// Primary or secondary buffer.
    pub level: VkCommandBufferLevel,
    /// Number of buffers to allocate.
    pub command_buffer_count: u32,
}

impl Default for VkCommandBufferAllocateInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::CommandBufferAllocateInfo,
            p_next: core::ptr::null(),
            command_pool: VK_NULL_HANDLE_NDISP,
            level: VkCommandBufferLevel::Primary,
            command_buffer_count: 1,
        }
    }
}

/// `VkCommandBufferBeginInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkCommandBufferBeginInfo {
    /// Must be [`VkStructureType::CommandBufferBeginInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Bitmask of [`VkCommandBufferUsageFlag`].
    pub flags: u32,
    /// Pointer to inheritance-info (only for secondary buffers).
    pub p_inheritance_info: *const core::ffi::c_void,
}

impl Default for VkCommandBufferBeginInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::CommandBufferBeginInfo,
            p_next: core::ptr::null(),
            flags: VkCommandBufferUsageFlag::OneTimeSubmit as u32,
            p_inheritance_info: core::ptr::null(),
        }
    }
}

/// `VkSubmitInfo`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VkSubmitInfo {
    /// Must be [`VkStructureType::SubmitInfo`].
    pub s_type: VkStructureType,
    /// pNext chain head.
    pub p_next: *const core::ffi::c_void,
    /// Number of wait-semaphores.
    pub wait_semaphore_count: u32,
    /// Pointer to wait-semaphore array.
    pub p_wait_semaphores: *const VkSemaphore,
    /// Pointer to wait-stage-mask array.
    pub p_wait_dst_stage_mask: *const u32,
    /// Number of command-buffers.
    pub command_buffer_count: u32,
    /// Pointer to command-buffer array.
    pub p_command_buffers: *const VkCommandBuffer,
    /// Number of signal-semaphores.
    pub signal_semaphore_count: u32,
    /// Pointer to signal-semaphore array.
    pub p_signal_semaphores: *const VkSemaphore,
}

impl Default for VkSubmitInfo {
    fn default() -> Self {
        Self {
            s_type: VkStructureType::SubmitInfo,
            p_next: core::ptr::null(),
            wait_semaphore_count: 0,
            p_wait_semaphores: core::ptr::null(),
            p_wait_dst_stage_mask: core::ptr::null(),
            command_buffer_count: 0,
            p_command_buffers: core::ptr::null(),
            signal_semaphore_count: 0,
            p_signal_semaphores: core::ptr::null(),
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// § C signature declarations.
// ───────────────────────────────────────────────────────────────────

/// `vkCreateCommandPool` C signature.
pub type PfnVkCreateCommandPool = unsafe extern "C" fn(
    device: VkDevice,
    p_create_info: *const VkCommandPoolCreateInfo,
    p_allocator: PVkAllocationCallbacks,
    p_command_pool: *mut VkCommandPool,
) -> i32;

/// `vkDestroyCommandPool` C signature.
pub type PfnVkDestroyCommandPool = unsafe extern "C" fn(
    device: VkDevice,
    command_pool: VkCommandPool,
    p_allocator: PVkAllocationCallbacks,
);

/// `vkAllocateCommandBuffers` C signature.
pub type PfnVkAllocateCommandBuffers = unsafe extern "C" fn(
    device: VkDevice,
    p_allocate_info: *const VkCommandBufferAllocateInfo,
    p_command_buffers: *mut VkCommandBuffer,
) -> i32;

/// `vkBeginCommandBuffer` C signature.
pub type PfnVkBeginCommandBuffer = unsafe extern "C" fn(
    command_buffer: VkCommandBuffer,
    p_begin_info: *const VkCommandBufferBeginInfo,
) -> i32;

/// `vkEndCommandBuffer` C signature.
pub type PfnVkEndCommandBuffer =
    unsafe extern "C" fn(command_buffer: VkCommandBuffer) -> i32;

/// `vkCmdBindPipeline` C signature.
pub type PfnVkCmdBindPipeline = unsafe extern "C" fn(
    command_buffer: VkCommandBuffer,
    pipeline_bind_point: VkPipelineBindPoint,
    pipeline: VkPipeline,
);

/// `vkCmdDispatch` C signature.
pub type PfnVkCmdDispatch = unsafe extern "C" fn(
    command_buffer: VkCommandBuffer,
    group_count_x: u32,
    group_count_y: u32,
    group_count_z: u32,
);

/// `vkQueueSubmit` C signature.
pub type PfnVkQueueSubmit = unsafe extern "C" fn(
    queue: VkQueue,
    submit_count: u32,
    p_submits: *const VkSubmitInfo,
    fence: VkFence,
) -> i32;

/// `vkQueueWaitIdle` C signature.
pub type PfnVkQueueWaitIdle = unsafe extern "C" fn(queue: VkQueue) -> i32;

// ───────────────────────────────────────────────────────────────────
// § Rust-side wrappers : record-+-submit fluent API for tests + cssl-rt
// host_gpu STUB body delegation.
// ───────────────────────────────────────────────────────────────────

/// Errors surfaced by the cmd-layer wrappers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdError {
    /// Loader returned NULL for a cmd entry-point.
    LoaderMissingSymbol(String),
    /// Stage A : real loaders not yet wired.
    StubLoaderUnsupported,
    /// `record()` was called twice in a row without an intervening `submit()`.
    DoubleRecord,
    /// `submit()` was called without any prior `record()`.
    SubmitWithoutRecord,
    /// Pipeline handle was null at bind-time.
    NullPipeline,
}

/// State of an in-progress command-buffer record session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordState {
    /// Idle — neither recording nor submitted.
    Idle,
    /// Currently recording (between `begin()` and `end()`).
    Recording,
    /// Recorded but not yet submitted.
    Recorded,
    /// Submitted ; ready for a new record session.
    Submitted,
}

/// Minimal command-buffer state-machine for unit-tests and as the
/// shape cssl-rt host_gpu STUB bodies will switch on once Stage B
/// wires real FFI dispatch.
#[derive(Debug)]
pub struct CommandRecorder {
    state: RecordState,
    bound_pipeline: VkPipeline,
    dispatch_count: usize,
}

impl Default for CommandRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRecorder {
    /// New idle recorder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RecordState::Idle,
            bound_pipeline: VK_NULL_HANDLE_NDISP,
            dispatch_count: 0,
        }
    }

    /// Current recorder state (introspection helper).
    #[must_use]
    pub fn state(&self) -> RecordState {
        self.state
    }

    /// Number of `cmd_dispatch` calls recorded so far.
    #[must_use]
    pub fn dispatch_count(&self) -> usize {
        self.dispatch_count
    }

    /// Currently-bound pipeline handle (`VK_NULL_HANDLE_NDISP` if none).
    #[must_use]
    pub fn bound_pipeline(&self) -> VkPipeline {
        self.bound_pipeline
    }

    /// Begin recording.
    ///
    /// # Errors
    /// [`CmdError::DoubleRecord`] if already in `Recording` state.
    pub fn begin(&mut self) -> Result<(), CmdError> {
        if matches!(self.state, RecordState::Recording) {
            return Err(CmdError::DoubleRecord);
        }
        self.state = RecordState::Recording;
        self.dispatch_count = 0;
        self.bound_pipeline = VK_NULL_HANDLE_NDISP;
        Ok(())
    }

    /// Record a `vkCmdBindPipeline` for the supplied compute pipeline.
    ///
    /// # Errors
    /// [`CmdError::NullPipeline`] if `pipeline == VK_NULL_HANDLE_NDISP`.
    pub fn cmd_bind_compute_pipeline(&mut self, pipeline: VkPipeline) -> Result<(), CmdError> {
        if pipeline == VK_NULL_HANDLE_NDISP {
            return Err(CmdError::NullPipeline);
        }
        self.bound_pipeline = pipeline;
        Ok(())
    }

    /// Record a `vkCmdDispatch` (compute group-count xyz).
    pub fn cmd_dispatch(&mut self, _gx: u32, _gy: u32, _gz: u32) {
        self.dispatch_count = self.dispatch_count.saturating_add(1);
    }

    /// End recording.
    pub fn end(&mut self) {
        self.state = RecordState::Recorded;
    }

    /// Submit the recorded buffer via the supplied loader.
    ///
    /// # Errors
    /// See [`CmdError`].
    pub fn submit_with_loader<L: VulkanLoader>(&mut self, loader: &L) -> Result<(), CmdError> {
        if !matches!(self.state, RecordState::Recorded) {
            return Err(CmdError::SubmitWithoutRecord);
        }
        match loader.resolve(core::ptr::null_mut(), "vkQueueSubmit") {
            None => Err(CmdError::LoaderMissingSymbol("vkQueueSubmit".to_string())),
            Some(_addr) if !loader.is_real() => {
                self.state = RecordState::Submitted;
                // Mock loader successfully recorded the resolve ; surface
                // the stub-unsupported error so callers know real FFI
                // hasn't dispatched yet.
                Err(CmdError::StubLoaderUnsupported)
            }
            Some(_addr) => {
                self.state = RecordState::Submitted;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CmdError, CommandRecorder, RecordState, VkCommandBufferAllocateInfo,
        VkCommandBufferLevel, VkCommandBufferUsageFlag, VkCommandPoolCreateFlag,
        VkCommandPoolCreateInfo, VkPipelineBindPoint, VkPipelineStageFlag, VkSubmitInfo,
    };
    use crate::pure_ffi::{MockLoader, StubLoader, VK_NULL_HANDLE_NDISP};

    #[test]
    fn command_pool_create_info_default_has_reset_flag() {
        let info = VkCommandPoolCreateInfo::default();
        assert_eq!(
            info.flags & VkCommandPoolCreateFlag::ResetCommandBuffer as u32,
            VkCommandPoolCreateFlag::ResetCommandBuffer as u32
        );
        assert_eq!(info.queue_family_index, 0);
    }

    #[test]
    fn command_buffer_allocate_info_default_is_primary_count_one() {
        let info = VkCommandBufferAllocateInfo::default();
        assert_eq!(info.level, VkCommandBufferLevel::Primary);
        assert_eq!(info.command_buffer_count, 1);
    }

    #[test]
    fn command_buffer_begin_info_default_is_one_time_submit() {
        let info = super::VkCommandBufferBeginInfo::default();
        assert_eq!(
            info.flags & VkCommandBufferUsageFlag::OneTimeSubmit as u32,
            VkCommandBufferUsageFlag::OneTimeSubmit as u32
        );
    }

    #[test]
    fn pipeline_bind_point_compute_is_one() {
        assert_eq!(VkPipelineBindPoint::Compute as i32, 1);
    }

    #[test]
    fn pipeline_stage_flag_compute_shader_is_0x800() {
        assert_eq!(VkPipelineStageFlag::ComputeShader as u32, 0x0000_0800);
    }

    #[test]
    fn submit_info_default_is_zero_buffers() {
        let info = VkSubmitInfo::default();
        assert_eq!(info.command_buffer_count, 0);
        assert!(info.p_command_buffers.is_null());
    }

    #[test]
    fn recorder_state_machine_record_and_submit() {
        let mut r = CommandRecorder::new();
        assert_eq!(r.state(), RecordState::Idle);
        r.begin().expect("begin");
        assert_eq!(r.state(), RecordState::Recording);
        // Bind a non-null pipeline (synthetic).
        r.cmd_bind_compute_pipeline(0xCAFE).expect("bind");
        assert_eq!(r.bound_pipeline(), 0xCAFE);
        r.cmd_dispatch(64, 1, 1);
        r.cmd_dispatch(128, 1, 1);
        assert_eq!(r.dispatch_count(), 2);
        r.end();
        assert_eq!(r.state(), RecordState::Recorded);

        // Submit via mock loader : surfaces stub-unsupported but state advances.
        let l = MockLoader::new();
        let res = r.submit_with_loader(&l);
        assert!(matches!(res, Err(CmdError::StubLoaderUnsupported)));
        assert_eq!(r.state(), RecordState::Submitted);
        assert_eq!(l.resolve_count(), 1);
        assert_eq!(l.resolved_names()[0], "vkQueueSubmit");
    }

    #[test]
    fn recorder_double_begin_errors() {
        let mut r = CommandRecorder::new();
        r.begin().expect("first begin");
        let r2 = r.begin();
        assert!(matches!(r2, Err(CmdError::DoubleRecord)));
    }

    #[test]
    fn recorder_submit_without_record_errors() {
        let mut r = CommandRecorder::new();
        let l = MockLoader::new();
        let res = r.submit_with_loader(&l);
        assert!(matches!(res, Err(CmdError::SubmitWithoutRecord)));
        // Loader was not invoked because state-machine refused.
        assert_eq!(l.resolve_count(), 0);
    }

    #[test]
    fn recorder_bind_null_pipeline_errors() {
        let mut r = CommandRecorder::new();
        r.begin().expect("begin");
        let res = r.cmd_bind_compute_pipeline(VK_NULL_HANDLE_NDISP);
        assert!(matches!(res, Err(CmdError::NullPipeline)));
    }

    #[test]
    fn recorder_submit_with_stub_loader_missing_symbol() {
        let mut r = CommandRecorder::new();
        r.begin().expect("begin");
        r.cmd_dispatch(1, 1, 1);
        r.end();
        let l = StubLoader;
        let res = r.submit_with_loader(&l);
        assert!(matches!(res, Err(CmdError::LoaderMissingSymbol(ref n)) if n == "vkQueueSubmit"));
    }
}

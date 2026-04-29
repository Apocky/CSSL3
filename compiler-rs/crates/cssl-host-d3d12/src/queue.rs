//! D3D12 command queue + allocator + list wrappers.
//!
//! § DESIGN
//!   - `CommandQueue` wraps `ID3D12CommandQueue` ; tied to a [`CommandListType`].
//!   - `CommandAllocator` wraps `ID3D12CommandAllocator` ; reset between submits.
//!   - `CommandList` wraps `ID3D12GraphicsCommandList6` (graphics + compute) or
//!     `ID3D12CommandList` (copy / video). Created in the recording state and
//!     transitions through `Close` -> queue submission.
//!
//! § PRIORITY
//!   `CommandQueuePriority` mirrors `D3D12_COMMAND_QUEUE_PRIORITY_*` :
//!     - `Normal` — default ; used by most game frames.
//!     - `High`   — preempts normal ; for low-latency frames.
//!     - `GlobalRealtime` — system-wide priority ; requires admin.
//!
//! § NON-WINDOWS
//!   Every constructor returns `D3d12Error::LoaderMissing`.

use crate::heap::CommandListType;
// (Device + error types re-imported inside cfg-gated `imp` modules)

/// Command queue priority (mirrors `D3D12_COMMAND_QUEUE_PRIORITY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandQueuePriority {
    /// `D3D12_COMMAND_QUEUE_PRIORITY_NORMAL`.
    Normal,
    /// `D3D12_COMMAND_QUEUE_PRIORITY_HIGH`.
    High,
    /// `D3D12_COMMAND_QUEUE_PRIORITY_GLOBAL_REALTIME`.
    GlobalRealtime,
}

impl CommandQueuePriority {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::High => "high",
            Self::GlobalRealtime => "global-realtime",
        }
    }

    /// Raw `D3D12_COMMAND_QUEUE_PRIORITY` integer.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::High => 100,
            Self::GlobalRealtime => 10_000,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::{CommandListType, CommandQueuePriority};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D12::{
        ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12GraphicsCommandList,
        ID3D12PipelineState, D3D12_COMMAND_LIST_TYPE_BUNDLE, D3D12_COMMAND_LIST_TYPE_COMPUTE,
        D3D12_COMMAND_LIST_TYPE_COPY, D3D12_COMMAND_LIST_TYPE_DIRECT,
        D3D12_COMMAND_LIST_TYPE_VIDEO_DECODE, D3D12_COMMAND_LIST_TYPE_VIDEO_ENCODE,
        D3D12_COMMAND_LIST_TYPE_VIDEO_PROCESS, D3D12_COMMAND_QUEUE_DESC,
        D3D12_COMMAND_QUEUE_FLAG_NONE,
    };

    fn list_type_to_raw(
        t: CommandListType,
    ) -> windows::Win32::Graphics::Direct3D12::D3D12_COMMAND_LIST_TYPE {
        match t {
            CommandListType::Direct => D3D12_COMMAND_LIST_TYPE_DIRECT,
            CommandListType::Compute => D3D12_COMMAND_LIST_TYPE_COMPUTE,
            CommandListType::Copy => D3D12_COMMAND_LIST_TYPE_COPY,
            CommandListType::Bundle => D3D12_COMMAND_LIST_TYPE_BUNDLE,
            CommandListType::VideoDecode => D3D12_COMMAND_LIST_TYPE_VIDEO_DECODE,
            CommandListType::VideoProcess => D3D12_COMMAND_LIST_TYPE_VIDEO_PROCESS,
            CommandListType::VideoEncode => D3D12_COMMAND_LIST_TYPE_VIDEO_ENCODE,
        }
    }

    /// Command queue.
    pub struct CommandQueue {
        pub(crate) queue: ID3D12CommandQueue,
        pub(crate) list_type: CommandListType,
        pub(crate) priority: CommandQueuePriority,
    }

    impl core::fmt::Debug for CommandQueue {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("CommandQueue")
                .field("list_type", &self.list_type)
                .field("priority", &self.priority)
                .finish_non_exhaustive()
        }
    }

    impl CommandQueue {
        /// Create a new command queue on the device.
        pub fn new(
            device: &Device,
            list_type: CommandListType,
            priority: CommandQueuePriority,
        ) -> Result<Self> {
            let desc = D3D12_COMMAND_QUEUE_DESC {
                Type: list_type_to_raw(list_type),
                Priority: priority.as_i32(),
                Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
                NodeMask: 0,
            };
            // SAFETY : `CreateCommandQueue` is FFI ; desc + GUID are valid for
            // the call duration ; the returned interface is owned (RAII).
            let queue: ID3D12CommandQueue = unsafe { device.device.CreateCommandQueue(&desc) }
                .map_err(|e| crate::device::imp_map_hresult("CreateCommandQueue", e))?;
            Ok(Self {
                queue,
                list_type,
                priority,
            })
        }

        /// Submit a list of command lists.
        pub fn submit(&self, lists: &[&CommandList]) -> Result<()> {
            // ID3D12CommandQueue::ExecuteCommandLists takes an array of ID3D12CommandList*.
            let raw_lists: Vec<Option<ID3D12CommandList>> = lists
                .iter()
                .map(|cl| Some(cl.list.cast::<ID3D12CommandList>().unwrap()))
                .collect();
            // SAFETY : the slice points to refs that outlive the call.
            unsafe {
                self.queue.ExecuteCommandLists(&raw_lists);
            }
            let _ = lists; // borrow held for the duration above
            Ok(())
        }

        /// Signal a fence on this queue (delegates to `Fence::signal_on_queue`).
        pub fn signal(&self, fence: &crate::fence::Fence, value: u64) -> Result<()> {
            // SAFETY : fence + queue both live, `Signal` is documented stable.
            unsafe { self.queue.Signal(&fence.fence, value) }
                .map_err(|e| crate::device::imp_map_hresult("CommandQueue::Signal", e))
        }

        /// What kind of command list does this queue accept?
        #[must_use]
        pub const fn list_type(&self) -> CommandListType {
            self.list_type
        }

        /// What priority is this queue at?
        #[must_use]
        pub const fn priority(&self) -> CommandQueuePriority {
            self.priority
        }
    }

    /// Command allocator.
    pub struct CommandAllocator {
        pub(crate) allocator: ID3D12CommandAllocator,
        pub(crate) list_type: CommandListType,
    }

    impl core::fmt::Debug for CommandAllocator {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("CommandAllocator")
                .field("list_type", &self.list_type)
                .finish_non_exhaustive()
        }
    }

    impl CommandAllocator {
        /// Create a command allocator.
        pub fn new(device: &Device, list_type: CommandListType) -> Result<Self> {
            // SAFETY : FFI ; type tag valid ; result owned.
            let allocator: ID3D12CommandAllocator = unsafe {
                device
                    .device
                    .CreateCommandAllocator(list_type_to_raw(list_type))
            }
            .map_err(|e| crate::device::imp_map_hresult("CreateCommandAllocator", e))?;
            Ok(Self {
                allocator,
                list_type,
            })
        }

        /// Reset the allocator. Must only be called when the allocator is
        /// finished being read by the GPU (synchronize via fence).
        pub fn reset(&self) -> Result<()> {
            // SAFETY : alloc lives ; Reset is documented re-entrant-safe.
            unsafe { self.allocator.Reset() }
                .map_err(|e| crate::device::imp_map_hresult("CommandAllocator::Reset", e))
        }

        /// What kind of command list does this allocator vend?
        #[must_use]
        pub const fn list_type(&self) -> CommandListType {
            self.list_type
        }
    }

    /// Command list.
    pub struct CommandList {
        pub(crate) list: ID3D12GraphicsCommandList,
        pub(crate) list_type: CommandListType,
        pub(crate) closed: core::cell::Cell<bool>,
    }

    impl core::fmt::Debug for CommandList {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("CommandList")
                .field("list_type", &self.list_type)
                .field("closed", &self.closed.get())
                .finish_non_exhaustive()
        }
    }

    impl CommandList {
        /// Create a new command list bound to an allocator + initial PSO (or
        /// `None` for default).
        pub fn new(
            device: &Device,
            allocator: &CommandAllocator,
            initial_pso: Option<&crate::pso::PipelineState>,
        ) -> Result<Self> {
            if allocator.list_type != CommandListType::Direct
                && allocator.list_type != CommandListType::Compute
                && allocator.list_type != CommandListType::Copy
                && allocator.list_type != CommandListType::Bundle
            {
                return Err(D3d12Error::invalid(
                    "CommandList::new",
                    "video lists not supported in this wrapper",
                ));
            }
            let raw_pso: Option<&ID3D12PipelineState> = initial_pso.and_then(|p| p.imp_pso());
            // SAFETY : FFI ; node-mask 0 = default ; allocator + pso live.
            let list: ID3D12GraphicsCommandList = unsafe {
                device.device.CreateCommandList(
                    0,
                    list_type_to_raw(allocator.list_type),
                    &allocator.allocator,
                    raw_pso,
                )
            }
            .map_err(|e| crate::device::imp_map_hresult("CreateCommandList", e))?;
            Ok(Self {
                list,
                list_type: allocator.list_type,
                closed: core::cell::Cell::new(false),
            })
        }

        /// Close the list (transition to executable state).
        pub fn close(&self) -> Result<()> {
            // SAFETY : list lives ; Close is the documented terminal call.
            unsafe { self.list.Close() }
                .map_err(|e| crate::device::imp_map_hresult("CommandList::Close", e))?;
            self.closed.set(true);
            Ok(())
        }

        /// Reset the list to recording state on the given allocator + initial PSO.
        pub fn reset(
            &self,
            allocator: &CommandAllocator,
            initial_pso: Option<&crate::pso::PipelineState>,
        ) -> Result<()> {
            let raw_pso: Option<&ID3D12PipelineState> = initial_pso.and_then(|p| p.imp_pso());
            // SAFETY : FFI ; allocator + pso live.
            unsafe { self.list.Reset(&allocator.allocator, raw_pso) }
                .map_err(|e| crate::device::imp_map_hresult("CommandList::Reset", e))?;
            self.closed.set(false);
            Ok(())
        }

        /// Set the active compute pipeline state.
        pub fn set_compute_pipeline_state(&self, pso: &crate::pso::PipelineState) -> Result<()> {
            let raw = pso
                .imp_pso()
                .ok_or_else(|| D3d12Error::invalid("set_compute_pipeline_state", "PSO unwired"))?;
            // SAFETY : FFI ; PSO + list live.
            unsafe { self.list.SetPipelineState(raw) };
            Ok(())
        }

        /// Set the active root signature for compute.
        pub fn set_compute_root_signature(
            &self,
            rs: &crate::root_signature::RootSignature,
        ) -> Result<()> {
            let raw = rs
                .imp_signature()
                .ok_or_else(|| D3d12Error::invalid("set_compute_root_signature", "rs unwired"))?;
            // SAFETY : FFI ; rs + list live.
            unsafe { self.list.SetComputeRootSignature(raw) };
            Ok(())
        }

        /// Dispatch a compute kernel `(x, y, z)` thread-groups.
        pub fn dispatch(&self, x: u32, y: u32, z: u32) -> Result<()> {
            if self.list_type != CommandListType::Direct
                && self.list_type != CommandListType::Compute
            {
                return Err(D3d12Error::invalid(
                    "CommandList::dispatch",
                    "list not direct/compute",
                ));
            }
            // SAFETY : FFI.
            unsafe { self.list.Dispatch(x, y, z) };
            Ok(())
        }

        /// Has this list been closed?
        #[must_use]
        pub fn is_closed(&self) -> bool {
            self.closed.get()
        }

        /// What kind of command list is this?
        #[must_use]
        pub const fn list_type(&self) -> CommandListType {
            self.list_type
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{CommandListType, CommandQueuePriority};
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};

    /// Command queue stub.
    #[derive(Debug)]
    pub struct CommandQueue;

    impl CommandQueue {
        /// Always returns `LoaderMissing`.
        pub fn new(
            _device: &Device,
            _list_type: CommandListType,
            _priority: CommandQueuePriority,
        ) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn submit(&self, _lists: &[&CommandList]) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn signal(&self, _fence: &crate::fence::Fence, _value: u64) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub list type (always direct).
        #[must_use]
        pub const fn list_type(&self) -> CommandListType {
            CommandListType::Direct
        }

        /// Stub priority.
        #[must_use]
        pub const fn priority(&self) -> CommandQueuePriority {
            CommandQueuePriority::Normal
        }
    }

    /// Command allocator stub.
    #[derive(Debug)]
    pub struct CommandAllocator;

    impl CommandAllocator {
        /// Always returns `LoaderMissing`.
        pub fn new(_device: &Device, _list_type: CommandListType) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn reset(&self) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub list type.
        #[must_use]
        pub const fn list_type(&self) -> CommandListType {
            CommandListType::Direct
        }
    }

    /// Command list stub.
    #[derive(Debug)]
    pub struct CommandList;

    impl CommandList {
        /// Always returns `LoaderMissing`.
        pub fn new(
            _device: &Device,
            _allocator: &CommandAllocator,
            _initial_pso: Option<&crate::pso::PipelineState>,
        ) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn close(&self) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn reset(
            &self,
            _allocator: &CommandAllocator,
            _initial_pso: Option<&crate::pso::PipelineState>,
        ) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn set_compute_pipeline_state(&self, _pso: &crate::pso::PipelineState) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn set_compute_root_signature(
            &self,
            _rs: &crate::root_signature::RootSignature,
        ) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn dispatch(&self, _x: u32, _y: u32, _z: u32) -> Result<()> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub closed flag.
        #[must_use]
        pub const fn is_closed(&self) -> bool {
            false
        }

        /// Stub list type.
        #[must_use]
        pub const fn list_type(&self) -> CommandListType {
            CommandListType::Direct
        }
    }
}

pub use imp::{CommandAllocator, CommandList, CommandQueue};

#[cfg(test)]
mod tests {
    use super::CommandQueuePriority;

    #[test]
    fn priority_names_match_spec() {
        assert_eq!(CommandQueuePriority::Normal.as_str(), "normal");
        assert_eq!(CommandQueuePriority::High.as_str(), "high");
        assert_eq!(
            CommandQueuePriority::GlobalRealtime.as_str(),
            "global-realtime"
        );
    }

    #[test]
    fn priority_integer_ordering_matches_d3d12() {
        // Per D3D12_COMMAND_QUEUE_PRIORITY enum : NORMAL=0, HIGH=100, GLOBAL_REALTIME=10000.
        assert_eq!(CommandQueuePriority::Normal.as_i32(), 0);
        assert_eq!(CommandQueuePriority::High.as_i32(), 100);
        assert_eq!(CommandQueuePriority::GlobalRealtime.as_i32(), 10_000);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn factory_construction_returns_loader_missing() {
        // On non-Windows, every constructor returns LoaderMissing.
        let r = crate::device::Factory::new();
        assert!(r.is_err());
        assert!(r.unwrap_err().is_loader_missing());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn command_queue_priority_round_trip() {
        use super::CommandQueuePriority;
        for p in [
            CommandQueuePriority::Normal,
            CommandQueuePriority::High,
            CommandQueuePriority::GlobalRealtime,
        ] {
            assert_eq!(p, p);
            assert!(!p.as_str().is_empty());
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn command_queue_creation_or_skip() {
        // Real-hardware test — skips with LoaderMissing on CI runners without GPU.
        use super::{CommandListType, CommandQueue, CommandQueuePriority};
        use crate::device::{AdapterPreference, Device, Factory};
        let factory = match Factory::new() {
            Ok(f) => f,
            Err(e) => {
                assert!(
                    e.is_loader_missing() || matches!(e, crate::error::D3d12Error::Hresult { .. })
                );
                return;
            }
        };
        let device = match Device::new(&factory, AdapterPreference::Hardware) {
            Ok(d) => d,
            Err(e) => {
                assert!(
                    e.is_loader_missing()
                        || matches!(e, crate::error::D3d12Error::AdapterNotFound { .. })
                        || matches!(e, crate::error::D3d12Error::Hresult { .. })
                        || matches!(e, crate::error::D3d12Error::NotSupported { .. })
                );
                return;
            }
        };
        let queue = CommandQueue::new(
            &device,
            CommandListType::Direct,
            CommandQueuePriority::Normal,
        )
        .expect("queue creation should succeed on real hardware");
        assert_eq!(queue.list_type(), CommandListType::Direct);
        assert_eq!(queue.priority(), CommandQueuePriority::Normal);
    }
}

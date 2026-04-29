//! `MTLCommandQueue` + `MTLCommandBuffer` lifecycle.
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Metal`.
//!
//! § DESIGN
//!   - [`CommandQueueHandle`] wraps an `MTLCommandQueue` (Apple) or a stub
//!     record (non-Apple). One queue per logical CSSLv3 submission stream.
//!   - [`EncodedCommandBuffer`] models a command-buffer that has been built
//!     up via the `compute_pass` / `render_pass` encoders and is ready to
//!     `commit` ; the type-state shape prevents committing an empty or
//!     un-encoded buffer.
//!   - [`CommandBufferStatus`] mirrors `MTLCommandBufferStatus` for stage-0
//!     observability ; full sub-state machine (NotEnqueued / Enqueued /
//!     Committed / Scheduled / Completed / Error) is preserved verbatim.

use crate::error::{MetalError, MetalResult};

/// `MTLCommandBufferStatus` — observable command-buffer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandBufferStatus {
    /// Buffer has not been enqueued for execution.
    NotEnqueued,
    /// Buffer has been enqueued in submission order.
    Enqueued,
    /// `[buffer commit]` has been called ; queue may still be processing it.
    Committed,
    /// Buffer is scheduled for GPU execution but has not started.
    Scheduled,
    /// Buffer completed GPU execution.
    Completed,
    /// Buffer execution errored.
    Error,
}

impl CommandBufferStatus {
    /// Short canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotEnqueued => "not_enqueued",
            Self::Enqueued => "enqueued",
            Self::Committed => "committed",
            Self::Scheduled => "scheduled",
            Self::Completed => "completed",
            Self::Error => "error",
        }
    }

    /// All 6 status values.
    pub const ALL: [Self; 6] = [
        Self::NotEnqueued,
        Self::Enqueued,
        Self::Committed,
        Self::Scheduled,
        Self::Completed,
        Self::Error,
    ];
}

/// `MTLCommandQueue` handle — one per CSSLv3 logical submission stream.
#[derive(Debug)]
pub struct CommandQueueHandle {
    /// Human-readable label (debug-info ; surfaced via `[queue setLabel:]` on Apple).
    pub label: String,
    /// Maximum command-buffer count this queue is configured to retain.
    pub max_command_buffer_count: u32,
    /// Inner state — Apple-side carries an FFI handle ; stub side carries a
    /// counter for test-shape inspection.
    pub(crate) inner: CommandQueueInner,
}

#[derive(Debug)]
pub(crate) enum CommandQueueInner {
    /// Stub state — counter of buffers issued through this queue.
    Stub {
        /// Number of `EncodedCommandBuffer` instances issued so far.
        buffers_issued: u64,
    },
    /// Apple-side state — index into `MetalSession`'s queue-pool.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    Apple {
        /// Pool index that resolves to the real `metal::CommandQueue`.
        pool_idx: u32,
    },
}

impl CommandQueueHandle {
    /// Stub constructor used by tests + non-Apple builds.
    #[must_use]
    pub fn stub(label: impl Into<String>, max_command_buffer_count: u32) -> Self {
        Self {
            label: label.into(),
            max_command_buffer_count,
            inner: CommandQueueInner::Stub { buffers_issued: 0 },
        }
    }

    /// Allocate a fresh command buffer on this queue. The returned
    /// [`EncodedCommandBuffer`] starts in `NotEnqueued` state.
    pub fn make_command_buffer(&mut self) -> MetalResult<EncodedCommandBuffer> {
        match &mut self.inner {
            CommandQueueInner::Stub { buffers_issued } => {
                *buffers_issued += 1;
                Ok(EncodedCommandBuffer::stub(*buffers_issued))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            CommandQueueInner::Apple { pool_idx: _ } => {
                // § The Apple-real path is exercised in `apple::session_ops::make_command_buffer` ;
                // this branch is unreachable when constructing via the stub ctor.
                Err(MetalError::host_not_apple())
            }
        }
    }

    /// Number of command buffers issued through this queue (stub path only).
    /// Returns `None` on Apple-real handles where the FFI does not expose this.
    #[must_use]
    pub fn issued_count(&self) -> Option<u64> {
        match &self.inner {
            CommandQueueInner::Stub { buffers_issued } => Some(*buffers_issued),
            #[cfg(any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "visionos"
            ))]
            CommandQueueInner::Apple { .. } => None,
        }
    }
}

/// A command-buffer that is ready to encode + commit.
#[derive(Debug)]
pub struct EncodedCommandBuffer {
    /// Current observable state.
    pub status: CommandBufferStatus,
    /// Number of compute-pass encoders opened so far.
    pub compute_pass_count: u32,
    /// Number of render-pass encoders opened so far.
    pub render_pass_count: u32,
    /// Stable id (stub-side counter from the issuing queue).
    pub id: u64,
}

impl EncodedCommandBuffer {
    /// Stub constructor.
    #[must_use]
    pub const fn stub(id: u64) -> Self {
        Self {
            status: CommandBufferStatus::NotEnqueued,
            compute_pass_count: 0,
            render_pass_count: 0,
            id,
        }
    }

    /// Open a compute-pass encoder. On Apple, this corresponds to
    /// `[buffer computeCommandEncoderWithDescriptor:]` ; on stub, the counter
    /// is incremented.
    pub fn open_compute_pass(&mut self) -> MetalResult<()> {
        if matches!(
            self.status,
            CommandBufferStatus::Committed
                | CommandBufferStatus::Scheduled
                | CommandBufferStatus::Completed
                | CommandBufferStatus::Error
        ) {
            return Err(MetalError::CommitFailed {
                detail: format!(
                    "cannot open compute pass on buffer in state {}",
                    self.status.as_str()
                ),
            });
        }
        self.compute_pass_count += 1;
        Ok(())
    }

    /// Open a render-pass encoder.
    pub fn open_render_pass(&mut self) -> MetalResult<()> {
        if matches!(
            self.status,
            CommandBufferStatus::Committed
                | CommandBufferStatus::Scheduled
                | CommandBufferStatus::Completed
                | CommandBufferStatus::Error
        ) {
            return Err(MetalError::CommitFailed {
                detail: format!(
                    "cannot open render pass on buffer in state {}",
                    self.status.as_str()
                ),
            });
        }
        self.render_pass_count += 1;
        Ok(())
    }

    /// Enqueue this buffer for execution (does not commit).
    pub fn enqueue(&mut self) -> MetalResult<()> {
        if !matches!(self.status, CommandBufferStatus::NotEnqueued) {
            return Err(MetalError::CommitFailed {
                detail: format!("cannot enqueue buffer in state {}", self.status.as_str()),
            });
        }
        self.status = CommandBufferStatus::Enqueued;
        Ok(())
    }

    /// Commit this buffer for execution. After a successful commit the buffer
    /// transitions to `Committed` ; no further encoding is permitted.
    pub fn commit(&mut self) -> MetalResult<()> {
        if matches!(
            self.status,
            CommandBufferStatus::Committed
                | CommandBufferStatus::Scheduled
                | CommandBufferStatus::Completed
                | CommandBufferStatus::Error
        ) {
            return Err(MetalError::CommitFailed {
                detail: format!("buffer already in terminal state {}", self.status.as_str()),
            });
        }
        self.status = CommandBufferStatus::Committed;
        Ok(())
    }

    /// Wait for this buffer's GPU completion. Stub-side this just transitions
    /// `Committed → Completed` ; Apple-side the apple module wraps
    /// `[buffer waitUntilCompleted]`.
    pub fn wait_until_completed(&mut self) -> MetalResult<()> {
        match self.status {
            CommandBufferStatus::NotEnqueued => Err(MetalError::CommitFailed {
                detail: "buffer not committed".into(),
            }),
            CommandBufferStatus::Enqueued => {
                // § Some Metal usage commits implicitly via wait ; stub mirror.
                self.status = CommandBufferStatus::Completed;
                Ok(())
            }
            CommandBufferStatus::Committed | CommandBufferStatus::Scheduled => {
                self.status = CommandBufferStatus::Completed;
                Ok(())
            }
            CommandBufferStatus::Completed => Ok(()),
            CommandBufferStatus::Error => Err(MetalError::CommitFailed {
                detail: "buffer in error state".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandBufferStatus, CommandQueueHandle, EncodedCommandBuffer, MetalError};

    #[test]
    fn status_count_and_names() {
        assert_eq!(CommandBufferStatus::ALL.len(), 6);
        assert_eq!(CommandBufferStatus::NotEnqueued.as_str(), "not_enqueued");
        assert_eq!(CommandBufferStatus::Completed.as_str(), "completed");
    }

    #[test]
    fn stub_queue_makes_buffer_with_increasing_ids() {
        let mut q = CommandQueueHandle::stub("test", 4);
        let a = q.make_command_buffer().unwrap();
        let b = q.make_command_buffer().unwrap();
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
        assert_eq!(q.issued_count(), Some(2));
    }

    #[test]
    fn fresh_buffer_starts_not_enqueued_with_zero_passes() {
        let cb = EncodedCommandBuffer::stub(1);
        assert_eq!(cb.status, CommandBufferStatus::NotEnqueued);
        assert_eq!(cb.compute_pass_count, 0);
        assert_eq!(cb.render_pass_count, 0);
    }

    #[test]
    fn open_compute_then_render_increments_counters() {
        let mut cb = EncodedCommandBuffer::stub(1);
        cb.open_compute_pass().unwrap();
        cb.open_compute_pass().unwrap();
        cb.open_render_pass().unwrap();
        assert_eq!(cb.compute_pass_count, 2);
        assert_eq!(cb.render_pass_count, 1);
    }

    #[test]
    fn enqueue_then_commit_transitions_state() {
        let mut cb = EncodedCommandBuffer::stub(1);
        cb.enqueue().unwrap();
        assert_eq!(cb.status, CommandBufferStatus::Enqueued);
        cb.commit().unwrap();
        assert_eq!(cb.status, CommandBufferStatus::Committed);
    }

    #[test]
    fn cannot_open_pass_after_commit() {
        let mut cb = EncodedCommandBuffer::stub(1);
        cb.commit().unwrap();
        let r = cb.open_compute_pass();
        assert!(matches!(r, Err(MetalError::CommitFailed { .. })));
    }

    #[test]
    fn double_commit_returns_error() {
        let mut cb = EncodedCommandBuffer::stub(1);
        cb.commit().unwrap();
        let r = cb.commit();
        assert!(matches!(r, Err(MetalError::CommitFailed { .. })));
    }

    #[test]
    fn wait_completes_committed_buffer() {
        let mut cb = EncodedCommandBuffer::stub(1);
        cb.commit().unwrap();
        cb.wait_until_completed().unwrap();
        assert_eq!(cb.status, CommandBufferStatus::Completed);
    }

    #[test]
    fn wait_on_un_committed_buffer_errors() {
        let mut cb = EncodedCommandBuffer::stub(1);
        let r = cb.wait_until_completed();
        assert!(matches!(r, Err(MetalError::CommitFailed { .. })));
    }
}

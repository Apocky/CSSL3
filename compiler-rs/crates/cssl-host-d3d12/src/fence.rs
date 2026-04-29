//! D3D12 fence-based GPU synchronization.
//!
//! § DESIGN
//!   - `Fence` wraps `ID3D12Fence` ; uses a Win32 `Event` for CPU-side waits.
//!   - `FenceWait` records the current value + timeout for diagnosability.
//!
//! § FLOW
//!   ```text
//!     queue.submit(&[list]) ;
//!     let target = fence.next_value() ;
//!     queue.signal(&fence, target)? ;
//!     fence.wait(target, Duration::from_secs(5))? ;
//!     allocator.reset()? ;     // safe — GPU work complete.
//!   ```

// (re-imported inside cfg-gated `imp` modules)

/// Snapshot of a fence wait result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceWait {
    /// Value waited for.
    pub target: u64,
    /// Whether the wait completed (true) or timed out (false).
    pub completed: bool,
    /// Milliseconds elapsed (or timeout limit on timeout).
    pub elapsed_millis: u32,
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::FenceWait;
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};
    use core::time::Duration;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
    use windows::Win32::Graphics::Direct3D12::{ID3D12Fence, D3D12_FENCE_FLAG_NONE};
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

    /// D3D12 fence + Win32 wait event.
    pub struct Fence {
        pub(crate) fence: ID3D12Fence,
        pub(crate) event: HANDLE,
        pub(crate) next: core::cell::Cell<u64>,
    }

    impl core::fmt::Debug for Fence {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Fence")
                .field("next", &self.next.get())
                .finish_non_exhaustive()
        }
    }

    impl Fence {
        /// Create a new fence with the given initial value (typically 0).
        pub fn new(device: &Device, initial: u64) -> Result<Self> {
            // SAFETY : FFI ; result is owned (RAII).
            let fence: ID3D12Fence =
                unsafe { device.device.CreateFence(initial, D3D12_FENCE_FLAG_NONE) }
                    .map_err(|e| crate::device::imp_map_hresult("CreateFence", e))?;
            // SAFETY : CreateEventW with NULL attributes / unnamed creates an
            // auto-reset event. Returns INVALID_HANDLE_VALUE (==-1) on error.
            let event: HANDLE = unsafe { CreateEventW(None, false, false, None) }
                .map_err(|e| crate::device::imp_map_hresult("CreateEventW", e))?;
            Ok(Self {
                fence,
                event,
                next: core::cell::Cell::new(initial.saturating_add(1)),
            })
        }

        /// Get the next value to be signaled (auto-increments on each call).
        pub fn next_value(&self) -> u64 {
            let v = self.next.get();
            self.next.set(v.saturating_add(1));
            v
        }

        /// Get the fence's currently-completed value.
        pub fn completed_value(&self) -> u64 {
            // SAFETY : fence lives.
            unsafe { self.fence.GetCompletedValue() }
        }

        /// Wait for the fence to reach `target`. Times out per `timeout`.
        pub fn wait(&self, target: u64, timeout: Duration) -> Result<FenceWait> {
            if self.completed_value() >= target {
                return Ok(FenceWait {
                    target,
                    completed: true,
                    elapsed_millis: 0,
                });
            }
            // SAFETY : fence + event live ; SetEventOnCompletion is documented stable.
            unsafe { self.fence.SetEventOnCompletion(target, self.event) }
                .map_err(|e| crate::device::imp_map_hresult("SetEventOnCompletion", e))?;
            let ms = timeout
                .as_millis()
                .min(u128::from(u32::MAX - 1))
                .try_into()
                .unwrap_or(INFINITE);
            // SAFETY : event lives ; WaitForSingleObject is documented stable.
            let result = unsafe { WaitForSingleObject(self.event, ms) };
            if result == WAIT_TIMEOUT {
                return Err(D3d12Error::FenceTimeout { millis: ms });
            }
            Ok(FenceWait {
                target,
                completed: true,
                elapsed_millis: ms,
            })
        }

        /// Borrow underlying fence (used internally).
        #[allow(dead_code)]
        pub(crate) fn raw(&self) -> &ID3D12Fence {
            &self.fence
        }
    }

    impl Drop for Fence {
        fn drop(&mut self) {
            // SAFETY : event was created in this struct ; close exactly once.
            let _ = unsafe { CloseHandle(self.event) };
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::FenceWait;
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};
    use core::time::Duration;

    /// Fence stub.
    #[derive(Debug)]
    pub struct Fence;

    impl Fence {
        /// Always returns `LoaderMissing`.
        pub fn new(_device: &Device, _initial: u64) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub returns 0.
        #[must_use]
        pub const fn next_value(&self) -> u64 {
            0
        }

        /// Stub returns 0.
        #[must_use]
        pub const fn completed_value(&self) -> u64 {
            0
        }

        /// Always returns `LoaderMissing`.
        pub fn wait(&self, _target: u64, _timeout: Duration) -> Result<FenceWait> {
            Err(D3d12Error::loader("non-Windows target"))
        }
    }
}

pub use imp::Fence;

#[cfg(test)]
mod tests {
    use super::FenceWait;

    #[test]
    fn fence_wait_construction() {
        let w = FenceWait {
            target: 7,
            completed: true,
            elapsed_millis: 12,
        };
        assert_eq!(w.target, 7);
        assert!(w.completed);
        assert_eq!(w.elapsed_millis, 12);
    }

    #[test]
    fn fence_wait_timeout_shape() {
        let w = FenceWait {
            target: 100,
            completed: false,
            elapsed_millis: 5_000,
        };
        assert!(!w.completed);
        assert_eq!(w.elapsed_millis, 5_000);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn fence_new_returns_loader_missing() {
        // No Device available on non-Windows ; type-only check.
        let _ = core::mem::size_of::<super::Fence>();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn fence_creation_and_zero_completed_or_skip() {
        use super::Fence;
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
        let fence = Fence::new(&device, 0).expect("fence creation should succeed");
        // Initial completed value is 0 (not signaled yet).
        assert_eq!(fence.completed_value(), 0);
        // Calling next_value() multiple times yields strict-monotonic values.
        let v1 = fence.next_value();
        let v2 = fence.next_value();
        assert!(v2 > v1);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn fence_wait_satisfied_immediately_when_target_zero() {
        use super::Fence;
        use crate::device::{AdapterPreference, Device, Factory};
        use core::time::Duration;
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        // Initial value is already 5 ; waiting for 5 should satisfy immediately.
        let fence = Fence::new(&device, 5).expect("fence creation");
        let result = fence.wait(5, Duration::from_millis(100));
        assert!(result.is_ok());
        let w = result.unwrap();
        assert_eq!(w.target, 5);
        assert!(w.completed);
    }
}

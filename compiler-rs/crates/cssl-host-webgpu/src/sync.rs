//! Queue submission + synchronization helpers.
//!
//! § §§ 14_BACKEND § HOST-SUBMIT BACKENDS § per-backend adapter-layer §
//!   submit + wait.
//! § §§ 12_CAPABILITIES § ISO-OWNERSHIP : `wgpu::Buffer` is `iso<gpu-buffer>` ;
//!   sync via `Queue.on_submitted_work_done`.
//!
//! § DESIGN
//!   wgpu submits a `CommandBuffer` via `Queue::submit`. Two sync primitives
//!   are exposed at stage-0 :
//!     1. `submit_and_block` — fire + `Device::poll(Wait)` until the GPU
//!        finishes. Used in tests where the next step needs the result.
//!     2. `submit_with_callback` — fire + register `Queue::on_submitted_work_done`
//!        callback. Used in async / streaming workloads.
//!
//!   `read_buffer_sync` is the standard buffer-readback pattern :
//!     map_async → poll(Wait) → get_mapped_range → memcpy → unmap.

use crate::buffer::WebGpuBuffer;
use crate::device::WebGpuDevice;
use crate::error::WebGpuError;
use std::sync::mpsc;

/// Submit a recorded command-buffer + block until GPU completes.
pub fn submit_and_block(
    device: &WebGpuDevice,
    cmd: wgpu::CommandBuffer,
) -> Result<(), WebGpuError> {
    let queue = device.raw_queue();
    queue.submit(std::iter::once(cmd));
    // Poll the device until all submitted work is done.
    // wgpu 23: PollType is Wait | WaitForSubmissionIndex | Poll.
    device
        .raw_device()
        .poll(wgpu::Maintain::Wait)
        .panic_on_timeout();
    Ok(())
}

/// Submit a recorded command-buffer + register a callback for completion.
/// Returns immediately ; the callback fires from a wgpu-internal thread
/// when the GPU finishes the submission.
pub fn submit_with_callback<F>(
    device: &WebGpuDevice,
    cmd: wgpu::CommandBuffer,
    on_done: F,
) -> Result<(), WebGpuError>
where
    F: FnOnce() + Send + 'static,
{
    let queue = device.raw_queue();
    queue.submit(std::iter::once(cmd));
    queue.on_submitted_work_done(on_done);
    Ok(())
}

/// Read a `MAP_READ`-able buffer's contents back to CPU.
///
/// ‼ Buffer must have been created with `BufferUsages::MAP_READ`.
/// Typical pattern : create staging buffer with MAP_READ + COPY_DST, copy
/// GPU-result into staging, submit, then read.
pub fn read_buffer_sync(device: &WebGpuDevice, buf: &WebGpuBuffer) -> Result<Vec<u8>, WebGpuError> {
    if !buf.usage().contains(wgpu::BufferUsages::MAP_READ) {
        return Err(WebGpuError::Buffer(
            "read_buffer_sync requires MAP_READ usage".into(),
        ));
    }

    let slice = buf.raw().slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        // Send the map-result back ; ignore send-failure (rx dropped).
        let _ = tx.send(result);
    });

    // Drive wgpu until the map_async callback fires.
    device
        .raw_device()
        .poll(wgpu::Maintain::Wait)
        .panic_on_timeout();

    let map_result = rx
        .recv()
        .map_err(|e| WebGpuError::Buffer(format!("map_async channel : {e}")))?;
    map_result.map_err(|e| WebGpuError::Buffer(format!("map_async failed : {e}")))?;

    let view = slice.get_mapped_range();
    let data = view.to_vec();
    drop(view);
    buf.raw().unmap();
    Ok(data)
}

#[cfg(test)]
mod tests {
    // Sync helpers need a real device, covered by integration tests in
    // `tests/compute_pipeline_smoke.rs`. This module-level test ensures
    // the surface compiles on hosts without a GPU.
    use super::{read_buffer_sync, submit_and_block, submit_with_callback};

    #[test]
    fn sync_fns_have_expected_signatures() {
        // Compile-only check : the three sync helpers exist with the
        // documented signatures. Real behaviour covered by integration tests.
        let _: fn(&_, _) -> _ = submit_and_block;
        let _: fn(&_, _, _) -> _ = submit_with_callback::<fn()>;
        let _: fn(&_, &_) -> _ = read_buffer_sync;
    }
}

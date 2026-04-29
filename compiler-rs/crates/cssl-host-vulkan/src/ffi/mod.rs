//! § ffi : ash-backed Vulkan host backend (T11-D65, S6-E1).
//!
//! § ROLE
//!   Real `ash`-FFI implementation of the Vulkan host surface that
//!   stage-0 uses to drive a compute pipeline through to fence-signal.
//!   The pre-existing catalog modules (`device.rs` / `extensions.rs`
//!   / `arc_a770.rs` / `probe.rs`) carry the type-system + spec-fact
//!   layer ; this submodule wires them into `vkCreateInstance` /
//!   `vkEnumeratePhysicalDevices` / `vkCreateDevice` / etc. via ash.
//!
//! § ash-VERSION
//!   Pinned to workspace `ash = "0.38"` (Vulkan 1.3.281 SDK headers).
//!   1.4 functionality reachable through ash's raw `Entry::vk_make_api_version`
//!   and matching `vk::API_VERSION_1_4`. Per `specs/10_HW.csl § VULKAN
//!   1.4 BASELINE` we declare 1.4 as the api-version we request from the
//!   loader ; the loader negotiates with the installed ICD.
//!
//! § LOADER-MISSING SAFETY
//!   `Entry::linked()` requires the build-time linkage path — we use
//!   `Entry::load()` so a missing `vulkan-1.dll` / `libvulkan.so.1` at
//!   runtime surfaces as `LoaderError::Loading`. Tests gate-skip when
//!   the loader is absent (mirrors the `BinaryMissing` pattern from
//!   `csslc::linker`).
//!
//! § UNSAFE-FFI POLICY
//!   The crate-level attribute (in `lib.rs`) is `#![deny(unsafe_code)]`
//!   so the cap-/extension-/device-catalog code stays sound-by-default.
//!   Only this `ffi` submodule (and its children) opt back in via
//!   `#![allow(unsafe_code)]` at the module level — every unsafe block
//!   carries an inline `// SAFETY :` paragraph stating the precondition.
//!
//! § PRIME-DIRECTIVE
//!   This is FFI to a graphics driver — no surveillance, no telemetry-
//!   exfiltration. Validation layers are gated to `cfg(debug_assertions)`
//!   so release-builds don't open a side-channel through the debug-utils
//!   callback. The callback (when active in debug builds) records into
//!   a process-local ring ; nothing escapes the process.

#![allow(unsafe_code)]

pub mod buffer;
pub mod command;
pub mod device;
pub mod error;
pub mod instance;
pub mod physical_device;
pub mod pipeline;
pub mod telemetry;

pub use buffer::{BufferKind, VkBufferHandle};
pub use command::{CommandContext, FenceState};
pub use device::LogicalDevice;
pub use error::{AshError, LoaderError};
pub use instance::{InstanceConfig, VkInstanceHandle};
pub use physical_device::{PhysicalDevicePick, ScoredPhysical};
pub use pipeline::{ComputePipelineHandle, ShaderModuleHandle};
pub use telemetry::{TelemetrySnapshot, VulkanTelemetryRing};

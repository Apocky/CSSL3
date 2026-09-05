# Audit 11 — Host-Runtime Crates

**Auditor:** automated full-file audit  
**Date:** 2026-05-14  
**Repo root:** `compiler-rs/crates/cssl-host-*/`  
**Spec authority:** `specs/10_HW.csl`, `specs/14_BACKEND.csl`, `specs/22_TELEMETRY.csl`  
**Files audited:** 22 source files + 5 Cargo.toml files (27 total)  

---

## 1. Slice Overview

These five crates form the **host-runtime device-abstraction layer** of the CSSLv3 stage-0 (Rust-hosted "throwaway bootstrap") compiler. Their role is to hold the capability model — device identity, extension/feature catalogs, heap and memory abstractions, and telemetry probe traits — for the five GPU/compute submission paths that compiled CSSL code will eventually run on: **Vulkan, Intel Level-Zero, Direct3D 12, Metal, and WebGPU**.

### Common pattern

Every crate follows an identical phase-1 scaffold pattern:

1. **Pure Rust, zero FFI at phase-1.** All five crates have `#![forbid(unsafe_code)]` and `thiserror` as their only non-workspace dependency. The actual GPU API crates (`ash`, `windows`, `wgpu`, `metal`, `level-zero-sys`) are declared in the workspace `[workspace.dependencies]` table but are **not listed in any individual crate's `[dependencies]`**. The FFI boundary is entirely deferred to phase-2.

2. **Canonical Arc A770 stubs.** Every crate provides at least one stub constructor hard-coded to the Intel Arc A770 (PCI device ID `0x56A0`, vendor `0x8086`), the v1 primary development target identified in `specs/10_HW.csl`. This lets the rest of the compiler toolchain reference device capabilities without a live GPU.

3. **Trait + stub impl pair.** Where an active probe is needed (Vulkan `FeatureProbe`, Level-Zero `TelemetryProbe`) the crate defines a trait for the real phase-2 impl and ships a `Stub*` that returns the Arc A770 canonical values.

4. **In-file `#[cfg(test)]` modules.** No external `tests/` directories exist. Every file's test coverage is co-located, totalling roughly 85–90 test functions across all five crates.

### Maturity / real-vs-stubbed status

| Crate | FFI crate | Status |
|---|---|---|
| `cssl-host-vulkan` | `ash 0.38` | Workspace-declared, **not wired** — phase-1 pure scaffold |
| `cssl-host-level-zero` | `level-zero-sys` | Commented out in workspace (`# level-zero-sys = "0.3"`) — **no FFI at all**, not even the dep |
| `cssl-host-d3d12` | `windows 0.58` | Workspace-declared, **not wired** — phase-1 pure scaffold |
| `cssl-host-metal` | `metal` crate | Workspace comment says "mac-only per-target-cfg @ T10", **not wired** in any crate Cargo.toml |
| `cssl-host-webgpu` | `wgpu 23` | Workspace-declared, **not wired** — phase-1 pure scaffold |

All five crates are pure Rust enumerations, structs, and trait definitions with no OS or GPU API calls at stage-0. They compile on Windows, macOS, and Linux unchanged.

---

## 2. Crate: `cssl-host-vulkan`

**Path:** `compiler-rs/crates/cssl-host-vulkan/`  
**Description in Cargo.toml:** "CSSLv3 stage0 — Vulkan 1.4.333 host submission via ash (FFI)"

**Purpose:** Catalogs the complete Vulkan 1.4 capability surface that CSSLv3 targets. Provides device identification (vendor, device-ID, API version, device type), a 30-variant extension catalog (all extensions confirmed on the Arc A770 ISV driver), a feature-flag bitset struct, a hard-coded Arc A770 hardware profile, and a probe trait whose stub implementation returns the A770 canonical data. This is the most thorough of the five crates and is the direct spec implementation of `specs/10_HW.csl § VULKAN 1.4 BASELINE`.

**Cargo.toml dependencies:** `thiserror` (workspace). `ash 0.38` is in the workspace dependency table but is **absent from this crate's `[dependencies]`** — phase-2 blocked per `T1-D7` MSVC toolchain gating.

**Total LOC:** ~918 (lib.rs 63 + probe.rs 119 + extensions.rs 284 + arc_a770.rs 212 + device.rs 344)

**File list:**
- `src/lib.rs` — crate root, attributes, module declarations, re-exports, `STAGE0_SCAFFOLD` const
- `src/device.rs` — `VulkanVersion`, `GpuVendor`, `DeviceType`, `DeviceFeatures`, `VulkanDevice`
- `src/extensions.rs` — `VulkanExtension` (30 variants), `VulkanLayer`, `VulkanExtensionSet`
- `src/arc_a770.rs` — `ArcA770Profile` (compile-time A770 hardware constants + stub factory)
- `src/probe.rs` — `FeatureProbe` trait, `ProbeError`, `StubProbe`

---

### `src/lib.rs` — 63 lines

Crate entry point. Declares `#![forbid(unsafe_code)]`, three `#![deny]` and two `#![allow]` lint attrs. Declares four public submodules (`arc_a770`, `device`, `extensions`, `probe`) and re-exports all public types. Defines one public constant and one internal test.

**Items:**

- `pub const STAGE0_SCAFFOLD: &str` — exposes `CARGO_PKG_VERSION` for external scaffold-version assertions.

**Test module `scaffold_tests`:**
- `fn scaffold_version_present()` — asserts `STAGE0_SCAFFOLD` is non-empty.

---

### `src/device.rs` — 344 lines

Device and vendor enumeration. Mirrors Vulkan's `VkPhysicalDevice*` property structs as pure Rust types.

**Items:**

- `enum VulkanVersion` — `V1_0`, `V1_1`, `V1_2`, `V1_3`, `V1_4`. Derives `PartialOrd`/`Ord` so version comparisons work.
  - `fn dotted(self) -> &'static str` — human-readable dotted form ("1.4").
  - `fn packed(self) -> u32` — ash-compatible packed integer `(major << 22) | (minor << 12)` — **note:** patch bits are always 0, which is correct for Vulkan's typical VK_MAKE_API_VERSION encoding when patch is 0.
  - `impl fmt::Display` — delegates to `dotted`.

- `enum GpuVendor` — `Intel`, `Nvidia`, `Amd`, `Apple`, `Qualcomm`, `Arm`, `Mesa`, `Other`. Covers all PCI vendor IDs relevant to CSSLv3 targets.
  - `fn from_pci_id(id: u32) -> Self` — constant PCI-ID→vendor map.
  - `fn as_str(self) -> &'static str` — lowercase short name.

- `enum DeviceType` — `Integrated`, `Discrete`, `Virtual`, `Cpu`, `Other`. Mirrors `VkPhysicalDeviceType`.
  - `fn as_str(self) -> &'static str` — lowercase short name.

- `struct DeviceFeatures` — 25 `bool` fields covering every Vulkan feature CSSLv3 exercises. Fields: `storage_buffer_16bit_access`, `uniform_and_storage_buffer_8bit_access`, `shader_float16`, `shader_int8`, `shader_int16`, `shader_int64`, `buffer_device_address`, `runtime_descriptor_array`, `shader_non_uniform`, `vulkan_memory_model`, `vulkan_memory_model_device_scope`, `cooperative_matrix`, `ray_tracing_pipeline`, `ray_query`, `acceleration_structure`, `mesh_shader`, `subgroup_uniform_control_flow`, `shader_subgroup_rotate`, `shader_expect_assume`, `shader_float_controls2`, `shader_atomic_float_add`, `shader_atomic_float_min_max`, `mutable_descriptor_type`, `demote_to_helper_invocation`, `shader_non_semantic_info`.
  - `fn none() -> Self` — all-false baseline (const fn).
  - `fn count_enabled(&self) -> u32` — counts true fields; collects into a `[bool; 25]` slice, filters, counts. Safe but non-const (method uses `try_from` and iterator).

- `struct VulkanDevice` — canonical adapter record: `name: String`, `vendor_id: u32`, `device_id: u32`, `vendor: GpuVendor`, `device_type: DeviceType`, `api_version: VulkanVersion`, `driver_version: u32`, `features: DeviceFeatures`.
  - `fn stub(name, vendor_id, device_id) -> Self` — builds a minimal record (DeviceType::Other, VulkanVersion::V1_4, DeviceFeatures::none()).
  - `fn summary(&self) -> String` — formats `"name / vendor / VK version / type / N features"`.

**Test module `tests` (device.rs):**
- `fn vulkan_version_dotted()` — spot-checks V1_0 and V1_4.
- `fn vulkan_version_packed_is_monotonic()` — asserts packed(V1_4) > packed(V1_3) > packed(V1_2).
- `fn vendor_from_pci_id()` — checks Intel/NVIDIA/AMD and an unknown ID.
- `fn device_type_names()` — spot-checks Discrete and Integrated.
- `fn device_features_none_count_is_zero()` — none().count_enabled() == 0.
- `fn device_features_count_correct()` — sets 3 flags, asserts count == 3.
- `fn stub_device_defaults_to_vk_1_4()` — stub() gives V1_4 and correct vendor.
- `fn stub_summary_contains_vendor_and_version()` — summary() contains "intel" and "VK 1.4".

---

### `src/extensions.rs` — 284 lines

Vulkan extension and layer catalog with a `BTreeSet`-backed extension set.

**Items:**

- `enum VulkanExtension` — 31 variants (31 names, but the enum is labeled "30-variant" in lib.rs — the count is actually 31 with `ExtConservativeRasterization` and all the others). Annotated with the canonical Vulkan string name for each. Derives `PartialOrd`/`Ord` (enum declaration order, used by `BTreeSet` for deterministic iteration).
  - `fn as_str(self) -> &'static str` — returns the canonical `VK_KHR_*` / `VK_EXT_*` string (const fn).
  - `fn is_core_in_vk_1_4(self) -> bool` — returns true for 13 extensions promoted to VK-1.4 core (const fn using `matches!`).
  - `impl fmt::Display` — delegates to `as_str`.

- `enum VulkanLayer` — 5 variants: `KhronosValidation`, `LunarGApiDump`, `LunarGMonitor`, `KhronosProfiles`, `KhronosSynchronization2`.
  - `fn as_str(self) -> &'static str` — canonical `VK_LAYER_*` string.

- `struct VulkanExtensionSet` — wraps `BTreeSet<VulkanExtension>`. Sorted iteration guaranteed.
  - `fn new() -> Self` — empty set.
  - `fn add(&mut self, e: VulkanExtension)` — inserts.
  - `fn contains(&self, e: VulkanExtension) -> bool` — membership.
  - `fn iter(&self) -> impl Iterator<Item = VulkanExtension> + '_` — sorted copy-iter.
  - `fn len(&self) -> usize` — size.
  - `fn is_empty(&self) -> bool` — empty check.
  - `impl FromIterator<VulkanExtension>` — builds from any iterator; enables `from_iter([...])` construction.
  - `impl Default` — via `#[derive(Default)]`.

**Test module `tests` (extensions.rs):**
- `fn extension_names()` — spot-checks three extension strings.
- `fn core_in_vk_1_4_flag()` — asserts KhrMaintenance5 and KhrShaderFloatControls2 are core-1.4; KhrRayQuery and KhrCooperativeMatrix are not.
- `fn layer_names()` — checks KhronosValidation string.
- `fn extension_set_ops()` — add, contains, len, is_empty.
- `fn extension_set_from_iter_sorted()` — verifies BTreeSet produces declaration-order output (KhrSwapchain < KhrRayQuery).

**Bug / correctness note:** The lib.rs doc comment says "30-variant catalog" (`VulkanExtension` has 31 enum variants). This is a minor count mismatch in the documentation, not a functional error, but a new contributor reading the module-level doc comment will count wrong. (`lib.rs:14`)

---

### `src/arc_a770.rs` — 212 lines

Hard-coded hardware profile for the Intel Arc A770 (Alchemist DG2-512 Xe-HPG). Verified against `specs/10_HW.csl § ARC A770 DETAILED SPECS`.

**Items:**

- `struct ArcA770Profile` — all-public fields: `device_name: &'static str`, `vendor_id: u32`, `device_id: u32`, `api_version: VulkanVersion`, `xe_cores: u32`, `total_xve: u32`, `total_xmx: u32`, `rt_cores: u32`, `clock_boost_mhz: u32`, `vram_mb: u32`, `memory_bandwidth_gbps: u32`, `l2_cache_mb: u32`, `pcie_gen: u32`, `pcie_lanes: u32`, `tdp_w: u32`.

  - `fn canonical() -> Self` — const fn returning the verified A770 constants:
    - `device_name`: "Intel(R) Arc(TM) A770 Graphics"
    - `vendor_id`: 0x8086 (Intel)
    - `device_id`: 0x56A0 (DG2-512)
    - `api_version`: VulkanVersion::V1_4
    - `xe_cores`: 32, `total_xve`: 512, `total_xmx`: 512, `rt_cores`: 32
    - `clock_boost_mhz`: 2100, `vram_mb`: 16384, `memory_bandwidth_gbps`: 560
    - `l2_cache_mb`: 16, `pcie_gen`: 4, `pcie_lanes`: 16, `tdp_w`: 225

  - `fn to_vulkan_device(&self) -> VulkanDevice` — constructs a `VulkanDevice` from the profile. Sets `driver_version` to `0x2000_2165` with comment "driver 32.0.101.8629 approximate". Calls private `expected_features()`.

  - `fn expected_extensions() -> VulkanExtensionSet` — builds the 29-extension set the ISV driver enables on this device. Notable inclusions: `KhrCooperativeMatrix`, `KhrRayTracingPipeline`, `KhrAccelerationStructure`, `KhrRayQuery`, `ExtMutableDescriptorType`, `ExtShaderAtomicFloat`, `ExtShaderAtomicFloat2`, `ExtMeshShader`. **Note:** `ExtConservativeRasterization` and `KhrGlobalPriority` are in the `VulkanExtension` enum but absent from the A770 expected extension set — this implies these may not be available on the ISV driver, which is correct for `VK_EXT_conservative_rasterization` (not exposed by Intel's Alchemist driver at time of writing).

  - `fn peak_fp32_tflops_times_10(&self) -> u32` — const fn returning `172` (representing 17.2 TFLOPs stored as integer to avoid float in const fn). Spec value: 17.2 per `specs/10`.

- `fn expected_features() -> DeviceFeatures` — private free function; sets all 25 feature flags to true. Called only from `to_vulkan_device`.

**Test module `tests` (arc_a770.rs):**
- `fn canonical_matches_spec()` — checks all hardware constants against spec-documented values.
- `fn to_vulkan_device_preserves_spec_facts()` — vendor=Intel, type=Discrete, api=V1_4, cooperative_matrix/ray_tracing/shader_float_controls2 all true.
- `fn expected_extensions_includes_coop_matrix_and_rt()` — spot-checks 4 critical extensions.
- `fn peak_fp32_tflops_value()` — asserts == 172.
- `fn expected_features_all_set()` — asserts `count_enabled() == 25`.

---

### `src/probe.rs` — 119 lines

Feature-probe trait and stage-0 stub.

**Items:**

- `enum ProbeError` — derives `Error` via `thiserror`. Three variants:
  - `LoaderMissing` — "Vulkan loader missing — no `vulkan-1.dll` / `libvulkan.so.1` on PATH"
  - `FfiNotWired` — "FFI backend not wired at stage-0 (T10-phase-2 delivers `ash` integration)"
  - `DeviceNotFound { query: String }` — "no Vulkan device matches predicate `{query}`"

- `trait FeatureProbe` — defines the async-free, synchronous phase-2 API:
  - `fn enumerate_devices(&self) -> Result<Vec<VulkanDevice>, ProbeError>` — lists all physical devices.
  - `fn supported_extensions(&self, device_idx: usize) -> Result<VulkanExtensionSet, ProbeError>` — extensions for a specific device by index.
  - `fn has_extension(&self, device_idx: usize, ext: VulkanExtension) -> Result<bool, ProbeError>` — provided default impl: calls `supported_extensions` then `contains`. Correctly propagates errors.

- `struct StubProbe` — `#[derive(Debug, Clone, Default)]`, zero-size.
  - `fn new() -> Self` — const fn.
  - `impl FeatureProbe for StubProbe`:
    - `enumerate_devices` — returns a single-element Vec with `ArcA770Profile::canonical().to_vulkan_device()`.
    - `supported_extensions` — returns Err(DeviceNotFound) for `device_idx != 0`; otherwise returns `ArcA770Profile::expected_extensions()`.

**Test module `tests` (probe.rs):**
- `fn stub_enumerates_arc_a770()` — devices.len() == 1, vendor == Intel, device_id == 0x56A0.
- `fn stub_supported_extensions_for_dev_0()` — KhrRayTracingPipeline and KhrCooperativeMatrix present.
- `fn stub_out_of_range_device_errors()` — idx=42 gives DeviceNotFound.
- `fn has_extension_returns_false_for_absent()` — note: the comment in this test is incorrect — it says it tests the false-path but actually calls `has_extension(0, KhrRayQuery)` which is in the A770 profile and returns `true`. The test asserts `true`, so the test is correct but the comment is misleading (`probe.rs:110-117`).

---

## 3. Crate: `cssl-host-level-zero`

**Path:** `compiler-rs/crates/cssl-host-level-zero/`  
**Description in Cargo.toml:** "CSSLv3 stage0 — Intel Level-Zero host submission + sysman (R18) (FFI)"

**Purpose:** Catalogs the Intel Level-Zero compute API surface and the sysman R18 telemetry metric matrix. Level-Zero is the low-level compute-first API for Intel GPUs (parallel to Vulkan, gives direct USM memory and SPIR-V module submission). The sysman subsystem provides hardware telemetry (power, temperature, frequency, engine activity, RAS events). This crate is referenced by `specs/10_HW.csl § LEVEL-ZERO BASELINE` and `specs/22_TELEMETRY.csl`.

**Cargo.toml dependencies:** `thiserror` only. `level-zero-sys` is **explicitly commented out** in the workspace Cargo.toml (`# level-zero-sys = "0.3"  # T10 : verify-registry-availability`), meaning the crate is at zero FFI — not even a placeholder link. This crate has the least-wired FFI path of all five.

**Total LOC:** ~744 (lib.rs 57 + driver.rs 139 + api.rs 170 + sysman.rs 382)

**File list:**
- `src/lib.rs` — crate root, attributes, module declarations, re-exports, `STAGE0_SCAFFOLD` const
- `src/driver.rs` — `L0DeviceType`, `L0DeviceProperties`, `L0Device`, `L0Driver`
- `src/api.rs` — `L0ApiSurface` (24 variants), `UsmAllocType`
- `src/sysman.rs` — `SysmanMetric`, `MetricCategory`, `SysmanMetricSet`, `SysmanSample`, `SysmanCapture`, `TelemetryError`, `TelemetryProbe` trait, `StubTelemetryProbe`

---

### `src/lib.rs` (level-zero) — 57 lines

Same scaffold pattern as Vulkan: `#![forbid(unsafe_code)]`, three `#![deny]`, two `#![allow]`. Declares `api`, `driver`, `sysman` modules. Re-exports all public types. `STAGE0_SCAFFOLD` const. One scaffold test.

**Items:**

- `pub const STAGE0_SCAFFOLD: &str` — version string.

**Test module `scaffold_tests`:**
- `fn scaffold_version_present()` — non-empty check.

---

### `src/driver.rs` — 139 lines

Level-Zero driver and device representation.

**Items:**

- `enum L0DeviceType` — `Gpu`, `Cpu`, `Fpga`, `Mca`, `Vpu`. Mirrors `ze_device_type_t`.
  - `fn as_str(self) -> &'static str` — lowercase short name.

- `struct L0DeviceProperties` — mirrors relevant fields from `ze_device_properties_t`: `name: String`, `device_type: L0DeviceType`, `vendor_id: u32`, `device_id: u32`, `core_clock_rate_mhz: u32`, `max_compute_units: u32`, `global_memory_mb: u32`, `max_workgroup_size: u32`, `api_major: u16`, `api_minor: u16`.

- `struct L0Device` — opaque stage-0 handle: `driver_index: u32`, `device_index: u32`, `properties: L0DeviceProperties`.

- `struct L0Driver` — `index: u32`, `api_major: u16`, `api_minor: u16`, `devices: Vec<L0Device>`.
  - `fn stub_arc_a770() -> Self` — returns a driver at L0 API 1.14 with one device: A770 (vendor_id=0x8086, device_id=0x56A0, core_clock=2100 MHz, 32 compute units, 16 GB global memory, max workgroup size 1024).

**Test module `tests` (driver.rs):**
- `fn device_type_names()` — checks Gpu and Fpga.
- `fn stub_driver_exposes_arc_a770()` — api_major==1, api_minor>=14, 1 device, correct vendor/device IDs, type==Gpu, max_compute_units==32, global_memory==16384 MB.
- `fn stub_device_name()` — name contains "Arc".

---

### `src/api.rs` — 170 lines

Level-Zero API surface enumeration and USM allocation types.

**Items:**

- `enum L0ApiSurface` — 24 variants covering the complete CSSLv3-relevant L0 API surface. Core APIs: `ZeInit`, `ZeDriverGet`, `ZeDeviceGet`, `ZeDeviceGetProperties`, `ZeContextCreate`, `ZeCommandListCreate`, `ZeCommandListCreateImmediate`, `ZeEventPoolCreate`, `ZeEventCreate`, `ZeModuleCreate`, `ZeKernelCreate`, `ZeCommandListAppendLaunchKernel`, `ZeMemAllocDevice`, `ZeMemAllocHost`, `ZeMemAllocShared`. Sysman APIs: `ZesDeviceGetProperties`, `ZesPowerGetEnergyCounter`, `ZesPowerSetLimits`, `ZesTemperatureGetState`, `ZesFrequencyGetState`, `ZesFrequencyOcGet`, `ZesEngineGetActivity`, `ZesRasGetState`, `ZesDeviceProcessesGetState`.
  - `fn as_str(self) -> &'static str` — canonical `ze*` / `zes*` entry-point name (const fn).
  - `fn is_sysman(self) -> bool` — true for the 9 `zes*` variants (const fn using `matches!`).
  - `impl fmt::Display` — delegates to `as_str`.

- `enum UsmAllocType` — `Host`, `Device`, `Shared`. Mirrors the L0 USM allocation model.
  - `fn as_str(self) -> &'static str` — lowercase short name.

**Test module `tests` (api.rs):**
- `fn api_names()` — spot-checks ZeInit, ZeModuleCreate, ZesPowerGetEnergyCounter strings.
- `fn sysman_flag()` — ZesPowerGetEnergyCounter and ZesTemperatureGetState are sysman; ZeInit and ZeModuleCreate are not.
- `fn usm_alloc_types()` — checks all three short names.

---

### `src/sysman.rs` — 382 lines

Sysman R18 telemetry metric catalog, capture abstractions, and stub probe.

**Items:**

- `enum SysmanMetric` — 11 variants, derives `PartialOrd`/`Ord` (BTreeSet ordering). Maps each metric to a `zes*` API call:
  - `PowerEnergyCounter` → `zesPowerGetEnergyCounter` (millijoules, accumulated)
  - `PowerLimits` → `zesPowerGetLimits`/`SetLimits` (watts TDP)
  - `TemperatureCurrent` → `zesTemperatureGetState` (°C die temp)
  - `TemperatureMaxRange` → `zesTemperatureGetMaxRange` (°C thermal envelope)
  - `FrequencyCurrent` → `zesFrequencyGetState` (MHz)
  - `FrequencyRange` → `zesFrequencyGetRange` (MHz min/max)
  - `FrequencyOverclock` → `zesFrequencyOcGet` (factor)
  - `EngineActivity` → `zesEngineGetActivity` (µs accumulated)
  - `RasEvents` → `zesRasGetState` (reliability/availability/serviceability count)
  - `ProcessList` → `zesDeviceProcessesGetState` (running process count)
  - `PerformanceFactor` → `zesPerformanceFactorGetConfig` (%)
  - `fn as_str(self) -> &'static str` — dotted metric key (e.g. "power.energy_counter_mj").
  - `fn category(self) -> MetricCategory` — groups into 7 categories.
  - `pub const ALL_METRICS: [Self; 11]` — exhaustive array for full-set construction.
  - `impl fmt::Display` — delegates to `as_str`.

- `enum MetricCategory` — `Power`, `Thermal`, `Frequency`, `EngineActivity`, `Ras`, `Processes`, `Performance`.

- `struct SysmanMetricSet` — wraps `BTreeSet<SysmanMetric>`.
  - `fn new() -> Self` — empty.
  - `fn full_r18() -> Self` — all 11 metrics (canonical R18 fidelity set).
  - `fn advisory() -> Self` — 3-metric subset (PowerEnergyCounter, TemperatureCurrent, FrequencyCurrent) for non-privileged probing.
  - `fn add(&mut self, m: SysmanMetric)` — insert.
  - `fn contains(&self, m: SysmanMetric) -> bool` — membership.
  - `fn iter(&self) -> impl Iterator<Item = SysmanMetric> + '_` — sorted copy-iter.
  - `fn len(&self) -> usize` — size.
  - `fn is_empty(&self) -> bool` — empty check.
  - `impl FromIterator<SysmanMetric>` — enables from_iter construction.

- `struct SysmanSample` — `metric: SysmanMetric`, `value: f64`, `timestamp_us: u64`.

- `struct SysmanCapture` — `samples: Vec<SysmanSample>`, `device_index: u32`. One full capture round.

- `enum TelemetryError` — derives `Error`. Three variants:
  - `SysmanNotInitialized` — "Sysman not initialized — call `zesInit` first"
  - `UnsupportedMetric { device_index: u32, metric: SysmanMetric }` — per-metric unavailability
  - `FfiNotWired` — stage-0 stub sentinel

- `trait TelemetryProbe` — single method:
  - `fn capture(&self, device: &L0Device, metrics: &SysmanMetricSet) -> Result<SysmanCapture, TelemetryError>`

- `struct StubTelemetryProbe` — contains `next_timestamp_us: core::cell::Cell<u64>` for monotonic timestamp simulation.
  - `fn new() -> Self` — const fn, timestamp starts at 0.
  - `impl TelemetryProbe for StubTelemetryProbe` — `capture` advances the timestamp by 1000 µs per call, maps each metric through the private `stub_value` function.

- `fn stub_value(metric: SysmanMetric) -> f64` — private const fn; returns canonical Arc A770 operating values:
  - PowerEnergyCounter: 100_000.0 mJ, PowerLimits: 225.0 W, TemperatureCurrent: 55.0 °C, TemperatureMaxRange: 95.0 °C, FrequencyCurrent: 1800.0 MHz, FrequencyRange: 2100.0 MHz, FrequencyOverclock: 0.0, EngineActivity: 1_000_000.0 µs, RasEvents: 0.0, ProcessList: 1.0, PerformanceFactor: 100.0.

**Test module `tests` (sysman.rs):**
- `fn metric_names()` — spot-checks PowerEnergyCounter and TemperatureCurrent strings.
- `fn metric_category_maps()` — PowerEnergyCounter→Power, FrequencyCurrent→Frequency, RasEvents→Ras.
- `fn metric_all_count()` — ALL_METRICS.len() == 11.
- `fn full_r18_has_all_metrics()` — full_r18() has 11 metrics, all present.
- `fn advisory_has_subset()` — advisory() has 3, RasEvents absent.
- `fn stub_probe_captures_declared_metrics()` — advisory set → 3 samples, device_index == 0.
- `fn stub_probe_advances_timestamp()` — second capture has higher timestamp.
- `fn stub_captures_arc_canonical_tdp()` — PowerLimits value == 225.0 W (matches specs/10).

**Correctness note:** `stub_value` is declared `const fn` but returns `f64`. In stable Rust, `const fn` returning `f64` is valid (float is allowed in const context since Rust 1.72). However, the `match` body inside `stub_value` at `sysman.rs:278` contains float literals. This works fine in current Rust but is worth noting for future const-evaluation constraints.

---

## 4. Crate: `cssl-host-d3d12`

**Path:** `compiler-rs/crates/cssl-host-d3d12/`  
**Description in Cargo.toml:** "CSSLv3 stage0 — D3D12 host submission via windows-rs (FFI)"

**Purpose:** Catalogs the Direct3D 12 adapter and feature capability surface. Covers DXGI adapter identification (from `IDXGIAdapter4::GetDesc3`), D3D feature levels (12.0/12.1/12.2), the D3D12 options/features struct (raytracing tier, mesh shaders, wave-matrix, dynamic resources, etc.), and the D3D12 heap/command-list/descriptor-heap type enumerations. References `specs/14_BACKEND.csl § D3D12`. Phase-2 will wire `ID3D12Device`, `ID3D12CommandQueue`, and `IDXGIAdapter4` via the `windows` crate (declared in workspace but absent from this crate's deps at phase-1).

**Cargo.toml dependencies:** `thiserror` only. `windows 0.58` declared at workspace level, **not in this crate's deps**.

**Total LOC:** ~430 (lib.rs 45 + adapter.rs 146 + features.rs 114 + heap.rs 129)

**File list:**
- `src/lib.rs` — crate root, attributes, module declarations, re-exports, `STAGE0_SCAFFOLD`
- `src/adapter.rs` — `FeatureLevel`, `DxgiAdapter`
- `src/features.rs` — `WaveMatrixTier`, `D3d12FeatureOptions`
- `src/heap.rs` — `CommandListType`, `DescriptorHeapType`, `HeapType`

---

### `src/lib.rs` (d3d12) — 45 lines

Same scaffold pattern. Declares `adapter`, `features`, `heap` modules. Re-exports `DxgiAdapter`, `FeatureLevel`, `D3d12FeatureOptions`, `WaveMatrixTier`, `CommandListType`, `DescriptorHeapType`, `HeapType`. `STAGE0_SCAFFOLD` const.

**Test module `scaffold_tests`:**
- `fn scaffold_version_present()` — non-empty check.

---

### `src/adapter.rs` — 146 lines

DXGI adapter identification and D3D feature level.

**Items:**

- `enum FeatureLevel` — `Fl12_0`, `Fl12_1`, `Fl12_2`. Derives `PartialOrd`/`Ord`.
  - `fn dotted(self) -> &'static str` — "12.0" / "12.1" / "12.2".
  - `fn as_u32(self) -> u32` — canonical D3D_FEATURE_LEVEL_* integer: 0xc000 / 0xc100 / 0xc200.
  - `pub const ALL_LEVELS: [Self; 3]` — all three feature levels.
  - `impl fmt::Display` — delegates to `dotted`.

- `struct DxgiAdapter` — `description: String`, `vendor_id: u32`, `device_id: u32`, `sub_sys_id: u32`, `revision: u32`, `dedicated_video_memory: u64`, `dedicated_system_memory: u64`, `shared_system_memory: u64`, `feature_level: FeatureLevel`, `is_software: bool`.
  - `fn stub_arc_a770() -> Self` — A770 record: vendor 0x8086, device 0x56A0, revision 0x08, 16 GiB dedicated video memory, 16 GiB shared system memory, FeatureLevel::Fl12_2, is_software=false.
  - `fn stub_warp() -> Self` — Microsoft Basic Render Driver (vendor 0x1414, device 0x008c), 2 GiB shared system memory, FeatureLevel::Fl12_1, is_software=true.

**Test module `tests` (adapter.rs):**
- `fn feature_level_dotted()` — Fl12_0 → "12.0", Fl12_2 → "12.2".
- `fn feature_level_integer_monotonic()` — Fl12_2 > Fl12_1 > Fl12_0.
- `fn feature_level_count()` — ALL_LEVELS.len() == 3.
- `fn stub_arc_matches_spec()` — vendor/device/feature_level/dedicated_video_memory correct.
- `fn stub_warp_is_software()` — is_software true, dedicated_video_memory 0.

---

### `src/features.rs` — 114 lines

D3D12 feature options struct and wave-matrix tier.

**Items:**

- `enum WaveMatrixTier` — `NotSupported`, `Tier1`, `Tier1_0`. Mirrors `D3D12_WAVE_MATRIX_TIER_*`.
  - `fn as_str(self) -> &'static str` — "not-supported" / "tier1" / "tier1.0".

- `struct D3d12FeatureOptions` — 10 fields: `raytracing_tier_1_1: bool`, `mesh_shader_tier_1: bool`, `sampler_feedback: bool`, `vrs_tier_2: bool`, `atomic_int64: bool`, `shader_fp16: bool`, `shader_int16: bool`, `dynamic_resources: bool`, `wave_matrix: WaveMatrixTier`, `wave_size_specialization: bool`.
  - `fn none() -> Self` — all-false, wave_matrix=NotSupported (const fn).
  - `fn arc_a770() -> Self` — all booleans true, wave_matrix=Tier1, wave_size_specialization=true (const fn). Represents the Alchemist D3D12 ISV driver capability profile.

**Test module `tests` (features.rs):**
- `fn wave_matrix_names()` — NotSupported and Tier1 strings.
- `fn none_all_off()` — all false, WaveMatrixTier::NotSupported.
- `fn arc_a770_enables_rt_and_mesh()` — raytracing_tier_1_1, mesh_shader_tier_1, dynamic_resources true, wave_matrix=Tier1.

---

### `src/heap.rs` — 129 lines

D3D12 heap, command-list, and descriptor-heap type enumerations.

**Items:**

- `enum CommandListType` — 7 variants: `Direct`, `Compute`, `Copy`, `Bundle`, `VideoDecode`, `VideoProcess`, `VideoEncode`. Mirrors `D3D12_COMMAND_LIST_TYPE`.
  - `fn as_str(self) -> &'static str` — lowercase hyphenated names.
  - `pub const ALL_TYPES: [Self; 7]` — exhaustive array.

- `enum DescriptorHeapType` — 4 variants: `CbvSrvUav`, `Sampler`, `Rtv`, `Dsv`. Mirrors `D3D12_DESCRIPTOR_HEAP_TYPE`.
  - `fn as_str(self) -> &'static str` — lowercase hyphenated names.

- `enum HeapType` — 4 variants: `Default`, `Upload`, `Readback`, `Custom`. Mirrors `D3D12_HEAP_TYPE`.
  - `fn as_str(self) -> &'static str` — lowercase names.

**Test module `tests` (heap.rs):**
- `fn command_list_type_count()` — ALL_TYPES.len() == 7.
- `fn command_list_type_names()` — Direct → "direct", VideoEncode → "video-encode".
- `fn descriptor_heap_type_names()` — CbvSrvUav, Sampler.
- `fn heap_type_names()` — Default, Upload, Readback.

---

## 5. Crate: `cssl-host-metal`

**Path:** `compiler-rs/crates/cssl-host-metal/`  
**Description in Cargo.toml:** "CSSLv3 stage0 — Metal host submission (cfg-gated macOS / iOS) (FFI)"

**Purpose:** Catalogs the Metal GPU-family / feature-set / heap-type surface for Apple platforms. Metal is relevant to the CSSLv3 portability story as the macOS/iOS target. References `specs/14_BACKEND.csl § Metal`. The `metal` crate is Apple-platform-only; the workspace Cargo.toml has a comment "metal crate mac-only : added per-target-cfg @ T10" but the actual `[target.'cfg(target_os = "macos")'.dependencies]` section is absent at stage-0 — this crate has no `metal` dep at all and compiles on all platforms.

**Cargo.toml dependencies:** `thiserror` only. No `metal` crate dependency anywhere in this crate's Cargo.toml.

**Total LOC:** ~414 (lib.rs 44 + device.rs 162 + feature_set.rs 125 + heap.rs 87)

**File list:**
- `src/lib.rs` — crate root, attributes, module declarations, re-exports, `STAGE0_SCAFFOLD`
- `src/device.rs` — `GpuFamily`, `MtlDevice`
- `src/feature_set.rs` — `MetalFeatureSet`
- `src/heap.rs` — `MetalHeapType`, `MetalResourceOptions`

**Divergence note:** This crate targets macOS/iOS conceptually but its stub devices include an "Intel Mac Pro" stub (`stub_intel_mac`) that has the device name "AMD Radeon Pro W6800X" — which is not an Intel device but an AMD eGPU. The function name `stub_intel_mac` is misleading; the device it represents is an AMD Radeon on an Intel Mac. This is a naming inconsistency (not a runtime bug at stage-0, but will confuse contributors). (`device.rs:116-127`)

---

### `src/lib.rs` (metal) — 44 lines

Same scaffold. Declares `device`, `feature_set`, `heap` modules. Re-exports `GpuFamily`, `MtlDevice`, `MetalFeatureSet`, `MetalHeapType`, `MetalResourceOptions`. `STAGE0_SCAFFOLD` const.

**Test module `scaffold_tests`:**
- `fn scaffold_version_present()` — non-empty check.

---

### `src/device.rs` (metal) — 162 lines

Metal GPU-family and device record.

**Items:**

- `enum GpuFamily` — 14 variants: `Apple1` through `Apple9`, `Mac1`, `Mac2`, `Common1`, `Common2`, `Common3`. Derives `PartialOrd`/`Ord` (enum declaration order, so Apple1 < Apple2 < ... < Common3).
  - `fn as_str(self) -> &'static str` — lowercase name ("apple7", "mac2", etc.).
  - `pub const ALL_FAMILIES: [Self; 14]` — exhaustive array.

- `struct MtlDevice` — `name: String`, `registry_id: u64`, `supports_raytracing: bool`, `supports_function_pointers: bool`, `supports_dynamic_libraries: bool`, `max_buffer_length: u64`, `has_unified_memory: bool`, `gpu_family: GpuFamily`.
  - `fn stub_m3_max() -> Self` — Apple M3 Max: registry_id=1, all three supports_* true, max_buffer_length=128 GiB, has_unified_memory=true, GpuFamily::Apple9.
  - `fn stub_intel_mac() -> Self` — **misleadingly named**: returns a device named "AMD Radeon Pro W6800X", registry_id=2, all supports_* false, max_buffer_length=8 GiB, has_unified_memory=false, GpuFamily::Mac1. Represents a discrete eGPU on an Intel Mac chassis, not an Intel GPU.

**Test module `tests` (device.rs):**
- `fn gpu_family_names()` — Apple7 and Mac2.
- `fn gpu_family_count()` — ALL_FAMILIES.len() == 14.
- `fn m3_max_has_raytracing_and_unified_memory()` — supports_raytracing, supports_function_pointers, has_unified_memory true, GpuFamily::Apple9.
- `fn intel_mac_no_raytracing()` — !supports_raytracing, !has_unified_memory, GpuFamily::Mac1.

---

### `src/feature_set.rs` — 125 lines

Metal feature-set enumeration with capability predicates.

**Items:**

- `enum MetalFeatureSet` — 7 variants: `MacOsGpuFamily1V1`, `MacOsGpuFamily2V1`, `IosGpuFamily6`, `Metal3Apple7`, `Metal3_1Apple8`, `Metal3_1Apple9`, `Metal3_2`. Derives `PartialOrd`/`Ord`.
  - `fn as_str(self) -> &'static str` — dotted canonical name (e.g. "metal3.1.apple9").
  - `fn supports_raytracing(self) -> bool` — true for Metal3Apple7 and above (const fn).
  - `fn supports_mesh_shaders(self) -> bool` — true for Metal3Apple7 and above (const fn). Same predicate as raytracing — both gated at Metal 3 baseline.
  - `fn supports_cooperative_matrix(self) -> bool` — true only for Metal3_1Apple8 and above (const fn). Correctly excludes Metal3Apple7 (A14/M1 does not have cooperative-matrix hardware).
  - `pub const ALL_FEATURE_SETS: [Self; 7]` — exhaustive array.
  - `impl fmt::Display` — delegates to `as_str`.

**Test module `tests` (feature_set.rs):**
- `fn feature_set_count()` — ALL_FEATURE_SETS.len() == 7.
- `fn feature_set_names()` — Metal3_1Apple9 and Metal3_2 strings.
- `fn metal3_supports_raytracing()` — Metal3Apple7, Metal3_1Apple9, Metal3_2 all true.
- `fn pre_metal3_no_raytracing()` — MacOsGpuFamily1V1, IosGpuFamily6 false.
- `fn only_apple8_plus_has_coop_matrix()` — Metal3Apple7 false, Metal3_1Apple8/Apple9 true.
- `fn mesh_shaders_metal3_plus()` — Metal3Apple7 true, MacOsGpuFamily1V1 false.

---

### `src/heap.rs` (metal) — 87 lines

Metal storage-mode and resource-options enumeration.

**Items:**

- `enum MetalHeapType` — 4 variants: `Shared`, `Private`, `Managed`, `Memoryless`. Mirrors `MTLStorageMode`.
  - `fn as_str(self) -> &'static str` — lowercase names ("shared", "private", "managed", "memoryless").

- `struct MetalResourceOptions` — `hazard_tracked: bool`, `cpu_cache_mode_default: bool`, `storage_mode: MetalHeapType`.
  - `fn default_shared() -> Self` — hazard_tracked=true, cpu_cache_mode_default=true, storage_mode=Shared (const fn).
  - `fn gpu_private() -> Self` — hazard_tracked=true, cpu_cache_mode_default=true, storage_mode=Private (const fn).

**Test module `tests` (heap.rs):**
- `fn heap_type_names()` — all four short names.
- `fn default_shared_options()` — storage_mode=Shared, hazard_tracked=true.
- `fn gpu_private_options()` — storage_mode=Private.

---

## 6. Crate: `cssl-host-webgpu`

**Path:** `compiler-rs/crates/cssl-host-webgpu/`  
**Description in Cargo.toml:** "CSSLv3 stage0 — WebGPU host submission via wgpu"

**Purpose:** Catalogs the WebGPU adapter/backend/feature/limits surface. WebGPU is a cross-platform GPU API primarily targeting browsers (via Dawn/Chromium) but also available as a native library via `wgpu` (Rust). The crate references `specs/14_BACKEND.csl § WebGPU` and `specs/07_CODEGEN.csl § GPU BACKEND — WGSL path`. Because `wgpu` can dispatch to Vulkan, Metal, or D3D12 under the hood, this crate provides an abstraction that covers all three native backends plus the browser path.

**Cargo.toml dependencies:** `thiserror` only. `wgpu 23` declared in workspace but **absent from this crate's deps**. `naga 23` also present workspace-wide (for WGSL parse/validate) but similarly not listed here.

**Total LOC:** ~468 (lib.rs 43 + adapter.rs 177 + features.rs 250)

**File list:**
- `src/lib.rs` — crate root, attributes, module declarations, re-exports, `STAGE0_SCAFFOLD`
- `src/adapter.rs` — `WebGpuBackend`, `AdapterPowerPref`, `WebGpuAdapter`
- `src/features.rs` — `WebGpuFeature`, `SupportedFeatureSet`, `WebGpuLimits`

---

### `src/lib.rs` (webgpu) — 43 lines

Same scaffold. Declares `adapter`, `features` modules. Re-exports `AdapterPowerPref`, `WebGpuAdapter`, `WebGpuBackend`, `SupportedFeatureSet`, `WebGpuFeature`, `WebGpuLimits`. `STAGE0_SCAFFOLD` const.

**Test module `scaffold_tests`:**
- `fn scaffold_version_present()` — non-empty check.

---

### `src/adapter.rs` (webgpu) — 177 lines

WebGPU backend, power preference, and adapter record.

**Items:**

- `enum WebGpuBackend` — 5 variants: `Browser`, `Vulkan`, `Metal`, `Dx12`, `Gl`.
  - `fn as_str(self) -> &'static str` — lowercase short name.
  - `pub const ALL_BACKENDS: [Self; 5]` — exhaustive array.
  - `impl fmt::Display` — delegates to `as_str`.

- `enum AdapterPowerPref` — `LowPower`, `HighPerformance`, `NoPreference`. Maps to `wgpu::PowerPreference`.
  - `fn as_str(self) -> &'static str` — "low-power", "high-performance", "no-preference".

- `struct WebGpuAdapter` — `name: String`, `vendor_id: u32`, `device_id: u32`, `backend: WebGpuBackend`, `driver_description: String`, `is_fallback: bool`. **Note:** `vendor_id` documented as "best-effort — 0 on Browser-WebGPU" because browsers fingerprint-gate these values.
  - `fn stub_arc_a770_vulkan() -> Self` — A770 via Vulkan passthrough: vendor 0x8086, device 0x56A0, backend=Vulkan, driver_description="Mesa ANV / Intel ISV 32.0.101.8629", is_fallback=false.
  - `fn stub_browser_webgpu() -> Self` — browser adapter: vendor/device=0, backend=Browser, driver_description="Dawn via Chromium", is_fallback=false.
  - `fn stub_software() -> Self` — software adapter: vendor/device=0, backend=Gl, driver_description="SwiftShader", is_fallback=true.

**Test module `tests` (adapter.rs):**
- `fn backend_names()` — Browser, Vulkan, Dx12 strings.
- `fn backend_count()` — ALL_BACKENDS.len() == 5.
- `fn power_pref_names()` — LowPower, HighPerformance.
- `fn stub_arc_a770_vulkan()` — vendor/device IDs, backend=Vulkan, !is_fallback.
- `fn stub_browser_zeros_vendor_id()` — vendor_id=0, backend=Browser.
- `fn stub_software_marked_fallback()` — is_fallback=true.

---

### `src/features.rs` (webgpu) — 250 lines

WebGPU feature catalog, feature set, and GPU limits snapshot.

**Items:**

- `enum WebGpuFeature` — 14 variants covering the `GPUFeatureName` enum: `DepthClipControl`, `Depth32FloatStencil8`, `TextureCompressionBc`, `TextureCompressionEtc2`, `TextureCompressionAstc`, `TimestampQuery`, `IndirectFirstInstance`, `ShaderF16`, `Rg11b10UfloatRenderable`, `Bgra8UnormStorage`, `Float32Filterable`, `DualSourceBlending`, `ClipDistances`, `Subgroups`. Derives `PartialOrd`/`Ord`.
  - `fn as_str(self) -> &'static str` — canonical WebGPU feature name string (const fn).
  - `pub const ALL_FEATURES: [Self; 14]` — exhaustive array.
  - `impl fmt::Display` — delegates to `as_str`.

- `struct SupportedFeatureSet` — wraps `BTreeSet<WebGpuFeature>`. Parallel design to `VulkanExtensionSet` and `SysmanMetricSet`.
  - `fn new() -> Self` — empty.
  - `fn add(&mut self, f: WebGpuFeature)` — insert.
  - `fn contains(&self, f: WebGpuFeature) -> bool` — membership.
  - `fn iter(&self) -> impl Iterator<Item = WebGpuFeature> + '_` — sorted copy-iter.
  - `fn len(&self) -> usize` — size.
  - `fn is_empty(&self) -> bool` — empty check.
  - `impl FromIterator<WebGpuFeature>` — from_iter construction.
  - `impl Default` — via `#[derive(Default)]`.

- `struct WebGpuLimits` — 26 `u32`/`u64` fields representing `GPUSupportedLimits`. All fields public: `max_texture_dimension_1d/2d/3d`, `max_texture_array_layers`, `max_bind_groups`, `max_bindings_per_bind_group`, `max_dynamic_uniform_buffers_per_pipeline_layout`, `max_dynamic_storage_buffers_per_pipeline_layout`, `max_sampled_textures_per_shader_stage`, `max_samplers_per_shader_stage`, `max_storage_buffers_per_shader_stage`, `max_storage_textures_per_shader_stage`, `max_uniform_buffers_per_shader_stage`, `max_uniform_buffer_binding_size`, `max_storage_buffer_binding_size`, `max_vertex_buffers`, `max_buffer_size: u64`, `max_vertex_attributes`, `max_vertex_buffer_array_stride`, `max_inter_stage_shader_components`, `max_compute_workgroup_storage_size`, `max_compute_invocations_per_workgroup`, `max_compute_workgroup_size_x/y/z`, `max_compute_workgroups_per_dimension`.
  - `fn webgpu_default() -> Self` — const fn; returns the canonical WebGPU spec required-defaults table values. Key values: max_texture_dimension_2d=8192, max_bind_groups=4, max_compute_invocations_per_workgroup=256, max_buffer_size=256 MiB, max_compute_workgroups_per_dimension=65535.
  - `impl Default` — delegates to `webgpu_default()`.

**Test module `tests` (features.rs):**
- `fn feature_count()` — ALL_FEATURES.len() == 14.
- `fn feature_names()` — TimestampQuery, ShaderF16, TextureCompressionBc strings.
- `fn feature_set_ops()` — add/contains/len, Subgroups absent.
- `fn webgpu_default_limits_have_canonical_values()` — spot-checks four limit values against the WebGPU spec required-defaults table.

---

## 7. Slice Notes

### Test Coverage

All five crates have inline `#[cfg(test)]` modules only — no external integration test files. Total test count is approximately:

| Crate | Test functions |
|---|---|
| `cssl-host-vulkan` | ~26 (device.rs:8 + extensions.rs:5 + arc_a770.rs:5 + probe.rs:4 + lib.rs:1) |
| `cssl-host-level-zero` | ~15 (driver.rs:3 + api.rs:3 + sysman.rs:8 + lib.rs:1) |
| `cssl-host-d3d12` | ~12 (adapter.rs:5 + features.rs:3 + heap.rs:3 + lib.rs:1) |
| `cssl-host-metal` | ~12 (device.rs:4 + feature_set.rs:6 + heap.rs:2 + lib.rs:1 [sic, but the test is 1]) |
| `cssl-host-webgpu` | ~10 (adapter.rs:6 + features.rs:4 + lib.rs:1) |

Coverage is proportional to complexity. The Vulkan crate is by far the most thoroughly tested. The D3D12, Metal, and WebGPU crates are lighter on tests because their types are simpler enumeration catalogs.

### What is Incomplete / Stubbed

All five crates are explicitly Phase-1 scaffolds. The deferred Phase-2 items documented in each `lib.rs` are:

**Vulkan:**
- `ash`-backed `VkInstance` / `VkPhysicalDevice` / `VkDevice` creation (blocked on MSVC toolchain, T1-D7)
- Extension-request arbitration (required vs. optional)
- `vkAllocateDescriptorSet`, `vkUpdateDescriptorSet` for bindless
- `VkPipeline` via SPIR-V from `cssl-cgen-gpu-spirv`
- `VkCommandBuffer` recording, `vkQueueSubmit`, `vkQueuePresentKHR`
- Validation-layer routing

**Level-Zero:**
- `level-zero-sys` crate integration (crates.io availability to be verified)
- `ze_driver_handle_t` / `ze_device_handle_t` / `ze_command_list_t` lifetimes
- SPIR-V `ze_module_t` + `ze_kernel_t` dispatch
- USM allocation (host/device/shared)
- Actual sysman property sampling (`zesPowerGetEnergyCounter` etc.)
- Multi-device / multi-context concurrency

**D3D12:**
- `ID3D12Device` / `ID3D12CommandQueue` / `IDXGIAdapter4` via windows crate
- (rest undocumented in this crate's lib.rs)

**Metal:**
- `metal` crate `cfg`-gated FFI (macOS/iOS/tvOS/visionOS)
- (rest undocumented in this crate's lib.rs)

**WebGPU:**
- `wgpu::Adapter` / `wgpu::Device` / `wgpu::Queue` wiring

### Real FFI vs. Mocks

All five crates are pure mocks at stage-0. None make any OS or GPU API calls. The ranking from most-wired to least-wired:

1. **Vulkan** — most complete catalog; `ash` declared in workspace ready to wire.
2. **D3D12** — solid catalog; `windows` declared in workspace ready to wire.
3. **WebGPU** — solid catalog; `wgpu` + `naga` in workspace ready to wire.
4. **Metal** — complete catalog; `metal` crate noted in workspace comment but not even listed as a dependency — will require `[target.'cfg(target_os = "macos")'.dependencies]` addition.
5. **Level-Zero** — `level-zero-sys` is **commented out** in the workspace, so even declaring the dep will require uncommenting and verifying crates.io availability before phase-2 can proceed.

### Bug / Correctness Findings

1. **`extensions.rs:14` doc count mismatch:** `lib.rs:15` says "30-variant catalog" but `VulkanExtension` has **31 variants** (counted: KhrSwapchain through ExtCalibratedTimestamps inclusive). Minor but misleading.

2. **`probe.rs:110-117` misleading comment:** The test `has_extension_returns_false_for_absent` claims in its comment "pick one that's NOT [in the profile]" but immediately calls `has_extension(0, VulkanExtension::KhrRayQuery)` which **is** in the Arc A770 expected extension set. The test asserts `true` and passes, but the test name and comment promise false-path coverage that is never exercised. There is no test asserting `has_extension` returns `false` for an extension genuinely absent from the stub profile.

3. **`device.rs:116-127` naming inconsistency in Metal:** `stub_intel_mac()` returns a device named "AMD Radeon Pro W6800X". The stub models an AMD eGPU attached to an Intel Mac chassis, which is a valid platform configuration, but the function name implies an Intel GPU. A contributor reading `stub_intel_mac` will expect an Intel GPU device record.

4. **`sysman.rs:278` const fn with float literals:** `stub_value` is `const fn -> f64`. This works on current stable Rust (1.72+), but the function is effectively uncallable in const contexts (it returns `f64` not a `const`-context-usable value through the `const fn` path). This is not a bug but is an unusual pattern — `const fn` for a function that can only be called at runtime.

5. **`adapter.rs:85` (D3D12) `shared_system_memory` value:** `stub_arc_a770()` sets `shared_system_memory: 16 * 1024 * 1024 * 1024` (16 GiB). On Windows, `IDXGIAdapter4::GetDesc3` reports `SharedSystemMemory` as the portion of system RAM accessible to the GPU — this is typically the total system RAM, not VRAM. Reporting 16 GiB shared matches the VRAM figure, not a typical system RAM value. On a system with 32 GiB RAM, a real adapter would report ~32 GiB here. This is a spec-accuracy divergence in the stub that could mislead tests comparing shared_system_memory to VRAM amounts.

### README / Spec Divergences

- No `README.md` files exist in any of these five crates. Not a functional issue but makes discoverability harder for contributors.
- All five crates cite their spec authority correctly (`specs/10_HW.csl`, `specs/14_BACKEND.csl`, `specs/22_TELEMETRY.csl`) in their `lib.rs` module doc.
- The Vulkan crate's extension catalog (29 extensions in the A770 expected set) is consistent with the `specs/10 § VULKAN 1.4 BASELINE` reference; `ExtConservativeRasterization` and `KhrGlobalPriority` are catalogued as enum variants but not included in the A770 expected set, which matches the real Intel ISV driver behavior (conservative rasterization not exposed on Alchemist).
- The D3D12 `stub_arc_a770()` correctly uses `FeatureLevel::Fl12_2`, matching the A770's D3D12 capability per specs/10.
- The Level-Zero stub correctly encodes L0 API version 1.14 for the A770, consistent with the Intel L0 API roadmap.

### Dead Code

No dead code found. Every public item is either re-exported from `lib.rs` or reachable from a test. Every private item (`expected_features()` in arc_a770.rs, `stub_value()` in sysman.rs) is called from a public function or test.

### Surprises

- The WebGPU `SupportedFeatureSet` and the Vulkan `VulkanExtensionSet` and the Level-Zero `SysmanMetricSet` all have **identical structural design** (BTreeSet wrapper with add/contains/iter/len/is_empty/FromIterator). This is consistent and correct but is a candidate for a generic abstraction in a later refactor.
- The `StubTelemetryProbe` uses `core::cell::Cell<u64>` for interior mutability of the timestamp counter — an elegant choice for a `&self`-taking trait method that needs to mutate state, avoiding `RefCell`'s overhead. The field is `pub`, meaning external code can read or reset the counter, which could cause flaky tests if the probe instance is shared. Currently no test shares a single probe across concurrent threads, so this is low-risk at stage-0.

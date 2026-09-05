# GPU Code-Generation Backend Audit

**Slice:** Four GPU codegen crates downstream of MIR  
**Audited:** 2026-05-14  
**Auditor:** Claude Sonnet 4.6  
**Spec authority:** `specs/07_CODEGEN.csl`, `specs/10_HW.csl`, `specs/14_BACKEND.csl`  
**Total files:** 20 source files + 4 Cargo.toml  
**Total LOC:** 5,035 (source) — SPIR-V 2,123 / DXIL 1,096 / MSL 929 / WGSL 887

---

## 1. SLICE OVERVIEW

These four crates implement the GPU-targeted code-generation layer that sits immediately downstream of the MIR (Mid-level IR). Their collective job is to translate `MirModule` / `SpirvModule` representations into GPU-consumable binary or text for four distinct runtimes: Vulkan/Level-Zero (SPIR-V), Direct3D 12 (DXIL), Metal (MSL), and WebGPU (WGSL).

**Architecture: independent paths, not a funnel through SPIR-V.** Despite `cssl-cgen-gpu-msl` having a module named `spirv_cross`, the three non-SPIR-V backends do **not** lower through the SPIR-V emitter. Each backend accepts a `MirModule` directly and produces its own textual output independently. The `spirv_cross` and `dxc` subprocess invokers are optional validation/transpilation adapters available as a fallback or CI gate, not as mandatory pipeline steps. This is an explicit design choice: the crate descriptions say "via spirv-cross shim" and "via DirectXShaderCompiler shim" to describe the optional subprocess path, not a hard dependency.

**Maturity spectrum:**

- `cssl-cgen-gpu-spirv` — most mature. Two distinct emitters: a text disassembler (`emit.rs`) and a real binary emitter via `rspirv` (`binary_emit.rs`). The binary emitter round-trips through `rspirv::dr::load_words` for structural validation. This is the only backend with a live external crate dependency (`rspirv`) in its production dependencies.
- `cssl-cgen-gpu-dxil` — second most developed. Has a full target-enum tree (shader models SM 6.0–6.8, 15 shader stages), a textual HLSL builder, and a `dxc.exe` subprocess invoker. No external crate dependencies beyond `cssl-mir` and `thiserror`.
- `cssl-cgen-gpu-msl` — comparable to DXIL. Full target-enum tree (MSL 2.0–3.2, 7 stages), a textual MSL builder, and a `spirv-cross --msl` subprocess invoker. No external crate dependencies.
- `cssl-cgen-gpu-wgsl` — comparable to MSL but slightly narrower in stage coverage (only 3 WebGPU stages). The only backend with a real structural validator actually exercised in unit tests: the `naga` crate (`dev-dependency`) is used to parse emitted WGSL text in-process, proving syntactic and structural validity without an external subprocess.

**Shared patterns across all four backends:**

1. Every backend has a `lib.rs` (public re-exports + scaffold version constant + one trivial test), a `target.rs` (enum catalog for the backend's version/stage space), an `emit.rs` (MIR → text emitter, accepts a named entry point, errors on non-empty bodies at stage-0), and a builder type (`HlslModule`, `MslModule`, `WgslModule`, or `SpirvModule`) in a dedicated file.
2. Every emitter errors on non-empty MIR fn bodies — stage-0 only emits skeleton functions. The comment "T10-phase-2 lowers bodies" appears verbatim in all four.
3. All crates forbid unsafe code and use `thiserror` for error enums.
4. Tests are co-located with source in `#[cfg(test)]` modules; no separate `tests/` directories exist.

---

## 2. CRATE: `cssl-cgen-gpu-spirv`

**Path:** `compiler-rs/crates/cssl-cgen-gpu-spirv/`  
**Description:** `cssl-cgen-gpu-spirv` is the primary GPU backend. It implements SPIR-V binary and text emission for Vulkan 1.4 (primary), Vulkan 1.0–1.3 (legacy catalog), Universal SPIR-V 1.5/1.6, OpenCL-Kernel 2.2 (Level-Zero), and WebGPU profiles. It is the only backend that produces real binary output (`Vec<u32>`) validated at test time via `rspirv`'s parser.

**Cargo.toml dependencies:**
- `cssl-mir` (path dep) — MIR types
- `thiserror` (workspace) — error derives
- `rspirv` (workspace) — pure-Rust SPIR-V builder and parser; the comment on line 13–19 of Cargo.toml explains this is the T11-D34 closure of the T10-phase-2 deferred "rspirv FFI integration" note

**Total LOC:** 2,123

**Files:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 60 | Public re-exports, crate-level doc, scaffold version constant |
| `src/target.rs` | 316 | `SpirvTargetEnv`, `MemoryModel`, `AddressingModel`, `ExecutionModel` enums |
| `src/capability.rs` | 447 | `SpirvCapability` (32 variants), `SpirvExtension` (24 variants), `SpirvCapabilitySet`, `SpirvExtensionSet` |
| `src/module.rs` | 234 | `SpirvSection` (11 variants), `SpirvEntryPoint`, `SpirvModule` builder |
| `src/emit.rs` | 296 | Text (disasm-like) emitter, `emit_module`, `minimal_vulkan_compute_module` helper |
| `src/binary_emit.rs` | 770 | Binary emitter via `rspirv::dr::Builder`, `emit_module_binary`, enum mappers |

---

### 2.1 `src/lib.rs` (60 lines)

This file is the crate root. It applies crate-level lint attributes (`#![forbid(unsafe_code)]`, `#![deny(rustdoc::broken_intra_doc_links)]`, `#![deny(rustdoc::private_intra_doc_links)]`, plus two clippy allows), declares the five public modules, and re-exports the key public API items.

**Items:**

- `pub const STAGE0_SCAFFOLD: &str` — set to `env!("CARGO_PKG_VERSION")`; used by scaffold verification harnesses to assert the crate is linked.
- `mod scaffold_tests` — one test: `scaffold_version_present()` asserts `STAGE0_SCAFFOLD` is non-empty.

**Re-exports (all from sub-modules):**
- `binary_emit::{emit_module_binary, BinaryEmitError}`
- `capability::{SpirvCapability, SpirvCapabilitySet, SpirvExtension, SpirvExtensionSet}`
- `emit::{emit_module, SpirvEmitError}`
- `module::{SpirvModule, SpirvSection}`
- `target::{AddressingModel, ExecutionModel, MemoryModel, SpirvTargetEnv}`

---

### 2.2 `src/target.rs` (316 lines)

Defines the four canonical SPIR-V enum mirrors that correspond directly to SPIR-V specification constructs. All implement `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, and `fmt::Display`.

**Items:**

- `pub enum SpirvTargetEnv` (9 variants) — `VulkanKhr1_0`, `VulkanKhr1_1`, `VulkanKhr1_2`, `VulkanKhr1_3`, `VulkanKhr1_4`, `UniversalSpirv1_5`, `UniversalSpirv1_6`, `OpenClKernel2_2`, `WebGpu`. The WebGpu variant is catalogued here but there is a separate WGSL-emitting crate; this variant exists for the case where WGSL output is produced via the SPIR-V → Tint/naga transpilation path.
  - `pub const fn target_env_str(self) -> &'static str` — returns the spirv-val `--target-env` flag string.
  - `pub const fn default_memory_model(self) -> MemoryModel` — Vulkan for all Vulkan/SPIR-V/WebGPU variants; OpenCL for Level-Zero.
  - `pub const fn default_addressing_model(self) -> AddressingModel` — `PhysicalStorageBuffer64` for Vulkan variants, `Physical64` for OpenCL, `Logical` for WebGPU.
  - `impl fmt::Display for SpirvTargetEnv` — delegates to `target_env_str`.

- `pub enum MemoryModel` (4 variants) — `Simple`, `Glsl450`, `OpenCL`, `Vulkan`.
  - `pub const fn as_str(self) -> &'static str` — canonical disassembler form.
  - `impl fmt::Display`

- `pub enum AddressingModel` (4 variants) — `Logical`, `Physical32`, `Physical64`, `PhysicalStorageBuffer64`.
  - `pub const fn as_str(self) -> &'static str`
  - `impl fmt::Display`

- `pub enum ExecutionModel` (15 variants) — `Vertex`, `TessellationControl`, `TessellationEvaluation`, `Geometry`, `Fragment`, `GlCompute`, `Kernel`, `TaskExt`, `MeshExt`, `RayGenerationKhr`, `IntersectionKhr`, `AnyHitKhr`, `ClosestHitKhr`, `MissKhr`, `CallableKhr`. Covers the full modern SPIR-V catalog including mesh-shader EXT and all DXR KHR ray-tracing stages.
  - `pub const fn as_str(self) -> &'static str` — canonical SPIR-V disassembler form (e.g. `"GLCompute"`, `"RayGenerationKHR"`).
  - `pub const ALL_MODELS: [Self; 15]` — complete catalog in declaration order for iteration.
  - `impl fmt::Display`

**Tests (7):** `target_env_strings`, `vulkan_default_models`, `opencl_default_models`, `webgpu_default_models`, `memory_model_names`, `addressing_model_names`, `execution_model_catalog_complete`, `rt_execution_models_have_khr_suffix`. All are correctness smoke-checks on string representations and model defaults.

---

### 2.3 `src/capability.rs` (447 lines)

Defines the SPIR-V capability and extension catalogs, backed by `BTreeSet` for deterministic (sorted) emission order.

**Items:**

- `pub enum SpirvCapability` (32 variants) — the full set of capabilities CSSLv3 may declare. Notable entries: `AtomicFloat32AddEXT`/`AtomicFloat32MinMaxEXT` (telemetry/histogram kernels per spec comment), `FloatControls2` (tied to F1-AD numerical stability), `CooperativeMatrixKHR`/`CooperativeMatrixNV` (both variants catalogued), `MeshShadingEXT`, `ShaderNonSemanticInfo` (RenderDoc correlation). Derives include `PartialOrd`/`Ord` so the `BTreeSet` sorts by declaration order.
  - `pub const fn as_str(self) -> &'static str` — canonical SPIR-V disassembler name for each variant.
  - `pub const fn requires_extension(self) -> bool` — returns true for 12 variants that need a corresponding `OpExtension`; the remaining 20 are core and need no extension pairing.

- `pub enum SpirvExtension` (24 variants) — the KHR, EXT, INTEL, NV, and NonSemantic extensions. Three variants are ext-inst-set imports rather than plain extensions: `NonSemanticShaderDebugInfo100`, `NonSemanticDebugPrintf`, `GlslStd450`.
  - `pub const fn as_str(self) -> &'static str` — the exact extension string as embedded in `OpExtension` or `OpExtInstImport`.
  - `pub const fn is_ext_inst_set(self) -> bool` — distinguishes the three ext-inst-set imports from the 21 plain extensions.

- `pub struct SpirvCapabilitySet` — wraps `BTreeSet<SpirvCapability>`.
  - `pub fn new() -> Self`
  - `pub fn add(&mut self, c: SpirvCapability)` — inserts into the BTree.
  - `pub fn contains(&self, c: SpirvCapability) -> bool`
  - `pub fn iter(&self) -> impl Iterator<Item = SpirvCapability> + '_` — stable sorted order.
  - `pub fn len(&self) -> usize`
  - `pub fn is_empty(&self) -> bool`
  - `impl FromIterator<SpirvCapability> for SpirvCapabilitySet`
  - `impl Default for SpirvCapabilitySet`

- `pub struct SpirvExtensionSet` — wraps `BTreeSet<SpirvExtension>`.
  - `pub fn new() -> Self`
  - `pub fn add(&mut self, e: SpirvExtension)`
  - `pub fn contains(&self, e: SpirvExtension) -> bool`
  - `pub fn iter_plain(&self) -> impl Iterator<Item = SpirvExtension> + '_` — filters `is_ext_inst_set() == false`.
  - `pub fn iter_ext_inst_sets(&self) -> impl Iterator<Item = SpirvExtension> + '_` — filters `is_ext_inst_set() == true`.
  - `pub fn iter_all(&self) -> impl Iterator<Item = SpirvExtension> + '_` — unfiltered.
  - `pub fn len(&self) -> usize`
  - `pub fn is_empty(&self) -> bool`
  - `impl FromIterator<SpirvExtension> for SpirvExtensionSet`
  - `impl Default for SpirvExtensionSet`

**Notable invariant:** `SpirvCapabilitySet` uses `BTreeSet` which sorts by the derived `Ord` on the enum, which follows declaration order. The test `cap_set_from_iter_is_sorted` explicitly verifies that `Shader < Int64 < Float64` in iteration order, matching their declaration positions in the enum. This is important for deterministic SPIR-V emission.

**Tests (7):** `capability_names`, `capability_requires_extension_shape`, `extension_names`, `ext_inst_set_flag`, `cap_set_ops`, `ext_set_splits_plain_and_ext_inst`, `cap_set_from_iter_is_sorted`.

---

### 2.4 `src/module.rs` (234 lines)

Defines the stage-0 SPIR-V module builder. The central design constraint is enforcing canonical SPIR-V section ordering so the emitter can simply walk sections in array order without sorting.

**Items:**

- `pub enum SpirvSection` (11 variants) — `Capability`, `Extension`, `ExtInstImport`, `MemoryModel`, `EntryPoint`, `ExecutionMode`, `Debug`, `Annotation`, `TypesConstantsGlobals`, `FnDecl`, `FnDef`. Derives `PartialOrd`/`Ord` so the section order can be checked for monotonicity.
  - `pub const fn as_str(self) -> &'static str` — human-readable name for diagnostics.
  - `pub const ALL_SECTIONS: [Self; 11]` — all sections in canonical order; used by emitters to walk in sequence.

- `pub struct SpirvEntryPoint` — derives `Debug`, `Clone`, `PartialEq`, `Eq`.
  - `pub model: ExecutionModel` — the SPIR-V execution model.
  - `pub name: String` — canonical entry-point name; must match the MIR fn name.
  - `pub execution_modes: Vec<String>` — free-form text execution-mode declarations (e.g. `"LocalSize 32 1 1"`). This is the "text representation" that `binary_emit.rs` re-parses to produce real `OpExecutionMode` instructions.

- `pub struct SpirvModule` — derives `Debug`, `Clone`, `PartialEq`, `Eq`. The central builder.
  - `pub target_env: SpirvTargetEnv`
  - `pub memory_model: MemoryModel`
  - `pub addressing_model: AddressingModel`
  - `pub capabilities: SpirvCapabilitySet`
  - `pub extensions: SpirvExtensionSet`
  - `pub entry_points: Vec<SpirvEntryPoint>`
  - `pub source_language: Option<String>` — initialized to `Some("CSSLv3")`.
  - `pub source_version: Option<u32>`
  - `pub fn new(target_env: SpirvTargetEnv) -> Self` — sets `memory_model` and `addressing_model` from `target_env`'s defaults; sets `source_language` to `"CSSLv3"`.
  - `pub fn declare_capability(&mut self, c: SpirvCapability)` — inserts into `capabilities`.
  - `pub fn declare_extension(&mut self, e: SpirvExtension)` — inserts into `extensions`.
  - `pub fn add_entry_point(&mut self, ep: SpirvEntryPoint)` — appends to `entry_points`.
  - `pub fn seed_vulkan_1_4_defaults(&mut self)` — declares `Shader`, `PhysicalStorageBufferAddresses`, `VulkanMemoryModelDeviceScope` capabilities plus `KhrPhysicalStorageBuffer`, `KhrVulkanMemoryModel`, `GlslStd450` extensions. This is the standard starting point for all Vulkan 1.4 modules.
  - `pub fn seed_opencl_kernel_defaults(&mut self)` — declares `Kernel`, `Int64`, `Float64` capabilities plus `IntelFunctionPointers` extension.

**Tests (5):** `all_sections_listed_in_order` (monotonicity via `PartialOrd`), `section_names_are_unique` (no duplicate strings), `new_module_picks_canonical_models`, `seed_vulkan_defaults_adds_expected_caps`, `seed_opencl_defaults_adds_kernel_cap`, `entry_point_push_preserves_order`.

---

### 2.5 `src/emit.rs` (296 lines)

The text (disassembly-like) emitter. Produces a string in the format accepted by `spirv-as`. The output is human-readable and diff-able, but function bodies are stubs.

**Items:**

- `pub enum SpirvEmitError` — derives `Debug`, `Error`, `PartialEq`, `Eq`.
  - `ExtensionNotValidForTarget { extension: String, target_env: String }` — currently never returned (emit.rs does no lax-checking at stage-0 other than the entry-point check).
  - `CapabilityMissingExtension { capability: String }` — currently never returned.
  - `NoEntryPoints { target_env: String }` — returned when a shader-like target has zero entry points.

- `pub fn emit_module(module: &SpirvModule) -> Result<String, SpirvEmitError>` — the main public function. Checks for empty entry points on non-kernel targets, writes a comment banner, then iterates `SpirvSection::ALL_SECTIONS` calling the appropriate section-level helper. Annotation, TypesConstantsGlobals, and FnDecl sections emit placeholder comments. FnDef emits one stub `OpFunction`/`OpLabel`/`OpReturn`/`OpFunctionEnd` block per entry point.

- `fn emit_capabilities(module: &SpirvModule, out: &mut String)` — iterates `module.capabilities.iter()` (stable BTree order), writes `OpCapability <name>` per line.

- `fn emit_extensions(module: &SpirvModule, out: &mut String)` — iterates `module.extensions.iter_plain()`, writes `OpExtension "<str>"`.

- `fn emit_ext_inst_imports(module: &SpirvModule, out: &mut String)` — iterates `module.extensions.iter_ext_inst_sets()`, writes `%<handle> = OpExtInstImport "<str>"` using `ext_inst_handle()`.

- `fn emit_memory_model(module: &SpirvModule, out: &mut String)` — writes one `OpMemoryModel` line.

- `fn emit_entry_points(module: &SpirvModule, out: &mut String)` — writes `OpEntryPoint <model> %<name> "<name>"` per entry.

- `fn emit_execution_modes(module: &SpirvModule, out: &mut String)` — writes `OpExecutionMode %<name> <mode>` for each execution mode string on each entry point.

- `fn emit_debug(module: &SpirvModule, out: &mut String)` — writes `OpSource <lang> <version>` and `OpName %<name> "<name>"` per entry.

- `fn ext_inst_handle(name: &str) -> String` — replaces dots with underscores in an extension name to produce a valid SPIR-V ID token (e.g. `"GLSL.std.450"` → `"GLSL_std_450"`).

- `pub fn minimal_vulkan_compute_module(entry: &str) -> SpirvModule` — public helper that builds a standard Vulkan 1.4 compute module with one named entry point and `LocalSize 1 1 1`. Used extensively by tests in this file and in `binary_emit.rs`.

**Key stub comment (emit.rs:83–84):**
```
// Stage-0 : these sections are empty placeholders ; T10-phase-2 populates them
// when fn-body lowering lands.
```
Applies to `Annotation` and `TypesConstantsGlobals` sections.

**Key stub comment (emit.rs:99):**
```
// stage-0 skeleton — body @ T10-phase-2
```
Appears inside each emitted `OpFunction` stub body.

**Bug / correctness note (emit.rs:92–100):** The `FnDef` section emits `OpFunction <name> None TypeFunction_void__void ; <model>`. The first token after `OpFunction` is the result ID, but here `ep.name` (e.g. `"main_cs"`) is used raw without a `%` sigil, and `TypeFunction_void__void` is a human-readable placeholder not a real SPIR-V ID. This text is **not** valid `spirv-as` input despite the module header claiming compatibility. A `spirv-as` parse of this output would fail on the function section. The binary emitter (`binary_emit.rs`) does not have this problem — it uses real `rspirv` IDs. This is an intentional stage-0 approximation but is worth flagging: the text emitter's output is closer to a debug dump than actual SPIR-V assembly.

**Tests (9):** `shader_module_without_entry_fails`, `kernel_module_without_entry_succeeds`, `minimal_compute_module_emits_all_sections`, `capabilities_are_emitted_before_extensions`, `entry_point_line_shape`, `execution_mode_line_shape`, `memory_model_line_shape`, `ext_inst_import_handle_shape`, `debug_source_line`, `fn_def_stub_per_entry_point`.

---

### 2.6 `src/binary_emit.rs` (770 lines)

The most substantial file in the slice. Implements real SPIR-V binary emission via `rspirv::dr::Builder`, with round-trip validation via `rspirv::dr::load_words` in tests. This is the only file in the four crates that depends on an external non-`cssl-mir` crate at runtime.

**Items:**

- `pub enum BinaryEmitError` — derives `Debug`, `Error`, `PartialEq`, `Eq`.
  - `NoEntryPoints { target_env: String }` — same guard as text emitter, for non-kernel shader targets.
  - `BuilderFailed(String)` — wraps `rspirv` builder errors (e.g. from `begin_function`, `begin_block`, `ret`, `end_function`); in practice only surfaces if the caller passes a malformed `SpirvModule`.

- `pub fn emit_module_binary(module: &SpirvModule) -> Result<Vec<u32>, BinaryEmitError>` — the main public function. Algorithm:
  1. Guard: rejects empty entry points for non-kernel targets.
  2. Creates `rspirv::dr::Builder::new()`, sets version to SPIR-V 1.5 (rationale in comment: broadest consumer acceptance while satisfying Vulkan 1.4 baseline).
  3. Emits `OpCapability` instructions by iterating `module.capabilities.iter()` and calling `map_capability`.
  4. Emits `OpExtension` strings via `b.extension(ext.as_str())` for plain extensions.
  5. Emits `OpExtInstImport` via `b.ext_inst_import(ext.as_str())` for ext-inst-set imports; the result ID is discarded (`let _ = ...`) since `rspirv` tracks it internally.
  6. Emits `OpMemoryModel` via `b.memory_model(map_addressing_model(...), map_memory_model(...))`.
  7. Creates shared `void` and `void()` function types via `b.type_void()` / `b.type_function(void_ty, vec![])`.
  8. For each entry point: calls `b.begin_function / begin_block / ret / end_function` to produce a minimal void function; collects `(fn_id, &SpirvEntryPoint)` pairs.
  9. For each entry point: calls `b.entry_point(map_execution_model(...), fn_id, name, [])`.
  10. For each entry point: calls `emit_execution_modes_for_entry`.
  11. Emits debug: `b.source(SourceLanguage::Unknown, version, None, None::<String>)` and `b.name(fn_id, name)`.
  12. Returns `b.module().assemble()` — rspirv sorts instructions into SPIR-V section order and produces the full binary word stream including the 5-word header (magic + version + generator + bound + schema).

- `fn emit_execution_modes_for_entry(b: &mut Builder, fn_id: u32, ep: &SpirvEntryPoint)` — parses the text execution mode strings stored on `SpirvEntryPoint::execution_modes` and emits real `OpExecutionMode` instructions. Recognizes four modes:
  - `"LocalSize X Y Z"` → `ExecutionMode::LocalSize` with `[x, y, z]` params.
  - `"LocalSizeHint X Y Z"` → `ExecutionMode::LocalSizeHint`.
  - `"OriginUpperLeft"` → `ExecutionMode::OriginUpperLeft` (no params).
  - `"OriginLowerLeft"` → `ExecutionMode::OriginLowerLeft`.
  - Unrecognized modes are **silently skipped** (comment: "T10-phase-2 extends this"). This means that a mis-spelled or unsupported execution mode string on an entry point will be dropped without any error or warning.

- `fn parse_three_u32(s: &str) -> Option<[u32; 3]>` — splits whitespace, parses exactly three `u32` tokens, rejects extra tokens. Returns `None` on any parse failure.

- `fn map_capability(c: SpirvCapability) -> spirv::Capability` — 32-arm match. Two notable arms:
  - `C::FloatControls2 => spirv::Capability::Shader` — `FloatControls2` does not exist in rspirv 0.12's `spirv` crate (which ships the SPIR-V 1.3.268 enum set). The comment explains this maps to `Shader` as a placeholder and a future rspirv bump would expose the real variant. **This is a silent correctness hole**: a module that declares `FloatControls2` will have two `OpCapability Shader` instructions emitted (one from the explicit `Shader` capability if present, one from this mapping), and the `FloatControls2` capability will not actually appear in the binary.
  - `C::ShaderNonSemanticInfo => spirv::Capability::Shader` — same issue; `ShaderNonSemanticInfo` is mapped to `Shader` as a placeholder with the comment "no direct enum". Emitting `ShaderNonSemanticInfo` and `FloatControls2` as `Shader` is semantically wrong; they require their own distinct `OpCapability` opcode values (5433 and 6247 respectively in the SPIR-V spec). Tests do not exercise these two capabilities in the round-trip suite.

- `fn map_memory_model(m: MemoryModel) -> spirv::MemoryModel` — 4-arm match, 1:1 mapping.

- `fn map_addressing_model(a: AddressingModel) -> spirv::AddressingModel` — 4-arm match, 1:1 mapping.

- `fn map_execution_model(e: ExecutionModel) -> spirv::ExecutionModel` — 15-arm match, 1:1 mapping.

- `const _: Option<SpirvExtension> = None` — suppresses dead-code lint on the `SpirvExtension` import; extensions use `.as_str()` directly rather than a mapping function.

**Tests (19):** Organized into three sections.

*Structural invariants:*
- `empty_shader_module_emits_error` — no entry points on Vulkan target → `NoEntryPoints`.
- `empty_kernel_module_emits_ok` — OpenCL-Kernel target, zero entry points is valid.
- `minimal_compute_module_starts_with_magic` — `words[0] == 0x07230203`.
- `minimal_compute_module_version_word_is_1_5` — extracts major/minor from word[1].

*Round-trip via `rspirv::dr::load_words`:*
- `compute_module_round_trips_via_rspirv_loader` — verifies entry-point name survives.
- `compute_module_round_trip_preserves_local_size` — verifies `LocalSize` execution mode survives.
- `vertex_fragment_combo_round_trips` — two-entry-point module with `OriginUpperLeft` fragment mode.
- `compute_module_capabilities_survive_round_trip` — `Shader` and `PhysicalStorageBufferAddresses` checked.
- `compute_module_extensions_survive_round_trip` — `SPV_KHR_physical_storage_buffer` checked.
- `compute_module_ext_inst_import_survives_round_trip` — `GLSL.std.450` import checked.
- `memory_model_survives_round_trip` — both addressing and memory model operands verified.
- `entry_point_function_has_void_return` — finds `OpTypeVoid` ID and checks function return type.
- `name_debug_instruction_points_to_function` — `OpName` for `"main_cs"` survives.
- `three_entry_points_round_trip_cleanly` — VS + FS + CS combo.

*Enum-mapping coverage:*
- `all_15_execution_models_map_without_panic`
- `all_4_memory_models_map_without_panic`
- `all_4_addressing_models_map_without_panic`
- `capability_catalog_round_trips_for_shader_like` — 9 capabilities in one module.
- `capability_ext_inst_and_plain_ext_coexist` — counts plain extensions (2) vs ext-inst imports (1).
- `map_capability_smoke_all_variants` — spot-checks four variants.

*Parse helper:*
- `parse_three_u32_happy_path`
- `parse_three_u32_wrong_arity_rejects`
- `parse_three_u32_non_numeric_rejects`

---

## 3. CRATE: `cssl-cgen-gpu-dxil`

**Path:** `compiler-rs/crates/cssl-cgen-gpu-dxil/`  
**Description:** The Direct3D 12 / DXIL backend. It emits HLSL source text from a `MirModule` and optionally invokes `dxc.exe` as a subprocess to produce a real DXIL binary. The DXC subprocess is entirely optional — if the binary is not on PATH, emission still returns the HLSL text and `DxcOutcome::BinaryMissing` is treated as non-fatal. No `windows-rs` / COM interfaces are used; that is explicitly T10-phase-2 deferred.

**Cargo.toml dependencies:**
- `cssl-mir` (path dep)
- `thiserror` (workspace)
- No external GPU/shader dependencies at all.

**Total LOC:** 1,096

**Files:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 58 | Public re-exports, scaffold constant |
| `src/target.rs` | 422 | `ShaderModel`, `ShaderStage`, `HlslProfile`, `RootSignatureVersion`, `DxilTargetProfile` |
| `src/hlsl.rs` | 213 | `HlslStatement`, `HlslModule` builder |
| `src/emit.rs` | 213 | MIR → HLSL text emitter |
| `src/dxc.rs` | 190 | `DxcCliInvoker`, `DxcInvocation`, `DxcOutcome` subprocess adapter |

---

### 3.1 `src/lib.rs` (58 lines)

Same pattern as SPIR-V `lib.rs`: lint attributes, module declarations, re-exports, `STAGE0_SCAFFOLD` constant, one trivial test.

**Re-exports:**
- `dxc::{DxcCliInvoker, DxcInvocation, DxcOutcome}`
- `emit::{emit_hlsl, DxilError}`
- `hlsl::{HlslModule, HlslStatement}`
- `target::{DxilTargetProfile, HlslProfile, RootSignatureVersion, ShaderModel, ShaderStage}`

---

### 3.2 `src/target.rs` (422 lines)

The most substantial file in this crate, covering the full HLSL/DXIL target space.

**Items:**

- `pub enum ShaderModel` (9 variants, derives `PartialOrd`/`Ord`) — `Sm60` through `Sm68`. Covers SM 6.0 (DXIL baseline) through SM 6.8 (cooperative-matrix + work-graphs).
  - `pub const fn profile_suffix(self) -> &'static str` — underscore form like `"6_6"` for use in profile strings.
  - `pub const fn dotted(self) -> &'static str` — dot form like `"6.6"` for display.
  - `pub const ALL_MODELS: [Self; 9]`
  - `impl fmt::Display` — uses `dotted`.

- `pub enum ShaderStage` (15 variants) — `Vertex`, `Pixel`, `Compute`, `Geometry`, `Hull`, `Domain`, `Mesh`, `Amplification`, `Lib`, `RayGeneration`, `ClosestHit`, `AnyHit`, `Miss`, `Intersection`, `Callable`. Mirrors `ExecutionModel` in the SPIR-V crate.
  - `pub const fn profile_prefix(self) -> &'static str` — the dxc `-T` prefix: `"vs"`, `"ps"`, `"cs"`, `"gs"`, `"hs"`, `"ds"`, `"ms"`, `"as"`, `"lib"`, `"raygeneration"`, `"closesthit"`, `"anyhit"`, `"miss"`, `"intersection"`, `"callable"`.
  - `pub const fn min_shader_model(self) -> ShaderModel` — shader-model gating: VS/PS/CS/GS/HS/DS require SM 6.0, DXR stages require SM 6.3, Mesh/Amplification require SM 6.5.
  - `pub const ALL_STAGES: [Self; 15]`
  - `impl fmt::Display` — uses `profile_prefix`.

- `pub struct HlslProfile` — `{ pub stage: ShaderStage, pub model: ShaderModel }`. Derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`.
  - `pub fn new(stage: ShaderStage, model: ShaderModel) -> Option<Self>` — returns `None` if `model < stage.min_shader_model()`. This is the stage-model compatibility gate.
  - `pub fn render(self) -> String` — produces `"<prefix>_<suffix>"` like `"cs_6_6"`.
  - `impl fmt::Display` — calls `render`.

- `pub enum RootSignatureVersion` (3 variants) — `V1_0`, `V1_1`, `V1_2`.
  - `pub const fn dotted(self) -> &'static str` — `"1.0"`, `"1.1"`, `"1.2"`.

- `pub struct DxilTargetProfile` — derives `Debug`, `Clone`, `PartialEq`, `Eq`.
  - `pub profile: HlslProfile`
  - `pub root_sig: RootSignatureVersion`
  - `pub wave_size: Option<u32>` — subgroup size hint; `None` = driver default.
  - `pub enable_16_bit_types: bool`
  - `pub enable_dynamic_resources: bool`
  - `pub fn compute_sm66_default() -> Self` — CS @ SM 6.6, root-sig 1.1, 16-bit types, dynamic resources.
  - `pub fn vertex_sm66_default() -> Self`
  - `pub fn pixel_sm66_default() -> Self`
  - `pub fn summary(&self) -> String` — diagnostic string like `"cs_6_6 / rs1.1 / 16bit+dyn-res"`. Has a latent bug: the `wave` flag is pushed to `flags` before the `format!()` call that includes `wave={}`, but if `wave_size` is `Some`, `flags` already contains `"wave"`, meaning the output would contain both `"wave=<N>"` in the main string and `"wave"` in the flags list. This is cosmetic only (diagnostic string) but is a code smell: `flags.push("wave")` at target.rs:313 is within the `if let Some(w)` arm where `wave=<N>` is already in the format string, making the `flags` entry redundant.

**Tests (12):** `shader_model_profile_suffix`, `shader_model_dotted`, `shader_model_count`, `shader_stage_profile_prefix`, `shader_stage_count`, `stage_min_shader_model`, `hlsl_profile_renders`, `hlsl_profile_rejects_too_low_model`, `root_sig_dotted`, `compute_default_profile`, `vertex_default_profile`, `pixel_default_profile`, `summary_contains_profile_and_rs`.

---

### 3.3 `src/hlsl.rs` (213 lines)

The HLSL source builder. Provides a statement-level representation of an HLSL translation unit.

**Items:**

- `pub enum HlslStatement` (5 variants, derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `CBuffer { name: String, body: String, register: Option<String> }` — renders as `cbuffer Name : register(bN) { body };`.
  - `Struct { name: String, fields: Vec<String> }` — renders as `struct Name { fields };`.
  - `RwBuffer { element_type: String, name: String, register: Option<String> }` — renders as `RWStructuredBuffer<T> name : register(uN);`.
  - `Function { return_type: String, name: String, params: Vec<String>, attributes: Vec<String>, semantic: Option<String>, body: Vec<String> }` — renders attributes first (one per line), then the function signature with optional semantic annotation, then the body indented by 4 spaces.
  - `Raw(String)` — pass-through line.
  - `pub fn render(&self) -> String` — non-consuming; allocates a new `String` for each statement.

- `pub struct HlslModule` (derives `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`):
  - `pub header: Option<String>` — emitted as a comment block before statements.
  - `pub statements: Vec<HlslStatement>`
  - `pub fn new() -> Self`
  - `pub fn push(&mut self, s: HlslStatement)`
  - `pub fn render(&self) -> String` — renders header (with blank line after) then all statements with blank line separators.

**Tests (5):** `struct_statement_rendering`, `function_statement_rendering`, `rw_buffer_statement_rendering`, `cbuffer_statement_rendering`, `module_assembly`.

---

### 3.4 `src/emit.rs` (213 lines)

The MIR-to-HLSL emitter. Takes a `MirModule`, a `DxilTargetProfile`, and an entry-point name.

**Items:**

- `pub enum DxilError` (derives `Debug`, `Error`, `PartialEq`, `Eq`):
  - `EntryPointMissing { entry: String, profile: String }` — the named fn is absent from the MIR module.
  - `BodyNotEmpty { fn_name: String, count: usize }` — stage-0 only emits skeletons; non-empty MIR bodies are rejected.

- `pub fn emit_hlsl(module: &MirModule, profile: &DxilTargetProfile, entry_name: &str) -> Result<HlslModule, DxilError>` — finds the entry fn by name, rejects non-empty bodies, then builds an `HlslModule` with a comment header, an optional `[numthreads(1, 1, 1)]` attribute for compute stages, a skeleton function signature appropriate to the stage, and stubs for all other functions in the module.

- `fn stage_entry_return_type(stage: ShaderStage) -> &'static str` — `"float4"` for VS/PS, `"void"` for CS/Mesh/Amplification, `"void"` for everything else (catch-all).

- `fn stage_entry_params(stage: ShaderStage) -> &'static [&'static str]` — `["uint vid : SV_VertexID"]` for VS, `["float4 pos : SV_Position"]` for PS, `["uint3 tid : SV_DispatchThreadID"]` for CS, `[]` for everything else.

- `fn stage_entry_semantic(stage: ShaderStage) -> Option<&'static str>` — `Some("SV_Position")` for VS, `Some("SV_Target0")` for PS, `None` otherwise.

- `fn synthesize_helper_fn(f: &MirFunc) -> HlslStatement` — produces a `void helper() { // helper fn ... }` stub with a comment listing param/result counts.

**Tests (5):** `missing_entry_point_errors`, `compute_skeleton_has_numthreads`, `vertex_skeleton_has_sv_position_semantic`, `pixel_skeleton_has_sv_target_semantic`, `helper_fns_emitted_as_stubs`, `header_carries_profile_metadata`.

---

### 3.5 `src/dxc.rs` (190 lines)

The `dxc.exe` subprocess adapter. Mirrors the T6-D1 / T9-D1 CLI subprocess pattern.

**Items:**

- `pub struct DxcInvocation` (derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `pub hlsl_text: String`
  - `pub profile: DxilTargetProfile`
  - `pub entry_point: String`
  - `pub extra_args: Vec<String>`

- `pub enum DxcOutcome` (derives `Debug`, `Clone`, `PartialEq`, `Eq`) — 4 variants:
  - `Success { dxil_bytes: Vec<u8>, stderr: String }` — DXC found + compilation succeeded.
  - `DiagnosticFailure { status: i32, stdout: String, stderr: String }` — DXC found but returned non-zero.
  - `BinaryMissing` — `dxc` not on PATH; treated as non-fatal by callers.
  - `IoError(String)` — OS-level failure during subprocess management.

- `pub struct DxcCliInvoker` (derives `Debug`, `Clone`, `Default`):
  - `pub binary_path: Option<PathBuf>` — override path; `None` = use `"dxc"` from PATH.
  - `pub const fn new() -> Self` — no override.
  - `pub fn with_binary(path: PathBuf) -> Self` — explicit override.
  - `pub fn compile(&self, inv: &DxcInvocation) -> DxcOutcome` — builds a `Command` with `-T <profile>`, `-E <entry>`, `-HV 2021`; adds `-enable-16bit-types` if the profile requests it; adds `-DDYNAMIC_RESOURCES` define if dynamic resources are enabled. Feeds `hlsl_text` to stdin. Catches `NotFound` from `cmd.spawn()` and maps it to `BinaryMissing`. Returns `Success` or `DiagnosticFailure` based on exit status.

**Tests (4):** `new_invoker_has_no_binary_override`, `with_binary_stores_path`, `default_matches_new`, `outcome_variants_construct`, `compile_with_missing_binary_does_not_panic`.

**Note on `-spirv` flag:** The lib.rs doc comment mentions `dxc -spirv` round-trip as a T10-phase-2 item, but the current `compile` method never passes `-spirv`. The method builds a clean non-spirv HLSL-to-DXIL invocation.

---

## 4. CRATE: `cssl-cgen-gpu-msl`

**Path:** `compiler-rs/crates/cssl-cgen-gpu-msl/`  
**Description:** The Metal Shading Language backend. Emits MSL source text directly from `MirModule`. Provides an optional `spirv-cross --msl` subprocess invoker for SPIR-V → MSL transpilation in CI/validation contexts. No FFI, no Apple SDK dependency.

**Cargo.toml dependencies:**
- `cssl-mir` (path dep)
- `thiserror` (workspace)
- No MSL/Metal external dependencies.

**Total LOC:** 929

**Files:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 54 | Public re-exports, scaffold constant |
| `src/target.rs` | 323 | `MslVersion`, `MetalStage`, `MetalPlatform`, `ArgumentBufferTier`, `MslTargetProfile` |
| `src/msl.rs` | 174 | `MslStatement`, `MslModule` builder |
| `src/emit.rs` | 194 | MIR → MSL text emitter |
| `src/spirv_cross.rs` | 184 | `SpirvCrossInvoker`, `SpirvCrossInvocation`, `SpirvCrossOutcome` subprocess adapter |

---

### 4.1 `src/lib.rs` (54 lines)

Standard pattern: lint attributes, module declarations, re-exports, scaffold constant, one test.

**Re-exports:**
- `emit::{emit_msl, MslError}`
- `msl::{MslModule, MslStatement}`
- `spirv_cross::{SpirvCrossInvocation, SpirvCrossInvoker, SpirvCrossOutcome}`
- `target::{ArgumentBufferTier, MetalPlatform, MetalStage, MslTargetProfile, MslVersion}`

---

### 4.2 `src/target.rs` (323 lines)

**Items:**

- `pub enum MslVersion` (8 variants, derives `PartialOrd`/`Ord`) — `V2_0` through `V3_2`. Covers MSL from macOS 10.13 (the 2017 Metal 2 launch) through macOS 15 / iOS 18.
  - `pub const fn dotted(self) -> &'static str` — `"2.0"` through `"3.2"`.
  - `pub const fn underscored(self) -> &'static str` — `"2_0"` through `"3_2"` for identifier use.
  - `pub const ALL_VERSIONS: [Self; 8]`
  - `impl fmt::Display` — uses `dotted`.

- `pub enum MetalStage` (7 variants) — `Vertex`, `Fragment`, `Kernel`, `Object`, `Mesh`, `Tile`, `VisibleFunction`. Note: `Tile` maps to `[[tile]]` which is an Apple-specific tile-shader attribute (Metal 3, iOS-only). `VisibleFunction` maps to `[[visible]]` (Metal 3.1+).
  - `pub const fn attribute(self) -> &'static str` — MSL double-bracket attribute form.
  - `pub const fn min_msl_version(self) -> MslVersion` — version gating: Vertex/Fragment/Kernel at V2_0, Tile at V2_3, Object/Mesh at V2_4, VisibleFunction at V3_1.
  - `pub const ALL_STAGES: [Self; 7]`
  - `impl fmt::Display` — uses `attribute`.

- `pub enum MetalPlatform` (4 variants) — `MacOs`, `IOs`, `TvOs`, `VisionOs`.
  - `pub const fn as_str(self) -> &'static str` — lowercase name.

- `pub enum ArgumentBufferTier` (2 variants) — `Tier1`, `Tier2`. Tier-2 allows nested argument buffers, indirect command buffers, and heap resources.
  - `pub const fn as_str(self) -> &'static str` — `"tier1"` or `"tier2"`.

- `pub struct MslTargetProfile` (derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `pub version: MslVersion`
  - `pub platform: MetalPlatform`
  - `pub stage: MetalStage`
  - `pub argument_buffer_tier: ArgumentBufferTier`
  - `pub fast_math: bool`
  - `pub fn kernel_default() -> Self` — MSL 3.0 / macOS / Kernel / Tier-2 / fast-math.
  - `pub fn vertex_default() -> Self` — MSL 3.0 / macOS / Vertex / Tier-2 / fast-math.
  - `pub fn fragment_default() -> Self` — MSL 3.0 / macOS / Fragment / Tier-2 / fast-math.
  - `pub fn summary(&self) -> String` — human-readable summary.

**Tests (11):** `version_dotted_forms`, `version_underscored_forms`, `version_count`, `stage_attributes`, `stage_count`, `stage_min_version_ordering`, `platform_names`, `argument_buffer_tier_names`, `kernel_default_profile_summary`, `vertex_default_profile`, `fragment_default_profile`.

---

### 4.3 `src/msl.rs` (174 lines)

The MSL source builder.

**Items:**

- `pub enum MslStatement` (6 variants, derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `Include(String)` — renders as `#include <name>`.
  - `UsingNamespace(String)` — renders as `using namespace ns;`.
  - `Struct { name: String, fields: Vec<String> }` — standard struct.
  - `Typedef { existing: String, new: String }` — renders as `typedef existing new;`.
  - `Function { stage_attribute: Option<String>, return_type: String, name: String, params: Vec<String>, body: Vec<String> }` — if `stage_attribute` is `Some`, it is emitted on the line before the function signature.
  - `Raw(String)` — pass-through.
  - `pub fn render(&self) -> String` — allocates a new String per statement.

- `pub struct MslModule` (derives `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`):
  - `pub header: Option<String>`
  - `pub statements: Vec<MslStatement>`
  - `pub fn new() -> Self`
  - `pub fn push(&mut self, s: MslStatement)`
  - `pub fn seed_prelude(&mut self)` — pushes `Include("metal_stdlib")` then `UsingNamespace("metal")`. Called by the emitter for every module.
  - `pub fn render(&self) -> String`

**Tests (6):** `include_statement_renders`, `using_namespace_renders`, `function_with_kernel_attribute_renders`, `struct_renders`, `module_prelude_seeds_stdlib`, `module_header_rendering`.

---

### 4.4 `src/emit.rs` (194 lines)

The MIR-to-MSL emitter.

**Items:**

- `pub enum MslError` (derives `Debug`, `Error`, `PartialEq`, `Eq`):
  - `EntryPointMissing { entry: String, stage: String }`
  - `BodyNotEmpty { fn_name: String, count: usize }`

- `pub fn emit_msl(module: &MirModule, profile: &MslTargetProfile, entry_name: &str) -> Result<MslModule, MslError>` — finds entry fn, rejects non-empty bodies, builds module with header, calls `seed_prelude()`, then emits the entry function using `stage_signature()` to pick return type and params, then emits helper stubs.

- `fn stage_signature(stage: MetalStage) -> (&'static str, &'static [&'static str])` — returns `(return_type, params_slice)`:
  - Kernel: `("void", ["uint3 gid [[thread_position_in_grid]]", "device float* out [[buffer(0)]]"])` — hard-coded `device float*` output buffer.
  - Vertex: `("float4", ["uint vid [[vertex_id]]"])`.
  - Fragment: `("float4", ["float4 pos [[position]]"])`.
  - Object/Mesh/Tile/VisibleFunction: `("void", [])`.

- `fn synthesize_helper(f: &MirFunc) -> MslStatement` — void helper stub.

**Key stub comment (emit.rs:67):** `"// stage-0 skeleton — MIR body lowered @ T10-phase-2"`

**Tests (7):** `missing_entry_point_errors`, `kernel_skeleton_has_kernel_attribute`, `vertex_skeleton_returns_float4`, `fragment_skeleton_has_position_attribute`, `prelude_is_first`, `helper_fns_have_no_stage_attribute`, `header_records_profile`.

---

### 4.5 `src/spirv_cross.rs` (184 lines)

The `spirv-cross --msl` subprocess adapter, structurally parallel to `dxc.rs` in the DXIL crate.

**Items:**

- `pub struct SpirvCrossInvocation` (derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `pub spirv_bytes: Vec<u8>` — SPIR-V binary blob fed to stdin.
  - `pub profile: MslTargetProfile`
  - `pub extra_args: Vec<String>`

- `pub enum SpirvCrossOutcome` (derives `Debug`, `Clone`, `PartialEq`, `Eq`) — 4 variants matching `DxcOutcome` in structure:
  - `Success { msl_text: String, stderr: String }`
  - `DiagnosticFailure { status: i32, stdout: String, stderr: String }`
  - `BinaryMissing`
  - `IoError(String)`

- `pub struct SpirvCrossInvoker` (derives `Debug`, `Clone`, `Default`):
  - `pub binary_path: Option<PathBuf>`
  - `pub const fn new() -> Self`
  - `pub fn with_binary(path: PathBuf) -> Self`
  - `pub fn translate(&self, inv: &SpirvCrossInvocation) -> SpirvCrossOutcome` — builds `Command::new("spirv-cross")` with `--msl`, `--msl-version <ver>`, `--stage <stage>` args. The `--msl-version` argument is computed from `inv.profile.version.underscored().replace('_', "")` concatenated with `"000"` — e.g. MSL 3.0 → `"30000"`. This matches spirv-cross's expected version integer format (major×10000 + minor×100 + patch). The SPIR-V bytes are fed to stdin.

- `fn stage_name(stage: MetalStage) -> &'static str` — maps Metal stages to spirv-cross `--stage` argument:
  - Vertex → `"vert"`, Fragment → `"frag"`, Kernel → `"comp"`, Object|Mesh → `"mesh"`, Tile → `"comp"`, VisibleFunction → `"callable"`.
  - Note: spirv-cross does not have a `"mesh"` stage argument in all versions; mapping `Object` and `Mesh` both to `"mesh"` may fail on older spirv-cross versions. This is a CI-environment concern more than a correctness bug since the subprocess is optional.

**Tests (4):** `new_invoker_no_binary`, `with_binary_stores_path`, `outcome_equality_shapes`, `translate_with_missing_binary_non_panicking`.

---

## 5. CRATE: `cssl-cgen-gpu-wgsl`

**Path:** `compiler-rs/crates/cssl-cgen-gpu-wgsl/`  
**Description:** The WebGPU Shading Language backend. Emits WGSL source text directly from `MirModule`. The most distinguished feature vs the other three backends is that it actually exercises a structural validator (`naga`) in its unit tests, not just text-shape assertions.

**Cargo.toml dependencies:**
- `cssl-mir` (path dep)
- `thiserror` (workspace)
- `naga` (workspace) — dev-dependency only; used in tests to parse emitted WGSL.

**Total LOC:** 887

**Files:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 51 | Public re-exports, scaffold constant |
| `src/target.rs` | 267 | `WebGpuStage`, `WebGpuFeature`, `WgslLimits`, `WgslTargetProfile` |
| `src/wgsl.rs` | 248 | `WgslStatement`, `WgslModule` builder |
| `src/emit.rs` | 321 | MIR → WGSL text emitter, naga-validation tests |

---

### 5.1 `src/lib.rs` (51 lines)

Standard pattern.

**Re-exports:**
- `emit::{emit_wgsl, WgslError}`
- `target::{WebGpuFeature, WebGpuStage, WgslLimits, WgslTargetProfile}`
- `wgsl::{WgslModule, WgslStatement}`

---

### 5.2 `src/target.rs` (267 lines)

The WGSL crate has the narrowest stage catalog (only 3 WebGPU stages) but uniquely introduces hardware-limit encoding.

**Items:**

- `pub enum WebGpuStage` (3 variants) — `Vertex`, `Fragment`, `Compute`.
  - `pub const fn attribute(self) -> &'static str` — `"@vertex"`, `"@fragment"`, `"@compute"`.
  - `pub const ALL_STAGES: [Self; 3]`
  - `impl fmt::Display`

- `pub enum WebGpuFeature` (7 variants, derives `PartialOrd`/`Ord`) — `Float32Filterable`, `ShaderF16`, `TimestampQuery`, `Subgroups`, `DualSourceBlending`, `Bgra8UnormStorage`, `ClipDistances`. Stored in a `BTreeSet<WebGpuFeature>` on the profile for ordered iteration.
  - `pub const fn as_str(self) -> &'static str` — WebGPU feature-name string.

- `pub struct WgslLimits` (derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`):
  - `pub max_workgroup_size_x: u32` (default 256)
  - `pub max_workgroup_size_y: u32` (default 256)
  - `pub max_workgroup_size_z: u32` (default 64)
  - `pub max_workgroup_invocations: u32` (default 256)
  - `pub max_bind_groups: u32` (default 4)
  - `pub max_storage_buffers_per_stage: u32` (default 8)
  - `pub max_storage_textures_per_stage: u32` (default 4)
  - `pub max_uniform_buffers_per_stage: u32` (default 12)
  - `pub const fn webgpu_default() -> Self` — canonical WebGPU default limits.
  - `pub const fn compat() -> Self` — conservative "compat" preset: x/y halved, z halved, bind-groups 2, storage-buffers 4. Useful for targeting a broader device range.
  - `impl Default` — delegates to `webgpu_default()`.

- `pub struct WgslTargetProfile` (derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `pub stage: WebGpuStage`
  - `pub limits: WgslLimits`
  - `pub features: BTreeSet<WebGpuFeature>` — sorted feature set for deterministic enable-directive order.
  - `pub fn compute_default() -> Self` — Compute + default limits + TimestampQuery + ShaderF16.
  - `pub fn vertex_default() -> Self` — Vertex + default limits + no features.
  - `pub fn fragment_default() -> Self` — Fragment + default limits + Float32Filterable.
  - `pub fn summary(&self) -> String` — diagnostic string including stage, max-wg, bind-groups, feature list.

**Tests (9):** `stage_attributes`, `stage_count`, `feature_names`, `webgpu_default_limits`, `compat_limits_lower_than_default`, `compute_default_profile_has_timestamp_query`, `vertex_default_profile`, `fragment_default_profile_has_float32_filterable`, `summary_shape`.

---

### 5.3 `src/wgsl.rs` (248 lines)

The WGSL source builder. The richest statement type set of the four text builders, reflecting WGSL's more annotation-heavy syntax.

**Items:**

- `pub enum WgslStatement` (6 variants, derives `Debug`, `Clone`, `PartialEq`, `Eq`):
  - `Enable(String)` — `enable <name>;`.
  - `Struct { name: String, fields: Vec<String> }` — WGSL struct; fields are rendered with trailing commas (required by WGSL syntax).
  - `Binding { group: u32, binding: u32, address_space: String, access: Option<String>, name: String, ty: String }` — renders as `@group(G) @binding(B) var<space[, access]> name : ty;`.
  - `EntryFunction { stage_attribute: String, workgroup_size: Option<(u32, u32, u32)>, return_type: Option<String>, name: String, params: Vec<String>, body: Vec<String> }` — emits `@stage [@workgroup_size(x,y,z)]` on one line, then `fn name(params) [-> return_type] {`.
  - `HelperFunction { return_type: Option<String>, name: String, params: Vec<String>, body: Vec<String> }` — `fn name(params) [-> return_type] {`.
  - `Raw(String)` — pass-through.
  - `pub fn render(&self) -> String`

- `pub struct WgslModule` (derives `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`):
  - `pub header: Option<String>`
  - `pub statements: Vec<WgslStatement>`
  - `pub fn new() -> Self`
  - `pub fn push(&mut self, s: WgslStatement)`
  - `pub fn render(&self) -> String`

**Tests (8):** `enable_directive_renders`, `struct_renders`, `binding_renders`, `entry_function_compute_renders`, `entry_function_vertex_renders`, `helper_function_renders`, `module_header_rendering`.

---

### 5.4 `src/emit.rs` (321 lines)

The MIR-to-WGSL emitter. The largest file in this crate, partly because of the naga-validation test section.

**Items:**

- `pub enum WgslError` (derives `Debug`, `Error`, `PartialEq`, `Eq`):
  - `EntryPointMissing { entry: String, stage: String }`
  - `BodyNotEmpty { fn_name: String, count: usize }`

- `pub fn emit_wgsl(module: &MirModule, profile: &WgslTargetProfile, entry_name: &str) -> Result<WgslModule, WgslError>` — finds entry fn, rejects non-empty bodies, builds module with comment header, emits `enable f16;` if `ShaderF16` is in features, emits `enable subgroups;` if `Subgroups` is in features, emits the entry function via `stage_signature()`, emits helper stubs.

- `type StageSignature = (Option<&'static str>, &'static [&'static str], Option<(u32, u32, u32)>)` — type alias for the tuple returned by `stage_signature`.

- `fn stage_signature(profile: &WgslTargetProfile) -> StageSignature` — returns `(return_type, params, workgroup_size)`:
  - Compute: `(None, ["@builtin(global_invocation_id) gid : vec3<u32>"], Some((min(max_workgroup_size_x, 64), 1, 1)))` — notably the workgroup X size is clamped to 64 using the limits from the profile, so different `WgslLimits` settings produce different workgroup sizes in the emitted skeleton.
  - Vertex: `(Some("@builtin(position) vec4<f32>"), ["@builtin(vertex_index) vid : u32"], None)`.
  - Fragment: `(Some("@location(0) vec4<f32>"), ["@builtin(position) pos : vec4<f32>"], None)`.

- `fn stage_skeleton_return(stage: WebGpuStage) -> &'static str` — for VS/FS returns `"return vec4<f32>(0.0, 0.0, 0.0, 1.0);"` to produce valid WGSL (a function with a return type must always return). Compute returns `"// no return"`.

- `fn synthesize_helper(f: &MirFunc) -> WgslStatement` — `WgslStatement::HelperFunction` with no return type and empty params.

**Key stub comments:** Same pattern as other backends — `"// stage-0 skeleton — MIR body lowered @ T10-phase-2"` in the body.

**Tests (12) in two sections:**

*Text-shape tests:* `missing_entry_errors`, `compute_skeleton_has_workgroup_size`, `vertex_skeleton_returns_position`, `fragment_skeleton_emits_location_0`, `shader_f16_feature_emits_enable_directive`, `helpers_emitted_without_stage_attribute`, `header_records_profile`.

*Naga validation tests (T11-D32):* These use `naga::front::wgsl::parse_str` to structurally validate emitted WGSL. The test section includes a note explaining that naga 23 does not yet support `enable f16;`, so naga tests use profiles with `ShaderF16` removed:
- `fn naga_compatible_compute_profile() -> WgslTargetProfile` — Compute stage, no features. Private helper.
- `fn naga_compatible_fragment_profile() -> WgslTargetProfile` — Fragment stage, no features. Private helper.
- `naga_validates_compute_skeleton` — parses a compute shader through naga, asserts no error.
- `naga_validates_vertex_skeleton` — uses `vertex_default()` (no F16 features) directly.
- `naga_validates_fragment_skeleton` — uses naga-compatible fragment profile.
- `naga_validates_shader_with_helpers` — compute shader with two helper fns.
- `naga_validated_module_has_entry_point` — full structural check: verifies naga's parsed entry-points list is non-empty and contains a compute entry named `"main_cs"`.

---

## 6. SLICE NOTES

### 6.1 Test coverage summary

| Crate | Tests (approx.) | Validation depth |
|-------|-----------------|-----------------|
| `cssl-cgen-gpu-spirv` | ~40 | Binary round-trip via `rspirv::dr::load_words`; structural checks on the parsed module |
| `cssl-cgen-gpu-dxil` | ~26 | Text-shape assertions only; no binary validation (DXC not available in CI without Windows SDK) |
| `cssl-cgen-gpu-msl` | ~24 | Text-shape assertions only; `spirv-cross` validation is optional/subprocess |
| `cssl-cgen-gpu-wgsl` | ~19 | Text-shape + naga in-process structural parse (strongest of the three non-SPIR-V backends) |

### 6.2 Stubs and deferred work

All four backends share the same fundamental stub: they emit skeleton functions with no real body, and error if the MIR fn has any ops. The canonical deferred label across all is "T10-phase-2". The specific gaps by crate:

**SPIR-V (emit.rs):**
- `Annotation` section (line 82–84): `"; (stage-0 : empty — populated @ T10-phase-2)"`
- `TypesConstantsGlobals` section (same arm): same comment.
- `FnDecl` section (line 86–87): `"; (stage-0 : no fn-decls — T10-phase-2 adds externs)"`
- `FnDef` section (lines 97–100): `"; stage-0 skeleton — body @ T10-phase-2"`

**SPIR-V (binary_emit.rs):**
- Unrecognized execution modes are silently skipped (line 217 comment): `"§ Unrecognized : silent skip at stage-0. T10-phase-2 extends this."`
- `spirv-val` semantic validation is deferred (lines 54–55).
- `spirv-opt` is deferred.

**DXIL (emit.rs:84):** `"// stage-0 skeleton — MIR body lowered at T10-phase-2"`

**MSL (emit.rs:67):** `"// stage-0 skeleton — MIR body lowered @ T10-phase-2"`

**WGSL (emit.rs:75):** `"// stage-0 skeleton — MIR body lowered @ T10-phase-2"`

### 6.3 Actual bugs and correctness issues

**Bug 1 — binary_emit.rs:274 — `FloatControls2` maps to `Shader`:**
```
C::FloatControls2 => spirv::Capability::Shader,
```
Any module that declares `FloatControls2` (e.g. for F1-AD numerical stability per the capability comment) will have this capability silently dropped and an extra `Shader` capability emitted instead. The SPIR-V binary will lack the `FloatControls2` capability entirely, which means the module cannot legally use float-controls instructions. The `spirv-val` subprocess (deferred) would catch this, but the rspirv round-trip test does not because rspirv only checks structural validity, not capability-vs-instruction coherence.

**Bug 2 — binary_emit.rs:277 — `ShaderNonSemanticInfo` maps to `Shader`:**
```
C::ShaderNonSemanticInfo => spirv::Capability::Shader, // placeholder — no direct enum
```
Same category of issue. The `NonSemantic.Shader.DebugInfo.100` and `NonSemantic.DebugPrintf` ext-inst-set imports will be emitted (since they go through `b.ext_inst_import(ext.as_str())`), but the required `ShaderNonSemanticInfo` capability will not be present — instead a duplicate `Shader` capability will be emitted. Per the SPIR-V spec, using `NonSemantic.Shader.DebugInfo.100` without `ShaderNonSemanticInfo` is an error that `spirv-val` would catch.

**Bug 3 — emit.rs (SPIR-V text emitter):92–100 — invalid `spirv-as` output in FnDef section:**
The text emitter emits `OpFunction main_cs None TypeFunction_void__void ; main_cs` where `TypeFunction_void__void` is a human-readable placeholder, not a `%`-prefixed ID. The result ID of `OpFunction` (which should be `%main_cs` or a numeric ID) is also missing the `%` sigil. A `spirv-as` invocation on this text would fail in the `fn-defs` section. The comment "can be fed to `spirv-as`" in the doc-comment of `emit_module` is therefore inaccurate for modules with entry points at stage-0. This is a documentation correctness issue; the binary emitter is the correct path for any real validation.

**Bug 4 — target.rs (DXIL):313 — redundant `flags.push("wave")` in `DxilTargetProfile::summary`:**
```rust
if let Some(w) = self.wave_size {
    flags.push("wave");     // ← this
    format!("... / wave={} / {}", w, flags.join("+"))
}
```
The `flags` vector already accumulates `"16bit"` and `"dyn-res"` before the `if let`. Inside the `Some(w)` arm, `"wave"` is pushed to `flags` and then `flags.join("+")` is included in the format string that already has `"wave={w}"` embedded. The output would look like `"cs_6_6 / rs1.1 / wave=32 / 16bit+dyn-res+wave"` — the `wave` label appears twice, once as `wave=32` and once in the flags suffix. Cosmetic only but is a code smell.

**Bug 5 — spirv_cross.rs:78–81 — MSL version string construction:**
```rust
.arg("--msl-version")
.arg(format!(
    "{}000",
    (inv.profile.version.underscored().replace('_', ""))
))
```
For MSL 3.0, `underscored()` returns `"3_0"`, `replace('_', "")` gives `"30"`, and appending `"000"` gives `"30000"`. spirv-cross expects the version as `major×10000 + minor×100` (e.g. 3.0 = 30000, 3.1 = 30100, 2.4 = 20400). For single-digit minor versions this formula is correct: `"30" + "000" = "30000"` = 3.0 ✓, `"31" + "000" = "31000"` = 3.1 ✓. However for MSL 2.4, `underscored()` = `"2_4"`, `replace` = `"24"`, result = `"24000"` which equals 2.4×10000. This is actually correct numerically. No bug here — the formula works because minor version is always a single digit in the MSL 2.x / 3.x range. A comment explaining the intent would help future maintainers.

### 6.4 Missing features vs spec

Per `specs/07_CODEGEN.csl` (referenced in lib.rs doc-comments), the following are explicitly deferred across all four backends:

- Full MIR `CsslOp` → GPU opcode lowering tables (the entire body-lowering path for F1–F6).
- `spirv-val` subprocess gate for SPIR-V validation (SPIR-V only).
- `spirv-opt` optimizer invocation.
- `NonSemantic.Shader.DebugInfo.100` debug-info emission.
- Structured-CFG emission from `scf.*` / `cssl.region.*` MIR ops.
- Root-signature auto-generation (DXIL).
- Argument-buffer auto-generation (MSL).
- Metal-fn-constants for specialization (MSL).
- `naga` round-trip subprocess (WGSL; currently naga is used in-process in dev-deps only).
- Ray-query / subgroup extension emission for WGSL.
- Real DXIL binary emission via `windows-rs` COM interfaces.

### 6.5 Cross-crate dependency shape

All four crates depend on `cssl-mir` for `MirModule`, `MirFunc`, and related types. No cross-dependency exists between the four GPU backends themselves. `cssl-cgen-gpu-spirv` is the only one with a non-trivial production external dependency (`rspirv`). The other three have no external GPU/shader library dependencies, relying entirely on subprocess adapters for external tool integration.

### 6.6 Code quality observations

- All four crates share `#![forbid(unsafe_code)]` and use `thiserror`. No unsafe code anywhere in the slice.
- BTreeSet is used consistently for capability and feature sets, ensuring deterministic emission order. This is the correct choice for reproducible builds.
- The subprocess adapters (`DxcCliInvoker`, `SpirvCrossInvoker`) share a clean pattern: `NotFound` from `spawn` → `BinaryMissing`, non-zero exit → `DiagnosticFailure`, success → `Success`. Neither panics.
- The `minimal_vulkan_compute_module` helper in `emit.rs` is `pub` (exported), making it accessible for integration tests outside the crate. This is intentional and useful but means it is part of the public API surface.
- `STAGE0_SCAFFOLD` constants appear in all four lib.rs files at the same pattern. They serve as a hook for a scaffold verification harness but the harness itself is not visible in this slice.
- Dead code: `SpirvEmitError::ExtensionNotValidForTarget` and `SpirvEmitError::CapabilityMissingExtension` are defined but never constructed. The emit logic at stage-0 does no capability-vs-target or capability-vs-extension validation. These error arms are pre-declared for T10-phase-2 but are dead code today.
- The `StageSignature` type alias in `wgsl/emit.rs:92–96` is module-private but is a helpful self-documentation device for a complex tuple return type.

### 6.7 Spec divergence notes

- `specs/07_CODEGEN.csl` calls for a `spirv-val` subprocess gate; this is not wired (explicitly deferred).
- The text emitter's `FnDef` stubs are not valid `spirv-as` input despite the module doc-comment implying they are. The binary emitter is the correct validation path.
- `specs/10_HW.csl` § ARC A770 DETAILED SPECS references `SPV_INTEL_subgroup_matrix_multiply_accumulate` for XMX direct-access; this extension is catalogued in `SpirvExtension::IntelSubgroupMatrixMultiplyAccumulate` but no capability or emit path for the corresponding Intel XMX intrinsics exists yet.
- The WGSL crate is described as using a "Tint shim" in its Cargo.toml description; however, there is no Tint subprocess adapter — only the naga dev-dependency. The description is misleading; "Tint shim" appears to be aspirational (T10-phase-2) rather than present.

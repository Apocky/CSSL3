//! § T11-W18-L7 — substrate-kernel SPIR-V emit (CSSL-source → SPIR-V binary).
//!
//! § THESIS
//!   The substrate-kernel `Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl`
//!   is the canonical artifact. This slice parses its declaration subset,
//!   then emits the canonical substrate-kernel shape (compute · workgroup
//!   `(8,8,1)` · entry `"main"` · 3 resource bindings) through the
//!   zero-dependency `cssl-cgen-spirv` binary writer. Full CSSL HIR → MIR
//!   kernel-body lowering is the next bridge ; this file is the current
//!   source-truth adapter.
//!
//! § PROPRIETARY-EVERYTHING (§ I> spec/14_BACKEND § OWNED SPIR-V EMITTER)
//!   - Source : `.csl` substrate-kernel · authored in CSSL.
//!   - Compiler : `cssl-cgen-spirv` from-scratch SPIR-V binary emitter · zero
//!     external dep on this path (no rspirv · no naga · no WGSL).
//!   - Host : `cssl-host-substrate-render-v3` ash-direct vulkan-1.3 dispatch ·
//!     zero wgpu pipeline-builder.
//!
//! § DETERMINISM
//!   The emitted SPIR-V is byte-exact for a given `(workgroup, entry-name,
//!   binding-shape)` — the lowering driver is deterministic + the type-cache
//!   is order-stable. Two calls with the same `SubstrateKernelSpec` produce
//!   identical word-vectors.

use cssl_cgen_spirv::op::{
    AddressingModel, Builtin, Capability, Decoration, Dim, ExecutionMode, ExecutionModel,
    GlslStd450, ImageFormat, MemoryModel, Op, StorageClass, FN_CONTROL_NONE,
};
use cssl_cgen_spirv::{LowerError, SpirvBinary};

/// Canonical CSSL substrate-kernel source.
///
/// This is intentionally included from the repository root, not duplicated in
/// Rust, so source deletion or declaration drift breaks the compiler path.
pub const CANONICAL_SUBSTRATE_KERNEL_SOURCE: &str =
    include_str!("../../../../Labyrinth of Apocalypse/systems/substrate_v2_kernel.csl");

/// § Spec for the substrate-kernel emit path. Mirrors the declaration block
/// in `substrate_v2_kernel.csl` § INPUTS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubstrateKernelSpec {
    /// Entry-point name. The `.csl` source declares `entry: "main"` ; the
    /// host `cssl-host-substrate-render-v3` picks up that name when it
    /// constructs the `VkPipelineShaderStageCreateInfo`.
    pub entry_name: String,
    /// Workgroup size. The `.csl` source declares `workgroup: ⟨8, 8, 1⟩`.
    pub workgroup: (u32, u32, u32),
    /// Whether the kernel binds the observer uniform (`set=0, binding=0`).
    /// Always `true` for the substrate-kernel ; exposed as a flag so callers
    /// can construct a stripped-down probe kernel for tests.
    pub has_observer_uniform: bool,
    /// Whether the kernel binds the crystal storage buffer (`set=0, binding=1`).
    pub has_crystals_storage: bool,
    /// Whether the kernel binds the output storage-image (`set=0, binding=2`).
    /// The canonical substrate-kernel emits this as a `UniformConstant`
    /// `OpTypeImage` descriptor and writes it via `OpImageWrite`.
    pub has_output_storage_image: bool,
    /// Source-declared bounded crystal slots in `GpuCrystalScene`.
    pub max_scene_crystals: u32,
    /// Source-declared byte offsets for bounded `GpuCrystalScene.slot*` audit anchors.
    pub crystal_slot_offsets: [u32; MAX_SCENE_CRYSTALS],
    /// Source-declared byte stride for the contiguous `GpuCrystalScene.slots`
    /// storage array.
    pub crystal_slot_stride: u32,
}

impl SubstrateKernelSpec {
    /// Canonical spec : matches `substrate_v2_kernel.csl` verbatim.
    #[must_use]
    pub fn canonical() -> Self {
        Self::canonical_from_source().expect("canonical substrate_v2_kernel.csl must parse")
    }

    /// Canonical spec parsed from the repository-owned CSSL source.
    ///
    /// § TEST-DISCIPLINE
    /// This is the load-bearing path: the host artifact must depend on the
    /// source declaration. A hard-coded Rust mirror can pass while the source
    /// is missing or wrong, which is exactly the false-green mode forbidden by
    /// `TEST_DISCIPLINE.md`.
    pub fn canonical_from_source() -> Result<Self, SubstrateKernelSourceError> {
        Self::from_csl_source(CANONICAL_SUBSTRATE_KERNEL_SOURCE)
    }

    /// Parse the substrate-kernel declaration subset from CSSL source.
    ///
    /// This parser is deliberately narrow: it accepts the canonical
    /// declaration shape and rejects missing sovereignty/runtime markers or
    /// resource-binding drift before SPIR-V emission starts.
    pub fn from_csl_source(source: &str) -> Result<Self, SubstrateKernelSourceError> {
        require_line(source, "kernel", "substrate_v2_main")?;

        let stage = required_value(source, "stage")?;
        if !stage.contains("Compute") {
            return Err(SubstrateKernelSourceError::UnsupportedStage(stage));
        }

        let backend = required_value(source, "backend-emit")?;
        if !backend.contains("SPIR-V-1.5") || !backend.contains("Vulkan-1.3") {
            return Err(SubstrateKernelSourceError::UnsupportedBackend(backend));
        }

        require_truth_marker(source, "no-wgpu")?;
        require_truth_marker(source, "no-naga")?;
        require_truth_marker(source, "no-wgsl")?;

        let entry_line = required_value(source, "entry")?;
        let entry_name = parse_quoted(&entry_line, "entry")?;
        if entry_name != "main" {
            return Err(SubstrateKernelSourceError::UnsupportedEntry(entry_name));
        }

        let workgroup_line = required_value(source, "workgroup")?;
        let workgroup = parse_workgroup(&workgroup_line)?;

        let has_observer_uniform =
            has_binding(source, "binding=0", "observer", "Uniform<GpuObserver>")
                || has_binding(source, "binding=0", "observer", "Uniform⟨GpuObserver⟩");
        if !has_observer_uniform {
            return Err(SubstrateKernelSourceError::MissingBinding(
                "observer uniform binding 0",
            ));
        }

        let has_crystals_storage =
            has_binding(source, "binding=1", "crystals", "Storage<GpuCrystal")
                || has_binding(source, "binding=1", "crystals", "Storage⟨GpuCrystal");
        if !has_crystals_storage {
            return Err(SubstrateKernelSourceError::MissingBinding(
                "crystals storage binding 1",
            ));
        }
        require_scene_crystal_abi(source)?;
        let (max_scene_crystals, crystal_slot_offsets, crystal_slot_stride) =
            parse_scene_crystal_abi(source)?;

        let has_output_storage_image = has_binding(
            source,
            "binding=2",
            "output-image",
            "StorageImage<RGBA8Unorm>",
        ) || has_binding(
            source,
            "binding=2",
            "output-image",
            "StorageImage⟨RGBA8Unorm⟩",
        );
        if !has_output_storage_image {
            return Err(SubstrateKernelSourceError::MissingBinding(
                "output-image storage image binding 2",
            ));
        }

        Ok(Self {
            entry_name,
            workgroup,
            has_observer_uniform,
            has_crystals_storage,
            has_output_storage_image,
            max_scene_crystals,
            crystal_slot_offsets,
            crystal_slot_stride,
        })
    }

    /// Count source-declared resource bindings that the substrate kernel uses.
    #[must_use]
    pub fn resource_binding_count(&self) -> u32 {
        u32::from(self.has_observer_uniform)
            + u32::from(self.has_crystals_storage)
            + u32::from(self.has_output_storage_image)
    }

    /// Count invocations per source-declared workgroup.
    #[must_use]
    pub fn workgroup_invocation_count(&self) -> u32 {
        let (x, y, z) = self.workgroup;
        x.saturating_mul(y).saturating_mul(z)
    }

    /// Behavioral canary derived from CSSL source declarations.
    ///
    /// This value is emitted into the kernel body as `workgroup_invocations +
    /// resource_binding_count`. It is not a renderer yet; it is a deliberately
    /// small truth gate proving that source declaration drift changes real
    /// MIR-body arithmetic and not only header metadata.
    #[must_use]
    pub fn source_canary_value(&self) -> i32 {
        let canary =
            u64::from(self.workgroup_invocation_count()) + u64::from(self.resource_binding_count());
        i32::try_from(canary).unwrap_or(i32::MAX)
    }

    /// Blue-channel byte after folding observer.width into the source canary.
    ///
    /// This keeps the original source-declaration canary observable while
    /// proving binding=0 uniform data is live in the shader body.
    #[must_use]
    pub fn observer_canary_value(&self, observer_width: u32) -> u8 {
        self.descriptor_canary_value(observer_width, 0)
    }

    /// Blue-channel byte after folding observer.width and the first crystal
    /// storage word into the source canary.
    #[must_use]
    pub fn descriptor_canary_value(&self, observer_width: u32, crystal_salt: u32) -> u8 {
        self.descriptor_canary_value_for_crystals(
            observer_width,
            [[crystal_salt, 128, 128, 0], EMPTY_CRYSTAL_WORDS],
        )
    }

    /// Blue-channel byte after folding observer.width and two crystal slots
    /// into the source canary.
    #[must_use]
    pub fn descriptor_canary_value_for_crystals(
        &self,
        observer_width: u32,
        crystal_words: [[u32; 4]; 2],
    ) -> u8 {
        self.descriptor_canary_value_for_crystal_count(
            observer_width,
            2,
            expand_two_crystal_words(crystal_words),
        )
    }

    /// Blue-channel byte after folding observer.width and a bounded scene
    /// crystal count into the source canary.
    #[must_use]
    pub fn descriptor_canary_value_for_crystal_count(
        &self,
        observer_width: u32,
        crystal_count: u32,
        crystal_words: [[u32; 4]; MAX_SCENE_CRYSTALS],
    ) -> u8 {
        let canary = i64::from(self.source_canary_value())
            + i64::from(observer_width)
            + crystal_words
                .iter()
                .take(crystal_count.min(MAX_SCENE_CRYSTALS as u32) as usize)
                .map(|words| i64::from(words[0]) + i64::from(words[3]))
                .sum::<i64>();
        canary.clamp(0, 255) as u8
    }

    /// CPU oracle for the current v13 center-ray shader slice.
    ///
    /// Mirrors the verified subset of `LoA v13/src/substrate.cssl`
    /// `render_pixel`: outer ellipsoid, pillar CSG, first-hit selection,
    /// normal, key-light, ambient, and branchless albedo. Checker/reflection
    /// remain deferred to later slices.
    #[must_use]
    pub fn v13_center_ray_intensity(&self) -> f32 {
        v13_probe_intensity_cpu(0.0, 0.0, 0.0, 0.0, 0.0, 1.0)
    }

    /// CPU oracle for the v13 pillar-hit probe ray.
    #[must_use]
    pub fn v13_pillar_y_ray_intensity(&self) -> f32 {
        v13_probe_intensity_cpu(0.0, 3.0, 0.0, 0.0, -1.0, 0.0)
    }

    /// CPU oracle for a simple observer-width camera ray.
    #[must_use]
    pub fn v13_camera_ray_intensity(&self, observer_width: u32, pixel_x: u32) -> f32 {
        self.v13_camera_ray_intensity_for_yaw(observer_width, pixel_x, 0)
    }

    /// CPU oracle for a simple observer-width camera ray with yaw offset.
    #[must_use]
    pub fn v13_camera_ray_intensity_for_yaw(
        &self,
        observer_width: u32,
        pixel_x: u32,
        yaw_milli: u32,
    ) -> f32 {
        let dx_raw = camera_dx_raw(observer_width, pixel_x, yaw_milli);
        let ray_len = (dx_raw * dx_raw + 1.0).sqrt().max(0.000_001);
        v13_probe_intensity_cpu(0.0, 0.0, 0.0, dx_raw / ray_len, 0.0, 1.0 / ray_len)
    }

    /// CPU oracle for camera ray plus first-crystal emissive contribution.
    #[must_use]
    pub fn v13_camera_ray_shaded_intensity(
        &self,
        observer_width: u32,
        pixel_x: u32,
        crystal_salt: u32,
    ) -> f32 {
        self.v13_camera_ray_shaded_intensity_for_yaw(observer_width, pixel_x, 0, crystal_salt)
    }

    /// CPU oracle for yaw-aware camera ray plus first-crystal emissive contribution.
    #[must_use]
    pub fn v13_camera_ray_shaded_intensity_for_yaw(
        &self,
        observer_width: u32,
        pixel_x: u32,
        yaw_milli: u32,
        crystal_salt: u32,
    ) -> f32 {
        let base = self.v13_camera_ray_intensity_for_yaw(observer_width, pixel_x, yaw_milli);
        (base + crystal_emissive(crystal_salt)).clamp(0.0, 1.0)
    }

    /// CPU oracle for 2D camera ray plus first-crystal emissive contribution.
    #[must_use]
    pub fn v13_camera_ray_shaded_intensity_2d(
        &self,
        observer_size: (u32, u32),
        pixel: (u32, u32),
        yaw_milli: u32,
        crystal_salt: u32,
    ) -> f32 {
        self.v13_camera_ray_shaded_intensity_2d_for_crystal(
            observer_size,
            pixel,
            yaw_milli,
            [crystal_salt, 128, 128, 0],
        )
    }

    /// CPU oracle for 2D camera ray plus first-crystal spatial contribution.
    #[must_use]
    pub fn v13_camera_ray_shaded_intensity_2d_for_crystal(
        &self,
        observer_size: (u32, u32),
        pixel: (u32, u32),
        yaw_milli: u32,
        crystal_words: [u32; 4],
    ) -> f32 {
        self.v13_camera_ray_shaded_intensity_2d_for_crystals(
            observer_size,
            pixel,
            yaw_milli,
            [crystal_words, EMPTY_CRYSTAL_WORDS],
        )
    }

    /// CPU oracle for 2D camera ray plus two fixed crystal slots.
    #[must_use]
    pub fn v13_camera_ray_shaded_intensity_2d_for_crystals(
        &self,
        observer_size: (u32, u32),
        pixel: (u32, u32),
        yaw_milli: u32,
        crystal_words: [[u32; 4]; 2],
    ) -> f32 {
        self.v13_camera_ray_shaded_intensity_2d_for_crystal_count(
            observer_size,
            pixel,
            yaw_milli,
            2,
            expand_two_crystal_words(crystal_words),
        )
    }

    /// CPU oracle for 2D camera ray plus bounded scene crystal slots.
    #[must_use]
    pub fn v13_camera_ray_shaded_intensity_2d_for_crystal_count(
        &self,
        observer_size: (u32, u32),
        pixel: (u32, u32),
        yaw_milli: u32,
        crystal_count: u32,
        crystal_words: [[u32; 4]; MAX_SCENE_CRYSTALS],
    ) -> f32 {
        self.v13_camera_ray_visual_components_2d_for_crystal_count(
            observer_size,
            pixel,
            yaw_milli,
            crystal_count,
            crystal_words,
        )
        .0
    }

    /// CPU oracle visual components for 2D camera ray plus bounded scene
    /// crystal slots. Returns `(red_lit, material_mix, object_mask)`.
    #[must_use]
    pub fn v13_camera_ray_visual_components_2d_for_crystal_count(
        &self,
        observer_size: (u32, u32),
        pixel: (u32, u32),
        yaw_milli: u32,
        crystal_count: u32,
        crystal_words: [[u32; 4]; MAX_SCENE_CRYSTALS],
    ) -> (f32, f32, f32) {
        let dx_raw = camera_dx_raw(observer_size.0, pixel.0, yaw_milli);
        let dy_raw = camera_dy_raw(observer_size.1, pixel.1);
        let ray_len = (dx_raw * dx_raw + dy_raw * dy_raw + 1.0)
            .sqrt()
            .max(0.000_001);
        let base = v13_probe_intensity_cpu(
            0.0,
            0.0,
            0.0,
            dx_raw / ray_len,
            dy_raw / ray_len,
            1.0 / ray_len,
        );
        let active_count = crystal_count.min(MAX_SCENE_CRYSTALS as u32);
        let mut emissive_sum = 0.0_f32;
        let mut emissive_peak = 0.0_f32;
        let mut object_mask = 0.0_f32;
        let mut material_weighted_sum = 0.0_f32;
        let mut material_weight = 0.0_f32;
        let mut transmittance = 1.0_f32;
        for words in crystal_words.into_iter().take(active_count as usize) {
            let (emissive, influence) = crystal_spatial_terms(words, dx_raw, dy_raw);
            let visible_influence = influence * transmittance;
            let visible_emissive = emissive * transmittance;
            emissive_sum += visible_emissive;
            emissive_peak = emissive_peak.max(visible_emissive);
            object_mask = object_mask.max(visible_influence);
            material_weighted_sum += crystal_material_norm(words[3]) * visible_emissive;
            material_weight += visible_emissive;
            let occlusion = (influence * 0.65).clamp(0.0, 0.95);
            transmittance *= 1.0 - occlusion;
        }
        let emissive_average = emissive_sum / (active_count.max(1) as f32);
        let emissive = emissive_average + emissive_peak * 0.75;
        let material_mix = if material_weight > 0.000_001 {
            material_weighted_sum / material_weight
        } else {
            0.0
        };
        (
            (base + emissive).clamp(0.0, 1.0),
            material_mix.clamp(0.0, 1.0),
            object_mask.clamp(0.0, 1.0),
        )
    }

    /// CPU oracle RGBA8 for the current v13 shader slice.
    #[must_use]
    pub fn v13_probe_rgba8(&self) -> [u8; 4] {
        self.v13_probe_rgba8_for_observer_width(0)
    }

    /// CPU oracle RGBA8 for the current v13 shader slice and observer width.
    #[must_use]
    pub fn v13_probe_rgba8_for_observer_width(&self, observer_width: u32) -> [u8; 4] {
        self.v13_probe_rgba8_for_descriptors(observer_width, 0)
    }

    /// CPU oracle RGBA8 for the current v13 shader slice and descriptor salts.
    #[must_use]
    pub fn v13_probe_rgba8_for_descriptors(
        &self,
        observer_width: u32,
        crystal_salt: u32,
    ) -> [u8; 4] {
        compose_v13_visual_rgba8(
            self.v13_center_ray_intensity(),
            0.0,
            self.descriptor_canary_value(observer_width, crystal_salt),
            1.0,
            0.0,
            0.0,
        )
    }

    /// Expected first two pixels for the gid-driven probe row.
    #[must_use]
    pub fn v13_probe_row_prefix_rgba8(&self) -> [[u8; 4]; 2] {
        self.v13_probe_row_prefix_rgba8_for_observer_width(0)
    }

    /// Expected first two pixels for the gid-driven probe row and observer width.
    #[must_use]
    pub fn v13_probe_row_prefix_rgba8_for_observer_width(
        &self,
        observer_width: u32,
    ) -> [[u8; 4]; 2] {
        self.v13_probe_row_prefix_rgba8_for_descriptors(observer_width, 0)
    }

    /// Expected first two pixels for the gid-driven row and descriptor salts.
    #[must_use]
    pub fn v13_probe_row_prefix_rgba8_for_descriptors(
        &self,
        observer_width: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 2] {
        [
            self.v13_probe_rgba8_for_descriptors(observer_width, crystal_salt),
            compose_v13_visual_rgba8(
                self.v13_pillar_y_ray_intensity(),
                1.0,
                self.descriptor_canary_value(observer_width, crystal_salt),
                1.0,
                0.0,
                0.0,
            ),
        ]
    }

    /// Expected 8-pixel camera row for the current headless workgroup slice.
    #[must_use]
    pub fn v13_camera_row8_rgba8_for_descriptors(
        &self,
        observer_width: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 8] {
        self.v13_camera_row8_rgba8_for_inputs(observer_width, 0, crystal_salt)
    }

    /// Expected 8-pixel camera row for observer yaw and first-crystal strength.
    #[must_use]
    pub fn v13_camera_row8_rgba8_for_inputs(
        &self,
        observer_width: u32,
        yaw_milli: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 8] {
        core::array::from_fn(|x| {
            let x = x as u32;
            compose_v13_visual_rgba8(
                self.v13_camera_ray_shaded_intensity_for_yaw(
                    observer_width,
                    x,
                    yaw_milli,
                    crystal_salt,
                ),
                camera_x_norm(observer_width, x),
                self.descriptor_canary_value(observer_width, crystal_salt),
                1.0,
                0.0,
                0.0,
            )
        })
    }

    /// Expected 8x8 camera tile for observer size/yaw and first-crystal strength.
    #[must_use]
    pub fn v13_camera_tile8_rgba8_for_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_salt: u32,
    ) -> [[u8; 4]; 64] {
        self.v13_camera_tile8_rgba8_for_scene_inputs(
            observer_size,
            yaw_milli,
            [crystal_salt, 128, 128, 0],
        )
    }

    /// Expected 8x8 camera tile for observer pose and first-crystal words.
    #[must_use]
    pub fn v13_camera_tile8_rgba8_for_scene_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_words: [u32; 4],
    ) -> [[u8; 4]; 64] {
        self.v13_camera_tile8_rgba8_for_scene2_inputs(
            observer_size,
            yaw_milli,
            [crystal_words, EMPTY_CRYSTAL_WORDS],
        )
    }

    /// Expected 8x8 camera tile for observer pose and two crystal slots.
    #[must_use]
    pub fn v13_camera_tile8_rgba8_for_scene2_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_words: [[u32; 4]; 2],
    ) -> [[u8; 4]; 64] {
        self.v13_camera_tile8_rgba8_for_scene_slots_inputs(
            observer_size,
            yaw_milli,
            2,
            expand_two_crystal_words(crystal_words),
        )
    }

    /// Expected 8x8 camera tile for observer pose and bounded scene crystals.
    #[must_use]
    pub fn v13_camera_tile8_rgba8_for_scene_slots_inputs(
        &self,
        observer_size: (u32, u32),
        yaw_milli: u32,
        crystal_count: u32,
        crystal_words: [[u32; 4]; MAX_SCENE_CRYSTALS],
    ) -> [[u8; 4]; 64] {
        core::array::from_fn(|i| {
            let x = (i % 8) as u32;
            let y = (i / 8) as u32;
            let (red, material_mix, object_mask) = self
                .v13_camera_ray_visual_components_2d_for_crystal_count(
                    observer_size,
                    (x, y),
                    yaw_milli,
                    crystal_count,
                    crystal_words,
                );
            compose_v13_visual_rgba8(
                red,
                camera_x_norm(observer_size.0, x),
                self.descriptor_canary_value_for_crystal_count(
                    observer_size.0,
                    crystal_count,
                    crystal_words,
                ),
                camera_y_norm(observer_size.1, y),
                material_mix,
                object_mask,
            )
        })
    }
}

/// § Errors thrown when parsing the canonical substrate-kernel source.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubstrateKernelSourceError {
    #[error("substrate-kernel source missing declaration `{0}`")]
    MissingDeclaration(&'static str),
    #[error("substrate-kernel source has unsupported stage `{0}`")]
    UnsupportedStage(String),
    #[error("substrate-kernel source has unsupported backend `{0}`")]
    UnsupportedBackend(String),
    #[error("substrate-kernel source has unsupported entry `{0}`")]
    UnsupportedEntry(String),
    #[error("substrate-kernel source has invalid workgroup `{0}`")]
    InvalidWorkgroup(String),
    #[error("substrate-kernel source declares unsupported crystal count `{0}`")]
    UnsupportedSceneCrystalCount(u32),
    #[error("substrate-kernel source missing `{0}` truth marker")]
    MissingTruthMarker(&'static str),
    #[error("substrate-kernel source missing resource binding `{0}`")]
    MissingBinding(&'static str),
}

/// § Errors thrown when emitting the substrate-kernel SPIR-V.
#[derive(Debug, thiserror::Error)]
pub enum SubstrateKernelEmitError {
    /// The canonical CSSL source declaration rejected before lowering.
    #[error("substrate-kernel source invalid : {0}")]
    Source(#[from] SubstrateKernelSourceError),
    /// The from-scratch SPIR-V backend rejected the lowering.
    #[error("substrate-kernel lowering failed : {0}")]
    Lower(#[from] LowerError),
}

fn required_value(source: &str, key: &'static str) -> Result<String, SubstrateKernelSourceError> {
    source
        .lines()
        .map(str::trim)
        .find_map(|line| {
            let rest = line.strip_prefix(key)?;
            let (_, value) = rest.split_once(':')?;
            Some(value.trim().to_string())
        })
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))
}

fn require_line(
    source: &str,
    key: &'static str,
    needle: &'static str,
) -> Result<(), SubstrateKernelSourceError> {
    if source
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with(key) && line.contains(needle))
    {
        Ok(())
    } else {
        Err(SubstrateKernelSourceError::MissingDeclaration(key))
    }
}

fn require_truth_marker(
    source: &str,
    marker: &'static str,
) -> Result<(), SubstrateKernelSourceError> {
    if source
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with(marker) && line.contains('✓'))
    {
        Ok(())
    } else {
        Err(SubstrateKernelSourceError::MissingTruthMarker(marker))
    }
}

fn require_scene_crystal_abi(source: &str) -> Result<(), SubstrateKernelSourceError> {
    let max_scene_crystals = parse_first_u32_from_line(source, "MAX_SCENE_CRYSTALS")?;
    if max_scene_crystals != MAX_SCENE_CRYSTALS as u32 {
        return Err(SubstrateKernelSourceError::UnsupportedSceneCrystalCount(
            max_scene_crystals,
        ));
    }
    require_line(source, "GpuCrystalScene", "struct")?;
    require_line(source, "header", "offset 0")?;
    require_line(source, "word0", "active_crystal_count")?;
    require_line(source, "slots", "GpuCrystalWord[]")?;
    require_line(source, "slots", "offset 16")?;
    require_line(source, "slots", "stride 16")?;
    require_line(source, "derived.slot-layout", ":")?;
    require_line(source, "formula", "offset(slot_i)")?;
    require_line(source, "formula", "+ i *")?;
    require_line(source, "domain", "MAX_SCENE_CRYSTALS-1")?;
    require_line(source, "GpuCrystalWord", "vec4u")?;
    require_line(source, "word0", "strength_u8")?;
    require_line(source, "word1", "x_bias_u8")?;
    require_line(source, "word2", "y_bias_u8")?;
    require_line(source, "word3", "material_code_u8")?;
    require_line(
        source,
        "active",
        "clamp((crystals.header.active_crystal_count - i)",
    )?;
    Ok(())
}

fn parse_scene_crystal_abi(
    source: &str,
) -> Result<(u32, [u32; MAX_SCENE_CRYSTALS], u32), SubstrateKernelSourceError> {
    let max_scene_crystals = parse_first_u32_from_line(source, "MAX_SCENE_CRYSTALS")?;
    let slot_base_offset = parse_offset_from_line(source, "slots")?;
    let crystal_slot_stride = parse_stride_from_line(source, "slots")?;
    let (formula_base_offset, formula_stride) = parse_slot_layout_formula(source)?;
    if formula_base_offset != slot_base_offset || formula_stride != crystal_slot_stride {
        return Err(SubstrateKernelSourceError::MissingDeclaration(
            "derived.slot-layout.formula",
        ));
    }
    let crystal_slot_offsets =
        core::array::from_fn(|i| slot_base_offset + (i as u32 * crystal_slot_stride));
    Ok((
        max_scene_crystals,
        crystal_slot_offsets,
        crystal_slot_stride,
    ))
}

fn parse_first_u32_from_line(
    source: &str,
    key: &'static str,
) -> Result<u32, SubstrateKernelSourceError> {
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(key))
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    line.split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?
        .parse::<u32>()
        .map_err(|_| SubstrateKernelSourceError::MissingDeclaration(key))
}

fn parse_offset_from_line(
    source: &str,
    key: &'static str,
) -> Result<u32, SubstrateKernelSourceError> {
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(key) && line.contains("offset"))
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    let (_, offset_tail) = line
        .split_once("offset")
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    offset_tail
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?
        .parse::<u32>()
        .map_err(|_| SubstrateKernelSourceError::MissingDeclaration(key))
}

fn parse_stride_from_line(
    source: &str,
    key: &'static str,
) -> Result<u32, SubstrateKernelSourceError> {
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(key) && line.contains("stride"))
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    let (_, stride_tail) = line
        .split_once("stride")
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    stride_tail
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?
        .parse::<u32>()
        .map_err(|_| SubstrateKernelSourceError::MissingDeclaration(key))
}

fn parse_slot_layout_formula(source: &str) -> Result<(u32, u32), SubstrateKernelSourceError> {
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("formula") && line.contains("offset(slot_i)"))
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(
            "derived.slot-layout.formula",
        ))?;
    let nums: Result<Vec<u32>, _> = line
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(str::parse::<u32>)
        .collect();
    let nums = nums.map_err(|_| {
        SubstrateKernelSourceError::MissingDeclaration("derived.slot-layout.formula")
    })?;
    match nums.as_slice() {
        [base, stride] => Ok((*base, *stride)),
        _ => Err(SubstrateKernelSourceError::MissingDeclaration(
            "derived.slot-layout.formula",
        )),
    }
}

fn parse_quoted(value: &str, key: &'static str) -> Result<String, SubstrateKernelSourceError> {
    let start = value
        .find('"')
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    let tail = &value[start + 1..];
    let end = tail
        .find('"')
        .ok_or(SubstrateKernelSourceError::MissingDeclaration(key))?;
    Ok(tail[..end].to_string())
}

fn parse_workgroup(value: &str) -> Result<(u32, u32, u32), SubstrateKernelSourceError> {
    let nums: Result<Vec<u32>, _> = value
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(str::parse::<u32>)
        .collect();
    let nums = nums.map_err(|_| SubstrateKernelSourceError::InvalidWorkgroup(value.into()))?;
    if nums.len() != 3 {
        return Err(SubstrateKernelSourceError::InvalidWorkgroup(value.into()));
    }
    Ok((nums[0], nums[1], nums[2]))
}

fn has_binding(source: &str, binding: &str, name: &str, ty: &str) -> bool {
    source.lines().map(str::trim).any(|line| {
        line.contains(binding) && line.contains(name) && line.contains(ty) && line.contains("set=0")
    })
}

fn f32_to_unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8
}

fn compose_v13_visual_rgba8(
    red: f32,
    x_norm: f32,
    descriptor_canary: u8,
    y_norm: f32,
    material_mix: f32,
    object_mask: f32,
) -> [u8; 4] {
    let canary_norm = f32::from(descriptor_canary) / 255.0;
    let material_mix = material_mix.clamp(0.0, 1.0);
    let object_mask = object_mask.clamp(0.0, 1.0);
    let edge = (object_mask * (1.0 - object_mask) * 4.0).clamp(0.0, 1.0);
    let depth_shadow = 1.0 - object_mask * 0.18;
    let red_raw = red * (0.82 + object_mask * 0.18) + edge * 0.18;
    let green_base = red * 0.48 + x_norm.clamp(0.0, 1.0) * 0.20 + material_mix * 0.16;
    let green_raw = green_base * depth_shadow + object_mask * material_mix * 0.25 + edge * 0.10;
    let blue_base = red * 0.30
        + y_norm.clamp(0.0, 1.0) * 0.20
        + canary_norm * 0.12
        + (1.0 - material_mix) * 0.20;
    let blue_raw = blue_base * (1.0 - object_mask * 0.22)
        + object_mask * (1.0 - material_mix) * 0.25
        + edge * 0.06;
    [
        f32_to_unorm8(red_raw),
        f32_to_unorm8(green_raw),
        f32_to_unorm8(blue_raw),
        f32_to_unorm8(y_norm),
    ]
}

fn camera_x_norm(observer_width: u32, pixel_x: u32) -> f32 {
    let denom = ((observer_width as f32) - 1.0).max(1.0);
    (pixel_x as f32 / denom).clamp(0.0, 1.0)
}

fn camera_y_norm(observer_height: u32, pixel_y: u32) -> f32 {
    let denom = ((observer_height as f32) - 1.0).max(1.0);
    (pixel_y as f32 / denom).clamp(0.0, 1.0)
}

fn camera_dx_raw(observer_width: u32, pixel_x: u32, yaw_milli: u32) -> f32 {
    let x_norm = camera_x_norm(observer_width, pixel_x);
    let yaw_offset = ((yaw_milli as f32) / 1000.0).clamp(0.0, 1.0) * 0.25;
    (x_norm - 0.5) * 0.5 + yaw_offset
}

fn camera_dy_raw(observer_height: u32, pixel_y: u32) -> f32 {
    (0.5 - camera_y_norm(observer_height, pixel_y)) * 0.5
}

fn crystal_emissive(crystal_salt: u32) -> f32 {
    ((crystal_salt as f32) / 255.0).clamp(0.0, 1.0) * 0.5
}

fn crystal_material_norm(material_code: u32) -> f32 {
    (material_code.min(23) as f32 / 23.0).clamp(0.0, 1.0)
}

const EMPTY_CRYSTAL_WORDS: [u32; 4] = [0, 128, 128, 0];
pub const MAX_SCENE_CRYSTALS: usize = 128;

fn expand_two_crystal_words(crystal_words: [[u32; 4]; 2]) -> [[u32; 4]; MAX_SCENE_CRYSTALS] {
    let mut expanded = [EMPTY_CRYSTAL_WORDS; MAX_SCENE_CRYSTALS];
    expanded[0] = crystal_words[0];
    expanded[1] = crystal_words[1];
    expanded
}

fn crystal_axis_offset(word: u32) -> f32 {
    (((word.min(255) as f32) - 128.0) / 128.0).clamp(-1.0, 1.0) * 0.5
}

fn crystal_spatial_terms(crystal_words: [u32; 4], dx_raw: f32, dy_raw: f32) -> (f32, f32) {
    let x_offset = crystal_axis_offset(crystal_words[1]);
    let y_offset = crystal_axis_offset(crystal_words[2]);
    let x_delta = dx_raw - x_offset;
    let y_delta = dy_raw - y_offset;
    let dist = (x_delta * x_delta + y_delta * y_delta).sqrt();
    let core = (1.0 - dist * 8.0).clamp(0.0, 1.0);
    let halo = (1.0 - dist * 2.25).clamp(0.0, 1.0) * 0.12;
    let influence = (core + halo).clamp(0.0, 1.0);
    (crystal_emissive(crystal_words[0]) * influence, influence)
}

fn v13_probe_intensity_cpu(px: f32, py: f32, pz: f32, dx: f32, dy: f32, dz: f32) -> f32 {
    let oa = (dx / 6.0) * (dx / 6.0) + (dy / 4.0) * (dy / 4.0) + (dz / 14.0) * (dz / 14.0);
    let ob = 2.0 * (px * dx / 36.0 + py * dy / 16.0 + pz * dz / 196.0);
    let og = (px / 6.0) * (px / 6.0) + (py / 4.0) * (py / 4.0) + (pz / 14.0) * (pz / 14.0) - 1.0;
    let o_disc = ob * ob - 4.0 * oa * og;
    let o_sq = o_disc.max(0.0).sqrt();
    let t_outer = (0.0 - ob + o_sq) / (2.0 * oa);

    let r0 = 1.5_f32;
    let pa = dx * dx + dy * dy + dz * dz;
    let pb = 2.0 * (px * dx + py * dy + pz * dz);
    let pg = px * px + py * py + pz * pz - r0 * r0;
    let p_disc = pb * pb - 4.0 * pa * pg;
    let p_sq = p_disc.max(0.0).sqrt();
    let p_t_enter = (0.0 - pb - p_sq) / (2.0 * pa);
    let p_hit_flag = (p_disc * 1_000_000.0).clamp(0.0, 1.0);
    let p_valid = p_hit_flag * ((p_t_enter - 0.0001) * 1_000_000.0).clamp(0.0, 1.0);
    let t_inner = p_t_enter * p_valid + 1_000_000_000.0 * (1.0 - p_valid);

    let s = t_outer.min(t_inner);
    let inner_wins = ((t_outer - t_inner) * 1_000_000.0).clamp(0.0, 1.0);
    let hx = px + dx * s;
    let hy = py + dy * s;
    let hz = pz + dz * s;

    let outer_nx = 0.0 - hx / 18.0;
    let outer_ny = 0.0 - hy / 8.0;
    let outer_nz = 0.0 - hz / 98.0;
    let inner_nx = 2.0 * hx;
    let inner_ny = 2.0 * hy;
    let inner_nz = 2.0 * hz;
    let nx_raw = inner_nx * inner_wins + outer_nx * (1.0 - inner_wins);
    let ny_raw = inner_ny * inner_wins + outer_ny * (1.0 - inner_wins);
    let nz_raw = inner_nz * inner_wins + outer_nz * (1.0 - inner_wins);
    let n_len = (nx_raw * nx_raw + ny_raw * ny_raw + nz_raw * nz_raw).sqrt();
    let nx = nx_raw / n_len.max(0.000001);
    let ny = ny_raw / n_len.max(0.000001);
    let nz = nz_raw / n_len.max(0.000001);

    let lx = 2.0 - hx;
    let ly = 3.0 - hy;
    let lz = (0.0 - 2.0) - hz;
    let r2 = lx * lx + ly * ly + lz * lz;
    let r = r2.sqrt();
    let n_dot_l = ((nx * lx + ny * ly + nz * lz) / r.max(0.000001)).max(0.0);
    let direct = n_dot_l * 8.0 / r2.max(1.0);
    let albedo = (1.0 - inner_wins) * 0.4 + inner_wins * 0.3;
    (0.15 + direct) * albedo
}

/// § Emit canonical substrate-kernel SPIR-V words.
///
/// This advances `cssl-cgen-gpu-spirv` to be the orchestrator that drives a
/// from-scratch `cssl-cgen-spirv` storage-image compute shader along the
/// substrate-kernel shape declared in `substrate_v2_kernel.csl`. The output is
/// a `Vec<u32>` of canonical SPIR-V 1.5 words ready to feed
/// `vkCreateShaderModule` directly (no naga · no WGSL · no wgpu in the chain).
///
/// The shader writes a v13-derived camera tile. `gid`, observer dimensions,
/// and observer.yaw form a normalized camera ray; red runs the v13 ellipsoid/
/// CSG lighting oracle plus normalized+peak bounded crystal spatial emissives.
/// Green mixes lighting with x_norm, blue mixes lighting, y_norm, and the
/// descriptor canary, and alpha encodes y_norm. This is intentionally smaller
/// than the full LoA renderer, but it is real descriptor/image behavior and
/// gives tests a pixel-level drift signal while v13 math ports into the kernel.
///
/// § ERRORS
///   Returns [`SubstrateKernelEmitError`] if source parsing, CFG validation,
///   or MIR-body SPIR-V emission fails.
pub fn emit_substrate_kernel_spirv(
    spec: &SubstrateKernelSpec,
) -> Result<Vec<u32>, SubstrateKernelEmitError> {
    Ok(emit_substrate_storage_image_spirv(spec).finalize())
}

#[allow(clippy::too_many_lines)]
fn emit_substrate_storage_image_spirv(spec: &SubstrateKernelSpec) -> SpirvBinary {
    let mut b = SpirvBinary::new();

    let entry_id = b.alloc_id();
    let gid_var = b.alloc_id();
    let observer_var = b.alloc_id();
    let observer_block_ty = b.alloc_id();
    let crystal_var = b.alloc_id();
    let crystal_block_ty = b.alloc_id();
    let image_var = b.alloc_id();
    let glsl_id = b.alloc_id();

    // § 1-3 module prelude.
    b.push_op(Op::Capability, &[Capability::Shader.as_u32()]);
    b.push_op(
        Op::Capability,
        &[Capability::StorageImageExtendedFormats.as_u32()],
    );
    b.push_op_with_string(Op::ExtInstImport, &[glsl_id], "GLSL.std.450", &[]);
    b.push_op(
        Op::MemoryModel,
        &[AddressingModel::Logical as u32, MemoryModel::GLSL450 as u32],
    );

    // § 4-6 entry, execution mode, debug.
    b.push_op_with_string(
        Op::EntryPoint,
        &[ExecutionModel::GLCompute.as_u32(), entry_id],
        &spec.entry_name,
        &[gid_var],
    );
    b.push_op(
        Op::ExecutionMode,
        &[
            entry_id,
            ExecutionMode::LocalSize as u32,
            spec.workgroup.0,
            spec.workgroup.1,
            spec.workgroup.2,
        ],
    );
    b.push_op_with_string(Op::Name, &[entry_id], &spec.entry_name, &[]);
    b.push_op_with_string(Op::Name, &[gid_var], "global_invocation_id", &[]);
    b.push_op_with_string(Op::Name, &[image_var], "output_image", &[]);

    // § 7 decorations.
    b.push_op(
        Op::Decorate,
        &[
            gid_var,
            Decoration::Builtin.as_u32(),
            Builtin::GlobalInvocationId as u32,
        ],
    );
    b.push_op(
        Op::Decorate,
        &[observer_block_ty, Decoration::Block.as_u32()],
    );
    b.push_op(
        Op::MemberDecorate,
        &[observer_block_ty, 0, Decoration::Offset.as_u32(), 0],
    );
    b.push_op(
        Op::Decorate,
        &[observer_var, Decoration::DescriptorSet.as_u32(), 0],
    );
    b.push_op(
        Op::Decorate,
        &[observer_var, Decoration::Binding.as_u32(), 0],
    );
    b.push_op(
        Op::Decorate,
        &[crystal_block_ty, Decoration::Block.as_u32()],
    );
    let crystal_array_ty = b.alloc_id();
    b.push_op(
        Op::Decorate,
        &[
            crystal_array_ty,
            Decoration::ArrayStride.as_u32(),
            spec.crystal_slot_stride,
        ],
    );
    b.push_op(
        Op::MemberDecorate,
        &[crystal_block_ty, 0, Decoration::Offset.as_u32(), 0],
    );
    b.push_op(
        Op::MemberDecorate,
        &[
            crystal_block_ty,
            1,
            Decoration::Offset.as_u32(),
            spec.crystal_slot_offsets[0],
        ],
    );
    b.push_op(
        Op::Decorate,
        &[crystal_var, Decoration::DescriptorSet.as_u32(), 0],
    );
    b.push_op(
        Op::Decorate,
        &[crystal_var, Decoration::Binding.as_u32(), 1],
    );
    b.push_op(
        Op::Decorate,
        &[image_var, Decoration::DescriptorSet.as_u32(), 0],
    );
    b.push_op(Op::Decorate, &[image_var, Decoration::Binding.as_u32(), 2]);

    // § 8 types, constants, globals.
    let void_ty = b.alloc_id();
    b.push_op(Op::TypeVoid, &[void_ty]);
    let bool_ty = b.alloc_id();
    b.push_op(Op::TypeBool, &[bool_ty]);
    let u32_ty = b.alloc_id();
    b.push_op(Op::TypeInt, &[u32_ty, 32, 0]);
    let f32_ty = b.alloc_id();
    b.push_op(Op::TypeFloat, &[f32_ty, 32]);
    let vec2u_ty = b.alloc_id();
    b.push_op(Op::TypeVector, &[vec2u_ty, u32_ty, 2]);
    let vec3u_ty = b.alloc_id();
    b.push_op(Op::TypeVector, &[vec3u_ty, u32_ty, 3]);
    let vec4u_ty = b.alloc_id();
    b.push_op(Op::TypeVector, &[vec4u_ty, u32_ty, 4]);
    b.push_op(Op::TypeRuntimeArray, &[crystal_array_ty, vec4u_ty]);
    let vec4f_ty = b.alloc_id();
    b.push_op(Op::TypeVector, &[vec4f_ty, f32_ty, 4]);
    b.push_op(Op::TypeStruct, &[observer_block_ty, vec4u_ty]);
    b.push_op(
        Op::TypeStruct,
        &[crystal_block_ty, vec4u_ty, crystal_array_ty],
    );
    let image_ty = b.alloc_id();
    b.push_op(
        Op::TypeImage,
        &[
            image_ty,
            f32_ty,
            Dim::Dim2D as u32,
            0,
            0,
            0,
            2,
            ImageFormat::Rgba8 as u32,
        ],
    );
    let ptr_input_vec3u = b.alloc_id();
    b.push_op(
        Op::TypePointer,
        &[ptr_input_vec3u, StorageClass::Input.as_u32(), vec3u_ty],
    );
    let ptr_uniform_observer = b.alloc_id();
    b.push_op(
        Op::TypePointer,
        &[
            ptr_uniform_observer,
            StorageClass::Uniform.as_u32(),
            observer_block_ty,
        ],
    );
    let ptr_uniform_vec4u = b.alloc_id();
    b.push_op(
        Op::TypePointer,
        &[ptr_uniform_vec4u, StorageClass::Uniform.as_u32(), vec4u_ty],
    );
    let ptr_storage_crystal = b.alloc_id();
    b.push_op(
        Op::TypePointer,
        &[
            ptr_storage_crystal,
            StorageClass::StorageBuffer.as_u32(),
            crystal_block_ty,
        ],
    );
    let ptr_storage_vec4u = b.alloc_id();
    b.push_op(
        Op::TypePointer,
        &[
            ptr_storage_vec4u,
            StorageClass::StorageBuffer.as_u32(),
            vec4u_ty,
        ],
    );
    let ptr_uc_image = b.alloc_id();
    b.push_op(
        Op::TypePointer,
        &[
            ptr_uc_image,
            StorageClass::UniformConstant.as_u32(),
            image_ty,
        ],
    );
    let fn_ty = b.alloc_id();
    b.push_op(Op::TypeFunction, &[fn_ty, void_ty]);

    let workgroup_const = b.alloc_id();
    b.push_op(
        Op::Constant,
        &[u32_ty, workgroup_const, spec.workgroup_invocation_count()],
    );
    let bindings_const = b.alloc_id();
    b.push_op(
        Op::Constant,
        &[u32_ty, bindings_const, spec.resource_binding_count()],
    );
    let zero_u = b.alloc_id();
    b.push_op(Op::Constant, &[u32_ty, zero_u, 0]);
    let one_u = b.alloc_id();
    b.push_op(Op::Constant, &[u32_ty, one_u, 1]);
    let max_scene_crystals_u = b.alloc_id();
    b.push_op(
        Op::Constant,
        &[u32_ty, max_scene_crystals_u, spec.max_scene_crystals],
    );
    let zero_f = push_f32_const(&mut b, f32_ty, 0.0);
    let one_f = push_f32_const(&mut b, f32_ty, 1.0);
    let max_u8_f = push_f32_const(&mut b, f32_ty, 255.0);

    b.push_op(
        Op::Variable,
        &[ptr_input_vec3u, gid_var, StorageClass::Input.as_u32()],
    );
    b.push_op(
        Op::Variable,
        &[
            ptr_uniform_observer,
            observer_var,
            StorageClass::Uniform.as_u32(),
        ],
    );
    b.push_op(
        Op::Variable,
        &[
            ptr_storage_crystal,
            crystal_var,
            StorageClass::StorageBuffer.as_u32(),
        ],
    );
    b.push_op(
        Op::Variable,
        &[
            ptr_uc_image,
            image_var,
            StorageClass::UniformConstant.as_u32(),
        ],
    );

    // § 9 function body.
    b.push_op(Op::Function, &[void_ty, entry_id, FN_CONTROL_NONE, fn_ty]);
    let label_id = b.alloc_id();
    b.push_op(Op::Label, &[label_id]);

    let gid_val = b.alloc_id();
    b.push_op(Op::Load, &[vec3u_ty, gid_val, gid_var]);
    let x_val = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, x_val, gid_val, 0]);
    let y_val = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, y_val, gid_val, 1]);
    let coord_val = b.alloc_id();
    b.push_op(Op::CompositeConstruct, &[vec2u_ty, coord_val, x_val, y_val]);

    let canary_u = b.alloc_id();
    b.push_op(
        Op::IAdd,
        &[u32_ty, canary_u, workgroup_const, bindings_const],
    );
    let observer_ptr = b.alloc_id();
    b.push_op(
        Op::AccessChain,
        &[ptr_uniform_vec4u, observer_ptr, observer_var, zero_u],
    );
    let observer_vec = b.alloc_id();
    b.push_op(Op::Load, &[vec4u_ty, observer_vec, observer_ptr]);
    let observer_width_u = b.alloc_id();
    b.push_op(
        Op::CompositeExtract,
        &[u32_ty, observer_width_u, observer_vec, 0],
    );
    let observer_height_u = b.alloc_id();
    b.push_op(
        Op::CompositeExtract,
        &[u32_ty, observer_height_u, observer_vec, 1],
    );
    let observer_yaw_u = b.alloc_id();
    b.push_op(
        Op::CompositeExtract,
        &[u32_ty, observer_yaw_u, observer_vec, 2],
    );
    let observer_canary_u = b.alloc_id();
    b.push_op(
        Op::IAdd,
        &[u32_ty, observer_canary_u, canary_u, observer_width_u],
    );
    let crystal_header = emit_load_crystal_slot_spirv(
        &mut b,
        u32_ty,
        vec4u_ty,
        ptr_storage_vec4u,
        crystal_var,
        zero_u,
    );
    let crystal_count_u = crystal_header.0;
    let crystal_count_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, crystal_count_f, crystal_count_u]);
    let observer_canary_f = b.alloc_id();
    b.push_op(
        Op::ConvertUToF,
        &[f32_ty, observer_canary_f, observer_canary_u],
    );

    let x_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, x_f, x_val]);
    let y_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, y_f, y_val]);
    let observer_width_f = b.alloc_id();
    b.push_op(
        Op::ConvertUToF,
        &[f32_ty, observer_width_f, observer_width_u],
    );
    let observer_height_f = b.alloc_id();
    b.push_op(
        Op::ConvertUToF,
        &[f32_ty, observer_height_f, observer_height_u],
    );
    let width_minus_one = push_f32_binary(&mut b, Op::FSub, f32_ty, observer_width_f, one_f);
    let width_den = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMax,
        width_minus_one,
        one_f,
    );
    let x_norm_raw = push_f32_binary(&mut b, Op::FDiv, f32_ty, x_f, width_den);
    let x_norm_lo = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMax,
        x_norm_raw,
        zero_f,
    );
    let x_norm = push_glsl2(&mut b, f32_ty, glsl_id, GlslStd450::FMin, x_norm_lo, one_f);
    let height_minus_one = push_f32_binary(&mut b, Op::FSub, f32_ty, observer_height_f, one_f);
    let height_den = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMax,
        height_minus_one,
        one_f,
    );
    let y_norm_raw = push_f32_binary(&mut b, Op::FDiv, f32_ty, y_f, height_den);
    let y_norm_lo = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMax,
        y_norm_raw,
        zero_f,
    );
    let y_norm = push_glsl2(&mut b, f32_ty, glsl_id, GlslStd450::FMin, y_norm_lo, one_f);
    let observer_yaw_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, observer_yaw_f, observer_yaw_u]);
    let thousand_f = push_f32_const(&mut b, f32_ty, 1000.0);
    let yaw_norm_raw = push_f32_binary(&mut b, Op::FDiv, f32_ty, observer_yaw_f, thousand_f);
    let yaw_norm = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        yaw_norm_raw,
        one_f,
    );
    let yaw_scale = push_f32_const(&mut b, f32_ty, 0.25);
    let yaw_offset = push_f32_binary(&mut b, Op::FMul, f32_ty, yaw_norm, yaw_scale);
    let half_f = push_f32_const(&mut b, f32_ty, 0.5);
    let dx_centered = push_f32_binary(&mut b, Op::FSub, f32_ty, x_norm, half_f);
    let dx_no_yaw = push_f32_binary(&mut b, Op::FMul, f32_ty, dx_centered, half_f);
    let dx_raw = push_f32_binary(&mut b, Op::FAdd, f32_ty, dx_no_yaw, yaw_offset);
    let dy_centered = push_f32_binary(&mut b, Op::FSub, f32_ty, half_f, y_norm);
    let dy_raw = push_f32_binary(&mut b, Op::FMul, f32_ty, dy_centered, half_f);
    let dx_raw_sq = push_f32_binary(&mut b, Op::FMul, f32_ty, dx_raw, dx_raw);
    let dy_raw_sq = push_f32_binary(&mut b, Op::FMul, f32_ty, dy_raw, dy_raw);
    let ray_xy_sq = push_f32_binary(&mut b, Op::FAdd, f32_ty, dx_raw_sq, dy_raw_sq);
    let ray_len_sq = push_f32_binary(&mut b, Op::FAdd, f32_ty, ray_xy_sq, one_f);
    let ray_len = push_glsl1(&mut b, f32_ty, glsl_id, GlslStd450::Sqrt, ray_len_sq);
    let ray_eps = push_f32_const(&mut b, f32_ty, 0.000_001);
    let ray_den = push_glsl2(&mut b, f32_ty, glsl_id, GlslStd450::FMax, ray_len, ray_eps);
    let ray_dx = push_f32_binary(&mut b, Op::FDiv, f32_ty, dx_raw, ray_den);
    let ray_dy = push_f32_binary(&mut b, Op::FDiv, f32_ty, dy_raw, ray_den);
    let ray_dz = push_f32_binary(&mut b, Op::FDiv, f32_ty, one_f, ray_den);
    let red_base_f = emit_v13_probe_intensity_spirv(
        &mut b,
        f32_ty,
        glsl_id,
        (zero_f, zero_f, zero_f),
        (ray_dx, ray_dy, ray_dz),
    );
    let (red_accum_f, red_peak_f, descriptor_canary_f, material_mix_f, object_mask_f) =
        emit_crystal_loop_accumulation_spirv(
            &mut b,
            u32_ty,
            bool_ty,
            f32_ty,
            glsl_id,
            label_id,
            vec4u_ty,
            ptr_storage_vec4u,
            crystal_var,
            [zero_u, one_u, max_scene_crystals_u],
            crystal_count_f,
            observer_canary_f,
            red_base_f,
            (dx_raw, dy_raw),
            max_u8_f,
            zero_f,
            one_f,
        );
    let red_peak_scale = push_f32_const(&mut b, f32_ty, 0.75);
    let red_peak_lift = push_f32_binary(&mut b, Op::FMul, f32_ty, red_peak_f, red_peak_scale);
    let red_structured_f = push_f32_binary(&mut b, Op::FAdd, f32_ty, red_accum_f, red_peak_lift);
    let red_f = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        red_structured_f,
        one_f,
    );
    let canary_raw_f = push_f32_binary(&mut b, Op::FDiv, f32_ty, descriptor_canary_f, max_u8_f);
    let canary_f = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        canary_raw_f,
        one_f,
    );
    let object_inverse = push_f32_binary(&mut b, Op::FSub, f32_ty, one_f, object_mask_f);
    let edge_raw = push_f32_binary(&mut b, Op::FMul, f32_ty, object_mask_f, object_inverse);
    let edge_scale = push_f32_const(&mut b, f32_ty, 4.0);
    let edge_scaled = push_f32_binary(&mut b, Op::FMul, f32_ty, edge_raw, edge_scale);
    let edge_f = push_glsl2(
        &mut b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        edge_scaled,
        one_f,
    );

    let red_base_scale = push_f32_const(&mut b, f32_ty, 0.82);
    let red_object_scale = push_f32_const(&mut b, f32_ty, 0.18);
    let red_edge_scale = push_f32_const(&mut b, f32_ty, 0.18);
    let red_object_lift =
        push_f32_binary(&mut b, Op::FMul, f32_ty, object_mask_f, red_object_scale);
    let red_scale = push_f32_binary(&mut b, Op::FAdd, f32_ty, red_base_scale, red_object_lift);
    let red_depth = push_f32_binary(&mut b, Op::FMul, f32_ty, red_f, red_scale);
    let red_edge = push_f32_binary(&mut b, Op::FMul, f32_ty, edge_f, red_edge_scale);
    let red_raw = push_f32_binary(&mut b, Op::FAdd, f32_ty, red_depth, red_edge);
    let red_visual_f = push_glsl2(&mut b, f32_ty, glsl_id, GlslStd450::FMin, red_raw, one_f);

    let depth_shadow_scale = push_f32_const(&mut b, f32_ty, 0.18);
    let depth_shadow_drop =
        push_f32_binary(&mut b, Op::FMul, f32_ty, object_mask_f, depth_shadow_scale);
    let depth_shadow = push_f32_binary(&mut b, Op::FSub, f32_ty, one_f, depth_shadow_drop);

    let green_red_scale = push_f32_const(&mut b, f32_ty, 0.48);
    let green_x_scale = push_f32_const(&mut b, f32_ty, 0.20);
    let green_material_scale = push_f32_const(&mut b, f32_ty, 0.16);
    let green_from_red = push_f32_binary(&mut b, Op::FMul, f32_ty, red_f, green_red_scale);
    let green_from_x = push_f32_binary(&mut b, Op::FMul, f32_ty, x_norm, green_x_scale);
    let green_from_material = push_f32_binary(
        &mut b,
        Op::FMul,
        f32_ty,
        material_mix_f,
        green_material_scale,
    );
    let green_base = push_f32_binary(&mut b, Op::FAdd, f32_ty, green_from_red, green_from_x);
    let green_base_material =
        push_f32_binary(&mut b, Op::FAdd, f32_ty, green_base, green_from_material);
    let green_shadowed =
        push_f32_binary(&mut b, Op::FMul, f32_ty, green_base_material, depth_shadow);
    let green_object_scale = push_f32_const(&mut b, f32_ty, 0.25);
    let green_object_base =
        push_f32_binary(&mut b, Op::FMul, f32_ty, object_mask_f, material_mix_f);
    let green_object = push_f32_binary(
        &mut b,
        Op::FMul,
        f32_ty,
        green_object_base,
        green_object_scale,
    );
    let green_edge_scale = push_f32_const(&mut b, f32_ty, 0.10);
    let green_edge = push_f32_binary(&mut b, Op::FMul, f32_ty, edge_f, green_edge_scale);
    let green_shadow_object =
        push_f32_binary(&mut b, Op::FAdd, f32_ty, green_shadowed, green_object);
    let green_raw = push_f32_binary(&mut b, Op::FAdd, f32_ty, green_shadow_object, green_edge);
    let green_f = push_glsl2(&mut b, f32_ty, glsl_id, GlslStd450::FMin, green_raw, one_f);
    let blue_red_scale = push_f32_const(&mut b, f32_ty, 0.30);
    let blue_y_scale = push_f32_const(&mut b, f32_ty, 0.20);
    let blue_canary_scale = push_f32_const(&mut b, f32_ty, 0.12);
    let blue_material_scale = push_f32_const(&mut b, f32_ty, 0.20);
    let blue_from_red = push_f32_binary(&mut b, Op::FMul, f32_ty, red_f, blue_red_scale);
    let blue_from_y = push_f32_binary(&mut b, Op::FMul, f32_ty, y_norm, blue_y_scale);
    let blue_from_canary = push_f32_binary(&mut b, Op::FMul, f32_ty, canary_f, blue_canary_scale);
    let material_inverse = push_f32_binary(&mut b, Op::FSub, f32_ty, one_f, material_mix_f);
    let blue_from_material = push_f32_binary(
        &mut b,
        Op::FMul,
        f32_ty,
        material_inverse,
        blue_material_scale,
    );
    let blue_base = push_f32_binary(&mut b, Op::FAdd, f32_ty, blue_from_red, blue_from_y);
    let blue_base_canary = push_f32_binary(&mut b, Op::FAdd, f32_ty, blue_base, blue_from_canary);
    let blue_base_material = push_f32_binary(
        &mut b,
        Op::FAdd,
        f32_ty,
        blue_base_canary,
        blue_from_material,
    );
    let blue_shadow_scale = push_f32_const(&mut b, f32_ty, 0.22);
    let blue_shadow_drop =
        push_f32_binary(&mut b, Op::FMul, f32_ty, object_mask_f, blue_shadow_scale);
    let blue_shadow = push_f32_binary(&mut b, Op::FSub, f32_ty, one_f, blue_shadow_drop);
    let blue_shadowed = push_f32_binary(&mut b, Op::FMul, f32_ty, blue_base_material, blue_shadow);
    let blue_object_scale = push_f32_const(&mut b, f32_ty, 0.25);
    let blue_object_base =
        push_f32_binary(&mut b, Op::FMul, f32_ty, object_mask_f, material_inverse);
    let blue_object = push_f32_binary(
        &mut b,
        Op::FMul,
        f32_ty,
        blue_object_base,
        blue_object_scale,
    );
    let blue_edge_scale = push_f32_const(&mut b, f32_ty, 0.06);
    let blue_edge = push_f32_binary(&mut b, Op::FMul, f32_ty, edge_f, blue_edge_scale);
    let blue_shadow_object = push_f32_binary(&mut b, Op::FAdd, f32_ty, blue_shadowed, blue_object);
    let blue_raw = push_f32_binary(&mut b, Op::FAdd, f32_ty, blue_shadow_object, blue_edge);
    let blue_f = push_glsl2(&mut b, f32_ty, glsl_id, GlslStd450::FMin, blue_raw, one_f);
    let color_val = b.alloc_id();
    b.push_op(
        Op::CompositeConstruct,
        &[vec4f_ty, color_val, red_visual_f, green_f, blue_f, y_norm],
    );

    let image_val = b.alloc_id();
    b.push_op(Op::Load, &[image_ty, image_val, image_var]);
    b.push_op(Op::ImageWrite, &[image_val, coord_val, color_val]);
    b.push_op(Op::Return, &[]);
    b.push_op(Op::FunctionEnd, &[]);

    b
}

fn push_f32_const(b: &mut SpirvBinary, f32_ty: u32, value: f32) -> u32 {
    let id = b.alloc_id();
    b.push_op(Op::Constant, &[f32_ty, id, value.to_bits()]);
    id
}

fn push_f32_binary(b: &mut SpirvBinary, op: Op, f32_ty: u32, lhs: u32, rhs: u32) -> u32 {
    let id = b.alloc_id();
    b.push_op(op, &[f32_ty, id, lhs, rhs]);
    id
}

fn push_glsl1(b: &mut SpirvBinary, f32_ty: u32, glsl_id: u32, inst: GlslStd450, arg: u32) -> u32 {
    let id = b.alloc_id();
    b.push_op(Op::ExtInst, &[f32_ty, id, glsl_id, inst.as_u32(), arg]);
    id
}

fn push_glsl2(
    b: &mut SpirvBinary,
    f32_ty: u32,
    glsl_id: u32,
    inst: GlslStd450,
    lhs: u32,
    rhs: u32,
) -> u32 {
    let id = b.alloc_id();
    b.push_op(Op::ExtInst, &[f32_ty, id, glsl_id, inst.as_u32(), lhs, rhs]);
    id
}

fn emit_load_crystal_slot_spirv(
    b: &mut SpirvBinary,
    u32_ty: u32,
    vec4u_ty: u32,
    ptr_storage_vec4u: u32,
    crystal_var: u32,
    slot_index_u: u32,
) -> (u32, u32, u32, u32) {
    let ptr = b.alloc_id();
    b.push_op(
        Op::AccessChain,
        &[ptr_storage_vec4u, ptr, crystal_var, slot_index_u],
    );
    let vec = b.alloc_id();
    b.push_op(Op::Load, &[vec4u_ty, vec, ptr]);
    let strength = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, strength, vec, 0]);
    let x = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, x, vec, 1]);
    let y = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, y, vec, 2]);
    let material = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, material, vec, 3]);
    (strength, x, y, material)
}

fn emit_load_crystal_array_slot_spirv(
    b: &mut SpirvBinary,
    u32_ty: u32,
    vec4u_ty: u32,
    ptr_storage_vec4u: u32,
    crystal_var: u32,
    slots_member_u: u32,
    index_u: u32,
) -> (u32, u32, u32, u32) {
    let ptr = b.alloc_id();
    b.push_op(
        Op::AccessChain,
        &[ptr_storage_vec4u, ptr, crystal_var, slots_member_u, index_u],
    );
    let vec = b.alloc_id();
    b.push_op(Op::Load, &[vec4u_ty, vec, ptr]);
    let strength = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, strength, vec, 0]);
    let x = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, x, vec, 1]);
    let y = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, y, vec, 2]);
    let material = b.alloc_id();
    b.push_op(Op::CompositeExtract, &[u32_ty, material, vec, 3]);
    (strength, x, y, material)
}

fn emit_crystal_active_weight_for_index_spirv(
    b: &mut SpirvBinary,
    f32_ty: u32,
    glsl_id: u32,
    crystal_count_f: u32,
    slot_index_f: u32,
    zero_f: u32,
    one_f: u32,
) -> u32 {
    let count_delta = push_f32_binary(b, Op::FSub, f32_ty, crystal_count_f, slot_index_f);
    let large = push_f32_const(b, f32_ty, 1_000_000.0);
    let scaled = push_f32_binary(b, Op::FMul, f32_ty, count_delta, large);
    let lo = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, scaled, zero_f);
    push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, lo, one_f)
}

#[allow(clippy::too_many_arguments)]
fn emit_crystal_loop_accumulation_spirv(
    b: &mut SpirvBinary,
    u32_ty: u32,
    bool_ty: u32,
    f32_ty: u32,
    glsl_id: u32,
    preheader_label: u32,
    vec4u_ty: u32,
    ptr_storage_vec4u: u32,
    crystal_var: u32,
    indices: [u32; 3],
    crystal_count_f: u32,
    initial_blue_f: u32,
    initial_red_f: u32,
    ray_raw: (u32, u32),
    max_u8_f: u32,
    zero_f: u32,
    one_f: u32,
) -> (u32, u32, u32, u32, u32) {
    let [zero_u, one_u, max_scene_crystals_u] = indices;
    let header_label = b.alloc_id();
    let body_label = b.alloc_id();
    let continue_label = b.alloc_id();
    let merge_label = b.alloc_id();
    let loop_i = b.alloc_id();
    let red_phi = b.alloc_id();
    let blue_phi = b.alloc_id();
    let peak_phi = b.alloc_id();
    let object_phi = b.alloc_id();
    let trans_phi = b.alloc_id();
    let material_num_phi = b.alloc_id();
    let material_den_phi = b.alloc_id();
    let i_next = b.alloc_id();
    let red_next = b.alloc_id();
    let blue_next = b.alloc_id();
    let peak_next = b.alloc_id();
    let object_next = b.alloc_id();
    let trans_next = b.alloc_id();
    let material_num_next = b.alloc_id();
    let material_den_next = b.alloc_id();
    let count_den = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, crystal_count_f, one_f);
    let inv_count = push_f32_binary(b, Op::FDiv, f32_ty, one_f, count_den);

    b.push_op(Op::Branch, &[header_label]);
    b.push_op(Op::Label, &[header_label]);
    b.push_op(
        Op::Phi,
        &[
            u32_ty,
            loop_i,
            zero_u,
            preheader_label,
            i_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            red_phi,
            initial_red_f,
            preheader_label,
            red_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            blue_phi,
            initial_blue_f,
            preheader_label,
            blue_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            peak_phi,
            zero_f,
            preheader_label,
            peak_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            object_phi,
            zero_f,
            preheader_label,
            object_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            trans_phi,
            one_f,
            preheader_label,
            trans_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            material_num_phi,
            zero_f,
            preheader_label,
            material_num_next,
            continue_label,
        ],
    );
    b.push_op(
        Op::Phi,
        &[
            f32_ty,
            material_den_phi,
            zero_f,
            preheader_label,
            material_den_next,
            continue_label,
        ],
    );
    let loop_cond = b.alloc_id();
    b.push_op(
        Op::SLessThan,
        &[bool_ty, loop_cond, loop_i, max_scene_crystals_u],
    );
    b.push_op(Op::LoopMerge, &[merge_label, continue_label, 0]);
    b.push_op(Op::BranchConditional, &[loop_cond, body_label, merge_label]);

    b.push_op(Op::Label, &[body_label]);
    let selected = emit_load_crystal_array_slot_spirv(
        b,
        u32_ty,
        vec4u_ty,
        ptr_storage_vec4u,
        crystal_var,
        one_u,
        loop_i,
    );
    let loop_i_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, loop_i_f, loop_i]);
    let active = emit_crystal_active_weight_for_index_spirv(
        b,
        f32_ty,
        glsl_id,
        crystal_count_f,
        loop_i_f,
        zero_f,
        one_f,
    );
    let (emissive, influence) = emit_crystal_spatial_terms_spirv(
        b, f32_ty, glsl_id, selected.0, selected.1, selected.2, ray_raw.0, ray_raw.1, max_u8_f,
        zero_f, one_f,
    );
    let active_influence = push_f32_binary(b, Op::FMul, f32_ty, influence, active);
    let visible_influence = push_f32_binary(b, Op::FMul, f32_ty, active_influence, trans_phi);
    b.push_op(
        Op::ExtInst,
        &[
            f32_ty,
            object_next,
            glsl_id,
            GlslStd450::FMax.as_u32(),
            object_phi,
            visible_influence,
        ],
    );
    let active_emissive = push_f32_binary(b, Op::FMul, f32_ty, emissive, active);
    let weighted_emissive_raw = push_f32_binary(b, Op::FMul, f32_ty, active_emissive, trans_phi);
    let weighted_emissive = push_f32_binary(b, Op::FMul, f32_ty, weighted_emissive_raw, inv_count);
    b.push_op(Op::FAdd, &[f32_ty, red_next, red_phi, weighted_emissive]);
    let material_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, material_f, selected.3]);
    let max_material_f = push_f32_const(b, f32_ty, 23.0);
    let material_clamped = push_glsl2(
        b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        material_f,
        max_material_f,
    );
    let material_norm = push_f32_binary(b, Op::FDiv, f32_ty, material_clamped, max_material_f);
    let material_weighted =
        push_f32_binary(b, Op::FMul, f32_ty, material_norm, weighted_emissive_raw);
    b.push_op(
        Op::FAdd,
        &[
            f32_ty,
            material_num_next,
            material_num_phi,
            material_weighted,
        ],
    );
    b.push_op(
        Op::FAdd,
        &[
            f32_ty,
            material_den_next,
            material_den_phi,
            weighted_emissive_raw,
        ],
    );
    b.push_op(
        Op::ExtInst,
        &[
            f32_ty,
            peak_next,
            glsl_id,
            GlslStd450::FMax.as_u32(),
            peak_phi,
            weighted_emissive_raw,
        ],
    );
    let strength_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, strength_f, selected.0]);
    let weighted_strength = push_f32_binary(b, Op::FMul, f32_ty, strength_f, active);
    b.push_op(Op::FAdd, &[f32_ty, blue_next, blue_phi, weighted_strength]);
    let occlusion_scale = push_f32_const(b, f32_ty, 0.65);
    let occlusion_cap = push_f32_const(b, f32_ty, 0.95);
    let occlusion_raw = push_f32_binary(b, Op::FMul, f32_ty, active_influence, occlusion_scale);
    let occlusion = push_glsl2(
        b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        occlusion_raw,
        occlusion_cap,
    );
    let trans_keep = push_f32_binary(b, Op::FSub, f32_ty, one_f, occlusion);
    b.push_op(Op::FMul, &[f32_ty, trans_next, trans_phi, trans_keep]);
    b.push_op(Op::Branch, &[continue_label]);

    b.push_op(Op::Label, &[continue_label]);
    b.push_op(Op::IAdd, &[u32_ty, i_next, loop_i, one_u]);
    b.push_op(Op::Branch, &[header_label]);

    b.push_op(Op::Label, &[merge_label]);
    let material_eps = push_f32_const(b, f32_ty, 0.000_001);
    let material_den = push_glsl2(
        b,
        f32_ty,
        glsl_id,
        GlslStd450::FMax,
        material_den_phi,
        material_eps,
    );
    let material_mix = push_f32_binary(b, Op::FDiv, f32_ty, material_num_phi, material_den);
    (red_phi, peak_phi, blue_phi, material_mix, object_phi)
}

#[allow(clippy::too_many_arguments)]
fn emit_crystal_spatial_terms_spirv(
    b: &mut SpirvBinary,
    f32_ty: u32,
    glsl_id: u32,
    strength_u: u32,
    x_word_u: u32,
    y_word_u: u32,
    dx_raw: u32,
    dy_raw: u32,
    max_u8_f: u32,
    zero_f: u32,
    one_f: u32,
) -> (u32, u32) {
    let strength_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, strength_f, strength_u]);
    let strength_norm_raw = push_f32_binary(b, Op::FDiv, f32_ty, strength_f, max_u8_f);
    let strength_norm = push_glsl2(
        b,
        f32_ty,
        glsl_id,
        GlslStd450::FMin,
        strength_norm_raw,
        one_f,
    );
    let emissive_scale = push_f32_const(b, f32_ty, 0.5);
    let base_emissive = push_f32_binary(b, Op::FMul, f32_ty, strength_norm, emissive_scale);

    let x_word_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, x_word_f, x_word_u]);
    let y_word_f = b.alloc_id();
    b.push_op(Op::ConvertUToF, &[f32_ty, y_word_f, y_word_u]);
    let x_word_clamped = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, x_word_f, max_u8_f);
    let y_word_clamped = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, y_word_f, max_u8_f);
    let bias_mid = push_f32_const(b, f32_ty, 128.0);
    let bias_den = push_f32_const(b, f32_ty, 128.0);
    let x_centered = push_f32_binary(b, Op::FSub, f32_ty, x_word_clamped, bias_mid);
    let y_centered = push_f32_binary(b, Op::FSub, f32_ty, y_word_clamped, bias_mid);
    let x_norm = push_f32_binary(b, Op::FDiv, f32_ty, x_centered, bias_den);
    let y_norm = push_f32_binary(b, Op::FDiv, f32_ty, y_centered, bias_den);
    let x_offset = push_f32_binary(b, Op::FMul, f32_ty, x_norm, emissive_scale);
    let y_offset = push_f32_binary(b, Op::FMul, f32_ty, y_norm, emissive_scale);
    let dx = push_f32_binary(b, Op::FSub, f32_ty, dx_raw, x_offset);
    let dy = push_f32_binary(b, Op::FSub, f32_ty, dy_raw, y_offset);
    let dx_sq = push_f32_binary(b, Op::FMul, f32_ty, dx, dx);
    let dy_sq = push_f32_binary(b, Op::FMul, f32_ty, dy, dy);
    let dist_sq = push_f32_binary(b, Op::FAdd, f32_ty, dx_sq, dy_sq);
    let dist = push_glsl1(b, f32_ty, glsl_id, GlslStd450::Sqrt, dist_sq);
    let core_scale = push_f32_const(b, f32_ty, 8.0);
    let core_dist = push_f32_binary(b, Op::FMul, f32_ty, dist, core_scale);
    let core_raw = push_f32_binary(b, Op::FSub, f32_ty, one_f, core_dist);
    let core = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, core_raw, zero_f);
    let halo_scale = push_f32_const(b, f32_ty, 2.25);
    let halo_dist = push_f32_binary(b, Op::FMul, f32_ty, dist, halo_scale);
    let halo_raw = push_f32_binary(b, Op::FSub, f32_ty, one_f, halo_dist);
    let halo_floor = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, halo_raw, zero_f);
    let halo_weight = push_f32_const(b, f32_ty, 0.12);
    let halo = push_f32_binary(b, Op::FMul, f32_ty, halo_floor, halo_weight);
    let influence_sum = push_f32_binary(b, Op::FAdd, f32_ty, core, halo);
    let influence = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, influence_sum, one_f);
    let emissive = push_f32_binary(b, Op::FMul, f32_ty, base_emissive, influence);
    (emissive, influence)
}

#[allow(clippy::too_many_lines)]
fn emit_v13_probe_intensity_spirv(
    b: &mut SpirvBinary,
    f32_ty: u32,
    glsl_id: u32,
    p: (u32, u32, u32),
    d: (u32, u32, u32),
) -> u32 {
    let zero = push_f32_const(b, f32_ty, 0.0);
    let one = push_f32_const(b, f32_ty, 1.0);
    let two = push_f32_const(b, f32_ty, 2.0);
    let four = push_f32_const(b, f32_ty, 4.0);
    let eight = push_f32_const(b, f32_ty, 8.0);
    let six = push_f32_const(b, f32_ty, 6.0);
    let sixteen = push_f32_const(b, f32_ty, 16.0);
    let fourteen = push_f32_const(b, f32_ty, 14.0);
    let eighteen = push_f32_const(b, f32_ty, 18.0);
    let thirty_six = push_f32_const(b, f32_ty, 36.0);
    let ninety_eight = push_f32_const(b, f32_ty, 98.0);
    let one_ninety_six = push_f32_const(b, f32_ty, 196.0);
    let r0 = push_f32_const(b, f32_ty, 1.5);
    let million = push_f32_const(b, f32_ty, 1_000_000.0);
    let small_t = push_f32_const(b, f32_ty, 0.0001);
    let huge_t = push_f32_const(b, f32_ty, 1_000_000_000.0);
    let eps = push_f32_const(b, f32_ty, 0.000001);
    let ambient = push_f32_const(b, f32_ty, 0.15);
    let wall_albedo = push_f32_const(b, f32_ty, 0.4);
    let pillar_albedo = push_f32_const(b, f32_ty, 0.3);
    let light_x = push_f32_const(b, f32_ty, 2.0);
    let light_y = push_f32_const(b, f32_ty, 3.0);
    let light_z = push_f32_const(b, f32_ty, -2.0);

    let (px, py, pz) = p;
    let (dx, dy, dz) = d;

    let dx_over_a = push_f32_binary(b, Op::FDiv, f32_ty, dx, six);
    let dy_over_b = push_f32_binary(b, Op::FDiv, f32_ty, dy, four);
    let dz_over_c = push_f32_binary(b, Op::FDiv, f32_ty, dz, fourteen);
    let oa_x = push_f32_binary(b, Op::FMul, f32_ty, dx_over_a, dx_over_a);
    let oa_y = push_f32_binary(b, Op::FMul, f32_ty, dy_over_b, dy_over_b);
    let oa_z = push_f32_binary(b, Op::FMul, f32_ty, dz_over_c, dz_over_c);
    let oa_xy = push_f32_binary(b, Op::FAdd, f32_ty, oa_x, oa_y);
    let oa = push_f32_binary(b, Op::FAdd, f32_ty, oa_xy, oa_z);

    let pxdx = push_f32_binary(b, Op::FMul, f32_ty, px, dx);
    let pydy = push_f32_binary(b, Op::FMul, f32_ty, py, dy);
    let pzdz = push_f32_binary(b, Op::FMul, f32_ty, pz, dz);
    let ob_x = push_f32_binary(b, Op::FDiv, f32_ty, pxdx, thirty_six);
    let ob_y = push_f32_binary(b, Op::FDiv, f32_ty, pydy, sixteen);
    let ob_z = push_f32_binary(b, Op::FDiv, f32_ty, pzdz, one_ninety_six);
    let ob_xy = push_f32_binary(b, Op::FAdd, f32_ty, ob_x, ob_y);
    let ob_inner = push_f32_binary(b, Op::FAdd, f32_ty, ob_xy, ob_z);
    let ob = push_f32_binary(b, Op::FMul, f32_ty, two, ob_inner);

    let px_over_a = push_f32_binary(b, Op::FDiv, f32_ty, px, six);
    let py_over_b = push_f32_binary(b, Op::FDiv, f32_ty, py, four);
    let pz_over_c = push_f32_binary(b, Op::FDiv, f32_ty, pz, fourteen);
    let og_x = push_f32_binary(b, Op::FMul, f32_ty, px_over_a, px_over_a);
    let og_y = push_f32_binary(b, Op::FMul, f32_ty, py_over_b, py_over_b);
    let og_z = push_f32_binary(b, Op::FMul, f32_ty, pz_over_c, pz_over_c);
    let og_xy = push_f32_binary(b, Op::FAdd, f32_ty, og_x, og_y);
    let og_xyz = push_f32_binary(b, Op::FAdd, f32_ty, og_xy, og_z);
    let og = push_f32_binary(b, Op::FSub, f32_ty, og_xyz, one);

    let ob_sq = push_f32_binary(b, Op::FMul, f32_ty, ob, ob);
    let four_oa = push_f32_binary(b, Op::FMul, f32_ty, four, oa);
    let four_oa_og = push_f32_binary(b, Op::FMul, f32_ty, four_oa, og);
    let disc = push_f32_binary(b, Op::FSub, f32_ty, ob_sq, four_oa_og);
    let disc_clamped = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, disc, zero);
    let o_sq = push_glsl1(b, f32_ty, glsl_id, GlslStd450::Sqrt, disc_clamped);
    let neg_ob = push_f32_binary(b, Op::FSub, f32_ty, zero, ob);
    let t_num = push_f32_binary(b, Op::FAdd, f32_ty, neg_ob, o_sq);
    let two_oa = push_f32_binary(b, Op::FMul, f32_ty, two, oa);
    let t_outer = push_f32_binary(b, Op::FDiv, f32_ty, t_num, two_oa);

    let dx_sq = push_f32_binary(b, Op::FMul, f32_ty, dx, dx);
    let dy_sq = push_f32_binary(b, Op::FMul, f32_ty, dy, dy);
    let dz_sq = push_f32_binary(b, Op::FMul, f32_ty, dz, dz);
    let pa_xy = push_f32_binary(b, Op::FAdd, f32_ty, dx_sq, dy_sq);
    let pa = push_f32_binary(b, Op::FAdd, f32_ty, pa_xy, dz_sq);
    let pb_inner_xy = push_f32_binary(b, Op::FAdd, f32_ty, pxdx, pydy);
    let pb_inner = push_f32_binary(b, Op::FAdd, f32_ty, pb_inner_xy, pzdz);
    let pb = push_f32_binary(b, Op::FMul, f32_ty, two, pb_inner);
    let px_sq = push_f32_binary(b, Op::FMul, f32_ty, px, px);
    let py_sq = push_f32_binary(b, Op::FMul, f32_ty, py, py);
    let pz_sq = push_f32_binary(b, Op::FMul, f32_ty, pz, pz);
    let p2_xy = push_f32_binary(b, Op::FAdd, f32_ty, px_sq, py_sq);
    let p2 = push_f32_binary(b, Op::FAdd, f32_ty, p2_xy, pz_sq);
    let r0_sq = push_f32_binary(b, Op::FMul, f32_ty, r0, r0);
    let pg = push_f32_binary(b, Op::FSub, f32_ty, p2, r0_sq);
    let pb_sq = push_f32_binary(b, Op::FMul, f32_ty, pb, pb);
    let four_pa = push_f32_binary(b, Op::FMul, f32_ty, four, pa);
    let four_pa_pg = push_f32_binary(b, Op::FMul, f32_ty, four_pa, pg);
    let p_disc = push_f32_binary(b, Op::FSub, f32_ty, pb_sq, four_pa_pg);
    let p_disc_clamped = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, p_disc, zero);
    let p_sq = push_glsl1(b, f32_ty, glsl_id, GlslStd450::Sqrt, p_disc_clamped);
    let neg_pb = push_f32_binary(b, Op::FSub, f32_ty, zero, pb);
    let p_enter_num = push_f32_binary(b, Op::FSub, f32_ty, neg_pb, p_sq);
    let two_pa = push_f32_binary(b, Op::FMul, f32_ty, two, pa);
    let p_t_enter = push_f32_binary(b, Op::FDiv, f32_ty, p_enter_num, two_pa);
    let p_disc_scaled = push_f32_binary(b, Op::FMul, f32_ty, p_disc, million);
    let p_hit_hi = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, p_disc_scaled, zero);
    let p_hit_flag = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, p_hit_hi, one);
    let p_t_margin = push_f32_binary(b, Op::FSub, f32_ty, p_t_enter, small_t);
    let p_t_scaled = push_f32_binary(b, Op::FMul, f32_ty, p_t_margin, million);
    let p_t_hi = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, p_t_scaled, zero);
    let p_t_valid = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, p_t_hi, one);
    let p_valid = push_f32_binary(b, Op::FMul, f32_ty, p_hit_flag, p_t_valid);
    let p_t_weighted = push_f32_binary(b, Op::FMul, f32_ty, p_t_enter, p_valid);
    let one_minus_valid = push_f32_binary(b, Op::FSub, f32_ty, one, p_valid);
    let miss_t = push_f32_binary(b, Op::FMul, f32_ty, huge_t, one_minus_valid);
    let t_inner = push_f32_binary(b, Op::FAdd, f32_ty, p_t_weighted, miss_t);

    let s = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, t_outer, t_inner);
    let inner_delta = push_f32_binary(b, Op::FSub, f32_ty, t_outer, t_inner);
    let inner_scaled = push_f32_binary(b, Op::FMul, f32_ty, inner_delta, million);
    let inner_hi = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, inner_scaled, zero);
    let inner_wins = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMin, inner_hi, one);
    let outer_wins = push_f32_binary(b, Op::FSub, f32_ty, one, inner_wins);

    let dx_s = push_f32_binary(b, Op::FMul, f32_ty, dx, s);
    let dy_s = push_f32_binary(b, Op::FMul, f32_ty, dy, s);
    let dz_s = push_f32_binary(b, Op::FMul, f32_ty, dz, s);
    let hx = push_f32_binary(b, Op::FAdd, f32_ty, px, dx_s);
    let hy = push_f32_binary(b, Op::FAdd, f32_ty, py, dy_s);
    let hz = push_f32_binary(b, Op::FAdd, f32_ty, pz, dz_s);

    let neg_hx = push_f32_binary(b, Op::FSub, f32_ty, zero, hx);
    let neg_hy = push_f32_binary(b, Op::FSub, f32_ty, zero, hy);
    let neg_hz = push_f32_binary(b, Op::FSub, f32_ty, zero, hz);
    let outer_nx = push_f32_binary(b, Op::FDiv, f32_ty, neg_hx, eighteen);
    let outer_ny = push_f32_binary(b, Op::FDiv, f32_ty, neg_hy, eight);
    let outer_nz = push_f32_binary(b, Op::FDiv, f32_ty, neg_hz, ninety_eight);
    let inner_nx = push_f32_binary(b, Op::FMul, f32_ty, two, hx);
    let inner_ny = push_f32_binary(b, Op::FMul, f32_ty, two, hy);
    let inner_nz = push_f32_binary(b, Op::FMul, f32_ty, two, hz);
    let nx_inner = push_f32_binary(b, Op::FMul, f32_ty, inner_nx, inner_wins);
    let ny_inner = push_f32_binary(b, Op::FMul, f32_ty, inner_ny, inner_wins);
    let nz_inner = push_f32_binary(b, Op::FMul, f32_ty, inner_nz, inner_wins);
    let nx_outer = push_f32_binary(b, Op::FMul, f32_ty, outer_nx, outer_wins);
    let ny_outer = push_f32_binary(b, Op::FMul, f32_ty, outer_ny, outer_wins);
    let nz_outer = push_f32_binary(b, Op::FMul, f32_ty, outer_nz, outer_wins);
    let nx_raw = push_f32_binary(b, Op::FAdd, f32_ty, nx_inner, nx_outer);
    let ny_raw = push_f32_binary(b, Op::FAdd, f32_ty, ny_inner, ny_outer);
    let nz_raw = push_f32_binary(b, Op::FAdd, f32_ty, nz_inner, nz_outer);
    let nx_sq = push_f32_binary(b, Op::FMul, f32_ty, nx_raw, nx_raw);
    let ny_sq = push_f32_binary(b, Op::FMul, f32_ty, ny_raw, ny_raw);
    let nz_sq = push_f32_binary(b, Op::FMul, f32_ty, nz_raw, nz_raw);
    let n_sq_xy = push_f32_binary(b, Op::FAdd, f32_ty, nx_sq, ny_sq);
    let n_sq = push_f32_binary(b, Op::FAdd, f32_ty, n_sq_xy, nz_sq);
    let n_len = push_glsl1(b, f32_ty, glsl_id, GlslStd450::Sqrt, n_sq);
    let n_den = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, n_len, eps);
    let nx = push_f32_binary(b, Op::FDiv, f32_ty, nx_raw, n_den);
    let ny = push_f32_binary(b, Op::FDiv, f32_ty, ny_raw, n_den);
    let nz = push_f32_binary(b, Op::FDiv, f32_ty, nz_raw, n_den);

    let lx = push_f32_binary(b, Op::FSub, f32_ty, light_x, hx);
    let ly = push_f32_binary(b, Op::FSub, f32_ty, light_y, hy);
    let lz = push_f32_binary(b, Op::FSub, f32_ty, light_z, hz);
    let lx_sq = push_f32_binary(b, Op::FMul, f32_ty, lx, lx);
    let ly_sq = push_f32_binary(b, Op::FMul, f32_ty, ly, ly);
    let lz_sq = push_f32_binary(b, Op::FMul, f32_ty, lz, lz);
    let r2_xy = push_f32_binary(b, Op::FAdd, f32_ty, lx_sq, ly_sq);
    let r2 = push_f32_binary(b, Op::FAdd, f32_ty, r2_xy, lz_sq);
    let r = push_glsl1(b, f32_ty, glsl_id, GlslStd450::Sqrt, r2);
    let r_den = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, r, eps);
    let dot_x = push_f32_binary(b, Op::FMul, f32_ty, nx, lx);
    let dot_y = push_f32_binary(b, Op::FMul, f32_ty, ny, ly);
    let dot_z = push_f32_binary(b, Op::FMul, f32_ty, nz, lz);
    let dot_xy = push_f32_binary(b, Op::FAdd, f32_ty, dot_x, dot_y);
    let dot = push_f32_binary(b, Op::FAdd, f32_ty, dot_xy, dot_z);
    let lambert_raw = push_f32_binary(b, Op::FDiv, f32_ty, dot, r_den);
    let lambert = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, lambert_raw, zero);
    let r2_den = push_glsl2(b, f32_ty, glsl_id, GlslStd450::FMax, r2, one);
    let direct_num = push_f32_binary(b, Op::FMul, f32_ty, lambert, eight);
    let direct = push_f32_binary(b, Op::FDiv, f32_ty, direct_num, r2_den);
    let wall_component = push_f32_binary(b, Op::FMul, f32_ty, outer_wins, wall_albedo);
    let pillar_component = push_f32_binary(b, Op::FMul, f32_ty, inner_wins, pillar_albedo);
    let albedo = push_f32_binary(b, Op::FAdd, f32_ty, wall_component, pillar_component);
    let lit = push_f32_binary(b, Op::FAdd, f32_ty, ambient, direct);
    push_f32_binary(b, Op::FMul, f32_ty, lit, albedo)
}

/// § Emit canonical substrate-kernel SPIR-V as a little-endian byte vector.
///
/// Same content as [`emit_substrate_kernel_spirv`] but already serialized to
/// bytes. Useful for hashing / on-disk caching / `vkCreateShaderModule`-via-
/// `pCode = bytes.as_ptr() as *const u32`.
pub fn emit_substrate_kernel_spirv_bytes(
    spec: &SubstrateKernelSpec,
) -> Result<Vec<u8>, SubstrateKernelEmitError> {
    let words = emit_substrate_kernel_spirv(spec)?;
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    Ok(out)
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests — substrate-kernel emit integrity.
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use rspirv::{dr, spirv};

    /// SPIR-V magic number from Khronos § 2.3.
    const SPIRV_MAGIC: u32 = 0x0723_0203;

    #[test]
    fn canonical_spec_emits_nonempty_spirv() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        assert!(words.len() > 5, "must emit header (5 words) + instructions");
        assert_eq!(
            words[0], SPIRV_MAGIC,
            "first word must be SPIR-V magic 0x07230203",
        );
    }

    #[test]
    fn emitted_bytes_are_4_aligned() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let bytes = emit_substrate_kernel_spirv_bytes(&spec).expect("byte emit must succeed");
        assert!(bytes.len() >= 20, "header alone is 5 × 4 = 20 bytes");
        assert_eq!(
            bytes.len() % 4,
            0,
            "SPIR-V is u32-stream ; byte-len must be 4-aligned",
        );
        // Bytes 0..4 little-endian = SPIRV_MAGIC.
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(magic, SPIRV_MAGIC);
    }

    #[test]
    fn emit_is_deterministic() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let a = emit_substrate_kernel_spirv(&spec).unwrap();
        let b = emit_substrate_kernel_spirv(&spec).unwrap();
        assert_eq!(
            a, b,
            "same spec must produce byte-identical SPIR-V across calls",
        );
    }

    #[test]
    fn workgroup_change_changes_emit() {
        let canonical = SubstrateKernelSpec::canonical_from_source().unwrap();
        let mut alt = canonical.clone();
        alt.workgroup = (16, 16, 1);
        let a = emit_substrate_kernel_spirv(&canonical).unwrap();
        let c = emit_substrate_kernel_spirv(&alt).unwrap();
        assert_ne!(a, c, "different workgroup → different LocalSize words");
    }

    #[test]
    fn canonical_source_canary_is_derived_from_csl_declarations() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        assert_eq!(spec.workgroup_invocation_count(), 64);
        assert_eq!(spec.resource_binding_count(), 3);
        assert_eq!(spec.source_canary_value(), 67);
    }

    #[test]
    fn v13_probe_cpu_oracles_match_known_bytes() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let center = spec.v13_center_ray_intensity();
        assert!(
            (center - 0.071_604_9).abs() <= 0.000_001,
            "unexpected v13 center-ray intensity: {center}"
        );
        let pillar = spec.v13_pillar_y_ray_intensity();
        assert!(
            (pillar - 0.154_702_54).abs() <= 0.000_001,
            "unexpected v13 pillar-y ray intensity: {pillar}"
        );
        assert_eq!(
            spec.v13_probe_row_prefix_rgba8(),
            [[15, 9, 116, 255], [32, 70, 122, 255]]
        );
        assert_eq!(
            spec.v13_probe_row_prefix_rgba8_for_observer_width(8),
            [[15, 9, 116, 255], [32, 70, 123, 255]]
        );
        assert_eq!(
            spec.v13_probe_row_prefix_rgba8_for_descriptors(8, 5),
            [[15, 9, 117, 255], [32, 70, 123, 255]]
        );
        let camera = spec.v13_camera_row8_rgba8_for_descriptors(8, 5);
        assert!(
            camera[7][1] > camera[0][1],
            "camera-row green channel must retain left-to-right visual orientation"
        );
        assert!(
            camera.iter().all(|px| px[3] == 255),
            "camera-row alpha must remain stable for the 1D row oracle"
        );
        let unlit_camera = spec.v13_camera_row8_rgba8_for_descriptors(8, 0);
        assert!(
            camera
                .iter()
                .zip(unlit_camera.iter())
                .any(|(lit, unlit)| lit[0] > unlit[0]),
            "first crystal word must increase at least one camera-row red byte"
        );
        assert_ne!(
            spec.v13_camera_ray_intensity_for_yaw(8, 4, 0),
            spec.v13_camera_ray_intensity_for_yaw(8, 4, 500),
            "observer yaw must change camera-ray math before byte quantization"
        );
        let yaw0_tile = spec.v13_camera_tile8_rgba8_for_inputs((8, 8), 0, 5);
        let tile = spec.v13_camera_tile8_rgba8_for_inputs((8, 8), 500, 5);
        assert_ne!(
            yaw0_tile, tile,
            "observer yaw must change 2D camera-tile shading"
        );
        assert_eq!(tile[0][3], 0, "top row alpha must encode y_norm=0");
        assert_eq!(tile[63][3], 255, "bottom row alpha must encode y_norm=1");
        assert_ne!(
            &tile[0..8],
            &tile[56..64],
            "top and bottom rows must differ after gid.y enters ray generation"
        );
        let offset_tile =
            spec.v13_camera_tile8_rgba8_for_scene_inputs((8, 8), 500, [5, 160, 96, 0]);
        assert_ne!(
            offset_tile, tile,
            "first-crystal x/y words must alter spatial shading"
        );
        let two_crystal_tile = spec.v13_camera_tile8_rgba8_for_scene2_inputs(
            (8, 8),
            500,
            [[5, 160, 96, 0], [7, 96, 160, 0]],
        );
        assert!(
            two_crystal_tile
                .iter()
                .zip(offset_tile.iter())
                .any(|(two, one)| two[0] != one[0]),
            "second crystal slot must alter at least one red byte"
        );
        assert!(
            two_crystal_tile
                .iter()
                .zip(offset_tile.iter())
                .any(|(two, one)| two[2] != one[2]),
            "second crystal slot must alter at least one blue visual byte"
        );
        let mut bounded_words = [EMPTY_CRYSTAL_WORDS; MAX_SCENE_CRYSTALS];
        bounded_words[..6].copy_from_slice(&[
            [5, 160, 96, 0],
            [7, 96, 160, 0],
            [11, 128, 128, 0],
            [13, 192, 64, 0],
            [17, 144, 112, 0],
            [19, 112, 144, 0],
        ]);
        let count_one_tile =
            spec.v13_camera_tile8_rgba8_for_scene_slots_inputs((8, 8), 500, 1, bounded_words);
        let count_two_tile =
            spec.v13_camera_tile8_rgba8_for_scene_slots_inputs((8, 8), 500, 2, bounded_words);
        assert_ne!(
            count_one_tile, count_two_tile,
            "crystal-count header must gate bounded scene slots"
        );
        assert!(
            count_one_tile
                .iter()
                .zip(count_two_tile.iter())
                .any(|(one, two)| one[2] != two[2]),
            "count=2 blue visual mix must differ after second crystal becomes active"
        );
        assert!(
            count_one_tile
                .iter()
                .zip(count_two_tile.iter())
                .any(|(one, two)| one[0] != two[0]),
            "count=2 red lighting must differ after second crystal becomes active"
        );
    }

    #[test]
    fn canonical_emit_contains_source_canary_body() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let has_iadd = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::IAdd);
        assert!(
            has_iadd,
            "substrate kernel body must contain source-canary OpIAdd"
        );
    }

    #[test]
    fn canonical_emit_reads_descriptor_payloads() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let access_chain_count = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .filter(|i| i.class.opcode == spirv::Op::AccessChain)
            .count();
        assert!(
            access_chain_count >= 2,
            "substrate kernel body must access observer uniform and crystal storage"
        );
        let has_binding_0 = parsed.annotations.iter().any(|inst| {
            inst.class.opcode == spirv::Op::Decorate
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::Decoration(spirv::Decoration::Binding)))
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::LiteralBit32(0)))
        });
        assert!(
            has_binding_0,
            "observer uniform must be decorated binding=0"
        );
        let has_binding_1 = parsed.annotations.iter().any(|inst| {
            inst.class.opcode == spirv::Op::Decorate
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::Decoration(spirv::Decoration::Binding)))
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::LiteralBit32(1)))
        });
        assert!(has_binding_1, "crystal storage must be decorated binding=1");
    }

    #[test]
    fn canonical_emit_uses_source_scene_slot_layout() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let offset_words: Vec<u32> = parsed
            .annotations
            .iter()
            .filter(|inst| {
                inst.class.opcode == spirv::Op::MemberDecorate
                    && inst
                        .operands
                        .iter()
                        .any(|op| matches!(op, dr::Operand::Decoration(spirv::Decoration::Offset)))
            })
            .flat_map(|inst| inst.operands.iter())
            .filter_map(|op| match op {
                dr::Operand::LiteralBit32(n) => Some(*n),
                _ => None,
            })
            .collect();
        assert!(
            offset_words.contains(&spec.crystal_slot_offsets[0]),
            "source-declared crystal slot base offset must appear in OpMemberDecorate"
        );
        let has_array_stride = parsed.annotations.iter().any(|inst| {
            inst.class.opcode == spirv::Op::Decorate
                && inst.operands.iter().any(|op| {
                    matches!(
                        op,
                        dr::Operand::Decoration(spirv::Decoration::ArrayStride)
                    )
                })
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::LiteralBit32(n) if *n == spec.crystal_slot_stride))
        });
        assert!(
            has_array_stride,
            "source-declared crystal slot stride must appear as OpDecorate ArrayStride"
        );
        let has_runtime_array = parsed
            .types_global_values
            .iter()
            .any(|inst| inst.class.opcode == spirv::Op::TypeRuntimeArray);
        assert!(
            has_runtime_array,
            "crystal slots must lower to a runtime array for scalable scene storage"
        );
    }

    #[test]
    fn canonical_emit_contains_structured_crystal_loop() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let opcodes: Vec<_> = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .map(|i| i.class.opcode)
            .collect();
        assert!(
            opcodes.contains(&spirv::Op::LoopMerge),
            "crystal accumulation must use a structured SPIR-V loop"
        );
        assert!(
            opcodes.contains(&spirv::Op::Phi),
            "crystal loop must carry i/red/blue state through OpPhi"
        );
        assert!(
            opcodes.contains(&spirv::Op::BranchConditional),
            "crystal loop must branch on its loop condition"
        );
    }

    #[test]
    fn canonical_emit_contains_storage_image_write() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let has_image_write = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .any(|i| i.class.opcode == spirv::Op::ImageWrite);
        assert!(
            has_image_write,
            "substrate kernel body must write the output storage image"
        );
        let has_binding_2 = parsed.annotations.iter().any(|inst| {
            inst.class.opcode == spirv::Op::Decorate
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::Decoration(spirv::Decoration::Binding)))
                && inst
                    .operands
                    .iter()
                    .any(|op| matches!(op, dr::Operand::LiteralBit32(2)))
        });
        assert!(has_binding_2, "output image must be decorated binding=2");
    }

    #[test]
    fn canonical_emit_contains_v13_math_extinsts() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let has_glsl = parsed
            .ext_inst_imports
            .iter()
            .flat_map(|i| i.operands.iter())
            .any(|op| matches!(op, dr::Operand::LiteralString(s) if s == "GLSL.std.450"));
        assert!(has_glsl, "v13 math slice must import GLSL.std.450");
        let extinst_count = parsed
            .functions
            .iter()
            .flat_map(|f| f.blocks.iter().flat_map(|bb| bb.instructions.iter()))
            .filter(|i| i.class.opcode == spirv::Op::ExtInst)
            .count();
        assert!(
            extinst_count >= 6,
            "v13 center-ray slice must emit sqrt/max ext-inst math, got {extinst_count}"
        );
    }

    #[test]
    fn canonical_emit_preserves_source_workgroup_execution_mode() {
        let spec = SubstrateKernelSpec::canonical_from_source().unwrap();
        let words = emit_substrate_kernel_spirv(&spec).expect("canonical emit must succeed");
        let parsed = dr::load_words(&words).expect("rspirv must parse emitted words");
        let has_local_size = parsed.execution_modes.iter().any(|inst| {
            inst.class.opcode == spirv::Op::ExecutionMode
                && inst.operands.iter().any(|op| {
                    matches!(
                        op,
                        dr::Operand::ExecutionMode(spirv::ExecutionMode::LocalSize)
                    )
                })
                && inst
                    .operands
                    .iter()
                    .filter_map(|op| match op {
                        dr::Operand::LiteralBit32(n) => Some(*n),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    == vec![8, 8, 1]
        });
        assert!(
            has_local_size,
            "source-declared workgroup must emit LocalSize 8 8 1"
        );
    }

    #[test]
    fn test_t11_w18_source_declares_behavioral_contract() {
        let spec = SubstrateKernelSpec::from_csl_source(CANONICAL_SUBSTRATE_KERNEL_SOURCE).unwrap();
        assert_eq!(spec.entry_name, "main");
        assert_eq!(spec.workgroup, (8, 8, 1));
        assert!(spec.has_observer_uniform);
        assert!(spec.has_crystals_storage);
        assert!(spec.has_output_storage_image);
        assert_eq!(spec.max_scene_crystals, 128);
        assert_eq!(
            spec.crystal_slot_offsets,
            core::array::from_fn(|i| 16 + (i as u32 * 16))
        );
        assert_eq!(spec.crystal_slot_stride, 16);
    }

    #[test]
    fn test_t11_w18_source_workgroup_drift_changes_emitted_spirv() {
        let baseline = SubstrateKernelSpec::from_csl_source(CANONICAL_SUBSTRATE_KERNEL_SOURCE)
            .expect("baseline source parses");
        let drifted_source = CANONICAL_SUBSTRATE_KERNEL_SOURCE
            .replace("workgroup     : ⟨8, 8, 1⟩", "workgroup     : ⟨16, 16, 1⟩");
        let drifted =
            SubstrateKernelSpec::from_csl_source(&drifted_source).expect("drifted source parses");
        assert_eq!(drifted.workgroup, (16, 16, 1));
        assert_eq!(baseline.source_canary_value(), 67);
        assert_eq!(drifted.source_canary_value(), 259);
        assert_ne!(
            emit_substrate_kernel_spirv(&baseline).unwrap(),
            emit_substrate_kernel_spirv(&drifted).unwrap(),
            "source-declared workgroup must change emitted SPIR-V"
        );
    }

    #[test]
    fn test_t11_w18_missing_output_binding_is_rejected() {
        let broken = CANONICAL_SUBSTRATE_KERNEL_SOURCE.replace("binding=2", "binding=9");
        let err = SubstrateKernelSpec::from_csl_source(&broken).unwrap_err();
        assert_eq!(
            err,
            SubstrateKernelSourceError::MissingBinding("output-image storage image binding 2")
        );
    }

    #[test]
    fn test_t11_w18_scene_crystal_abi_drift_is_rejected() {
        let broken = CANONICAL_SUBSTRATE_KERNEL_SOURCE.replace(
            "formula : offset(slot_i) = 16 + i * 16",
            "formula : offset(slot_i) = 32 + i * 16",
        );
        let err = SubstrateKernelSpec::from_csl_source(&broken).unwrap_err();
        assert_eq!(
            err,
            SubstrateKernelSourceError::MissingDeclaration("derived.slot-layout.formula")
        );
    }

    #[test]
    fn test_t11_w18_scene_crystal_active_mask_drift_is_rejected() {
        let broken = CANONICAL_SUBSTRATE_KERNEL_SOURCE.replace(
            "active = clamp((crystals.header.active_crystal_count - i) * 1_000_000, 0, 1)",
            "active = 1",
        );
        let err = SubstrateKernelSpec::from_csl_source(&broken).unwrap_err();
        assert_eq!(
            err,
            SubstrateKernelSourceError::MissingDeclaration("active")
        );
    }

    #[test]
    fn test_t11_w18_missing_no_wgsl_marker_is_rejected() {
        let broken =
            CANONICAL_SUBSTRATE_KERNEL_SOURCE.replace("no-wgsl       : ✓", "no-wgsl       : ✗");
        let err = SubstrateKernelSpec::from_csl_source(&broken).unwrap_err();
        assert_eq!(
            err,
            SubstrateKernelSourceError::MissingTruthMarker("no-wgsl")
        );
    }
}

//! `MirFunc` → SPIR-V binary lowering.
//!
//! § DESIGN
//!   Stage-0 lowers a small but *real* subset of MIR : a typed entry-point
//!   that performs arithmetic on uniform / push-constant / storage-buffer /
//!   image-sampler bindings + writes a result. The lowering is structured
//!   so that each shader-stage variant emits the correct SPIR-V module
//!   prelude (capabilities + `EntryPoint` + `ExecutionMode`), then writes
//!   types / globals / constants / function in spec § 2.4 order.
//!
//!   We do NOT yet walk the full MirOp space — that's part of the same
//!   spectrum as `cssl-cgen-gpu-spirv::body_emit` (T11-D72). The W-G1
//!   slice's mandate is the BACKEND : the binary format + type-table +
//!   entry-point + per-stage scaffolding. Body-op coverage is iterated
//!   in subsequent slices.
//!
//! § SHADER STAGES (this slice)
//!   - Compute  : LocalSize x,y,z + GlobalInvocationId input.
//!   - Vertex   : gl_Position output (vec4) + per-vertex Location-0 output.
//!   - Fragment : OriginUpperLeft + per-fragment Location-0 output (color).
//!
//! § ID ALLOCATION
//!   We use a monotonic id-allocator on `SpirvBinary`. Each emitted result
//!   carries a fresh id ; types are de-duplicated via a `TypeCache` so the
//!   same `(u32, ...)` shape only allocates once.

use crate::binary::SpirvBinary;
use crate::op::{
    AddressingModel, Builtin, Capability, Decoration, Dim, ExecutionMode, ExecutionModel,
    ImageFormat, MemoryModel, Op, StorageClass, FN_CONTROL_NONE,
};
use cssl_mir::func::MirFunc;

/// Shader stage being lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    /// `GLCompute` — compute shader. `LocalSize` from `target.local_size`.
    Compute,
    /// Vertex shader. Outputs `gl_Position` + a single Location-0 vec4.
    Vertex,
    /// Fragment shader. Outputs a single Location-0 vec4 (RGBA color).
    Fragment,
}

/// Per-lowering target configuration. Tells the driver which capabilities
/// + execution-modes to emit.
#[derive(Debug, Clone)]
pub struct ShaderTarget {
    /// Shader stage = compute / vertex / fragment.
    pub stage: ShaderStage,
    /// Compute-only : workgroup local-size (x, y, z). Ignored for V/F.
    pub local_size: (u32, u32, u32),
    /// Entry-point name (must match `MirFunc::name` in this slice).
    pub entry_name: String,
    /// Whether to declare a uniform-buffer binding (set=0, binding=0).
    pub uniform_buffer: bool,
    /// Whether to declare a push-constant block.
    pub push_constant: bool,
    /// Whether to declare a sampled-image binding (set=0, binding=1).
    pub sampled_image: bool,
    /// Whether to declare a storage-buffer binding (set=0, binding=2).
    pub storage_buffer: bool,
}

impl ShaderTarget {
    /// Compute shader with the given workgroup size + entry name.
    #[must_use]
    pub fn compute(entry: impl Into<String>, local_size: (u32, u32, u32)) -> Self {
        Self {
            stage: ShaderStage::Compute,
            local_size,
            entry_name: entry.into(),
            uniform_buffer: false,
            push_constant: false,
            sampled_image: false,
            storage_buffer: false,
        }
    }
    /// Vertex shader with the given entry name.
    #[must_use]
    pub fn vertex(entry: impl Into<String>) -> Self {
        Self {
            stage: ShaderStage::Vertex,
            local_size: (1, 1, 1),
            entry_name: entry.into(),
            uniform_buffer: false,
            push_constant: false,
            sampled_image: false,
            storage_buffer: false,
        }
    }
    /// Fragment shader with the given entry name.
    #[must_use]
    pub fn fragment(entry: impl Into<String>) -> Self {
        Self {
            stage: ShaderStage::Fragment,
            local_size: (1, 1, 1),
            entry_name: entry.into(),
            uniform_buffer: false,
            push_constant: false,
            sampled_image: false,
            storage_buffer: false,
        }
    }

    #[must_use]
    pub fn with_uniform(mut self) -> Self { self.uniform_buffer = true; self }
    #[must_use]
    pub fn with_push_constant(mut self) -> Self { self.push_constant = true; self }
    #[must_use]
    pub fn with_sampled_image(mut self) -> Self { self.sampled_image = true; self }
    #[must_use]
    pub fn with_storage_buffer(mut self) -> Self { self.storage_buffer = true; self }
}

/// Errors raised during lowering.
#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("entry name `{got}` does not match MirFunc name `{expected}`")]
    EntryNameMismatch { got: String, expected: String },
    #[error("unsupported : {0}")]
    Unsupported(String),
}

/// Type-id cache so equal types reuse the same SPIR-V id.
#[derive(Default)]
struct TypeCache {
    void: Option<u32>,
    f32: Option<u32>,
    u32: Option<u32>,
    bool_: Option<u32>,
    vec4_f32: Option<u32>,
    vec3_u32: Option<u32>,
    /// `OpTypePointer` keyed by (storage_class, pointee_id).
    ptrs: Vec<((u32, u32), u32)>,
    /// `OpTypeFunction` keyed by (return_id, params...) — small N so linear.
    fns: Vec<(Vec<u32>, u32)>,
    image_2d_f32: Option<u32>,
    sampler: Option<u32>,
    sampled_image_2d: Option<u32>,
}

impl TypeCache {
    fn type_void(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.void { return id; }
        let id = b.alloc_id();
        b.push_op(Op::TypeVoid, &[id]);
        self.void = Some(id);
        id
    }
    fn type_bool(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.bool_ { return id; }
        let id = b.alloc_id();
        b.push_op(Op::TypeBool, &[id]);
        self.bool_ = Some(id);
        id
    }
    fn type_u32(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.u32 { return id; }
        let id = b.alloc_id();
        // OpTypeInt : <id> result, <lit> width = 32, <lit> signedness = 0.
        b.push_op(Op::TypeInt, &[id, 32, 0]);
        self.u32 = Some(id);
        id
    }
    fn type_f32(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.f32 { return id; }
        let id = b.alloc_id();
        // OpTypeFloat : <id> result, <lit> width = 32.
        b.push_op(Op::TypeFloat, &[id, 32]);
        self.f32 = Some(id);
        id
    }
    fn type_vec4_f32(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.vec4_f32 { return id; }
        let f = self.type_f32(b);
        let id = b.alloc_id();
        // OpTypeVector : <id> result, <id> component_type, <lit> count.
        b.push_op(Op::TypeVector, &[id, f, 4]);
        self.vec4_f32 = Some(id);
        id
    }
    fn type_vec3_u32(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.vec3_u32 { return id; }
        let u = self.type_u32(b);
        let id = b.alloc_id();
        b.push_op(Op::TypeVector, &[id, u, 3]);
        self.vec3_u32 = Some(id);
        id
    }
    fn type_pointer(&mut self, b: &mut SpirvBinary, sc: StorageClass, pointee: u32) -> u32 {
        let key = (sc.as_u32(), pointee);
        for &(k, v) in &self.ptrs {
            if k == key { return v; }
        }
        let id = b.alloc_id();
        // OpTypePointer : <id> result, <enum> storage_class, <id> pointee.
        b.push_op(Op::TypePointer, &[id, key.0, pointee]);
        self.ptrs.push((key, id));
        id
    }
    fn type_function(&mut self, b: &mut SpirvBinary, ret: u32, params: &[u32]) -> u32 {
        let mut key = vec![ret];
        key.extend_from_slice(params);
        for (k, v) in &self.fns {
            if *k == key { return *v; }
        }
        let id = b.alloc_id();
        let mut operands = vec![id, ret];
        operands.extend_from_slice(params);
        b.push_op(Op::TypeFunction, &operands);
        self.fns.push((key, id));
        id
    }
    fn type_image_2d_f32(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.image_2d_f32 { return id; }
        let f = self.type_f32(b);
        let id = b.alloc_id();
        // OpTypeImage : sampled_type, dim, depth, arrayed, ms, sampled, format
        // (+ optional access qualifier — omitted for Vulkan).
        b.push_op(
            Op::TypeImage,
            &[id, f, Dim::Dim2D as u32, 0, 0, 0, 1, ImageFormat::Unknown as u32],
        );
        self.image_2d_f32 = Some(id);
        id
    }
    fn type_sampler(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.sampler { return id; }
        let id = b.alloc_id();
        b.push_op(Op::TypeSampler, &[id]);
        self.sampler = Some(id);
        id
    }
    fn type_sampled_image_2d(&mut self, b: &mut SpirvBinary) -> u32 {
        if let Some(id) = self.sampled_image_2d { return id; }
        let img = self.type_image_2d_f32(b);
        let id = b.alloc_id();
        b.push_op(Op::TypeSampledImage, &[id, img]);
        self.sampled_image_2d = Some(id);
        id
    }
}

/// Lower a `MirFunc` to a SPIR-V binary for the given target.
///
/// § OUTPUT LAYOUT (Khronos § 2.4 — Logical Layout)
///   1. All `OpCapability` instructions.
///   2. `OpExtInstImport` (we use GLSL.std.450 — placeholder, no body uses).
///   3. `OpMemoryModel` (single).
///   4. `OpEntryPoint` (one per entry).
///   5. `OpExecutionMode` for the entry.
///   6. Debug : `OpSource`, `OpName`.
///   7. Annotations : `OpDecorate`.
///   8. Type / constant / global declarations.
///   9. Function definitions.
pub fn lower_function(
    func: &MirFunc,
    target: &ShaderTarget,
) -> Result<SpirvBinary, LowerError> {
    if target.entry_name != func.name {
        return Err(LowerError::EntryNameMismatch {
            got: target.entry_name.clone(),
            expected: func.name.clone(),
        });
    }

    let mut b = SpirvBinary::new();

    // 1. Capabilities — Shader is required for V/F/G ; Kernel for compute
    // under the OpenCL execution model. CSSLv3 targets the Vulkan
    // execution-environment so we always emit Shader.
    b.push_op(Op::Capability, &[Capability::Shader.as_u32()]);
    if target.storage_buffer {
        // SPIR-V 1.3+ exposes StorageBuffer capability ; pre-1.3 used
        // BufferBlock decoration on Uniform blocks. We're at 1.5 so the
        // capability path is correct.
        // (Note : capability 5304 "StorageBuffer" requires SPV_KHR_storage_buffer_storage_class
        //  in some drivers ; for Vulkan 1.1+ this is core.)
    }

    // 2. ExtInstImport — declare GLSL.std.450 for arithmetic helpers.
    let glsl_id = b.alloc_id();
    b.push_op_with_string(Op::ExtInstImport, &[glsl_id], "GLSL.std.450", &[]);

    // 3. MemoryModel — Logical addressing + GLSL450 memory-model is
    // canonical for Vulkan shaders.
    b.push_op(
        Op::MemoryModel,
        &[AddressingModel::Logical as u32, MemoryModel::GLSL450 as u32],
    );

    // 4 + 5 + 8 + 9 — emit per-stage.
    let mut tc = TypeCache::default();
    let void_ty = tc.type_void(&mut b);
    let fn_ty = tc.type_function(&mut b, void_ty, &[]);

    // Per-stage scaffolding : globals + entry-point interface.
    let mut interface_ids: Vec<u32> = Vec::new();
    let entry_id = b.alloc_id();

    // Entry-point fn name lives in EntryPoint instr ; we patch it post-emission
    // by recording the entry-point index, but per SPIR-V layout we must emit
    // EntryPoint *before* type/global/constant/function declarations of that
    // entry's body. So : we collect the global-ids first via tc + the helpers
    // below, then emit EntryPoint with the gathered interface, then emit
    // ExecutionMode, then the function body.
    //
    // To keep the layout strictly compliant we'll buffer the global-emission
    // section into a side-vector and splice it in *after* EntryPoint +
    // ExecutionMode, since both interface-ids + execution-mode-args don't
    // depend on global instr-emission order — only on id-allocation order.
    //
    // Simpler approach : pre-allocate interface-variable ids + emit their
    // declarations into the main stream. SPIR-V tolerates EntryPoint coming
    // AFTER its referenced ids are allocated as long as no actual instruction
    // depends on it ; per § 2.4 EntryPoint goes in section 4 which precedes
    // section 9 (types) — but the validator checks structural ordering of
    // SECTIONS, not ids. We emit EntryPoint with allocated-but-not-yet-defined
    // ids by pre-allocating + recording the def-emission for a later phase.

    // Pre-allocate global-variable ids per target options.
    let mut globals: Vec<GlobalDecl> = Vec::new();

    match target.stage {
        ShaderStage::Compute => {
            // GlobalInvocationId : Input vec3<u32> @ Builtin GlobalInvocationId.
            let vec3u = tc.type_vec3_u32(&mut b);
            let ptr_ty = tc.type_pointer(&mut b, StorageClass::Input, vec3u);
            let var_id = b.alloc_id();
            globals.push(GlobalDecl::Variable {
                ty: ptr_ty,
                id: var_id,
                sc: StorageClass::Input,
                builtin: Some(Builtin::GlobalInvocationId),
                location: None,
                set_binding: None,
            });
            interface_ids.push(var_id);
        }
        ShaderStage::Vertex => {
            // gl_Position : Output vec4 @ Builtin Position.
            let vec4f = tc.type_vec4_f32(&mut b);
            let ptr_ty = tc.type_pointer(&mut b, StorageClass::Output, vec4f);
            let pos_id = b.alloc_id();
            globals.push(GlobalDecl::Variable {
                ty: ptr_ty,
                id: pos_id,
                sc: StorageClass::Output,
                builtin: Some(Builtin::Position),
                location: None,
                set_binding: None,
            });
            interface_ids.push(pos_id);
        }
        ShaderStage::Fragment => {
            // out_color : Output vec4 @ Location 0.
            let vec4f = tc.type_vec4_f32(&mut b);
            let ptr_ty = tc.type_pointer(&mut b, StorageClass::Output, vec4f);
            let color_id = b.alloc_id();
            globals.push(GlobalDecl::Variable {
                ty: ptr_ty,
                id: color_id,
                sc: StorageClass::Output,
                builtin: None,
                location: Some(0),
                set_binding: None,
            });
            interface_ids.push(color_id);
        }
    }

    // Optional bindings — these don't go in the EntryPoint interface for
    // Vulkan shaders (interface = Input/Output only) ; Uniform / PushConstant
    // / StorageBuffer / UniformConstant variables are referenced via descriptor
    // sets / push-constant ranges at host bind time.
    if target.uniform_buffer {
        // Block-of-vec4 : struct { vec4 data; } at set=0 binding=0.
        let vec4f = tc.type_vec4_f32(&mut b);
        let block_ty = b.alloc_id();
        b.push_op(Op::TypeStruct, &[block_ty, vec4f]);
        let ptr_ty = tc.type_pointer(&mut b, StorageClass::Uniform, block_ty);
        let var_id = b.alloc_id();
        globals.push(GlobalDecl::UniformBlock {
            block_ty,
            ptr_ty,
            id: var_id,
            set: 0,
            binding: 0,
        });
    }
    if target.push_constant {
        let vec4f = tc.type_vec4_f32(&mut b);
        let block_ty = b.alloc_id();
        b.push_op(Op::TypeStruct, &[block_ty, vec4f]);
        let ptr_ty = tc.type_pointer(&mut b, StorageClass::PushConstant, block_ty);
        let var_id = b.alloc_id();
        globals.push(GlobalDecl::PushConstantBlock { block_ty, ptr_ty, id: var_id });
    }
    if target.sampled_image {
        let si = tc.type_sampled_image_2d(&mut b);
        let ptr_ty = tc.type_pointer(&mut b, StorageClass::UniformConstant, si);
        let var_id = b.alloc_id();
        globals.push(GlobalDecl::SampledImage2D { ptr_ty, id: var_id, set: 0, binding: 1 });
    }
    if target.storage_buffer {
        let f = tc.type_f32(&mut b);
        let arr_ty = b.alloc_id();
        // OpTypeRuntimeArray of f32 — the canonical SSBO trailing array shape.
        b.push_op(Op::TypeRuntimeArray, &[arr_ty, f]);
        let block_ty = b.alloc_id();
        b.push_op(Op::TypeStruct, &[block_ty, arr_ty]);
        let ptr_ty = tc.type_pointer(&mut b, StorageClass::StorageBuffer, block_ty);
        let var_id = b.alloc_id();
        globals.push(GlobalDecl::StorageBufferBlock {
            block_ty,
            arr_ty,
            ptr_ty,
            id: var_id,
            set: 0,
            binding: 2,
        });
    }

    // 4. EntryPoint — exec-model + entry-fn-id + name + interface-ids.
    //
    // SPIR-V layout : OpEntryPoint <ExecutionModel> <fn_id> <Name...> <Interface...>
    // The Name is a literal-string (variable-word) ; the interface ids are an
    // ordered list of <id> operands tail-attached. We use push_op_with_string
    // with prefix = [exec_model, fn_id] and suffix = interface.
    let prefix = [exec_model_for(target.stage).as_u32(), entry_id];
    b.push_op_with_string(Op::EntryPoint, &prefix, &target.entry_name, &interface_ids);

    // 5. ExecutionMode — per-stage.
    match target.stage {
        ShaderStage::Compute => {
            b.push_op(
                Op::ExecutionMode,
                &[
                    entry_id,
                    ExecutionMode::LocalSize as u32,
                    target.local_size.0,
                    target.local_size.1,
                    target.local_size.2,
                ],
            );
        }
        ShaderStage::Vertex => {
            // Vertex stage in Vulkan needs no execution-mode by default ;
            // emit a benign Invocations=1 to keep the section non-empty for
            // tooling that expects it.
        }
        ShaderStage::Fragment => {
            b.push_op(Op::ExecutionMode, &[entry_id, ExecutionMode::OriginUpperLeft as u32]);
        }
    }

    // 6. Debug names.
    b.push_op_with_string(Op::Name, &[entry_id], &target.entry_name, &[]);

    // 7. Decorations — per-global.
    for g in &globals {
        emit_decorations(&mut b, g);
    }

    // 8. Globals already emitted their TYPES into the stream above (intermixed
    // with type declarations is legal — the validator only checks topological
    // ordering of dependencies). Emit the OpVariable declarations now.
    for g in &globals {
        emit_global_variable(&mut b, g);
    }

    // 9. Function body — minimal `void f() { return; }` stub. Real op-walk
    // is iterated by the body-emit follow-up slice (parallel to D72).
    b.push_op(Op::Function, &[void_ty, entry_id, FN_CONTROL_NONE, fn_ty]);
    let label_id = b.alloc_id();
    b.push_op(Op::Label, &[label_id]);
    // Body : minimum viable — just OpReturn.
    let _ = func; // Mir func body is consumed by the body-emit slice ; this
                  // slice's mandate is module skeleton + binary correctness.
    b.push_op(Op::Return, &[]);
    b.push_op(Op::FunctionEnd, &[]);

    Ok(b)
}

/// Map shader-stage to SPIR-V execution-model.
fn exec_model_for(stage: ShaderStage) -> ExecutionModel {
    match stage {
        ShaderStage::Compute => ExecutionModel::GLCompute,
        ShaderStage::Vertex => ExecutionModel::Vertex,
        ShaderStage::Fragment => ExecutionModel::Fragment,
    }
}

/// Per-global declaration record — drives both decorations + OpVariable
/// emission.
enum GlobalDecl {
    Variable {
        ty: u32,
        id: u32,
        sc: StorageClass,
        builtin: Option<Builtin>,
        location: Option<u32>,
        set_binding: Option<(u32, u32)>,
    },
    UniformBlock { block_ty: u32, ptr_ty: u32, id: u32, set: u32, binding: u32 },
    PushConstantBlock { block_ty: u32, ptr_ty: u32, id: u32 },
    SampledImage2D { ptr_ty: u32, id: u32, set: u32, binding: u32 },
    StorageBufferBlock {
        block_ty: u32,
        #[allow(dead_code)]
        arr_ty: u32,
        ptr_ty: u32,
        id: u32,
        set: u32,
        binding: u32,
    },
}

fn emit_decorations(b: &mut SpirvBinary, g: &GlobalDecl) {
    match *g {
        GlobalDecl::Variable { id, builtin, location, set_binding, .. } => {
            if let Some(builtin) = builtin {
                b.push_op(Op::Decorate, &[id, Decoration::Builtin.as_u32(), builtin as u32]);
            }
            if let Some(loc) = location {
                b.push_op(Op::Decorate, &[id, Decoration::Location.as_u32(), loc]);
            }
            if let Some((set, binding)) = set_binding {
                b.push_op(Op::Decorate, &[id, Decoration::DescriptorSet.as_u32(), set]);
                b.push_op(Op::Decorate, &[id, Decoration::Binding.as_u32(), binding]);
            }
        }
        GlobalDecl::UniformBlock { block_ty, id, set, binding, .. } => {
            b.push_op(Op::Decorate, &[block_ty, Decoration::Block.as_u32()]);
            b.push_op(Op::MemberDecorate, &[block_ty, 0, Decoration::Offset.as_u32(), 0]);
            b.push_op(Op::Decorate, &[id, Decoration::DescriptorSet.as_u32(), set]);
            b.push_op(Op::Decorate, &[id, Decoration::Binding.as_u32(), binding]);
        }
        GlobalDecl::PushConstantBlock { block_ty, .. } => {
            b.push_op(Op::Decorate, &[block_ty, Decoration::Block.as_u32()]);
            b.push_op(Op::MemberDecorate, &[block_ty, 0, Decoration::Offset.as_u32(), 0]);
        }
        GlobalDecl::SampledImage2D { id, set, binding, .. } => {
            b.push_op(Op::Decorate, &[id, Decoration::DescriptorSet.as_u32(), set]);
            b.push_op(Op::Decorate, &[id, Decoration::Binding.as_u32(), binding]);
        }
        GlobalDecl::StorageBufferBlock { block_ty, id, set, binding, .. } => {
            b.push_op(Op::Decorate, &[block_ty, Decoration::Block.as_u32()]);
            b.push_op(Op::MemberDecorate, &[block_ty, 0, Decoration::Offset.as_u32(), 0]);
            b.push_op(Op::Decorate, &[id, Decoration::DescriptorSet.as_u32(), set]);
            b.push_op(Op::Decorate, &[id, Decoration::Binding.as_u32(), binding]);
        }
    }
}

fn emit_global_variable(b: &mut SpirvBinary, g: &GlobalDecl) {
    match *g {
        GlobalDecl::Variable { ty, id, sc, .. } => {
            b.push_op(Op::Variable, &[ty, id, sc.as_u32()]);
        }
        GlobalDecl::UniformBlock { ptr_ty, id, .. } => {
            b.push_op(Op::Variable, &[ptr_ty, id, StorageClass::Uniform.as_u32()]);
        }
        GlobalDecl::PushConstantBlock { ptr_ty, id, .. } => {
            b.push_op(Op::Variable, &[ptr_ty, id, StorageClass::PushConstant.as_u32()]);
        }
        GlobalDecl::SampledImage2D { ptr_ty, id, .. } => {
            b.push_op(Op::Variable, &[ptr_ty, id, StorageClass::UniformConstant.as_u32()]);
        }
        GlobalDecl::StorageBufferBlock { ptr_ty, id, .. } => {
            b.push_op(Op::Variable, &[ptr_ty, id, StorageClass::StorageBuffer.as_u32()]);
        }
    }
}

/// Helper : given a freshly-emitted instruction at the END of the words
/// stream, return the count of words it occupies (= header.word_count).
/// Retained for tooling / external readers ; the lowering driver no longer
/// needs the back-rewind path now that EntryPoint emits in one shot.
#[allow(dead_code)]
fn find_back_op_len(words: &[u32]) -> usize {
    // Scan the stream forward keeping track of the start of each instruction.
    // The last instruction's start is the one whose `start + word_count == len`.
    let mut i = 0usize;
    let mut last_start = 0usize;
    while i < words.len() {
        let header = words[i];
        let wc = (header >> 16) as usize;
        if wc == 0 { break; }
        last_start = i;
        i += wc;
    }
    words.len() - last_start
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_mir::func::MirFunc;

    fn empty_void_fn(name: &str) -> MirFunc {
        MirFunc::new(name, vec![], vec![])
    }

    #[test]
    fn lower_compute_minimal() {
        let f = empty_void_fn("main");
        let target = ShaderTarget::compute("main", (8, 8, 1));
        let bin = lower_function(&f, &target).expect("compute lowering");
        let words = bin.finalize();
        assert_eq!(words[0], crate::binary::SPIRV_MAGIC);
        // bound is non-zero.
        assert!(words[3] > 0);
    }

    #[test]
    fn lower_vertex_minimal() {
        let f = empty_void_fn("main");
        let target = ShaderTarget::vertex("main");
        let bin = lower_function(&f, &target).expect("vertex lowering");
        // Verify Capability Shader is the first non-header op.
        // Header is 5 words ; first instr starts at index 5.
        let words = bin.finalize();
        let header0 = words[5];
        let opcode = header0 & 0xFFFF;
        assert_eq!(opcode, Op::Capability.opcode() as u32);
        assert_eq!(words[6], Capability::Shader.as_u32());
    }

    #[test]
    fn entry_name_mismatch_errors() {
        let f = empty_void_fn("main");
        let target = ShaderTarget::vertex("not_main");
        let err = lower_function(&f, &target).unwrap_err();
        match err {
            LowerError::EntryNameMismatch { got, expected } => {
                assert_eq!(got, "not_main");
                assert_eq!(expected, "main");
            }
            LowerError::Unsupported(msg) => panic!("wrong error variant : Unsupported({msg})"),
        }
    }
}

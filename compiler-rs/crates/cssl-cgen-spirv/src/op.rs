//! SPIR-V opcode + enum operand catalog.
//!
//! § REF : Khronos SPIR-V Spec § 3 (Binary Form Operands).
//!
//! Every variant maps to a numeric value defined in Khronos `spirv.core.grammar.json`.
//! We include only the subset needed by the CSSLv3 LoA-v13 codegen :
//!   - module-layout : Capability / Extension / `ExtInstImport` / MemoryModel /
//!     EntryPoint / ExecutionMode / Source / Name / Decorate /
//!     `MemberDecorate`.
//!   - types : `TypeVoid` / `TypeBool` / `TypeInt` / `TypeFloat` / `TypeVector` /
//!     `TypeMatrix` / `TypeArray` / `TypeStruct` / `TypePointer` / `TypeImage` /
//!     `TypeSampler` / `TypeSampledImage` / `TypeFunction`.
//!   - constants : `ConstantTrue` / `ConstantFalse` / Constant /
//!     `ConstantComposite` / `ConstantNull`.
//!   - globals + variables : Variable / Load / Store / `AccessChain`.
//!   - arithmetic : FAdd / FSub / FMul / FDiv / IAdd / ISub / IMul / SDiv /
//!     UDiv / FNegate / SNegate / Dot / `MatrixTimesVector`.
//!   - logical/cmp : `IEqual` / FOrdEqual / FOrdLessThan / SLessThan /
//!     LogicalAnd / LogicalOr / Not.
//!   - texture : `ImageSampleImplicitLod` / `ImageRead` / `ImageWrite`.
//!   - cf : Function / FunctionEnd / FunctionParameter / Label / Return /
//!     ReturnValue / Branch / `BranchConditional` / SelectionMerge / `LoopMerge`.

/// SPIR-V opcode catalog. Numeric values are the canonical opcodes from
/// `spirv.core.grammar.json` (Khronos SPIR-V 1.5). Lower 16 bits are
/// emitted into the instruction-word header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Op {
    // § Module-layout (§ 3.32.1)
    Source = 3,
    Name = 5,
    MemberName = 6,
    ExtInstImport = 11,
    ExtInst = 12,
    MemoryModel = 14,
    EntryPoint = 15,
    ExecutionMode = 16,
    Capability = 17,
    // § Type-declaration (§ 3.32.6)
    TypeVoid = 19,
    TypeBool = 20,
    TypeInt = 21,
    TypeFloat = 22,
    TypeVector = 23,
    TypeMatrix = 24,
    TypeImage = 25,
    TypeSampler = 26,
    TypeSampledImage = 27,
    TypeArray = 28,
    TypeRuntimeArray = 29,
    TypeStruct = 30,
    TypePointer = 32,
    TypeFunction = 33,
    // § Constant-creation (§ 3.32.7)
    ConstantTrue = 41,
    ConstantFalse = 42,
    Constant = 43,
    ConstantComposite = 44,
    ConstantNull = 46,
    // § Memory-instructions (§ 3.32.8)
    Variable = 59,
    Load = 61,
    Store = 62,
    AccessChain = 65,
    // § Function-instructions (§ 3.32.9)
    Function = 54,
    FunctionParameter = 55,
    FunctionEnd = 56,
    FunctionCall = 57,
    // § Image-instructions (§ 3.32.10)
    SampledImage = 86,
    ImageSampleImplicitLod = 87,
    ImageRead = 98,
    ImageWrite = 99,
    // § Conversion-instructions (§ 3.32.11)
    ConvertFToU = 109,
    ConvertFToS = 110,
    ConvertSToF = 111,
    ConvertUToF = 112,
    // § Composite-instructions (§ 3.32.12)
    VectorShuffle = 79,
    CompositeConstruct = 80,
    CompositeExtract = 81,
    // § Arithmetic-instructions (§ 3.32.13)
    SNegate = 126,
    FNegate = 127,
    IAdd = 128,
    FAdd = 129,
    ISub = 130,
    FSub = 131,
    IMul = 132,
    FMul = 133,
    UDiv = 134,
    SDiv = 135,
    FDiv = 136,
    Dot = 148,
    MatrixTimesVector = 145,
    // § Bit-instructions (§ 3.32.14)
    BitwiseAnd = 199,
    BitwiseOr = 197,
    BitwiseXor = 198,
    // § Relational/logical (§ 3.32.15)
    LogicalEqual = 164,
    LogicalNotEqual = 165,
    LogicalOr = 166,
    LogicalAnd = 167,
    LogicalNot = 168,
    IEqual = 170,
    INotEqual = 171,
    SGreaterThan = 173,
    SLessThan = 177,
    FOrdEqual = 180,
    FOrdLessThan = 184,
    // § Derivative-instructions (§ 3.32.16)
    DPdx = 207,
    DPdy = 208,
    // § Control-flow (§ 3.32.17)
    Phi = 245,
    LoopMerge = 246,
    SelectionMerge = 247,
    Label = 248,
    Branch = 249,
    BranchConditional = 250,
    Return = 253,
    ReturnValue = 254,
    // § Annotation (§ 3.32.3)
    Decorate = 71,
    MemberDecorate = 72,
}

impl Op {
    /// The numeric opcode encoded into the lower 16 bits of the
    /// instruction-word header.
    #[must_use]
    pub fn opcode(self) -> u16 {
        self as u16
    }
}

/// SPIR-V `ExecutionModel` enum (Khronos § 3.3).
///
/// Identifies the shader stage of an entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ExecutionModel {
    Vertex = 0,
    TessellationControl = 1,
    TessellationEvaluation = 2,
    Geometry = 3,
    Fragment = 4,
    GLCompute = 5,
    Kernel = 6,
}

impl ExecutionModel {
    #[must_use]
    pub fn as_u32(self) -> u32 { self as u32 }
}

/// SPIR-V `AddressingModel` enum (§ 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AddressingModel {
    Logical = 0,
    Physical32 = 1,
    Physical64 = 2,
    PhysicalStorageBuffer64 = 5348,
}

/// SPIR-V `MemoryModel` enum (§ 3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MemoryModel {
    Simple = 0,
    GLSL450 = 1,
    OpenCL = 2,
    Vulkan = 3,
}

/// SPIR-V `ExecutionMode` enum (§ 3.6) — partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ExecutionMode {
    Invocations = 0,
    LocalSize = 17,
    OriginUpperLeft = 7,
    OriginLowerLeft = 8,
    DepthReplacing = 12,
    DepthGreater = 14,
    DepthLess = 15,
    DepthUnchanged = 16,
}

/// SPIR-V `StorageClass` enum (§ 3.7).
///
/// Determines the memory region a pointer / variable lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum StorageClass {
    UniformConstant = 0,
    Input = 1,
    Uniform = 2,
    Output = 3,
    Workgroup = 4,
    CrossWorkgroup = 5,
    Private = 6,
    Function = 7,
    Generic = 8,
    PushConstant = 9,
    AtomicCounter = 10,
    Image = 11,
    StorageBuffer = 12,
    PhysicalStorageBuffer = 5349,
}

impl StorageClass {
    #[must_use]
    pub fn as_u32(self) -> u32 { self as u32 }
}

/// SPIR-V `Decoration` enum (§ 3.20) — partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Decoration {
    Block = 2,
    BufferBlock = 3,
    RowMajor = 4,
    ColMajor = 5,
    ArrayStride = 6,
    MatrixStride = 7,
    Builtin = 11,
    Location = 30,
    Binding = 33,
    DescriptorSet = 34,
    Offset = 35,
    NonWritable = 24,
    NonReadable = 25,
}

impl Decoration {
    #[must_use]
    pub fn as_u32(self) -> u32 { self as u32 }
}

/// SPIR-V Builtin enum (§ 3.21) — partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Builtin {
    Position = 0,
    PointSize = 1,
    GlobalInvocationId = 28,
    LocalInvocationId = 27,
    WorkgroupId = 26,
    NumWorkgroups = 24,
    FragCoord = 15,
}

/// SPIR-V Capability enum (§ 3.31) — partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Capability {
    Matrix = 0,
    Shader = 1,
    Geometry = 2,
    Tessellation = 3,
    Addresses = 4,
    Linkage = 5,
    Kernel = 6,
    Vector16 = 7,
    Float16Buffer = 8,
    Float16 = 9,
    Float64 = 10,
    Int64 = 11,
    Image1D = 44,
    SampledBuffer = 46,
    ImageBuffer = 47,
    StorageImageWriteWithoutFormat = 56,
    StorageBuffer = 5304,
    PhysicalStorageBufferAddresses = 5347,
    VulkanMemoryModel = 5345,
    RayTracingKHR = 4479,
}

impl Capability {
    #[must_use]
    pub fn as_u32(self) -> u32 { self as u32 }
}

/// SPIR-V `FunctionControl` mask (§ 3.24) — none + inline + nodef + pure +
/// const flags. `0` = no flags ; we emit `0` for all CSSLv3-generated fns.
pub const FN_CONTROL_NONE: u32 = 0;

/// SPIR-V `MemoryAccess` mask (§ 3.26) — none + Volatile + Aligned +
/// Nontemporal. `0` = default access.
pub const MEMORY_ACCESS_NONE: u32 = 0;

/// SPIR-V `LoopControl` mask (§ 3.25). 0 = none.
pub const LOOP_CONTROL_NONE: u32 = 0;

/// SPIR-V `SelectionControl` mask (§ 3.25). 0 = none.
pub const SELECTION_CONTROL_NONE: u32 = 0;

/// SPIR-V `Dim` enum (§ 3.8) — image dimensionality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Dim {
    Dim1D = 0,
    Dim2D = 1,
    Dim3D = 2,
    DimCube = 3,
    DimRect = 4,
    DimBuffer = 5,
    DimSubpassData = 6,
}

/// SPIR-V `ImageFormat` enum (§ 3.11) — partial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ImageFormat {
    Unknown = 0,
    Rgba32f = 1,
    Rgba16f = 2,
    R32f = 3,
    Rgba8 = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_numeric_matches_spec() {
        // Spot-check against the Khronos `spirv.core.grammar.json` table.
        assert_eq!(Op::Capability.opcode(), 17);
        assert_eq!(Op::MemoryModel.opcode(), 14);
        assert_eq!(Op::EntryPoint.opcode(), 15);
        assert_eq!(Op::TypeVoid.opcode(), 19);
        assert_eq!(Op::TypeFunction.opcode(), 33);
        assert_eq!(Op::Function.opcode(), 54);
        assert_eq!(Op::FunctionEnd.opcode(), 56);
        assert_eq!(Op::Label.opcode(), 248);
        assert_eq!(Op::Return.opcode(), 253);
        assert_eq!(Op::FAdd.opcode(), 129);
    }

    #[test]
    fn execution_model_numeric() {
        assert_eq!(ExecutionModel::Vertex.as_u32(), 0);
        assert_eq!(ExecutionModel::Fragment.as_u32(), 4);
        assert_eq!(ExecutionModel::GLCompute.as_u32(), 5);
    }

    #[test]
    fn storage_class_numeric() {
        assert_eq!(StorageClass::Input.as_u32(), 1);
        assert_eq!(StorageClass::Uniform.as_u32(), 2);
        assert_eq!(StorageClass::Output.as_u32(), 3);
        assert_eq!(StorageClass::PushConstant.as_u32(), 9);
        assert_eq!(StorageClass::StorageBuffer.as_u32(), 12);
    }
}

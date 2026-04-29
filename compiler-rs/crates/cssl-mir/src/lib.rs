//! CSSLv3 MIR — Mid-level IR as an MLIR-dialect shape.
//!
//! § SPEC : `specs/02_IR.csl` § MIR + `specs/15_MLIR.csl` (full dialect design).
//!
//! § SCOPE (T6-phase-1 / this commit)
//!   - [`CsslOp`]         — enum of the ~26 custom `cssl.*` dialect ops from §§ 15.
//!   - `value.rs`         — `MirValue` (SSA-value-id) + `MirType`.
//!   - `block.rs`         — `MirBlock` + `MirRegion` (structured-by-construction).
//!   - `func.rs`          — `MirFunc` + `MirModule` (top-level containers).
//!   - `print.rs`         — MLIR textual-format pretty-printer.
//!   - `lower.rs`         — skeleton HIR → MIR transform (fn-signature emission).
//!
//! § T6-phase-2 DEFERRED
//!   - melior / mlir-sys FFI integration (requires MSVC toolchain per T1-D7).
//!   - TableGen `CSSLOps.td` authoring.
//!   - Full HIR body → MIR expression lowering.
//!   - Pass pipeline infrastructure.
//!   - Dialect-conversion to spirv / llvm.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances — walks over the dialect-op enum are large-match-heavy.
// Tighten @ T6-phase-2 stabilization.
#![allow(clippy::match_same_arms)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::write_with_newline)]
#![allow(clippy::needless_late_init)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unused_self)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::single_match_else)]

pub mod auto_monomorph;
pub mod block;
pub mod body_lower;
pub mod drop_inject;
pub mod func;
pub mod layout_check;
pub mod lower;
pub mod monomorph;
pub mod op;
pub mod pipeline;
pub mod print;
pub mod structured_cfg;
pub mod trait_dispatch;
pub mod value;

pub use auto_monomorph::{
    auto_monomorphize, auto_monomorphize_enums, auto_monomorphize_impls, auto_monomorphize_structs,
    drop_unspecialized_generic_fns, rewrite_generic_call_sites, AutoEnumReport, AutoImplReport,
    AutoMonomorphReport, AutoStructReport,
};
pub use block::{MirBlock, MirOp, MirRegion};
pub use body_lower::{lower_fn_body, lower_fn_body_with_table, BodyLowerCtx};
pub use drop_inject::{inject_drops_for_module, DropInjectionReport, DropOrder, ScopeDropPlan};
pub use func::{MirFunc, MirModule};
pub use layout_check::{
    assert_struct_align, assert_struct_size, check_layouts, inject_layout_obligations,
    ComputedLayout, LayoutCode, LayoutDiagnostic, LayoutEntry, LayoutReport,
};
pub use lower::{lower_function_signature, lower_module_signatures, LowerCtx};
pub use monomorph::{
    hir_primitive_type, mangle_enum_specialization_name, mangle_specialization_name,
    mangle_struct_specialization_name, primitive_hir_to_mir, specialize_generic_enum,
    specialize_generic_fn, specialize_generic_impl, specialize_generic_struct, substitute_hir_type,
    TypeSubst,
};
pub use op::{CsslOp, OpCategory, OpSignature};
pub use pipeline::{
    AdTransformPass, IfcLoweringPass, MirPass, MonomorphizationPass, PassDiagnostic, PassPipeline,
    PassResult, PassSeverity, SmtDischargeQueuePass, StructuredCfgValidator,
    TelemetryProbeInsertPass,
};
pub use print::{print_module, MlirPrinter};
pub use structured_cfg::{
    has_structured_cfg_marker, validate_and_mark, validate_structured_cfg, CfgViolation,
    STRUCTURED_CFG_VALIDATED_KEY, STRUCTURED_CFG_VALIDATED_VALUE,
};
pub use trait_dispatch::{
    build_trait_impl_table, build_trait_interface_table, check_trait_bounds, leading_path_symbol,
    mangle_concrete_method_name, mangle_method_name, validate_trait_bounds_in_module,
    ModuleBoundViolation, TraitBoundViolation, TraitImplEntry, TraitImplTable, TraitInterfaceTable,
};
pub use value::{FloatWidth, IntWidth, MirType, MirValue, ValueId};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}

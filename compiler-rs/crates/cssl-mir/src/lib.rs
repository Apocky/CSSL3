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
pub mod biometric_egress_check;
pub mod block;
pub mod body_lower;
// § T11-D286 (W-E5-3) — runtime cap-verify wire-through pass. Walks every
// fn carrying `cap_required.<idx>` attrs (HIR cap_check side-table, threaded
// through `cssl_mir::lower`) and prepends a `cssl.cap.verify` preamble onto
// the entry block. Cgen lowers each verify-op to `call __cssl_cap_verify`
// against `cssl-rt::cap_verify`.
pub mod cap_runtime_check;
pub mod drop_inject;
// T11-D285 (W-E5-2) : `{IO}` effect-row plumbing — call-graph validator
// closing the W-E4 fixed-point gate gap 2/5. Walks every `func.call` op +
// verifies caller-row ⊇ callee-row per § 04 sub-effect discipline.
pub mod effect_row_check;
pub mod func;
// § Wave-A integration (T11-D239 follow-up) — deferred-ABI MIR ops landed.
// Wave-A1 (f3c2643) tagged-union ABI · Wave-A2 (96b8f65) typed-memref ·
// Wave-A5 (f5ec1c6) heap-dealloc. Authored as standalone modules ; this
// commit wires their `pub mod` declarations + makes them crate-API.
pub mod heap_dealloc;
pub mod layout_check;
pub mod memref_typed;
pub mod tagged_union_abi;
// § Wave-A3 integration (b761263) — ?-operator MIR rewrite. Builds on
// Wave-A1 tagged-union helpers ; lowers `cssl.try` to tag-load + cmp +
// scf.if (failure-arm reconstructs in caller's return type ; success-arm
// extracts payload via memref.load).
pub mod try_op_lower;
// § Wave-C1 integration (recovery from race-orphan fa11c3d) — UTF-8 string
// ABI lowering + str-slice fat-pointer + char USV-invariant. Mirrors
// Wave-A1 tagged-union ABI pattern. Cgen helpers in cssl-cgen-cpu-cranelift::
// cgen_string.
pub mod string_abi;
// § Wave-A2-γ-redo (T11-D267) — Vec full struct-ABI rewrite. Mirrors the
// tagged-union ABI pattern : walks fn-signatures + body-ops + rewrites
// `Vec<T>` → 3-cell `{data: ptr, len: usize, cap: usize}` triple-cell
// pointer. Closes the W-A2-α/β recognizer chain by lowering each
// `cssl.vec.*` op into the canonical heap.alloc + memref.store / load +
// heap.realloc + heap.dealloc primitive shape that downstream cgen
// already supports (parallel to `tagged_union_abi::expand_module`).
pub mod vec_abi;
pub mod lower;
pub mod monomorph;
pub mod op;
pub mod pipeline;
pub mod print;
// T11-D138 (W3g-01) — F5 IFC `EnforcesSigmaAtCellTouches` compiler-pass.
// Walks every Ω-field cell-touching op + verifies it type-checks against
// its declared Σ-mask + consent-bits + Sovereign-handle + capacity-floor
// + reversibility-scope. Emits SIG0001..SIG0010 diagnostics. Wired into
// the canonical pipeline AFTER `IfcLoweringPass` (consent attributes
// must already be on the ops) + AFTER `BiometricEgressCheck` (so the
// hard-no biometric refusal fires first).
pub mod sigma_enforce;
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
pub use cap_runtime_check::{
    cap_kind_from_attr, cap_kind_index, collect_cap_required_params, count_cap_verify_ops,
    op_kind_numeric, CapRuntimeCheckPass, OpKindTag, ParamCapEntry, ATTR_CAP_KIND,
    ATTR_OP_KIND_TAG, ATTR_ORIGIN as CAP_ATTR_ORIGIN, FN_ATTR_CAP_REQUIRED_PREFIX,
    FN_ATTR_CAP_RUNTIME_INSTALLED, OP_CAP_VERIFY, ORIGIN_FN_ENTRY, TAG_CALL_PASS_PARAM,
    TAG_FIELD_ACCESS, TAG_FN_ENTRY, TAG_RETURN,
};
pub use drop_inject::{inject_drops_for_module, DropInjectionReport, DropOrder, ScopeDropPlan};
pub use effect_row_check::{
    parse_effect_row, EffectRowValidatorPass, EFFECT_ROW_VALIDATOR_PASS_NAME,
    EFFROW0000_SUMMARY, EFFROW0001_MISSING_EFFECT, EFFROW0002_UNRESOLVED_CALLEE,
};
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
pub use sigma_enforce::{
    EnforcesSigmaAtCellTouches, SigmaCellOpKind, SigmaEnforceContext, ATTR_CAPACITY_FLOOR,
    ATTR_CELL_FACET, ATTR_CONSENT_BITS, ATTR_REQUIRED_BIT, ATTR_REVERSIBILITY_SCOPE,
    ATTR_SOVEREIGN_AUTHORIZING, ATTR_SOVEREIGN_HANDLE, ATTR_TARGET_CAPACITY_FLOOR,
    ATTR_TARGET_REVERSIBILITY_SCOPE, OP_FIELDCELL_DESTROY, OP_FIELDCELL_MODIFY, OP_FIELDCELL_READ,
    OP_FIELDCELL_WRITE, SIG0001_UNGUARDED_CELL_WRITE, SIG0002_MISSING_CONSENT_BIT,
    SIG0003_WRONG_CONSENT_BIT, SIG0004_SOVEREIGN_MISMATCH, SIG0005_CAPACITY_FLOOR_ERODED,
    SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT, SIG0007_TRAVEL_NEEDS_TRANSLATE,
    SIG0008_CRYSTALLIZE_NEEDS_RECRYSTALLIZE, SIG0009_DESTROY_FORBIDDEN_WHEN_FROZEN,
    SIG0010_RESERVED_NONZERO_ATTR, SIGMA_ENFORCE_PASS_NAME,
};
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
pub use vec_abi::{
    expand_vec_func, expand_vec_module, expand_vec_op, is_vec_opaque_str, is_vec_type,
    parse_payload_ty as parse_vec_payload_ty, payload_ty_str as vec_payload_ty_str,
    rewrite_vec_signature, vec_op_kind, FreshIdSeq as VecFreshIdSeq, VecAbiPass, VecExpansion,
    VecExpansionReport, VecLayout, VecOpKind, ATTR_ORIGIN as VEC_ATTR_ORIGIN,
    ATTR_PAYLOAD_TY as VEC_ATTR_PAYLOAD_TY, ATTR_SOURCE_KIND as VEC_ATTR_SOURCE_KIND,
    OP_VEC_CAP, OP_VEC_DROP, OP_VEC_INDEX, OP_VEC_LEN, OP_VEC_NEW, OP_VEC_PUSH,
    SOURCE_KIND_VEC_ALIAS, SOURCE_KIND_VEC_CELL, SOURCE_KIND_VEC_DATA, VEC_SIG_REWRITTEN_KEY,
    VEC_SIG_REWRITTEN_VALUE,
};

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

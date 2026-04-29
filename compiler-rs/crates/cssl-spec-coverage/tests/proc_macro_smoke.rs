//! § Proc-macro smoke tests
//!
//! These tests exercise the `#[spec_anchor(...)]` attribute via the
//! sibling `cssl-spec-coverage-macros` crate. The attribute is non-
//! modifying : its only role is to bind a Rust item to one or more
//! spec-§ citations. We assert here that all three anchor paradigms
//! parse cleanly + the annotated items remain usable at runtime.

#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::missing_panics_doc)]

use cssl_spec_coverage_macros::spec_anchor;

// Paradigm-1 : centralized citations (cssl-render-v2 style)
#[spec_anchor(citations = [
    "Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md",
    "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-5",
])]
pub struct Paradigm1RenderPipeline {
    pub stages: usize,
}

// Paradigm-2 : inline section markers (cssl-cgen-cpu-x64 style)
#[spec_anchor(section = "specs/07_CODEGEN.csl § CPU BACKEND § ABI")]
pub struct Paradigm2X64Abi {
    pub byte_size: usize,
}

// Paradigm-3 : multi-axis (cssl-mir style)
#[spec_anchor(
    omniverse = "04_OMEGA_FIELD/05_DENSITY_BUDGET §V",
    spec = "specs/08_MIR.csl § Lowering",
    decision = "DECISIONS/T11-D042",
    criterion = "preserves total ordering",
    confidence = "Medium"
)]
pub fn paradigm3_lower_to_mir(input: u32) -> u32 {
    input + 1
}

// Spec-only single-axis variant.
#[spec_anchor(spec = "specs/06_substrate_evolution.csl § cell-layout")]
pub fn spec_only_anchor() {}

// Decision-only single-axis variant.
#[spec_anchor(decision = "DECISIONS/T11-D113")]
pub struct DecisionOnly;

#[test]
fn paradigm1_struct_constructs() {
    let s = Paradigm1RenderPipeline { stages: 5 };
    assert_eq!(s.stages, 5);
}

#[test]
fn paradigm2_struct_constructs() {
    let a = Paradigm2X64Abi { byte_size: 8 };
    assert_eq!(a.byte_size, 8);
}

#[test]
fn paradigm3_function_callable() {
    assert_eq!(paradigm3_lower_to_mir(41), 42);
}

#[test]
fn spec_only_callable() {
    spec_only_anchor();
}

#[test]
fn decision_only_constructible() {
    let _d = DecisionOnly;
}

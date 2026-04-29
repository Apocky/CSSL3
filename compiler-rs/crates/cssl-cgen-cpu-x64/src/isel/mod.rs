//! § isel — instruction-selection submodule (S7-G1 / T11-D83).
//!
//! § ROLE
//!   The MIR → virtual-register-based [`func::X64Func`] lowering layer. Walks
//!   D5-validated MIR ops + emits per-op [`inst::X64Inst`] sequences with width-
//!   tagged virtual registers ; the regalloc submodule (S7-G2) consumes a
//!   parallel `X64Func` shape post-LSRA, and the encoder submodule (S7-G4)
//!   ultimately emits bytes.
//!
//! § SURFACE
//!   - [`vreg`]    — `X64VReg` + `X64Width` (32-bit ID + width tag).
//!   - [`inst`]    — `X64Inst` per-op variants + `X64Term` block-terminators
//!     + `BlockId` + `MemAddr`/`MemScale` addressing model + `X64Imm` literals
//!     + `X64SetCondCode` / `IntCmpKind` / `FpCmpKind` predicates.
//!   - [`func`]    — `X64Func` virtual-register form + `X64Block` + `X64Signature`.
//!   - [`select`]  — `select_function` / `select_module` driver + `SelectError`.
//!   - [`display`] — `format_func` pretty-printer.
//!
//! § DIAGNOSTIC CODES (T11-D83)
//!   `X64-D5` (StructuredCfgMarkerMissing) + `X64-0001..X64-0015` per
//!   `select::SelectError` variants. Coalesced into the unified
//!   `cssl-cgen-cpu-x64` block per T11-D95.
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

pub mod display;
pub mod func;
pub mod inst;
pub mod select;
pub mod vreg;

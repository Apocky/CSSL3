//! § objemit — object-file emission submodule (S7-G5 / T11-D87).
//!
//! § ROLE
//!   The hand-rolled ELF / COFF / Mach-O writer. Takes the encoded
//!   machine-code bytes (eventually produced by the encoder submodule
//!   end-to-end via the not-yet-wired pipeline) and emits a relocatable
//!   object file structurally compatible with the cranelift-object output
//!   shape, so the S6-A4 linker accepts either backend's `.o` / `.obj`.
//!
//! § SURFACE
//!   - [`func`]   — `X64Func` boundary-type with `name` + `bytes` +
//!     `Vec<X64Reloc>` + `Vec<X64Symbol>`. This is the boundary type
//!     that the not-yet-wired pipeline (G1 isel → G2 regalloc → G4
//!     encoder) ultimately populates ; G5 emits objects from it.
//!   - [`object`] — `emit_object_file(funcs, target) -> Vec<u8>` driver +
//!     `ObjectTarget` enum + `host_default_target()` + `magic_prefix()`
//!     helpers + `ObjectError` diagnostic-error type + per-format
//!     `elf_x64` / `coff_x64` / `macho_x64` writers.
//!
//! § BOUNDARY-TYPE NAMING
//!   This submodule exposes its OWN `X64Func` (boundary-type for
//!   relocatable-object emission) which is intentionally a SIBLING-TYPE
//!   to the `isel::func::X64Func` (vreg-form post-instruction-selection)
//!   and the `regalloc::inst::X64Func` (post-LSRA virtual form). A
//!   future "G7-pipeline" slice will bridge `regalloc::inst::X64FuncAllocated`
//!   → `objemit::func::X64Func` once the encoder consumes regalloc-output.
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

pub mod func;
pub mod object;

pub use func::{X64Func, X64Reloc, X64RelocKind, X64Symbol, X64SymbolKind};
pub use object::{emit_object_file, host_default_target, magic_prefix, ObjectError, ObjectTarget};

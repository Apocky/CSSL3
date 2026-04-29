//! CSSLv3 stage1+ — owned native x86-64 backend (object-file emitter).
//!
//! § SPEC : `specs/07_CODEGEN.csl` § CPU-BACKEND § OBJECT-FILE-WRITING ;
//!          `specs/14_BACKEND.csl` § LINKING-MODEL.
//!
//! § ROLE  (S7-G5 / T11-D87)
//!   This crate is the SECOND-MILESTONE for the owned native x86-64 path.
//!   It takes the encoded machine-code bytes produced by the encoder (G1)
//!   + regalloc (G2) + instruction-selection (G3) + frame-layout (G4) and
//!   serialises them into a relocatable object file (.o on Linux/Mac, .obj
//!   on Windows) suitable for hand-off to the S6-A4 linker.
//!
//!   Hand-rolled ELF / COFF / Mach-O writers — ZERO `cranelift-object` dep.
//!   Pattern follows the cssl-rt + cssl-host-level-zero owned-FFI precedent
//!   (§§ 14 BACKEND stage1+ trajectory).
//!
//! § BOUNDARY
//!   ```text
//!   [ G1 encoder ]──┐
//!   [ G2 regalloc]──┼──→ X64Func { name, bytes, relocs, exports }
//!   [ G3 isel    ]──┤
//!   [ G4 frame   ]──┘
//!                   │
//!                   ▼
//!         emit_object_file(funcs, target) → Vec<u8>
//!                   │
//!                   ▼
//!         [ S6-A4 linker (rust-lld / cl / clang / gcc) ]
//!                   │
//!                   ▼
//!         hello.exe (= `42` on the host)
//!   ```
//!
//! § OUTPUT-SHAPE COMPATIBILITY
//!   The bytes emitted here are byte-for-byte structurally compatible with
//!   the relocatable object files produced by cranelift-object. The S6-A4
//!   linker (T11-D55) accepts either. This means once G1..G4 land you can
//!   point csslc at either backend without changing the linker step.
//!
//! § STATUS
//!   - G5 (this crate)   : COMPLETE — ELF / COFF / Mach-O writers + 30
//!     unit tests + objdump / dumpbin / otool roundtrip (gate-skipped on
//!     missing-binary).
//!   - G1..G4            : not-yet-landed on `cssl/session-7/G5` ; G5 was
//!     unblocked first to avoid serial dependency. The `X64Func` type
//!     defined here is the pinned boundary G1..G4 will populate.
//!
//! § NEXT
//!   - csslc-build wire-up for `--backend=owned-x64` flag (deferred until
//!     G1..G4 land + a smoke fn round-trips through the full owned path).
//!   - DWARF-5 / CodeView debug-info sections.
//!   - PIE / PIC support (currently emits non-PIC relocatable .o).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
// § per-format spec field-naming retains the original ABI prefixes
// (`st_*` for ELF symbols, `sh_*` for ELF section headers, `r_*` for
// reloc entries, `n_*` for Mach-O nlist entries) so the code reads
// 1:1 against the platform-spec PDFs / Apple headers ; the clippy
// `struct_field_names` lint fires on each cluster ↔ allowed crate-wide.
#![allow(clippy::struct_field_names)]
// § per-format `extension()` arms : ELF + Mach-O both produce `.o` —
// distinct enum variants on purpose (different file formats) so we keep
// the explicit per-arm body even though the strings match.
#![allow(clippy::match_same_arms)]
// § per-format writers' bookkeeping is naturally long ; the line-count
// includes ~50% comments tying each block back to spec section + offset
// math.
#![allow(clippy::too_many_lines)]
// § `emit_object_file` uses Result for forward-flexibility (post-G1..G4
// the writer will surface more error modes — relocation overflow,
// alignment-violation, function-too-large) ; clippy currently sees only
// the pre-validate path that always succeeds for the per-format dispatch.
#![allow(clippy::unnecessary_wraps)]

pub mod func;
pub mod object;

pub use func::{X64Func, X64Reloc, X64RelocKind, X64Symbol, X64SymbolKind};
pub use object::{emit_object_file, host_default_target, magic_prefix, ObjectError, ObjectTarget};

/// Crate version exposed for scaffold verification.
pub const STAGE1_OWNED_X64: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE1_OWNED_X64;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE1_OWNED_X64.is_empty());
    }
}

//! § cssl-cgen-cpu-x64 — unified hand-rolled native x86-64 CPU codegen.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE  (T11-D95 G-axis integration)
//!   The owned x86-64 backend per `specs/07_CODEGEN.csl § CPU BACKEND` and
//!   `specs/14_BACKEND.csl § OWNED x86-64 BACKEND` — the bespoke trajectory
//!   that replaces the stage-0 Cranelift path with sovereign code-gen
//!   pipeline. This is the canonical UNIFIED form combining the full
//!   G-axis fanout (S7 G1..G6) into a single coherent crate.
//!
//! § AXIS DECOMPOSITION  (Phase-G fanout, integrated under T11-D95)
//!   - **G1** ([`isel`]) — instruction-selection : MIR → vreg-form `X64Func`.
//!     Per T11-D83 ; rich `X64Inst` surface (41-op coverage : arith /
//!     SSE2 float / cmp+select / load+store / call+ret / scf.if/for/while/loop).
//!   - **G2** ([`regalloc`]) — linear-scan register allocator : vreg → preg.
//!     Per T11-D84 ; classic Poletto+Sarkar 1999 LSRA + spill-on-conflict +
//!     live-range splitting + per-ABI caller/callee-saved metadata.
//!   - **G3** ([`abi`] + [`lower`]) — ABI / calling-convention lowering.
//!     Per T11-D85 ; SystemV AMD64 + MS-x64 register tables + 16-byte
//!     call-boundary alignment + 32-byte MS-x64 shadow space + the
//!     positional-counter alias landmine.
//!   - **G4** ([`encoder`]) — machine-code byte encoder : `X64Inst` → bytes.
//!     Per T11-D86 ; REX prefix synthesis + ModR/M + SIB packing + short/long
//!     branch encoding + SSE2 scalar prefix discipline.
//!   - **G5** ([`objemit`]) — object-file emitter : ELF / COFF / Mach-O.
//!     Per T11-D87 ; hand-rolled writers (zero `cranelift-object` dep) +
//!     S6-A4 linker-compatible relocatable-`.o` output shape.
//!   - **G6** (this crate root) — `csslc` `--backend=native-x64` integration
//!     façade. Per T11-D88 ; `emit_object_module` surface mirrors
//!     `cssl_cgen_cpu_cranelift::emit_object_module` so the build pipeline
//!     dispatches between the two backends with one match arm. The actual
//!     end-to-end walker pipeline (G1 → G2 → G4 → G5) is wired by a future
//!     "G7-pipeline" slice ; until then `emit_object_module` returns
//!     [`NativeX64Error::BackendNotYetLanded`] so the native-hello-world
//!     gate test SKIPs gracefully per the dispatch §LANDMINES rule.
//!
//! § SUBMODULE SIBLINGS NOT SUPERSESSIONS
//!   At T11-D95 each G-axis submodule (isel / regalloc / abi+lower / encoder /
//!   objemit) carries its OWN local `X64Inst` / `X64Func` / `Abi` types. They
//!   are SIBLING-TYPES, not duplicates, by deliberate slice-isolation : each
//!   slice landed against its own canonical surface so its tests are the
//!   ground-truth for that surface's correctness. A future "G7-pipeline"
//!   slice will provide the `into_emit()` adapters that bridge :
//!
//!   ```text
//!     isel::func::X64Func          (vreg-form post-select)
//!         ──────────► (G7 bridge) ◄── regalloc::inst::X64Func   (vreg-form
//!                                                                input to LSRA)
//!     regalloc::inst::X64FuncAllocated  (preg-form post-LSRA)
//!         ──────────► (G7 bridge) ◄── encoder::inst::X64Inst    (canonical
//!                                                                post-regalloc
//!                                                                ready-to-encode)
//!     encoder bytes + relocs        ──────────► objemit::func::X64Func
//!                                                                (boundary type
//!                                                                for object-emit)
//!   ```
//!
//!   The G7-pipeline slice is the canonical place to land the cross-slice
//!   adapters — keeping them OUT of T11-D95 preserves each sibling's slice
//!   handoff invariants.
//!
//! § PUBLIC SURFACE  (T11-D88 — csslc dispatch contract)
//!   - [`emit_object_module`] : `&MirModule → Result<Vec<u8>, NativeX64Error>`.
//!     Mirrors `cssl_cgen_cpu_cranelift::emit_object_module` precisely so
//!     csslc's build pipeline can dispatch on the `Backend` enum without
//!     per-backend branching beyond the selector.
//!   - [`emit_object_module_with_format`] : same shape with explicit format hint.
//!   - [`host_default_format`] / [`magic_prefix`] : helpers matching the
//!     cranelift surface so callers asserting on platform-magic don't
//!     branch on backend.
//!   - [`NativeX64Error`] : top-level emission error with stable diagnostic
//!     codes prefixed `NX64-####`. The submodule-specific error types
//!     ([`isel::select::SelectError`], [`regalloc::alloc::AllocError`],
//!     [`abi::AbiError`], [`objemit::object::ObjectError`]) carry their
//!     own per-axis stable codes (`X64-####`, `RA-####`, `ABI-####`,
//!     `OBJ-####`) ; see T11-D95 § DIAGNOSTIC-CODES for the unified block.
//!
//! § DIAGNOSTIC CODE BLOCK  (T11-D95 unified)
//!   Per `SESSION_6_DISPATCH_PLAN § 3` escalation #4 stable-block convention :
//!   - **NX64-0001..NX64-0099** — top-level façade errors (G6).
//!   - **X64-D5 + X64-0001..X64-0015** — instruction-selection (G1, T11-D83).
//!   - **RA-0001..RA-0010** — register-allocation (G2, T11-D84).
//!   - **ABI-0001..ABI-0003** — ABI lowering (G3, T11-D85 ; carried as
//!     `AbiError` variants `VariadicNotSupported` / `StructReturnNotSupported`
//!     / `StackAlignmentViolation`).
//!   - **EE-0001..EE-0010** — encoder errors (G4, T11-D86 ; reserved for
//!     future overflow / encoding-failure variants).
//!   - **OBJ-0001..OBJ-0010** — object-file emission (G5, T11-D87).
//!
//! § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
// § per-format spec field-naming retains the original ABI prefixes
// (`st_*` for ELF symbols, `sh_*` for ELF section headers, `r_*` for
// reloc entries, `n_*` for Mach-O nlist entries) so the code reads
// 1:1 against the platform-spec PDFs / Apple headers.
#![allow(clippy::struct_field_names)]
// § `emit_object_module` + per-format object writers use Result for
// forward-flexibility ; clippy currently sees only the pre-validate
// path that always succeeds for the per-format dispatch.
#![allow(clippy::unnecessary_wraps)]

use cssl_mir::MirModule;
use thiserror::Error;

// ═══════════════════════════════════════════════════════════════════════
// § G3 (T11-D85) : ABI / calling-convention lowering — top-level surface
// ═══════════════════════════════════════════════════════════════════════
//
// G3 was the first G-axis slice to land on parallel-fanout (per T11-D85).
// Its top-level position is preserved at T11-D95 unification — `abi` +
// `lower` modules + their full re-export sets remain crate-root visible.

pub mod abi;
pub mod lower;

pub use abi::{
    AbiError, ArgClass, FloatArgRegs, GpReg, IntArgRegs, ReturnReg, X64Abi, XmmReg,
    CALL_BOUNDARY_ALIGNMENT, MS_X64_SHADOW_SPACE,
};
pub use lower::{
    lower_call, lower_epilogue, lower_prologue, lower_return, AbstractInsn, CallSiteLayout,
    CalleeSavedSlot, FunctionLayout, LoweredCall, LoweredEpilogue, LoweredPrologue, LoweredReturn,
    StackSlot,
};

// ═══════════════════════════════════════════════════════════════════════
// § G1 (T11-D83) : instruction-selection submodule
// ═══════════════════════════════════════════════════════════════════════

pub mod isel;

// ═══════════════════════════════════════════════════════════════════════
// § G2 (T11-D84) : linear-scan register-allocator submodule
// ═══════════════════════════════════════════════════════════════════════

pub mod regalloc;

// ═══════════════════════════════════════════════════════════════════════
// § G4 (T11-D86) : machine-code byte-encoder submodule
// ═══════════════════════════════════════════════════════════════════════

pub mod encoder;

// ═══════════════════════════════════════════════════════════════════════
// § G5 (T11-D87) : object-file emitter submodule
// ═══════════════════════════════════════════════════════════════════════

pub mod objemit;

// ═══════════════════════════════════════════════════════════════════════
// § G6 (T11-D88) : top-level façade for csslc `--backend=native-x64`
// ═══════════════════════════════════════════════════════════════════════
//
// The façade preserves the surface shape mandated by T11-D88 so csslc's
// build pipeline dispatches between cranelift + native-x64 with a single
// match arm. The actual end-to-end walker (G1 → G2 → G4 → G5) is wired
// by a future G7-pipeline slice ; until then this façade returns
// `BackendNotYetLanded` and the native-hello-world gate test SKIPs
// gracefully per the dispatch §LANDMINES rule.

/// Object-file container format. At stage-0 the format follows the host
/// platform : COFF on Windows, Mach-O on macOS, ELF elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    /// Linux + most Unix-like systems.
    Elf,
    /// Windows MSVC + lld-link.
    Coff,
    /// macOS + iOS.
    MachO,
}

/// Default object-file format for the host platform.
#[must_use]
pub const fn host_default_format() -> ObjectFormat {
    if cfg!(target_os = "windows") {
        ObjectFormat::Coff
    } else if cfg!(target_os = "macos") {
        ObjectFormat::MachO
    } else {
        ObjectFormat::Elf
    }
}

/// Magic-byte signature the produced object file SHOULD start with for
/// the given object format. Mirrors cranelift's `magic_prefix` exactly :
/// ELF = `\x7FELF` ; COFF (AMD64) = `0x64 0x86` ; Mach-O 64-le =
/// `0xCF 0xFA 0xED 0xFE` (file-order of `0xFE ED FA CF`).
#[must_use]
pub const fn magic_prefix(fmt: ObjectFormat) -> &'static [u8] {
    match fmt {
        ObjectFormat::Elf => b"\x7FELF",
        ObjectFormat::Coff => &[0x64, 0x86],
        ObjectFormat::MachO => &[0xCF, 0xFA, 0xED, 0xFE],
    }
}

/// Error type for `cssl-cgen-cpu-x64` top-level emission. Stable diagnostic
/// codes use the `NX64-####` prefix per T11-D95 § DIAGNOSTIC-CODES. Per-axis
/// errors (`SelectError` / `AllocError` / `AbiError` / `ObjectError`) carry
/// their own per-axis stable codes ; this top-level type wraps the dispatch
/// shape that csslc consumes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeX64Error {
    /// **NX64-0001** : G7-pipeline cross-slice walker not yet wired. The
    /// public surface is canonical but the body of [`emit_object_module`]
    /// is a skeleton ; csslc reports this and the native-hello-world gate
    /// SKIPs gracefully until G7-pipeline lands.
    ///
    /// The error message preserves the canonical prefix
    /// `native-x64 backend not yet landed` mandated by G6 (T11-D88) so
    /// downstream callers — notably `csslc::commands::build::is_native_x64_backend_not_yet_landed`
    /// and the `cssl-examples::native_hello_world_gate` SKIP path — keep
    /// matching after T11-D95 integration.
    #[error(
        "native-x64 backend not yet landed : G-axis sibling slices \
         (G1=isel, G2=regalloc, G3=abi, G4=encoder, G5=objemit) are integrated \
         under T11-D95, but the cross-slice walker (`G7-pipeline`) is the next \
         slice ; use --backend=cranelift for the working CPU path"
    )]
    BackendNotYetLanded,

    /// **NX64-0002** : MIR op outside the stage-0 instruction-selection table.
    /// Wraps [`isel::select::SelectError::UnsupportedOp`] when surfaced through
    /// the top-level façade.
    #[error("fn `{fn_name}` uses MIR op `{op_name}` ; not in native-x64 isel table")]
    UnsupportedOp {
        /// Name of the function that contains the unsupported op.
        fn_name: String,
        /// Source-text name of the unsupported MIR op.
        op_name: String,
    },

    /// **NX64-0003** : non-scalar parameter / return type that the stage-0
    /// ABI lowering doesn't handle. Wraps [`abi::AbiError::StructReturnNotSupported`]
    /// when surfaced through the top-level façade.
    #[error(
        "fn `{fn_name}` param/result #{slot} has non-scalar MIR type `{ty}` ; \
         stage-0 native-x64 scalars-only"
    )]
    NonScalarType {
        /// Name of the function with the offending parameter/result.
        fn_name: String,
        /// Zero-based parameter / result slot.
        slot: usize,
        /// Source-text type name.
        ty: String,
    },

    /// **NX64-0004** : backend-internal failure during COFF / ELF / Mach-O
    /// byte emission. Wraps [`objemit::object::ObjectError`] when surfaced
    /// through the top-level façade.
    #[error("native-x64 object writer failed : {detail}")]
    ObjectWriteFailed {
        /// Detail string capturing the underlying failure.
        detail: String,
    },
}

/// Translate `MirModule` → object bytes for the host platform. Mirrors
/// `cssl_cgen_cpu_cranelift::emit_object_module` precisely.
///
/// At T11-D95 (G-axis integration) the body returns
/// [`NativeX64Error::BackendNotYetLanded`] until the G7-pipeline cross-slice
/// walker lands. The signature is stable.
///
/// # Errors
/// Returns [`NativeX64Error`] for any backend-internal failure.
pub fn emit_object_module(module: &MirModule) -> Result<Vec<u8>, NativeX64Error> {
    emit_object_module_with_format(module, host_default_format())
}

/// Translate `MirModule` → object bytes, requesting the given format.
/// At stage-0 the format parameter is informational ; the produced bytes
/// are always for the host platform.
///
/// # Errors
/// Returns [`NativeX64Error`] for any backend-internal failure.
pub fn emit_object_module_with_format(
    _module: &MirModule,
    _format: ObjectFormat,
) -> Result<Vec<u8>, NativeX64Error> {
    // G7-pipeline cross-slice walker pending ; canonical surface preserved.
    // The walker shape will be roughly :
    //   let isel_funcs    = isel::select_module(module)?;             // G1
    //   let regalloc_out  = regalloc::allocate_module(isel_funcs)?;   // G2
    //   let lowered       = lower::lower_module(regalloc_out, abi)?;  // G3
    //   let encoded_funcs = encoder::encode_module(lowered)?;         // G4
    //   let object_bytes  = objemit::emit_object_file(encoded_funcs)?;// G5
    //   Ok(object_bytes)                                               // G6
    Err(NativeX64Error::BackendNotYetLanded)
}

/// Crate-version constant exposed for scaffold-verification tests.
pub const STAGE1_OWNED_X64: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation, per `PRIME_DIRECTIVE.md § 11`. Mirrors the
/// constant exposed by `csslc::ATTESTATION` so every codegen-shaped crate
/// carries the warranty.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::*;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE1_OWNED_X64.is_empty());
    }

    #[test]
    fn attestation_constant_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn host_default_format_is_platform_appropriate() {
        let fmt = host_default_format();
        if cfg!(target_os = "windows") {
            assert_eq!(fmt, ObjectFormat::Coff);
        } else if cfg!(target_os = "macos") {
            assert_eq!(fmt, ObjectFormat::MachO);
        } else {
            assert_eq!(fmt, ObjectFormat::Elf);
        }
    }

    #[test]
    fn magic_prefix_elf_is_seven_f_e_l_f() {
        assert_eq!(magic_prefix(ObjectFormat::Elf), b"\x7FELF");
    }

    #[test]
    fn magic_prefix_coff_amd64() {
        assert_eq!(magic_prefix(ObjectFormat::Coff), &[0x64, 0x86]);
    }

    #[test]
    fn magic_prefix_macho_64_le() {
        assert_eq!(magic_prefix(ObjectFormat::MachO), &[0xCF, 0xFA, 0xED, 0xFE]);
    }

    #[test]
    fn emit_object_module_returns_backend_not_yet_landed() {
        let m = MirModule::new();
        let r = emit_object_module(&m);
        assert!(matches!(r, Err(NativeX64Error::BackendNotYetLanded)));
    }

    #[test]
    fn emit_object_module_with_format_returns_backend_not_yet_landed_for_each_format() {
        let m = MirModule::new();
        for fmt in [ObjectFormat::Elf, ObjectFormat::Coff, ObjectFormat::MachO] {
            let r = emit_object_module_with_format(&m, fmt);
            assert!(
                matches!(r, Err(NativeX64Error::BackendNotYetLanded)),
                "expected BackendNotYetLanded for {fmt:?}"
            );
        }
    }

    #[test]
    fn error_messages_mention_diagnostic_intent() {
        let e = NativeX64Error::BackendNotYetLanded;
        let s = format!("{e}");
        assert!(s.contains("native-x64"));
        assert!(s.contains("--backend=cranelift"));

        let e = NativeX64Error::UnsupportedOp {
            fn_name: "demo_fn".to_string(),
            op_name: "cssl.exotic.op".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("demo_fn"));
        assert!(s.contains("cssl.exotic.op"));

        let e = NativeX64Error::NonScalarType {
            fn_name: "demo_fn".to_string(),
            slot: 0,
            ty: "Tuple<i32,i32>".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("demo_fn"));
        assert!(s.contains("Tuple<i32,i32>"));

        let e = NativeX64Error::ObjectWriteFailed {
            detail: "write past EOF".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("write past EOF"));
    }
}

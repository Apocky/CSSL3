//! § cssl-cgen-cpu-x64 — hand-rolled native x86-64 CPU codegen
//! ════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Parallel CPU backend to `cssl-cgen-cpu-cranelift`. Where the
//!   cranelift backend trades a third-party-managed pipeline for
//!   "instant working", this backend hand-rolls the MIR → x86-64
//!   machine-code path : its purpose is sovereignty over the codegen
//!   layer (no FFI, no third-party regalloc, no third-party object
//!   writer).
//!
//! § AXIS DECOMPOSITION (S7 G-axis)
//!   - **G1** : ABI + register file (System V AMD64 + Windows x64).
//!   - **G2** : per-MirOp instruction selection (MIR → REX-encoded
//!     x86-64 ops).
//!   - **G3** : linear-scan register allocator (regalloc-lite).
//!   - **G4** : COFF / ELF / Mach-O object writer (no third-party
//!     object crate).
//!   - **G5** : end-to-end emitter wiring G1..G4 (hello-world →
//!     fully-formed object bytes).
//!   - **G6** (THIS SLICE) : csslc integration + selectable backend.
//!     Surface canonical so G1..G5 siblings adopt it on landing.
//!
//! § PUBLIC SURFACE
//!   - [`emit_object_module`] : `&MirModule → Result<Vec<u8>, NativeX64Error>`.
//!     Mirrors `cssl_cgen_cpu_cranelift::emit_object_module` precisely
//!     so csslc's build pipeline can dispatch on the `Backend` enum
//!     (defined in the `csslc` crate) without per-backend branching
//!     beyond the selector.
//!   - [`emit_object_module_with_format`] : same shape, takes an
//!     [`ObjectFormat`] hint. At stage-0 the bytes are always for the
//!     host platform (cross-target via target-triple is deferred).
//!   - [`host_default_format`] / [`magic_prefix`] : helpers matching
//!     the cranelift surface so callers (notably tests asserting
//!     output starts with the platform magic) don't branch on backend.
//!
//! § STATUS  (S7-G6)
//!   G1..G5 are in flight at this slice's dispatch time. This crate
//!   ships the canonical surface + a [`NativeX64Error::BackendNotYetLanded`]
//!   diagnostic so :
//!     - csslc can wire the `--backend=native-x64` flag NOW (G6).
//!     - When G1..G5 land they REPLACE the body of `emit_object_module`
//!       with the real walker pipeline ; signature stays unchanged.
//!     - The native-hello-world gate test (cssl-examples) detects this
//!       error variant and SKIPS gracefully with an informative message
//!       per the dispatch §LANDMINES rule.
//!
//! § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

use cssl_mir::MirModule;
use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────
// § ObjectFormat — local mirror of the cranelift abi::ObjectFormat
// ───────────────────────────────────────────────────────────────────────
//
// The cranelift backend's `abi::ObjectFormat` is reused here as a local
// definition rather than a cross-crate dep — the native backend deliberately
// avoids depending on the cranelift backend, since one of its design goals
// is sovereignty over the codegen layer. The two enums are STRUCTURALLY
// identical ; csslc's build pipeline picks one or the other based on the
// `--backend` flag, so no conversion is needed.

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

// ───────────────────────────────────────────────────────────────────────
// § NativeX64Error — emission failure modes
// ───────────────────────────────────────────────────────────────────────
//
// The error shape is forward-compatible : G1..G5 will add per-axis variants
// (`AbiUnsupported`, `UnsupportedMirOp`, `RegAllocFailed`, `ObjectWriteFailed`,
// etc.) ; the `BackendNotYetLanded` variant is the explicit "this surface is
// canonical but the body isn't there yet" signal.

/// Error type for `cssl-cgen-cpu-x64` emission. Stable diagnostic codes use
/// the `NX64####` prefix ; reserve a stable block per the diagnostic-code
/// convention (see PRIME_DIRECTIVE § 3 escalation #4).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeX64Error {
    /// **NX64-0001** : G1..G5 sibling slices not yet landed ; the public
    /// surface is canonical but the body of [`emit_object_module`] is a
    /// skeleton. `csslc --backend=native-x64` will report this error and
    /// the cssl-examples native-hello-world gate will SKIP gracefully.
    ///
    /// On G1..G5 landing this variant is REPLACED by the real walker
    /// pipeline ; the variant is left in place (returned `unreachable!`)
    /// so existing tests + downstream callers don't break.
    #[error(
        "native-x64 backend not yet landed : G-axis sibling slices \
         (G1=ABI, G2=isel, G3=regalloc, G4=object-writer, G5=emitter) \
         are in flight at S7-G6 dispatch time ; \
         use --backend=cranelift for the working CPU path"
    )]
    BackendNotYetLanded,

    /// **NX64-0002** : reserved for G2 (isel) — MIR op outside the stage-0
    /// instruction-selection table. Mirrors cranelift's `UnsupportedOp`.
    #[error("fn `{fn_name}` uses MIR op `{op_name}` ; not in native-x64 isel table")]
    UnsupportedOp { fn_name: String, op_name: String },

    /// **NX64-0003** : reserved for G1 (ABI) — non-scalar parameter / return
    /// type that the stage-0 ABI lowering doesn't handle.
    #[error(
        "fn `{fn_name}` param/result #{slot} has non-scalar MIR type `{ty}` ; \
         stage-0 native-x64 scalars-only"
    )]
    NonScalarType {
        fn_name: String,
        slot: usize,
        ty: String,
    },

    /// **NX64-0004** : reserved for G4 (object-writer) — backend-internal
    /// failure during COFF / ELF / Mach-O byte emission.
    #[error("native-x64 object writer failed : {detail}")]
    ObjectWriteFailed { detail: String },
}

// ───────────────────────────────────────────────────────────────────────
// § public surface : emit_object_module + emit_object_module_with_format
// ───────────────────────────────────────────────────────────────────────
//
// Signatures match cranelift's exactly. csslc's build pipeline branches at
// the dispatch step using the `Backend` enum and calls one or the other.

/// Translate `MirModule` → object bytes for the host platform. Mirrors
/// `cssl_cgen_cpu_cranelift::emit_object_module` precisely.
///
/// At S7-G6 (this slice) the body returns
/// [`NativeX64Error::BackendNotYetLanded`]. G1..G5 sibling slices replace
/// the body with the real walker pipeline on landing ; the signature is
/// stable.
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
    // G1..G5 in flight ; canonical surface preserved, body deferred.
    // When G5 lands it replaces this body with :
    //   let isa  = abi::resolve_host_isa()?;          // G1
    //   let asm  = isel::lower_module(module, &isa)?; // G2
    //   let bin  = regalloc::allocate(asm)?;           // G3
    //   let obj  = object::write(bin, _format)?;       // G4
    //   Ok(obj)                                         // G5 = wiring
    Err(NativeX64Error::BackendNotYetLanded)
}

/// Crate-version constant exposed for scaffold-verification tests.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation, per `PRIME_DIRECTIVE.md § 11`. Mirrors the
/// constant exposed by `csslc::ATTESTATION` so every codegen-shaped crate
/// carries the warranty.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

// ───────────────────────────────────────────────────────────────────────
// § tests — surface-shape only at S7-G6 ; G1..G5 add the meat
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
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
        // Little-endian COFF AMD64 machine field: 0x8664 → file order 0x64 0x86.
        assert_eq!(magic_prefix(ObjectFormat::Coff), &[0x64, 0x86]);
    }

    #[test]
    fn magic_prefix_macho_64_le() {
        // 64-bit little-endian Mach-O magic: 0xFEEDFACF → file order 0xCF 0xFA 0xED 0xFE.
        assert_eq!(magic_prefix(ObjectFormat::MachO), &[0xCF, 0xFA, 0xED, 0xFE]);
    }

    #[test]
    fn emit_object_module_returns_backend_not_yet_landed() {
        // S7-G6 surface contract : while G1..G5 are in flight, calling
        // emit_object_module must return BackendNotYetLanded so csslc can
        // surface it cleanly + the gate test can SKIP gracefully.
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
        // Verify each error variant's Display includes the actionable info
        // the user needs (which axis, which fn, which op, etc.). This is
        // the diagnostic-quality contract per CSSLv3 specs § DIAGNOSTICS.
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

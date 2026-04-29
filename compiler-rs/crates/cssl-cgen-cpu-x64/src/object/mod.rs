//! § object — owned ELF / COFF / Mach-O writer for the native x86-64 path.
//!
//! Spec : `specs/07_CODEGEN.csl § OBJECT-FILE-WRITING`.
//!
//! § ROLE
//!   Take a slice of `X64Func` (encoded by G1..G4) plus extern imports,
//!   and emit a relocatable object file (`.o` or `.obj`) byte-stream the
//!   S6-A4 linker accepts.
//!
//! § OUTPUT-COMPATIBILITY
//!   The S6-A4 linker invokes a host linker driver (rust-lld / cl /
//!   clang / gcc) — those are insensitive to which Rust crate produced
//!   the object bytes, so as long as G5's output passes per-format
//!   spec validation it's interchangeable with cranelift-object's.

use thiserror::Error;

use crate::func::{X64Func, X64Symbol};

mod coff_x64;
mod elf_x64;
mod macho_x64;

// ───────────────────────────────────────────────────────────────────────
// § ObjectTarget — discriminated tag for the per-format writer.
// ───────────────────────────────────────────────────────────────────────

/// Per-format target selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectTarget {
    /// ELF for x86-64 (Linux + BSD).
    ElfX64,
    /// COFF for x86-64 (Windows MSVC + MinGW).
    CoffX64,
    /// Mach-O for x86-64 (macOS-Intel ; Apple-Silicon uses Mach-O-aarch64
    /// which is out of scope for the x64 backend).
    MachOX64,
}

impl ObjectTarget {
    /// Short-name for diagnostics + filenames.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ElfX64 => "elf-x64",
            Self::CoffX64 => "coff-x64",
            Self::MachOX64 => "macho-x64",
        }
    }

    /// Canonical object-file extension on disk.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::ElfX64 => ".o",
            Self::CoffX64 => ".obj",
            Self::MachOX64 => ".o",
        }
    }
}

/// Default object-file target for the host platform.
#[must_use]
pub const fn host_default_target() -> ObjectTarget {
    if cfg!(target_os = "windows") {
        ObjectTarget::CoffX64
    } else if cfg!(target_os = "macos") {
        ObjectTarget::MachOX64
    } else {
        ObjectTarget::ElfX64
    }
}

/// Magic-byte signature the produced object file SHOULD start with for the
/// given target.
///
/// - ELF       : `\x7FELF` (4 bytes ; verified by `EI_MAG0..3`).
/// - COFF      : `64 86`   (2 bytes ; little-endian Machine field for AMD64
///   = `IMAGE_FILE_MACHINE_AMD64 = 0x8664`).
/// - Mach-O LE : `CF FA ED FE` (4 bytes ; little-endian `MH_MAGIC_64`
///   = `0xFEEDFACF`).
#[must_use]
pub const fn magic_prefix(target: ObjectTarget) -> &'static [u8] {
    match target {
        ObjectTarget::ElfX64 => b"\x7FELF",
        ObjectTarget::CoffX64 => &[0x64, 0x86],
        ObjectTarget::MachOX64 => &[0xCF, 0xFA, 0xED, 0xFE],
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ObjectError — emission failure modes.
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ObjectError {
    /// A relocation's `target_index` is out of range of the combined
    /// (funcs ++ extern_imports) symbol table.
    #[error(
        "fn `{fn_name}` reloc at byte +{offset} references symbol index {target_index} \
         but only {symbol_count} symbols are in the module table"
    )]
    BadRelocSymbolIndex {
        fn_name: String,
        offset: u32,
        target_index: u32,
        symbol_count: usize,
    },

    /// A relocation's `offset` falls outside the function's `bytes`.
    #[error("fn `{fn_name}` reloc offset +{offset} is past end-of-bytes (len = {byte_len})")]
    BadRelocOffset {
        fn_name: String,
        offset: u32,
        byte_len: usize,
    },

    /// Two functions share the same name (the linker will reject this).
    #[error("duplicate exported symbol name `{name}` (functions #{first} and #{second})")]
    DuplicateSymbol {
        name: String,
        first: usize,
        second: usize,
    },

    /// The section bodies grew past the 32-bit limit Mach-O / COFF support
    /// for relocatable objects (we don't bother with the COFF "big obj"
    /// extension at stage1+).
    #[error("section `{section}` exceeded size limit ({actual} > {limit})")]
    SectionTooLarge {
        section: &'static str,
        actual: u64,
        limit: u64,
    },
}

// ───────────────────────────────────────────────────────────────────────
// § public API — emit_object_file
// ───────────────────────────────────────────────────────────────────────

/// Serialise a slice of `X64Func` into a relocatable object file targeting
/// the given format.
///
/// `extern_imports` lists symbols referenced by relocations but defined
/// outside this object (e.g. cssl-rt entry points). They become UNDEF
/// entries in the symbol table.
///
/// # Errors
/// Returns [`ObjectError`] on relocation-table corruption or section-size
/// overflow.
pub fn emit_object_file(
    funcs: &[X64Func],
    extern_imports: &[X64Symbol],
    target: ObjectTarget,
) -> Result<Vec<u8>, ObjectError> {
    // ─── 1. validate the symbol table layout ───────────────────────────
    //
    // The combined module symbol table is :
    //   [0] : null/file symbol (per-format conventions)
    //   [1..] : the `funcs` (in slice order)
    //   [funcs.len()+1..] : the `extern_imports` (in slice order)
    //
    // Relocations refer by raw `target_index` (a number assigned by the
    // encoder ; G5 does NOT reassign indices). The encoder is responsible
    // for putting funcs before imports and matching the layout above.
    //
    // We validate that every reloc's index is in range of (funcs ++ imports)
    // — i.e. `[1, 1 + funcs.len() + extern_imports.len())` — and that the
    // offset doesn't run off the end of the function body.
    let total_symbols_excluding_null = funcs.len() + extern_imports.len();
    for func in funcs {
        for r in &func.relocs {
            // target_index 0 is the null symbol — never a valid target.
            // Valid range : 1 ..= total_symbols_excluding_null.
            if r.target_index == 0 || (r.target_index as usize) > total_symbols_excluding_null {
                return Err(ObjectError::BadRelocSymbolIndex {
                    fn_name: func.name.clone(),
                    offset: r.offset,
                    target_index: r.target_index,
                    symbol_count: total_symbols_excluding_null,
                });
            }
            if (r.offset as usize) >= func.bytes.len() {
                return Err(ObjectError::BadRelocOffset {
                    fn_name: func.name.clone(),
                    offset: r.offset,
                    byte_len: func.bytes.len(),
                });
            }
        }
    }

    // ─── 2. duplicate-name check ───────────────────────────────────────
    //
    // The linker would reject duplicate strong symbols anyway, but
    // surfacing it here gives a much clearer error than a downstream
    // link failure.
    for (i, a) in funcs.iter().enumerate() {
        for (j, b) in funcs.iter().enumerate().skip(i + 1) {
            if a.name == b.name {
                return Err(ObjectError::DuplicateSymbol {
                    name: a.name.clone(),
                    first: i,
                    second: j,
                });
            }
        }
    }

    // ─── 3. dispatch to the per-format writer ──────────────────────────
    match target {
        ObjectTarget::ElfX64 => elf_x64::write(funcs, extern_imports),
        ObjectTarget::CoffX64 => coff_x64::write(funcs, extern_imports),
        ObjectTarget::MachOX64 => macho_x64::write(funcs, extern_imports),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § shared writer helpers
// ───────────────────────────────────────────────────────────────────────

/// Append `value` as little-endian bytes (for u8/u16/u32/u64 fields).
pub(crate) trait LeWriter {
    fn write_u8(&mut self, v: u8);
    fn write_u16_le(&mut self, v: u16);
    fn write_u32_le(&mut self, v: u32);
    fn write_u64_le(&mut self, v: u64);
}

impl LeWriter for Vec<u8> {
    fn write_u8(&mut self, v: u8) {
        self.push(v);
    }
    fn write_u16_le(&mut self, v: u16) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_u32_le(&mut self, v: u32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_u64_le(&mut self, v: u64) {
        self.extend_from_slice(&v.to_le_bytes());
    }
}

/// Pad `out` with zero bytes until its length is a multiple of `align`.
pub(crate) fn pad_to_align(out: &mut Vec<u8>, align: usize) {
    while out.len() % align != 0 {
        out.push(0);
    }
}

/// Concatenate all funcs' bytes into one `.text` section, returning :
///   - the combined byte vector (16-byte aligned per function)
///   - per-function entry offset within the section
fn pack_text_section(funcs: &[X64Func]) -> (Vec<u8>, Vec<u32>) {
    let mut section = Vec::new();
    let mut offsets = Vec::with_capacity(funcs.len());
    for f in funcs {
        // Functions are 16-byte aligned per x86-64 ABI convention.
        pad_to_align(&mut section, 16);
        offsets.push(section.len() as u32);
        section.extend_from_slice(&f.bytes);
    }
    // Final pad to 16 so the section size is a clean multiple.
    pad_to_align(&mut section, 16);
    (section, offsets)
}

// ───────────────────────────────────────────────────────────────────────
// § module-level validation tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        emit_object_file, host_default_target, magic_prefix, pack_text_section, ObjectError,
        ObjectTarget,
    };
    use crate::func::{X64Func, X64Reloc, X64RelocKind, X64Symbol};

    #[test]
    fn target_extensions() {
        assert_eq!(ObjectTarget::ElfX64.extension(), ".o");
        assert_eq!(ObjectTarget::CoffX64.extension(), ".obj");
        assert_eq!(ObjectTarget::MachOX64.extension(), ".o");
    }

    #[test]
    fn target_str_short_names() {
        assert_eq!(ObjectTarget::ElfX64.as_str(), "elf-x64");
        assert_eq!(ObjectTarget::CoffX64.as_str(), "coff-x64");
        assert_eq!(ObjectTarget::MachOX64.as_str(), "macho-x64");
    }

    #[test]
    fn host_default_picks_one_target() {
        // Just make sure the const-fn returns *some* target — exact value
        // is platform-dependent.
        let t = host_default_target();
        assert!(matches!(
            t,
            ObjectTarget::ElfX64 | ObjectTarget::CoffX64 | ObjectTarget::MachOX64
        ));
    }

    #[test]
    fn magic_prefix_per_format() {
        assert_eq!(magic_prefix(ObjectTarget::ElfX64), b"\x7FELF");
        assert_eq!(magic_prefix(ObjectTarget::CoffX64), &[0x64, 0x86]);
        assert_eq!(
            magic_prefix(ObjectTarget::MachOX64),
            &[0xCF, 0xFA, 0xED, 0xFE]
        );
    }

    #[test]
    fn bad_reloc_symbol_index_rejected() {
        let r = X64Reloc {
            offset: 0,
            target_index: 99,
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("x", vec![0xE8, 0, 0, 0, 0], vec![r], true).unwrap();
        let err = emit_object_file(&[f], &[], ObjectTarget::ElfX64).unwrap_err();
        assert!(matches!(err, ObjectError::BadRelocSymbolIndex { .. }));
    }

    #[test]
    fn zero_reloc_target_rejected() {
        let r = X64Reloc {
            offset: 0,
            target_index: 0, // null symbol — invalid
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("x", vec![0xE8, 0, 0, 0, 0], vec![r], true).unwrap();
        let err = emit_object_file(&[f], &[], ObjectTarget::ElfX64).unwrap_err();
        assert!(matches!(err, ObjectError::BadRelocSymbolIndex { .. }));
    }

    #[test]
    fn reloc_offset_past_end_rejected() {
        let r = X64Reloc {
            offset: 50,
            target_index: 1,
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("x", vec![0xC3], vec![r], true).unwrap();
        let err = emit_object_file(&[f], &[], ObjectTarget::ElfX64).unwrap_err();
        assert!(matches!(err, ObjectError::BadRelocOffset { .. }));
    }

    #[test]
    fn duplicate_func_name_rejected() {
        let a = X64Func::leaf_export("dup", vec![0xC3]).unwrap();
        let b = X64Func::leaf_export("dup", vec![0xC3]).unwrap();
        let err = emit_object_file(&[a, b], &[], ObjectTarget::ElfX64).unwrap_err();
        assert!(matches!(err, ObjectError::DuplicateSymbol { .. }));
    }

    #[test]
    fn extern_import_index_within_range() {
        let r = X64Reloc {
            offset: 1,
            target_index: 2, // 1 = funcs[0], 2 = imports[0]
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("caller", vec![0xE8, 0, 0, 0, 0, 0xC3], vec![r], true).unwrap();
        let imp = X64Symbol::new_function("__cssl_callee").unwrap();
        // Should not error — index 2 is in range (1 func + 1 import).
        emit_object_file(&[f], &[imp], ObjectTarget::ElfX64).unwrap();
    }

    #[test]
    fn pack_text_aligns_each_function_to_16() {
        let a = X64Func::leaf_export("a", vec![0xC3]).unwrap(); // 1 byte
        let b = X64Func::leaf_export("b", vec![0xB8, 0x2A, 0, 0, 0, 0xC3]).unwrap(); // 6 bytes
        let (section, offsets) = pack_text_section(&[a, b]);
        // First func at offset 0, second padded to 16-byte boundary.
        assert_eq!(offsets, vec![0, 16]);
        // Section padded to 16-byte multiple.
        assert_eq!(section.len() % 16, 0);
    }
}

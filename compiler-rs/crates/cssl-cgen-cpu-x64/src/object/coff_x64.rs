//! § coff_x64 — relocatable COFF (`.obj`) writer for x86-64 (Windows MSVC).
//!
//! Reference : <https://learn.microsoft.com/en-us/windows/win32/debug/pe-format>
//! § "COFF File Header (Object and Image)" + § "Section Table" + § "COFF
//! Symbol Table" + § "COFF Relocations" + AMD64 relocation types.
//!
//! § FILE LAYOUT
//!   ```text
//!   [ COFF file header        ]   20 bytes
//!   [ section header table    ]   40 bytes per section
//!   [ section bodies          ]   .text bytes + .rdata + ... (4-byte aligned)
//!   [ section relocations     ]   10 bytes per IMAGE_RELOCATION
//!   [ symbol table            ]   18 bytes per IMAGE_SYMBOL (+ aux)
//!   [ string table            ]   4-byte size header + NUL-separated strings
//!   ```
//!
//! § AMD64 RELOCATION TYPES (subset we emit)
//!   - `IMAGE_REL_AMD64_ABSOLUTE = 0x0000`  (no-op ; never emitted)
//!   - `IMAGE_REL_AMD64_ADDR64   = 0x0001`  (64-bit absolute VA)
//!   - `IMAGE_REL_AMD64_REL32    = 0x0004`  (32-bit RIP-relative)
//!
//! § SHORT-VS-LONG SYMBOL NAMES
//!   COFF stores symbol names ≤ 8 bytes inline in the symbol record's
//!   `Name[8]` field. Longer names live in a separate string table ; the
//!   `Name[8]` field then holds `0,0,0,0,offset_le32`.

use crate::func::{X64Func, X64Symbol};
use crate::object::{pack_text_section, LeWriter, ObjectError};

// ───────────────────────────────────────────────────────────────────────
// § COFF spec constants
// ───────────────────────────────────────────────────────────────────────

const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_LINE_NUMS_STRIPPED: u16 = 0x0004;

const COFF_HEADER_SIZE: u32 = 20;
const COFF_SECTION_HEADER_SIZE: u32 = 40;
const COFF_SYMBOL_SIZE: u32 = 18;
const COFF_RELOCATION_SIZE: u32 = 10;

// (kept under `_` prefix for documentation of the per-format mapping ;
//  the canonical values come from `X64RelocKind::coff_type()`.)
const _IMAGE_REL_AMD64_ADDR64: u16 = 0x0001;
const _IMAGE_REL_AMD64_REL32: u16 = 0x0004;

// Section characteristics.
const IMAGE_SCN_CNT_CODE: u32 = 0x0000_0020;
const IMAGE_SCN_ALIGN_16BYTES: u32 = 0x0050_0000;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;

// Storage classes.
const IMAGE_SYM_CLASS_EXTERNAL: u8 = 2;
const IMAGE_SYM_CLASS_STATIC: u8 = 3;
const IMAGE_SYM_CLASS_FILE: u8 = 103;

// Symbol types.
const IMAGE_SYM_TYPE_NULL: u16 = 0x0000;
const IMAGE_SYM_DTYPE_FUNCTION: u16 = 0x0020; // 2 << 4

// Section numbers.
const IMAGE_SYM_UNDEFINED: i16 = 0;
const IMAGE_SYM_ABSOLUTE: i16 = -1;

// ───────────────────────────────────────────────────────────────────────
// § COFF symbol-name slot helper
// ───────────────────────────────────────────────────────────────────────

/// Encode `name` into COFF's 8-byte `Name[8]` field. Names ≤ 8 bytes are
/// inlined ; longer ones spill into `strtab` and the slot becomes
/// `[0, 0, 0, 0, offset_le32]`. Returns the 8-byte value to place in the
/// symbol record.
fn coff_name(strtab: &mut Vec<u8>, name: &str) -> [u8; 8] {
    let bytes = name.as_bytes();
    if bytes.len() <= 8 {
        let mut buf = [0u8; 8];
        buf[..bytes.len()].copy_from_slice(bytes);
        buf
    } else {
        let off = strtab.len() as u32;
        strtab.extend_from_slice(bytes);
        strtab.push(0); // NUL-terminate
        let mut buf = [0u8; 8];
        // bytes 0..4 = 0 (signals "use string-table offset")
        buf[4..8].copy_from_slice(&off.to_le_bytes());
        buf
    }
}

// ───────────────────────────────────────────────────────────────────────
// § public — write
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub(super) fn write(
    funcs: &[X64Func],
    extern_imports: &[X64Symbol],
) -> Result<Vec<u8>, ObjectError> {
    // ─── 1. pack .text section bytes + per-fn offsets ──────────────────
    let (text_bytes, text_offsets) = pack_text_section(funcs);

    // ─── 2. build the relocation table for .text ───────────────────────
    let mut text_relocs: Vec<CoffReloc> = Vec::new();

    // We need the eventual symbol-table layout BEFORE we can index into
    // it. COFF symbol order :
    //   [0] file symbol (".file") + 1 aux
    //   [..] section symbol for .text + 1 aux
    //   [..] funcs (each : 1 entry, no aux for stage1+ minimum)
    //   [..] extern imports (each : 1 entry)
    //
    // raw_to_sym[r.target_index] = symtab index of the corresponding sym.
    let mut raw_to_sym: Vec<u32> = vec![0; 1 + funcs.len() + extern_imports.len()];

    // [0] = .file (1 entry + 1 aux = 2 slots)
    let file_sym_idx: u32 = 0;
    let mut next_sym_idx: u32 = file_sym_idx + 2;

    // [next] = section symbol for .text (1 entry + 1 aux). The index is
    // reserved here ; relocations don't refer to it directly (they use the
    // per-function symbol indices), so we just skip the slots.
    next_sym_idx += 2;

    // [next..] = funcs
    for (i, _f) in funcs.iter().enumerate() {
        raw_to_sym[i + 1] = next_sym_idx;
        next_sym_idx += 1;
    }
    // [next..] = imports
    for (j, _imp) in extern_imports.iter().enumerate() {
        raw_to_sym[funcs.len() + 1 + j] = next_sym_idx;
        next_sym_idx += 1;
    }
    let total_sym_entries = next_sym_idx;

    // Now build the actual relocation entries.
    for (i, f) in funcs.iter().enumerate() {
        for r in &f.relocs {
            let target_sym = raw_to_sym[r.target_index as usize];
            text_relocs.push(CoffReloc {
                virtual_address: text_offsets[i] + r.offset,
                symbol_table_index: target_sym,
                ty: r.kind.coff_type(),
            });
        }
    }

    // ─── 3. build the string table (4-byte size header + names) ────────
    //
    // COFF inlines short names (≤ 8 bytes) directly in the symbol record.
    // Longer names live in this table. Size header = 4 LE bytes giving
    // the table's total size including itself, so the minimum is 4
    // (= just the header, no strings).
    let mut strtab = Vec::<u8>::new();
    strtab.extend_from_slice(&[0, 0, 0, 0]); // placeholder ; patched below

    // ─── 4. compute section offsets (header table + bodies) ────────────
    //
    // File layout :
    //   [ COFF header (20) ]
    //   [ section headers (40 * num_sections) ]
    //   [ .text body ]
    //   [ .text relocations ]
    //   [ symbol table ]
    //   [ string table ]

    let num_sections: u16 = 1; // .text only ; .data/.bss empty for stage1+
    let header_table_end = u32::from(num_sections) * COFF_SECTION_HEADER_SIZE + COFF_HEADER_SIZE;
    // Align .text body to 16 bytes within the file.
    let text_file_off = align_up(header_table_end, 4); // COFF requires 4-byte minimum

    let text_size = text_bytes.len() as u32;
    let text_relocs_file_off = text_file_off + text_size;
    let text_relocs_size = (text_relocs.len() as u32) * COFF_RELOCATION_SIZE;

    let symtab_file_off = text_relocs_file_off + text_relocs_size;
    let symtab_size = total_sym_entries * COFF_SYMBOL_SIZE;
    let strtab_file_off = symtab_file_off + symtab_size;

    // ─── 5. build symbol entries (need string table populated as we go) ─
    let mut sym_records: Vec<CoffSym> = Vec::new();

    // [0] .file symbol + aux (aux holds the source filename)
    let file_name = "cssl_x64.cssl";
    sym_records.push(CoffSym {
        name: *b".file\0\0\0",
        value: 0,
        section_number: IMAGE_SYM_ABSOLUTE,
        ty: IMAGE_SYM_TYPE_NULL,
        storage_class: IMAGE_SYM_CLASS_FILE,
        number_of_aux_symbols: 1,
        aux: AuxRecord::File(*b"cssl_x64.cssl\0\0\0\0\0"),
    });
    let _ = file_name; // kept for future-flexibility ; aux already inlined

    // [..] .text section symbol + 1 aux
    sym_records.push(CoffSym {
        name: *b".text\0\0\0",
        value: 0,
        section_number: 1,
        ty: IMAGE_SYM_TYPE_NULL,
        storage_class: IMAGE_SYM_CLASS_STATIC,
        number_of_aux_symbols: 1,
        aux: AuxRecord::Section {
            length: text_size,
            number_of_relocations: text_relocs.len() as u16,
            number_of_line_numbers: 0,
            checksum: 0,
            number: 1, // section number this aux describes
            selection: 0,
        },
    });

    // [..] funcs
    for (i, f) in funcs.iter().enumerate() {
        let storage = if f.is_export {
            IMAGE_SYM_CLASS_EXTERNAL
        } else {
            IMAGE_SYM_CLASS_STATIC
        };
        // Windows MSVC convention prepends `_` to symbol names on x86 ;
        // x86-64 uses the bare name (no underscore prefix).
        let name = coff_name(&mut strtab, &f.name);
        sym_records.push(CoffSym {
            name,
            value: text_offsets[i],
            section_number: 1,
            ty: IMAGE_SYM_DTYPE_FUNCTION,
            storage_class: storage,
            number_of_aux_symbols: 0,
            aux: AuxRecord::None,
        });
    }

    // [..] extern imports — UNDEF + EXTERNAL
    for imp in extern_imports {
        let name = coff_name(&mut strtab, &imp.name);
        sym_records.push(CoffSym {
            name,
            value: 0,
            section_number: IMAGE_SYM_UNDEFINED,
            ty: IMAGE_SYM_DTYPE_FUNCTION,
            storage_class: IMAGE_SYM_CLASS_EXTERNAL,
            number_of_aux_symbols: 0,
            aux: AuxRecord::None,
        });
    }

    // Patch the strtab size header now that the table is final.
    let strtab_size = strtab.len() as u32;
    strtab[..4].copy_from_slice(&strtab_size.to_le_bytes());

    // ─── 6. emit the bytes in order ────────────────────────────────────
    let mut out: Vec<u8> = Vec::new();

    // [a] COFF file header
    out.write_u16_le(IMAGE_FILE_MACHINE_AMD64);
    out.write_u16_le(num_sections);
    out.write_u32_le(0); // TimeDateStamp = 0 (deterministic)
    out.write_u32_le(symtab_file_off);
    out.write_u32_le(total_sym_entries);
    out.write_u16_le(0); // SizeOfOptionalHeader = 0 for objects
    out.write_u16_le(IMAGE_FILE_LINE_NUMS_STRIPPED);

    // [b] Section header table
    let chars =
        IMAGE_SCN_CNT_CODE | IMAGE_SCN_ALIGN_16BYTES | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ;
    write_section_header(
        &mut out,
        *b".text\0\0\0",
        text_size,
        text_file_off,
        text_relocs_file_off,
        text_relocs.len() as u16,
        chars,
    );

    // [c] Section bodies + relocations
    out.extend_from_slice(&text_bytes);
    for r in &text_relocs {
        out.write_u32_le(r.virtual_address);
        out.write_u32_le(r.symbol_table_index);
        out.write_u16_le(r.ty);
    }

    // [d] Symbol table
    for s in &sym_records {
        out.extend_from_slice(&s.name);
        out.write_u32_le(s.value);
        out.write_u16_le(s.section_number as u16);
        out.write_u16_le(s.ty);
        out.write_u8(s.storage_class);
        out.write_u8(s.number_of_aux_symbols);
        match &s.aux {
            AuxRecord::None => {}
            AuxRecord::File(name_bytes) => {
                // 18-byte aux record carrying the source-file name.
                out.extend_from_slice(name_bytes);
            }
            AuxRecord::Section {
                length,
                number_of_relocations,
                number_of_line_numbers,
                checksum,
                number,
                selection,
            } => {
                out.write_u32_le(*length);
                out.write_u16_le(*number_of_relocations);
                out.write_u16_le(*number_of_line_numbers);
                out.write_u32_le(*checksum);
                out.write_u16_le(*number);
                out.write_u8(*selection);
                // 3 bytes of padding to reach 18 bytes total.
                out.write_u8(0);
                out.write_u8(0);
                out.write_u8(0);
            }
        }
    }

    // [e] String table
    out.extend_from_slice(&strtab);

    // Sanity check : the symbol-table file offset should match what we
    // claimed in the header. (If this fires, we'd be writing a malformed
    // .obj — fail loudly.)
    debug_assert_eq!(
        symtab_file_off,
        (header_table_end + text_size + text_relocs_size),
        "symtab file offset mismatch"
    );
    debug_assert_eq!(strtab_file_off, symtab_file_off + symtab_size);

    Ok(out)
}

// ───────────────────────────────────────────────────────────────────────
// § COFF data structures
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct CoffReloc {
    virtual_address: u32,
    symbol_table_index: u32,
    ty: u16,
}

struct CoffSym {
    name: [u8; 8],
    value: u32,
    section_number: i16,
    ty: u16,
    storage_class: u8,
    number_of_aux_symbols: u8,
    aux: AuxRecord,
}

enum AuxRecord {
    None,
    /// File symbol aux : 18-byte path (NUL-padded).
    File([u8; 18]),
    /// Section symbol aux record (per PE-Format § "Auxiliary Format 5: Section Definitions").
    Section {
        length: u32,
        number_of_relocations: u16,
        number_of_line_numbers: u16,
        checksum: u32,
        number: u16,
        selection: u8,
    },
}

fn write_section_header(
    out: &mut Vec<u8>,
    name: [u8; 8],
    size: u32,
    file_offset: u32,
    relocs_offset: u32,
    num_relocs: u16,
    characteristics: u32,
) {
    out.extend_from_slice(&name);
    out.write_u32_le(0); // VirtualSize
    out.write_u32_le(0); // VirtualAddress
    out.write_u32_le(size); // SizeOfRawData
    out.write_u32_le(file_offset); // PointerToRawData
    out.write_u32_le(if num_relocs > 0 { relocs_offset } else { 0 }); // PointerToRelocations
    out.write_u32_le(0); // PointerToLineNumbers
    out.write_u16_le(num_relocs); // NumberOfRelocations
    out.write_u16_le(0); // NumberOfLineNumbers
    out.write_u32_le(characteristics);
}

#[inline]
fn align_up(value: u32, align: u32) -> u32 {
    (value + align - 1) & !(align - 1)
}

// ───────────────────────────────────────────────────────────────────────
// § per-format unit tests (10)
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::write;
    use crate::func::{X64Func, X64Reloc, X64RelocKind, X64Symbol};
    use crate::object::magic_prefix;
    use crate::object::ObjectTarget;

    const MAIN_42: &[u8] = &[0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];

    fn take_main42() -> X64Func {
        X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap()
    }

    #[test]
    fn coff_starts_with_machine_amd64() {
        let out = write(&[take_main42()], &[]).unwrap();
        // Machine field at offset 0 = IMAGE_FILE_MACHINE_AMD64 = 0x8664.
        // Little-endian on disk = bytes 64 86.
        assert_eq!(&out[..2], magic_prefix(ObjectTarget::CoffX64));
    }

    #[test]
    fn coff_section_count_is_one() {
        let out = write(&[take_main42()], &[]).unwrap();
        let n_sections = u16::from_le_bytes([out[2], out[3]]);
        assert_eq!(n_sections, 1);
    }

    #[test]
    fn coff_timedatestamp_is_zero() {
        let out = write(&[take_main42()], &[]).unwrap();
        let ts = u32::from_le_bytes(out[4..8].try_into().unwrap());
        assert_eq!(ts, 0); // deterministic
    }

    #[test]
    fn coff_optional_header_size_is_zero() {
        let out = write(&[take_main42()], &[]).unwrap();
        let size_opt = u16::from_le_bytes([out[16], out[17]]);
        assert_eq!(size_opt, 0); // .obj has no optional header
    }

    #[test]
    fn coff_symtab_file_offset_present() {
        let out = write(&[take_main42()], &[]).unwrap();
        let symtab_off = u32::from_le_bytes(out[8..12].try_into().unwrap());
        let n_syms = u32::from_le_bytes(out[12..16].try_into().unwrap());
        assert!(symtab_off > 20); // past the file header
        assert!(symtab_off < out.len() as u32);
        // We always have at least file (2) + section (2) + 1 func = 5 entries.
        assert!(n_syms >= 5);
    }

    #[test]
    fn coff_text_section_header_present() {
        let out = write(&[take_main42()], &[]).unwrap();
        // First section header starts at offset 20.
        // Name field is `.text\0\0\0`.
        assert_eq!(&out[20..28], b".text\0\0\0");
    }

    #[test]
    fn coff_text_section_has_code_flag() {
        let out = write(&[take_main42()], &[]).unwrap();
        // Characteristics is the last 4 bytes of the 40-byte section header.
        let chars = u32::from_le_bytes(out[56..60].try_into().unwrap());
        // IMAGE_SCN_CNT_CODE = 0x20.
        assert_ne!(chars & 0x20, 0);
        // IMAGE_SCN_MEM_EXECUTE = 0x2000_0000.
        assert_ne!(chars & 0x2000_0000, 0);
    }

    #[test]
    fn coff_text_bytes_present() {
        let out = write(&[take_main42()], &[]).unwrap();
        let body_start = (60..out.len() - 6)
            .find(|&i| &out[i..i + 6] == MAIN_42)
            .expect("`mov eax, 42 ; ret` body should appear");
        assert!(body_start >= 60);
    }

    #[test]
    fn coff_relocations_emitted_when_present() {
        let r = X64Reloc {
            offset: 1,
            target_index: 2, // imports[0]
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("caller", vec![0xE8, 0, 0, 0, 0, 0xC3], vec![r], true).unwrap();
        let imp = X64Symbol::new_function("__cssl_callee").unwrap();
        let out = write(&[f], &[imp]).unwrap();
        // The .text section header's NumberOfRelocations field (offset 56-2 = 32 bytes
        // into the section header = 20 + 32 = 52) should be 1.
        let n_relocs = u16::from_le_bytes([out[52], out[53]]);
        assert_eq!(n_relocs, 1);
    }

    #[test]
    fn coff_long_symbol_name_uses_string_table() {
        // Symbol name longer than 8 bytes must spill into the string table.
        let f = X64Func::leaf_export("function_with_long_name", MAIN_42.to_vec()).unwrap();
        let out = write(&[f], &[]).unwrap();
        // The output should contain the long name somewhere (in the string table).
        let has_long_name = out
            .windows(b"function_with_long_name".len())
            .any(|w| w == b"function_with_long_name");
        assert!(has_long_name);
    }
}

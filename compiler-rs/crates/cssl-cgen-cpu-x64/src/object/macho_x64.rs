//! § macho_x64 — relocatable Mach-O 64-bit (`MH_OBJECT`) writer for x86-64.
//!
//! Reference : <https://github.com/aidansteele/osx-abi-macho-file-format-reference>
//! + Apple's `<mach-o/loader.h>`, `<mach-o/nlist.h>`, `<mach-o/reloc.h>`,
//! `<mach-o/x86_64/reloc.h>`.
//!
//! § FILE LAYOUT
//!   ```text
//!   [ mach_header_64                          ]   32 bytes
//!   [ LC_SEGMENT_64 + section_64 array        ]   72 + 80 * num_sects
//!   [ LC_SYMTAB                               ]   24 bytes
//!   [ LC_DYSYMTAB                             ]   80 bytes (minimal stub)
//!   [ section bodies (.text)                  ]   16-byte aligned
//!   [ relocation entries                      ]   8 bytes per relocation_info
//!   [ symbol table                            ]   16 bytes per nlist_64
//!   [ string table                            ]   NUL-separated names
//!   ```
//!
//! § X86_64 RELOCATION TYPES (`<mach-o/x86_64/reloc.h>`)
//!   - `X86_64_RELOC_UNSIGNED  = 0`   (64-bit absolute address)
//!   - `X86_64_RELOC_SIGNED    = 1`   (32-bit RIP-relative load/store)
//!   - `X86_64_RELOC_BRANCH    = 2`   (32-bit RIP-relative call)
//!   - `X86_64_RELOC_GOT_LOAD  = 3`   (LEA via GOT — out of stage1+ scope)

use crate::func::{X64Func, X64Symbol};
use crate::object::{pack_text_section, LeWriter, ObjectError};

// ───────────────────────────────────────────────────────────────────────
// § Mach-O spec constants
// ───────────────────────────────────────────────────────────────────────

const MH_MAGIC_64: u32 = 0xFEED_FACF;
const CPU_TYPE_X86_64: u32 = 0x0100_0007; // CPU_TYPE_X86 | CPU_ARCH_ABI64
const CPU_SUBTYPE_X86_64_ALL: u32 = 3;
const MH_OBJECT: u32 = 1;
const MH_SUBSECTIONS_VIA_SYMBOLS: u32 = 0x0000_2000;

const LC_SEGMENT_64: u32 = 0x19;
const LC_SYMTAB: u32 = 0x02;
const LC_DYSYMTAB: u32 = 0x0B;

const MACH_HEADER_64_SIZE: u32 = 32;
const SEGMENT_COMMAND_64_SIZE: u32 = 72;
const SECTION_64_SIZE: u32 = 80;
const SYMTAB_COMMAND_SIZE: u32 = 24;
const DYSYMTAB_COMMAND_SIZE: u32 = 80;
const NLIST_64_SIZE: u32 = 16;
const RELOCATION_INFO_SIZE: u32 = 8;

// nlist_64 n_type bits.
const N_EXT: u8 = 0x01;
const N_SECT: u8 = 0x0E;
const N_UNDF: u8 = 0x00;

// Section attributes / flags.
const S_ATTR_PURE_INSTRUCTIONS: u32 = 0x8000_0000;
const S_ATTR_SOME_INSTRUCTIONS: u32 = 0x0000_0400;
const S_REGULAR: u32 = 0;

// VM protection bits.
const VM_PROT_READ: u32 = 0x1;
const VM_PROT_EXECUTE: u32 = 0x4;

// ───────────────────────────────────────────────────────────────────────
// § Mach-O strtab + symbol-name helpers
// ───────────────────────────────────────────────────────────────────────

/// Append `s` followed by a NUL byte to the strtab and return the byte
/// offset where the string starts.
fn add_str(buf: &mut Vec<u8>, s: &[u8]) -> u32 {
    let off = buf.len() as u32;
    buf.extend_from_slice(s);
    buf.push(0);
    off
}

/// Per Mach-O convention, x86-64 user-space symbols are stored in the
/// symbol table with a leading `_` prefix (so `main` becomes `_main`).
/// otool / nm strip the underscore for display.
fn underscored(name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() + 1);
    out.push(b'_');
    out.extend_from_slice(name.as_bytes());
    out
}

// ───────────────────────────────────────────────────────────────────────
// § public — write
// ───────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub(super) fn write(
    funcs: &[X64Func],
    extern_imports: &[X64Symbol],
) -> Result<Vec<u8>, ObjectError> {
    // ─── 1. pack .text + record per-fn offsets ─────────────────────────
    let (text_bytes, text_offsets) = pack_text_section(funcs);

    // ─── 2. assign symbol-table indices ────────────────────────────────
    //
    // Mach-O nlist_64 ordering :
    //   [..] locally-defined funcs (is_export = false)
    //   [..] exported funcs (is_export = true)
    //   [..] undefined externals
    //
    // LC_DYSYMTAB lets us declare each segment range.
    //
    // raw_to_sym[r.target_index] = nlist index of the corresponding sym.
    let mut raw_to_sym: Vec<u32> = vec![0; 1 + funcs.len() + extern_imports.len()];
    let mut local_symbols: Vec<NList> = Vec::new();
    let mut external_symbols: Vec<NList> = Vec::new();
    let mut undef_symbols: Vec<NList> = Vec::new();

    let mut strtab = Vec::<u8>::new();
    // Mach-O strtab : index 0 = NUL byte (empty string sentinel), index 1
    // typically = " " (space) per nm convention. We use just NUL — many
    // tools accept either.
    strtab.push(0);

    // Phase A : locally-defined funcs.
    for (i, f) in funcs.iter().enumerate() {
        if !f.is_export {
            let name_off = add_str(&mut strtab, &underscored(&f.name));
            local_symbols.push(NList {
                n_strx: name_off,
                n_type: N_SECT,
                n_sect: 1, // section number 1 = __TEXT,__text
                n_desc: 0,
                n_value: u64::from(text_offsets[i]),
            });
            raw_to_sym[i + 1] = (local_symbols.len() - 1) as u32;
        }
    }

    let local_count = local_symbols.len() as u32;

    // Phase B : exported funcs.
    for (i, f) in funcs.iter().enumerate() {
        if f.is_export {
            let name_off = add_str(&mut strtab, &underscored(&f.name));
            external_symbols.push(NList {
                n_strx: name_off,
                n_type: N_EXT | N_SECT,
                n_sect: 1,
                n_desc: 0,
                n_value: u64::from(text_offsets[i]),
            });
            raw_to_sym[i + 1] = local_count + (external_symbols.len() - 1) as u32;
        }
    }

    let external_count = external_symbols.len() as u32;

    // Phase C : undefined externals.
    for (j, imp) in extern_imports.iter().enumerate() {
        let name_off = add_str(&mut strtab, &underscored(&imp.name));
        undef_symbols.push(NList {
            n_strx: name_off,
            n_type: N_EXT | N_UNDF,
            n_sect: 0, // NO_SECT
            n_desc: 0,
            n_value: 0,
        });
        raw_to_sym[funcs.len() + 1 + j] =
            local_count + external_count + (undef_symbols.len() - 1) as u32;
    }

    let undef_count = undef_symbols.len() as u32;
    let total_syms = local_count + external_count + undef_count;

    // ─── 3. build relocation entries ───────────────────────────────────
    //
    // Mach-O r_info layout (32-bit, little-endian) :
    //   bits [0..23]   r_symbolnum (or section number for scattered)
    //   bit  24        r_pcrel (1 = PC-relative)
    //   bits [25..26]  r_length (0=1B, 1=2B, 2=4B, 3=8B)
    //   bit  27        r_extern (1 = symbol number ; 0 = section number)
    //   bits [28..31]  r_type
    let mut text_relocs: Vec<MachOReloc> = Vec::new();
    for (i, f) in funcs.iter().enumerate() {
        for r in &f.relocs {
            let sym = raw_to_sym[r.target_index as usize];
            let r_pcrel: u32 = u32::from(r.kind.is_pc_relative());
            let r_length: u32 = match r.kind {
                crate::func::X64RelocKind::Abs64 => 3, // 8 bytes
                _ => 2,                                // 4 bytes
            };
            let r_extern: u32 = 1; // always symbol-based
            let r_type: u32 = u32::from(r.kind.macho_r_type());
            let r_info = (sym & 0x00FF_FFFF)
                | (r_pcrel << 24)
                | (r_length << 25)
                | (r_extern << 27)
                | (r_type << 28);
            text_relocs.push(MachOReloc {
                r_address: text_offsets[i] + r.offset,
                r_info,
            });
        }
    }

    // ─── 4. compute file layout ────────────────────────────────────────
    //
    // Headers come first, then the section bodies, then relocs, then
    // symtab, then strtab.
    let num_sections: u32 = 1; // __TEXT,__text
    let load_commands_size = SEGMENT_COMMAND_64_SIZE
        + SECTION_64_SIZE * num_sections
        + SYMTAB_COMMAND_SIZE
        + DYSYMTAB_COMMAND_SIZE;
    let n_cmds: u32 = 3; // LC_SEGMENT_64 + LC_SYMTAB + LC_DYSYMTAB

    let header_end = MACH_HEADER_64_SIZE + load_commands_size;

    // Section bodies : 16-byte aligned within the file.
    let text_file_off = align_up(header_end, 16);
    let text_size = text_bytes.len() as u32;

    let reloc_file_off = align_up(text_file_off + text_size, 4);
    let reloc_size = (text_relocs.len() as u32) * RELOCATION_INFO_SIZE;

    let symtab_file_off = align_up(reloc_file_off + reloc_size, 4);
    let symtab_size = total_syms * NLIST_64_SIZE;

    let strtab_file_off = symtab_file_off + symtab_size;
    let strtab_size = strtab.len() as u32;

    // ─── 5. emit the bytes ─────────────────────────────────────────────
    let mut out: Vec<u8> = Vec::new();

    // [a] mach_header_64
    out.write_u32_le(MH_MAGIC_64);
    out.write_u32_le(CPU_TYPE_X86_64);
    out.write_u32_le(CPU_SUBTYPE_X86_64_ALL);
    out.write_u32_le(MH_OBJECT);
    out.write_u32_le(n_cmds);
    out.write_u32_le(load_commands_size);
    out.write_u32_le(MH_SUBSECTIONS_VIA_SYMBOLS);
    out.write_u32_le(0); // reserved

    // [b] LC_SEGMENT_64 — segment with one section (__TEXT,__text)
    out.write_u32_le(LC_SEGMENT_64);
    out.write_u32_le(SEGMENT_COMMAND_64_SIZE + SECTION_64_SIZE * num_sections);
    let mut segname = [0u8; 16];
    // Segment name is empty in MH_OBJECT files (per Apple convention) ;
    // sections supply __TEXT individually.
    out.extend_from_slice(&segname);
    out.write_u64_le(0); // vmaddr
    out.write_u64_le(u64::from(text_size)); // vmsize = sum of section sizes
    out.write_u64_le(u64::from(text_file_off)); // fileoff
    out.write_u64_le(u64::from(text_size)); // filesize
    out.write_u32_le(VM_PROT_READ | VM_PROT_EXECUTE); // maxprot
    out.write_u32_le(VM_PROT_READ | VM_PROT_EXECUTE); // initprot
    out.write_u32_le(num_sections);
    out.write_u32_le(0); // flags

    // [b.1] section_64 — __TEXT,__text
    let mut sectname = [0u8; 16];
    let s = b"__text";
    sectname[..s.len()].copy_from_slice(s);
    out.extend_from_slice(&sectname);
    let s = b"__TEXT";
    segname.fill(0);
    segname[..s.len()].copy_from_slice(s);
    out.extend_from_slice(&segname);
    out.write_u64_le(0); // addr
    out.write_u64_le(u64::from(text_size)); // size
    out.write_u32_le(text_file_off); // offset
    out.write_u32_le(4); // align = 2^4 = 16
    out.write_u32_le(if text_relocs.is_empty() {
        0
    } else {
        reloc_file_off
    }); // reloff
    out.write_u32_le(text_relocs.len() as u32); // nreloc
    out.write_u32_le(S_REGULAR | S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS);
    out.write_u32_le(0); // reserved1
    out.write_u32_le(0); // reserved2
    out.write_u32_le(0); // reserved3

    // [c] LC_SYMTAB
    out.write_u32_le(LC_SYMTAB);
    out.write_u32_le(SYMTAB_COMMAND_SIZE);
    out.write_u32_le(symtab_file_off);
    out.write_u32_le(total_syms);
    out.write_u32_le(strtab_file_off);
    out.write_u32_le(strtab_size);

    // [d] LC_DYSYMTAB — declares the local/external/undef ranges in the
    // sym table that LC_SYMTAB pointed to.
    out.write_u32_le(LC_DYSYMTAB);
    out.write_u32_le(DYSYMTAB_COMMAND_SIZE);
    out.write_u32_le(0); // ilocalsym
    out.write_u32_le(local_count); // nlocalsym
    out.write_u32_le(local_count); // iextdefsym
    out.write_u32_le(external_count); // nextdefsym
    out.write_u32_le(local_count + external_count); // iundefsym
    out.write_u32_le(undef_count); // nundefsym
                                   // The remaining 14 fields of DYSYMTAB are tables we don't emit (TOC,
                                   // module table, indirect symbol table, external relocations,
                                   // local relocations) — all zero offsets + zero counts.
    for _ in 0..14 {
        out.write_u32_le(0);
    }

    // Pad to text_file_off.
    while out.len() < text_file_off as usize {
        out.push(0);
    }

    // [e] .text body
    out.extend_from_slice(&text_bytes);

    // Pad to reloc_file_off.
    while out.len() < reloc_file_off as usize {
        out.push(0);
    }

    // [f] relocations
    for r in &text_relocs {
        out.write_u32_le(r.r_address);
        out.write_u32_le(r.r_info);
    }

    // Pad to symtab_file_off.
    while out.len() < symtab_file_off as usize {
        out.push(0);
    }

    // [g] symbol table — locals + externals + undefs in that order
    for s in local_symbols
        .iter()
        .chain(external_symbols.iter())
        .chain(undef_symbols.iter())
    {
        out.write_u32_le(s.n_strx);
        out.write_u8(s.n_type);
        out.write_u8(s.n_sect);
        out.write_u16_le(s.n_desc);
        out.write_u64_le(s.n_value);
    }

    // [h] string table
    out.extend_from_slice(&strtab);

    Ok(out)
}

// ───────────────────────────────────────────────────────────────────────
// § Mach-O data structures
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct NList {
    n_strx: u32,
    n_type: u8,
    n_sect: u8,
    n_desc: u16,
    n_value: u64,
}

#[derive(Debug, Clone, Copy)]
struct MachOReloc {
    r_address: u32,
    r_info: u32,
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
    fn macho_starts_with_magic() {
        let out = write(&[take_main42()], &[]).unwrap();
        // mach_header_64 magic (LE) = 0xFEEDFACF → bytes CF FA ED FE.
        assert_eq!(&out[..4], magic_prefix(ObjectTarget::MachOX64));
    }

    #[test]
    fn macho_cpu_type_is_x86_64() {
        let out = write(&[take_main42()], &[]).unwrap();
        let cpu = u32::from_le_bytes(out[4..8].try_into().unwrap());
        assert_eq!(cpu, 0x0100_0007); // CPU_TYPE_X86_64
    }

    #[test]
    fn macho_filetype_is_object() {
        let out = write(&[take_main42()], &[]).unwrap();
        let filetype = u32::from_le_bytes(out[12..16].try_into().unwrap());
        assert_eq!(filetype, 1); // MH_OBJECT
    }

    #[test]
    fn macho_ncmds_is_three() {
        let out = write(&[take_main42()], &[]).unwrap();
        let n_cmds = u32::from_le_bytes(out[16..20].try_into().unwrap());
        assert_eq!(n_cmds, 3); // LC_SEGMENT_64 + LC_SYMTAB + LC_DYSYMTAB
    }

    #[test]
    fn macho_subsections_via_symbols_flag_set() {
        let out = write(&[take_main42()], &[]).unwrap();
        let flags = u32::from_le_bytes(out[24..28].try_into().unwrap());
        assert_ne!(flags & 0x0000_2000, 0);
    }

    #[test]
    fn macho_segment_command_present() {
        let out = write(&[take_main42()], &[]).unwrap();
        // First load command begins at offset 32.
        let cmd = u32::from_le_bytes(out[32..36].try_into().unwrap());
        assert_eq!(cmd, 0x19); // LC_SEGMENT_64
    }

    #[test]
    fn macho_text_section_named() {
        let out = write(&[take_main42()], &[]).unwrap();
        // section_64 starts at 32 + 72 = 104. sectname is the first 16 bytes.
        // Expect "__text\0..." for the first six bytes.
        assert_eq!(&out[104..110], b"__text");
        // segname follows at offset 104+16 = 120. Expect "__TEXT".
        assert_eq!(&out[120..126], b"__TEXT");
    }

    #[test]
    fn macho_text_bytes_present() {
        let out = write(&[take_main42()], &[]).unwrap();
        let body_start = (32..out.len() - 6)
            .find(|&i| &out[i..i + 6] == MAIN_42)
            .expect("`mov eax, 42 ; ret` body should appear");
        assert!(body_start > 32);
    }

    #[test]
    fn macho_relocations_emitted_when_present() {
        let r = X64Reloc {
            offset: 1,
            target_index: 2, // imports[0]
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("caller", vec![0xE8, 0, 0, 0, 0, 0xC3], vec![r], true).unwrap();
        let imp = X64Symbol::new_function("__cssl_callee").unwrap();
        let out = write(&[f], &[imp]).unwrap();
        // The section_64's nreloc field is at offset 104+16+16+8+8+4+4 = 160
        // (sectname[16] + segname[16] + addr[8] + size[8] + offset[4] + align[4]
        //  + reloff[4]) = 160 ; nreloc lives at 164.
        let nreloc = u32::from_le_bytes(out[164..168].try_into().unwrap());
        assert_eq!(nreloc, 1);
    }

    #[test]
    fn macho_external_symbol_underscored() {
        let f = X64Func::leaf_export("hello", MAIN_42.to_vec()).unwrap();
        let out = write(&[f], &[]).unwrap();
        // Mach-O convention prepends `_` to user symbols.
        let has_underscored = out.windows(b"_hello".len()).any(|w| w == b"_hello");
        assert!(has_underscored);
    }
}

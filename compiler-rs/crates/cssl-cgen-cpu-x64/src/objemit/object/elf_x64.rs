//! § elf_x64 — relocatable ELF64 (`ET_REL`) writer for x86-64.
//!
//! Reference : <https://refspecs.linuxfoundation.org/elf/elf.pdf> +
//! the AMD64 psABI ELF supplement.
//!
//! § FILE LAYOUT
//!   ```text
//!   [ ELF64 header           ]   64 bytes
//!   [ .text bytes            ]   16-byte aligned
//!   [ .rela.text entries     ]   24-byte each (Elf64_Rela)
//!   [ .symtab entries        ]   24-byte each (Elf64_Sym)
//!   [ .strtab bytes          ]   NUL-separated symbol names
//!   [ .shstrtab bytes        ]   NUL-separated section names
//!   [ section header table   ]   64-byte each (Elf64_Shdr)
//!   ```
//!
//! § SECTION INDEX MAP  (relied on by the section header table)
//!   - 0 : SHN_UNDEF (null section)
//!   - 1 : .text
//!   - 2 : .rela.text  (only when at-least-one relocation is present)
//!   - 3 : .symtab
//!   - 4 : .strtab
//!   - 5 : .shstrtab
//!   When no relocations are present we drop `.rela.text` and shift
//!   the indices down by one.

use crate::objemit::func::{X64Func, X64Symbol};
use crate::objemit::object::{pack_text_section, LeWriter, ObjectError};

// ───────────────────────────────────────────────────────────────────────
// § ELF64 spec constants
// ───────────────────────────────────────────────────────────────────────

const EI_CLASS_64: u8 = 2;
const EI_DATA_LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ELFOSABI_SYSV: u8 = 0;

const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;

const ELF64_HEADER_SIZE: u16 = 64;
const ELF64_SHDR_SIZE: u16 = 64;
const ELF64_SYM_SIZE: u64 = 24;
const ELF64_RELA_SIZE: u64 = 24;

// Section header types.
const SHT_NULL: u32 = 0;
const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;

// Section header flags.
const SHF_ALLOC: u64 = 0x2;
const SHF_EXECINSTR: u64 = 0x4;
const SHF_INFO_LINK: u64 = 0x40;

// Symbol bindings (high nibble of `st_info`).
const STB_LOCAL: u8 = 0;
const STB_GLOBAL: u8 = 1;

// Symbol types (low nibble of `st_info`).
const STT_NOTYPE: u8 = 0;
const STT_FUNC: u8 = 2;
const STT_SECTION: u8 = 3;
const STT_FILE: u8 = 4;

// Symbol visibility.
const STV_DEFAULT: u8 = 0;

// Special section indexes.
const SHN_UNDEF: u16 = 0;
const SHN_ABS: u16 = 0xFFF1;

#[inline]
const fn st_info(bind: u8, ty: u8) -> u8 {
    (bind << 4) | (ty & 0x0F)
}

/// Append a NUL-terminated string to the given table and return the byte
/// offset where the string starts (the value to put in `st_name` /
/// `sh_name` fields).
fn add_str(buf: &mut Vec<u8>, s: &[u8]) -> u32 {
    let off = buf.len() as u32;
    buf.extend_from_slice(s);
    buf.push(0);
    off
}

// ───────────────────────────────────────────────────────────────────────
// § public — write
// ───────────────────────────────────────────────────────────────────────

pub(super) fn write(
    funcs: &[X64Func],
    extern_imports: &[X64Symbol],
) -> Result<Vec<u8>, ObjectError> {
    // ─── 1. pack .text + record per-function offsets ───────────────────
    let (text_bytes, text_offsets) = pack_text_section(funcs);

    // ─── 2. build the .strtab + .shstrtab + symbol table ───────────────
    //
    // The symbol table layout (per `tools/readelf -s`) :
    //   index 0 : null symbol (mandatory ; STN_UNDEF)
    //   index 1 : STT_FILE for the source unit name
    //   index 2 : STT_SECTION for .text
    //   index 3..3+N : the funcs (locals first, then globals)
    //   index 3+N..  : the extern imports (UNDEF + STB_GLOBAL)
    //
    // ELF requires locals BEFORE globals in the symtab — `sh_info` of the
    // symtab section header points to the index of the first global
    // (= one-past-end-of-locals).

    let mut strtab = Vec::<u8>::new();
    strtab.push(0); // index 0 = empty string (per spec)

    let mut shstrtab = Vec::<u8>::new();
    shstrtab.push(0);

    // .shstrtab section names — in the order the section header table
    // references them.
    let sh_text_name = add_str(&mut shstrtab, b".text");
    // We only emit .rela.text when at least one func has relocations.
    let has_relocs = funcs.iter().any(|f| !f.relocs.is_empty());
    let sh_rela_text_name = if has_relocs {
        add_str(&mut shstrtab, b".rela.text")
    } else {
        0
    };
    let sh_symtab_name = add_str(&mut shstrtab, b".symtab");
    let sh_strtab_name = add_str(&mut shstrtab, b".strtab");
    let sh_shstrtab_name = add_str(&mut shstrtab, b".shstrtab");

    // .strtab — symbol names. Order matches symbol indexing.
    let file_name_off = add_str(&mut strtab, b"cssl_x64.cssl");

    // Section indices. (`.rela.text` index reserved when relocs exist —
    // the section header table writes it implicitly between `.text` and
    // `.symtab` ; we only need explicit indices for the cross-references.)
    let shndx_text: u16 = 1;
    let shndx_symtab: u16 = if has_relocs { 3 } else { 2 };
    let shndx_strtab: u16 = if has_relocs { 4 } else { 3 };
    let shndx_shstrtab: u16 = if has_relocs { 5 } else { 4 };

    // ─── 3. build symbol entries (locals first, then globals) ──────────
    //
    // We split funcs by export status because ELF demands locals first.

    // Phase A : null + file + section symbols (always local).
    let mut local_syms: Vec<ElfSym> = Vec::new();
    local_syms.push(ElfSym {
        // index 0 — null symbol
        st_name: 0,
        st_info: st_info(STB_LOCAL, STT_NOTYPE),
        st_other: STV_DEFAULT,
        st_shndx: SHN_UNDEF,
        st_value: 0,
        st_size: 0,
    });
    local_syms.push(ElfSym {
        // STT_FILE
        st_name: file_name_off,
        st_info: st_info(STB_LOCAL, STT_FILE),
        st_other: STV_DEFAULT,
        st_shndx: SHN_ABS,
        st_value: 0,
        st_size: 0,
    });
    local_syms.push(ElfSym {
        // STT_SECTION for .text
        st_name: 0, // section symbols use empty name
        st_info: st_info(STB_LOCAL, STT_SECTION),
        st_other: STV_DEFAULT,
        st_shndx: shndx_text,
        st_value: 0,
        st_size: 0,
    });

    // The encoder uses raw 1-based indices into (funcs ++ imports). We
    // need a mapping from those raw indices to the actual symtab indices
    // we end up writing. Build it as we go.
    //
    // Plan : record (raw_index → final_symtab_index) in `raw_to_sym`.
    // raw_index 1.. funcs.len() = funcs[i-1] ; raw_index funcs.len()+1.. =
    // extern_imports[i - funcs.len() - 1].
    let mut raw_to_sym: Vec<u32> = vec![0; 1 + funcs.len() + extern_imports.len()];

    // Phase B : locally-defined funcs (is_export = false).
    for (i, f) in funcs.iter().enumerate() {
        if !f.is_export {
            let name_off = add_str(&mut strtab, f.name.as_bytes());
            let sym_idx = local_syms.len() as u32;
            local_syms.push(ElfSym {
                st_name: name_off,
                st_info: st_info(STB_LOCAL, STT_FUNC),
                st_other: STV_DEFAULT,
                st_shndx: shndx_text,
                st_value: u64::from(text_offsets[i]),
                st_size: f.bytes.len() as u64,
            });
            raw_to_sym[i + 1] = sym_idx;
        }
    }

    // sh_info of the symtab = first global symbol index = one-past-locals.
    let symtab_first_global = local_syms.len() as u32;

    // Phase C : exported funcs (is_export = true).
    let mut global_syms: Vec<ElfSym> = Vec::new();
    for (i, f) in funcs.iter().enumerate() {
        if f.is_export {
            let name_off = add_str(&mut strtab, f.name.as_bytes());
            let sym_idx = (local_syms.len() + global_syms.len()) as u32;
            global_syms.push(ElfSym {
                st_name: name_off,
                st_info: st_info(STB_GLOBAL, STT_FUNC),
                st_other: STV_DEFAULT,
                st_shndx: shndx_text,
                st_value: u64::from(text_offsets[i]),
                st_size: f.bytes.len() as u64,
            });
            raw_to_sym[i + 1] = sym_idx;
        }
    }

    // Phase D : external imports (UNDEF + STB_GLOBAL).
    for (j, imp) in extern_imports.iter().enumerate() {
        let name_off = add_str(&mut strtab, imp.name.as_bytes());
        let sym_idx = (local_syms.len() + global_syms.len()) as u32;
        global_syms.push(ElfSym {
            st_name: name_off,
            st_info: st_info(STB_GLOBAL, STT_NOTYPE),
            st_other: STV_DEFAULT,
            st_shndx: SHN_UNDEF,
            st_value: 0,
            st_size: 0,
        });
        raw_to_sym[funcs.len() + 1 + j] = sym_idx;
    }

    // ─── 4. build relocations against .text ────────────────────────────
    //
    // r_offset is the offset within .text of the patch site = the per-fn
    // text-section offset PLUS the per-reloc within-fn offset.
    let mut relas: Vec<ElfRela> = Vec::new();
    if has_relocs {
        for (i, f) in funcs.iter().enumerate() {
            for r in &f.relocs {
                let target_sym = raw_to_sym[r.target_index as usize];
                relas.push(ElfRela {
                    r_offset: u64::from(text_offsets[i]) + u64::from(r.offset),
                    r_info: ((u64::from(target_sym)) << 32) | u64::from(r.kind.elf_r_type()),
                    r_addend: i64::from(r.addend),
                });
            }
        }
    }

    // ─── 5. compute section-body sizes + offsets ───────────────────────
    // Pre-fill the 64-byte header range with zeros — we patch in the
    // actual header values at step 7 once section offsets are known.
    let mut out: Vec<u8> = vec![0; ELF64_HEADER_SIZE as usize];

    // .text body
    super::pad_to_align(&mut out, 16);
    let text_off_in_file = out.len() as u64;
    out.extend_from_slice(&text_bytes);
    let text_size_in_file = text_bytes.len() as u64;

    // .rela.text body (if any)
    super::pad_to_align(&mut out, 8);
    let rela_off_in_file = out.len() as u64;
    let rela_size_in_file = (relas.len() as u64) * ELF64_RELA_SIZE;
    if has_relocs {
        for r in &relas {
            out.write_u64_le(r.r_offset);
            out.write_u64_le(r.r_info);
            out.write_u64_le(r.r_addend as u64);
        }
    }

    // .symtab body
    super::pad_to_align(&mut out, 8);
    let symtab_off_in_file = out.len() as u64;
    let symtab_count = (local_syms.len() + global_syms.len()) as u64;
    let symtab_size_in_file = symtab_count * ELF64_SYM_SIZE;
    for s in local_syms.iter().chain(global_syms.iter()) {
        out.write_u32_le(s.st_name);
        out.write_u8(s.st_info);
        out.write_u8(s.st_other);
        out.write_u16_le(s.st_shndx);
        out.write_u64_le(s.st_value);
        out.write_u64_le(s.st_size);
    }

    // .strtab body
    let strtab_off_in_file = out.len() as u64;
    let strtab_size_in_file = strtab.len() as u64;
    out.extend_from_slice(&strtab);

    // .shstrtab body
    let shstrtab_off_in_file = out.len() as u64;
    let shstrtab_size_in_file = shstrtab.len() as u64;
    out.extend_from_slice(&shstrtab);

    // ─── 6. section header table (must be 8-byte aligned) ──────────────
    super::pad_to_align(&mut out, 8);
    let shoff = out.len() as u64;

    let shnum: u16 = if has_relocs { 6 } else { 5 };

    // [0] null
    write_shdr(&mut out, &ShdrData::null());

    // [1] .text
    write_shdr(
        &mut out,
        &ShdrData {
            sh_name: sh_text_name,
            sh_type: SHT_PROGBITS,
            sh_flags: SHF_ALLOC | SHF_EXECINSTR,
            sh_addr: 0,
            sh_offset: text_off_in_file,
            sh_size: text_size_in_file,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 16,
            sh_entsize: 0,
        },
    );

    // [2] .rela.text (optional)
    if has_relocs {
        write_shdr(
            &mut out,
            &ShdrData {
                sh_name: sh_rela_text_name,
                sh_type: SHT_RELA,
                sh_flags: SHF_INFO_LINK,
                sh_addr: 0,
                sh_offset: rela_off_in_file,
                sh_size: rela_size_in_file,
                sh_link: u32::from(shndx_symtab),
                sh_info: u32::from(shndx_text),
                sh_addralign: 8,
                sh_entsize: ELF64_RELA_SIZE,
            },
        );
    }

    // [N+0] .symtab
    write_shdr(
        &mut out,
        &ShdrData {
            sh_name: sh_symtab_name,
            sh_type: SHT_SYMTAB,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: symtab_off_in_file,
            sh_size: symtab_size_in_file,
            sh_link: u32::from(shndx_strtab),
            sh_info: symtab_first_global,
            sh_addralign: 8,
            sh_entsize: ELF64_SYM_SIZE,
        },
    );

    // [N+1] .strtab
    write_shdr(
        &mut out,
        &ShdrData {
            sh_name: sh_strtab_name,
            sh_type: SHT_STRTAB,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: strtab_off_in_file,
            sh_size: strtab_size_in_file,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 1,
            sh_entsize: 0,
        },
    );

    // [N+2] .shstrtab
    write_shdr(
        &mut out,
        &ShdrData {
            sh_name: sh_shstrtab_name,
            sh_type: SHT_STRTAB,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: shstrtab_off_in_file,
            sh_size: shstrtab_size_in_file,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 1,
            sh_entsize: 0,
        },
    );

    // ─── 7. patch in the file header ───────────────────────────────────
    write_header(&mut out[..64], shoff, shnum, shndx_shstrtab);

    Ok(out)
}

// ───────────────────────────────────────────────────────────────────────
// § ELF data structures
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct ElfSym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

#[derive(Debug, Clone, Copy)]
struct ElfRela {
    r_offset: u64,
    r_info: u64,
    r_addend: i64,
}

#[derive(Debug, Clone, Copy)]
struct ShdrData {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

impl ShdrData {
    fn null() -> Self {
        Self {
            sh_name: 0,
            sh_type: SHT_NULL,
            sh_flags: 0,
            sh_addr: 0,
            sh_offset: 0,
            sh_size: 0,
            sh_link: 0,
            sh_info: 0,
            sh_addralign: 0,
            sh_entsize: 0,
        }
    }
}

fn write_shdr(out: &mut Vec<u8>, s: &ShdrData) {
    out.write_u32_le(s.sh_name);
    out.write_u32_le(s.sh_type);
    out.write_u64_le(s.sh_flags);
    out.write_u64_le(s.sh_addr);
    out.write_u64_le(s.sh_offset);
    out.write_u64_le(s.sh_size);
    out.write_u32_le(s.sh_link);
    out.write_u32_le(s.sh_info);
    out.write_u64_le(s.sh_addralign);
    out.write_u64_le(s.sh_entsize);
}

/// Write the 64-byte Elf64 file header into the leading 64 bytes of `out`.
fn write_header(out: &mut [u8], shoff: u64, shnum: u16, shstrndx: u16) {
    debug_assert_eq!(out.len(), 64);
    // e_ident
    out[0] = 0x7F;
    out[1] = b'E';
    out[2] = b'L';
    out[3] = b'F';
    out[4] = EI_CLASS_64;
    out[5] = EI_DATA_LSB;
    out[6] = EV_CURRENT;
    out[7] = ELFOSABI_SYSV;
    // e_ident[8..16] = 0 (padding)
    for b in &mut out[8..16] {
        *b = 0;
    }
    // e_type = ET_REL
    out[16..18].copy_from_slice(&ET_REL.to_le_bytes());
    // e_machine = EM_X86_64
    out[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
    // e_version = EV_CURRENT (32-bit)
    out[20..24].copy_from_slice(&u32::from(EV_CURRENT).to_le_bytes());
    // e_entry = 0 (no entry-point in relocatable .o)
    out[24..32].copy_from_slice(&0u64.to_le_bytes());
    // e_phoff = 0 (no program headers)
    out[32..40].copy_from_slice(&0u64.to_le_bytes());
    // e_shoff
    out[40..48].copy_from_slice(&shoff.to_le_bytes());
    // e_flags = 0
    out[48..52].copy_from_slice(&0u32.to_le_bytes());
    // e_ehsize
    out[52..54].copy_from_slice(&ELF64_HEADER_SIZE.to_le_bytes());
    // e_phentsize = 0, e_phnum = 0
    out[54..56].copy_from_slice(&0u16.to_le_bytes());
    out[56..58].copy_from_slice(&0u16.to_le_bytes());
    // e_shentsize
    out[58..60].copy_from_slice(&ELF64_SHDR_SIZE.to_le_bytes());
    // e_shnum
    out[60..62].copy_from_slice(&shnum.to_le_bytes());
    // e_shstrndx
    out[62..64].copy_from_slice(&shstrndx.to_le_bytes());
}

// ───────────────────────────────────────────────────────────────────────
// § per-format unit tests (10)
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::write;
    use crate::objemit::func::{X64Func, X64Reloc, X64RelocKind, X64Symbol};
    use crate::objemit::object::magic_prefix;
    use crate::objemit::object::ObjectTarget;

    /// `mov eax, 42 ; ret`
    const MAIN_42: &[u8] = &[0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];

    fn take_main42() -> X64Func {
        X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap()
    }

    #[test]
    fn elf_starts_with_magic() {
        let out = write(&[take_main42()], &[]).unwrap();
        assert_eq!(&out[..4], magic_prefix(ObjectTarget::ElfX64));
    }

    #[test]
    fn elf_class_is_64_bit() {
        let out = write(&[take_main42()], &[]).unwrap();
        assert_eq!(out[4], 2); // EI_CLASS_64
    }

    #[test]
    fn elf_data_is_lsb() {
        let out = write(&[take_main42()], &[]).unwrap();
        assert_eq!(out[5], 1); // EI_DATA_LSB
    }

    #[test]
    fn elf_type_is_relocatable() {
        let out = write(&[take_main42()], &[]).unwrap();
        let e_type = u16::from_le_bytes([out[16], out[17]]);
        assert_eq!(e_type, 1); // ET_REL
    }

    #[test]
    fn elf_machine_is_x86_64() {
        let out = write(&[take_main42()], &[]).unwrap();
        let e_machine = u16::from_le_bytes([out[18], out[19]]);
        assert_eq!(e_machine, 62); // EM_X86_64
    }

    #[test]
    fn elf_no_program_headers() {
        let out = write(&[take_main42()], &[]).unwrap();
        // e_phoff at [32..40] should be 0
        let e_phoff = u64::from_le_bytes(out[32..40].try_into().unwrap());
        assert_eq!(e_phoff, 0);
    }

    #[test]
    fn elf_section_count_no_relocs_is_5() {
        let out = write(&[take_main42()], &[]).unwrap();
        // e_shnum at [60..62]
        let e_shnum = u16::from_le_bytes([out[60], out[61]]);
        assert_eq!(e_shnum, 5); // null + .text + .symtab + .strtab + .shstrtab
    }

    #[test]
    fn elf_section_count_with_relocs_is_6() {
        let r = X64Reloc {
            offset: 1,
            target_index: 2, // imports[0]
            kind: X64RelocKind::NearCall,
            addend: -4,
        };
        let f = X64Func::new("caller", vec![0xE8, 0, 0, 0, 0, 0xC3], vec![r], true).unwrap();
        let imp = X64Symbol::new_function("__cssl_callee").unwrap();
        let out = write(&[f], &[imp]).unwrap();
        let e_shnum = u16::from_le_bytes([out[60], out[61]]);
        assert_eq!(e_shnum, 6); // + .rela.text
    }

    #[test]
    fn elf_text_bytes_present() {
        let out = write(&[take_main42()], &[]).unwrap();
        // The .text body lies somewhere after the 64-byte header. Search
        // for the canonical `B8 2A 00 00 00 C3` signature.
        let body_start = (64..out.len() - 6)
            .find(|&i| &out[i..i + 6] == MAIN_42)
            .expect("`mov eax, 42 ; ret` body should appear in the .text section");
        assert!(body_start >= 64);
    }

    #[test]
    fn elf_two_funcs_both_emitted() {
        let a = X64Func::leaf_export("foo", vec![0x90, 0xC3]).unwrap();
        let b = X64Func::leaf_export("bar", MAIN_42.to_vec()).unwrap();
        let out = write(&[a, b], &[]).unwrap();
        // Both bodies should appear (`90 C3` and `B8 2A 00 00 00 C3`).
        let has_foo = out.windows(2).any(|w| w == [0x90, 0xC3]);
        let has_bar = out.windows(6).any(|w| w == MAIN_42);
        assert!(has_foo);
        assert!(has_bar);
    }
}

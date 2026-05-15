//! Post-codegen object-symbol audit for the WinMain-class dead-code risk.

use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget, SectionIndex, SymbolIndex};
use thiserror::Error;

const ENTRY_WRAPPER_SYMBOLS: &[&str] = &["WinMain", "mainCRTStartup", "_main", "_start"];
const DEFAULT_USER_MAIN_SYMBOLS: &[&str] = &["__cssl_user_main", "user_main"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolAuditReport {
    pub object_format: String,
    pub symbols_walked: usize,
    pub user_main_symbols: Vec<String>,
    pub entry_wrapper_symbols: Vec<String>,
    pub user_main_relocations: usize,
}

impl SymbolAuditReport {
    pub fn saw_user_main(&self) -> bool {
        !self.user_main_symbols.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum SymbolAuditError {
    #[error("object-symbol audit failed to parse emitted object: {0}")]
    Parse(#[from] object::Error),

    #[error("{entry} does not tail-call user-main · WinMain-class dead-code risk")]
    EntryWrapperDoesNotReachUserMain { entry: String },
}

#[derive(Debug, Clone)]
struct SymbolFact {
    index: SymbolIndex,
    name: String,
    section: Option<SectionIndex>,
    offset: u64,
    size: u64,
}

impl SymbolFact {
    fn covers_relocation(&self, section: SectionIndex, relocation_offset: u64) -> bool {
        if self.section != Some(section) {
            return false;
        }
        if self.size == 0 {
            return true;
        }
        let end = self.offset.saturating_add(self.size);
        (self.offset..end).contains(&relocation_offset)
    }
}

pub fn audit_object_for_user_main(bytes: &[u8]) -> Result<SymbolAuditReport, SymbolAuditError> {
    audit_object_for_user_symbols(bytes, DEFAULT_USER_MAIN_SYMBOLS)
}

pub fn audit_object_for_user_symbols(
    bytes: &[u8],
    user_main_symbols: &[&str],
) -> Result<SymbolAuditReport, SymbolAuditError> {
    let file = object::File::parse(bytes)?;
    let mut facts = Vec::new();
    let mut user_indices = Vec::new();
    let mut user_names = Vec::new();
    let mut entry_facts = Vec::new();

    for symbol in file.symbols() {
        let Ok(name) = symbol.name() else {
            continue;
        };
        let fact = SymbolFact {
            index: symbol.index(),
            name: name.to_owned(),
            section: symbol.section().index(),
            offset: symbol_offset_in_section(&file, &symbol),
            size: symbol.size(),
        };
        if symbol_matches_any(name, user_main_symbols) {
            user_indices.push(fact.index);
            user_names.push(fact.name.clone());
        }
        if symbol_matches_any(name, ENTRY_WRAPPER_SYMBOLS) {
            entry_facts.push(fact.clone());
        }
        facts.push(fact);
    }

    let mut entry_reaches_user = vec![false; entry_facts.len()];
    let mut user_main_relocations = 0usize;
    for section in file.sections() {
        let section_index = section.index();
        for (relocation_offset, relocation) in section.relocations() {
            let RelocationTarget::Symbol(target) = relocation.target() else {
                continue;
            };
            if !user_indices.contains(&target) {
                continue;
            }
            for (entry_index, entry) in entry_facts.iter().enumerate() {
                if entry.covers_relocation(section_index, relocation_offset) {
                    entry_reaches_user[entry_index] = true;
                    user_main_relocations += 1;
                }
            }
        }
    }

    for (entry, reaches_user) in entry_facts.iter().zip(entry_reaches_user) {
        if !reaches_user {
            return Err(SymbolAuditError::EntryWrapperDoesNotReachUserMain {
                entry: entry.name.clone(),
            });
        }
    }

    Ok(SymbolAuditReport {
        object_format: format!("{:?}", file.format()),
        symbols_walked: facts.len(),
        user_main_symbols: user_names,
        entry_wrapper_symbols: entry_facts.into_iter().map(|entry| entry.name).collect(),
        user_main_relocations,
    })
}

fn symbol_offset_in_section<'data, Symbol>(file: &object::File<'data>, symbol: &Symbol) -> u64
where
    Symbol: ObjectSymbol<'data>,
{
    let Some(section_index) = symbol.section().index() else {
        return symbol.address();
    };
    let Ok(section) = file.section_by_index(section_index) else {
        return symbol.address();
    };
    symbol.address().saturating_sub(section.address())
}

fn symbol_matches_any(name: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| symbol_matches(name, candidate))
}

fn symbol_matches(name: &str, candidate: &str) -> bool {
    name == candidate || name.trim_start_matches('_') == candidate.trim_start_matches('_')
}

#[cfg(test)]
mod tests {
    use super::{audit_object_for_user_main, SymbolAuditError};

    #[derive(Clone, Copy)]
    struct CoffSymbol<'a> {
        name: &'a str,
        section_number: i16,
    }

    impl<'a> CoffSymbol<'a> {
        const fn defined(name: &'a str) -> Self {
            Self {
                name,
                section_number: 1,
            }
        }

        const fn undefined(name: &'a str) -> Self {
            Self {
                name,
                section_number: 0,
            }
        }
    }

    #[test]
    fn a03_1_audit_walks_user_main_symbol() {
        let bytes = coff_object(&[CoffSymbol::defined("__cssl_user_main")], &[]);
        let report = audit_object_for_user_main(&bytes).expect("audit should pass");
        assert!(report.saw_user_main());
        assert_eq!(report.user_main_symbols, ["__cssl_user_main"]);
        assert_eq!(report.symbols_walked, 1);
    }

    #[test]
    fn a03_2_wrapper_with_user_main_relocation_passes() {
        let bytes = coff_object(
            &[
                CoffSymbol::defined("WinMain"),
                CoffSymbol::undefined("__cssl_user_main"),
            ],
            &[(1, 1)],
        );
        let report = audit_object_for_user_main(&bytes).expect("audit should pass");
        assert_eq!(report.entry_wrapper_symbols, ["WinMain"]);
        assert_eq!(report.user_main_relocations, 1);
    }

    #[test]
    fn a03_3_known_bad_winmain_returns_zero_fails() {
        let bytes = decode_hex(include_str!(
            "../tests/golden/a03_winmain_returns_zero.coff.hex"
        ));
        let err = audit_object_for_user_main(&bytes).unwrap_err();
        assert!(matches!(
            err,
            SymbolAuditError::EntryWrapperDoesNotReachUserMain { .. }
        ));
        assert_eq!(
            err.to_string(),
            "WinMain does not tail-call user-main · WinMain-class dead-code risk"
        );
    }

    fn coff_object(symbols: &[CoffSymbol<'_>], relocations: &[(u32, u32)]) -> Vec<u8> {
        let text = if relocations.is_empty() {
            &[0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3][..]
        } else {
            &[0xE9, 0x00, 0x00, 0x00, 0x00, 0xC3][..]
        };
        let raw_ptr = 20u32 + 40;
        let reloc_ptr = raw_ptr + u32::try_from(text.len()).unwrap();
        let sym_ptr = reloc_ptr + u32::try_from(relocations.len() * 10).unwrap();
        let mut out = Vec::new();

        push_u16(&mut out, 0x8664);
        push_u16(&mut out, 1);
        push_u32(&mut out, 0);
        push_u32(&mut out, sym_ptr);
        push_u32(&mut out, u32::try_from(symbols.len()).unwrap());
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);

        push_name_8(&mut out, ".text");
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, u32::try_from(text.len()).unwrap());
        push_u32(&mut out, raw_ptr);
        push_u32(&mut out, if relocations.is_empty() { 0 } else { reloc_ptr });
        push_u32(&mut out, 0);
        push_u16(&mut out, u16::try_from(relocations.len()).unwrap());
        push_u16(&mut out, 0);
        push_u32(&mut out, 0x6000_0020);

        out.extend_from_slice(text);
        for (offset, symbol_index) in relocations {
            push_u32(&mut out, *offset);
            push_u32(&mut out, *symbol_index);
            push_u16(&mut out, 0x0004);
        }

        let mut strings = vec![0, 0, 0, 0];
        for symbol in symbols {
            push_coff_symbol(&mut out, &mut strings, symbol);
        }
        let string_len = u32::try_from(strings.len()).unwrap().to_le_bytes();
        strings[..4].copy_from_slice(&string_len);
        out.extend_from_slice(&strings);
        out
    }

    fn push_coff_symbol(out: &mut Vec<u8>, strings: &mut Vec<u8>, symbol: &CoffSymbol<'_>) {
        if symbol.name.len() <= 8 {
            push_name_8(out, symbol.name);
        } else {
            push_u32(out, 0);
            push_u32(out, u32::try_from(strings.len()).unwrap());
            strings.extend_from_slice(symbol.name.as_bytes());
            strings.push(0);
        }
        push_u32(out, 0);
        push_i16(out, symbol.section_number);
        push_u16(out, 0x0020);
        out.push(2);
        out.push(0);
    }

    fn decode_hex(src: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut hi = None;
        for ch in src.chars().filter(char::is_ascii_hexdigit) {
            let nibble = ch.to_digit(16).unwrap() as u8;
            if let Some(first) = hi.take() {
                bytes.push((first << 4) | nibble);
            } else {
                hi = Some(nibble);
            }
        }
        assert!(
            hi.is_none(),
            "golden hex fixture must have even digit count"
        );
        bytes
    }

    fn push_name_8(out: &mut Vec<u8>, name: &str) {
        let mut bytes = [0u8; 8];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&bytes);
    }

    fn push_i16(out: &mut Vec<u8>, value: i16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

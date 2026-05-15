//! Effect-row runtime validation against emitted FFI symbols.

use std::collections::{BTreeMap, BTreeSet};

use cssl_mir::MirModule;
use thiserror::Error;

const IO_EFFECT: &str = "IO";
const IO_FFI_PREFIXES: &[&str] = &[
    "__cssl_io_",
    "__cssl_fs_",
    "__cssl_net_",
    "__cssl_window_",
    "__cssl_time_",
    "__cssl_audio_",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRuntimeWitness {
    pub fn_name: String,
    pub declared_effect_row: Option<String>,
    pub emitted_ffi_symbols: Vec<String>,
}

impl EffectRuntimeWitness {
    pub fn new(
        fn_name: impl Into<String>,
        declared_effect_row: Option<String>,
        emitted_ffi_symbols: Vec<String>,
    ) -> Self {
        Self {
            fn_name: fn_name.into(),
            declared_effect_row,
            emitted_ffi_symbols,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectRuntimeReport {
    pub functions_checked: usize,
    pub io_symbols_seen: usize,
}

#[derive(Debug, Error)]
pub enum EffectRuntimeError {
    #[error("effect-row mismatch: fn `{fn_name}` declares IO but emitted no IO FFI symbol")]
    MissingIoFfiSymbol { fn_name: String },

    #[error(
        "effect-row mismatch: fn `{fn_name}` declares {{}} but emitted IO FFI symbol `{symbol}`"
    )]
    PureFnEmitsIoFfiSymbol { fn_name: String, symbol: String },
}

pub fn effect_witnesses_from_module(
    module: &MirModule,
    emitted_symbols_by_fn: &BTreeMap<String, Vec<String>>,
) -> Vec<EffectRuntimeWitness> {
    module
        .funcs
        .iter()
        .filter(|func| !func.is_generic)
        .map(|func| {
            EffectRuntimeWitness::new(
                func.name.clone(),
                func.effect_row.clone(),
                emitted_symbols_by_fn
                    .get(&func.name)
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect()
}

pub fn verify_effect_row_witnesses(
    witnesses: &[EffectRuntimeWitness],
) -> Result<EffectRuntimeReport, EffectRuntimeError> {
    let mut io_symbols_seen = 0usize;

    for witness in witnesses {
        let effects = parse_effect_row(witness.declared_effect_row.as_deref());
        let declares_io = effects.contains(IO_EFFECT);
        let first_io_symbol = witness
            .emitted_ffi_symbols
            .iter()
            .find(|symbol| is_io_ffi_symbol(symbol));

        if declares_io && first_io_symbol.is_none() {
            return Err(EffectRuntimeError::MissingIoFfiSymbol {
                fn_name: witness.fn_name.clone(),
            });
        }
        if !declares_io {
            if let Some(symbol) = first_io_symbol {
                return Err(EffectRuntimeError::PureFnEmitsIoFfiSymbol {
                    fn_name: witness.fn_name.clone(),
                    symbol: symbol.clone(),
                });
            }
        }
        io_symbols_seen += witness
            .emitted_ffi_symbols
            .iter()
            .filter(|symbol| is_io_ffi_symbol(symbol))
            .count();
    }

    Ok(EffectRuntimeReport {
        functions_checked: witnesses.len(),
        io_symbols_seen,
    })
}

pub fn is_io_ffi_symbol(symbol: &str) -> bool {
    IO_FFI_PREFIXES
        .iter()
        .any(|prefix| symbol.starts_with(prefix))
}

fn parse_effect_row(row: Option<&str>) -> BTreeSet<String> {
    let Some(row) = row else {
        return BTreeSet::new();
    };
    let trimmed = row
        .trim()
        .trim_start_matches('/')
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    if trimmed.is_empty() {
        return BTreeSet::new();
    }
    trimmed
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        effect_witnesses_from_module, is_io_ffi_symbol, parse_effect_row,
        verify_effect_row_witnesses, EffectRuntimeError, EffectRuntimeWitness,
    };
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirType};

    #[test]
    fn a06_1_cross_checks_declared_effect_rows_against_emitted_ffi_symbols() {
        let witnesses = vec![
            EffectRuntimeWitness::new(
                "read_file",
                Some("{IO}".to_string()),
                vec!["__cssl_fs_read".to_string()],
            ),
            EffectRuntimeWitness::new("pure_math", None, Vec::new()),
        ];
        let report = verify_effect_row_witnesses(&witnesses).expect("effect check passes");
        assert_eq!(report.functions_checked, 2);
        assert_eq!(report.io_symbols_seen, 1);
    }

    #[test]
    fn a06_2_declared_io_with_no_io_symbol_fails_link_preflight() {
        let witnesses = [EffectRuntimeWitness::new(
            "declares_io",
            Some("/{IO}".to_string()),
            Vec::new(),
        )];
        let err = verify_effect_row_witnesses(&witnesses).unwrap_err();
        assert!(matches!(err, EffectRuntimeError::MissingIoFfiSymbol { .. }));
        assert_eq!(
            err.to_string(),
            "effect-row mismatch: fn `declares_io` declares IO but emitted no IO FFI symbol"
        );
    }

    #[test]
    fn a06_3_pure_fn_with_io_symbol_fails_link_preflight() {
        let witnesses = [EffectRuntimeWitness::new(
            "pure_fn",
            Some("{}".to_string()),
            vec!["__cssl_io_write".to_string()],
        )];
        let err = verify_effect_row_witnesses(&witnesses).unwrap_err();
        assert!(matches!(
            err,
            EffectRuntimeError::PureFnEmitsIoFfiSymbol { .. }
        ));
        assert_eq!(
            err.to_string(),
            "effect-row mismatch: fn `pure_fn` declares {} but emitted IO FFI symbol `__cssl_io_write`"
        );
    }

    #[test]
    fn effect_witnesses_thread_module_rows_and_symbol_map() {
        let mut module = MirModule::new();
        let mut func = MirFunc::new("read_file", vec![], vec![MirType::Int(IntWidth::I32)]);
        func.effect_row = Some("{IO}".to_string());
        module.push_func(func);
        let mut emitted = BTreeMap::new();
        emitted.insert("read_file".to_string(), vec!["__cssl_fs_open".to_string()]);

        let witnesses = effect_witnesses_from_module(&module, &emitted);
        assert_eq!(witnesses.len(), 1);
        assert_eq!(witnesses[0].declared_effect_row.as_deref(), Some("{IO}"));
        assert_eq!(witnesses[0].emitted_ffi_symbols, ["__cssl_fs_open"]);
    }

    #[test]
    fn io_symbol_prefixes_cover_stage0_host_domains() {
        assert!(is_io_ffi_symbol("__cssl_io_write"));
        assert!(is_io_ffi_symbol("__cssl_fs_read"));
        assert!(is_io_ffi_symbol("__cssl_net_send"));
        assert!(!is_io_ffi_symbol("__cssl_alloc"));
    }

    #[test]
    fn effect_row_parser_accepts_slash_brace_and_plain_forms() {
        assert!(parse_effect_row(None).is_empty());
        assert!(parse_effect_row(Some("/{}")).is_empty());
        assert!(parse_effect_row(Some("{IO, GPU}")).contains("IO"));
        assert!(parse_effect_row(Some("IO GPU")).contains("GPU"));
    }
}

//! MLIR textual-format emission helpers.
//!
//! Wraps `cssl_mir::print_module` so that the compiler driver has a stable emission
//! API regardless of whether the melior FFI path is active.

use std::io::{self, Write};

use cssl_mir::MirModule;

/// Emit a `MirModule` as an MLIR textual-format string.
#[must_use]
pub fn emit_module_to_string(module: &MirModule) -> String {
    cssl_mir::print_module(module)
}

/// Emit a `MirModule` to any `io::Write` sink (e.g., `File`, `stdout`, `Vec<u8>`).
pub fn emit_module_to_writer<W: Write>(module: &MirModule, w: &mut W) -> io::Result<()> {
    let s = emit_module_to_string(module);
    w.write_all(s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{emit_module_to_string, emit_module_to_writer};
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirType};

    fn sample() -> MirModule {
        let mut m = MirModule::with_name("sample");
        let f = MirFunc::new(
            "add",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        m.push_func(f);
        m
    }

    #[test]
    fn emit_to_string_produces_valid_text() {
        let m = sample();
        let s = emit_module_to_string(&m);
        assert!(s.contains("module @sample"));
        assert!(s.contains("func.func @add"));
    }

    #[test]
    fn emit_to_writer_matches_string() {
        let m = sample();
        let mut buf: Vec<u8> = Vec::new();
        emit_module_to_writer(&m, &mut buf).unwrap();
        let from_string = emit_module_to_string(&m);
        assert_eq!(String::from_utf8(buf).unwrap(), from_string);
    }

    #[test]
    fn emit_empty_module() {
        let m = MirModule::new();
        let s = emit_module_to_string(&m);
        assert!(s.starts_with("module"));
    }
}

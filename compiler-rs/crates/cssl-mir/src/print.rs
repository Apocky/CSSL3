//! MLIR textual-format pretty-printer.
//!
//! § FORMAT (subset sufficient for `--emit-mlir` dumps)
//!
//! ```text
//!   module @name {
//!     func.func @fnname(%arg0: i32, %arg1: i32) -> i32 attributes { … } {
//!       %0 = cssl.handle.pack %arg0, %arg1 : (i32, i32) -> !cssl.handle
//!       %1 = arith.constant 42 : i32
//!       cssl.telemetry.probe { scope = "Counters" }
//!       func.return %1 : i32
//!     }
//!   }
//! ```
//!
//! § STAGE-0
//!   Pretty-print is linear O(N) in IR size ; no structured-indent optimization. Output
//!   is valid MLIR textual format for the subset of ops we emit. Full round-trip parity
//!   with `mlir-opt` is T6-phase-2 (requires melior / CLI validator in CI).

use core::fmt::Write;

use crate::block::{MirBlock, MirOp, MirRegion};
use crate::func::{MirFunc, MirModule};

/// Pretty-printer state : accumulates output into a `String`.
#[derive(Debug, Default)]
pub struct MlirPrinter {
    pub out: String,
    indent: usize,
}

impl MlirPrinter {
    /// Build an empty printer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume and return the accumulated output.
    #[must_use]
    pub fn into_string(self) -> String {
        self.out
    }

    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push(' ');
        }
    }

    fn nl(&mut self) {
        self.out.push('\n');
    }

    fn write_module(&mut self, module: &MirModule) {
        self.push_indent();
        match &module.name {
            Some(n) => {
                let _ = write!(self.out, "module @{n}");
            }
            None => self.out.push_str("module"),
        }
        self.out.push_str(" {");
        self.nl();
        self.indent += 2;
        for f in &module.funcs {
            self.write_func(f);
        }
        self.indent = self.indent.saturating_sub(2);
        self.push_indent();
        self.out.push('}');
        self.nl();
    }

    fn write_func(&mut self, f: &MirFunc) {
        self.push_indent();
        let _ = write!(self.out, "func.func @{}(", f.name);
        if let Some(entry) = f.body.entry() {
            for (i, arg) in entry.args.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                let _ = write!(self.out, "{}: {}", arg.id, arg.ty);
            }
        }
        self.out.push(')');
        if !f.results.is_empty() {
            self.out.push_str(" -> ");
            if f.results.len() == 1 {
                let _ = write!(self.out, "{}", f.results[0]);
            } else {
                self.out.push('(');
                for (i, r) in f.results.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    let _ = write!(self.out, "{r}");
                }
                self.out.push(')');
            }
        }
        // Attribute block.
        if f.effect_row.is_some()
            || f.cap.is_some()
            || f.ifc_label.is_some()
            || !f.attributes.is_empty()
        {
            self.out.push_str(" attributes {");
            let mut first = true;
            let sep = |this: &mut Self, first: &mut bool| {
                if *first {
                    *first = false;
                } else {
                    this.out.push_str(", ");
                }
            };
            if let Some(er) = &f.effect_row {
                sep(self, &mut first);
                let _ = write!(self.out, "effect_row = \"{er}\"");
            }
            if let Some(cap) = &f.cap {
                sep(self, &mut first);
                let _ = write!(self.out, "cap = \"{cap}\"");
            }
            if let Some(ifc) = &f.ifc_label {
                sep(self, &mut first);
                let _ = write!(self.out, "ifc_label = \"{ifc}\"");
            }
            for (k, v) in &f.attributes {
                sep(self, &mut first);
                let _ = write!(self.out, "{k} = \"{v}\"");
            }
            self.out.push('}');
        }
        self.out.push_str(" {");
        self.nl();
        self.indent += 2;
        self.write_region(&f.body, /*skip_entry_args=*/ true);
        self.indent = self.indent.saturating_sub(2);
        self.push_indent();
        self.out.push('}');
        self.nl();
    }

    fn write_region(&mut self, region: &MirRegion, skip_entry_args: bool) {
        for (i, block) in region.blocks.iter().enumerate() {
            // The fn-level entry block's args were already printed in the `func.func`
            // header ; nested regions print their own entry-block args.
            if i > 0 || !skip_entry_args {
                self.write_block_header(block);
            }
            for op in &block.ops {
                self.write_op(op);
            }
        }
    }

    fn write_block_header(&mut self, block: &MirBlock) {
        self.push_indent();
        let _ = write!(self.out, "^{}", block.label);
        if !block.args.is_empty() {
            self.out.push('(');
            for (i, a) in block.args.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                let _ = write!(self.out, "{}: {}", a.id, a.ty);
            }
            self.out.push(')');
        }
        self.out.push(':');
        self.nl();
    }

    fn write_op(&mut self, op: &MirOp) {
        self.push_indent();
        // Results : `%a, %b = ` prefix.
        if !op.results.is_empty() {
            for (i, r) in op.results.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                let _ = write!(self.out, "{}", r.id);
            }
            self.out.push_str(" = ");
        }
        // Op name.
        self.out.push_str(&op.name);
        // Operands.
        if !op.operands.is_empty() {
            self.out.push(' ');
            for (i, v) in op.operands.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                let _ = write!(self.out, "{v}");
            }
        }
        // Attribute dict.
        if !op.attributes.is_empty() {
            self.out.push_str(" { ");
            for (i, (k, v)) in op.attributes.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                let _ = write!(self.out, "{k} = \"{v}\"");
            }
            self.out.push_str(" }");
        }
        // Nested regions.
        if !op.regions.is_empty() {
            self.out.push_str(" ({");
            self.nl();
            self.indent += 2;
            for region in &op.regions {
                self.write_region(region, /*skip_entry_args=*/ false);
            }
            self.indent = self.indent.saturating_sub(2);
            self.push_indent();
            self.out.push_str("})");
        }
        // Type annotation : `(operand-types) -> result-types`.
        if !op.operands.is_empty() || !op.results.is_empty() {
            self.out.push_str(" : (");
            // Operand types are not tracked in operand-only form ; printer records
            // result-types and an indeterminate operand-type list. This is a stage-0
            // simplification ; full type-elaboration lands with the lowering walk.
            self.out.push(')');
            self.out.push_str(" -> ");
            if op.results.len() == 1 {
                let _ = write!(self.out, "{}", op.results[0].ty);
            } else if op.results.is_empty() {
                self.out.push_str("()");
            } else {
                self.out.push('(');
                for (i, r) in op.results.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    let _ = write!(self.out, "{}", r.ty);
                }
                self.out.push(')');
            }
        }
        self.nl();
    }
}

/// Convenience entry : pretty-print a module to a `String`.
#[must_use]
pub fn print_module(module: &MirModule) -> String {
    let mut p = MlirPrinter::new();
    p.write_module(module);
    p.into_string()
}

#[cfg(test)]
mod tests {
    use super::print_module;
    use crate::block::MirOp;
    use crate::func::{MirFunc, MirModule};
    use crate::op::CsslOp;
    use crate::value::{IntWidth, MirType, ValueId};

    #[test]
    fn print_empty_module() {
        let m = MirModule::new();
        let s = print_module(&m);
        assert!(s.contains("module"));
        assert!(s.contains('{'));
        assert!(s.contains('}'));
    }

    #[test]
    fn print_named_module() {
        let m = MirModule::with_name("com.apocky.loa");
        let s = print_module(&m);
        assert!(s.contains("module @com.apocky.loa"));
    }

    #[test]
    fn print_fn_signature() {
        let mut m = MirModule::new();
        let f = MirFunc::new(
            "add",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        m.push_func(f);
        let s = print_module(&m);
        assert!(s.contains("func.func @add"));
        assert!(s.contains("%0: i32"));
        assert!(s.contains("%1: i32"));
        assert!(s.contains("-> i32"));
    }

    #[test]
    fn print_fn_with_ops() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("f", vec![MirType::Int(IntWidth::I32)], vec![]);
        let op = MirOp::new(CsslOp::TelemetryProbe).with_attribute("scope", "Counters");
        f.push_op(op);
        m.push_func(f);
        let s = print_module(&m);
        assert!(s.contains("cssl.telemetry.probe"));
        assert!(s.contains("scope = \"Counters\""));
    }

    #[test]
    fn print_fn_with_effect_row_attribute() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("f", vec![], vec![]);
        f.effect_row = Some("{GPU, NoAlloc}".into());
        f.cap = Some("val".into());
        m.push_func(f);
        let s = print_module(&m);
        assert!(s.contains("attributes"));
        assert!(s.contains("effect_row = \"{GPU, NoAlloc}\""));
        assert!(s.contains("cap = \"val\""));
    }

    #[test]
    fn print_op_with_results_and_operands() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new(
            "g",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Handle],
        );
        let id = f.fresh_value_id();
        let op = MirOp::new(CsslOp::HandlePack)
            .with_operand(ValueId(0))
            .with_result(id, MirType::Handle);
        f.push_op(op);
        m.push_func(f);
        let s = print_module(&m);
        assert!(s.contains("cssl.handle.pack"));
        assert!(s.contains("!cssl.handle"));
    }
}

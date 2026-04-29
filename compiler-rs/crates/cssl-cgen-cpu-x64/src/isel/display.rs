//! § display — text-format pretty-printer for [`X64Func`].
//!
//! § ROLE
//!   Round-trip-displayable text format for golden-file regression tests.
//!   Tests build a known MIR fn, run it through [`crate::isel::select::select_function`],
//!   format the result via [`format_func`], and diff against an expected
//!   string. This catches per-op selection regressions immediately at
//!   `cargo test` time.
//!
//! § FORMAT
//!   ```text
//!   fn <name> (i32, i32) -> i32 {
//!     bb0:
//!       v3:i32 <- mov v1:i32
//!       v3:i32 <- add v3:i32, v2:i32
//!       ret v3:i32
//!   }
//!   ```
//!
//!   Conventions :
//!     - Block labels use the canonical [`crate::isel::inst::BlockId`] display form (`bb0`).
//!     - Each inst is on its own indented line.
//!     - Result-producing insts prefix with `<dst> <- ` ; void insts (cmp,
//!       cdq, store) lead with the mnemonic.
//!     - Terminator is the last line of the block.
//!     - Trailing newline after the closing brace.

use super::func::X64Func;
use super::inst::{X64Inst, X64Term};

/// Format an [`X64Func`] as a multi-line text string.
#[must_use]
pub fn format_func(f: &X64Func) -> String {
    let mut s = String::new();
    s.push_str("fn ");
    s.push_str(&f.name);
    s.push_str(" (");
    for (i, w) in f.sig.params.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(w.as_str());
    }
    s.push(')');
    if !f.sig.results.is_empty() {
        s.push_str(" -> ");
        if f.sig.results.len() == 1 {
            s.push_str(f.sig.results[0].as_str());
        } else {
            s.push('(');
            for (i, w) in f.sig.results.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(w.as_str());
            }
            s.push(')');
        }
    }
    s.push_str(" {\n");
    for block in &f.blocks {
        s.push_str("  ");
        s.push_str(&block.id.to_string());
        s.push_str(":\n");
        for inst in &block.insts {
            s.push_str("    ");
            format_inst(inst, &mut s);
            s.push('\n');
        }
        s.push_str("    ");
        format_term(&block.terminator, &mut s);
        s.push('\n');
    }
    s.push_str("}\n");
    s
}

fn format_inst(inst: &X64Inst, s: &mut String) {
    use core::fmt::Write;
    match inst {
        X64Inst::Mov { dst, src } => write!(s, "{dst} <- mov {src}").unwrap(),
        X64Inst::MovImm { dst, imm } => write!(s, "{dst} <- mov.imm {imm}").unwrap(),
        X64Inst::Add { dst, src } => write!(s, "{dst} <- add {dst}, {src}").unwrap(),
        X64Inst::Sub { dst, src } => write!(s, "{dst} <- sub {dst}, {src}").unwrap(),
        X64Inst::IMul { dst, src } => write!(s, "{dst} <- imul {dst}, {src}").unwrap(),
        X64Inst::Cdq => s.push_str("cdq"),
        X64Inst::Cqo => s.push_str("cqo"),
        X64Inst::Idiv { divisor } => write!(s, "idiv {divisor}").unwrap(),
        X64Inst::Div { divisor } => write!(s, "div {divisor}").unwrap(),
        X64Inst::XorRdx { width } => write!(s, "xor.rdx {width}").unwrap(),
        X64Inst::And { dst, src } => write!(s, "{dst} <- and {dst}, {src}").unwrap(),
        X64Inst::Or { dst, src } => write!(s, "{dst} <- or {dst}, {src}").unwrap(),
        X64Inst::Xor { dst, src } => write!(s, "{dst} <- xor {dst}, {src}").unwrap(),
        X64Inst::Shl { dst, src } => write!(s, "{dst} <- shl {dst}, {src}").unwrap(),
        X64Inst::Shr { dst, src } => write!(s, "{dst} <- shr {dst}, {src}").unwrap(),
        X64Inst::Sar { dst, src } => write!(s, "{dst} <- sar {dst}, {src}").unwrap(),
        X64Inst::Neg { dst } => write!(s, "{dst} <- neg {dst}").unwrap(),
        X64Inst::Not { dst } => write!(s, "{dst} <- not {dst}").unwrap(),
        X64Inst::FpAdd { dst, src } => write!(s, "{dst} <- fadd {dst}, {src}").unwrap(),
        X64Inst::FpSub { dst, src } => write!(s, "{dst} <- fsub {dst}, {src}").unwrap(),
        X64Inst::FpMul { dst, src } => write!(s, "{dst} <- fmul {dst}, {src}").unwrap(),
        X64Inst::FpDiv { dst, src } => write!(s, "{dst} <- fdiv {dst}, {src}").unwrap(),
        X64Inst::FpNeg { dst, width } => write!(s, "{dst} <- fneg.{width} {dst}").unwrap(),
        X64Inst::Ucomi { lhs, rhs } => write!(s, "ucomi {lhs}, {rhs}").unwrap(),
        X64Inst::Comi { lhs, rhs } => write!(s, "comi {lhs}, {rhs}").unwrap(),
        X64Inst::Cmp { lhs, rhs } => write!(s, "cmp {lhs}, {rhs}").unwrap(),
        X64Inst::Setcc { dst, cond_kind } => write!(s, "{dst} <- {cond_kind}").unwrap(),
        X64Inst::Movzx { dst, src } => write!(s, "{dst} <- movzx {src}").unwrap(),
        X64Inst::Movsx { dst, src } => write!(s, "{dst} <- movsx {src}").unwrap(),
        X64Inst::Cmov {
            dst,
            src,
            cond_kind,
        } => write!(s, "{dst} <- cmov {cond_kind} {src}").unwrap(),
        X64Inst::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => write!(s, "{dst} <- select {cond} ? {if_true} : {if_false}").unwrap(),
        X64Inst::Test { src } => write!(s, "test {src}, {src}").unwrap(),
        X64Inst::Load { dst, addr } => write!(s, "{dst} <- load {addr}").unwrap(),
        X64Inst::Store { src, addr } => write!(s, "store {addr}, {src}").unwrap(),
        X64Inst::Lea { dst, addr } => write!(s, "{dst} <- lea {addr}").unwrap(),
        X64Inst::Call {
            callee,
            args,
            results,
        } => {
            if let Some((first, rest)) = results.split_first() {
                write!(s, "{first}").unwrap();
                for r in rest {
                    write!(s, ", {r}").unwrap();
                }
                s.push_str(" <- ");
            }
            write!(s, "call {callee}(").unwrap();
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                write!(s, "{a}").unwrap();
            }
            s.push(')');
        }
        X64Inst::Push { src } => write!(s, "push {src}").unwrap(),
        X64Inst::Pop { dst } => write!(s, "{dst} <- pop").unwrap(),
    }
}

fn format_term(t: &X64Term, s: &mut String) {
    use core::fmt::Write;
    write!(s, "{t}").unwrap();
}

#[cfg(test)]
mod tests {
    use super::super::func::{X64Func, X64Signature};
    use super::super::inst::{X64Imm, X64Inst, X64Term};
    use super::super::vreg::X64Width;
    use super::format_func;

    #[test]
    fn format_empty_func_void_void() {
        let mut f = X64Func::new("noop", X64Signature::empty());
        f.set_terminator(
            crate::isel::inst::BlockId::ENTRY,
            X64Term::Ret { operands: vec![] },
        );
        let s = format_func(&f);
        // Header line.
        assert!(s.starts_with("fn noop ()"));
        // Single block + terminator.
        assert!(s.contains("bb0:"));
        assert!(s.contains("ret"));
        assert!(s.ends_with("}\n"));
    }

    #[test]
    fn format_signature_with_result() {
        let mut f = X64Func::new("answer", X64Signature::new(vec![], vec![X64Width::I32]));
        let v = f.fresh_vreg(X64Width::I32);
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::MovImm {
                dst: v,
                imm: X64Imm::I32(42),
            },
        );
        f.set_terminator(
            crate::isel::inst::BlockId::ENTRY,
            X64Term::Ret { operands: vec![v] },
        );
        let s = format_func(&f);
        assert!(s.starts_with("fn answer ()"));
        assert!(s.contains("-> i32"));
        assert!(s.contains("mov.imm 42i32"));
        assert!(s.contains("ret v1:i32"));
    }

    #[test]
    fn format_signature_multi_result() {
        let f = X64Func::new(
            "twoback",
            X64Signature::new(vec![], vec![X64Width::I32, X64Width::I64]),
        );
        let s = format_func(&f);
        assert!(s.contains("-> (i32, i64)"));
    }

    #[test]
    fn format_inst_arithmetic_three_address() {
        let mut f = X64Func::new(
            "add",
            X64Signature::new(vec![X64Width::I32, X64Width::I32], vec![X64Width::I32]),
        );
        let p0 = f.param_vreg(0);
        let p1 = f.param_vreg(1);
        let dst = f.fresh_vreg(X64Width::I32);
        // dst <- mov p0 ; dst <- add dst, p1
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::Mov { dst, src: p0 },
        );
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::Add { dst, src: p1 },
        );
        f.set_terminator(
            crate::isel::inst::BlockId::ENTRY,
            X64Term::Ret {
                operands: vec![dst],
            },
        );
        let s = format_func(&f);
        assert!(s.contains("v3:i32 <- mov v1:i32"));
        assert!(s.contains("v3:i32 <- add v3:i32, v2:i32"));
        assert!(s.contains("ret v3:i32"));
    }

    #[test]
    fn format_call_with_args_and_result() {
        let mut f = X64Func::new("caller", X64Signature::empty());
        let arg = f.fresh_vreg(X64Width::I32);
        let ret = f.fresh_vreg(X64Width::I32);
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::Call {
                callee: "callee".to_string(),
                args: vec![arg],
                results: vec![ret],
            },
        );
        f.set_terminator(
            crate::isel::inst::BlockId::ENTRY,
            X64Term::Ret { operands: vec![] },
        );
        let s = format_func(&f);
        assert!(s.contains("v2:i32 <- call callee(v1:i32)"));
    }

    #[test]
    fn format_call_void_no_result_arrow() {
        let mut f = X64Func::new("caller", X64Signature::empty());
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::Call {
                callee: "void_callee".to_string(),
                args: vec![],
                results: vec![],
            },
        );
        f.set_terminator(
            crate::isel::inst::BlockId::ENTRY,
            X64Term::Ret { operands: vec![] },
        );
        let s = format_func(&f);
        // No `<- ` since no results.
        assert!(s.contains("call void_callee()"));
        assert!(!s.contains("<- call void_callee"));
    }

    #[test]
    fn format_loads_and_stores() {
        let mut f = X64Func::new("memops", X64Signature::empty());
        let ptr = f.fresh_vreg(X64Width::Ptr);
        let v = f.fresh_vreg(X64Width::I32);
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::Load {
                dst: v,
                addr: crate::isel::inst::MemAddr::base(ptr),
            },
        );
        f.push_inst(
            crate::isel::inst::BlockId::ENTRY,
            X64Inst::Store {
                src: v,
                addr: crate::isel::inst::MemAddr::base(ptr),
            },
        );
        f.set_terminator(
            crate::isel::inst::BlockId::ENTRY,
            X64Term::Ret { operands: vec![] },
        );
        let s = format_func(&f);
        assert!(s.contains("v2:i32 <- load [v1:ptr]"));
        assert!(s.contains("store [v1:ptr], v2:i32"));
    }
}

//! Skeletal HLSL source builder.
//!
//! § ROLE — S6-D2 (T11-D73)
//!   Phase-1 (T10) emitted HLSL with empty fn-bodies + a few top-level
//!   declarations (struct / cbuffer / RWStructuredBuffer / fn-decl).
//!   Phase-2 / S6-D2 grows the IR so MIR ops can lower op-by-op into HLSL :
//!     - typed local-var declarations (`int x = ...;`)
//!     - assignments (`y = expr;`)
//!     - return statements (`return v;`)
//!     - structured `if (cond) { then } else { else }` blocks
//!     - structured `for / while / loop` (do-while-true) constructs
//!     - block statements (curly-brace scope)
//!     - HLSL expressions : variable refs, integer / float literals, binary
//!       operators, unary negate, comparison predicates, ternary-select,
//!       address-of-buffer-element, function-call (intrinsic or user fn).
//!
//!   Each variant is render-only ; the emitter (`emit.rs`) is responsible
//!   for choosing the right HLSL types + names. We render expressions
//!   parenthesized when they're sub-expressions to keep precedence
//!   unambiguous — DXC parses standard HLSL precedence, so this is purely
//!   a defensive choice that costs one byte per nested op + makes diffs
//!   stable across emitter changes.

use core::fmt::Write as _;

/// One HLSL expression. Rendered without trailing `;` ; statements own
/// their own semicolon. All binary forms render with explicit parentheses
/// around the operand pair so HLSL operator-precedence is unambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlslExpr {
    /// Reference to a named variable (`v3`, `tid`, `g_buf`).
    Var(String),
    /// Integer literal — already rendered as an HLSL-source string
    /// (e.g. `"42"` or `"-1"`). Width inferred by type-context.
    IntLit(String),
    /// Unsigned-integer literal — rendered with the `u` suffix HLSL uses
    /// for unsigned constants (e.g. `"42u"`).
    UintLit(String),
    /// Floating literal — already rendered as an HLSL-source string
    /// (e.g. `"3.14"` or `"-0.5"`). The `f` suffix is added by the renderer
    /// when `is_f32` is true to keep DXC happy on SM6.0+ where literals
    /// default to `float` already (the suffix is redundant but harmless +
    /// clarifies intent in generated source).
    FloatLit { text: String, is_f32: bool },
    /// Boolean literal (`true` / `false`).
    BoolLit(bool),
    /// Binary operator on two sub-expressions (`(lhs op rhs)`).
    Binary {
        op: HlslBinaryOp,
        lhs: Box<HlslExpr>,
        rhs: Box<HlslExpr>,
    },
    /// Unary operator on one sub-expression (`(op rhs)`).
    Unary { op: HlslUnaryOp, rhs: Box<HlslExpr> },
    /// Ternary select — `(cond ? then : else)` — used by `arith.select`.
    Ternary {
        cond: Box<HlslExpr>,
        then_branch: Box<HlslExpr>,
        else_branch: Box<HlslExpr>,
    },
    /// Function call : `name(arg0, arg1, ...)`. Used both for intrinsics
    /// (`min` / `max` / `sqrt` / `abs` / ...) and user-defined helpers.
    Call { name: String, args: Vec<HlslExpr> },
    /// Buffer-element access : `buffer[index]` — produced for `memref.load`
    /// + `memref.store` lowerings backed by an `RWStructuredBuffer<T>`.
    /// At stage-0 we render buffer accesses through a synthetic
    /// `g_dyn_buf` global cast to `RWByteAddressBuffer` ; the emitter
    /// picks the actual buffer name + index expression.
    BufferLoad {
        buffer: String,
        index: Box<HlslExpr>,
    },
    /// Cast `(ty)expr`. Used to bridge the signless MIR integer model into
    /// HLSL's signed `int` / unsigned `uint` distinction at boundaries.
    Cast { ty: String, rhs: Box<HlslExpr> },
    /// Raw pre-rendered HLSL expression. Escape hatch for intrinsics the
    /// table doesn't yet model (e.g. `WaveActiveSum`) ; emitter is
    /// responsible for proper parenthesization.
    Raw(String),
}

/// Binary HLSL operator. The discriminant maps 1:1 to MLIR `arith.*` ops
/// the emitter recognizes — kept small + closed so the renderer can be
/// exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlslBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    LogicAnd,
    LogicOr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl HlslBinaryOp {
    /// HLSL source-form spelling (`+`, `-`, `*`, `==`, `<`, ...).
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::BitAnd => "&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::Shl => "<<",
            Self::Shr => ">>",
            Self::LogicAnd => "&&",
            Self::LogicOr => "||",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

/// Unary HLSL operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlslUnaryOp {
    /// Arithmetic negate (`-x`).
    Neg,
    /// Logical not (`!x`).
    LogicNot,
    /// Bitwise complement (`~x`).
    BitNot,
}

impl HlslUnaryOp {
    /// HLSL source-form spelling.
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Neg => "-",
            Self::LogicNot => "!",
            Self::BitNot => "~",
        }
    }
}

impl HlslExpr {
    /// Render the expression as an HLSL source string.
    ///
    /// Sub-expressions are wrapped in `(...)` to keep DXC operator-precedence
    /// unambiguous regardless of nesting context.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Var(name) => name.clone(),
            Self::IntLit(text) => text.clone(),
            Self::UintLit(text) => format!("{text}u"),
            Self::FloatLit { text, is_f32 } => {
                if *is_f32 {
                    format!("{text}f")
                } else {
                    text.clone()
                }
            }
            Self::BoolLit(b) => if *b { "true" } else { "false" }.into(),
            Self::Binary { op, lhs, rhs } => {
                format!("({} {} {})", lhs.render(), op.spelling(), rhs.render())
            }
            Self::Unary { op, rhs } => format!("({}{})", op.spelling(), rhs.render()),
            Self::Ternary {
                cond,
                then_branch,
                else_branch,
            } => format!(
                "({} ? {} : {})",
                cond.render(),
                then_branch.render(),
                else_branch.render(),
            ),
            Self::Call { name, args } => {
                let rendered: Vec<String> = args.iter().map(Self::render).collect();
                format!("{name}({})", rendered.join(", "))
            }
            Self::BufferLoad { buffer, index } => format!("{buffer}[{}]", index.render()),
            Self::Cast { ty, rhs } => format!("(({ty}){})", rhs.render()),
            Self::Raw(s) => s.clone(),
        }
    }
}

/// One HLSL top-level statement (struct / cbuffer / fn-decl / raw pass-through).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlslStatement {
    /// `cbuffer Name : register(b0) { ... }` — represented as raw body text.
    CBuffer {
        name: String,
        body: String,
        register: Option<String>,
    },
    /// `struct Name { field1; field2; ... };`.
    Struct { name: String, fields: Vec<String> },
    /// `RWStructuredBuffer<T> Name : register(u0);`.
    RwBuffer {
        element_type: String,
        name: String,
        register: Option<String>,
    },
    /// A fn declaration : `ReturnType Name(params) : semantic { body }`.
    Function {
        return_type: String,
        name: String,
        params: Vec<String>,
        attributes: Vec<String>,
        semantic: Option<String>,
        body: Vec<String>,
    },
    /// Raw pass-through line (caller-formatted).
    Raw(String),
}

/// One HLSL fn-body statement — rendered with one level of indent inside
/// the enclosing fn or block.
///
/// Statement kinds correspond directly to the MIR ops the emitter
/// recognizes : declarations (`int x = ...;`), assignments, returns,
/// structured if / loops, and raw escape lines. Every variant is
/// render-only ; the emitter is responsible for choosing names + types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlslBodyStmt {
    /// `<type> <name> = <expr>;` — one-shot typed declaration. Used for
    /// the canonical SSA-name lowering of `arith.constant`, `arith.add*`,
    /// `memref.load`, `func.call` results, etc.
    VarDecl {
        ty: String,
        name: String,
        init: HlslExpr,
    },
    /// `<lhs> = <rhs>;` — re-assignment to an existing variable. Stage-0
    /// MIR is SSA + every result has a fresh ValueId, so this is reserved
    /// for the loop-induction-variable / phi-merge writes the structured
    /// CFG lowering emits.
    Assign { lhs: String, rhs: HlslExpr },
    /// `return <expr>;` or `return;`.
    Return { value: Option<HlslExpr> },
    /// `if (cond) { then_body } else { else_body }`. Both branches own
    /// statement-vectors ; an empty else-vec elides the `else { }` clause.
    If {
        cond: HlslExpr,
        then_body: Vec<HlslBodyStmt>,
        else_body: Vec<HlslBodyStmt>,
    },
    /// `for (init; cond; step) { body }`. The MIR `scf.for` shape is
    /// stage-0 single-trip per the C2 deferred-bullets ; the emitter
    /// renders the structurally-correct `for` skeleton even when the
    /// init / cond / step expressions are minimal placeholders.
    For {
        init: Option<Box<HlslBodyStmt>>,
        cond: Option<HlslExpr>,
        step: Option<HlslExpr>,
        body: Vec<HlslBodyStmt>,
    },
    /// `while (cond) { body }`. Maps directly from `scf.while`.
    While {
        cond: HlslExpr,
        body: Vec<HlslBodyStmt>,
    },
    /// `do { body } while (true);` — the `scf.loop` infinite-loop shape.
    /// Inner body must terminate via an embedded `return` (or a future
    /// `break` op once that lowering lands).
    Loop { body: Vec<HlslBodyStmt> },
    /// `{ body }` — bare block statement. Used for nested scope where
    /// every inner statement runs in one fall-through path.
    Block { body: Vec<HlslBodyStmt> },
    /// `<expr>;` — an expression statement (e.g. a function call whose
    /// result is discarded). Used by `memref.store` and any future void
    /// MIR op that maps to an HLSL call.
    ExprStmt(HlslExpr),
    /// Raw pass-through line (caller-formatted ; the emitter handles
    /// the trailing semicolon if present).
    Raw(String),
    /// `// comment` — emitted as a line-comment. Used to thread MIR-op
    /// names + source-loc attributes into generated HLSL for diagnostics.
    Comment(String),
}

impl HlslBodyStmt {
    /// Render this statement at the given indent level (in spaces).
    #[must_use]
    pub fn render(&self, indent: usize) -> String {
        let mut out = String::new();
        self.render_into(&mut out, indent);
        out
    }

    fn render_into(&self, out: &mut String, indent: usize) {
        let pad = " ".repeat(indent);
        match self {
            Self::VarDecl { ty, name, init } => {
                writeln!(out, "{pad}{ty} {name} = {};", init.render()).unwrap();
            }
            Self::Assign { lhs, rhs } => {
                writeln!(out, "{pad}{lhs} = {};", rhs.render()).unwrap();
            }
            Self::Return { value } => match value {
                Some(v) => writeln!(out, "{pad}return {};", v.render()).unwrap(),
                None => writeln!(out, "{pad}return;").unwrap(),
            },
            Self::If {
                cond,
                then_body,
                else_body,
            } => {
                writeln!(out, "{pad}if ({}) {{", cond.render()).unwrap();
                for s in then_body {
                    s.render_into(out, indent + 4);
                }
                if else_body.is_empty() {
                    writeln!(out, "{pad}}}").unwrap();
                } else {
                    writeln!(out, "{pad}}} else {{").unwrap();
                    for s in else_body {
                        s.render_into(out, indent + 4);
                    }
                    writeln!(out, "{pad}}}").unwrap();
                }
            }
            Self::For {
                init,
                cond,
                step,
                body,
            } => {
                let init_str = init.as_ref().map_or(String::new(), |s| {
                    // Render single-line ; strip the trailing newline + indent.
                    let raw = s.render(0);
                    raw.trim_end().to_string()
                });
                let cond_str = cond.as_ref().map_or(String::new(), HlslExpr::render);
                let step_str = step.as_ref().map_or(String::new(), HlslExpr::render);
                writeln!(out, "{pad}for ({init_str} {cond_str}; {step_str}) {{").unwrap();
                for s in body {
                    s.render_into(out, indent + 4);
                }
                writeln!(out, "{pad}}}").unwrap();
            }
            Self::While { cond, body } => {
                writeln!(out, "{pad}while ({}) {{", cond.render()).unwrap();
                for s in body {
                    s.render_into(out, indent + 4);
                }
                writeln!(out, "{pad}}}").unwrap();
            }
            Self::Loop { body } => {
                writeln!(out, "{pad}do {{").unwrap();
                for s in body {
                    s.render_into(out, indent + 4);
                }
                writeln!(out, "{pad}}} while (true);").unwrap();
            }
            Self::Block { body } => {
                writeln!(out, "{pad}{{").unwrap();
                for s in body {
                    s.render_into(out, indent + 4);
                }
                writeln!(out, "{pad}}}").unwrap();
            }
            Self::ExprStmt(e) => {
                writeln!(out, "{pad}{};", e.render()).unwrap();
            }
            Self::Raw(line) => {
                writeln!(out, "{pad}{line}").unwrap();
            }
            Self::Comment(text) => {
                writeln!(out, "{pad}// {text}").unwrap();
            }
        }
    }
}

impl HlslStatement {
    /// Render this statement as HLSL source text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        match self {
            Self::CBuffer {
                name,
                body,
                register,
            } => {
                write!(out, "cbuffer {name}").unwrap();
                if let Some(r) = register {
                    write!(out, " : register({r})").unwrap();
                }
                writeln!(out, " {{\n{body}\n}};").unwrap();
            }
            Self::Struct { name, fields } => {
                writeln!(out, "struct {name} {{").unwrap();
                for f in fields {
                    writeln!(out, "    {f}").unwrap();
                }
                writeln!(out, "}};").unwrap();
            }
            Self::RwBuffer {
                element_type,
                name,
                register,
            } => {
                write!(out, "RWStructuredBuffer<{element_type}> {name}").unwrap();
                if let Some(r) = register {
                    write!(out, " : register({r})").unwrap();
                }
                writeln!(out, ";").unwrap();
            }
            Self::Function {
                return_type,
                name,
                params,
                attributes,
                semantic,
                body,
            } => {
                for a in attributes {
                    writeln!(out, "{a}").unwrap();
                }
                write!(out, "{return_type} {name}({})", params.join(", ")).unwrap();
                if let Some(s) = semantic {
                    write!(out, " : {s}").unwrap();
                }
                writeln!(out, " {{").unwrap();
                for line in body {
                    writeln!(out, "    {line}").unwrap();
                }
                writeln!(out, "}}").unwrap();
            }
            Self::Raw(line) => writeln!(out, "{line}").unwrap(),
        }
        out
    }
}

/// Skeletal HLSL translation unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlslModule {
    /// Optional `#pragma` header comment block.
    pub header: Option<String>,
    /// Top-level statements in declaration order.
    pub statements: Vec<HlslStatement>,
}

impl HlslModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a statement.
    pub fn push(&mut self, s: HlslStatement) {
        self.statements.push(s);
    }

    /// Render the whole module as HLSL text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(h) = &self.header {
            writeln!(out, "{h}").unwrap();
            writeln!(out).unwrap();
        }
        for s in &self.statements {
            out.push_str(&s.render());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{HlslBinaryOp, HlslBodyStmt, HlslExpr, HlslModule, HlslStatement, HlslUnaryOp};

    #[test]
    fn struct_statement_rendering() {
        let s = HlslStatement::Struct {
            name: "Vertex".into(),
            fields: vec![
                "float3 position : POSITION;".into(),
                "float2 uv : TEXCOORD;".into(),
            ],
        };
        let r = s.render();
        assert!(r.contains("struct Vertex"));
        assert!(r.contains("float3 position : POSITION;"));
    }

    #[test]
    fn function_statement_rendering() {
        let s = HlslStatement::Function {
            return_type: "void".into(),
            name: "main".into(),
            params: vec!["uint3 tid : SV_DispatchThreadID".into()],
            attributes: vec!["[numthreads(64, 1, 1)]".into()],
            semantic: None,
            body: vec!["// stage-0 skeleton".into()],
        };
        let r = s.render();
        assert!(r.contains("[numthreads(64, 1, 1)]"));
        assert!(r.contains("void main(uint3 tid : SV_DispatchThreadID)"));
    }

    #[test]
    fn rw_buffer_statement_rendering() {
        let s = HlslStatement::RwBuffer {
            element_type: "float4".into(),
            name: "OutBuf".into(),
            register: Some("u0".into()),
        };
        let r = s.render();
        assert!(r.contains("RWStructuredBuffer<float4> OutBuf : register(u0);"));
    }

    #[test]
    fn cbuffer_statement_rendering() {
        let s = HlslStatement::CBuffer {
            name: "Globals".into(),
            body: "    float4x4 view_proj;".into(),
            register: Some("b0".into()),
        };
        let r = s.render();
        assert!(r.contains("cbuffer Globals : register(b0) {"));
        assert!(r.contains("float4x4 view_proj;"));
    }

    #[test]
    fn module_assembly() {
        let mut m = HlslModule::new();
        m.header = Some("// autogenerated by cssl-cgen-gpu-dxil".into());
        m.push(HlslStatement::Raw("#define FOO 1".into()));
        m.push(HlslStatement::Function {
            return_type: "void".into(),
            name: "main".into(),
            params: vec![],
            attributes: vec!["[numthreads(1,1,1)]".into()],
            semantic: None,
            body: vec!["// empty".into()],
        });
        let r = m.render();
        assert!(r.contains("autogenerated by cssl-cgen-gpu-dxil"));
        assert!(r.contains("#define FOO 1"));
        assert!(r.contains("void main()"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § S6-D2 (T11-D73) : HlslExpr renderings.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn expr_var_renders() {
        assert_eq!(HlslExpr::Var("x".into()).render(), "x");
    }

    #[test]
    fn expr_int_lit_renders() {
        assert_eq!(HlslExpr::IntLit("42".into()).render(), "42");
    }

    #[test]
    fn expr_uint_lit_renders_with_u_suffix() {
        assert_eq!(HlslExpr::UintLit("7".into()).render(), "7u");
    }

    #[test]
    fn expr_float_lit_renders_with_f_suffix_when_f32() {
        let f = HlslExpr::FloatLit {
            text: "2.5".into(),
            is_f32: true,
        };
        assert_eq!(f.render(), "2.5f");
    }

    #[test]
    fn expr_float_lit_renders_without_f_suffix_when_not_f32() {
        let f = HlslExpr::FloatLit {
            text: "2.5".into(),
            is_f32: false,
        };
        assert_eq!(f.render(), "2.5");
    }

    #[test]
    fn expr_bool_lit_renders() {
        assert_eq!(HlslExpr::BoolLit(true).render(), "true");
        assert_eq!(HlslExpr::BoolLit(false).render(), "false");
    }

    #[test]
    fn expr_binary_renders_parenthesized() {
        let e = HlslExpr::Binary {
            op: HlslBinaryOp::Add,
            lhs: Box::new(HlslExpr::Var("a".into())),
            rhs: Box::new(HlslExpr::Var("b".into())),
        };
        assert_eq!(e.render(), "(a + b)");
    }

    #[test]
    fn expr_binary_renders_all_operators() {
        let pairs = [
            (HlslBinaryOp::Add, "+"),
            (HlslBinaryOp::Sub, "-"),
            (HlslBinaryOp::Mul, "*"),
            (HlslBinaryOp::Div, "/"),
            (HlslBinaryOp::Rem, "%"),
            (HlslBinaryOp::Eq, "=="),
            (HlslBinaryOp::Ne, "!="),
            (HlslBinaryOp::Lt, "<"),
            (HlslBinaryOp::Le, "<="),
            (HlslBinaryOp::Gt, ">"),
            (HlslBinaryOp::Ge, ">="),
            (HlslBinaryOp::LogicAnd, "&&"),
            (HlslBinaryOp::LogicOr, "||"),
        ];
        for (op, glyph) in pairs {
            assert_eq!(op.spelling(), glyph);
        }
    }

    #[test]
    fn expr_unary_renders_parenthesized() {
        let e = HlslExpr::Unary {
            op: HlslUnaryOp::Neg,
            rhs: Box::new(HlslExpr::Var("x".into())),
        };
        assert_eq!(e.render(), "(-x)");
    }

    #[test]
    fn expr_ternary_renders_parenthesized() {
        let e = HlslExpr::Ternary {
            cond: Box::new(HlslExpr::Var("c".into())),
            then_branch: Box::new(HlslExpr::Var("a".into())),
            else_branch: Box::new(HlslExpr::Var("b".into())),
        };
        assert_eq!(e.render(), "(c ? a : b)");
    }

    #[test]
    fn expr_call_renders_args_with_commas() {
        let e = HlslExpr::Call {
            name: "min".into(),
            args: vec![HlslExpr::Var("a".into()), HlslExpr::Var("b".into())],
        };
        assert_eq!(e.render(), "min(a, b)");
    }

    #[test]
    fn expr_buffer_load_renders() {
        let e = HlslExpr::BufferLoad {
            buffer: "g_buf".into(),
            index: Box::new(HlslExpr::IntLit("3".into())),
        };
        assert_eq!(e.render(), "g_buf[3]");
    }

    #[test]
    fn expr_cast_renders() {
        let e = HlslExpr::Cast {
            ty: "int".into(),
            rhs: Box::new(HlslExpr::Var("x".into())),
        };
        assert_eq!(e.render(), "((int)x)");
    }

    #[test]
    fn expr_raw_passes_through() {
        assert_eq!(
            HlslExpr::Raw("WaveActiveSum(x)".into()).render(),
            "WaveActiveSum(x)"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § HlslBodyStmt renderings.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn body_var_decl_renders_with_indent() {
        let s = HlslBodyStmt::VarDecl {
            ty: "int".into(),
            name: "v3".into(),
            init: HlslExpr::IntLit("42".into()),
        };
        assert_eq!(s.render(4), "    int v3 = 42;\n");
    }

    #[test]
    fn body_return_value_renders() {
        let s = HlslBodyStmt::Return {
            value: Some(HlslExpr::Var("x".into())),
        };
        assert_eq!(s.render(0), "return x;\n");
    }

    #[test]
    fn body_return_void_renders() {
        let s = HlslBodyStmt::Return { value: None };
        assert_eq!(s.render(0), "return;\n");
    }

    #[test]
    fn body_if_renders_then_only() {
        let s = HlslBodyStmt::If {
            cond: HlslExpr::Var("c".into()),
            then_body: vec![HlslBodyStmt::Return {
                value: Some(HlslExpr::IntLit("1".into())),
            }],
            else_body: vec![],
        };
        let r = s.render(0);
        assert!(r.contains("if (c) {"));
        assert!(r.contains("    return 1;"));
        assert!(r.contains("\n}\n"));
        assert!(!r.contains("else"));
    }

    #[test]
    fn body_if_renders_then_else() {
        let s = HlslBodyStmt::If {
            cond: HlslExpr::Var("c".into()),
            then_body: vec![HlslBodyStmt::Return {
                value: Some(HlslExpr::IntLit("1".into())),
            }],
            else_body: vec![HlslBodyStmt::Return {
                value: Some(HlslExpr::IntLit("0".into())),
            }],
        };
        let r = s.render(0);
        assert!(r.contains("if (c) {"));
        assert!(r.contains("} else {"));
        assert!(r.contains("    return 0;"));
    }

    #[test]
    fn body_for_renders() {
        let s = HlslBodyStmt::For {
            init: Some(Box::new(HlslBodyStmt::VarDecl {
                ty: "int".into(),
                name: "i".into(),
                init: HlslExpr::IntLit("0".into()),
            })),
            cond: Some(HlslExpr::Binary {
                op: HlslBinaryOp::Lt,
                lhs: Box::new(HlslExpr::Var("i".into())),
                rhs: Box::new(HlslExpr::IntLit("4".into())),
            }),
            step: Some(HlslExpr::Raw("i++".into())),
            body: vec![],
        };
        let r = s.render(0);
        assert!(r.contains("for ("));
        assert!(r.contains("int i = 0;"));
        assert!(r.contains("(i < 4);"));
        assert!(r.contains("i++"));
    }

    #[test]
    fn body_while_renders() {
        let s = HlslBodyStmt::While {
            cond: HlslExpr::Var("k".into()),
            body: vec![HlslBodyStmt::Return { value: None }],
        };
        let r = s.render(0);
        assert!(r.contains("while (k) {"));
        assert!(r.contains("    return;"));
    }

    #[test]
    fn body_loop_renders_do_while_true() {
        let s = HlslBodyStmt::Loop {
            body: vec![HlslBodyStmt::Return {
                value: Some(HlslExpr::IntLit("0".into())),
            }],
        };
        let r = s.render(0);
        assert!(r.contains("do {"));
        assert!(r.contains("} while (true);"));
    }

    #[test]
    fn body_block_renders_with_braces() {
        let s = HlslBodyStmt::Block {
            body: vec![HlslBodyStmt::Return { value: None }],
        };
        let r = s.render(0);
        assert!(r.starts_with('{'));
        assert!(r.contains("    return;"));
    }

    #[test]
    fn body_expr_stmt_renders() {
        let s = HlslBodyStmt::ExprStmt(HlslExpr::Call {
            name: "discard".into(),
            args: vec![],
        });
        assert_eq!(s.render(0), "discard();\n");
    }

    #[test]
    fn body_comment_renders_with_slash_slash() {
        let s = HlslBodyStmt::Comment("hello".into());
        assert_eq!(s.render(0), "// hello\n");
    }

    #[test]
    fn body_raw_passes_through() {
        let s = HlslBodyStmt::Raw("[loop] // unroll-hint".into());
        assert_eq!(s.render(0), "[loop] // unroll-hint\n");
    }

    #[test]
    fn body_assign_renders() {
        let s = HlslBodyStmt::Assign {
            lhs: "x".into(),
            rhs: HlslExpr::Var("y".into()),
        };
        assert_eq!(s.render(0), "x = y;\n");
    }
}

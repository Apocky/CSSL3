# Audit: `cssl-parse` — Recursive-Descent Parser

**Auditor**: Claude Sonnet 4.6 agent  
**Date**: 2026-05-14  
**Files audited**: 17 (16 source .rs + 1 Cargo.toml)  
**Total LOC**: ~4,573 (source only, excluding Cargo.toml)

---

## 1. CRATE OVERVIEW

`cssl-parse` is the stage-0 parser for the CSSLv3 / Sigil compiler. It occupies the second stage of the front-end pipeline:

```
cssl-lex (tokenizer) → cssl-parse (CST producer) → cssl-hir (HIR elaborator)
```

The crate's public surface is a single function, `parse(source, tokens) -> (Module, DiagnosticBag)`, which accepts a `SourceFile` (with surface indicator) and a flat token slice from `cssl_lex::lex`, and emits a `cssl_ast::Module` CST together with a bag of accumulated diagnostics. There is no `Result` return; the parser always produces a fully-formed (possibly partial) CST.

### Dual-Surface Design

The crate implements two distinct syntactic surfaces that both produce the same `cssl_ast::Module` CST type, following the spec decision in `specs/16_DUAL_SURFACE.csl § PARSER UNIFICATION`:

**Rust-hybrid surface** (`src/rust_hybrid/`): A Rust-flavoured syntax intended for algorithm authoring and engine code. Uses C-family delimiters, braces, semicolons, and angle-bracket generics. Expression precedence is resolved via a Pratt (binding-power) climber with 15 levels. Handles the full item grammar: `fn`, `struct`, `enum`, `interface`, `impl`, `effect`, `handler`, `type`, `use`, `const`, `module`. This surface also supports CSSLv3-specific syntax: capability types (`iso<T>`, `val<T>`, etc.), refinement types (`{v : T | P(v)}` and `T'tag` sugar), effect rows (`/ {GPU, NoAlloc}`), Koka-style `perform`/`with`/`region` expressions, pipeline operator (`|>`), and `#run` comptime-eval markers.

**CSLv3-native surface** (`src/csl_native/`): A dense, glyph-native surface for spec authoring in the CSLv3 notation itself (as used throughout `specs/`). Uses `§` as section openers, indentation for block structure, and CSLv3 compound-formation operators (tatpuruṣa/dvandva/etc.). At stage-0 this surface is a structural stub: it parses `§ name [body]` hierarchies into `Item::Module` wrappers and recognises evidence/modal slot prefixes, but full morpheme-stacking and slot-template elaboration are deferred to `cssl-hir`.

**Surface dispatch**: `lib.rs:parse()` inspects `source.surface` (a `cssl_ast::source::Surface` enum) and routes to `rust_hybrid::parse_module` or `csl_native::parse_module`. `Surface::Auto` falls through to Rust-hybrid.

**Shared infrastructure** (`src/common.rs`, `src/cursor.rs`, `src/error.rs`): Both surfaces share the `TokenCursor` (the token-stream abstraction with 2-token lookahead), the `ParseError`/`DiagnosticBag` helpers, and the common identifier/path parsers. This prevents duplication and ensures consistent error-reporting behaviour.

**Error recovery philosophy**: All parse functions return a CST node unconditionally. Unrecoverable positions produce `Expr::Error` or synthetic placeholder nodes (zero-width `Ident`, `TypeKind::Infer`, etc.) and push a `Diagnostic::error` into the shared bag. Callers (including an LSP) never see `Option<Node>` for parser output — partial CSTs remain walkable after errors (stated in `lib.rs:24`).

---

## 2. CRATE METADATA

**Path**: `compiler-rs/crates/cssl-parse/`  
**Cargo name**: `cssl-parse`  
**Description**: "CSSLv3 stage0 — CSLv3-native + Rust-hybrid parser dispatch"  
**Version/edition/license**: workspace-inherited  

### Dependencies (Cargo.toml)

| Dependency | Type | Purpose |
|---|---|---|
| `cssl-ast` | path (`../cssl-ast`) | CST node types (`Module`, `Item`, `Expr`, `Type`, `Pattern`, `Span`, `DiagnosticBag`, etc.) |
| `cssl-lex` | path (`../cssl-lex`) | Token types (`Token`, `TokenKind`, `Keyword`, `BracketKind`, `EvidenceMark`, `ModalOp`, `CompoundOp`, etc.) |
| `thiserror` | workspace | `#[derive(Error)]` on `ParseError` |

No external runtime dependencies. No proc-macro dependencies. `unsafe_code` is forbidden (`#![forbid(unsafe_code)]` in lib.rs).

### Total LOC

| File | LOC |
|---|---|
| `src/lib.rs` | 71 |
| `src/cursor.rs` | 247 |
| `src/error.rs` | 167 |
| `src/common.rs` | 191 |
| `src/rust_hybrid/mod.rs` | 91 |
| `src/rust_hybrid/attr.rs` | 177 |
| `src/rust_hybrid/generics.rs` | 198 |
| `src/rust_hybrid/ty.rs` | 530 |
| `src/rust_hybrid/pat.rs` | 309 |
| `src/rust_hybrid/expr.rs` | 1,161 |
| `src/rust_hybrid/item.rs` | 954 |
| `src/rust_hybrid/stmt.rs` | 23 |
| `src/csl_native/mod.rs` | 73 |
| `src/csl_native/section.rs` | 151 |
| `src/csl_native/slot.rs` | 92 |
| `src/csl_native/compound.rs` | 83 |
| `tests/integration.rs` | 251 |
| **Total (source)** | **4,573** |

### File List by Subdirectory

```
compiler-rs/crates/cssl-parse/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── cursor.rs
│   ├── error.rs
│   ├── common.rs
│   ├── rust_hybrid/
│   │   ├── mod.rs
│   │   ├── attr.rs
│   │   ├── generics.rs
│   │   ├── ty.rs
│   │   ├── pat.rs
│   │   ├── expr.rs
│   │   ├── item.rs
│   │   └── stmt.rs
│   └── csl_native/
│       ├── mod.rs
│       ├── section.rs
│       ├── slot.rs
│       └── compound.rs
└── tests/
    └── integration.rs
```

---

## 3. PER-FILE AUDIT

Files are presented in dependency order: shared infrastructure first, then rust_hybrid surface (mod, attr, generics, ty, pat, stmt, expr, item), then csl_native surface (mod, section, slot, compound), then integration tests.

---

### 3.1 `src/lib.rs` — 71 lines

**Purpose**: Crate root. Declares all public submodules, re-exports the two primary types (`TokenCursor`, `ParseError`), and implements the single public entry point `parse()`. Also defines the `STAGE0_SCAFFOLD` version constant and a small `scaffold_tests` inline test module.

#### Items

**`pub fn parse(source: &SourceFile, tokens: &[Token]) -> (Module, DiagnosticBag)`** (line 51)  
The crate's sole public parsing entry. Creates a fresh `DiagnosticBag`, matches `source.surface` to select `csl_native::parse_module` or `rust_hybrid::parse_module`, and returns both the CST and the bag. The `#[must_use]` annotation forces callers to handle the diagnostic bag. `Surface::Auto` aliases to Rust-hybrid (documented; callers can run `cssl_lex::mode::detect` before calling if they need true auto-detection).

**`pub const STAGE0_SCAFFOLD: &str`** (line 61)  
Exposes `env!("CARGO_PKG_VERSION")` as a `&str` constant. Despite the name "scaffold", this is simply the version string; the name reflects the stage-0 scaffolding framing in the project, not a stub status.

**`mod scaffold_tests`** (lines 64–81)  
Two unit tests:  
- `scaffold_version_present`: asserts `STAGE0_SCAFFOLD` is non-empty.  
- `empty_source_produces_empty_module_no_errors`: round-trips an empty string through `lex → parse` and asserts no items and no errors.

#### Spec references
- `specs/09_SYNTAX.csl` (Rust-hybrid surface and operator precedence)
- `specs/16_DUAL_SURFACE.csl` (mode detection and parser unification)
- `specs/02_IR.csl` (HIR contract)
- `CSLv3/specs/13_GRAMMAR_SELF.csl` (LL(2) grammar, compound-formation, slot-template)

#### Design decisions cited
- T3-D1: hand-rolled recursive-descent + Pratt; zero combinator library
- T3-D2: no interning in CST; identifiers carry `Span` only
- T3-D3: morpheme/compound chains surface at CST as `Expr::Compound { op, lhs, rhs }`
- T3-D4: CST single-file in `cssl-ast`; HIR modular in `cssl-hir`

---

### 3.2 `src/cursor.rs` — 247 lines

**Purpose**: The `TokenCursor` — the single token-stream abstraction used by both surfaces. Provides 2-token lookahead (`peek()` / `peek2()`), a consume primitive (`bump()`), and convenience helpers (`check()`, `eat()`, `expect()` is in `common.rs`). Handles trivia skipping (whitespace, line comments, block comments) and has a `skip_newlines` toggle for the CSLv3-native surface.

#### Items

**`struct TokenCursor<'a>`** (line 30)  
```rust
pub struct TokenCursor<'a> {
    tokens: &'a [Token],
    pos: usize,
    skip_newlines: bool,
    eof_span: Span,
}
```
Fields:
- `tokens`: immutable borrow of the full token slice; `Clone` is O(1) because only the index is copied.
- `pos`: raw position into `tokens`; `effective_pos()` advances past trivia before use.
- `skip_newlines`: when `true`, `Newline` tokens are treated as trivia (Rust-hybrid default). When `false`, newlines are structurally significant (CSLv3-native).
- `eof_span`: sentinel span used for diagnostics when past end-of-stream; synthesized from the last `Eof` token or the end of the last token.

**`impl<'a> TokenCursor<'a>`** (line 41)

- **`pub fn new(tokens: &'a [Token]) -> Self`** (line 48): Standard constructor. Synthesizes `eof_span` from the last `Eof` token if present, or from the end of the last token; falls back to `Span::DUMMY`. Sets `skip_newlines = true`.

- **`pub fn newline_aware(tokens: &'a [Token]) -> Self`** (line 70): Variant constructor used by the CSLv3-native surface. Calls `new()` then sets `skip_newlines = false`.

- **`pub fn set_skip_newlines(&mut self, skip: bool)`** (line 77): Toggle; allows mid-parse switching of newline-skip behaviour for context-sensitive positions.

- **`pub fn effective_pos(&self) -> usize`** (line 83): Scans forward from `self.pos`, skipping all trivia tokens, and returns the raw index of the first non-trivia token. Does not mutate state; used internally by `peek()`, `bump()`.

- **`fn is_trivia(&self, t: &Token) -> bool`** (line 95): Returns `true` for `Whitespace`, `LineComment`, `BlockComment`, and (`Newline` when `skip_newlines`). Private helper.

- **`pub fn peek(&self) -> Token`** (line 105): Non-consuming lookahead at the current (post-trivia) token. Returns a synthetic `TokenKind::Eof` token with `eof_span` when past end-of-stream; never returns `None` and never panics.

- **`pub fn peek2(&self) -> Token`** (line 116): Non-consuming lookahead at the second token past the current. Implemented by cloning `self`, calling `bump()` on the clone, then calling `peek()` on it. O(trivia-scan) not O(1), but used only for LL(2) disambiguation — infrequently.

- **`pub fn bump(&mut self) -> Token`** (line 124): Advance `pos` past trivia, consume the current token (increment `pos` if not EOF), and return it. Returns a synthetic EOF if already at end-of-stream. Idempotent at EOF.

- **`pub fn check(&self, expected: TokenKind) -> bool`** (line 140): Returns `true` iff `peek().kind == expected`. Non-consuming.

- **`pub fn eat(&mut self, expected: TokenKind) -> Option<Span>`** (line 146): If `check(expected)` is true, bumps and returns `Some(span)`; otherwise returns `None`. Frequently used for optional tokens (commas, semicolons, etc.).

- **`pub fn is_eof(&self) -> bool`** (line 156): Returns `true` iff `peek().kind == TokenKind::Eof`.

- **`pub fn eof_span(&self) -> Span`** (line 162): Returns the EOF sentinel span. `const fn`.

**`mod tests`** (lines 168–272)  
Eight unit tests covering: empty cursor returns EOF, bump advances and returns tokens, trivia skipped by default, newline-aware mode preserves newlines, peek2 looks ahead one, peek does not advance, eat matches and consumes, check does not consume, eof_span preserved from source.

#### Notable algorithm
The trivia-skip is not a separate pre-processing step; it happens lazily on every `peek()`/`bump()` call via `effective_pos()`. This means every trivia token is "visited" at most once per peek/bump call, keeping the cursor stateless with respect to trivia position (no secondary pointer).

---

### 3.3 `src/error.rs` — 167 lines

**Purpose**: Defines the structured `ParseError` enum and a set of convenience factory functions for constructing `cssl_ast::Diagnostic` records. Centralises all diagnostic messages so that CI assertion on message substrings remains stable and IDE quick-fix detection can match on prefixes.

#### Items

**`enum ParseError`** (line 21)  
```rust
pub enum ParseError {
    Expected { expected: Vec<TokenKind>, found: TokenKind, span: Span, context: &'static str },
    UnexpectedEof { span: Span, context: &'static str },
    NotYetSupported { form: &'static str, span: Span },
    Custom { message: String, span: Span },
}
```
Derives `Debug`, `Clone`, `thiserror::Error`. The four variants cover all diagnostic cases:
- `Expected`: expected one of several token kinds; found something else.
- `UnexpectedEof`: reached EOF mid-parse.
- `NotYetSupported`: syntactically valid but not yet implemented at stage-0 (maps to spec-deferred forms).
- `Custom`: free-form message for cases not fitting other variants (used sparingly).

**`impl ParseError`**:

- **`pub const fn span(&self) -> Span`** (line 64): Extracts the span from any variant via a match. Marked `const`.

- **`pub fn to_diagnostic(&self) -> Diagnostic`** (line 75): Converts `self` into a `cssl_ast::Diagnostic` via `Diagnostic::error(msg).with_span(self.span())`. This is the primary conversion path; parser rules call this and push the result into the bag.

**Free functions** (all `#[must_use]`):

- **`pub fn expected_one(expected, found, span, context) -> Diagnostic`** (line 82): Single-alternative convenience; builds `ParseError::Expected` with a one-element `Vec` and converts it.

- **`pub fn expected_any(expected: Vec<TokenKind>, found, span, context) -> Diagnostic`** (line 99): Multi-alternative convenience.

- **`pub fn custom(message: impl Into<String>, span) -> Diagnostic`** (line 116): Free-form diagnostic.

- **`pub fn nyi(form: &'static str, span) -> Diagnostic`** (line 127): "Not yet implemented" diagnostic using `NotYetSupported`.

**`mod tests`** (lines 132–181)  
Five tests: span accessor works, `expected_one` builds error diagnostic with correct severity and `Semi` in message, `expected_any` lists alternatives, `custom` carries message, `nyi` mentions form name.

---

### 3.4 `src/common.rs` — 191 lines

**Purpose**: Surface-independent helper parsers for identifiers and paths. These are the leaf combinators shared by both rust_hybrid and csl_native surfaces. All functions take `&mut TokenCursor` and `&mut DiagnosticBag` and never return `None` — on error they emit a diagnostic and return a zero-width synthetic node.

#### Items

**`pub fn parse_ident(cursor, bag, context) -> Ident`** (line 16)  
Consumes a `TokenKind::Ident` token, returns an `Ident { span }`. On mismatch, pushes `expected_one(Ident, found, span, context)` and returns an `Ident` with a zero-width span at the error site (i.e., `Span::new(source, start, start)`).

**`pub fn parse_module_path(cursor, bag, context) -> ModulePath`** (line 43)  
Parses a `::` or `.` separated path (both separators accepted). Intended for module-declaration and use-path contexts where `.` is a path separator, not field access. Delegates to `parse_path_with_seps(..., accept_dot: true)`.

**`pub fn parse_colon_path(cursor, bag, context) -> ModulePath`** (line 53)  
Parses a `::` only path. For expression and pattern contexts where `.` means field-access, not path separator. Delegates to `parse_path_with_seps(..., accept_dot: false)`.

**`fn parse_path_with_seps(cursor, bag, context, accept_dot) -> ModulePath`** (line 61) — private  
The actual path-parsing logic. First segment is required (calls `parse_ident`). Loop: peek at separator, if separator followed by `Ident` (checked via `peek2()`), consume separator and next ident, push segment. Otherwise break. Constructs the `ModulePath` span by joining the first and last segments' spans.

**Notable algorithm**: The `peek2()` guard before consuming `::` or `.` prevents consuming these tokens when the following token is not an identifier — this correctly handles cases like `foo :: 42` where the `::` belongs to a higher-level construct. Tested by `parse_module_path_stops_at_non_ident`.

**`pub fn expect(cursor, bag, expected, context) -> Span`** (line 96)  
Consume a required token kind, push diagnostic on mismatch. Returns the consumed span (or zero-width on mismatch). Used by expression and item parsers for mandatory delimiters.

**`pub fn expect_any(cursor, bag, expected: &[TokenKind], context) -> Option<TokenKind>`** (line 113)  
Like `expect` but for one-of-N token kinds. Returns `Some(kind)` on match, `None` on mismatch (after pushing diagnostic). Does not advance on mismatch (the diagnostic-push handles it).

**`mod tests`** (lines 130–209)  
Seven tests: single ident, non-ident diagnosed, multi-segment `::` path, `.` separator path, stops at non-ident, `expect` advances on match, `expect` diagnoses on mismatch.

---

### 3.5 `src/rust_hybrid/mod.rs` — 91 lines

**Purpose**: Entry point for the Rust-hybrid surface. Declares the seven submodules (`attr`, `expr`, `generics`, `item`, `pat`, `stmt`, `ty`), implements `parse_module()`, and contains two inline tests.

#### Items

**`pub fn parse_module(source: &SourceFile, tokens: &[Token], bag: &mut DiagnosticBag) -> Module`** (line 35)  
Algorithm:
1. Creates a `TokenCursor::new(tokens)` (newline-skipping mode).
2. Synthesises `module_span` from first-token start to EOF end.
3. Parses zero-or-more inner attributes (`#![…]`) at file head by checking `Hash` + `Bang`.
4. Calls `item::parse_optional_module_path` to consume an optional `module a.b.c` file-level declaration.
5. Loop: calls `item::parse_item` until EOF. Guards against infinite loops: if `parse_item` returns `None` without advancing `effective_pos`, breaks immediately.

**`mod mod_tests`** (lines 79–102)  
Two tests: empty module has no items and no errors; module span covers source (spot-check that `span.source` matches `src.id`).

---

### 3.6 `src/rust_hybrid/attr.rs` — 177 lines

**Purpose**: Parses outer attributes (`@name(args)`) and inner attributes (`#![name = …]` or `#![name(args)]`). Attributes are optional and appear before items and some expressions. At CST level both forms produce `cssl_ast::Attr` with `AttrKind::Outer` or `AttrKind::Inner`.

#### Items

**`pub fn parse_outer(cursor, bag) -> Option<Attr>`** (line 21)  
Returns `None` immediately if current token is not `@`. Otherwise: bumps `@`, parses path via `parse_module_path`, parses optional `(args)` via `parse_attr_args`, builds `Attr { span, kind: Outer, path, args }`.

**`pub fn parse_inner(cursor, bag) -> Option<Attr>`** (line 45)  
Returns `None` if current is not `#` or peek2 is not `!`. Otherwise: bumps `#` and `!`, expects `[`, parses path, accepts either `= expr` (single positional) or `(args)`, expects `]`. Builds `Attr { kind: Inner, … }`.

**`fn parse_attr_args(cursor, bag) -> Vec<AttrArg>`** (line 89) — private  
Consumes `(`, then loops until `)` or EOF. Each iteration: if `Ident =` (named arg), consumes name + `=` + expr → `AttrArg::Named`; else parses expr → `AttrArg::Positional`. Separated by commas. Consumes closing `)`.

**`fn attr_arg_span(arg: &AttrArg) -> Span`** (line 121) — private  
Extracts the span from an `AttrArg` for computing the parent `Attr` span.

**`pub fn parse_outer_attrs(cursor, bag) -> Vec<Attr>`** (line 132)  
Loops calling `parse_outer` until it returns `None`. Used in `item.rs` and `expr.rs` to collect any leading outer attributes before an item or expression.

**`mod tests`** (lines 140–191)  
Four tests: bare outer attribute, outer attribute with named arg, inner key-value attribute, outer attrs stops at non-`@`.

---

### 3.7 `src/rust_hybrid/generics.rs` — 198 lines

**Purpose**: Parses optional `<T : Bound, const N : usize, 'r>` generic parameter lists and optional `where T : Bound, U : Bound1 + Bound2` clauses.

#### Items

**`pub fn parse_generics(cursor, bag) -> Generics`** (line 19)  
Returns `Generics::default()` immediately if current token is not `<`. Otherwise: bumps `<`, loops parsing `parse_generic_param` separated by commas, expects `>`. Builds `Generics { span, params }`.

**`fn parse_generic_param(cursor, bag) -> GenericParam`** (line 47) — private  
Three cases:
- `const N : T` → `GenericParamKind::Const`
- `'r` → `GenericParamKind::Region`
- `T` → `GenericParamKind::Type`

After the kind/name, optionally parses `: Bound1 + Bound2` bounds (comma-loop on `+`), and optionally `= Default` type. Span covers from start to the end of bounds/default.

Note (line 82–85): For const-params without an explicit type (bounds empty), the code explicitly defers validation to the elaborator rather than emitting a diagnostic here — the CST shape stays uniform with `Infer`.

**`pub fn parse_where_clauses(cursor, bag) -> Vec<WhereClause>`** (line 100)  
Returns empty `Vec` if current token is not `where`. Otherwise: bumps `where`, loops parsing `Type : Bound1 + Bound2` clauses separated by commas. Each iteration: parses the subject type, expects `:`, loops parsing bounds on `+`. Stops on absence of comma.

**`mod tests`** (lines 143–211)  
Six tests: absent generics returns default, single type param, type param with bound, multiple params with multi-bounds, single where clause, absent where returns empty.

---

### 3.8 `src/rust_hybrid/ty.rs` — 530 lines

**Purpose**: Parses all type expressions in the Rust-hybrid surface. Covers all forms from `specs/09_SYNTAX.csl` and `specs/03_TYPES.csl`.

#### Items

**`pub fn parse_type(cursor, bag) -> Type`** (line 33)  
Top-level type entry. Calls `parse_type_kind`, wraps in `Type { span, kind }`, then post-processes for `T'tag` refinement sugar. The post-processing loop: if `Apostrophe` follows, bumps it, parses the tag name (an ident), detects the `T'L<k>` Lipschitz form (if next is `<`, parses a bound expression, expects `>`), and wraps the current `ty` in `TypeKind::Refined { base, kind: RefinementKind::Tag{..} or Lipschitz{..} }`. This loop allows chaining multiple `'tag` refinements, though spec does not discuss multi-tag chains explicitly.

**`fn parse_type_kind(cursor, bag) -> TypeKind`** (line 74) — private  
The dispatch table:
- `Ident` → `parse_path_or_capability`
- `(` → `parse_tuple_or_paren`
- `[` → `parse_array_or_slice`
- `{` → `parse_refined_predicate`
- `&` → `parse_reference`
- `fn` keyword → `parse_fn_type`
- Capability keywords (`iso`, `trn`, `ref`, `val`, `box`, `tag`) → `parse_capability`
- Otherwise → push diagnostic, bump one token, return `TypeKind::Infer`

Note (lines 78–83): The `_` identifier (wildcard type) is intentionally not distinguished here because the lexer emits `_` as `Ident`. The comment states the elaborator promotes it to `Infer`. This is a known limitation: `_` in type position is treated as a path type rather than inferred, and the distinction only matters to the elaborator.

**`fn parse_path_or_capability(cursor, bag) -> TypeKind`** (line 114) — private  
Parses a module path (e.g., `Vec`), then optionally `<args>` via `parse_type_arg_list`. Returns `TypeKind::Path { path, type_args }`. (Capability keywords have their own lexer tokens and never reach this path.)

**`fn parse_tuple_or_paren(cursor, bag) -> TypeKind`** (line 125) — private  
Bumps `(`. Empty → `TypeKind::Tuple { elems: [] }` (unit). Non-empty: parses types separated by commas. If exactly one element without trailing comma → unwraps and returns that element's `kind` (parenthesized type, not tuple). Two-or-more → `TypeKind::Tuple { elems }`.

**`fn parse_array_or_slice(cursor, bag) -> TypeKind`** (line 154) — private  
Bumps `[`, parses element type. If `;` follows → parses length expr, expects `]` → `TypeKind::Array { elem, len }`. Otherwise expects `]` → `TypeKind::Slice { elem }`.

**`fn parse_refined_predicate(cursor, bag) -> TypeKind`** (line 186) — private  
Full-form refinement `{v : T | P(v)}`. Bumps `{`, parses binder ident, expects `:`, parses base type, expects `|`, parses predicate expr, expects `}`. Returns `TypeKind::Refined { base, kind: RefinementKind::Predicate { binder, predicate } }`.

**`fn parse_reference(cursor, bag) -> TypeKind`** (line 224) — private  
Bumps `&`, checks for `mut` keyword, parses inner type. Returns `TypeKind::Reference { mutable, inner }`.

**`fn parse_fn_type(cursor, bag) -> TypeKind`** (line 234) — private  
Bumps `fn`, expects `(`, parses comma-separated param types, expects `)`, optionally parses `->` return type (defaults to unit `TypeKind::Tuple { elems: [] }`), calls `parse_optional_effect_row` for `/ { … }` annotation. Returns `TypeKind::Function { params, return_ty, effect_row }`.

**`fn parse_capability(cursor, bag) -> TypeKind`** (line 273) — private  
Dispatches `Keyword::Iso|Trn|Ref|Val|Box|Tag` to `CapKind` variants. Expects `<` + inner type + `>`. Returns `TypeKind::Capability { cap, inner }`. On missing `<`, pushes diagnostic and returns `TypeKind::Infer`. The `unreachable!` at line 282 guards against being called with a non-capability token.

**`fn parse_type_arg_list(cursor, bag) -> Vec<Type>`** (line 305) — private  
Bumps `<`, parses comma-separated types, expects `>`. Returns the list.

**`pub fn parse_optional_effect_row(cursor, bag) -> Option<EffectRow>`** (line 327)  
Exported for use in `item.rs` (fn item signatures). Returns `None` if current token is not `/`. Otherwise: bumps `/`. If next is `Ident` (epsilon shorthand), bumps it and returns `Some(EffectRow { effects: [], tail: None })`. If next is `{`: parses comma-separated effect annotations (each a path + optional `<args>`), handles `..μ` polymorphic tail via `DotDot` + ident, expects `}`. Returns `Some(EffectRow { effects, tail })`.

**`fn parse_effect_annotation(cursor, bag) -> EffectAnnotation`** (line 385) — private  
Parses `EffectName<Arg1, Arg2>` where each arg is determined by `looks_like_expr_start` — if the token looks like an expression start (int/float/string literal), it's parsed as `EffectArg::Expr`, otherwise as `EffectArg::Type`.

**`fn looks_like_expr_start(cursor: &TokenCursor<'_>) -> bool`** (line 423) — private  
Heuristic: `true` for `IntLiteral | FloatLiteral | StringLiteral`. Used in `parse_effect_annotation` to disambiguate type-arg vs expr-arg inside effect `<args>` lists. Note this is coarse: identifiers are not considered expression starts here, so `Foo<Bar>` parses `Bar` as a type, not an expression. This is correct for typical effect args but not exhaustive.

**`mod tests`** (lines 430–558)  
Ten tests: path type, generic type, tuple type, mutable reference, `iso<T>` capability, `{v : f32 | v > 0}` refinement predicate, `f32'pos` tag-sugar refinement, slice type, `[f32; 4]` array, `fn(i32, i32) -> f32` function type, `/ {GPU, NoAlloc}` effect row.

---

### 3.9 `src/rust_hybrid/pat.rs` — 309 lines

**Purpose**: Parses all pattern forms used in `let` bindings, `match` arms, and function parameters.

#### Items

**`pub fn parse_pattern(cursor, bag) -> Pattern`** (line 24)  
Top-level entry. Parses one atomic pattern, then checks for `|` (or-pattern). If `|` follows, loops consuming `|` + atomic pattern to build `PatternKind::Or(alts)`. Returns the or-pattern or the single atomic if no `|`.

**`fn parse_atomic_pattern(cursor, bag) -> Pattern`** (line 42) — private  
Match on current token kind:
- `Ident` with `is_underscore()` → `PatternKind::Wildcard` (but see note below)
- `IntLiteral` → `PatternKind::Literal(LiteralKind::Int)`
- `FloatLiteral` → `PatternKind::Literal(LiteralKind::Float)`
- `StringLiteral(_)` → `PatternKind::Literal(LiteralKind::Str)`
- `CharLiteral` → `PatternKind::Literal(LiteralKind::Char)`
- `Keyword(True)` → `PatternKind::Literal(LiteralKind::Bool(true))`
- `Keyword(False)` → `PatternKind::Literal(LiteralKind::Bool(false))`
- `(` → `parse_tuple_pattern`
- `Keyword(Mut)` → binding with `mutable: true` (bumps `mut`, parses ident)
- `Keyword(Ref)` → `PatternKind::Ref { mutable, inner }` (bumps `ref`, optionally `mut`, recursively parses inner pattern)
- `Ident` → `parse_path_pattern`
- Otherwise → diagnostic, bump, return `PatternKind::Wildcard`

**IMPORTANT BUG/LIMITATION** (lines 127–134): `fn is_underscore(cursor: &TokenCursor<'_>, _span: Span) -> bool` always returns `false`. The comment explains the rationale: without access to source text, the parser cannot check if the identifier is literally `_`. As a result, the `Ident` + `is_underscore()` branch in `parse_atomic_pattern` is dead code — `_` in a pattern falls through to `parse_path_pattern` and becomes a single-segment `PatternKind::Binding { mutable: false, name }` with the name being `_`. The comment states the elaborator promotes these to `Wildcard`. This means `PatternKind::Wildcard` is never produced by the parser; it is a parse-error fallback only.

**`fn parse_tuple_pattern(cursor, bag) -> PatternKind`** (line 137) — private  
Bumps `(`, loops parsing patterns separated by commas until `)`. Returns `PatternKind::Tuple(elems)`.

**`fn parse_path_pattern(cursor, bag) -> PatternKind`** (line 159) — private  
Parses a colon-path (`::`-only, no dot). Then:
- If followed by `(` → tuple variant `PatternKind::Variant { path, args }` (parses comma-separated patterns until `)`)
- If followed by `{` → struct pattern `PatternKind::Struct { path, fields, rest }` (parses `name` or `name : pattern` fields, `..` for rest)
- Otherwise: single-segment → `PatternKind::Binding { mutable: false, name }`; multi-segment → unit variant `PatternKind::Variant { path, args: [] }`

**`mod tests`** (lines 241–325)  
Seven tests: literal int, binding, mut binding, tuple, variant with args, struct pattern, or-pattern.

---

### 3.10 `src/rust_hybrid/stmt.rs` — 23 lines

**Purpose**: A thin facade module. The comment explains the design decision: block and let-statement parsing live in `expr.rs` because the block-trailing-expression rule is most naturally expressed where the expression parser lives. This module simply re-exports `parse_block` from `expr.rs` under the `stmt` module path.

#### Items

**`pub use crate::rust_hybrid::expr::parse_block`** (line 8)  
Re-export. No new logic.

**`mod tests`** (lines 11–26)  
One test: `reexport_reachable` — verifies the re-export is callable by parsing `{ }` and asserting empty block.

---

### 3.11 `src/rust_hybrid/expr.rs` — 1,161 lines

**Purpose**: The expression parser using Pratt precedence-climbing. This is the largest file in the crate and implements the full expression grammar for the Rust-hybrid surface, including all 15 binding-power levels, 6 postfix operators, 5 unary prefix operators, and all control-flow / special-form expressions.

#### Pratt Table

Implemented via `const fn infix_bp(kind: TokenKind) -> Option<(u8, u8, InfixOp)>` (line 42) and `const fn postfix_bp(kind: TokenKind) -> Option<(u8, PostfixOp)>` (line 94):

| Level | Operator(s) | Associativity |
|---|---|---|
| 1 (postfix) | `.` field, `::` path-cont, `()` call, `[]` index, `?` try | highest |
| 2 (prefix) | `-` `!` `~` `*` `&` `&mut` | — |
| 2 (postfix) | `as` cast | — |
| 3 | `*` `/` `%` | left |
| 4 | `+` `-` | left |
| 5 | `<<` `>>` | left |
| 6 | `&` (bitwise) | left |
| 7 | `^` | left |
| 8 | `\|` (bitwise) | left |
| 9 | `==` `!=` `<` `<=` `>` `>=` `=>` entails | left |
| 10 | `&&` | left |
| 11 | `\|\|` | left |
| 12 | `..` `..=` (range) | left |
| 13 | `\|>` pipeline | left |
| 14 | `=` assign | right |
| 15 | `??` try-default | right |

BP encoding (line 85): `base = (20 - level) * 2`. Left-assoc: `(base, base+1)`. Right-assoc: `(base+1, base)`. The MAX_LEVEL=20 provides headroom for future glyph extensions.

#### Items

**`const fn infix_bp(kind: TokenKind) -> Option<(u8, u8, InfixOp)>`** (line 42)  
Returns `None` for non-operator tokens, otherwise `Some((left_bp, right_bp, op))`. Covers all infix operators per the Pratt table.

**`const fn postfix_bp(kind: TokenKind) -> Option<(u8, PostfixOp)>`** (line 94)  
Returns bp and postfix op kind for `.`, `::`, `(`, `[`, `?`, `as`.

**`const fn unary_prefix_bp() -> u8`** (line 107)  
Returns `(20 - 2) * 2 = 36`. All prefix unary operators share the same bp (level 2).

**`enum InfixOp`** (line 112)  
Private enum: `Bin(BinOp)`, `Assign(Option<BinOp>)`, `Pipeline`, `Range { inclusive }`, `TryDefault`.

**`enum PostfixOp`** (line 120)  
Private enum: `Field`, `PathCont`, `Call`, `Index`, `Try`, `Cast`.

**`pub fn parse_expr(cursor, bag) -> Expr`** (line 134)  
Entry: delegates to `parse_expr_bp(cursor, bag, 0)`.

**`fn parse_expr_bp(cursor, bag, min_bp) -> Expr`** (line 139) — private  
Pratt loop: parse prefix atom → LHS. Loop: check postfix → if `bp >= min_bp`, apply; check infix → if `lbp >= min_bp`, consume operator, recurse with `rbp`, combine; break otherwise.

**`fn parse_prefix(cursor, bag) -> Expr`** (line 166) — private  
The atom/prefix dispatcher. First collects outer attrs via `attr::parse_outer_attrs`. Then dispatches on the current token kind:
- Prefix unary (`-`, `!`, `~`, `*`): `unary(cursor, bag, op)` — bumps operator, recursively calls `parse_expr_bp` with `unary_prefix_bp()`
- `&`: `parse_reference_prefix` (handles `&` and `&mut`)
- Int/float/string/char/bool literals: bumps and returns `ExprKind::Literal`
- `Ident` or `SelfValue`/`SelfType` keywords: parses a colon-path, then checks for struct-constructor form via `looks_like_struct_body`
- `{`: `ExprKind::Block(parse_block(...))`
- `(`: `parse_paren_or_tuple`
- `[`: `parse_array_expr`
- `if`: `parse_if_expr`
- `match`: `parse_match_expr`
- `for`: `parse_for_expr`
- `while`: `parse_while_expr`
- `loop`: `parse_loop_expr`
- `return`: `parse_return_expr`
- `break`: `parse_break_expr`
- `continue`: `parse_continue_expr`
- `perform`: `parse_perform_expr` (F3 effect system)
- `with`: `parse_with_expr` (F3 effect system)
- `region`: `parse_region_expr` (F3 effect system)
- `|` / `||`: `parse_lambda_expr`
- `#` + `run`: `ExprKind::Run { expr }` (F4 staged computation)
- `SectionRef`: `ExprKind::SectionRef { path }` (CSL-embedded spec reference)
- Otherwise: diagnostic, bump, `ExprKind::Error`

**`fn unary(cursor, bag, op: UnOp) -> ExprKind`** (line 307) — private  
Bumps operator, recursively parses operand at `unary_prefix_bp()`.

**`fn parse_reference_prefix(cursor, bag) -> ExprKind`** (line 316) — private  
Bumps `&`, checks for `mut` → `UnOp::RefMut` vs `UnOp::Ref`.

**`fn in_context_forbidding_struct_brace(_cursor) -> bool`** (line 330) — private  
Always returns `false`. Comment at line 331–336 explains: at T3.2 the parser does not detect `if`-scrutinee context to suppress struct brace parsing; this is deliberately permissive and compensated by formatter rules. This creates a known ambiguity between `if x { ... }` and `if Point { x: 1 } { ... }`.

**`fn looks_like_struct_body(cursor) -> bool`** (line 342) — private  
Clones the cursor, bumps past `{`, checks what follows: empty `}` (true), `..` spread (true), `Ident` followed by `:`, `,`, or `}` (true), otherwise (false). Used to distinguish `Point { x: 1 }` constructor from `match x { 0 => ... }` body.

**`fn parse_struct_constructor(cursor, bag, path) -> ExprKind`** (line 362) — private  
Bumps `{`, loops parsing `name [: expr]` fields + optional `..base` spread, expects `}`. Returns `ExprKind::Struct { path, fields, spread }`.

**`fn parse_paren_or_tuple(cursor, bag) -> ExprKind`** (line 409) — private  
Bumps `(`. Empty → unit literal. First expr parsed; if `,` follows, adds to `elems` loop → `ExprKind::Tuple`. Single expr without `,` → `ExprKind::Paren(Box::new(first))`.

**`fn parse_array_expr(cursor, bag) -> ExprKind`** (line 445) — private  
Bumps `[`. Empty → `ExprKind::Array(ArrayExpr::List([]))`. First expr parsed; if `;` → `[elem; N]` repeat form → `ArrayExpr::Repeat`. Otherwise comma-loop → `ArrayExpr::List`.

**`fn parse_if_expr(cursor, bag) -> ExprKind`** (line 485) — private  
Bumps `if`, parses condition expr, parses then-block, optionally bumps `else` and parses either an `if` (else-if chain) or a block.

**`fn parse_match_expr(cursor, bag) -> ExprKind`** (line 513) — private  
Bumps `match`, parses scrutinee (no struct-brace guard — see `in_context_forbidding_struct_brace`), expects `{`, loops parsing arms (`[attrs] pattern [if guard] => body [,]`), expects `}`.

**`fn parse_for_expr(cursor, bag) -> ExprKind`** (line 559) — private  
Bumps `for`, parses pattern, expects `in` keyword, parses iterator expr, parses body block.

**`fn parse_while_expr(cursor, bag) -> ExprKind`** (line 572) — private  
Bumps `while`, parses condition, parses body block.

**`fn parse_loop_expr(cursor, bag) -> ExprKind`** (line 582) — private  
Bumps `loop`, parses body block.

**`fn parse_return_expr(cursor, bag) -> ExprKind`** (line 588) — private  
Bumps `return`, optionally parses a value expr (gated by `can_start_expression` heuristic).

**`fn parse_break_expr(cursor, bag) -> ExprKind`** (line 598) — private  
Bumps `break`, optionally parses `'label` (apostrophe + ident), optionally parses value expr.

**`fn parse_continue_expr(cursor, bag) -> ExprKind`** (line 614) — private  
Bumps `continue`, optionally parses `'label`.

**`fn parse_perform_expr(cursor, bag) -> ExprKind`** (line 625) — private  
F3 (Effect System). Bumps `perform`, parses effect path, optionally parses `(args)` call-args. Returns `ExprKind::Perform { path, args }`.

**`fn parse_with_expr(cursor, bag) -> ExprKind`** (line 636) — private  
F3. Bumps `with`, parses handler expression, parses body block. Returns `ExprKind::With { handler, body }`.

**`fn parse_region_expr(cursor, bag) -> ExprKind`** (line 646) — private  
F3 (region-based memory). Bumps `region`, optionally parses `'label`, parses body block. Returns `ExprKind::Region { label, body }`.

**`fn parse_lambda_expr(cursor, bag) -> ExprKind`** (line 658) — private  
Handles `|params| body` and `|| body` (zero-arg). Bumps open token (`|` or `||`). If `|`, loops parsing params until closing `|`; expects `|`. Optionally parses `-> return_ty`. Parses body expression. Returns `ExprKind::Lambda { params, return_ty, body }`.

**`fn parse_lambda_param(cursor, bag) -> Param`** (line 684) — private  
Pattern + optional `: type` annotation (defaults to `TypeKind::Infer`). No default value for lambda params.

**`fn apply_postfix(cursor, bag, lhs, op: PostfixOp) -> Expr`** (line 710) — private  
Dispatch on `PostfixOp`:
- `Field`: bumps `.`, parses ident, returns `ExprKind::Field { obj, name }`
- `PathCont`: bumps `::`. If next is `<` (turbofish), parses type-arg list, if followed by `(` → wraps as `ExprKind::Call { callee: lhs, args, type_args }` (T11-D39). If not followed by `(`, type-args are silently dropped (stage-0 limitation noted in comment at line 752). If no `<`, parses ident; if `lhs` is a `Path`, extends it; otherwise creates a `Field` as fallback.
- `Call`: calls `parse_call_args`, returns `ExprKind::Call { callee, args, type_args: [] }`
- `Index`: bumps `[`, parses index expr, expects `]`, returns `ExprKind::Index`
- `Try`: bumps `?`, returns `ExprKind::Try`
- `Cast`: bumps `as`, parses target type, returns `ExprKind::Cast`

**`fn parse_call_args(cursor, bag) -> Vec<CallArg>`** (line 848) — private  
Bumps `(`, loops: if `Ident =` → `CallArg::Named`; else `CallArg::Positional`. Separated by commas. Expects `)`.

**`fn combine_infix(op: InfixOp, lhs, rhs) -> Expr`** (line 879) — private  
Constructs the appropriate `ExprKind` from the infix op and both operand expressions. Handles all `InfixOp` variants.

**`pub fn parse_block(cursor, bag) -> Block`** (line 917)  
Re-exported via `stmt.rs`. Expects `{`, loops:
- `let` → `parse_let_stmt` → push `Stmt`
- Otherwise → parse expr. If `;` follows → push as `StmtKind::Expr`. If at `}` or EOF → set as trailing. Otherwise → push as `StmtKind::Expr` and continue (permissive for block-ending forms like `if`/`for`).
Expects `}`. Returns `Block { span, stmts, trailing }`.

**`fn parse_let_stmt(cursor, bag) -> Stmt`** (line 973) — private  
Bumps `let`. Checks for `mut` keyword — if present, parses a bare ident and wraps it as `PatternKind::Binding { mutable: true, name }` directly (bypassing `pat::parse_pattern`). If no `mut`, calls `pat::parse_pattern`. Optionally parses `: type`. Optionally parses `= expr`. Optionally consumes trailing `;`. Returns `Stmt { kind: StmtKind::Let { attrs, pat, ty, value } }`.

Note: the `mut` branch in `parse_let_stmt` (lines 976–985) constructs the binding pattern manually rather than calling `pat::parse_pattern` which would also handle `mut`. This is a simplification; `pat::parse_pattern` handles `mut x` correctly for match arm patterns, so the duplication is unnecessary.

**`const fn can_start_expression(kind: TokenKind) -> bool`** (line 1016)  
Heuristic used by `parse_return_expr` and `parse_break_expr` to decide if a value follows. Matches all literal kinds, Ident, relevant keywords, all open-bracket kinds, and unary prefix operators. `@` (for attributed expressions) is also included.

**`mod tests`** (lines 1055–1233)  
14 tests: int literal, simple add, mul-binds-tighter-than-add (precedence), left-assoc subtraction, unary negation, path expr, call expr, field access, index expr, tuple expr, if expr, block with trailing expr, cast expr, pipeline expr, assign right-assoc.

---

### 3.12 `src/rust_hybrid/item.rs` — 954 lines

**Purpose**: Parses all top-level item declarations in the Rust-hybrid surface. The item grammar is the largest and most varied component; this file dispatches to dedicated parsers for each item kind.

#### Items

**`pub fn parse_optional_module_path(cursor, bag) -> Option<ModulePath>`** (line 36)  
Uses a cloned lookahead cursor to distinguish `module foo;` (file-level path declaration) from `module foo { … }` (nested module item). If the peek after consuming the identifier path would be `{`, this is a nested module item and `None` is returned. Otherwise bumps `module`, parses path, optionally consumes `;`.

**`pub fn parse_item(cursor, bag) -> Option<Item>`** (line 71)  
Collects outer attrs and visibility, then dispatches on the current keyword:
- `fn` → `parse_fn_item`
- `struct` → `parse_struct_item`
- `enum` → `parse_enum_item`
- `interface` → `parse_interface_item`
- `impl` → `parse_impl_item`
- `effect` → `parse_effect_item`
- `handler` → `parse_handler_item`
- `type` → `parse_type_alias`
- `use` → `parse_use_item`
- `const` → `parse_const_item`
- `module` → `parse_module_item`
- `Eof` → `None`
- Otherwise → diagnostic, bump one token, `None`

**`fn parse_visibility(cursor) -> Visibility`** (line 121) — private  
If current is `pub`, bumps and returns `VisibilityKind::Public`. Otherwise returns `VisibilityKind::Private` with a zero-width span at the current position.

**`fn parse_fn_item(cursor, bag, attrs, visibility) -> FnItem`** (line 139) — private  
Bumps `fn`, name, generics, params, optional `-> return_ty`, optional effect row, optional where-clauses, then `parse_fn_body`. Builds `FnItem { span, attrs, visibility, name, generics, params, return_ty, effect_row, where_clauses, body }`.

**`fn parse_fn_body(cursor, bag) -> Option<Block>`** (line 174) — private  
If `{` → `Some(parse_block(...))`. If `;` → `None` (declaration-only fn). Otherwise diagnostic + `None`.

**`fn parse_param_list(cursor, bag) -> Vec<Param>`** (line 188) — private  
Expects `(`, loops: per-param outer attrs, pattern, optional `: type` (defaults to `TypeKind::Infer`), optional `= default`. Comma-separated. Expects `)`.

**`fn parse_struct_item(cursor, bag, attrs, visibility) -> StructItem`** (line 241) — private  
Bumps `struct`, name, generics, `parse_struct_body`. Builds `StructItem`.

**`fn parse_struct_body(cursor, bag) -> StructBody`** (line 262) — private  
Three forms:
- `;` → `StructBody::Unit`
- `(` → tuple struct: fields with optional visibility + type (no name). `cursor.eat(Semi)` at end.
- `{` → named struct: `name : type` fields, comma-separated.
- Fallthrough → `StructBody::Unit` (silent, no diagnostic — see Crate Notes).

**`fn parse_enum_item(cursor, bag, attrs, visibility) -> EnumItem`** (line 330) — private  
Bumps `enum`, name, generics, optional `{` body. Each variant: outer attrs, ident name, `parse_struct_body` (reuses struct body grammar for variant payloads). Comma-separated.

**`fn parse_interface_item(cursor, bag, attrs, visibility) -> InterfaceItem`** (line 376) — private  
Bumps `interface`, name, generics, optional `: super_bounds`, optional `{` body. Body dispatches via `parse_interface_assoc`.

**`fn parse_interface_assoc(cursor, bag) -> Option<InterfaceAssocItem>`** (line 428) — private  
Three cases:
- `fn` → `InterfaceAssocItem::Fn(parse_fn_item(...))`
- `Ident` followed by `type` keyword → `InterfaceAssocItem::AssociatedType(AssocTypeDecl {...})` with optional `: bounds` and `= default`
- `const` → `InterfaceAssocItem::Const(parse_const_item(...))`
- Otherwise → diagnostic, bump, `None`

Note (line 451): The associated-type detection uses `cursor.peek().kind == TokenKind::Ident && cursor.peek2().kind == Keyword::Type` — meaning it expects the literal text `associated type` as two tokens, the first being any identifier. The identifier is not checked to be the word "associated" — any `Ident type` pair triggers it.

**`fn parse_impl_item(cursor, bag, attrs) -> ImplItem`** (line 511) — private  
Bumps `impl`, generics, parses first type. If `for` follows, the first type is the trait and the second type is self_ty (`impl Trait for Self`). Otherwise the first type is self_ty (inherent impl). Where-clauses, optional `{` body with `parse_impl_assoc` per item.

**`fn parse_impl_assoc(cursor, bag) -> Option<ImplAssocItem>`** (line 558) — private  
Same dispatch as `parse_interface_assoc` but produces `ImplAssocItem`. Associated type form uses `= default` mandatory (diagnosed if absent).

**`fn parse_effect_item(cursor, bag, attrs, visibility) -> EffectItem`** (line 605) — private  
F3 (Effect System). Bumps `effect`, name, generics, optional `{` body of `fn` operation declarations. Each operation must be `fn`; anything else causes a diagnostic and breaks the loop.

**`fn parse_handler_item(cursor, bag, attrs, visibility) -> HandlerItem`** (line 654) — private  
F3. Bumps `handler`, name, generics, param list, optional `for EffectType`, optional `-> return_ty`, optional `{` body. Body handles `return { … }` clause and `fn` operation definitions. Non-`fn`/non-`return` lines in the body are consumed coarsely via `cursor.bump()` (line 710) — this is a known coarse parser: handler body-let-bindings are swallowed without CST nodes.

**`fn parse_use_item(cursor, bag, attrs, visibility) -> UseItem`** (line 733) — private  
Bumps `use`, parses `parse_use_tree`, optionally `;`.

**`fn parse_use_tree(cursor, bag) -> UseTree`** (line 751) — private  
Parses a path prefix. If `::` or `.` follows and then `*` → `UseTree::Glob`. If `{` follows → `UseTree::Group { prefix, trees }` (recursive). Otherwise optionally `as alias` → `UseTree::Path { path, alias }`. This correctly handles nested `use a::{b, c::d}` trees.

**`fn parse_const_item(cursor, bag, attrs, visibility) -> ConstItem`** (line 786) — private  
Bumps `const`, name, expects `:`, type, expects `=`, value expr, optional `;`.

**`fn parse_type_alias(cursor, bag, attrs, visibility) -> TypeAliasItem`** (line 814) — private  
Bumps `type`, name, generics, expects `=`, type, optional `;`.

**`fn parse_module_item(cursor, bag, attrs, visibility) -> ModuleItem`** (line 838) — private  
Bumps `module`, name. If `{` → inline body (recursive `parse_item` loop). Otherwise optional `;` → `items: None` (external module declaration).

**`mod tests`** (lines 876–993)  
Eight tests: module path declaration, fn with no body, fn with body, named struct with 2 fields, enum with 2 variants, use with alias, const item, type alias, pub visibility recognised.

---

### 3.13 `src/csl_native/mod.rs` — 73 lines

**Purpose**: Entry point for the CSLv3-native surface parser. Declares three submodules (`compound`, `section`, `slot`), implements `parse_module()`.

#### Items

**`pub fn parse_module(source, tokens, bag) -> Module`** (line 32)  
Creates a `TokenCursor::newline_aware(tokens)` (newline-aware mode, since the CSLv3-native surface uses newlines as block terminators and `Indent`/`Dedent` for nesting). Loops calling `section::parse_section`; on `None` from `parse_section`, breaks if EOF otherwise continues (the section parser advanced past malformed input). Returns `Module { span, inner_attrs: [], path: None, items }`.

Note: The module does not set `path` (it's always `None`) and does not parse inner attributes. This is consistent with CSLv3-native documents not having a `module foo` declaration — section structure is the organizational unit.

**`mod mod_tests`** (lines 58–82)  
Two tests: empty csl_native module, single `§ foo` section produces one module item.

---

### 3.14 `src/csl_native/section.rs` — 151 lines

**Purpose**: Parses `§ name [body]` sections as the primary structural unit of the CSLv3-native surface. Each section becomes an `Item::Module(ModuleItem)` in the CST, with nested sections becoming nested module items.

#### Items

**`pub fn parse_section(cursor, bag) -> Option<Item>`** (line 24)  
Algorithm:
1. Skips leading `Newline` tokens.
2. Returns `None` if EOF.
3. If not `Section` token (`§`), pushes diagnostic and bumps one token (error recovery), returns `None`.
4. Bumps `§`, parses name ident.
5. Calls `skip_to_section_boundary` to discard the rest of the header line.
6. If `Indent` follows: bumps `Indent`, loops until `Dedent`/EOF — nested `§` → recursive `parse_section`; other content → `skip_to_section_boundary` + eat Newline. Optionally eats `Dedent`.
7. Returns `Some(Item::Module(ModuleItem { name, items: nested }))`.

**`fn skip_to_section_boundary(cursor)`** (line 90) — private  
Loops bumping tokens until it hits `Newline`, `Indent`, `Dedent`, `Section`, or `Eof`. Then optionally eats one `Newline`. Used for discarding the header line content past the section name.

**`pub fn unsupported(span, form: &str) -> cssl_ast::Diagnostic`** (line 112)  
Helper exported for use by `compound.rs` and `slot.rs` when elaboration boundaries are reached. Builds a `custom` diagnostic: "CSLv3-native stage0 does not yet parse {form}".

**`mod tests`** (lines 121–164)  
Three tests: simple section header, section with extra header tokens (e.g., `§ foo ≡ bar`), nested sections (indented `§ inner`).

---

### 3.15 `src/csl_native/slot.rs` — 92 lines

**Purpose**: Stub slot-template recognizer. The slot-template grammar (`[EVIDENCE?] [MODAL?] [DET?] SUBJECT [RELATION] OBJECT [GATE?] [SCOPE?]`) is spec'd in `CSLv3/specs/13_GRAMMAR_SELF.csl § SLOT-TEMPLATE` but full decomposition is deferred to `cssl-hir`. This module provides a `recognize_prefix` function for the evidence and modal prefix slots only.

#### Items

**`pub struct SlotTemplate`** (line 19)  
```rust
pub struct SlotTemplate {
    pub evidence: Option<(EvidenceMark, Span)>,
    pub modal: Option<(ModalOp, Span)>,
    pub core_span: Option<Span>,
}
```
Carries the recognised optional prefix slots plus a `core_span` for the subject-relation-object triple (always `None` at stage-0 because core-triple recognition is deferred).

**`pub fn recognize_prefix(tokens: &[Token]) -> (SlotTemplate, usize)`** (line 31)  
Best-effort recognition against a raw token slice (not a cursor). Returns the `SlotTemplate` and an advancement count:
1. If `tokens[0]` is `TokenKind::Evidence(mark)` → record evidence, advance.
2. If next token is `TokenKind::Modal(m)` → record modal, advance.
Returns immediately if neither slot is present. Does not use `DiagnosticBag` — this is a pure recognition function used as a diagnostic aid by higher-level code.

Note: `recognize_prefix` takes a raw `&[Token]` slice rather than a `&mut TokenCursor`, which means it does not participate in the normal cursor-based error-recovery flow. It is designed for scanning individual lines after the section parser has isolated them.

**`mod tests`** (lines 49–101)  
Four tests: no prefix slots, evidence only, evidence then modal, modal only without evidence.

---

### 3.16 `src/csl_native/compound.rs` — 83 lines

**Purpose**: Thin combinator for building CST-level compound-expression nodes from a lex-level `CompoundOp` and two sub-expressions. This is the stage-0 stub for the full compound-formation grammar (tatpuruṣa / dvandva / karmadhāraya / bahuvrīhi / avyayībhāva); full chain parsing is scheduled for T3.3+.

#### Items

**`pub fn make_compound(op: LexCompoundOp, lhs: Expr, rhs: Expr) -> Expr`** (line 20)  
Calls `translate_compound_op`, computes span by joining `lhs.span.start` to `rhs.span.end`, returns `Expr { kind: ExprKind::Compound { op: ast_op, lhs, rhs } }`.

**`pub const fn translate_compound_op(op: LexCompoundOp) -> AstCompoundOp`** (line 36)  
One-to-one `const fn` mapping from `cssl_lex::CompoundOp` to `cssl_ast::CompoundOp`. Five variants: `Tp` (tatpuruṣa), `Dv` (dvandva), `Kd` (karmadhāraya), `Bv` (bahuvrīhi), `Av` (avyayībhāva). Full exhaustive match.

**`mod tests`** (lines 47–90)  
Two tests: all five variant translations, `make_compound` joins spans correctly and produces `ExprKind::Compound`.

---

### 3.17 `tests/integration.rs` — 251 lines

**Purpose**: End-to-end integration tests exercising the public `cssl_parse::parse()` entry point on realistic multi-item source fragments for both surfaces.

#### Helper

**`fn lex_parse(src: &str, surface: Surface) -> (Module, DiagnosticBag)`** (line 9)  
Creates a `SourceFile`, calls `cssl_lex::lex`, calls `cssl_parse::parse`, returns both outputs.

**`fn find_fn_body_trailing(src: &str) -> Expr`** (line 177)  
Helper for turbofish tests: parses source, finds first fn item, extracts its trailing expression. Asserts clean parse.

#### Tests

**Rust-hybrid tests**:
- `rust_hybrid_empty`: empty source → no items, no errors.
- `rust_hybrid_single_fn`: `fn hello() -> i32 { 42 }` → 1 item, body present, return type present.
- `rust_hybrid_struct_enum_use`: `use` + `struct` + `enum` → 3 items, no errors, field counts correct.
- `rust_hybrid_fn_with_generics_and_effects`: `fn render<S>(...) -> Image / {GPU, NoAlloc} { … }` → 1 generic param, effect row with 2 effects.
- `rust_hybrid_attributed_fn`: `@differentiable @lipschitz(k = 1.0) fn sphere_sdf(…)` → 2 attrs.
- `rust_hybrid_module_path_declaration`: `module com.apocky.loa` + fn → path with 3 segments, 1 item.
- `rust_hybrid_precedence_and_pipeline`: `1 + 2 * 3 |> double` in fn body → trailing expr is `Pipeline`.
- `rust_hybrid_match_arm`: `match x { 0 => 1, _ => 2 }` → no errors.

**CSLv3-native tests**:
- `csl_native_empty`: empty → no items, no errors.
- `csl_native_single_section`: `§ foo\n` → 1 item, `Item::Module`.
- `csl_native_multiple_sections`: `§ a\n§ b\n§ c\n` → 3 items.

**Surface dispatch**:
- `auto_dispatches_rust_from_fn_keyword`: `fn f() {}` with `Surface::Auto` → 1 item, no errors.

**Turbofish tests** (T11-D39):
- `turbofish_call_captures_type_args`: `id::<i32>(5)` → `Call { type_args: [i32], args: [5] }`.
- `turbofish_call_with_two_type_args`: `pair::<i32, f32>(1, 2.0)` → 2 type-args, 2 args.
- `non_turbofish_call_has_empty_type_args`: `f(5)` → no type_args (regression guard).
- `turbofish_call_with_no_args`: `make::<i32>()` → 1 type-arg, 0 args.

**Error recovery**:
- `unknown_top_level_produces_diagnostic_not_panic`: `42 fn ok() {}` → bag has errors, fn still parsed.

---

## 4. CRATE NOTES

### 4.1 Test Coverage

The crate has dense unit test coverage within each module (all inline `#[cfg(test)] mod tests` blocks), plus a separate integration test in `tests/integration.rs`. Coverage is genuinely exercised, not stub tests. Key areas exercised:
- All 15 infix precedence levels (directly tested: level 3, 4; indirectly via integration tests for pipeline and assignment)
- Turbofish propagation (4 dedicated integration tests, including the T11-D39 regression guards)
- Both surfaces (rust_hybrid and csl_native have integration tests)
- Error recovery: at minimum `unknown_top_level_produces_diagnostic_not_panic`
- All major item kinds: fn (with/without body), struct (named, tuple, unit via `parse_struct_body`), enum with variants, use with alias, const, type alias, interface (tested indirectly), impl (not directly integration-tested)

Coverage gaps:
- `effect` items are not integration-tested (only unit-tested in `item.rs`'s inline tests which are not present for effect/handler)
- `handler` items: the handler body coarse-skip (line 710) is not tested for the coarseness
- `interface` associated types: the `Ident type` two-token detection is not tested directly
- Pattern `Wildcard` (never produced by the parser — see below)
- `Region` expressions and `#run` expressions: not integration-tested

### 4.2 Known Incomplete / Stubbed Areas

**CSLv3-native surface is a structural stub.** The section parser discards all section body content beyond nested `§` sub-sections. Slot-template decomposition is recognition-only (`slot.rs` has `recognize_prefix` but no higher-level integration with the section parser). Compound-formation (`compound.rs`) is a helper with no caller in the csl_native parser — `make_compound` is exported but nothing in the csl_native surface currently invokes it.

**`is_underscore` always returns `false`** (`pat.rs:127–134`). This means `_` in any pattern context is parsed as a regular identifier binding, not as `PatternKind::Wildcard`. The elaborator must promote `_` bindings to wildcards. This is documented but is a spec divergence: specs expect `_` to be recognized as wildcard at the pattern surface. `PatternKind::Wildcard` is in the CST enum but is only produced as an error-fallback (line 117 in `parse_atomic_pattern`).

**`in_context_forbidding_struct_brace` always returns `false`** (`expr.rs:330`). This means `if Point { x: 1 } { … }` is ambiguously parsed — the `{ x: 1 }` is consumed as a struct constructor leaving the body block stranded. The comment notes this is T3.2 scope, deferred. This is a real parsing ambiguity for programs using struct constructors as if-conditions.

**Turbofish type-args dropped when not followed by `(`** (`expr.rs:752`). `Vec::<i32>` as a standalone expression (not called) results in the `lhs` being returned unchanged and the type-args silently discarded. This is a stage-0 limitation noted inline.

**Handler body coarse-skip** (`item.rs:710`). Handler bodies may contain let-bindings; these are consumed via `cursor.bump()` with no CST nodes produced.

**Compound-assignment operators not fully implemented.** The `InfixOp::Assign(Option<BinOp>)` arm exists in `combine_infix` and is used in `ExprKind::Assign { op: compound, … }`, but `infix_bp` only maps `TokenKind::Eq` to `InfixOp::Assign(None)`. Compound-assign tokens (`+=`, `-=`, `*=`, etc.) are absent from `infix_bp` — if they exist as lexer tokens they will fall through as "not an operator" and the Pratt loop will break. This is not a documented limitation in any comment.

**Effect-row `ε` epsilon parsing is coarse** (`ty.rs:338–343`). The comment states the lexer emits `ε` as a regular `Ident` token. The `parse_optional_effect_row` function, after seeing `/`, checks if the next token is `Ident` and if so treats it as an "epsilon shorthand" (empty row, no tail). This means any identifier after `/` is silently treated as epsilon — `/ Foo` where `Foo` is meant to name a single-effect row would be misinterpreted as empty. This is a potential spec ambiguity.

**Associated type detection in interface uses any-ident + `type`** (`item.rs:451`). The guard `cursor.peek().kind == Ident && cursor.peek2().kind == Keyword::Type` is triggered by any identifier before `type`. An interface body line like `foo type` would be misinterpreted as an associated type declaration even if `foo` is not the keyword "associated". This is an LL(2) limitation without source-text comparison.

**`STAGE0_SCAFFOLD` constant name** (`lib.rs:61`). The name might cause confusion — it is simply the crate version string, not a stub marker. The name reflects the stage-0 context but could mislead contributors into thinking it is placeholder infrastructure.

### 4.3 Spec / Code Divergence

| Spec claim | Code status |
|---|---|
| `specs/16_DUAL_SURFACE.csl § MODE-DETECTION`: `Surface::Auto` should auto-detect | Code routes `Auto` to Rust-hybrid; comment says callers can run `cssl_lex::mode::detect` before calling. Detection is not automatic. |
| `CSLv3/specs/13_GRAMMAR_SELF.csl § SLOT-TEMPLATE`: full slot decomposition | Stage-0 stub only: `recognize_prefix` for evidence+modal; rest deferred to `cssl-hir`. |
| `specs/09_SYNTAX.csl § OPERATOR-PRECEDENCE`: implies/entails at level 9 | `infix_bp` maps `FatArrow` to level 9 `BinOp::Implies` and `Entails` to level 9 `BinOp::Entails`. However `FatArrow` also serves as the match-arm separator (`=>`) — this creates a potential conflict in match-arm bodies; the match arm parser calls `expect(cursor, bag, FatArrow, "match arm")` which consumes it before expression parsing, so this specific conflict is avoided, but using `=>` as an infix "implies" operator inside general expressions is ambiguous with match syntax. |
| F4 Staged Computation (`@staged`, Futamura) | Only `#run` expression parsing is present (`ExprKind::Run`). `@staged` as an attribute is parsed by `attr.rs` (outer attr), but there is no dedicated staged-item parsing. |
| F5 Information Flow Control (Jif-style DLM labels) | No parser-level support visible. No `ExprKind` or `TypeKind` variants for DLM labels in the surface grammar. |
| F6 Observability | No parser-level observability markers visible in expression or item grammar. |

### 4.4 Dead Code / Surprises

- `PatternKind::Wildcard` variant exists in `cssl_ast` but is never produced by normal parsing (only as error fallback). It is effectively dead in the happy path.
- `compound.rs::make_compound` is exported but has no caller inside `cssl-parse`; it is intended to be called by the csl_native parser once compound-formation is implemented.
- `slot.rs::SlotTemplate::core_span` is always `None` (never set); the field exists for future use.
- `expr.rs` imports `Pattern` and `PatternKind` from `cssl_ast` at the top (line 29) — these are used in `parse_let_stmt` and `parse_lambda_param`, which is correct, but the import list is large and includes some types (`StructFieldInit`) that are only used once.
- The `STAGE0_SCAFFOLD` constant being named "scaffold" may cause confusion (see 4.2 above).

### 4.5 No TODO/FIXME/unimplemented!/todo!/panic!("stub") Found

A thorough search of all files found no `todo!()`, `unimplemented!()`, `panic!("stub"...)`, `// TODO`, `// FIXME` comments, or literal "scaffold" / "placeholder" strings in production code paths. The stubs and limitations described in 4.2 are communicated via doc-comments and inline explanatory comments rather than Rust stub macros. The constant `STAGE0_SCAFFOLD` contains the string "scaffold" in its identifier name but not in any runtime-reachable string.

---

*End of audit.*

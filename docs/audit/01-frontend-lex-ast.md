# Audit: CSSLv3 Stage-0 Compiler Frontend — `cssl-lex` + `cssl-ast`

**Auditor:** Claude Sonnet 4.6  
**Date:** 2026-05-14  
**Repo root:** `compiler-rs/`  
**Scope:** `crates/cssl-lex/` and `crates/cssl-ast/`  
**Spec authority:** `specs/09_SYNTAX.csl`, `specs/16_DUAL_SURFACE.csl`, `specs/02_IR.csl`, `specs/03_TYPES.csl`, `CSLv3/specs/12_TOKENIZER.csl`, `CSLv3/specs/13_GRAMMAR_SELF.csl`

---

## 1. SLICE OVERVIEW

These two crates form the entire compiler frontend below the parser. `cssl-ast` provides the foundational types — source identity, byte-offset spans, human-facing locations, diagnostics, and the concrete syntax tree (CST) — while `cssl-lex` is the dual-surface lexer that consumes a `SourceFile` and emits a flat `Vec<Token>` ready for the parser. Together they define the two earliest pipeline stages of the CSSLv3 stage-0 compiler:

```
SourceFile (cssl-ast) → [cssl-lex] → Vec<Token> → [cssl-parse, future] → CST (cssl-ast)
```

The design is intentionally surface-agnostic at the CST level: both the Rust-hybrid and CSLv3-native surfaces parse into the same `cst::Module`. The lexer handles the surface split via a mode-detection step and dispatches to one of two concrete lexer implementations. This unified-CST policy means downstream passes (HIR, type inference, codegen) never need to know which surface a file originated from.

**Maturity:** Both crates are tagged T2/T3 in-progress in their module docs. The lexer layer (`cssl-lex`) appears substantially complete for the token inventory both surfaces require. The CST (`cssl-ast`) is declared complete through T3 for its structural coverage; the parser that would populate it lives in a separate crate (`cssl-parse`, not in this slice). The diagnostic type is explicitly marked as a T2 scaffold awaiting richer miette integration.

---

## 2. CRATE: `cssl-ast`

**Path:** `compiler-rs/crates/cssl-ast/`  
**Purpose:** Foundational primitives for the entire compiler pipeline. Owns: source-file representation (`SourceFile`, `SourceId`, `Surface`), byte-offset positioning (`Span`, `SourceLocation`), error accumulation (`Diagnostic`, `DiagnosticBag`, `Severity`), and the full concrete syntax tree (`cst::*`). No external crates are listed as dependencies — this crate is intentionally a leaf with zero upstream compiler dependencies, making it safe to depend on from every other crate.

**Pipeline role:** Produces no output itself; purely a type library. The lexer crate depends on it for `SourceFile`, `Span`, and `Surface`. The parser (not in this slice) produces `Module` values. All downstream passes consume spans and diagnostics from here.

**Cargo.toml dependencies:**
- No external dependencies declared. All workspace metadata (`version`, `edition`, `rust-version`, `license`, `authors`) is inherited via `version.workspace = true` etc.
- `[lints] workspace = true` — inherits the aggressive clippy lint profile from the workspace root.

**Total LOC (all .rs):** approximately 1,030 lines across 5 source files.

**Source files:**
- `src/lib.rs` — crate root, re-exports, version constant
- `src/source.rs` — `SourceFile`, `SourceId`, `Surface`, `SourceLocation`
- `src/span.rs` — `Span`
- `src/diagnostic.rs` — `Severity`, `Diagnostic`, `Note`, `DiagnosticBag`
- `src/cst.rs` — full concrete syntax tree node types

---

### 2.1 `cssl-ast/src/lib.rs` (49 lines)

Crate root. Declares four submodules (`cst`, `diagnostic`, `source`, `span`), then re-exports the complete public API with flat `pub use` statements. There are no items defined here beyond the re-exports and a crate-version constant.

**Items:**

| Item | Signature / Description |
|------|------------------------|
| `STAGE0_SCAFFOLD` | `pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION")` — exposes the crate semver at compile time. Used as a smoke-test guard in unit tests to confirm the crate compiled with the workspace version. |
| `scaffold_tests` module | `#[cfg(test)]` block containing one test: `scaffold_version_present` — asserts `STAGE0_SCAFFOLD` is non-empty. Trivial guard-rail. |

**Re-exports (flat):** All public items from `cst`, `diagnostic`, `source`, and `span` are re-exported at crate root. Users import from `cssl_ast::Module`, `cssl_ast::Span`, etc. without traversing module paths.

---

### 2.2 `cssl-ast/src/source.rs` (286 lines)

Defines the source-file abstraction layer. The file makes no external crate calls, only uses `core::fmt` and `core::num::NonZeroU32`.

**Structs:**

**`SourceId`** (`source.rs:17`)
- Fields: `pub u32` (newtype). Derived: `Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Ord, PartialOrd`.
- Invariant: `SourceId(0)` is the synthetic "no-source" sentinel; real files start at `SourceId(1)`.
- Notable methods:
  - `const SYNTHETIC: Self = Self(0)` — sentinel constant (`source.rs:22`).
  - `const fn first() -> Self` — returns `SourceId(1)` (`source.rs:27`).
  - `const fn next(self) -> Self` — increments without overflow protection (wraps at `u32::MAX + 1`); the source-map owner is responsible for not allocating more than ~4B files (`source.rs:32`).
  - `const fn is_synthetic(self) -> bool` — tests `self.0 == 0` (`source.rs:37`).
  - `impl fmt::Display` — prints `<synthetic>` or `src#N` (`source.rs:44`).

**`Surface`** (`source.rs:55`)
- Variants: `RustHybrid` (default), `CslNative`, `Auto`.
- No fields on any variant.
- `#[default]` on `RustHybrid` — `Surface::default()` is always `RustHybrid`, consistent with the spec policy "ambiguous → default Rust-hybrid".
- Methods:
  - `const fn label(self) -> &'static str` — returns `"rust-hybrid"`, `"csl-native"`, or `"auto"` (`source.rs:68`).
  - `impl fmt::Display` delegates to `label` (`source.rs:78`).

**`SourceFile`** (`source.rs:85`)
- Fields: `id: SourceId`, `path: String`, `contents: String`, `surface: Surface`, `line_offsets: Vec<u32>` (private).
- The `line_offsets` vector is precomputed at construction time by `compute_line_offsets`; it records the byte-offset of the start of every line (including `0` for line 1). This allows O(log n) binary-search mapping of byte-offset → line number.
- Methods:
  - `fn new(id, path, contents, surface) -> Self` — public constructor; calls `compute_line_offsets` before returning (`source.rs:102`).
  - `fn compute_line_offsets(text: &str) -> Vec<u32>` — private; scans the byte stream for `\n` and records `idx+1` as the start of the next line (`source.rs:119`). Pre-allocates with `text.len() / 40 + 1` as a heuristic capacity.
  - `fn len_bytes(&self) -> u32` — returns `contents.len()` as `u32`, saturating at `u32::MAX` (`source.rs:133`). Files larger than ~4 GiB would silently cap; acceptable for a stage-0 compiler.
  - `fn slice(&self, start: u32, end: u32) -> Option<&str>` — range-check + char-boundary-check, then `&contents[start..end]` (`source.rs:142`). Returns `None` for out-of-bounds or non-char-boundary slices.
  - `fn position_of(&self, offset: u32) -> SourceLocation` — binary-searches `line_offsets` for the largest entry ≤ `offset`, then computes 1-indexed line and column (`source.rs:160`). Column is counted in UTF-8 code-units (bytes), not grapheme clusters. Out-of-range offsets clamp gracefully.

**`SourceLocation`** (`source.rs:180`)
- Fields: `line: NonZeroU32`, `column: NonZeroU32`, both 1-indexed.
- Invariant: both fields are strictly greater than zero (enforced by `NonZeroU32`). This eliminates a class of off-by-one bugs where line 0 could be emitted.
- Methods:
  - `const fn new(line: u32, column: u32) -> Option<Self>` — returns `None` if either is zero (`source.rs:192`).
  - `impl fmt::Display` — prints `line:column` (`source.rs:199`).

**Test module** (`source.rs:205`): 8 tests. Covers: `SourceId` sentinel/next, `Surface` default, surface label uniqueness, `slice` happy path + out-of-bounds + non-char-boundary, `position_of` single-line + multi-line + past-EOF clamping, `len_bytes` for multi-byte UTF-8.

---

### 2.3 `cssl-ast/src/span.rs` (167 lines)

Defines the `Span` type — the authoritative position carrier used everywhere spans need to be stored, compared, and joined.

**Struct `Span`** (`span.rs:17`)
- Fields: `pub source: SourceId`, `pub start: u32`, `pub end: u32` (half-open `[start, end)` interval).
- All fields are public; `Copy`.
- Invariant: `start <= end`, enforced with `assert!` in `new`.

**Constants:**
- `const DUMMY: Self` (`span.rs:38`) — zero-length span on `SourceId::SYNTHETIC`, used for compiler-generated items with no source location.

**Methods:**
- `const fn new(source: SourceId, start: u32, end: u32) -> Self` — panics if `start > end` (`span.rs:32`). The panic message is "Span::new : start must not exceed end". Called ubiquitously by lexer and parser.
- `const fn len(&self) -> u32` — `end - start` (`span.rs:46`). Never underflows due to the construction invariant.
- `const fn is_empty(&self) -> bool` — `start == end` (`span.rs:52`).
- `const fn same_source(&self, other: &Self) -> bool` — compares `source.0` directly (`span.rs:58`).
- `const fn join(&self, other: &Self) -> Option<Self>` — returns the smallest covering span if both share the same source; `None` otherwise (`span.rs:64`). Uses `if` chains rather than `min`/`max` to remain `const`.
- `const fn contains_offset(&self, offset: u32) -> bool` — half-open check `start <= offset < end` (`span.rs:87`).
- `impl fmt::Display` — prints `src#N@start..end` (`span.rs:93`).

**Test module** (`span.rs:99`): 7 tests. Covers: new/fields/len, empty detection, DUMMY sentinel, inverted-span panic (`#[should_panic]`), join-same-source, join-overlapping, join-different-source returns `None`, half-open `contains_offset`.

---

### 2.4 `cssl-ast/src/diagnostic.rs` (262 lines)

Diagnostic accumulation infrastructure. Explicitly self-described as a T2 scaffold — the plan is to integrate `miette` more richly (code labels, rich rendering) at T3 when the parser needs multi-span errors.

**Enum `Severity`** (`diagnostic.rs:15`)
- Variants in ascending order: `Help`, `Note`, `Warning`, `Error`. Derives `Ord + PartialOrd`, so `Severity::Error > Severity::Warning` etc. is well-defined.
- Methods:
  - `const fn label(self) -> &'static str` (`diagnostic.rs:29`).
  - `const fn is_error(self) -> bool` — `matches!(self, Self::Error)` (`diagnostic.rs:38`).
  - `impl fmt::Display` delegates to `label` (`diagnostic.rs:45`).

**Struct `Diagnostic`** (`diagnostic.rs:52`)
- Fields: `severity: Severity`, `message: String`, `span: Option<Span>`, `notes: Vec<Note>`.
- The `span` is optional to allow span-less diagnostics (e.g., file-level errors, OS errors before any source is read).
- Builder-pattern constructors (all return `Self` or mutate `self` with `#[must_use]`):
  - `fn error(message: impl Into<String>) -> Self` (`diagnostic.rs:78`).
  - `fn warning(message: impl Into<String>) -> Self` (`diagnostic.rs:88`).
  - `fn with_span(mut self, span: Span) -> Self` (`diagnostic.rs:98`).
  - `fn with_note(mut self, message: impl Into<String>) -> Self` (`diagnostic.rs:107`).
  - `fn with_help(mut self, message: impl Into<String>) -> Self` (`diagnostic.rs:116`).
  - `fn with_labeled_note(mut self, message: impl Into<String>, span: Span) -> Self` (`diagnostic.rs:128`).

**Struct `Note`** (`diagnostic.rs:65`)
- Fields: `severity: Severity`, `message: String`, `span: Option<Span>`. Secondary diagnostic attached to a primary `Diagnostic`.

**Struct `DiagnosticBag`** (`diagnostic.rs:144`)
- Fields: `items: Vec<Diagnostic>` (private), `error_count: u32` (private).
- Maintains a separate counter for error-severity items so `has_errors()` and `error_count()` are O(1).
- `Default` implemented; `new()` delegates to `default()`.
- Methods:
  - `fn push(&mut self, d: Diagnostic)` — increments `error_count` if `d.severity.is_error()` before pushing (`diagnostic.rs:157`). Uses `saturating_add` to prevent overflow at `u32::MAX` errors.
  - `fn iter(&self) -> impl Iterator<Item = &Diagnostic>` — forward-order iteration (`diagnostic.rs:165`).
  - `fn len(&self) -> usize` (`diagnostic.rs:172`).
  - `fn is_empty(&self) -> bool` (`diagnostic.rs:179`).
  - `const fn error_count(&self) -> u32` (`diagnostic.rs:184`).
  - `const fn has_errors(&self) -> bool` (`diagnostic.rs:189`).
  - `fn into_vec(self) -> Vec<Diagnostic>` — consumes the bag (`diagnostic.rs:195`).

**Test module** (`diagnostic.rs:201`): 4 tests. Covers: severity ordering, all four labels, builder chain (span + note + help), bag error counting + `into_vec` order.

---

### 2.5 `cssl-ast/src/cst.rs` (1,063 lines)

The concrete syntax tree. Provides all node types shared by both surface parsers. No behavior beyond type definitions — no `impl` blocks beyond an `Item::span()` dispatch method and test helpers. A `#![allow(clippy::large_enum_variant)]` inner attribute acknowledges that some enum variants (notably `ExprKind`) are significantly larger than others; boxing is deferred to a later optimization pass.

**Key design decisions visible in the code:**
- **Span-everywhere:** every struct and every enum variant that represents a source construct carries a `Span`. `Ident` is just a `Span` with no stored string — text is re-sliced from `SourceFile` at need (T3-D2).
- **No interning at CST level:** string interning happens in `cssl-hir`, not here.
- **No trivia:** comments and whitespace are dropped. The module doc explicitly marks this as a non-goal for T3.
- **Error recovery node:** `ExprKind::Error` (`cst.rs:726`) is the parser's placeholder when expression parsing fails but recovery continues.

**Top-level node types:**

**`Module`** (`cst.rs:31`) — root of one compilation unit.  
Fields: `span`, `inner_attrs: Vec<Attr>`, `path: Option<ModulePath>`, `items: Vec<Item>`.

**`ModulePath`** (`cst.rs:43`) — dotted identifier path.  
Fields: `span`, `segments: Vec<Ident>`. Used for `use`, `module`, attribute names, path expressions.

**`Ident`** (`cst.rs:51`) — identifier reference.  
Fields: `span: Span` only. `Copy`. Text recovered via `SourceFile::slice(span.start, span.end)`.

**`Visibility`** / **`VisibilityKind`** (`cst.rs:57`, `cst.rs:63`).  
`VisibilityKind::Private` (no `pub`) and `Public` (plain `pub`). Scoped pub (`pub(crate)`, `pub(super)`) not represented — spec says these do not exist in CSSL.

**`Item`** (`cst.rs:110`) — discriminated union of all top-level declaration kinds:
- `Fn(FnItem)`, `Struct(StructItem)`, `Enum(EnumItem)`, `Interface(InterfaceItem)`, `Impl(ImplItem)`, `Effect(EffectItem)`, `Handler(HandlerItem)`, `TypeAlias(TypeAliasItem)`, `Use(UseItem)`, `Const(ConstItem)`, `Module(ModuleItem)`.
- `impl Item { pub fn span(&self) -> Span }` (`cst.rs:126`) — dispatches to the inner item's span. The only `impl` on a non-primitive CST type.

**Attribute types:**

**`Attr`** (`cst.rs:79`) — `@name(args)` or `#![name]`.  
Fields: `span`, `kind: AttrKind`, `path: ModulePath`, `args: Vec<AttrArg>`.

**`AttrKind`** (`cst.rs:91`) — `Outer` (`@name`) vs `Inner` (`#![name]`).

**`AttrArg`** (`cst.rs:99`) — either `Positional(Expr)` or `Named { name: Ident, value: Expr }`.

**Item struct types (all with `span`, `attrs`, `visibility`, `name`, `generics`):**

**`FnItem`** (`cst.rs:145`) — function declaration + optional body.  
Extra fields: `params: Vec<Param>`, `return_ty: Option<Type>`, `effect_row: Option<EffectRow>`, `where_clauses: Vec<WhereClause>`, `body: Option<Block>`. `body = None` is used for interface method signatures (no body).

**`StructItem`** (`cst.rs:162`) + **`StructBody`** (`cst.rs:173`):  
`StructBody` distinguishes `Unit`, `Tuple(Vec<FieldDecl>)`, and `Named(Vec<FieldDecl>)`.

**`EnumItem`** (`cst.rs:184`) + **`EnumVariant`** (`cst.rs:195`):  
Each variant reuses `StructBody` for its payload shape.

**`InterfaceItem`** (`cst.rs:204`) — analogous to Rust traits.  
Extra: `super_bounds: Vec<Type>`, `items: Vec<InterfaceAssocItem>`.

**`InterfaceAssocItem`** (`cst.rs:216`) — `Fn(FnItem)`, `AssociatedType(AssocTypeDecl)`, `Const(ConstItem)`.

**`AssocTypeDecl`** (`cst.rs:223`) — `type Name : Bounds` in an interface.  
Fields: `span`, `attrs`, `name`, `bounds: Vec<Type>`, `default: Option<Type>`.

**`ImplItem`** (`cst.rs:234`) — trait impl or inherent impl.  
Extra: `trait_: Option<Type>` (None → inherent), `self_ty: Type`, `where_clauses`, `items: Vec<ImplAssocItem>`.

**`ImplAssocItem`** (`cst.rs:248`) — `Fn`, `AssociatedType(AssocTypeDef)`, `Const`.

**`AssocTypeDef`** (`cst.rs:254`) — `type Name = T` (concrete, inside impl).

**`EffectItem`** (`cst.rs:264`) — `effect Name<G> { ops }`.  
`ops: Vec<FnItem>` (all body-less, representing the effect's operation signatures).

**`HandlerItem`** (`cst.rs:277`) — `handler name(...) for Effect -> Ret { ops + return-clause }`.  
Extra: `handled_effect: Type`, `return_ty: Option<Type>`, `ops: Vec<FnItem>`, `return_clause: Option<Block>`.

**`TypeAliasItem`** (`cst.rs:293`) — `type Alias<G> = Ty`.

**`UseItem`** (`cst.rs:304`) + **`UseTree`** (`cst.rs:314`):  
`UseTree` is a recursive enum: `Path { path, alias }`, `Glob { path }`, `Group { prefix, trees }`.

**`ConstItem`** (`cst.rs:330`) — `const NAME : T = expr`.

**`ModuleItem`** (`cst.rs:340`) — nested `module Name { items }`.  
`items: Option<Vec<Item>>` — `None` for declaration-only (external file reference), `Some` for inline body.

**Generic / param types:**

**`Generics`** (`cst.rs:357`) — `<T : Bound, U>`. `Default` derived (empty). `span: Option<Span>` is `None` when there are no generics.

**`GenericParam`** (`cst.rs:364`) — one parameter. `kind: GenericParamKind` distinguishes `Type`, `Region` (lifetime), `Const`.

**`WhereClause`** (`cst.rs:385`) — `T : Bound1 + Bound2`.

**`Param`** (`cst.rs:393`) — fn parameter. `pat: Pattern` covers destructuring (`(x, y) : (f32, f32)`). `default: Option<Expr>` for default parameter values.

**`FieldDecl`** (`cst.rs:404`) — struct/tuple field. `name: Option<Ident>` is `None` for tuple-struct positional fields.

**Type types:**

**`Type`** (`cst.rs:419`) — wrapper carrying `span` + `TypeKind`.

**`TypeKind`** (`cst.rs:427`) — flat enum of all type shapes:
- `Path { path, type_args }` — named type, possibly generic.
- `Array { elem, len }` — `[T; N]` with boxed length expression.
- `Slice { elem }` — `[T]`.
- `Tuple { elems }` — arity 0 is unit `()`.
- `Reference { mutable, inner }` — `&T` / `&mut T`.
- `Capability { cap: CapKind, inner }` — Pony-6 capability wrapper (`iso<T>` etc.).
- `Function { params, return_ty, effect_row }` — first-class function types with optional effect row.
- `Refined { base, kind: RefinementKind }` — refinement types (`T'tag` sugar or `{ v : T | P(v) }` full form). Serves F2 (Refinement Types).
- `Infer` — `_` type hole.

**`CapKind`** (`cst.rs:459`) — `Iso, Trn, Ref, Val, Box, Tag` (Pony-6 reference-capability set).

**`RefinementKind`** (`cst.rs:471`) — three shapes:
- `Tag { name: Ident }` — `T'tagname`.
- `Predicate { binder, predicate }` — `{ v : T | P(v) }`.
- `Lipschitz { bound }` — `SDF'L<k>` built-in shape.

**`EffectRow`** (`cst.rs:482`) — `/ { e1, e2<arg>, ... }` on a function type.  
Fields: `span`, `effects: Vec<EffectAnnotation>`, `tail: Option<Ident>` (polymorphic row tail `μ`). Serves F3 (Effect System).

**`EffectAnnotation`** (`cst.rs:491`) — one entry: `name: ModulePath`, `args: Vec<EffectArg>`.

**`EffectArg`** (`cst.rs:499`) — `Type(Type)` or `Expr(Expr)` for parameterized effects like `Deadline<16ms>`.

**Pattern types:**

**`Pattern`** (`cst.rs:511`) — wrapper carrying `span` + `PatternKind`.

**`PatternKind`** (`cst.rs:517`):
- `Wildcard` — `_`.
- `Literal(Literal)`.
- `Binding { mutable, name }`.
- `Tuple(Vec<Pattern>)`.
- `Struct { path, fields: Vec<PatternField>, rest: bool }` — struct-destructure; `rest = true` means trailing `..`.
- `Variant { path, args }` — enum-variant with positional args.
- `Or(Vec<Pattern>)` — `a | b | c`.
- `Range { start, end, inclusive }`.
- `Ref { mutable, inner }` — `ref x`.

**`PatternField`** (`cst.rs:550`) — one field in a struct pattern. `pat: Option<Pattern>` is `None` for shorthand `{ x }`.

**Statement / block types:**

**`Block`** (`cst.rs:564`) — `{ stmts* trailing? }`. The `trailing: Option<Box<Expr>>` is the block's value expression (the expression without trailing semicolon).

**`Stmt`** (`cst.rs:573`) + **`StmtKind`** (`cst.rs:580`):
- `Let { attrs, pat, ty, value }` — `let` binding.
- `Expr(Expr)` — expression statement.
- `Item(Item)` — block-level item (rare but valid).

**Expression types:**

**`Expr`** (`cst.rs:599`) — `span`, `attrs: Vec<Attr>` (expressions can carry `@metamorphic` etc. annotations), `kind: ExprKind`.

**`ExprKind`** (`cst.rs:608`) — large flat enum. Noteworthy variants:
- `Call { callee, args, type_args }` — `type_args` carries turbofish explicit type-arguments (comment at `cst.rs:617` notes this is propagated through HIR-lowering to monomorphization per T11-D39).
- `Perform { path, args }` — `perform Effect::op(args)` (F3 effect invocation).
- `With { handler, body }` — `with handler-expr { body }` (F3 handler installation).
- `Region { label, body }` — `region 'r { body }` (region/capability scoping).
- `Run { expr }` — `#run expr` compile-time eval (F4 staged computation).
- `Compound { op: CompoundOp, lhs, rhs }` — CSLv3-native compound-formation expressions.
- `SectionRef { path }` — `§§ path` reference to another section (CSLv3-native only).
- `TryDefault { expr, default }` — `expr ?: default` (null-coalesce early-return default).
- `Error` — recovery placeholder.
- `Pipeline { lhs, rhs }` — `expr |> f`.

**`BinOp`** (`cst.rs:789`) — includes `Implies` (`⇒`/`=>` in proposition context) and `Entails` (`⊢`), which are CSLv3-native logical operators not found in Rust.

**`UnOp`** (`cst.rs:814`) — `Not, Neg, BitNot, Ref, RefMut, Deref`.

**`CompoundOp`** (`cst.rs:825`) — `Tp` (tatpurusha, `.`), `Dv` (dvandva, `+`), `Kd` (karmadharaya, `-`), `Bv` (bahuvrihi, `⊗`), `Av` (avyayibhava, `@`). The Sanskrit grammar names are from CSLv3/specs/13_GRAMMAR_SELF.csl.

**Literal types:**

**`Literal`** (`cst.rs:765`) + **`LiteralKind`** (`cst.rs:772`):  
`Int`, `Float`, `Str`, `Char`, `Bool(bool)`, `Unit`. Actual value re-parsed from source text at elaboration time; `Literal` just carries the `span` and `kind` hint.

**`MatchArm`** (`cst.rs:740`) — `pat [if guard] => body`.

**`StructFieldInit`** (`cst.rs:749`) — `name : value` initializer; `value: Option<Expr>` is `None` for shorthand `{ x }`.

**`CallArg`** (`cst.rs:759`) — `Positional(Expr)` or `Named { name, value }`.

**`ArrayExpr`** (`cst.rs:730`) — `List(Vec<Expr>)` or `Repeat { elem, len }`.

**Test module** (`cst.rs:843`): 8 tests. Covers: module construction with inner attrs + path, fn-item field access, construction of representative `ExprKind` variants (Literal, Binary, Unary, Compound), `let` statement pattern matching, `CapKind` enumeration (count check), `CompoundOp` enumeration, `Item::span` dispatch.

---

## 3. CRATE: `cssl-lex`

**Path:** `compiler-rs/crates/cssl-lex/`  
**Purpose:** The dual-surface lexer. Reads a `SourceFile` and produces a `Vec<Token>` ready for the parser. Dispatches on `SourceFile::surface` to one of two concrete lexer implementations: a `logos`-derived automaton for Rust-hybrid text, and a hand-rolled byte-stream lexer for CSLv3-native glyphs. Also provides surface auto-detection.

**Pipeline role:** Stage 1 of the compiler. Input: `SourceFile`. Output: `Vec<Token>`. Terminator: always `TokenKind::Eof` as final element.

**Cargo.toml dependencies:**
- `cssl-ast = { path = "../cssl-ast" }` — for `SourceFile`, `SourceId`, `Surface`, `Span`.
- `logos = { workspace = true }` — version 0.14, for the Rust-hybrid surface lexer macro.
- `thiserror = { workspace = true }` — declared but not visibly used in the current source (may be reserved for a future error type or used indirectly).

**Total LOC (all .rs):** approximately 1,350 lines across 5 source files.

**Source files:**
- `src/lib.rs` — crate root, top-level `lex` dispatcher, dispatch tests
- `src/token.rs` — unified `Token` + `TokenKind` + sub-enums
- `src/mode.rs` — surface mode auto-detection
- `src/rust_hybrid.rs` — logos-based Rust-hybrid surface lexer
- `src/csl_native.rs` — hand-rolled CSLv3-native surface lexer

**Integration tests:**
- `tests/integration.rs` — fixture-driven end-to-end tests
- `tests/fixtures/rust_hybrid_basic.cssl-rust` — Rust-hybrid fixture
- `tests/fixtures/csl_native_basic.cssl-csl` — CSLv3-native fixture

---

### 3.1 `cssl-lex/src/lib.rs` (117 lines)

Crate root. Declares four public submodules and re-exports the most commonly used token types at crate root. Contains the top-level `lex` dispatch function and five dispatch smoke-tests.

**Items:**

**`pub fn lex(source: &SourceFile) -> Vec<Token>`** (`lib.rs:44`)  
The primary API entry point. Dispatches on `source.surface`:
- `Surface::RustHybrid` → `rust_hybrid::lex(source)` directly.
- `Surface::CslNative` → `csl_native::lex(source)` directly.
- `Surface::Auto` → calls `mode::detect(&source.path, &source.contents)`, then dispatches. If detection returns `Auto` (the default-fallback case), routes to the Rust-hybrid lexer. The detection result is not written back into the `SourceFile` — callers who need to record the chosen surface must create a new `SourceFile` themselves.

**`pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION")`** (`lib.rs:60`)  
Version constant, same pattern as `cssl-ast`.

**`dispatch_tests` module** (`lib.rs:63`, `#[cfg(test)]`): 5 tests:
- `scaffold_version_present` — non-empty check.
- `dispatch_rust_hybrid_explicit` — explicit `Surface::RustHybrid`, checks for `Keyword(Fn)`.
- `dispatch_csl_native_explicit` — explicit `Surface::CslNative`, checks for `Section`.
- `dispatch_auto_detects_csl_from_section_glyph` — `Surface::Auto` on `"§ prose\n"`, checks for `Section`.
- `dispatch_auto_detects_rust_from_fn_keyword` — `Surface::Auto` on `"fn bar() {}\n"`, checks for `Keyword(Fn)`.
- `dispatch_auto_extension_csl` — `Surface::Auto` on a `.cssl-csl` path, checks for `Section`.

---

### 3.2 `cssl-lex/src/token.rs` (720 lines)

Defines the unified token vocabulary used by both surface lexers. All types are `Copy` (or at least `Clone`) and derive `Debug`, `PartialEq`, `Eq`, `Hash`.

**`struct Token`** (`token.rs:25`)  
Fields: `pub kind: TokenKind`, `pub span: Span`. `Copy`.  
Method: `const fn new(kind, span) -> Self` (`token.rs:35`).

**`enum TokenKind`** (`token.rs:46`)  
The central discriminated union covering all token varieties from both surfaces. Groups:

- **Literals (shared):** `Ident`, `IntLiteral`, `FloatLiteral`, `StringLiteral(StringFlavor)`, `CharLiteral`, `Suffix(TypeSuffix)`.
- **Keywords (Rust-hybrid only):** `Keyword(Keyword)`.
- **Punctuation (shared):** `Bracket(BracketKind, BracketSide)`, `Comma`, `Semi`, `Colon`, `ColonColon` (also Unicode `∷`), `Dot`, `DotDot`, `DotDotEq`, `At`, `Hash`, `Dollar`, `Apostrophe`, `Question`, `QuestionQuestion`.
- **Arithmetic / comparison (shared):** `Plus`, `Minus`, `Star`, `Slash`, `Percent`, `Eq`, `EqEq` (also `≡`), `Ne` (also `≠`), `Lt`, `Le` (also `≤`), `Gt`, `Ge` (also `≥`), `Amp`, `Pipe`, `Caret`, `Tilde`, `Bang`, `AmpAmp` (also `∧`), `PipePipe` (also `∨`), `LShift`, `RShift`.
- **Flow arrows (shared, ASCII + Unicode):** `Arrow` (`->` / `→`), `LeftArrow` (`<-` / `←`), `BiArrow` (`<->` / `↔`), `FatArrow` (`=>` / `⇒`), `PipeArrow` (`|>` / `▷`), `PipeArrowBack` (`<|`), `SquigglyArrow` (`~>`).
- **CSLv3-native structural:** `Section` (`§`), `SectionRef` (`§§`).
- **CSLv3-native semantic:** `Evidence(EvidenceMark)`, `Modal(ModalOp)`, `Compound(CompoundOp)`, `Determinative(Determinative, BracketSide)`.
- **Dense math:** `ForAll` (`∀`/`all`), `Exists` (`∃`/`any`), `ElemOf` (`∈`/`in`), `NotElemOf` (`∉`), `Subset` (`⊂`), `Superset` (`⊃`), `Therefore` (`∴`), `Because` (`∵`), `Entails` (`⊢`), `Qed` (`∎`/`QED`), `EmptySet` (`∅`/`nil`), `Infinity` (`∞`/`inf`).
- **Layout:** `Newline` (significant), `Indent` (CSLv3-native block open), `Dedent` (CSLv3-native block close), `Whitespace` (trivia-preserving mode only), `LineComment`, `BlockComment`.
- **Terminators:** `Eof`, `Error`.

Notable `Apostrophe` doc comment (`token.rs:97`): describes the complex disambiguation — standalone `'` may introduce refinement tags, type-suffix morphemes, or lifetime-like annotations. When the next single recognized morpheme letter follows at word-boundary, the lexer emits `Suffix(TypeSuffix)` atomically; otherwise it emits `Apostrophe` and lets the next token capture the rest.

**`enum Keyword`** (`token.rs:234`) — 41 keywords grouped:
- Item/binding: `Fn, Let, Const, Mut, Pub, Use, Module, Type, Struct, Enum, Interface, Impl`.
- Control flow: `If, Else, Match, While, For, In, Return, Break, Continue, Loop, Where`.
- Effects (F3): `Effect, Handler, Perform, With, Region`.
- Pony-6 capabilities: `Iso, Trn, Ref, Val, Box, Tag`.
- Staging/comptime (F4): `Comptime, Run`.
- Literals: `True, False`.
- Casts/self: `As, SelfValue, SelfType`.

Methods:
- `fn from_word(word: &str) -> Option<Self>` (`token.rs:336`) — exhaustive `match` over all 41 keywords. Case-sensitive. Returns `None` for unknown words.
- `const fn as_str(self) -> &'static str` (`token.rs:385`) — reverse mapping; mirrors `from_word`. Both must be kept in sync — a comment in the test (`token.rs:669`) calls this out explicitly.

**`enum BracketKind`** (`token.rs:438`) — `Paren`, `Brace`, `Square`.

**`enum BracketSide`** (`token.rs:448`) — `Open`, `Close`.

**`enum EvidenceMark`** (`token.rs:461`) — 8 variants: `Confirmed` (`✓`/`[x]`), `Partial` (`◐`/`[~]`), `Pending` (`○`/`[ ]`), `Failed` (`✗`/`[!]`), `Unknown` (`⊘`/`[?]`), `Hypothetical` (`△`/`[^]`), `Deprecated` (`▽`/`[v]`), `Proven` (`‼`/`[!!]`). Per `CSLv3/specs/13_GRAMMAR_SELF.csl`.

**`enum ModalOp`** (`token.rs:482`) — 10 variants: `Must` (`W!`), `Should` (`R!`), `May` (`M?`), `MustNot` (`N!`), `Insight` (`I>`), `Question` (`Q?`), `PushFurther` (`P>`), `Decision` (`D>`), `Todo` (bareword `TODO`), `Fixme` (bareword `FIXME`).

**`enum CompoundOp`** (`token.rs:508`) — 5 Sanskrit compound-formation ops matching `cst::CompoundOp`: `Tp` (`.`), `Dv` (`+`), `Kd` (`-`), `Bv` (`⊗`/`x*`), `Av` (`@`). Note: this is a *separate* `CompoundOp` from `cst::CompoundOp` — both have identical structures but exist in different crates.

**`enum Determinative`** (`token.rs:522`) — 6 Unicode enclosure pairs: `AngleTuple` (`⟨⟩`), `Formula` (`⟦⟧`), `Constraint` (`⌈⌉`), `Precondition` (`⌊⌋`), `Quotation` (`«»`), `Temporal` (`⟪⟫`).

**`enum TypeSuffix`** (`token.rs:539`) — 9 morpheme letters: `Data` (`'d`), `Func` (`'f`), `System` (`'s`), `Type` (`'t`), `Entity` (`'e`), `Material` (`'m`), `Property` (`'p`), `Gate` (`'g`), `Rule` (`'r`). Methods:
- `const fn from_letter(letter: char) -> Option<Self>` (`token.rs:565`) — single-char dispatch.
- `const fn letter(self) -> char` (`token.rs:581`) — reverse.

**`enum StringFlavor`** (`token.rs:603`) — `Normal` (`"…"`) vs `Raw` (`r"…"` / `r#"…"#`).

**Test module** (`token.rs:615`): 4 tests. Covers: `Token` is `Copy`, keyword `from_word` / `as_str` roundtrip for a 7-keyword sample, unknown keyword returns `None`, `TypeSuffix` letter roundtrip for all 9, and a count-guard asserting exactly 41 keywords are recognized.

---

### 3.3 `cssl-lex/src/mode.rs` (305 lines)

Surface auto-detection. Implements a four-step priority chain as documented in `specs/16_DUAL_SURFACE.csl`.

**`struct Detection`** (`mode.rs:25`)  
Fields: `pub surface: Surface`, `pub reason: Reason`. `Copy`.

**`enum Reason`** (`mode.rs:33`) — `Extension`, `Pragma`, `FirstLine`, `Default`. Used downstream for diagnostic framing ("I guessed Rust-hybrid from the file extension").

**`pub fn detect(filename: &str, contents: &str) -> Detection`** (`mode.rs:50`)  
Entry point. Never reads the filesystem — takes the logical filename (extension only matters) and full contents as `&str`. The priority order:
1. Extension: `.cssl-csl` → `CslNative`, `.cssl-rust` → `RustHybrid`.
2. Pragma: calls `detect_pragma(contents)`.
3. First-line heuristic: calls `detect_first_line(contents)`.
4. Default: `RustHybrid` with `Reason::Default`.

**`fn detect_pragma(contents: &str) -> Option<Surface>`** (`mode.rs:85`)  
Private. Scans up to 9 lines (hard cutoff at `i > 8`) skipping blank + comment lines. Looks for `#![surface` prefix; matches `"csl"` or `"csl-native"` for `CslNative`, `"rust"` or `"rust-hybrid"` for `RustHybrid`. Stops scanning at the first non-pragma non-comment non-blank line.

**`fn detect_first_line(contents: &str) -> Option<Surface>`** (`mode.rs:111`)  
Private. Finds the first non-blank non-comment line. If it starts with `§` → `CslNative`. If it starts with any word from `RUST_HYBRID_ITEM_KEYWORDS` (checked via `has_prefix_word`) → `RustHybrid`. Returns `None` if the first real line matches neither.

**`const RUST_HYBRID_ITEM_KEYWORDS: &[&str]`** (`mode.rs:134`)  
12 keywords: `module, use, fn, struct, enum, pub, interface, impl, type, const, effect, handler`.

**`fn is_comment_line(line: &str) -> bool`** (`mode.rs:150`)  
Private. Returns `true` for lines starting with `//`, `/*`, or `#` (but not `#!` — inner attributes are not comments).

**`fn has_prefix_word(s: &str, word: &str) -> bool`** (`mode.rs:158`)  
Private. Checks that `s` starts with exactly `word` followed by a non-alphanumeric non-underscore byte or end-of-string. Prevents `functional` from matching `fn`.

**Test module** (`mode.rs:169`): 16 tests. Covers: extension wins for both surfaces, neutral `.cssl` falls through, pragma `"csl"` and `"rust"` and full forms, pragma through leading comments, first-line `§` detection, first-line keyword detection (`fn`, `module`, `use`), prefix-word partial-match guard (`functional` ≠ `fn`), empty file default, all-comments default, ambiguous expression-line default, leading whitespace tolerance.

---

### 3.4 `cssl-lex/src/rust_hybrid.rs` (660 lines)

The logos-based Rust-hybrid surface lexer.

**`pub fn lex(source: &SourceFile) -> Vec<Token>`** (`rust_hybrid.rs:26`)  
Primary entry. Creates a logos `Lexer<RawToken>` over `source.contents`, iterates, and for each token: (a) computes start/end byte offsets as `u32` (saturating at `u32::MAX` for pathological inputs), (b) builds a `Span`, (c) calls `promote(raw, slice)` to convert `RawToken` → `TokenKind`, (d) pushes `Token::new(kind, span)`. After the loop, appends a `TokenKind::Eof` token at `source.len_bytes()`. Finally calls `fold_morpheme_suffixes`.

**`fn fold_morpheme_suffixes(source: &SourceFile, tokens: &mut Vec<Token>)`** (`rust_hybrid.rs:57`)  
Private post-pass. Scans the token stream with a 3-element lookahead. When it finds `Ident + Apostrophe + Ident(single-byte)` all adjacent (no whitespace gaps, checked via span adjacency), and the third token's single byte is a recognized morpheme letter (`TypeSuffix::from_letter`), it replaces the last two tokens with a single `Suffix(_)` token whose span covers the apostrophe through the letter. Non-matching triples pass through unchanged.

This is the implementation of T2-D5: "turn `base'd` into `Ident + Suffix(Data)` atomically." Multi-letter attachments like `f32'pos` are intentionally not folded (three tokens remain: `Ident + Apostrophe + Ident`). The condition `tokens[i+2].span.len() == 1` enforces the single-letter requirement.

**`fn promote(raw: RawToken, text: &str) -> TokenKind`** (`rust_hybrid.rs:94`)  
Private. Maps each `RawToken` variant to a `TokenKind`. For `RawToken::Ident`, calls `Keyword::from_word(text)` and emits `TokenKind::Keyword(_)` if recognized, else `TokenKind::Ident`. Both `CotLine` and `LineComment` → `TokenKind::LineComment`; both `CotBlock` and `BlockComment` → `TokenKind::BlockComment`.

**`enum RawToken`** (`rust_hybrid.rs:162`)  
Private logos-derived enum. `#[logos(skip r"[ \t\r]+")] ` — horizontal whitespace silently consumed. Variants (selected notable ones):

- `Ident` — `#[regex(r"[A-Za-z_][A-Za-z0-9_]*", priority = 2)]`. Priority 2 prevents keywords from being swallowed by the ident regex before logos can match them by exact token; keyword discrimination is done in `promote` rather than in logos.
- `FloatLiteral` — placed before `IntLiteral` in the enum to win the longest-match on `3.14`.
- `IntLiteral` — allows digit separators (`_`) and optional `'suffix`.
- `StringLiteral` — handles `\n \t \\ \"` escape sequences inside the regex.
- `RawStringLiteral` — `r#"..."#` form. Regex `r##"r#*"[^"]*"#*"##` is a simplification that doesn't correctly handle nested quote-hashes (e.g., `r##"has a " in it"##` with different hash counts) — this is a known limitation of using a regex for raw strings.
- `Apostrophe` — `priority = 0` so `CharLiteral` wins for well-formed `'c'`.
- Arrow forms: `->` and `→` in the same `#[token]` attribute; same pattern for other Unicode aliases.
- `CotBlock` — `§{ … §}` block CoT comment. The regex comment notes that logos lacks non-greedy quantifiers, so the body is expressed as `(?:[^§]|§[^}])*` to exclude the terminator.
- `CotLine` — `§[ \t]+(?:I>|W!|R!|M\?|N!|Q\?|P>|D>)[^\n]*`. Handles all 8 modal operators in CSLv3-native CoT line form.

**Test module** (`rust_hybrid.rs:323`): 20 tests. Covers: empty input, ident/keyword discrimination, integer vs float disambiguation, string flavors, arrow family (ASCII + Unicode), multi-char vs single-char comparison operators, all three bracket kinds, fn declaration shape, dot-family disambiguation (`..=` vs `..` vs `.`), line comment emission, CoT line forms, CoT block multiline, `@` attribute prefix, effect row punctuation (`/` and `{...}`), span offset exactness, Eof always appended, and then 8 tests specifically for the morpheme-suffix folding machinery (T2-D8): single-letter fold, rule letter fold, multi-letter non-fold, non-morpheme non-fold, whitespace-break non-fold, lifetime-like non-fold, integer suffix intact, char literal priority, multi-char non-fold, span coverage of folded suffix.

---

### 3.5 `cssl-lex/src/csl_native.rs` (1,082 lines)

The hand-rolled CSLv3-native surface lexer. Does not use logos. Implements the algorithm from `CSLv3/specs/12_TOKENIZER.csl` + `13_GRAMMAR_SELF.csl` directly as a byte-stream scan with indent tracking.

**`pub fn lex(source: &SourceFile) -> Vec<Token>`** (`csl_native.rs:53`)  
Thin public wrapper: creates a `Lexer` and calls `run()`.

**`struct Lexer<'a>`** (`csl_native.rs:61`)  
Private. Fields:
- `source_id: SourceId`
- `text: &'a str` — full source as a `str` reference (needed for `chars()` on multi-byte code points).
- `bytes: &'a [u8]` — the same source as bytes (for fast single-byte dispatch).
- `pos: usize` — current byte position.
- `indent_stack: Vec<u32>` — stack of leading-space counts. Initialized to `vec![0]` (sentinel). Grows on indent, shrinks on dedent.
- `bracket_depth: u32` — tracks nesting of `()`, `{}`, `[]`, and all six determinative pairs. When non-zero, newlines and indent changes are suppressed.
- `at_line_start: bool` — set after every emitted `Newline`; cleared at `lex_one` entry.
- `tokens: Vec<Token>` — accumulating output.

**`impl<'a> Lexer<'a>` methods:**

**`fn new(source: &'a SourceFile) -> Self`** (`csl_native.rs:73`): constructor.

**`fn run(mut self) -> Vec<Token>`** (`csl_native.rs:86`): main loop. Dispatches to `handle_line_start` when `at_line_start && bracket_depth == 0`, then `lex_one`. After the main loop, emits trailing `Dedent` tokens for any open indent levels, then `Eof`.

**`fn handle_line_start(&mut self)`** (`csl_native.rs:107`): counts leading spaces and tabs (tab = 4 spaces for tolerance, per the `§ I>` comment). Skips blank lines and lines starting with `#` (comment) without perturbing the indent stack. Compares the column count to the indent-stack top and emits `Indent`/`Dedent(s)` as appropriate. Uses `Ordering::Greater/Less/Equal` from `core::cmp`.

**`fn lex_one(&mut self)`** (`csl_native.rs:153`): primary character dispatch. Processes one token per call. Priority order:
1. Horizontal whitespace (`' '` / `'\t'`) — consumed silently.
2. Newline (`'\n'`) — emits `Newline` if `bracket_depth == 0`; sets `at_line_start`.
3. Carriage-return (`'\r'`) — consumed silently (CRLF tolerance).
4. Hash (`#`) — line comment (consumed to end of line; emits `LineComment`).
5. `try_ascii_evidence_alias()`.
6. `try_ascii_multichar()`.
7. `try_bracket()` — plain brackets + all six determinative Unicode pairs.
8. Digit — `lex_number()`.
9. Double-quote — `lex_string()`.
10. Letter or `_` — `lex_identifier()`.
11. Apostrophe — morpheme-suffix or standalone `Apostrophe`.
12. Single-char ASCII — `try_ascii_single()`.
13. High byte (≥ 0x80) — `try_unicode_glyph()`.
14. Fallback — `advance_one_char()` + `Error` token.

**`fn advance_one_char(&mut self)`** (`csl_native.rs:271`): advances `pos` by one UTF-8 code point (using `chars().next()` to determine length). Falls back to `pos += 1` for invalid UTF-8.

**`fn current_char(&self) -> Option<char>`** (`csl_native.rs:280`): peeks at the current code point without consuming.

**`fn emit(&mut self, kind, start, end)`** (`csl_native.rs:284`): pushes a `Token` with a new `Span`.

**`fn emit_empty(&mut self, kind)`** (`csl_native.rs:289`): pushes a zero-length token at current `pos`.

**`fn try_ascii_evidence_alias(&mut self) -> Option<TokenKind>`** (`csl_native.rs:297`): matches the 8 ASCII aliases for evidence marks (`[x]`, `[~]`, `[ ]`, `[!]`, `[?]`, `[^]`, `[v]`, `[!!]`) using byte slice prefix matching.

**`fn try_ascii_multichar(&mut self) -> Option<TokenKind>`** (`csl_native.rs:314`): matches modal operators (`W!`, `R!`, `M?`, `N!`, `I>`, `Q?`, `P>`, `D>`) with a word-boundary check (requires a non-alphanumeric byte before the operator). Then matches three-byte constructs (`..=`, `<->`), then a large `match` over all two-byte combos (arrows, comparison, logical, shift, path separator, dots, `??`, `<|`, `~>`). This is where `PipeArrowBack` (`<|`) and `SquigglyArrow` (`~>`) are handled — the Rust-hybrid logos lexer does not handle these two operators.

**`fn try_bracket(&mut self, b: u8) -> Option<TokenKind>`** (`csl_native.rs:376`): handles ASCII brackets `() {} []` with bracket-depth tracking. Unicode determinatives are handled in `try_unicode_glyph`.

**`fn try_ascii_single(b: u8) -> Option<TokenKind>`** (`csl_native.rs:408`): static dispatch over the remaining single-char ASCII operators.

**`fn try_unicode_glyph(&mut self) -> Option<TokenKind>`** (`csl_native.rs:434`): handles all multi-byte Unicode code points. Large match over approximately 40 characters: `§`/`§§`, `∎`, all 8 evidence marks, all dense math symbols, comparison/logic aliases (`≡ ≠ ≤ ≥ ∧ ∨ ¬`), arrows (`→ ← ↔ ⇒ ▷`), and all 12 determinative bracket characters (6 open, 6 close). Determinative open/close update `bracket_depth`.

**`fn lex_number(&mut self, start: usize)`** (`csl_native.rs:540`): consumes integer digits (with `_` separators). Checks for `<digits>.<digits>` float form. Optionally consumes a trailing `'<letter(s)>` type suffix. Emits `IntLiteral` or `FloatLiteral`.

**`fn lex_string(&mut self, start: usize)`** (`csl_native.rs:573`): consumes a normal `"..."` string (no raw-string support in CSLv3-native mode). Handles `\\` escape pairs. Emits `StringLiteral(StringFlavor::Normal)` on success, `Error` on unterminated string (newline before closing quote).

**`fn lex_identifier(&mut self, start: usize)`** (`csl_native.rs:602`): consumes `[A-Za-z_][A-Za-z0-9_]*`. After consuming the word, checks for special bareword identifiers:
- `"TODO"` → `Modal(ModalOp::Todo)`
- `"FIXME"` → `Modal(ModalOp::Fixme)`
- `"all"` → `ForAll`, `"any"` → `Exists`, `"in"` → `ElemOf`, `"nil"` → `EmptySet`, `"inf"` → `Infinity`, `"QED"` → `Qed`
- All others → `Ident`

Then checks for an immediate trailing `'<letter>` morpheme suffix (same single-letter, not-identifier-continuation check as the Rust-hybrid fold, but handled inline).

**Test module** (`csl_native.rs:653`): 27 tests. Covers: empty input, single and double `§`, QED, all 8 Unicode evidence marks, all 8 ASCII evidence alias forms, all 8 modal operators, bareword `TODO`/`FIXME`, modal-in-identifier boundary check, dense math quantifiers (`∀ ∃ ∈ ∉ ⊂ ⊃`), inference symbols (`∴ ∵ ⊢`), ASCII alias quantifiers (`all any in nil inf`), Unicode comparison aliases, full arrow family (ASCII + Unicode), all six determinative pairs, identifier with type suffix (single-letter), integer with suffix, float literal, normal string, string with escape, hash line comment, bahuvrīhi `⊗` compound op, indent/dedent single level, nested indent, blank-line no-perturb, bracket-suppresses-indent, full CSLv3-native fragment (sanity), span offset exactness, Eof always appended, unrecognized control char emits Error.

---

### 3.6 `cssl-lex/tests/integration.rs` (132 lines)

Integration tests running the full public `lex` API over real fixture files.

**`fn load_fixture(name: &str, surface: Surface) -> SourceFile`** (`integration.rs:10`): reads from `tests/fixtures/{name}` using `std::fs::read_to_string`. Panics with a descriptive message on failure.

**`fn tokenize(name: &str, surface: Surface) -> Vec<TokenKind>`** (`integration.rs:18`): convenience wrapper that calls `lex` and maps tokens to kinds only.

**Tests:**
- `rust_hybrid_fixture_tokenizes_without_errors` (`integration.rs:30`): checks no `Error` tokens, last is `Eof`.
- `rust_hybrid_fixture_has_expected_kinds` (`integration.rs:39`): checks for at least one keyword, `Arrow`, `At`, `Slash`, at least one `LineComment`. The `Slash` check verifies the effect-row separator (`/ {GPU, ...}`) tokenizes correctly.
- `csl_native_fixture_tokenizes_without_errors` (`integration.rs:63`): more detailed check — collects all `Error` tokens, maps each to its source slice + position via `SourceFile::slice` + `position_of`, and formats them for a meaningful failure message.
- `csl_native_fixture_has_expected_kinds` (`integration.rs:81`): checks for `Section`, at least one modal, at least one evidence mark, `ElemOf` (`∈`), `Indent`, `Dedent`, `Arrow` (`→`), `Qed` (`∎`).
- `auto_surface_dispatch_csl_via_extension` (`integration.rs:104`): `Surface::Auto` over `csl_native_basic.cssl-csl` → expects `Section`.
- `auto_surface_dispatch_rust_via_extension` (`integration.rs:109`): `Surface::Auto` over `rust_hybrid_basic.cssl-rust` → expects `Keyword(Fn)`.
- `differential_oracle_preflight_both_surfaces_terminate` (`integration.rs:126`): a named ship-gate stub for T6+. Currently just asserts both lexers terminate (last token is `Eof`). Comment explains the real oracle (comparing against `parser.exe --tokens`) is deferred until T10+ when `csslc tokens --json` is wired.

---

### 3.7 Fixture: `tests/fixtures/csl_native_basic.cssl-csl` (31 lines)

A representative CSLv3-native file covering the main token varieties:
- `§ sphere_sdf ≡ @differentiable ⊗ @lipschitz<1.0>` — section header with attribute annotations and compound `⊗`.
- Indented body with `I>` insight modal, `W!` must modals, `∈` element-of, `f32'pos` type with refinement tag suffix `'pos`.
- `→` arrow, `✓` and `‼` evidence marks.
- `§ render_scene ≡ @staged` — second section.
- `∈ {GPU, NoAlloc, Deadline<16ms>}` — set membership with angle-bracket parameterized effects.
- `▷ parallel` pipeline arrow.
- `§ effect Physics<World>` — effect declaration in CSLv3-native style.
- `§ invariants` — invariant declarations including `∀`, `∃`, `bwd_diff`, `◐` partial mark, `○` pending mark, `TODO` and `FIXME` barewords.
- `∎` — QED terminator.

This fixture exercises: Section, SectionRef-chain, EqEq (via `≡`), Compound (via `⊗`), At, Lt/Gt (via `<1.0>`), Indent/Dedent, Modal (I>, W!), ElemOf, Ident, Suffix (via `'pos`), Arrow, Evidence (Confirmed, Proven, Partial, Pending), Brace open/close, ForAll, Exists, Qed. Also exercises the `TODO`/`FIXME` bareword modal recognition.

---

### 3.8 Fixture: `tests/fixtures/rust_hybrid_basic.cssl-rust` (44 lines)

A representative Rust-hybrid file:
- `module com.apocky.demo` — module declaration.
- `use` statements with `::` paths and `{...}` import groups.
- `// § I> ...` — CoT-style line comment in Rust-hybrid file.
- `@differentiable`, `@lipschitz(k = 1.0)` — outer attributes.
- `fn sphere_sdf(p : vec3, r : f32) -> f32 { ... }` — typed function with `->` return arrow.
- `@staged fn render_scene<S : Scene>(cam : Camera) -> Image / {GPU, Deadline<16ms>, ...}` — staged function with generic, effect row via `/`.
- `for (x, y) in grid(cam.size) |> parallel { ... }` — `for`/`in` with pipeline `|>`.
- `let` bindings, `?:` try-default operator.
- `effect Physics<World> { fn ... }` — effect declaration in Rust-hybrid style.
- `struct Entity { ... }` and `enum Shape { ... }`.

Exercises: Keyword (Module, Use, Fn, For, In, Let, Return, Effect, Struct, Enum), Ident, Arrow, At, Slash, Comma, Colon, ColonColon, LBrace/RBrace, LBracket/RBracket, LParen/RParen, Lt/Gt (generics), PipeArrow (`|>`), IntLiteral, FloatLiteral, StringLiteral not directly but line comments, QuestionQuestion (via `?:`), LineComment (plain + CoT form).

---

## 4. CRATE NOTES

### 4.1 Test Coverage

**`cssl-ast`:** Well-covered for the primitive types (`SourceId`, `Surface`, `SourceFile`, `Span`, `Diagnostic`). The CST itself (`cst.rs`) has structural spot-checks but no parser-round-trip tests (the parser isn't in this crate). The CST tests verify construction and field access, not that the types correctly represent any syntactic invariant.

**`cssl-lex`:** Comprehensive. Each lexer module has inline unit tests (20 for Rust-hybrid, 27 for CSLv3-native, 16 for mode detection, 5 for dispatch). Integration tests exercise the fixture files end-to-end. The test naming convention is descriptive and the failure messages for CSLv3 fixture errors include source slice + position, which is genuinely useful.

### 4.2 Stubs, Deferred Work, and TODOs

The following markers appear in the code:

**`integration.rs:126`** — `differential_oracle_preflight_both_surfaces_terminate`: The comment explicitly marks this as a T6 ship-gate scaffold. The real differential oracle (comparing token streams against `parser.exe --tokens` from the `CSLv3` repo) is deferred until T10+ when `csslc tokens --json` is wired.

**`diagnostic.rs:4`** — `§ STATUS : T2 scaffold`: The `Diagnostic` type is explicitly flagged as missing `miette` integration (code labels, multi-span rendering) which is planned for T3.

**`cst.rs:19`** — `§ NON-GOAL (for T3 scope)`: Trivia preservation (comments, whitespace) is explicitly deferred. The formatter path will need a trivia-preserving layer in stage-1.

No `todo!()`, `unimplemented!()`, or `panic!("stub...")` calls appear anywhere in either crate. The deferred work is documented in comments rather than runtime traps, which is appropriate for a library-only layer that is never directly invoked at runtime in the current state.

### 4.3 Dead Code

No dead code found. `thiserror` is listed as a dependency in `cssl-lex/Cargo.toml` but does not appear to be used anywhere in the current source files of this crate. It may be a placeholder for a future `LexError` type derived with `thiserror`, or it may have been left over from an earlier design. This is the only apparent unused dependency.

### 4.4 Architectural Surprises and Notable Patterns

**Duplicate `CompoundOp`:** Both `cssl-ast::cst::CompoundOp` and `cssl-lex::token::CompoundOp` define the same 5 Sanskrit compound-formation operators with the same variant names. These are structurally identical but are separate types in separate crates. The `promote()` function in `rust_hybrid.rs` and the `try_unicode_glyph` / `try_ascii_multichar` functions in `csl_native.rs` produce `token::CompoundOp`; the parser (not in scope) would need to translate them to `cst::CompoundOp`. This duplication is a conscious consequence of the crate dependency direction (`cssl-lex` may not depend on `cssl-ast::cst` for token types — it only uses `SourceFile` + `Span`), but it is worth noting for maintainers: changes to the compound-op set must be made in two places.

**`PipeArrowBack` (`<|`) and `SquigglyArrow` (`~>`) gap in Rust-hybrid:** The `TokenKind` enum defines both, and `csl_native.rs` handles both in `try_ascii_multichar`. However, `rust_hybrid.rs` (the logos lexer) has no `#[token("<|")]` or `#[token("~>")]` annotations in `RawToken`. If these operators appear in a `.cssl-rust` file, they will be lexed as `Lt + Pipe` and `Tilde + Gt` respectively. The spec (`token.rs:158-160`) says these belong to "CSLv3-native" semantically, but the `TokenKind` enum makes them shared. This is a potential discrepancy unless the specification intends that only `.cssl-csl` files can use these operators.

**Raw string regex limitation:** The logos regex for `RawStringLiteral` (`r##"r#*"[^"]*"#*"##`) does not correctly handle cases like `r##"contains a " char"##` where the hash-count on both sides must match exactly. A Rust-native raw string requires the closing sequence to match the opening hash-count; the current regex allows mismatched counts to succeed. This is a low-risk limitation in stage-0 but should be noted.

**`auto` fallback not propagated:** The `lex` function in `lib.rs` runs mode detection when `Surface::Auto` but does not update the `SourceFile` with the chosen surface. Callers who care about which surface was selected must reconstruct the `SourceFile` or call `mode::detect` themselves. The code acknowledges this in a comment (`lib.rs:49-51`).

**Indent stack sentinel:** The CSLv3-native lexer initializes `indent_stack: vec![0]` — a sentinel zero that is never popped (the pop loop in `run()` only pops while `indent_stack.len() > 1`). This ensures `indent_stack.last().expect(...)` never panics, and that a file at column 0 always has a reference point.

**`position_of` column counting in bytes:** `SourceLocation::column` counts UTF-8 code units (bytes), not grapheme clusters. For ASCII-only code this is correct. For code containing multi-byte identifiers or CSLv3-native glyphs, the column number may be visually off by the byte-width of preceding characters. The doc comment at `source.rs:161` acknowledges this: "sufficient for diagnostic arrow alignment in monospace renderers." This is a deliberate approximation, not a bug.

### 4.5 Spec / README Divergences

No `README.md` files exist for either crate. The module-level doc comments reference the spec files (`specs/09_SYNTAX.csl`, `specs/16_DUAL_SURFACE.csl`, `CSLv3/specs/12_TOKENIZER.csl`, `CSLv3/specs/13_GRAMMAR_SELF.csl`) as the authoritative design sources.

No divergences from spec were found in the implemented features. The two deferred areas are explicitly scoped-out:
1. The `#![allow(clippy::large_enum_variant)]` on `cst.rs` implicitly acknowledges that boxing of large CST variants is deferred.
2. The `csl_native.rs` module doc header `§ DEFERRED-TO-LATER-TURN` section explicitly lists slot-template determinative parsing and morpheme-stack parsing as parser-layer concerns.

The `mode::detect_pragma` function scans up to 9 lines (index ≤ 8) but the spec doc comment says "first ~5 non-blank lines". The implementation is more permissive than the spec description — this is a minor discrepancy in the opposite direction of a spec violation (the implementation accepts pragmas on more lines than the prose suggests).

---

## 5. SUMMARY STATISTICS

| Metric | Value |
|--------|-------|
| Files audited | 11 (9 `.rs` + 2 fixture text files) |
| Cargo.toml files audited | 3 (2 crate + 1 workspace) |
| Functions/methods documented | 77 |
| Types (struct/enum/type alias) documented | 52 |
| Trait impls documented | 11 (`Display` × 6, `Default` × 2, `Iterator` × 1, `Logos` × 1, `Ord/PartialOrd` noted) |
| Test functions counted | ~108 (5 dispatch + 4 token + 16 mode + 20 rust-hybrid + 27 csl-native + 8 cst + 4 diagnostic + 7 span + 8 source + 7 integration) |
| Unimplemented/todo!/panic stubs | 0 |
| Unused dependencies | 1 (`thiserror` in `cssl-lex`) |
| Notable gaps | Differential oracle deferred (T6+); miette richness deferred (T3); `PipeArrowBack`/`SquigglyArrow` missing from Rust-hybrid logos lexer; raw-string regex does not validate hash-count symmetry; `CompoundOp` duplicated across two crates |

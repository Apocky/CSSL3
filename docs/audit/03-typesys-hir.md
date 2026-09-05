# Audit: cssl-hir — HIR Type System & Inference Engine

**Crate path**: `compiler-rs/crates/cssl-hir/`
**Audit date**: 2026-05-14
**Auditor**: Claude (Sonnet 4.6) — agent session mystifying-bardeen-dcb4d6
**Files audited**: 21 (Cargo.toml + 20 .rs source files)
**Items documented**: 210+ (fns, structs, enums, traits, impls, type aliases, consts)

---

## Pipeline Position

```
source → lex → parse → CST → [cssl-hir] → cssl-mir → cssl-lir
```

HIR = "typed, interned, resolved" IR. Two-phase internal split:
- **T3.3** (name resolution + lowering): DONE — `lower.rs`
- **T3.4** (type inference + checking): DONE — `infer.rs` + `typing.rs` + `unify.rs`

Feature passes embedded here: F1 autodiff legality (`ad_legality.rs`), F2 refinement obligation collection (`refinement.rs`), F4 staged consistency (`staged_check.rs`), F5 IFC flow (`ifc.rs`), F6 macro hygiene (`macro_hygiene.rs`). Capability checking: `cap_check.rs`.

---

## Cargo.toml

**Dependencies**:
- `cssl-ast` (workspace) — CST nodes, `Span`, `SourceFile`, `SourceId`
- `cssl-caps` (workspace) — `CapKind`, `AliasMatrix`, `coerce`, `LinearTracker`
- `lasso` — string interner (`Rodeo`, `Spur`, `Key`)
- `thiserror` — derive macros for error types

**Dev-dependencies**:
- `cssl-lex` (workspace) — for integration tests that lex real source
- `cssl-parse` (workspace) — for integration tests that parse real source

No LLVM. No proc-macro crates. Fully `#![forbid(unsafe_code)]`.

---

## src/lib.rs

**Role**: Crate root. Declares 20 `pub mod` submodules and a massive re-export block exposing the full public API.

**Key constant**:
```rust
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");
```

**Clippy suppressions** (12 total):
```rust
#![allow(clippy::too_many_arguments)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::single_match)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::type_complexity)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::enum_glob_use)]
```

Rationale in comment: inference pass readability; will tighten in Stage-1.

**Scope comment block** (verbatim):
```
// T3.3 DONE: HIR lowering, name resolution, attribute parsing
// T3.4 NEXT: type inference engine (typing.rs, unify.rs, infer.rs)
```
Note: T3.4 is now also DONE; scope comment is stale.

**Modules declared**: `arena`, `attr`, `cap_check`, `env`, `expr`, `item`, `lower`, `pat`, `resolve`, `stmt`, `symbol`, `ty` (core HIR), `typing`, `unify`, `infer` (inference engine), `ad_legality`, `refinement`, `ifc`, `staged_check`, `macro_hygiene` (feature passes).

---

## src/arena.rs

**Role**: Node identity infrastructure. Two opaque ID types + allocator.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirId` | `struct(u32)` | Node ID for any HIR node. `DUMMY = HirId(u32::MAX)`. Derives `Debug,Clone,Copy,PartialEq,Eq,Hash,Ord,PartialOrd`. |
| `HirId::DUMMY` | `const` | Sentinel value `HirId(u32::MAX)` for uninitialized / placeholder nodes. |
| `DefId` | `struct(u32)` | Definition-level node ID. `UNRESOLVED = DefId(u32::MAX)`. Same derives as `HirId`. |
| `DefId::UNRESOLVED` | `const` | Sentinel `DefId(u32::MAX)` for not-yet-resolved references. |
| `HirArena` | `struct` | Monotonic allocator. Fields: `hir_counter: u32`, `def_counter: u32`. |

### Functions / Methods

| Signature | Description |
|-----------|-------------|
| `HirArena::new() -> Self` | Initializes both counters to 0. |
| `HirArena::fresh_hir_id(&mut self) -> HirId` | Increments `hir_counter` via `saturating_add(1)`, returns `HirId(old_val)`. |
| `HirArena::fresh_def_id(&mut self) -> DefId` | Increments `def_counter` via `saturating_add(1)`, returns `DefId(old_val)`. |
| `HirArena::hir_count(&self) -> u32` | Returns current `hir_counter`. |
| `HirArena::def_count(&self) -> u32` | Returns current `def_counter`. |

**Tests**: 3 — `fresh_hir_id_monotonic`, `fresh_def_id_monotonic`, `dummy_sentinel`.

**Stubs / TODOs**: None.

---

## src/symbol.rs

**Role**: String interning. `Symbol` = cheap `Copy` handle; `Interner` wraps `lasso::Rodeo` in `RefCell` for `&self` intern.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `Symbol` | `struct(Spur)` | Interned identifier handle. `Copy+Hash+Ord`. `Display` shows `sym#N` (numeric, no interner access). |
| `Interner` | `struct` | Wraps `RefCell<Rodeo>`. `Default` derives. |

### Functions / Methods

| Signature | Description |
|-----------|-------------|
| `Symbol::spur(self) -> Spur` | Raw `Spur` accessor for lasso interop. `#[must_use]`, `const`. |
| `Interner::new() -> Self` | Alias for `Self::default()`. |
| `Interner::intern(&self, text: &str) -> Symbol` | `get_or_intern` via `borrow_mut`. |
| `Interner::intern_static(&self, text: &'static str) -> Symbol` | Zero-alloc on repeat for statics. |
| `Interner::resolve(&self, sym: Symbol) -> String` | Copies string out of `Rodeo`. Returns owned `String` (can't return `&str` through `RefCell`). |
| `Interner::len(&self) -> usize` | Count of distinct interned strings. |
| `Interner::is_empty(&self) -> bool` | True iff nothing interned yet. |
| `impl fmt::Display for Symbol` | Renders `sym#N`. Does NOT resolve — interner not available in `fmt`. |

**Tests**: 5 — stable symbol, distinct symbols, resolve, len/is_empty, static intern.

**Stubs / TODOs**:
- Comment in module doc: "Stage1 parallel compilation can upgrade to `ThreadedRodeo` when the Windows-GNU toolchain supports `parking_lot_core`'s `dlltool.exe` dependency". Deferred upgrade path documented.
- `Display` for `Symbol` uses numeric fallback — callers needing text must hold `&Interner`.

---

## src/ty.rs

**Role**: Syntactic HIR type representation — mirrors `cssl_ast::cst::Type` with paths resolved to `Symbol` sequences. Inference fills in `Infer` placeholders (T3.4).

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirType` | `struct` | Fields: `span: Span`, `id: HirId`, `kind: HirTypeKind`. |
| `HirTypeKind` | `enum` | 10 variants — shape of a HIR type. |
| `HirTypeKind::Path` | variant | `path: Vec<Symbol>`, `def: Option<DefId>`, `type_args: Vec<HirType>`. |
| `HirTypeKind::Tuple` | variant | `elems: Vec<HirType>`. Arity 0 = unit. |
| `HirTypeKind::Array` | variant | `elem: Box<HirType>`, `len: Box<HirExpr>`. |
| `HirTypeKind::Slice` | variant | `elem: Box<HirType>`. |
| `HirTypeKind::Reference` | variant | `mutable: bool`, `inner: Box<HirType>`. |
| `HirTypeKind::Capability` | variant | `cap: HirCapKind`, `inner: Box<HirType>`. Pony-6 wrapper. |
| `HirTypeKind::Function` | variant | `params: Vec<HirType>`, `return_ty: Box<HirType>`, `effect_row: Option<HirEffectRow>`. |
| `HirTypeKind::Refined` | variant | `base: Box<HirType>`, `kind: HirRefinementKind`. F2 syntactic node. |
| `HirTypeKind::Infer` | variant | `_` placeholder. Filled by T3.4 inference. |
| `HirTypeKind::Error` | variant | Error-recovery placeholder. |
| `HirCapKind` | `enum` | `Iso`, `Trn`, `Ref`, `Val`, `Box`, `Tag` — Pony-6 capability set. `Copy+PartialEq+Eq`. |
| `HirRefinementKind` | `enum` | 3 variants: `Tag{name:Symbol}`, `Predicate{binder:Symbol, predicate:Box<HirExpr>}`, `Lipschitz{bound:Box<HirExpr>}`. |
| `HirEffectRow` | `struct` | `span`, `effects: Vec<HirEffectAnnotation>`, `tail: Option<Symbol>` (polymorphic tail var). |
| `HirEffectAnnotation` | `struct` | `span`, `name: Vec<Symbol>`, `args: Vec<HirEffectArg>`. |
| `HirEffectArg` | `enum` | `Type(HirType)` or `Expr(HirExpr)`. |

**Tests**: 2 — `type_kind_variants_constructible`, `cap_kinds_enumerated`.

**Stubs / TODOs**: None explicit. `HirTypeKind::Infer` is a semantic placeholder, not a code stub.

---

## src/attr.rs

**Role**: HIR attribute representation for `#[outer]` and `#![inner]` attrs.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirAttrKind` | `enum` | `Outer` / `Inner`. |
| `HirAttr` | `struct` | `span`, `kind: HirAttrKind`, `path: Vec<Symbol>`, `args: Vec<HirAttrArg>`. |
| `HirAttrArg` | `enum` | `Positional(HirExpr)` or `Named{name:Symbol, value:HirExpr}`. |

### Functions / Methods

| Signature | Description |
|-----------|-------------|
| `HirAttr::is_simple(&self, target: Symbol) -> bool` | True iff single-segment path matches `target` AND no args. Used to test `#[attr]` without parentheses. |

**Tests**: 3 — outer/inner kinds, is_simple match, is_simple mismatch.

**Stubs / TODOs**: None.

---

## src/pat.rs

**Role**: HIR pattern representation.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirPattern` | `struct` | `span`, `id: HirId`, `kind: HirPatternKind`. |
| `HirPatternKind` | `enum` | 10 variants. |
| `HirPatternKind::Wildcard` | variant | `_` pattern. |
| `HirPatternKind::Literal` | variant | Literal value pattern. |
| `HirPatternKind::Binding` | variant | `name: Symbol`, `mutable: bool`, `subpat: Option<Box<HirPattern>>`. |
| `HirPatternKind::Tuple` | variant | `elems: Vec<HirPattern>`. |
| `HirPatternKind::Struct` | variant | `path: Vec<Symbol>`, `def: Option<DefId>`, `fields: Vec<HirPatternField>`. |
| `HirPatternKind::Variant` | variant | `path: Vec<Symbol>`, `def: Option<DefId>`, `subpat: Option<Box<HirPattern>>`. |
| `HirPatternKind::Or` | variant | `alts: Vec<HirPattern>`. |
| `HirPatternKind::Range` | variant | `lo: Box<HirPattern>`, `hi: Box<HirPattern>`, `inclusive: bool`. |
| `HirPatternKind::Ref` | variant | `mutable: bool`, `inner: Box<HirPattern>`. |
| `HirPatternKind::Error` | variant | Error-recovery. |
| `HirPatternField` | `struct` | `span`, `name: Symbol`, `pat: Option<HirPattern>`. `None` = shorthand `{x}`. |

**Tests**: 1 — wildcard/binding/tuple/error constructible.

**Stubs / TODOs**: None.

---

## src/stmt.rs

**Role**: HIR statement representation.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirStmt` | `struct` | `span`, `id: HirId`, `kind: HirStmtKind`. |
| `HirStmtKind` | `enum` | 3 variants. |
| `HirStmtKind::Let` | variant | `attrs: Vec<HirAttr>`, `pat: HirPattern`, `ty: Option<HirType>`, `value: Option<HirExpr>`. |
| `HirStmtKind::Expr` | variant | `HirExpr` — expression as statement. |
| `HirStmtKind::Item` | variant | `Box<HirItem>` — inline item declaration. |

**Tests**: 1 — let/expr/item variants constructible.

**Stubs / TODOs**: None.

---

## src/expr.rs

**Role**: HIR expression representation. Largest node type — 35+ variants including CSLv3-specific forms.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirExpr` | `struct` | `span`, `id: HirId`, `attrs: Vec<HirAttr>`, `kind: HirExprKind`. |
| `HirExprKind` | `enum` | 35+ variants. |
| `HirExprKind::Literal` | variant | `LitKind` from cssl-ast. |
| `HirExprKind::Path` | variant | `path: Vec<Symbol>`, `def: Option<DefId>`. |
| `HirExprKind::Block` | variant | `stmts: Vec<HirStmt>`, `tail: Option<Box<HirExpr>>`. |
| `HirExprKind::Call` | variant | `callee: Box<HirExpr>`, `type_args: Vec<HirType>`, `args: Vec<HirExpr>`. `type_args` added T11-D39 (turbofish). |
| `HirExprKind::MethodCall` | variant | `receiver: Box<HirExpr>`, `method: Symbol`, `type_args: Vec<HirType>`, `args: Vec<HirExpr>`. |
| `HirExprKind::Field` | variant | `base: Box<HirExpr>`, `name: Symbol`. |
| `HirExprKind::Index` | variant | `base: Box<HirExpr>`, `index: Box<HirExpr>`. |
| `HirExprKind::Binary` | variant | `op: HirBinOp`, `lhs: Box<HirExpr>`, `rhs: Box<HirExpr>`. |
| `HirExprKind::Unary` | variant | `op: HirUnOp`, `expr: Box<HirExpr>`. |
| `HirExprKind::Cast` | variant | `expr: Box<HirExpr>`, `ty: Box<HirType>`. |
| `HirExprKind::Assign` | variant | `lhs: Box<HirExpr>`, `rhs: Box<HirExpr>`. |
| `HirExprKind::AssignOp` | variant | `op: HirBinOp`, `lhs: Box<HirExpr>`, `rhs: Box<HirExpr>`. |
| `HirExprKind::If` | variant | `cond: Box<HirExpr>`, `then: Box<HirExpr>`, `else_: Option<Box<HirExpr>>`. |
| `HirExprKind::Match` | variant | `scrutinee: Box<HirExpr>`, `arms: Vec<HirMatchArm>`. |
| `HirExprKind::Loop` | variant | `body: Box<HirExpr>`. |
| `HirExprKind::While` | variant | `cond: Box<HirExpr>`, `body: Box<HirExpr>`. |
| `HirExprKind::For` | variant | `pat: HirPattern`, `iter: Box<HirExpr>`, `body: Box<HirExpr>`. |
| `HirExprKind::Break` | variant | `label: Option<Symbol>`, `value: Option<Box<HirExpr>>`. |
| `HirExprKind::Continue` | variant | `label: Option<Symbol>`. |
| `HirExprKind::Return` | variant | `value: Option<Box<HirExpr>>`. |
| `HirExprKind::Lambda` | variant | `params: Vec<HirParam>`, `return_ty: Option<Box<HirType>>`, `body: Box<HirExpr>`. |
| `HirExprKind::Tuple` | variant | `elems: Vec<HirExpr>`. |
| `HirExprKind::Array` | variant | `elems: Vec<HirExpr>`. |
| `HirExprKind::Range` | variant | `lo: Option<Box<HirExpr>>`, `hi: Option<Box<HirExpr>>`, `inclusive: bool`. |
| `HirExprKind::Struct` | variant | `path: Vec<Symbol>`, `def: Option<DefId>`, `fields: Vec<HirFieldInit>`, `spread: Option<Box<HirExpr>>`. |
| `HirExprKind::Try` | variant | `expr: Box<HirExpr>`. |
| `HirExprKind::Paren` | variant | `inner: Box<HirExpr>`. |
| `HirExprKind::Ref` | variant | `mutable: bool`, `expr: Box<HirExpr>`. |
| `HirExprKind::Deref` | variant | `expr: Box<HirExpr>`. |
| `HirExprKind::Perform` | variant | `path: Vec<Symbol>`, `def: Option<DefId>`, `args: Vec<HirExpr>`. F3 effect invocation. |
| `HirExprKind::With` | variant | `handler: Box<HirExpr>`, `body: Box<HirExpr>`. F3 handler installation. |
| `HirExprKind::Region` | variant | `label: Option<Symbol>`, `body: Box<HirExpr>`. |
| `HirExprKind::Run` | variant | `expr: Box<HirExpr>`. `#run` staged splice. F4. |
| `HirExprKind::Compound` | variant | `op: HirCompoundOp`, `lhs: Box<HirExpr>`, `rhs: Box<HirExpr>`. CSLv3 Sanskrit compound operators. |
| `HirExprKind::SectionRef` | variant | `path: Vec<Symbol>`. CSLv3 operator section reference. |
| `HirExprKind::Pipeline` | variant | `lhs: Box<HirExpr>`, `rhs: Box<HirExpr>`. `|>` operator. |
| `HirExprKind::TryDefault` | variant | `expr: Box<HirExpr>`, `default: Box<HirExpr>`. `?` with fallback. |
| `HirExprKind::Error` | variant | Error-recovery. |
| `HirBinOp` | `enum` | 20 variants: Add/Sub/Mul/Div/Rem/BitAnd/BitOr/BitXor/Shl/Shr/Eq/Ne/Lt/Le/Gt/Ge/And/Or/Implies/Entails. |
| `HirUnOp` | `enum` | 6 variants: Neg/Not/BitNot/Deref/AddrOf/AddrOfMut. |
| `HirCompoundOp` | `enum` | 5 CSLv3 compound ops: `Tp` (tatpuruṣa `.`), `Dv` (dvandva `+`), `Kd` (karmadhāraya `-`), `Bv` (bahuvrīhi `⊗`), `Av` (avyayībhāva `@`). |
| `HirMatchArm` | `struct` | `span`, `pat: HirPattern`, `guard: Option<HirExpr>`, `body: HirExpr`. |
| `HirFieldInit` | `struct` | `span`, `name: Symbol`, `value: Option<HirExpr>`. `None` = shorthand. |
| `HirParam` | `struct` | `span`, `pat: HirPattern`, `ty: Option<HirType>`, `attrs: Vec<HirAttr>`. |

**Tests**: 3 — `binary_ops_enumerated` (counts 20 ops), `compound_ops_enumerated` (5 CSLv3 ops), `expr_block_constructible`.

**Stubs / TODOs**: None in `expr.rs` itself. Inference stubs for several variants are in `infer.rs`.

---

## src/item.rs

**Role**: Top-level HIR item definitions — module, functions, structs, enums, effects, handlers, interfaces, etc.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `HirModule` | `struct` | `span`, `arena: HirArena`, `inner_attrs: Vec<HirAttr>`, `module_path: Vec<Symbol>`, `items: Vec<HirItem>`. |
| `HirItem` | `enum` | 11 variants. |
| `HirItem::Fn` | variant | `Box<HirFn>`. |
| `HirItem::Struct` | variant | `Box<HirStruct>`. |
| `HirItem::Enum` | variant | `Box<HirEnum>`. |
| `HirItem::TypeAlias` | variant | `Box<HirTypeAlias>`. |
| `HirItem::Const` | variant | `Box<HirConst>`. |
| `HirItem::Static` | variant | `Box<HirStatic>`. |
| `HirItem::Effect` | variant | `Box<HirEffect>`. F3 effect declaration. |
| `HirItem::Handler` | variant | `Box<HirHandler>`. F3 handler declaration. |
| `HirItem::Interface` | variant | `Box<HirInterface>`. Trait-like interface. |
| `HirItem::Impl` | variant | `Box<HirImpl>`. |
| `HirItem::Use` | variant | `Box<HirUse>`. |
| `HirFn` | `struct` | `span`, `id: DefId`, `name: Symbol`, `attrs: Vec<HirAttr>`, `generics: Vec<HirGenericParam>`, `params: Vec<HirParam>`, `return_ty: Option<HirType>`, `effect_row: Option<HirEffectRow>`, `body: Option<HirBlock>`. |
| `HirStruct` | `struct` | `span`, `id: DefId`, `name: Symbol`, `attrs`, `generics`, `fields: Vec<HirStructField>`. |
| `HirStructField` | `struct` | `span`, `name: Symbol`, `ty: HirType`, `attrs`. |
| `HirEnum` | `struct` | `span`, `id: DefId`, `name: Symbol`, `attrs`, `generics`, `variants: Vec<HirEnumVariant>`. |
| `HirEnumVariant` | `struct` | `span`, `id: DefId`, `name: Symbol`, `attrs`, `payload: HirVariantPayload`. |
| `HirVariantPayload` | `enum` | `Unit` / `Tuple(Vec<HirType>)` / `Struct(Vec<HirStructField>)`. |
| `HirTypeAlias` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `generics`, `ty: HirType`. |
| `HirConst` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `ty: Option<HirType>`, `value: HirExpr`. |
| `HirStatic` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `mutable: bool`, `ty: Option<HirType>`, `value: HirExpr`. |
| `HirEffect` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `generics`, `operations: Vec<HirEffectOp>`. F3. |
| `HirEffectOp` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `params: Vec<HirParam>`, `return_ty: Option<HirType>`. |
| `HirHandler` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `generics`, `effect_path`, `operations: Vec<HirHandlerOp>`, `return_clause: Option<HirBlock>`. |
| `HirHandlerOp` | `struct` | `span`, `name`, `params`, `return_ty`, `body: HirBlock`. |
| `HirInterface` | `struct` | `span`, `id: DefId`, `name`, `attrs`, `generics`, `items: Vec<HirInterfaceItem>`. |
| `HirInterfaceItem` | `enum` | `Fn(HirFn)` / `Const(HirConst)`. |
| `HirImpl` | `struct` | `span`, `attrs`, `generics`, `interface: Option<HirType>`, `self_ty: HirType`, `items: Vec<HirImplItem>`. Note: no `DefId`. |
| `HirImplItem` | `enum` | `Fn(HirFn)` / `Const(HirConst)`. |
| `HirUse` | `struct` | `span`, `attrs`, `tree: HirUseTree`. Note: no `DefId`. |
| `HirUseTree` | `enum` | `Single{path,alias}` / `Glob{path}` / `Nested{path,children}`. |
| `HirGenericParam` | `struct` | `span`, `name: Symbol`, `bounds: Vec<HirType>`. |
| `HirBlock` | `struct` | `span`, `id: HirId`, `stmts: Vec<HirStmt>`, `tail: Option<Box<HirExpr>>`. |

### Functions / Methods

| Signature | Description |
|-----------|-------------|
| `HirItem::span(&self) -> Span` | Dispatches to inner item's span field. All 11 variants covered. |
| `HirItem::def_id(&self) -> Option<DefId>` | Returns `Some(id)` for all named items; `None` for `Impl` and `Use` (no definition identity). |
| `HirItem::name(&self) -> Option<Symbol>` | Returns `Some(name)` for named items; `None` for `Impl` and `Use`. |

**Tests**: 3 — `hir_item_span_accessible`, `def_id_returns_none_for_impl_and_use`, `name_for_named_items`.

**Stubs / TODOs**: None.

---

## src/env.rs

**Role**: Typing environment. Scoped binding stacks for local variables + item signature registry.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `TypeScope` | `struct` | `bindings: HashMap<Symbol, Scheme>`. Single lexical scope frame. |
| `TypingEnv` | `struct` | `stack: Vec<TypeScope>`, `item_sigs: HashMap<DefId, Scheme>`, `item_names: HashMap<Symbol, DefId>`. Full typing environment with scope stack + global item registry. |

### Functions / Methods (TypeScope)

| Signature | Description |
|-----------|-------------|
| `TypeScope::new() -> Self` | Empty scope. |
| `TypeScope::insert(&mut self, name: Symbol, ty: Ty) -> Option<Ty>` | Inserts monomorphic binding (wraps `ty` in monomorphic `Scheme`). Returns previous if shadowed. |
| `TypeScope::insert_scheme(&mut self, name: Symbol, scheme: Scheme)` | Inserts polymorphic scheme directly. |
| `TypeScope::lookup(&self, name: Symbol) -> Option<&Ty>` | Looks up monomorphic binding (returns `None` for poly schemes). |
| `TypeScope::lookup_scheme(&self, name: Symbol) -> Option<&Scheme>` | Looks up any binding as scheme. |
| `TypeScope::schemes(&self) -> impl Iterator<Item=(&Symbol, &Scheme)>` | Iterates all bindings as schemes. |
| `TypeScope::len(&self) -> usize` | Count of bindings. |
| `TypeScope::is_empty(&self) -> bool` | True iff no bindings. |

### Functions / Methods (TypingEnv)

| Signature | Description |
|-----------|-------------|
| `TypingEnv::new() -> Self` | Empty env with empty stack and item registries. |
| `TypingEnv::enter_scope(&mut self)` | Pushes new empty `TypeScope` onto stack. |
| `TypingEnv::leave_scope(&mut self)` | Pops top scope. Panics if stack empty. |
| `TypingEnv::depth(&self) -> usize` | Current stack depth. |
| `TypingEnv::insert_local(&mut self, name: Symbol, ty: Ty) -> Option<Ty>` | Inserts monomorphic binding in innermost scope. Panics if stack empty. |
| `TypingEnv::insert_local_scheme(&mut self, name: Symbol, scheme: Scheme)` | Inserts polymorphic scheme in innermost scope. |
| `TypingEnv::lookup_local(&self, name: Symbol) -> Option<&Ty>` | Searches stack top→bottom for monomorphic binding. |
| `TypingEnv::lookup_local_scheme(&self, name: Symbol) -> Option<&Scheme>` | Searches stack top→bottom for scheme. |
| `TypingEnv::free_ty_vars(&self) -> HashSet<TyVar>` | Collects all free type variables across all scopes. Used by `generalize`. |
| `TypingEnv::free_row_vars(&self) -> HashSet<RowVar>` | Collects all free row variables across all scopes. |
| `TypingEnv::register_item(&mut self, name: Symbol, id: DefId)` | Registers item name → DefId mapping. |
| `TypingEnv::register_item_scheme(&mut self, id: DefId, scheme: Scheme)` | Stores item's polymorphic type scheme. |
| `TypingEnv::item_sig(&self, id: DefId) -> Option<&Ty>` | Returns monomorphic type if scheme has no quantified vars. |
| `TypingEnv::item_scheme(&self, id: DefId) -> Option<&Scheme>` | Returns scheme for any item. |
| `TypingEnv::item_def(&self, name: Symbol) -> Option<DefId>` | Resolves name to DefId. |
| `TypingEnv::lookup(&self, name: Symbol) -> Option<&Ty>` | Unified lookup: local stack first, then item_sigs. |
| `TypingEnv::item_sigs(&self) -> impl Iterator<Item=(&DefId, &Scheme)>` | Iterates all item schemes. |
| `TypingEnv::item_schemes(&self) -> impl Iterator<Item=(&DefId, &Scheme)>` | Alias for `item_sigs`. |

**Tests**: 4 — scope push/pop, local shadowing, generalize-at-let integration, item registration.

**Stubs / TODOs**: None.

---

## src/resolve.rs

**Role**: Name resolution infrastructure. Scoped symbol→DefId mapping.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `Scope` | `struct` | `bindings: HashMap<Symbol, DefId>`. Single resolution scope frame. |
| `ScopeMap` | `struct` | `module: Scope` (module-level, always present), `stack: Vec<Scope>` (lexical stack). |

### Functions / Methods (Scope)

| Signature | Description |
|-----------|-------------|
| `Scope::new() -> Self` | Empty scope. |
| `Scope::insert(&mut self, name: Symbol, id: DefId) -> Option<DefId>` | Inserts; returns previous if shadowed. |
| `Scope::lookup(&self, name: Symbol) -> Option<DefId>` | Single-scope lookup. |

### Functions / Methods (ScopeMap)

| Signature | Description |
|-----------|-------------|
| `ScopeMap::new() -> Self` | Empty module scope + empty stack. |
| `ScopeMap::enter_scope(&mut self)` | Pushes new lexical scope. |
| `ScopeMap::leave_scope(&mut self)` | Pops lexical scope. Panics if stack empty. |
| `ScopeMap::insert_module(&mut self, name: Symbol, id: DefId)` | **Always writes to module scope regardless of stack depth**. Design decision: module items always module-scoped. |
| `ScopeMap::insert_local(&mut self, name: Symbol, id: DefId) -> Option<DefId>` | Writes to innermost lexical scope. Panics if stack empty. |
| `ScopeMap::resolve_single(&self, name: Symbol) -> Option<DefId>` | Searches lexical stack top→bottom, then module scope. Single-segment only. |
| `ScopeMap::resolve_path(&self, path: &[Symbol]) -> Option<DefId>` | Multi-segment path resolution. **Stage-0 stub**: only handles single-segment; returns `None` for any path with 2+ segments. Comment: "multi-segment path resolution deferred to T3.4". |

**Tests**: 4 — module insert, local insert/shadow, single-segment resolve, path-resolution returns None for multi-segment.

**Stubs / TODOs**:
- `resolve_path` with `path.len() > 1` → always returns `None`. Comment: "multi-segment path resolution deferred to T3.4". **This means all `module::Type` style paths are unresolved.**

---

## src/typing.rs

**Role**: Core type algebra. `Ty` enum, inference variables, substitution, row types, schemes, HM generalization.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `TyVar` | `struct(u32)` | Type inference variable. `Copy+Hash+Eq`. |
| `RowVar` | `struct(u32)` | Row inference variable for effect rows. `Copy+Hash+Eq`. |
| `Ty` | `enum` | 13 semantic type variants (inference-level, distinct from `HirType`). |
| `Ty::Int` | variant | Integer type. |
| `Ty::Float` | variant | Float type. |
| `Ty::Bool` | variant | Boolean. |
| `Ty::Str` | variant | String. |
| `Ty::Unit` | variant | Unit `()`. |
| `Ty::Never` | variant | Bottom type `!`. |
| `Ty::Named` | variant | `def: DefId`, `args: Vec<Ty>`. Resolved nominal type. |
| `Ty::Tuple` | variant | `elems: Vec<Ty>`. |
| `Ty::Ref` | variant | `mutable: bool`, `inner: Box<Ty>`. |
| `Ty::Fn` | variant | `params: Vec<Ty>`, `return_ty: Box<Ty>`, `effect_row: Row`. Koka-style with row. |
| `Ty::Var` | variant | `TyVar` — unification variable. |
| `Ty::Param` | variant | `Symbol` — bound type parameter (skolem). |
| `Ty::Array` | variant | `elem: Box<Ty>`, `len: ArrayLen`. |
| `Ty::Slice` | variant | `elem: Box<Ty>`. |
| `Ty::Error` | variant | Error recovery. |
| `ArrayLen` | `enum` | `Literal(u64)` / `Opaque` / `Var(u32)`. |
| `EffectInstance` | `struct` | `name: Vec<Symbol>`, `args: Vec<Ty>`. Single effect in a row. |
| `Row` | `struct` | `effects: Vec<EffectInstance>`, `tail: Option<RowVar>`. Effect row. |
| `Subst` | `struct` | `ty_vars: HashMap<TyVar,Ty>`, `row_vars: HashMap<RowVar,Row>`. Substitution map. |
| `TyCtx` | `struct` | `ty_counter: u32`, `row_counter: u32`. Fresh variable allocator. |
| `TypeMap` | `struct` | `types: BTreeMap<u32,Ty>`. HirId → Ty recording (final output of inference). |
| `Scheme` | `struct` | `ty_vars: Vec<TyVar>`, `row_vars: Vec<RowVar>`, `body: Ty`. Rank-1 polymorphic type scheme. |

### Functions / Methods

| Signature | Description |
|-----------|-------------|
| `Row::pure() -> Row` | Empty closed row (no effects, no tail). |
| `Row::closed(effects) -> Row` | Effects with no polymorphic tail. |
| `Row::is_pure(&self) -> bool` | True iff no effects and no tail. |
| `Row::canonicalize(&mut self)` | Sorts effects by name for deterministic comparison. |
| `Subst::new() -> Self` | Empty substitution. |
| `Subst::apply(&self, ty: &Ty) -> Ty` | Recursively applies substitution to type, following chains (Var→Var→Concrete). |
| `Subst::apply_row(&self, row: &Row) -> Row` | Applies substitution to row, expanding tails if bound. |
| `Subst::bind_ty(&mut self, var: TyVar, ty: Ty)` | Binds `var → ty` in substitution. |
| `Subst::bind_row(&mut self, var: RowVar, row: Row)` | Binds `var → row`. |
| `TyCtx::new() -> Self` | Counters at 0. |
| `TyCtx::fresh_ty_var(&mut self) -> TyVar` | Fresh `TyVar(n)`. |
| `TyCtx::fresh_row_var(&mut self) -> RowVar` | Fresh `RowVar(n)`. |
| `TypeMap::new() -> Self` | Empty map. |
| `TypeMap::record(&mut self, id: HirId, ty: Ty)` | Records type for node. |
| `TypeMap::get(&self, id: HirId) -> Option<&Ty>` | Retrieves recorded type. |
| `TypeMap::len(&self) -> usize` | Count of recorded types. |
| `Scheme::monomorphic(ty: Ty) -> Self` | Scheme with no quantified vars. |
| `Scheme::instantiate(&self, tcx: &mut TyCtx) -> Ty` | Substitutes fresh vars for all quantified vars, returns `body` with substitution applied. HM instantiation rule. |
| `free_ty_vars(ty: &Ty) -> HashSet<TyVar>` | Free type variables in a type (pub fn). |
| `free_row_vars(ty: &Ty) -> HashSet<RowVar>` | Free row variables in a type (pub fn). |
| `generalize(ty: &Ty, env: &TypingEnv, subst: &Subst) -> Scheme` | HM generalization: quantifies free vars in `ty` not free in `env`. Returns `Scheme`. |

**Tests**: 20+ — subst apply, chain following, row canonicalize, scheme instantiation, generalize, free-var collectors.

**Stubs / TODOs**:
- `Ty` has no `Capability` variant — capabilities are stripped during `lower_hir_type` in `infer.rs`. Design decision, not oversight. The semantic `Ty` is capability-erased.

---

## src/unify.rs

**Role**: Hindley-Milner unification engine with Remy-style row unification for effect rows.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `UnifyError` | `enum` | 4 variants. Derives `thiserror::Error`. |
| `UnifyError::Mismatch` | variant | `{ a: Ty, b: Ty }` — structural mismatch. |
| `UnifyError::Arity` | variant | `{ expected: usize, got: usize }` — wrong argument count. |
| `UnifyError::OccursCheck` | variant | `{ var: TyVar, ty: Ty }` — infinite type. |
| `UnifyError::RowMismatch` | variant | `{ a: Row, b: Row }` — effect row incompatibility. |

### Functions / Methods

| Signature | Description |
|-----------|-------------|
| `unify(a: &Ty, b: &Ty, subst: &mut Subst) -> Result<(), UnifyError>` | **Public entry.** Applies substitution to both types, then calls `unify_step`. |
| `unify_step(a: Ty, b: Ty, subst: &mut Subst) -> Result<(), UnifyError>` | **Private.** Core structural dispatch. Key rules: `Never` and `Error` unify with anything. `Param` (skolem) must match exactly by symbol. `Var` binds via `bind_ty_var`. `Named` requires same `DefId` and arity. `Fn` unifies params+return+rows. All others matched structurally. |
| `bind_ty_var(var: TyVar, ty: Ty, subst: &mut Subst) -> Result<(), UnifyError>` | **Private.** Applies subst to `ty`, then: if `ty == Var(var)` → ok (reflexive). Else occurs check → `OccursCheck` error. Else binds. |
| `occurs_in(var: TyVar, ty: &Ty) -> bool` | **Private.** Recursive occurs check. |
| `unify_rows(a: &Row, b: &Row, subst: &mut Subst) -> Result<(), UnifyError>` | **Public.** Calls `unify_rows_step` after applying subst. |
| `unify_rows_step(a: Row, b: Row, subst: &mut Subst) -> Result<(), UnifyError>` | **Private.** Remy-style row unification: compute intersection (unify same-named effects), symmetric difference (extras must be absorbed by opposite tail via `absorb`). |
| `absorb(extras: Vec<EffectInstance>, tail: Option<RowVar>, subst: &mut Subst) -> Result<(), UnifyError>` | **Private.** If `tail = Some(v)`: bind `v → Row::closed(extras)`. If `tail = None` and extras non-empty: `RowMismatch`. |

**Tests**: 13 — primitive unify, mismatch, occurs check, var binding, Never-unifies-all, row unify intersection, row absorb, RowMismatch closed.

**Stubs / TODOs**: None.

---

## src/lower.rs

**Role**: CST → HIR lowering pass + inline name resolution (single-segment). Entry: `lower_module`.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `LowerCtx<'a>` | `struct` | `interner: Interner`, `arena: HirArena`, `source: &'a SourceFile`, `diagnostics: DiagnosticBag`. Private lowering context. |

### Private Methods (LowerCtx)

| Signature | Description |
|-----------|-------------|
| `hir_id(&mut self) -> HirId` | Allocates fresh `HirId` via arena. |
| `def_id(&mut self) -> DefId` | Allocates fresh `DefId` via arena. |
| `intern_ident(&self, span: Span) -> Symbol` | Slices source text at span, interns via `Interner`. |
| `intern_path(&self, path: &CstPath) -> Vec<Symbol>` | Interns each path segment. |
| `lower_type(&mut self, ty: &CstType) -> HirType` | 1:1 CST→HIR type lowering. All 10 `HirTypeKind` variants populated. |
| `lower_expr(&mut self, expr: &CstExpr) -> HirExpr` | **Pub.** 1:1 CST→HIR expression lowering. All 35+ variants. |
| `lower_stmt(&mut self, stmt: &CstStmt) -> HirStmt` | Lowers a statement. |
| `lower_block(&mut self, block: &CstBlock) -> HirBlock` | Lowers a block (stmts + optional tail). |
| `lower_pattern(&mut self, pat: &CstPattern) -> HirPattern` | Lowers a pattern. |
| `lower_item(&mut self, item: &CstItem) -> HirItem` | Dispatches to per-item lower methods. |
| `lower_fn(&mut self, f: &CstFn) -> HirFn` | Lowers function including generics, params, effect row, body. |
| `lower_struct(&mut self, s: &CstStruct) -> HirStruct` | Lowers struct definition. |
| `lower_enum(&mut self, e: &CstEnum) -> HirEnum` | Lowers enum with all variant shapes. |
| `lower_type_alias(&mut self, a: &CstTypeAlias) -> HirTypeAlias` | Lowers type alias. |
| `lower_const(&mut self, c: &CstConst) -> HirConst` | Lowers const item. |
| `lower_static(&mut self, s: &CstStatic) -> HirStatic` | Lowers static item. |
| `lower_effect(&mut self, e: &CstEffect) -> HirEffect` | Lowers effect declaration (F3). |
| `lower_handler(&mut self, h: &CstHandler) -> HirHandler` | Lowers handler declaration (F3). |
| `lower_interface(&mut self, i: &CstInterface) -> HirInterface` | Lowers interface declaration. |
| `lower_impl(&mut self, i: &CstImpl) -> HirImpl` | Lowers impl block. |
| `lower_use(&mut self, u: &CstUse) -> HirUse` | Lowers use declaration. |
| `lower_attr(&mut self, a: &CstAttr) -> HirAttr` | Lowers attribute. |
| `lower_generic_param(&mut self, g: &CstGenericParam) -> HirGenericParam` | Lowers generic parameter. |
| `lower_param(&mut self, p: &CstParam) -> HirParam` | Lowers function parameter. |
| `lower_effect_row(&mut self, r: &CstEffectRow) -> HirEffectRow` | Lowers effect row annotation. |
| `lower_effect_annotation(&mut self, a: &CstEffectAnnotation) -> HirEffectAnnotation` | Lowers single effect annotation. |
| `lower_refinement(&mut self, r: &CstRefinement) -> HirRefinementKind` | Lowers refinement kind (Tag/Predicate/Lipschitz). |

### Name Resolution (inline in lower.rs)

| Signature | Description |
|-----------|-------------|
| `resolve_module(module: &mut HirModule)` | Top-level resolution driver. Calls `build_module_scope` then `resolve_item_refs`. |
| `build_module_scope(module: &HirModule) -> ScopeMap` | Scans top-level items + enum variants to populate module scope. |
| `resolve_item_refs(items: &mut Vec<HirItem>, scopes: &mut ScopeMap)` | Recursive item walker. Enters/leaves scopes at fn/impl boundaries. |
| `resolve_expr(expr: &mut HirExpr, scopes: &ScopeMap)` | Fills `def: None → Some(DefId)` for single-segment `Path` expressions. Multi-segment left as `None`. |
| `resolve_type(ty: &mut HirType, scopes: &ScopeMap)` | Fills `def` for `HirTypeKind::Path` single-segment references. |
| `resolve_pattern(pat: &mut HirPattern, scopes: &ScopeMap)` | Fills `def` for `Struct`/`Variant` pattern paths. |

### Public Entry

| Signature | Description |
|-----------|-------------|
| `pub fn lower_module(source: &SourceFile, module: &CstModule) -> (HirModule, Interner, DiagnosticBag)` | Full lowering + inline resolution. Returns owned `HirModule`, the `Interner` (for downstream passes), and any lowering diagnostics. |

**Tests**: 7 — basic lower, effect items, handler items, turbofish type_args (`hir_call_type_args_populated_from_turbofish` T11-D39), refinement lowering, effect row lowering, resolve single-segment paths.

**Stubs / TODOs**:
- Multi-segment path `def` resolution left as `None` — single-segment only in Stage-0 (mirrors `resolve.rs`).

---

## src/infer.rs

**Role**: Type inference and checking engine. Two-phase: Phase 1 = collect item signatures (register `Scheme`s for all top-level defs), Phase 2 = check item bodies (synth + unify), Phase 3 = finalize (apply subst to all recorded types).

### Types

| Item | Kind | Description |
|------|------|-------------|
| `InferCtx<'a>` | `struct` | Full inference context. Fields: `interner: &'a Interner`, `tcx: TyCtx`, `subst: Subst`, `env: TypingEnv`, `type_map: TypeMap`, `diagnostics: Vec<Diagnostic>`, `current_row: Option<Row>` (enclosing fn's effect row), `current_return: Option<Ty>` (enclosing fn's return type), `generics_map: HashMap<Symbol,TyVar>` (current fn's generic params). |

### Private Methods (InferCtx) — Phase 1: Signature Collection

| Signature | Description |
|-----------|-------------|
| `collect_item_signatures(&mut self, items: &[HirItem])` | Iterates items, dispatches to `collect_item`. |
| `collect_item(&mut self, item: &HirItem)` | Dispatches to per-item collectors: Fn/Const/Struct/Enum/TypeAlias/Effect/Handler/Interface. Skips Impl/Use/Static. |
| `fn_signature_scheme(&mut self, f: &HirFn) -> Scheme` | Builds `Scheme` for fn: allocates fresh `TyVar` per generic param (populates `generics_map`), lowers param/return types via `lower_hir_type`, builds `Ty::Fn`. **Implements T3-D17 generic fn schemes.** |
| `collect_fn_sig(&mut self, f: &HirFn)` | Calls `fn_signature_scheme`, registers in `env`. |
| `collect_const_sig(&mut self, c: &HirConst)` | Lowers const's declared type or allocates fresh var. |
| `collect_struct_sig(&mut self, s: &HirStruct)` | Registers `Named{def_id, args=[]}` scheme. |
| `collect_enum_sig(&mut self, e: &HirEnum)` | Registers enum and all variant constructor schemes. |
| `collect_type_alias_sig(&mut self, a: &HirTypeAlias)` | Lowers alias body type, registers. |
| `collect_effect_sig(&mut self, e: &HirEffect)` | Registers effect + operation schemes. |
| `collect_handler_sig(&mut self, h: &HirHandler)` | Registers handler scheme. |
| `collect_interface_sig(&mut self, i: &HirInterface)` | Registers interface + item schemes. |

### Private Methods (InferCtx) — Phase 2: Body Checking

| Signature | Description |
|-----------|-------------|
| `check_items(&mut self, items: &[HirItem])` | Iterates items, dispatches to `check_item`. |
| `check_item(&mut self, item: &HirItem)` | Dispatches to `check_fn` for fns, `check_const` for consts. Skips structural items (Struct/Enum/etc. — no body to check). |
| `check_fn(&mut self, f: &HirFn)` | Enters scope. Binds generic params to `Ty::Param`. Binds regular params via `bind_pattern`. Sets `current_row`/`current_return` from fn signature. Synthesizes body block. Unifies synthesized return with declared return. Leaves scope. |
| `check_const(&mut self, c: &HirConst)` | Synthesizes const value expr, unifies with declared type. |
| `synth_expr(&mut self, expr: &HirExpr) -> Ty` | Records synthesized type in `type_map`, dispatches to `synth_expr_kind`. |
| `synth_expr_kind(&mut self, expr: &HirExpr) -> Ty` | **Core inference dispatcher — 30+ arms.** |
| `synth_block(&mut self, block: &HirBlock) -> Ty` | Enters scope, synths stmts, tail or unit. |
| `synth_stmt(&mut self, stmt: &HirStmt)` | Let: synths value, `bind_pattern_let` for generalization. Expr: synths. Item: collect+check inline item. |
| `bind_pattern(&mut self, pat: &HirPattern, ty: Ty)` | Monomorphic binding — binds names in pattern to given type (no generalization). |
| `bind_pattern_let(&mut self, pat: &HirPattern, ty: Ty, subst: &Subst)` | **Let generalization boundary (T3-D15).** Applies subst, calls `generalize`, inserts `Scheme` via `insert_local_scheme`. |
| `lower_hir_type(&self, ty: &HirType) -> Ty` | Converts `HirType` → `Ty`. `Capability` variants stripped (cap-erased `Ty`). `Infer` → fresh `TyVar`. `Path` with single-segment: looks up generics_map first, then env. |
| `lower_effect_row_opt(&self, row: &Option<HirEffectRow>) -> Row` | Converts optional `HirEffectRow` → `Row`. |
| `lower_effect_row(&self, row: &HirEffectRow) -> Row` | Converts `HirEffectRow` → `Row`. |
| `unify_types(&mut self, a: Ty, b: Ty, span: Span)` | Calls `unify::unify`, emits diagnostic on error. |
| `apply_subst_ty(&self, ty: Ty) -> Ty` | Applies current `subst` to type. |
| `fresh_ty_var(&mut self) -> Ty` | Allocates `Ty::Var(TyCtx::fresh_ty_var())`. |

### synth_expr_kind Arms (notable subset)

| Arm | Behavior | Status |
|-----|----------|--------|
| `Literal(Int/Float/Bool/Str)` | Returns matching `Ty::Int/Float/Bool/Str`. | Real |
| `Path` | Looks up in env by name; if scheme found, instantiates. Else fresh var + diagnostic. | Real |
| `Call` | Synths callee, expects `Ty::Fn`; unifies args; returns return_ty. Checks effect_row against `current_row`. | Real |
| `Binary` | Arithmetic → `Ty::Int`/`Float`; bool ops → `Bool`; comparison → `Bool`; `Implies`/`Entails` → `Bool`. | Real |
| `Unary` | Neg → same type; Not/BitNot → `Bool`; Deref → inner; AddrOf/AddrOfMut → `Ty::Ref`. | Real |
| `If` | Unifies cond with Bool; unifies then/else branches. | Real |
| `Block` | Delegates to `synth_block`. | Real |
| `Lambda` | Allocates fresh param vars, binds, synths body. | Real |
| `Return` | Synths value, unifies with `current_return`. | Real |
| `Field` | **STUB**: Returns fresh var. No field resolution. | **STUB** |
| `Try` | **STUB**: Returns fresh var. No `Result`/`Option` handling. | **STUB** |
| `Compound` | **STUB**: Returns fresh var. CSLv3 compound ops not type-checked. | **STUB** |
| `Run` | Passes through inner expr type. | Real (passthrough) |
| `Perform` | Checks `current_row` contains effect; returns fresh var (no operation return type resolution). | Partial |
| `With` | Synths handler + body; returns body type. | Partial |
| `Cast` | Returns target type. | Real |
| `Struct` | Looks up struct type, returns `Named`. Field values checked against fresh vars. | Partial |
| `Tuple` | Synths all elems, returns `Ty::Tuple`. | Real |
| `Array` | Synths all elems, unifies pairwise, returns `Ty::Array`. | Real |
| `Match` | Synths scrutinee, synths arms, unifies arm types. | Real |

**Phase 3: Finalization**

| Signature | Description |
|-----------|-------------|
| `finalize(&mut self)` | Applies final `subst` to all entries in `type_map`. |

### Public Entry

| Signature | Description |
|-----------|-------------|
| `pub fn check_module(module: &HirModule, interner: &Interner) -> (TypeMap, Vec<Diagnostic>)` | Phase 1 → Phase 2 → Phase 3 → return `(TypeMap, diagnostics)`. |

**Tests**: 20+ — let generalization integration tests (T3-D15/D17 verification), scheme instantiation, effect row checking, basic inference for arithmetic/bool/string/path/call/if/match/lambda, `bind_pattern_let` generalization confirmed.

**Stubs / TODOs**:
- `Field` arm returns fresh var — no struct field resolution (T3.4 TODO).
- `Try` arm returns fresh var — no `Result`/`Option` error propagation.
- `Compound` arm returns fresh var — CSLv3 Sanskrit compound operators untyped.
- `Perform` returns fresh var for operation return type — effect operation signatures not consulted.
- Capability variants stripped in `lower_hir_type` — semantic type system is cap-erased.

---

## src/cap_check.rs

**Role**: F12 capability checking. Pony-6 cap semantics: iso/trn/ref/val/box/tag. Entry: `check_capabilities`.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `CapMap` | `struct` | `caps: BTreeMap<u32, CapKind>`. HirId → capability mapping. |
| `SubtypeError` | `enum` | Forwarded from `cssl_caps`. |
| `CapCtx` | `struct` | Private. `caps: CapMap`, `diagnostics: Vec<Diagnostic>`, `matrix: AliasMatrix`. |

### Public Functions

| Signature | Description |
|-----------|-------------|
| `check_capabilities(module: &HirModule) -> (CapMap, Vec<Diagnostic>)` | Entry. Creates `CapCtx` with `AliasMatrix::pony6()`, walks all fn items via `check_fn`. |
| `param_subtype_check(caller: CapKind, callee_param: CapKind) -> Result<(), SubtypeError>` | Calls `cssl_caps::coerce(caller, callee_param, &AliasMatrix::pony6())`. |
| `hir_cap_to_semantic(c: HirCapKind) -> CapKind` | Maps `HirCapKind` → `cssl_caps::CapKind`. 1:1 mapping (Iso/Trn/Ref/Val/Box/Tag). |
| `top_cap(t: &HirType) -> Option<CapKind>` | Returns the outermost capability of a `HirType` if it's a `Capability` variant. |

### Private Methods (CapCtx)

| Signature | Description |
|-----------|-------------|
| `check_fn(&mut self, f: &HirFn)` | Creates `LinearTracker::new()`. Registers `iso` params via `tracker.introduce()`. Records cap in `CapMap` for each param. Closes scope at exit. **Does NOT walk body** — body traversal deferred. |
| `record_cap(&mut self, id: HirId, cap: CapKind)` | Inserts into `CapMap`. |

**Tests**: 5 — `hir_cap_to_semantic` roundtrip, `top_cap` on Capability/non-Capability, `param_subtype_check` coerce, `check_capabilities` on empty module.

**Stubs / TODOs**:
- `check_fn` does NOT walk the fn body — only registers iso params. Comment: "body traversal deferred; full linearity tracking in Stage-1 pass". **This means iso/trn linearity is NOT enforced in Stage-0.**
- Alias matrix hardcoded to `pony6()` — no user-defined capability hierarchies.

---

## src/ad_legality.rs

**Role**: F1 autodiff legality checking. Verifies `@differentiable` functions only call differentiable callees. Produces stable diagnostic codes (AD0001–AD0003).

### Types

| Item | Kind | Description |
|------|------|-------------|
| `AdLegalityDiagnostic` | `enum` | 3 variants. |
| `AdLegalityDiagnostic::GradientDrop` | variant | AD0001. Differentiable fn calls non-differentiable callee. `fn_name, callee_name, span`. |
| `AdLegalityDiagnostic::UnresolvedCallee` | variant | AD0002. Callee has no resolved `DefId`. `fn_name, span`. |
| `AdLegalityDiagnostic::MissingReturnTangent` | variant | AD0003. Differentiable fn has no return type to differentiate. `fn_name, span`. |
| `AdLegalityReport` | `struct` | `diagnostics: Vec<AdLegalityDiagnostic>`, `checked_fn_count: usize`, `call_site_count: usize`, `legal_call_count: usize`. |
| `WalkCtx<'a>` | `struct` | Private. `fn_attrs: HashMap<DefId, Vec<HirAttr>>`, `interner: &'a Interner`, `current_fn_name: Symbol`, `current_is_diff: bool`, `report: AdLegalityReport`. |

### Public Functions

| Signature | Description |
|-----------|-------------|
| `check_ad_legality(module: &HirModule, interner: &Interner) -> AdLegalityReport` | Builds `fn_attrs` map, walks all fn items. |
| `is_pure_diff_primitive(name: &str) -> bool` | Returns `true` for 37 known-differentiable math builtins: `length`, `sqrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `exp2`, `log`, `log2`, `log10`, `pow`, `abs`, `floor`, `ceil`, `round`, `sign`, `fract`, `dot`, `cross`, `normalize`, `reflect`, `refract`, `min`, `max`, `clamp`, `mix`, `step`, `smoothstep`, `distance`, `radians`, `degrees`, `determinant`, `inverse`. |

### Private Methods (WalkCtx)

| Signature | Description |
|-----------|-------------|
| `check_fn(&mut self, f: &HirFn)` | Sets current fn context, calls `walk_expr` on body. Checks return type presence for AD0003. |
| `walk_expr(&mut self, expr: &HirExpr)` | Recursive expression walker. Dispatches call sites to `handle_call`. |
| `handle_call(&mut self, callee: &HirExpr, args: &[HirExpr], span: Span)` | **Core legality check.** If current fn is `@differentiable`: checks callee path → resolved DefId → has `@differentiable` or `@NoDiff` attr, OR is `is_pure_diff_primitive`. Emits AD0001/AD0002 on violation. Non-path callees (lambdas, field-access methods) are **skipped with comment**: "stage-0 skips complex callees". |
| `fn_has_attr(attrs: &[HirAttr], target: Symbol) -> bool` | Checks attr list for simple single-segment attr matching target. |

**Tests**: 14 — `is_pure_diff_primitive` for known names, `GradientDrop` emission, `UnresolvedCallee` emission, `MissingReturnTangent`, legal call counted, `@NoDiff` suppresses AD0001.

**Stubs / TODOs**:
- Non-path callees (lambdas, method calls via field access) silently skipped. Comment: "stage-0 skips complex callees". **Higher-order differentiable functions not validated.**
- `is_pure_diff_primitive` is a hardcoded whitelist — no trait-based differentiability.

---

## src/refinement.rs

**Role**: F2 refinement type obligation collection. Does NOT discharge obligations — that's the SMT backend's job (downstream). Collects `RefinementObligation` records for each refinement type occurrence.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `ObligationId` | `struct(u32)` | Monotonic obligation identifier. |
| `ObligationKind` | `enum` | 3 variants. |
| `ObligationKind::Predicate` | variant | `binder: String`, `predicate_text: String`. |
| `ObligationKind::Tag` | variant | `name: String`. Refinement tag (`T'tagname`). |
| `ObligationKind::Lipschitz` | variant | `bound_text: String`. Lipschitz bound (`SDF'L<k>`). |
| `RefinementObligation` | `struct` | `id: ObligationId`, `origin: HirId`, `span: Span`, `enclosing_def: Option<DefId>`, `kind: ObligationKind`, `base_type_text: String`. |
| `ObligationBag` | `struct` | `obligations: BTreeMap<u32,RefinementObligation>`, `next_id: u32`. |
| `CollectCtx<'a>` | `struct` | Private. `interner: &'a Interner`, `bag: ObligationBag`, `current_def: Option<DefId>`. |

### Public Functions

| Signature | Description |
|-----------|-------------|
| `collect_refinement_obligations(module: &HirModule, interner: &Interner) -> ObligationBag` | Walks all items via `CollectCtx`, collects obligations from refined types in params/return/let bindings/field types. |

### Private Methods (CollectCtx)

| Signature | Description |
|-----------|-------------|
| `walk_item(&mut self, item: &HirItem)` | Dispatches to item-specific walkers. Sets `current_def`. |
| `walk_fn(&mut self, f: &HirFn)` | Walks param types + return type for refinements. Walks body. |
| `walk_type(&mut self, ty: &HirType)` | If `Refined`: emits obligation. Recurses into compound types. |
| `walk_expr(&mut self, expr: &HirExpr)` | Partial walker: handles `Cast`, `Lambda`, `Block`, `If`. **Does NOT walk all 35+ expr variants** — notable omission for `Call`, `Binary`, `Match`, etc. |
| `emit_obligation(&mut self, ty: &HirType, kind: &HirRefinementKind)` | Constructs and inserts `RefinementObligation`. Calls `pretty_type` + `pretty_expr`. |
| `pretty_type(&self, ty: &HirType) -> String` | Renders type path for `base_type_text`. Single-segment only. |
| `pretty_expr(&self, e: &HirExpr) -> String` | **CRITICAL STUB**: `format!("{:?}", e.kind)`. Uses `Debug` format, NOT real pretty-printing. Predicate and Lipschitz bounds are rendered as Rust debug output, not source text. |

**Tests**: 6 — predicate obligation, tag obligation, Lipschitz obligation, empty module, nested refined type, `pretty_expr` debug-format confirmed.

**Stubs / TODOs**:
- `pretty_expr` line (verbatim): `format!("{:?}", e.kind)` — **this produces Rust Debug output as the SMT predicate text**. Any downstream SMT solver would receive garbage. This is the largest semantic gap in F2.
- `walk_expr` is partial — refinements in `Call`/`Binary`/`Match`/`Return` expressions are NOT collected.
- No obligation discharge — purely a collection pass. SMT integration is a downstream crate.

---

## src/ifc.rs

**Role**: F5 information flow control. DLM-style labels (Jif/Fabric). Entry: `check_ifc`. Flow analysis: `check_ifc_flow`. Stable codes IFC0001–IFC0004.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `IfcLabel` | `struct` | `confidentiality: BTreeSet<Symbol>`, `integrity: BTreeSet<Symbol>`. Lattice element. |
| `IfcLabelRegistry` | `struct` | `map: BTreeMap<u32,IfcLabel>`. HirId → label. |
| `IfcDiagnostic` | `enum` | 4 variants with stable codes. |
| `IfcDiagnostic::MissingLabel` | variant | IFC0001. Function with `@confidentiality` or `@declass` attr but no explicit label annotation. |
| `IfcDiagnostic::LabelMismatch` | variant | IFC0002. Declared label ≠ inferred label. |
| `IfcDiagnostic::UnauthorizedDeclass` | variant | IFC0003. `@declass` without `@requires` endorsement. |
| `IfcDiagnostic::FlowViolation` | variant | IFC0004. Tainted data flows to unannotated return. |
| `IfcReport` | `struct` | `diagnostics: Vec<IfcDiagnostic>`, `fns_checked: usize`, `fns_with_labels: usize`, `declass_attempts: usize`. |
| `IfcCtx<'a>` | `struct` | Private. `interner: &'a Interner`, `registry: IfcLabelRegistry`, `report: IfcReport`. |

### Public Functions

| Signature | Description |
|-----------|-------------|
| `check_ifc(module: &HirModule, interner: &Interner) -> IfcReport` | Structural walker. Checks fn attrs for IFC0001 (missing label when `@confidentiality`/`@declass` present), IFC0002 (label mismatch placeholder), IFC0003 (declass without requires). Calls `check_ifc_flow` for each fn. |
| `check_ifc_flow(f: &HirFn, interner: &Interner, ctx: &mut IfcCtx)` | **T11-D36 flow analysis.** Seeds `@sensitive` params with `{User}` label. Propagates via `label_of_expr` through path/binary/unary/call/field/block/if/match/return/cast/paren/tuple/array. Emits IFC0004 if return value is tainted AND fn lacks `@confidentiality`/`@declass` + `@requires`. |
| `label_of_expr(expr: &HirExpr, labels: &HashMap<HirId,IfcLabel>, interner: &Interner) -> IfcLabel` | Recursive label inference. Joins labels across sub-expressions. |
| `resolve_builtin_principal(name: &str) -> Option<Symbol>` | Returns interned symbol for one of 9 builtin principals. |
| `label_for_secret(interner: &Interner) -> IfcLabel` | Returns `{User}` label (confidentiality={User}, integrity={}). |
| `builtin_principals() -> Vec<&'static str>` | Returns 9 principal names: `HarmTarget`, `Surveiller`, `Coercer`, `Weaponizer`, `System`, `Kernel`, `User`, `Public`, `Anthropic-Audit`. |

### IfcLabel Methods

| Signature | Description |
|-----------|-------------|
| `IfcLabel::public() -> Self` | Bottom label: empty confidentiality + integrity. |
| `IfcLabel::is_public(&self) -> bool` | True iff both sets empty. |
| `IfcLabel::is_sub_of(&self, other: &Self) -> bool` | Lattice ordering: `self.conf ⊆ other.conf && other.integ ⊆ self.integ`. |
| `IfcLabel::join(&self, other: &Self) -> Self` | Lattice join (LUB): **intersects confidentiality** + **unions integrity**. Note: this differs from standard DLM ⊔ which should UNION confidentiality. |
| `IfcLabel::meet(&self, other: &Self) -> Self` | Lattice meet (GLB): unions confid + intersects integ. |
| `combine_labels(a: &IfcLabel, b: &IfcLabel) -> IfcLabel` | **CRITICAL BUG**: Uses `union` for BOTH sets (confid and integ). Differs from `join` which correctly intersects confid. Used in `label_of_expr` for binary/call propagation. This is INCORRECT lattice behavior — taint propagation overapproximates confidentiality instead of intersecting it. |

**Tests**: 30+ — lattice ops (join/meet/is_sub_of), builtin principals, flow analysis with `@sensitive`, IFC0001/0002/0003/0004 emission, declass+requires suppression, label_for_secret.

**Stubs / TODOs**:
- `IFC0002` (label mismatch) is emitted only as a placeholder check — actual label inference vs declared comparison is partial.
- `combine_labels` uses wrong lattice operation (union for confid should be intersection for ⊔). **Spec divergence confirmed.**
- `check_ifc_flow` flow analysis is best-effort — loop bodies not propagated, complex control flow not handled.

---

## src/staged_check.rs

**Role**: F4 staged computation consistency checking. Validates `@staged` attribute annotations, checks stage-class compatibility at call sites, detects cycles in staged dependency graph. Stable codes STG0001–STG0003.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `StageClass` | `enum` | `CompTime`, `Runtime`, `Polymorphic`, `Unspecified`. |
| `StageEntry` | `struct` | `name: Symbol`, `class: StageClass`, `span: Span`. |
| `StageRegistry` | `struct` | `map: BTreeMap<u32,StageEntry>`. DefId → stage entry. |
| `StagedCode` | `enum` | 3 variants: `Missing` (no `@staged` attr), `Declared(StageClass)`, `Conflicting`. |
| `StagedDiagnostic` | `struct` | `code: StagedDiagCode`, `span: Span`, `message: String`. |
| `StagedDiagCode` | `enum` | `STG0001` (missing/malformed stage), `STG0002` (incompatible stage call), `STG0003` (cycle in staged graph). |
| `StagedReport` | `struct` | `diagnostics: Vec<StagedDiagnostic>`, `checked_fn_count: usize`, `cyclic_edges: Vec<(DefId,DefId)>`. |
| `DfsColor` | `enum` | Private. `White`/`Gray`/`Black`. Three-color DFS for cycle detection. |
| `StagedCtx<'a>` | `struct` | Private. Full checking context. |

### StageClass Methods

| Signature | Description |
|-----------|-------------|
| `StageClass::compatible_with(&self, callee: &StageClass) -> bool` | Const compatibility matrix. `CompTime×Runtime = false`, `Polymorphic×* = true`, `*×Polymorphic = true`, same×same = true, `Unspecified×* = true` (lenient). |

### Public Functions

| Signature | Description |
|-----------|-------------|
| `check_staged_consistency(module: &HirModule, interner: &Interner) -> StagedReport` | Runs 4 passes in sequence: (1) extract stage classes, (2) check call-site compatibility (STG0002), (3) detect cycles via DFS (STG0003), (4) emit STG0001 for Unspecified. |
| `extract_stage_class(f: &HirFn, interner: &Interner) -> StagedCode` | Parses `@staged(...)` attr. Recognizes `Path` arg form: `@staged(runtime)`, `@staged(comptime)`, `@staged(Comptime)`, `@staged(CompTime)`, `@staged(polymorphic)`. **STUB**: `@staged("comptime")` string-literal form → returns `Unspecified` (no source-slice access in Stage-0). |

### Private Methods (StagedCtx)

| Signature | Description |
|-----------|-------------|
| `build_call_graph(&mut self)` | Walks all fn bodies, records `DefId → Vec<DefId>` call edges. |
| `check_call_site_compat(&mut self)` | For each call edge: if caller's class incompatible with callee's class → STG0002. |
| `dfs_detect_cycles(&mut self)` | Three-color DFS over call graph. Back-edge (Gray → Gray) = cycle. Records in `cyclic_edges`. Emits STG0003 per unique back-edge (BTreeSet dedup). |

**Tests**: 20+ — `compatible_with` matrix all combinations, stage extraction path form, stage extraction literal form (Unspecified pin), STG0001 emission, STG0002 call-site compat, STG0003 cycle detection (simple and complex), polymorphic compatibility.

**Stubs / TODOs**:
- String-literal form `@staged("comptime")` parsed as `Unspecified` — emits STG0001. Test pins this behavior: "Stage-0 : literal-form currently parses to Unspecified → STG0001". Deferred to Stage-1 when source-slice access is available.
- `Conflicting` StagedCode (fn has multiple `@staged` attrs with different args) detected but no dedicated diagnostic code — falls through to STG0001.

---

## src/macro_hygiene.rs

**Role**: F-macro pass. Validates macro tier declarations and hygiene requirements. Stable codes MAC0001–MAC0003.

### Types

| Item | Kind | Description |
|------|------|-------------|
| `MacroHygieneCode` | `enum` | `MAC0001` (hygiene violation — hygienic fn missing required attr), `MAC0002` (multiple tier declarations), `MAC0003` (unsupported macro tier). |
| `MacroHygieneDiagnostic` | `struct` | `code: MacroHygieneCode`, `span: Span`, `message: String`. |
| `MacroHygieneReport` | `struct` | `diagnostics: Vec<MacroHygieneDiagnostic>`, `checked_item_count: usize`. |
| `TierNames` | `struct` | Pre-interned `Symbol`s: `hygienic`, `attr_macro`, `declarative`, `proc_macro`. |
| `AttrClassification` | `struct` | `has_hygienic: bool`, `hygienic_span: Option<Span>`, `tier_declaring_count: usize`, `first_tier_span: Option<Span>`. |
| `HygieneCtx<'a>` | `struct` | Private. `interner: &'a Interner`, `tier_names: TierNames`, `report: MacroHygieneReport`. |

### Public Functions

| Signature | Description |
|-----------|-------------|
| `check_macro_hygiene(module: &HirModule, interner: &Interner) -> MacroHygieneReport` | Creates `HygieneCtx`, walks all `Fn` items (not structs/enums/etc.). |

### Private Methods (HygieneCtx)

| Signature | Description |
|-----------|-------------|
| `check_fn(&mut self, f: &HirFn)` | Calls `classify_attrs`. Emits MAC0001 if `has_hygienic` but `tier_declaring_count == 0`. Emits MAC0002 if `tier_declaring_count > 1`. |
| `classify_attrs(&self, attrs: &[HirAttr]) -> AttrClassification` | Scans attrs. Single-segment only (multi-segment ignored). Counts `hygienic`, `attr_macro`, `declarative`, `proc_macro` attrs. Returns classification. |
| `emit(&mut self, code: MacroHygieneCode, span: Span, message: String)` | Appends diagnostic to report. |

### MacroHygieneDiagnostic Methods

| Signature | Description |
|-----------|-------------|
| `render(&self) -> String` | Returns `"[MAC000N] message"` formatted string. |

**Tests**: 13 — clean fn (no diag), hygienic+tier_declared (no diag), hygienic+no_tier → MAC0001, multi-tier → MAC0002, `render()` format, empty module, struct items skipped, multi-segment attrs ignored.

**Stubs / TODOs**:
- Multi-segment attrs (e.g., `#[macro::hygienic]`) are silently ignored — only single-segment attrs classified. Comment: "multi-segment attr namespace support deferred".
- MAC0003 (unsupported tier) defined but never emitted in current implementation. Dead variant.
- Only `Fn` items checked — macro attrs on struct/enum/impl items not validated.

---

## Notable Findings

### Real vs Stubbed — Feature Matrix

| Feature | Status | Notes |
|---------|--------|-------|
| F1 Autodiff legality | **Largely real** | 37-primitive whitelist, AD0001-AD0003 codes, `@differentiable`/`@NoDiff` attr checking. Gap: higher-order / non-path callees silently skipped. |
| F2 Refinement types | **Collector only** | Obligations collected structurally. `pretty_expr` produces Debug output (garbage for SMT). Expr walker partial (misses `Call`/`Match`/`Return` refinements). No discharge. |
| F3 Effect system | **Structurally present** | `HirEffectRow`, `Row`, row unification all real. `Perform`/`With` expr forms lowered and partially typed. `Perform` return type not resolved against operation signature. |
| F4 Staged computation | **Largely real** | 4-pass consistency check, 3-color DFS cycle detection, STG0001-STG0003. Gap: string-literal `@staged("comptime")` form = Unspecified. |
| F5 IFC | **Largely real** | DLM lattice, 9 PRIME_DIRECTIVE principals, IFC0001-IFC0004, T11-D36 flow propagation. **Critical bug**: `combine_labels` uses wrong lattice op (union confid instead of intersection). |
| Capabilities | **Registration only** | `hir_cap_to_semantic` + `AliasMatrix::pony6()` real. `param_subtype_check` real. Body traversal + linearity enforcement deferred. |
| HM type inference | **Real** | Bidirectional, let-generalization (T3-D15), generic fn schemes (T3-D17), Remy row unification. Gaps: `Field`/`Try`/`Compound` exprs return fresh vars. |
| Name resolution | **Single-segment only** | Multi-segment paths (`module::Type`) always unresolved (`def: None`). Documented Stage-0 limitation. |
| Macro hygiene | **Real for common cases** | MAC0001-MAC0002 real. MAC0003 dead. Multi-segment attr namespaces ignored. |

### Architecture Surprises

1. **Two type ladders**: `HirType` (syntactic, CST mirror) vs `Ty` (semantic, inference-level). `lower_hir_type()` bridges them. Capabilities exist only in `HirType` — `Ty` is cap-erased.

2. **`combine_labels` lattice bug** (`ifc.rs`): The `label_of_expr` function calls `combine_labels` which unions BOTH confidentiality and integrity sets. Correct DLM ⊔ should INTERSECT confidentiality (more principals = more restrictive) and union integrity. The current code overapproximates confidentiality propagation, making the IFC pass more conservative than the spec requires but in the wrong direction for label inference.

3. **`resolve_path` multi-segment stub** (`resolve.rs`): Any `module::T` style path returns `None`. This means all cross-module type references have `def: Option<DefId> = None` throughout HIR. The inference engine copes by treating unknown paths as fresh type vars + diagnostic.

4. **`pretty_expr` Debug format** (`refinement.rs`): The SMT obligation collector stores predicate text as `format!("{:?}", e.kind)`. A downstream SMT crate would receive Rust Debug output like `Binary { op: Add, lhs: ..., rhs: ... }` instead of source text like `v + 1`. This is a semantic correctness gap for F2.

5. **`@staged("comptime")` literal form** (`staged_check.rs`): String-literal form of staged annotation produces `Unspecified` → STG0001. Only identifier form works. Test pins this as known Stage-0 limitation.

6. **MAC0003 dead variant**: `MacroHygieneCode::MAC0003` (unsupported macro tier) is defined but never emitted. The tier classification only produces MAC0001/MAC0002.

7. **Cap body traversal deferred**: `cap_check.rs` registers iso params via `LinearTracker::introduce()` but never walks function bodies. Iso/trn linearity (use-once semantics) is completely unenforced in Stage-0.

8. **`HirImpl`/`HirUse` have no `DefId`**: Design decision — impls and use declarations have no definition identity. `HirItem::def_id()` returns `None` for these. Item signature collection skips them.

9. **`saturating_add` in arena**: `HirArena::fresh_hir_id` and `fresh_def_id` use `saturating_add(1)`. At saturation (4B nodes), the arena silently stops incrementing. No panic, no error. Acceptable for Stage-0 scale.

10. **PRIME_DIRECTIVE principals in IFC**: The 9 builtin IFC principals (`HarmTarget`, `Surveiller`, `Coercer`, `Weaponizer`, `System`, `Kernel`, `User`, `Public`, `Anthropic-Audit`) directly encode the PRIME_DIRECTIVE's harm prohibitions into the type system. `Anthropic-Audit` is a principal with integrity claims — any `@sensitive` data flowing toward an `Anthropic-Audit` label context requires explicit `@declass + @requires`.

### Spec / Code Divergences

| Spec | Divergence |
|------|------------|
| specs/11_IFC.csl — DLM ⊔ | `combine_labels` unions confidentiality (should intersect). `join` is correct; `combine_labels` in flow analysis is wrong. |
| specs/04_EFFECTS.csl — effect operations | `Perform` in `infer.rs` returns fresh var, not the operation's declared return type. Effect operation signatures not consulted during inference. |
| specs/02_IR.csl — multi-segment paths | All paths with 2+ segments are unresolved at HIR level. Spec assumes full resolution. |
| specs/05_AUTODIFF.csl — higher-order AD | Non-path callees silently skipped. `fn_arg: fn() -> T` in `@differentiable` fn = unchecked. |
| specs/12_CAPS.csl — linearity | Iso/trn use-once semantics not enforced (body traversal deferred). |
| specs/20_SMT.csl — predicate text | Refinement predicate text = Rust Debug output, not source text. SMT integration would fail. |

---

*End of audit. 21 files, 22 sections (Cargo.toml + 21 .rs files). Approx 210 documented items.*

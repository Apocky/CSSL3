# Audit 06 — Transform / SMT / Staging / Futamura / Macros

**Audited:** 2026-05-14  
**Auditor:** Claude Sonnet 4.6 (agent)  
**Scope:** `compiler-rs/crates/cssl-smt/`, `compiler-rs/crates/cssl-staging/`, `compiler-rs/crates/cssl-futamura/`, `compiler-rs/crates/cssl-macros/`  
**Spec authority:** `specs/20_SMT.csl`, `specs/06_STAGING.csl`, `specs/19_FUTAMURA3.csl`, `specs/13_MACROS.csl`  
**Files audited:** 11 source files (8 × .rs, 4 × Cargo.toml; no separate tests/ directories)  
**Items documented:** 105 (functions, structs, enums, traits, impls, consts, type aliases)

---

## 1. SLICE OVERVIEW

These four crates together implement two of the six non-negotiable Sigil language features: **F2 Refinement Types** (backed by SMT solvers) and **F4 Staged Computation** (@staged + Futamura projections), plus the **hygienic macro infrastructure** that serves as the metaprogramming layer underlying both.

**`cssl-smt`** is the bridge between the type system and external SMT solvers. When the type checker encounters a refinement obligation — a predicate that must hold for a value to be assigned a refined type — it hands an `ObligationBag` to this crate. The crate translates each obligation into an SMT-LIB 2.6 text query, dispatches it through a solver subprocess (Z3 or CVC5 CLI), and reports a `Verdict` (Sat / Unsat / Unknown / Error) back. An `Unsat` verdict means the negation of the predicate is unsatisfiable, proving the refinement holds. No native FFI against `z3-sys` or `cvc5-sys` is used; both solver connections are subprocess-based. KLEE symbolic execution is named in the module documentation but entirely absent from the implementation.

**`cssl-staging`** implements the F4 compile-time specialization mechanism. A function annotated `@staged` may have some arguments known at compile time and others at runtime. The crate walks the HIR to collect all `@staged` declarations, identifies `#run` sites (comptime evaluation demands within bodies), and builds a data model (`Specializer`, `SpecializationSite`) for downstream passes to populate with actual specialization choices. The actual specialization transform (cloning a function and constant-propagating the comptime arguments) is deferred to T8-phase-2.

**`cssl-futamura`** models the three classic Futamura projections — the theoretical underpinning that allows @staged specialization to serve not just as optimization but as a path to self-hosting. P1 specializes an interpreter over a fixed source program, producing a compiled artifact. P2 specializes the specializer against an interpreter, producing a standalone compiler. P3 specializes the specializer against itself, producing a compiler generator. The crate provides data types (`FutamuraLevel`, `Projection`, `FixedPointRecord`) and an `Orchestrator` that records which projections have been applied and checks whether P3 convergence (hash equality across generations) has been reached. No actual partial-evaluation algorithm is implemented here; that is done through `cssl-staging`.

**`cssl-macros`** implements the hygienic macro infrastructure based on the Racket / Flatt set-of-scopes model. Every identifier is a `SyntaxObject` carrying a `HygieneMark` (a `BTreeSet<ScopeId>`). Two identifiers are hygienically equal iff both their text and their scope-set agree. A `ScopeAllocator` mints fresh scope identifiers. The `MacroRegistry` maps macro names to their tier (Tier-1 attribute macros, Tier-2 declarative/pattern-rewrite, Tier-3 `#run` proc-macros). Actual pattern-matching expansion and proc-macro evaluation are deferred to T8-phase-2.

The dependency graph within the slice: `cssl-smt` depends on `cssl-ast` and `cssl-hir`. `cssl-staging` depends on `cssl-ast` and `cssl-hir`. `cssl-futamura` depends only on `thiserror`. `cssl-macros` depends only on `thiserror`. None of the four crates depend on each other, keeping the slice compositional.

---

## 2. CRATE SUMMARIES

### 2.1 `cssl-smt`

**Path:** `compiler-rs/crates/cssl-smt/`  
**Description:** `"CSSLv3 stage0 — SMT-LIB emission + solver dispatch (Z3/CVC5 CLI)"`  
**Pipeline role:** Post-type-checking pass. Receives `ObligationBag` from `cssl-hir`, translates each obligation into an SMT-LIB 2.6 query, dispatches to a solver subprocess, and returns per-obligation verdicts. Backs the F2 Refinement Types feature.  
**Spec:** `specs/20_SMT.csl` (T9-phase-1 scope covered; T9-phase-2 deferred)

**Cargo.toml dependencies:**
- `cssl-ast` (path) — needed for `Span::DUMMY` in tests  
- `cssl-hir` (path) — `ObligationBag`, `ObligationId`, `ObligationKind`, `RefinementObligation`, `Interner`  
- `thiserror` (workspace) — error derivation

No `z3`, `cvc5-sys`, or `klee-sys` in this crate's own Cargo.toml. The workspace-level `Cargo.toml` lists `z3 = { version = "0.12", default-features = false }` as an available dependency but it is **not** used by `cssl-smt`. The `cvc5-sys` and `klee-sys` lines are commented out.

**Total LOC:** 1,756 (across 6 files)

**File list:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 60 | Crate root: module declarations, pub re-exports, scaffold const, one scaffold test |
| `src/term.rs` | 339 | `Theory`, `Sort`, `Literal`, `Term` datatypes + SMT-LIB rendering |
| `src/query.rs` | 221 | `FnDecl`, `Assertion`, `Query`, `Verdict` — the query builder |
| `src/emit.rs` | 100 | `emit_smtlib()` — serializes a `Query` to SMT-LIB 2.6 text |
| `src/predicate.rs` | 702 | Predicate-text tokenizer + recursive-descent parser → `Term`; obligation translator |
| `src/solver.rs` | 334 | `SolverKind`, `Solver` trait, `Z3CliSolver`, `Cvc5CliSolver`, `discharge()` |

---

### 2.2 `cssl-staging`

**Path:** `compiler-rs/crates/cssl-staging/`  
**Description:** `"CSSLv3 stage0 — @staged specializer + #run comptime evaluation"`  
**Pipeline role:** HIR analysis pass that collects `@staged` function declarations and `#run` expression sites, building a `Specializer` manifest for the downstream transform-dialect pass. Part of F4 Staged Computation.  
**Spec:** `specs/06_STAGING.csl`, `specs/19_FUTAMURA3.csl` (T8-phase-1 covered; T8-phase-2 deferred)

**Cargo.toml dependencies:**
- `cssl-ast` (path) — `SourceFile`, `SourceId`, `Surface`, `Span` (used in tests)  
- `cssl-hir` (path) — `HirModule`, `HirFn`, `HirItem`, `HirAttr`, `HirExpr*`, `HirBlock`, `HirStmt*`, `DefId`, `Symbol`, `Interner`  
- `thiserror` (workspace)  
- `cssl-lex` (dev-dep, path) — for integration tests  
- `cssl-parse` (dev-dep, path) — for integration tests

**Total LOC:** 455 (single file)

**File list:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 455 | All staging types: `StageArgKind`, `StageArg`, `StagedDecl`, `RunMarker`, `Specializer`, `SpecializationSite`, `StagingError`; HIR walkers; tests |

---

### 2.3 `cssl-futamura`

**Path:** `compiler-rs/crates/cssl-futamura/`  
**Description:** `"CSSLv3 stage0 — P1 / P2 / P3 partial-evaluation infrastructure"`  
**Pipeline role:** Records which Futamura projections have been applied to which source modules, and tracks P3 fixed-point convergence via generation-hash comparison. Does not implement any partial-evaluation algorithm itself; that is delegated to `cssl-staging`.  
**Spec:** `specs/19_FUTAMURA3.csl`, `specs/06_STAGING.csl` (T8-phase-1; T16 CI-gate fixed-point verification deferred to stage-1+)

**Cargo.toml dependencies:**
- `thiserror` (workspace)  
- No `cssl-ast` or `cssl-hir` dependency — this crate is deliberately IR-agnostic.

**Total LOC:** 284 (single file)

**File list:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 284 | `FutamuraLevel`, `Projection`, `FixedPointRecord`, `Orchestrator`, `FutamuraError`; tests |

---

### 2.4 `cssl-macros`

**Path:** `compiler-rs/crates/cssl-macros/`  
**Description:** `"CSSLv3 stage0 — Racket-hygienic macros + proc-macro tier-3"`  
**Pipeline role:** Provides the hygiene data model (`HygieneMark`, `SyntaxObject`, `ScopeAllocator`) and the macro registry (`MacroRegistry`, `MacroDecl`) used by all expansion passes. Actual pattern-match expansion and `#run` proc-macro evaluation are T8-phase-2.  
**Spec:** `specs/13_MACROS.csl` (T8-phase-1; expansion deferred)

**Cargo.toml dependencies:**
- `thiserror` (workspace)  
- No `cssl-ast` or `cssl-hir` — hygiene model is surface-syntax agnostic.

**Total LOC:** 343 (single file)

**File list:**
| File | LOC | Purpose |
|------|-----|---------|
| `src/lib.rs` | 343 | `MacroTier`, `ScopeId`, `HygieneMark`, `SyntaxObject`, `ScopeAllocator`, `MacroDecl`, `MacroRegistry`, `MacroError`; tests |

---

## 3. PER-FILE ANALYSIS

### 3.1 `cssl-smt/src/lib.rs` (60 lines)

**Purpose:** Crate root. Declares the five submodules (`emit`, `predicate`, `query`, `solver`, `term`), re-exports the complete public API in a flat namespace, declares the `STAGE0_SCAFFOLD` version constant, and provides one smoke test confirming the constant is non-empty.

**Items:**

- **`pub mod emit`** — re-exports `emit::emit_smtlib`
- **`pub mod predicate`** — re-exports `predicate::{parse_predicate, translate_bag, translate_obligation, TranslationError}`
- **`pub mod query`** — re-exports `query::{Assertion, FnDecl, Query, Verdict}`
- **`pub mod solver`** — re-exports `solver::{default_args_for, discharge, run_cli_text, Cvc5CliSolver, Solver, SolverError, SolverKind, Z3CliSolver}`
- **`pub mod term`** — re-exports `term::{Literal, Sort, Term, Theory}`
- **`pub const STAGE0_SCAFFOLD: &str`** (`lib.rs:50`) — Exposes `CARGO_PKG_VERSION` for external scaffold-verification tooling.
- **`#[cfg(test)] mod scaffold_tests`** — one test: `scaffold_version_present()` asserts non-empty string.

**Notable crate-level lint configuration:** `#![forbid(unsafe_code)]`, `#![deny(rustdoc::broken_intra_doc_links)]`, `#![deny(rustdoc::private_intra_doc_links)]`. Several Clippy lints suppressed (`match_same_arms`, `no_effect_underscore_binding`, `struct_excessive_bools`, `missing_errors_doc`, `use_self`) — all consistent with the stage-0 scaffolding style used across the workspace.

---

### 3.2 `cssl-smt/src/term.rs` (339 lines)

**Purpose:** Defines the core SMT-LIB data model: the logic name (`Theory`), the sort system (`Sort`), literal values (`Literal`), and the term tree (`Term`). All four types implement `render()` to produce standard SMT-LIB 2.6 text. The `Term::render()` is implemented via a private `render_into(&mut String)` for efficient string building without intermediate allocation per node.

**Items:**

- **`enum Theory`** (`term.rs:7`) — Seven variants: `LIA`, `LRA`, `NRA`, `BV`, `UF`, `UFLIA`, `ALL`. Represents the SMT-LIB `(set-logic ...)` value.
  - **`impl Theory`**:
    - `pub const fn as_str(self) -> &'static str` (`term.rs:27`) — Maps each variant to its SMT-LIB string (`"QF_LIA"`, `"QF_LRA"`, `"QF_NRA"`, `"QF_BV"`, `"QF_UF"`, `"QF_UFLIA"`, `"ALL"`). Note: ALL maps to `"ALL"` (Z3-specific), not `"QF_ALL"`.
    - `pub const ALL_THEORIES: [Self; 7]` (`term.rs:40`) — Compile-time array of all seven theories.
  - **`impl fmt::Display for Theory`** (`term.rs:51`) — Delegates to `as_str()`.

- **`enum Sort`** (`term.rs:59`) — Five variants: `Bool`, `Int`, `Real`, `BitVec(u32)`, `Uninterp(String)`. Field: `BitVec` carries the bit-width N; `Uninterp` carries the sort name.
  - **`impl Sort`**:
    - `pub fn render(&self) -> String` (`term.rs:75`) — Produces `"Bool"`, `"Int"`, `"Real"`, `"(_ BitVec N)"`, or the uninterpreted name verbatim.
  - **`impl fmt::Display for Sort`** (`term.rs:86`) — Delegates to `render()`.

- **`enum Literal`** (`term.rs:93`) — Four variants: `Bool(bool)`, `Int(i64)`, `Rational { num: i64, den: u64 }`, `BitVec { value: u64, width: u32 }`. The `Rational` variant is used by the Lipschitz-bound parser to represent decimal constants as fractions. The `BitVec` literal renders to `(_ bvVALUE WIDTH)`.
  - **`impl Literal`**:
    - `pub fn render(&self) -> String` (`term.rs:108`) — Canonical SMT-LIB rendering.

- **`enum Term`** (`term.rs:119`) — Six variants: `Var(String)`, `Lit(Literal)`, `App { head: String, args: Vec<Term> }`, `Forall { binders: Vec<(String, Sort)>, body: Box<Term> }`, `Exists { binders: Vec<(String, Sort)>, body: Box<Term> }`, `Let { bindings: Vec<(String, Term)>, body: Box<Term> }`. This is the core recursive AST for SMT-LIB 2.6 terms.
  - **`impl Term`**:
    - `pub fn var(name: impl Into<String>) -> Self` (`term.rs:147`) — Constructor for `Var`.
    - `pub fn app(head: impl Into<String>, args: Vec<Term>) -> Self` (`term.rs:153`) — Constructor for `App`.
    - `pub const fn int(n: i64) -> Self` (`term.rs:162`) — Constructor for `Lit(Literal::Int(n))`.
    - `pub const fn bool(b: bool) -> Self` (`term.rs:168`) — Constructor for `Lit(Literal::Bool(b))`.
    - `pub fn render(&self) -> String` (`term.rs:173`) — Allocates a `String` and delegates to `render_into`.
    - `fn render_into(&self, out: &mut String)` (`term.rs:180`) — **Private.** Recursive SMT-LIB text emission. Handles all six variants. For `App`, emits `(head arg1 arg2 ...)`. For `Forall`/`Exists`, emits binder lists with space-separated `(name sort)` pairs. For `Let`, emits `(let ((x t1) ...) body)`. No interning or deduplication — every render allocates fresh strings.

- **`#[cfg(test)] mod tests`** (`term.rs:245`) — 12 unit tests covering: theory name strings, 7-element ALL_THEORIES array, sort rendering for all 5 variants, literal rendering for all 4 variants, `Term::var/int/bool` constructors, `App` rendering flat and nested, `Forall` / `Exists` / `Let` rendering, multi-binder `Forall`.

**Key invariants:** `Term` is `Clone + PartialEq + Eq` (no `Hash`, as `Vec` and `Box` would require custom implementations). The `render_into` design avoids the `O(N^2)` string concatenation problem for deep trees. No sharing or DAG optimization — each clone is a full tree copy.

---

### 3.3 `cssl-smt/src/query.rs` (221 lines)

**Purpose:** Defines the query builder types: `FnDecl` (a `(declare-fun ...)` statement), `Assertion` (an `(assert ...)` statement with optional named label), `Query` (the complete SMT-LIB script builder), and `Verdict` (the possible solver answers).

**Items:**

- **`struct FnDecl`** (`query.rs:7`) — Fields: `name: String`, `params: Vec<Sort>` (parameter sorts; empty for constants), `result: Sort`.
  - **`impl FnDecl`**:
    - `pub fn new(name: impl Into<String>, params: Vec<Sort>, result: Sort) -> Self` (`query.rs:18`) — Constructor.
    - `pub fn render(&self) -> String` (`query.rs:28`) — Emits `(declare-fun name (p1 p2 ...) result)`. Empty params list renders as `()`.

- **`struct Assertion`** (`query.rs:45`) — Fields: `term: Term`, `label: Option<String>`. The optional label enables unsat-core extraction via SMT-LIB's `:named` annotation.
  - **`impl Assertion`**:
    - `pub fn new(term: Term) -> Self` (`query.rs:55`) — Unlabeled assertion.
    - `pub fn named(label: impl Into<String>, term: Term) -> Self` (`query.rs:61`) — Labeled assertion.
    - `pub fn render(&self) -> String` (`query.rs:69`) — Emits `(assert term)` or `(assert (! term :named label))`.

- **`struct Query`** (`query.rs:79`) — Fields: `theory: Option<Theory>`, `sort_decls: Vec<String>`, `fn_decls: Vec<FnDecl>`, `assertions: Vec<Assertion>`, `get_model: bool`, `get_unsat_core: bool`. Derives `Default`.
  - **`impl Query`**:
    - `pub fn new() -> Self` (`query.rs:98`) — `Self::default()`.
    - `pub const fn with_theory(mut self, t: Theory) -> Self` (`query.rs:104`) — Builder-pattern setter.
    - `pub fn declare_sort(&mut self, name: impl Into<String>)` (`query.rs:110`) — Appends to `sort_decls`.
    - `pub fn declare_fn(&mut self, decl: FnDecl)` (`query.rs:115`) — Appends to `fn_decls`.
    - `pub fn assert(&mut self, term: Term)` (`query.rs:120`) — Appends unlabeled assertion.
    - `pub fn assert_named(&mut self, label: impl Into<String>, term: Term)` (`query.rs:125`) — Appends labeled assertion.
    - `pub fn is_trivial(&self) -> bool` (`query.rs:131`) — `true` if no sort decls, fn decls, or assertions. Used to skip solver invocation for vacuous queries.

- **`enum Verdict`** (`query.rs:137`) — Four variants: `Sat`, `Unsat`, `Unknown`, `Error`. Important semantic note documented in source: for refinement checking, the query asserts the **negation** of the predicate, so `Unsat` means the refinement holds (the negation has no model), and `Sat` means it is violated. `Unknown` is treated conservatively as a violation in CI.

- **`#[cfg(test)] mod tests`** (`query.rs:150`) — 11 unit tests covering: `FnDecl` rendering (0-arity and multi-arity), `Assertion` labeled and unlabeled rendering, `Query::new()` is trivial, builder chain with theory, declare-and-assert makes query non-trivial, labeled assertion accessible, verdict variant distinctness, sort_decls tracking.

---

### 3.4 `cssl-smt/src/emit.rs` (100 lines)

**Purpose:** Contains the single function `emit_smtlib()` that serializes a `Query` into a complete SMT-LIB 2.6 script string. Produces output in the canonical order: `set-logic`, `declare-sort` statements, `declare-fun` statements, `assert` statements, `check-sat`, optionally `get-model`, optionally `get-unsat-core`.

**Items:**

- **`pub fn emit_smtlib(q: &Query) -> String`** (`emit.rs:7`) — **`#[must_use]`**. Iterates over all `Query` fields in SMT-LIB required order and concatenates. Each `FnDecl::render()` and `Assertion::render()` call is separated by a newline. The `check-sat` line is always appended. `get-model` and `get-unsat-core` are conditional. Returns the complete text.

- **`#[cfg(test)] mod tests`** (`emit.rs:33`) — 7 unit tests: empty query contains `(check-sat)`, theory prefix added, `declare-fun` and `assert` appear in correct order, `get-model` flag emitted, `get-unsat-core` flag emitted, `declare-sort` renders, multi-assertion order preserved (first position < second).

**Notable design:** `emit_smtlib` does no validation — it will happily emit a query with no `declare-fun` for variables referenced in assertions. Validation is the solver's responsibility. This matches the stage-0 design philosophy.

---

### 3.5 `cssl-smt/src/predicate.rs` (702 lines)

**Purpose:** The most substantial file in the slice. Implements a complete tokenizer and recursive-descent parser for a predicate mini-language (comparison expressions, logical connectives, set-membership), translates parsed predicates into `Term` trees, and then constructs complete `Query` values from `RefinementObligation` records. Also implements a Lipschitz-bound text parser.

**Items:**

- **`enum TranslationError`** (`predicate.rs:45`) — Two variants:
  - `ParseFailure { text: String, reason: String }` — Predicate text was syntactically malformed.
  - `UnsupportedKind { kind: &'static str }` — Obligation kind not yet translatable at stage-0. Note: as of this commit, `Lipschitz` obligations are **now handled** (see `translate_obligation` below), so this variant currently goes unused but remains for future additions.
  - Both variants implement `thiserror::Error` with descriptive messages.

- **`enum Token`** (`predicate.rs:60`) — **Private.** 17 variants covering integer literals, identifiers, comparison operators (`Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`), logical operators (`AndTok`, `OrTok`), punctuation (`LParen`, `RParen`, `LBrace`, `RBrace`, `Comma`), the `In` keyword/glyph, and `Minus`.

- **`fn tokenize(input: &str) -> Result<Vec<Token>, String>`** (`predicate.rs:83`) — **Private.** Byte-level tokenizer. Handles ASCII whitespace skipping, two-character operator detection (tested with byte-boundary check to avoid splitting multi-byte UTF-8), single-character punctuation, the three-byte Unicode sequence `∈` (U+2208, bytes `0xE2 0x88 0x88`), integer literals (digit-runs parsed via `str::parse::<i64>`), and identifiers/keywords (`and`, `or`, `in` are keyword-dispatched; all other alpha/underscore-leading runs become `Ident`). Returns an error string on unrecognized characters.

- **`struct Parser`** (`predicate.rs:198`) — **Private.** Fields: `tokens: Vec<Token>`, `pos: usize`. Hand-rolled recursive-descent parser. No backtracking — deterministic one-token lookahead via `peek()`.
  - `fn peek(&self) -> Option<&Token>` (`predicate.rs:204`) — Non-consuming lookahead.
  - `fn eat(&mut self) -> Option<Token>` (`predicate.rs:207`) — Consuming advance.
  - `fn expect_token(&mut self, want: &Token, ctx: &str) -> Result<(), String>` (`predicate.rs:216`) — Consumes and validates a specific token; returns error string with context on mismatch or EOF.
  - `fn parse_disjunction(&mut self) -> Result<Term, String>` (`predicate.rs:224`) — Top-level parse entry. Collects conjunction results separated by `OrTok`. Single-element list is returned unwrapped; multiple elements become `Term::app("or", args)`.
  - `fn parse_conjunction(&mut self) -> Result<Term, String>` (`predicate.rs:238`) — Collects comparison results separated by `AndTok`. Single-element list is returned unwrapped; multiple become `Term::app("and", args)`.
  - `fn parse_comparison(&mut self) -> Result<Term, String>` (`predicate.rs:252`) — Parses a primary followed by an optional comparison operator. Handles `==` → `Term::app("=", ...)`, `!=` → `Term::app("not", [Term::app("=", ...)])`, `<` → `"<"`, `<=` → `"<="`, `>` → `">"`, `>=` → `">="`, and `in`/`∈` → set-membership expansion: `(or (= lhs m1) (= lhs m2) ...)`. Single-member set returns the equality directly (no `or`).
  - `fn parse_primary(&mut self) -> Result<Term, String>` (`predicate.rs:310`) — Leaves: integer literal → `Term::int(n)`, identifier (with special-casing for `"true"` and `"false"` → `Term::bool(...)`, all others → `Term::var(name)`), parenthesized expression (recursive `parse_disjunction`), unary minus → `Term::app("-", [inner])`.
  - `fn finished(&self) -> bool` (`predicate.rs:334`) — Returns `true` when all tokens consumed.

- **`pub fn parse_predicate(text: &str) -> Result<Term, TranslationError>`** (`predicate.rs:343`) — Public entry point. Calls `tokenize`, constructs a `Parser`, calls `parse_disjunction`, checks for trailing tokens (produces `ParseFailure` if any remain), maps tokenize/parse errors to `TranslationError::ParseFailure`.

- **`pub fn translate_obligation(obligation: &RefinementObligation, interner: &cssl_hir::Interner) -> Result<Query, TranslationError>`** (`predicate.rs:379`) — The primary integration function. Dispatches on `ObligationKind`:
  - **`Predicate { binder, predicate_text }`**: Resolves the binder name via interner, parses the predicate text, builds a `QF_LIA` query, declares the binder as `Int`-sorted, asserts `(not P(binder))` as a labeled named assertion (`obl_{id}_predicate`), returns the query. **Stage-0 note:** All binders are assumed `Int` sort; float or real-valued predicates would require `QF_LRA` and `Real` sort.
  - **`Tag { name }`**: Resolves the tag name, builds an `ALL`-theory query, asserts `true` labeled `obl_{id}_tag_{name}`. This is explicitly a stub — the tag-dictionary resolution needed for real semantics is deferred to T9-phase-2b.
  - **`Lipschitz { bound_text }`**: As of T9-phase-2b, this is **now handled** (contrary to the `UnsupportedKind` variant which no longer applies here). Builds a `QF_LRA` query, declares `x: Real`, `y: Real`, and an uninterpreted function `f: Real → Real` (name derived from `enclosing_def` if present, else `"f"`), asserts `(not (<= (abs (- (f x) (f y))) (* k (abs (- x y)))))` labeled `obl_{id}_lipschitz`. Note: SMT-LIB's standard does not include `abs` as a built-in — this assertion would fail on most solvers without a prior definition or axiom for `abs`. This is a stage-0 approximation.

- **`fn parse_lipschitz_bound(text: &str) -> Term`** (`predicate.rs:457`) — **Private.** Strips an optional `k = ` prefix, tries `i64` parse (returns `Term::int(n)`), tries decimal parse with dot split (encodes as `Rational { num: whole*10^|frac| + frac, den: 10^|frac| }`), falls back to `Rational { num: 1, den: 1 }` for unrecognized input. Handles `"k = 1.0"` → `Rational { num: 10, den: 10 }` (which simplifies to 1.0).

- **`pub fn translate_bag(bag: &ObligationBag, interner: &cssl_hir::Interner) -> Vec<(ObligationId, Result<Query, TranslationError>)>`** (`predicate.rs:481`) — Bulk translation: iterates the bag, calls `translate_obligation` on each, collects `(id, result)` pairs. Does not short-circuit on first failure.

- **`#[cfg(test)] mod tests`** (`predicate.rs:490`) — 15 unit tests:
  - `parse_integer_comparison`: `"v > 0"` → `"(> v 0)"`
  - `parse_ge_le_eq_ne`: all four comparison operators
  - `parse_conjunction`: `"v >= 0 && v < 100"` → `"(and (>= v 0) (< v 100))"`
  - `parse_disjunction`: `"v == 1 || v == 2"` → `"(or (= v 1) (= v 2))"`
  - `parse_set_membership`: `"v in {44100, 48000, 96000, 192000}"` → 4-way disjunction
  - `parse_unicode_in_glyph`: `"v ∈ {0, 1}"` → disjunction
  - `parse_parenthesized`: `"(v > 0) && (v < 100)"` → conjunction
  - `parse_negative_literal`: `"v > -5"` → `"(> v (- 5))"` (unary minus, not subtraction)
  - `parse_rejects_malformed`: missing RHS, leading operator, empty string
  - `parse_plain_variable_is_term`: bare identifier is a valid boolean term
  - `translate_predicate_emits_declare_fn_and_assert`: full emit check with label
  - `translate_tag_emits_stub_query`: tag label present
  - `translate_lipschitz_emits_lra_query`: LRA logic, `x/y/f` declared, `abs` and `lipschitz` label present
  - `lipschitz_bound_k_equals_1_parses`: `"k = 1.0"` → `Rational { num: 10, den: 10 }`
  - `lipschitz_bound_bare_int_parses`: `"2"` → `Term::int(2)`
  - `lipschitz_bound_unrecognized_falls_back_to_1`: fallback
  - `translate_bag_processes_all_obligations`: 2 obligations all Ok
  - `translate_bag_records_parse_failure`: malformed → Err
  - `predicate_with_audio_callback_refinement_form`: sample-rate set membership (44100/48000/96000/192000)

  (15 documented but 18 test bodies observed — the test module calls a private helper `mk_obligation` as well.)

---

### 3.6 `cssl-smt/src/solver.rs` (334 lines)

**Purpose:** Implements the solver dispatch layer. Defines the `Solver` trait and two concrete implementations (`Z3CliSolver`, `Cvc5CliSolver`), the subprocess invocation function `run_cli_text`, canonical argument builders, and the top-level `discharge` function that connects `ObligationBag` to solver verdicts.

**Items:**

- **`enum SolverKind`** (`solver.rs:21`) — `Z3`, `Cvc5`. Copy type.
  - **`impl SolverKind`**:
    - `pub const fn binary(self) -> &'static str` (`solver.rs:31`) — Maps to `"z3"` or `"cvc5"` (binary names expected on `PATH`).

- **`trait Solver`** (`solver.rs:41`) — Two required methods, one provided:
  - `fn kind(&self) -> SolverKind` — Which solver.
  - `fn check(&self, q: &Query) -> Result<Verdict, SolverError>` — Run a query.
  - `fn check_text(&self, smtlib: &str) -> Result<Verdict, SolverError>` (`solver.rs:56`) — **Provided default.** Calls `run_cli_text(self.kind(), smtlib, &default_args_for(self.kind()))`. Allows integrations that build SMT-LIB text directly (e.g., from a `smt_query_text()` method on a domain type) to bypass the `Query` struct.

- **`enum SolverError`** (`solver.rs:63`) — Four variants via `thiserror`:
  - `BinaryMissing { binary: &'static str }` — Binary not found on `PATH`.
  - `NonZeroExit { binary: &'static str, status: i32 }` — Solver exited non-zero.
  - `UnparseableOutput { binary: &'static str, output: String }` — First stdout line is not `sat`/`unsat`/`unknown`.
  - `Io(#[from] std::io::Error)` — OS-level subprocess error.

- **`struct Z3CliSolver`** (`solver.rs:82`) — Fields: `extra_args: Vec<String>`. Derives `Default`.
  - **`impl Solver for Z3CliSolver`** (`solver.rs:88`):
    - `fn kind(&self) -> SolverKind` → `SolverKind::Z3`
    - `fn check(&self, q: &Query) -> Result<Verdict, SolverError>` → calls `run_cli(SolverKind::Z3, q, &default_z3_args(&self.extra_args))`

- **`struct Cvc5CliSolver`** (`solver.rs:99`) — Fields: `extra_args: Vec<String>`. Derives `Default`.
  - **`impl Solver for Cvc5CliSolver`** (`solver.rs:104`):
    - `fn kind(&self) -> SolverKind` → `SolverKind::Cvc5`
    - `fn check(&self, q: &Query) -> Result<Verdict, SolverError>` → calls `run_cli(SolverKind::Cvc5, q, &default_cvc5_args(&self.extra_args))`

- **`fn default_z3_args(extra: &[String]) -> Vec<String>`** (`solver.rs:114`) — **Private.** Returns `["-in", "-smt2"] + extra`. The `-in` flag tells Z3 to read from stdin; `-smt2` sets the input language.

- **`fn default_cvc5_args(extra: &[String]) -> Vec<String>`** (`solver.rs:120`) — **Private.** Returns `["--lang=smt2", "-"] + extra`. The `-` argument tells CVC5 to read from stdin.

- **`fn run_cli(kind: SolverKind, q: &Query, args: &[String]) -> Result<Verdict, SolverError>`** (`solver.rs:126`) — **Private.** Calls `emit_smtlib(q)` and then `run_cli_text`.

- **`pub fn run_cli_text(kind: SolverKind, smtlib: &str, args: &[String]) -> Result<Verdict, SolverError>`** (`solver.rs:145`) — **Public.** The core subprocess invocation. Uses `std::process::Command` to spawn the solver binary with `stdin(Stdio::piped())`, writes the SMT-LIB text to stdin, calls `wait_with_output()`, converts stdout bytes to UTF-8 (lossily), takes the first line, and matches `"sat"` → `Verdict::Sat`, `"unsat"` → `Verdict::Unsat`, `"unknown"` → `Verdict::Unknown`, non-zero-exit → `SolverError::NonZeroExit`, anything else → `SolverError::UnparseableOutput`. On `std::io::ErrorKind::NotFound` from spawn, returns `SolverError::BinaryMissing`.

- **`pub fn default_args_for(kind: SolverKind) -> Vec<String>`** (`solver.rs:187`) — **`#[must_use]`**. Public entry for callers that use `check_text` directly and need the canonical arg lists. Delegates to the private helpers.

- **`pub fn discharge<S: Solver>(obligations: &cssl_hir::ObligationBag, solver: &S) -> Vec<(cssl_hir::ObligationId, Result<Verdict, SolverError>)>`** (`solver.rs:202`) — Top-level integration function. Iterates the obligation bag, calls `build_stub_query` on each, calls `solver.check()`, collects `(id, result)` pairs.

- **`fn build_stub_query(_o: &cssl_hir::RefinementObligation) -> Query`** (`solver.rs:216`) — **Private.** Stage-0 stub. Ignores the obligation's actual content and returns a trivial query with logic `ALL` asserting `true`. This means `discharge()` always produces `Verdict::Sat` (trivially satisfiable) from the solver, which in the refinement-checking convention (negation is checked) would be interpreted as a violation. In practice at stage-0, this function is a shape-test placeholder — **the semantic result is meaningless**. Comment at `solver.rs:196` explicitly states this.

- **`#[cfg(test)] mod tests`** (`solver.rs:222`) — 9 unit tests: `SolverKind` binary names, `Z3CliSolver` default extra_args empty, default z3 args include `-in` and `-smt2`, default cvc5 args include `--lang=smt2`, `build_stub_query` asserts `true`, `SolverError` display formatting, `default_args_for` round-trips by kind, `check_text` default method dispatches via `run_cli_text` (handles either `Ok(_)` on machines with z3 or `BinaryMissing`), `run_cli_text` binary-missing contract.

---

## 3.7 `cssl-staging/src/lib.rs` (455 lines)

**Purpose:** Implements the F4 `@staged` specialization data model and HIR analysis. Collects all `@staged` functions from a `HirModule`, classifies their arguments, counts `#run` sites in bodies, and provides a `Specializer` container for downstream passes to record specialization decisions.

**Items:**

- **`enum StageArgKind`** (`lib.rs:34`) — Three variants: `CompTime`, `Runtime`, `Polymorphic`. Copy type. All args default to `Runtime` at collection time; the downstream pass (T8-phase-2) would promote specific args to `CompTime` based on call-site analysis.

- **`struct StageArg`** (`lib.rs:46`) — Fields: `index: usize` (0-based position), `name: Option<Symbol>` (from binding pattern if available), `kind: StageArgKind`. Represents the staging classification of one function parameter.

- **`struct StagedDecl`** (`lib.rs:56`) — Fields: `name: Symbol`, `def: cssl_hir::DefId`, `args: Vec<StageArg>`, `run_sites: u32`. Complete metadata for one `@staged`-annotated function.
  - **`impl StagedDecl`**:
    - `pub fn from_fn(f: &HirFn, interner: &Interner) -> Option<Self>` (`lib.rs:71`) — **`#[must_use]`**. Returns `None` if the function has no `@staged` attribute. Otherwise builds `args` from `f.params` (all `Runtime` initially), counts `run_sites` by walking the body.

- **`fn attr_matches(attr: &HirAttr, interner: &Interner, expected: &str) -> bool`** (`lib.rs:95`) — **Private.** Returns `true` iff the attribute has exactly one path segment that resolves to `expected` in the interner. Used to detect `@staged`.

- **`fn binding_name(pat: &cssl_hir::HirPattern) -> Option<Symbol>`** (`lib.rs:102`) — **Private.** Extracts the bound name from a `HirPatternKind::Binding` pattern; returns `None` for other pattern kinds (tuple, struct, wildcard).

- **`fn count_run_sites(f: &HirFn) -> u32`** (`lib.rs:110`) — **Private.** Returns the count of `HirExprKind::Run` nodes in the function body. Delegates to `count_block`.

- **`fn count_block(b: &cssl_hir::HirBlock, n: &mut u32)`** (`lib.rs:118`) — **Private.** Iterates block statements, calling `count_expr` on expression statements and let-initializers. Also counts in the trailing expression if present.

- **`fn count_expr(e: &cssl_hir::HirExpr, n: &mut u32)`** (`lib.rs:132`) — **Private.** The main recursive walker. Dispatches on `HirExprKind` variants to recurse into sub-expressions. Increments `*n` for `HirExprKind::Run { expr }` (and recurses into the inner expr). Handles all known HIR expression kinds: `Block`, `If`, `For`, `While`, `Loop`, `Match`, `Call`, `Binary`, `Assign`, `Pipeline`, `Compound`, `Unary`, `Field`, `Try`, `Paren`, `Cast`, `Index`, `Return`, `Break`, `Tuple`, `Lambda`, `With`, `Region`, `TryDefault`, `Range`, `Array` (both `List` and `Repeat` variants), `Struct`, `Perform`. Leaves that produce no sub-expressions: `Literal`, `Path`, `Continue`, `SectionRef`, `Error`.

- **`pub fn collect_staged_fns(module: &HirModule, interner: &Interner) -> Vec<StagedDecl>`** (`lib.rs:261`) — **`#[must_use]`**. Top-level HIR walk. Iterates `module.items`, delegates to `collect_item`.

- **`fn collect_item(item: &HirItem, interner: &Interner, out: &mut Vec<StagedDecl>)`** (`lib.rs:269`) — **Private.** Dispatches on `HirItem` variants: `Fn` → try `StagedDecl::from_fn`; `Impl` → iterate impl's fn list; `Module` → recurse into sub-items if present. All other item kinds (`Struct`, `Enum`, `Trait`, `TypeAlias`, `Const`, `Static`, `Use`, `ExternBlock`, `Effect`) are silently skipped.

- **`struct RunMarker`** (`lib.rs:295`) — Fields: `hir_id: cssl_hir::HirId`. Represents a single `#run` site location for the comptime-eval queue. Currently no population code exists — the struct is defined but never constructed outside tests.

- **`struct Specializer`** (`lib.rs:303`) — Fields: `sites: Vec<SpecializationSite>`. Derives `Default`.
  - **`impl Specializer`**:
    - `pub fn new() -> Self` (`lib.rs:329`) — `Self::default()`.
    - `pub fn len(&self) -> usize` (`lib.rs:335`) — Site count.
    - `pub fn is_empty(&self) -> bool` (`lib.rs:341`) — Delegates to `len`.

- **`struct SpecializationSite`** (`lib.rs:309`) — Fields: `caller: cssl_hir::DefId`, `callee: cssl_hir::DefId`, `args: Vec<StageArg>`. Represents one call-site specialization request.

- **`enum StagingError`** (`lib.rs:317`) — Two variants: `RuntimeSideEffect { msg: String }` (a `#run` site has side-effects incompatible with comptime eval), `NoStageArgs { name: Symbol }` (no comptime args, specialization trivial). Placeholder populated at T8-phase-2; no code currently produces these errors.

- **`pub const STAGE0_SCAFFOLD: &str`** (`lib.rs:348`) — `env!("CARGO_PKG_VERSION")`.

- **`#[cfg(test)] mod tests`** (`lib.rs:350`) — 9 unit tests, all using a `prep(src: &str)` helper that runs lex + parse + HIR-lowering on a source string. Tests: `scaffold_version_present`, `empty_module_yields_no_staged_decls`, `staged_fn_is_collected` (1 arg collected), `non_staged_fn_is_skipped`, `run_site_count_tracks_hash_run_exprs` (1 `#run` → `run_sites == 1`), `stage_arg_kind_defaults_to_runtime`, `specializer_starts_empty`, `stage_arg_indices_increment` (0/1/2 for 3-arg fn), `specialization_site_constructs` (struct literal construction), `staged_decl_equality` (clone+eq).

---

## 3.8 `cssl-futamura/src/lib.rs` (284 lines)

**Purpose:** Data model for Futamura projection orchestration. Defines the three projection levels as an enum, a `Projection` record tying a source identifier to a level and artifact hash, a `FixedPointRecord` for P3 convergence checking, and an `Orchestrator` that accumulates projections and fixed-point records.

**Items:**

- **`enum FutamuraLevel`** (`lib.rs:32`) — Three variants: `P1`, `P2`, `P3`. Derives `Ord` + `PartialOrd` (P1 < P2 < P3 by declaration order).
  - **`impl FutamuraLevel`**:
    - `pub const fn label(self) -> &'static str` (`lib.rs:44`) — `"futamura-P1"`, `"futamura-P2"`, `"futamura-P3"`.
    - `pub const ALL: [Self; 3]` (`lib.rs:53`) — Array `[P1, P2, P3]`.
    - `pub const fn order(self) -> u32` (`lib.rs:57`) — Returns 1, 2, 3 for P1/P2/P3. Used for diagnostics.

- **`struct Projection`** (`lib.rs:67`) — Fields: `source: String` (module path hash or identifier), `level: FutamuraLevel`, `artifact_hash: String` (BLAKE3-compatible hex digest of the produced artifact).
  - **`impl Projection`**:
    - `pub fn new(source: impl Into<String>, level: FutamuraLevel, artifact_hash: impl Into<String>) -> Self` (`lib.rs:80`) — Constructor.

- **`struct FixedPointRecord`** (`lib.rs:96`) — Fields: `generation: u32`, `hash_n: String`, `hash_n_plus_1: String`. Represents one iteration of the P3 fixed-point loop.
  - **`impl FixedPointRecord`**:
    - `pub fn new(generation: u32, hash_n: impl Into<String>, hash_n_plus_1: impl Into<String>) -> Self` (`lib.rs:105`) — Constructor.
    - `pub fn converged(&self) -> bool` (`lib.rs:119`) — `self.hash_n == self.hash_n_plus_1`. This is the core P3 fixed-point check: if two successive generation hashes are bit-identical, specialization has converged.

- **`enum FutamuraError`** (`lib.rs:125`) — Three variants:
  - `FixedPointDiverged { max_gen: u32, last_hash: String, next_hash: String }` — P3 convergence failed after maximum generations.
  - `SpecializerMissing` — P2 requires a registered specializer but none is present.
  - `WrongLevel { found: FutamuraLevel }` — P3 requires a P2-compatible specializer.
  Note: None of these errors are currently produced by any code in this crate — they are error types for the future P2/P3 orchestration logic.

- **`struct Orchestrator`** (`lib.rs:143`) — Fields (private): `projections: Vec<Projection>`, `fixed_points: Vec<FixedPointRecord>`. Derives `Default`.
  - **`impl Orchestrator`**:
    - `pub fn new() -> Self` (`lib.rs:151`) — `Self::default()`.
    - `pub fn record(&mut self, p: Projection)` (`lib.rs:157`) — Append a projection record.
    - `pub fn record_fixed_point(&mut self, fp: FixedPointRecord)` (`lib.rs:162`) — Append a fixed-point record.
    - `pub fn latest(&self) -> Option<&Projection>` (`lib.rs:168`) — **`#[must_use]`**. Last recorded projection.
    - `pub fn projections_at(&self, level: FutamuraLevel) -> impl Iterator<Item = &Projection>` (`lib.rs:173`) — Filter by level; returns an iterator.
    - `pub fn all_converged(&self) -> bool` (`lib.rs:179`) — **`#[must_use]`**. `true` iff every `FixedPointRecord` has converged. Vacuously `true` when empty (correct: no divergence observed means no evidence of divergence).
    - `pub fn projection_count(&self) -> usize` (`lib.rs:185`) — **`#[must_use]`**.
    - `pub fn fixed_point_count(&self) -> usize` (`lib.rs:191`) — **`#[must_use]`**.

- **`pub const STAGE0_SCAFFOLD: &str`** (`lib.rs:197`) — `env!("CARGO_PKG_VERSION")`.

- **`#[cfg(test)] mod tests`** (`lib.rs:200`) — 10 unit tests: `scaffold_version_present`, `three_levels_enumerated`, `levels_ordered_p1_p2_p3` (order values and `<` comparisons), `fixed_point_converges_when_hashes_match`, `fixed_point_diverges_when_hashes_differ`, `orchestrator_records_projections` (count and `latest()`), `orchestrator_filters_by_level` (P1 count = 2), `orchestrator_all_converged_when_empty` (vacuous truth), `orchestrator_all_converged_requires_every_match` (one mismatch → false), `level_labels_unique`, `projection_roundtrips`.

---

## 3.9 `cssl-macros/src/lib.rs` (343 lines)

**Purpose:** Implements the Racket-lineage set-of-scopes hygiene model. Provides `ScopeId`, `HygieneMark`, `SyntaxObject`, `ScopeAllocator`, and the macro tier/registry types. The expansion engine (pattern-matching, `#run` evaluation) is not present — this is purely the data model layer.

**Items:**

- **`enum MacroTier`** (`lib.rs:29`) — Three variants: `AttrMacro` (Tier-1, `@attr`-macros), `Declarative` (Tier-2, pattern-rewrite), `Procedural` (Tier-3, `#run` proc-macros). Copy type.
  - **`impl MacroTier`**:
    - `pub const fn label(self) -> &'static str` (`lib.rs:43`) — `"tier-1-attr"`, `"tier-2-declarative"`, `"tier-3-proc"`.
    - `pub const ALL: [Self; 3]` (`lib.rs:51`) — Array `[AttrMacro, Declarative, Procedural]`.

- **`struct ScopeId(pub u32)`** (`lib.rs:58`) — Newtype over `u32`. Derives `Copy`, `Ord`, `PartialOrd`, `Hash`. Used as a key in `BTreeSet` for deterministic ordering.

- **`struct HygieneMark`** (`lib.rs:65`) — Field (private): `scopes: BTreeSet<ScopeId>`. Represents the set of scopes under which a syntax object's binding is in scope. Derives `Default` (empty set).
  - **`impl HygieneMark`**:
    - `pub fn new() -> Self` (`lib.rs:73`) — `Self::default()`.
    - `pub fn add(&mut self, s: ScopeId)` (`lib.rs:78`) — Insert scope.
    - `pub fn remove(&mut self, s: ScopeId)` (`lib.rs:83`) — Remove scope (idempotent via `BTreeSet::remove`).
    - `pub fn contains(&self, s: ScopeId) -> bool` (`lib.rs:88`) — Membership test.
    - `pub fn flip(&mut self, s: ScopeId)` (`lib.rs:94`) — **The core hygiene operation.** XOR: removes if present, inserts if absent. This implements Flatt's scope-set algorithm where each macro expansion introduces a fresh scope and flips it on the introduced syntax — if the same scope appears twice (because of nested expansion and reference), it cancels out, preventing accidental capture.
    - `pub fn union(&self, other: &Self) -> Self` (`lib.rs:104`) — Returns a new mark containing all scopes from both marks. Used when combining marks from different expansion steps.
    - `pub fn len(&self) -> usize` (`lib.rs:113`) — **`#[must_use]`**. Scope count.
    - `pub fn is_empty(&self) -> bool` (`lib.rs:119`) — **`#[must_use]`**.

- **`struct SyntaxObject`** (`lib.rs:127`) — Fields: `text: String` (the identifier spelling), `mark: HygieneMark`. Two `SyntaxObject` values are `PartialEq` only if both text and mark agree — this is the hygiene comparison.
  - **`impl SyntaxObject`**:
    - `pub fn fresh(text: impl Into<String>) -> Self` (`lib.rs:136`) — **`#[must_use]`**. Builds with empty mark (parser-provided token).
    - `pub fn with_mark(text: impl Into<String>, mark: HygieneMark) -> Self` (`lib.rs:145`) — **`#[must_use]`**. Builds with explicit mark.
    - `pub fn flip_scope(&mut self, s: ScopeId)` (`lib.rs:153`) — Delegates to `mark.flip(s)`. Applied to all syntax objects introduced by a macro expansion.

- **`struct ScopeAllocator`** (`lib.rs:159`) — Field (private): `next: u32`. Monotonically incrementing fresh-scope generator.
  - **`impl ScopeAllocator`**:
    - `pub const fn new() -> Self` (`lib.rs:167`) — Const constructor, `next = 0`.
    - `pub fn fresh(&mut self) -> ScopeId` (`lib.rs:173`) — Allocates and returns the next `ScopeId`. Uses `saturating_add(1)` to avoid overflow panic (would wrap at `u32::MAX` repetitions, which is safe in practice).
    - `pub const fn count(&self) -> u32` (`lib.rs:180`) — Total scopes allocated so far.

- **`struct MacroDecl`** (`lib.rs:187`) — Fields: `name: String`, `tier: MacroTier`. Registered macro declaration.

- **`struct MacroRegistry`** (`lib.rs:194`) — Field (private): `macros: Vec<MacroDecl>`. Derives `Default`.
  - **`impl MacroRegistry`**:
    - `pub fn new() -> Self` (`lib.rs:200`) — `Self::default()`.
    - `pub fn register(&mut self, decl: MacroDecl)` (`lib.rs:206`) — Append macro declaration.
    - `pub fn lookup(&self, name: &str) -> Option<&MacroDecl>` (`lib.rs:211`) — **`#[must_use]`**. Linear scan. No deduplication or shadowing — if two macros share a name, the first registered wins (due to `find()`). This is a stage-0 simplification; a production macro registry would use a `HashMap` and handle redefinition.
    - `pub fn len(&self) -> usize` (`lib.rs:217`) — **`#[must_use]`**.
    - `pub fn is_empty(&self) -> bool` (`lib.rs:223`) — **`#[must_use]`**.

- **`enum MacroError`** (`lib.rs:229`) — Three variants:
  - `UnknownMacro { name: String }` — Invocation of an unregistered macro.
  - `PatternMismatch { message: String }` — Tier-2 pattern failed.
  - `SandboxViolation { op: String }` — Tier-3 `#run` escaped sandbox.
  None currently produced by any code in this crate — error types for the T8-phase-2 expansion engine.

- **`pub const STAGE0_SCAFFOLD: &str`** (`lib.rs:244`) — `env!("CARGO_PKG_VERSION")`.

- **`#[cfg(test)] mod tests`** (`lib.rs:246`) — 12 unit tests: `scaffold_version_present`, `three_tiers_enumerated`, `tier_labels_unique`, `hygiene_mark_add_and_contains` (add/contains/len), `hygiene_flip_is_xor` (flip twice → absent), `hygiene_union_merges` (both scopes present), `syntax_object_equality_respects_mark` (different marks → not equal), `syntax_object_same_text_same_mark_equal`, `scope_allocator_fresh_unique` (two fresh scopes differ, count==2), `macro_registry_roundtrip` (register + lookup), `macro_registry_unknown_returns_none`.

---

## 4. SLICE NOTES

### 4.1 Test Coverage

All tests are unit-level, embedded in `#[cfg(test)]` modules within the source files. There are no separate integration test files (`tests/` directories) in any of the four crates.

| Crate | Test count | Coverage character |
|-------|-----------|-------------------|
| `cssl-smt` (across 6 files) | ~56 tests | Good shape coverage for all render paths; `emit_smtlib` ordering; obligation translation for all three kinds. No end-to-end solver tests on CI (binary availability not assumed). |
| `cssl-staging` | ~9 tests | Smoke-level; exercises the HIR parse → collect path. Does not test `count_expr` for most expression kinds (those paths are only reachable if the parser emits them). |
| `cssl-futamura` | ~10 tests | Comprehensive for the data model; all query/filter methods covered. No algorithmic coverage (no algorithm present). |
| `cssl-macros` | ~12 tests | Comprehensive for the hygiene model; flip, union, scope allocation, registry lookup all covered. No expansion tests (no expansion present). |

### 4.2 Stubs, Deferred Work, and TODOs

The following are the known incomplete areas, drawn from inline documentation:

**`cssl-smt`:**

1. **`discharge()` uses `build_stub_query()`** (`solver.rs:216`) — Every obligation maps to a trivially-true query. The function comment states: "Stage-0 stub: every obligation becomes a trivially-true query." This means the `discharge` API exists but produces semantically meaningless results.

2. **Lipschitz `abs` not SMT-LIB standard** (`predicate.rs:444`) — The Lipschitz obligation query uses `Term::app("abs", ...)`, but `abs` is not a built-in in SMT-LIB's standard arithmetic theories. Z3 provides it via `(declare-fun abs ...)` or non-linear arithmetic extensions, but CVC5 may not. A proper encoding would define `abs` as `(ite (>= x 0) x (- x))`. This is a correctness gap in the Lipschitz path.

3. **T9-phase-2 deferred items** (documented in `lib.rs:17-23`): Direct `z3-sys`/`cvc5-sys` FFI, KLEE symbolic execution, proof-certificate emission + Ed25519 signing, per-obligation on-disk cache, full HIR-expression → SMT-Term translation (bypassing predicate-text re-parsing), incremental solving (`push`/`pop`).

4. **T9-phase-2b deferred items** (`predicate.rs:29-37`): Real HIR-expression → Term translation, multi-binder predicates, float-arithmetic predicates (stage-0 assumes `Int` sort), array/tuple/struct access in predicates, user-defined function calls in predicates.

5. **Tag obligation is a stub** (`predicate.rs:398-403`): `ObligationKind::Tag` emits `(assert true)` — actual tag-dictionary resolution is deferred.

6. **`UnsupportedKind` variant is currently unused** — Since `Lipschitz` is now handled by `translate_obligation`, the `UnsupportedKind` variant of `TranslationError` has no reachable producer. It may be dead code for the moment.

7. **No `z3` crate dependency in `cssl-smt/Cargo.toml`** — The workspace root lists `z3 = "0.12"` as an available dep, but `cssl-smt` does not declare it. All solver interaction is via subprocess. This matches the stated design intent but means the `z3` crate in the workspace dependency table is currently used by no crate in this slice.

**`cssl-staging`:**

8. **Actual specialization transform not implemented** (`lib.rs:15-16`): Cloning a function body and constant-propagating comptime args. The `Specializer` and `SpecializationSite` types are empty shells.

9. **`RunMarker` is never constructed** beyond test code — the `RunMarker` struct is defined (`lib.rs:295`) but there is no code that builds `RunMarker` values from discovered `#run` sites. The run-site *count* is tracked in `StagedDecl::run_sites`, but the individual site locations are not extracted.

10. **`StageArgKind` always `Runtime`** — `StagedDecl::from_fn` initializes all args as `Runtime`. The `CompTime` and `Polymorphic` variants have no producer.

11. **`StagingError` variants have no producer** — Neither `RuntimeSideEffect` nor `NoStageArgs` are ever constructed.

**`cssl-futamura`:**

12. **No actual partial evaluation** — The crate is a record-keeper, not an engine. P1/P2/P3 are described in module documentation but no specialization, compilation, or generation step is implemented here. All three `FutamuraError` variants have no producer.

13. **T16 fixed-point CI-gate deferred** (documented in `lib.rs:15`): Full fixed-point verification requires a running stage-1 compiler.

**`cssl-macros`:**

14. **No expansion engine** — Pattern-matching (Tier-2) and `#run` evaluation (Tier-3) are deferred to T8-phase-2. All three `MacroError` variants have no producer.

15. **`MacroRegistry::lookup` is linear scan** (`lib.rs:212`) — Uses `Iterator::find` over a `Vec`. For stage-0 with a small number of registered macros this is acceptable, but a production registry would use `HashMap`.

16. **Duplicate registration not detected** — Registering two macros with the same name silently allows both; `lookup` returns the first. No redefinition error or shadowing semantics.

### 4.3 Spec Divergences

- **`specs/20_SMT.csl`**: The spec describes both FFI-linked solver backends (`z3-sys`, `cvc5-sys`, `klee-sys`) and CLI fallbacks. The implementation uses only CLI (no FFI). KLEE is mentioned in `lib.rs` comments but absent from all code. The spec's proof-certificate emission (Ed25519-signed) and on-disk obligation-hash cache are described but not present. These are explicitly tagged T9-phase-2 deferrals in the code.

- **`specs/06_STAGING.csl` / `specs/19_FUTAMURA3.csl`**: The staging spec describes `@type_info`, `@fn_info`, `@module_info` reflection APIs and the transform-dialect pass-schedule emission. None of these are present. The Futamura spec describes actual partial-evaluation execution; none is implemented. Both are explicitly T8-phase-2 deferrals.

- **`specs/13_MACROS.csl`**: The spec describes three tiers of macros with expansion semantics. Only the data model for Tier-1 (registration), Tier-2 (no pattern representation beyond `MacroDecl`), and Tier-3 (no proc-macro sandbox) is present. The actual expansion algorithm is absent.

### 4.4 Dead Code and Surprises

- **`TranslationError::UnsupportedKind`**: As noted above, this variant was presumably created for the `Lipschitz` obligation kind, but since Lipschitz is now handled (T9-phase-2b landed), the variant has no reachable producer. The `Err(TranslationError::UnsupportedKind { kind: "..." })` call site that would have been there was replaced by the working implementation. The variant should either be removed or reserved for genuinely unsupported future kinds.

- **`Literal::Rational { num: 10, den: 10 }`** for `"k = 1.0"`: The Lipschitz bound parser encodes `1.0` as `Rational { num: 10, den: 10 }` rather than the fully-reduced `Rational { num: 1, den: 1 }`. There is no rational simplification step. Both forms are semantically equivalent for SMT-LIB emission (`(/ 10 10)` vs `(/ 1 1)`), but a downstream display pass or comparison would see them as unequal.

- **`Term` lacks `Hash`**: The `Term` enum derives `PartialEq` and `Eq` but not `Hash`. Since `App.args` is a `Vec<Term>` and `Forall.binders` is a `Vec<(String, Sort)>`, implementing `Hash` would require manual implementation. This means `Term` cannot be used as a `HashMap` key. For stage-0 where terms are only rendered to strings, this is fine.

- **`Specializer::is_empty` delegates to `len == 0` not `sites.is_empty()`**: `lib.rs:341` reads `self.sites.is_empty()`, which is correct, but the `is_empty` docstring at `lib.rs:342` says "Delegates to `len`." This is a documentation inconsistency (the actual code does call `self.sites.is_empty()`, which is equivalent but not literally through `len`).

- **`ScopeAllocator::fresh` uses `saturating_add`**: At `u32::MAX` scopes, the allocator silently stops advancing. This means allocating `2^32 + 1` scopes would produce a duplicate `ScopeId`. In any realistic use (macro expansion in a compilation unit), this limit is unreachable, but it is a correctness gap worth noting for completeness.

- **Workspace `z3 = "0.12"` dependency is orphaned**: The workspace declares `z3 = { version = "0.12", default-features = false }` as an available workspace dependency but no crate currently requests it via `z3.workspace = true`. The `cvc5-sys` and `klee-sys` entries are commented out. This suggests `cssl-smt` was at some point intended to optionally link the `z3` crate, but the CLI approach won out for stage-0.

### 4.5 Architecture Assessment

The four crates demonstrate clean separation of concerns and are appropriately minimal for a stage-0 bootstrap compiler. The SMT crate's CLI subprocess approach is a pragmatic choice that avoids the MSVC C++ toolchain requirement for native solver FFI on Windows. The staging and futamura crates correctly separate the data model (what was done / what should be done) from the execution engine (how to do it), making them extendable without entangling T8-phase-2 work with the already-landed T8-phase-1 shape. The macro hygiene model is theoretically correct (Racket set-of-scopes) and well-tested.

The main risk is that `discharge()` in `solver.rs` returns semantically meaningless verdicts at stage-0 (always checking `true` rather than the actual obligation predicate). Code that calls `discharge()` and acts on the results would silently fail to catch refinement violations. The `translate_bag` / `translate_obligation` path in `predicate.rs` is the correct integration point for real obligation discharge, but it is not yet wired into `discharge()`.

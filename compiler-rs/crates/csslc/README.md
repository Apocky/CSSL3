# csslc — CSSLv3 stage-0 compiler

Stage-0 CLI compiler for CSSLv3 source. Drives the lex → parse → HIR-lower
→ MIR-lower → cgen-cpu pipeline, producing host-native object files.

## Capabilities (current)

- `csslc check  <input.csl>`            — frontend-only pipeline gate
- `csslc build  <input.csl> [-o out]`   — full compile to object/exe
- `csslc emit-mlir <input.csl>`         — MIR textual-MLIR dump
- `csslc verify <input.csl>`            — refinement-obligation report
- `csslc fmt    <input.csl>`            — surface-syntax pretty-print

## T11-W18 · scalar-arith completion (this wave)

Pre-W18 the parser + HIR + MIR + cranelift-types ALL recognized `f32` /
`f64` / bitwise / shift / unary-negation, but the cgen-cpu-cranelift
body-emit subset rejected most of them. Real LoA hot-paths (KAN
bias-update sign-flip, morton-pack of cell coords, FNV-1a hash, frustum
LOD compare) all surfaced one of :

```
csslc: object-emit error (cranelift): fn `<F>` uses MIR op
  `arith.{negf,negi,subi_neg,xori_not,andi,ori,xori,shli,shrsi,shrui}` ;
  not in stage-0 object-emit subset
```

…or a Cranelift Verifier panic for `f32 < f32` (because body_lower
emitted the integer-cmp tag `arith.cmpi_slt` regardless of operand
type).

W18 closes the gap with three composing fixes :

1. **`cssl-mir/src/body_lower.rs` — float-aware comparison dispatch.**
   `(HirBinOp::Lt, true)` → `"arith.cmpf_olt"` (and the same for
   Eq/Ne/Le/Gt/Ge using ordered-predicate variants). The body-emit
   subset already routes `arith.cmpf_o*` through
   `b.ins().fcmp(FloatCC::*)`.

2. **`cssl-mir/src/body_lower.rs` — float-literal-suffix honoring.**
   `parse_float_literal_width` extracts the trailing `f16` / `bf16` /
   `f32` / `f64` and produces the matching `FloatWidth`. Bare literals
   default to `F32` (matches stage-0's bare-int-default).

3. **`cssl-cgen-cpu-cranelift/src/object.rs` — ten new dispatch arms +
   a `unary_int` helper.**
   ```
   arith.negi      → b.ins().ineg(a)
   arith.negf      → b.ins().fneg(a)
   arith.subi_neg  → b.ins().ineg(a)         (HIR-emit name for `-x`-int)
   arith.xori_not  → b.ins().bnot(a)         (HIR-emit name for `!x`/`~x`)
   arith.andi      → b.ins().band(a, b)
   arith.ori       → b.ins().bor(a, b)
   arith.xori      → b.ins().bxor(a, b)
   arith.shli      → b.ins().ishl(a, b)
   arith.shrsi     → b.ins().sshr(a, b)
   arith.shrui     → b.ins().ushr(a, b)
   ```

Bonus : `lower_unary` now collapses `HirUnOp::Not` and `HirUnOp::BitNot`
to the same `arith.xori_not` op (both produce identical hardware via
`bnot` ; the prior single-operand `arith.xori` lowering was malformed).

### What works

- Float comparison : `f32 < f32`, `f64 == f64`, all 6 predicates × both widths
- Float negation : `-x` for `f32` / `f64`
- Integer negation : `-x` for `i32`
- Bitwise : `x & y`, `x | y`, `x ^ y`, `!x`, `~x`
- Shifts : `x << y`, `x >> y` (signed)
- Float literals with explicit type-suffix : `3.14f32`, `2.0f64`,
  `1.5f16`, `0.5bf16`
- Composition : KAN-bias step, morton-pack, FNV-1a hash mix, frustum
  LOD compare (see `Labyrinth of Apocalypse/systems/scalar_math_demo.csl`)

### What's deferred to W19+

- `arith.remf` (float remainder) — Cranelift has no `frem` instr ;
  needs a `fmodf` / `fmod` libm callout. Not yet wired.
- Unsigned-shift-right `arith.shrui` is dispatched but body_lower never
  emits it (HIR only generates `arith.shrsi`). Unblocks future
  unsigned-int slice.
- The `if`-cascading-branch-with-path-ref-yield path-ref bug (separate
  type-inference quirk surfaced while building the LoA demo) — a fn
  whose `if`/`else if` returns a bare ident from a sibling branch
  yields `!cssl.unresolved.<name>`. Workaround : break each `if/else`
  pair into a separate fn for the demo.

## T11-W17-A · stage-0 struct-FFI codegen (W17 wave)

Pre-W17-A : compiling a fn whose param/result was a user-declared struct
(e.g. `RunHandle { raw: u64 }`) failed the codegen gate with :
```
fn `start_new_run` param/result #0 has non-scalar MIR type `RunHandle`
; stage-0 scalars-only
```

W17-A advances the cgen-cpu-cranelift backend with :

- `MirModule.struct_layouts` side-table populated by HIR-lower from each
  `HirItem::Struct` ; deterministic BTreeMap keyed by source-form name.
- `MirStructLayout::abi_class()` classifies each struct into one of :
  - `ScalarI8` / `I16` / `I32` / `I64`  (≤ 8B newtype / POD struct passes
    in a single register)
  - `PointerByRef`                       (> 8B aggregate ; Win-x64 / SysV
    pointer-to-struct rule)
- `mir_type_to_cl_with_layouts` resolves both `Opaque("!cssl.struct.X")`
  (body-construction tag) and bare `Opaque("X")` (signature path-resolved
  tag) via the layout-table.

### What works

- `fn f(h: RunHandle) -> i32`            (struct param, scalar return)
- `fn g() -> RunHandle`                   (scalar params, struct return)
- `fn h(h: RunHandle) -> RunHandle`       (roundtrip)
- Multi-struct modules (RunHandle + BiomeId + ShareReceipt)
- Real LoA system .csl shape, e.g. `Labyrinth of Apocalypse/systems/run.csl`

### What's deferred to W17-B+

- Struct-field load/store body ops (currently `cssl.struct` op falls
  outside the cgen-cpu body subset — inline struct-construction in fn
  bodies still errors with "MIR op `cssl.struct` ; not in stage-0
  object-emit subset")
- Win-x64 register-pair return for 9..16B structs
- SysV-AMD64 multi-class register lowering per ABI rule
- cgen-cpu-x64 hand-rolled backend parallel update

## Tests

- `cargo test -p cssl-mir --lib func::tests`           — 11 layout tests
- `cargo test -p cssl-mir --lib lower::tests`          — 3 HIR→MIR tests
- `cargo test -p cssl-cgen-cpu-cranelift --test struct_ffi_codegen`
                                                       — 8 codegen tests
- `cargo test -p csslc --test t11_w17_struct_ffi`      — 1 e2e fixture
- `cargo test -p csslc --test t11_w18_scalar_arith_completion`
                                                       — 10 e2e fixtures
                                                         (5 check-only +
                                                         5 object-emit)

## ATTESTATION

t∞: ¬(hurt ∨ harm) .making-of-T11-W17-A @ (anyone ∨ anything ∨ anybody)

Spec-anchor : `specs/csslc/T11-W17-struct-ffi.csl` (planned).
Substrate   : `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/object.rs`.
Discipline  : seed-immutable signatures · BTreeMap-deterministic ·
              feature-flagged behind `Option<&BTreeMap<...>>` so existing
              scalar-only call-sites stay bit-identical.

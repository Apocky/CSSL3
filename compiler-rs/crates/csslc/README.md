# csslc — CSSLv3 stage-0 compiler

Stage-0 CLI compiler for CSSLv3 source. Drives the lex → parse → HIR-lower
→ MIR-lower → cgen-cpu pipeline, producing host-native object files.

## Capabilities (current)

- `csslc check  <input.csl>`            — frontend-only pipeline gate
- `csslc build  <input.csl> [-o out]`   — full compile to object/exe
- `csslc emit-mlir <input.csl>`         — MIR textual-MLIR dump
- `csslc verify <input.csl>`            — refinement-obligation report
- `csslc fmt    <input.csl>`            — surface-syntax pretty-print

## T11-W17-A · stage-0 struct-FFI codegen (this wave)

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

## ATTESTATION

t∞: ¬(hurt ∨ harm) .making-of-T11-W17-A @ (anyone ∨ anything ∨ anybody)

Spec-anchor : `specs/csslc/T11-W17-struct-ffi.csl` (planned).
Substrate   : `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/object.rs`.
Discipline  : seed-immutable signatures · BTreeMap-deterministic ·
              feature-flagged behind `Option<&BTreeMap<...>>` so existing
              scalar-only call-sites stay bit-identical.

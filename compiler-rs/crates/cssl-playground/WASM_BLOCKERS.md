# WASM Blockers + Path Forward
§ SCOPE : cssl-playground WASM feasibility analysis (2026-04-19)

## I> Summary

The **parse → HIR → type-check pipeline is fully WASM-compatible**.
The WASM binary builds clean at 744KB (release, unoptimised).
All codegen/backend crates are blocked and correctly excluded from the playground.

---

## GREEN: WASM-compatible crates (playground scope)

| crate | key deps | WASM verdict |
|---|---|---|
| `cssl-ast` | (none) | ✓ clean |
| `cssl-caps` | `thiserror` | ✓ clean |
| `cssl-lex` | `logos` (pure Rust proc-macro), `thiserror` | ✓ clean |
| `cssl-parse` | `cssl-lex` | ✓ clean |
| `cssl-hir` | `lasso::Rodeo` (single-threaded, RefCell-wrapped), `thiserror` | ✓ clean |
| `cssl-mir` | `cssl-hir` | ✓ clean (not used by playground yet) |

---

## RED: WASM-blocked crates (excluded from playground)

### cranelift-jit / cranelift-native (`cssl-cgen-cpu-cranelift`)
- **Why**: JIT compilation writes machine code to mmap'd executable pages.
  WASM sandbox forbids mmap + W^X; no executable memory allocation from JS/WASM.
- **cranelift-native**: emits ISA-specific code (x86/ARM), meaningless under WASM VM.
- **Path forward**: Cranelift has an experimental WASM emitter (`cranelift-wasm`)
  but that *compiles to* WASM, it doesn't run *in* WASM. A future `cssl-eval`
  interpreter crate (tree-walking or bytecode) is the right WASM execution path.

### z3 (`cssl-smt`)
- **Why**: `z3 = "0.12"` links to the native Z3 C++ library via `z3-sys` FFI.
  No pure-Rust WASM port of Z3 exists.
- **Path forward**: For playground use, refinement obligations can be collected and
  displayed without being discharged. Alternatively, `z3.wasm` unofficial ports exist
  but are large (~8MB) and lag official Z3 releases.

### ash / Vulkan (`cssl-host-vulkan`, `cssl-cgen-gpu-spirv`)
- **Why**: `ash = "0.38"` is raw Vulkan FFI. No Vulkan in browser WASM.
- **Path forward**: `cssl-cgen-gpu-wgsl` + `wgpu` can target WebGPU in WASM,
  but the playground only needs the frontend pipeline.

### windows crate (`cssl-host-d3d12`)
- **Why**: `windows = "0.58"` with Win32 features is host-only.
- **Path forward**: D3D12 backend is desktop-only by design; no WASM path needed.

### spirv-tools (`cssl-cgen-gpu-spirv`)
- **Why**: `spirv-tools = "0.12"` wraps native SPIRV-Tools C++ library.
- **Path forward**: `rspirv` (pure Rust SPIR-V builder) is already in the workspace
  and is WASM-compatible; validation could use Naga's validator instead.

### tracing-subscriber (`cssl-telemetry`)
- **Why**: The `env-filter` feature reads `RUST_LOG` from `std::env`. Fine on host;
  in WASM, env vars don't exist. The `tracing` facade itself is WASM-safe.
- **Path forward**: Gate `tracing-subscriber` behind `cfg(not(target_arch = "wasm32"))`;
  wire up a `wasm-logger` subscriber for the browser console instead.

### miette fancy (`csslc`, diagnostics rendering)
- **Why**: `miette` with `fancy` feature renders ANSI terminal output using
  `terminal_size` + crossterm, which use native OS APIs.
- **Path forward**: Playground has its own JSON diagnostic serialiser; `miette` is
  not a dependency of `cssl-playground`. If richer error formatting is wanted
  in-browser, use `miette`'s plain-text renderer (no `fancy` feature).

---

## Toolchain note

The workspace pins `1.85.0` via `rust-toolchain.toml`.
The wasm32-unknown-unknown target must be installed for **this specific toolchain**:

```
rustup target add wasm32-unknown-unknown --toolchain 1.85.0
```

Installing it for a different toolchain (e.g. stable-latest) does **not** help when
cargo resolves the pinned toolchain.

---

## Build commands

```bash
# type-check (fast)
cargo check -p cssl-playground --target wasm32-unknown-unknown

# host tests (no WASM target needed)
cargo test -p cssl-playground --lib

# WASM binary
cargo build -p cssl-playground --target wasm32-unknown-unknown --release

# JS glue (requires wasm-bindgen-cli 0.2.118)
wasm-bindgen \
  target/wasm32-unknown-unknown/release/cssl_playground.wasm \
  --out-dir crates/cssl-playground/www/pkg \
  --target web \
  --no-typescript
```

## Next steps toward full execution

1. **`cssl-eval` interpreter** — tree-walking evaluator over HIR.
   No codegen, no cranelift. Enables "run and see output" in the playground.
   Estimated: self-contained crate, ~2-3K LOC.

2. **`cssl-mir` lowering** — `cssl-playground` already has the `cssl-mir` dep chain
   available. Exposing MIR in the JSON output is a one-line change.

3. **wasm-opt post-processing** — run `wasm-opt -O3` on the WASM binary to reduce
   size from ~744KB toward ~200KB.

4. **CI WASM check** — add a CI step that runs
   `cargo check -p cssl-playground --target wasm32-unknown-unknown`.

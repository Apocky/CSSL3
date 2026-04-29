# cssl-playground build

## Prerequisites

```bash
# 1. WASM target for the pinned toolchain (one-time)
rustup target add wasm32-unknown-unknown --toolchain 1.85.0

# 2. wasm-bindgen-cli — must match crate version (0.2.118)
cargo install wasm-bindgen-cli --version "=0.2.118"

# 3. (optional) wasm-opt for size reduction
#    Windows: https://github.com/WebAssembly/binaryen/releases
```

## Build

```bash
# From compiler-rs/
cargo build -p cssl-playground --target wasm32-unknown-unknown --release

wasm-bindgen \
  target/wasm32-unknown-unknown/release/cssl_playground.wasm \
  --out-dir crates/cssl-playground/www/pkg \
  --target web \
  --no-typescript

# (optional) shrink WASM further
# wasm-opt -O3 -o crates/cssl-playground/www/pkg/cssl_playground_bg.wasm \
#              crates/cssl-playground/www/pkg/cssl_playground_bg.wasm
```

## Dev server (any static file server works)

```bash
# Python
python -m http.server 8080 --directory crates/cssl-playground/www

# Node
npx serve crates/cssl-playground/www

# Then open: http://localhost:8080
```

## Tests

```bash
# Host tests (no WASM target needed)
cargo test -p cssl-playground --lib

# Type-check for WASM
cargo check -p cssl-playground --target wasm32-unknown-unknown
```

## Size budget

| artifact | size |
|---|---|
| `cssl_playground.wasm` (cargo release) | 744 KB |
| `cssl_playground_bg.wasm` (after wasm-bindgen) | 684 KB |
| after `wasm-opt -O3` (estimated) | ~200 KB |

## WASM blockers

See `WASM_BLOCKERS.md` for the full analysis of which workspace crates can and
cannot compile to WASM, and the path forward to full execution in-browser.

# Sigil

**Hardware-first systems language with algebraic effects, autodiff, SMT verification, and multi-GPU backends.**

No LLVM. Cranelift JIT. Consent encoded structurally.

---

## What is Sigil?

Sigil (formally *CSSL — Caveman-Sigil-Substrate-Language*) is a compiled systems language for programs that span CPU, GPU, and real-time hardware. Effect tracking, automatic differentiation, formal verification, and information-flow control are core language features — not library add-ons.

## Six non-negotiable features

| | Feature | Description |
|--|---------|-------------|
| F1 | **Autodiff** | Source-to-source AD on MIR. `Jet<T,N>` higher-order. No LLVM dependency. |
| F2 | **Refinement Types** | SMT-backed `{v:T \| P(v)}`, Lipschitz bounds, control-flow narrowing. |
| F3 | **Effect System** | Row-polymorphic effect rows (Koka semantics). 28+ built-in effects. |
| F4 | **Staged Computation** | `@staged` + `#run` comptime. Futamura P1/P2/P3 self-applicable. |
| F5 | **Information Flow Control** | Jif-style DLM labels + declassification. Structurally enforced. |
| F6 | **Observability** | R18 telemetry, signed audit chain, oracle test modes. |

## Syntax

```sigil
// Differentiable SDF with Lipschitz bound — compiler-checked.
@differentiable
@lipschitz(k = 1.0)
fn sphere_sdf(p: vec3, r: f32'pos) -> f32 {
    length(p) - r
}

// Real-time audio callback — effect row is part of the type.
// Calling this from a GPU kernel is a compile error.
fn audio_callback<G: AudioDSPGraph>(buf: &mut [f32])
    / {CPU, SIMD256, NoAlloc, Deadline<1ms>, Realtime<Crit>, Audit<"audio-callback">}
{
    process_block(&mut G::default(), buf)
}

// Autodiff: backward-pass gradient of sphere_sdf, bit-exact vs analytic.
let normal = bwd_diff(sphere_sdf)(p, r).d_p.normalize();
```

Effect rows (`/ {CPU, SIMD256, ...}`) are checked and enforced by the compiler — they participate in type inference and unification.

## Architecture

**32 Rust crates** in a Cargo workspace. Stage0 bootstrap — Rust-hosted, throwaway once stage1 self-hosts.

```
Frontend
  cssl-lex          dual-surface lexer (logos + hand-rolled)
  cssl-parse        recursive-descent parser
  cssl-ast          CST + source-preservation + Diagnostic

Type System
  cssl-hir          typed HIR — Hindley-Milner + effect-row unification
  cssl-caps         Pony-6 capability checker (iso/trn/ref/val/box/tag)
  cssl-ifc          Jif-DLM information-flow labels (structurally encoded)
  cssl-effects      28+ effect registry + discipline checker

Transformation
  cssl-autodiff     source-to-source AD (19 primitives, fwd + bwd)
  cssl-jets         Jet<T,N> higher-order AD
  cssl-smt          Z3/CVC5/KLEE drivers + SMT-LIB emission
  cssl-staging      @staged comptime specializer
  cssl-futamura     Futamura P1/P2/P3 partial evaluation
  cssl-macros       Racket-hygienic macro system

IR
  cssl-mir          structured mid-level IR (MLIR dialect)
  cssl-lir          low-level IR + target orchestration
  cssl-mlir-bridge  melior FFI

Codegen (No LLVM)
  cssl-cgen-cpu-cranelift     CPU via Cranelift 0.115
  cssl-cgen-gpu-spirv         SPIR-V emitter
  cssl-cgen-gpu-dxil          DXIL emitter
  cssl-cgen-gpu-msl           MSL emitter
  cssl-cgen-gpu-wgsl          WGSL emitter

Host Runtimes
  cssl-host-vulkan            Vulkan 1.4
  cssl-host-level-zero        Intel Level-Zero + sysman
  cssl-host-d3d12             Direct3D 12
  cssl-host-metal             Metal (macOS)
  cssl-host-webgpu            WebGPU (wgpu)

Observability & Testing
  cssl-telemetry              R18 ring-buffer + OTLP + signed audit chain
  cssl-testing                oracle test modes dispatcher
  cssl-persist                orthogonal persistence + schema migration
  cssl-rt                     runtime library
  csslc                       compiler binary
```

## Effect system

28+ built-in effects cover resource constraints, timing, determinism, hardware backends, power, and audit:

```
Timing      Deadline<16ms>  Realtime<Crit>
Hardware    CPU  GPU  XMX  RT  SIMD256  NUMA<node>  Cache<L1>
Backends    Backend<Vulkan>  Backend<D3D12>  Backend<Metal>  Backend<WebGPU>
Alloc       NoAlloc  NoUnbounded  Region<'r>
Determinism PureDet  DetRNG  Reversible
Power       Power<15w>  Thermal<85c>
Audit       Audit<"domain">  Sensitive<privacy>  Verify<Z3>
```

Zero runtime overhead — compiled to evidence-records via Xie+Leijen ICFP'21 semantics.

## No LLVM

The CPU backend targets [Cranelift](https://cranelift.dev/) directly. GPU output goes SPIR-V → Vulkan/D3D12/WebGPU or MSL for Metal. The full compiler builds with a plain `cargo build --release`.

## Status

Stage0 (Rust-hosted bootstrap) — all six features implemented at minimum-viable depth. 1600+ tests passing.

Stage1 (self-hosted) is the next milestone. See [`stage1/README.csl`](stage1/README.csl) for the P1–P10 roadmap.

## Build from source

Requires Rust 1.75+. No other system dependencies.

```bash
git clone https://github.com/Apocky/CSSL3
cd CSSL3/compiler-rs
cargo build --release
# binary: target/release/csslc
```

Or grab the pre-built Windows binary from [Releases](https://github.com/Apocky/CSSL3/releases).

## Documentation

- [`specs/`](specs/) — complete formal design in CSLv3 notation (20+ documents)
- [`DECISIONS.md`](DECISIONS.md) — every architectural decision, T1-D1 through present
- [`PRIME_DIRECTIVE.md`](PRIME_DIRECTIVE.md) — foundational consent axiom, encoded for humans, AI agents, and compilers
- [`examples/`](examples/) — annotated Sigil programs (Vulkan triangle, SDF autodiff, real-time audio)

## Distribution scope

This repository ships the **open-source compiler** — lexer, parser, AST, HIR, MIR, codegen,
runtime, examples, specs, and lore. The compiler is the public face of the language.

Other components in the broader CSSL/Sigil ecosystem (proprietary engine binaries, trained
weights, server-side coordination services, private-tier integrations) are distributed under
separate terms and are **not** part of this public repository.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Good first issues are labeled in the [issue tracker](https://github.com/Apocky/CSSL3/issues?q=label%3A%22good+first+issue%22).

## Security

To report a security vulnerability, please follow the [responsible-disclosure process in SECURITY.md](SECURITY.md). Do **not** open a public issue.

## License

Dual-licensed: **Apache-2.0 OR MIT** at your option. See [LICENSE.md](LICENSE.md) for full
details and contributor terms. Trademark and attribution notices live in [NOTICE.md](NOTICE.md).

© 2026 Apocky. All rights reserved where not otherwise granted by the project license.

# Contributing to Sigil (CSSL)

## Overview

The Sigil compiler lives in `compiler-rs/` — a Cargo workspace of 32 crates written in Rust (MSRV 1.75). This is the **stage0 bootstrap compiler**: Rust-hosted, throwaway once the language self-hosts.

## Crate map

### Frontend

| Crate | Purpose |
|-------|---------|
| `cssl-lex` | Dual-surface lexer (logos + hand-rolled CSLv3-native path) |
| `cssl-parse` | Recursive-descent parser — no parser-combinator framework |
| `cssl-ast` | CST + source-preservation + `Diagnostic` type |

### Type system

| Crate | Purpose |
|-------|---------|
| `cssl-hir` | Typed HIR — Hindley-Milner inference + effect-row unification |
| `cssl-caps` | Pony-6 capability checker (`iso/trn/ref/val/box/tag`) |
| `cssl-ifc` | Jif-DLM information-flow labels; encodes PRIME_DIRECTIVE structurally |
| `cssl-effects` | 28+ effect registry + sub-effect discipline + banned-composition checker |

### Transformation passes

| Crate | Purpose |
|-------|---------|
| `cssl-autodiff` | Source-to-source AD — 19 primitives, `fwd` + `bwd` on MIR |
| `cssl-jets` | `Jet<T,N>` higher-order AD for inverse-rendering / inverse-fluids |
| `cssl-smt` | Z3/CVC5/KLEE drivers + SMT-LIB emission + obligation discharge |
| `cssl-staging` | `@staged` comptime specializer + `#run` evaluation |
| `cssl-futamura` | Futamura P1/P2/P3 partial-evaluation infrastructure |
| `cssl-macros` | Racket-hygienic macro system (unified with staging) |

### IR

| Crate | Purpose |
|-------|---------|
| `cssl-mir` | Structured mid-level IR (MLIR dialect; control-flow + SSA) |
| `cssl-lir` | Low-level IR + target orchestration |
| `cssl-mlir-bridge` | melior FFI for MLIR dialect emissions |

### Codegen

| Crate | Purpose |
|-------|---------|
| `cssl-cgen-cpu-cranelift` | CPU codegen via Cranelift 0.115 — no LLVM |
| `cssl-cgen-gpu-spirv` | SPIR-V emitter via rspirv |
| `cssl-cgen-gpu-dxil` | DXIL emitter |
| `cssl-cgen-gpu-msl` | MSL emitter |
| `cssl-cgen-gpu-wgsl` | WGSL emitter |

### Host runtimes

| Crate | Purpose |
|-------|---------|
| `cssl-host-vulkan` | Vulkan 1.4 via ash |
| `cssl-host-level-zero` | Intel Level-Zero + sysman |
| `cssl-host-d3d12` | D3D12 via windows-rs |
| `cssl-host-metal` | Metal (macOS, cfg-gated) |
| `cssl-host-webgpu` | WebGPU via wgpu |

### Observability & testing

| Crate | Purpose |
|-------|---------|
| `cssl-telemetry` | R18 ring-buffer + OTLP + signed audit chain |
| `cssl-testing` | Oracle-modes dispatcher (property / differential / metamorphic / fuzz / replay) |
| `cssl-persist` | Orthogonal persistence image + schema migration |
| `cssl-rt` | Runtime library |
| `csslc` | Compiler binary entry point |

## Good entry points

If you're new and want to make a real contribution, start here:

- **`cssl-lex`** — self-contained, well-tested, good surface-area for the dual-syntax design
- **`cssl-effects`** — data-heavy, logic-light; good for adding effects or improving documentation
- **`cssl-smt`** — SMT-LIB emission improvements are isolated and high-value
- **`cssl-cgen-gpu-spirv`** — SPIR-V emission is a well-scoped domain with clear correctness criteria

For larger contributions, read [`DECISIONS.md`](DECISIONS.md) for prior architectural decisions and `specs/` for the authoritative design.

## Running tests

```bash
cd compiler-rs

# All 1600+ tests
cargo test --workspace

# Single crate
cargo test -p cssl-lex
cargo test -p cssl-parse
cargo test -p cssl-autodiff

# Clippy (must be clean)
cargo clippy --workspace -- -D warnings
```

## Conventions

**Spec authority** — if your code diverges from `specs/`, the spec wins. File the code as a bug, not the spec.

**Unsafe** — all non-FFI crates use `#![forbid(unsafe_code)]`. Don't add `unsafe` to `cssl-lex`, `cssl-parse`, `cssl-ast`, `cssl-hir`, `cssl-effects`, `cssl-autodiff`, `cssl-mir`, `cssl-staging`, or `csslc`. FFI crates (`cssl-host-*`, `cssl-smt`, `cssl-mlir-bridge`) opt-in per-file.

**Clippy** — the workspace runs pedantic + nursery lint sets. New code must pass `cargo clippy -- -D warnings`. Existing allowances in `Cargo.toml` are scaffold-phase; don't add new ones without discussion.

**Comments** — only write comments when the *why* is non-obvious (hidden constraint, spec-deviation workaround, subtle invariant). Don't comment what the code does; use descriptive identifiers instead.

**Decision log** — significant design decisions go in `DECISIONS.md`:
```
T<N>-D<M> : <decision-title> — <rationale>
```

## Submitting changes

1. Fork the repo and create a branch from `main`.
2. Make your change and ensure `cargo test --workspace` passes.
3. Ensure `cargo clippy --workspace -- -D warnings` is clean.
4. Sign off your commits with the Developer Certificate of Origin: use `git commit -s` (see [DCO sign-off](#dco-sign-off) below).
5. Open a pull request against `main` with a concise description of what and why.

If your change touches the type system, effect registry, autodiff passes, or IFC, link to the relevant spec section in `specs/`.

## DCO sign-off

This project uses the [Developer Certificate of Origin](https://developercertificate.org/)
(DCO) v1.1 instead of a CLA. Every commit must carry a `Signed-off-by:` trailer:

```
Signed-off-by: Your Name <your.email@example.com>
```

The easiest way is to pass `-s` to git:

```bash
git commit -s -m "your message"
```

Configure your git `user.name` and `user.email` once and `-s` will fill in the trailer
automatically. Pull requests with unsigned commits will be asked to amend or rebase
with sign-off.

By signing off, you certify that you have the right to submit your change under the
project's `Apache-2.0 OR MIT` license. This does not transfer copyright; you retain
ownership of your contribution.

## Commit-message style (optional but appreciated)

The project's design documents and many internal commits use a dense CSLv3-glyph
notation (`§` blocks, `⊑`, `⊔`, `→`, `R!`, `W!`, etc.) for compactness. You are
welcome to write commit messages in plain English — that is fully accepted. If you
prefer the dense style, see existing commits in `git log` for examples; the only hard
requirement is that the *first line* of the commit message is a short, descriptive
summary that renders well in `git log --oneline`.

## Reporting security issues

Please **do not** open public issues for security vulnerabilities. Follow the
responsible-disclosure process in [SECURITY.md](SECURITY.md): email
`apocky13@gmail.com` with subject prefix `[CSSL3 SECURITY]`.

## Code of conduct

We expect contributors to act with respect, good faith, and consent — consistent with
the [PRIME_DIRECTIVE.md](PRIME_DIRECTIVE.md) values that underpin the project.
Harassment, discrimination, or attempts to weaponize the project's tools against
users will not be tolerated. Concerns can be raised privately to `apocky13@gmail.com`.

## License

By contributing you agree your work is licensed under the project's dual
`Apache-2.0 OR MIT` terms; see [LICENSE.md](LICENSE.md) and [NOTICE.md](NOTICE.md)
for details.

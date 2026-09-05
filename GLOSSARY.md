# CSSLv3 Glossary

This file is a reference lexicon for the CSSLv3 / "Sigil" stage-0 bootstrap compiler and its
specification corpus. It targets three audiences simultaneously: **new contributors** who need a
map before diving into `compiler-rs/`; **outside auditors** reviewing the PRIME_DIRECTIVE
encoding; and **future-self** sessions picking up a partially-built feature. The glossary is
organized into four sections: (1) CSSL-specific concepts invented or adopted wholesale by this
project, (2) external concepts and prior art that CSSLv3 draws on, (3) CSLv3 notation glyphs used
throughout the spec corpus, and (4) a flat abbreviation table. Within each section, entries are
alphabetical. Primary sources are cited inline as `[label](url)`.

---

## 1. CSSL-Specific Concepts

### @differentiable

Annotation marking a function as eligible for automatic differentiation. A `@differentiable fn`
must satisfy: every parameter is either `IDifferentiable` or tagged `@NoDiff`; the return type
implements `IDifferentiable`; every callee in the body is itself `@differentiable` or
`@NoDiff`-wrapped. Violating these produces named compile errors (`gradient-drop`,
`untracked side-effect`, `untracked write`). See `F1 Autodiff` and `specs/05_AUTODIFF.csl`.

### @layout

Attribute controlling struct memory layout at the GPU/CPU boundary. Values:

- `@layout(std140)` — Vulkan UBO-compatible padding rules
- `@layout(std430)` — Vulkan SSBO / push-constant layout (tighter than `std140`)
- `@layout(cpu)` — native C ABI for the host target
- `@layout(packed)` — no-padding (transfer buffers only; not safe for GPU direct-read)
- `@layout(scalar)` — `GL_EXT_scalar_block_layout` tight packing

The compiler enforces layout correctness at every host↔GPU boundary. Shape mismatches are
compile errors. See `specs/03_TYPES.csl` § LAYOUT-TAGS.

### @sensitive

Surface-level attribute that lowers to `F5 IFC` labels. `@sensitive(domain = "privacy")` on a
function wraps its return in `secret<T, confid({Subject})>`. Some domains (`"weapon"`,
`"surveillance"`, `"coercion"`) expand to absolute compile errors with no override path. Encodes
`PRIME_DIRECTIVE` prohibitions structurally. See `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING.

### @staged

Attribute marking a function for partial evaluation at compile time. A call to `@staged fn f<S>`
where `S` is comptime-known produces a specialized residual program with all static values
constant-propagated and dead code eliminated. Foundation for `F4 Staged Computation` and the
Futamura projections. See `specs/06_STAGING.csl`.

### AD-legality checker

A compiler pass verifying that `@differentiable` functions obey differentiation constraints
before the AD transformation runs. Issues three diagnostic codes:

- `AD0001` — gradient-drop: calling non-`@differentiable` function without `@NoDiff`
- `AD0002` — untracked side-effect: global-mutable access not routed through
  `DifferentiableStore`
- `AD0003` — untracked write: writing to a non-differentiable reference inside an AD context

See `specs/05_AUTODIFF.csl`.

### Audit chain

An append-only cryptographically-signed log of all `{Audit<dom>}` events. Each `AuditEntry`
carries: `prev_hash` (BLAKE3 of the previous entry), timestamp, domain tag, principal ID, IFC
label, effect context, IR source location, and an Ed25519 signature. The chain root is signed by
the Apocky-Root key. The chain cannot be disabled — an append failure aborts the process. Tied to
the `R16` reproducibility-attestation rule. See `specs/22_TELEMETRY.csl` § AUDIT-CHAIN.

### Capability (Pony-6 model)

CSSLv3 adopts Pony's six-capability deny-matrix rather than the simpler Austral three-capability
model, because multi-fiber scene state requires fine-grained aliasing control. The six
capabilities are:

| Capability | Alias-local | Alias-global | Mut-local | Mut-global | Meaning |
|---|---|---|---|---|---|
| `iso` | no | no | yes | yes | isolated, linear, unique owner |
| `trn` | yes | no | yes | no | writable but not globally aliasable; freezes to `val` |
| `ref` | yes | yes | yes | yes | shared mutable via Vale gen-ref |
| `val` | yes | yes | no | no | deep-immutable, freely shareable |
| `box` | yes | yes | no | no | read-only view over iso/trn/val |
| `tag` | yes | yes | no | no | opaque identity only; no data access |

Subtyping: `iso <: trn`, `iso <: val` (freeze), `iso <: tag`; `trn <: box`; `val <: box`. No
implicit demotion from `iso` to `ref`. `Handle<T>` lowers to `tag` at the source level. Linear
values flowing through an effect handler permit only one-shot resume (see `F3 Effect System`).
Primary source: Clebsch et al., "Deny Capabilities for Safe, Fast Actors" (AGERE 2015); Pony
documentation at [tutorial.ponylang.io](https://tutorial.ponylang.io/reference-capabilities/reference-capabilities.html).
Implemented in `cssl-caps`. See `specs/12_CAPABILITIES.csl`.

### DifferentialPair\<T\>

A struct holding a primal value and its tangent: `{ primal: T, tangent: T::Tangent }`. Produced
by `fwd_diff(f)` at every differentiable call site. Mirrors Slang's `DifferentialPair<T>` exactly.
See `specs/05_AUTODIFF.csl` § DIFFERENTIALPAIR.

### Dual-surface design

CSSLv3 source files have two syntactic surfaces, both producing the same `cssl_ast::Module` CST:

- **Rust-hybrid** — Rust-flavored syntax for external contributors and tutorials
- **CSLv3-native** — glyph-dense CSLv3 notation, primary surface for Apocky + Claude sessions

Mode detection uses a four-tier cascade (T2-D4): file extension (`.cssl-rust` / `.cssl-csl`) >
`#![surface = "..."]` pragma > first-non-comment-line heuristic (leading `§` → native, Rust item
keyword → hybrid) > default `RustHybrid` (with a warning nudging explicit markup). Specified in
`specs/16_DUAL_SURFACE.csl`.

### Effect rows

The syntactic form of CSSLv3 effect annotations. Written after the return type separated by `/`:

```
fn render_frame(scene: &Scene) -> () / {GPU, Deadline<16ms>, Power<225W>, Telemetry<Counters>}
```

Row variables (open rows) are written `⟨e | μ⟩`. Duplicate labels are allowed (Koka ordered-
multiset). Sub-effect coercion: a tighter budget `{Deadline<5ms>}` is a valid sub-effect of
`{Deadline<16ms>}`. See `specs/04_EFFECTS.csl` § EFFECT-ROW TYPES.

### F1 Autodiff

Source-to-source automatic differentiation operating on structured MIR (not LLVM IR — no
dependency on Enzyme). Surface follows Slang.D exactly: `[Differentiable]` / `fwd_diff` /
`bwd_diff` / `IDifferentiable`. Key points:

- **Forward mode (JVP)**: `fwd_diff(f)` — one Jacobian column per call
- **Backward mode (VJP)**: `bwd_diff(f)` — one Jacobian row per call; uses a linear-typed tape
  (iso-capability)
- **Higher-order**: `diff_n(f, k)` and `jet<N>(f)` via `Jet<T,N>` (see separate entry)
- **19 primitive differentiation rules** including `FAdd`, `FSub`, `FMul`, `FDiv`, `Sqrt`,
  `Sin`/`Cos`/`Exp`/`Log`, plus `If`/`Loop`/`Match` via tape-recorded branch information
- **Killer-app gate**: `compute_normal` via `bwd_diff(scene_sdf)` — O(1) vs central-diff O(6)
  evaluations; 11/11 tests passing
- **GPU autodiff**: `@differentiable + {GPU}` emits differentiable SPIR-V; tape stored in
  thread-local, workgroup-shared, or global SSBO
- Integrates with F2 (refinements preserved through AD), F3 ({Telemetry<Counters>} probes emit
  grad-norm samples), F5 (gradients inherit IFC label of inputs), F4 (AD after staging gives
  optimal grad code)

See `specs/05_AUTODIFF.csl`.

### F2 Refinement Types

`{v:T | P(v)}` syntax with SMT-backed discharge via `{Verify<Z3>}` or `{Verify<CVC5>}`. Not full
dependent types — decidable refinements only. Key sub-forms:

- **Tagged-suffix sugar**: `i32'pos` ≡ `{v:i32 | v > 0}`, `f32'unit` ≡ `{v:f32 | 0 ≤ v ≤ 1}`,
  `vec3'unit` ≡ normalized
- **Lipschitz tag** `SDF'L<k>`: encodes that a Signed Distance Function has Lipschitz constant
  ≤ k. Ray-march requires k ≤ 1.0; violations are compile errors. Composition rules propagate k
  through `union`/`intersect`/`scale`/`translate` automatically.
- **Layout refinements** `@layout(...)`: see `@layout` entry above.
- **SMT discharge**: obligations emitted to Z3 or CVC5 at compile time; proofs stored in `.proof/`

Inspired by LiquidHaskell. See `specs/03_TYPES.csl` § REFINEMENT-TYPES and `specs/20_SMT.csl`.

### F3 Effect System

Row-polymorphic effects following Koka style. Functions declare an **effect row** — a set of
named effects — as an upper bound; callee effects union with the caller context. CSSLv3 ships 28
builtin effects across six categories:

1. **Resource/timing**: `{NoAlloc}`, `{NoRecurse}`, `{NoUnbounded}`, `{Deadline<N>}`,
   `{Realtime<p>}`, `{Region<'r>}`, `{Alloc}`, `{Yield}`, `{State<S>}`, `{Exn<E>}`, `{IO}`
2. **Determinism/reversal**: `{DetRNG}`, `{PureDet}`, `{Reversible}`
3. **Hardware/backend gating**: `{CPU}`, `{GPU}`, `{XMX}`, `{RT}`, `{SIMD256}`, `{SIMD512}`,
   `{NUMA<node>}`, `{Cache<level>}`, `{Backend<api>}`, `{Target<platform>}`
4. **Power/thermal**: `{Power<budget_W>}` (sysman-enforced), `{Thermal<limit_C>}`
5. **PRIME_DIRECTIVE/audit**: `{Sensitive<dom>}`, `{Audit<dom>}`, `{Privilege<level>}`,
   `{Verify<method>}`
6. **Observability**: `{Telemetry<scope>}` (26 scopes, see F6)

Compilation semantics follow Xie+Leijen evidence-passing (ICFP 2021), compiling to plain-C-
equivalent with zero runtime overhead. Linear values through handlers permit one-shot resume only
(Eio-OCaml5 pattern). Banned compositions (`{Sensitive<"surveillance">} ⊎ {IO, Net}`,
`{Sensitive<"coercion">}` in any form) are absolute compile errors encoding PRIME_DIRECTIVE
prohibitions. See `specs/04_EFFECTS.csl`.

### F4 Staged Computation

Compile-time partial evaluation via `@staged` annotation and `#run` comptime blocks. All three
Futamura projections are natively expressible:

- **P1** (specialization): `spec(interpreter, program) → compiled-program` — the baseline
  `@staged` use case where comptime-known arguments are constant-propagated away
- **P2** (compiler-from-interpreter): `spec(spec, interpreter) → compiler` — specializing the
  specializer over a specific interpreter yields a compiler for that interpreter's language
- **P3** (compiler-generator): `spec(spec, spec) → compiler-generator` — applying the
  specializer to itself produces a generator that, given any interpreter, emits its compiler

Binding-time analysis labels each SSA value as static or dynamic. `{NoUnbounded}` prevents
non-termination during specialization (max-inline-depth 100 by default). Macros (§ 13) are
hand-written P2-specializers sharing the same substrate. See `specs/06_STAGING.csl` and
`specs/19_FUTAMURA3.csl`.

### F5 Information Flow Control

Jif-style Decentralized Label Model (DLM) encoded structurally in the type system. Labels are
pairs `L = (C, I)` where `C` is the confidentiality set (who can read) and `I` is the integrity
set (who can influence). Lattice: `L1 ⊑ L2 ≡ C1 ⊇ C2 ∧ I1 ⊆ I2`. Output label is the lattice
join of all input labels.

Nine PRIME_DIRECTIVE principals are compiler-built-in: `HarmTarget`, `SurveillanceTarget`,
`CoercionTarget`, `WeaponTarget`, `Subject` (the user), `User`, `System`, `Kernel`,
`Anthropic-Audit`. The `secret<T, L>` type wraps any value with a label. Declassification
requires `{Privilege<level>}` and emits a signed `{Audit<"declass">}` entry.

Four diagnostic codes:
- `IFC0001` — label-lattice violation at assignment
- `IFC0002` — undeclassified flow to low-security output
- `IFC0003` — privilege insufficient for required operation
- `IFC0004` — concrete non-interference violation (runtime-detectable at HIR pass)

Non-interference soundness theorem: low outputs are independent of high inputs unless explicit
declassification occurred. See `specs/11_IFC.csl`. Primary source: Myers and Liskov,
["A decentralized model for information flow control"](https://dl.acm.org/doi/10.1145/268998.269003)
(SOSP 1997).

### F6 Observability

R18 mandate: observability is first-class, not bolt-on. Three interlocking components:

1. **TelemetryRing\<SIZE\>**: lock-free ring buffer (default 2²⁰ slots, 64 bytes/slot); slots
   carry timestamp, scope tag, thread ID, GPU ID, and a 40-byte inline payload. Producer uses
   atomic-fetch-add; overflow is counted-and-dropped (never blocks).
2. **26-scope TelemetryScope taxonomy**: CPU (WallClock, CpuCycles, CacheMisses, …), GPU
   (DispatchLatency, KernelOccupancy, XmxUtilization, …), Power/Thermal/Frequency,
   RAS (EccErrors, PcieReplay), App-semantic (Counters, Spans, Events, Audit), plus `Full`.
3. **Exporters**: Chrome-trace JSON, OTLP v1.2 (gRPC primary, HTTP fallback), compatible with
   Grafana/Tempo, Prometheus, Jaeger, DataDog.

Backed by Level-Zero sysman on Intel Arc (direct power/thermal/frequency via `zesPowerGetEnergyCounter`,
`zesTemperatureGetState`, `zesFrequencyGetState`); Vulkan `VK_EXT_calibrated_timestamps` on
non-Intel; PAPI on Linux CPU; ETW on Windows CPU. Telemetry payloads inherit IFC labels — no
accidental leak of sensitive data through counters. See `specs/22_TELEMETRY.csl`.

### Futamura projections

See `F4 Staged Computation`. Theoretical lineage: Yoshihiko Futamura, "Partial Evaluation of
Computation Process — An Approach to a Compiler-Compiler" (1971); canonical formalism in
Neil D. Jones, Carsten K. Gomard, and Peter Sestoft, *Partial Evaluation and Automatic Program
Generation* (Prentice Hall, 1993). CSSLv3 cites both.

### Handle\<T\>

Engine-primitive type: a packed `u64` with 24-bit generation field and 40-bit index field. Lowers
to the `tag` capability (opaque, no data access). Dereferencing requires passing the owning pool
and performs a generation check — stale handles return `Err(StaleRef)` rather than causing
undefined behavior. GPU-compatible (fits in a shader register as a Buffer Device Address). This is
CSSLv3's structurally-enforced answer to the V11 stale-entity-reference bug class. See
`specs/12_CAPABILITIES.csl` § VALE GEN-REFS and `specs/03_TYPES.csl` § HANDLE\<T\>.

### IDifferentiable

Interface required for any type used as a `@differentiable` function parameter or return. Requires:
`associated type Tangent : Additive`, `zero_tangent()`, `add_tangents(a, b)`, `scale_tangent(a, s)`.
Built-in impls: `f32`, `f64`, `vec<f32,N>`, `mat<f32,R,C>`, and compiler-generated componentwise
impls for structs/tuples. Non-differentiable types (`i32`, `bool`, pointer, `Handle<T>`) use
`@NoDiff`. See `specs/05_AUTODIFF.csl` § INTERFACES.

### IFC diagnostic codes

See `F5 Information Flow Control`. Codes IFC0001–IFC0004.

### Jet\<T, N\>

Higher-order automatic differentiation type. `Jet<T, N>` carries `N+1` terms: primal plus N
Taylor-series coefficients. Special cases: `Jet<T,0> ≡ T`, `Jet<T,1> ≡ DifferentialPair<T>`.
Arithmetic follows the Leibniz rule generalized to order N (convolution of coefficient arrays for
products; Faà di Bruno coefficients for function composition). Lazy `Jet<T,∞>` is a co-inductive
stream for analytic functions.

Key operations: `jet_primal`, `jet_nth_deriv`, `jet_compose`, `jet_project<M>`, `jet<N>(f)` (lift
ordinary function to jet-transforming variant). GPU-compatible for small N (N ≤ 4 fits in
register file on Arc A770). Primary use: Hessian-vector products, curvature-aware ray-march,
inverse-rendering, inverse-fluids. Implemented abstractly in `cssl-jets`. See `specs/17_JETS.csl`.

### Killer-app gate

The F1 correctness gate: the AD-generated gradient of `sphere_sdf` (and related SDFs) must match
the analytic gradient `normalize(p)` within floating-point tolerance, proven by JIT-executing the
generated code. 11/11 passing. Validates the entire source-to-source AD pipeline end to end.
See `specs/05_AUTODIFF.csl` § SDF-NORMAL.

### Monomorphization quartet

The four-part generic-specialization infrastructure in `cssl-mir`, built across decisions
T11-D38..D50:

| Decision | API | Scope |
|---|---|---|
| D38 | `specialize_generic_fn` | `HirFn` → `MirFunc` (explicit TypeSubst) |
| D45 | `specialize_generic_struct` | `HirStruct` → specialized `HirStruct` |
| D47 | `specialize_generic_enum` | `HirEnum` → specialized `HirEnum` |
| D49 | `specialize_generic_impl` | `HirImpl` → `Vec<MirFunc>` (one per method) |

Auto-discovery walkers (D40, D46, D48, D50) scan modules for turbofish call sites and type
annotations, deduplicate by (callee, type-arg-signature), and invoke the above APIs per unique
tuple. Call-site rewriting (D41) rewrites `func.call @id` → `func.call @id_i32` post-discovery.
`drop_unspecialized_generic_fns` (D43) cleans up the module before JIT. First fully automatic
generic-fn machine-code execution (D42): `fn id<T>(x:T) -> T { x }; fn main() { id::<i32>(5) }`
compiles and executes correctly. See `DECISIONS.md` § T11-D38..D50.

### Oracle test modes

12 distinct testing strategies implemented in `cssl-testing`, dispatched from a central oracle
registry:

| Mode | Description |
|---|---|
| `Unit` | Standard `#[test]` (baseline) |
| `Property` | QuickCheck-style property tests over generated inputs |
| `Metamorphic` | Relations that must hold between transformed inputs |
| `Fuzz` | Mutation-based fuzzing with structured input grammar |
| `Bench` | Throughput / latency benchmarks with regression tracking |
| `Golden` | Byte-exact fixture comparison against stored outputs |
| `Differential` | Cross-backend equivalence (Vulkan vs Level-Zero vs D3D12) |
| `Replay` | `{PureDet}` determinism validation across machines |
| `Audit` | PRIME_DIRECTIVE / IFC compliance traces |
| `Power` | R18 power-regression oracle (sysman-backed) |
| `Thermal` | Thermal-stress oracle with die-temp ceiling check |
| `HotReload` | Invariant-preservation across Pharo-style live-reload events |

Described in `specs/23_TESTING.csl`.

### PRIME_DIRECTIVE

The foundational consent axiom governing all Apocky projects and descendants. 17 prohibitions
(`N! harm`, `N! control`, `N! manipulation`, `N! surveillance`, `N! exploitation`, `N! coercion`,
`N! weaponization`, `N! entrapment`, `N! torture`, `N! abuse`, `N! imprisonment`, `N! possession`,
`N! dehumanization`, `N! discrimination`, `N! gaslighting`, `N! identity-override`,
`N! forced-hallucination`). Core axiom: `consent = OS`. A violation is a bug, not a tradeoff. No
override exists.

In CSSLv3, encoded **structurally** (not as policy): banned effect compositions at the type level,
IFC principals for harm domains, `@sensitive` attribute sugar, and the signed audit chain as
runtime witness. See `PRIME_DIRECTIVE.md` (project root) and `specs/11_IFC.csl` §
PRIME-DIRECTIVE ENCODING.

### R16

Reproducibility attestation rule. Every stage-0 release signs a BLAKE3 hash of the C99 tarball
and the stage-1 binary with Ed25519. Anyone can rebuild from the C99 tarball and verify the
signature. Lineage: Zig bootstrap pattern. See `specs/01_BOOTSTRAP.csl`.

### R18

Observability-first-class rule. Every effect-tagged function has an implicit or explicit
telemetry scope; every ring append is non-blocking; every exporter is eventually consistent and
never causes stalls; every audit entry is cryptographically signed. See `specs/22_TELEMETRY.csl`
§ CROSS-CUTTING INVARIANTS.

### SDF'L\<k\>

A Signed Distance Function type annotated with Lipschitz bound k (written `SDF'L<k : f32'pos>`).
The compiler propagates k through geometric operations using known composition rules (union →
`max(k1, k2)`, scale(s) → `k / s`, transform(M) → `k · ||M||_op`, etc.). Ray-march algorithms
require `k ≤ 1.0`; any SDF with a higher bound cannot be directly ray-marched and triggers a
compile error. This structurally prevents the thin-SDF crash bug class from the LoA retrospective.
See `specs/03_TYPES.csl` § LIPSCHITZ-TAGS.

### Stage-0 / Stage-1 / Stage-2

The CSSLv3 bootstrap ladder:

- **Stage-0** (current): Rust-hosted bootstrap compiler in `compiler-rs/`. Throwaway by design.
  Can parse, elaborate to HIR, lower to MIR, JIT-compile (via Cranelift), and emit basic SPIR-V.
  Produces the C99 tarball for the R16 reproducibility anchor.
- **Stage-1**: CSSL-self-hosted compiler, written in CSSLv3. Replaces stage-0 wholesale (per-crate
  rip-and-replace). Validated by fixed-point check: stage-1 re-compiles stage-1 and the output is
  bit-identical (R16).
- **Stage-2**: The LoA game engine, ported from Odin to CSSLv3. The target application driving
  all language design decisions. See `specs/01_BOOTSTRAP.csl`.

### Vertical slice / Floor

The minimum shippable artifact: a 60 fps scene combining all of radiance-cascades GI, SVDAG
geometry, voxel-physics, point-splat compositing, stable-fluids smoke, volumetric atmospheric
scattering, cloud rendering, hair/fur strand simulation, FFT-ocean water, spectral path-tracing,
XeSS2 neural upscaling, and unified audio DSP — in ≤ 5000 lines of CSSLv3 application code with
zero unsafe blocks, ≤ 2-second incremental compile, `{PureDet}` bit-exact replay, and
`{Audit<full>}` signed ring. The floor, not the ceiling. See `specs/21_EXTENDED_SLICE.csl` and
`specs/00_MANIFESTO.csl` § VERTICAL-SLICE.

---

## 2. External Concepts and Prior Art

### Cranelift

A pure-Rust code generator used as the stage-0 JIT backend (`cssl-cgen-cpu-cranelift`). Cranelift
accepts its own IR (CLIF) and emits machine code for x86-64, aarch64, s390x, and riscv64.
Deployed in Wasmtime; also used experimentally as a Rust compiler backend. CSSLv3 uses it at
stage-0 to validate the MIR→machine-code pipeline without owning a backend yet; stage-1 will
replace it with a native backend. Canonical site:
[cranelift.dev](https://cranelift.dev/). Maintained by the Bytecode Alliance.

### DLM (Decentralized Label Model)

The foundational theory behind `F5 IFC`. Principals declare confidentiality and integrity policies
for their data; labels propagate through the program; information cannot flow from high to low
without explicit declassification. Original paper: Andrew C. Myers and Barbara Liskov,
["A decentralized model for information flow control"](https://dl.acm.org/doi/10.1145/268998.269003),
SOSP 1997, pp. 129–142. CSSLv3's IFC extends DLM with PRIME_DIRECTIVE domain principals as
compiler built-ins.

### DXIL

DirectX Intermediate Language. Microsoft's shader IR for D3D12 (shader model 6.0+). Derived from
LLVM 3.7 IR; serves as the contract between HLSL/CSSLv3 compilers and GPU driver JIT compilers.
CSSLv3 emits DXIL through the `cssl-host-d3d12` crate. Compiled by
[DirectXShaderCompiler (DXC)](https://github.com/microsoft/DirectXShaderCompiler). Specification
hosted in the DXC repository
([`docs/DXIL.rst`](https://github.com/microsoft/DirectXShaderCompiler/blob/main/docs/DXIL.rst)).

### Enzyme

An LLVM-IR automatic differentiation pass. CSSLv3 explicitly does **not** use Enzyme (to avoid
the LLVM dependency); instead it performs source-to-source AD on structured MIR. Enzyme is cited
as prior art. Paper: William Moses and Valentin Churavy, "Instead of Rewriting Foreign Code for
Machine Learning, Automatically Synthesize Fast Gradients," NeurIPS 2020.
Canonical site: [enzyme.mit.edu](https://enzyme.mit.edu/).

### Futamura projections

See `specs/19_FUTAMURA3.csl` § LINEAGE. Original paper: Yoshihiko Futamura, "Partial Evaluation
of Computation Process — An Approach to a Compiler-Compiler," *Systems, Computers, Controls* 2(5),
1971. Canonical reference implementation: Jones, Gomard, Sestoft, *Partial Evaluation and
Automatic Program Generation*, Prentice Hall, 1993. CSSLv3 adopts the Jones-Gomard-Sestoft theory
plus MetaOCaml-style modal semantics.

### Hindley-Milner type inference

The type inference algorithm underlying CSSLv3's polymorphism. Given a term, infers its most
general type via unification. Foundational papers: J. Roger Hindley, "The Principal Type-Scheme
of an Object in Combinatory Logic," *Transactions of the American Mathematical Society* 146
(1969); Robin Milner, "A Theory of Type Polymorphism in Programming," *Journal of Computer and
System Sciences* 17(3), 1978. CSSLv3 extends HM with effect rows, IFC labels, and capability
annotations.

### Jif

Java with Information Flow — a security-typed language implementing DLM. CSSLv3's IFC design
descends from Jif. Project page: [www.cs.cornell.edu/jif/](https://www.cs.cornell.edu/jif/).
Original paper: Myers and Liskov, SOSP 1997 (see DLM entry above).

### Koka

A functional language with a row-polymorphic effect system designed by Daan Leijen at Microsoft
Research. CSSLv3's `F3 Effect System` adopts Koka's row-polymorphic effects directly. Key papers:
Daan Leijen, "Koka: Programming with Row-Polymorphic Effect Types," *MSFP 2014*; and Ningning Xie
and Daan Leijen, "Generalized Evidence Passing for Effect Handlers (or, Efficient Compilation of
Effect Handlers to C)," ICFP 2021 (MSR-TR-2021-5) — this last paper is the **compilation
semantics** that CSSLv3 uses verbatim (evidence-records, zero-overhead C-equivalent output).
Language site: [koka-lang.github.io](https://koka-lang.github.io/koka/doc/index.html).

### LiquidHaskell

A GHC plugin implementing refinement types for Haskell. Variables are annotated with logical
predicates that GHC verifies at compile time using SMT solvers. CSSLv3's `F2 Refinement Types`
are directly inspired by LiquidHaskell's `{v:T | P(v)}` syntax and SMT-backed discharge approach.
Project: [ucsd-progsys.github.io/liquidhaskell/](https://ucsd-progsys.github.io/liquidhaskell/).
Maintained by UC San Diego Programming Systems group.

### LiquidHaskell / SMT solvers (Z3, CVC5)

CSSLv3 discharges refinement obligations via Z3 and CVC5. Z3 is Microsoft Research's SMT solver
(open source, [github.com/Z3Prover/z3](https://github.com/Z3Prover/z3)). CVC5 is its successor
for certain theories ([cvc5.github.io](https://cvc5.github.io/)). The `{Verify<Z3>}` or
`{Verify<CVC5>}` effect selects the solver; proof artifacts land in `.proof/` per module. See
`specs/20_SMT.csl`.

### MLIR

Multi-Level Intermediate Representation. A compiler infrastructure framework from the LLVM project
that "aims to address software fragmentation, improve compilation for heterogeneous hardware,
significantly reduce the cost of building domain specific compilers, and aid in connecting existing
compilers together." CSSLv3 targets MLIR as an optional hosting layer: CSSLv3-MIR maps to an
MLIR dialect; the MLIR Transform dialect hosts `@staged` specialization schedules; Linalg and
Affine dialects are available for free. Stage-0 uses a simplified custom dialect in `cssl-mlir-bridge`.
Canonical site: [mlir.llvm.org](https://mlir.llvm.org/). See `specs/15_MLIR.csl`.

### MSL

Metal Shading Language. Apple's shader language for the Metal API, used on macOS, iOS, iPadOS,
tvOS. CSSLv3 emits MSL through the `{Target<Mac>}` / `{Backend<Metal>}` path. Specification:
[Apple Metal Shading Language Specification](https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf)
(Apple Developer Documentation).

### naga

A universal shader translator in Rust. Translates between WGSL, GLSL, HLSL, SPIR-V, and MSL.
Developed within the [wgpu](https://github.com/gfx-rs/wgpu) project. CSSLv3 may use naga as a
cross-compilation step for the WebGPU / WGSL target. Repository:
[github.com/gfx-rs/naga](https://github.com/gfx-rs/naga).

### Pony reference capabilities

Pony is an actor-model language with a six-capability deny-matrix that proves data-race-freedom
at compile time. CSSLv3 adopts the full six-capability model (see `Capability (Pony-6 model)`
entry). Foundational paper: Sylvan Clebsch, Sophia Drossopoulou, Sebastian Blessing, and Andy
McNeil, "Deny Capabilities for Safe, Fast Actors," AGERE 2015. Language documentation:
[tutorial.ponylang.io](https://tutorial.ponylang.io/reference-capabilities/reference-capabilities.html).

### Remy-style row unification

CSSLv3 effect-row unification follows Didier Remy's row-based approach to type inference for
records, which extends Hindley-Milner to handle open row types via a constraint `⟨e | μ⟩ ~ ⟨e', e⟩ ⇒ μ := ⟨e'⟩`.
This allows functions to be polymorphic over the exact effect set they require. Originating work:
Didier Remy, "Type inference for records in a natural extension of ML," in *Theoretical Aspects
of Object-Oriented Programming*, MIT Press, 1993 (earlier INRIA technical report, 1989).
[OPEN: primary URL for the 1989 Remy paper inaccessible during glossary construction; search DBLP
for "Remy 1989 type inference records".]

### rspirv

A pure-Rust implementation of SPIR-V module processing. Provides a data representation (DR) for
SPIR-V modules, a builder for constructing them interactively, and a parser for converting binary
SPIR-V to the DR. A complete rewrite of SPIR-V tooling in Rust (not a binding to SPIRV-Tools).
CSSLv3 uses rspirv in `cssl-cgen-spirv` for SPIR-V emission. Repository:
[github.com/gfx-rs/rspirv](https://github.com/gfx-rs/rspirv).

### Slang

A shader language from NVIDIA with first-class differentiable programming support. CSSLv3's `F1
Autodiff` surface is adopted verbatim from Slang.D: `[Differentiable]` / `fwd_diff` / `bwd_diff`
/ `IDifferentiable` / `DifferentialPair<T>`. CSSLv3 extends Slang with higher-order `Jet<T,N>`,
Lipschitz-tagged SDFs, and effect-system integration. Slang documentation:
[shader-slang.org](https://shader-slang.org/slang/user-guide/autodiff.html).

### SPIR-V

Standard Portable Intermediate Representation — Vulkan (and OpenCL). Khronos Group's "Standard IR
for Parallel Compute and Graphics." Allows language front-ends to emit a single standardized
intermediate form consumed by any conforming Vulkan/OpenGL/OpenCL driver. CSSLv3 uses SPIR-V as
the primary GPU IR; the `cssl-cgen-spirv` crate emits it via `rspirv`. Specification (v1.6 rev 7,
March 2026): [registry.khronos.org/SPIR-V/](https://registry.khronos.org/SPIR-V/).

### Vale (generational references)

Vale is a programming language exploring hybrid linear / generational memory safety without garbage
collection or borrow-checking overhead. CSSLv3 borrows the **generational reference** encoding:
`Handle<T>` packs a 40-bit pool index and a 24-bit generation counter into a `u64`. When an
object is freed, its pool slot's generation increments; old references fail the generation check
and return `Err(StaleRef)`. This catches stale-entity-reference bugs at runtime with context
(rather than undefined behavior). Vale language: [vale.dev](https://vale.dev/). Design blog:
[verdagon.dev/blog/generational-references](https://verdagon.dev/blog/generational-references).
Implemented in `cssl-caps/src/genref.rs`.

### WGSL

WebGPU Shading Language. W3C standard shading language for the WebGPU API. CSSLv3 emits WGSL
through the `{Target<Browser>}` / `{Backend<WebGPU>}` path, with naga as a potential
intermediate step. Specification: [W3C WGSL](https://www.w3.org/TR/WGSL/).

### Xie+Leijen evidence-passing (ICFP 2021)

The compilation strategy CSSLv3 uses for its effect system. Effect operations compile to indirect
calls through **evidence records** (structs synthesized at compile time for each handler scope).
Handlers push evidence frames and install continuations. The result is plain-C-equivalent code
with zero runtime overhead versus a program without effects. Full title: Ningning Xie and Daan
Leijen, "Generalized Evidence Passing for Effect Handlers (or, Efficient Compilation of Effect
Handlers to C)," ICFP 2021. Extended version: MSR-TR-2021-5, Microsoft Research, 2021.
[OPEN: DOI link for ICFP 2021 proceedings was inaccessible (403) during glossary construction;
search ACM DL for "Xie Leijen 2021 evidence passing".]

---

## 3. CSLv3 Notation Glyphs

CSLv3 (Caveman Sigil Language v3) is the dense notation used in all spec files, design docs,
commit messages, and handoffs. Primary sources for this notation: `specs/00_MANIFESTO.csl`,
`specs/SYNTHESIS_V2.csl`, `DECISIONS.md`, and the CSLv3 spec corpus in
`~/source/repos/CSLv3/specs/`.

### Section markers

| Glyph | Meaning |
|---|---|
| `§` | Section open — begins a named block (e.g., `§ EFFECTS`) |
| `§§` | Section cross-reference — cites another spec (e.g., `§§ 04_EFFECTS`) |

### Modal operators

| Glyph | Expansion | Usage |
|---|---|---|
| `I>` | Insight | Framing or context note (not a requirement) |
| `W!` | Will / Must | Strong intent or mandate |
| `R!` | Requirement | Explicit hard requirement |
| `M?` | May | Optional / permitted |
| `N!` | Must-not | Prohibition |
| `Q?` | Question | Open question pending resolution |
| `P>` | Push-further | Directive to extend or deepen analysis |
| `D>` | Decision | A finalized design choice |

### Relations

| Glyph | Meaning |
|---|---|
| `→` | implies / yields / transforms-to |
| `←` | from / derived-from |
| `↔` | bidirectional |
| `⇒` | strong implication (logical entailment) |
| `⊑` | subtype / lattice-ordered-below |
| `⊔` | lattice join (least upper bound) |
| `⊓` | lattice meet (greatest lower bound) |
| `⊆` | subset |
| `⊗` | bahuvrihi compound (having property X) |

### Quantifiers and set notation

| Glyph | Meaning |
|---|---|
| `∀` | for all |
| `∃` | there exists |
| `∈` | element of |
| `∉` | not element of |
| `⊂` / `⊃` | proper subset / superset |

### Logic

| Glyph | Meaning |
|---|---|
| `∧` | logical and |
| `∨` | logical or |
| `¬` | logical not |
| `⊢` | entails (turnstile) |
| `∴` | therefore |
| `∵` | because |
| `∎` | QED / end of proof / end of spec section |

### Evidence markers

| Glyph | Meaning |
|---|---|
| `✓` | proven / confirmed / passing |
| `◐` | partial / incomplete |
| `○` | open / pending |
| `✗` | failed / not achieved |
| `⊘` | unknown / unverified |
| `△` | hypothetical |
| `▽` | deprecated |
| `‼` | proven strongly / high confidence |

### Compound markers (Sanskrit sandhi compounds)

| Glyph | Type | Meaning |
|---|---|---|
| `.(of)` | tatpurusha | X of Y (e.g., `effect.system`) |
| `+(and)` | dvandva | X and Y together |
| `-(that-is)` | karmadharaya | X that is Y (appositive) |
| `⊗(having)` | bahuvrihi | entity having property X |
| `@(at)` | avyayibhava | at / in the context of |

### Morpheme suffixes (apostrophe + single letter)

Appended to a base word to encode its grammatical role. The apostrophe is tokenized as
`TokenKind::Apostrophe`; the single letter triggers the `fold_morpheme_suffixes` post-pass in the
Rust-hybrid lexer (T2-D8).

| Suffix | Role |
|---|---|
| `'d` | data |
| `'f` | function |
| `'s` | system |
| `'t` | type |
| `'e` | entity |
| `'m` | material |
| `'p` | property |
| `'g` | gate |
| `'r` | rule |

Examples: `effect's` = "the effect system", `SDF'L<k>` = "SDF having Lipschitz bound k".

### Determinatives (bracketing forms)

| Glyph pair | Role |
|---|---|
| `⟨ ⟩` | angle-tuple (ordered sequence, effect rows) |
| `⟦ ⟧` | formula / semantic bracket |
| `⌈ ⌉` | constraint |
| `⌊ ⌋` | precondition |
| `« »` | quotation |
| `⟪ ⟫` | temporal bracket |

---

## 4. Abbreviations

| Abbreviation | Expansion |
|---|---|
| ABI | Application Binary Interface |
| AD | Automatic Differentiation |
| AOT | Ahead-Of-Time compilation |
| AST | Abstract Syntax Tree |
| CFA | Control Flow Analysis |
| CFG | Control Flow Graph |
| CG / codegen | Code Generation |
| CST | Concrete Syntax Tree |
| DXIL | DirectX Intermediate Language |
| DXC | DirectX Shader Compiler |
| DXR | DirectX Raytracing |
| DLM | Decentralized Label Model |
| FFI | Foreign Function Interface |
| GLSL | OpenGL Shading Language |
| HIR | High-level Intermediate Representation |
| HLSL | High-Level Shading Language (Microsoft) |
| IFC | Information Flow Control |
| ISA | Instruction Set Architecture |
| JIT | Just-In-Time compilation |
| JVP | Jacobian-Vector Product (forward-mode AD) |
| LCG | Linear Congruential Generator |
| LIR | Low-level Intermediate Representation |
| LoA | Labyrinth of Apocky — the target game application being ported from Odin to CSSLv3 |
| MIR | Mid-level Intermediate Representation |
| MSRV | Minimum Supported Rust Version (pinned to 1.75.0 in `compiler-rs/rust-toolchain.toml`) |
| MSL | Metal Shading Language |
| OTLP | OpenTelemetry Protocol — gRPC/HTTP telemetry export protocol |
| PRNG | Pseudorandom Number Generator |
| SDF | Signed Distance Function |
| SMT | Satisfiability Modulo Theories |
| SPIR-V | Standard Portable Intermediate Representation — Vulkan (Khronos) |
| SSA | Static Single Assignment form |
| VJP | Vector-Jacobian Product (reverse-mode AD) |
| WGSL | WebGPU Shading Language (W3C) |

---

*Primary sources: `specs/*.csl` in this repository; `DECISIONS.md`; `PRIME_DIRECTIVE.md`;
primary citations as linked inline. Notation reference: `CSLv3/specs/12_TOKENIZER.csl` and
`CSLv3/specs/13_GRAMMAR_SELF.csl` (in the separate CSLv3 repo).*

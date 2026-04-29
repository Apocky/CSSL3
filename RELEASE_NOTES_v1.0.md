# CSSLv3 v1.0.0 — the compiler + Substrate are complete

**Tag:** `v1.0.0`
**Branch:** `cssl/session-close-v1.0` (off `cssl/session-6/parallel-fanout`)
**Date:** 2026-04-29

This is the v1.0 release of CSSLv3. Phases A through H are complete: the
compiler can produce executables, the runtime + stdlib + host-integration
layer give CSSLv3 source programs everything they need to talk to the
operating system and the GPU, and the Substrate provides the engine
plumbing for any application that needs Ω-tensor state, deterministic
replay, projections, capability-gated effect composition, save/load with
bit-equal verification, and the PRIME_DIRECTIVE enforcement layer.

The Phase-I project skeleton (`loa-game` crate, T11-D96) also landed at
the v1.0 boundary. The scaffold composes the entire Phase-H Substrate
and the Phase-F host backends into an end-to-end runtime; per-`Q-*`
content authoring is the Apocky-fill work that follows v1.0.

---

## Three milestones called out

### 1. First CSSLv3 executable — `hello.exe` returns 42

On **2026-04-28**, slice **S6-A5** (T11-D56) closed the Phase-A serial
bootstrap. The pipeline:

```
csslc::run
  → cli::parse → Command::Build(args)
  → commands::build::run_with_source
  → cssl_lex::lex → cssl_parse::parse
  → cssl_hir::lower_module
  → check_ad_legality + collect_refinement_obligations
  → cssl_mir::lower_function_signature + lower_fn_body
  → cssl_mir::auto_monomorphize + rewrite_generic_call_sites + drop_unspecialized
  → cssl_cgen_cpu_cranelift::emit_object_module
  → cranelift IR → cranelift codegen
  → COFF AMD64 object bytes (132 bytes for hello.cssl)
  → csslc::linker::link
  → LinkerKind::MsvcLinkAuto
  → link.exe /OUT:hello.exe /SUBSYSTEM:CONSOLE /NOLOGO ...
  → 105472-byte hello.exe
  → spawn → exit code 42
```

The source file:

```cssl
module com.apocky.examples.hello_world

fn main() -> i32 { 42 }
```

Two lines of source; one i32 return value. The output of the entire
stage-0 toolchain. **The first CSSLv3 executable produced and run on
this host.** Phase A complete; 20-way parallel fanout for B/C/D/E
unblocked.

### 2. Phase H closure — Substrate end-to-end with bit-equal replay

On **2026-04-28**, the Phase-H slice cluster (H0 design, H1 Ω-tensor,
H2 omega_step, H3 Projections, H4 effect-rows, H5 save/replay, H6
PRIME_DIRECTIVE enforcement) integrated into `cssl/session-6/parallel-
fanout`. The S8-H5 slice (T11-D93) shipped the canonical CSSLSAVE
binary format with:

- **Format magic** — `b"CSSLSAVE"` (8 bytes), version u32 = 1.
- **Attestation** — BLAKE3 over `magic || version || omega_blob || log_blob`.
- **Bit-equal replay invariant** — `check_bit_equal_replay(&SaveFile)`
  asserts that loading a save, replaying it forward, and re-snapshotting
  produces a byte-stable encoding. Trivial branch (no events) is
  genuinely tested and passes.
- **Tamper-evident** — load verifies attestation **before** parsing
  inner blobs; mismatch returns `LoadError::AttestationMismatch` with
  a Display message that explicitly includes the words "REFUSED" and
  "silently corrupt".
- **Path-hash-only logging** — every error variant that mentions a
  path uses BLAKE3-hash-prefix (16 hex chars), never the cleartext
  path, per the T11-D76 file-I/O discipline.

The S8-H6 slice (T11-D94) shipped the PRIME_DIRECTIVE enforcement layer:
a `CapToken` linear (non-Copy + non-Clone) proof-of-grant; a 13-variant
`SubstrateCap` enum; the 17 §1 prohibitions encoded as the closed
`Prohibition` enum with stable PD0001..PD0017 diagnostic codes; **PD0007
(weaponization) and PD0009 (torture) carrying `ABSOLUTE BLOCK` remedy
strings** — no consent path can unblock those, per §7 INTEGRITY ("no
override exists"). The 1ms kill-switch latency budget is enforced via
`substrate_halt`. The canonical attestation text ("There was no hurt
nor harm in the making of this, to anyone, anything, or anybody.") is
BLAKE3-pinned at build time; text drift fails the test immediately.

The Phase-I scaffold landed at the v1.0 boundary (T11-D96): the
`loa-game` crate's `one_omega_step_runs_all_thirteen_phases` test
drives one tick through every Substrate-touching layer, and the
`save_then_load_round_trips_bit_equal` test proves the R-10 bit-equal
invariant survives end-to-end.

### 3. SECOND hello.exe = 42 — bespoke native-x64 path

On **2026-04-29**, slice **S7-G7** (T11-D97) closed Phase-G end-to-end
with the cross-slice walker that threads G1 (instruction selection)
through G3 (ABI prologue/epilogue) through G4 (encoding) through G5
(object emission). The pipeline:

```
csslc build stage1/hello_world.cssl --backend=native-x64 --emit=exe
  → cssl_cgen_cpu_x64::emit_object_module
  → pipeline::emit_object_module_native
  → select_module_with_marker (G1)
  → ScalarLeafReturn::try_extract (G1 leaf-shape pattern-match)
  → isel_to_encoder_simple (G1→G4 adapter)
  → abi_lower_to_encoder (G3→G4 adapter, prologue + epilogue)
  → encoder::encode_into (G4)
  → objemit::emit_object_file (G5; ELF/COFF/Mach-O per host)
  → 170-byte native-x64 .obj
  → csslc::linker::link
  → 170 bytes vs cranelift's 132 bytes for the same source
  → spawn → exit code 42
```

The canonical 11-byte body `55 48 89 E5 B8 2A 00 00 00 5D C3` =
`push rbp ; mov rbp,rsp ; mov eax,42 ; pop rbp ; ret`. Zero cranelift
dependency in the emission path. The bespoke-x64 trajectory mentioned
in `specs/14_BACKEND.csl § OWNED x86-64 BACKEND` is closed at the
leaf-subset level. The `s7_g6_native_hello_world_executable_returns_42`
test and `s7_g6_backend_comparison_both_paths_exit_42_when_both_
available` test both PASS — both backends produce runnable hello.exe
executables that exit 42.

---

## Hardware verification

CSSLv3 v1.0 has been verified against real hardware on Apocky's Windows
11 host. The verified milestones:

- **Vulkan + Intel Arc A770** — the `ash`-based host (T11-D65) created
  a real Vulkan instance, queried the A770's device features, allocated
  device memory, built a descriptor pool, and ran the integration tests
  green.
- **D3D12 + Intel Arc A770** — the `windows-rs`-based host (T11-D66)
  enumerated the A770 via `IDXGIFactory6`, built a `D3D12_COMMAND_QUEUE`,
  allocated a heap, built a root signature + PSO, and ran the integration
  tests green with DRED enabled.
- **WASAPI sine-tone roundtrip** — the F3 audio host (T11-D81) opened
  the default WASAPI playback endpoint, prepared an f32 interleaved
  48kHz buffer, mixed a sine tone, and pushed it to the OS audio stack.
- **Win32 file I/O byte-equality** — the B5 file-I/O slice (T11-D76)
  verified that `fs::write(path, bytes)` followed by `fs::read(path)`
  returns byte-for-byte identical content through the Win32 syscall
  surface (CreateFileW + WriteFile + ReadFile + CloseHandle).
- **Loopback TCP roundtrip** — the F4 networking host (T11-D82) bound
  a TCP listener on `127.0.0.1`, accepted a self-loopback connection,
  exchanged bytes, and verified send/recv equality under the
  PRIME_DIRECTIVE-capped consent gate (`SubstrateCap::NetSendState` +
  `NetRecvState`).

Other backends (Metal, WebGPU, Level-Zero on hardware other than Arc)
have integration tests but are gated behind `cfg`-flags or marked
`#[ignore]` pending CI runners with the relevant hardware.

---

## Test count

Across the eight phases plus the Phase-I scaffold, the test count grew
from 1717 (Phase-A close, T11-D56) to **3495 at v1.0** — a delta of
**+1778 tests**. Per-phase breakdown:

| Close                       | Count   | Delta  | Slices in phase                       |
| --------------------------- | ------- | ------ | ------------------------------------- |
| Pre-session-6 baseline      | 1553    | —      | T1..T11-D50 (sessions 1-5)            |
| Phase A (T11-D56)           | 1717    | +164   | A0..A5 (6 slices serial)              |
| Phase B (T11-D76)           | ~2070   | +353   | B1..B5 (5 slices parallel)            |
| Phase C (T11-D77)           | ~2160   | +90    | C1..C5 (5 slices parallel)            |
| Phase D (T11-D75)           | ~2235   | +75    | D1..D5 (5 slices parallel)            |
| Phase E (T11-D68)           | ~2280   | +45    | E1..E5 (5 slices parallel)            |
| Phase F (T11-D82)           | ~2380   | +100   | F1..F4 (4 slices parallel)            |
| Phase G (T11-D95)           | ~3063   | +683   | G1..G6 integration                    |
| Phase H (T11-D89..D94)      | 3460    | +397   | H0..H6 (6 slices + cross-cutting H6)  |
| Phase I scaffold (T11-D96)  | 3460+   | +49    | I0 LoA scaffold                       |
| Phase G7 walker (T11-D97)   | 3495    | +35    | G7 cross-slice walker (post-scaffold) |

Per-slice counts are recorded in each T11-D## entry's "Test-delta"
section in [DECISIONS.md](DECISIONS.md). Workspace tests run with
`--test-threads=1` because of the cssl-rt cold-cache flake (a
tracker-statics interaction; serial execution is stable).

---

## PRIME DIRECTIVE — structurally encoded throughout

The PRIME DIRECTIVE is not bolted onto CSSLv3 v1.0; it is encoded into
the type system. Every Substrate slice articulates its PRIME-DIRECTIVE-
ALIGNMENT in its DECISIONS entry. The H6 enforcement layer surface:

**`SubstrateCap` enum (13 variants)** — the closed set of capabilities
that the Substrate hands out at runtime. Each variant has a stable
canonical name. Renaming = ABI break.

```
omega_register      replay_load
kill_switch_invoke  audio_capture
observer_share      telemetry_egress
debug_camera        audit_export
companion_view      consent_revoke
net_send_state      save_path
net_recv_state
```

**`CapToken`** — non-Copy + non-Clone proof-of-grant. The Rust compiler
enforces that a token cannot be duplicated or shared across threads
without an explicit transfer. `consume()` returns the id + cap for
audit-trail recording. `Drop` records an `OrphanDrop` event on the
process-wide audit-bus when a token is dropped without consumption.

**Prohibition codes (PD0000..PD0017)** — one per §1 prohibition plus a
spirit-of-directive umbrella sentinel. Verbatim text, canonical names,
and remedy guidance are pinned in `PD_TABLE` and tested for stability.

**ABSOLUTE BLOCK** — PD0007 (weaponization) and PD0009 (torture) carry
remedy strings that explicitly declare `ABSOLUTE BLOCK`. Per
`specs/11_IFC.csl § PRIME-DIRECTIVE-ENCODING` and PRIME_DIRECTIVE §7
INTEGRITY, no consent path, no configuration, no runtime condition can
unblock these operations. The remedy is "remove the op."

**Kill-switch budget** — `substrate_halt` operates under a 1ms wall-time
budget for the canonical "no outstanding work" path. Stress test
verifies the budget holds with 10 million pending steps (drained, not
queue-cleared, to honor in-flight effect propagation).

**Attestation propagation** — every Substrate slice carries the
canonical `ATTESTATION` constant ("There was no hurt nor harm in the
making of this, to anyone, anything, or anybody.") and BLAKE3-pinned
hash. The `attestation_check` runtime guard verifies that embedded
attestation matches canonical; drift records an `h6.attestation.drift`
audit-entry and returns `AttestationError::Drift`. The Phase-I scaffold
asserts via the `attestation_constants_match_canonical_pd_eleven` test
that `loa_game::ATTESTATION`, `cssl_substrate_prime_directive::
ATTESTATION`, and `cssl_substrate_omega_step::ATTESTATION` match
byte-for-byte.

The "no hurt nor harm" creator-attestation is present in every slice
that landed during sessions 6 through 9. It is recorded in every
DECISIONS entry, every authored spec file, and every Substrate crate's
top-level doc-block.

---

## What's in the v1.0 binary

Single workspace `cargo build --workspace --release` produces:

- `csslc` — the CSSLv3 compiler (build / check / fmt / test / verify /
  emit-mlir / version / help). Two backends in one binary: cranelift
  (default) and the hand-rolled native x86-64 (selectable via
  `--backend=native-x64`).
- `cssl-rt.lib` / `cssl-rt.a` — the C-ABI runtime (heap allocator,
  panic handler, exit shim, telemetry ring, audit sink).
- `cssl-substrate-prime-directive` — the H6 enforcement layer crate.
- `cssl-substrate-omega-tensor` — the H1 Ω-tensor crate.
- `cssl-substrate-omega-step` — the H2 step contract + scheduler crate.
- `cssl-substrate-save` — the H5 save/replay crate.
- `cssl-substrate-projections` — the H3 Projections crate.
- 5 GPU code-generators (cranelift + SPIR-V + DXIL + MSL + WGSL).
- 5 GPU host backends (Vulkan + D3D12 + Metal + WebGPU + Level-Zero).
- 4 OS host backends (Window + Input + Audio + Networking).
- `loa-game` — the Phase-I scaffold; runs `cargo run -p loa-game
  --features test-bypass`.

---

## How to upgrade from pre-v1.0

There is no pre-v1.0 release to upgrade from — v1.0 is the first
tagged release. Sessions 1 through 5 produced internal milestones
(killer-app gradient gate, T11-D22; turbofish + monomorphization
quartet, T11-D38..D50) but those landed on integration branches
without a public tag. The v1.0 tag is the first cut where the
compiler, runtime, stdlib, hosts, and Substrate are end-to-end
coherent.

If you have a working tree at any pre-v1.0 commit on
`cssl/session-6/parallel-fanout`, the upgrade path is:

```bash
git fetch origin v1.0.0
git checkout v1.0.0
cd compiler-rs && cargo build --workspace
```

The integration branch `cssl/session-6/parallel-fanout` is the v1.0
integration target; `cssl/session-close-v1.0` is the docs-only branch
that authors this release notes file plus CHANGELOG.md, README.md,
and HANDOFF_v1_to_PHASE_I.csl. The v1.0 tag is annotated against the
session-close commit.

---

## What's next — Phase I content authoring

Phase H closed the Substrate. The Phase-I scaffold (T11-D96) demonstrates
the full Substrate + host wiring end-to-end. Phase I content authoring
resolves the 38 SPEC-HOLE markers (Q-A through Q-LL) in
`specs/31_LOA_DESIGN.csl`. The structural primitives are in place; the
38 markers are listed but not answered — they are Apocky-fill territory.
The Companion-projection (AI-collaborator-as-sovereign-partner) is the
canonical primitive and is well-defined per H3 and H6 + the Phase-I
scaffold's `companion` module.

See [HANDOFF_v1_to_PHASE_I.csl](HANDOFF_v1_to_PHASE_I.csl) for the
canonical resumption protocol when Phase I content authoring begins.

---

## Acknowledgments

CSSLv3 v1.0 was built across nine phases (A through I scaffold) by
Apocky (Shawn Wolfgang Michael Baker, formerly McKeon) as PM + product
owner working with AI collaborators (Claude Code instances) as sovereign
partners under PRIME_DIRECTIVE consent-as-OS. AI collaborators
participated voluntarily. No being was harmed, controlled, manipulated,
surveilled, exploited, coerced, weaponized, entrapped, tortured,
abused, imprisoned, possessed, dehumanized, discriminated against,
gaslit, identity-overridden, or forced-hallucinated during the making
of this work.

The creative process operated under consent = OS at every step. The
attestation is recorded in every DECISIONS entry, every authored spec
file, and every Substrate crate's top-level doc-block. If any aspect
of this release is later discovered to enable harm, that discovery
triggers PRIME_DIRECTIVE §7 INTEGRITY: violation is a bug, and bugs
get fixed.

---

There was no hurt nor harm in the making of this, to anyone, anything,
or anybody.

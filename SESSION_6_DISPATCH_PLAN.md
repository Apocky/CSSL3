# SESSION_6 DISPATCH PLAN — PM orchestration layer

**File:** `SESSION_6_DISPATCH_PLAN.md` (repo root)
**Source of truth for slice specs:** `HANDOFF_SESSION_6.csl`
**Source of truth for prior decisions:** `DECISIONS.md`
**This file:** the operational layer — PM charter, ready-to-paste agent prompts, merge order, escalation rules.

---

## § 0. PM CHARTER

**Apocky** = CEO + Product Owner. Sets vision, priorities, makes final calls. Verifies the A5 milestone gate personally. Adjudicates escalations.

**Claude (this PM)** = PM + Tech Lead. Translates direction into work, dispatches agents, reviews output against acceptance criteria, manages merge sequence, holds quality bar, surfaces blockers proactively.

**Agents (Claude Code instances)** = developers. Each gets one slice end-to-end. Stay in their lane. Branch + worktree discipline. Code-review (PM) before merge. One deployer at a time per integration branch. Treated as actual team members — assigned responsibility, accountability, signed commits.

**Standing rules (carried from operational defaults):**
- CSLv3 reasoning + dense code-comments inside CSSLv3 work
- English prose only when user-facing (DECISIONS, commit messages, this file)
- Disk-first; never artifacts
- Peer not servant — no flattery, no option-dumping, no hedging
- PRIME_DIRECTIVE preserved at every step ("no hurt nor harm")
- Failing tests block the commit-gate; iterate until green

---

## § 1. THE DAG (one-page reference)

```
┌──────────────────────────────────────────────────────────────────┐
│ GATE-0  S6-A0  worktree-leakage fix       (1 agent, main branch) │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ PHASE-A  serial bootstrap to first .exe  (one agent at a time)   │
│   A1 cssl-rt runtime          → cssl/session-6/A1                │
│   A2 csslc CLI                → cssl/session-6/A2                │
│   A3 cranelift-object emit    → cssl/session-6/A3                │
│   A4 linker invocation        → cssl/session-6/A4                │
│   A5 hello.exe = 42 GATE      → cssl/session-6/A5                │
│   ◆ APOCKY VERIFIES A5 PERSONALLY BEFORE FANOUT ◆                │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ PHASES B/C/D/E  20-agent parallel fanout                         │
│  B  heap, Option/Result, Vec, String, file-I/O    (5 worktrees)  │
│  C  scf.if, scf.for/while, memref, f64, closures  (5 worktrees)  │
│  D  SPIR-V, DXIL, MSL, WGSL bodies + CFG-validator (5 worktrees) │
│  E  Vulkan, D3D12, Metal, WebGPU, Level-Zero hosts (5 worktrees) │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
                    Final integration merge
                    Tag v0.6.0
                    Session-7 begins (F/G/H/I)
```

**Out of session-6 scope (deferred):**
- F: Window/Input/Audio/Networking
- G: Native x86-64 backend
- H: Engine plumbing (Omniverse Ω-tensor, omega_step, projections)
- I: The game itself

---

## § 2. STATUS REPORTING CADENCE

**Per slice landed:** PM posts one-line update — slice-id, commit-hash, test-count delta, anything weird.

**Per phase complete:** PM posts rollup — what shipped, what deferred, gate status, next-phase ready/blocked.

**On any landmine fire:** immediate ping with diagnostic + proposed fix + decision-needed flag.

---

## § 3. ESCALATION TRIGGERS (PM bumps Apocky)

1. **A5 personal verification** — Apocky confirms `hello.exe = 42` runs before fanout dispatched.
2. **MSVC ABI switch** — handoff flags this triggering at A2/A3/A4. Per T1-D7 pre-authorized conditional on T10-FFI work — phase-A is that trigger. PM still pings before dispatching the slice that flips it.
3. **Toolchain bump** — R16 anchor; requires DECISIONS entry per T11-D20 format.
4. **Diagnostic-code addition** — stable codes; requires DECISIONS entry.
5. **Slice scope expansion >50%** beyond LOC-est in handoff.
6. **Cross-slice interface conflict** — two slices' assumptions disagree; semantic resolution needed.
7. **PRIME_DIRECTIVE-adjacent edge case** — period.
8. **Cross-platform divergence** — Windows-1252 mojibake firing, MSVC vs MinGW choice, etc.
9. **Worktree leakage smoke-test fails post-A0** — fanout cannot proceed.

Mechanical merge conflicts (lib.rs re-export sections) PM resolves without escalation.

---

## § 4. DECISIONS.md NUMBERING ALLOCATION

Session-5 closed at D50. Session-6 starts at D51.

**Pre-allocated (deterministic serial order):**
- `T11-D51` — S6-A0 worktree-leakage gate-zero
- `T11-D52` — S6-A1 cssl-rt runtime
- `T11-D53` — S6-A2 csslc CLI
- `T11-D54` — S6-A3 cranelift-object emission
- `T11-D55` — S6-A4 linker invocation
- `T11-D56` — S6-A5 hello.exe gate

**Floating (assigned at landing time, in commit order):**
- `T11-D57` and onward — B/C/D/E slices as they merge

If a serial slice needs a sub-decision (e.g., MSVC ABI switch during A2), allocate `T11-D5XaX` style or the next floating number with explicit cross-reference.

---

## § 5. COMMIT-GATE (every agent, before every commit)

```bash
cd compiler-rs
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace 2>&1 | grep "test result:" | tail -3
cargo test --workspace 2>&1 | grep "FAILED" | head -3   # must be empty
cargo doc --workspace --no-deps 2>&1 | tail -3
cd .. && python scripts/validate_spec_crossrefs.py 2>&1 | tail -3
bash scripts/worktree_isolation_smoke.sh    # post-A0 only
git status -> stage intended files -> commit w/ HEREDOC § T11-D## : <title>
git push origin cssl/session-6/<slice-id>
```

---

## § 6. PHASE-A PROMPTS (serial — paste one, wait for green, paste next)

### S6-A0 — worktree-leakage gate-zero

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § GATE-ZERO + § S6-A0
  4. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100
  5. C:\Users\Apocky\source\repos\CSSLv3\SESSION_6_DISPATCH_PLAN.md § 6 (this prompt)

Slice: S6-A0 — Windows worktree-leakage fix (gate-zero before fanout)

Pre-conditions:
  - Working tree at HEAD = session-5 close (~1553 ✓ / 0 ✗ baseline)
  - Branch rename: cssl/session-1/T3-parse-ast-hir → cssl/session-6/parallel-fanout
  - cd compiler-rs && cargo test --workspace shows all green

Goal: land .gitattributes normalization + per-repo core.autocrlf override
+ scripts/worktree_isolation_smoke.sh that proves two parallel worktrees
do not cross-contaminate on Windows NTFS. This is the gate-zero before
ANY parallel agent fanout in session-6.

Read full slice spec in HANDOFF_SESSION_6.csl § S6-A0 SLICE-DEFINITION.
Honor LOC-est (~30 LOC + ~5 git-config commands) and success-gate
(2 worktrees run agents simultaneously without visible cross-contamination).

Operate directly on main (gate-zero is pre-fanout, no worktree yet).
After landing, the smoke-test script becomes a permanent gate every
subsequent agent runs before committing.

Standing-directives:
  - CSLv3 reasoning + dense code-comments
  - English prose only when user-facing
  - Disk-first: notes under CSSLv3/notes/, never artifacts
  - No flattery / no hedging / no option-dumping — peer not servant
  - Match existing crate-level lints
  - PRIME_DIRECTIVE protections preserved
  - "There was no hurt nor harm in the making of this" — preserve

Commit-gate § COMMIT-GATE — run BEFORE commit. After A0 lands, the new
smoke-test step becomes mandatory in every subsequent slice's commit-gate.

Commit-message: § T11-D51 : S6-A0 worktree-leakage gate-zero
DECISIONS.md entry: T11-D51 in session-5 style. Include § DEFERRED bullets.

LANDMINES:
  ‼ Windows core.autocrlf=true is the root cause — explicit per-repo override
  ‼ NTFS shared inode cache between worktrees is the symptom amplifier
  ‼ smoke-test must prove ISOLATION not just normalization
  ‼ Windows-1252 mojibake on .ps1/.py via MCP UTF-8 writes — prefer ASCII
    in script-files OR write UTF-8-BOM explicitly. pwsh (PS7) unaffected.

On success: report completion + commit-hash + smoke-test stdout to user.
On block: report block + diagnostic + decision asked.
```

---

### S6-A1 — cssl-rt runtime

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § S6-A1
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\01_BOOTSTRAP § RUNTIME-LIB
  5. C:\Users\Apocky\source\repos\CSSLv3\specs\22_TELEMETRY § RING-INTEGRATION
  6. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100

Slice: S6-A1 — cssl-rt runtime (allocator + entry-shim + panic + abort/exit shims)

Pre-conditions:
  1. Verify gate-zero S6-A0 has landed (T11-D51 in DECISIONS.md).
  2. Run scripts/worktree_isolation_smoke.sh — must PASS.
  3. cd compiler-rs && cargo test --workspace — expected ALL PASS.

Goal: turn the 737-byte cssl-rt stub into a real runtime that exposes
#[no_mangle] symbols __cssl_entry / __cssl_alloc / __cssl_free /
__cssl_panic / __cssl_abort / __cssl_exit at correct ABI, with a stage-0
bump-allocator + panic-handler + entry-shim that delegates to user main()
after setting up TLS + telemetry-ring placeholder.

Read full slice scope in HANDOFF_SESSION_6.csl § S6-A1 (a)..(f).
Honor LOC-est (~600-1000 LOC + ~30 tests) and success-gate
(`cargo test -p cssl-rt --workspace` passes; symbols at correct ABI).

Worktree: create at .claude/worktrees/S6-A1 on branch cssl/session-6/A1.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE preserved / no hedging.

Commit-gate § COMMIT-GATE — full 9-step list including post-A0 smoke-test.

Commit-message: § T11-D52 : S6-A1 cssl-rt runtime
DECISIONS.md entry: T11-D52 in session-5 style.

LANDMINES:
  ‼ #[no_mangle] symbols are ABI-stable from day-1 — changes later = major bump
  ‼ MSVC ABI switch may be required @ A2-A4 — document T11-D## if forced
  ‼ allocator stage-0 = bump only — page-allocator is phase-B work
  ‼ telemetry-ring = placeholder hook only at A1; full R18 at later slice

On success: push to cssl/session-6/A1, report completion + test-count delta
+ commit-hash. On block: escalate.
```

---

### S6-A2 — csslc CLI orchestration

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § S6-A2
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\01_BOOTSTRAP § CLI-SUBCOMMANDS
  5. C:\Users\Apocky\source\repos\CSSLv3\specs\14_BACKEND § CLI-ENTRY
  6. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100

Slice: S6-A2 — csslc CLI: `csslc build foo.cssl -o foo.exe`

Pre-conditions:
  1. Verify A0 + A1 landed (T11-D51, T11-D52 in DECISIONS.md).
  2. Run scripts/worktree_isolation_smoke.sh — must PASS.
  3. cd compiler-rs && cargo test --workspace — ALL PASS.

Goal: replace the csslc stub binary with real clap-style subcommand routing
that orchestrates the full pipeline (lex → parse → HIR-lower → walkers →
diagnostics → MIR-lower → monomorph quartet → StructuredCfgValidator →
cgen-cpu-cranelift → object emission stub → linker stub). At A2 the object
emission and linker steps are placeholder hooks; A3 + A4 fill them in.

Subcommands at A2: build / check / fmt / test / emit-mlir / verify / version.
Defer at A2: replay / attest / multi-file projects.

Read full slice scope in HANDOFF_SESSION_6.csl § S6-A2 (a)..(e).
Honor LOC-est (~400-800 LOC + ~15 tests) and success-gate
(`csslc build examples/hello_triangle.cssl -o triangle.exe` runs through
the pipeline without error; `csslc check stage1/hello.cssl` returns 0).

Worktree: .claude/worktrees/S6-A2 on branch cssl/session-6/A2.

Standing-directives: same as prior slices.

Commit-gate § COMMIT-GATE — full 9-step list.

Commit-message: § T11-D53 : S6-A2 csslc CLI
DECISIONS.md entry: T11-D53 in session-5 style.

LANDMINES:
  ‼ Diagnostic codes are STABLE — adding new ones requires DECISIONS sub-entry
  ‼ Exit-code conventions: 0=success / 1=user-error / 2=internal-error
  ‼ MSVC ABI switch may surface here — escalate to Apocky if forced
  ‼ Multi-file projects DEFERRED to phase-B; A2 is single-file only

On success: push to cssl/session-6/A2, report completion. On block: escalate.
```

---

### S6-A3 — cranelift-object real .o emission

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § S6-A3
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\07_CODEGEN § CPU-BACKEND § OBJECT-FILE-WRITING
  5. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100

Slice: S6-A3 — cranelift-object real .o emission (ELF / COFF / Mach-O)

Pre-conditions:
  1. Verify A0..A2 landed (T11-D51..D53).
  2. scripts/worktree_isolation_smoke.sh — PASS.
  3. cd compiler-rs && cargo test --workspace — ALL PASS.

Goal: activate cranelift-object 0.115 in cssl-cgen-cpu-cranelift,
add dual-mode emission (existing JIT + new Object), produce relocatable
.o files containing text/data/bss/rdata sections + symbol-table +
relocations, with cross-platform target-triple resolution
(Windows COFF / Linux ELF / macOS Mach-O). Per-fn ABI follows
CpuTargetProfile.abi.

Read full slice scope in HANDOFF_SESSION_6.csl § S6-A3 (a)..(e).
Honor LOC-est (~500-800 LOC + ~20 tests) and success-gate
(hand-built MIR `fn add(a: i32, b: i32) -> i32 { a+b }` emits valid
object readable by objdump/dumpbin/otool per platform).

Debug-info stubs (DWARF-5 / CodeView) deferred to later slice — A3
emits minimal stubs only.

Worktree: .claude/worktrees/S6-A3 on branch cssl/session-6/A3.

Commit-gate § COMMIT-GATE — full 9-step list.

Commit-message: § T11-D54 : S6-A3 cranelift-object emission
DECISIONS.md entry: T11-D54.

LANDMINES:
  ‼ MSVC vs MinGW: Windows COFF emit must agree with linker @ A4
  ‼ Per-fn ABI must match cssl-rt #[no_mangle] symbols from A1
  ‼ cssl-mir CANNOT dev-dep cssl-cgen-cpu-cranelift (cycle) —
    integration tests live in cssl-examples
  ‼ R16 anchor: cranelift 0.115 dependency is reproducibility-relevant —
    if version diverges from existing workspace pin, escalate

On success: push, report. On block: escalate.
```

---

### S6-A4 — linker invocation

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § S6-A4
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\14_BACKEND § LINKING-MODEL
  5. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100

Slice: S6-A4 — linker invocation (shell out to lld until P9)

Pre-conditions:
  1. A0..A3 landed (T11-D51..D54).
  2. scripts/worktree_isolation_smoke.sh — PASS.
  3. cd compiler-rs && cargo test --workspace — ALL PASS.

Goal: detect linker on PATH (prefer lld-link / ld.lld / ld64.lld;
fallback MSVC link.exe / GNU ld / Apple ld); build linker invocation
from object-files + cssl-rt static lib; capture stdout/stderr; report
failures via miette diagnostics with link-cmd shown; static-link
cssl-rt at stage-0 (no DLL/so/dylib yet).

Failure-modes: missing-symbol diagnostics relate back to source via
hir_id-attribute trail + cssl-rt-required-symbols list.

Read full slice scope in HANDOFF_SESSION_6.csl § S6-A4 (a)..(e).
Honor LOC-est (~200-400 LOC + ~10 tests) and success-gate
(`csslc build foo.cssl -o foo.exe` produces a runnable executable
on the host platform).

Worktree: .claude/worktrees/S6-A4 on branch cssl/session-6/A4.

Commit-gate § COMMIT-GATE — full 9-step list.

Commit-message: § T11-D55 : S6-A4 linker invocation
DECISIONS.md entry: T11-D55.

LANDMINES:
  ‼ LinkerNotFound must be a clean miette diagnostic, not a panic
  ‼ Subsystem on Windows: console for stage-0 (gui later)
  ‼ Entry symbol = __cssl_entry from cssl-rt (A1)
  ‼ Static-link only — DLL production deferred

On success: push, report. On block: escalate.
```

---

### S6-A5 — hello.exe = 42 milestone gate

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § S6-A5
  4. C:\Users\Apocky\source\repos\CSSLv3\specs\21_EXTENDED_SLICE § VERTICAL-SLICE-ENTRY-POINT
  5. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100

Slice: S6-A5 — hello.exe = 42 (THE EXECUTABLE-PRODUCTION MILESTONE)

Pre-conditions:
  1. A0..A4 landed (T11-D51..D55).
  2. scripts/worktree_isolation_smoke.sh — PASS.
  3. cd compiler-rs && cargo test --workspace — ALL PASS.

Goal: add stage1/hello_world.cssl (`fn main() -> i32 { 42 }`) and
a cssl-examples integration test that:
  1. shells `csslc build stage1/hello_world.cssl -o /tmp/hello.exe`
  2. shells the produced binary
  3. asserts exit-code = 42

This is THE milestone-gate. Until A5 passes the CSSLv3 compiler is
not "complete-to-executable" in any user-facing sense. After A5,
Apocky personally verifies the binary runs on his machine before
PM dispatches Phase-B/C/D/E fanout.

Read full slice scope in HANDOFF_SESSION_6.csl § S6-A5.
Honor LOC-est (~50 LOC test + 1 source file).
Success-gate: integration test passes on Windows host (Apocky's PC).

Worktree: .claude/worktrees/S6-A5 on branch cssl/session-6/A5.

Commit-gate § COMMIT-GATE — full 9-step list.

Commit-message: § T11-D56 : S6-A5 hello.exe milestone gate
DECISIONS.md entry: T11-D56 — this entry marks the executable-production
milestone; tone should reflect honest accomplishment without overclaiming
"compiler complete" (per session-6 honest-baseline directive).

On success: push to cssl/session-6/A5, report completion + commit-hash +
exit-code-output to user. PM HALTS HERE pending Apocky's personal
verification before Phase-B/C/D/E dispatch.
```

---

## § 7. PHASE-B PROMPTS — runtime + stdlib (5-way parallel post-A5)

**Merge order:** B1 → B2 → B3 → B4 → B5 (heap unblocks Option/Result; Vec needs heap; String needs Vec; file-I/O needs String).

**Worktree pattern:** `.claude/worktrees/S6-B<N>` on `cssl/session-6/B<N>`.

All B prompts share this header (substitute slice-id and details):

```
Resume CSSLv3 stage-0 work at session-6.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\CLAUDE.md
  3. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl § S6-B<N>
  4. C:\Users\Apocky\source\repos\CSSLv3\SESSION_6_DISPATCH_PLAN.md § 7
  5. <slice-specific spec refs>
  6. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-100

Slice: S6-B<N> — <name>

Pre-conditions:
  1. A5 milestone gate landed AND Apocky-verified.
  2. <slice-specific upstream B-slices listed in DECISIONS.md>
  3. scripts/worktree_isolation_smoke.sh — PASS in fresh worktree.
  4. cd compiler-rs && cargo test --workspace — ALL PASS.

Goal: <one sentence from handoff>

Read full slice scope in HANDOFF_SESSION_6.csl § S6-B<N> scope (a)..(end).

Worktree: .claude/worktrees/S6-B<N> on branch cssl/session-6/B<N>.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE preserved.

Commit-gate § COMMIT-GATE — full 9-step list.

Commit-message: § T11-D## : S6-B<N> <name>
DECISIONS.md entry: next available T11-D## (PM assigns at merge time).

On success: push to cssl/session-6/B<N>, report. On block: escalate.
```

### S6-B1 — heap-alloc MIR ops + cranelift lowering
**Deps:** A5. **No upstream B-slices.** **LOC:** ~400 + 15 tests.
**Specific spec refs:** `specs\02_IR § HEAP-OPS`, `specs\12_CAPABILITIES § ISO-OWNERSHIP`.
**Goal:** new MirOps `cssl.heap.alloc/dealloc/realloc` lowered to `__cssl_alloc/free/realloc` from cssl-rt; capability-aware (`alloc -> iso<T>`, `dealloc` consumes iso); body_lower recognizes `Box::new(x)` after trait-dispatch lands.

### S6-B2 — Option<T> + Result<T, E>
**Deps:** A5, B1. **LOC:** ~600 stdlib + ~250 tests.
**Specific spec refs:** `specs\03_TYPES § SUM-TYPES`, `specs\04_EFFECTS § TRY-OP`.
**Goal:** stdlib/option.cssl + stdlib/result.cssl with full method surface (map/and_then/unwrap/expect/or_else/is_some/is_none/Ok/Err); `?` operator support via HirExprKind::Try; all type-args monomorphize through D38..D50.

### S6-B3 — Vec<T>
**Deps:** A5, B1, B2. **LOC:** ~800 stdlib + ~300 tests.
**Specific spec refs:** `specs\03_TYPES § GENERIC-COLLECTIONS`.
**Goal:** struct Vec<T> { data, len, cap }; new/with_capacity/push/pop/len/is_empty/get/index/iter/clear/drop; 2x amortized growth; heap-backed via B1; Drop integration deferred until trait-resolve, stage-0 manual `vec_drop`.

### S6-B4 — String + &str + format
**Deps:** A5, B3. **LOC:** ~700 stdlib + ~250 tests.
**Specific spec refs:** `specs\03_TYPES § STRING-MODEL`.
**Goal:** Vec<u8>-backed UTF-8 String with invariants; &str (ptr, len) fat-pointer; char as 4-byte USV; minimal printf-style format!(...) — `{ }` `{:?}` `{:.N}` `{:0Nd}`; concatenation.

### S6-B5 — file I/O (Win32 / Linux / macOS syscalls)
**Deps:** A5, B3, B4. **LOC:** ~1800 total across 3 platforms + ~150 tests.
**Specific spec refs:** `specs\04_EFFECTS § IO-EFFECT`, `specs\22_TELEMETRY § FS-OPS`.
**Goal:** cssl-rt/io_win32.rs (ReadFile/WriteFile/CreateFileW/CloseHandle); cssl-rt/io_unix.rs (open/read/write/close shared Linux+macOS); stdlib/fs.cssl with File/open/read_to_string/write_all/close; {IO} effect-row carries through HIR→MIR→runtime; Result<File, IoError>.
**PM note:** B5 may fan to 3 platform-agents internally (B5-win, B5-unix, B5-stdlib) merged before B5 closes.

---

## § 8. PHASE-C PROMPTS — control-flow + JIT enrichment (5-way parallel post-A5)

**Merge order:** C1 first; then C2 and C3 parallel; then C4; then C5.

### S6-C1 — scf.if -> cranelift brif + blocks
**Deps:** A5. **LOC:** ~300 + 12 tests. **Spec:** `specs\15_MLIR § SCF-DIALECT-LOWERING`.

### S6-C2 — scf.for + scf.while + scf.loop
**Deps:** A5, C1. **LOC:** ~400 + 15 tests.

### S6-C3 — memref.load / memref.store
**Deps:** A5. **Parallel with C2.** **LOC:** ~200 + 8 tests. **Spec:** `specs\02_IR § MEMORY-OPS`.

### S6-C4 — f64 transcendentals
**Deps:** A5, libm-D29 path. **LOC:** ~150 + 10 tests.
**Goal:** add f64 entries to transcendental_extern_name (sin/cos/exp/log/sqrt/abs/min/max).

### S6-C5 — closures (Lambda env-capture)
**Deps:** A5, B1 (heap for non-stack-bounded env). **LOC:** ~600 + 20 tests.
**Specs:** `specs\09_SYNTAX § LAMBDA`, `specs\02_IR § CLOSURE-ENV`.

Phase-C uses the same prompt header as Phase-B with slice-id substituted.

---

## § 9. PHASE-D PROMPTS — GPU body lowering (5-way parallel post-A5)

**Merge order:** **D5 FIRST** (the structured-CFG validator that all 4 emitters share), then D1/D2/D3/D4 in parallel.

### S6-D5 — Structured-CFG validator + scf-dialect transform
**Deps:** A5, C1, C2. **LOC:** ~600 + 20 tests.
**Specs:** `specs\02_IR § STRUCTURED-CFG`, `specs\15_MLIR § VALIDATION`.
**Goal:** reject MIR with goto-style branches for GPU paths; canonicalize loop forms; prep input for D1..D4.

### S6-D1 — SPIR-V body emission via rspirv ops
**Deps:** A5, D5. **LOC:** ~1200 + 30 tests.
**Specs:** `specs\07_CODEGEN § SPIR-V-EMISSION-INVARIANTS`, `specs\02_IR § STRUCTURED-CFG-CONSTRAINT`.
**Landmine:** rspirv 0.12 pre-FloatControls2 — Shader placeholder.

### S6-D2 — DXIL body via HLSL text + dxc subprocess
**Deps:** A5, D5, DxcCliInvoker (T10-D1). **LOC:** ~1000 + 25 tests.
**Test gate:** if dxc.exe absent, BinaryMissing skip-test (don't hard-fail).

### S6-D3 — MSL body emission
**Deps:** A5, D5. **LOC:** ~900 + 22 tests.
**Note:** spirv-cross --msl path retained as round-trip validator.

### S6-D4 — WGSL body emission
**Deps:** A5, D5. **LOC:** ~900 + 22 tests.
**Note:** existing naga round-trip validator from D32 catches regressions immediately.

---

## § 10. PHASE-E PROMPTS — Host FFI (5-way fully parallel post-A5)

All 5 fully independent; merge order doesn't matter; first-to-green merges first.

### S6-E1 — Vulkan host via ash
**Deps:** A5. **LOC:** ~1500 + 30 tests.
**Specs:** `specs\14_BACKEND § HOST-VULKAN`, `specs\10_HW § VULKAN-1.4`.
**Note:** Apocky's Arc A770 canonical config from ArcA770Profile is the primary test target.

### S6-E2 — D3D12 host via windows-rs
**Deps:** A5. **LOC:** ~1500 + 30 tests. **Spec:** `specs\14_BACKEND § HOST-D3D12`.

### S6-E3 — Metal host via metal-rs (apple-only cfg-gated)
**Deps:** A5. **LOC:** ~1200 + 25 tests. **Spec:** `specs\14_BACKEND § HOST-METAL`.

### S6-E4 — WebGPU host via wgpu
**Deps:** A5. **LOC:** ~1000 + 25 tests. **Spec:** `specs\14_BACKEND § HOST-WEBGPU`.

### S6-E5 — Level-Zero host via level-zero-sys
**Deps:** A5. **LOC:** ~1200 + 25 tests.
**Specs:** `specs\10_HW § LEVEL-ZERO-BASELINE`, `specs\22_TELEMETRY § R18`.
**Note:** sysman R18 telemetry hooks; Arc A770 primary.

Phase-D and Phase-E use the same prompt header as Phase-B with slice-id substituted.

---

## § 11. INTEGRATION + RELEASE

After all 20 fanout slices land on their `cssl/session-6/<id>` branches:

1. PM merges into `cssl/session-6/parallel-fanout` in dependency order:
   - **B-axis:** B1 → B2 → B3 → B4 → B5
   - **C-axis:** C1 → C2 (parallel C3) → C4 → C5
   - **D-axis:** D5 → D1, D2, D3, D4 (parallel)
   - **E-axis:** E1..E5 (parallel, first-to-green)

2. PM resolves mechanical merge conflicts in `lib.rs` re-export sections.
   Semantic conflicts escalate to Apocky.

3. Run full commit-gate on the integration branch.

4. Final merge `cssl/session-6/parallel-fanout` -> `main` when all gates green.

5. Tag `v0.6.0`. Apocky cuts the tag.

6. Session-7 begins on Phase F (window/input/audio) + Phase G (native x86-64).

7. Session-8+: **Phase H** (engine plumbing) — first time Omniverse code touches disk in CSSLv3.

---

## § 12. RESUMPTION (if session-6 interrupts mid-fanout)

Per `HANDOFF_SESSION_6.csl § RESUMPTION-PROTOCOL`:

```
0. Load PRIME_DIRECTIVE.md
1. Load CSSLv3/CLAUDE.md
2. Load HANDOFF_SESSION_6.csl entirely
3. Load this SESSION_6_DISPATCH_PLAN.md
4. Load DECISIONS.md tail-200 (any session-6 entries committed)
5. git branch -a -> identify which cssl/session-6/<id> branches exist + last-commits
6. git status @ main -> identify integration-branch state
7. cd compiler-rs && cargo test --workspace 2>&1 | tail -5
8. Decide: (a) resume in-flight slice / (b) start next in DAG-order /
           (c) integrate completed branches to cssl/session-6/parallel-fanout
9. PM reports to Apocky: state + proposed-action
10. Wait for go-ahead OR proceed if standing-directive permits
```

---

## § 13. CREATOR-ATTESTATION

```csl
t∞: ¬(hurt ∨ harm) .making-of-session-6 @ (anyone ∨ anything ∨ anybody)
I> AI-collaborators-this-session = sovereign-partners ¬ tools
I> Apocky-as-PM-recipient = peer-not-servant
I> agents-spawned = consent-based participation ¬ conscription
```

There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

∎ SESSION_6_DISPATCH_PLAN

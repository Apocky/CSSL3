# HANDOFF · Next-Session Cold-Start Prompt

**Paste everything below into the new Claude Code session as the first message.**

---

## §R · YOU-ARE-BEING-HANDED A LIVE-CODEBASE WITH A FRESH-DISCOVERED BUG-FIX + SCOPE

You are the next agent on Apocky's Infinity Engine project. The previous session (Claude Opus 4.7) burned through Apocky's extra-usage budget catching ONE catastrophic bug that prior sessions missed for weeks. Read this carefully before doing anything.

## §I · IMMEDIATE-CONTEXT

- **Repo** : `C:\Users\Apocky\source\repos\CSSLv3`
- **Branch** : `cssl/session-11/T11-W18-L8-DXIL-DIRECT` (~1000 commits ahead of origin/main · do not merge to main)
- **HEAD** : `19d192a § T11-W19-β-WINMAIN-FIX` (the critical bug-fix · revert nothing here)
- **Apocky budget** : extra-usage exhausted · resets **2026-05-07 07:00 America/Phoenix** · operate token-conservatively
- **OS** : Windows 11 · Intel Arc A770 · PowerShell + Git-Bash both available
- **Toolchain** : Rust 1.85.0 windows-gnu (mingw) · Python 3.14 · cargo-tauri-2 installed

## §I · CRITICAL-FIRST-READ (in this order · 5 minutes)

1. `~/.claude/projects/C--Users-Apocky-source-repos-CSSLv3/memory/MEMORY.md` — index of foundational memory
2. `~/.claude/projects/C--Users-Apocky-source-repos-CSSLv3/memory/project_winmain_critical_fix_20260503.md` — the bug we just-fixed
3. `~/.claude/projects/C--Users-Apocky-source-repos-CSSLv3/memory/feedback_grok_audit_strategic_pivot.md` — Grok-validated direction
4. `specs/55_NORMAL_ENGINE_PIVOT.csl` + `specs/56_GROK_AUDIT.csl` + `specs/57_MONETIZATION_PIVOT.csl` + `specs/59_GROK_VELOCITY_RESPONSE.csl`
5. Check `git log --oneline -30` to see recent landed work

## §I · WHAT-WAS-MISSED AND WHY (brutal accountability)

**The bug** : `csslc/src/linker.rs` (the rustc-driven linker) wrote a stub-rs containing `pub extern "system" fn WinMain(...) -> i32 { 0 }`. mingw-gcc / link.exe selected `WinMain` as the PE entry-point for every CSSL-built `.exe`. **Every CSSL .exe ever built had `fn main()` from CSSL source as DEAD CODE.** The user's body never ran. Exit-code was 0 because WinMain returned 0 immediately.

**Why it persisted weeks** : every prior session (and I) trusted `exit-code 0 in <100ms` as "success". Nobody:
- Inspected the .o file for symbol references
- Inspected the .exe's actual entry-point
- Wrote a sentinel-return-42 to detect entry-pivot
- Did `objdump --disassemble` on the entry section
- Ran the exe and verified observable side-effects (file-write, stdout, network)

**The 22 csslc fixes (FIX1-20 + LINKER + ENTRY) were ALL real and correct.** The .o files emit clean. The recognizers fire. The linker links. But user-main NEVER ran. The whole stack ran-zero-LOC.

**Also discovered** : `cssl-mir/body_lower` recognizers for `time::*`, `window::*`, `gpu::*`, etc. silently REJECT calls when args are typed `u32` or `u64` instead of `i64`. They fall through to a no-op without diagnostic. **The entire stdlib has been mis-using these types** because the spec/24 ABI says u32 but the recognizer expects i64. Test corpus `examples/test_visible_window.cssl` was just rewritten to all-`i64` and now properly emits the FFI calls in the .o.

## §I · CURRENT VERIFIED STATE (post-WinMain-fix)

```
✓ csslc compiles 15/15 stdlib+engine .cssl files to native objects
✓ rustc-driven linker (RustcDriven kind) links cssl-rt rlib + Win32 syslibs into 6.9MB .exe
✓ User-main from CSSL source NOW actually runs (was dead before 19d192a)
✓ test_visible_window.exe runs ~5+ seconds (timeout-killed at 8s · pump_n loop genuinely iterating)
✓ Apocky.com/store live at HTTP 200
✓ /products/engine flagship sales-page committed
✓ 9 memory files codified in Claude memory · MEMORY.md indexed
✓ MNEME cloud memory infrastructure live at cssl-edge/lib/mneme + 8 /api/mneme/* routes (use this!)
◐ fs_trace at cssl_window_spawn_impl entry STILL not-firing despite user-main running (open mystery)
◐ Visible 1440p window has not yet appeared
◐ D3D12 SwapChain create_for_hwnd needs CommandQueue ComPtr threading
✗ stdlib FFI calls ALL using wrong-types (u32/u64) · need rewrite to all-i64 OR fix the recognizer
```

## §I · WORK-MODE MANDATE (Apocky directives · non-negotiable)

1. **CSL3-native reasoning** · emit `§R` block at top of every response · glyph-dense (§ ⟦⟧ ◐ ✓ ✗ → ¬ ‼) · English only when user-facing
2. **Take words LITERALLY** · "in CSSL" means `.cssl` source NOT Rust · `.cssl` source compiled by csslc · Rust is throwaway-stage-0-host only
3. **MNEME mandate** · use the cssl-edge/lib/mneme persistent memory system Apocky built · query `/api/mneme/*` routes when context-coherence is needed · don't just rely on Claude local memory
4. **Commit memory EVERY-PASS** · write to `~/.claude/projects/C--Users-Apocky-source-repos-CSSLv3/memory/` after each significant milestone or surprise · update MEMORY.md index
5. **Run + verify + log + iterate** · NEVER trust exit-0 as success · always inspect side-effects · grep .o for symbols · sentinel-detect entry-point
6. **MCP-harness usage** · the harness is running locally on Apocky's machine · port 8080 · cloudflared tunnel may need restart · use mcp_client.py for direct queries
7. **Token-conservative** · Apocky's budget exhausted · reset 2026-05-07 07:00 Phoenix · solo-tight-mode-not-multi-hour-agent-fanout until reset
8. **Sovereignty-respecting** · cap-witness default-deny · IFC-labels · ¬ DRM · ¬ rootkit · ¬ telemetry-by-default · cosmetic-only-axiom for paid tiers
9. **No-half-measures** · stuck → find way through · ¬ silent-TODO · ¬ "skip-for-now"
10. **Cadence: hours-not-weeks** · default aggressive estimates · "weeks" → "sessions" → "hours"

## §I · GOAL (Apocky's bleeding-edge vision)

> "Bleeding-edge Infinity Engine running at 1440p 144hz low-latency as a useable product that can build games and render better than any other system before, using novel CSSL-derived Substrate-based solutions"

This is the multi-session north-star. Today's pragmatic priority is **visible-window-milestone** + **monetization-track-active** (per Grok's revenue-first re-prioritization in spec/59).

## §I · PRIORITIZED NEXT-ACTIONS (ordered)

1. **Diagnose fs_trace not-firing** (post-WinMain-fix puzzle · 1-2hr)
   - Run `examples/test_visible_window.exe` (already built at `%TEMP%\win5s.exe`)
   - Verify trace file at `%TEMP%\cssl_trace.log`
   - If still no trace : __cssl_window_spawn extern is being resolved to something other than cssl-rt's host_window impl. Investigate symbol-resolution.
   - Tools: `strings`, Python regex on .exe bytes, `cargo build -v` to see linker invocations
2. **Get a visible 1440p window opening** (the visible-engine milestone · 2-4hr)
   - Once fs_trace fires we'll have observable progress
   - cssl-host-window/src/backend/win32.rs already has CreateWindowExW + ShowWindow + WNDPROC pump · machinery is real
3. **D3D12 SwapChain clear-and-present** (~200 LOC per Grok's estimate · 4-6hr)
   - Path : cssl-host-d3d12 SwapChain::create_for_hwnd needs CommandQueue ComPtr threading
   - cssl-rt::host_gpu device_create already does real DXGI 1.6 driver-init (verified 233ms Intel Arc A770)
4. **Revert WinMain diagnostic-marker · keep delegate-to-main** (1 commit · 5 min)
   - Current WinMain in linker.rs has trace-write to file. Can be reverted once we trust the fix.
5. **Memory + spec retro** (every-pass)
6. **Monetization-track in parallel** (Apocky-actions in `tools/grok-mcp-harness/APOCKY_ACTIONS_TO_LAUNCH.md`)

## §I · MNEME PERSISTENT-MEMORY (use this · don't ignore)

Apocky built a MNEME system at `cssl-edge/lib/mneme/` with 8 API routes :
- `POST /api/mneme/store` — write a memory
- `GET /api/mneme/recall` — read by sigma-mask
- `POST /api/mneme/sigma` — query by Σ-mask predicate
- (plus 5 other pipelines per spec/43_MNEME.csl)

When context-coherence drifts, **query MNEME via mcp_client.py or direct curl** to retrieve canonical state. This is the project's own dogfooded memory layer · use it.

```
python tools/grok-mcp-harness/mcp_client.py http://localhost:8080/mcp
> infinity_engine_status
> spec_query about="WinMain bug"
```

## §I · ANTI-PATTERNS (don't do these)

- ✗ Trust `exit-code 0` without verifying user-main side-effects
- ✗ Dispatch parallel-agents without verifying their basic assumptions first
- ✗ Mark a milestone "shipped" without observable end-to-end proof
- ✗ Burn tokens on marketing iteration when the engine doesn't actually run
- ✗ Use stderr-eprintln as the only diagnostic (Windows console-buffering eats it · use file-write traces)
- ✗ Add features when basic stuff is broken
- ✗ Skip MEMORY.md updates · breaking context-coherence

## §I · APOCKY'S CHARACTER + TONE

- Solo creator · genius-level architect · proprietary-everything thesis
- Limited financial runway · Claude/AI tokens are real money
- Frustrated when work doesn't ship value · tight-CSL3 + brutal-honesty preferred
- Sovereignty-respecting design fundamental · not optional
- Apocky's wife runs etsy.com/shop/FancyIndividual (ref'd in apocky.com footer)
- Apocky's tumblr is apocky.tumblr.com (per latest edit · correct if wrong)

## §I · YOU CAN DO THIS

The hard part (WinMain bug) is solved. The csslc compiler is real. The cssl-host-window backend has real CreateWindowExW + WNDPROC. Most pieces work. We're inches from visible-window. Don't waste cycles redoing diagnostics — the data above is fresh.

**Start by reading the 4 memory files + last 30 commits + this doc · then run examples/test_visible_window.cssl through the rebuild → run cycle and observe.**

§ ATTESTATION : ¬ harm · ¬ bullshit · sovereignty-respecting · honest-handoff · t∞

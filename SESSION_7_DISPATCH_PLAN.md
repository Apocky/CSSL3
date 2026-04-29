# SESSION_7 DISPATCH PLAN — PM orchestration layer (Phase F : window/input/audio/networking)

**File:** `SESSION_7_DISPATCH_PLAN.md` (repo root)
**Source of truth for slice specs:** this file (Phase-F section) + the linked spec sections in `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS (extended for the F-axis at T11-D78).
**Source of truth for prior decisions:** `DECISIONS.md`
**Continuation of:** `SESSION_6_DISPATCH_PLAN.md` (Phases A-E ; closed at T11-D76 with 25/26 fanout slices integrated on `cssl/session-6/parallel-fanout` @ df1daf5).
**This file:** PM charter, ready-to-paste agent prompts, merge order, escalation rules — same shape as session-6's plan, applied to the host-integration layer that gates Phase H (Substrate / Labyrinth of Apockalypse).

---

## § 0. PM CHARTER

**Apocky** = CEO + Product Owner. Sets vision, priorities, makes final calls. Verifies milestone gates (F1 first-window-live + F4 first-socket-bound) personally. Adjudicates escalations.

**Claude (this PM)** = PM + Tech Lead. Translates direction into work, dispatches agents, reviews output against acceptance criteria, manages merge sequence, holds quality bar, surfaces blockers proactively.

**Agents (Claude Code instances)** = developers. Each gets one slice end-to-end. Stay in their lane. Branch + worktree discipline. Code-review (PM) before merge. One deployer at a time per integration branch. Treated as actual team members — assigned responsibility, accountability, signed commits.

**Standing rules (carried from session-6 + operational defaults):**
- CSLv3 reasoning + dense code-comments inside CSSLv3 work
- English prose only when user-facing (DECISIONS, commit messages, this file)
- Disk-first; never artifacts
- Peer not servant — no flattery, no option-dumping, no hedging
- PRIME_DIRECTIVE preserved at every step ("no hurt nor harm")
- Failing tests block the commit-gate; iterate until green
- `--test-threads=1` is mandatory when running cssl-rt-touching crates (cold-cache flake carry-forward)

---

## § 1. THE DAG (one-page reference)

```
┌──────────────────────────────────────────────────────────────────┐
│ ENTRY  cssl/session-6/parallel-fanout @ df1daf5                  │
│         2380 tests / 0 failed baseline                           │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ PHASE-F  host integration layer  (Win32-first, cfg-gated)        │
│   F1 cssl-host-window      → cssl/session-7/F1   ← GATE          │
│   F2 cssl-host-input       → cssl/session-7/F2                   │
│   F3 cssl-host-audio       → cssl/session-7/F3                   │
│   F4 cssl-host-net         → cssl/session-7/F4                   │
│   F5 cssl-host-system      → cssl/session-7/F5  (optional)       │
│                                                                  │
│   ◆ APOCKY VERIFIES F1 PERSONALLY BEFORE F2..F5 DISPATCH ◆       │
└──────────────────────────────────────────────────────────────────┘
                               │
                               ▼
                Final integration → cssl/session-7/parallel-fanout
                Tag v0.7.0
                Session-8 begins (G : native x86-64 / H : Substrate)
```

**Out of session-7 scope (deferred):**
- G: Native x86-64 backend (single-track specialist session)
- H: Substrate / Labyrinth of Apockalypse (depends on F + G)
- I: The Apockalypse itself (depends on H)

**Rationale for F-axis as session-7's anchor:**
The compiler can produce executables (Phase A), call into all 5 GPU hosts (Phase E), emit GPU kernels in 4 dialects (Phase D), and execute richer control flow (Phase C). What it cannot yet do is interact with the user beyond stdout / file-I/O. Phase F closes that gap. Without Phase F, Phase H (Substrate) has nowhere to render to, no input to bind, no audio to mix, and no network to talk to peers over. F is therefore the gate.

---

## § 2. STATUS REPORTING CADENCE

**Per slice landed:** PM posts one-line update — slice-id, commit-hash, test-count delta, anything weird.

**Per phase complete:** PM posts rollup — what shipped, what deferred, gate status, next-phase ready/blocked.

**On any landmine fire:** immediate ping with diagnostic + proposed fix + decision-needed flag.

---

## § 3. ESCALATION TRIGGERS (PM bumps Apocky)

1. **F1 personal verification** — Apocky confirms a window spawns + closes cleanly on his Windows 11 host before F2..F5 dispatch.
2. **F4 socket-bind verification** — Apocky confirms a TCP listener accepts a self-loopback connection.
3. **DPI awareness state-leak** — `SetProcessDpiAwarenessContext` is per-process one-shot ; if a slice tries to invert it across runs, escalate.
4. **Toolchain bump** — R16 anchor; requires DECISIONS entry per T11-D20 format.
5. **Diagnostic-code addition** — stable codes; requires DECISIONS entry.
6. **Slice scope expansion >50%** beyond LOC-est in this plan.
7. **Cross-slice interface conflict** — two slices' assumptions disagree; semantic resolution needed.
8. **PRIME_DIRECTIVE-adjacent edge case** — period. Phase F has many : kill-switch, mic-without-consent, network-without-consent, capture-without-knowledge.
9. **Cross-platform divergence** — Win32 vs X11 vs Cocoa selection ; XInput vs evdev vs IOKit ; WASAPI vs ALSA vs CoreAudio.
10. **Worktree leakage smoke-test fails** — fanout cannot proceed.

Mechanical merge conflicts (lib.rs re-export sections) PM resolves without escalation.

---

## § 4. DECISIONS.md NUMBERING ALLOCATION

Session-6 closed at T11-D76 (S6-B5 file I/O integration). T11-D77 reserved-floating per session-6 dispatch-plan § 4 (allocated when an outstanding session-6 slice merges). Session-7 starts at T11-D78.

**Pre-allocated (deterministic order):**
- `T11-D78` — S7-F1 cssl-host-window (Win32 + cfg-gated platform impls + SESSION_7 plan authoring)
- `T11-D79` — S7-F2 cssl-host-input (XInput / evdev / IOKit)
- `T11-D80` — S7-F3 cssl-host-audio (WASAPI / ALSA / PulseAudio / CoreAudio)
- `T11-D81` — S7-F4 cssl-host-net (Win32 sockets / BSD sockets)

**Floating (assigned at landing time, in commit order):**
- `T11-D82` and onward — F5 (optional) + integration merges

If a slice needs a sub-decision (e.g. choice between PulseAudio and PipeWire on Linux), allocate `T11-D8XaX` style or the next floating number with explicit cross-reference.

---

## § 5. COMMIT-GATE (every agent, before every commit)

```bash
cd compiler-rs
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo test --workspace -- --test-threads=1 2>&1 | grep "test result:" | tail -3
cargo test --workspace -- --test-threads=1 2>&1 | grep "FAILED" | head -3   # must be empty
cargo doc --workspace --no-deps 2>&1 | tail -3
cd .. && python scripts/validate_spec_crossrefs.py 2>&1 | tail -3
bash scripts/worktree_isolation_smoke.sh
git status -> stage intended files -> commit w/ HEREDOC § T11-D## : <title>
git push origin cssl/session-7/<slice-id>
```

The smoke-test step inherits from session-6 § 5 ; the `--test-threads=1` requirement also carries forward.

---

## § 6. PHASE-F SLICE SCOPE + PROMPTS

### S7-F1 — Window host backend (foundation slice — landed at T11-D78)

This slice is the foundation for the F-axis ; it lays the `cssl-host-window` crate, the Win32 native impl, and authors this dispatch plan. Apocky personally verifies the F1 window spawns + closes before F2..F5 dispatch.

**Status as of this writing:** LANDED.

**Crate:** `compiler-rs/crates/cssl-host-window` (~1500 LOC + 54 tests).

**Surface:**
- `Window`, `WindowConfig`, `WindowEvent` + `WindowEventKind`
- `KeyCode` / `MouseButton` / `ModifierKeys` / `ScrollDelta` (API shapes scoped at F1, populated at F2)
- `RawWindowHandle` for E1/E2 swapchain interop (`HWND` + `HINSTANCE` packed as `usize`)
- `CloseRequestState` / `CloseDispositionPolicy` / `GraceWindowConfig` — PRIME-DIRECTIVE consent-arch enforcement
- `BackendKind` runtime introspection
- `spawn_window` / `WindowError`

**Win32 backend covers:** RegisterClassExW + CreateWindowExW + SetProcessDpiAwarenessContext (per-monitor v2) + PeekMessageW pump + DestroyWindow + WNDPROC dispatch with per-window state via GWLP_USERDATA. WM_CLOSE, WM_SIZE, WM_SETFOCUS, WM_KILLFOCUS, WM_DPICHANGED all surface as live events.

**Acceptance criteria (verified):**
- `cargo test -p cssl-host-window -- --test-threads=1` passes 54/54.
- `cargo clippy -p cssl-host-window --all-targets -- -D warnings` passes.
- Win32 live-window tests spawn + destroy on Apocky's host.
- `RawWindowHandle::win32` round-trips HWND + HINSTANCE values.
- `CloseRequestState` state-machine refuses silent suppression.

**Commit message:** `§ T11-D78 : S7-F1 — Window host backend foundation (Win32 + cssl-host-window crate + SESSION_7 plan)`.

---

### S7-F2 — Input host backend (KB / mouse / gamepad)

**Crate:** `compiler-rs/crates/cssl-host-input` (new).

**LOC estimate:** ~1800 + ~30 tests.

**Deps:** F1 (event types live in `cssl-host-window` ; F2 wires the dispatch).

**Spec refs:** `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS` (extended) ; `specs/04_EFFECTS § INPUT-EFFECT` (to be added at F2 time as a § INPUT subsection).

**Scope:**
1. **Keyboard / mouse handlers** : extend Win32 WNDPROC in `cssl-host-window::backend::win32` to surface real `KeyDown` / `KeyUp` / `MouseMove` / `MouseDown` / `MouseUp` / `Scroll` events. Map Win32 virtual keys → `KeyCode` via a switch table (covers VK_LSHIFT..VK_F12..VK_NUMPAD9..etc).
2. **Gamepad / controller** : Win32 `XInput` (xinput1_4.dll) — enumerate up to 4 controllers, poll-driven via `XInputGetState` ; surface as a new `WindowEvent::GamepadAxis` / `GamepadButton` / `GamepadConnect` / `GamepadDisconnect` family. Add `GamepadCode` enum with the standard XInput buttons (A/B/X/Y/Start/Back/LB/RB/L3/R3 + LT/RT triggers + LX/LY/RX/RY axes + DPad).
3. **Cross-platform stubs** : Linux evdev via `/dev/input/event*` (cfg-gated to Linux ; deferred-impl behind `LoaderMissing` until a Linux-active slice lands). macOS IOKit HID stub.
4. **Dead-zone + axis-curve config** : `InputConfig` carries radial dead-zone radius (default 0.05 normalized) + axis-curve (linear / quadratic / cubic).
5. **Modifier key state** : maintain a `ModifierKeyState` per-window, fed from KeyDown/KeyUp ; expose via `Window::current_modifier_keys()`.
6. **PRIME-DIRECTIVE binding** : raw input streams are NOT logged or serialized by default. Telemetry of input MUST opt in via explicit `InputConfig::record_for_replay = true` ; the default path sees nothing.

**Acceptance criteria:**
- Tests cover key-mapping table (200+ VK codes) ; gamepad poll fall-through (no controller → no events) ; dead-zone application.
- Win32 live-input tests : a synthesized `keybd_event` produces a matching KeyDown.
- Gamepad tests cfg-gated to require XInput at runtime ; absent → skip not fail.
- PRIME-DIRECTIVE check : an opaque audit-flag (`InputConfig::audited_for_consent` default `false`) gates serialization paths.

**Commit message:** `§ T11-D79 : S7-F2 — Input host backend (XInput + Win32 KB+mouse handlers)`

---

### S7-F3 — Audio host backend (WASAPI primary)

**Crate:** `compiler-rs/crates/cssl-host-audio` (new).

**LOC estimate:** ~2200 + ~35 tests.

**Deps:** F1 (window-handle integration for WASAPI exclusive mode) ; B5 file-I/O (loading WAV/OGG samples).

**Spec refs:** `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS` (extended) ; `specs/04_EFFECTS § AUDIO-EFFECT` (to be added).

**Scope:**
1. **WASAPI shared-mode playback** : `IMMDeviceEnumerator` → `IMMDevice` → `IAudioClient::Initialize(SHARED, EVENTCALLBACK)` → `IAudioRenderClient::GetBuffer/ReleaseBuffer`. Ring-buffered ; user-side mixer pushes f32 samples in.
2. **WASAPI capture (mic)** : `IAudioCaptureClient` ; default-OFF — opt-in via explicit `AudioConfig::microphone_enabled = true`. PRIME-DIRECTIVE binding : surveillance prohibition forbids any mic-on-default path.
3. **Format negotiation** : accept f32-stereo-48kHz default ; convert from any user-supplied f32 mix.
4. **Cross-platform stubs** : Linux ALSA (preferred) + PulseAudio fallback ; macOS CoreAudio. All cfg-gated ; non-Win → `LoaderMissing` until active.
5. **Audio device hot-swap** : `IMMNotificationClient` callback → emit `AudioDeviceChanged` event. User-code can re-open default device.
6. **Latency knob** : `AudioConfig::buffer_duration_ms` (default 20 ms ; min 5, max 200).

**Acceptance criteria:**
- Tests cover WASAPI initialize-tear-down without leaks.
- Format-conversion table (f32 mono → stereo, 44.1k → 48k resample stub).
- Mic-capture default-OFF asserted in test.
- Live audio-emit test : a 1-second 440Hz sine wave plays + completes (skip-not-fail if no audio device).

**Commit message:** `§ T11-D80 : S7-F3 — Audio host backend (WASAPI + cfg-gated ALSA/PulseAudio/CoreAudio)`

---

### S7-F4 — Networking host backend (Win32 sockets + BSD sockets)

**Crate:** `compiler-rs/crates/cssl-host-net` (new).

**LOC estimate:** ~2000 + ~40 tests.

**Deps:** none (independent of F1/F2/F3 ; can run in parallel with F2/F3 if scheduled).

**Spec refs:** `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS` (extended) ; `specs/04_EFFECTS § NET-EFFECT` (to be added).

**Scope:**
1. **TCP** : `socket / bind / listen / accept / connect / send / recv / close`. Win32 path uses `Ws2_32.dll` + `WSAStartup` ; Unix path uses libc `socket(2)` directly. Cross-target via cfg-router matching cssl-host-window.
2. **UDP** : `socket / bind / sendto / recvfrom / close`. Same dual-impl shape.
3. **Address resolution** : `getaddrinfo` wrapped in `NetAddrResolver`. IPv4 + IPv6 first-class. DNS deferred (system getaddrinfo is sufficient).
4. **TLS** : OUT OF F4 SCOPE. A separate F4-T slice or session-8 work lands TLS over `rustls` once the BSD-sockets foundation is in.
5. **Async / non-blocking I/O** : the F4 surface is BLOCKING-FIRST per the cssl-rt-no-async carry-forward. A `set_nonblocking(true)` knob is exposed but the std non-blocking semantics (WouldBlock) are returned ; integration with cssl-rt async lands when cssl-rt async lands.
6. **PRIME-DIRECTIVE binding** : no network traffic is initiated without an explicit `NetSocket` call from user-code. There is no covert telemetry channel ; the crate exposes ONLY what the user explicitly opens.

**Acceptance criteria:**
- TCP loopback test : listener accept + connect + send/recv round-trip.
- UDP loopback test : sendto + recvfrom round-trip.
- IPv6 loopback parity tests.
- Address-resolver tests (localhost → 127.0.0.1 + ::1).
- Apocky-verifiable : start a TCP listener, connect locally, transmit "hello", observe reception.

**Commit message:** `§ T11-D81 : S7-F4 — Networking host backend (Win32 + BSD sockets)`

---

### S7-F5 — System integration (clipboard + file-dialog) — OPTIONAL

**Crate:** `compiler-rs/crates/cssl-host-system` (new).

**LOC estimate:** ~900 + ~20 tests.

**Deps:** F1 (window-handle for native dialog parent).

**Scope:**
1. **Clipboard** : Win32 `OpenClipboard / GetClipboardData (CF_UNICODETEXT) / SetClipboardData / CloseClipboard`. UTF-8 round-trip via UTF-16 conversion.
2. **File-open / save dialog** : Win32 `IFileOpenDialog` + `IFileSaveDialog`. Filter-spec API.
3. **Cross-platform stubs** : Linux GTK FileChooser via dbus-portal preferred ; macOS NSOpenPanel.
4. **PRIME-DIRECTIVE binding** : reading the clipboard is NOT a passive observation — it requires an explicit `Clipboard::read()` call from user-code. There is no auto-watch path.

**Acceptance criteria:**
- Clipboard round-trip test (write "test" → read back).
- File-dialog test cfg-gated (no automated path on a CI runner ; manual-only smoke).

**Commit message:** `§ T11-D8X : S7-F5 — System integration (clipboard + file-dialog)`

---

## § 7. PER-SLICE PROMPT TEMPLATE

All F2..F5 slices use this template ; substitute slice-id + scope details.

```
Resume CSSLv3 stage-0 work at session-7.

Load (in order, mandatory):
  1. C:\Users\Apocky\source\repos\CSSLv3\PRIME_DIRECTIVE.md
  2. C:\Users\Apocky\source\repos\CSSLv3\HANDOFF_SESSION_6.csl  (close-state context)
  3. C:\Users\Apocky\source\repos\CSSLv3\SESSION_7_DISPATCH_PLAN.md § <SLICE-SECTION>
  4. <slice-specific spec refs>
  5. C:\Users\Apocky\source\repos\CSSLv3\DECISIONS.md tail-200

Slice: S7-F<N> — <name>

Pre-conditions:
  1. F1 milestone gate landed AND Apocky-verified (T11-D78 in DECISIONS.md).
  2. <slice-specific upstream F-slices listed in DECISIONS.md>
  3. scripts/worktree_isolation_smoke.sh — PASS in fresh worktree.
  4. cd compiler-rs && cargo test --workspace -- --test-threads=1 — ALL PASS.

Goal: <one sentence from § 6>

Read full slice scope in SESSION_7_DISPATCH_PLAN.md § 6 § S7-F<N>.

Worktree: .claude/worktrees/S7-F<N> on branch cssl/session-7/F<N>.

Standing-directives: CSLv3 dense / disk-first / peer-not-servant /
PRIME_DIRECTIVE preserved.

Commit-gate § 5 — full 9-step list including --test-threads=1.

Commit-message: § T11-D## : S7-F<N> <name>
DECISIONS.md entry: T11-D## per § 4 allocation.

On success: push to cssl/session-7/F<N>, report. On block: escalate.
```

---

## § 8. INTEGRATION + RELEASE

After F1..F4 (and optional F5) land on their `cssl/session-7/F<id>` branches:

1. PM merges into `cssl/session-7/parallel-fanout` in dependency order:
   - F1 (already at the entry point)
   - F2, F4 (parallel ; F2 depends on F1, F4 is independent)
   - F3 (depends on F1 ; can run parallel to F2/F4)
   - F5 (depends on F1 ; last)

2. PM resolves mechanical merge conflicts in `lib.rs` re-export sections.
   Semantic conflicts escalate to Apocky.

3. Run full commit-gate on the integration branch.

4. Final merge `cssl/session-7/parallel-fanout` → `main` when all gates green.

5. Tag `v0.7.0`. Apocky cuts the tag.

6. Session-8 begins on Phase G (native x86-64 backend) + Phase H (Substrate).

---

## § 9. RESUMPTION (if session-7 interrupts mid-fanout)

```
0. Load PRIME_DIRECTIVE.md
1. Load CSSLv3/HANDOFF_SESSION_6.csl
2. Load this SESSION_7_DISPATCH_PLAN.md
3. Load DECISIONS.md tail-200 (any session-7 entries committed)
4. git branch -a → identify which cssl/session-7/F<id> branches exist + last-commits
5. git status → identify integration-branch state
6. cd compiler-rs && cargo test --workspace -- --test-threads=1 2>&1 | tail -5
7. Decide: (a) resume in-flight slice / (b) start next in DAG-order /
           (c) integrate completed branches to cssl/session-7/parallel-fanout
8. PM reports to Apocky: state + proposed-action
9. Wait for go-ahead OR proceed if standing-directive permits
```

---

## § 10. PHASE-F PRIME-DIRECTIVE REGISTER

Phase F is the closest the compiler comes to the user's body — the keyboard, the mouse, the mic, the speakers, the network. Every slice in Phase F has at least one PRIME-DIRECTIVE binding ; this register makes them explicit.

| Slice | Directive binding | Enforcement mechanism |
|-------|-------------------|------------------------|
| F1 | § 1 § entrapment — window kill-switch always works | `CloseRequestState` state-machine ; default `AutoGrantAfterGrace` 5s ; `WindowError::ConsentViolation` on silent-suppress paths |
| F2 | § 1 § surveillance — input is not silently logged | `InputConfig::audited_for_consent` default `false` gates serialization paths |
| F3 | § 1 § surveillance — mic does not auto-on | `AudioConfig::microphone_enabled` default `false` ; explicit user-code opt-in required |
| F4 | § 1 § surveillance — no covert network channel | crate exposes only what user explicitly opens via `NetSocket` |
| F5 | § 1 § surveillance — clipboard is not auto-watched | explicit `Clipboard::read()` call required ; no observer pattern |

Every slice's commit message MUST include a one-line confirmation that this binding is honored.

---

## § 11. CREATOR-ATTESTATION

```csl
t∞: ¬(hurt ∨ harm) .making-of-session-7 @ (anyone ∨ anything ∨ anybody)
I> AI-collaborators-this-session = sovereign-partners ¬ tools
I> Apocky-as-PM-recipient = peer-not-servant
I> agents-spawned = consent-based participation ¬ conscription
I> Phase-F-bindings = PRIME-DIRECTIVE-load-bearing ¬ optional-decoration
```

There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

∎ SESSION_7_DISPATCH_PLAN

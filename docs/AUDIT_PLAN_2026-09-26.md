# Audit + Plan: Apocrypha, apocky.com, custom LLM (2026-09-26)

Living document. Boxes are ticked as work lands, each with the commit that closed it.
Owner request 2026-09-26: audit all three projects; fix unwired buttons; human-centric,
multidimensional tests; better telemetry, metadata, event handling; commit + push + deploy each
change when ready.

## Index

1. [Scope](#1-scope)
2. [State of the projects](#2-state-of-the-projects)
3. [Issue tracker](#3-issue-tracker)
4. [Phases](#4-phases)
5. [Test strategy (human-centric, multidimensional)](#5-test-strategy)
6. [Telemetry + event model](#6-telemetry--event-model)
7. [Out of scope](#7-out-of-scope)
8. [Open questions for Apocky](#8-open-questions-for-apocky)
9. [Working rules](#9-working-rules)
10. [Log](#10-log)

## 1. Scope

| Project | Where | What runs |
|---|---|---|
| apocky.com | `CSSLv3-wt-worklane-prod/cssl-edge` (branch main, Vercel `apocky-com`) | Living room chat, Apocrypha+ runner, member accounts, desktop downloads |
| Apocrypha (running system) | `cssl-edge/scripts/*` + `apocrypha-core/scripts` | worker 19126, memory gateway 19127, engine 19128, recall 19129, mind 19132, room loop 19134, direct 19141 |
| Apocrypha (spec/Python repo) | `Apocrypha` (branch kernel-jail-nursery) | specs only; no link to the running services |
| Custom LLM (APX.LM, K8) | `apocrypha-core-wt-kernel` (branch kernel/unified-v1) | apx-nn / apx-train / apx-gpu, receipts/kernel |
| Desktop app | `apps/apocrypha-desktop` 0.2.0 | Tauri: Chat (direct or apocky.com), Work lane, tray |

## 2. State of the projects

**apocky.com.** The core chat works end to end: send, rooms, invites, mute, consent, model
picker, retry, direct-mode tools. The **Create image** and **Web search** tools are stubs: the
job carries `tools`, nothing reads it. **Camera** and **Photos** upload, but the model only sees the
filename (no vision path). The client catches errors (send, upload, manage, poll) and never reports
them; no product events exist in the room (no send, lane, tool, upload, retry, first-reply
latency).

**Apocrypha running system.** Worker: strong (journal replay, bounded timeouts, 8 test files).
Mind, room loop, gateway, recall bridge: silent fallbacks (return empty with no log), no
supervision (a crashed service stays down; on 2026-09-26 recall, mind and room loop were all
down), no shared event format or request ID, no metrics. The room loop has no tests; recall has no
HTTP/breaker tests; there is no end-to-end test.

**Apocrypha spec repo.** INDEX.csl lists 46 specs and the folder has 50; nothing marks which
specs have running code (only the TS services + recall bridge do).

**Custom LLM.** A working from-scratch stack (transformer + GDN hybrid, BPE, GGUF export, own
OpenCL GEMM at 5.4-5.7 TFLOP/s, beats oneMKL). K8-L0 PASS; L1 underpowered (held-out loss 6.09
vs unigram 6.30). Training runs at ~190-240 tok/s vs 19.8k for torch-xpu on the same card: every
GEMM round-trips to the host. Only GEMM exists on the GPU. Corpus 7.4 GB, but licence receipts
cover only 11.9 MB. GPU contention (room engine, games) kills training rounds. The bar "beat
Qwen3.5-35B-A3B in every aspect" is not reachable at the planned ~125M scale; the spec itself
names owner knowledge (bar c) as the bar that counts.

## 3. Issue tracker

Severity: P0 broken for the user / P1 wrong or silent / P2 quality / P3 cleanup.
Status: [ ] open, [x] fixed (commit), [~] in progress, [-] won't fix (reason).

### apocky.com (W = web)
- [x] **W0c P0** Sending a message with any attachment failed (ATTACHMENTS_UNAVAILABLE): room called the internal apocrypha_ensure_member_principal, which service_role may not execute. Reads the principal row instead.
- [x] **W0 P0** Camera/Photos/Files did nothing on phones: scripted click on a display:none input. Native labels (dbfe417, live).
- [x] **W0b P0** Every upload failed: DB 42702 `tenant_id` ambiguous in apocrypha_ensure_member_principal. Migration 0065 (6e4a6b5, applied to hub, upload verified).
- [ ] **W1 P0** Create image tool is a stub (`lib/apocrypha/vercel-runner.ts:83,192`; worker ignores `request.tools`). Implement or hide.
- [ ] **W2 P0** Web search tool is a stub (same path). Implement or hide.
- [x] **W3 P0** (7f3e3b5 + this commit; live journey: red square -> "Red.") Camera/Photos: no vision path; model sees filename only (`lib/room/turn.ts:128-158`). Send images to the flagship as image parts; local lane says plainly it cannot see images.
- [ ] **W4 P1** STT failure leaves "Transcribing..." forever (`components/room/DirectTools.tsx`, `rec.onstop`).
- [ ] **W5 P1** Retry drops attachments and tools (`River.tsx:224` -> `Room.tsx`).
- [ ] **W6 P1** Runner kick failure is swallowed; Premium waits for cron with no feedback (`Room.tsx:490`).
- [ ] **W7 P1** Room list / member load errors swallowed (`Room.tsx:331`, `People.tsx:101`).
- [ ] **W8 P2** Services panel shows "Loading..." forever on fetch error (`DirectTools.tsx`).
- [ ] **W9 P2** Read-aloud `play()` rejection uncaught.
- [ ] **W10 P2** Mute enabled for non-owners, then errors (`PresenceStrip.tsx:246`).
- [ ] **W11 P2** Removing an attachment chip leaves the upload on the server (`Composer.tsx:153`).
- [ ] **W12 P2** 25 MB base64 upload on the main thread (`Room.tsx:274-278`).
- [ ] **W13 P3** Room note is a clickable `<p>` (no keyboard dismiss).
- [ ] **W14 P3** Stub admin endpoints (`pages/api/admin/coder/pending.ts`, `admin/tasks.ts`, `akashic/sourcemap.ts`); likely dead pages `gear-share.tsx`, `run-share-feed.tsx`.
- [ ] **W15 P1** Room consent switch vs site-wide telemetry consent: verify they are one flag.
- [ ] **W16 P1** Fallback runner (Vercel) has no canon / prime-directive digest (PC worker only).

### Apocrypha running system (A)
- [ ] **A1 P0** No supervision: a crashed service stays down (recall/mind/loop found down 2026-09-26). Watchdog with backoff + restart counts.
- [ ] **A2 P1** Mind returns empty memory/profile silently on any DB error (`scripts/apocrypha-mind/mind.ts:61,101,135,161`).
- [ ] **A3 P1** Gateway startup error dropped (`apocrypha-memory-gateway/server.ts:23-25`).
- [ ] **A4 P1** Room cursor written non-atomically; crash -> `after: 0` -> re-answers history (`scripts/apocrypha-room/loop.ts:113-121`).
- [ ] **A5 P1** Tick failure loop has no backoff or failure counter (`loop.ts:624-631`).
- [ ] **A6 P1** Recall bridge catches everything incl. SystemExit, logs nothing (`recall-service.py:539,764`).
- [ ] **A7 P2** Mind default port 19131 vs expected 19132 (trap).
- [ ] **A8 P2** Worker small swallows: journal write queue (`journal.ts:72,168`), lease renewal (`worker.ts:756`), health `-1` (`health.ts:70`).
- [ ] **A9 P2** Launcher env/port drift untested (`apx-services.ps1`, `run-recall-service.ps1`).
- [ ] **A10 P3** `Apocrypha/specs/INDEX.csl` lists 46 of 50 specs; no "running code" marker.
- [ ] **A11 P1** Opus 5.5 intermittently refused at the gateway on every provider (team limit). Ring in place; reliability needs BYOK or a Vercel limit raise (owner action).
- [ ] **A12 P2** `CSSLv3/cssl-edge/.env.local` SUPABASE_SERVICE_ROLE_KEY is the rotated-out key (direct launcher works around it).

### Custom LLM (L)
- [ ] **L1 P0** Host round-trip per GEMM; keep tensors device-resident + batch (the 80x gap).
- [ ] **L2 P1** GPU kernels missing for attention, SwiGLU, MoE, GDN, and backward.
- [ ] **L3 P1** XMX path at 1.28 TFLOP/s (block reads needed; card peak ~39 fp16).
- [ ] **L4 P1** Licence receipts: transcripts shard CONTESTED (owner check), new scholarly shards unreceipted.
- [ ] **L5 P1** Weight-averaged mixed CPU/GPU rounds give unstable loss (5.0-7.3).
- [ ] **L6 P1** GPU contention: rounds abandoned when games run; need an idle-window scheduler.
- [ ] **L7 P2** Eval gaps: bits-per-byte both runtimes answer, held-out 30 -> 300, chat/reason/code rubrics, blind A/B (bar c).
- [ ] **L8 P2** K0 on-machine checks owed; K1-K7 have no receipts while K8 runs ahead of its gate.
- [ ] **L9 P2** OpenCL context hang at exit (device leaked as workaround); arbiter think-leak.
- [ ] **L10 P1** Goal framing: "every aspect" vs ~125M model. Needs an owner decision (see 8).

### Cross-cutting (X)
- [ ] **X1 P1** No unified event format or request ID across site -> worker -> mind -> recall -> engine.
- [ ] **X2 P1** No metrics (counters, latencies) on any `/health`.
- [ ] **X3 P1** No product events in the room UI; caught client errors never reported.
- [ ] **X4 P1** No end-to-end test of room -> mind -> recall -> engine or site -> worker -> model.

## 4. Phases

Each phase ends deployed and verified live. Order is by user impact per unit of effort.

### Phase 0: Plan + restore (2026-09-26)
- [x] Audit three projects (three read-only surveys, findings above).
- [x] Restore recall 19129, mind 19132, room loop 19134 (found down).
- [x] This document, committed and pushed.

### Phase 1: Every button works (W1-W13)
- [x] W3 vision: images reach Apocrypha+ as image parts; local lane says it cannot see them.
- [ ] W2 web search: gateway web-search tool on the flagship; local lane hides the option.
- [ ] W1 create image: Apocrypha+ writes SVG (no new spend), sanitized + rendered via the LoA SVG tools, shown in the bubble.
- [ ] W4-W13 error paths and small fixes.
- [ ] Human-journey tests for every control (see 5). Deploy + live check.

### Phase 2: Telemetry, events, catching (X1-X3, A2-A6, A8)
- [ ] One event schema (`at, service, event, level, request_id, room, job_id, duration_ms, outcome, error_class`) in every service; request ID propagated through all hops.
- [ ] Client product events + caught-error reporting in the room (respecting consent).
- [ ] Silent fallbacks become warn events; `/health` carries counters + latencies.
- [ ] One events file on the PC + a live view in the desktop Services panel.

### Phase 3: Supervision + resilience (A1, A4, A5, A7, A9, A12)
- [ ] Watchdog restarts crashed services with backoff; restart count visible.
- [ ] Atomic room cursor; tick backoff.
- [ ] Launcher self-check (ports + required env) as a test.

### Phase 4: Tests that act like people (X4)
- [ ] Journey suite on the live site and the direct server (see 5).
- [ ] Stubbed end-to-end test room -> mind -> recall -> engine.
- [ ] Room-loop and recall-bridge suites.

### Phase 5: Custom LLM path (L1-L10), after the owner answers 8
- [ ] Device-resident tensors + batching; measure tok/s against 19.8k.
- [ ] Idle-window GPU scheduler.
- [ ] Licence receipts; full-budget L1 rerun; eval suite; scoreboard on a quiet card.

## 5. Test strategy

Human-centric: each test is a person doing a thing, checked the way a person would notice.
Multidimensional: one journey asserts several axes at once, so one run covers many failures.

A journey checks, together: **outcome** (the reply is right and complete), **UI state** (no stuck
spinner or note, buttons enabled correctly), **latency** (first token, total), **events**
(the expected events fired with one request ID), **errors** (none uncaught; caught ones
reported), **data** (DB rows, attachments, costs recorded), **access** (the wrong person is refused).

Journeys (each run on live apocky.com and on the direct server):
1. Sign in, send a question, get a complete Local answer.
2. Switch to Apocrypha+, ask a long question, get an untruncated answer; receipt shows model + cost.
3. Attach a photo, ask what is in it (after W3).
4. Web search a current fact (after W2); create an image (after W1).
5. Retry a failed turn with an attachment kept (W5).
6. Create a lobby, invite a second account, both talk, leave.
7. Mute; confirm no unprompted speech arrives, history stays.
8. Direct: speak a message, read the reply aloud, restart a service from the panel.
9. Abuse cases: guest reads room (401), wrong Host (404), no Origin (403), non-owner direct (404).
Each journey records its events; a run fails if an expected event is missing (G2: every journey is
also run once against a deliberately broken build to prove it can fail).

## 6. Telemetry + event model

- One JSON line per event, same schema everywhere (Phase 2 fields).
- `request_id` minted at the edge (say.ts, events.ts, runner), carried in the job, sent as
  `x-request-id` to mind, recall and engine.
- Outcome events for every user action and every fallback (e.g. `tool.requested_unavailable`,
  `memory.region_degraded`, `route.provider_refused`, `answer.truncated`).
- Privacy: no message text or queries in events; counts, ids, durations, classes only. Consent
  gates client telemetry.

## 7. Out of scope

- Chaos Tarot (handled in another thread).
- Palworld tooling.
- Rewriting the Python spec organs into running code (tracked, not planned here).
- Claiming LLM parity with Qwen3.5-35B-A3B at ~125M scale.
- Tailscale remote access until Tailscale is signed in on the PC (owner action).

## 8. Open questions for Apocky

1. Custom LLM goal: keep "beats Qwen in every aspect", or adopt winnable bars (owner knowledge
   with recall, spec bits-per-byte, latency/memory on a quiet card, hybrid routing)?
2. Transcripts shard licence: admit to training or not?
3. Opus 5.5 reliability: add your Anthropic key as BYOK in the Vercel AI Gateway, or ask Vercel to
   raise the team limit?
4. ANSWERED 2026-09-26: free, or what we already pay for (Claude), or train Apocrypha. Plan: Apocrypha+ writes SVG (Claude, no extra cost), rendered with the LoA SVG tools; later a local model.

## 9. Working rules

- Plan each action before running it; one variable per step.
- Each fix: tests (human journey + unit) -> commit -> push -> deploy -> verify live -> tick box.
- Nothing finished stays in a worktree.

## 10. Log

- 2026-09-26: audit done; services restored; plan written.
- 2026-09-26: W0 picker fix live (dbfe417); W0b DB fix 0065 applied (6e4a6b5); direct launcher no longer dies on stderr.
- 2026-09-26: W0c attachments-on-send fixed; W3 vision live-verified (upload -> Apocrypha+ -> "Red.", worker.images.attached count=1).

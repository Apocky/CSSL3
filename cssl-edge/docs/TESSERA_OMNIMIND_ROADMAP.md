# Tessera / OmniMind Runtime Roadmap For Lazarus

This document is the `cssl-edge` operational companion to `C:\Users\Apocky\source\repos\Omnimindv2\specs\06_LAZARUS_TESSERA_SYNTHESIS.csl.md`.

It keeps production Lazarus stable while defining how Tessera becomes the reasoning backend behind Lazarus tasks.

## Working Model

```text
Lazarus = control plane, queue, runner, approvals, admin UI
Tessera = cognitive runtime, sub-minds, LR routing, memory, confidence
DeepSeek = optional LR tier, disabled in production until gates pass
```

The first integration is a bridge contract, not a production behavior change.

## Parallel Lanes

| Lane | Workstream | Files | Can Run While |
|---|---|---|---|
| Lazarus Ops | deploy/smoke/service runner | `pages/api/admin/lazarus/*`, `scripts/lazarus-runner.ts`, `scripts/lazarus-service.ps1` | Tessera spec/runtime work |
| Tessera Core | LR/router/memory runtime | `Omnimindv2/crates/tessera-*` | docs/UI work |
| Bridge Contract | DTOs/events/artifact mapping | new bridge module + Omnimind spec 06 | Lazarus MVP smoke |
| Admin UX | cockpit page and event replay | `pages/admin/tessera-omnimind.tsx` | runtime stubs |
| Safety/Cost | approvals, caps, PD surfacing | Lazarus approvals + Tessera budget ledger | UI docs and tests |

## Concurrency Rules

```text
one deploy terminal at a time
one long-lived Lazarus runner at a time
prefer npm run lazarus:service:smoke during integration work
no production API route edits without test:lazarus + check + build
no live model calls until bridge dry-run and cost caps pass
no secret values in logs, docs, events, screenshots, or chat
```

## Phase Plan

### S0 — Docs + Indexes

Current slice. No production behavior changes.

Deliverables:

```text
Omnimindv2/specs/06_LAZARUS_TESSERA_SYNTHESIS.csl.md
Omnimindv2/specs/00_MASTER.csl.md index update
Omnimindv2/README.md spec tree update
Omnimindv2/DECISIONS.md D025
cssl-edge/docs/TESSERA_OMNIMIND_ROADMAP.md
cssl-edge/README_CLAUDE_CODE_HANDOFF.md link/update
```

### S1 — Lazarus Production MVP

Use current Lazarus implementation and Windows service tooling.

Required checks:

```powershell
npm run test:lazarus
npm run test:auth-redirect
npm run test:health
npm run check
npm run build
```

Live checks:

```text
public root/login/auth callback/health succeed
unauthenticated Lazarus admin endpoints return 401/403
wrong runner token returns 401
one-shot runner registers and completes a stub-safe smoke task
```

### S2 — Bridge Stub

Add an inert bridge module behind `LAZARUS_TESSERA_BRIDGE=1`.

Current implementation files:

```text
lib/tessera/types.ts
lib/tessera/bridge.ts
lib/tessera/runner-client.ts
tests/lib/tessera-bridge.test.ts
tests/lib/tessera-runner.test.ts
```

Current verification:

```powershell
npm run test:tessera-bridge
npm run test:tessera-runner
npm run check
```

The bridge is pure TypeScript mapping/validation only: no API route, no fetch,
no Supabase write, no runner mutation, no model call.
`scripts/lazarus-runner.ts` can now emit Tessera dry-run events when
`LAZARUS_TESSERA_BRIDGE=1`, but it still forces dry-run in this phase.

Contract:

```text
LazarusTask -> TesseraGoalEnvelope -> TesseraResult -> Lazarus events/run summary
```

Required fields:

```text
lazarus_task_id
lazarus_run_id
trace_id
goal_text
role
tier_policy
budget
privacy_class
approval_policy
artifact_policy
max_depth <= 3
dry_run
```

### S3 — Tessera Runtime Stub

OmniMindv2 provides a deterministic dry-run runtime that accepts `TesseraGoalEnvelope` and emits JSON events.

No DeepSeek call required.

### S4 — LR Routed Execution

Enable Tessera LR tiers after dry-run gates pass.

Order:

```text
T1 local adapter
cost ledger
approval-gated T3 DeepSeek adapter
speculative critic for code tasks
```

### S5 — Admin Cockpit

Add `/admin/tessera-omnimind` as an adjacent dashboard.

Current implementation files:

```text
pages/admin/tessera-omnimind.tsx
tests/pages/tessera-omnimind.test.ts
components/AdminLayout.tsx
```

Current verification:

```powershell
npm run test:tessera-omnimind-page
npm run check
npm run build
```

Expected panels:

```text
goal stack
sub-mind tree
LR call ledger
cost meter
confidence meter
approval projection
event replay
artifact list
```

## File Handling Protocol

Do not mix unrelated lanes in one patch.

Recommended slice shapes:

```text
docs-only synthesis slice
bridge DTO + tests slice
Tessera runtime stub slice
admin cockpit shell slice
cost/approval hardening slice
live LR enablement slice
```

Every slice records:

```text
files touched
tests run
production risk
secrets observed: no values printed
next gate
```

## Current Production Guardrails

Keep these true until S4 is explicitly opened:

```text
LAZARUS_ENABLE_MODEL_CALLS=0
LAZARUS_TESSERA_BRIDGE unset or 0
DEEPSEEK_API_KEY may exist but must not be used
runner service uses stub-safe execution
```

## First Implementation Threads

Thread A can continue Lazarus deploy/smoke independently.

Thread B can start Omnimindv2 Tessera crate stubs independently.

Thread C has landed the first inert DTO/dry-run adapter and runner-side event
wiring behind `LAZARUS_TESSERA_BRIDGE=1`. Next work is production smoke plus a
one-shot bridge runner smoke, still with `LAZARUS_ENABLE_MODEL_CALLS=0`.

Thread D has landed the first cockpit shell at `/admin/tessera-omnimind`, using existing Lazarus endpoints and Tessera event kinds.

Thread E can harden cost and approval gates before live LR calls.

## Exit Criteria For Real Synthesis

The synthesis is real, not just documented, when:

```text
Lazarus queues a task
runner leases it
bridge sends TesseraGoalEnvelope
Tessera emits sub-mind/LR events
Lazarus records events and artifacts
admin can inspect trace/cost/confidence
external effects wait for approval
run finishes with replayable provenance
```
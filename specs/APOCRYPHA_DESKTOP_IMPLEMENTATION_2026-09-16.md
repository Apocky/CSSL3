# Apocrypha Desktop Implementation

Approved by Shawn's direct "Start implementation" instruction on September 16, 2026. Evidence timestamps may be September 17 UTC. English documents and user communication; CSL internal reasoning.

This is the executable continuation of the full research plan retained in session memory and the evidence checkpoint at `apocrypha-core/specs/FULL_AUDIT_2026-09-16.md`. The audit remains incomplete; an old completion label is not production evidence. The companion JSON is this objective's dependency and coverage record. It does not replace the older entity research graphs or silently activate their dormant goals.

## Outcome

One local, owner-operated Apocrypha coding agent on Windows, with a minimal native desktop interface and Android remote control of the SAME entity and tasks. It must read the real repository, create/edit files on disk, run checks, inspect failures, repair changes, and preserve results. Code printed in chat is not implementation.

No website login, Cloudflare, Vercel or cloud database is required for local work. Qwen is the initial language/code faculty; Apocrypha owns tasks, effects, memory, evidence and continuity. Grant the workspace once for routine authorized work. Do not confuse a local grant with public access or unrestricted authority for a remote device.

## Architecture and Reuse

- Keep the tested TypeScript WorkAgent, EngineLike, TurnRunner, SessionStore, Workspace and file/shell/MCP contracts as the initial host. Package a pinned runtime and compiled sources, not an invocation requiring the developer checkout.
- Adapt the existing Tauri application from a cloud-account client into a native client of that local host. The visible window is not the task owner. Closing it must not discard work.
- One executor applies changes. Rust apx-agents contributes optional model-backed proposals/critiques through the host's model lease, not a second write path. apx-hive is serving-process routing, not a proof system.
- Use a single transactional task store and durable event cursor. Record effect intent before execution; acknowledge/stream only committed events. Filesystem and shell effects need their own recovery protocol; SQLite does not make external effects transactional.
- Reuse the memory federation, exact source provenance and task working set. Repair failed adapters, strict expiry/deletion, stale-snapshot diagnostics and learning resumption. Never multiply confidence merely because several stores repeat one source.
- Preserve the historical entity denominator of 18 organs, 10 Brainmonsoon lenses, 10 stores and 22 instruments as rows to reconcile. Do not equate old graph completion with current running integration or replace the entity with a system prompt.
- Private phone transport uses Tailscale Serve and application-level paired-device authorization. No Funnel or exposed raw engine/shell endpoint. Offline approvals are never automatically replayed.
- One workbench: task thread, workspace/task switcher, stop control, and collapsible Files/Changes, Terminal, Artifacts and Context inspector. Model controls, devices, memory and maintenance belong in settings. All functions remain reachable.

## First Acceptance Milestone

From the installed desktop app, request a realistic change in a fixture repository containing pre-existing owner edits. Apocrypha must modify two required files, create a required file, run a check known to fail before the change, inspect and repair an intermediate failure, and leave the final changes on disk. Independently read the bytes and rerun the check. Restart and recover the task. Undo only the agent's changes, preserving the owner's edits, and observe the original regression fail again. Repeat task control from the paired phone after desktop acceptance.

Passing mocks supports a slice; it does not satisfy this milestone. Mobile, federation, learning and research remain in the complete objective even while the first milestone is being delivered.

## Phases

| Phase | Work | Dependencies | Exit Evidence |
| --- | --- | --- | --- |
| P0 | Source/runtime preservation, contracts and baseline | None | Exact heads/dirty paths, known-failing tests, coverage graph |
| P1 | Resident local host, transactional sessions/events and crash recovery | P0 | No lost acknowledged events or replayed uncertain effects; restart recovery |
| P2 | Federated task memory and entity/cycle ownership | P0; integration P1 | Relevant exact recall, deletion/expiry/injection failures, honest adapter states |
| P3 | Complete file tools, command outcomes, scope, transactions/review/undo and process cancellation | P0; integration P1 | Real disk edits and tests; conflict-safe undo; negative authority controls |
| P4 | One model manifest/supervisor, progress, warmup and resource scheduling | P0; integration P1 | Model/config identity, bounded stalls, effective knob readback |
| P5 | Local native desktop workbench and packaging entrypoint | P0; integration P1/P3/P4 | Actual native app completes the first disk task without website login |
| P6 | Android pairing, scoped remote effects and durable reconnect | P1/P3/P5 | Same PC task from phone, duplicate/stale command rejection and revoke |
| P7 | Selective native review teams and nonconflicting parallel work | P1/P2/P3/P4 | Measured improvement versus single agent; no second effect authority |
| P8 | Outcome-backed learning and idle cognition | P1/P2/P4 | Failed-batch resume, correction/deletion propagation, foreground priority |
| P9 | Hardware-local model/runtime/context/strategy research | P0/P4; relevant intervention gates | Correct-task frontier with failures and rejected candidates retained |
| P10 | Installed-product acceptance, signed updates, backup and delivery | Required functional exits | Installer/APK and source hashes; real task, restore, update and remote readback |

## Complete Coverage

C01 fresh local install; C02 persistent entity/tasks/recovery; C03 code/line/symbol search; C04 real file/directory operations; C05 preimages, transactions, review and undo; C06 real builds/tests/status; C07 background jobs and tree cancellation; C08 queue/interrupt/resume/budgets; C09 CLI and MCP ingress; C10 external MCP/resources/prompts; C11 one coder faculty; C12 backend identity/warmup; C13 effective dials/resource controls; C14 task context/recall; C15 federation availability; C16 durable corrections/provenance; C17 cognitive/FHRR/Brainmonsoon contributions; C18 selective review teams; C19 isolated parallel tasks; C20 continuous useful idle cognition; C21 artifacts/export; C22 browser/DOM/screenshot verification; C23 image input and approved local generative tools; C24 minimal shared desktop UI; C25 device pairing/revocation; C26 phone task control; C27 mobile lifecycle/accessibility; C28 causal diagnostics and missing-event detection; C29 measured candidate selection; C30 bespoke training/runtime promotion; C31 backup/update/storage lifecycle; C32 source-to-installed-release traceability.

An unavailable image backend is a named gap, not a fake button. No paid compute, new hardware purchase, public filesystem exposure, unrelated website redesign, or unconditional training is included.

## Verification and Research

Reuse the existing Work tests, native apx-agents tests, PRISM terminal/composition/adaptive-stop instruments, and _HARNESS's scoped counterexamples. Candidate arms: direct Qwen; current Work; durable host; admitted memory; selective reviewers; separately trained/native candidates. Keep identical task snapshots, model/dial hashes, budgets, warm/cold conditions and holdouts. Record failed runs. Select by correctness first, then time-to-correct-result, interventions and resource headroom. No claim of globally best local model is made by this plan.

Research questions: total latency decomposition; checkpoint/quant choice; CPU/GPU/batch/KV placement; prefix reuse; memory relevance; selective teams; stopping rules; compatible speculative decoding; effect-tested sampling/grammar/bias controls; native decoder parity; outcome-backed learning; useful cognitive-component ablations. Each has a null/known-bad control and a retained rollback candidate. Existing 8192/2048 MoE tools-prompt crash evidence remains a regression fixture, not permission to try crashing the current engine.

Required failure scenarios include nonzero command exits, disk full, crash between effect/result, duplicated or changed-body command IDs, stale event cursors, symlink/junction escape, owner edit after approval, partial multi-file write, surviving subprocess, hung/malicious MCP, wrong model on a live port, unsupported template arguments, contentless stream, GPU pressure, expired/deleted/injected memory, correlated reviewers, failed learning batch gaps, expired pairing, conflicting phone approvals, and interrupted signed updates. Preserve the known-bad outcomes and record the precise scope of every pass.

## Lane Ownership

Integration lead owns this plan/graph, shared contract integration, server/config changes, release and live process lifecycle. Nonconflicting lanes own (A) sessions/runner recovery; (B) file tools and their tests; (C) desktop-native controller/UI; (D) memory cache/adapter fixes; (E) native strategy research. Shared type changes require integration-owner handoff. Performance trials and learning reserve the single GPU; no parallel model swaps. A blocked lane never stalls independent source/test work.

## Implementation Evidence

- Start: app branch `claude/work-lane-prod-20260914`; existing graphify and tsconfig build-info changes left untouched.
- First null: new real-shell test exiting 7 failed because WorkAgent reported success.
- First repair: shell boundary rejects nonzero exits, termination and cancellation while preserving output; all nine Work test groups then passed. This is command-status evidence, not whole-product completion.

## Delivery and Rollback

Commit only owned verified paths, push to the intended existing remote branch, and read back commit/tree identity. Keep original session JSON/JSONL and current services until replacements pass the required checks. Do not blanket merge scratch work or delete old environments/data. A manual owner install does not require a paid signing certificate; automatic updates still require their signature trust, and Android keeps its existing signing identity. Phone trust and external credential handling remain scoped gates, not blockers to local tools.
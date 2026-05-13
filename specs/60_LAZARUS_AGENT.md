# 60 — LAZARUS AGENT

**Status**: design v1 · 2026-05-06
**Owner**: Apocky / sovereign
**Depends**: 27 (Sigma Mask), 43 / 44 / 45 (MNEME), cssl-edge stack, apocky.com domain
**Replaces**: nothing — net new
**Working name**: LAZARUS. Rename freely; nothing in this doc cares.

---

## 0. Why this exists

Cloud coding agents (Cursor / Copilot / Devin / etc.) are sub-par
at our work because:

1. **No persistent memory** — every session starts cold, every
   convention re-explained, every prior decision re-litigated.
2. **No CSL-canonical reasoning** — facts and decisions get buried
   in chat history nobody re-reads.
3. **No sovereignty** — code, prompts, and intermediates live on
   someone else's box; consent gating is whatever the vendor allows.
4. **No HITL gates on destructive ops** — force-pushes, drops,
   `rm -rf` all happen silently or behind UI we don't control.
5. **No auditability** — token usage, tool calls, and rationale
   are vendor-shaped, not ours.

LAZARUS fixes all five by sitting on top of MNEME (memory),
cssl-edge / apocky.com (control plane + REST surface), and a local
runner process that does the actual work on this Windows box.

---

## 1. Architecture (≤ 1 screen)

```
┌──────────────────────────────────────────────────────────────────┐
│ apocky.com  (Vercel · cssl-edge · Pages Router)                  │
│                                                                   │
│  pages/lazarus/*   ← UI: dashboard, run viewer, task creator     │
│  pages/api/lazarus/*  ← REST + lease + events + approvals        │
│  pages/api/mneme/[profile]/*  ← already shipped                  │
│                                                                   │
│        │              │                    │                     │
│        ▼              ▼                    ▼                     │
│  Supabase (pzirbmyfmrbtkllrtcmx · same project as MNEME)         │
│   ├─ mneme_*           ← already shipped                         │
│   └─ lazarus_*         ← new in migration 0050                   │
└──────────────────────────────────────────────────────────────────┘
                              ▲
                              │  HTTPS · runner_token (Ed25519-signed)
                              │
┌─────────────────────────────┴────────────────────────────────────┐
│  Local runner  (Windows 11 · Node 22 · this box)                 │
│                                                                   │
│  lazarus-runner/                                                  │
│   ├─ src/loop.ts             leases · executes · streams events   │
│   ├─ src/tools/              fs · shell · git · test · web · …    │
│   ├─ src/llm/claude.ts       Anthropic Messages API · prompt-cache │
│   ├─ src/llm/local.ts        llama.cpp server adapter (phase 5)   │
│   ├─ src/mneme.ts            REST client → /api/mneme/<profile>/* │
│   ├─ src/sandbox.ts          per-task working dir + allow-lists   │
│   └─ src/runner.ts           heartbeat + capabilities + token mgmt │
└──────────────────────────────────────────────────────────────────┘
```

**Trust boundary**: the runner process is fully trusted (it's on
our box, holds the sovereign key path). Vercel routes are the
*coordination layer*, not the trust boundary — they store state,
broker tasks, and serve the UI. The sovereign Ed25519 PK in
`MNEME_SOVEREIGN_PUBKEY_HEX` already gates MNEME; we extend the
same gate to LAZARUS.

---

## 2. Naming, profiles, and IDs

- **Runner**: `runner_id` = `sha256(hostname || pubkey_hex)[:16]`.
  Friendly name set by user (e.g. `apocky-desktop`).
- **Repo**: `repo_id` = stable slug; e.g. `loa-v10`, `cssl-v3`,
  `infinite-labyrinth`, `apockyrom`.
- **MNEME profile per repo**: `lazarus-{repo_id}`. So one profile
  per code base. Cross-repo memories go in `lazarus-meta`.
- **Task**: `task_id` = uuid; **Run**: `run_id` = uuid.
- **Event**: append-only log row, ordered by `run_id, seq`.

Why one profile per repo: keeps memory blast-radius scoped, makes
"forget everything LAZARUS learned about LoA v10" a one-call
profile-wide forget, and naturally maps to sigma-mask audience
bits per repo.

---

## 3. Supabase schema (migration `0050_lazarus.sql`)

Six tables. All carry `sigma_mask bytea` and an audit-log
`lazarus_event` row on every state change.

```sql
-- 0050_lazarus.sql · sketch · authoritative form goes in cssl-supabase/migrations/

create table lazarus_runner (
    runner_id      text primary key,         -- sha256 prefix (16 hex)
    name           text not null,
    hostname       text not null,
    capabilities   jsonb not null default '{}'::jsonb,
    pubkey         bytea not null,           -- runner Ed25519 PK
    sovereign_sig  bytea not null,           -- sovereign signature over pubkey
    sigma_mask     bytea not null,
    last_heartbeat timestamptz,
    created_at     timestamptz not null default now()
);

create table lazarus_task (
    task_id        uuid primary key default gen_random_uuid(),
    repo_id        text not null,
    title          text not null,
    prompt         text not null,            -- the actual instruction
    context_csl    text,                     -- optional pre-baked context pack
    status         text not null,            -- queued | leased | running | awaiting_approval | completed | failed | canceled
    leased_by      text references lazarus_runner(runner_id),
    leased_at      timestamptz,
    cost_ceiling_usd numeric(10,4) not null default 5.00,
    created_by     text not null,            -- sovereign pubkey hex of submitter
    created_at     timestamptz not null default now(),
    sigma_mask     bytea not null
);
create index on lazarus_task (status, repo_id);

create table lazarus_run (
    run_id         uuid primary key default gen_random_uuid(),
    task_id        uuid not null references lazarus_task(task_id) on delete cascade,
    runner_id      text not null references lazarus_runner(runner_id),
    status         text not null,            -- running | completed | failed | canceled
    started_at     timestamptz not null default now(),
    ended_at       timestamptz,
    cost_usd       numeric(10,4) not null default 0,
    tokens_in      bigint not null default 0,
    tokens_out     bigint not null default 0,
    summary_csl    text,                     -- written at end · also ingested into MNEME
    sigma_mask     bytea not null
);
create index on lazarus_run (task_id);
create index on lazarus_run (runner_id, status);

create table lazarus_event (
    event_id       bigserial primary key,
    run_id         uuid not null references lazarus_run(run_id) on delete cascade,
    seq            int  not null,
    kind           text not null,            -- see §6
    payload        jsonb not null,
    ts             timestamptz not null default now(),
    unique (run_id, seq)
);
create index on lazarus_event (run_id, seq);
create index on lazarus_event (kind, ts);

create table lazarus_approval (
    approval_id    uuid primary key default gen_random_uuid(),
    run_id         uuid not null references lazarus_run(run_id) on delete cascade,
    requested_seq  int  not null,
    kind           text not null,            -- destructive_op | cost_ceiling | egress | escalation
    payload        jsonb not null,
    decision       text,                     -- granted | denied | timed_out
    decided_at     timestamptz,
    decided_by     text,                     -- sovereign pubkey hex
    expires_at     timestamptz not null,
    sigma_mask     bytea not null
);
create index on lazarus_approval (run_id, decision);

create table lazarus_artifact (
    artifact_id    uuid primary key default gen_random_uuid(),
    run_id         uuid not null references lazarus_run(run_id) on delete cascade,
    kind           text not null,            -- file | patch | commit | log
    path           text,
    content_hash   text,                     -- sha256 hex
    bytes          bigint,
    storage_url    text,                     -- supabase storage if large; inline if small
    inline_content text,
    created_at     timestamptz not null default now(),
    sigma_mask     bytea not null
);
create index on lazarus_artifact (run_id);

-- RLS: same pattern as mneme · service-role bypass · sovereign-PK predicate on
-- everything user-facing. Per-row sigma-mask revocation hides revoked rows.
```

Companion migration `0051_lazarus_rls.sql` mirrors `0041_mneme_rls.sql`
exactly, swapping table names.

`verify-lazarus.sql` does the smoke equivalent of `verify-mneme.sql`.

---

## 4. cssl-edge surface (new routes)

All under `pages/api/lazarus/*`. Same envelope convention as the rest
of cssl-edge: every response carries `served_by` + `ts`; errors use
`{ error, served_by, ts }`.

| Route                                | Method | Body / Query                                                              | Caller          |
| ---                                  | ---    | ---                                                                       | ---             |
| `/api/lazarus/health`                | GET    | —                                                                         | anyone          |
| `/api/lazarus/runners/register`      | POST   | `{ runner_id, name, hostname, pubkey_hex, capabilities, sovereign_sig }`  | runner (once)   |
| `/api/lazarus/runners/heartbeat`     | POST   | `{ runner_id, signed_nonce }`                                             | runner (15 s)   |
| `/api/lazarus/runners/lease`         | POST   | `{ runner_id, signed_nonce, repo_filter? }` → task or `null`              | runner          |
| `/api/lazarus/tasks`                 | POST   | `{ repo_id, title, prompt, context_csl?, cost_ceiling_usd? }`             | sovereign UI    |
| `/api/lazarus/tasks`                 | GET    | `?repo_id=&status=&limit=&cursor=`                                        | sovereign UI    |
| `/api/lazarus/tasks/:id`             | GET    | —                                                                         | sovereign UI    |
| `/api/lazarus/tasks/:id/cancel`      | POST   | —                                                                         | sovereign UI    |
| `/api/lazarus/runs/:id`              | GET    | —                                                                         | sovereign UI    |
| `/api/lazarus/runs/:id/events`       | GET    | `?since_seq=` long-poll                                                   | sovereign UI    |
| `/api/lazarus/runs/:id/event`        | POST   | `{ seq, kind, payload }`                                                  | runner          |
| `/api/lazarus/runs/:id/finalize`     | POST   | `{ status, summary_csl, cost_usd, tokens_in, tokens_out }`                | runner          |
| `/api/lazarus/approvals/:id/decide`  | POST   | `{ decision, signed_nonce }`                                              | sovereign UI    |
| `/api/lazarus/approvals/pending`     | GET    | `?runner_id=`                                                             | runner & UI     |

**Streaming**: events flow runner → cssl-edge → Supabase
(`lazarus_event`). UI subscribes via Supabase Realtime on the
`lazarus_event` table, scoped to the active `run_id`. This sidesteps
Vercel function-timeout limits on SSE; the API only writes, the
client streams direct from Supabase.

**Auth between runner and cssl-edge**: the runner generates an
Ed25519 keypair on first start. Its PK is signed by the sovereign
PK (one-time bootstrap, signed offline using existing sigma-chain
tooling — see specs/28). Every authenticated runner request carries
a recent signed nonce; the server verifies via stored `pubkey` +
`sovereign_sig`.

---

## 5. UI surface (new pages)

Under `pages/lazarus/*`. Auth-gated to sovereign (mirror existing
admin gating in cssl-edge).

- `pages/lazarus/index.tsx` — dashboard
  - active runs · queue depth · today's $ · pending approvals
  - per-repo breakdown
- `pages/lazarus/new.tsx` — task creator
  - repo picker · title · prompt (CSL or NL) · cost ceiling · pre-flight MNEME recall preview
- `pages/lazarus/runs/[id].tsx` — live run viewer
  - live event stream (Supabase Realtime subscription)
  - tool-call timeline · diffs · cost meter · token meter
  - approve / deny / cancel buttons · escalation banner
- `pages/lazarus/runners.tsx` — runner registry
  - last heartbeat · capabilities · current run · revoke key

Components live in `cssl-edge/components/lazarus/*` mirroring
the `mneme/*` and `mycelium/*` component conventions.

---

## 6. Event taxonomy (`lazarus_event.kind`)

Every kind has a fixed `payload` JSON shape. Logged to the same
table for one ordered timeline per run.

| kind                  | payload shape (key fields)                                                   |
| ---                   | ---                                                                          |
| `run_started`         | `{ task_id, runner_id, repo_id }`                                            |
| `mneme_recall`        | `{ query, k, citation_ids, latency_ms, profile }`                            |
| `mneme_remember`      | `{ memory_id, type, topic_key, profile }`                                    |
| `llm_call`            | `{ model, sys_prompt_hash, input_tokens, n_messages }`                       |
| `llm_response`        | `{ model, output_tokens, input_tokens, stop_reason, cost_usd }`              |
| `tool_call`           | `{ name, args, why? }`                                                       |
| `tool_result`         | `{ name, ok, bytes_returned, summary, truncated? }`                          |
| `stdout` / `stderr`   | `{ tool_call_id, chunk }`                                                    |
| `approval_requested`  | `{ approval_id, kind, why }`                                                 |
| `approval_granted`    | `{ approval_id, by }`                                                        |
| `approval_denied`     | `{ approval_id, by, reason }`                                                |
| `escalation`          | `{ reason, summary, suggested_next }`                                        |
| `error`               | `{ where, message, stack_hash }`                                             |
| `run_completed`       | `{ summary_csl, cost_usd, tokens_in, tokens_out }`                           |
| `run_failed`          | `{ where, message }`                                                         |
| `run_canceled`        | `{ by, reason }`                                                             |

Adding a kind requires bumping the doc, not the schema (payload is
jsonb).

---

## 7. Tool surface (the agent's hands)

Tools run inside the runner process, not on Vercel. The runner
exposes them to Claude (or local LLM) via the standard Messages
API tool spec.

```ts
// lazarus-runner/src/tools/index.ts
export const TOOLS = {
    fs_read:        { /* path → content (UTF-8 or base64) */ },
    fs_write:       { /* path, content → bytes_written */ },
    fs_edit:        { /* path, old, new → diff */ },           // exact-match
    glob:           { /* pattern → paths[] */ },
    grep:           { /* pattern, path → matches[] */ },
    shell:          { /* cmd, timeout_ms → {stdout, stderr, code} */ },
    git:            { /* args[] → result */ },                 // status/diff/log/branch/commit
    test:           { /* cmd → result */ },                    // shell-but-tagged
    mneme_recall:   { /* query, types?, k? → MnemeRecall */ }, // → /api/mneme/<profile>/recall
    mneme_remember: { /* csl, type, topic_key? → memory_id */ },
    web_search:     { /* query → results[] */ },
    web_fetch:      { /* url → text */ },
    request_approval: { /* kind, payload, why → granted? */ }, // blocks until decided
    escalate:       { /* reason, summary → ack */ },           // ends run
};
```

**Workspace scoping**: every task pins a `workspace_root`
(default: `<repo_path>` for the task's `repo_id`). All `fs_*` and
`shell` calls are forced to resolve under `workspace_root`; an
attempted escape triggers `request_approval`.

**Allow-lists**:

- `shell` cmd allow-list (default): `npm`, `pnpm`, `cargo`, `git`,
  `python`, `rg`, `node`, `npx`, `tsc`, `vitest`, `pytest`,
  `dotnet`, `odin`. Anything else triggers approval.
- `web_fetch` domain allow-list: `npmjs.com`, `crates.io`,
  `pypi.org`, `github.com`, `docs.*`, `developer.mozilla.org`,
  `nextjs.org`, `vercel.com`, `supabase.com`, `anthropic.com`.
  Anything else → approval.
- `git push`, `git reset --hard`, `git clean -fdx` → always
  approval, no allow-list bypass.

---

## 8. Destructive-op gates (HITL)

Hard rules — bake these into the tool layer, not the prompt:

1. `git push` → approval, always.
2. `git reset --hard` / `git clean -fdx` / `git branch -D` → approval.
3. `rm -rf` outside workspace → blocked outright.
4. `rm` of more than 50 files at once → approval.
5. Network egress to non-allow-listed domains → approval.
6. Cumulative run cost ≥ 80 % of `cost_ceiling_usd` → approval.
7. `mneme_remember` of type=`instruction` → approval (these are
   standing rules; sovereign blesses them explicitly).
8. Any `shell` command containing `sudo`, `mkfs`, `fdisk`,
   `format`, `> /dev/`, `Remove-Item -Recurse -Force /` → blocked.

Approval timeout default = 10 min; on timeout the run pauses
indefinitely (status `awaiting_approval`) rather than failing,
so the sovereign can come back later.

---

## 9. The agent loop (canonical pseudocode · CSL-flavored)

```
loop forever:
    task ← POST /api/lazarus/runners/lease (runner_id, signed_nonce)
    if task = ∅:
        sleep 5s; continue

    run_id ← create_run(task.task_id, runner_id)
    emit run_started

    workspace ← sandbox.open(repo_root[task.repo_id])
    profile   ← "lazarus-" + task.repo_id

    context ← mneme.recall(query=task.prompt, profile=profile, k=20)
    emit mneme_recall { query=task.prompt, k=20, citation_ids=context.citations }

    history ← []
    cost    ← 0.0
    seq     ← 1

    loop until terminal:
        sys ← system_prompt(task, context, workspace_state)
        msg ← history ++ [pending_tool_results]

        emit llm_call
        rsp ← claude.messages(model=opus_4_7, sys, msg, tools=TOOLS, cache=ephemeral)
        cost += rsp.cost
        emit llm_response { stop_reason=rsp.stop_reason, ..rsp.usage }

        if cost ≥ 0.8 × task.cost_ceiling_usd ∧ ¬approved_overage:
            request_approval(kind=cost_ceiling, payload={cost, ceiling}, why="80% spent")

        for tu in rsp.tool_uses:
            if classify(tu) ∈ DESTRUCTIVE_OPS:
                granted ← request_approval(kind=destructive_op, payload=tu, why=tu.why)
                if ¬granted: emit run_failed; break
            result ← tools[tu.name](tu.args)
            emit tool_call ; emit tool_result

        history += [rsp, results]

        if rsp.stop_reason = "end_turn" ∧ rsp.tool_uses = []:
            break

    summary_csl ← synthesize_summary(history)            // local · cheap call
    mneme.remember(profile=profile, type="event",
                   csl=summary_csl, topic_key=task.repo_id+"/"+task.title)
    emit mneme_remember
    POST /api/lazarus/runs/<run_id>/finalize { status="completed", summary_csl, cost, tokens }
    emit run_completed
```

---

## 10. MNEME integration specifics

**On task start** — single `recall` against the per-repo profile:

```ts
const context = await mneme.recall({
    profile: `lazarus-${task.repo_id}`,
    query:   task.prompt,
    k:       20,
    types:   ['instruction', 'fact', 'event'],
});
```

The runner prepends context to the system prompt as a CSL block
labeled `// MNEME RECALL`. `instruction`-type memories get
top billing; `fact` next; `event` for prior decisions.

**During run** — agent can call `mneme_recall` ad-hoc when it
needs more (e.g. encounters an unfamiliar module).

**On task end** — single `remember` writing the run summary:

```ts
await mneme.remember({
    profile:   `lazarus-${task.repo_id}`,
    type:      'event',
    csl:       summary_csl,
    topic_key: `${task.repo_id}/${task.title}`,
});
```

Plus zero-or-more `instruction`-type writes if the agent learned
a new convention (these gate through approval per §8 rule 7).

**MNEME profile bootstrap** — first run against a new `repo_id`
creates the profile via existing MNEME flow. We seed each new
profile with one `instruction` memory:

```
:instruction
:topic_key   "lazarus/<repo_id>/standing"
:csl         "respect PRIME_DIRECTIVE.md · respect specs/27 sigma · ↓
              workspace_root=<repo_path> · escalate on uncertainty ↓
              prefer pnpm over npm · CSLv3 native reasoning · ↓
              never push to main without approval"
```

---

## 11. Local LLM option (phase 5, deferred but designed in)

`src/llm/local.ts` mirrors `src/llm/claude.ts`'s shape so the loop
doesn't care which is wired. Initial target: `llama.cpp` server
with an Intel Arc A770 backend (Vulkan or IPEX-LLM). Tool-use
support requires a model with reliable function-calling — at
present that means Llama 3.3 70B Instruct or Qwen 2.5 Coder 32B,
both of which fit on 16 GB VRAM at Q4_K_M.

**Routing rule** (per task, set at creation time):
- `quality` → Claude Opus 4.7 via API
- `cost`    → Claude Haiku 4.5 via API
- `local`   → llama.cpp server on this box
- `auto`    → Opus for new repos, Haiku for repeat patterns
   (gated on MNEME recall hitting ≥ 3 prior `event` memories
   tagged with the same `topic_key` prefix)

The decoupled-inference idea in the doc you pasted is interesting
but a research-grade detour for this milestone. Note it and
ignore it for v1.

---

## 12. Repo layout

New repo: `C:\Users\Apocky\source\repos\lazarus-runner`. Sibling
to `LoA v10` etc., not nested under CSSLv3 because it's a separate
deployable artifact (a long-running local process), and we don't
want CSSLv3's TS config / lockfile to bleed into it.

```
lazarus-runner/
  package.json          # type:module · "lazarus-runner": "node ./dist/loop.js"
  tsconfig.json
  .env.example
  README.md
  src/
    loop.ts             # main entrypoint
    runner.ts           # registration, heartbeat, lease
    sandbox.ts          # workspace_root + path-resolution guards
    tools/
      index.ts
      fs.ts
      shell.ts
      git.ts
      web.ts
      mneme.ts          # client-side MNEME wrapper
      approval.ts
    llm/
      claude.ts
      local.ts
      types.ts
    summarize.ts        # tail-of-run synthesizer
    sigchain.ts         # ed25519 + sovereign-sig glue
    config.ts
  tests/
    sandbox.test.ts
    tools.test.ts
    loop.smoke.test.ts  # stub LLM, stub MNEME, end-to-end
```

cssl-edge additions:

```
cssl-edge/
  pages/api/lazarus/...               # all routes from §4
  pages/lazarus/...                   # all UI from §5
  components/lazarus/...
  lib/lazarus/
    types.ts                          # mirror of runner types where they overlap
    auth.ts                           # nonce + sig verify
    store.ts                          # supabase repo
    realtime.ts                       # supabase realtime helpers for UI
  tests/api/lazarus/
    lease.test.ts
    finalize.test.ts
    approval.test.ts
```

cssl-supabase additions:

```
cssl-supabase/
  migrations/
    0050_lazarus.sql
    0051_lazarus_rls.sql
  verify-lazarus.sql
```

---

## 13. Phasing & acceptance criteria

### Phase 0 — scaffold (one evening)

- [ ] `0050_lazarus.sql` + `0051_lazarus_rls.sql` apply cleanly to
      `pzirbmyfmrbtkllrtcmx`.
- [ ] `verify-lazarus.sql` passes.
- [ ] Stub routes return 200 with envelope: `health`,
      `runners/register`, `runners/lease` (always returns null).
- [ ] `lazarus-runner` repo scaffolded; `npm run check` passes;
      runner can register, heartbeat, and lease (gets null) without
      crashing.

### Phase 1 — single-task loop, no MNEME, no UI

- [ ] Sovereign can `curl POST /api/lazarus/tasks` to enqueue.
- [ ] Runner leases, calls Claude with `fs_*`, `shell`, `git`
      tools, executes a "make a TODO list of changes to
      `<repo>/README.md`" task end-to-end.
- [ ] All `llm_call`, `llm_response`, `tool_call`, `tool_result`,
      `run_started`, `run_completed` events land in
      `lazarus_event`.
- [ ] `lazarus_run.cost_usd` is non-zero and matches the sum of
      `llm_response.cost_usd` payloads.
- [ ] Idempotency: re-leasing while `status='leased'` returns the
      same run; re-finalizing is a no-op.

### Phase 2 — MNEME wiring

- [ ] Profile auto-creation on first task per repo.
- [ ] `mneme_recall` event present at run start with k≥1 (after
      first run) or k=0 (cold).
- [ ] `mneme_remember` event present at run end.
- [ ] Second run on same repo references first run's memory in
      its prompt (verifiable in `lazarus_event.payload`).

### Phase 3 — approvals + UI

- [ ] `pages/lazarus/runs/[id].tsx` shows live event stream via
      Supabase Realtime.
- [ ] Triggering `git push` raises `approval_requested`; UI shows
      modal; granting unblocks the run; denying fails it.
- [ ] Cost-ceiling 80 % gate works.
- [ ] Approval timeout pauses run rather than failing.

### Phase 4 — multi-repo / multi-runner hardening

- [ ] Capability flags filter `lease` so a runner without `cargo`
      doesn't get a Rust task.
- [ ] Two runners can both heartbeat; tasks are dealt out without
      collision (unique-leased index covers it).
- [ ] Runner key revocation works (sovereign signs a revocation,
      runner's heartbeat starts 401-ing).

### Phase 5 — local LLM backend

- [ ] llama.cpp server adapter passes the same `loop.smoke.test.ts`
      that Claude does.
- [ ] `auto` routing works against the documented heuristic.

---

## 14. What I'm explicitly *not* doing in v1

- **Strong sandboxing.** The runner runs as our user with our
  privileges. We're trading isolation for trust (it's our box,
  our code, our LLM) and tool-level allow-lists. If we ever want
  multi-tenant, this changes.
- **Multi-step planning ahead of the LLM.** No external planner /
  graph orchestrator. The model is the planner; we shape the
  loop and the tools. If runs grow beyond ~50 turns we revisit.
- **Replay / time-travel.** Events are append-only and replayable
  in principle, but no UI for it in v1.
- **Cross-runner coordination.** A task is leased by exactly one
  runner. No swarm, no quorum.
- **Anything Cloudflare-shaped.** We're on Vercel + Supabase by
  choice; do not port back to DOs.

---

## 15. Open questions for sovereign

1. **Naming**: keep LAZARUS or pick something else? It pairs
   nicely with MNEME (Memory + Lazarus = "called back from
   nothing"), but call it.
2. **Where does the runner live in the repo tree?** Proposed
   `repos/lazarus-runner` (sibling). Alternative: under
   `CSSLv3/cssl-runner/` to share tooling.
3. **Default model for v1**: Opus 4.7 for everything, or Haiku
   default with Opus opt-in?
4. **MNEME profile granularity**: per-repo (proposed) or per-
   worktree? Per-worktree gives sharper recall on `LoA-wt-*`
   branches but multiplies profile count by ~30.
5. **Approval channel**: in-UI only, or also fan-out to
   Discord / pushover / sms for off-keyboard approvals?
6. **Cost cap**: per-task only (proposed) or also daily/monthly
   global? I'd add the global cap in phase 1, not later.
7. **Worktree-aware leasing**: should `lease` be allowed to pin
   to a specific worktree path, so two runs in the same repo on
   different worktrees don't collide? Probably yes; cheap to add.

---

## 16. First three Claude-Code prompts (suggested handoff)

When ready to execute, feed these to Claude Code in order. Each
one ends in a checkpoint that lets you sanity-check before the
next.

**Prompt 1 — schema + smoke**

> Read `specs/60_LAZARUS_AGENT.md`. Implement §3 (the migrations)
> in `cssl-supabase/migrations/0050_lazarus.sql` and
> `0051_lazarus_rls.sql`. Mirror the style of `0040_mneme.sql` and
> `0041_mneme_rls.sql` exactly — same comment headers, same
> trigger and index conventions, same RLS policy shape. Add
> `cssl-supabase/verify-lazarus.sql` mirroring `verify-mneme.sql`.
> Apply via `supabase db push` against `pzirbmyfmrbtkllrtcmx` and
> run verify. Report results.

**Prompt 2 — REST surface**

> Read `specs/60_LAZARUS_AGENT.md` §4 + §6. Implement every route
> under `cssl-edge/pages/api/lazarus/*` per the table. Mirror
> `pages/api/mneme/[profile]/*.ts` for envelope and error
> handling. Implement `lib/lazarus/{types,auth,store,realtime}.ts`.
> Wire `tests/api/lazarus/{lease,finalize,approval}.test.ts`.
> `npm run check` and `npm run test` must pass before you finish.
> Do NOT touch anything outside `pages/api/lazarus`,
> `pages/lazarus`, `components/lazarus`, `lib/lazarus`,
> `tests/api/lazarus`. Stop and ask before deploying.

**Prompt 3 — runner skeleton**

> Read `specs/60_LAZARUS_AGENT.md` §7 + §9 + §12. Scaffold
> `repos/lazarus-runner/` per §12. Implement Phase-1 acceptance
> criteria (§13). The tools that exist in v1: `fs_read`,
> `fs_write`, `fs_edit`, `glob`, `grep`, `shell`, `git`, `test`,
> `mneme_recall`, `mneme_remember`, `request_approval`,
> `escalate`. Stub `web_search` and `web_fetch` with TODO. The
> sandbox layer is hard-required (every fs/shell call must
> resolve through `sandbox.ts`). All `tests/*.test.ts` must pass.
> When the smoke test passes, stop and report.

---

*end · 60_LAZARUS_AGENT.md*

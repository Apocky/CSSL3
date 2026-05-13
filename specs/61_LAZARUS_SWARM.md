# 61 — LAZARUS SWARM (delta to spec 60)

**Status**: design v1 · 2026-05-07
**Owner**: Apocky / sovereign
**Depends**: 60 (LAZARUS Agent), 27 (Sigma), 43-45 (MNEME)
**Replaces**: nothing — extends 60 with a model-fleet + peer-review layer

---

## 0. What's wrong with spec 60 alone

Spec 60 hard-wired Claude Opus / Haiku as the brains. Sovereign
pushed back: that's still vendor-lockin, still pay-per-token, and
ignores the substrate available in May 2026 — DeepSeek V4-Pro
shipped April 24, ktransformers added V4-Flash heterogeneous
serving May 2, vLLM disaggregated-prefill is in production at
Meta-scale, peer-review multi-agent patterns (MoA, generator-
critic-verifier) are research-grounded and real.

So this spec layers a **model fleet + peer-review swarm + smart
router** onto the LAZARUS bones. Same MNEME, same Supabase, same
Vercel control plane. Different muscle.

I'm also going to be candid in §1 about what the original article
the sovereign pushed back on actually says — because the substrate
is real even if its specifics were slop.

---

## 1. Reading the article honestly (mea culpa + caveats)

The piece I dismissed conflated three things:

1. **Real and shipping** — disaggregated prefill/decode (vLLM,
   NVIDIA Dynamo at GTC 2025); attention-FFN disaggregation
   (MegaScale-Infer, ByteDance 2025); MoE expert offloading
   (ktransformers, since 2024); home distributed inference
   (exo → prima.cpp).
2. **Real research, not yet plug-and-play** — running attention
   on a laptop while FFN experts live on remote HTTP-served
   shards. The numbers in the transcript (25 tok/s LAN, 1.8 tok/s
   internet) are *plausible* — KV-cache transfer over WAN is
   genuinely the bottleneck.
3. **Probably fabricated** — "Xenon AI OS", "Gemma 4 26B MoE"
   (Google's released line is Gemma 3; Gemma 4 is not announced
   as of this writing). The `【docID†Lstart-Lend】` markers are
   Gemini's video-transcript citation format, not paper cites.

So my v1 was right to be skeptical of the article and wrong to
be skeptical of the substrate. The substrate informs §3 below.

---

## 2. The fleet (May 2026 facts)

| Tier | Model                  | Where                                | Cost                          | Use                                          |
| ---  | ---                    | ---                                  | ---                           | ---                                          |
| L0   | DeepSeek V4-Flash      | local · ktransformers + SGLang       | electricity (~$0.01/task)     | bulk codegen, lints, refactors, small edits  |
| L1   | DeepSeek V4-Pro API    | api.deepseek.com                     | ~$0.27/M in · ~$1.10/M out    | architect-level reasoning, hard debugging    |
| L2   | DeepSeek V4-Pro-Max    | self-host · rented H200/B200 (burst) | ~$2-5/hr while spun up        | long-context (>256K) refactors, the hardest  |
| L3   | Claude Opus 4.7 API    | api.anthropic.com                    | $15/M in · $75/M out          | adversarial peer review · safety review      |
| L4   | Claude Haiku 4.5 API   | api.anthropic.com                    | $1/M in · $5/M out            | tie-breaker · quick verifier                 |
| L5   | Local Qwen2.5-Coder    | local llama.cpp · 32B Q4             | electricity                   | offline / connectivity-loss fallback         |

**Why all of them?** Because architectural diversity catches errors
self-similar ensembles miss. DeepSeek and Claude were trained on
overlapping but different corpora with different RLHF objectives.
A bug Claude rationalizes, DeepSeek often flags, and vice versa.
This is a real measured effect (see Mixture-of-Agents, Wang et al
2024) — not vibes.

**What's actually buildable at home for L0:**

- **V4-Flash on a single workstation**: RTX 5090 32GB VRAM +
  256GB DDR5 + fast NVMe. ktransformers v0.6.1 + SGLang with
  MXFP4 routed experts on CPU, attention + routing on GPU.
  Realistic decode: 10-25 tok/s for single-stream, much higher
  for batched.
- **V4-Pro at home is not realistic** — 1.6T params is multi-GPU
  server territory. Don't try. Use the API or burst-rent H200/B200.

**For L2, the burst pattern**:

- RunPod / Vast.ai / Shadeform H100 on-demand: $1.49-2.69/hr
  in May 2026.
- Spin up only for tasks the router classifies as "L2-required"
  (long-context refactor, multi-file architectural reasoning).
- Self-host V4-Pro-Max via SGLang on the rented box for 1-4
  hours, tear down. Cost: $5-20 per heavy session.
- Alternatively, just use the DeepSeek V4-Pro API. It's so cheap
  that L2-self-host is only worth it for privacy or for
  custom-quantized variants.

---

## 3. The peer-review topology

This is where multi-agent earns its keep. The pattern is
generator → critics → synthesizer, with model-class diversity
enforced at each step.

```
                    ┌──────────────┐
                    │  ARCHITECT   │   (L1 V4-Pro · think-mid)
                    │  plan + carve│
                    └──────┬───────┘
                           │ subtasks[1..N]
        ┌──────────────────┼──────────────────┐
        ▼                  ▼                  ▼
   ┌────────┐         ┌────────┐         ┌────────┐
   │CODER A │         │CODER B │   ···   │CODER N │   (parallel · L0 V4-Flash)
   │(local) │         │(local) │         │(local) │
   └────┬───┘         └────┬───┘         └────┬───┘
        │ patch_a          │ patch_b          │ patch_n
        └──────────────────┼──────────────────┘
                           ▼
                    ┌──────────────┐
                    │  REVIEWER    │   (L3 Claude Opus · diff-vendor critic)
                    │  per-patch   │
                    └──────┬───────┘
                           │ verdicts + suggested edits
                           ▼
                    ┌──────────────┐
                    │  VERIFIER    │   (L0 V4-Flash + actual test runner)
                    │  build/test  │
                    └──────┬───────┘
                           │ green / red + logs
                           ▼
                    ┌──────────────┐
                    │ SYNTHESIZER  │   (L1 V4-Pro · think-max)
                    │ merge + ship │
                    └──────────────┘
```

**Key invariants:**

1. **Coder ≠ Reviewer at the model-vendor level.** If V4-Flash
   wrote it, Claude reviews it (or vice versa). Same vendor
   reviewing same vendor is theatre.
2. **Verifier touches reality.** Tests run, builds run, types
   check. No "looks-good-to-me" approvals from any model.
3. **Architect plans once, refuses to plan again unless
   verifier failed twice.** Plan-thrash is the failure mode of
   naive multi-agent loops.
4. **Synthesizer can override reviewer**, but must justify the
   override in writing (logged to MNEME as `instruction` if it
   becomes a standing rule).
5. **Disagreement → tie-breaker, not deadlock.** When reviewer
   rejects and coder defends, L4 Haiku gets one shot at
   resolution; if still split, escalate to sovereign.

---

## 4. Routing — when does each tier fire?

Routing happens at three levels: per-task (architect classifies),
per-subtask (architect distributes), and per-call (router can
upgrade mid-loop). All three consult MNEME first.

### 4a. Task classification (architect's first call)

The Architect's first action on a fresh task is a structured
classification call that returns:

```
type RouteDecision = {
    difficulty:      'trivial' | 'standard' | 'hard' | 'frontier',
    novelty:         'pattern_match' | 'variant' | 'novel',
    safety_class:    'normal' | 'destructive' | 'prime_directive',
    needs_long_ctx:  boolean,                 // > 128K tokens of context
    n_subtasks:      number,
    review_required: boolean,
    suggested_team:  TeamRecipe,
};
```

This call always goes to L1 V4-Pro (cheap, reliable, structured).
Then a deterministic table maps `RouteDecision` to the team:

| difficulty | novelty       | needs_long_ctx | team                                           |
| ---        | ---           | ---            | ---                                            |
| trivial    | pattern_match | no             | L0 solo · skip review                          |
| trivial    | *             | yes            | L0 coder · L0 reviewer (same model is OK here) |
| standard   | pattern_match | no             | L0 coder · L4 Haiku reviewer                   |
| standard   | variant/novel | no             | L0 coder · L3 Opus reviewer                    |
| hard       | *             | no             | L1 architect · 2-3× L0 coder · L3 reviewer · L0 verifier · L1 synthesizer |
| hard       | *             | yes            | bump architect to L2 (if rented box up) or L1  |
| frontier   | *             | *              | L1 architect · 2× L0 + 1× L1 coder (diverse) · L3 + L1 reviewers · sovereign-in-the-loop |
| any        | *             | *              | safety_class=`prime_directive` → forces L3 review regardless of difficulty |

### 4b. MNEME-as-cache (the "skip if you can" rule)

Before *anything* runs, the runner does:

```ts
const hits = await mneme.recall({
    profile:   `lazarus-${task.repo_id}`,
    query:     task.prompt,
    types:     ['event', 'instruction'],
    k:         10,
});

const exact = hits.find(h => h.confidence > 0.92
                             && h.topic_key === stable_key(task));
if (exact && task.allow_replay) {
    return replay_plan_from_memory(exact);  // skip Architect
}
```

If MNEME says "we did this before, here's the plan and the
patches", we replay the plan and only invoke Coder/Reviewer/
Verifier. Architect is bypassed entirely. This is where the
persistent memory actually pays for itself.

### 4c. Mid-loop upgrade

The router watches for upgrade triggers during a run:

- Coder produced 3 failed builds in a row → upgrade Coder to L1
  for that subtask.
- Reviewer flagged "this looks dangerous, I cannot evaluate
  safely" → upgrade Reviewer to L3 + alert sovereign.
- Verifier sees compiler error from a domain Architect didn't
  plan for → kick back to Architect (counts toward the plan-
  thrash limit; 2 strikes = escalate to sovereign).

---

## 5. The disaggregated-prefill option (real talk)

The "decoupled inference" idea from the article can be done
right now in production form. It's not laptop-attention +
internet-FFN — that's too lossy over WAN — but a defensible
version is:

**Prefill on rented GPU, decode locally.**

```
┌───────────────────────────────┐         ┌─────────────────────────┐
│  Local box (workstation)      │         │  Rented H100 (RunPod)   │
│  RTX 5090 + 256GB RAM         │  KV     │  vLLM disaggregated     │
│  V4-Flash decode-instance     │ ◄────── │  V4-Pro prefill-only    │
│  10-25 tok/s sustained        │  cache  │  fast prefill on big    │
│                               │  via    │  context (1M tokens)    │
└───────────────────────────────┘  NIXL   └─────────────────────────┘
```

**When this wins**: long-context tasks (full-repo refactors,
1M-token reasoning). The prefill is the expensive part; if you
ship the KV cache once and decode locally for 10K+ tokens, you
amortize the cloud burn.

**When this loses**: short prompts. KV transfer overhead eats
the savings.

**Honest assessment**: this is a Phase 5+ experiment, not a
v1 feature. v1 just routes to V4-Pro API for long-context. We
revisit when the rented-GPU pattern proves itself for L2.

References that informed this: vLLM disaggregated-prefill
docs, NVIDIA Dynamo (GTC 2025), LMSYS Day-0 V4 post (April 25,
2026, ShadowRadix prefix cache), MegaScale-Infer paper.

---

## 6. Hardware reality check

What you need to make L0 (local V4-Flash) actually fly:

| Component | Minimum               | Recommended                       | Why                                 |
| ---       | ---                   | ---                               | ---                                 |
| GPU       | RTX 4090 24GB         | RTX 5090 32GB (or 2× 4090)        | attention + router on GPU           |
| RAM       | 192GB DDR5            | 256-384GB DDR5                    | FP4 routed experts live here        |
| CPU       | 16-core / 32-thread   | 32-core (Threadripper / Epyc)     | CPU does FP4 expert math via AMX    |
| Storage   | 2TB NVMe Gen4         | 4TB NVMe Gen5                     | mmap'd weight loading                |
| Network   | 1 Gbps                | 10 Gbps + low-latency to AWS/RunPod | for L2 burst-rent + KV transfer    |

If you're already on a Threadripper-class workstation with the
existing GPU work for LoA v12 — you may need to add RAM. The
gating factor is not GPU; it's RAM capacity for the FP4
experts. 256GB gets you V4-Flash comfortably; 512GB opens
V4-Pro-quantized as a stretch goal.

**If you don't have this and don't want to buy it**: skip L0
entirely. Run L1/L3/L4 only. The cost math still works because
DeepSeek V4-Flash via API is ~$0.10/M input, $0.30/M output — so
"local for everything" is mostly a privacy + latency play, not
a cost play. The article's framing of "pro-tier AI at home" is
real for V4-Flash, illusory for V4-Pro.

---

## 7. What changes in spec 60

Concrete deltas:

### 7a. Schema (`0050_lazarus.sql`)

Add columns to `lazarus_task`:

```sql
alter table lazarus_task
    add column route_decision jsonb,                 -- §4a output
    add column team_recipe    jsonb,                 -- which agents fire
    add column allow_replay   boolean default true;  -- skip Architect on MNEME hit
```

New table for sub-runs (one per agent invocation in the team):

```sql
create table lazarus_subrun (
    subrun_id    uuid primary key default gen_random_uuid(),
    run_id       uuid not null references lazarus_run(run_id) on delete cascade,
    role         text not null,         -- architect | coder | reviewer | verifier | synthesizer
    model_tier   text not null,         -- L0 | L1 | L2 | L3 | L4 | L5
    model_id     text not null,         -- e.g. "deepseek-v4-flash" or "claude-opus-4-7"
    parent_subrun uuid references lazarus_subrun(subrun_id),
    started_at   timestamptz not null default now(),
    ended_at     timestamptz,
    status       text not null,
    cost_usd     numeric(10,4) not null default 0,
    tokens_in    bigint not null default 0,
    tokens_out   bigint not null default 0,
    output       jsonb,                  -- role-specific
    sigma_mask   bytea not null
);
create index on lazarus_subrun (run_id, role);
```

Events get a `subrun_id` foreign key so the timeline groups
correctly in the UI.

### 7b. Runner internals

Replace the single agent loop in spec 60 §9 with the topology
in §3 above. Each role becomes a function that drives one or
more sub-runs:

```
lazarus-runner/src/agents/
    architect.ts       # planner + classifier
    coder.ts           # parallel coders
    reviewer.ts        # cross-vendor reviewer
    verifier.ts        # build + test driver
    synthesizer.ts     # merger + writer-of-record
    router.ts          # team recipe → execution graph
    fleet/
        v4_flash_local.ts    # ktransformers/SGLang client
        v4_api.ts            # DeepSeek API client
        claude_api.ts        # Anthropic API client
        v4_self_host.ts      # rented-box client (Phase 5)
```

The `loop.ts` orchestrator becomes a DAG executor over the
team recipe. Coders run in parallel; Reviewer and Verifier in
parallel after Coders; Synthesizer last.

### 7c. Cloud burst (Phase 5)

New cssl-edge route `/api/lazarus/burst/lease` that spins up a
RunPod H100 instance via the RunPod API, returns its IP +
auth, and registers it as a transient L2 endpoint. Tear-down
on task completion or 1-hour idle. This is gated by approval
(§8 of spec 60) — bursting GPU capacity costs real money.

### 7d. Routing config (live, not baked)

`lib/lazarus/routing.ts` exposes a config that the sovereign
can edit through the admin UI:

```ts
export const FleetConfig = {
    enabled_tiers: ['L0', 'L1', 'L3', 'L4'],   // L2/L5 opt-in
    coder_default: 'L0',
    reviewer_default: 'L3',
    daily_budget_usd: 25.00,
    monthly_budget_usd: 500.00,
    upgrade_on_failure: true,
    diversity_required: true,                   // coder/reviewer must differ
    safety_overrides: {
        prime_directive: ['L3', 'L1'],          // Opus + V4-Pro both review
    },
};
```

---

## 8. Cost model (worked example)

Take a typical "add an endpoint to cssl-edge" task:

- Architect (L1, 800 in / 600 out tokens) = $0.0009
- 2× Coder (L0, 4K in / 2K out each) = ~$0.00 (electricity)
- Reviewer (L3 Opus, 6K in / 800 out) = $0.15
- Verifier (L0, run tests) = ~$0.00
- Synthesizer (L1, 8K in / 1K out) = $0.0033
- **Total: ~$0.16 per medium task**

Compare:
- All-Opus baseline (current sub-par worker): ~$1.50-3.00 per
  similar task.
- All-V4-Pro: ~$0.05 per task (cheap! but loses cross-vendor
  review → bug-catch rate drops measurably).

The peer-review premium (~$0.16 vs $0.05) buys cross-vendor
diversity. The baseline savings vs all-Opus is ~10x. A monthly
budget of $25 should cover ~150 medium tasks.

For frontier tasks (full-repo refactor):
- Architect L1/L2: $0.50-2.00
- 4× Coder mix (3× L0 + 1× L1): $0.20-1.00
- 2× Reviewer (L3 + L1): $1.50-3.00
- Synthesizer L1: $0.50-1.00
- **Total: $3-7 per frontier task**

vs all-Opus frontier: $20-50.

---

## 9. Bleeding-edge tricks (each one is real, with citations)

1. **Anthropic prompt caching** (`cache_control: ephemeral`) for
   the system prelude. ≥90% cache reuse in steady state — already
   used by MNEME, extend to LAZARUS. Source: Anthropic docs.

2. **SGLang ShadowRadix prefix cache** for V4-Flash. When
   Coder/Reviewer/Verifier all see the same task description,
   the radix tree gives near-instant prefill on shared prefixes.
   Source: LMSYS blog, April 25, 2026.

3. **MTP speculative decoding** — V4 ships with a speculative
   draft head. SGLang has Day-0 in-graph metadata. ~2x decode
   speedup for free. Source: DeepSeek-V4 paper + LMSYS.

4. **MNEME-as-plan-cache** — replay prior plans on topic_key
   match. This is the biggest single saving for repeat work.
   Already designed; just lean on it harder.

5. **Diverse-vendor ensemble for review** — Mixture-of-Agents
   pattern (Wang et al, 2024). Real measured improvement on
   AlpacaEval / MT-Bench. We use it as Coder/Reviewer split.

6. **Disaggregated prefill on burst-rented GPU** — vLLM +
   NIXL connector. Production at Meta. Phase 5+, not v1.
   Source: vLLM docs, NVIDIA Dynamo (GTC 2025).

7. **KV-cache offload to CPU** — HiSparse Coordinator pattern
   (LMSYS Day-0 V4 post). Lets V4-Flash hold longer contexts
   than VRAM alone allows.

8. **Self-consistency sampling** — for hard subtasks, run Coder
   3× with different seeds, majority-vote on the patch. Costs
   3× tokens for ~15% accuracy lift on hard problems. Use
   only when difficulty='frontier'.

9. **Local Qwen2.5-Coder-32B as fallback** — if internet drops
   mid-task, runner falls over to local 32B model rather than
   failing. Won't match V4-Pro but won't dead-stop.

---

## 10. Phasing (delta over spec 60 §13)

- **Spec 60 phases 0-4 stay as written**, with the model
  tier fixed at "L1 V4-Pro API" instead of Opus for those phases.
  This validates the LAZARUS bones without the swarm complexity.
- **New phase 4.5 — Local L0 wiring**:
  - ktransformers + SGLang installed locally.
  - V4-Flash weights pulled from HF.
  - `fleet/v4_flash_local.ts` adapter passing the runner's
    smoke tests.
  - Side-by-side benchmark: same task to L0 vs L1 → measure
    pass-rate delta.
- **New phase 4.7 — Peer review topology**:
  - Architect/Coder/Reviewer/Verifier/Synthesizer roles wired.
  - Diversity invariant enforced (coder vendor ≠ reviewer
    vendor).
  - 50-task eval run; compare bug-catch rate vs solo-loop.
- **Phase 5 (was: local LLM) → Phase 5a + 5b**:
  - 5a: cloud burst lease via RunPod for L2 V4-Pro-Max.
  - 5b: disaggregated-prefill experiment (research-grade).

---

## 11. Decisions I want from sovereign

1. **Hardware**: do you already have ≥256GB RAM on the
   workstation, or is this an upgrade ask first? (gates L0)
2. **Vendor diversity floor**: is "DeepSeek + Claude" enough,
   or do you want to add a third (Mistral / Qwen / Gemini) for
   extra error-mode diversity?
3. **DeepSeek API**: are we OK with sending code over to
   DeepSeek's API (they're a Chinese company; PRC data law
   applies)? This is a sovereign call, not a technical one.
   Privacy-strict path: L0 local + L3 Claude only, no L1.
4. **Burst-rent provider**: RunPod (most beginner-friendly),
   Vast.ai (cheapest), Shadeform (aggregator)? I'd default to
   RunPod for v1.
5. **Daily budget cap**: $25/day, $50/day, or "no cap, sovereign
   approves overages live"?
6. **Self-consistency**: enable 3× sampling on `frontier`
   tasks by default? Triples cost on those, but they're rare.
7. **Peer-review on PRIME-DIRECTIVE**: do you want a *human*
   review step (sovereign reads + signs) before any patch
   touching capability gates lands? I'd recommend yes — no
   model should be able to weaken consent-as-OS solo.

---

## 12. The first three Claude-Code prompts (revised)

**Prompt 1** — schema delta + sub-run table

> Read `specs/60_LAZARUS_AGENT.md` and `specs/61_LAZARUS_SWARM.md`.
> Implement migration `0050_lazarus.sql` per spec 60 §3, with the
> §7a additions from spec 61 (route_decision, team_recipe,
> allow_replay columns plus the new lazarus_subrun table).
> Companion `0051_lazarus_rls.sql` and `verify-lazarus.sql`
> mirror MNEME conventions. Apply to pzirbmyfmrbtkllrtcmx.

**Prompt 2** — REST surface + fleet adapters

> Read `specs/61_LAZARUS_SWARM.md` §7. Implement
> `cssl-edge/pages/api/lazarus/*` per spec 60 §4, plus the
> burst-lease endpoint stub from spec 61 §7c. Implement
> `lib/lazarus/fleet/{v4_api.ts,claude_api.ts}` first; stub
> `v4_flash_local.ts` and `v4_self_host.ts` with TODOs. Add
> `lib/lazarus/routing.ts` per spec 61 §7d. Tests must pass.

**Prompt 3** — runner with peer-review topology

> Read `specs/60` §7+§9 and `specs/61` §3+§4+§7b. Scaffold
> `repos/lazarus-runner/` per spec 60 §12. Implement the
> agents/ directory per spec 61 §7b. Wire the DAG executor.
> Make the loop.smoke.test.ts pass against stubbed fleet
> adapters. Diversity invariant (coder vendor ≠ reviewer
> vendor) is a hard test.

---

*end · 61_LAZARUS_SWARM.md*

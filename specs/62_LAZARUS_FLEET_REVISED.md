# 62 — LAZARUS FLEET REVISED (correction to spec 61 §2 + §6)

**Status**: design v1 · 2026-05-07
**Owner**: Apocky / sovereign
**Supersedes**: spec 61 §2 (fleet table) and §6 (hardware reality).
**Leaves intact**: spec 60 entirely; spec 61 §3 (peer-review topology),
§4 (routing), §5 (disaggregation idea), §7 (schema/runner deltas),
§8 (cost model — needs new numbers), §9 (bleeding-edge tricks).

---

## 0. Why this exists

Spec 61 assumed a Threadripper-class workstation with 256 GB DDR5
and an RTX 5090. The actual hardware is an i5-12600KF, 32 GB DDR4,
Intel Arc A770 16 GB, Z690 motherboard. That kills "local L0
V4-Flash via ktransformers" outright — wrong CPU instruction set,
wrong GPU vendor, wrong RAM ceiling.

Sovereign also has access to a dedicated DeepSeek-V4-Pro
deployment slot: 8× NVIDIA B300 288 GB · $96/hr active · $0/hr idle
with autoscaling 0→1 replica. That's a fundamentally better L2
than rented H100 hours, and it changes the routing math.

This spec corrects the fleet to fit both facts.

---

## 1. The corrected fleet (May 2026, sovereign-specific)

| Tier | Model                  | Where                                | Cost                                | Use                                          |
| ---  | ---                    | ---                                  | ---                                 | ---                                          |
| ~~L0~~ | ~~V4-Flash local~~   | ~~ktransformers~~                    | —                                   | **DELETED** · hardware can't host it         |
| L0′  | Qwen2.5-Coder-14B Q4   | local · llama.cpp Vulkan / IPEX-LLM  | electricity                         | offline fallback · fast classifiers · embeddings |
| L1f  | DeepSeek V4-Flash API  | api.deepseek.com                     | ~$0.10/M in · ~$0.30/M out          | bulk codegen, edits, lints (workhorse)       |
| L1p  | DeepSeek V4-Pro API    | api.deepseek.com                     | ~$0.27/M in · ~$1.10/M out          | architect, hard reasoning, default reviewer  |
| L2   | V4-Pro dedicated       | 8× B300 (your screenshot)            | $96/hr active · $0/hr idle          | session-mode heavy work · privacy-sensitive  |
| L3   | Claude Opus 4.7 API    | api.anthropic.com                    | $15/M in · $75/M out                | adversarial cross-vendor reviewer            |
| L4   | Claude Haiku 4.5 API   | api.anthropic.com                    | $1/M in · $5/M out                  | tie-breaker · quick verifier                 |

L0′ is the surviving local tier. Realistic at-home model on the
Arc A770 / 32 GB box:

- **Primary**: Qwen2.5-Coder-14B-Instruct Q4_K_M (~9 GB VRAM, fits
  the A770 with room for a 16-32K context).
- **Alternate**: DeepSeek-Coder-V2-Lite-16B (MoE · 2.4B active, ~9 GB
  Q4_K_M, very fast on A770 because of low active params).
- **Backend**: IPEX-LLM (Intel's PyTorch + LLVM/oneAPI stack, has
  first-class Arc A770 support and ships an llama.cpp-compatible
  llama-cpp-python build). Fallback: vanilla llama.cpp Vulkan.
- **Decode speed estimate**: 30-60 tok/s on small contexts, both
  models.

What L0′ is for: tiny edits ("rename this variable everywhere"),
fast classifications (e.g. "is this CSL valid?"), connectivity-loss
fallback so a run doesn't dead-stop. NOT for architecting,
reviewing, or anything frontier-grade. The frontier work all goes
to L1/L2/L3.

---

## 2. The dedicated deployment as L2 (the real win)

Scale-to-zero changes the strategy from spec 61's "burst-rent for
heavy tasks" to a cleaner **session model**:

```
Sovereign opens a coding session
   │
   ▼
Runner notices the lease queue has L2-tagged tasks
   │
   ▼
Runner POSTs to deployment-platform API: replicas=1
   │ (cold start: ~30-90s typical for dedicated MoE serving)
   ▼
L2 endpoint becomes a regular OpenAI-compatible URL
   │
   ▼
Architect / Coder / Synthesizer drain the queue at L2
   │ (potentially many tasks in one warm session)
   ▼
After idle_timeout (configurable, e.g. 5 min), platform
   scales back to 0 → $0/hr again
```

**Why this beats per-task rental**:

- $96/hr is unforgiving per-call but trivial per-session.
  A 1-hour session that resolves 20 frontier subtasks =
  $4.80/subtask, comparable to all-Opus.
- Once warm, KV cache, prefix cache, and dispatcher state stay
  hot. The 21st task in a session is much cheaper than the 1st.
- Single-tenant (probably — needs confirmation from §6 below)
  means no token-data-residency concerns.
- B300 has the headroom to run multiple agent roles concurrently,
  so coder + reviewer + synthesizer can all hit the same endpoint
  without queueing.

**Honest sizing math**:

- V4-Pro at FP8: ~1.1 TB. Fits 8× B300 288 GB (2.3 TB total) with
  ~1.2 TB left for KV cache. Roughly 2-4 concurrent users at 1M
  context, or 30+ concurrent at 32K context.
- Throughput estimate at MoE-tuned settings: 30-60 tok/s per stream,
  6-10 streams in parallel, so ~200-500 tok/s aggregate.
- $96/hr ÷ 300 tok/s aggregate ≈ $0.089 per 1M tokens.
- That undercuts the V4-Pro API ($0.27/M input, $1.10/M output)
  *if and only if* you saturate the endpoint. Idle B300 minutes
  are pure burn.

**Routing rule** for L2:

```
use_l2 ← (
    queue_has_2_or_more_l2_tasks
    OR (single_l2_task AND task.estimated_tokens > 500_000)
    OR sovereign_explicit_warm_up
) AND within_daily_budget AND not_already_warm_for_5min_ahead
```

i.e. don't spin up B300 for one 50K-token query. Let those go to
the API.

---

## 3. Revised cost model (medium task · same example as spec 61 §8)

"Add an endpoint to cssl-edge" style task:

| Role         | Tier  | Model              | In/Out tokens | Cost      |
| ---          | ---   | ---                | ---           | ---       |
| Architect    | L1p   | V4-Pro API         | 800 / 600     | $0.0009   |
| Coder × 2    | L1f   | V4-Flash API       | 4K / 2K each  | $0.002    |
| Reviewer     | L3    | Claude Opus        | 6K / 800      | $0.15     |
| Verifier     | L0′   | local Qwen-Coder   | tests run     | ~$0.00    |
| Synthesizer  | L1p   | V4-Pro API         | 8K / 1K       | $0.003    |
| **Total**    |       |                    |               | **~$0.16**|

Same as spec 61. The Coder change from "L0 local" to "L1f API"
costs an extra ~$0.002 per task, which is rounding error. Verifier
becomes L0′ (local Qwen) for cheap test-orchestration; this is a
real cost saving since verifier calls are frequent.

Frontier task with L2 warm session (5 frontier tasks in 1 hour
batch):

| Component                        | Cost    |
| ---                              | ---     |
| L2 endpoint, 1 hour              | $96.00  |
| 5× Reviewer L3 Claude Opus       | ~$5.00  |
| 5× Synthesizer (uses warm L2)    | $0      |
| All Architect/Coder (warm L2)    | $0      |
| **Total session**                | **~$101** |
| **Per frontier task**            | **~$20** |

vs all-Opus frontier (spec 61 estimate): $20-50 per task. So L2
session-mode breaks even with all-Opus when you push 5+ frontier
tasks through one warm hour. Below 5, just use the API.

---

## 4. Updated routing decision points

Add to spec 61 §4a's classifier output:

```ts
type RouteDecision_v2 = RouteDecision & {
    l2_warm_recommended: boolean,
    estimated_total_tokens: number,    // for the whole task
    privacy_class: 'public' | 'sovereign' | 'prime_directive',
};
```

`privacy_class='prime_directive'` forces L2 (single-tenant) over
L1 (shared API) for any code that touches consent/sigma/capability
gating. This is a hard rule — PRIME_DIRECTIVE-adjacent code never
ships through DeepSeek's shared inference farm.

The router maintains an L2 warm-window state machine:

```
states: COLD → WARMING → WARM → COOLING → COLD

transitions:
  COLD     →WARMING     queue has eligible L2 tasks
  WARMING  →WARM        deployment health-check returns 200
  WARM     →WARM        keep-alive every 60s while tasks pending
  WARM     →COOLING     no eligible task for 4 min
  COOLING  →COLD        platform autoscale fires (5 min idle)
  COOLING  →WARM        new eligible task arrives — cancel cooldown
```

Surfaces in the runner as `lib/lazarus/l2-warm.ts`. Single
state-machine instance per runner; locks on Supabase row in
`lazarus_runner.capabilities.l2_warm` to coordinate if multiple
runners ever exist.

---

## 5. What survives from spec 61, what changes

| Spec 61 section | Status                                      |
| ---             | ---                                         |
| §1 (article reading) | unchanged · still right                |
| §2 (fleet table)     | **superseded by §1 above**             |
| §3 (peer-review topology) | unchanged                         |
| §4 (routing)         | **augmented by §4 above**              |
| §5 (disaggregation)  | demoted to "research curiosity" — L2 makes it unnecessary for v1-v4. The dedicated deployment IS the better answer. |
| §6 (hardware reality)| **superseded by §0 above**             |
| §7 (schema deltas)   | unchanged                              |
| §7c (cloud burst)    | becomes "L2 warm-state controller" instead of "RunPod lease" |
| §8 (cost model)      | **superseded by §3 above**             |
| §9 (bleeding-edge)   | unchanged · all tricks still apply     |
| §10 (phasing)        | Phase 4.5 (local L0) becomes "wire L0′ Qwen-Coder via IPEX-LLM"; Phase 5a (cloud burst) becomes "L2 warm-state controller" |
| §11 (open questions) | mostly answered now — see §6 below     |

---

## 6. Open questions, revised

**Answered by the screenshots**:
- ~~Q1: Hardware ≥ 256 GB?~~ → No. L0 is L0′ (Qwen-Coder small).
- ~~Q4: Burst-rent provider?~~ → It's whatever platform image 1
  shows. See open question below.

**Still open**:
1. **Which platform is image 1?** I can't fingerprint it from the
   UI alone — the violet rocket-Deploy and "Minimal/Maximum" perf
   tiers look like Vercel AI Cloud or Nebius AI Studio, but
   neither is confirmed. Need: platform name, OpenAI-compat endpoint
   URL pattern, cold-start time SLA, and whether the deployment is
   truly single-tenant.
2. **Cold-start time?** If it's >2 min the warm-state machine in
   §4 needs different timeouts. If it's <30 s we can be more
   aggressive about cooldown.
3. **Cold-start cost?** Some "scale-to-zero" platforms still bill
   for the warm-up minutes. Need to read the fine print.
4. **Vendor diversity floor** (was Q2)? My recommendation:
   DeepSeek + Claude is enough for v1. Add a third only if peer-
   review accuracy plateaus.
5. **DeepSeek API privacy** (was Q3)? Recommendation: L1 (shared
   API) for non-sensitive code, L2 (dedicated) for anything
   PRIME_DIRECTIVE-adjacent. This sidesteps the PRC-data-law
   question for the parts that matter.
6. **Daily budget cap** (was Q5)? Recommendation: $30/day soft cap
   covers ~5 frontier-task sessions or ~150 medium tasks. Hard cap
   $100/day. Sovereign approval to override.
7. **Self-consistency on frontier** (was Q6)? Recommendation: enable
   only when L2 is warm — token cost is negligible inside a warm
   session. Disable on L1 paths.
8. **PRIME-DIRECTIVE human review** (was Q7)? Recommendation: yes,
   non-negotiable, sovereign signs. No model can solo-merge
   capability changes.

---

## 7. The L0′ local stack (concrete commands)

Because it's specific to your hardware, worth nailing down:

```powershell
# install IPEX-LLM with Arc A770 wheels
python -m pip install --upgrade pip
python -m pip install --pre --upgrade ipex-llm[xpu] `
    --extra-index-url https://pytorch-extension.intel.com/release-whl/stable/xpu/us/

# pull a Qwen2.5-Coder-14B GGUF from unsloth (Q4_K_M)
huggingface-cli download unsloth/Qwen2.5-Coder-14B-Instruct-GGUF `
    Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf `
    --local-dir C:\Users\Apocky\models\qwen-coder-14b

# serve via IPEX-LLM's llama.cpp port
$env:SYCL_CACHE_PERSISTENT="1"
$env:GGML_SYCL_DISABLE_OPT="0"
ipex-llm-init
.\llama-server.exe `
    -m C:\Users\Apocky\models\qwen-coder-14b\Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf `
    -ngl 99 `
    --host 127.0.0.1 --port 8080 `
    -c 16384
```

Runner-side adapter at `lazarus-runner/src/llm/local.ts` speaks
OpenAI-compatible to `http://127.0.0.1:8080/v1/chat/completions`.

Smoke test target: 30+ tok/s sustained on Qwen-Coder-14B Q4 with
8K context. If you don't hit that, fall back to Vulkan llama.cpp
(also works on A770 but slightly slower in our experience).

---

## 8. Action items in priority order

1. **You**: tell me which platform image 1 is, so I can pin the L2
   adapter to its API shape. (Vercel AI Cloud? Nebius? Fireworks?
   Together? Sambanova?)
2. **You**: confirm cold-start time and whether the deployment is
   truly single-tenant.
3. **You**: pick the daily soft cap ($30 default).
4. **Me, with Claude Code**: phase 0-2 from spec 60 stand. Use
   L1p V4-Pro API as the brain for those phases.
5. **Me, with Claude Code**: spec 60 phase 3 (UI + approvals) stand.
6. **Me, with Claude Code**: phase 4.5 = L0′ wiring per §7 above.
7. **Me, with Claude Code**: phase 4.7 = peer-review topology per
   spec 61 §3, with the corrected fleet from §1 above.
8. **Me, with Claude Code**: phase 5 = L2 warm-state controller
   per §4 above. Triggered only after sovereign confirms the
   platform and SLA.

---

*end · 62_LAZARUS_FLEET_REVISED.md*

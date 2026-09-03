# lib/mneme — developer guide

Status: v1 · 2026-09-03 · private member broker wired · self-tests inline.

MNEME is the agent-memory service for the CSSL/LoA portfolio. It is a
proprietary, sovereign-gated mirror of Cloudflare's "Agent Memory" pattern,
adapted to use CSLv3 as the canonical storage form and Postgres + pgvector
+ Voyage embeddings for retrieval. The Cloudflare implementation we
mirrored has 6 retrieval channels and a 2-pass extractor; we keep that
exact retrieval/ingest topology and re-implement it here under our stack.

## Source of truth

Specs (under `specs/`):

- `43_MNEME.csl`           — canonical spec · types · ops · invariants · model choices
- `44_MNEME_PIPELINES.csl` — 8-stage ingest + 7-stage retrieve detail
- `45_MNEME_SCHEMA.csl`    — Postgres DDL · indexes · RLS · triggers

Migrations (under `cssl-supabase/migrations/`):

- `0040_mneme.sql`     — extensions + 4 tables + 3 triggers + index plan + helper fn
- `0041_mneme_rls.sql` — RLS policies (service-role bypass + caller-PK predicate)
- `verify-mneme.sql`   — smoke tests (run after applying migrations)

## Layout

```
lib/mneme/
  types.ts                  — TypeScript shapes (Memory, Profile, RecallResponse, …)
  csl.ts                    — minimal CSLv3 validator + topic-key extractor
  sigma.ts                  — SigmaMask 19B/32B codec + cap helpers
  embed.ts                  — Voyage-3-large client (document/query)
  anthropic.ts              — Claude Messages API (Haiku + Sonnet) with prompt-caching
  store.ts                  — Supabase repo + 6 retrieval channels + RRF
  pipeline-ingest.ts        — 8-stage ingest orchestrator
  pipeline-retrieve.ts      — 7-stage retrieve orchestrator
  prompts/
    system-prelude.ts       — cached CSLv3 onboarding system prompt
    extract-full.ts         — Haiku · 10K-char chunks · 4-concurrent
    extract-detail.ts       — Haiku · 5-msg windows
    verify.ts               — Haiku · 8-check verifier
    classify.ts             — Haiku · type + topic + queries
    query-analyze.ts        — Haiku · topic_keys + fts_terms + HyDE
    synthesize.ts           — Sonnet · returns {result_nl, result_csl, citations, confidence}
```

## REST surface

The public member broker exposes only `/api/mneme/me/*`. `me` is a route
sentinel, not a profile identifier: the server derives an opaque profile ID
from the verified site session. Client-supplied profile names, and the old
public `ingest` and `smoke` routes, are neutral `404` responses in middleware.

| Route       | Method | Body / Query                                                | Requirement                              |
| ---         | ---    | ---                                                         | ---                                      |
| `health`    | GET    | —                                                           | verified session                         |
| `recall`    | POST   | `{ query, k?, types?, audience_bits? }`                     | provisioned profile + semantic services  |
| `remember`  | POST   | `{ csl, paraphrase?, type?, topic_key?, sigma_mask_hex? }`  | provisioned profile + semantic services  |
| `list`      | GET    | `?type=&limit=&cursor=`                                     | provisioned profile + storage            |
| `forget`    | POST   | `{ memory_id, reason }`                                     | provisioned profile + storage            |
| `export`    | GET    | —                                                           | provisioned profile + storage            |

POST requests must be same-origin. Every response is private and non-cacheable.
The member routes never create a profile implicitly and never report mock
storage as a successful write. A signed-in member without a provisioned
profile receives `MNEME_PROFILE_NOT_PROVISIONED`; unavailable storage receives
`MNEME_STORAGE_UNAVAILABLE`.

Successful responses carry `served_by` + `ts` per the cssl-edge envelope
convention. Errors include a stable machine-readable `code` alongside a short,
non-sensitive `error` message.

## Local development

1. From `cssl-edge/`:

   ```sh
   npm install
   cp .env.example .env.local
   # set ANTHROPIC_API_KEY, VOYAGE_API_KEY, NEXT_PUBLIC_SUPABASE_URL,
   # SUPABASE_SERVICE_ROLE_KEY, MNEME_SOVEREIGN_PUBKEY_HEX
   npm run check        # typecheck (must pass before push)
   npm run dev          # next dev — use /memory-tools through a signed-in site session
   ```

2. Apply migrations to a Supabase project:

   ```sh
   # via Supabase CLI:
   supabase db push
   # or paste 0040 then 0041 into SQL editor
   psql -f cssl-supabase/verify-mneme.sql
   ```

3. Run the isolated pipeline suite without external services. These tests call
   the library directly; no unauthenticated HTTP smoke route is exposed:

   ```sh
   npm run test:mneme
   ```

4. Self-test sigma codec:

   ```sh
   npx tsx lib/mneme/sigma.ts
   # → "sigma.ts : OK · 12 self-tests passed"
   ```

## Pipeline tests

Located in `tests/api/mneme/`. Each pipeline stage has a stubbed-LLM smoke:

- `ingest.smoke.test.ts`        — full pipeline against null sb + stub LLM
- `retrieve.smoke.test.ts`      — full pipeline against null sb + stub LLM
- `extract-full.test.ts`        — chunker boundary + stub-extracted shape
- `extract-detail.test.ts`      — should-skip threshold + window builder
- `verify.test.ts`              — verdict mapping + corrected branch
- `classify.test.ts`            — topic-discipline (event/task → null)
- `query-analyze.test.ts`       — temporal regex + fallback path
- `synthesize.test.ts`          — confidence floor + invalid-CSL fallback
- `sigma.test.ts`               — hex round-trip + revoke monotonicity
- `csl.test.ts`                 — validator pass/fail across canonical forms
- `pipeline-rrf.test.ts`        — RRF math determinism

Run: `npm run test:mneme`.

## Anti-patterns

These are the production-shaped traps we explicitly avoid:

- ✗ Storing English-only paraphrase as canonical
  ✓ `csl` is the source of truth; paraphrase is a denormalised view
- ✗ Letting the LLM build SQL queries (token burn)
  ✓ Constrained tools: `recall|remember|list|forget`; never raw SQL
- ✗ Asking the LLM to do date math
  ✓ Pre-compute via regex in `pipeline-retrieve.ts §STAGE-5`
- ✗ DB-per-profile (Cloudflare DO model)
  ✓ Shared schema + RLS-per-profile_id
- ✗ Skipping CSL validation
  ✓ `validateCsl()` gate at STAGE-6 of ingest

## Sovereign / consent

Every memory carries a `SigmaMask` (specs/27). Forgetting a memory flips
the `revoked_at` u32 LE bytes inside the mask; cascade forgets walk both
the supersede chain and rows that point at the target. RLS policies use
`mneme_mask_revoked_at(sigma_mask) = 0` so revoked rows become invisible
without DELETE. The `forget` route emits an audit row. Export bundles
the full set so leaving with your data is trivial.

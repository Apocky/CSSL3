# MNEME — agent memory service

Sovereign-gated, CSL-canonical, Anthropic-aligned memory layer for AI
agents. Built on Supabase (Postgres + pgvector) and deployed via
cssl-edge (Vercel Pages-Router).

This is a desktop-bespoke mirror of Cloudflare's Agent Memory pattern,
re-implemented under the Apocky stack with CSLv3 as the canonical
storage form. Because we own the substrate, every memory is consent-
gated by a per-row Sigma mask (specs/27) and revocation cascades
through supersession chains.

## Why this exists

Long-running AI agents need durable memory across sessions, projects,
and contexts. SaaS memory products are a vendor-lockin trap. MNEME is
ours: data lives in our Supabase project, retrieval is cited and
auditable, and "leaving" is a one-call gzip export.

## What it provides

- **Profile** — per-context namespace (e.g. `loa-v10`, `infinite-labyrinth`).
- **Ingest** — push raw conversation messages; run a 2-pass extractor;
  verify; classify; supersede; embed. Idempotent re-ingest via
  content-addressed message IDs.
- **Recall** — 6-channel retrieval (`fts_csl + fts_paraphrase +
  fts_messages + topic_exact + vec_direct + vec_hyde`) → reciprocal-
  rank fusion → Sonnet synthesis with citations + confidence.
- **Remember** — single-shot direct write (skip extraction).
- **List / Forget / Export** — discoverable, revocable, portable.

## Files

- `specs/43_MNEME.csl`            — canonical spec
- `specs/44_MNEME_PIPELINES.csl`  — 8-stage ingest + 7-stage retrieve
- `specs/45_MNEME_SCHEMA.csl`     — Postgres DDL + RLS

- `cssl-supabase/migrations/0040_mneme.sql`     — tables + triggers + indexes
- `cssl-supabase/migrations/0041_mneme_rls.sql` — RLS policies
- `cssl-supabase/verify-mneme.sql`              — smoke tests

- `cssl-edge/lib/mneme/`                        — implementation
- `cssl-edge/pages/api/mneme/[profile]/`        — REST surface
- `cssl-edge/lib/mneme/README.md`               — developer guide
- `cssl-edge/tests/api/mneme/`                  — self-tests

## Getting started

1. **Apply migrations** to your Supabase project:

   ```sh
   cd cssl-supabase
   # via CLI
   supabase db push
   # or paste 0040 then 0041 into the SQL editor
   psql -f verify-mneme.sql
   ```

2. **Wire env** in `cssl-edge/.env.local`:

   ```env
   ANTHROPIC_API_KEY=sk-ant-...
   VOYAGE_API_KEY=pa-...
   NEXT_PUBLIC_SUPABASE_URL=https://<proj>.supabase.co
   SUPABASE_SERVICE_ROLE_KEY=eyJ...
   MNEME_SOVEREIGN_PUBKEY_HEX=<64 hex chars · your Ed25519 PK>
   ```

3. **Run cssl-edge locally**:

   ```sh
   cd cssl-edge
   npm install
   npm run check        # typecheck
   npm run dev
   ```

4. **Probe**:

   ```sh
   curl http://localhost:3000/api/mneme/scratch/health
   curl http://localhost:3000/api/mneme/scratch/smoke
   ```

5. **Ingest a conversation**:

   ```sh
   curl -X POST http://localhost:3000/api/mneme/scratch/ingest \
     -H 'Content-Type: application/json' \
     -d '{
       "session_id": "demo-1",
       "messages": [
         { "role": "user", "content": "I prefer pnpm over npm." },
         { "role": "assistant", "content": "Got it — pnpm noted." }
       ]
     }'
   ```

6. **Recall**:

   ```sh
   curl -X POST http://localhost:3000/api/mneme/scratch/recall \
     -H 'Content-Type: application/json' \
     -d '{ "query": "which package manager?" }'
   ```

## Model choices (frozen at v1)

| Stage          | Model                  | Notes                              |
| ---            | ---                    | ---                                |
| Extraction     | claude-haiku-4-5       | temp=0 · 4096 tok · 4-concurrent   |
| Verification   | claude-haiku-4-5       | temp=0 · 2048 tok                  |
| Classification | claude-haiku-4-5       | temp=0 · 1024 tok                  |
| Query-analyze  | claude-haiku-4-5       | temp=0 · 1024 tok                  |
| Synthesis      | claude-sonnet-4-6      | temp=0.3 · 2048 tok                |
| Embedding      | voyage-3-large (1024d) | cosine                             |

System prompts are prompt-cached via `cache_control: ephemeral`. Per
Anthropic docs this hits ≥90% cache reuse in steady state.

## Sovereign + consent

- Every memory carries a `SigmaMask` (specs/27); forgetting flips a u32
  inside the mask, and RLS hides rows where `revoked_at != 0`.
- Cascade forgets walk both supersession ancestors and descendants.
- Audit table appends a row on every state-changing op.
- Export endpoint dumps the full bundle as JSON; round-tripping it
  through ingest yields the same memories (deterministic message-ids).

## License

Proprietary · part of the Apocky CSL/CSSL/LoA portfolio. No external
copy/redistribute. PRIME_DIRECTIVE.md applies in full.

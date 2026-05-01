# cssl-edge

LoA-v13 Edge — public MCP gateway for CSSL/LoA scenes. Deployed as a Next.js
app on Vercel; serves five serverless API routes plus a static landing page.

This is a **stage-0 scaffold**. Every API route returns a well-shaped JSON
response with `stub:true`, so client code can integrate today and pick up real
behavior as the implementations land.

---

## Endpoints

| Method | Path                                | Purpose                                       |
| ------ | ----------------------------------- | --------------------------------------------- |
| GET    | `/api/health`                       | Liveness ping; returns commit SHA             |
| POST   | `/api/intent`                       | Text → scene-graph (LLM-backed when keyed)    |
| GET    | `/api/asset/search?q=...&license=`  | License-filtered asset search                 |
| GET    | `/api/asset/<src>/<id>/glb`         | Cached binary proxy for a specific asset      |
| POST   | `/api/generate/3d`                  | Neural-3D gateway; provider fan-out           |

Every JSON response includes `served_by` and `ts` for tracing.

### `/api/intent` contract

```bash
curl -X POST https://<deploy>/api/intent \
  -H 'content-type: application/json' \
  -d '{"text":"a cabin in the woods at dusk","cap":"sovereign"}'
```

Returns:

```json
{
  "scene_graph": { "nodes": [], "edges": [] },
  "warnings": ["LLM not configured · returning stub (set CLAUDE_API_KEY in Vercel env)"],
  "latency_ms": 1,
  "served_by": "cssl-edge",
  "ts": "2026-04-30T00:00:00.000Z",
  "stub": true,
  "todo": "wire Anthropic SDK · text→scene-graph compiler",
  "cap": "sovereign"
}
```

### `/api/asset/search` contract

```bash
curl 'https://<deploy>/api/asset/search?q=cabin&license=cc0'
```

Returns:

```json
{
  "results": [
    {
      "src": "polyhaven",
      "id": "wooden_cabin_01",
      "name": "Wooden Cabin 01",
      "license": "cc0",
      "format": "glb",
      "url": "https://polyhaven.com/a/wooden_cabin_01",
      "preview_url": "https://cdn.polyhaven.com/asset_img/primary/wooden_cabin_01.png"
    }
  ],
  "total": 1,
  "query": "cabin",
  "license_filter": "cc0",
  "served_by": "cssl-edge",
  "ts": "2026-04-30T00:00:00.000Z",
  "stub": true,
  "todo": "fan-out to upstream catalogs · cache via Supabase · paginate"
}
```

---

## Local development

```bash
cd cssl-edge
npm install
npm run dev   # http://localhost:3000
```

Type-check without building:

```bash
npm run check     # tsc --noEmit
```

Build for production locally:

```bash
npm run build
npm run start
```

---

## Deploy to Vercel

The scaffold is `vercel deploy`-able as-is. Two paths:

### 1. Vercel CLI

```bash
npm i -g vercel
cd cssl-edge
vercel link            # one-time: pick / create the project
vercel --prod          # deploy
```

### 2. Git integration

1. Push `cssl-edge/` to a Git remote Vercel can see.
2. In the Vercel dashboard, **New Project** → import the repo.
3. **Root Directory** = `cssl-edge` (this is the key step — the project lives
   in a subdirectory of the parent `CSSLv3` repo).
4. Framework will auto-detect as **Next.js**.
5. Click **Deploy**. The first build runs `npm install` + `npm run build`.

---

## Wiring real implementations

All keys in `.env.example` are **optional** — the scaffold ships
stub-friendly. To unlock real behavior, add the relevant variables in the
Vercel project settings (Settings → Environment Variables).

### `CLAUDE_API_KEY` — unlocks `/api/intent`

1. Generate a key at https://console.anthropic.com/.
2. Add to Vercel as `CLAUDE_API_KEY`.
3. The `/api/intent` route detects the key and routes to the Anthropic SDK
   (real implementation lands in a follow-up slice).

### `SUPABASE_URL` + `SUPABASE_SERVICE_ROLE_KEY` — unlocks asset cache + jobs

1. Create a project at https://supabase.com.
2. Copy `Project URL` and `Service Role Key` from `Settings → API`.
3. Add both to Vercel.
4. The asset proxy + neural-3D gateway will use Supabase Storage for binary
   caching and Supabase Postgres for the `jobs` table.

### Provider keys (Sketchfab, Stability, Meshy, Tripo)

See `.env.example` for the complete list. Each provider can be enabled
independently — missing keys cause that upstream to be skipped without
breaking the scaffold.

---

## Architecture notes

- `lib/license_filter.ts` — shared license-permit logic. Stage-0 ships a
  conservative whitelist (`cc0`, `cc-by`, `cc-by-sa`, `public-domain`).
  Closed/unknown licenses are rejected by default.
- `lib/response.ts` — standard envelope helpers (`served_by`, `ts`, stub
  shape, commit-SHA resolver). Every endpoint composes responses through this.
- `vercel.json` — pins region (`iad1`), sets `maxDuration: 30s` on functions,
  and emits permissive CORS plus an `X-Served-By: cssl-edge` header.
- `tests/api/health.test.ts` — framework-agnostic test stub (lives outside
  `pages/` so Next does not register it as a route). Importable under
  vitest / jest / node:test without rewrites.

---

## Status

| Slice    | State        | Description                                    |
| -------- | ------------ | ---------------------------------------------- |
| Stage-0  | this commit  | Scaffold + 5 stubs; `vercel deploy`-able       |
| Stage-1  | next         | Wire Anthropic SDK in `/api/intent`            |
| Stage-2  | next         | Real upstream fan-out + Supabase cache         |
| Stage-3  | next         | Neural-3D job queue + poll-status route        |

---

## Attestation

There was no hurt nor harm in the making of this, to anyone/anything/anybody.

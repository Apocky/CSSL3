# cssl-supabase

Canonical Supabase schema for the CSSL / LoA stack: asset library, player-saved
scenes, opt-in text->scene history corpus, and PRIME-DIRECTIVE-required
audit logs for AI-companion operations.

This directory is everything you need to bootstrap a Supabase project for the
CSSL backend. Apply the migrations, run the seed, run `verify.sql`, and the
backend is ready for `cssl-edge` / Vercel functions / native CSSL programs to
talk to.

---

## Layout

```
cssl-supabase/
  migrations/
    0001_initial.sql          -- table DDL, indexes, helper functions/triggers
    0002_rls_policies.sql     -- row-level security
    0003_storage_buckets.sql  -- 3 buckets + per-bucket policies
  seed.sql                    -- 10 example public-domain / CC0 / CC-BY assets
  verify.sql                  -- post-migration assertions
  types.ts                    -- TypeScript types (cssl-edge / clients)
  README.md                   -- this file
```

---

## Apply the schema

There are two equally valid ways to apply.

### Option A: `supabase db push` (Supabase CLI)

```bash
# from a Supabase-linked project root
cp -r path/to/cssl-supabase/migrations supabase/migrations
supabase db push
psql "$SUPABASE_DB_URL" -f cssl-supabase/seed.sql
psql "$SUPABASE_DB_URL" -f cssl-supabase/verify.sql
```

### Option B: paste into the Supabase SQL editor

Open the project's SQL editor in the Supabase dashboard and paste the
contents of these files in order:

1. `migrations/0001_initial.sql`
2. `migrations/0002_rls_policies.sql`
3. `migrations/0003_storage_buckets.sql`
4. `seed.sql`
5. `verify.sql`

`verify.sql` raises an exception on any failed assertion (RLS not enabled,
seed not loaded, buckets missing, etc.). Successful run prints
`verify.sql : all assertions passed`.

---

## Schema at a glance

| Table                      | Purpose                                                  | Privacy model                                |
|----------------------------|----------------------------------------------------------|----------------------------------------------|
| `public.assets`            | Cached metadata for upstream 3D / HDRI / texture assets  | Public read, service-role write              |
| `public.scenes`            | Player-saved scene-graphs                                | Own-only (or `is_public = true` for share)   |
| `public.history`           | Opt-in text->scene corpus (anonymous or signed-in)       | Own rows + anonymous rows are world-visible  |
| `public.companion_logs`    | PRIME-DIRECTIVE audit-trail for AI-companion ops         | Own-only, append-only, user-immutable        |

Storage buckets:

| Bucket        | Public | Per-file limit | Path convention                   |
|---------------|--------|----------------|-----------------------------------|
| `assets`      | yes    | 50 MiB         | `<source>/<source_id>.<ext>`      |
| `screenshots` | mixed  | 10 MiB         | `<user_id>/<scene_id>.<ext>`      |
| `audio`       | no     | 10 MiB         | `<user_id>/<recording_id>.<ext>`  |

`screenshots` is *mixed*: own-read+write always, plus public-read iff the
referenced scene row has `is_public = true`.

---

## RLS policy summary

```
public.assets
    SELECT  : public
    INSERT  : service_role only
    UPDATE  : service_role only
    DELETE  : service_role only

public.scenes
    SELECT  : own rows OR is_public = true
    INSERT  : own (user_id must equal auth.uid())
    UPDATE  : own
    DELETE  : own

public.history
    SELECT  : own rows OR rows where user_id IS NULL (anonymous opt-in)
    INSERT  : own OR anonymous (NULL user_id)
    UPDATE  : DENIED (no policy = forbidden)
    DELETE  : own (anonymous rows = service-role only)

public.companion_logs
    SELECT  : own only
    INSERT  : own (user_id must equal auth.uid())
    UPDATE  : DENIED (no policy = forbidden)
    DELETE  : service-role only (audit-immutable for users)
```

---

## Environment variables

```bash
SUPABASE_URL=https://<project-ref>.supabase.co
SUPABASE_ANON_KEY=eyJ...           # browser / CSSL client
SUPABASE_SERVICE_ROLE_KEY=eyJ...   # cssl-edge crawlers / migrations only
```

Per-environment values (`.env.development`, `.env.staging`, `.env.production`)
should be wired through the same triple. **Never** ship `SUPABASE_SERVICE_ROLE_KEY`
to a browser or to a CSSL program running on a player's device. It is for
trusted server-side code only (asset crawlers, batch jobs, the migration
runner).

---

## CSSL FFI surface (cssl-edge entry points)

CSSL programs reach Supabase via `cssl-edge` HTTP wrappers (defined in the
sibling `W-WAVE3-http-ffi` worktree). The expected surface, from the CSSL
side:

```cssl
@extern "cssl-edge"
fn supabase_assets_search(query: str, license_filter: str?) -> Json
@extern "cssl-edge"
fn supabase_asset_get(source: str, source_id: str) -> Json

@extern "cssl-edge"
fn supabase_scene_save(name: str, seed: str, graph: Json, is_public: bool) -> Uuid
@extern "cssl-edge"
fn supabase_scene_load(scene_id: Uuid) -> Json
@extern "cssl-edge"
fn supabase_scene_record_play(scene_id: Uuid) -> i64

@extern "cssl-edge"
fn supabase_history_log(seed: str, graph: Json?, success: bool, anonymous: bool) -> Uuid

@extern "cssl-edge"
fn supabase_companion_log(handle: str, op: str, params: Json,
                          accepted: bool, refusal: str?) -> Uuid
```

`cssl-edge` is the Vercel-hosted edge function layer. It holds the
`SUPABASE_*_KEY` secrets and translates CSSL FFI calls into authenticated
PostgREST / Storage requests. CSSL programs never see raw keys.

---

## Tests

* `verify.sql` is the canonical assertion set: tables, RLS, seed counts,
  buckets, RPC functions, indexes, policies. Run it after every migration
  or schema change.
* CI: a future `cssl-supabase-ci` workflow can run a clean Postgres in
  Docker, apply the migrations + seed + verify, and fail on any assertion.
* The SQL itself is `pg_lint`-clean (no obvious syntax errors) and uses
  only stable Postgres / Supabase features (`pgcrypto`, `pg_trgm`, the
  `auth` and `storage` schemas).

---

## Privacy and PRIME-DIRECTIVE notes

* `public.history` accepts fully anonymous rows (`user_id IS NULL`) so a
  player can opt in to corpus contributions without creating an account.
  Anonymous rows are world-visible by design (it is the corpus).
* `public.companion_logs` is the substrate-side audit trail required by
  PRIME-DIRECTIVE Section 11 (Attestation): every cap-gated AI-companion
  operation, accepted or refused, is recorded with timestamp, sovereign
  handle, params, and refusal reason. Users can read their own log but
  cannot edit or delete it. Hard deletion is service-role-only and is
  intended for legal / GDPR-erasure paths only.
* No table stores raw player PII beyond the `auth.users` reference.
  Display names / avatars are out of scope for this schema and live in
  `auth.users.raw_user_meta_data` per Supabase convention.

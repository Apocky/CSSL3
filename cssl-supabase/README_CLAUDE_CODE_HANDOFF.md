# Claude Code Handoff README: cssl-supabase

`cssl-supabase` is the database schema and verification package for the CSSL /
LoA / apocky.com backend. It is consumed by `cssl-edge`, Vercel serverless
routes, Lazarus, and future CSSL clients through HTTP wrappers.

This handoff focuses on practical setup, migration order, verification, and the
current Lazarus schema.

## Directory Map

```text
cssl-supabase/
  README.md                    broad schema README
  README_CLAUDE_CODE_HANDOFF.md this handoff file
  migrations/                  ordered SQL migrations
  functions/                   Supabase edge functions, if/when used
  seed.sql                     early seed data
  types.ts                     TypeScript client/shared schema types
  verify.sql                   base schema assertions
  verify-signaling.sql         multiplayer signaling assertions
  verify-cocreative.sql        co-creative schema assertions
  verify-game-state.sql        game-state assertions
  verify-mneme.sql             MNEME assertions
```

## How cssl-edge Uses This Schema

`cssl-edge` reads and writes Supabase through server-side helpers and API
routes. Browser clients should call `cssl-edge` APIs. They should not receive
service-role keys.

Important server variables:

```text
SUPABASE_URL
SUPABASE_ANON_KEY
SUPABASE_SERVICE_ROLE_KEY
```

Important browser variables, configured in `cssl-edge`:

```text
NEXT_PUBLIC_SUPABASE_URL
NEXT_PUBLIC_SUPABASE_ANON_KEY
APOCKY_HUB_SUPABASE_URL
APOCKY_HUB_SUPABASE_ANON_KEY
```

Never print or commit real values.

## Migration Inventory

Migrations currently present:

```text
0001_initial.sql                    base assets/scenes/history/companion logs
0002_rls_policies.sql               base RLS policies
0003_storage_buckets.sql            storage buckets and policies
0004_signaling.sql                  multiplayer signaling tables/functions
0005_signaling_rls.sql              signaling RLS policies
0006_signaling_seed.sql             signaling demo seed
0007_cocreative.sql                 co-creative systems
0008_cocreative_rls.sql             co-creative RLS
0009_cocreative_seed.sql            co-creative seed
0010_game_state.sql                 game state schema
0011_sovereign_audit.sql            sovereign audit support
0012_game_state_rls.sql             game state RLS
0013_game_state_seed.sql            game state seed
0014_kan_canary.sql                 KAN canary schema
0015_kan_canary_rls.sql             KAN canary RLS
0016_gear_inventory.sql             gear/inventory schema
0017_runs.sql                       run records
0018_npc_state.sql                  NPC state
0019_multiplayer_shards.sql         multiplayer shard state
0020_player_progression.sql         player progression
0022_payments.sql                   payments schema
0023_payments_rls.sql               payments RLS
0024_analytics.sql                  analytics schema
0025_akashic.sql                    Akashic diagnostics/telemetry
0026_hotfix.sql                     hotfix system
0027_content.sql                    content publishing base
0028_content_rating.sql             content ratings
0029_content_subscriptions.sql      subscriptions
0030_content_remix.sql              remix system
0031_content_moderation.sql         moderation system
0032_battle_pass.sql                battle pass
0033_gacha.sql                      gacha system
0034_cloud_orchestrator.sql         cloud orchestrator
0035_mycelium_federation.sql        mycelium federation
0036_cloud_orchestrator_pgcron.sql  orchestrator cron hooks
0039_seasons.sql                    seasons
0040_mneme.sql                      MNEME memory
0041_mneme_rls.sql                  MNEME RLS
0042_lazarus.sql                    Lazarus control plane
```

Apply migrations in filename order. Do not skip numbers just because some are
missing from the sequence; apply the files that exist in lexical order.

## Applying The Schema

Option A: Supabase CLI from a linked project:

```powershell
supabase db push
```

Option B: SQL editor:

1. Open the Supabase project SQL editor.
2. Paste migration files in filename order.
3. Apply seed or verification files only when appropriate for the target.

Option C: direct `psql` when `SUPABASE_DB_URL` is available:

```powershell
psql "$env:SUPABASE_DB_URL" -f migrations/0001_initial.sql
psql "$env:SUPABASE_DB_URL" -f migrations/0002_rls_policies.sql
```

Continue in filename order. Do not paste secrets into chat or docs.

## Verification Files

Use verification SQL after schema changes:

```text
verify.sql
verify-signaling.sql
verify-cocreative.sql
verify-game-state.sql
verify-mneme.sql
```

The original README states that `verify.sql` raises an exception on failed
assertions and prints a success message when all assertions pass.

There is not yet a dedicated `verify-lazarus.sql` in this directory at handoff
time. For Lazarus, use the `cssl-edge` API tests and production smoke:

```powershell
cd C:\Users\Apocky\source\repos\CSSLv3\cssl-edge
npm run test:lazarus
```

## Lazarus Schema: `0042_lazarus.sql`

This migration creates the autonomous coding runner control-plane tables.

Tables:

```text
public.lazarus_runner
  Runner heartbeat, capabilities, status, current run, and metadata.

public.lazarus_task
  Admin-created work queue with prompt, repo path, model mode, cost ceiling,
  sensorium/playtest flags, status, and metadata.

public.lazarus_run
  One leased execution attempt for a task and runner.

public.lazarus_event
  Append-only run event stream for runner logs and progress.

public.lazarus_approval
  Hard gates for destructive, cost, network, PRIME-adjacent, or hardware actions.

public.lazarus_artifact
  Diff/log/screenshot/trace/report artifact metadata.

public.lazarus_fleet_config
  Privacy class, default model mode, budget, and review policy.
```

Important constraints:

```text
lazarus_runner.id shape: ^[A-Za-z0-9_.:-]{1,96}$
lazarus_task.title length: 3..160
lazarus_task.prompt length: 8..65536
model_mode: deepseek-v4-pro, deepseek-v4-flash, reviewer, stub-safe
task status: queued, leased, running, blocked, completed, failed, cancelled
runner status: online, offline, revoked
approval status: pending, approved, denied, expired
event level: info, warn, error, debug
```

Approval gates:

```text
git.push
git.destructive
fs.bulk_delete
network.unknown_egress
cost.overrun
mneme.standing_write
system.driver_or_setting
prime.sigma.capability_sensitive
hardware.mutation
```

RLS is enabled on every Lazarus table. Service role policies allow `service_role`
to operate on the control plane. Browser clients must go through
`cssl-edge/pages/api/admin/lazarus/*` and must not receive service-role access.

Default fleet row inserted by the migration:

```text
id: default
privacy_class: secret-ok
default_model_mode: deepseek-v4-pro
max_cost_usd_per_run: 2.0000
review_required: true
metadata: { "reviewer": "cross-vendor", "workspace": "LoA v14" }
```

## cssl-edge Lazarus Integration Points

The app-side store is:

```text
..\cssl-edge\lib\lazarus\store.ts
```

It uses Supabase service-role backing when configured, otherwise in-memory stub
mode. The UI and APIs are:

```text
..\cssl-edge\pages\admin\lazarus.tsx
..\cssl-edge\pages\api\admin\lazarus\*.ts
```

Expected security behavior:

```text
Admin reads require signed-in admin email.
Runner writes require LAZARUS_RUNNER_TOKEN.
Missing runner token env returns 503.
Wrong runner token returns 401.
Unauthenticated admin reads return 401.
```

## Base Schema Summary

The existing README describes the original core tables:

```text
public.assets            cached asset metadata, public read, service-role write
public.scenes            player-saved scene graphs, own/private or public share
public.history           opt-in text-to-scene corpus
public.companion_logs    append-only PRIME-directive audit trail
```

Storage buckets:

```text
assets       public assets, source/source_id naming
screenshots  mixed own/public depending on scene visibility
audio        private user recordings
```

## Agent Safety Notes

1. Never expose `SUPABASE_SERVICE_ROLE_KEY` to browser code.
2. Do not loosen RLS to make a front-end feature work. Add a server API route.
3. Do not paste production DB URLs or keys into docs or chat.
4. Run verification SQL after schema edits when DB access is available.
5. Run `npm run test:lazarus` in `cssl-edge` after changes to Lazarus tables or
   route contracts.
6. Treat `0042_lazarus.sql` as the source of truth for Lazarus persistence.

## Completion Standard

For database work, completion means:

1. Migration is ordered and idempotent where possible.
2. RLS is enabled and intentional.
3. Browser access goes through `cssl-edge`, not direct service-role access.
4. Verification SQL or app tests run.
5. Docs mention new tables, policies, and env requirements.

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

---

## Multiplayer Signaling

Wave 4 adds signaling primitives for peer-to-peer multiplayer. Real
WebRTC media never traverses Supabase; only **discovery** (find a room)
and **signaling** (exchange SDP offers/answers and ICE candidates) flow
through these tables. The replicated state-snapshot table is a
convenience for small authoritative data (lobby readiness, host-side
authoritative scene state) — not for high-rate game-state replication.

### Layout

```
cssl-supabase/
  migrations/
    0004_signaling.sql        -- 4 tables · 3 helper functions · indexes
    0005_signaling_rls.sql    -- 10 policies + current_user_id() helper
    0006_signaling_seed.sql   -- DEMO01 room · 3 peers · 4 messages
  verify-signaling.sql        -- post-migration assertions for 0004-0006
```

### Apply

Append to your existing apply pipeline; nothing in 0004-0006 modifies
0001-0003.

```bash
# Option A: Supabase CLI (continuing from "Apply the schema" above)
psql "$SUPABASE_DB_URL" -f cssl-supabase/migrations/0004_signaling.sql
psql "$SUPABASE_DB_URL" -f cssl-supabase/migrations/0005_signaling_rls.sql
psql "$SUPABASE_DB_URL" -f cssl-supabase/migrations/0006_signaling_seed.sql
psql "$SUPABASE_DB_URL" -f cssl-supabase/verify-signaling.sql
```

### Schema at a glance

| Table                          | Purpose                                                             |
|--------------------------------|---------------------------------------------------------------------|
| `public.multiplayer_rooms`     | Discovery rooms keyed by short shareable code (e.g. `DEMO01`)       |
| `public.room_peers`            | Per-room peer membership and presence (`last_seen_at`)              |
| `public.signaling_messages`    | WebRTC offer/answer/ICE envelopes (`to_peer = '*'` for broadcast)   |
| `public.room_state_snapshots`  | Sequential authoritative state snapshots (`seq` strictly increasing) |

Helpers (PL/pgSQL):

| Function                              | Returns       | Purpose                                              |
|---------------------------------------|---------------|------------------------------------------------------|
| `gen_room_code()`                     | `text`        | Random 6-char code from a legibility-friendly alphabet |
| `cleanup_expired_rooms()`             | `bigint`      | Deletes rows where `expires_at <= now()`             |
| `presence_touch(p_room, p_player)`    | `timestamptz` | Refreshes `room_peers.last_seen_at` heartbeat        |
| `current_user_id()`                   | `text`        | `auth.uid()::text`, used by RLS policies             |

### RLS policy summary

```
public.multiplayer_rooms
    SELECT  : is_open = true OR own (host) OR service_role
    INSERT  : authenticated AND host_player_id = current_user_id()
    UPDATE  : host only
    DELETE  : host only

public.room_peers
    SELECT  : peer is in same room as caller (or caller is host)
    INSERT  : authenticated AND player_id = current_user_id() AND room is open / own
    UPDATE  : own row only
    DELETE  : own row OR host

public.signaling_messages
    SELECT  : to_peer = current_user_id() OR to_peer = '*'  AND caller is room peer
    INSERT  : from_peer = current_user_id() AND caller is room peer

public.room_state_snapshots
    SELECT  : caller is room peer
    INSERT  : caller is room peer AND created_by = current_user_id()
    UPDATE  : DENIED (no policy = forbidden)
    DELETE  : service-role only (via parent CASCADE)
```

### Cleanup cron

Rooms auto-expire after 4 hours by default; running `cleanup_expired_rooms()`
periodically keeps the tables small. Wire it via `pg_cron` (Supabase
managed extension):

```sql
-- once, in the SQL editor, as service-role
SELECT cron.schedule(
    'cssl-cleanup-expired-rooms',
    '*/15 * * * *',                       -- every 15 minutes
    $$ SELECT public.cleanup_expired_rooms(); $$
);
```

If you do not have `pg_cron` available, hit the function from a cssl-edge
endpoint on a timer, or from any external scheduler.

### Realtime channel usage (clients)

Supabase Realtime exposes Postgres CDC over WebSocket. A peer subscribes
to its inbox by filtering `signaling_messages` for rows addressed to them
or to `'*'`:

```ts
import { createClient } from "@supabase/supabase-js";
import { Database, peerChannelName, MultiplayerRoom, SignalingMessage } from "./types";

const supabase = createClient<Database>(SUPABASE_URL, SUPABASE_ANON_KEY);

// 1. Host opens a room
const { data: code } = await supabase.rpc("gen_room_code");
const { data: room } = await supabase
    .from("multiplayer_rooms")
    .insert({ code: code!, host_player_id: myPlayerId })
    .select()
    .single<MultiplayerRoom>();

// 2. Host inserts themselves into room_peers (RLS allows host into own room)
await supabase.from("room_peers").insert({
    room_id: room!.id,
    player_id: myPlayerId,
    display_name: "host",
    is_host: true,
});

// 3. Subscribe to inbox = "messages addressed to me OR to '*'"
const inbox = supabase
    .channel(peerChannelName(room!.id, myPlayerId))
    .on(
        "postgres_changes",
        {
            event: "INSERT",
            schema: "public",
            table: "signaling_messages",
            filter: `room_id=eq.${room!.id}`,
        },
        (payload) => {
            const msg = payload.new as SignalingMessage;
            if (msg.to_peer === myPlayerId || msg.to_peer === "*") {
                handleSignal(msg);
            }
        },
    )
    .subscribe();

// 4. Send an offer (RLS verifies from_peer = me AND I'm a room peer)
await supabase.from("signaling_messages").insert({
    room_id: room!.id,
    from_peer: myPlayerId,
    to_peer: peerId,
    kind: "offer",
    payload: { type: "offer", sdp: rtcOffer.sdp },
});

// 5. Heartbeat
setInterval(() => supabase.rpc("presence_touch", {
    p_room: room!.id,
    p_player: myPlayerId,
}), 30_000);
```

### RLS notes

* `current_user_id()` returns `auth.uid()::text` and is `STABLE` so PG
  caches it within a query plan.
* Anonymous (anon-key) clients cannot INSERT rows — every signaling
  policy requires `auth.uid() IS NOT NULL`. Use the anon key only to
  SELECT public room codes for "discover open rooms" UX.
* `signaling_messages` SELECT is gated on **two** conditions: the row
  must be addressed to you (or `'*'`), AND you must already be a member
  of the room. This prevents a passive lurker from harvesting
  broadcasts in a room they have not joined.
* `room_peers` inserts allow joining a room when `is_open = true` OR
  when you are the host. Closing a room (`UPDATE is_open = false`) is
  the host-side gating mechanism.
* The realtime channel itself does **not** enforce RLS automatically —
  RLS gates the table read, but Realtime forwards every CDC event to
  every subscribed client. The client filter in step 3 above still
  receives only the rows it has SELECT permission on, because Realtime
  reapplies RLS on the row before delivery.

### CSSL FFI surface (cssl-edge entry points, signaling)

```cssl
@extern "cssl-edge"
fn supabase_room_create(host_player_id: str, max_peers: i32, is_open: bool) -> Json
@extern "cssl-edge"
fn supabase_room_join(code: str, player_id: str, display_name: str?) -> Json
@extern "cssl-edge"
fn supabase_room_leave(room_id: Uuid, player_id: str) -> bool
@extern "cssl-edge"
fn supabase_room_close(room_id: Uuid) -> bool

@extern "cssl-edge"
fn supabase_signal_send(room_id: Uuid, from_peer: str, to_peer: str,
                        kind: str, payload: Json) -> i64
@extern "cssl-edge"
fn supabase_signal_recv(room_id: Uuid, peer_id: str, since: i64) -> Json

@extern "cssl-edge"
fn supabase_state_snapshot(room_id: Uuid, seq: i64, state: Json) -> i64
@extern "cssl-edge"
fn supabase_state_latest(room_id: Uuid) -> Json

@extern "cssl-edge"
fn supabase_presence_touch(room_id: Uuid, player_id: str) -> Timestamptz
```

### Demo data

`0006_signaling_seed.sql` inserts a single open room with code `DEMO01`,
three peers, and four signaling messages (offer · answer · 2× ICE).
Every seed row is tagged `meta = '{"seed": true}'::jsonb` so cleanup is
a one-liner:

```sql
DELETE FROM public.multiplayer_rooms WHERE meta @> '{"seed": true}'::jsonb;
-- CASCADE removes seeded peers / signals / snapshots
```

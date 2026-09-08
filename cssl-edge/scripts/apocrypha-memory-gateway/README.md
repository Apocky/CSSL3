# Apocrypha read-only memory gateway

This loopback service gives the outbound Qwen worker one bounded HTTP adapter
surface for MemPalace, Brainmonsoon recall, Anamnesis, Graphify, 3MNEME, and
MetaHarness observation. It has no write route and does not accept a backend
method from the caller.

## Contract

The six worker URLs are:

```text
http://127.0.0.1:19127/v1/memory/mempalace
http://127.0.0.1:19127/v1/memory/brainmonsoon
http://127.0.0.1:19127/v1/memory/anamnesis
http://127.0.0.1:19127/v1/memory/graphify
http://127.0.0.1:19127/v1/memory/mneme
http://127.0.0.1:19127/v1/memory/metaharness
```

Every URL accepts only `POST application/json`, a bearer token, and the
existing worker envelope:

```json
{
  "operation": "search",
  "read_only": true,
  "query": "bounded query",
  "limit": 8,
  "tenant_id": "admitted tenant",
  "principal_id": "admitted principal",
  "capability": "apocky_owner_chat",
  "memory_manifest_hash": "64 lowercase hex"
}
```

The closed tenant and capability lists remain mandatory. Owner-only requests
also require an exact `tenant_id:principal_id` scope pair and principal
allowlist match. Dynamic `chaos_tarot_reading` and `apocky_member_chat` members
require a canonical UUID principal plus an exact `tenant_id:capability` scope,
so a member cannot cross between the Chaos, Apocky-member, or owner tenants.
Native request identifiers
bind an opaque digest of tenant, principal, and capability; raw identities are
never placed on child process command lines or returned in records.

Responses contain only bounded `records` with opaque provenance, text, and
allowlisted scalar metadata. They always state `authority: none`,
`execution_authorized: false`, and `read_only: true`. Raw queries, bearer
tokens, capability files, and absolute source paths are not returned or logged.

`GET /health` reports per-faculty states using a representative Apocrypha and
Chaos Tarot recall query. `GET /ready` returns 200 only when at least one
faculty has a verified read-only native or upstream recall surface. Both
endpoints require the same bearer token and loopback caller.

## Native and upstream sources

MemPalace uses the existing native federator's `framed` immutable SQLite
reader. Its gateway policy preserves the native result, candidate, content,
concurrency, and immutable-source bounds while allowing the request deadline to
use the native 30-second hard cap under host-wide CPU or disk pressure.
Graphify uses the native graph organ's sealed JSONL query service.
3MNEME and MetaHarness use the federator's zero-state `observe` command; the
gateway cannot open their sealed capabilities itself. Those child processes
receive closed environments and bounded JSONL over stdin, so private queries
are absent from command lines.

Anamnesis uses a disposable SQLite `mode=ro`/`query_only` reader. Brainmonsoon
is package-hash pinned and recalls one explicitly pinned protected lineage
through managed stdio. Its helper admits only `health`, `status`, and `recall`
and verifies that the protected state tree is byte-identical before and after.
Gateway readiness requires a real lineage recall with admitted records, so a
health-only response cannot claim that the Brainmonsoon corpus is accessible.

Copy `gateway.env.example` into the host's ignored environment store, replace
all placeholder identities, and use a random token of at least 32 bytes. Then:

```powershell
node --import tsx scripts/apocrypha-memory-gateway/server.ts
```

On the production Windows host, `run-production.ps1` loads the ignored shared
environment file and keeps the gateway attached to its supervisor. Install or
refresh that hidden sign-in supervisor with `register-gateway-task.ps1`.

MetaHarness has two direct scheduled tasks managed together by
`metaharness-register-resident-task.ps1`. The observer task runs the pinned
virtual-environment Python module. The separate hidden renewal task invokes the
native capability bootstrap directly every five minutes, renewing the gateway's
exact DPAPI bundle with a fifteen-minute TTL. Its action pins owner `apocky`,
endpoint `http://127.0.0.1:8765/mcp`, and the bundle selected by the admitted
federator configuration. The bearer remains in the current-user environment;
it is never placed in task arguments or output. Renewal repeats indefinitely
while the task exists. Both tasks are pinned to Task Scheduler's root path.
Install verifies renewal before it touches a running observer and restores the
prior task definition and running state if reconciliation fails. `-Operation
Uninstall` prevalidates and snapshots both exact tasks before either is removed;
if removal fails, it restores every task already touched. The credential and
capability bundle remain preserved.

Point every `APOCRYPHA_*_READ_URL` in the worker environment at its gateway URL
and set every corresponding worker token to the gateway token. Rollback is to
stop this process and remove those worker URL/token bindings. No source memory
or derived state is changed.

For member admission, append the active tenant UUID whose database slug is
`apocky-members` to `APOCRYPHA_MEMORY_GATEWAY_ALLOWED_TENANTS`, append
`apocky_member_chat` to the capability list, and append the exact
`tenant_id:apocky_member_chat` dynamic scope. Existing owner and Chaos entries
remain required.

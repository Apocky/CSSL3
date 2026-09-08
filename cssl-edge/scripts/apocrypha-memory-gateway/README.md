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

Responses contain only bounded `records` with opaque provenance, text, and
allowlisted scalar metadata. They always state `authority: none`,
`execution_authorized: false`, and `read_only: true`. Raw queries, bearer
tokens, capability files, and absolute source paths are not returned or logged.

`GET /health` reports truthful per-faculty states. `GET /ready` returns 200 only
when at least one faculty has a verified read-only native or upstream health
surface. Both endpoints require the same bearer token and loopback caller.

## Native and upstream sources

MemPalace uses the existing native federator's `framed` immutable SQLite
reader. Graphify uses the native graph organ's sealed JSONL query service.
3MNEME and MetaHarness use the federator's zero-state `observe` command; the
gateway cannot open their sealed capabilities itself. Those child processes
receive closed environments and bounded JSONL over stdin, so private queries
are absent from command lines.

Anamnesis and Brainmonsoon require an already-running read-only loopback HTTP
surface. This is intentional: the current federation and Brainmonsoon service
construct derived state during initialization or analysis, which would violate
this gateway's no-write boundary. Their gateway routes remain present and
truthfully report `unconfigured` until those read-only upstreams are supplied.

Copy `gateway.env.example` into the host's ignored environment store, replace
all placeholder identities, and use a random token of at least 32 bytes. Then:

```powershell
node --import tsx scripts/apocrypha-memory-gateway/server.ts
```

Point every `APOCRYPHA_*_READ_URL` in the worker environment at its gateway URL
and set every corresponding worker token to the gateway token. Rollback is to
stop this process and remove those worker URL/token bindings. No source memory
or derived state is changed.

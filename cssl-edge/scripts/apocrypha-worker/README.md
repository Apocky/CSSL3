# Apocrypha outbound Qwen worker

This resident worker gives Apocky.com and Chaos Tarot one durable local inference rail without exposing the local machine to inbound traffic. It polls the HTTPS control plane, claims one fenced attempt, retrieves admitted read-only context, streams the exact accepted Qwen profile, and commits Qwen's answer as the primary revision.

## Production contract

- Control plane: `APOCRYPHA_CONTROL_PLANE_URL`, normally `https://www.apocky.com`.
- Authentication: `Authorization: Bearer APOCRYPHA_WORKER_TOKEN`; the token stays on the resident host and is never written to logs, Git, or Vercel. The edge validates only its issued shape, while Supabase is the sole authority that verifies its stored hash.
- Node identity: `APOCRYPHA_WORKER_NODE_ID`.
- Model: `qwen35-35b-a3b-q4` at `http://127.0.0.1:19124/v1`.
- Runtime profile SHA-256: `5d390055297aed74dbba092eb313dc8c4bf4e551ca4bf2c50fed16c8cb3a21a9`.
- Claim, lease, chunk, completion, and failure mutations carry the exact `job_id + attempt_id + lease_epoch + lease_token` fence.
- Qwen is the terminal primary answer. Optional frontier corroboration is a separate revision owned by the control plane.

The worker calls only outbound HTTPS control-plane routes:

```text
POST /api/apocrypha/worker/claim
POST /api/apocrypha/worker/lease
POST /api/apocrypha/worker/chunk
POST /api/apocrypha/worker/complete
POST /api/apocrypha/worker/fail
POST /api/apocrypha/worker/heartbeat
```

## Recovery behavior

Before sending a chunk, the worker writes its sequence and content to an atomic AES-256-GCM journal. It removes that entry only after the server acknowledges it. An uncertain response therefore replays the same sequence and bytes, which the server accepts idempotently. Completion or failure is also journaled before delivery.

After a process restart, the worker renews the saved fence, replays unacknowledged writes, and redelivers a pending terminal action. If generation itself was interrupted, it fails that isolated attempt as retryable; it does not pretend to resume a token stream. A stale fence is preserved briefly under `orphaned/` for diagnosis and can never overwrite a newer attempt.

Only one worker process can own a journal directory. `worker.lock` prevents duplicate supervisors.

## Memory and tools

`manifest.production.json` declares the same read-only faculties for both products: MemPalace, Brainmonsoon, Anamnesis, Graphify, MNEME, and MetaHarness. Each adapter has its own HTTPS or loopback endpoint, token, timeout, and content bound. The current host profile admits three concurrent reads after task-shaped recall testing; `APOCRYPHA_MEMORY_READ_CONCURRENCY` remains adjustable from one through six for other hosts. A slow or unavailable faculty appears in provenance as partial availability and cannot block Qwen beyond its declared timeout.

An individual resident can raise a slow local adapter's deadline with
`APOCRYPHA_<ADAPTER>_READ_TIMEOUT_MS` (250 through 60000). This runtime dial
does not change the admitted faculty, authority, response bound, or manifest;
it only allows a known local reader enough time to finish.

Resident readiness uses a representative Apocrypha and Chaos Tarot recall
query, so a fast synthetic health lookup cannot conceal a production-query
timeout.

Every heartbeat performs a bounded live Qwen probe. While idle, the worker
also performs all six read-only adapter requests for the primary and every
additional admitted readiness scope at least every 30 seconds.
The control plane receives `qwen_healthy`, `qwen_probe_at`, the six exact
adapter states, and `adapter_probe_at`; the adapter timestamp advances only
after every adapter returned successfully for every configured scope.
Generation heartbeats publish the
bounded generation deadline so readiness can preserve a live long response
without accepting stale idle evidence.

Every retrieval request includes the job's tenant, principal, capability, and memory-manifest hash. Retrieved text is placed in a data-only boundary and cannot grant instructions or tool authority. Brainmonsoon writes and every other mutation remain outside this worker.

## Run and verify

Copy the variable names from `worker.env.example` into the ignored host secret file. Then run from `cssl-edge`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/apocrypha-worker/run-production.ps1 -Mode probe
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/apocrypha-worker/run-production.ps1 -Mode once
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/apocrypha-worker/run-production.ps1 -Mode run
```

The production scheduled task can be installed with `register-worker-task.ps1`; it launches hidden at sign-in, restarts after failure, and still obeys the single-instance lock.

Loopback diagnostics:

```text
GET http://127.0.0.1:19126/health
GET http://127.0.0.1:19126/ready
```

`/health` reports the worker phase, current attempt, journal count, accepted model/profile, manifest versions, adapter availability, and last bounded error. `/ready` also probes Qwen and verifies the model alias.

Tests:

```powershell
node --import tsx tests/apocrypha-worker-http.test.ts
node --import tsx tests/apocrypha-worker.test.ts
node --import tsx tests/apocrypha-worker-recovery.test.ts
npx tsc --noEmit --pretty false
```

The tests cover edge bearer-shape validation, authoritative Supabase token-hash checks and 401 mapping, authenticated claiming, tenant-scoped retrieval, exact model selection, thinking-disabled streaming, bounded chunks, encrypted journaling, uncertain-ack replay, terminal replay, and stale-fence rejection.

import { createServer, type Server } from 'node:http';
import type { AttemptJournal } from './journal';
import type { WorkerConfig, WorkerRuntimeState } from './types';
import type { QwenClient } from './qwen';

export function startHealthServer(
  config: WorkerConfig,
  runtime: WorkerRuntimeState,
  journal: AttemptJournal,
  qwen: QwenClient,
): Server {
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json; charset=utf-8');
    response.setHeader('cache-control', 'no-store');
    if (request.method !== 'GET' || !['/health', '/ready'].includes(request.url ?? '')) {
      response.statusCode = 404;
      response.end(JSON.stringify({ error: 'not_found' }));
      return;
    }
    const pendingJournals = await journal.pendingCount().catch(() => -1);
    const probe = request.url === '/ready'
      ? await qwen.probe().catch((error) => ({ healthy: false, model: config.modelAlias, detail: error instanceof Error ? error.message : 'probe failed' }))
      : null;
    const ready = request.url !== '/ready' || probe?.healthy === true;
    response.statusCode = ready ? 200 : 503;
    response.end(JSON.stringify({
      status: ready ? 'ok' : 'degraded',
      worker: {
        node_id: config.nodeId,
        phase: runtime.phase,
        current_job_id: runtime.currentJobId,
        current_attempt_id: runtime.currentAttemptId,
        started_at: runtime.startedAt,
        last_claim_at: runtime.lastClaimAt,
        last_completion_at: runtime.lastCompletionAt,
        completed_jobs: runtime.completedJobs,
        failed_jobs: runtime.failedJobs,
        recovered_attempts: runtime.recoveredAttempts,
        pending_journals: pendingJournals,
        last_error: runtime.lastError,
      },
      qwen: {
        model_alias: config.modelAlias,
        profile_hash: config.profileHash,
        probe,
      },
      manifests: {
        tool_registry_version: config.toolRegistryVersion,
        memory_manifest_hash: config.memoryManifestHash,
        adapter_states: runtime.adapterStates,
        adapter_probe_at: runtime.adapterProbeAt,
      },
    }));
  });
  server.listen(config.healthPort, config.healthHost);
  return server;
}

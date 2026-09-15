import { execFileSync } from 'node:child_process';
import { createServer, type Server } from 'node:http';
import { controlPlaneMetrics } from './control-plane';
import { currentLogFile, recentEvents } from './log';
import type { AttemptJournal } from './journal';
import type { WorkerConfig, WorkerRuntimeState } from './types';
import type { QwenClient } from './qwen';

const STARTED_MS = Date.now();

// Which code is actually running. Resolved once; a health call must stay cheap.
let checkoutHead: string | null | undefined;
function resolveCheckoutHead(): string | null {
  if (checkoutHead !== undefined) return checkoutHead;
  try {
    checkoutHead = execFileSync('git', ['rev-parse', '--short=12', 'HEAD'], { cwd: process.cwd(), encoding: 'utf8', timeout: 3_000 }).trim();
  } catch {
    checkoutHead = null;
  }
  return checkoutHead;
}

// Names only. The values are the secrets; the names tell a reader what the
// process was configured with, which is what a wrong-env diagnosis needs.
function presentEnvNames(): string[] {
  return Object.keys(process.env).filter((key) => key.startsWith('APOCRYPHA_')).sort();
}

function diagnostics(config: WorkerConfig, runtime: WorkerRuntimeState) {
  return {
    uptime_s: Math.round((Date.now() - STARTED_MS) / 1000),
    pid: process.pid,
    node: process.version,
    cwd: process.cwd(),
    checkout_head: resolveCheckoutHead(),
    log_file: currentLogFile(),
    recent_events: recentEvents(20),
    recent_errors: runtime.recentErrors,
    control_plane: controlPlaneMetrics,
    config: {
      control_plane_url: config.controlPlaneUrl,
      qwen_base_url: config.qwenBaseUrl,
      poll_interval_ms: config.pollIntervalMs,
      heartbeat_interval_ms: config.heartbeatIntervalMs,
      control_plane_timeout_ms: config.controlPlaneTimeoutMs,
      qwen_idle_timeout_ms: config.qwenIdleTimeoutMs,
      qwen_max_runtime_ms: config.qwenMaxRuntimeMs,
      context_window_tokens: config.contextWindowTokens,
      max_output_tokens: config.maxOutputTokens,
      journal_dir: config.journalDir,
    },
    env_present: presentEnvNames(),
  };
}

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
        capability_memory: Object.fromEntries(config.manifest.capabilities.map((capability) => [capability, {
          adapter_states: runtime.capabilityAdapterStates[capability],
          adapter_probe_at: runtime.capabilityAdapterProbeAt[capability] ?? null,
        }])),
      },
      diagnostics: diagnostics(config, runtime),
    }));
  });
  server.listen(config.healthPort, config.healthHost);
  return server;
}

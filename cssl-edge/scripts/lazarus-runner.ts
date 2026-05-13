import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { LazarusRun, LazarusTask } from '../lib/lazarus/types';
import { submitTesseraGoalFromRunner } from '../lib/tessera/runner-client';

interface RunnerConfig {
  controlUrl: string;
  runnerId: string;
  label: string;
  token: string | null;
  once: boolean;
  enableModelCalls: boolean;
  deepseekApiKey: string | null;
  tesseraBridge: boolean;
}

interface LeaseResponse {
  task: LazarusTask | null;
  run: LazarusRun | null;
  error?: string;
}

function loadDotEnvLocal(): void {
  const path = join(process.cwd(), '.env.local');
  if (!existsSync(path)) return;
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq <= 0) continue;
    const key = trimmed.slice(0, eq).trim();
    const raw = trimmed.slice(eq + 1).trim();
    if (process.env[key] !== undefined) continue;
    process.env[key] = raw.replace(/^['"]|['"]$/g, '');
  }
}

function env(name: string): string | null {
  const v = process.env[name];
  return v && v.trim() ? v.trim() : null;
}

function config(): RunnerConfig {
  return {
    controlUrl: env('LAZARUS_CONTROL_URL') ?? 'http://localhost:3000',
    runnerId: env('LAZARUS_RUNNER_ID') ?? `local-${process.env.COMPUTERNAME ?? 'runner'}`.toLowerCase(),
    label: env('LAZARUS_RUNNER_LABEL') ?? 'LoA v14 local Lazarus runner',
    token: env('LAZARUS_RUNNER_TOKEN'),
    once: process.env.LAZARUS_ONCE === '1',
    enableModelCalls: process.env.LAZARUS_ENABLE_MODEL_CALLS === '1',
    deepseekApiKey: env('DEEPSEEK_API_KEY'),
    tesseraBridge: process.env.LAZARUS_TESSERA_BRIDGE === '1',
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function postJson<T>(cfg: RunnerConfig, path: string, body: unknown): Promise<T> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (cfg.token) headers.Authorization = `Bearer ${cfg.token}`;
  let res: Response;
  try {
    res = await fetch(`${cfg.controlUrl}${path}`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });
  } catch (err) {
    throw new Error(
      `Lazarus control plane is unreachable at ${cfg.controlUrl}. Start it in another terminal with: npm run dev`,
      { cause: err },
    );
  }
  const json = await res.json() as T & { error?: string };
  if (!res.ok) throw new Error(json.error ?? `HTTP ${res.status}`);
  return json;
}

async function register(cfg: RunnerConfig): Promise<void> {
  await postJson(cfg, '/api/admin/lazarus/runners', {
    runner_id: cfg.runnerId,
    label: cfg.label,
    capabilities: [
      'fs',
      'grep',
      'git',
      'shell',
      'test',
      'mneme',
      ...(cfg.tesseraBridge ? ['tessera-bridge'] : []),
      'playtest',
      'screenshot',
      'pixdiff',
      'perf',
      'sensorium',
    ],
    metadata: {
      workspace: 'C:\\Users\\Apocky\\source\\repos\\LoA v14',
      model_calls_enabled: cfg.enableModelCalls,
      deepseek_configured: Boolean(cfg.deepseekApiKey),
      tessera_bridge_enabled: cfg.tesseraBridge,
    },
  });
}

async function emit(cfg: RunnerConfig, runId: string, kind: string, message: string, level: 'info' | 'warn' | 'error' = 'info'): Promise<void> {
  await postJson(cfg, '/api/admin/lazarus/events', { run_id: runId, kind, message, level });
}

async function callDeepSeek(cfg: RunnerConfig, task: LazarusTask): Promise<string> {
  if (!cfg.enableModelCalls) {
    throw new Error('LAZARUS_ENABLE_MODEL_CALLS=1 required for live Lazarus model execution. Refusing to return fake runner output.');
  }
  if (!cfg.deepseekApiKey) {
    throw new Error('DEEPSEEK_API_KEY required when LAZARUS_ENABLE_MODEL_CALLS=1');
  }

  const model = task.model_mode === 'deepseek-v4-flash' ? 'deepseek-chat' : 'deepseek-reasoner';
  const res = await fetch('https://api.deepseek.com/chat/completions', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${cfg.deepseekApiKey}`,
    },
    body: JSON.stringify({
      model,
      messages: [
        {
          role: 'system',
          content:
            'You are Lazarus, a LoA v14 coding runner. Return a concise execution plan and never perform destructive actions without approval.',
        },
        { role: 'user', content: task.prompt },
      ],
      stream: false,
    }),
  });
  const json = await res.json() as {
    choices?: Array<{ message?: { content?: string } }>;
    error?: { message?: string };
  };
  if (!res.ok) throw new Error(json.error?.message ?? `DeepSeek HTTP ${res.status}`);
  return json.choices?.[0]?.message?.content ?? 'DeepSeek returned no content';
}

async function processLease(cfg: RunnerConfig, lease: LeaseResponse): Promise<void> {
  if (!lease.task || !lease.run) return;
  const { task, run } = lease;
  await emit(cfg, run.id, 'run.started', `starting ${task.id}`);
  try {
    let summary: string;
    if (cfg.tesseraBridge) {
      const submission = submitTesseraGoalFromRunner(task, run, {
        bridge_enabled: true,
        model_calls_enabled: cfg.enableModelCalls,
        force_dry_run: true,
      });
      for (const event of submission.runner_events) {
        await emit(cfg, run.id, event.kind, event.message, event.level);
      }
      if (!submission.ok) throw new Error(submission.result.summary);
      summary = submission.result.summary;
    } else {
      summary = await callDeepSeek(cfg, task);
    }
    await emit(cfg, run.id, 'model.summary', summary.slice(0, 2000));
    await postJson(cfg, '/api/admin/lazarus/runs', {
      run_id: run.id,
      status: 'completed',
      summary: summary.slice(0, 4000),
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    await emit(cfg, run.id, 'run.failed', msg, 'error');
    await postJson(cfg, '/api/admin/lazarus/runs', { run_id: run.id, status: 'failed', summary: msg });
  }
}

async function main(): Promise<void> {
  loadDotEnvLocal();
  const cfg = config();
  // eslint-disable-next-line no-console
  console.log(`Λ Lazarus runner · ${cfg.runnerId} · ${cfg.controlUrl}`);
  do {
    await register(cfg);
    const lease = await postJson<LeaseResponse>(cfg, '/api/admin/lazarus/lease', { runner_id: cfg.runnerId });
    if (!lease.task) {
      if (cfg.once) break;
      await sleep(5_000);
      continue;
    }
    await processLease(cfg, lease);
    if (cfg.once) break;
  } while (true);
}

void main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error(err);
  process.exit(1);
});

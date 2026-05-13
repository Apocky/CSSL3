import { randomUUID } from 'node:crypto';
import { createClient, type SupabaseClient } from '@supabase/supabase-js';

import {
  LAZARUS_APPROVAL_GATES,
  type CreateLazarusTaskInput,
  type JsonRecord,
  type LazarusApproval,
  type LazarusApprovalGate,
  type LazarusEvent,
  type LazarusEventLevel,
  type LazarusFleetConfig,
  type LazarusHealth,
  type LazarusModelMode,
  type LazarusRun,
  type LazarusRunStatus,
  type LazarusRunner,
  type LazarusTask,
  type LazarusTaskStatus,
  type LazarusToolSpec,
  type LeaseResult,
  type RegisterRunnerInput,
} from './types';

const DEFAULT_REPO = 'C:\\Users\\Apocky\\source\\repos\\LoA v14';

const TOOL_CATALOG: LazarusToolSpec[] = [
  { name: 'loa.run_build', group: 'build', description: 'Run LoA v14 build.bat and capture compiler output.' },
  { name: 'loa.run_tests', group: 'test', description: 'Run project test harness and return pass/fail evidence.' },
  { name: 'loa.run_playtest', group: 'sensorium', description: 'Launch scripted playtest with telemetry capture.' },
  { name: 'loa.capture_screenshot', group: 'vision', description: 'Capture current frame for visual validation.' },
  { name: 'loa.evaluate_image', group: 'vision', description: 'Score screenshot against explicit visual criteria.' },
  { name: 'loa.compare_pixdiff', group: 'vision', description: 'Compare screenshot against golden image.' },
  { name: 'loa.get_frame_stats', group: 'sensorium', description: 'Read frame timing and budget summary.' },
  { name: 'loa.get_gpu_snapshot', group: 'sensorium', description: 'Read GPU/device diagnostics snapshot.' },
  { name: 'loa.get_sensorium_summary', group: 'sensorium', description: 'Summarize frame, input, trace, and screenshot state.' },
  { name: 'loa.replay_input', group: 'input', description: 'Replay a recorded input trace.' },
  { name: 'loa.inject_input', group: 'input', description: 'Inject controlled input into a playtest run.', approval_gate: 'hardware.mutation' },
  { name: 'loa.reload_shader', group: 'engine', description: 'Hot-reload shader source in the running engine.' },
  { name: 'loa.dump_trace', group: 'trace', description: 'Dump current engine trace buffer.' },
  { name: 'loa.summarize_trace', group: 'trace', description: 'Summarize trace buffer into actionable findings.' },
  { name: 'loa.inspect_procgen_region', group: 'engine', description: 'Inspect generated dungeon region data.' },
  { name: 'loa.inspect_npc_lod', group: 'engine', description: 'Inspect NPC LOD and behavior state.' },
  { name: 'loa.inspect_audio_mix', group: 'engine', description: 'Inspect adaptive music and spatial audio mix.' },
  { name: 'mneme.recall', group: 'memory', description: 'Recall standing project memory through MNEME.' },
  { name: 'mneme.remember', group: 'memory', description: 'Write standing memory after approval.', approval_gate: 'mneme.standing_write' },
  { name: 'git.push', group: 'git', description: 'Push committed work to remote.', approval_gate: 'git.push' },
];

interface LazarusMemory {
  tasks: LazarusTask[];
  runs: LazarusRun[];
  runners: LazarusRunner[];
  events: LazarusEvent[];
  approvals: LazarusApproval[];
  fleet: LazarusFleetConfig[];
  nextEventId: number;
}

const globalState = globalThis as typeof globalThis & { __lazarusMemory?: LazarusMemory };

function iso(): string {
  return new Date().toISOString();
}

function id(prefix: string): string {
  return `${prefix}_${randomUUID()}`;
}

function emptyRecord(value: JsonRecord | undefined): JsonRecord {
  return value ?? {};
}

function memory(): LazarusMemory {
  if (!globalState.__lazarusMemory) {
    const now = iso();
    globalState.__lazarusMemory = {
      tasks: [
        {
          id: 'task_bootstrap_lazarus',
          title: 'Bootstrap Lazarus LoA v14 control loop',
          prompt: 'Verify runner registration, lease polling, approval gates, and LoA Sensorium tool catalog.',
          repo_path: DEFAULT_REPO,
          model_mode: 'deepseek-v4-pro',
          cost_ceiling_usd: 2,
          sensorium_enabled: true,
          playtest_enabled: false,
          status: 'queued',
          created_at: now,
          updated_at: now,
          leased_by: null,
          metadata: { seeded: true },
        },
      ],
      runs: [],
      runners: [],
      events: [],
      approvals: [],
      fleet: [
        {
          id: 'default',
          privacy_class: 'secret-ok',
          default_model_mode: 'deepseek-v4-pro',
          max_cost_usd_per_run: 2,
          review_required: true,
          updated_at: now,
          metadata: { reviewer: 'cross-vendor' },
        },
      ],
      nextEventId: 1,
    };
  }
  return globalState.__lazarusMemory;
}

let sbCache: SupabaseClient | null | undefined;

function getLazarusSupabase(): SupabaseClient | null {
  if (sbCache !== undefined) return sbCache;
  const url = process.env.NEXT_PUBLIC_SUPABASE_URL ?? process.env.SUPABASE_URL;
  const serviceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!url || !serviceKey) {
    sbCache = null;
    return null;
  }
  sbCache = createClient(url, serviceKey, { auth: { persistSession: false } });
  return sbCache;
}

export function isLazarusStubMode(): boolean {
  return getLazarusSupabase() === null;
}

function validateModelMode(raw: unknown): LazarusModelMode {
  if (
    raw === 'deepseek-v4-pro' ||
    raw === 'deepseek-v4-flash' ||
    raw === 'reviewer' ||
    raw === 'stub-safe'
  ) {
    return raw;
  }
  return 'deepseek-v4-pro';
}

function validateGate(raw: unknown): LazarusApprovalGate {
  if (typeof raw === 'string' && LAZARUS_APPROVAL_GATES.includes(raw as LazarusApprovalGate)) {
    return raw as LazarusApprovalGate;
  }
  throw new Error(`unknown Lazarus approval gate: ${String(raw)}`);
}

export function listLazarusTools(): LazarusToolSpec[] {
  return TOOL_CATALOG;
}

export async function getLazarusHealth(): Promise<LazarusHealth> {
  const sb = getLazarusSupabase();
  if (!sb) {
    const m = memory();
    return {
      ok: true,
      stub: true,
      task_count: m.tasks.length,
      queued_count: m.tasks.filter((t) => t.status === 'queued').length,
      active_run_count: m.runs.filter((r) => r.status === 'leased' || r.status === 'running').length,
      pending_approval_count: m.approvals.filter((a) => a.status === 'pending').length,
      online_runner_count: m.runners.filter((r) => r.status === 'online').length,
      tool_count: TOOL_CATALOG.length,
    };
  }

  const [tasks, queued, active, approvals, runners] = await Promise.all([
    sb.from('lazarus_task').select('id', { count: 'exact', head: true }),
    sb.from('lazarus_task').select('id', { count: 'exact', head: true }).eq('status', 'queued'),
    sb.from('lazarus_run').select('id', { count: 'exact', head: true }).in('status', ['leased', 'running']),
    sb.from('lazarus_approval').select('id', { count: 'exact', head: true }).eq('status', 'pending'),
    sb.from('lazarus_runner').select('id', { count: 'exact', head: true }).eq('status', 'online'),
  ]);

  for (const result of [tasks, queued, active, approvals, runners]) {
    if (result.error) throw new Error(result.error.message);
  }

  return {
    ok: true,
    stub: false,
    task_count: tasks.count ?? 0,
    queued_count: queued.count ?? 0,
    active_run_count: active.count ?? 0,
    pending_approval_count: approvals.count ?? 0,
    online_runner_count: runners.count ?? 0,
    tool_count: TOOL_CATALOG.length,
  };
}

export async function listTasks(): Promise<{ tasks: LazarusTask[]; stub: boolean }> {
  const sb = getLazarusSupabase();
  if (!sb) return { tasks: [...memory().tasks].sort((a, b) => b.created_at.localeCompare(a.created_at)), stub: true };

  const { data, error } = await sb.from('lazarus_task').select('*').order('created_at', { ascending: false }).limit(100);
  if (error) throw new Error(error.message);
  return { tasks: (data ?? []) as LazarusTask[], stub: false };
}

export async function createTask(input: CreateLazarusTaskInput): Promise<{ task: LazarusTask; stub: boolean }> {
  const title = input.title.trim();
  const prompt = input.prompt.trim();
  if (title.length < 3) throw new Error('title must be at least 3 characters');
  if (prompt.length < 8) throw new Error('prompt must be at least 8 characters');
  const now = iso();
  const task: LazarusTask = {
    id: id('task'),
    title,
    prompt,
    repo_path: input.repo_path?.trim() || DEFAULT_REPO,
    model_mode: validateModelMode(input.model_mode),
    cost_ceiling_usd: input.cost_ceiling_usd ?? 2,
    sensorium_enabled: input.sensorium_enabled ?? true,
    playtest_enabled: input.playtest_enabled ?? false,
    status: 'queued',
    created_at: now,
    updated_at: now,
    leased_by: null,
    metadata: emptyRecord(input.metadata),
  };

  const sb = getLazarusSupabase();
  if (!sb) {
    memory().tasks.unshift(task);
    return { task, stub: true };
  }

  const { data, error } = await sb.from('lazarus_task').insert(task).select('*').single();
  if (error) throw new Error(error.message);
  return { task: data as LazarusTask, stub: false };
}

export async function listRunners(): Promise<{ runners: LazarusRunner[]; stub: boolean }> {
  const sb = getLazarusSupabase();
  if (!sb) return { runners: [...memory().runners].sort((a, b) => b.last_seen_at.localeCompare(a.last_seen_at)), stub: true };

  const { data, error } = await sb.from('lazarus_runner').select('*').order('last_seen_at', { ascending: false });
  if (error) throw new Error(error.message);
  return { runners: (data ?? []) as LazarusRunner[], stub: false };
}

export async function registerRunner(input: RegisterRunnerInput): Promise<{ runner: LazarusRunner; stub: boolean }> {
  const now = iso();
  const runner: LazarusRunner = {
    id: input.runner_id?.trim() || id('runner'),
    label: input.label?.trim() || 'local-lazarus-runner',
    status: 'online',
    capabilities: input.capabilities ?? ['fs', 'grep', 'git', 'shell', 'test', 'sensorium'],
    current_run_id: null,
    last_seen_at: now,
    registered_at: now,
    metadata: emptyRecord(input.metadata),
  };

  const sb = getLazarusSupabase();
  if (!sb) {
    const m = memory();
    const i = m.runners.findIndex((r) => r.id === runner.id);
    if (i >= 0) {
      runner.registered_at = m.runners[i]!.registered_at;
      runner.current_run_id = m.runners[i]!.current_run_id;
      m.runners[i] = runner;
    } else {
      m.runners.unshift(runner);
    }
    return { runner, stub: true };
  }

  const { data, error } = await sb
    .from('lazarus_runner')
    .upsert(runner, { onConflict: 'id' })
    .select('*')
    .single();
  if (error) throw new Error(error.message);
  return { runner: data as LazarusRunner, stub: false };
}

export async function leaseNextTask(runner_id: string): Promise<LeaseResult> {
  if (!runner_id.trim()) throw new Error('runner_id required');
  const now = iso();
  const sb = getLazarusSupabase();

  if (!sb) {
    const m = memory();
    const task = m.tasks.find((t) => t.status === 'queued') ?? null;
    if (!task) return { task: null, run: null, stub: true };
    task.status = 'leased';
    task.leased_by = runner_id;
    task.updated_at = now;
    const run: LazarusRun = {
      id: id('run'),
      task_id: task.id,
      runner_id,
      status: 'leased',
      model_mode: task.model_mode,
      started_at: now,
      finished_at: null,
      summary: null,
      cost_usd_estimate: 0,
      metadata: {},
    };
    m.runs.unshift(run);
    const runner = m.runners.find((r) => r.id === runner_id);
    if (runner) {
      runner.current_run_id = run.id;
      runner.last_seen_at = now;
      runner.status = 'online';
    }
    m.events.push({
      id: m.nextEventId++,
      run_id: run.id,
      ts: now,
      level: 'info',
      kind: 'lease.created',
      message: `leased ${task.id} to ${runner_id}`,
      payload: {},
    });
    return { task, run, stub: true };
  }

  const { data: queued, error: qErr } = await sb
    .from('lazarus_task')
    .select('*')
    .eq('status', 'queued')
    .order('created_at', { ascending: true })
    .limit(1)
    .maybeSingle();
  if (qErr) throw new Error(qErr.message);
  if (!queued) return { task: null, run: null, stub: false };

  const task = queued as LazarusTask;
  const { data: updatedTask, error: uErr } = await sb
    .from('lazarus_task')
    .update({ status: 'leased' satisfies LazarusTaskStatus, leased_by: runner_id, updated_at: now })
    .eq('id', task.id)
    .eq('status', 'queued')
    .select('*')
    .single();
  if (uErr) throw new Error(uErr.message);

  const run: LazarusRun = {
    id: id('run'),
    task_id: task.id,
    runner_id,
    status: 'leased',
    model_mode: task.model_mode,
    started_at: now,
    finished_at: null,
    summary: null,
    cost_usd_estimate: 0,
    metadata: {},
  };
  const { data: insertedRun, error: rErr } = await sb.from('lazarus_run').insert(run).select('*').single();
  if (rErr) throw new Error(rErr.message);
  const { error: runnerErr } = await sb
    .from('lazarus_runner')
    .update({ current_run_id: run.id, last_seen_at: now, status: 'online' })
    .eq('id', runner_id);
  if (runnerErr) throw new Error(runnerErr.message);
  await recordEvent(run.id, 'info', 'lease.created', `leased ${task.id} to ${runner_id}`, {});

  return { task: updatedTask as LazarusTask, run: insertedRun as LazarusRun, stub: false };
}

export async function listRuns(): Promise<{ runs: LazarusRun[]; stub: boolean }> {
  const sb = getLazarusSupabase();
  if (!sb) return { runs: [...memory().runs].sort((a, b) => b.started_at.localeCompare(a.started_at)), stub: true };

  const { data, error } = await sb.from('lazarus_run').select('*').order('started_at', { ascending: false }).limit(100);
  if (error) throw new Error(error.message);
  return { runs: (data ?? []) as LazarusRun[], stub: false };
}

export async function listEvents(run_id?: string): Promise<{ events: LazarusEvent[]; stub: boolean }> {
  const sb = getLazarusSupabase();
  if (!sb) {
    const rows = run_id ? memory().events.filter((e) => e.run_id === run_id) : memory().events;
    return { events: [...rows].sort((a, b) => a.id - b.id), stub: true };
  }

  let query = sb.from('lazarus_event').select('*').order('id', { ascending: true }).limit(500);
  if (run_id) query = query.eq('run_id', run_id);
  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return { events: (data ?? []) as LazarusEvent[], stub: false };
}

export async function recordEvent(
  run_id: string,
  level: LazarusEventLevel,
  kind: string,
  message: string,
  payload: JsonRecord = {},
): Promise<{ event: LazarusEvent; stub: boolean }> {
  if (!run_id.trim()) throw new Error('run_id required');
  if (!kind.trim()) throw new Error('kind required');
  if (!message.trim()) throw new Error('message required');
  const sb = getLazarusSupabase();
  const event: LazarusEvent = {
    id: 0,
    run_id,
    ts: iso(),
    level,
    kind,
    message,
    payload,
  };

  if (!sb) {
    const m = memory();
    event.id = m.nextEventId++;
    m.events.push(event);
    return { event, stub: true };
  }

  const { data, error } = await sb
    .from('lazarus_event')
    .insert({ run_id, ts: event.ts, level, kind, message, payload })
    .select('*')
    .single();
  if (error) throw new Error(error.message);
  return { event: data as LazarusEvent, stub: false };
}

export async function finishRun(
  run_id: string,
  status: Extract<LazarusRunStatus, 'completed' | 'failed' | 'cancelled'>,
  summary: string,
): Promise<{ run: LazarusRun; stub: boolean }> {
  const now = iso();
  const sb = getLazarusSupabase();
  if (!sb) {
    const m = memory();
    const run = m.runs.find((r) => r.id === run_id);
    if (!run) throw new Error(`run not found: ${run_id}`);
    run.status = status;
    run.summary = summary;
    run.finished_at = now;
    const task = m.tasks.find((t) => t.id === run.task_id);
    if (task) {
      task.status = status === 'completed' ? 'completed' : status;
      task.updated_at = now;
    }
    const runner = m.runners.find((r) => r.id === run.runner_id);
    if (runner) runner.current_run_id = null;
    await recordEvent(run_id, status === 'completed' ? 'info' : 'error', `run.${status}`, summary, {});
    return { run, stub: true };
  }

  const { data: runData, error: runErr } = await sb
    .from('lazarus_run')
    .update({ status, summary, finished_at: now })
    .eq('id', run_id)
    .select('*')
    .single();
  if (runErr) throw new Error(runErr.message);
  const run = runData as LazarusRun;
  const { error: taskErr } = await sb.from('lazarus_task').update({ status, updated_at: now }).eq('id', run.task_id);
  if (taskErr) throw new Error(taskErr.message);
  const { error: runnerErr } = await sb
    .from('lazarus_runner')
    .update({ current_run_id: null, last_seen_at: now })
    .eq('id', run.runner_id);
  if (runnerErr) throw new Error(runnerErr.message);
  await recordEvent(run_id, status === 'completed' ? 'info' : 'error', `run.${status}`, summary, {});
  return { run, stub: false };
}

export async function listApprovals(): Promise<{ approvals: LazarusApproval[]; stub: boolean }> {
  const sb = getLazarusSupabase();
  if (!sb) return { approvals: [...memory().approvals].sort((a, b) => b.requested_at.localeCompare(a.requested_at)), stub: true };

  const { data, error } = await sb.from('lazarus_approval').select('*').order('requested_at', { ascending: false }).limit(100);
  if (error) throw new Error(error.message);
  return { approvals: (data ?? []) as LazarusApproval[], stub: false };
}

export async function requestApproval(
  run_id: string,
  gateRaw: unknown,
  reason: string,
  payload: JsonRecord = {},
): Promise<{ approval: LazarusApproval; stub: boolean }> {
  const approval: LazarusApproval = {
    id: id('approval'),
    run_id,
    gate: validateGate(gateRaw),
    status: 'pending',
    requested_at: iso(),
    decided_at: null,
    decided_by: null,
    reason: reason.trim() || 'approval requested',
    payload,
  };
  const sb = getLazarusSupabase();
  if (!sb) {
    memory().approvals.unshift(approval);
    return { approval, stub: true };
  }
  const { data, error } = await sb.from('lazarus_approval').insert(approval).select('*').single();
  if (error) throw new Error(error.message);
  return { approval: data as LazarusApproval, stub: false };
}

export async function decideApproval(
  approval_id: string,
  decision: 'approved' | 'denied',
  decided_by: string,
): Promise<{ approval: LazarusApproval; stub: boolean }> {
  const now = iso();
  const sb = getLazarusSupabase();
  if (!sb) {
    const approval = memory().approvals.find((a) => a.id === approval_id);
    if (!approval) throw new Error(`approval not found: ${approval_id}`);
    approval.status = decision;
    approval.decided_at = now;
    approval.decided_by = decided_by;
    return { approval, stub: true };
  }

  const { data, error } = await sb
    .from('lazarus_approval')
    .update({ status: decision, decided_at: now, decided_by })
    .eq('id', approval_id)
    .eq('status', 'pending')
    .select('*')
    .single();
  if (error) throw new Error(error.message);
  return { approval: data as LazarusApproval, stub: false };
}

export async function listFleetConfig(): Promise<{ fleet: LazarusFleetConfig[]; stub: boolean }> {
  const sb = getLazarusSupabase();
  if (!sb) return { fleet: memory().fleet, stub: true };

  const { data, error } = await sb.from('lazarus_fleet_config').select('*').order('updated_at', { ascending: false });
  if (error) throw new Error(error.message);
  return { fleet: (data ?? []) as LazarusFleetConfig[], stub: false };
}

export function resetLazarusMemoryForTests(): void {
  globalState.__lazarusMemory = undefined;
  sbCache = undefined;
}

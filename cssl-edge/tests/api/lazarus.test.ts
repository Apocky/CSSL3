import type { NextApiRequest, NextApiResponse } from 'next';

import healthHandler from '@/pages/api/admin/lazarus/health';
import tasksHandler from '@/pages/api/admin/lazarus/tasks';
import runnersHandler from '@/pages/api/admin/lazarus/runners';
import leaseHandler from '@/pages/api/admin/lazarus/lease';
import eventsHandler from '@/pages/api/admin/lazarus/events';
import runsHandler from '@/pages/api/admin/lazarus/runs';
import approvalsHandler from '@/pages/api/admin/lazarus/approvals';
import toolsHandler from '@/pages/api/admin/lazarus/tools';
import fleetHandler from '@/pages/api/admin/lazarus/fleet';
import { resetLazarusMemoryForTests } from '@/lib/lazarus/store';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error(`assert failed: ${msg}`);
}

function mockReqRes(
  method: string,
  query: Record<string, string> = {},
  body: unknown = undefined,
  headers: Record<string, string> = {},
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query, headers, body } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(k: string, v: string) { out.headers[k] = v; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

async function call(
  handler: (req: NextApiRequest, res: NextApiResponse) => unknown | Promise<unknown>,
  method: string,
  body: unknown = undefined,
  query: Record<string, string> = {},
  headers: Record<string, string> = {},
): Promise<MockedResponse> {
  const { req, res, out } = mockReqRes(method, query, body, headers);
  await handler(req, res);
  return out;
}

const ADMIN_HEADERS = { 'x-apocky-test-admin-email': 'apocky13@gmail.com' };
const RUNNER_HEADERS = { authorization: 'Bearer test-runner-token' };

function configureAuth(): void {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  process.env.LAZARUS_RUNNER_TOKEN = 'test-runner-token';
}

export async function testLazarusAuthGates(): Promise<void> {
  resetLazarusMemoryForTests();
  configureAuth();

  const unauthHealth = await call(healthHandler, 'GET');
  assert(unauthHealth.statusCode === 401, 'health rejects unauthenticated admin read');

  const unauthTasks = await call(tasksHandler, 'GET');
  assert(unauthTasks.statusCode === 401, 'tasks reject unauthenticated admin read');

  const badRunner = await call(runnersHandler, 'POST', { runner_id: 'bad-runner' }, {}, { authorization: 'Bearer wrong' });
  assert(badRunner.statusCode === 401, 'runner write rejects wrong token');

  const oldToken = process.env.LAZARUS_RUNNER_TOKEN;
  delete process.env.LAZARUS_RUNNER_TOKEN;
  const missingToken = await call(leaseHandler, 'POST', { runner_id: 'test-runner' }, {}, RUNNER_HEADERS);
  assert(missingToken.statusCode === 503, 'runner write fails closed without configured token');
  process.env.LAZARUS_RUNNER_TOKEN = oldToken;
}

export async function testLazarusHealthAndTools(): Promise<void> {
  resetLazarusMemoryForTests();
  configureAuth();
  const health = await call(healthHandler, 'GET', undefined, {}, ADMIN_HEADERS);
  assert(health.statusCode === 200, 'health 200');
  const h = health.body as { ok?: boolean; stub?: boolean; tool_count?: number };
  assert(h.ok === true, 'health ok');
  assert(h.stub === true, 'stub mode without service role');
  assert(typeof h.tool_count === 'number' && h.tool_count >= 17, 'tool count');

  const tools = await call(toolsHandler, 'GET', undefined, {}, ADMIN_HEADERS);
  const t = tools.body as { tools?: unknown[]; approval_gates?: unknown[] };
  assert(Array.isArray(t.tools) && t.tools.length >= 17, 'tools listed');
  assert(Array.isArray(t.approval_gates) && t.approval_gates.includes('git.push'), 'approval gate listed');

  const fleet = await call(fleetHandler, 'GET', undefined, {}, ADMIN_HEADERS);
  const f = fleet.body as { fleet?: unknown[] };
  assert(Array.isArray(f.fleet) && f.fleet.length >= 1, 'fleet listed');
}

export async function testLazarusTaskLeaseEventRun(): Promise<void> {
  resetLazarusMemoryForTests();
  configureAuth();
  const create = await call(tasksHandler, 'POST', {
    title: 'Renderer telemetry slice',
    prompt: 'Wire LoA v14 renderer telemetry into Lazarus Sensorium summary.',
  }, {}, ADMIN_HEADERS);
  assert(create.statusCode === 201, 'task created');
  const taskId = (create.body as { task: { id: string } }).task.id;
  assert(taskId.startsWith('task_'), 'task id shape');

  const runner = await call(runnersHandler, 'POST', {
    runner_id: 'test-runner',
    label: 'test runner',
    capabilities: ['test', 'sensorium'],
  }, {}, RUNNER_HEADERS);
  assert(runner.statusCode === 200, 'runner registered');

  const lease = await call(leaseHandler, 'POST', { runner_id: 'test-runner' }, {}, RUNNER_HEADERS);
  assert(lease.statusCode === 200, 'lease 200');
  const body = lease.body as { task: { id: string } | null; run: { id: string } | null };
  assert(body.task !== null, 'lease task');
  assert(body.run !== null, 'lease run');

  const event = await call(eventsHandler, 'POST', {
    run_id: body.run!.id,
    kind: 'test.marker',
    message: 'marker event',
    level: 'info',
  }, {}, RUNNER_HEADERS);
  assert(event.statusCode === 201, 'event created');

  const finish = await call(runsHandler, 'POST', {
    run_id: body.run!.id,
    status: 'completed',
    summary: 'test run complete',
  }, {}, RUNNER_HEADERS);
  assert(finish.statusCode === 200, 'run finished');

  const list = await call(tasksHandler, 'GET', undefined, {}, ADMIN_HEADERS);
  const tasks = (list.body as { tasks: Array<{ id: string; status: string }> }).tasks;
  const created = tasks.find((t) => t.id === taskId);
  assert(created?.status === 'completed', 'task completed');
}

export async function testLazarusApprovals(): Promise<void> {
  resetLazarusMemoryForTests();
  configureAuth();
  await call(runnersHandler, 'POST', { runner_id: 'approval-runner' }, {}, RUNNER_HEADERS);
  const lease = await call(leaseHandler, 'POST', { runner_id: 'approval-runner' }, {}, RUNNER_HEADERS);
  const runId = (lease.body as { run: { id: string } | null }).run?.id;
  assert(typeof runId === 'string', 'run id leased');

  const request = await call(approvalsHandler, 'POST', {
    run_id: runId,
    gate: 'git.push',
    reason: 'verify push gate',
  }, {}, ADMIN_HEADERS);
  assert(request.statusCode === 201, 'approval requested');
  const approvalId = (request.body as { approval: { id: string } }).approval.id;

  const decide = await call(approvalsHandler, 'POST', {
    action: 'decide',
    approval_id: approvalId,
    decision: 'approved',
    decided_by: 'test',
  }, {}, ADMIN_HEADERS);
  assert(decide.statusCode === 200, 'approval decided');
  const approval = (decide.body as { approval: { status: string } }).approval;
  assert(approval.status === 'approved', 'approval approved');
}

async function runAll(): Promise<void> {
  await testLazarusAuthGates();
  await testLazarusHealthAndTools();
  await testLazarusTaskLeaseEventRun();
  await testLazarusApprovals();
  // eslint-disable-next-line no-console
  console.log('lazarus.test : OK · 4 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  void runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}

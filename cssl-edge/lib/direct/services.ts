// The PC's Apocrypha services, as the desktop app's control panel sees them: up/down by port, and
// a hidden restart for each. Restarts never open a window (owner rule: no visible consoles) --
// every child is spawned with windowsHide and the long-running ones through run-hidden.py.

import { execFile, spawn } from 'node:child_process';
import { connect } from 'node:net';

const EDGE = 'C:\\Users\\Apocky\\source\\repos\\CSSLv3-wt-worklane-prod\\cssl-edge';
const RUN_HIDDEN = 'C:\\Users\\Apocky\\source\\repos\\apocrypha-core\\tools\\run-hidden.py';
const PYTHONW = 'C:\\Python314\\pythonw.exe';
const RUNTIME_ENV = 'C:\\Apocrypha\\apocrypha-runtime.env';

type Restart =
  | { kind: 'task'; task: string }
  | { kind: 'hidden'; log: string; args: string[] }
  | { kind: 'none' };

export interface ServiceSpec {
  readonly key: string;
  readonly name: string;
  readonly what: string;
  readonly port: number;
  readonly restart: Restart;
}

export const SERVICES: readonly ServiceSpec[] = [
  { key: 'engine', name: 'Local engine', what: 'The free local model (llama.cpp)', port: 19128, restart: { kind: 'task', task: 'Apocrypha-Qwen35-Vulkan' } },
  { key: 'worker', name: 'Worker', what: 'Answers queued messages, with memory and tools', port: 19126, restart: { kind: 'task', task: 'Apocrypha Outbound Worker' } },
  { key: 'memory', name: 'Memory gateway', what: 'Recall across mempalace, ledger, sessions and more', port: 19127, restart: { kind: 'task', task: 'Apocrypha Memory Gateway' } },
  { key: 'recall', name: 'UniRecall', what: 'Federated memory search', port: 19129, restart: { kind: 'none' } },
  { key: 'mind', name: 'Mind', what: 'Persona + memory in front of the engine', port: 19132, restart: { kind: 'hidden', log: 'C:\\Apocrypha\\diagnostics\\apocrypha-mind.log', args: ['node.exe', `--env-file=${RUNTIME_ENV}`, '--import', 'tsx', 'scripts\\apocrypha-mind\\server.ts'] } },
  { key: 'loop', name: 'Room loop', what: 'Presence and unprompted speech (free model only)', port: 19134, restart: { kind: 'hidden', log: 'C:\\Apocrypha\\diagnostics\\room-site-loop.log', args: ['node.exe', `--env-file=${RUNTIME_ENV}`, '--import', 'tsx', 'scripts/apocrypha-room/loop.ts'] } },
  { key: 'mempalace', name: 'MemPalace', what: 'Long-term memory store', port: 8766, restart: { kind: 'none' } },
];

export function portOpen(port: number, timeoutMs = 600): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = connect({ host: '127.0.0.1', port });
    const done = (up: boolean) => { socket.destroy(); resolve(up); };
    socket.setTimeout(timeoutMs, () => done(false));
    socket.once('connect', () => done(true));
    socket.once('error', () => done(false));
  });
}

export async function serviceStatus(): Promise<Array<{ key: string; name: string; what: string; port: number; up: boolean; restartable: boolean }>> {
  return Promise.all(SERVICES.map(async (s) => ({
    key: s.key, name: s.name, what: s.what, port: s.port, up: await portOpen(s.port), restartable: s.restart.kind !== 'none',
  })));
}

function run(file: string, args: string[]): Promise<string> {
  return new Promise((resolve) => {
    execFile(file, args, { windowsHide: true, timeout: 15_000 }, (_error, stdout, stderr) => resolve(`${stdout}${stderr}`));
  });
}

/** The pid listening on a port, from netstat (no window). */
async function listener(port: number): Promise<number | null> {
  const out = await run('netstat.exe', ['-ano', '-p', 'TCP']);
  for (const line of out.split(/\r?\n/)) {
    const cols = line.trim().split(/\s+/);
    if (cols.length >= 5 && cols[1]?.endsWith(`:${port}`) && cols[3] === 'LISTENING') {
      const pid = Number(cols[4]);
      if (Number.isInteger(pid) && pid > 0) return pid;
    }
  }
  return null;
}

/** Stop whatever holds the port, then start the service again, hidden. */
export async function restartService(key: string): Promise<{ ok: boolean; detail: string }> {
  const spec = SERVICES.find((s) => s.key === key);
  if (!spec || spec.restart.kind === 'none') return { ok: false, detail: 'that service cannot be restarted from here' };
  const pid = await listener(spec.port);
  if (pid !== null) await run('taskkill.exe', ['/PID', String(pid), '/F']);
  await new Promise((r) => setTimeout(r, 2_000));
  if (spec.restart.kind === 'task') {
    await run('schtasks.exe', ['/run', '/tn', spec.restart.task]);
  } else {
    const child = spawn(PYTHONW, [RUN_HIDDEN, '--log', spec.restart.log, '--cwd', EDGE, '--', ...spec.restart.args], {
      windowsHide: true, detached: true, stdio: 'ignore',
    });
    child.unref();
  }
  for (let i = 0; i < 30; i += 1) {
    if (await portOpen(spec.port)) return { ok: true, detail: `${spec.name} is back on port ${spec.port}` };
    await new Promise((r) => setTimeout(r, 2_000));
  }
  return { ok: false, detail: `${spec.name} did not come back within a minute` };
}

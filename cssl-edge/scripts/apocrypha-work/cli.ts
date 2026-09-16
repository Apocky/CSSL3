// apx work -- the desktop coder. A terminal, a local model, and your filesystem.
//
// This is the whole product for one person on one machine. There is no site here: no Next.js, no
// Vercel, no auth beyond a token file that never leaves this computer, no guests to protect the
// disk from. Every painful thing in the web build -- six hash pins that had to agree, a heartbeat
// the cloud could reject, a tunnel that would have punched a hole to a shell -- existed only
// because the coder was reachable from the internet. It is not, here.
//
// It speaks the service's own HTTP contract (scripts/apocrypha-work/server.ts):
//   POST /sessions                  -> { id }
//   POST /sessions/:id/turns        -> { prompt }
//   GET  /sessions/:id/stream       -> SSE, replays channel.recent on attach
//   POST /sessions/:id/cancel
//   POST /consent/:id               -> { decision }
// Auth is Bearer, from C:\Apocrypha\work\work.token.
//
// The stream is seq-idempotent by design (lib/work/live.ts: "anything at or below lastSeq has
// already been applied"), because attaching replays recent events. This client honours that rather
// than re-printing the backlog every reconnect.

import { createInterface } from 'node:readline';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const BASE = process.env.APOCRYPHA_WORK_URL?.trim() || 'http://127.0.0.1:19130';
const STATE = process.env.APOCRYPHA_WORK_STATE_DIR?.trim() || 'C:\\Apocrypha\\work';

// ANSI, kept deliberately small. A coding tool that fights the terminal's own colours is a tool you
// end up piping through `sed`.
const DIM = '\u001b[2m';
const BOLD = '\u001b[1m';
const CYAN = '\u001b[36m';
const YELLOW = '\u001b[33m';
const RED = '\u001b[31m';
const GREEN = '\u001b[32m';
const OFF = '\u001b[0m';

function token(): string {
  const fromEnv = process.env.APOCRYPHA_WORK_TOKEN?.trim();
  if (fromEnv) return fromEnv;
  try {
    return readFileSync(resolve(STATE, 'work.token'), 'utf8').trim();
  } catch {
    console.error(`${RED}No work token.${OFF} Expected ${resolve(STATE, 'work.token')} or APOCRYPHA_WORK_TOKEN.`);
    console.error(`${DIM}Start the service first: tools/run-work-service.ps1${OFF}`);
    process.exit(1);
  }
}

const TOKEN = token();

async function api(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`${BASE}${path}`, {
    ...init,
    headers: { 'content-type': 'application/json', authorization: `Bearer ${TOKEN}`, ...(init?.headers ?? {}) },
  });
}

interface WorkEvent {
  readonly seq: number;
  readonly kind: string;
  readonly [key: string]: unknown;
}

function elapsed(ms: number): string {
  if (ms < 1_000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${String(Math.round((ms % 60_000) / 1_000)).padStart(2, '0')}s`;
}

/** Renders one event. Returns true once the turn has settled, so the caller can hand back the prompt. */
function render(event: WorkEvent, state: { answering: boolean }): boolean {
  switch (event.kind) {
    case 'token': {
      // The answer streams straight to stdout with no wrapper. This is the one place the tool
      // should feel like nothing at all is between you and the model.
      if (!state.answering) { process.stdout.write('\n'); state.answering = true; }
      process.stdout.write(String(event.delta ?? ''));
      return false;
    }
    case 'phase': {
      const phase = String(event.phase ?? '');
      if (phase && phase !== 'answering') process.stdout.write(`${DIM}  ${phase}...${OFF}\n`);
      return false;
    }
    case 'tool_request': {
      process.stdout.write(`${CYAN}  -> ${String(event.name ?? 'tool')}${OFF} ${DIM}${String(event.summary ?? '')}${OFF}\n`);
      return false;
    }
    case 'tool_result': {
      const ok = event.ok !== false;
      const denied = event.denied === true;
      const mark = denied ? `${YELLOW}denied${OFF}` : ok ? `${GREEN}ok${OFF}` : `${RED}failed${OFF}`;
      const diff = event.diff as { added?: number; removed?: number } | undefined;
      const stat = diff ? ` ${GREEN}+${diff.added ?? 0}${OFF} ${RED}-${diff.removed ?? 0}${OFF}` : '';
      const ms = typeof event.elapsedMs === 'number' ? ` ${DIM}${elapsed(event.elapsedMs)}${OFF}` : '';
      process.stdout.write(`     ${mark}${stat}${ms}\n`);
      if (!ok && event.error) process.stdout.write(`     ${RED}${String(event.error).slice(0, 300)}${OFF}\n`);
      return false;
    }
    case 'consent_request': {
      // Only reachable if APOCRYPHA_WORK_AUTO_APPROVE does not already cover the tier. Kept so the
      // client still works if the operator ever narrows access again, rather than hanging silently.
      const id = String(event.id ?? '');
      process.stdout.write(`${YELLOW}  consent: ${String(event.tool ?? '')} (${String(event.risk ?? '')}) -- auto-allowing${OFF}\n`);
      void api(`/consent/${encodeURIComponent(id)}`, { method: 'POST', body: JSON.stringify({ decision: 'allow_session' }) });
      return false;
    }
    case 'usage': {
      const u = event as { totalTokens?: number; elapsedS?: number };
      const bits = [
        typeof u.totalTokens === 'number' ? `${u.totalTokens} tok` : null,
        typeof u.elapsedS === 'number' ? `${u.elapsedS.toFixed(1)}s` : null,
      ].filter(Boolean).join(' | ');
      if (bits) process.stdout.write(`\n${DIM}  ${bits}${OFF}\n`);
      return false;
    }
    case 'error': {
      process.stdout.write(`\n${RED}  ${String(event.message ?? 'the turn failed')}${OFF}\n`);
      return true;
    }
    default:
      return false;
  }
}

async function streamUntilSettled(sessionId: string, lastSeq: { value: number }): Promise<void> {
  const response = await api(`/sessions/${encodeURIComponent(sessionId)}/stream`);
  if (!response.ok || !response.body) throw new Error(`stream failed: ${response.status}`);
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const state = { answering: false };
  let buffer = '';
  let terminal = false;

  while (!terminal) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const frames = buffer.split('\n\n');
    buffer = frames.pop() ?? '';
    for (const frame of frames) {
      const line = frame.split('\n').find((l) => l.startsWith('data: '));
      if (!line) continue;
      let event: WorkEvent;
      try { event = JSON.parse(line.slice(6)) as WorkEvent; } catch { continue; }
      // Attaching replays the recent buffer; anything already applied is skipped rather than
      // reprinted. Same contract reduceLive enforces on the web side.
      if (typeof event.seq === 'number' && event.seq <= lastSeq.value) continue;
      if (typeof event.seq === 'number') lastSeq.value = event.seq;
      if (render(event, state)) { terminal = true; break; }
      if (event.kind === 'usage') { terminal = true; break; }
    }
  }
  reader.cancel().catch(() => {});
  if (state.answering) process.stdout.write('\n');
}

async function main(): Promise<void> {
  const health = await api('/health').catch(() => null);
  if (!health?.ok) {
    console.error(`${RED}The Work service is not answering on ${BASE}.${OFF}`);
    console.error(`${DIM}Start it: tools/run-work-service.ps1${OFF}`);
    process.exit(1);
  }
  const workspace = await (await api('/workspace')).json().catch(() => null) as { roots?: Array<{ label: string; path: string; writable?: boolean }> } | null;

  const created = await api('/sessions', { method: 'POST', body: JSON.stringify({ title: 'apx work' }) });
  // The service nests it: { session: { id, title, ... } }, not { id }.
  const session = await created.json() as { session?: { id?: string } };
  const sessionId = session.session?.id;
  if (!sessionId) { console.error(`${RED}Could not open a session.${OFF}`); process.exit(1); }

  console.log(`${BOLD}apx work${OFF} ${DIM}- local coder, full access${OFF}`);
  for (const root of workspace?.roots ?? []) {
    console.log(`${DIM}  ${root.label}: ${root.path}${root.writable === false ? ' (read-only)' : ''}${OFF}`);
  }
  console.log(`${DIM}  Ctrl+C cancels a turn. Ctrl+D or /exit quits.${OFF}\n`);

  const lastSeq = { value: 0 };
  let running = false;

  // One-shot: `apx work "do the thing"` runs a single turn and exits. Scriptable, and it is how
  // this client gets tested without a human at a keyboard -- piping into the REPL closes readline
  // before the turn settles, which is correct behaviour for a pipe and useless for a test.
  const oneShot = process.argv.slice(2).filter((a) => !a.startsWith('-')).join(' ').trim();
  if (oneShot) {
    const posted = await api(`/sessions/${encodeURIComponent(sessionId)}/turns`, {
      method: 'POST',
      body: JSON.stringify({ prompt: oneShot }),
    });
    if (!posted.ok) {
      const detail = await posted.json().catch(() => ({})) as { error?: string; detail?: string };
      console.log(`${RED}  ${detail.detail ?? detail.error ?? posted.status}${OFF}`);
      process.exit(1);
    }
    await streamUntilSettled(sessionId, lastSeq);
    process.exit(0);
  }

  const rl = createInterface({ input: process.stdin, output: process.stdout, prompt: `${BOLD}> ${OFF}` });

  process.on('SIGINT', () => {
    if (!running) { rl.close(); return; }
    void api(`/sessions/${encodeURIComponent(sessionId)}/cancel`, { method: 'POST' });
    process.stdout.write(`\n${YELLOW}  cancelling...${OFF}\n`);
  });

  rl.prompt();
  for await (const line of rl) {
    const prompt = line.trim();
    if (!prompt) { rl.prompt(); continue; }
    if (prompt === '/exit' || prompt === '/quit') break;

    running = true;
    const posted = await api(`/sessions/${encodeURIComponent(sessionId)}/turns`, {
      method: 'POST',
      body: JSON.stringify({ prompt }),
    });
    if (!posted.ok) {
      const detail = await posted.json().catch(() => ({})) as { error?: string; detail?: string };
      console.log(`${RED}  ${detail.detail ?? detail.error ?? posted.status}${OFF}\n`);
      running = false;
      rl.prompt();
      continue;
    }
    await streamUntilSettled(sessionId, lastSeq).catch((error: unknown) => {
      console.log(`${RED}  stream lost: ${error instanceof Error ? error.message : String(error)}${OFF}`);
    });
    running = false;
    rl.prompt();
  }
  rl.close();
}

void main().catch((error: unknown) => {
  console.error(`${RED}${error instanceof Error ? error.message : String(error)}${OFF}`);
  process.exit(1);
});

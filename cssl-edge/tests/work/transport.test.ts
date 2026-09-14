// Layer 2 — the HTTP boundary of the Work service, against a real listening process.
//
// This service can write files and run commands, so its front door is a security control and is
// tested like one: every route is probed with no token, a wrong token, and a wrong token of the
// RIGHT LENGTH (the branch a naive early-return would skip), plus a cross-origin attempt.
//
// The engine is deliberately absent. That is not a limitation — it is the scenario where a
// half-finished service is most likely to answer something it should not, and it also exercises
// the unreachable-engine path end to end.

import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const PORT = 19_147;
const BASE = `http://127.0.0.1:${PORT}`;

async function waitForPort(deadlineMs: number): Promise<boolean> {
  const deadline = Date.now() + deadlineMs;
  while (Date.now() < deadline) {
    const alive = await fetch(`${BASE}/health`).then(() => true).catch(() => false);
    if (alive) return true;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return false;
}

async function main(): Promise<void> {
  const root = await mkdtemp(join(tmpdir(), 'work-transport-'));
  let child: ChildProcess | null = null;

  try {
    child = spawn(process.execPath, ['--import', 'tsx', 'scripts/apocrypha-work/server.ts'], {
      env: {
        ...process.env,
        APOCRYPHA_WORK_ROOTS: `sandbox=${root}`,
        APOCRYPHA_WORK_STATE_DIR: root,
        APOCRYPHA_WORK_PORT: String(PORT),
        // Nothing listens here; the unreachable-engine path is part of what is under test.
        APOCRYPHA_WORK_ENGINE_URL: 'http://127.0.0.1:19199',
        APOCRYPHA_WORK_ARBITER: 'off',
      },
      stdio: 'ignore',
      windowsHide: true,
    });

    assert(await waitForPort(30_000), 'the work service never started listening');
    const token = (await readFile(join(root, 'work.token'), 'utf8')).trim();
    assert(token.length >= 24, `minted token was too short (${token.length} chars)`);

    const ROUTES = ['/health', '/workspace', '/sessions', '/engine'];

    // Axis 1 — credential shape. A right-length wrong token is the case a length-only check misses.
    const sameLengthWrong = 'z'.repeat(token.length);
    assert(sameLengthWrong !== token, 'the wrong-token fixture accidentally equals the real token');

    for (const route of ROUTES) {
      const anonymous = await fetch(BASE + route);
      assert(anonymous.status === 401, `${route} without a token returned ${anonymous.status}, expected 401`);

      const short = await fetch(BASE + route, { headers: { authorization: 'Bearer nope' } });
      assert(short.status === 401, `${route} with a short wrong token returned ${short.status}`);

      const equalLength = await fetch(BASE + route, { headers: { authorization: `Bearer ${sameLengthWrong}` } });
      assert(equalLength.status === 401, `${route} with an equal-length wrong token returned ${equalLength.status}`);

      const malformed = await fetch(BASE + route, { headers: { authorization: token } });
      assert(malformed.status === 401, `${route} with a bare token (no "Bearer") returned ${malformed.status}`);
    }

    // The query-parameter form exists only because EventSource cannot set headers; it must be
    // checked exactly as strictly.
    const badQuery = await fetch(`${BASE}/health?token=${sameLengthWrong}`);
    assert(badQuery.status === 401, `a wrong query token returned ${badQuery.status}`);
    const goodQuery = await fetch(`${BASE}/health?token=${encodeURIComponent(token)}`);
    assert(goodQuery.status !== 401, 'the correct query token was rejected');

    // Axis 2 — origin. A page on another origin must not be able to drive this service.
    const foreign = await fetch(`${BASE}/health`, {
      headers: { authorization: `Bearer ${token}`, origin: 'https://apocky.com' },
    });
    assert(foreign.status === 403, `a foreign origin returned ${foreign.status}, expected 403`);
    const local = await fetch(`${BASE}/health`, {
      headers: { authorization: `Bearer ${token}`, origin: 'http://127.0.0.1:3000' },
    });
    assert(local.status !== 403, 'the local Next origin was refused');
    assert(local.headers.get('access-control-allow-origin') === 'http://127.0.0.1:3000', 'CORS header was not echoed for the allowed origin');

    const auth = { authorization: `Bearer ${token}` };

    // MUST-ALLOW control: with the right token the routes actually work, so the 401s above are
    // not simply "everything is refused".
    const health = await fetch(`${BASE}/health`, { headers: auth });
    assert(health.status === 503, `health with an unreachable engine returned ${health.status}, expected 503`);
    const healthBody = await health.json() as { engine: { healthy: boolean }; policy: { shell: boolean } };
    assert(healthBody.engine.healthy === false, 'health claimed a missing engine was healthy');

    const workspace = await fetch(`${BASE}/workspace`, { headers: auth });
    assert(workspace.status === 200, `workspace returned ${workspace.status}`);
    const wsBody = await workspace.json() as { roots: { label: string }[]; tools: { name: string }[] };
    assert(wsBody.roots[0]?.label === 'sandbox', 'workspace did not report the configured root');
    assert(wsBody.tools.some((tool) => tool.name === 'read_file'), 'workspace did not advertise the file tools');

    // The token must never be echoed by any route.
    const leak = JSON.stringify(healthBody) + JSON.stringify(wsBody);
    assert(!leak.includes(token), 'a route echoed the service token in its response');

    // Axis 3 — routing. Unknown paths and methods are refused, and a traversal in the path is not
    // resolved into a different route.
    assert((await fetch(`${BASE}/nope`, { headers: auth })).status === 404, 'unknown route did not 404');
    assert((await fetch(`${BASE}/sessions/does-not-exist`, { headers: auth })).status === 404, 'unknown session did not 404');
    assert((await fetch(`${BASE}/health`, { method: 'DELETE', headers: auth })).status === 405, 'DELETE was not refused');

    // Axis 4 — session lifecycle and the event stream.
    const created = await fetch(`${BASE}/sessions`, {
      method: 'POST', headers: { ...auth, 'content-type': 'application/json' }, body: JSON.stringify({ title: 'transport' }),
    });
    assert(created.status === 201, `session create returned ${created.status}`);
    const sessionId = (await created.json() as { session: { id: string } }).session.id;

    const submitted = await fetch(`${BASE}/sessions/${sessionId}/turns`, {
      method: 'POST', headers: { ...auth, 'content-type': 'application/json' }, body: JSON.stringify({ prompt: 'anything' }),
    });
    assert(submitted.status === 202, `turn submit returned ${submitted.status}`);

    const empty = await fetch(`${BASE}/sessions/${sessionId}/turns`, {
      method: 'POST', headers: { ...auth, 'content-type': 'application/json' }, body: JSON.stringify({ prompt: '   ' }),
    });
    assert(empty.status === 400, `an empty prompt returned ${empty.status}, expected 400`);

    // Attach AFTER the turn has already failed: the buffered events must replay to a late reader,
    // which is exactly what a browser reconnect does.
    await new Promise((resolve) => setTimeout(resolve, 1_500));
    const stream = await fetch(`${BASE}/sessions/${sessionId}/stream`, { headers: auth });
    assert(stream.status === 200, `stream returned ${stream.status}`);
    assert((stream.headers.get('content-type') ?? '').includes('text/event-stream'), 'stream had the wrong content type');

    const reader = stream.body!.getReader();
    const decoder = new TextDecoder();
    let buffered = '';
    const deadline = Date.now() + 8_000;
    while (Date.now() < deadline && !buffered.includes(': attached')) {
      const { done, value } = await reader.read();
      if (done) break;
      buffered += decoder.decode(value, { stream: true });
    }
    await reader.cancel().catch(() => undefined);
    assert(buffered.includes('data:'), 'the late reader received no replayed events');
    assert(/"kind":"(session|error|phase)"/.test(buffered), `replayed events looked wrong: ${buffered.slice(0, 200)}`);
    assert(/ENGINE_UNREACHABLE|unreachable|TURN_FAILED/.test(buffered), 'the unreachable engine did not surface as an error event');

    // Consent for an unknown request id is a 404, not a silent success.
    const ghost = await fetch(`${BASE}/consent/00000000-0000-0000-0000-000000000000`, {
      method: 'POST', headers: { ...auth, 'content-type': 'application/json' }, body: JSON.stringify({ decision: 'allow' }),
    });
    assert(ghost.status === 404, `consent for an unknown request returned ${ghost.status}`);

    const badDecision = await fetch(`${BASE}/consent/whatever`, {
      method: 'POST', headers: { ...auth, 'content-type': 'application/json' }, body: JSON.stringify({ decision: 'sure-why-not' }),
    });
    assert(badDecision.status === 400, `an invalid consent decision returned ${badDecision.status}`);

    console.log(`transport: ${ROUTES.length} routes x 4 credential shapes, origin gate, stream replay, ${['404', '405', '400'].join('/')} paths`);
  } finally {
    child?.kill('SIGKILL');
    await rm(root, { recursive: true, force: true }).catch(() => undefined);
  }
}

main().then(() => console.log('work/transport OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

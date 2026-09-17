import { timingSafeEqual } from 'node:crypto';
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { WorkAgent } from './agent';
import { EngineArbiter } from './arbiter';
import { loadWorkConfig } from './config';
import { EngineClient } from './engine';
import { log } from './log';
import { SessionStore } from './sessions';
import { TurnRunner } from './runner';
import { Workspace } from './workspace';
import { McpHub } from './tools/mcp';
import { resolveSampling, SAMPLING_PRESETS } from '../../lib/apocrypha/sampling';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

const MAX_BODY_BYTES = 1024 * 1024;
// The Work tab is served by the local Next.js instance; nothing else may drive this service.
const ALLOWED_ORIGINS = new Set(['http://127.0.0.1:3000', 'http://localhost:3000', 'http://127.0.0.1:19130', 'http://localhost:19130']);

function tokenMatches(supplied: string, expected: string): boolean {
  const a = Buffer.from(supplied);
  const b = Buffer.from(expected);
  if (a.length !== b.length) return false;
  return timingSafeEqual(a, b);
}

async function readBody(request: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  let total = 0;
  for await (const chunk of request) {
    total += (chunk as Buffer).byteLength;
    if (total > MAX_BODY_BYTES) throw new Error('request body too large');
    chunks.push(chunk as Buffer);
  }
  if (total === 0) return {};
  const parsed = JSON.parse(Buffer.concat(chunks).toString('utf8')) as unknown;
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('body must be a JSON object');
  return parsed as Record<string, unknown>;
}

function send(response: ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
    'x-content-type-options': 'nosniff',
  });
  response.end(payload);
}

async function main(): Promise<void> {
  const config = loadWorkConfig();
  const lease = new DatabaseSync(join(config.stateDir, 'host-lease.sqlite'));
  try { lease.exec('PRAGMA busy_timeout=0; BEGIN EXCLUSIVE;'); }
  catch {
    lease.close();
    throw new Error('The Work state directory is already owned by another host, or its lease is unavailable.');
  }
  let leaseReleased = false;
  const releaseLease = (): void => {
    if (leaseReleased) return;
    leaseReleased = true;
    lease.close();
  };
  process.once('exit', releaseLease);
  const workspace = await Workspace.open(config.roots);
  const store = await SessionStore.open(config.stateDir);
  const engine = new EngineClient(config.engine);
  const agent = new WorkAgent(config, workspace, engine);
  const runner = new TurnRunner(config, agent, store);

  // MCP starts concurrently with the rest of boot and is never awaited by a request path: the
  // built-in file and shell tools are live from the first instant, and a server that never answers
  // its handshake costs discovery, not availability.
  const mcp = new McpHub();
  const mcpTools = await mcp.start(config.mcpConfigPath);
  if (mcpTools.length > 0) agent.attachMcp(mcp, mcpTools);
  const arbiter = new EngineArbiter({
    mode: config.arbiter.mode,
    enginePort: config.arbiter.enginePort,
    chat: {
      lane: 'chat',
      launcher: config.arbiter.chatLauncher,
      launcherArgs: [],
      label: 'chat model',
      modelPath: config.arbiter.chatModelPath,
    },
    work: {
      lane: 'work',
      launcher: config.arbiter.workLauncher,
      launcherArgs: ['-Profile', config.arbiter.workProfile],
      label: config.engine.alias,
      modelPath: config.arbiter.workModelPath,
    },
    chatWorkerHealthUrl: config.arbiter.chatWorkerHealthUrl,
    drainTimeoutMs: config.arbiter.drainTimeoutMs,
    startTimeoutMs: config.arbiter.startTimeoutMs,
    idleYieldMs: config.arbiter.idleYieldMs,
    launcherLogDir: config.arbiter.launcherLogDir,
  });

  const server = createServer((request, response) => {
    void handle(request, response).catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      log('error', 'work.request.failed', { url: request.url, error: message });
      if (!response.headersSent) send(response, 500, { error: 'internal_error', detail: message.slice(0, 400) });
      else response.end();
    });
  });

  async function handle(request: IncomingMessage, response: ServerResponse): Promise<void> {
    const origin = request.headers.origin;
    if (origin && ALLOWED_ORIGINS.has(origin)) {
      response.setHeader('access-control-allow-origin', origin);
      response.setHeader('access-control-allow-headers', 'authorization, content-type');
      response.setHeader('access-control-allow-methods', 'GET, POST, OPTIONS');
      response.setHeader('vary', 'origin');
    } else if (origin) {
      send(response, 403, { error: 'origin_not_allowed' });
      return;
    }
    if (request.method === 'OPTIONS') { response.writeHead(204); response.end(); return; }
    // Say "that verb is not supported here" rather than letting PUT/DELETE fall through to a 404,
    // which reads as "no such route" and hides the fact that the path exists.
    if (request.method !== 'GET' && request.method !== 'POST') {
      response.setHeader('allow', 'GET, POST, OPTIONS');
      send(response, 405, { error: 'method_not_allowed' });
      return;
    }

    const url = new URL(request.url ?? '/', `http://${config.host}:${config.port}`);

    // The desktop window, served BEFORE the token check. The page is static markup with no data in
    // it; the credential belongs on the API, not on the document. Gating the HTML meant a plain
    // reload returned 401 JSON, so the page's own JS never ran and could not restore the token it
    // had deliberately stripped from the address bar -- a dead window on F5.
    // Safe because the listener is bound to 127.0.0.1 and every endpoint behind it still requires
    // the bearer token.
    if (request.method === 'GET' && (url.pathname === '/' || url.pathname === '/app')) {
      // Read per request, so editing the page is a refresh rather than a service restart.
      const html = readFileSync(join(__dirname, 'ui.html'), 'utf8');
      response.writeHead(200, {
        'content-type': 'text/html; charset=utf-8',
        'cache-control': 'no-store',
        'x-content-type-options': 'nosniff',
      });
      response.end(html);
      return;
    }

    const header = request.headers.authorization ?? '';
    // The stream endpoint is opened by EventSource, which cannot set headers, so it carries the
    // token as a query parameter instead. Same secret, same comparison.
    const supplied = header.startsWith('Bearer ') ? header.slice(7) : (url.searchParams.get('token') ?? '');
    if (!tokenMatches(supplied, config.token)) { send(response, 401, { error: 'unauthorized' }); return; }

    const path = url.pathname.replace(/\/+$/, '') || '/';
    const segments = path.split('/').filter(Boolean);
    const [head, second, third] = segments;

    if (request.method === 'GET' && path === '/health') {
      const probe = await engine.probe();
      send(response, probe.healthy ? 200 : 503, {
        status: probe.healthy ? 'ok' : 'degraded',
        service: 'apocrypha-work',
        lane: 'work',
        engine: { ...probe, base_url: config.engine.baseUrl, alias: config.engine.alias },
        workspace: workspace.list().map((root) => ({ label: root.label, writable: root.writable })),
        policy: {
          shell: config.shellAllowed,
          auto_approve: config.autoApprove,
          max_iterations: config.maxToolIterations,
        },
        mcp: { servers: new Set(mcpTools.map((t) => t.name.split('__')[1])).size, tools: mcpTools.length },
        presets: SAMPLING_PRESETS.map((p) => ({ id: p.id, label: p.label, effect: p.effect, profile: p.profile })),
        sampling: config.engine.sampling,
        active_turns: runner.activeCount(),
        arbiter: await arbiter.status(),
      });
      return;
    }

    if (head === 'engine') {
      if (request.method === 'GET' && segments.length === 1) { send(response, 200, await arbiter.status()); return; }
      if (request.method === 'POST' && second === 'acquire') {
        const body = await readBody(request);
        try {
          send(response, 200, await arbiter.acquire('work', { force: body.force === true }));
        } catch (error) {
          send(response, 409, { error: 'handover_refused', detail: error instanceof Error ? error.message : 'handover failed' });
        }
        return;
      }
      if (request.method === 'POST' && second === 'release') {
        try {
          send(response, 200, await arbiter.release());
        } catch (error) {
          send(response, 409, { error: 'release_failed', detail: error instanceof Error ? error.message : 'release failed' });
        }
        return;
      }
    }

    if (request.method === 'GET' && path === '/workspace') {
      send(response, 200, {
        roots: workspace.list().map((root) => ({ label: root.label, path: root.path, writable: root.writable })),
        tools: agent.tools.map((tool) => ({ name: tool.name, risk: tool.risk, description: tool.description })),
      });
      return;
    }

    if (head === 'sessions' && segments.length === 1) {
      if (request.method === 'GET') { send(response, 200, { sessions: await store.list() }); return; }
      if (request.method === 'POST') {
        const body = await readBody(request);
        send(response, 201, { session: await store.create(String(body.title ?? 'Untitled task')) });
        return;
      }
    }

    if (head === 'sessions' && second !== undefined) {
      const id = second;
      const record = await store.load(id);
      if (!record) { send(response, 404, { error: 'unknown_session' }); return; }

      if (request.method === 'GET' && segments.length === 2) { send(response, 200, record); return; }

      if (request.method === 'GET' && third === 'stream') {
        const cursor = url.searchParams.get('after') ?? request.headers['last-event-id'] ?? '0';
        const after = typeof cursor === 'string' && /^\d+$/.test(cursor) ? Number(cursor) : NaN;
        if (!Number.isSafeInteger(after) || after < 0) { send(response, 400, { error: 'invalid_event_cursor' }); return; }
        await runner.attachStream(id, response, after);
        return;
      }

      if (request.method === 'POST' && third === 'turns') {
        const body = await readBody(request);
        const prompt = String(body.prompt ?? '').trim();
        if (!prompt) { send(response, 400, { error: 'prompt_required' }); return; }
        // In auto mode a Work turn is itself the request for the GPU; in manual mode the operator
        // presses the button and this just keeps the idle-yield clock from firing mid-task.
        if (config.arbiter.mode === 'auto') {
          try {
            await arbiter.acquire('work');
          } catch (error) {
            send(response, 409, { error: 'engine_unavailable', detail: error instanceof Error ? error.message : 'could not take the GPU' });
            return;
          }
        }
        arbiter.touch();
        // Dials for this turn only. Whatever arrives is CLAMPED against the chosen preset, so the
        // window can offer real knobs without a bad value being able to break a turn -- the same
        // never-trust-the-client-never-punish-it rule the chat lane uses.
        const preset = typeof body.preset === 'string' ? body.preset : null;
        // Falls back to the coder's own band, not to the chat default, when only overrides are sent.
        const sampling = (preset || body.sampling)
          ? resolveSampling(preset ?? 'precise', body.sampling)
          : undefined;
        const turn = await runner.start(record.session, prompt, sampling);
        send(response, 202, { turn_id: turn.id });
        return;
      }

      if (request.method === 'POST' && third === 'cancel') {
        send(response, 200, { cancelled: runner.cancel(id) });
        return;
      }

      if (request.method === 'POST' && third === 'title') {
        const body = await readBody(request);
        await store.rename(id, String(body.title ?? ''));
        send(response, 200, { ok: true });
        return;
      }
    }

    if (request.method === 'POST' && head === 'consent' && second !== undefined && segments.length === 2) {
      const body = await readBody(request);
      const decision = String(body.decision ?? '');
      if (!['allow', 'allow_session', 'deny'].includes(decision)) { send(response, 400, { error: 'bad_decision' }); return; }
      const resolved = runner.resolveConsent(second, decision as 'allow' | 'allow_session' | 'deny');
      send(response, resolved ? 200 : 404, { resolved });
      return;
    }

    send(response, 404, { error: 'not_found' });
  }

  server.listen(config.port, config.host, () => {
    log('info', 'work.server.listening', {
      host: config.host,
      port: config.port,
      engine: config.engine.baseUrl,
      alias: config.engine.alias,
      roots: config.roots.map((root) => `${root.label}${root.writable ? '' : ':ro'}`),
      shell: config.shellAllowed,
      auto_approve: config.autoApprove,
      token_file: `${config.stateDir}\\work.token`,
    });
  });

  let shuttingDown = false;
  const shutdown = (): void => {
    if (shuttingDown) return;
    shuttingDown = true;
    log('info', 'work.server.stopping', {});
    arbiter.stop();
    server.close();
    void runner.stopAll().then(() => {
      store.close();
      releaseLease();
      process.exitCode = 0;
    }).catch((error) => {
      log('error', 'work.server.shutdown_failed', { error: error instanceof Error ? error.message : String(error) });
      process.exitCode = 1;
    });
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
}

void main().catch((error) => {
  log('error', 'work.server.fatal', { error: error instanceof Error ? error.message : String(error) });
  process.exitCode = 1;
});

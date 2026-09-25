import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdir, readFile } from 'node:fs/promises';
import { dirname, extname, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const app = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dist = resolve(app, 'frontend/dist');
const output = resolve(app, 'target/workbench-qa');
const modulePath = process.argv[2];
if (!modulePath) throw new Error('Pass the path to an existing playwright/index.mjs. This check does not install dependencies.');
const { chromium } = await import(pathToFileURL(resolve(modulePath)).href);
const config = JSON.parse(await readFile(resolve(app, 'tauri.conf.json'), 'utf8'));
await mkdir(output, { recursive: true });
const server = createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url, 'http://127.0.0.1').pathname;
    const file = resolve(dist, `.${pathname === '/' ? '/index.html' : pathname}`);
    if (!file.startsWith(`${dist}${sep}`)) throw new Error('outside asset root');
    const body = await readFile(file);
    response.writeHead(200, {
      'content-type': ({ '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.ico': 'image/x-icon' })[extname(file)] ?? 'application/octet-stream',
      'content-security-policy': config.app.security.csp,
    });
    response.end(body);
  } catch { response.writeHead(404); response.end(); }
});
await new Promise((accept, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', accept); });
const base = `http://127.0.0.1:${server.address().port}`;
let browser;

function installFixture() {
  globalThis.isTauri = true;
  const callbacks = new Map();
  const listeners = new Map();
  let callbackId = 0;
  let epoch = 0;
  let seq = 0;
  let activeSession;
  let finishSend;
  const sessionId = '46ed0eab-9678-4a4a-bfc9-120db71d0ed8';
  const turnId = 'e6bcce20-972b-40d5-86f3-a1b59157ee47';
  const consentId = 'faf06870-937e-4bba-8c96-3b37d1ea8d44';
  const saved = {
    id: sessionId, title: 'Fix the parser', createdAt: '2026-09-16T10:00:00Z', lastActiveAt: '2026-09-16T10:03:00Z', standingGrants: [],
  };
  const savedTurn = {
    id: turnId, sessionId, prompt: 'Fix the parser and run its tests.', phase: 'done', startedAt: '2026-09-16T10:00:00Z',
    output: 'Saved result: the parser was updated.', toolCalls: [{
      id: 'saved-edit', name: 'file_edit', ok: true, elapsedMs: 12, summary: 'Updated the parser', content: 'Changed src/parser.ts',
      diff: { path: 'src/parser.ts', added: 1, removed: 1, patch: '-const broken = true;\n+const broken = false;' },
    }],
  };
  const sessions = [saved];
  function emit(name, payload) {
    for (const listener of listeners.get(name) ?? []) callbacks.get(listener)?.({ event: name, id: listener, payload });
  }
  function event(kind, data, chosenEpoch = epoch) {
    emit('apocrypha://work-event', { session_id: activeSession.id, epoch: chosenEpoch, event: { seq: ++seq, at: '2026-09-16T10:10:00Z', kind, data } });
  }
  globalThis.fixture = {
    calls: [], holdSend: false, failSend: false,
    event,
    connection(status, message = '') { emit('apocrypha://work-stream', { session_id: activeSession.id, epoch, status, message }); },
    finishSend() { finishSend?.({ turn_id: turnId }); },
    approval() { event('consent_request', { id: consentId, turnId, tool: 'shell', risk: 'execute', summary: 'Run parser tests', detail: 'npm test -- parser\nWorking directory: C:/fixture/project' }); },
  };
  globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  globalThis.__TAURI_INTERNALS__ = {
    transformCallback(callback) { callbacks.set(++callbackId, callback); return callbackId; },
    async invoke(command, args = {}) {
      if (command === 'plugin:event|listen') {
        const current = listeners.get(args.event) ?? new Set();
        current.add(args.handler);
        listeners.set(args.event, current);
        return args.handler;
      }
      if (command === 'plugin:event|unlisten') { listeners.get(args.event)?.delete(args.eventId); return; }
      fixture.calls.push({ command, args });
      if (command === 'local_bootstrap') return {
        online: true,
        host: { connection_id: 'fixture-native-connection', service: 'apocrypha-work', endpoint: 'http://127.0.0.1:19130/', state_dir: 'C:/fixture/work' },
        health: {
          service: 'apocrypha-work', status: 'ok', engine: { alias: 'Fixture coder', healthy: true },
          policy: { shell: true, auto_approve: ['read'] }, mcp: { servers: 0, tools: 0 }, active_turns: 0,
          presets: [{ id: 'precise', label: 'Precise', profile: { temperature: 0.3, topP: 0.9, topK: 40, minP: 0.05, repeatPenalty: 1 } }],
          sampling: { temperature: 0.3, topP: 0.9, topK: 40, minP: 0.05, repeatPenalty: 1 },
        },
        workspace: { roots: [{ label: 'Project', path: 'C:/fixture/project', writable: true }], tools: [{ name: 'shell', risk: 'execute', description: 'Run a scoped command' }] },
        sessions, notice: '',
      };
      if (command === 'local_open_session' || command === 'local_new_session') {
        activeSession = command === 'local_open_session' ? saved : {
          ...saved, id: '15365f88-5d66-49db-8809-cf3e58a4599c', title: args.title,
        };
        epoch += 1;
        seq = 0;
        if (!sessions.some((session) => session.id === activeSession.id)) sessions.push(activeSession);
        return { session: activeSession, turns: command === 'local_open_session' ? [savedTurn] : [], epoch, after_seq: 0 };
      }
      if (command === 'local_request') {
        if (args.request.operation === 'subscribe') fixture.connection('connected');
        if (args.request.operation === 'rename') saved.title = args.request.title;
        return { subscribed: true, detached: true, ok: true };
      }
      if (command === 'local_send') {
        if (fixture.failSend) throw new Error('Fixture connection lost after send');
        event('session', { turn_id: '8b17be03-5dcb-4e0f-ae63-8fdb7bd46921', prompt: args.options.prompt });
        event('phase', { phase: 'thinking' });
        if (fixture.holdSend) return new Promise((accept) => { finishSend = accept; });
        return { turn_id: turnId };
      }
      if (command === 'local_consent') {
        event('consent_resolved', { id: args.requestId, decision: args.decision });
        event('phase', { phase: 'thinking' });
        return { resolved: true };
      }
      if (command === 'local_cancel') { event('phase', { phase: 'cancelled', terminal: true }); return { cancelled: true }; }
      throw new Error(`Unexpected IPC command ${command}`);
    },
  };
}

async function visibleText(page, text) { await page.getByText(text, { exact: true }).first().waitFor(); }

async function geometry(page, name) {
  const failures = await page.evaluate(() => {
    const visible = (element) => element.getClientRects().length && getComputedStyle(element).visibility !== 'hidden';
    const failures = [];
    if (document.documentElement.scrollWidth > innerWidth + 1) failures.push('document width overflow');
    if (document.documentElement.scrollHeight > innerHeight + 1) failures.push('document height overflow');
    const controls = [...document.querySelectorAll('button, input, select, textarea, summary')].filter(visible);
    for (const control of controls) {
      const rect = control.getBoundingClientRect();
      if (rect.left < -1 || rect.right > innerWidth + 1) failures.push(`${control.getAttribute('aria-label') || control.textContent} outside viewport`);
      if (control.tagName === 'BUTTON' && control.scrollWidth > control.clientWidth + 2) failures.push(`${control.getAttribute('aria-label') || control.textContent} text overflow`);
    }
    for (const container of document.querySelectorAll('.work-topbar, .work-composer-controls, .work-consent-actions')) {
      if (!visible(container)) continue;
      const children = [...container.children].filter(visible);
      for (let first = 0; first < children.length; first += 1) {
        for (let second = first + 1; second < children.length; second += 1) {
          const left = children[first].getBoundingClientRect();
          const right = children[second].getBoundingClientRect();
          const width = Math.min(left.right, right.right) - Math.max(left.left, right.left);
          const height = Math.min(left.bottom, right.bottom) - Math.max(left.top, right.top);
          if (width > 1 && height > 1) failures.push('sibling controls overlap');
        }
      }
    }
    if (![...document.images].every((image) => image.complete && image.naturalWidth > 0)) failures.push('image missing');
    return failures;
  });
  await page.screenshot({ path: resolve(output, `${name}.png`), fullPage: true });
  assert.deepEqual(failures, [], name);
}

try {
  browser = await chromium.launch({ headless: true, channel: process.argv[3] ?? 'msedge' });
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  const errors = [];
  const external = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('request', (request) => { if (!request.url().startsWith(base)) external.push(request.url()); });
  await page.goto(base);
  await visibleText(page, 'Waiting for the local Work host.');
  assert.equal(await page.getByRole('button', { name: 'Send task', exact: true }).isEnabled(), false);
  assert.equal(await page.getByText('Sign in', { exact: true }).count(), 0);
  await geometry(page, 'desktop-offline');
  await page.addInitScript(installFixture);
  await page.reload();
  await visibleText(page, 'Local host ready');
  await page.getByRole('button', { name: 'Task history', exact: true }).click();
  await page.getByRole('button', { name: /Fix the parser/ }).click();
  await visibleText(page, 'Saved result: the parser was updated.');
  await page.getByLabel('Task prompt', { exact: true }).fill('Repair the next parser case');
  await page.getByRole('combobox', { name: 'Sampling preset', exact: true }).selectOption('precise');
  await page.getByRole('button', { name: 'Send task', exact: true }).click();
  await visibleText(page, 'Thinking');
  await page.evaluate(() => {
    fixture.event('token', { delta: 'Inspecting the parser.' });
    fixture.event('tool_result', {
      id: 'shell-1', name: 'shell', ok: false, elapsedMs: 120, summary: 'Parser regression failed', content: 'npm test -- parser\n1 test failed\nexit code 1', error: 'Process exited with code 1',
    });
    fixture.approval();
  });
  await visibleText(page, 'Approval required');
  await geometry(page, 'desktop-approval');
  await page.getByRole('button', { name: 'Deny', exact: true }).click();
  assert.equal(await page.getByText('Approval required', { exact: true }).count(), 0);
  await page.evaluate(() => fixture.approval());
  await page.getByRole('button', { name: 'Allow once', exact: true }).click();
  await page.evaluate(() => fixture.approval());
  await page.getByRole('button', { name: 'Allow tool for session', exact: true }).click();
  await page.evaluate(() => fixture.approval());
  await page.evaluate(() => fixture.connection('reconnecting', 'Fixture stream disconnected'));
  assert.equal(await page.getByRole('button', { name: 'Allow once', exact: true }).isEnabled(), false);
  await page.evaluate(() => fixture.connection('connected'));
  await page.getByRole('button', { name: 'Stop current task', exact: true }).click();
  await visibleText(page, 'Cancelled');
  await page.getByRole('button', { name: 'Open inspector', exact: true }).click();
  await page.getByRole('tab', { name: 'Terminal', exact: true }).click();
  await page.getByText('Parser regression failed', { exact: true }).last().click();
  await visibleText(page, 'Process exited with code 1');
  await geometry(page, 'desktop-terminal');
  await page.getByRole('tab', { name: 'Files / Changes', exact: true }).click();
  await page.getByText('Updated the parser', { exact: true }).last().click();
  await visibleText(page, '-const broken = true;\n+const broken = false;');
  await page.getByRole('tab', { name: 'Artifacts', exact: true }).click();
  await page.getByText(/Artifact listing and export endpoints are unavailable/).waitFor();
  await page.getByRole('tab', { name: 'Context', exact: true }).click();
  await visibleText(page, 'C:/fixture/project');
  await page.getByRole('button', { name: 'Task settings', exact: true }).click();
  await page.getByLabel('Temperature', { exact: true }).fill('0.7');
  await page.getByRole('button', { name: 'Reset overrides', exact: true }).click();
  assert.equal(await page.getByLabel('Temperature', { exact: true }).inputValue(), '0.3');
  await page.getByLabel('Task title', { exact: true }).fill('Parser repair reviewed');
  await page.getByRole('button', { name: 'Rename', exact: true }).click();
  await page.getByRole('button', { name: 'Close inspector', exact: true }).first().click();
  await page.getByRole('button', { name: 'New task', exact: true }).click();
  await page.evaluate(() => { fixture.holdSend = true; });
  await page.getByLabel('Task prompt', { exact: true }).fill('Keep the UI responsive');
  await page.getByRole('button', { name: 'Send task', exact: true }).click();
  await page.getByRole('button', { name: 'Stop current task', exact: true }).click();
  await visibleText(page, 'Cancelled');
  await page.evaluate(() => { fixture.finishSend(); fixture.holdSend = false; });
  await page.getByRole('button', { name: 'New task', exact: true }).click();
  await page.getByLabel('Task prompt', { exact: true }).fill('Do not lose this draft');
  await page.evaluate(() => { fixture.failSend = true; });
  await page.getByRole('button', { name: 'Send task', exact: true }).click();
  await page.getByText('Reload this task to reconcile the last send', { exact: true }).waitFor();
  assert.equal(await page.getByLabel('Task prompt', { exact: true }).inputValue(), 'Do not lose this draft');
  assert.equal(await page.getByRole('button', { name: 'Send task', exact: true }).isEnabled(), false);
  const calls = await page.evaluate(() => fixture.calls);
  assert.deepEqual(calls.filter((call) => call.command === 'local_consent').map((call) => call.args.decision), ['deny', 'allow', 'allow_session']);
  assert.ok(calls.some((call) => call.command === 'local_send' && call.args.options.preset === 'precise'));
  assert.equal(calls.filter((call) => call.command === 'local_send' && call.args.options.prompt === 'Do not lose this draft').length, 1);
  assert.ok(calls.every((call) => call.command.startsWith('local_')));
  for (const [width, height] of [[1920, 1080], [768, 1024], [390, 844], [360, 800], [844, 390]]) {
    await page.setViewportSize({ width, height });
    await geometry(page, `task-${width}x${height}`);
    await page.getByRole('button', { name: 'Open inspector', exact: true }).click();
    await geometry(page, `inspector-${width}x${height}`);
    await page.getByRole('button', { name: 'Close inspector', exact: true }).first().click();
  }
  await page.setViewportSize({ width: 720, height: 450 });
  await page.getByRole('button', { name: 'Task settings', exact: true }).click();
  await geometry(page, 'settings-reflow-720x450');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('dialog[open]').count(), 0);
  await page.getByRole('button', { name: 'Task history', exact: true }).focus();
  await page.keyboard.press('Enter');
  await page.getByRole('searchbox', { name: 'Search task history' }).fill('missing task');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('dialog[open]').count(), 0);
  assert.deepEqual(external, []);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ result: 'passed', scope: 'Browser with fixture IPC, not native WebView or model acceptance', viewport_checks: 15, tested: ['offline', 'history', 'send', 'tokens', 'failure', 'diff', 'consent decisions', 'disconnect', 'cancel during send', 'draft recovery', 'inspector', 'settings', 'keyboard'], screenshots: output }));
} finally {
  if (browser) await browser.close();
  server.closeAllConnections();
  await new Promise((accept) => server.close(accept));
}
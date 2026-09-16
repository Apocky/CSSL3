// Open the apx work window.
//
// This is the whole desktop app: the service already speaks HTTP and already serves the page, so a
// window is a browser told to drop its browser-ness. `--app=` gives a frameless window with its own
// taskbar entry and no tabs, address bar or bookmarks -- which is what Electron would have shipped,
// minus 150 MB of runtime and a build step.
//
// It starts the service first if nothing is listening, so the window is the only thing you launch.

const { spawn, spawnSync } = require('node:child_process');
const { existsSync, readFileSync } = require('node:fs');
const { join, resolve } = require('node:path');
const net = require('node:net');

const PORT = Number(process.env.APOCRYPHA_WORK_PORT ?? 19130);
const STATE = process.env.APOCRYPHA_WORK_STATE_DIR ?? 'C:\\Apocrypha\\work';
const ENV_FILE = process.env.APOCRYPHA_WORK_ENV ?? join(STATE, 'work.env');
const EDGE_ROOT = resolve(__dirname, '..', '..');

// Chromium first, whichever exists; Edge ships with Windows so there is always one.
const BROWSERS = [
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
];

function listening(port) {
  return new Promise((done) => {
    const socket = net.connect({ port, host: '127.0.0.1' });
    socket.setTimeout(700);
    socket.on('connect', () => { socket.destroy(); done(true); });
    socket.on('error', () => done(false));
    socket.on('timeout', () => { socket.destroy(); done(false); });
  });
}

async function waitFor(port, ms) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (await listening(port)) return true;
    await new Promise((r) => setTimeout(r, 400));
  }
  return false;
}

/**
 * Start the model engine from the committed dial file, if nothing is already serving it.
 *
 * This exists because the engine that answers everything was, at one point, started by an EXIT trap
 * inside a throwaway benchmark script -- a reboot would have lost the whole measured configuration
 * with no record of what it had been. Never kills a running engine: if the port answers, whatever
 * is there is already doing the job.
 */
async function startEngine() {
  const dialsPath = join(__dirname, 'engine-dials.json');
  if (!existsSync(dialsPath)) return true;
  const dials = JSON.parse(readFileSync(dialsPath, 'utf8'));
  if (await listening(dials.port)) return true;

  for (const [label, path] of [['engine binary', dials.binary], ['model', dials.model]]) {
    if (!existsSync(path)) { console.error(`Cannot start the engine: ${label} missing at ${path}`); return false; }
  }
  console.log('Starting the model engine (this loads ~25 GiB, give it a minute)...');
  spawn(dials.binary, [
    '--model', dials.model, '--alias', dials.alias,
    '--port', String(dials.port), '--host', dials.host,
    ...dials.args,
  ], { detached: true, stdio: 'ignore', windowsHide: true }).unref();
  // Weights come off NVMe and the MoE placement runs at load; a cold start is minutes, not seconds.
  if (!(await waitFor(dials.port, 300_000))) return false;
  await warm(dials);
  return true;
}

/**
 * Generate a few times before handing the engine over.
 *
 * MEASURED, and larger than any dial on this box: the same engine at the same settings produced
 * 11.6 tok/s on its first generations and ~17.9 once it had run a few, a ~55% climb that then held
 * steady. `--warmup` alone does not buy this -- it primes a single batch, while what actually warms
 * an 80B-A3B with 30 expert layers in CPU RAM is real token generation paging those experts in.
 * Unwarmed, the first thing you ask costs you nearly half the throughput.
 *
 * Bounded and best-effort: a warm-up that fails or hangs must never stop the window opening.
 */
async function warm(dials) {
  const base = `http://${dials.host}:${dials.port}`;
  console.log('Warming the engine (first generations run ~55% slow until the experts are paged in)...');
  for (let i = 0; i < 4; i += 1) {
    try {
      await fetch(`${base}/completion`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        // A distinct prefix each time, so this exercises generation instead of replaying the
        // prompt cache -- and short, so warming costs seconds rather than minutes.
        body: JSON.stringify({ prompt: `warmup ${i}: count to three.`, n_predict: 48, temperature: 0.3 }),
        signal: AbortSignal.timeout(60_000),
      });
    } catch { return; } // engine busy or slow: the window is still worth opening
  }
}

async function main() {
  if (!(await startEngine())) {
    console.error('The engine did not come up. The window will open but answer nothing.');
  }
  if (!(await listening(PORT))) {
    if (!existsSync(ENV_FILE)) {
      console.error(`No work env at ${ENV_FILE}. Cannot start the service.`);
      process.exit(1);
    }
    console.log('Starting the work service...');
    // detached + ignored stdio: the window outlives this launcher, which exits immediately.
    spawn(process.execPath, ['--env-file=' + ENV_FILE, '--import', 'tsx', 'scripts/apocrypha-work/server.ts'], {
      cwd: EDGE_ROOT, detached: true, stdio: 'ignore', windowsHide: true,
    }).unref();
    // MCP handshakes run during boot, so first start is slower than a bare listen.
    if (!(await waitFor(PORT, 90_000))) {
      console.error(`The service did not come up on ${PORT}. Check ${join(STATE, 'service.log')}.`);
      process.exit(1);
    }
  }

  const tokenPath = join(STATE, 'work.token');
  if (!existsSync(tokenPath)) { console.error(`No token at ${tokenPath}.`); process.exit(1); }
  const token = readFileSync(tokenPath, 'utf8').trim();

  // The token rides in the URL because a window cannot be handed a header. The page strips it from
  // its own address bar on load, so it does not sit in history or in a screenshot of the window.
  const url = `http://127.0.0.1:${PORT}/app?token=${encodeURIComponent(token)}`;
  const browser = BROWSERS.find((path) => existsSync(path));

  if (!browser) {
    // No Chromium: hand it to whatever handles http. A tab is worse than a window, but it works.
    spawnSync('cmd', ['/c', 'start', '', url], { windowsHide: true });
    return;
  }
  spawn(browser, [
    `--app=${url}`,
    // Its own profile, so the window does not inherit or disturb the real browser's session.
    `--user-data-dir=${join(STATE, 'gui-profile')}`,
    '--window-size=1280,860',
    '--no-first-run',
    '--no-default-browser-check',
  ], { detached: true, stdio: 'ignore', windowsHide: true }).unref();
}

void main();

/**
 * Work-lane diagnostics: many small probes, each blind to what the others catch.
 *
 * This is not a test suite. Tests answer "is the code correct"; this answers "is THIS HOST, right
 * now, able to run the lane" -- a question whose answer changes without any code changing.
 *
 * Design rule (kernel G4): every probe here must be able to fail in a way no other probe would
 * notice. Two probes that go red together are one probe. The four that earned their place by
 * catching real defects in this build are marked EARNED.
 *
 * Read-only. Starts nothing, stops nothing, writes nothing.
 *
 *   node --import tsx scripts/apocrypha-work/doctor.ts
 *   node --import tsx scripts/apocrypha-work/doctor.ts --json
 */
import { execFile } from 'node:child_process';
import { openSync, readSync, closeSync, statSync } from 'node:fs';
import { promisify } from 'node:util';

import { loadWorkConfig } from './config';
import type { WorkConfig } from './types';

const run = promisify(execFile);

type Level = 'OK' | 'WARN' | 'FAIL' | 'SKIP';
interface Probe { group: string; name: string; level: Level; detail: string }

const probes: Probe[] = [];
const add = (group: string, name: string, level: Level, detail: string): void => {
  probes.push({ group, name, level, detail });
};

async function pwsh(script: string): Promise<string> {
  const { stdout } = await run('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true });
  return stdout.trim();
}

async function props(port: number): Promise<{ alias?: string; ctx?: number; path?: string } | null> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/props`, { signal: AbortSignal.timeout(5_000) });
    if (!response.ok) return null;
    const body = await response.json() as { model_alias?: string; model_path?: string; default_generation_settings?: { n_ctx?: number } };
    return { alias: body.model_alias, ctx: body.default_generation_settings?.n_ctx, path: body.model_path };
  } catch {
    return null;
  }
}

function readSpeed(path: string, bytes = 512 * 1024 * 1024): number | null {
  try {
    const fd = openSync(path, 'r');
    const buffer = Buffer.allocUnsafe(8 * 1024 * 1024);
    let total = 0;
    const started = process.hrtime.bigint();
    while (total < bytes) {
      const n = readSync(fd, buffer, 0, buffer.length, total);
      if (n <= 0) break;
      total += n;
    }
    const seconds = Number(process.hrtime.bigint() - started) / 1e9;
    closeSync(fd);
    return Math.round((total / 1024 / 1024) / seconds);
  } catch {
    return null;
  }
}

async function storage(config: WorkConfig): Promise<void> {
  const models: Record<string, string> = {
    'work (exclusive)': 'C:\\Apocrypha\\models\\work-lane\\Qwen3-Coder-Next-UD-Q2_K_XL.gguf',
    'work (fallback)': 'D:\\Apocrypha\\models\\work-lane\\Devstral-Small-2-24B-Instruct-2512-UD-Q2_K_XL.gguf',
    'chat': 'D:\\Apocrypha\\models\\Qwen3.5-35B-A3B-Q4\\Qwen3.5-35B-A3B-Q4_K_S.gguf',
  };
  for (const [label, path] of Object.entries(models)) {
    try {
      const gib = statSync(path).size / 1024 ** 3;
      add('storage', `model ${label}`, 'OK', `${gib.toFixed(2)} GiB  ${path}`);
    } catch {
      add('storage', `model ${label}`, label.includes('fallback') ? 'WARN' : 'FAIL', `missing: ${path}`);
    }
  }

  // EARNED. The exclusive model was on the SATA disk and load I/O was ~10x slower. A path check
  // alone would have said "present" and been useless; only a measured rate catches a silent move.
  const primary = models['work (exclusive)'];
  if (primary) {
    const speed = readSpeed(primary);
    if (speed === null) add('storage', 'read speed (work model)', 'SKIP', 'could not read the file');
    else if (speed < 800) add('storage', 'read speed (work model)', 'WARN', `${speed} MB/s -- this looks like the SATA disk; C: measured 3426 MB/s`);
    else add('storage', 'read speed (work model)', 'OK', `${speed} MB/s`);
  }

  const free = await pwsh("(Get-PSDrive -PSProvider FileSystem | Where-Object {$_.Name -in 'C','D'} | ForEach-Object { \"$($_.Name)=$([math]::Round($_.Free/1GB,1))\" }) -join ' '");
  const cFree = Number(/C=([\d.]+)/.exec(free)?.[1] ?? '0');
  add('storage', 'free space', cFree < 15 ? 'WARN' : 'OK', `${free} GB${cFree < 15 ? '  -- C: is tight for a model swap' : ''}`);
  add('storage', 'state dir', 'OK', config.stateDir);
}

async function memory(): Promise<void> {
  const raw = await pwsh(
    '$os = Get-CimInstance Win32_OperatingSystem;'
    + '$avail = (Get-Counter \'\\Memory\\Available MBytes\' -EA SilentlyContinue).CounterSamples[0].CookedValue;'
    + '"{0}|{1}|{2}" -f [math]::Round($os.TotalVisibleMemorySize/1MB,1), [math]::Round($os.FreePhysicalMemory/1MB,2), [math]::Round($avail/1024,2)',
  );
  const [total, free, avail] = raw.split('|').map(Number);
  add('memory', 'system RAM', 'OK', `${total} GB total, ${free} GB free, ${avail} GB available`);

  // The WDDM shared pool is half of system RAM and is the real ceiling on a second engine.
  const cap = (total ?? 0) / 2;
  add('memory', 'WDDM shared cap', 'OK', `${cap.toFixed(1)} GB (half of system RAM) -- one engine at a time is a consequence of this, not a preference`);
  if ((avail ?? 0) < 2) add('memory', 'headroom', 'WARN', `${avail} GB available; a handover needs the resident engine stopped first (the arbiter does this)`);
  else add('memory', 'headroom', 'OK', `${avail} GB available`);
}

async function gpu(): Promise<void> {
  const raw = await pwsh(
    "$s = (Get-Counter '\\GPU Process Memory(*)\\Total Committed' -EA SilentlyContinue).CounterSamples | Where-Object {$_.CookedValue -gt 500MB} | Sort-Object CookedValue -Descending | Select-Object -First 3;"
    + 'if ($s) { ($s | ForEach-Object { "{0:N0}MB" -f ($_.CookedValue/1MB) }) -join " " } else { "none" }',
  );
  add('gpu', 'committed by process', 'OK', raw);
  const vram = await pwsh("(Get-ItemProperty -Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0*' -Name 'HardwareInformation.qwMemorySize' -EA SilentlyContinue | ForEach-Object { [math]::Round($_.'HardwareInformation.qwMemorySize'/1GB,1) }) -join ' '");
  add('gpu', 'dedicated VRAM', 'OK', `${vram} GB`);
}

async function ports(config: WorkConfig): Promise<void> {
  const lanes: [string, number][] = [
    ['chat worker', 19126], ['memory gateway', 19127],
    ['engine (shared)', config.arbiter.enginePort],
    ['work service', config.port],
  ];
  for (const [label, port] of lanes) {
    const owner = await pwsh(`$c = Get-NetTCPConnection -LocalPort ${port} -State Listen -EA SilentlyContinue | Select-Object -First 1; if ($c) { (Get-CimInstance Win32_Process -Filter "ProcessId=$($c.OwningProcess)").Name + ' pid ' + $c.OwningProcess } else { '' }`);
    add('ports', `${label} :${port}`, owner ? 'OK' : 'WARN', owner || 'not listening');
  }
}

async function engines(config: WorkConfig): Promise<void> {
  const port = config.arbiter.enginePort;
  const live = await props(port);
  if (!live) { add('engine', 'resident model', 'FAIL', `nothing serving on ${port} -- BOTH lanes are down`); return; }

  const isWork = (live.path ?? '').toLowerCase() === config.arbiter.workModelPath.toLowerCase();
  add('engine', 'resident model', 'OK', `${isWork ? 'work' : 'chat'} :: ${live.alias}`);
  add('engine', 'serves both lanes', 'OK', `chat worker and work service share :${port}; llama-server ignores the model field`);
  add('engine', 'context', 'OK', `${live.ctx} tokens`);
  if (isWork) add('engine', 'model on fast disk', live.path?.toUpperCase().startsWith('C:') ? 'OK' : 'WARN', live.path ?? 'unknown');

  // Both lanes must actually point at this port, or one of them is talking to nothing.
  const workTarget = Number(new URL(config.engine.baseUrl).port || 0);
  add('engine', 'work service target', workTarget === port ? 'OK' : 'FAIL',
    workTarget === port ? `:${port}` : `work service targets :${workTarget} but the engine is on :${port}`);

  // A second engine anywhere is the OOM condition; check the old split port is genuinely empty.
  const stray = await props(19131);
  add('engine', 'no second engine', stray ? 'FAIL' : 'OK',
    stray ? `a second engine is serving on :19131 (${stray.alias}) -- measured to exhaust RAM` : 'only one engine is running');
}

/**
 * Does the resident engine actually do tool calls, and does it answer at all?
 *
 * EARNED, twice. An engine started without --jinja reports chat_format "Content-only" and silently
 * never emits a tool call -- every task then ends as a chatty non-answer. And a Qwen template left
 * in thinking mode burns the whole output budget on reasoning the server discards, returning empty.
 * Neither shows up in /health, /props, or any unit test; both look like "the model is just bad".
 */
async function capability(config: WorkConfig): Promise<void> {
  const port = config.arbiter.enginePort;
  const alias = (await props(port))?.alias;
  if (!alias) { add('capability', 'tool calling', 'SKIP', 'no engine resident'); return; }

  const body = {
    model: alias,
    messages: [
      { role: 'system', content: 'You are a coding agent. Use a tool to answer.' },
      { role: 'user', content: "List the files in the directory 'src'. Use the tool." },
    ],
    tools: [{
      type: 'function',
      function: {
        name: 'list_dir', description: 'List the entries of a directory.',
        parameters: { type: 'object', properties: { path: { type: 'string' } }, required: ['path'] },
      },
    }],
    tool_choice: 'auto',
    chat_template_kwargs: { enable_thinking: false },
    max_tokens: 200, temperature: 0.2, stream: false,
  };
  try {
    const started = Date.now();
    const response = await fetch(`http://127.0.0.1:${port}/v1/chat/completions`, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body), signal: AbortSignal.timeout(180_000),
    });
    const json = await response.json() as { choices?: { finish_reason?: string; message?: { content?: string; tool_calls?: unknown[] } }[] };
    const choice = json.choices?.[0];
    const calls = choice?.message?.tool_calls ?? [];
    const seconds = ((Date.now() - started) / 1_000).toFixed(1);

    if (calls.length > 0) add('capability', 'tool calling', 'OK', `emitted ${calls.length} tool call in ${seconds}s`);
    else add('capability', 'tool calling', 'FAIL', `no tool call (finish_reason ${choice?.finish_reason}). Was the engine started with --jinja?`);

    const said = (choice?.message?.content ?? '').trim();
    if (calls.length === 0 && said === '' && choice?.finish_reason === 'length') {
      add('capability', 'thinking disabled', 'FAIL', 'burned the output budget and returned nothing -- thinking mode is still on');
    } else add('capability', 'thinking disabled', 'OK', 'engine returned usable output');
  } catch (error) {
    add('capability', 'tool calling', 'FAIL', error instanceof Error ? error.message : 'probe failed');
  }
}

async function service(config: WorkConfig): Promise<void> {
  try {
    const response = await fetch(`http://127.0.0.1:${config.port}/health`, {
      headers: { authorization: `Bearer ${config.token}` }, signal: AbortSignal.timeout(20_000),
    });
    const body = await response.json() as {
      engine?: { healthy?: boolean }; policy?: { shell?: boolean; auto_approve?: string[] };
      arbiter?: { mode?: string; resident?: string; chatBusy?: boolean | null; lastError?: string | null };
    };
    add('service', 'work service', 'OK', `responding (${response.status})`);
    add('service', 'policy', 'OK', `shell=${body.policy?.shell} auto_approve=${(body.policy?.auto_approve ?? []).join(',')}`);
    const arbiter = body.arbiter;
    add('service', 'arbiter', 'OK', `mode=${arbiter?.mode} resident=${arbiter?.resident} chatBusy=${arbiter?.chatBusy}`);
    if (arbiter?.lastError) add('service', 'last handover', 'WARN', arbiter.lastError.split('\n')[0] ?? '');

    // An unauthenticated request must be refused. A front door that has quietly stopped checking
    // looks identical to a healthy one from the inside.
    const anon = await fetch(`http://127.0.0.1:${config.port}/health`, { signal: AbortSignal.timeout(10_000) });
    add('service', 'auth gate', anon.status === 401 ? 'OK' : 'FAIL', `unauthenticated request returned ${anon.status}, expected 401`);
  } catch {
    add('service', 'work service', 'WARN', `not responding on ${config.port}`);
  }
}

async function launchers(config: WorkConfig): Promise<void> {
  for (const [label, path] of [['work', config.arbiter.workLauncher], ['chat', config.arbiter.chatLauncher]] as const) {
    try {
      statSync(path);
      // EARNED. Both launchers once wrapped the model path in embedded quotes, so llama.cpp tried
      // to open a filename starting with a double-quote. It stayed invisible until a relaunch.
      const text = await run('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', `Get-Content -Raw -LiteralPath '${path}'`], { windowsHide: true });
      // Comment lines are stripped first. The launchers now carry a comment QUOTING the old bad
      // pattern to stop it being reintroduced, and the first version of this probe matched that
      // comment and reported the fixed file as broken -- the detector reading its own warning sign.
      const code = text.stdout.split('\n').filter((line) => !line.trimStart().startsWith('#')).join('\n');
      const quoted = /'--model',\s*"`"/.test(code);
      add('launcher', `${label} script`, quoted ? 'FAIL' : 'OK',
        quoted ? 'model path is wrapped in embedded quotes -- llama.cpp will not open it' : path);
    } catch {
      add('launcher', `${label} script`, 'FAIL', `missing: ${path}`);
    }
  }
}

function policy(config: WorkConfig): void {
  // Fire the deny-list and the confinement rule rather than asserting they exist. A rule that is
  // present but never observed refusing anything is not a rule (G2).
  const denied = config.shellDenyPatterns.some((pattern) => { pattern.lastIndex = 0; return pattern.test('rm -rf /'); });
  const allowed = !config.shellDenyPatterns.some((pattern) => { pattern.lastIndex = 0; return pattern.test('npm run format'); });
  add('policy', 'deny-list fires', denied ? 'OK' : 'FAIL', denied ? 'rm -rf / refused' : 'rm -rf / WAS NOT REFUSED');
  add('policy', 'deny-list discriminates', allowed ? 'OK' : 'FAIL', allowed ? 'npm run format allowed' : 'ordinary commands are being blocked');
  add('policy', 'roots', 'OK', config.roots.map((root) => `${root.label}${root.writable ? '' : ':ro'}`).join(' '));
  add('policy', 'token', config.token.length >= 24 ? 'OK' : 'FAIL', `${config.token.length} chars (value never printed)`);
}

async function main(): Promise<void> {
  const config = loadWorkConfig();
  await storage(config);
  await memory();
  await gpu();
  await ports(config);
  await engines(config);
  await capability(config);
  await service(config);
  await launchers(config);
  policy(config);

  if (process.argv.includes('--json')) {
    console.log(JSON.stringify({ at: new Date().toISOString(), probes }, null, 2));
  } else {
    let group = '';
    for (const probe of probes) {
      if (probe.group !== group) { group = probe.group; console.log(`\n${group.toUpperCase()}`); }
      const mark = { OK: ' ok ', WARN: 'warn', FAIL: 'FAIL', SKIP: 'skip' }[probe.level];
      console.log(`  [${mark}] ${probe.name.padEnd(26)} ${probe.detail}`);
    }
  }

  const fails = probes.filter((probe) => probe.level === 'FAIL');
  const warns = probes.filter((probe) => probe.level === 'WARN');
  console.log(`\n${probes.length} probes: ${probes.length - fails.length - warns.length} ok, ${warns.length} warn, ${fails.length} fail`);
  if (fails.length > 0) process.exitCode = 1;
}

void main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});

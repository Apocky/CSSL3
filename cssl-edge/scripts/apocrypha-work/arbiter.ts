import { spawn } from 'node:child_process';
import { closeSync, openSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { log } from './log';

export type Lane = 'chat' | 'work';
export type ArbiterMode = 'off' | 'manual' | 'auto';

export interface LaneProfile {
  readonly lane: Lane;
  /** PowerShell launcher that brings this lane's model up on the shared engine port. */
  readonly launcher: string;
  readonly launcherArgs: readonly string[];
  readonly label: string;
  /** Absolute .gguf path, used to identify which model is currently loaded. */
  readonly modelPath: string;
}

export interface ArbiterConfig {
  readonly mode: ArbiterMode;
  /**
   * The ONE port an engine ever listens on.
   *
   * llama-server ignores the `model` field in an OpenAI request and simply serves whatever it has
   * loaded (verified: a nonsense model name was accepted and answered). So both lanes can point at
   * the same endpoint and neither needs to know which model is behind it. Selecting Work swaps the
   * model; it does not move the socket, and the Chat worker's configuration never changes.
   */
  readonly enginePort: number;
  readonly chat: LaneProfile;
  readonly work: LaneProfile;
  /** Worker health endpoint that reports whether a Chat job is in flight. */
  readonly chatWorkerHealthUrl: string;
  readonly drainTimeoutMs: number;
  readonly startTimeoutMs: number;
  readonly idleYieldMs: number;
  /** Where the launcher's own stdout/stderr is captured, so a failed start leaves evidence. */
  readonly launcherLogDir: string;
}

export interface ArbiterStatus {
  mode: ArbiterMode;
  /** Which lane's MODEL is loaded. Both lanes are served by it, whichever it is. */
  resident: Lane | 'none';
  engineUp: boolean;
  residentModel: string | null;
  chatBusy: boolean | null;
  handoverInFlight: boolean;
  lastHandoverAt: string | null;
  lastError: string | null;
}

async function portIsServing(port: number, timeoutMs = 3_000): Promise<boolean> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(timeoutMs) });
    return response.ok;
  } catch {
    return false;
  }
}

/** The .gguf the engine currently has open, or null if nothing is serving. */
async function loadedModel(port: number, timeoutMs = 5_000): Promise<string | null> {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/props`, { signal: AbortSignal.timeout(timeoutMs) });
    if (!response.ok) return null;
    const body = await response.json() as { model_path?: string };
    return body.model_path ?? null;
  } catch {
    return null;
  }
}

function sameModel(a: string | null, b: string): boolean {
  if (!a) return false;
  const norm = (value: string): string => value.split('/').join('\\').toLowerCase();
  return norm(a) === norm(b);
}

function pwsh(command: string, timeoutMs: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', command], { windowsHide: true });
    let out = '';
    child.stdout.on('data', (chunk: Buffer) => { out += chunk.toString('utf8'); });
    child.stderr.on('data', (chunk: Buffer) => { out += chunk.toString('utf8'); });
    const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('pwsh timed out')); }, timeoutMs);
    child.on('error', (error) => { clearTimeout(timer); reject(error); });
    child.on('close', () => { clearTimeout(timer); resolve(out.trim()); });
  });
}

/**
 * Owner of the single GPU slot: ONE engine on ONE port, and a choice of which model it holds.
 *
 * Measured on this host 2026-09-14: the Chat engine commits 19.7 GB against a 15.9 GB card whose
 * WDDM shared pool is capped at 15.9 GB. A second engine beside it drove free RAM from 19.4 GB to
 * 1.1 GB and died out-of-memory mid-generation. Two engines do not fit.
 *
 * The first design answered that by making the lanes take turns on the card, which meant Chat
 * went DOWN whenever Work held it. That was unnecessary. llama-server ignores the `model` field in
 * an OpenAI request and serves whatever it has loaded, so both lanes can share one endpoint and
 * Chat is simply answered by whichever model is resident. Selecting Work swaps the model once;
 * Chat keeps working throughout, in the coder's voice. Nothing switches back on its own.
 */
export class EngineArbiter {
  private readonly config: ArbiterConfig;
  private handover: Promise<void> | null = null;
  private lastHandoverAt: string | null = null;
  private lastError: string | null = null;
  private idleTimer: NodeJS.Timeout | null = null;

  constructor(config: ArbiterConfig) {
    this.config = config;
  }

  private profile(lane: Lane): LaneProfile {
    return lane === 'chat' ? this.config.chat : this.config.work;
  }

  async status(): Promise<ArbiterStatus> {
    const model = await loadedModel(this.config.enginePort);
    const resident: Lane | 'none' = sameModel(model, this.config.work.modelPath) ? 'work'
      : sameModel(model, this.config.chat.modelPath) ? 'chat'
        : model === null ? 'none' : 'work';
    return {
      mode: this.config.mode,
      resident: model === null ? 'none' : resident,
      engineUp: model !== null,
      residentModel: model,
      chatBusy: await this.chatBusy(),
      handoverInFlight: this.handover !== null,
      lastHandoverAt: this.lastHandoverAt,
      lastError: this.lastError,
    };
  }

  /**
   * Is a Chat job in flight?
   *
   * `null` means the worker did not answer, which is NOT the same as idle and is never treated as
   * permission to take the card.
   */
  private async chatBusy(): Promise<boolean | null> {
    try {
      const response = await fetch(this.config.chatWorkerHealthUrl, { signal: AbortSignal.timeout(4_000) });
      if (!response.ok) return null;
      const body = await response.json() as { worker?: { phase?: string; current_job_id?: string | null } };
      const worker = body.worker;
      if (!worker) return null;
      return worker.current_job_id !== null || (worker.phase !== 'idle' && worker.phase !== 'starting');
    } catch {
      return null;
    }
  }

  private async stopEngine(): Promise<void> {
    const port = this.config.enginePort;
    // Resolve the PID from the listening socket, never from a command-line pattern: a pattern
    // match once killed this session's own process alongside its target.
    const pidText = await pwsh(
      `$c = Get-NetTCPConnection -LocalPort ${port} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; if ($c) { $c.OwningProcess } else { '' }`,
      15_000,
    );
    const pid = Number(pidText.trim());
    if (!Number.isInteger(pid) || pid <= 0) { log('info', 'arbiter.stop.not_running', { port }); return; }
    log('warn', 'arbiter.stop', { pid, port });
    await pwsh(`Stop-Process -Id ${pid} -Force -ErrorAction SilentlyContinue`, 15_000);
    for (let attempt = 0; attempt < 30; attempt += 1) {
      if (!await portIsServing(port, 1_500)) return;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
    throw new Error(`the engine on ${port} did not stop`);
  }

  private async startLane(lane: Lane): Promise<void> {
    const profile = this.profile(lane);
    const transcript = join(this.config.launcherLogDir, `arbiter-${lane}-launch.log`);
    log('info', 'arbiter.start', { lane, port: this.config.enginePort, label: profile.label, transcript });

    // Capture the launcher's own stdout/stderr to a file. The first version of this used
    // stdio:'ignore', and when the launcher threw before llama-server ever ran, the failure was
    // indistinguishable from a slow load: the poll below simply ran out its 420 s and reported a
    // timeout, having destroyed the one artifact that said why. G2 -- a detector that reports the
    // same thing for "still working" and "died instantly" has measured nothing.
    let sink: number | null = null;
    try {
      sink = openSync(transcript, 'w');
    } catch {
      // A missing log directory must not be the reason the engine cannot start.
    }

    // windowsHide (CREATE_NO_WINDOW) and NOT detached.
    //
    // detached:true on Windows means DETACHED_PROCESS, which gives the child no console at all.
    // pwsh is a console application: with no console it exits 0 immediately, having run nothing.
    // That is exactly what happened here -- an instant exit 0 with an empty transcript, which the
    // old poll-only loop then reported as a 420 s load timeout. CREATE_NO_WINDOW still gives the
    // child a console, just an invisible one, so the script runs and nothing steals focus from a
    // fullscreen game. The engine outlives this service either way: Windows does not cascade-kill
    // children on parent exit unless they share a Job Object.
    const child = spawn('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', '-File', profile.launcher, ...profile.launcherArgs], {
      windowsHide: true,
      stdio: sink === null ? 'ignore' : ['ignore', sink, sink],
    });

    let exited: { code: number | null; signal: string | null } | null = null;
    let spawnError: string | null = null;
    // An 'error' event with no listener is itself an unhandled throw, which would take the whole
    // service down for what is a recoverable condition.
    child.on('error', (error) => { spawnError = error.message; });
    child.on('exit', (code, signal) => { exited = { code, signal }; });
    child.unref();

    const readTranscript = (): string => {
      try {
        return readFileSync(transcript, 'utf8').trim().split('\n').slice(-12).join('\n');
      } catch {
        return '(no launcher output captured)';
      }
    };

    const started = Date.now();
    const deadline = started + this.config.startTimeoutMs;
    while (Date.now() < deadline) {
      // The port binds before the weights finish loading, so wait until /props names the model we
      // actually asked for -- otherwise a lingering previous engine reads as a successful start.
      if (sameModel(await loadedModel(this.config.enginePort, 3_000), profile.modelPath)) {
        if (sink !== null) closeSync(sink);
        log('info', 'arbiter.start.ready', { lane, port: this.config.enginePort, seconds: Math.round((Date.now() - started) / 1_000) });
        return;
      }
      if (spawnError !== null) {
        if (sink !== null) closeSync(sink);
        throw new Error(`could not launch the ${lane} engine: ${spawnError}`);
      }
      // Fail on the child's own exit rather than outliving it by seven minutes.
      if (exited !== null) {
        if (sink !== null) closeSync(sink);
        const { code, signal } = exited as { code: number | null; signal: string | null };
        throw new Error(
          `the ${lane} launcher exited (${signal ? `signal ${signal}` : `code ${code}`}) without the engine coming up.\n${readTranscript()}`,
        );
      }
      await new Promise((resolve) => setTimeout(resolve, 3_000));
    }
    if (sink !== null) closeSync(sink);
    throw new Error(
      `${lane} engine did not become ready within ${Math.round(this.config.startTimeoutMs / 1_000)}s.\n${readTranscript()}`,
    );
  }

  /** Wait for the Chat lane to finish whatever it is doing. Returns false if it never went idle. */
  private async drainChat(): Promise<boolean> {
    const deadline = Date.now() + this.config.drainTimeoutMs;
    while (Date.now() < deadline) {
      const busy = await this.chatBusy();
      if (busy === false) return true;
      if (busy === null) return false;
      await new Promise((resolve) => setTimeout(resolve, 2_000));
    }
    return false;
  }

  /**
   * Make `lane` the resident engine, stopping the other one first.
   *
   * Serialised: concurrent callers await the same handover rather than racing two launchers onto
   * one GPU, which is the failure this class exists to prevent.
   */
  async acquire(lane: Lane, opts: { force?: boolean } = {}): Promise<ArbiterStatus> {
    if (this.config.mode === 'off') throw new Error('the engine arbiter is off; start the engine you want by hand');
    if (this.handover) { await this.handover; return this.status(); }

    const profile = this.profile(lane);
    const current = await loadedModel(this.config.enginePort);
    // Already the right model: nothing to do. This is the common case once Work is selected,
    // because Chat is served by the same engine and no longer forces a swap back.
    if (sameModel(current, profile.modelPath)) { this.cancelIdleYield(); return this.status(); }

    const run = (async () => {
      // Swapping the model interrupts whatever the Chat worker is doing, so drain it first.
      if (current !== null) {
        const drained = await this.drainChat();
        if (!drained && opts.force !== true) {
          throw new Error('the Chat lane is busy or unreachable, so the model was not swapped; retry when Chat is idle, or force it');
        }
      }
      await this.stopEngine();
      await this.startLane(lane);
      this.lastHandoverAt = new Date().toISOString();
      this.lastError = null;
    })();

    this.handover = run.catch((error) => {
      this.lastError = error instanceof Error ? error.message : String(error);
      log('error', 'arbiter.handover.failed', { lane, error: this.lastError });
      throw error;
    }).finally(() => { this.handover = null; });

    await this.handover;
    if (lane === 'work') this.scheduleIdleYield();
    return this.status();
  }

  /**
   * Put the Chat model back on the engine.
   *
   * This is now OPTIONAL rather than something the system does to protect Chat. The Chat lane is
   * served by whatever model is loaded, so leaving the coder resident costs Chat nothing but a
   * change of voice. Call this only when you actually want the chat model back.
   */
  async release(): Promise<ArbiterStatus> {
    this.cancelIdleYield();
    if (this.config.mode === 'off') return this.status();
    const current = await loadedModel(this.config.enginePort);
    if (sameModel(current, this.config.chat.modelPath)) return this.status();
    return this.acquire('chat');
  }

  private cancelIdleYield(): void {
    if (this.idleTimer) { clearTimeout(this.idleTimer); this.idleTimer = null; }
  }

  /**
   * Give the card back after a quiet spell.
   *
   * Chat is the public lane; leaving it down because someone closed a browser tab would turn a
   * convenience into an outage. `touch()` restarts the clock on every Work turn.
   */
  private scheduleIdleYield(): void {
    this.cancelIdleYield();
    if (this.config.idleYieldMs <= 0) return;
    this.idleTimer = setTimeout(() => {
      void this.release().catch((error) => {
        log('error', 'arbiter.idle_yield.failed', { error: error instanceof Error ? error.message : String(error) });
      });
    }, this.config.idleYieldMs);
    this.idleTimer.unref();
  }

  touch(): void {
    if (this.idleTimer) this.scheduleIdleYield();
  }

  stop(): void {
    this.cancelIdleYield();
  }
}

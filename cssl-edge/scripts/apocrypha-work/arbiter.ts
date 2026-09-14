import { spawn } from 'node:child_process';
import { log } from './log';

export type Lane = 'chat' | 'work';
export type ArbiterMode = 'off' | 'manual' | 'auto';

export interface LaneProfile {
  readonly lane: Lane;
  readonly port: number;
  /** PowerShell launcher that brings this lane's engine up. Run detached; it blocks while serving. */
  readonly launcher: string;
  readonly launcherArgs: readonly string[];
  readonly label: string;
}

export interface ArbiterConfig {
  readonly mode: ArbiterMode;
  readonly chat: LaneProfile;
  readonly work: LaneProfile;
  /** Worker health endpoint that reports whether a Chat job is in flight. */
  readonly chatWorkerHealthUrl: string;
  readonly drainTimeoutMs: number;
  readonly startTimeoutMs: number;
  readonly idleYieldMs: number;
}

export interface ArbiterStatus {
  mode: ArbiterMode;
  resident: Lane | 'none';
  chatUp: boolean;
  workUp: boolean;
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
 * Owner of the single GPU slot.
 *
 * Measured on this host 2026-09-14: the Chat engine commits 19.7 GB (3.9 dedicated + 15.8 shared)
 * against a 15.9 GB card whose WDDM shared pool is capped at 15.9 GB. Bringing a second engine up
 * beside it drove free system RAM from 19.4 GB to 1.1 GB, pushed the D: queue to 26, and the
 * second engine died out-of-memory mid-generation. Two engines do not fit. So they take turns,
 * and this class is the thing that makes the turn-taking safe.
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
    const [chatUp, workUp] = await Promise.all([
      portIsServing(this.config.chat.port),
      portIsServing(this.config.work.port),
    ]);
    return {
      mode: this.config.mode,
      resident: workUp ? 'work' : chatUp ? 'chat' : 'none',
      chatUp,
      workUp,
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

  private async stopLane(lane: Lane): Promise<void> {
    const profile = this.profile(lane);
    // Resolve the PID from the listening socket, never from a command-line pattern: a pattern
    // match once killed this session's own process alongside its target.
    const pidText = await pwsh(
      `$c = Get-NetTCPConnection -LocalPort ${profile.port} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; if ($c) { $c.OwningProcess } else { '' }`,
      15_000,
    );
    const pid = Number(pidText.trim());
    if (!Number.isInteger(pid) || pid <= 0) { log('info', 'arbiter.stop.not_running', { lane, port: profile.port }); return; }
    log('warn', 'arbiter.stop', { lane, pid, port: profile.port, label: profile.label });
    await pwsh(`Stop-Process -Id ${pid} -Force -ErrorAction SilentlyContinue`, 15_000);
    for (let attempt = 0; attempt < 30; attempt += 1) {
      if (!await portIsServing(profile.port, 1_500)) return;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
    throw new Error(`${lane} engine on ${profile.port} did not stop`);
  }

  private async startLane(lane: Lane): Promise<void> {
    const profile = this.profile(lane);
    log('info', 'arbiter.start', { lane, port: profile.port, label: profile.label });
    // Detached and windowless: a console here steals focus from whatever is fullscreen.
    const child = spawn('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', '-File', profile.launcher, ...profile.launcherArgs], {
      detached: true,
      windowsHide: true,
      stdio: 'ignore',
    });
    child.unref();

    const deadline = Date.now() + this.config.startTimeoutMs;
    while (Date.now() < deadline) {
      if (await portIsServing(profile.port, 2_000)) {
        // The port binds before the weights finish loading, so wait for a real answer.
        const ready = await fetch(`http://127.0.0.1:${profile.port}/props`, { signal: AbortSignal.timeout(4_000) })
          .then((response) => response.ok).catch(() => false);
        if (ready) { log('info', 'arbiter.start.ready', { lane, port: profile.port }); return; }
      }
      await new Promise((resolve) => setTimeout(resolve, 3_000));
    }
    throw new Error(`${lane} engine did not become ready within ${Math.round(this.config.startTimeoutMs / 1_000)}s`);
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
    if (await portIsServing(profile.port)) { this.cancelIdleYield(); return this.status(); }

    const other: Lane = lane === 'work' ? 'chat' : 'work';
    const run = (async () => {
      if (lane === 'work' && await portIsServing(this.config.chat.port)) {
        const drained = await this.drainChat();
        if (!drained && opts.force !== true) {
          throw new Error('the Chat lane is busy or unreachable, so the GPU was not taken from it; retry when Chat is idle, or force the handover');
        }
      }
      await this.stopLane(other);
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

  /** Hand the GPU back to Chat. Safe to call when Chat already holds it. */
  async release(): Promise<ArbiterStatus> {
    this.cancelIdleYield();
    if (this.config.mode === 'off') return this.status();
    if (await portIsServing(this.config.chat.port)) return this.status();
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

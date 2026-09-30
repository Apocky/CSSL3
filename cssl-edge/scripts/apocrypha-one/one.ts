// apocrypha-one host, phase 2 (R02-2b): ONE node process alternates the room
// loop tick and the worker claim loop. Single claimant: the instance lock is
// acquired once and lease code is untouched.
//
// Slots are independent: either side failing never stops the other.
// APOCRYPHA_ONE_LOOP=on joins the loop tick (2d cutover; default off so this
// host first proves worker parity with the split loop still running).
//
// Run: node --env-file=C:\Apocrypha\apocrypha-runtime.env --import tsx scripts/apocrypha-one/one.ts

import { loadConfig } from '../apocrypha-worker/config';
import { startHealthServer } from '../apocrypha-worker/health';
import { acquireWorkerLock } from '../apocrypha-worker/lock';
import { log } from '../apocrypha-worker/log';
import { ApocryphaWorker } from '../apocrypha-worker/worker';
import { configureLogFile } from '../apocrypha-worker/log';
import { join } from 'node:path';
import { tick as loopTick } from '../apocrypha-room/loop';

const LOOP_ON = process.env.APOCRYPHA_ONE_LOOP === 'on';
const LOOP_MS = 1_500;

async function main(): Promise<void> {
  const config = loadConfig();
  configureLogFile(join(config.journalDir, 'one.log'));
  const worker = new ApocryphaWorker(config);
  await worker.journal.initialize();
  const instanceLock = await acquireWorkerLock(config.journalDir);
  const health = startHealthServer(config, worker.runtime, worker.journal, worker.qwen);
  log('info', 'one.start', { loop: LOOP_ON ? 'on' : 'off', node_id: config.nodeId });

  const stop = (signal: string): void => {
    log('info', 'one.stop.requested', { signal });
    worker.stop(signal);
  };
  process.once('SIGINT', () => stop('SIGINT'));
  process.once('SIGTERM', () => stop('SIGTERM'));

  // Worker claim loop (runner.ts shape, cooperative): runOnce, sleep when idle.
  const workerLoop = async (): Promise<void> => {
    try {
      const didWork = await worker.runOnce();
      if (!didWork) await new Promise((resolve) => setTimeout(resolve, config.pollIntervalMs));
    } catch (error) {
      log('error', 'one.worker.error', { detail: String(error).slice(0, 300) });
      await new Promise((resolve) => setTimeout(resolve, config.pollIntervalMs));
    } finally {
      setTimeout(() => { void workerLoop(); }, 0);
    }
  };

  // Loop slot (flagged off until 2d): same 1.5 s cadence, never throws out.
  const roomLoop = async (): Promise<void> => {
    try {
      await loopTick();
    } catch (error) {
      log('error', 'one.loop.error', { detail: String(error).slice(0, 300) });
    } finally {
      setTimeout(() => { void roomLoop(); }, LOOP_MS);
    }
  };

  void workerLoop();
  if (LOOP_ON) void roomLoop();
  try {
    await new Promise(() => { /* runs until signal */ });
  } finally {
    await new Promise<void>((resolve) => health.close(() => resolve()));
    await instanceLock.release();
  }
}

main().catch((error) => {
  log('error', 'one.fatal', { detail: error instanceof Error ? error.message : String(error) });
  process.exitCode = 1;
});

import { loadConfig } from './config';
import { startHealthServer } from './health';
import { acquireWorkerLock } from './lock';
import { log } from './log';
import { ApocryphaWorker } from './worker';

async function main(): Promise<void> {
  const config = loadConfig();
  const worker = new ApocryphaWorker(config);
  await worker.journal.initialize();
  const instanceLock = await acquireWorkerLock(config.journalDir);
  const health = startHealthServer(config, worker.runtime, worker.journal, worker.qwen, worker.frontier);
  const stop = (signal: string): void => {
    log('info', 'worker.stop.requested', { signal });
    worker.stop(signal);
  };
  process.once('SIGINT', () => stop('SIGINT'));
  process.once('SIGTERM', () => stop('SIGTERM'));
  log('info', 'worker.starting', {
    node_id: config.nodeId,
    control_plane: config.controlPlaneUrl,
    model_alias: config.modelAlias,
    profile_hash: config.profileHash,
    tool_registry_version: config.toolRegistryVersion,
    memory_manifest_hash: config.memoryManifestHash,
    capabilities: config.manifest.capabilities,
  });
  try {
    await worker.run();
  } finally {
    await new Promise<void>((resolve) => health.close(() => resolve()));
    await instanceLock.release();
  }
}

main().catch((error) => {
  log('error', 'worker.fatal', { detail: error instanceof Error ? error.message : String(error) });
  process.exitCode = 1;
});

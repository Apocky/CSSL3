import { open, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';

function processExists(pid: number): boolean {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export async function acquireWorkerLock(journalDir: string): Promise<{ release: () => Promise<void>; path: string }> {
  const path = join(journalDir, 'worker.lock');
  for (let pass = 0; pass < 2; pass += 1) {
    try {
      const handle = await open(path, 'wx', 0o600);
      await handle.writeFile(JSON.stringify({ pid: process.pid, started_at: new Date().toISOString() }), 'utf8');
      await handle.close();
      return { path, release: () => rm(path, { force: true }) };
    } catch (error) {
      const code = error && typeof error === 'object' && 'code' in error ? String(error.code) : '';
      if (code !== 'EEXIST') throw error;
      let existingPid = 0;
      try {
        existingPid = Number((JSON.parse(await readFile(path, 'utf8')) as { pid?: number }).pid ?? 0);
      } catch {
        // A malformed lock with no live process is stale.
      }
      if (processExists(existingPid)) throw new Error(`another Apocrypha worker is already running as PID ${existingPid}`);
      await rm(path, { force: true });
    }
  }
  throw new Error('unable to acquire Apocrypha worker lock');
}

import { mkdir, open, readFile, readdir, rename, rm, stat } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { decryptJournal, encryptJournal, type EncryptedEnvelope } from './crypto';
import type {
  AttemptJournalState,
  ClaimedJob,
  CompletionPayload,
  FailurePayload,
  JournalTerminal,
  OutputChunk,
} from './types';

function now(): string {
  return new Date().toISOString();
}

function validateState(raw: unknown): AttemptJournalState {
  if (!raw || typeof raw !== 'object') throw new Error('journal state is not an object');
  const state = raw as Partial<AttemptJournalState>;
  if (state.version !== 1 || !state.claim?.jobId || !state.claim.attemptId) {
    throw new Error('journal state is missing its claim identity');
  }
  if (!Array.isArray(state.pendingChunks)) throw new Error('journal pendingChunks is invalid');
  if (typeof state.lastAcknowledgedSeq !== 'number') throw new Error('journal acknowledgement cursor is invalid');
  return state as AttemptJournalState;
}

export class AttemptJournal {
  private readonly root: string;
  private readonly nodeToken: string;
  private readonly nodeId: string;
  private readonly writeQueues = new Map<string, Promise<void>>();

  constructor(root: string, nodeToken: string, nodeId: string) {
    this.root = root;
    this.nodeToken = nodeToken;
    this.nodeId = nodeId;
  }

  async initialize(): Promise<void> {
    await mkdir(this.root, { recursive: true, mode: 0o700 });
    await mkdir(join(this.root, 'orphaned'), { recursive: true, mode: 0o700 });
    await mkdir(join(this.root, 'corrupt'), { recursive: true, mode: 0o700 });
  }

  pathFor(attemptId: string): string {
    const safe = attemptId.replace(/[^a-zA-Z0-9_-]/g, '_');
    return join(this.root, `attempt-${safe}.journal`);
  }

  async create(claim: ClaimedJob): Promise<AttemptJournalState> {
    const timestamp = now();
    const state: AttemptJournalState = {
      version: 1,
      claim,
      pendingChunks: [],
      lastAcknowledgedSeq: -1,
      terminal: null,
      createdAt: timestamp,
      updatedAt: timestamp,
    };
    await this.save(state);
    return state;
  }

  async save(state: AttemptJournalState): Promise<void> {
    await this.initialize();
    state.updatedAt = now();
    const path = this.pathFor(state.claim.attemptId);
    const plaintext = JSON.stringify(state);
    const previous = this.writeQueues.get(path) ?? Promise.resolve();
    const write = previous.catch(() => undefined).then(async () => {
      const envelope = encryptJournal(plaintext, this.nodeToken, this.nodeId);
      const temporary = `${path}.${process.pid}.${Date.now()}.tmp`;
      const temporaryHandle = await open(temporary, 'w', 0o600);
      try {
        await temporaryHandle.writeFile(JSON.stringify(envelope), { encoding: 'utf8' });
        await temporaryHandle.sync();
      } finally {
        await temporaryHandle.close();
      }
      await rename(temporary, path);
      try {
        const directoryHandle = await open(this.root, 'r');
        try {
          await directoryHandle.sync();
        } finally {
          await directoryHandle.close();
        }
      } catch {
        // Directory fsync is unavailable on Windows. The file itself was
        // flushed before the atomic replacement and remains process-safe.
      }
      try {
        const handle = await open(path, 'r+');
        try {
          await handle.chmod(0o600);
        } finally {
          await handle.close();
        }
      } catch {
        // Windows ACLs are inherited from the protected journal directory.
      }
    });
    this.writeQueues.set(path, write);
    try {
      await write;
    } finally {
      if (this.writeQueues.get(path) === write) this.writeQueues.delete(path);
    }
  }

  async load(path: string): Promise<AttemptJournalState> {
    const raw = await readFile(path, 'utf8');
    const envelope = JSON.parse(raw) as EncryptedEnvelope;
    return validateState(JSON.parse(decryptJournal(envelope, this.nodeToken, this.nodeId)));
  }

  async list(): Promise<Array<{ path: string; state: AttemptJournalState }>> {
    await this.initialize();
    const names = (await readdir(this.root)).filter((name) => name.endsWith('.journal')).sort();
    const found: Array<{ path: string; state: AttemptJournalState }> = [];
    for (const name of names) {
      const path = join(this.root, name);
      try {
        found.push({ path, state: await this.load(path) });
      } catch {
        await this.move(path, 'corrupt');
      }
    }
    return found;
  }

  async addPendingChunk(state: AttemptJournalState, chunk: OutputChunk): Promise<void> {
    const existing = state.pendingChunks.find((item) => item.seq === chunk.seq);
    if (existing && JSON.stringify(existing) !== JSON.stringify(chunk)) {
      throw new Error(`journal sequence ${chunk.seq} already has different content`);
    }
    if (!existing) state.pendingChunks.push(chunk);
    state.pendingChunks.sort((left, right) => left.seq - right.seq);
    await this.save(state);
  }

  async acknowledgeChunk(state: AttemptJournalState, seq: number): Promise<void> {
    state.pendingChunks = state.pendingChunks.filter((item) => item.seq !== seq);
    state.lastAcknowledgedSeq = Math.max(state.lastAcknowledgedSeq, seq);
    await this.save(state);
  }

  async setCompletion(state: AttemptJournalState, payload: CompletionPayload): Promise<void> {
    await this.setTerminal(state, { kind: 'complete', payload });
  }

  async setFailure(state: AttemptJournalState, payload: FailurePayload): Promise<void> {
    await this.setTerminal(state, { kind: 'fail', payload });
  }

  private async setTerminal(state: AttemptJournalState, terminal: JournalTerminal): Promise<void> {
    if (state.terminal && JSON.stringify(state.terminal) !== JSON.stringify(terminal)) {
      throw new Error('journal terminal action cannot be replaced');
    }
    state.terminal = terminal;
    await this.save(state);
  }

  async remove(state: AttemptJournalState): Promise<void> {
    const path = this.pathFor(state.claim.attemptId);
    await this.writeQueues.get(path)?.catch(() => undefined);
    await rm(path, { force: true });
  }

  async orphan(stateOrPath: AttemptJournalState | string, reason: string): Promise<string> {
    const source = typeof stateOrPath === 'string' ? stateOrPath : this.pathFor(stateOrPath.claim.attemptId);
    const suffix = reason.replace(/[^a-zA-Z0-9_-]/g, '_').slice(0, 64);
    return this.move(source, 'orphaned', suffix);
  }

  async pendingCount(): Promise<number> {
    await this.initialize();
    const names = await readdir(this.root);
    return names.filter((name) => name.endsWith('.journal')).length;
  }

  async pruneArchives(maxAgeMs = 14 * 24 * 60 * 60 * 1_000): Promise<void> {
    const cutoff = Date.now() - maxAgeMs;
    for (const folder of ['orphaned', 'corrupt']) {
      const path = join(this.root, folder);
      for (const name of await readdir(path)) {
        const item = join(path, name);
        if ((await stat(item)).mtimeMs < cutoff) await rm(item, { force: true });
      }
    }
  }

  private async move(source: string, folder: 'orphaned' | 'corrupt', suffix: string = folder): Promise<string> {
    await this.initialize();
    const target = join(this.root, folder, `${Date.now()}-${suffix}-${basename(source)}`);
    try {
      await rename(source, target);
    } catch {
      await rm(source, { force: true });
    }
    return target;
  }
}

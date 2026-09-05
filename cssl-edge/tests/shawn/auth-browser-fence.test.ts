import assert from 'node:assert/strict';

const AUTH_MUTATION_STATE_KEY = 'apocky.auth-cookie-mutation.v1';

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>();
  failMutationWrites = false;

  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  removeItem(key: string): void { this.values.delete(key); }
  setItem(key: string, value: string): void {
    if (this.failMutationWrites && key === AUTH_MUTATION_STATE_KEY) throw new Error('fixture storage rejection');
    this.values.set(key, value);
  }
}

const local = new MemoryStorage();
const session = new MemoryStorage();
const lockManager = {
  request: async <T>(_name: string, _options: { signal: AbortSignal }, callback: () => Promise<T>): Promise<T> => callback(),
};

Object.defineProperty(globalThis, 'window', {
  configurable: true,
  value: {
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  },
});
Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: local });
Object.defineProperty(globalThis, 'sessionStorage', { configurable: true, value: session });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: lockManager } });

function reset(): void {
  local.clear();
  session.clear();
  local.failMutationWrites = false;
}

async function main(): Promise<void> {
const { persistSessionToCookie } = await import('../../lib/auth');
const authAttempt = 'fixture-auth-attempt-'.repeat(8);

reset();
let requestCount = 0;
globalThis.fetch = (async () => {
  requestCount += 1;
  return new Response('{}', { status: 200, headers: { 'Content-Type': 'application/json' } });
}) as typeof fetch;
assert.deepEqual(
  await persistSessionToCookie('access-token', { reauthenticated: true, authAttempt }),
  { status: 'established' },
  'a verified 200 plus durable browser mutation is established',
);
assert.equal(requestCount, 1, 'established mirror issues one request');

reset();
requestCount = 0;
globalThis.fetch = (async () => {
  requestCount += 1;
  local.failMutationWrites = true;
  return new Response('{}', { status: 200, headers: { 'Content-Type': 'application/json' } });
}) as typeof fetch;
assert.deepEqual(
  await persistSessionToCookie('access-token', { reauthenticated: true, authAttempt }),
  { status: 'commit_uncertain' },
  'a server 200 followed by browser-state failure remains explicitly commit-uncertain',
);
assert.equal(requestCount, 1, 'post-200 storage fault does not replay the session mutation');

reset();
requestCount = 0;
globalThis.fetch = (async () => {
  requestCount += 1;
  throw new TypeError('connection lost after dispatch');
}) as typeof fetch;
assert.deepEqual(
  await persistSessionToCookie('access-token', { reauthenticated: true, authAttempt }),
  { status: 'commit_uncertain' },
  'a post-dispatch transport failure cannot be mislabeled as a proven non-commit',
);
assert.equal(requestCount, 1, 'transport uncertainty remains single-attempt');

reset();
requestCount = 0;
globalThis.fetch = (async () => {
  requestCount += 1;
  return new Response('{}', { status: 409, headers: { 'Content-Type': 'application/json' } });
}) as typeof fetch;
assert.deepEqual(
  await persistSessionToCookie('access-token', { reauthenticated: true, authAttempt }),
  { status: 'not_established' },
  'an explicit non-success response is not established',
);

// eslint-disable-next-line no-console
console.log('shawn/auth-browser-fence.test : OK · three-state session commit boundary');
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

import { accountDiagnostic, accountDiagnosticReason, diagnosticFromBody, type AccountDiagnostic } from './diagnostics';
export interface AccountMessage { role: 'user' | 'assistant'; content: string; request_id: string; recorded_at: string }
export interface AccountSessionSummary { session_id: string; title: string; message_count: number }
export interface AccountSession { session_id: string; title: string; messages: AccountMessage[]; events_truncated: boolean }
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
export const isAccountSessionId = (value: unknown): value is string => typeof value === 'string' && UUID.test(value);
const row = (value: unknown): Record<string, unknown> | null => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
const text = (value: unknown, limit: number): value is string => typeof value === 'string' && value.length > 0 && value.length <= limit;
export function parseAccountSessions(value: unknown): { sessions: AccountSessionSummary[]; discovery_scope: string } | null {
  const body = row(value);
  if (!body || body.schema_version !== 'apocky.mobile.sessions.v1' || body.status !== 'live' || !Array.isArray(body.sessions)
    || body.sessions.length > 128 || body.count !== body.sessions.length || typeof body.discovery_scope !== 'string' || !['account_conversations', 'latest_conversation_only'].includes(body.discovery_scope)) return null;
  const sessions: AccountSessionSummary[] = [];
  for (const value of body.sessions) {
    const item = row(value);
    if (!item || !isAccountSessionId(item.session_id) || !text(item.title, 1024) || !Number.isSafeInteger(item.message_count) || Number(item.message_count) < 0) return null;
    sessions.push({ session_id: item.session_id, title: item.title, message_count: Number(item.message_count) });
  }
  return { sessions, discovery_scope: String(body.discovery_scope) };
}
export function parseAccountSession(value: unknown, expectedId: string): AccountSession | null {
  const body = row(value); const session = row(body?.session);
  if (!body || body.schema_version !== 'apocky.mobile.session.v1' || body.status !== 'live' || !session
    || session.schema_version !== 'apocky.mobile.history-session.v1' || session.session_id !== expectedId || !text(session.title, 1024)
    || !Array.isArray(session.messages) || session.messages.length > 128 || typeof session.events_truncated !== 'boolean') return null;
  const messages: AccountMessage[] = [];
  for (const value of session.messages) {
    const item = row(value);
    if (!item || typeof item.role !== 'string' || !['user', 'assistant'].includes(item.role) || !text(item.content, 128 * 1024) || !isAccountSessionId(item.request_id) || !text(item.recorded_at, 64)) return null;
    messages.push({ role: item.role as AccountMessage['role'], content: item.content, request_id: item.request_id, recorded_at: item.recorded_at });
  }
  return { session_id: expectedId, title: session.title, messages, events_truncated: session.events_truncated };
}
export function parseAccountTurn(value: unknown, sessionId: string, requestId: string): { text: string } | null {
  const body = row(value);
  if (!body || body.schema_version !== 'apocky.mobile.turn.v1' || body.status !== 'completed' || body.session_id !== sessionId || body.request_id !== requestId
    || !text(body.text, 256 * 1024) || !text(body.model_id, 512) || typeof body.response_digest !== 'string' || !/^[0-9a-f]{64}$/.test(body.response_digest)) return null;
  return { text: body.text };
}


export interface AccountPendingTurn { text: string; session_id: string; request_id: string }
export class AccountTurnFailure extends Error {
  constructor(readonly diagnostic: AccountDiagnostic) { super(accountDiagnosticReason(diagnostic)); }
}
export function sameAccountTurn(left: AccountPendingTurn, right: AccountPendingTurn): boolean {
  return left.request_id === right.request_id && left.session_id === right.session_id && left.text === right.text;
}
export function validAccountPendingTurn(value: unknown): value is AccountPendingTurn {
  const item = row(value);
  return Boolean(item && Object.keys(item).sort().join(',') === 'request_id,session_id,text'
    && isAccountSessionId(item.request_id) && isAccountSessionId(item.session_id)
    && typeof item.text === 'string' && item.text.length > 0 && item.text === item.text.trim()
    && new TextEncoder().encode(item.text).byteLength <= 16_384);
}
export function isAccountAdmissionPending(value: unknown, status: number, turn: AccountPendingTurn): boolean {
  const item = row(value); const diagnostic = row(item?.diagnostic);
  return Boolean(status === 503 && item && validAccountPendingTurn(turn)
    && Object.keys(item).sort().join(',') === 'code,diagnostic,error,request_id,retry_after_ms,schema_version,session_id,stage,status'
    && item.schema_version === 'apocky.mobile.turn-pending.v1' && item.status === 'pending'
    && item.code === 'ACCOUNT_ADMISSION_PENDING' && item.stage === 'reply' && typeof item.error === 'string'
    && item.request_id === turn.request_id && item.session_id === turn.session_id && item.retry_after_ms === 1000
    && diagnostic?.schema_version === 'apocky.mobile.diagnostic.v1' && diagnostic.operation === 'turn'
    && diagnostic.status === 503 && diagnostic.code === item.code && diagnostic.stage === 'reply');
}
export function accountHistoryCompletes(history: AccountSession, turn: AccountPendingTurn): boolean {
  if (history.session_id !== turn.session_id) return false;
  const own = history.messages.filter(message => message.request_id === turn.request_id);
  return own.length === 2 && own[0]?.role === 'user' && own[0].content === turn.text && own[1]?.role === 'assistant' && own[1].content.length > 0;
}
export function accountHistoryWithPending(history: AccountSession, turn: AccountPendingTurn | null): AccountMessage[] {
  if (!turn || turn.session_id !== history.session_id) return history.messages;
  const users = history.messages.filter(message => message.role === 'user' && message.request_id === turn.request_id);
  if (users.some(message => message.content !== turn.text)) throw pendingFailure('ACCOUNT_PENDING_CONFLICT');
  return users.length ? history.messages : [...history.messages, { role: 'user', content: turn.text, request_id: turn.request_id, recorded_at: new Date().toISOString() }];
}
function pendingFailure(code: 'ACCOUNT_PENDING_CONFLICT' | 'ACCOUNT_PENDING_STORAGE_UNAVAILABLE'): AccountTurnFailure {
  return new AccountTurnFailure(accountDiagnostic({ operation: 'turn', status: 0, code }));
}
function aborted(): Error { const error = new Error('Stopped waiting.'); error.name = 'AbortError'; return error; }
function checkSignal(signal: AbortSignal): void { if (signal.aborted) throw aborted(); }
export function waitForAccountRetry(milliseconds: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(aborted()); return; }
    const stop = () => { clearTimeout(timer); signal.removeEventListener('abort', stop); reject(aborted()); };
    const timer = setTimeout(() => { signal.removeEventListener('abort', stop); resolve(); }, milliseconds);
    signal.addEventListener('abort', stop, { once: true });
  });
}
async function accountBody(response: Response, signal: AbortSignal): Promise<unknown> {
  if (response.redirected || response.headers.get('content-type')?.split(';', 1)[0]?.trim().toLowerCase() !== 'application/json') return null;
  const reader = response.body?.getReader(); if (!reader) return null;
  const stop = () => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener('abort', stop, { once: true });
  try {
    const chunks: Uint8Array[] = []; let length = 0;
    for (;;) { checkSignal(signal); const next = await reader.read(); checkSignal(signal); if (next.done) break;
      length += next.value.byteLength; if (length > 262_144) return null; chunks.push(next.value); }
    const bytes = new Uint8Array(length); let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)) as unknown;
  } catch (error) { checkSignal(signal); return null; }
  finally { signal.removeEventListener('abort', stop); void reader.cancel().catch(() => undefined); reader.releaseLock(); }
}
export function createAccountBoundTurnFetcher(
  account: string,
  getSession: () => Promise<{ access_token: string; user: { id: string } } | null>,
  fetcher: typeof fetch = fetch,
): typeof fetch {
  return async (input, init = {}) => {
    if (!isAccountSessionId(account) || input !== '/api/mobile/turn' || init.method !== 'POST') {
      throw new AccountTurnFailure(accountDiagnostic({ operation: 'turn', status: 0, code: 'ACCOUNT_REQUEST_INVALID' }));
    }
    if (init.signal?.aborted) throw aborted();
    let session: Awaited<ReturnType<typeof getSession>>;
    try { session = await getSession(); }
    catch { throw new AccountTurnFailure(accountDiagnostic({ operation: 'turn', status: 0, code: 'ACCOUNT_SIGN_IN_UNAVAILABLE' })); }
    if (init.signal?.aborted) throw aborted();
    if (!session || typeof session.access_token !== 'string' || !session.access_token) {
      throw new AccountTurnFailure(accountDiagnostic({ operation: 'turn', status: 0, code: 'ACCOUNT_SIGN_IN_REQUIRED' }));
    }
    if (session.user?.id !== account) {
      throw new AccountTurnFailure(accountDiagnostic({ operation: 'turn', status: 0, code: 'ACCOUNT_RESPONSE_SCOPE_MISMATCH' }));
    }
    // W! every attempt uses the captured account's token; cookies cannot substitute another account.
    const headers = new Headers(init.headers); headers.delete('Cookie');
    headers.set('Authorization', 'Bearer ' + session.access_token);
    return fetcher(input, { ...init, headers, credentials: 'omit', redirect: 'error' });
  };
}

export async function sendAccountPendingTurn(input: {
  turn: AccountPendingTurn; fetcher: typeof fetch; signal: AbortSignal;
  onPending?: () => void; wait?: typeof waitForAccountRetry;
}): Promise<{ text: string }> {
  if (!validAccountPendingTurn(input.turn)) throw pendingFailure('ACCOUNT_PENDING_CONFLICT');
  // W! same body + same request identity ∀ bounded pending retries ; other failures never auto-retry.
  const body = JSON.stringify(input.turn); const controller = new AbortController();
  const stop = () => controller.abort(); input.signal.addEventListener('abort', stop, { once: true });
  const deadline = setTimeout(stop, 120_000); if (input.signal.aborted) stop();
  try {
    for (let attempt = 0; ; attempt += 1) {
      checkSignal(controller.signal);
      const response = await input.fetcher('/api/mobile/turn', { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body, signal: controller.signal, cache: 'no-store' });
      const value = await accountBody(response, controller.signal); checkSignal(controller.signal);
      if (response.ok) {
        const result = parseAccountTurn(value, input.turn.session_id, input.turn.request_id);
        if (!result) throw new AccountTurnFailure(accountDiagnostic({ operation: 'turn', status: response.status, code: 'ACCOUNT_TURN_UNVERIFIED', trace_id: response.headers.get('x-apocky-trace-id') }));
        return result;
      }
      const diagnostic = diagnosticFromBody(value, 'turn', response.status, response.headers.get('x-apocky-trace-id'));
      if (!isAccountAdmissionPending(value, response.status, input.turn) || attempt >= 4) throw new AccountTurnFailure(diagnostic);
      input.onPending?.(); await (input.wait ?? waitForAccountRetry)(1000 * (2 ** attempt), controller.signal);
    }
  } finally { clearTimeout(deadline); input.signal.removeEventListener('abort', stop); }
}

export interface AccountPendingRecord { schema_version: 'apocky.account-pending.encrypted.v1'; key: CryptoKey; iv: Uint8Array; ciphertext: ArrayBuffer }
export interface AccountPendingStorage {
  read(account: string): Promise<AccountPendingRecord | null>;
  write(account: string, record: AccountPendingRecord): Promise<void>;
  remove(account: string): Promise<void>;
}
export type AccountPendingLock = <T>(name: string, operation: () => Promise<T>, signal?: AbortSignal) => Promise<T>;
export const browserAccountPendingLock: AccountPendingLock = async (name, operation, signal) => {
  if (typeof navigator === 'undefined' || !navigator.locks) throw pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE');
  const controller = new AbortController(); const stop = () => controller.abort();
  signal?.addEventListener('abort', stop, { once: true }); if (signal?.aborted) stop();
  const deadline = setTimeout(stop, 120_000);
  try { return await navigator.locks.request(name, { mode: 'exclusive', signal: controller.signal }, operation); }
  finally { clearTimeout(deadline); signal?.removeEventListener('abort', stop); }
};
export class AccountPendingJournal {
  constructor(private readonly storage: AccountPendingStorage, private readonly cryptoHost: Crypto = crypto,
    private readonly lock: AccountPendingLock = browserAccountPendingLock) {}
  private scope(account: string): string { if (!isAccountSessionId(account)) throw pendingFailure('ACCOUNT_PENDING_CONFLICT'); return 'apocky.account-pending.v1.' + account; }
  private async read(account: string): Promise<AccountPendingTurn | null> {
    const record = await this.storage.read(account); if (!record) return null;
    try {
      if (record.schema_version !== 'apocky.account-pending.encrypted.v1' || record.iv.byteLength !== 12 || record.ciphertext.byteLength > 131_072
        || record.key.extractable || record.key.algorithm.name !== 'AES-GCM') throw Error('invalid');
      const bytes = await this.cryptoHost.subtle.decrypt({ name: 'AES-GCM', iv: record.iv as BufferSource,
        additionalData: new TextEncoder().encode(this.scope(account)) }, record.key, record.ciphertext);
      try { const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
        if (!validAccountPendingTurn(value)) throw Error('invalid'); return value;
      } finally { new Uint8Array(bytes).fill(0); }
    } catch { throw pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE'); }
  }
  load(account: string): Promise<AccountPendingTurn | null> { return this.lock(this.scope(account), () => this.read(account)); }
  save(account: string, turn: AccountPendingTurn): Promise<void> {
    return this.lock(this.scope(account), async () => {
      if (!validAccountPendingTurn(turn)) throw pendingFailure('ACCOUNT_PENDING_CONFLICT');
      const previous = await this.read(account);
      if (previous) { if (!sameAccountTurn(previous, turn)) throw pendingFailure('ACCOUNT_PENDING_CONFLICT'); return; }
      const key = await this.cryptoHost.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
      const iv = this.cryptoHost.getRandomValues(new Uint8Array(12)); const bytes = new TextEncoder().encode(JSON.stringify(turn));
      try { const ciphertext = await this.cryptoHost.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: new TextEncoder().encode(this.scope(account)) }, key, bytes);
        await this.storage.write(account, { schema_version: 'apocky.account-pending.encrypted.v1', key, iv, ciphertext });
      } finally { bytes.fill(0); }
    });
  }
  resolve(account: string, turn: AccountPendingTurn): Promise<boolean> {
    return this.lock(this.scope(account), async () => {
      const current = await this.read(account); if (!current || !sameAccountTurn(current, turn)) return false;
      await this.storage.remove(account); return true;
    });
  }
  deliver(account: string, turn: AccountPendingTurn, input: Omit<Parameters<typeof sendAccountPendingTurn>[0], 'turn'>): Promise<{ text: string } | null> {
    return this.lock(this.scope(account) + '.delivery', async () => {
      const current = await this.load(account); if (!current) return null;
      if (!sameAccountTurn(current, turn)) throw pendingFailure('ACCOUNT_PENDING_CONFLICT');
      const result = await sendAccountPendingTurn({ ...input, turn: current });
      await this.resolve(account, current); return result;
    }, input.signal);
  }
}
export function openAccountPendingJournal(): Promise<AccountPendingJournal> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') { reject(pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE')); return; }
    const request = indexedDB.open('apocky.account-pending.v1', 1);
    request.onupgradeneeded = () => { if (!request.result.objectStoreNames.contains('pending')) request.result.createObjectStore('pending'); };
    request.onerror = () => reject(pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE'));
    request.onblocked = () => reject(pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE'));
    request.onsuccess = () => {
      const database = request.result; database.onversionchange = () => database.close();
      const transact = <T>(mode: IDBTransactionMode, operation: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> => new Promise((done, fail) => {
        try { const transaction = database.transaction('pending', mode); const item = operation(transaction.objectStore('pending'));
          transaction.oncomplete = () => done(item.result);
          transaction.onerror = transaction.onabort = () => fail(pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE'));
        } catch { fail(pendingFailure('ACCOUNT_PENDING_STORAGE_UNAVAILABLE')); }
      });
      resolve(new AccountPendingJournal({
        read: async account => (await transact<AccountPendingRecord | undefined>('readonly', store => store.get(account))) ?? null,
        write: async (account, value) => { await transact('readwrite', store => store.put(value, account)); },
        remove: async account => { await transact('readwrite', store => store.delete(account)); },
      }));
    };
  });
}

import type { BrainSnapshot } from './contracts';
import {
  MINI_BRAIN_SYNC_REQUEST_SCHEMA,
  bytesToBase64Url,
  canonicalJson,
  sha256Hex,
  syncSigningPayload,
  type MiniBrainSyncPayload,
  type MiniBrainSyncRequest,
  type MiniBrainSyncResponse,
  type MiniBrainSyncUnsignedRequest,
} from './mobile-contracts';

const DB_NAME = 'apocky-mini-brain-v1';
const DB_VERSION = 1;
const META_STORE = 'device';
const VAULT_STORE = 'vault';
const IDENTITY_KEY = 'identity';
const VAULT_KEY = 'state';
const MAX_SESSIONS = 6;
const MAX_MESSAGES = 80;
const MAX_MEMORIES = 120;
const MAX_QUEUE = 32;
const SHELL_CACHE = 'apocky-mini-brain-shell-v2';
const LEGACY_SHELL_CACHE = 'apocky-mini-brain-shell-v1';

export type MiniBrainVaultState = 'ready' | 'empty' | 'unbound' | 'unavailable';

interface DeviceIdentity {
  readonly key: typeof IDENTITY_KEY;
  readonly device_id: string;
  readonly signing_private_key: CryptoKey;
  readonly signing_public_jwk: JsonWebKey;
  readonly encryption_key: CryptoKey;
  readonly owner_ref: string | null;
  readonly device_token: string | null;
  readonly token_expires_at: string | null;
  readonly next_sequence: number;
}

interface EncryptedVault {
  readonly revision?: number;
  readonly key: typeof VAULT_KEY;
  readonly iv: ArrayBuffer;
  readonly ciphertext: ArrayBuffer;
}

export interface MiniBrainMessage {
  readonly id: string;
  readonly role: 'user' | 'assistant';
  readonly content: string;
  readonly recorded_at: string;
  readonly request_id: string;
  readonly event_digest: string | null;
  readonly origin: 'desktop' | 'queued-mobile' | 'local-reflection';
  readonly provenance_digests: readonly string[];
  readonly terminal_failure?: {
    readonly code: 'engine_failure' | 'chat_prompt_capacity_exceeded';
    readonly error_digest: string;
    readonly receipt_digest: string;
  };
}

export interface MiniBrainSession {
  readonly session_id: string;
  readonly cursor: string | null;
  readonly messages: readonly MiniBrainMessage[];
  readonly updated_at: string;
  readonly events_truncated: boolean;
  readonly tombstoned_at: string | null;
}

export interface MiniBrainMemory {
  readonly id_digest: string;
  readonly topic: string;
  readonly paraphrase: string;
  readonly created_at: string;
  readonly source_digests: readonly string[];
  readonly record_digest: string;
}

export interface MiniBrainQueuedTurn {
  readonly request_id: string;
  readonly session_id: string;
  readonly text: string;
  readonly queued_at: string;
  readonly base_cursor: string | null;
  readonly local_message_ids: readonly string[];
  // § local observation only; never enters the signed request or grants delivery authority.
  readonly admission_pending?: true;
}

export interface MiniBrainState {
  readonly revision?: number;
  readonly schema_version: 'apocky.mini-brain.local-state.v1';
  readonly owner_ref: string;
  readonly device_id: string;
  readonly current_session_id: string;
  readonly selection_origin?: 'provisional' | 'user' | 'remote';
  readonly sessions: readonly MiniBrainSession[];
  readonly memories: readonly MiniBrainMemory[];
  readonly queue: readonly MiniBrainQueuedTurn[];
  readonly updated_at: string;
}

export interface MiniBrainRequestIntent {
  readonly operation: 'pull' | 'append';
  readonly sessionId: string;
  readonly requestId: string;
  readonly baseCursor: string | null;
  readonly payload: MiniBrainSyncPayload | null;
}

export interface MiniBrainDeviceRegistration {
  readonly device_token: string;
  readonly owner_ref: string;
  readonly expires_at: string;
}

export async function registerMiniBrainOfflineShell(): Promise<boolean> {
  if (!('serviceWorker' in navigator) || !('caches' in globalThis)) return false;
  const workerUrl = new URL('/brain-sw.js', location.origin).href;
  const rootScope = new URL('/', location.origin).href;
  for (const registration of await navigator.serviceWorker.getRegistrations()) {
    const workers = [registration.active, registration.waiting, registration.installing];
    if (
      registration.scope === rootScope
      && workers.some(worker => worker !== null)
      && workers.every(worker => worker === null || worker.scriptURL === workerUrl)
    ) {
      await registration.unregister();
    }
  }
  await caches.delete(LEGACY_SHELL_CACHE);
  await navigator.serviceWorker.register(workerUrl, { scope: '/brain' });
  return warmMiniBrainOfflineShell();
}

export async function warmMiniBrainOfflineShell(): Promise<boolean> {
  if (!('serviceWorker' in navigator) || !('caches' in globalThis)) return false;
  await navigator.serviceWorker.ready;
  const cache = await caches.open(SHELL_CACHE);
  if (!navigator.onLine) return Boolean(await cache.match('/brain'));

  const shell = await fetch('/brain', {
    credentials: 'same-origin',
    cache: 'no-store',
    redirect: 'error',
    headers: { Accept: 'text/html' },
  });
  if (!shell.ok || new URL(shell.url).pathname !== '/brain') return false;
  const html = await shell.text();
  if (!html.includes('"serverAccess":"owner"')) return false;
  await cache.put('/brain', new Response(html, {
    status: shell.status,
    statusText: shell.statusText,
    headers: shell.headers,
  }));

  const assetUrls = new Set<string>();
  for (const element of document.querySelectorAll<HTMLScriptElement | HTMLLinkElement>('script[src],link[href]')) {
    const raw = element instanceof HTMLScriptElement ? element.src : element.href;
    if (!raw) continue;
    const url = new URL(raw, location.href);
    if (url.origin === location.origin && url.pathname.startsWith('/_next/static/')) assetUrls.add(url.href);
  }
  for (const entry of performance.getEntriesByType('resource')) {
    const url = new URL(entry.name, location.href);
    if (url.origin === location.origin && url.pathname.startsWith('/_next/static/')) assetUrls.add(url.href);
  }
  const outcomes = await Promise.all([...assetUrls].map(async url => {
    try {
      const response = await fetch(url, { credentials: 'same-origin', cache: 'no-store' });
      if (!response.ok) return false;
      await cache.put(url, response);
      return true;
    } catch {
      return false;
    }
  }));
  return assetUrls.size > 0 && outcomes.every(Boolean);
}

interface MiniBrainOpenResult {
  readonly state: MiniBrainVaultState;
  readonly vault: MiniBrainVault | null;
  readonly reason_code: string | null;
}

// § writer.scope := origin-wide Web Lock; fallback serialized within this realm.
// § CAS below rejects competing legacy or unsupported-context writes.
let vaultWriterTail: Promise<unknown> = Promise.resolve();
async function withVaultWriter<T>(operation: () => Promise<T>): Promise<T> {
  if (typeof navigator !== 'undefined' && navigator.locks?.request) {
    return navigator.locks.request(DB_NAME + ':writer', { mode: 'exclusive' }, operation);
  }
  const pending = vaultWriterTail.then(operation, operation);
  vaultWriterTail = pending.then(() => undefined, () => undefined);
  return pending;
}

const VAULT_CHANGES = DB_NAME + ':changes';
function notifyVaultChange(identity: DeviceIdentity, revision: number): void {
  if (typeof BroadcastChannel === 'undefined') return;
  try {
    const channel = new BroadcastChannel(VAULT_CHANGES);
    channel.postMessage({ device_id: identity.device_id, owner_ref: identity.owner_ref, revision });
    channel.close();
  } catch { /* Durable commit remains valid; focus readback is the fallback. */ }
}

const deliveryTails = new Map<string, Promise<void>>();
function deliveryAbortReason(signal: AbortSignal): Error {
  return signal.reason instanceof Error ? signal.reason : new Error('MINI_BRAIN_DELIVERY_ABORTED');
}
function abortableDelivery<T>(pending: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(deliveryAbortReason(signal));
    if (signal.aborted) abort();
    else signal.addEventListener('abort', abort, { once: true });
    pending.then(resolve, reject).finally(() => signal.removeEventListener('abort', abort));
  });
}
const ADMISSION_RETRY_DELAYS_MS = [1000, 2000, 4000, 8000] as const;
function admissionPendingFor(error: unknown, input: MiniBrainRequestIntent): boolean {
  if (input.operation !== 'append' || !error || typeof error !== 'object') return false;
  const value = error as { code?: unknown; status?: unknown; payload?: unknown };
  if (value.code !== 'BRAIN_APEX_ADMISSION_PENDING' || value.status !== 503
    || !value.payload || typeof value.payload !== 'object' || Array.isArray(value.payload)) return false;
  const payload = value.payload as Record<string, unknown>;
  return payload.code === 'BRAIN_APEX_ADMISSION_PENDING' && payload.request_id === input.requestId
    && payload.session_id === input.sessionId && payload.retry_after_ms === 1000;
}
function waitForAdmissionRetry(delay: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const abort = () => { clearTimeout(timer); signal.removeEventListener('abort', abort); reject(deliveryAbortReason(signal)); };
    const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve(); }, delay);
    if (signal.aborted) abort();
    else signal.addEventListener('abort', abort, { once: true });
  });
}

async function withDeliveryLock<T>(deviceId: string, signal: AbortSignal, operation: () => Promise<T>): Promise<T> {
  const name = DB_NAME + ':delivery:' + deviceId;
  const run = () => {
    if (signal.aborted) throw deliveryAbortReason(signal);
    return abortableDelivery(operation(), signal);
  };
  if (typeof navigator !== 'undefined' && navigator.locks?.request) {
    return navigator.locks.request(name, { mode: 'exclusive', signal }, run);
  }
  if (typeof indexedDB !== 'undefined') throw new Error('MINI_BRAIN_DELIVERY_LOCK_UNAVAILABLE');
  // § realm fallback := non-browser disposable storage only; real vaults fail closed.
  const previous = deliveryTails.get(name) ?? Promise.resolve();
  const pending = previous.then(run);
  const tail = pending.then(() => undefined, () => undefined);
  deliveryTails.set(name, tail);
  void tail.then(() => { if (deliveryTails.get(name) === tail) deliveryTails.delete(name); });
  return abortableDelivery(pending, signal);
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('MINI_BRAIN_IDB_REQUEST_FAILED'));
  });
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error ?? new Error('MINI_BRAIN_IDB_TRANSACTION_ABORTED'));
    transaction.onerror = () => reject(transaction.error ?? new Error('MINI_BRAIN_IDB_TRANSACTION_FAILED'));
  });
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(META_STORE)) database.createObjectStore(META_STORE, { keyPath: 'key' });
      if (!database.objectStoreNames.contains(VAULT_STORE)) database.createObjectStore(VAULT_STORE, { keyPath: 'key' });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('MINI_BRAIN_IDB_OPEN_FAILED'));
    request.onblocked = () => reject(new Error('MINI_BRAIN_IDB_BLOCKED'));
  });
}

async function readIdentity(database: IDBDatabase): Promise<DeviceIdentity | null> {
  const transaction = database.transaction(META_STORE, 'readonly');
  const done = transactionDone(transaction);
  const value = await requestResult(transaction.objectStore(META_STORE).get(IDENTITY_KEY));
  await done;
  return value && typeof value === 'object' ? value as DeviceIdentity : null;
}

async function writeIdentity(database: IDBDatabase, identity: DeviceIdentity): Promise<void> {
  const transaction = database.transaction(META_STORE, 'readwrite');
  const done = transactionDone(transaction);
  transaction.objectStore(META_STORE).put(identity);
  await done;
}

async function reserveSigningIdentity(database: IDBDatabase, expected: DeviceIdentity): Promise<DeviceIdentity> {
  const transaction = database.transaction(META_STORE, 'readwrite');
  const done = transactionDone(transaction);
  const store = transaction.objectStore(META_STORE);
  const request = store.get(IDENTITY_KEY);
  let reserved: DeviceIdentity | null = null;
  let failure: Error | null = null;
  request.onsuccess = () => {
    const current = request.result as DeviceIdentity | undefined;
    if (!current || current.device_id !== expected.device_id || current.owner_ref !== expected.owner_ref) {
      failure = new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
    } else if (!current.owner_ref || !current.device_token) {
      failure = new Error('MINI_BRAIN_DEVICE_UNBOUND');
    } else if (!Number.isFinite(Date.parse(current.token_expires_at ?? ''))
      || Date.parse(current.token_expires_at!) <= Date.now() + 60_000) {
      failure = new Error('MINI_BRAIN_DEVICE_TOKEN_EXPIRED');
    } else if (!Number.isSafeInteger(current.next_sequence) || current.next_sequence < 1
      || current.next_sequence === Number.MAX_SAFE_INTEGER) {
      failure = new Error('MINI_BRAIN_SEQUENCE_INVALID');
    }
    if (failure || !current) {
      transaction.abort();
      return;
    }
    reserved = current;
    store.put({ ...current, next_sequence: current.next_sequence + 1 });
  };
  await done.catch(error => { throw failure ?? error; });
  if (!reserved) throw new Error('MINI_BRAIN_SEQUENCE_RESERVATION_FAILED');
  return reserved;
}

async function ensureIdentity(database: IDBDatabase): Promise<DeviceIdentity> {
  const existing = await readIdentity(database);
  if (existing) return existing;
  if (!globalThis.crypto?.subtle || typeof globalThis.crypto.randomUUID !== 'function') {
    throw new Error('MINI_BRAIN_WEBCRYPTO_UNAVAILABLE');
  }
  const signing = await crypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    false,
    ['sign', 'verify'],
  ) as CryptoKeyPair;
  const publicJwk = await crypto.subtle.exportKey('jwk', signing.publicKey);
  const encryptionKey = await crypto.subtle.generateKey(
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  );
  const identity: DeviceIdentity = {
    key: IDENTITY_KEY,
    device_id: crypto.randomUUID(),
    signing_private_key: signing.privateKey,
    signing_public_jwk: publicJwk,
    encryption_key: encryptionKey,
    owner_ref: null,
    device_token: null,
    token_expires_at: null,
    next_sequence: 1,
  };
  await writeIdentity(database, identity);
  return identity;
}

function vaultAad(identity: DeviceIdentity): Uint8Array {
  if (!identity.owner_ref) throw new Error('MINI_BRAIN_DEVICE_UNBOUND');
  return new TextEncoder().encode(`apocky.mini-brain.v1\u0000${identity.owner_ref}\u0000${identity.device_id}`);
}

function defaultState(identity: DeviceIdentity, sessionId?: string): MiniBrainState {
  if (!identity.owner_ref) throw new Error('MINI_BRAIN_DEVICE_UNBOUND');
  return {
    schema_version: 'apocky.mini-brain.local-state.v1',
    owner_ref: identity.owner_ref,
    device_id: identity.device_id,
    current_session_id: sessionId ?? crypto.randomUUID(),
    selection_origin: sessionId ? 'user' : 'provisional',
    sessions: [],
    memories: [],
    queue: [],
    updated_at: new Date().toISOString(),
  };
}

function boundedState(state: MiniBrainState): MiniBrainState {
  const sessionIds = new Set<string>();
  const sessions = [...state.sessions]
    .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
    .filter(session => {
      if (sessionIds.has(session.session_id)) return false;
      sessionIds.add(session.session_id);
      return true;
    })
    .slice(0, MAX_SESSIONS)
    .map(session => ({ ...session, messages: session.messages.slice(-MAX_MESSAGES) }));
  return {
    ...state,
    sessions,
    memories: state.memories.slice(0, MAX_MEMORIES),
    queue: state.queue.slice(0, MAX_QUEUE).map(({ admission_pending, ...turn }) =>
      admission_pending === true ? { ...turn, admission_pending: true as const } : turn),
    updated_at: new Date().toISOString(),
  };
}

async function readVault(database: IDBDatabase, identity: DeviceIdentity): Promise<MiniBrainState | null> {
  if (!identity.owner_ref) return null;
  const transaction = database.transaction(VAULT_STORE, 'readonly');
  const done = transactionDone(transaction);
  const stored = await requestResult(transaction.objectStore(VAULT_STORE).get(VAULT_KEY));
  await done;
  if (!stored || typeof stored !== 'object') return null;
  const vault = stored as EncryptedVault;
  try {
    const plaintext = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: new Uint8Array(vault.iv), additionalData: vaultAad(identity) },
      identity.encryption_key,
      vault.ciphertext,
    );
    const parsed = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(plaintext)) as MiniBrainState;
    if (
      parsed.schema_version !== 'apocky.mini-brain.local-state.v1'
      || parsed.owner_ref !== identity.owner_ref
      || parsed.device_id !== identity.device_id
    ) throw new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
    return boundedState({ ...parsed, revision: vault.revision ?? 0 });
  } catch {
    throw new Error('MINI_BRAIN_VAULT_DECRYPT_FAILED');
  }
}

async function writeVault(database: IDBDatabase, identity: DeviceIdentity, value: MiniBrainState): Promise<MiniBrainState> {
  const expectedRevision = value.revision ?? 0;
  if (!Number.isSafeInteger(expectedRevision) || expectedRevision < 0 || expectedRevision === Number.MAX_SAFE_INTEGER) {
    throw new Error('MINI_BRAIN_REVISION_INVALID');
  }
  const state = boundedState({ ...value, revision: expectedRevision + 1 });
  if (state.owner_ref !== identity.owner_ref || state.device_id !== identity.device_id) {
    throw new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
  }
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv, additionalData: vaultAad(identity) },
    identity.encryption_key,
    new TextEncoder().encode(canonicalJson(state)),
  );
  const transaction = database.transaction(VAULT_STORE, 'readwrite');
  const done = transactionDone(transaction);
  const store = transaction.objectStore(VAULT_STORE);
  const current = store.get(VAULT_KEY);
  let stale = false;
  current.onsuccess = () => {
    if ((current.result?.revision ?? 0) !== expectedRevision) {
      stale = true;
      transaction.abort();
      return;
    }
    store.put({ key: VAULT_KEY, revision: state.revision, iv: iv.buffer, ciphertext } satisfies EncryptedVault);
  };
  await done.catch(error => { throw stale ? new Error('MINI_BRAIN_STALE_WRITE') : error; });
  notifyVaultChange(identity, state.revision!);
  return state;
}

export function normalizeMiniBrainRemoteMessages(messages: readonly Record<string, unknown>[]): MiniBrainMessage[] {
  return messages.flatMap((message): MiniBrainMessage[] => {
    if (
      (message.role !== 'user' && message.role !== 'assistant')
      || typeof message.content !== 'string'
      || typeof message.request_id !== 'string'
      || typeof message.recorded_at !== 'string'
    ) return [];
    const failure = message.terminal_failure;
    if (failure !== undefined && (message.role !== 'user' || failure === null
      || typeof failure !== 'object' || Array.isArray(failure)
      || Object.keys(failure).sort().join(',') !== 'code,error_digest,receipt_digest'
      || !['engine_failure', 'chat_prompt_capacity_exceeded'].includes(String((failure as Record<string, unknown>).code))
      || !['error_digest', 'receipt_digest'].every(key => {
        const digest = (failure as Record<string, unknown>)[key];
        return typeof digest === 'string' && /^[0-9a-f]{64}$/u.test(digest);
      }))) {
      throw new Error('MINI_BRAIN_TERMINAL_FAILURE_INVALID');
    }
    const receipt = message.receipt && typeof message.receipt === 'object' && !Array.isArray(message.receipt)
      ? message.receipt as Record<string, unknown>
      : null;
    const context = receipt?.context && typeof receipt.context === 'object' && !Array.isArray(receipt.context)
      ? receipt.context as Record<string, unknown>
      : null;
    const digests = ['frame_digest', 'provenance_spine_digest']
      .map(key => context?.[key])
      .filter((value): value is string => typeof value === 'string' && /^[0-9a-f]{64}$/u.test(value));
    return [{
      id: typeof message.event_digest === 'string' ? message.event_digest : `${message.role}-${message.request_id}`,
      role: message.role,
      content: message.content,
      request_id: message.request_id,
      recorded_at: message.recorded_at,
      event_digest: typeof message.event_digest === 'string' ? message.event_digest : null,
      origin: 'desktop',
      provenance_digests: digests,
      ...(failure === undefined ? {} : { terminal_failure: failure as MiniBrainMessage['terminal_failure'] }),
    }];
  });
}

export class MiniBrainVault {
  private constructor(
    private readonly database: IDBDatabase,
    private identityValue: DeviceIdentity,
  ) {}

  static async open(): Promise<MiniBrainVault> {
    const database = await openDatabase();
    const identity = await withVaultWriter(() => ensureIdentity(database));
    return new MiniBrainVault(database, identity);
  }

  get deviceId(): string { return this.identityValue.device_id; }
  get publicKeyJwk(): JsonWebKey { return this.identityValue.signing_public_jwk; }
  get isBound(): boolean { return Boolean(this.identityValue.owner_ref && this.identityValue.device_token); }
  get tokenExpired(): boolean {
    const expires = this.identityValue.token_expires_at ? Date.parse(this.identityValue.token_expires_at) : 0;
    return !Number.isFinite(expires) || expires <= Date.now() + 60_000;
  }

  async bind(registration: MiniBrainDeviceRegistration): Promise<MiniBrainState> {
    return withVaultWriter(() => this.bindUnlocked(registration));
  }

  private async bindUnlocked(registration: MiniBrainDeviceRegistration): Promise<MiniBrainState> {
    const currentIdentity = await readIdentity(this.database);
    if (!currentIdentity || currentIdentity.device_id !== this.identityValue.device_id) {
      throw new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
    }
    this.identityValue = currentIdentity;
    const ownerChanged = this.identityValue.owner_ref !== null && this.identityValue.owner_ref !== registration.owner_ref;
    if (ownerChanged) {
      const transaction = this.database.transaction(VAULT_STORE, 'readwrite');
      const done = transactionDone(transaction);
      transaction.objectStore(VAULT_STORE).delete(VAULT_KEY);
      await done;
    }
    this.identityValue = {
      ...this.identityValue,
      owner_ref: registration.owner_ref,
      device_token: registration.device_token,
      token_expires_at: registration.expires_at,
      next_sequence: ownerChanged ? 1 : this.identityValue.next_sequence,
    };
    await writeIdentity(this.database, this.identityValue);
    const existing = await readVault(this.database, this.identityValue);
    return existing ?? writeVault(this.database, this.identityValue, defaultState(this.identityValue));
  }

  subscribe(listener: () => void): () => void {
    if (typeof BroadcastChannel === 'undefined') return () => undefined;
    let channel: BroadcastChannel;
    try { channel = new BroadcastChannel(VAULT_CHANGES); } catch { return () => undefined; }
    let lastRevision = -1;
    channel.onmessage = event => {
      const change = event.data as { device_id?: unknown; owner_ref?: unknown; revision?: unknown } | null;
      if (!change || change.device_id !== this.identityValue.device_id || change.owner_ref !== this.identityValue.owner_ref
        || typeof change.revision !== 'number' || !Number.isSafeInteger(change.revision) || change.revision <= lastRevision) return;
      lastRevision = change.revision;
      listener();
    };
    return () => { channel.onmessage = null; channel.close(); };
  }

  async load(): Promise<MiniBrainState | null> {
    return readVault(this.database, this.identityValue);
  }

  private async saveUnlocked(state: MiniBrainState): Promise<MiniBrainState> {
    return writeVault(this.database, this.identityValue, state);
  }

  async save(state: MiniBrainState): Promise<MiniBrainState> {
    return withVaultWriter(() => this.saveUnlocked(state));
  }

  async freshState(sessionId?: string): Promise<MiniBrainState> {
    return withVaultWriter(async () => await this.load()
      ?? this.saveUnlocked(defaultState(this.identityValue, sessionId)));
  }

  private async mutateCurrent(
    expected: MiniBrainState,
    mutation: (state: MiniBrainState) => MiniBrainState,
  ): Promise<MiniBrainState> {
    return withVaultWriter(async () => {
      const latest = await this.load() ?? expected;
      if (latest.owner_ref !== expected.owner_ref || latest.device_id !== expected.device_id) {
        throw new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
      }
      const next = mutation(latest);
      return next === latest ? latest : this.saveUnlocked(next);
    });
  }

  async selectSession(state: MiniBrainState, sessionId: string): Promise<MiniBrainState> {
    if (!sessionId) throw new Error('MINI_BRAIN_SESSION_ID_INVALID');
    return this.mutateCurrent(state, latest => latest.current_session_id === sessionId && latest.selection_origin === 'user'
      ? latest : { ...latest, current_session_id: sessionId, selection_origin: 'user' });
  }

  async adoptDiscoveredSession(state: MiniBrainState, sessionId: string): Promise<MiniBrainState> {
    if (!sessionId) throw new Error('MINI_BRAIN_SESSION_ID_INVALID');
    return this.mutateCurrent(state, latest => {
      // § adoption := exact provisional revision; legacy/explicit/pending state remains selected.
      if (state.selection_origin !== 'provisional' || latest.selection_origin !== 'provisional'
        || (latest.revision ?? 0) !== (state.revision ?? 0)
        || latest.current_session_id !== state.current_session_id
        || latest.queue.length > 0
        || latest.sessions.some(session => session.session_id === latest.current_session_id
          && (session.messages.length > 0 || session.cursor !== null || session.tombstoned_at !== null))) return latest;
      return { ...latest, current_session_id: sessionId, selection_origin: 'remote' };
    });
  }

  async cacheSnapshot(state: MiniBrainState, snapshot: BrainSnapshot): Promise<MiniBrainState> {
    const memories = await Promise.all(snapshot.memories.slice(0, MAX_MEMORIES).map(async memory => {
      const sourceDigests = await Promise.all(memory.source_msg_ids.map(source => sha256Hex(source)));
      const projection = {
        topic: memory.topic_key ?? memory.type,
        paraphrase: memory.paraphrase,
        created_at: memory.created_at,
        source_digests: sourceDigests,
      };
      return {
        id_digest: await sha256Hex(memory.id),
        ...projection,
        record_digest: await sha256Hex(canonicalJson(projection)),
      } satisfies MiniBrainMemory;
    }));
    return this.mutateCurrent(state, latest => ({ ...latest, memories }));
  }

  async applySync(state: MiniBrainState, response: MiniBrainSyncResponse): Promise<MiniBrainState> {
    return this.mutateCurrent(state, current => {
      const prior = current.sessions.find(session => session.session_id === response.session_id);
      const remote = normalizeMiniBrainRemoteMessages(response.messages);
      const completedRequestIds = new Set(remote.filter(message => message.role === 'assistant').map(message => message.request_id));
      for (const message of remote.filter(item => item.terminal_failure)) {
        const queued = current.queue.find(item => item.request_id === message.request_id);
        if (queued && (queued.session_id !== response.session_id || queued.text !== message.content)) {
          throw new Error('MINI_BRAIN_FAILED_REQUEST_BINDING_MISMATCH');
        }
        completedRequestIds.add(message.request_id);
      }
      const pendingLocalIds = new Set(
        current.queue
          .filter(turn => turn.session_id === response.session_id && !completedRequestIds.has(turn.request_id))
          .flatMap(turn => [...turn.local_message_ids]),
      );
      const pendingLocalMessages = prior?.messages.filter(message => pendingLocalIds.has(message.id)
        && !remote.some(item => item.request_id === message.request_id && item.role === message.role)) ?? [];
      const historicalReflections = prior?.messages.filter(message => message.origin === 'local-reflection') ?? [];
      const messages = response.status === 'current'
        ? prior?.messages ?? []
        : remote.length > 0
          ? [...new Map([...remote, ...pendingLocalMessages, ...historicalReflections].map(message => [message.id, message])).values()]
          : prior?.messages ?? [];
      const tombstone = response.tombstones.find(item => item.session_id === response.session_id);
      const session: MiniBrainSession = {
        session_id: response.session_id,
        cursor: response.cursor,
        messages,
        updated_at: response.ts,
        events_truncated: response.events_truncated,
        tombstoned_at: tombstone?.observed_at ?? null,
      };
      return {
        ...current,
        sessions: [session, ...current.sessions.filter(item => item.session_id !== response.session_id)],
        queue: current.queue.filter(item => item.session_id !== response.session_id || !completedRequestIds.has(item.request_id)),
      };
    });
  }

  async queueTurn(
    state: MiniBrainState,
    text: string,
    existingRequest?: { readonly session_id: string; readonly request_id: string },
  ): Promise<{ state: MiniBrainState; turn: MiniBrainQueuedTurn }> {
    // Keep the caller's selected conversation; append against the latest durable queue.
    const request = existingRequest ?? { session_id: state.current_session_id, request_id: crypto.randomUUID() };
    return withVaultWriter(async () => {
      const latest = await this.load() ?? state;
      if (latest.owner_ref !== state.owner_ref || latest.device_id !== state.device_id) {
        throw new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
      }
      return this.queueTurnUnlocked(latest, text, request);
    });
  }

  private async queueTurnUnlocked(
    state: MiniBrainState,
    text: string,
    existingRequest: { readonly session_id: string; readonly request_id: string },
  ): Promise<{ state: MiniBrainState; turn: MiniBrainQueuedTurn }> {
    const sessionId = existingRequest.session_id;
    const requestId = existingRequest?.request_id ?? crypto.randomUUID();
    let existingMessage: MiniBrainMessage | undefined;
    if (existingRequest) {
      if (!sessionId || !requestId) throw new Error('MINI_BRAIN_REQUEST_IDENTITY_INVALID');
      const queued = state.queue.filter(turn => turn.request_id === requestId);
      if (queued.length > 1 || queued.some(turn => turn.session_id !== sessionId || turn.text !== text)) {
        throw new Error('MINI_BRAIN_REQUEST_IDENTITY_CONFLICT');
      }
      for (const session of state.sessions) {
        for (const message of session.messages.filter(item => item.request_id === requestId)) {
          if (session.session_id !== sessionId || (message.role === 'user' && message.content !== text)) {
            throw new Error('MINI_BRAIN_REQUEST_IDENTITY_CONFLICT');
          }
          if (message.role === 'user') existingMessage = message;
          if (message.role === 'assistant' && message.origin === 'desktop') {
            throw new Error('MINI_BRAIN_REQUEST_ALREADY_COMPLETED');
          }
        }
      }
      if (queued[0]) return { state, turn: queued[0] };
    }
    if (state.queue.length >= MAX_QUEUE) throw new Error('MINI_BRAIN_QUEUE_FULL');
    const session = state.sessions.find(item => item.session_id === sessionId) ?? {
      session_id: sessionId,
      cursor: null,
      messages: [],
      updated_at: new Date().toISOString(),
      events_truncated: false,
      tombstoned_at: null,
    } satisfies MiniBrainSession;
    const now = new Date().toISOString();
    const userId = existingMessage?.id ?? `queued-user-${requestId}`;
    const turn: MiniBrainQueuedTurn = {
      request_id: requestId,
      session_id: sessionId,
      text,
      queued_at: now,
      base_cursor: session.cursor,
      local_message_ids: [userId],
    };
    const nextSession: MiniBrainSession = {
      ...session,
      updated_at: now,
      messages: existingMessage ? session.messages : [...session.messages, {
        id: userId,
        role: 'user',
        content: text,
        request_id: requestId,
        recorded_at: now,
        event_digest: null,
        origin: 'queued-mobile',
        provenance_digests: [],
      }],
    };
    const next = await this.saveUnlocked({
      ...state,
      ...(sessionId === state.current_session_id ? { selection_origin: 'user' as const } : {}),
      sessions: [nextSession, ...state.sessions.filter(item => item.session_id !== session.session_id)],
      queue: [...state.queue, turn],
    });
    return { state: next, turn };
  }

  async deliverSync(
    state: MiniBrainState,
    input: MiniBrainRequestIntent,
    send: (request: MiniBrainSyncRequest, signal: AbortSignal) => Promise<MiniBrainSyncResponse>,
    options: { readonly signal?: AbortSignal; readonly timeoutMs?: number; readonly onPending?: (state: MiniBrainState) => void } = {},
  ): Promise<MiniBrainState> {
    const controller = new AbortController();
    const timeoutMs = options.timeoutMs ?? 120_000;
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > 120_000) {
      throw new Error('MINI_BRAIN_DELIVERY_TIMEOUT_INVALID');
    }
    const abort = () => controller.abort(options.signal?.reason);
    if (options.signal?.aborted) abort();
    else options.signal?.addEventListener('abort', abort, { once: true });
    const timer = setTimeout(() => controller.abort(new Error('MINI_BRAIN_DELIVERY_TIMEOUT')), timeoutMs);
    const signal = controller.signal;
    try {
      return await withDeliveryLock(this.deviceId, signal, async () => {
        // § delivery lock spans sign→fetch→apply; storage lock never spans fetch.
        let replayRetries = 0;
        let admissionRetries = 0;
        while (true) {
          if (signal.aborted) throw deliveryAbortReason(signal);
          if (input.operation === 'append') {
            const latest = await this.load() ?? state;
            if (latest.owner_ref !== state.owner_ref || latest.device_id !== state.device_id) {
              throw new Error('MINI_BRAIN_VAULT_BINDING_MISMATCH');
            }
            const queued = latest.queue.filter(turn => turn.request_id === input.requestId);
            if (queued.length === 0) {
              const completed = latest.sessions.find(session => session.session_id === input.sessionId)?.messages
                .some(message => message.request_id === input.requestId && (message.role === 'assistant' || message.terminal_failure));
              if (completed) return latest;
              throw new Error('MINI_BRAIN_QUEUED_REQUEST_UNAVAILABLE');
            }
            const queuedTurn = queued[0];
            if (queued.length !== 1 || !queuedTurn || queuedTurn.session_id !== input.sessionId || queuedTurn.text !== input.payload?.text) {
              throw new Error('MINI_BRAIN_REQUEST_IDENTITY_CONFLICT');
            }
            state = latest;
          }
          const request = await this.signedRequest(input);
          if (signal.aborted) throw deliveryAbortReason(signal);
          let response: MiniBrainSyncResponse;
          try {
            response = await send(request, signal);
          } catch (error) {
            if (signal.aborted) throw deliveryAbortReason(signal);
            if (replayRetries === 0 && error && typeof error === 'object'
              && 'code' in error && error.code === 'BRAIN_SYNC_REPLAY_REJECTED') { replayRetries += 1; continue; }
            if (admissionPendingFor(error, input)) {
              state = await this.mutateCurrent(state, current => {
                const turn = current.queue.find(item => item.request_id === input.requestId);
                if (!turn) {
                  const terminal = current.sessions.find(session => session.session_id === input.sessionId)?.messages
                    .some(message => message.request_id === input.requestId && (message.role === 'assistant' || message.terminal_failure));
                  if (terminal) return current;
                  throw new Error('MINI_BRAIN_QUEUED_REQUEST_UNAVAILABLE');
                }
                if (turn.session_id !== input.sessionId || turn.text !== input.payload?.text) {
                  throw new Error('MINI_BRAIN_REQUEST_IDENTITY_CONFLICT');
                }
                return turn.admission_pending === true ? current : {
                  ...current,
                  queue: current.queue.map(item => item === turn ? { ...item, admission_pending: true as const } : item),
                };
              });
              options.onPending?.(state);
              if (!state.queue.some(turn => turn.request_id === input.requestId)) return state;
              const retryDelay = ADMISSION_RETRY_DELAYS_MS[admissionRetries];
              if (retryDelay !== undefined) {
                admissionRetries += 1;
                await waitForAdmissionRetry(retryDelay, signal);
                continue;
              }
            }
            throw error;
          }
          if (signal.aborted) throw deliveryAbortReason(signal);
          if (response.session_id !== input.sessionId || response.request_id !== input.requestId) {
            throw new Error('MINI_BRAIN_RESPONSE_IDENTITY_MISMATCH');
          }
          return this.applySync(state, response);
        }
      });
    } finally {
      clearTimeout(timer);
      options.signal?.removeEventListener('abort', abort);
    }
  }

  async signedRequest(input: MiniBrainRequestIntent): Promise<MiniBrainSyncRequest> {
    return withVaultWriter(async () => {
      // § reserve in one durable IDB transaction before a signature can escape.
      const identity = await reserveSigningIdentity(this.database, this.identityValue);
      this.identityValue = { ...identity, next_sequence: identity.next_sequence + 1 };
      const unsigned: MiniBrainSyncUnsignedRequest = {
        schema_version: MINI_BRAIN_SYNC_REQUEST_SCHEMA,
        device_id: identity.device_id,
        sequence: identity.next_sequence,
        issued_at: new Date().toISOString(),
        operation: input.operation,
        session_id: input.sessionId,
        request_id: input.requestId,
        base_cursor: input.baseCursor,
        payload: input.payload,
        payload_digest: await sha256Hex(canonicalJson(input.payload)),
      };
      const signature = await crypto.subtle.sign(
        { name: 'ECDSA', hash: 'SHA-256' },
        identity.signing_private_key,
        new TextEncoder().encode(syncSigningPayload(unsigned)),
      );
      return {
        ...unsigned,
        device_token: identity.device_token!,
        signature: bytesToBase64Url(new Uint8Array(signature)),
      };
    });
  }

  async erase(): Promise<void> {
    this.database.close();
    await new Promise<void>((resolve, reject) => {
      const request = indexedDB.deleteDatabase(DB_NAME);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error ?? new Error('MINI_BRAIN_ERASE_FAILED'));
      request.onblocked = () => reject(new Error('MINI_BRAIN_ERASE_BLOCKED'));
    });
    const registrations = await navigator.serviceWorker?.getRegistrations?.() ?? [];
    await Promise.all(registrations.filter(item => item.active?.scriptURL.endsWith('/brain-sw.js')).map(item => item.unregister()));
    if ('caches' in globalThis) await caches.delete(SHELL_CACHE);
  }
}

export async function openMiniBrain(): Promise<MiniBrainOpenResult> {
  if (typeof indexedDB === 'undefined' || !globalThis.crypto?.subtle) {
    return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_PLATFORM_UNAVAILABLE' };
  }
  try {
    const vault = await MiniBrainVault.open();
    if (!vault.isBound) return { state: 'unbound', vault, reason_code: 'MINI_BRAIN_DEVICE_UNBOUND' };
    const saved = await vault.load();
    return { state: saved ? 'ready' : 'empty', vault, reason_code: null };
  } catch (error) {
    return {
      state: 'unavailable',
      vault: null,
      reason_code: error instanceof Error ? error.message : 'MINI_BRAIN_OPEN_FAILED',
    };
  }
}

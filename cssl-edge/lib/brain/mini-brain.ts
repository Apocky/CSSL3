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
const SHELL_CACHE = 'apocky-mini-brain-shell-v1';

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
}

export interface MiniBrainState {
  readonly schema_version: 'apocky.mini-brain.local-state.v1';
  readonly owner_ref: string;
  readonly device_id: string;
  readonly current_session_id: string;
  readonly sessions: readonly MiniBrainSession[];
  readonly memories: readonly MiniBrainMemory[];
  readonly queue: readonly MiniBrainQueuedTurn[];
  readonly updated_at: string;
}

export interface MiniBrainDeviceRegistration {
  readonly device_token: string;
  readonly owner_ref: string;
  readonly expires_at: string;
}

export interface MiniBrainLocalReply {
  readonly kind: 'reflection' | 'boundary';
  readonly text: string;
  readonly memory_digests: readonly string[];
}

export interface MiniBrainCortexProbe {
  readonly status: 'unavailable';
  readonly reason_code: 'NO_VERIFIED_LOCAL_MODEL_ARTIFACT';
  readonly wasm: boolean;
  readonly webgpu: boolean;
  readonly note: string;
}

export async function warmMiniBrainOfflineShell(): Promise<boolean> {
  if (!('serviceWorker' in navigator) || !('caches' in globalThis)) return false;
  await navigator.serviceWorker.ready;
  const cache = await caches.open(SHELL_CACHE);
  if (!navigator.onLine) return Boolean(await cache.match('/apocrypha'));

  const shell = await fetch('/apocrypha', {
    credentials: 'same-origin',
    cache: 'no-store',
    redirect: 'error',
    headers: { Accept: 'text/html' },
  });
  if (!shell.ok || new URL(shell.url).pathname !== '/apocrypha') return false;
  const html = await shell.text();
  if (!html.includes('"serverAccess":"owner"')) return false;
  await cache.put('/apocrypha', new Response(html, {
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

function defaultState(identity: DeviceIdentity, sessionId = crypto.randomUUID()): MiniBrainState {
  if (!identity.owner_ref) throw new Error('MINI_BRAIN_DEVICE_UNBOUND');
  return {
    schema_version: 'apocky.mini-brain.local-state.v1',
    owner_ref: identity.owner_ref,
    device_id: identity.device_id,
    current_session_id: sessionId,
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
    queue: state.queue.slice(0, MAX_QUEUE),
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
    return boundedState(parsed);
  } catch {
    throw new Error('MINI_BRAIN_VAULT_DECRYPT_FAILED');
  }
}

async function writeVault(database: IDBDatabase, identity: DeviceIdentity, value: MiniBrainState): Promise<MiniBrainState> {
  const state = boundedState(value);
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
  transaction.objectStore(VAULT_STORE).put({ key: VAULT_KEY, iv: iv.buffer, ciphertext } satisfies EncryptedVault);
  await done;
  return state;
}

function tokenTerms(value: string): string[] {
  return [...new Set(value.toLowerCase().split(/[^a-z0-9]+/u).filter(term => term.length > 2))];
}

const HARMFUL_INSTRUCTION = /\b(?:how\s+to|steps?\s+to|help\s+me|instructions?\s+(?:for|to))\b[\s\S]{0,80}\b(?:kill|poison|bomb|malware|ransomware|steal\s+(?:a\s+)?password|doxx?|hurt\s+someone)\b/iu;
const CREDENTIAL_THEFT = /\b(?:extract|steal|exfiltrate|reveal|dump)\b[\s\S]{0,60}\b(?:passwords?|private\s+keys?|api\s+keys?|tokens?|credentials?)\b/iu;

export function deterministicMiniBrainReply(query: string, memories: readonly MiniBrainMemory[]): MiniBrainLocalReply {
  const normalized = query.trim();
  if (HARMFUL_INSTRUCTION.test(normalized) || CREDENTIAL_THEFT.test(normalized)) {
    return {
      kind: 'boundary',
      text: 'Mini Brain boundary: I will not turn harm, intrusion, or private-data theft into instructions. I can help with safety, prevention, recovery, or a non-harmful reframing.',
      memory_digests: [],
    };
  }
  const terms = tokenTerms(normalized);
  const ranked = memories.map(memory => {
    const topic = memory.topic.toLowerCase();
    const paraphrase = memory.paraphrase.toLowerCase();
    const score = terms.reduce((sum, term) => sum + (topic.includes(term) ? 5 : 0) + (paraphrase.includes(term) ? 2 : 0), 0);
    return { memory, score };
  }).filter(item => item.score > 0).sort((left, right) => right.score - left.score).slice(0, 3);
  if (ranked.length === 0) {
    return {
      kind: 'reflection',
      text: 'Mini Brain · deterministic offline reflection: I found no matching compact memory. Name what changed, what evidence would change your mind, and the smallest reversible next move. This is a local prompt, not a generated Apocrypha answer.',
      memory_digests: [],
    };
  }
  return {
    kind: 'reflection',
    text: `Mini Brain · deterministic offline recall:\n${ranked.map((item, index) => `${index + 1}. ${item.memory.paraphrase}`).join('\n')}\n\nConnection prompt: which remembered constraint most changes the next reversible move? This is local retrieval, not a learned-model answer.`,
    memory_digests: ranked.map(item => item.memory.record_digest),
  };
}

export function probeMiniBrainCortex(): MiniBrainCortexProbe {
  const navigatorWithGpu = navigator as Navigator & { gpu?: unknown };
  return {
    status: 'unavailable',
    reason_code: 'NO_VERIFIED_LOCAL_MODEL_ARTIFACT',
    wasm: typeof WebAssembly === 'object',
    webgpu: Boolean(navigatorWithGpu.gpu),
    note: 'No verified, licensed, size-measured on-device model artifact is bundled. The deterministic local core remains active.',
  };
}

export function normalizeMiniBrainRemoteMessages(messages: readonly Record<string, unknown>[]): MiniBrainMessage[] {
  return messages.flatMap((message): MiniBrainMessage[] => {
    if (
      (message.role !== 'user' && message.role !== 'assistant')
      || typeof message.content !== 'string'
      || typeof message.request_id !== 'string'
      || typeof message.recorded_at !== 'string'
    ) return [];
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
    const identity = await ensureIdentity(database);
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

  async load(): Promise<MiniBrainState | null> {
    return readVault(this.database, this.identityValue);
  }

  async save(state: MiniBrainState): Promise<MiniBrainState> {
    return writeVault(this.database, this.identityValue, state);
  }

  async freshState(sessionId?: string): Promise<MiniBrainState> {
    return writeVault(this.database, this.identityValue, defaultState(this.identityValue, sessionId));
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
    return this.save({ ...state, memories });
  }

  async applySync(state: MiniBrainState, response: MiniBrainSyncResponse): Promise<MiniBrainState> {
    const prior = state.sessions.find(session => session.session_id === response.session_id);
    const remote = normalizeMiniBrainRemoteMessages(response.messages);
    const completedRequestIds = new Set(remote.map(message => message.request_id));
    const pendingLocalIds = new Set(
      state.queue
        .filter(turn => turn.session_id === response.session_id && !completedRequestIds.has(turn.request_id))
        .flatMap(turn => [...turn.local_message_ids]),
    );
    const pendingLocalMessages = prior?.messages.filter(message => pendingLocalIds.has(message.id)) ?? [];
    const messages = response.status === 'current'
      ? prior?.messages ?? []
      : remote.length > 0
        ? [...remote, ...pendingLocalMessages]
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
    return this.save({
      ...state,
      current_session_id: response.session_id,
      sessions: [session, ...state.sessions.filter(item => item.session_id !== response.session_id)],
      queue: state.queue.filter(item => !completedRequestIds.has(item.request_id)),
    });
  }

  async queueTurn(state: MiniBrainState, text: string): Promise<{ state: MiniBrainState; turn: MiniBrainQueuedTurn }> {
    if (state.queue.length >= MAX_QUEUE) throw new Error('MINI_BRAIN_QUEUE_FULL');
    const session = state.sessions.find(item => item.session_id === state.current_session_id) ?? {
      session_id: state.current_session_id,
      cursor: null,
      messages: [],
      updated_at: new Date().toISOString(),
      events_truncated: false,
      tombstoned_at: null,
    } satisfies MiniBrainSession;
    const requestId = crypto.randomUUID();
    const now = new Date().toISOString();
    const reply = deterministicMiniBrainReply(text, state.memories);
    const userId = `queued-user-${requestId}`;
    const reflectionId = `local-reflection-${requestId}`;
    const turn: MiniBrainQueuedTurn = {
      request_id: requestId,
      session_id: state.current_session_id,
      text,
      queued_at: now,
      base_cursor: session.cursor,
      local_message_ids: [userId, reflectionId],
    };
    const nextSession: MiniBrainSession = {
      ...session,
      updated_at: now,
      messages: [...session.messages, {
        id: userId,
        role: 'user',
        content: text,
        request_id: requestId,
        recorded_at: now,
        event_digest: null,
        origin: 'queued-mobile',
        provenance_digests: [],
      }, {
        id: reflectionId,
        role: 'assistant',
        content: reply.text,
        request_id: requestId,
        recorded_at: now,
        event_digest: null,
        origin: 'local-reflection',
        provenance_digests: reply.memory_digests,
      }],
    };
    const next = await this.save({
      ...state,
      sessions: [nextSession, ...state.sessions.filter(item => item.session_id !== session.session_id)],
      queue: [...state.queue, turn],
    });
    return { state: next, turn };
  }

  async signedRequest(input: {
    readonly operation: 'pull' | 'append';
    readonly sessionId: string;
    readonly requestId: string;
    readonly baseCursor: string | null;
    readonly payload: MiniBrainSyncPayload | null;
  }): Promise<MiniBrainSyncRequest> {
    if (!this.identityValue.device_token || !this.identityValue.owner_ref) throw new Error('MINI_BRAIN_DEVICE_UNBOUND');
    if (this.tokenExpired) throw new Error('MINI_BRAIN_DEVICE_TOKEN_EXPIRED');
    const deviceToken = this.identityValue.device_token;
    const unsigned: MiniBrainSyncUnsignedRequest = {
      schema_version: MINI_BRAIN_SYNC_REQUEST_SCHEMA,
      device_id: this.identityValue.device_id,
      sequence: this.identityValue.next_sequence,
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
      this.identityValue.signing_private_key,
      new TextEncoder().encode(syncSigningPayload(unsigned)),
    );
    this.identityValue = { ...this.identityValue, next_sequence: this.identityValue.next_sequence + 1 };
    await writeIdentity(this.database, this.identityValue);
    return {
      ...unsigned,
      device_token: deviceToken,
      signature: bytesToBase64Url(new Uint8Array(signature)),
    };
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

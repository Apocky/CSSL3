import type { BrainSnapshot } from './contracts';
import {
  browserAuthMutationAllowsProtectedOpen,
  whileBrowserAuthMutationActive,
} from '../auth';
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
const SYNC_LEASE_KEY = 'sync-lease';
const LOCK_BOUNDARY_KEY = 'lock-boundary';
const MAX_SESSIONS = 6;
const MAX_MESSAGES = 80;
const MAX_MEMORIES = 120;
const MAX_QUEUE = 32;
const SHELL_CACHE = 'apocky-mini-brain-shell-v2';
const SYNC_LEASE_TTL_MS = 240_000;
const SYNC_LEASE_WAIT_MS = 12_000;
const CONTROL_CHANNEL = 'apocky-mini-brain-control-v1';
export const MINI_BRAIN_SESSION_LOCK_STORAGE_KEY = 'apocky-mini-brain-session-lock-v1';
const REBIND_CANDIDATE_KEY = 'apocky-mini-brain-rebind-candidate-v1';
const REBIND_CANDIDATE_TTL_MS = 5 * 60_000;
const REBIND_PROOF_TIMEOUT_MS = 8_000;
const LOCK_SIDE_EFFECT_TIMEOUT_MS = 1_500;
const LOCK_PEER_RELAY_TIMEOUT_MS = 4_000;
const CONTROL_PROTOCOL = 'apocky.mini-brain.control.v1';
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;
let offlineShellGeneration = 0;

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
  readonly authorized_lock_generation?: string | null;
}

interface LockBoundary {
  readonly key: typeof LOCK_BOUNDARY_KEY;
  readonly generation: string;
}

interface EncryptedVault {
  readonly key: typeof VAULT_KEY;
  readonly iv: ArrayBuffer;
  readonly ciphertext: ArrayBuffer;
  readonly revision?: number;
}

interface SyncLease {
  readonly key: typeof SYNC_LEASE_KEY;
  readonly holder: string;
  readonly fence: number;
  readonly expires_at: number;
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

export interface MiniBrainDeviceBindingRequest {
  readonly device_id: string;
  readonly public_key_jwk: JsonWebKey;
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

interface MiniBrainRebindCandidate {
  readonly schema_version: 'apocky.mini-brain.rebind-candidate.v1';
  readonly owner_ref: string;
  readonly lock_generation: string;
  readonly expires_at_ms: number;
}

export type MiniBrainLockResult =
  | { readonly status: 'locked'; readonly generation: string; readonly rebind_boundary_confirmed: boolean }
  | { readonly status: 'durability_unconfirmed'; readonly code: 'MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED' };

export type MiniBrainRebindStageResult =
  | { readonly status: 'staged' }
  | { readonly status: 'locked_no_rebind' }
  | { readonly status: 'durability_unconfirmed'; readonly code: 'MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED' };

export async function warmMiniBrainOfflineShell(): Promise<boolean> {
  if (!('serviceWorker' in navigator) || !('caches' in globalThis)) return false;
  const generation = offlineShellGeneration;
  await navigator.serviceWorker.ready;
  if (generation !== offlineShellGeneration) return false;
  const cache = await caches.open(SHELL_CACHE);
  if (!navigator.onLine) return Boolean(await cache.match('/apocrypha'));

  const shell = await fetch('/apocrypha', {
    credentials: 'same-origin',
    cache: 'no-store',
    redirect: 'error',
    headers: { Accept: 'text/html' },
  });
  if (!shell.ok || new URL(shell.url).pathname !== '/apocrypha') return false;
  const html = await shell.clone().text();
  if (!html.includes('"serverAccess":"owner"')) return false;
  if (generation !== offlineShellGeneration) return false;
  await cache.put('/apocrypha', shell.clone());

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
      if (!response.ok || generation !== offlineShellGeneration) return false;
      await cache.put(url, response);
      return true;
    } catch {
      return false;
    }
  }));
  return generation === offlineShellGeneration && assetUrls.size > 0 && outcomes.every(Boolean);
}

export async function eraseMiniBrainOfflineShell(): Promise<void> {
  offlineShellGeneration += 1;
  if ('caches' in globalThis) {
    const keys = await caches.keys();
    await Promise.all(keys.filter(key => key.startsWith('apocky-mini-brain-shell-')).map(key => caches.delete(key)));
  }
}

async function revokeMiniBrainAuthorizedLock(boundaryGeneration: string): Promise<boolean> {
  if (typeof indexedDB === 'undefined') return false;
  let database: IDBDatabase | null = null;
  try {
    database = await openDatabase();
    await new Promise<void>((resolve, reject) => {
      const transaction = database!.transaction(META_STORE, 'readwrite');
      const store = transaction.objectStore(META_STORE);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error ?? new Error('MINI_BRAIN_LOCK_REVOCATION_FAILED'));
      transaction.onabort = () => reject(transaction.error ?? new Error('MINI_BRAIN_LOCK_REVOCATION_ABORTED'));
      store.put({ key: LOCK_BOUNDARY_KEY, generation: boundaryGeneration } satisfies LockBoundary);
      const request = store.get(IDENTITY_KEY);
      request.onerror = () => transaction.abort();
      request.onsuccess = () => {
        const identity = request.result as DeviceIdentity | undefined;
        if (identity) {
          store.put({
            ...identity,
            authorized_lock_generation: null,
          } satisfies DeviceIdentity);
        }
      };
    });
    return true;
  } catch {
    return false;
  } finally {
    database?.close();
  }
}

async function relayMiniBrainLockThroughServiceWorker(requestId: string): Promise<boolean> {
  if (!('serviceWorker' in navigator) || typeof MessageChannel !== 'function') return false;
  try {
    let registration = await navigator.serviceWorker.getRegistration?.('/');
    const controller = navigator.serviceWorker.controller;
    let worker = controller?.scriptURL.endsWith('/brain-sw.js')
      ? controller
      : registration?.active?.scriptURL.endsWith('/brain-sw.js')
        ? registration.active
        : null;
    if (!worker) {
      registration = await navigator.serviceWorker.register('/brain-sw.js', { scope: '/' });
      const ready = await navigator.serviceWorker.ready;
      worker = ready.active?.scriptURL.endsWith('/brain-sw.js') ? ready.active : registration.active;
    }
    if (!worker) return false;
    return await new Promise<boolean>((resolve) => {
      const channel = new MessageChannel();
      channel.port1.onmessage = (event: MessageEvent) => {
        const message = event.data as Record<string, unknown> | null;
        channel.port1.close();
        resolve(Boolean(
          message
          && message.schema_version === CONTROL_PROTOCOL
          && message.type === 'LOCK_MINI_BRAIN_RESULT'
          && message.request_id === requestId
          && message.status === 'acknowledged'
        ));
      };
      try {
        worker.postMessage({
          schema_version: CONTROL_PROTOCOL,
          type: 'LOCK_MINI_BRAIN_REQUEST',
          request_id: requestId,
        }, [channel.port2]);
      } catch {
        channel.port1.close();
        resolve(false);
      }
    });
  } catch {
    return false;
  }
}

function boundedLockSideEffect<T>(operation: Promise<T>, timeoutMs = LOCK_SIDE_EFFECT_TIMEOUT_MS): Promise<T | null> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: T | null): void => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      resolve(value);
    };
    const timer = window.setTimeout(() => finish(null), timeoutMs);
    void operation.then(value => finish(value), () => finish(null));
  });
}

function freshSessionLockGeneration(): string {
  if (typeof crypto?.randomUUID === 'function') return crypto.randomUUID();
  const bytes = new Uint8Array(16);
  if (typeof crypto?.getRandomValues === 'function') crypto.getRandomValues(bytes);
  else {
    const seed = `${Date.now()}\u0000${performance?.now?.() ?? 0}\u0000${Math.random()}`;
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = seed.charCodeAt(index % seed.length) ^ Math.floor(Math.random() * 256);
    }
  }
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = [...bytes].map(value => value.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export async function lockMiniBrainForSignedOutSession(): Promise<MiniBrainLockResult> {
  const generation = freshSessionLockGeneration();
  let persisted = false;
  try {
    localStorage.setItem(MINI_BRAIN_SESSION_LOCK_STORAGE_KEY, generation);
    persisted = localStorage.getItem(MINI_BRAIN_SESSION_LOCK_STORAGE_KEY) === generation;
  } catch { /* live invalidation still runs; future opens fail closed when storage cannot be read */ }
  clearMiniBrainRebindCandidate();
  try {
    if (typeof BroadcastChannel === 'function') {
      const channel = new BroadcastChannel(CONTROL_CHANNEL);
      try { channel.postMessage('LOCK_MINI_BRAIN'); } finally { channel.close(); }
    }
  } catch { /* durable guards and the same-tab event must still run */ }
  window.dispatchEvent(new Event('apocky-mini-brain-locked'));
  // Persist the new generation before enumerating peers. A tab opened after
  // this commit cannot decrypt the prior epoch even if it misses every signal.
  const authorizationRevoked = await boundedLockSideEffect(revokeMiniBrainAuthorizedLock(generation));
  const peerRelayConfirmed = await boundedLockSideEffect(
    relayMiniBrainLockThroughServiceWorker(generation),
    LOCK_PEER_RELAY_TIMEOUT_MS,
  );
  // Relay may have to install the worker; erase its cache only after that
  // lifecycle finishes so a first install cannot recreate the retired shell.
  await boundedLockSideEffect(eraseMiniBrainOfflineShell());
  // A new local lock is the normal path. If localStorage rejects the write,
  // the independent IndexedDB revocation still prevents a restored older UUID
  // from matching the vault's authorized epoch on a later reload.
  if ((!persisted && authorizationRevoked !== true) || peerRelayConfirmed !== true) {
    window.dispatchEvent(new CustomEvent('apocky-mini-brain-lock-unconfirmed', {
      detail: { code: 'MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED' },
    }));
    return { status: 'durability_unconfirmed', code: 'MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED' };
  }
  return {
    status: 'locked',
    generation,
    rebind_boundary_confirmed: authorizationRevoked === true,
  };
}

function miniBrainLockGeneration(): string | null | undefined {
  try {
    const value = localStorage.getItem(MINI_BRAIN_SESSION_LOCK_STORAGE_KEY);
    if (value === null) return null;
    return UUID_V4.test(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

function readRebindCandidate(): MiniBrainRebindCandidate | null {
  try {
    const raw = sessionStorage.getItem(REBIND_CANDIDATE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<MiniBrainRebindCandidate>;
    if (
      parsed.schema_version !== 'apocky.mini-brain.rebind-candidate.v1'
      || typeof parsed.owner_ref !== 'string'
      || !/^[0-9a-f]{64}$/u.test(parsed.owner_ref)
      || typeof parsed.lock_generation !== 'string'
      || !UUID_V4.test(parsed.lock_generation)
      || typeof parsed.expires_at_ms !== 'number'
      || !Number.isFinite(parsed.expires_at_ms)
      || parsed.expires_at_ms <= Date.now()
    ) {
      sessionStorage.removeItem(REBIND_CANDIDATE_KEY);
      return null;
    }
    return parsed as MiniBrainRebindCandidate;
  } catch {
    return null;
  }
}

export function clearMiniBrainRebindCandidate(): boolean {
  try {
    sessionStorage.removeItem(REBIND_CANDIDATE_KEY);
    return sessionStorage.getItem(REBIND_CANDIDATE_KEY) === null;
  } catch {
    return false;
  }
}

function rebindCandidateFor(ownerRef: string, lockGeneration: string): MiniBrainRebindCandidate | null {
  const candidate = readRebindCandidate();
  return candidate?.owner_ref === ownerRef && candidate.lock_generation === lockGeneration
    ? candidate
    : null;
}

function consumeRebindCandidate(candidate: MiniBrainRebindCandidate): void {
  const current = rebindCandidateFor(candidate.owner_ref, candidate.lock_generation);
  if (!current) throw new Error('MINI_BRAIN_REBIND_PROOF_EXPIRED');
  try { sessionStorage.removeItem(REBIND_CANDIDATE_KEY); } catch { /* the persistent IDB epoch remains authoritative */ }
}

function clearRebindCandidateForLock(lockGeneration: string): void {
  const candidate = readRebindCandidate();
  if (candidate?.lock_generation !== lockGeneration) return;
  try { sessionStorage.removeItem(REBIND_CANDIDATE_KEY); } catch { /* an unreadable candidate cannot authorize a vault open */ }
}

export async function stageMiniBrainOwnerRebindAfterReauthentication(
  expectedSubjectKey: string,
  authAttempt: string,
): Promise<MiniBrainRebindStageResult> {
  // Every fresh authentication attempt is a Brain authority boundary, even for
  // ordinary members who are not allowed to bind the owner-only vault. Rotate
  // and broadcast first so invalid input, hashing failure, a stale browser-auth
  // state, or a denied server proof can never preserve an older open epoch.
  const lock = await lockMiniBrainForSignedOutSession();
  if (lock.status === 'durability_unconfirmed') return lock;
  if (!lock.rebind_boundary_confirmed) return { status: 'locked_no_rebind' };
  const lockGeneration = lock.generation;
  let expectedRebindOwnerRef: string;
  if (authAttempt.length < 80 || authAttempt.length > 8_192) return { status: 'locked_no_rebind' };
  try {
    expectedRebindOwnerRef = await expectedOwnerRef(expectedSubjectKey);
  } catch {
    return { status: 'locked_no_rebind' };
  }
  const staged = await whileBrowserAuthMutationActive(async () => {
    try {
      if (miniBrainLockGeneration() !== lockGeneration) return false;
      const controller = new AbortController();
      const timeout = window.setTimeout(() => controller.abort(new DOMException('Timed out', 'TimeoutError')), REBIND_PROOF_TIMEOUT_MS);
      let response: Response;
      try {
        response = await fetch('/api/brain/mobile/unlock', {
          method: 'POST',
          credentials: 'same-origin',
          cache: 'no-store',
          redirect: 'error',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ lock_generation: lockGeneration, auth_attempt: authAttempt }),
          signal: controller.signal,
        });
      } finally {
        window.clearTimeout(timeout);
      }
      if (!response.ok) return false;
      const payload = await response.json() as Record<string, unknown>;
      if (
        payload.schema_version !== 'apocky.mini-brain.owner-rebind.v1'
        || payload.status !== 'rebind_authorized'
        || typeof payload.owner_ref !== 'string'
        || !/^[0-9a-f]{64}$/u.test(payload.owner_ref)
        || payload.owner_ref !== expectedRebindOwnerRef
        || payload.lock_generation !== lockGeneration
        || miniBrainLockGeneration() !== lockGeneration
      ) return false;
      const candidate: MiniBrainRebindCandidate = {
        schema_version: 'apocky.mini-brain.rebind-candidate.v1',
        owner_ref: payload.owner_ref,
        lock_generation: lockGeneration,
        expires_at_ms: Date.now() + REBIND_CANDIDATE_TTL_MS,
      };
      sessionStorage.setItem(REBIND_CANDIDATE_KEY, JSON.stringify(candidate));
      return miniBrainLockGeneration() === lockGeneration;
    } catch {
      return false;
    }
  });
  if (!staged) {
    clearRebindCandidateForLock(lockGeneration);
  }
  return staged ? { status: 'staged' } : { status: 'locked_no_rebind' };
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

async function readLockBoundary(database: IDBDatabase): Promise<string | null> {
  const transaction = database.transaction(META_STORE, 'readonly');
  const done = transactionDone(transaction);
  const value = await requestResult(transaction.objectStore(META_STORE).get(LOCK_BOUNDARY_KEY)) as LockBoundary | undefined;
  await done;
  return value?.key === LOCK_BOUNDARY_KEY && UUID_V4.test(value.generation) ? value.generation : null;
}

async function writeIdentity(database: IDBDatabase, identity: DeviceIdentity): Promise<void> {
  const transaction = database.transaction(META_STORE, 'readwrite');
  const done = transactionDone(transaction);
  transaction.objectStore(META_STORE).put(identity);
  await done;
}

function reserveSigningIdentity(
  database: IDBDatabase,
  deviceId: string,
  holder: string,
  fence: number,
): Promise<{ identity: DeviceIdentity; sequence: number }> {
  return new Promise((resolve, reject) => {
    const transaction = database.transaction(META_STORE, 'readwrite');
    const store = transaction.objectStore(META_STORE);
    let reserved: { identity: DeviceIdentity; sequence: number } | null = null;
    let failure: Error | null = null;
    transaction.oncomplete = () => reserved ? resolve(reserved) : reject(failure ?? new Error('MINI_BRAIN_SEQUENCE_RESERVATION_FAILED'));
    transaction.onerror = () => reject(transaction.error ?? failure ?? new Error('MINI_BRAIN_SEQUENCE_RESERVATION_FAILED'));
    transaction.onabort = () => reject(transaction.error ?? failure ?? new Error('MINI_BRAIN_SEQUENCE_RESERVATION_ABORTED'));
    const leaseRequest = store.get(SYNC_LEASE_KEY);
    leaseRequest.onerror = () => {
      failure = leaseRequest.error ?? new Error('MINI_BRAIN_SYNC_LEASE_FAILED');
      transaction.abort();
    };
    leaseRequest.onsuccess = () => {
      const lease = leaseRequest.result as SyncLease | undefined;
      if (!lease || lease.holder !== holder || lease.fence !== fence || lease.expires_at <= Date.now()) {
        failure = new Error('MINI_BRAIN_SYNC_LEASE_LOST');
        transaction.abort();
        return;
      }
      const request = store.get(IDENTITY_KEY);
      request.onerror = () => {
        failure = request.error ?? new Error('MINI_BRAIN_IDENTITY_READ_FAILED');
        transaction.abort();
      };
      request.onsuccess = () => {
        const identity = request.result as DeviceIdentity | undefined;
        if (!identity || identity.device_id !== deviceId || !Number.isSafeInteger(identity.next_sequence)) {
          failure = new Error('MINI_BRAIN_DEVICE_IDENTITY_CHANGED');
          transaction.abort();
          return;
        }
        const sequence = identity.next_sequence;
        const next = { ...identity, next_sequence: sequence + 1 };
        reserved = { identity: next, sequence };
        store.put(next);
      };
    };
  });
}

function updateSyncLease(
  database: IDBDatabase,
  holder: string,
  operation: 'acquire' | 'renew' | 'release',
  expectedFence: number | null = null,
): Promise<number | null> {
  return new Promise((resolve, reject) => {
    const transaction = database.transaction(META_STORE, 'readwrite');
    const store = transaction.objectStore(META_STORE);
    let acquiredFence: number | null = null;
    transaction.oncomplete = () => resolve(acquiredFence);
    transaction.onerror = () => reject(transaction.error ?? new Error('MINI_BRAIN_SYNC_LEASE_FAILED'));
    transaction.onabort = () => reject(transaction.error ?? new Error('MINI_BRAIN_SYNC_LEASE_ABORTED'));
    const request = store.get(SYNC_LEASE_KEY);
    request.onerror = () => transaction.abort();
    request.onsuccess = () => {
      const current = request.result as SyncLease | undefined;
      if (operation === 'release') {
        if (current?.holder === holder && current.fence === expectedFence) {
          acquiredFence = current.fence;
          store.delete(SYNC_LEASE_KEY);
        }
        return;
      }
      const now = Date.now();
      if (operation === 'renew') {
        if (
          current?.holder === holder
          && current.fence === expectedFence
          && current.expires_at > now
        ) {
          acquiredFence = current.fence;
          store.put({ ...current, expires_at: now + SYNC_LEASE_TTL_MS } satisfies SyncLease);
        }
        return;
      }
      if (!current || current.expires_at <= now) {
        acquiredFence = (current?.fence ?? 0) + 1;
        store.put({ key: SYNC_LEASE_KEY, holder, fence: acquiredFence, expires_at: now + SYNC_LEASE_TTL_MS } satisfies SyncLease);
      } else if (current.holder === holder) {
        acquiredFence = current.fence;
        store.put({ ...current, expires_at: now + SYNC_LEASE_TTL_MS } satisfies SyncLease);
      }
    };
  });
}

function wait(milliseconds: number): Promise<void> {
  return new Promise(resolve => window.setTimeout(resolve, milliseconds));
}

function bindIdentity(
  database: IDBDatabase,
  deviceId: string,
  registration: MiniBrainDeviceRegistration,
  lease: { readonly holder: string; readonly fence: number },
  authorizedLockGeneration?: string | null,
): Promise<{ readonly identity: DeviceIdentity; readonly ownerChanged: boolean }> {
  return new Promise((resolve, reject) => {
    const transaction = database.transaction([META_STORE, VAULT_STORE], 'readwrite');
    const metadata = transaction.objectStore(META_STORE);
    const vaults = transaction.objectStore(VAULT_STORE);
    let result: { readonly identity: DeviceIdentity; readonly ownerChanged: boolean } | null = null;
    let failure: Error | null = null;
    transaction.oncomplete = () => result ? resolve(result) : reject(failure ?? new Error('MINI_BRAIN_BIND_FAILED'));
    transaction.onerror = () => reject(transaction.error ?? failure ?? new Error('MINI_BRAIN_BIND_FAILED'));
    transaction.onabort = () => reject(transaction.error ?? failure ?? new Error('MINI_BRAIN_BIND_ABORTED'));
    const leaseRequest = metadata.get(SYNC_LEASE_KEY);
    leaseRequest.onerror = () => {
      failure = leaseRequest.error ?? new Error('MINI_BRAIN_SYNC_LEASE_FAILED');
      transaction.abort();
    };
    leaseRequest.onsuccess = () => {
      const currentLease = leaseRequest.result as SyncLease | undefined;
      if (
        !currentLease
        || currentLease.holder !== lease.holder
        || currentLease.fence !== lease.fence
        || currentLease.expires_at <= Date.now()
      ) {
        failure = new Error('MINI_BRAIN_SYNC_LEASE_LOST');
        transaction.abort();
        return;
      }
      const readAndBindIdentity = (): void => {
        const identityRequest = metadata.get(IDENTITY_KEY);
        identityRequest.onerror = () => {
          failure = identityRequest.error ?? new Error('MINI_BRAIN_IDENTITY_READ_FAILED');
          transaction.abort();
        };
        identityRequest.onsuccess = () => {
          const persisted = identityRequest.result as DeviceIdentity | undefined;
          if (!persisted || persisted.device_id !== deviceId) {
            failure = new Error('MINI_BRAIN_DEVICE_IDENTITY_CHANGED');
            transaction.abort();
            return;
          }
          const ownerChanged = persisted.owner_ref !== null && persisted.owner_ref !== registration.owner_ref;
          const identity: DeviceIdentity = {
            ...persisted,
            owner_ref: registration.owner_ref,
            device_token: registration.device_token,
            token_expires_at: registration.expires_at,
            next_sequence: ownerChanged ? 1 : persisted.next_sequence,
            authorized_lock_generation: authorizedLockGeneration === undefined
              ? persisted.authorized_lock_generation ?? null
              : authorizedLockGeneration,
          };
          metadata.put(identity);
          if (ownerChanged) vaults.delete(VAULT_KEY);
          result = { identity, ownerChanged };
        };
      };
      if (authorizedLockGeneration === undefined) {
        readAndBindIdentity();
        return;
      }
      const boundaryRequest = metadata.get(LOCK_BOUNDARY_KEY);
      boundaryRequest.onerror = () => {
        failure = boundaryRequest.error ?? new Error('MINI_BRAIN_REBIND_BOUNDARY_READ_FAILED');
        transaction.abort();
      };
      boundaryRequest.onsuccess = () => {
        const boundary = boundaryRequest.result as LockBoundary | undefined;
        if (boundary?.generation !== authorizedLockGeneration) {
          failure = new Error('MINI_BRAIN_REBIND_BOUNDARY_CHANGED');
          transaction.abort();
          return;
        }
        readAndBindIdentity();
      };
    };
  });
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
    authorized_lock_generation: null,
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

interface VaultSnapshot {
  readonly state: MiniBrainState | null;
  readonly revision: number;
}

function encryptedVaultRevision(value: EncryptedVault | null | undefined): number {
  return Number.isSafeInteger(value?.revision) && (value?.revision ?? -1) >= 0 ? value!.revision! : 0;
}

async function readVault(database: IDBDatabase, identity: DeviceIdentity): Promise<VaultSnapshot> {
  if (!identity.owner_ref) return { state: null, revision: 0 };
  const transaction = database.transaction(VAULT_STORE, 'readonly');
  const done = transactionDone(transaction);
  const stored = await requestResult(transaction.objectStore(VAULT_STORE).get(VAULT_KEY));
  await done;
  if (!stored || typeof stored !== 'object') return { state: null, revision: 0 };
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
    return { state: boundedState(parsed), revision: encryptedVaultRevision(vault) };
  } catch {
    throw new Error('MINI_BRAIN_VAULT_DECRYPT_FAILED');
  }
}

async function writeVault(
  database: IDBDatabase,
  identity: DeviceIdentity,
  value: MiniBrainState,
  lease: { readonly holder: string; readonly fence: number },
  expectedRevision: number,
): Promise<{ readonly state: MiniBrainState; readonly revision: number }> {
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
  const nextRevision = expectedRevision + 1;
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction([META_STORE, VAULT_STORE], 'readwrite');
    const metadata = transaction.objectStore(META_STORE);
    const vaults = transaction.objectStore(VAULT_STORE);
    let failure: Error | null = null;
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error ?? failure ?? new Error('MINI_BRAIN_VAULT_WRITE_FAILED'));
    transaction.onabort = () => reject(transaction.error ?? failure ?? new Error('MINI_BRAIN_VAULT_WRITE_ABORTED'));
    const leaseRequest = metadata.get(SYNC_LEASE_KEY);
    leaseRequest.onerror = () => {
      failure = leaseRequest.error ?? new Error('MINI_BRAIN_SYNC_LEASE_FAILED');
      transaction.abort();
    };
    leaseRequest.onsuccess = () => {
      const currentLease = leaseRequest.result as SyncLease | undefined;
      if (
        !currentLease
        || currentLease.holder !== lease.holder
        || currentLease.fence !== lease.fence
        || currentLease.expires_at <= Date.now()
      ) {
        failure = new Error('MINI_BRAIN_SYNC_LEASE_LOST');
        transaction.abort();
        return;
      }
      const vaultRequest = vaults.get(VAULT_KEY);
      vaultRequest.onerror = () => {
        failure = vaultRequest.error ?? new Error('MINI_BRAIN_VAULT_READ_FAILED');
        transaction.abort();
      };
      vaultRequest.onsuccess = () => {
        const current = vaultRequest.result && typeof vaultRequest.result === 'object'
          ? vaultRequest.result as EncryptedVault
          : null;
        if (encryptedVaultRevision(current) !== expectedRevision) {
          failure = new Error('MINI_BRAIN_VAULT_REVISION_CONFLICT');
          transaction.abort();
          return;
        }
        vaults.put({ key: VAULT_KEY, iv: iv.buffer, ciphertext, revision: nextRevision } satisfies EncryptedVault);
      };
    };
  });
  return { state, revision: nextRevision };
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

function normalizedRemoteMessages(messages: readonly Record<string, unknown>[]): MiniBrainMessage[] {
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
  private syncTail: Promise<void> = Promise.resolve();
  private readonly syncHolder = crypto.randomUUID();
  private readonly controlChannel: BroadcastChannel | null;
  private activeLeaseFence: number | null = null;
  private leaseLost = false;
  private vaultRevisionValue: number | null = null;

  private constructor(
    private readonly database: IDBDatabase,
    private identityValue: DeviceIdentity,
    private readonly boundaryGenerationValue: string | null,
  ) {
    this.database.onversionchange = () => this.database.close();
    this.controlChannel = typeof BroadcastChannel === 'function' ? new BroadcastChannel(CONTROL_CHANNEL) : null;
    if (this.controlChannel) {
      this.controlChannel.onmessage = event => {
        if (
          event.data === 'CLOSE_MINI_BRAIN_DATABASE'
          || event.data === 'ERASE_MINI_BRAIN'
          || event.data === 'LOCK_MINI_BRAIN'
        ) {
          this.close();
          if (event.data === 'ERASE_MINI_BRAIN') window.dispatchEvent(new Event('apocky-mini-brain-erased'));
          if (event.data === 'LOCK_MINI_BRAIN') window.dispatchEvent(new Event('apocky-mini-brain-locked'));
        }
      };
    }
  }

  static async open(): Promise<MiniBrainVault> {
    const database = await openDatabase();
    const identity = await ensureIdentity(database);
    const boundaryGeneration = await readLockBoundary(database);
    return new MiniBrainVault(database, identity, boundaryGeneration);
  }

  get deviceId(): string { return this.identityValue.device_id; }
  get publicKeyJwk(): JsonWebKey { return this.identityValue.signing_public_jwk; }
  get ownerRef(): string | null { return this.identityValue.owner_ref; }
  get authorizedLockGeneration(): string | null { return this.identityValue.authorized_lock_generation ?? null; }
  get boundaryGeneration(): string | null { return this.boundaryGenerationValue; }
  sessionLockStillAuthorized(): boolean {
    const current = miniBrainLockGeneration();
    return current !== undefined && current !== null && current === this.authorizedLockGeneration;
  }
  async persistedSessionLockStillAuthorized(): Promise<boolean> {
    const current = miniBrainLockGeneration();
    if (current === undefined || current === null || current !== this.authorizedLockGeneration) return false;
    const persisted = await boundedLockSideEffect(readIdentity(this.database));
    if (!persisted || persisted.device_id !== this.identityValue.device_id) return false;
    this.identityValue = persisted;
    return persisted.authorized_lock_generation === current;
  }
  async isOwnedBySubject(subjectKey: string): Promise<boolean> {
    return this.identityValue.owner_ref === await expectedOwnerRef(subjectKey);
  }
  get isBound(): boolean { return Boolean(this.identityValue.owner_ref && this.identityValue.device_token); }
  get tokenExpired(): boolean {
    const expires = this.identityValue.token_expires_at ? Date.parse(this.identityValue.token_expires_at) : 0;
    return !Number.isFinite(expires) || expires <= Date.now() + 60_000;
  }

  close(): void {
    this.database.close();
    this.controlChannel?.close();
  }

  private activeLease(): { readonly holder: string; readonly fence: number } {
    if (this.activeLeaseFence === null || this.leaseLost) throw new Error('MINI_BRAIN_SYNC_LEASE_LOST');
    return { holder: this.syncHolder, fence: this.activeLeaseFence };
  }

  async withSyncLock<T>(operation: () => Promise<T>): Promise<T> {
    const prior = this.syncTail;
    let releaseLocal: () => void = () => undefined;
    this.syncTail = new Promise<void>(resolve => { releaseLocal = resolve; });
    await prior;
    let leaseFence: number | null = null;
    let renewalRunning = false;
    let renewalTimer: number | null = null;
    try {
      const deadline = Date.now() + SYNC_LEASE_WAIT_MS;
      while ((leaseFence = await updateSyncLease(this.database, this.syncHolder, 'acquire')) === null) {
        if (Date.now() >= deadline) throw new Error('MINI_BRAIN_SYNC_BUSY');
        await wait(60 + Math.floor(Math.random() * 90));
      }
      this.activeLeaseFence = leaseFence;
      this.leaseLost = false;
      renewalTimer = window.setInterval(() => {
        if (renewalRunning || leaseFence === null) return;
        renewalRunning = true;
        void updateSyncLease(this.database, this.syncHolder, 'renew', leaseFence)
          .then(renewed => { if (renewed !== leaseFence) this.leaseLost = true; })
          .catch(() => { this.leaseLost = true; })
          .finally(() => { renewalRunning = false; });
      }, Math.floor(SYNC_LEASE_TTL_MS / 4));
      return await operation();
    } finally {
      if (renewalTimer !== null) window.clearInterval(renewalTimer);
      this.activeLeaseFence = null;
      this.leaseLost = false;
      if (leaseFence !== null) await updateSyncLease(this.database, this.syncHolder, 'release', leaseFence).catch(() => undefined);
      releaseLocal();
    }
  }

  async bind(
    registration: MiniBrainDeviceRegistration,
    authorizedLockGeneration?: string | null,
  ): Promise<MiniBrainState> {
    const binding = await bindIdentity(
      this.database,
      this.identityValue.device_id,
      registration,
      this.activeLease(),
      authorizedLockGeneration,
    );
    this.identityValue = binding.identity;
    const existing = await readVault(this.database, this.identityValue);
    this.vaultRevisionValue = existing.revision;
    return existing.state ?? this.save(defaultState(this.identityValue));
  }

  async load(): Promise<MiniBrainState | null> {
    const snapshot = await readVault(this.database, this.identityValue);
    this.vaultRevisionValue = snapshot.revision;
    return snapshot.state;
  }

  async save(state: MiniBrainState): Promise<MiniBrainState> {
    if (this.vaultRevisionValue === null) throw new Error('MINI_BRAIN_VAULT_REVISION_UNKNOWN');
    const written = await writeVault(
      this.database,
      this.identityValue,
      state,
      this.activeLease(),
      this.vaultRevisionValue,
    );
    this.vaultRevisionValue = written.revision;
    return written.state;
  }

  async freshState(sessionId?: string): Promise<MiniBrainState> {
    return this.save(defaultState(this.identityValue, sessionId));
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
    const remote = normalizedRemoteMessages(response.messages);
    const completedRequestIds = new Set([
      ...remote.map(message => message.request_id),
      ...response.acknowledged_request_ids,
    ]);
    const pendingLocalIds = new Set(
      state.queue
        .filter(turn => turn.session_id === response.session_id && !completedRequestIds.has(turn.request_id))
        .flatMap(turn => [...turn.local_message_ids]),
    );
    const pendingLocalMessages = prior?.messages.filter(message => pendingLocalIds.has(message.id)) ?? [];
    const sessionQueue = state.queue.flatMap((turn, index) => (
      turn.session_id === response.session_id ? [{ turn, index }] : []
    ));
    let acknowledgedPrefixIndex = -1;
    for (const entry of sessionQueue) {
      if (!completedRequestIds.has(entry.turn.request_id)) break;
      acknowledgedPrefixIndex = entry.index;
    }
    const remainingQueue = state.queue
      .filter(item => item.session_id !== response.session_id || !completedRequestIds.has(item.request_id))
      .map((item) => {
        const originalIndex = state.queue.indexOf(item);
        return response.cursor !== null
          && item.session_id === response.session_id
          && originalIndex > acknowledgedPrefixIndex
          && acknowledgedPrefixIndex >= 0
          ? { ...item, base_cursor: response.cursor }
          : item;
      });
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
      queue: remainingQueue,
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

  async reissueTurn(
    state: MiniBrainState,
    failedRequestId: string,
    baseCursor: string | null,
  ): Promise<{ readonly state: MiniBrainState; readonly requestId: string }> {
    const failed = state.queue.find(turn => turn.request_id === failedRequestId);
    if (!failed) throw new Error('MINI_BRAIN_FAILED_TURN_NOT_FOUND');
    const requestId = crypto.randomUUID();
    const localIds = new Set(failed.local_message_ids);
    const replacements = {
      user: `queued-user-${requestId}`,
      assistant: `local-reflection-${requestId}`,
    } as const;
    const sessions = state.sessions.map(session => session.session_id !== failed.session_id ? session : {
      ...session,
      cursor: baseCursor,
      updated_at: new Date().toISOString(),
      messages: session.messages.map(message => localIds.has(message.id) ? {
        ...message,
        id: replacements[message.role],
        request_id: requestId,
      } : message),
    });
    const queue = state.queue.map(turn => turn.request_id !== failedRequestId ? turn : {
      ...turn,
      request_id: requestId,
      queued_at: new Date().toISOString(),
      base_cursor: baseCursor,
      local_message_ids: [replacements.user, replacements.assistant],
    });
    return { state: await this.save({ ...state, sessions, queue }), requestId };
  }

  async reviseQueuedTurn(
    state: MiniBrainState,
    failedRequestId: string,
    text: string,
  ): Promise<{ readonly state: MiniBrainState; readonly requestId: string }> {
    const failed = state.queue.find(turn => turn.request_id === failedRequestId);
    if (!failed) throw new Error('MINI_BRAIN_FAILED_TURN_NOT_FOUND');
    const localIds = new Set(failed.local_message_ids);
    const requestId = crypto.randomUUID();
    const now = new Date().toISOString();
    const reply = deterministicMiniBrainReply(text, state.memories);
    const userId = `queued-user-${requestId}`;
    const reflectionId = `local-reflection-${requestId}`;
    const sessions = state.sessions.map(session => session.session_id !== failed.session_id ? session : {
      ...session,
      updated_at: now,
      messages: session.messages.map(message => !localIds.has(message.id) ? message : message.role === 'user' ? {
        ...message,
        id: userId,
        content: text,
        request_id: requestId,
        recorded_at: now,
      } : {
        ...message,
        id: reflectionId,
        content: reply.text,
        request_id: requestId,
        recorded_at: now,
        provenance_digests: reply.memory_digests,
      }),
    });
    const queue = state.queue.map(turn => turn.request_id !== failedRequestId ? turn : {
      ...turn,
      request_id: requestId,
      text,
      queued_at: now,
      local_message_ids: [userId, reflectionId],
    });
    const next = await this.save({
      ...state,
      current_session_id: failed.session_id,
      sessions,
      queue,
      updated_at: now,
    });
    return { state: next, requestId };
  }

  async signedRequest(input: {
    readonly operation: 'pull' | 'append';
    readonly sessionId: string;
    readonly requestId: string;
    readonly baseCursor: string | null;
    readonly payload: MiniBrainSyncPayload | null;
  }): Promise<MiniBrainSyncRequest> {
    const lease = this.activeLease();
    const reservation = await reserveSigningIdentity(
      this.database,
      this.identityValue.device_id,
      lease.holder,
      lease.fence,
    );
    this.identityValue = reservation.identity;
    if (!this.identityValue.device_token || !this.identityValue.owner_ref) throw new Error('MINI_BRAIN_DEVICE_UNBOUND');
    if (this.tokenExpired) throw new Error('MINI_BRAIN_DEVICE_TOKEN_EXPIRED');
    const deviceToken = this.identityValue.device_token;
    const unsigned: MiniBrainSyncUnsignedRequest = {
      schema_version: MINI_BRAIN_SYNC_REQUEST_SCHEMA,
      device_id: this.identityValue.device_id,
      sequence: reservation.sequence,
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
    return {
      ...unsigned,
      device_token: deviceToken,
      signature: bytesToBase64Url(new Uint8Array(signature)),
    };
  }

  async erase(): Promise<'erased' | 'database_pending'> {
    this.controlChannel?.postMessage('ERASE_MINI_BRAIN');
    window.dispatchEvent(new Event('apocky-mini-brain-erased'));
    await wait(50);
    this.close();
    const databaseState = await new Promise<'erased' | 'database_pending'>((resolve, reject) => {
      const request = indexedDB.deleteDatabase(DB_NAME);
      const timeout = window.setTimeout(() => resolve('database_pending'), 5_000);
      request.onsuccess = () => { window.clearTimeout(timeout); resolve('erased'); };
      request.onerror = () => { window.clearTimeout(timeout); reject(request.error ?? new Error('MINI_BRAIN_ERASE_FAILED')); };
    });
    await eraseMiniBrainOfflineShell();
    return databaseState;
  }
}

async function expectedOwnerRef(subjectKey: string): Promise<string> {
  if (!subjectKey || subjectKey !== subjectKey.trim() || subjectKey.length > 512) {
    throw new Error('MINI_BRAIN_OWNER_SUBJECT_INVALID');
  }
  return sha256Hex(`apocky.mini-brain.owner.v1\u0000${subjectKey}`);
}

export async function openMiniBrain(): Promise<MiniBrainOpenResult> {
  if (!browserAuthMutationAllowsProtectedOpen()) {
    return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_SESSION_LOCKED' };
  }
  if (typeof indexedDB === 'undefined' || !globalThis.crypto?.subtle) {
    return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_PLATFORM_UNAVAILABLE' };
  }
  try {
    const lockGeneration = miniBrainLockGeneration();
    if (lockGeneration === undefined) {
      return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_SESSION_LOCK_STORAGE_UNAVAILABLE' };
    }
    const vault = await MiniBrainVault.open();
    if (lockGeneration === null || vault.authorizedLockGeneration !== lockGeneration) {
      vault.close();
      return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_SESSION_LOCKED' };
    }
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

export async function openMiniBrainForCurrentOwner(
  subjectKey: string,
  register: (request: MiniBrainDeviceBindingRequest) => Promise<MiniBrainDeviceRegistration>,
): Promise<MiniBrainOpenResult> {
  if (!browserAuthMutationAllowsProtectedOpen()) {
    return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_SESSION_LOCKED' };
  }
  if (typeof indexedDB === 'undefined' || !globalThis.crypto?.subtle) {
    return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_PLATFORM_UNAVAILABLE' };
  }
  let vault: MiniBrainVault | null = null;
  try {
    const ownerRef = await expectedOwnerRef(subjectKey);
    const lockGeneration = miniBrainLockGeneration();
    if (lockGeneration === undefined) {
      return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_SESSION_LOCK_STORAGE_UNAVAILABLE' };
    }
    // Online presentation is preceded by a fresh owner-bound registration. The
    // atomic bind deletes a prior owner's ciphertext before any caller can load it.
    vault = await MiniBrainVault.open();
    const lockAlreadyAuthorized = lockGeneration !== null && vault.authorizedLockGeneration === lockGeneration;
    const candidate = !lockAlreadyAuthorized && lockGeneration && vault.boundaryGeneration === lockGeneration
      ? rebindCandidateFor(ownerRef, lockGeneration)
      : null;
    if (!lockAlreadyAuthorized && (lockGeneration === null || !candidate)) {
      vault.close();
      return { state: 'unavailable', vault: null, reason_code: 'MINI_BRAIN_SESSION_LOCKED' };
    }
    const assertSessionGate = (): void => {
      const currentLock = miniBrainLockGeneration();
      if (currentLock !== lockGeneration) throw new Error('MINI_BRAIN_SESSION_LOCKED');
      if (candidate && !rebindCandidateFor(candidate.owner_ref, candidate.lock_generation)) {
        throw new Error('MINI_BRAIN_REBIND_PROOF_EXPIRED');
      }
    };
    await vault.withSyncLock(async () => {
      assertSessionGate();
      const registration = await register({
        device_id: vault!.deviceId,
        public_key_jwk: vault!.publicKeyJwk,
      });
      assertSessionGate();
      if (registration.owner_ref !== ownerRef) throw new Error('MINI_BRAIN_OWNER_BINDING_MISMATCH');
      await vault!.bind(registration, candidate ? candidate.lock_generation : undefined);
      assertSessionGate();
    });
    assertSessionGate();
    if (lockGeneration !== null && vault.authorizedLockGeneration !== lockGeneration) throw new Error('MINI_BRAIN_SESSION_LOCKED');
    if (candidate) consumeRebindCandidate(candidate);
    return { state: 'ready', vault, reason_code: null };
  } catch (error) {
    vault?.close();
    return {
      state: 'unavailable',
      vault: null,
      reason_code: error instanceof Error ? error.message : 'MINI_BRAIN_OWNER_BINDING_FAILED',
    };
  }
}

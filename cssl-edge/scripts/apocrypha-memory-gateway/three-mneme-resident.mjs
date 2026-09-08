#!/usr/bin/env node

import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import {
  closeSync, existsSync, openSync, readFileSync, rmSync, statSync, writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve, win32 } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(import.meta.url);

export const THREE_MNEME_CONTRACT = Object.freeze({
  adapterPath: 'C:/Users/Apocky/.codex/staging/conversation-direct-release-2026-09-05T06-23-28-895Z/inprocess-windows-adapter/authority-auto-freshness-20260905.cjs',
  adapterSha256: '8077EC6F6593997061AA5E5EEC702336BFA4E7485FB534E5F43419ABDCA2B049',
  stateRoot: 'C:/Users/Apocky/AppData/Local/Packages/OpenAI.Codex_2p2nqsd0c76g0/LocalCache/Local/Apocky/3mneme',
  manifestSha256: 'C5BA505499BB71D11C6826E305F3352DCBDDA63885986C7F01422CDAD9CD1CEA',
  ownerEntropy: '3MNEME.desktop-broker.secret-bundle.v2',
  agentId: 'apocv4',
  projectId: 'apocv4-owner',
  host: '127.0.0.1',
  port: 8787,
  taskName: 'Apocky 3MNEME Resident',
});

const KOFFI_ROOT = dirname(THREE_MNEME_CONTRACT.adapterPath);
const KOFFI_PINS = Object.freeze({
  'node_modules/koffi/index.cjs': 'A200E15EC168D00CB3155794808A98243051C61DD7F6989C23C57EE220C918B9',
  'node_modules/koffi/src/koffi/index.cjs': '3135201866D6611F4E47C7976140409C3D6A69A4C77F201F18543518764407F3',
  'node_modules/koffi/src/koffi/src/static.cjs': 'CA1E8BB6A3262025530CFB99E6BA27E3581C4AABFAEABFDD502BACCB3755BC70',
  'node_modules/@koromix/koffi-win32-x64/package.json': '3C6D427795E7C1E18220DFA56D9DFDF392C62B074E7A061F84A55FE44857670F',
  'node_modules/@koromix/koffi-win32-x64/index.js': 'F9991EC70A0F775487FFADC2461D69EDD1555E3D9BF80DE0BBDD2EEC7D245C49',
  'node_modules/@koromix/koffi-win32-x64/win32_x64/koffi.node': '8623DC57F3093A457F71FCBE31FAD77741F5B648D8015F7C1DAC959595F0972B',
});

const SENSITIVE_ENV = [
  'DATABASE_URL', 'SUPABASE_URL', 'SUPABASE_SERVICE_ROLE_KEY',
  'THREEMNEME_BROKER_JWT_SECRET', 'BROKER_JWT_SECRET',
  'THREEMNEME_OWNER_CAPABILITY', 'THREEMNEME_BROKER_OWNER_CAPABILITY',
  'THREEMNEME_INSTANCE_NONCE',
];
const RECOVERABLE = new Set([
  'BROKER_ABSENT', 'BROKER_CHILD_EXITED', 'BROKER_DEADLINE_EXCEEDED',
  'BROKER_HEALTH_FAILED', 'BROKER_START_FAILED', 'BROKER_TRANSPORT_FAILED',
]);

let native;

function failure(code, metadata = {}) {
  const error = new Error(code);
  error.code = code;
  Object.assign(error, metadata);
  return error;
}

function fail(code, metadata) {
  throw failure(code, metadata);
}

function sha256Bytes(bytes) {
  return createHash('sha256').update(bytes).digest('hex').toUpperCase();
}

export function sha256File(file) {
  return sha256Bytes(readFileSync(file));
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function normalized(file) {
  return win32.resolve(file).toLowerCase();
}

function parseJsonBytes(bytes, code = 'JSON_INVALID') {
  try {
    return JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/u, ''));
  } catch {
    fail(code);
  }
}

function bindNative() {
  if (native) return native;
  if (process.platform !== 'win32' || process.arch !== 'x64') fail('WINDOWS_X64_REQUIRED');
  if (sha256File(THREE_MNEME_CONTRACT.adapterPath) !== THREE_MNEME_CONTRACT.adapterSha256) {
    fail('PINNED_ADAPTER_DRIFT');
  }
  for (const [relative, expected] of Object.entries(KOFFI_PINS)) {
    const candidate = join(KOFFI_ROOT, relative);
    if (!existsSync(candidate) || sha256File(candidate) !== expected) fail('NATIVE_DEPENDENCY_DRIFT');
  }

  const koffi = require(join(KOFFI_ROOT, 'node_modules/koffi/index.cjs'));
  if (koffi.version !== '3.2.1') fail('NATIVE_VERSION_DRIFT');
  const kernel = koffi.load('C:/Windows/System32/kernel32.dll');
  const advapi = koffi.load('C:/Windows/System32/advapi32.dll');
  const crypt = koffi.load('C:/Windows/System32/crypt32.dll');
  const iphlp = koffi.load('C:/Windows/System32/iphlpapi.dll');
  const psapi = koffi.load('C:/Windows/System32/psapi.dll');
  const blob = koffi.struct({ cbData: 'uint32_t', pbData: 'void *' });
  const blobPointer = koffi.pointer(blob);
  const bound = {
    koffi,
    CloseHandle: kernel.func('int __stdcall CloseHandle(void *handle)'),
    GetCurrentProcess: kernel.func('void * __stdcall GetCurrentProcess()'),
    GetFileAttributes: kernel.func('uint32_t __stdcall GetFileAttributesW(const char16_t *file)'),
    GetLastError: kernel.func('uint32_t __stdcall GetLastError()'),
    GetProcessTimes: kernel.func('int __stdcall GetProcessTimes(void *process, void *created, void *exited, void *kernel, void *user)'),
    LocalFree: kernel.func('void * __stdcall LocalFree(void *memory)'),
    MoveFileEx: kernel.func('int __stdcall MoveFileExW(const char16_t *existing, const char16_t *replacement, uint32_t flags)'),
    OpenProcess: kernel.func('void * __stdcall OpenProcess(uint32_t access, int inherit, uint32_t pid)'),
    OpenProcessToken: advapi.func('int __stdcall OpenProcessToken(void *process, uint32_t access, _Out_ void **token)'),
    GetTokenInformation: advapi.func('int __stdcall GetTokenInformation(void *token, int type, void *information, uint32_t length, _Out_ uint32_t *needed)'),
    GetNamedSecurityInfo: advapi.func('uint32_t __stdcall GetNamedSecurityInfoW(const char16_t *name, int type, uint32_t flags, _Out_ void **owner, void *group, _Out_ void **dacl, void *sacl, _Out_ void **descriptor)'),
    GetSecurityDescriptorControl: advapi.func('int __stdcall GetSecurityDescriptorControl(void *descriptor, _Out_ uint16_t *control, _Out_ uint32_t *revision)'),
    GetAce: advapi.func('int __stdcall GetAce(void *acl, uint32_t index, _Out_ void **ace)'),
    SidString: advapi.func('int __stdcall ConvertSidToStringSidW(void *sid, _Out_ void **text)'),
    SetNamedSecurityInfo: advapi.func('uint32_t __stdcall SetNamedSecurityInfoW(const char16_t *name, int type, uint32_t flags, void *owner, void *group, void *dacl, void *sacl)'),
    TcpTable: iphlp.func('uint32_t __stdcall GetExtendedTcpTable(void *table, _Inout_ uint32_t *size, int order, uint32_t family, int type, uint32_t reserved)'),
    GetModuleFileName: psapi.func('uint32_t __stdcall GetModuleFileNameExW(void *process, void *module, void *name, uint32_t capacity)'),
    Unprotect: crypt.func('__stdcall', 'CryptUnprotectData', 'int', [blobPointer, 'void *', blobPointer, 'void *', 'void *', 'uint32_t', koffi.out(blobPointer)]),
  };
  const sidText = (sid) => {
    const out = [null];
    if (!bound.SidString(sid, out)) fail('SID_READ_FAILED');
    try {
      return koffi.decode(out[0], 'char16_t', -1);
    } finally {
      bound.LocalFree(out[0]);
    }
  };
  const token = [null];
  if (!bound.OpenProcessToken(bound.GetCurrentProcess(), 8, token)) fail('TOKEN_READ_FAILED');
  try {
    const needed = [0];
    bound.GetTokenInformation(token[0], 1, null, 0, needed);
    if (needed[0] < 8 || needed[0] > 65_536) fail('TOKEN_SHAPE_INVALID');
    const bytes = Buffer.alloc(needed[0]);
    if (!bound.GetTokenInformation(token[0], 1, bytes, bytes.length, needed)) fail('TOKEN_READ_FAILED');
    bound.sid = sidText(koffi.decode(bytes, 'void *'));
  } finally {
    bound.CloseHandle(token[0]);
  }
  bound.sidText = sidText;
  native = bound;
  return bound;
}

function assertNoReparse(file) {
  const bound = bindNative();
  let current = win32.resolve(file);
  while (true) {
    const attributes = bound.GetFileAttributes(current);
    if (attributes === 0xffffffff) fail('PATH_UNAVAILABLE');
    if (attributes & 0x400) fail('REPARSE_PATH_REJECTED');
    const parent = win32.dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function withPrivateAcl(file, callback = () => true) {
  const bound = bindNative();
  assertNoReparse(file);
  if (!statSync(file).isFile()) fail('PRIVATE_FILE_REQUIRED');
  const owner = [null];
  const dacl = [null];
  const descriptor = [null];
  const result = bound.GetNamedSecurityInfo(file, 1, 5, owner, null, dacl, null, descriptor);
  if (result !== 0) fail('ACL_READ_FAILED');
  try {
    const control = [0];
    const revision = [0];
    if (!owner[0] || !dacl[0] || !descriptor[0]
      || !bound.GetSecurityDescriptorControl(descriptor[0], control, revision)) {
      fail('ACL_SHAPE_INVALID');
    }
    const acl = Buffer.from(new Uint8Array(bound.koffi.view(dacl[0], 8)));
    if (bound.sidText(owner[0]) !== bound.sid || !(control[0] & 0x1000) || acl.readUInt16LE(4) !== 1) {
      fail('OWNER_PRIVATE_ACL_REQUIRED');
    }
    const ace = [null];
    if (!bound.GetAce(dacl[0], 0, ace) || !ace[0]) fail('ACL_ACE_INVALID');
    const head = Buffer.from(new Uint8Array(bound.koffi.view(ace[0], 8)));
    if (head[0] !== 0 || (head.readUInt32LE(4) & 0x1f01ff) !== 0x1f01ff
      || bound.sidText(ace[0] + 8n) !== bound.sid) {
      fail('OWNER_ONLY_FULL_ACCESS_REQUIRED');
    }
    return callback(dacl[0]);
  } finally {
    if (descriptor[0]) bound.LocalFree(descriptor[0]);
  }
}

function protectLike(source, target) {
  assertNoReparse(target);
  withPrivateAcl(source, (dacl) => {
    if (bindNative().SetNamedSecurityInfo(target, 1, 0x80000004, null, null, dacl, null) !== 0) {
      fail('ACL_WRITE_FAILED');
    }
  });
  withPrivateAcl(target);
}

function atomicReplace(source, target) {
  if (!bindNative().MoveFileEx(source, target, 0x1 | 0x8)) fail('ATOMIC_REPLACE_FAILED');
}

function writePrivateAtomic(target, bytes, aclSource) {
  const temporary = `${target}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, bytes, { flag: 'wx', mode: 0o600 });
    protectLike(aclSource, temporary);
    atomicReplace(temporary, target);
    withPrivateAcl(target);
  } finally {
    if (existsSync(temporary)) rmSync(temporary, { force: true });
  }
}

function decryptCurrentUser(cipher, entropy) {
  const bound = bindNative();
  const output = {};
  const input = { cbData: cipher.length, pbData: cipher };
  const salt = { cbData: entropy.length, pbData: entropy };
  if (!bound.Unprotect(input, null, salt, null, null, 1, output)) fail('DPAPI_OPERATION_FAILED');
  try {
    if (!output.pbData || !Number.isInteger(output.cbData) || output.cbData < 1 || output.cbData > 1_048_576) {
      fail('DPAPI_OUTPUT_INVALID');
    }
    return Buffer.from(new Uint8Array(bound.koffi.view(output.pbData, output.cbData)));
  } finally {
    if (output.pbData) {
      if (Number.isInteger(output.cbData) && output.cbData > 0 && output.cbData <= 1_048_576) {
        new Uint8Array(bound.koffi.view(output.pbData, output.cbData)).fill(0);
      }
      bound.LocalFree(output.pbData);
    }
  }
}

export function validateSecretBundle(bundle) {
  if (!bundle || typeof bundle !== 'object' || bundle.version !== 2
    || bundle.canonical?.mode !== 'supabase-rpc'
    || typeof bundle.canonical.supabase_url !== 'string'
    || typeof bundle.canonical.service_role_key !== 'string'
    || typeof bundle.auth?.jwt_secret !== 'string'
    || typeof bundle.auth.owner_capability !== 'string') {
    fail('BROKER_BUNDLE_INVALID');
  }
  let canonical;
  try {
    canonical = new URL(bundle.canonical.supabase_url);
  } catch {
    fail('BROKER_CANONICAL_URL_INVALID');
  }
  if (canonical.protocol !== 'https:') fail('BROKER_CANONICAL_URL_INVALID');
  for (const value of [
    bundle.canonical.service_role_key, bundle.auth.jwt_secret, bundle.auth.owner_capability,
  ]) {
    if (Buffer.byteLength(value, 'utf8') < 32) fail('BROKER_BUNDLE_CREDENTIAL_INVALID');
  }
  if (bundle.auth.jwt_secret === bundle.auth.owner_capability) fail('BROKER_BUNDLE_CREDENTIAL_COLLISION');
  const ttl = Number(bundle.auth.jwt_ttl_seconds ?? 900);
  if (!Number.isInteger(ttl) || ttl < 30 || ttl > 3_600) fail('BROKER_BUNDLE_TTL_INVALID');
  return bundle;
}

function readSecretBundle(secretPath) {
  withPrivateAcl(secretPath);
  const cipher = readFileSync(secretPath);
  const cipherSha256 = sha256Bytes(cipher);
  const entropy = Buffer.from(THREE_MNEME_CONTRACT.ownerEntropy, 'utf8');
  let plain;
  try {
    plain = decryptCurrentUser(cipher, entropy);
    const bundle = validateSecretBundle(parseJsonBytes(plain, 'BROKER_BUNDLE_JSON_INVALID'));
    if (sha256File(secretPath) !== cipherSha256) fail('BROKER_BUNDLE_CHANGED_DURING_READ');
    return bundle;
  } finally {
    cipher.fill(0);
    entropy.fill(0);
    if (plain) plain.fill(0);
  }
}

export function validateSealedManifest(manifest, options = {}) {
  const stateRoot = options.stateRoot ?? THREE_MNEME_CONTRACT.stateRoot;
  const hashFile = options.hashFile ?? sha256File;
  if (!manifest || manifest.schema !== 1
    || normalized(manifest.state_root) !== normalized(stateRoot)
    || !Array.isArray(manifest.files)
    || manifest.files.length < 1
    || !/^[A-F0-9]{64}$/u.test(String(manifest.source_tree_sha256 ?? ''))
    || !/^[A-F0-9]{64}$/u.test(String(manifest.dist_tree_sha256 ?? ''))
    || !/^[A-F0-9]{64}$/u.test(String(manifest.package_lock_sha256 ?? ''))) {
    fail('SCHEDULED_MANIFEST_INVALID');
  }
  const scope = manifest.rotation_scope;
  if (!scope || scope.agent_id !== THREE_MNEME_CONTRACT.agentId
    || scope.subject !== THREE_MNEME_CONTRACT.agentId
    || !sameJson(scope.allowed_projects, [THREE_MNEME_CONTRACT.projectId])) {
    fail('ROTATION_SCOPE_DRIFT');
  }
  const labels = new Set();
  for (const entry of manifest.files) {
    if (!entry || typeof entry.label !== 'string' || typeof entry.path !== 'string'
      || !/^[A-F0-9]{64}$/u.test(String(entry.sha256 ?? '')) || labels.has(entry.label)) {
      fail('MANIFEST_FILE_ENTRY_INVALID');
    }
    labels.add(entry.label);
    if (!existsSync(entry.path) || !statSync(entry.path).isFile() || hashFile(entry.path) !== entry.sha256) {
      fail('PINNED_FILE_DRIFT', { label: entry.label });
    }
  }
  for (const required of ['launcher.start', 'dist:dist/index.js', 'package-lock.json']) {
    if (!labels.has(required)) fail('MANIFEST_REQUIRED_FILE_MISSING', { label: required });
  }
  return manifest;
}

function loadSealedManifest() {
  const manifestPath = join(THREE_MNEME_CONTRACT.stateRoot, 'scheduled/task-manifest.v1.json');
  withPrivateAcl(manifestPath);
  if (sha256File(manifestPath) !== THREE_MNEME_CONTRACT.manifestSha256) fail('SCHEDULED_MANIFEST_DRIFT');
  const manifest = validateSealedManifest(parseJsonBytes(readFileSync(manifestPath), 'SCHEDULED_MANIFEST_JSON_INVALID'));
  if (sha256File(manifestPath) !== THREE_MNEME_CONTRACT.manifestSha256) fail('MANIFEST_CHANGED_DURING_PREFLIGHT');
  const entrypoint = manifest.files.find((entry) => entry.label === 'dist:dist/index.js').path;
  return { manifest, manifestPath, entrypoint, brokerDirectory: dirname(dirname(entrypoint)) };
}

function tcpListeners(port = THREE_MNEME_CONTRACT.port) {
  const bound = bindNative();
  const result = [];
  for (const family of [2, 23]) {
    const size = [0];
    const first = bound.TcpTable(null, size, 0, family, 3, 0);
    if (![0, 122].includes(first) || size[0] < 4 || size[0] > 16 * 1_048_576) fail('TCP_TABLE_UNAVAILABLE');
    const bytes = Buffer.alloc(size[0]);
    if (bound.TcpTable(bytes, size, 0, family, 3, 0) !== 0) fail('TCP_TABLE_READ_FAILED');
    const count = bytes.readUInt32LE(0);
    const width = family === 2 ? 24 : 56;
    if (count * width + 4 > bytes.length) fail('TCP_TABLE_SHAPE_INVALID');
    for (let index = 0; index < count; index += 1) {
      const start = 4 + index * width;
      const observedPort = bytes.readUInt16BE(start + (family === 2 ? 8 : 20));
      if (observedPort !== port) continue;
      const address = family === 2
        ? [...bytes.subarray(start + 4, start + 8)].join('.')
        : 'ipv6';
      result.push({ address, pid: bytes.readUInt32LE(start + (family === 2 ? 20 : 52)) });
    }
  }
  return result;
}

function observeProcess(pid) {
  if (!Number.isInteger(pid) || pid < 1) fail('LEASE_PID_INVALID');
  const bound = bindNative();
  const handle = bound.OpenProcess(0x0410, 0, pid);
  if (!handle) fail('BROKER_PROCESS_UNAVAILABLE');
  try {
    const name = Buffer.alloc(65_536);
    const length = bound.GetModuleFileName(handle, null, name, name.length / 2);
    if (!length || length >= name.length / 2) fail('BROKER_PATH_UNAVAILABLE');
    const nodePath = name.subarray(0, length * 2).toString('utf16le');
    const created = Buffer.alloc(8);
    const exited = Buffer.alloc(8);
    const kernel = Buffer.alloc(8);
    const user = Buffer.alloc(8);
    if (!bound.GetProcessTimes(handle, created, exited, kernel, user)) fail('BROKER_START_UNAVAILABLE');
    const unixMilliseconds = Number(created.readBigUInt64LE(0) / 10_000n - 11_644_473_600_000n);
    return { nodePath, processStartUtc: new Date(unixMilliseconds).toISOString() };
  } finally {
    bound.CloseHandle(handle);
  }
}

async function health(port = THREE_MNEME_CONTRACT.port, timeoutMs = 5_000) {
  let response;
  try {
    response = await fetch(`http://${THREE_MNEME_CONTRACT.host}:${port}/api/3mneme/health`, {
      method: 'GET', signal: AbortSignal.timeout(timeoutMs), headers: { accept: 'application/json' },
    });
  } catch {
    fail('BROKER_TRANSPORT_FAILED');
  }
  if (!response.ok) fail('BROKER_HEALTH_FAILED');
  let payload;
  try {
    payload = await response.json();
  } catch {
    fail('BROKER_HEALTH_FAILED');
  }
  return payload;
}

function assertHealth(payload, instanceDigest) {
  if (!payload?.ok || !payload.canonical_ready
    || payload.security?.environment !== 'production'
    || payload.security?.listener !== 'loopback-only'
    || payload.security?.mint !== 'owner-capability'
    || payload.security?.data !== 'delegated-bearer'
    || payload.security?.instance_digest !== instanceDigest) {
    fail('BROKER_HEALTH_IDENTITY_MISMATCH');
  }
  return payload;
}

export function buildLease({ manifest, manifestPath, entrypoint, pid, processIdentity, instanceDigest, nodePath = process.execPath }) {
  return {
    schema: 1,
    created_at: new Date().toISOString(),
    pid,
    process_start_utc: processIdentity.processStartUtc,
    node_path: processIdentity.nodePath,
    node_sha256: sha256File(nodePath),
    entrypoint_path: entrypoint,
    entrypoint_sha256: sha256File(entrypoint),
    manifest_path: manifestPath,
    manifest_sha256: THREE_MNEME_CONTRACT.manifestSha256,
    source_tree_sha256: manifest.source_tree_sha256,
    dist_tree_sha256: manifest.dist_tree_sha256,
    package_lock_sha256: manifest.package_lock_sha256,
    broker_instance_nonce_digest: instanceDigest,
    listener: THREE_MNEME_CONTRACT.host,
    port: THREE_MNEME_CONTRACT.port,
  };
}

export function validateLease(lease, sealed, listener, hashFile = sha256File) {
  const { manifest, manifestPath, entrypoint } = sealed;
  if (!lease || lease.schema !== 1 || lease.pid !== listener.pid
    || lease.port !== THREE_MNEME_CONTRACT.port || lease.listener !== THREE_MNEME_CONTRACT.host
    || lease.manifest_sha256 !== THREE_MNEME_CONTRACT.manifestSha256
    || normalized(lease.manifest_path) !== normalized(manifestPath)
    || lease.source_tree_sha256 !== manifest.source_tree_sha256
    || lease.dist_tree_sha256 !== manifest.dist_tree_sha256
    || lease.package_lock_sha256 !== manifest.package_lock_sha256
    || typeof lease.broker_instance_nonce_digest !== 'string'
    || !/^[a-f0-9]{64}$/u.test(lease.broker_instance_nonce_digest)
    || normalized(lease.entrypoint_path) !== normalized(entrypoint)
    || hashFile(entrypoint) !== lease.entrypoint_sha256) {
    fail('BROKER_PROCESS_LEASE_MISMATCH');
  }
  return lease;
}

function verifyProcessAgainstLease(lease) {
  const observed = observeProcess(lease.pid);
  const expectedStart = Date.parse(lease.process_start_utc);
  const observedStart = Date.parse(observed.processStartUtc);
  if (!Number.isFinite(expectedStart) || !Number.isFinite(observedStart)
    || Math.abs(expectedStart - observedStart) > 1_000
    || normalized(observed.nodePath) !== normalized(lease.node_path)
    || sha256File(observed.nodePath) !== lease.node_sha256) {
    fail('BROKER_PROCESS_IDENTITY_MISMATCH');
  }
  return observed;
}

function pinnedAdapter() {
  if (sha256File(THREE_MNEME_CONTRACT.adapterPath) !== THREE_MNEME_CONTRACT.adapterSha256) {
    fail('PINNED_ADAPTER_DRIFT');
  }
  return require(THREE_MNEME_CONTRACT.adapterPath);
}

async function inspectExisting(sealed) {
  const listeners = tcpListeners();
  if (listeners.length === 0) return null;
  if (listeners.length !== 1 || listeners[0].address !== THREE_MNEME_CONTRACT.host) {
    fail('PORT_OWNERSHIP_CONFLICT');
  }
  const leasePath = join(THREE_MNEME_CONTRACT.stateRoot, 'broker-process-lease.v1.json');
  withPrivateAcl(leasePath);
  const before = sha256File(leasePath);
  const lease = validateLease(parseJsonBytes(readFileSync(leasePath), 'BROKER_LEASE_JSON_INVALID'), sealed, listeners[0]);
  verifyProcessAgainstLease(lease);
  try {
    assertHealth(await health(), lease.broker_instance_nonce_digest);
    await pinnedAdapter().preflight();
  } catch (error) {
    throw failure(error.code ?? 'BROKER_HEALTH_FAILED', {
      owned_pid: lease.pid,
      owned_lease_sha256: before,
    });
  }
  if (sha256File(leasePath) !== before
    || sha256File(sealed.manifestPath) !== THREE_MNEME_CONTRACT.manifestSha256) {
    fail('AUTHORITY_CHANGED_DURING_PREFLIGHT');
  }
  return { lease, leaseSha256: before };
}

function scrubBundle(bundle) {
  if (!bundle || typeof bundle !== 'object') return;
  if (bundle.canonical) {
    bundle.canonical.supabase_url = null;
    bundle.canonical.service_role_key = null;
  }
  if (bundle.auth) {
    bundle.auth.jwt_secret = null;
    bundle.auth.owner_capability = null;
  }
}

function brokerEnvironment(bundle, instanceNonce) {
  const environment = { ...process.env };
  for (const name of SENSITIVE_ENV) delete environment[name];
  Object.assign(environment, {
    SUPABASE_URL: bundle.canonical.supabase_url,
    SUPABASE_SERVICE_ROLE_KEY: bundle.canonical.service_role_key,
    THREEMNEME_BROKER_JWT_SECRET: bundle.auth.jwt_secret,
    THREEMNEME_OWNER_CAPABILITY: bundle.auth.owner_capability,
    THREEMNEME_INSTANCE_NONCE: instanceNonce,
    BROKER_JWT_TTL_SECONDS: String(bundle.auth.jwt_ttl_seconds ?? 900),
    PORT: String(THREE_MNEME_CONTRACT.port),
    HOST: THREE_MNEME_CONTRACT.host,
    THREEMNEME_LOCAL_ONLY: 'true',
    NODE_ENV: 'production',
  });
  return environment;
}

function preparePrivateLog(file, aclSource) {
  if (!existsSync(file)) {
    writeFileSync(file, Buffer.alloc(0), { flag: 'wx', mode: 0o600 });
    protectLike(aclSource, file);
  } else {
    withPrivateAcl(file);
  }
  return openSync(file, 'a');
}

async function waitForStartedChild(child, instanceDigest, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  let lastCode = 'BROKER_DEADLINE_EXCEEDED';
  while (Date.now() < deadline) {
    if (child.exitCode !== null) fail('BROKER_CHILD_EXITED');
    try {
      return assertHealth(await health(THREE_MNEME_CONTRACT.port, 2_000), instanceDigest);
    } catch (error) {
      lastCode = error.code ?? 'BROKER_HEALTH_FAILED';
      await new Promise((resolveDelay) => setTimeout(resolveDelay, 200));
    }
  }
  fail(lastCode === 'BROKER_TRANSPORT_FAILED' ? 'BROKER_DEADLINE_EXCEEDED' : lastCode);
}

async function stopSpawnedChild(child) {
  if (!child || child.exitCode !== null) return;
  const exited = new Promise((resolveExit) => child.once('exit', resolveExit));
  child.kill('SIGTERM');
  await Promise.race([exited, new Promise((resolveDelay) => setTimeout(resolveDelay, 3_000))]);
  if (child.exitCode === null) {
    child.kill('SIGKILL');
    await Promise.race([exited, new Promise((resolveDelay) => setTimeout(resolveDelay, 2_000))]);
  }
}

function restoreLeasePreimage(leasePath, preimage, writtenHash, aclSource) {
  if (!writtenHash || !existsSync(leasePath) || sha256File(leasePath) !== writtenHash) {
    return 'concurrent_change_preserved';
  }
  if (preimage) {
    writePrivateAtomic(leasePath, preimage, aclSource);
    return 'restored_exact_preimage';
  }
  rmSync(leasePath, { force: true });
  return 'removed_new_lease';
}

async function startBroker(sealed) {
  if (tcpListeners().length !== 0) fail('PORT_OWNERSHIP_CONFLICT');
  const stateRoot = THREE_MNEME_CONTRACT.stateRoot;
  const secretPath = join(stateRoot, 'broker-secrets.v2.dpapi');
  const leasePath = join(stateRoot, 'broker-process-lease.v1.json');
  const stdoutPath = join(stateRoot, 'broker.stdout.log');
  const stderrPath = join(stateRoot, 'broker.stderr.log');
  if (existsSync(leasePath)) withPrivateAcl(leasePath);
  const leasePreimage = existsSync(leasePath) ? readFileSync(leasePath) : null;
  let bundle;
  let child;
  let writtenLeaseHash;
  let stdout;
  let stderr;
  let instanceNonce;
  let environment;
  try {
    bundle = readSecretBundle(secretPath);
    const nonce = randomBytes(48);
    instanceNonce = nonce.toString('base64');
    const digest = createHash('sha256').update(instanceNonce, 'utf8').digest('hex');
    nonce.fill(0);
    environment = brokerEnvironment(bundle, instanceNonce);
    const entrypointPin = sealed.manifest.files.find((entry) => entry.label === 'dist:dist/index.js').sha256;
    if (sha256File(sealed.entrypoint) !== entrypointPin
      || sha256File(sealed.manifestPath) !== THREE_MNEME_CONTRACT.manifestSha256) {
      fail('PINNED_FILE_DRIFT');
    }
    stdout = preparePrivateLog(stdoutPath, secretPath);
    stderr = preparePrivateLog(stderrPath, secretPath);
    child = spawn(process.execPath, ['--enable-source-maps', sealed.entrypoint], {
      cwd: sealed.brokerDirectory,
      env: environment,
      detached: true,
      windowsHide: true,
      shell: false,
      stdio: ['ignore', stdout, stderr],
    });
    await new Promise((resolveSpawn, rejectSpawn) => {
      child.once('spawn', resolveSpawn);
      child.once('error', () => rejectSpawn(failure('BROKER_START_FAILED')));
    });
    await waitForStartedChild(child, digest);
    const listeners = tcpListeners();
    if (listeners.length !== 1 || listeners[0].pid !== child.pid
      || listeners[0].address !== THREE_MNEME_CONTRACT.host) {
      fail('BROKER_LISTENER_IDENTITY_MISMATCH');
    }
    const processIdentity = observeProcess(child.pid);
    if (normalized(processIdentity.nodePath) !== normalized(process.execPath)) fail('BROKER_PROCESS_IDENTITY_MISMATCH');
    const lease = buildLease({
      manifest: sealed.manifest,
      manifestPath: sealed.manifestPath,
      entrypoint: sealed.entrypoint,
      pid: child.pid,
      processIdentity,
      instanceDigest: digest,
    });
    const encodedLease = Buffer.from(JSON.stringify(lease, null, 2), 'utf8');
    writePrivateAtomic(leasePath, encodedLease, secretPath);
    encodedLease.fill(0);
    writtenLeaseHash = sha256File(leasePath);
    validateLease(parseJsonBytes(readFileSync(leasePath), 'BROKER_LEASE_JSON_INVALID'), sealed, listeners[0]);
    verifyProcessAgainstLease(lease);
    await pinnedAdapter().preflight();
    await pinnedAdapter().ensureFreshSameScope();
    child.unref();
    return {
      ok: true,
      effect: 'started',
      pid: child.pid,
      listener: THREE_MNEME_CONTRACT.host,
      manifest_sha256: THREE_MNEME_CONTRACT.manifestSha256,
      process_lease_sha256: writtenLeaseHash,
      instance_digest: digest,
      scope: { agent_id: THREE_MNEME_CONTRACT.agentId, allowed_projects: [THREE_MNEME_CONTRACT.projectId] },
    };
  } catch (error) {
    await stopSpawnedChild(child);
    const rollbackState = restoreLeasePreimage(leasePath, leasePreimage, writtenLeaseHash, secretPath);
    throw failure(error.code ?? 'BROKER_START_FAILED', { rollback_state: rollbackState });
  } finally {
    if (stdout !== undefined) closeSync(stdout);
    if (stderr !== undefined) closeSync(stderr);
    if (environment) for (const name of SENSITIVE_ENV) delete environment[name];
    if (leasePreimage) leasePreimage.fill(0);
    scrubBundle(bundle);
    instanceNonce = null;
  }
}

export async function ensureThreeMnemeResident({ renewCapability = true } = {}) {
  const sealed = loadSealedManifest();
  const existing = await inspectExisting(sealed);
  if (existing) {
    if (renewCapability) await pinnedAdapter().ensureFreshSameScope();
    return {
      ok: true,
      effect: 'already_healthy',
      pid: existing.lease.pid,
      listener: THREE_MNEME_CONTRACT.host,
      manifest_sha256: THREE_MNEME_CONTRACT.manifestSha256,
      process_lease_sha256: existing.leaseSha256,
      instance_digest: existing.lease.broker_instance_nonce_digest,
      scope: { agent_id: THREE_MNEME_CONTRACT.agentId, allowed_projects: [THREE_MNEME_CONTRACT.projectId] },
    };
  }
  return startBroker(sealed);
}

export async function statusThreeMnemeResident() {
  const sealed = loadSealedManifest();
  const existing = await inspectExisting(sealed);
  if (!existing) return { ok: false, state: 'absent', code: 'BROKER_ABSENT' };
  return {
    ok: true,
    state: 'ready',
    pid: existing.lease.pid,
    listener: THREE_MNEME_CONTRACT.host,
    manifest_sha256: THREE_MNEME_CONTRACT.manifestSha256,
    process_lease_sha256: existing.leaseSha256,
  };
}

export async function stopThreeMnemeResident() {
  const sealed = loadSealedManifest();
  const listeners = tcpListeners();
  if (listeners.length === 0) return { ok: true, effect: 'already_stopped' };
  if (listeners.length !== 1 || listeners[0].address !== THREE_MNEME_CONTRACT.host) {
    fail('PORT_OWNERSHIP_CONFLICT');
  }
  const leasePath = join(THREE_MNEME_CONTRACT.stateRoot, 'broker-process-lease.v1.json');
  withPrivateAcl(leasePath);
  const leaseSha256 = sha256File(leasePath);
  const lease = validateLease(
    parseJsonBytes(readFileSync(leasePath), 'BROKER_LEASE_JSON_INVALID'), sealed, listeners[0],
  );
  verifyProcessAgainstLease(lease);
  await recycleOwnedBroker({ ownedPid: lease.pid, ownedLeaseSha256: leaseSha256 });
  if (existsSync(leasePath) && sha256File(leasePath) === leaseSha256) rmSync(leasePath, { force: true });
  return { ok: true, effect: 'stopped', pid: lease.pid, process_lease: 'removed' };
}

export async function restartThreeMnemeResident() {
  const stopped = await stopThreeMnemeResident();
  const started = await ensureThreeMnemeResident();
  return { ok: true, effect: 'restarted', stopped, started };
}

export function preflightThreeMnemeResident() {
  const sealed = loadSealedManifest();
  const secretPath = join(THREE_MNEME_CONTRACT.stateRoot, 'broker-secrets.v2.dpapi');
  let bundle;
  try {
    bundle = readSecretBundle(secretPath);
    return {
      ok: true,
      state: 'launch_ready',
      bundle_schema: bundle.version,
      canonical: bundle.canonical.mode,
      manifest_sha256: THREE_MNEME_CONTRACT.manifestSha256,
      entrypoint_sha256: sha256File(sealed.entrypoint),
      node_sha256: sha256File(process.execPath),
      scope: { agent_id: THREE_MNEME_CONTRACT.agentId, allowed_projects: [THREE_MNEME_CONTRACT.projectId] },
    };
  } finally {
    scrubBundle(bundle);
  }
}

function xml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

export function buildScheduledTaskXml({
  nodePath = process.execPath,
  scriptPath = fileURLToPath(import.meta.url),
  expectedScriptSha256,
  sid,
  workingDirectory = dirname(fileURLToPath(import.meta.url)),
} = {}) {
  if (!sid || !/^S-1-[0-9-]+$/u.test(sid)) fail('TASK_PRINCIPAL_INVALID');
  if (!/^[A-F0-9]{64}$/u.test(expectedScriptSha256 ?? '')) fail('TASK_LAUNCHER_PIN_INVALID');
  for (const value of [nodePath, scriptPath, workingDirectory]) {
    if (typeof value !== 'string' || value.length < 1 || value.includes('"')) fail('TASK_ACTION_INVALID');
  }
  return `<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Direct-Node 3MNEME resident supervisor</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>${xml(sid)}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>${xml(sid)}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled><Hidden>true</Hidden><WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>
  </Settings>
  <Actions Context="Author"><Exec>
    <Command>${xml(nodePath)}</Command>
    <Arguments>&quot;${xml(scriptPath)}&quot; supervise --self-sha256 ${expectedScriptSha256}</Arguments>
    <WorkingDirectory>${xml(workingDirectory)}</WorkingDirectory>
  </Exec></Actions>
</Task>`;
}

function decodeSchedulerOutput(value) {
  if (!Buffer.isBuffer(value) || value.length === 0) return '';
  const utf16 = (value[0] === 0xff && value[1] === 0xfe)
    || value.subarray(0, Math.min(value.length, 128)).filter((byte, index) => index % 2 === 1 && byte === 0).length > 8;
  return value.toString(utf16 ? 'utf16le' : 'utf8').replace(/^\uFEFF/u, '');
}

function runScheduler(scheduler, args, maxBuffer = 65_536) {
  const result = spawnSync(scheduler, args, {
    windowsHide: true, shell: false, maxBuffer,
  });
  return {
    status: result.status,
    error: result.error,
    stdout: decodeSchedulerOutput(result.stdout),
    stderr: decodeSchedulerOutput(result.stderr),
  };
}

function scheduledTask(command) {
  if (!['register', 'unregister'].includes(command)) fail('TASK_COMMAND_INVALID');
  const scheduler = 'C:/Windows/System32/schtasks.exe';
  if (!existsSync(scheduler) || !statSync(scheduler).isFile()) fail('TASK_SCHEDULER_UNAVAILABLE');
  if (command === 'unregister') {
    const result = runScheduler(scheduler, ['/Delete', '/TN', THREE_MNEME_CONTRACT.taskName, '/F']);
    if (result.status !== 0) fail('TASK_UNREGISTER_FAILED');
    return { ok: true, effect: 'unregistered', task_name: THREE_MNEME_CONTRACT.taskName };
  }
  loadSealedManifest();
  const bound = bindNative();
  const secretPath = join(THREE_MNEME_CONTRACT.stateRoot, 'broker-secrets.v2.dpapi');
  const sourcePath = fileURLToPath(import.meta.url);
  const installedPath = join(THREE_MNEME_CONTRACT.stateRoot, 'scheduled', 'three-mneme-resident.mjs');
  const installedPreimage = existsSync(installedPath) ? readFileSync(installedPath) : null;
  const source = readFileSync(sourcePath);
  let installedHash;
  let taskChanged = false;
  const previousTask = runScheduler(
    scheduler, ['/Query', '/TN', THREE_MNEME_CONTRACT.taskName, '/XML', 'ONE'], 262_144,
  );

  const createFromXml = (taskXml) => {
    const temporary = join(THREE_MNEME_CONTRACT.stateRoot, 'scheduled', `three-mneme-resident.${randomUUID()}.xml`);
    const encoded = Buffer.from(`\uFEFF${taskXml}`, 'utf16le');
    try {
      writeFileSync(temporary, encoded, { flag: 'wx', mode: 0o600 });
      protectLike(secretPath, temporary);
      return runScheduler(scheduler, ['/Create', '/TN', THREE_MNEME_CONTRACT.taskName, '/XML', temporary, '/F']);
    } finally {
      encoded.fill(0);
      if (existsSync(temporary)) rmSync(temporary, { force: true });
    }
  };

  try {
    installedHash = sha256Bytes(source);
    writePrivateAtomic(installedPath, source, secretPath);
    if (sha256File(installedPath) !== installedHash) fail('TASK_LAUNCHER_INSTALL_FAILED');
    const taskXml = buildScheduledTaskXml({
      sid: bound.sid, scriptPath: installedPath, expectedScriptSha256: installedHash,
      workingDirectory: dirname(installedPath),
    });
    const result = createFromXml(taskXml);
    taskChanged = true;
    if (result.status !== 0) fail('TASK_REGISTER_FAILED');
    const readback = runScheduler(
      scheduler, ['/Query', '/TN', THREE_MNEME_CONTRACT.taskName, '/XML', 'ONE'], 262_144,
    );
    if (readback.status !== 0
      || !readback.stdout.includes(resolve(process.execPath))
      || !readback.stdout.includes(resolve(installedPath))
      || /powershell|pwsh|cmd\.exe/iu.test(readback.stdout)) {
      fail('TASK_READBACK_FAILED');
    }
    return {
      ok: true,
      effect: 'registered',
      task_name: THREE_MNEME_CONTRACT.taskName,
      action: 'direct_node',
      launcher_sha256: installedHash,
    };
  } catch (error) {
    let taskRollback = 'unchanged';
    if (taskChanged) {
      if (previousTask.status === 0 && previousTask.stdout) {
        taskRollback = createFromXml(previousTask.stdout).status === 0 ? 'restored_previous_task' : 'task_restore_failed';
      } else {
        const removed = runScheduler(scheduler, ['/Delete', '/TN', THREE_MNEME_CONTRACT.taskName, '/F']);
        taskRollback = removed.status === 0 ? 'removed_new_task' : 'task_remove_failed';
      }
    }
    let launcherRollback = 'unchanged';
    if (installedHash && existsSync(installedPath) && sha256File(installedPath) === installedHash) {
      if (installedPreimage) {
        writePrivateAtomic(installedPath, installedPreimage, secretPath);
        launcherRollback = 'restored_exact_preimage';
      } else {
        rmSync(installedPath, { force: true });
        launcherRollback = 'removed_new_launcher';
      }
    }
    throw failure(error.code ?? 'TASK_REGISTER_FAILED', {
      rollback_state: `${taskRollback}:${launcherRollback}`,
    });
  } finally {
    source.fill(0);
    if (installedPreimage) installedPreimage.fill(0);
  }
}

async function recycleOwnedBroker({ ownedPid, ownedLeaseSha256 }) {
  if (!Number.isInteger(ownedPid) || ownedPid < 1 || !/^[A-F0-9]{64}$/u.test(ownedLeaseSha256 ?? '')) {
    fail('OWNED_RECYCLE_IDENTITY_INVALID');
  }
  const sealed = loadSealedManifest();
  const listeners = tcpListeners();
  if (listeners.length === 0) return { effect: 'already_stopped' };
  if (listeners.length !== 1 || listeners[0].pid !== ownedPid
    || listeners[0].address !== THREE_MNEME_CONTRACT.host) {
    fail('OWNED_RECYCLE_LISTENER_DRIFT');
  }
  const leasePath = join(THREE_MNEME_CONTRACT.stateRoot, 'broker-process-lease.v1.json');
  withPrivateAcl(leasePath);
  if (sha256File(leasePath) !== ownedLeaseSha256) fail('OWNED_RECYCLE_LEASE_DRIFT');
  const lease = validateLease(parseJsonBytes(readFileSync(leasePath), 'BROKER_LEASE_JSON_INVALID'), sealed, listeners[0]);
  verifyProcessAgainstLease(lease);
  try {
    process.kill(ownedPid, 'SIGTERM');
  } catch (error) {
    if (error.code === 'ESRCH') return { effect: 'already_stopped' };
    throw error;
  }
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (tcpListeners().every((listener) => listener.pid !== ownedPid)) return { effect: 'recycled_owned_broker' };
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }
  try {
    process.kill(ownedPid, 'SIGKILL');
  } catch (error) {
    if (error.code === 'ESRCH') return { effect: 'recycled_owned_broker' };
    throw error;
  }
  await new Promise((resolveDelay) => setTimeout(resolveDelay, 250));
  if (tcpListeners().some((listener) => listener.pid === ownedPid)) fail('OWNED_RECYCLE_FAILED');
  return { effect: 'recycled_owned_broker' };
}

export async function superviseThreeMnemeResident({ intervalMs = 5_000 } = {}) {
  if (!Number.isInteger(intervalMs) || intervalMs < 1_000 || intervalMs > 60_000) fail('SUPERVISOR_INTERVAL_INVALID');
  let stopped = false;
  const stop = () => { stopped = true; };
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
  let failures = 0;
  while (!stopped) {
    try {
      const report = await ensureThreeMnemeResident();
      failures = 0;
      process.stdout.write(`${JSON.stringify({ ...report, observed_at: new Date().toISOString() })}\n`);
    } catch (error) {
      failures += 1;
      const code = error.code ?? 'SUPERVISOR_FAILURE';
      process.stderr.write(`${JSON.stringify({ ok: false, code, observed_at: new Date().toISOString() })}\n`);
      if (!RECOVERABLE.has(code)) throw error;
      if (code === 'BROKER_TRANSPORT_FAILED' && failures >= 3
        && Number.isInteger(error.owned_pid) && /^[A-F0-9]{64}$/u.test(error.owned_lease_sha256 ?? '')) {
        await recycleOwnedBroker({ ownedPid: error.owned_pid, ownedLeaseSha256: error.owned_lease_sha256 });
        failures = 0;
      }
    }
    if (!stopped) {
      const backoff = failures > 0 ? Math.min(60_000, intervalMs * (2 ** Math.min(failures - 1, 4))) : intervalMs;
      await new Promise((resolveDelay) => setTimeout(resolveDelay, backoff));
    }
  }
  return { ok: true, effect: 'stopped' };
}

async function main(argv) {
  const pinIndex = argv.indexOf('--self-sha256');
  if (pinIndex >= 0) {
    const expected = argv[pinIndex + 1];
    if (!/^[A-F0-9]{64}$/u.test(expected ?? '')
      || sha256File(fileURLToPath(import.meta.url)) !== expected) {
      fail('TASK_LAUNCHER_DRIFT');
    }
    argv = [...argv.slice(0, pinIndex), ...argv.slice(pinIndex + 2)];
  }
  const command = argv[0] ?? 'supervise';
  let report;
  if (command === 'ensure') report = await ensureThreeMnemeResident();
  else if (command === 'status') report = await statusThreeMnemeResident();
  else if (command === 'preflight') report = preflightThreeMnemeResident();
  else if (command === 'stop') report = await stopThreeMnemeResident();
  else if (command === 'restart') report = await restartThreeMnemeResident();
  else if (command === 'supervise') report = await superviseThreeMnemeResident();
  else if (command === 'register' || command === 'unregister') report = scheduledTask(command);
  else fail('COMMAND_INVALID');
  process.stdout.write(`${JSON.stringify(report)}\n`);
}

const invokedAs = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : '';
if (invokedAs === import.meta.url) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${JSON.stringify({ ok: false, code: error.code ?? 'THREE_MNEME_RESIDENT_FAILED' })}\n`);
    process.exitCode = 1;
  });
}

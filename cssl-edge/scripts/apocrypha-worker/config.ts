import { existsSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256, stableJson } from './crypto';
import type { MemoryProbeScope, WorkerConfig, WorkerManifest } from './types';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const EXACT_MODEL_ALIAS = 'qwen35-35b-a3b-q4';
const EXACT_PROFILE_HASH = '5d390055297aed74dbba092eb313dc8c4bf4e551ca4bf2c50fed16c8cb3a21a9';
const EXACT_TOOL_REGISTRY_VERSION = 'apocrypha-readonly-v1';
const EXACT_MEMORY_MANIFEST_HASH = '307a86ce2ec83a37ad30f86327195e47259167728cf32e4276af377f08988273';
const EXACT_MEMORY_ADAPTERS = [
  'mempalace', 'brainmonsoon', 'anamnesis', 'graphify', 'mneme', 'metaharness',
] as const;
const EXACT_CAPABILITIES = ['apocky_owner_chat', 'chaos_tarot_reading', 'apocky_member_chat'] as const;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;

function integerEnv(env: NodeJS.ProcessEnv, name: string, fallback: number, min: number, max: number): number {
  const raw = env[name];
  if (!raw) return fallback;
  const parsed = Number(raw);
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`${name} must be an integer from ${min} through ${max}`);
  }
  return parsed;
}

function boolEnv(env: NodeJS.ProcessEnv, name: string, fallback: boolean): boolean {
  const raw = env[name]?.trim().toLowerCase();
  if (!raw) return fallback;
  if (['1', 'true', 'yes', 'on'].includes(raw)) return true;
  if (['0', 'false', 'no', 'off'].includes(raw)) return false;
  throw new Error(`${name} must be true or false`);
}

function required(env: NodeJS.ProcessEnv, name: string): string {
  const value = env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function firstCsv(value: string | undefined): string | null {
  return value?.split(',').map((item) => item.trim()).find(Boolean) ?? null;
}

function additionalProbeScopes(
  env: NodeJS.ProcessEnv,
  manifest: WorkerManifest,
): ReadonlyArray<MemoryProbeScope> {
  const name = 'APOCRYPHA_MEMORY_ADDITIONAL_PROBE_SCOPES';
  const raw = env[name]?.trim();
  if (!raw) return [];
  const result: MemoryProbeScope[] = [];
  const seen = new Set<string>();
  for (const entry of raw.split(',').map((item) => item.trim()).filter(Boolean)) {
    const parts = entry.split(':');
    if (parts.length !== 3 || parts.some((part) => !part || /[\r\n\0]/u.test(part))) {
      throw new Error(`${name} entries must use tenant_id:principal_id:capability`);
    }
    const [tenantId, principalId, capability] = parts as [string, string, string];
    if (tenantId.length > 160 || principalId.length > 160 || capability.length > 128) {
      throw new Error(`${name} contains an overlong value`);
    }
    if (!manifest.capabilities.includes(capability)) {
      throw new Error(`${name} capability is not admitted by the worker manifest`);
    }
    if (seen.has(entry)) continue;
    seen.add(entry);
    result.push({ tenantId, principalId, capability });
  }
  return result;
}

function safeUrl(raw: string, label: string, allowLoopbackHttp: boolean): string {
  const url = new URL(raw);
  const loopback = ['127.0.0.1', 'localhost', '::1', '[::1]'].includes(url.hostname);
  if (url.protocol !== 'https:' && !(allowLoopbackHttp && loopback && url.protocol === 'http:')) {
    throw new Error(`${label} must use HTTPS${allowLoopbackHttp ? ' or loopback HTTP' : ''}`);
  }
  url.pathname = url.pathname.replace(/\/+$/, '');
  return url.toString().replace(/\/$/, '');
}

export function loadManifest(path = join(moduleDir, 'manifest.production.json')): WorkerManifest {
  const manifest = JSON.parse(readFileSync(path, 'utf8')) as WorkerManifest;
  if (manifest.schema !== 'apocrypha.worker-manifest.v1') throw new Error('unsupported worker manifest schema');
  if (!manifest.model?.alias || !manifest.model.profileHash) throw new Error('manifest model is incomplete');
  if (!manifest.tools?.registryVersion || manifest.tools.mode !== 'read-only') {
    throw new Error('worker tool registry must be explicitly read-only');
  }
  if (manifest.memory?.tenantScoped !== true || !Array.isArray(manifest.memory.adapters)) {
    throw new Error('worker memory manifest must be tenant scoped');
  }
  if (!Array.isArray(manifest.capabilities) || manifest.capabilities.length === 0) {
    throw new Error('worker manifest needs at least one capability');
  }
  if (manifest.model.alias !== EXACT_MODEL_ALIAS || manifest.model.profileHash.toLowerCase() !== EXACT_PROFILE_HASH) {
    throw new Error('worker manifest differs from the exact accepted Qwen profile');
  }
  if (manifest.tools.registryVersion !== EXACT_TOOL_REGISTRY_VERSION) {
    throw new Error('worker manifest differs from the exact accepted read-only tool registry');
  }
  const adapterNames = manifest.memory.adapters.map((adapter) => adapter.name);
  if (adapterNames.length !== EXACT_MEMORY_ADAPTERS.length
    || adapterNames.some((name, index) => name !== EXACT_MEMORY_ADAPTERS[index])
    || memoryManifestHash(manifest) !== EXACT_MEMORY_MANIFEST_HASH) {
    throw new Error('worker manifest differs from the exact accepted six-adapter memory manifest');
  }
  if (manifest.capabilities.length !== EXACT_CAPABILITIES.length
    || manifest.capabilities.some((capability, index) => capability !== EXACT_CAPABILITIES[index])) {
    throw new Error('worker manifest differs from the exact accepted runtime capabilities');
  }
  return manifest;
}

export function memoryManifestHash(manifest: WorkerManifest): string {
  return sha256(stableJson(manifest.memory));
}

export function loadConfig(
  env: NodeJS.ProcessEnv = process.env,
  argv: string[] = process.argv.slice(2),
): WorkerConfig {
  const manifestRawPath = env.APOCRYPHA_WORKER_MANIFEST_PATH?.trim() || join(moduleDir, 'manifest.production.json');
  const manifestPath = isAbsolute(manifestRawPath) ? manifestRawPath : resolve(process.cwd(), manifestRawPath);
  if (!existsSync(manifestPath)) throw new Error(`worker manifest not found: ${manifestPath}`);
  const manifest = loadManifest(manifestPath);

  const journalDefault = env.LOCALAPPDATA
    ? join(env.LOCALAPPDATA, 'Apocrypha', 'worker-journal')
    : join(moduleDir, '.journal');
  const defaultProfilePath = 'D:\\Apocrypha\\models\\Qwen3.5-35B-A3B-Q4\\runtime-profile.json';
  const profilePathRaw = env.APOCRYPHA_QWEN_RUNTIME_PROFILE_PATH?.trim()
    || (existsSync(defaultProfilePath) ? defaultProfilePath : '');
  const gatewayHost = env.APOCRYPHA_MEMORY_GATEWAY_HOST?.trim() || '127.0.0.1';
  const gatewayPort = env.APOCRYPHA_MEMORY_GATEWAY_PORT?.trim();
  const readinessUrlRaw = env.APOCRYPHA_MEMORY_READINESS_URL?.trim()
    || (gatewayPort ? `http://${gatewayHost}:${gatewayPort}/ready` : '');
  const readinessToken = env.APOCRYPHA_MEMORY_READINESS_TOKEN?.trim()
    || env.APOCRYPHA_MEMORY_GATEWAY_TOKEN?.trim() || null;

  const config: WorkerConfig = {
    controlPlaneUrl: safeUrl(required(env, 'APOCRYPHA_CONTROL_PLANE_URL'), 'APOCRYPHA_CONTROL_PLANE_URL', false),
    nodeId: required(env, 'APOCRYPHA_WORKER_NODE_ID'),
    nodeToken: required(env, 'APOCRYPHA_WORKER_TOKEN'),
    qwenBaseUrl: safeUrl(env.APOCRYPHA_QWEN_BASE_URL?.trim() || 'http://127.0.0.1:19124/v1', 'APOCRYPHA_QWEN_BASE_URL', true),
    runtimeProfilePath: profilePathRaw ? resolve(profilePathRaw) : null,
    modelAlias: env.APOCRYPHA_MODEL_ALIAS?.trim() || manifest.model.alias,
    profileHash: env.APOCRYPHA_MODEL_PROFILE_HASH?.trim().toLowerCase() || manifest.model.profileHash.toLowerCase(),
    toolRegistryVersion: env.APOCRYPHA_TOOL_REGISTRY_VERSION?.trim() || manifest.tools.registryVersion,
    memoryManifestHash: env.APOCRYPHA_MEMORY_MANIFEST_HASH?.trim().toLowerCase() || memoryManifestHash(manifest),
    manifest,
    pollIntervalMs: integerEnv(env, 'APOCRYPHA_WORKER_POLL_MS', 1500, 250, 60_000),
    claimLeaseSeconds: integerEnv(env, 'APOCRYPHA_WORKER_LEASE_SECONDS', 180, 30, 900),
    leaseRenewIntervalMs: integerEnv(env, 'APOCRYPHA_WORKER_RENEW_MS', 10_000, 1_000, 60_000),
    leaseExpiryGraceMs: integerEnv(env, 'APOCRYPHA_WORKER_LEASE_GRACE_MS', 5_000, 1_000, 30_000),
    controlPlaneTimeoutMs: integerEnv(env, 'APOCRYPHA_CONTROL_PLANE_TIMEOUT_MS', 15_000, 1_000, 60_000),
    chunkFlushMs: integerEnv(env, 'APOCRYPHA_WORKER_CHUNK_FLUSH_MS', 1_000, 100, 10_000),
    chunkMaxChars: integerEnv(env, 'APOCRYPHA_WORKER_CHUNK_CHARS', 256, 32, 4_096),
    qwenIdleTimeoutMs: integerEnv(env, 'APOCRYPHA_QWEN_IDLE_TIMEOUT_MS', 180_000, 10_000, 900_000),
    qwenMaxRuntimeMs: integerEnv(env, 'APOCRYPHA_QWEN_MAX_RUNTIME_MS', 2_700_000, 60_000, 7_200_000),
    contextWindowTokens: integerEnv(env, 'APOCRYPHA_QWEN_CONTEXT_TOKENS', 4_096, 1_024, 131_072),
    maxOutputTokens: integerEnv(env, 'APOCRYPHA_QWEN_MAX_OUTPUT_TOKENS', 2_048, 64, 8_192),
    journalDir: resolve(env.APOCRYPHA_WORKER_JOURNAL_DIR?.trim() || journalDefault),
    healthHost: env.APOCRYPHA_WORKER_HEALTH_HOST?.trim() || '127.0.0.1',
    healthPort: integerEnv(env, 'APOCRYPHA_WORKER_HEALTH_PORT', 19_126, 1_024, 65_535),
    heartbeatIntervalMs: integerEnv(env, 'APOCRYPHA_WORKER_HEARTBEAT_MS', 15_000, 5_000, 300_000),
    heartbeatEnabled: boolEnv(env, 'APOCRYPHA_WORKER_HEARTBEAT_ENABLED', true),
    memoryReadConcurrency: integerEnv(env, 'APOCRYPHA_MEMORY_READ_CONCURRENCY', 1, 1, 6),
    memoryReadinessUrl: readinessUrlRaw
      ? safeUrl(readinessUrlRaw, 'APOCRYPHA_MEMORY_READINESS_URL', true) : null,
    memoryReadinessToken: readinessToken,
    memoryProbeTenantId: env.APOCRYPHA_MEMORY_PROBE_TENANT_ID?.trim()
      || firstCsv(env.APOCRYPHA_MEMORY_GATEWAY_ALLOWED_TENANTS),
    memoryProbePrincipalId: env.APOCRYPHA_MEMORY_PROBE_PRINCIPAL_ID?.trim() || required(env, 'APOCRYPHA_WORKER_NODE_ID'),
    memoryProbeCapability: env.APOCRYPHA_MEMORY_PROBE_CAPABILITY?.trim()
      || (manifest.capabilities.includes('chaos_tarot_reading') ? 'chaos_tarot_reading' : manifest.capabilities[0] as string),
    memoryAdditionalProbeScopes: additionalProbeScopes(env, manifest),
    once: argv.includes('--once'),
    probeOnly: argv.includes('--probe'),
    recoverOnly: argv.includes('--recover-only'),
  };

  if (config.modelAlias !== manifest.model.alias) throw new Error('configured model alias differs from accepted worker manifest');
  for (const [label, value, maximum] of [
    ['memory probe tenant', config.memoryProbeTenantId, 160],
    ['memory probe principal', config.memoryProbePrincipalId, 160],
    ['memory probe capability', config.memoryProbeCapability, 128],
  ] as const) {
    if (value !== null && (!value || value.length > maximum || /[\r\n\0]/u.test(value))) {
      throw new Error(`${label} is invalid`);
    }
  }
  if (!manifest.capabilities.includes(config.memoryProbeCapability)) {
    throw new Error('memory probe capability is not admitted by the worker manifest');
  }
  const memberProbeScopes = [
    ...(config.memoryProbeTenantId && config.memoryProbeCapability === 'apocky_member_chat' ? [{
      tenantId: config.memoryProbeTenantId,
      principalId: config.memoryProbePrincipalId,
      capability: config.memoryProbeCapability,
    }] : []),
    ...(config.memoryAdditionalProbeScopes ?? []).filter((scope) => scope.capability === 'apocky_member_chat'),
  ];
  if (manifest.capabilities.includes('apocky_member_chat') && memberProbeScopes.length === 0) {
    throw new Error('APOCRYPHA_MEMORY_ADDITIONAL_PROBE_SCOPES must include an apocky_member_chat tenant and principal');
  }
  if (memberProbeScopes.some((scope) => !UUID.test(scope.tenantId) || !UUID.test(scope.principalId))) {
    throw new Error('apocky_member_chat probe tenant and principal must be canonical UUIDs');
  }
  if (config.profileHash !== manifest.model.profileHash.toLowerCase()) {
    throw new Error('configured model profile hash differs from worker manifest');
  }
  if (config.runtimeProfilePath) {
    if (!existsSync(config.runtimeProfilePath)) throw new Error(`Qwen runtime profile not found: ${config.runtimeProfilePath}`);
    const actualProfileHash = sha256(readFileSync(config.runtimeProfilePath)).toLowerCase();
    if (actualProfileHash !== config.profileHash) {
      throw new Error(`Qwen runtime profile hash mismatch: expected ${config.profileHash}, received ${actualProfileHash}`);
    }
  }
  if (config.leaseRenewIntervalMs >= config.claimLeaseSeconds * 1_000 - config.leaseExpiryGraceMs) {
    throw new Error('lease renewal interval must leave room before lease expiry');
  }
  if (config.maxOutputTokens > config.contextWindowTokens - 768) {
    throw new Error('maximum output tokens must leave at least 768 context tokens for instructions and input');
  }
  return config;
}

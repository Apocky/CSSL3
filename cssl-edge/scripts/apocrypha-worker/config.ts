import { existsSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256, stableJson } from './crypto';
import type { WorkerConfig, WorkerManifest } from './types';

const moduleDir = dirname(fileURLToPath(import.meta.url));

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
    once: argv.includes('--once'),
    probeOnly: argv.includes('--probe'),
    recoverOnly: argv.includes('--recover-only'),
  };

  if (config.modelAlias !== manifest.model.alias) throw new Error('configured model alias differs from accepted worker manifest');
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

import { isAbsolute, resolve } from 'node:path';
import { ADAPTER_NAMES, type AdapterName, type GatewayConfig } from './types';

function required(env: NodeJS.ProcessEnv, name: string): string {
  const value = env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function integer(env: NodeJS.ProcessEnv, name: string, fallback: number, min: number, max: number): number {
  const raw = env[name]?.trim();
  if (!raw) return fallback;
  const parsed = Number(raw);
  if (!Number.isInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`${name} must be an integer from ${min} through ${max}`);
  }
  return parsed;
}

function allowlist(env: NodeJS.ProcessEnv, name: string): ReadonlySet<string> {
  const values = required(env, name).split(',').map((value) => value.trim()).filter(Boolean);
  if (values.length === 0 || values.includes('*')) throw new Error(`${name} must be a closed non-wildcard list`);
  if (values.some((value) => value.length > 160 || /[\r\n\0]/u.test(value))) {
    throw new Error(`${name} contains an invalid value`);
  }
  return new Set(values);
}

function absolutePath(env: NodeJS.ProcessEnv, name: string): string | undefined {
  const raw = env[name]?.trim();
  if (!raw) return undefined;
  if (!isAbsolute(raw)) throw new Error(`${name} must be absolute`);
  if (process.platform === 'win32' && !/^[a-z]:[\\/]/iu.test(raw)) {
    throw new Error(`${name} must be a local drive path`);
  }
  return resolve(raw);
}

export function isLoopbackUrl(raw: string): boolean {
  try {
    const url = new URL(raw);
    return url.protocol === 'http:' && ['127.0.0.1', '::1', '[::1]'].includes(url.hostname)
      && !url.username && !url.password;
  } catch {
    return false;
  }
}

function upstreams(env: NodeJS.ProcessEnv): GatewayConfig['upstreams'] {
  const result: GatewayConfig['upstreams'] = {};
  for (const name of ADAPTER_NAMES) {
    const prefix = `APOCRYPHA_${name.toUpperCase()}_READ`;
    const url = env[`${prefix}_UPSTREAM_URL`]?.trim();
    if (!url) continue;
    const healthUrl = env[`${prefix}_HEALTH_URL`]?.trim();
    if (!isLoopbackUrl(url) || (healthUrl && !isLoopbackUrl(healthUrl))) {
      throw new Error(`${prefix}_UPSTREAM_URL and health URL must use loopback HTTP`);
    }
    result[name as AdapterName] = {
      url,
      token: env[`${prefix}_UPSTREAM_TOKEN`]?.trim() || undefined,
      healthUrl: healthUrl || undefined,
    };
  }
  return result;
}

export function loadGatewayConfig(env: NodeJS.ProcessEnv = process.env): GatewayConfig {
  const host = env.APOCRYPHA_MEMORY_GATEWAY_HOST?.trim() || '127.0.0.1';
  if (host !== '127.0.0.1' && host !== '::1') {
    throw new Error('APOCRYPHA_MEMORY_GATEWAY_HOST must be 127.0.0.1 or ::1');
  }
  const token = required(env, 'APOCRYPHA_MEMORY_GATEWAY_TOKEN');
  if (Buffer.byteLength(token, 'utf8') < 32 || Buffer.byteLength(token, 'utf8') > 512) {
    throw new Error('APOCRYPHA_MEMORY_GATEWAY_TOKEN must contain 32 through 512 UTF-8 bytes');
  }
  for (const name of ['APOCRYPHA_MEMORY_OWNER_ID', 'APOCRYPHA_MEMORY_PRIVACY_PARTITION']) {
    const value = env[name]?.trim();
    if (value && (value.length > 160 || /[\r\n\0]/u.test(value))) throw new Error(`${name} is invalid`);
  }
  return {
    host,
    port: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_PORT', 19_127, 1_024, 65_535),
    token,
    allowedTenants: allowlist(env, 'APOCRYPHA_MEMORY_GATEWAY_ALLOWED_TENANTS'),
    allowedPrincipals: allowlist(env, 'APOCRYPHA_MEMORY_GATEWAY_ALLOWED_PRINCIPALS'),
    allowedCapabilities: allowlist(env, 'APOCRYPHA_MEMORY_GATEWAY_ALLOWED_CAPABILITIES'),
    limits: {
      bodyBytes: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_BODY_BYTES', 32_768, 1_024, 131_072),
      queryBytes: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_QUERY_BYTES', 4_000, 64, 8_192),
      responseBytes: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_RESPONSE_BYTES', 65_536, 4_096, 262_144),
      recordChars: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_RECORD_CHARS', 7_000, 128, 16_384),
      totalChars: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_TOTAL_CHARS', 28_000, 512, 131_072),
      maxRecords: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_MAX_RECORDS', 12, 1, 40),
      timeoutMs: integer(env, 'APOCRYPHA_MEMORY_GATEWAY_TIMEOUT_MS', 3_500, 250, 30_000),
    },
    native: {
      ownerId: env.APOCRYPHA_MEMORY_OWNER_ID?.trim() || undefined,
      federatorExecutable: absolutePath(env, 'APOCRYPHA_MEMORY_FEDERATOR_EXE'),
      federatorConfig: absolutePath(env, 'APOCRYPHA_MEMORY_FEDERATOR_CONFIG'),
      mempalaceDb: absolutePath(env, 'APOCRYPHA_MEMPALACE_DB_PATH'),
      privacyPartition: env.APOCRYPHA_MEMORY_PRIVACY_PARTITION?.trim() || undefined,
      graphExecutable: absolutePath(env, 'APOCRYPHA_GRAPH_ORGAN_EXE'),
      graphPath: absolutePath(env, 'APOCRYPHA_GRAPH_PATH'),
      graphCsl: absolutePath(env, 'APOCRYPHA_GRAPH_CSL_PATH'),
      graphNil: absolutePath(env, 'APOCRYPHA_GRAPH_NIL_PATH'),
      graphCssl: absolutePath(env, 'APOCRYPHA_GRAPH_CSSL_PATH'),
    },
    upstreams: upstreams(env),
  };
}

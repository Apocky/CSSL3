import { createHash, timingSafeEqual } from 'node:crypto';
import type { IncomingMessage } from 'node:http';
import type { GatewayConfig, SearchRequest } from './types';

export class GatewayError extends Error {
  constructor(readonly status: number, readonly code: string) {
    super(code);
  }
}

const REQUEST_FIELDS = new Set([
  'operation', 'read_only', 'query', 'limit', 'tenant_id', 'principal_id', 'capability', 'memory_manifest_hash',
]);

const DYNAMIC_MEMBER_CAPABILITIES = new Set(['chaos_tarot_reading', 'apocky_member_chat']);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;

function principalAdmitted(
  tenantId: string,
  principalId: string,
  capability: string,
  config: GatewayConfig,
): boolean {
  if (DYNAMIC_MEMBER_CAPABILITIES.has(capability)) {
    return config.allowedDynamicMemberScopes.has(`${tenantId}\0${capability}`)
      && UUID.test(principalId);
  }
  return config.allowedPrincipals.has(principalId)
    && config.allowedOwnerScopes.has(`${tenantId}\0${principalId}`);
}

export function isLoopbackAddress(address: string | undefined): boolean {
  if (!address) return false;
  return address === '127.0.0.1' || address === '::1' || address === '::ffff:127.0.0.1';
}

export function verifyBearer(request: IncomingMessage, expected: string): void {
  const value = request.headers.authorization ?? '';
  const candidate = value.startsWith('Bearer ') ? value.slice(7) : '';
  const expectedHash = createHash('sha256').update(expected).digest();
  const candidateHash = createHash('sha256').update(candidate).digest();
  if (!candidate || !timingSafeEqual(candidateHash, expectedHash)) throw new GatewayError(401, 'AUTH_REQUIRED');
}

export function validateSearchRequest(value: unknown, config: GatewayConfig): SearchRequest {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new GatewayError(400, 'REQUEST_INVALID');
  const body = value as Record<string, unknown>;
  if (Object.keys(body).some((key) => !REQUEST_FIELDS.has(key))) throw new GatewayError(400, 'REQUEST_FIELDS_INVALID');
  if (body.operation !== 'search' || body.read_only !== true) throw new GatewayError(403, 'READ_ONLY_REQUIRED');
  if (typeof body.query !== 'string' || body.query.trim() !== body.query || !body.query) {
    throw new GatewayError(400, 'QUERY_INVALID');
  }
  if (Buffer.byteLength(body.query, 'utf8') > config.limits.queryBytes || /\0/u.test(body.query)) {
    throw new GatewayError(413, 'QUERY_TOO_LARGE');
  }
  if (!Number.isInteger(body.limit) || Number(body.limit) < 1 || Number(body.limit) > config.limits.maxRecords) {
    throw new GatewayError(400, 'LIMIT_INVALID');
  }
  for (const [field, maximum] of [['tenant_id', 160], ['principal_id', 160], ['capability', 128]] as const) {
    const item = body[field];
    if (typeof item !== 'string' || !item || item.length > maximum || /[\r\n\0]/u.test(item)) {
      throw new GatewayError(400, `${field.toUpperCase()}_INVALID`);
    }
  }
  const tenantId = body.tenant_id as string;
  const principalId = body.principal_id as string;
  const capability = body.capability as string;
  if (!config.allowedTenants.has(tenantId)) throw new GatewayError(403, 'TENANT_DENIED');
  if (!config.allowedCapabilities.has(capability)) throw new GatewayError(403, 'CAPABILITY_DENIED');
  if (!principalAdmitted(tenantId, principalId, capability, config)) throw new GatewayError(403, 'PRINCIPAL_DENIED');
  if (body.memory_manifest_hash !== undefined
    && (typeof body.memory_manifest_hash !== 'string' || !/^[a-f0-9]{64}$/u.test(body.memory_manifest_hash))) {
    throw new GatewayError(400, 'MANIFEST_HASH_INVALID');
  }
  return body as unknown as SearchRequest;
}

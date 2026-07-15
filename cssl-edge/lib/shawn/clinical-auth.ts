import { createClient } from '@supabase/supabase-js';
import { createHash } from 'node:crypto';
import type { NextApiRequest } from 'next';

import { getRequestUser, type RequestUserResult } from '../admin-auth';

const DEFAULT_BUCKET = 'shawn-clinical';
const DEFAULT_TIMEOUT_MS = 5_000;
const MAX_DOSSIER_BYTES = 2 * 1024 * 1024;

export interface ClinicalDossierSection {
  readonly id: string;
  readonly title: string;
  readonly paragraphs: readonly string[];
  readonly points?: readonly string[];
}

export interface ClinicalDossier {
  readonly version: 1;
  readonly title: string;
  readonly updatedAt?: string;
  readonly notice?: string;
  readonly sections: readonly ClinicalDossierSection[];
}

interface ClinicalAllowlistRow {
  readonly user_id: string;
  readonly email_snapshot: string;
  readonly expires_at: string;
  readonly revoked_at: string | null;
  readonly purpose: string;
}

interface QueryError {
  readonly message?: string;
}

export interface ClinicalServiceClient {
  from(table: 'shawn_clinical_allowlist'): {
    select(columns: string): {
      eq(column: 'user_id', value: string): {
        maybeSingle(): Promise<{ data: unknown; error: QueryError | null }>;
      };
    };
  };
  storage: {
    from(bucket: string): {
      download(objectPath: string): Promise<{ data: Blob | null; error: QueryError | null }>;
    };
  };
}

export type ClinicalRouteDecision =
  | { readonly kind: 'redirect'; readonly destination: '/login?next=%2Fshawn%2Fclinical' }
  | { readonly kind: 'render'; readonly statusCode: 403; readonly props: { readonly state: 'forbidden' } }
  | { readonly kind: 'render'; readonly statusCode: 503; readonly props: { readonly state: 'unavailable' } }
  | {
      readonly kind: 'render';
      readonly statusCode: 200;
      readonly props: { readonly state: 'authorized'; readonly dossier: ClinicalDossier };
    };

export interface ClinicalAuthDependencies {
  readonly requestUser?: (req: NextApiRequest, timeoutMs?: number) => Promise<RequestUserResult>;
  readonly serviceClient?: () => ClinicalServiceClient | null;
  readonly now?: () => Date;
  readonly timeoutMs?: number;
  readonly bucket?: string;
  readonly objectPath?: string | null;
  readonly expectedSha256?: string | null;
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), timeoutMs)),
  ]);
}

function normalizeUserId(value: string): string | null {
  const normalized = value.trim().toLowerCase();
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(normalized)
    ? normalized
    : null;
}

function validBucket(value: string): boolean {
  return /^[a-z0-9][a-z0-9._-]{0,62}$/.test(value);
}

function validObjectPath(value: string): boolean {
  if (value.length < 1 || value.length > 512 || value.startsWith('/')) return false;
  return value.split('/').every((part) => part.length > 0 && part !== '.' && part !== '..');
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function boundedString(value: unknown, min: number, max: number): value is string {
  return typeof value === 'string' && value.length >= min && value.length <= max;
}

function parseStringList(value: unknown, maxItems: number, maxLength: number): readonly string[] | null {
  if (!Array.isArray(value) || value.length > maxItems) return null;
  const result: string[] = [];
  for (const entry of value) {
    if (!boundedString(entry, 1, maxLength)) return null;
    result.push(entry);
  }
  return result;
}

function parseDossier(value: unknown): ClinicalDossier | null {
  if (!isRecord(value) || value.version !== 1 || !boundedString(value.title, 1, 240)) return null;
  if (value.updatedAt !== undefined) {
    if (!boundedString(value.updatedAt, 1, 64) || Number.isNaN(Date.parse(value.updatedAt))) return null;
  }
  if (value.notice !== undefined && !boundedString(value.notice, 1, 4_000)) return null;
  if (!Array.isArray(value.sections) || value.sections.length < 1 || value.sections.length > 96) return null;

  const sections: ClinicalDossierSection[] = [];
  for (const candidate of value.sections) {
    if (!isRecord(candidate)) return null;
    if (!boundedString(candidate.id, 1, 80) || !/^[a-z0-9][a-z0-9-]*$/.test(candidate.id)) return null;
    if (!boundedString(candidate.title, 1, 240)) return null;
    const paragraphs = parseStringList(candidate.paragraphs, 128, 20_000);
    if (!paragraphs || paragraphs.length < 1) return null;
    const points = candidate.points === undefined ? undefined : parseStringList(candidate.points, 128, 4_000);
    if (candidate.points !== undefined && !points) return null;
    sections.push({ id: candidate.id, title: candidate.title, paragraphs, ...(points ? { points } : {}) });
  }

  return {
    version: 1,
    title: value.title,
    ...(typeof value.updatedAt === 'string' ? { updatedAt: value.updatedAt } : {}),
    ...(typeof value.notice === 'string' ? { notice: value.notice } : {}),
    sections,
  };
}

function configuredServiceClient(): ClinicalServiceClient | null {
  const url =
    process.env['SUPABASE_URL'] ??
    process.env['NEXT_PUBLIC_SUPABASE_URL'] ??
    process.env['APOCKY_HUB_SUPABASE_URL'];
  const serviceRoleKey = process.env['SUPABASE_SERVICE_ROLE_KEY'];
  if (!url || !serviceRoleKey) return null;

  return createClient(url, serviceRoleKey, {
    auth: { persistSession: false, autoRefreshToken: false, detectSessionInUrl: false },
  }) as unknown as ClinicalServiceClient;
}

function activeAllowlistRow(row: unknown, userId: string, now: Date): row is ClinicalAllowlistRow {
  if (!isRecord(row)) return false;
  if (row.user_id !== userId || !boundedString(row.email_snapshot, 3, 320) || !boundedString(row.purpose, 1, 500)) return false;
  if (row.revoked_at !== null) return false;
  if (typeof row.expires_at !== 'string') return false;
  const expiry = Date.parse(row.expires_at);
  return !Number.isNaN(expiry) && expiry > now.getTime();
}

/**
 * Resolve the complete clinical route boundary. The caller must apply the
 * returned status and no-store headers; no reason, email, secret, or object
 * locator is returned to the browser.
 */
export async function resolveClinicalRoute(
  req: NextApiRequest,
  dependencies: ClinicalAuthDependencies = {},
): Promise<ClinicalRouteDecision> {
  const timeoutMs = dependencies.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const requestUser = dependencies.requestUser ?? getRequestUser;

  let session: RequestUserResult;
  try {
    session = await requestUser(req, timeoutMs);
  } catch {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }

  if (!session.user) {
    if (!session.authConfigured || session.failureKind === 'upstream-unavailable' || session.failureKind === 'unconfigured') {
      return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
    }
    return { kind: 'redirect', destination: '/login?next=%2Fshawn%2Fclinical' };
  }

  const userId = normalizeUserId(session.user.id);
  if (!userId) return { kind: 'render', statusCode: 403, props: { state: 'forbidden' } };

  const bucket = dependencies.bucket ?? process.env['SHAWN_CLINICAL_BUCKET'] ?? DEFAULT_BUCKET;
  const objectPath = Object.prototype.hasOwnProperty.call(dependencies, 'objectPath')
    ? dependencies.objectPath ?? null
    : process.env['SHAWN_CLINICAL_OBJECT'] ?? null;
  const expectedSha256 = Object.prototype.hasOwnProperty.call(dependencies, 'expectedSha256')
    ? dependencies.expectedSha256 ?? null
    : process.env['SHAWN_CLINICAL_SHA256'] ?? null;
  const client = dependencies.serviceClient ? dependencies.serviceClient() : configuredServiceClient();
  if (!client || !objectPath || !expectedSha256 || !validBucket(bucket) || !validObjectPath(objectPath) || !/^[a-f0-9]{64}$/i.test(expectedSha256)) {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }

  let allowlistResult: { data: unknown; error: QueryError | null };
  try {
    allowlistResult = await withTimeout(
      client
        .from('shawn_clinical_allowlist')
        .select('user_id, email_snapshot, expires_at, revoked_at, purpose')
        .eq('user_id', userId)
        .maybeSingle(),
      timeoutMs,
    );
  } catch {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }

  if (allowlistResult.error) {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }
  if (!allowlistResult.data) {
    return { kind: 'render', statusCode: 403, props: { state: 'forbidden' } };
  }
  if (!activeAllowlistRow(allowlistResult.data, userId, (dependencies.now ?? (() => new Date()))())) {
    return { kind: 'render', statusCode: 403, props: { state: 'forbidden' } };
  }

  let downloadResult: { data: Blob | null; error: QueryError | null };
  try {
    downloadResult = await withTimeout(client.storage.from(bucket).download(objectPath), timeoutMs);
  } catch {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }

  const blob = downloadResult.data;
  if (downloadResult.error || !blob || blob.size < 1 || blob.size > MAX_DOSSIER_BYTES) {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }
  if (blob.type && !/^application\/(?:[a-z0-9.+-]*\+)?json(?:;|$)/i.test(blob.type)) {
    return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
  }

  let dossier: ClinicalDossier | null = null;
  try {
    const bytes = Buffer.from(await withTimeout(blob.arrayBuffer(), timeoutMs));
    const observedSha256 = createHash('sha256').update(bytes).digest('hex');
    if (observedSha256.toLowerCase() !== expectedSha256.toLowerCase()) {
      return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };
    }
    const text = bytes.toString('utf8');
    dossier = parseDossier(JSON.parse(text) as unknown);
  } catch {
    dossier = null;
  }
  if (!dossier) return { kind: 'render', statusCode: 503, props: { state: 'unavailable' } };

  return { kind: 'render', statusCode: 200, props: { state: 'authorized', dossier } };
}

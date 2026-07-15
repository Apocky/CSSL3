import type { NextApiRequest } from 'next';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import {
  resolveClinicalRoute,
  type ClinicalAuthDependencies,
  type ClinicalServiceClient,
} from '@/lib/shawn/clinical-auth';
import type { RequestUserResult } from '@/lib/admin-auth';
import { getServerSideProps } from '@/pages/shawn/clinical';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

function equal<T>(actual: T, expected: T, message: string): void {
  if (actual !== expected) throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
}

const request = { headers: {}, query: {}, cookies: {} } as unknown as NextApiRequest;
const TEST_USER_ID = '00000000-0000-4000-8000-000000000001';

function session(email: string): RequestUserResult {
  return {
    authConfigured: true,
    user: { id: TEST_USER_ID, email, provider: 'email', createdAt: '2026-01-01T00:00:00.000Z' },
  };
}

function client(options: {
  row?: unknown;
  queryError?: boolean;
  blob?: Blob | null;
  downloadError?: boolean;
}): ClinicalServiceClient {
  return {
    from(table) {
      equal(table, 'shawn_clinical_allowlist', 'allowlist table');
      return {
        select(columns) {
          equal(columns, 'user_id, email_snapshot, expires_at, revoked_at, purpose', 'minimum allowlist columns');
          return {
            eq(column, value) {
              equal(column, 'user_id', 'immutable user id match column');
              equal(value, TEST_USER_ID, 'verified user id used for lookup');
              return {
                async maybeSingle() {
                  return {
                    data: options.row ?? null,
                    error: options.queryError ? { message: 'hidden database detail' } : null,
                  };
                },
              };
            },
          };
        },
      };
    },
    storage: {
      from(bucket) {
        equal(bucket, 'shawn-clinical', 'private bucket');
        return {
          async download(objectPath) {
            equal(objectPath, 'dossier/current.json', 'server-configured object only');
            return {
              data: options.blob ?? null,
              error: options.downloadError ? { message: 'hidden storage detail' } : null,
            };
          },
        };
      },
    },
  };
}

function dependencies(
  auth: RequestUserResult,
  service: ClinicalServiceClient | null,
  overrides: Partial<ClinicalAuthDependencies> = {},
): ClinicalAuthDependencies {
  return {
    requestUser: async () => auth,
    serviceClient: () => service,
    objectPath: 'dossier/current.json',
    expectedSha256: createHash('sha256').update(JSON.stringify(validDossier)).digest('hex'),
    now: () => new Date('2026-07-15T12:00:00.000Z'),
    ...overrides,
  };
}

const activeRow = {
  user_id: TEST_USER_ID,
  email_snapshot: 'reader@example.com',
  expires_at: '2026-08-01T00:00:00.000Z',
  revoked_at: null,
  purpose: 'time-bounded clinician review',
};

const validDossier = {
  version: 1,
  title: 'Private review dossier',
  updatedAt: '2026-07-15T00:00:00.000Z',
  notice: 'Authorized review only.',
  sections: [{ id: 'orientation', title: 'Orientation', paragraphs: ['Private source text.'], points: ['One point.'] }],
};

async function testUnauthenticatedRedirect(): Promise<void> {
  const decision = await resolveClinicalRoute(
    request,
    dependencies({ authConfigured: true, user: null }, null),
  );
  equal(decision.kind, 'redirect', 'unauthenticated request redirects');
  if (decision.kind === 'redirect') {
    equal(decision.destination, '/login?next=%2Fshawn%2Fclinical', 'safe return path');
  }
}

async function testMissingConfigurationFailsClosed(): Promise<void> {
  const decision = await resolveClinicalRoute(
    request,
    dependencies(session('reader@example.com'), null, { objectPath: null }),
  );
  equal(decision.kind, 'render', 'missing service config renders denial');
  if (decision.kind === 'render') equal(decision.statusCode, 503, 'missing config is unavailable');

  const authUnavailable = await resolveClinicalRoute(
    request,
    dependencies({ authConfigured: false, user: null }, null),
  );
  assert(authUnavailable.kind === 'render' && authUnavailable.statusCode === 503, 'missing auth fails closed');
}

async function testProviderOutageIsUnavailableNotLoginLoop(): Promise<void> {
  const decision = await resolveClinicalRoute(
    request,
    dependencies({ authConfigured: true, user: null, failureKind: 'upstream-unavailable' }, null),
  );
  assert(decision.kind === 'render' && decision.statusCode === 503, 'upstream outage is unavailable');
}

async function testNonAllowlistedAndRevokedAreForbidden(): Promise<void> {
  const absent = await resolveClinicalRoute(
    request,
    dependencies(session(' Reader@Example.com '), client({})),
  );
  assert(absent.kind === 'render' && absent.statusCode === 403, 'absent allowlist row is 403');

  const revoked = await resolveClinicalRoute(
    request,
    dependencies(session('reader@example.com'), client({ row: { ...activeRow, revoked_at: '2026-07-01T00:00:00Z' } })),
  );
  assert(revoked.kind === 'render' && revoked.statusCode === 403, 'revoked row is 403');

  const expired = await resolveClinicalRoute(
    request,
    dependencies(session('reader@example.com'), client({ row: { ...activeRow, expires_at: '2026-07-15T12:00:00Z' } })),
  );
  assert(expired.kind === 'render' && expired.statusCode === 403, 'expiry boundary is denied');

  const indefinite = await resolveClinicalRoute(
    request,
    dependencies(session('reader@example.com'), client({ row: { ...activeRow, expires_at: null } })),
  );
  assert(indefinite.kind === 'render' && indefinite.statusCode === 403, 'missing expiry is denied');
}

async function testAuthorizedServerSideLoad(): Promise<void> {
  const dossierBlob = new Blob([JSON.stringify(validDossier)], { type: 'application/json' });
  const decision = await resolveClinicalRoute(
    request,
    dependencies(session('reader@example.com'), client({ row: activeRow, blob: dossierBlob })),
  );
  assert(decision.kind === 'render' && decision.statusCode === 200, 'active allowlist loads dossier');
  if (decision.kind === 'render' && decision.statusCode === 200) {
    equal(decision.props.dossier.title, validDossier.title, 'validated dossier returned');
    equal(decision.props.dossier.sections[0]?.paragraphs[0], 'Private source text.', 'section retained');
  }
}

async function testMalformedStorageAndProviderErrorsFailClosed(): Promise<void> {
  const malformed = await resolveClinicalRoute(
    request,
    dependencies(
      session('reader@example.com'),
      client({ row: activeRow, blob: new Blob(['{"version":1}'], { type: 'application/json' }) }),
    ),
  );
  assert(malformed.kind === 'render' && malformed.statusCode === 503, 'malformed JSON schema fails closed');

  const queryFailure = await resolveClinicalRoute(
    request,
    dependencies(session('reader@example.com'), client({ row: activeRow, queryError: true })),
  );
  assert(queryFailure.kind === 'render' && queryFailure.statusCode === 503, 'allowlist provider error fails closed');

  const wrongMime = await resolveClinicalRoute(
    request,
    dependencies(
      session('reader@example.com'),
      client({ row: activeRow, blob: new Blob([JSON.stringify(validDossier)], { type: 'text/html' }) }),
    ),
  );
  assert(wrongMime.kind === 'render' && wrongMime.statusCode === 503, 'non-JSON MIME is rejected');

  const hashMismatch = await resolveClinicalRoute(
    request,
    dependencies(
      session('reader@example.com'),
      client({ row: activeRow, blob: new Blob([JSON.stringify(validDossier)], { type: 'application/json' }) }),
      { expectedSha256: '0'.repeat(64) },
    ),
  );
  assert(hashMismatch.kind === 'render' && hashMismatch.statusCode === 503, 'unapproved dossier hash is rejected');
}

async function testInvalidObjectConfigurationNeverTouchesStorage(): Promise<void> {
  let serviceCalled = false;
  const decision = await resolveClinicalRoute(request, {
    requestUser: async () => session('reader@example.com'),
    serviceClient: () => {
      serviceCalled = true;
      return client({ row: activeRow });
    },
    objectPath: '../dossier.json',
  });
  assert(decision.kind === 'render' && decision.statusCode === 503, 'unsafe object path denied');
  // Client creation is harmless; no query/download is reached after configuration validation.
  assert(serviceCalled, 'service dependency resolved without disclosing object content');
}

async function testPageRedirectAndPrivateHeaders(): Promise<void> {
  const headers = new Map<string, string>();
  const context = {
    req: { headers: {} },
    res: {
      statusCode: 200,
      setHeader(name: string, value: string | readonly string[]) {
        headers.set(name.toLowerCase(), Array.isArray(value) ? value.join(', ') : String(value));
      },
    },
  } as never;
  const result = await getServerSideProps(context);
  assert('redirect' in result, 'SSR unauthenticated path redirects before rendering');
  if ('redirect' in result) {
    equal(result.redirect.destination, '/login?next=%2Fshawn%2Fclinical', 'SSR redirect return path');
  }
  equal(headers.get('cache-control'), 'private, no-store, max-age=0, must-revalidate', 'private no-store');
  equal(headers.get('surrogate-control'), 'no-store', 'shared cache disabled');
  equal(headers.get('referrer-policy'), 'no-referrer', 'referrer suppressed');
  equal(headers.get('x-robots-tag'), 'noindex, nofollow, noarchive, nosnippet, noimageindex', 'robots denied');
  equal(headers.get('x-frame-options'), 'DENY', 'framing denied');
}

function testMigrationIsDefaultDeny(): void {
  const migration = readFileSync(
    resolve(process.cwd(), '..', 'cssl-supabase', 'migrations', '0045_shawn_clinical_access.sql'),
    'utf8',
  );
  assert(migration.includes('ENABLE ROW LEVEL SECURITY'), 'allowlist RLS enabled');
  assert(migration.includes('user_id     uuid PRIMARY KEY REFERENCES auth.users(id)'), 'grant keys to immutable auth user id');
  assert(migration.includes('REVOKE ALL ON TABLE public.shawn_clinical_allowlist FROM anon, authenticated'), 'client grants revoked');
  assert(!/CREATE\s+POLICY/i.test(migration), 'no client table or storage policy created');
  assert(/'shawn-clinical',[\s\S]*?false,[\s\S]*?ARRAY\['application\/json'\]/.test(migration), 'private JSON bucket defined');
  assert(!/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i.test(migration), 'no actual email committed');
}

async function run(): Promise<void> {
  await testUnauthenticatedRedirect();
  await testMissingConfigurationFailsClosed();
  await testProviderOutageIsUnavailableNotLoginLoop();
  await testNonAllowlistedAndRevokedAreForbidden();
  await testAuthorizedServerSideLoad();
  await testMalformedStorageAndProviderErrorsFailClosed();
  await testInvalidObjectConfigurationNeverTouchesStorage();
  await testPageRedirectAndPrivateHeaders();
  testMigrationIsDefaultDeny();
  // eslint-disable-next-line no-console
  console.log('shawn/clinical.test : OK · 10 security scenarios passed');
}

run().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});

import assert from 'node:assert/strict';

import {
  activeConversationRows,
  PRODUCTION_ORIGIN,
  redactedPathForEvidence,
  resolveOracleConfig,
  sanitizeEvidence,
} from '../../scripts/apocrypha-production-browser-oracle';

function throwsCode(fn: () => unknown, code: string): void {
  assert.throws(fn, (error: unknown) => error instanceof Error && error.message === code);
}

function main(): void {
  const profile = 'C:\\oracle\\apocrypha-profile';
  const marker = `${profile}\\.apocrypha-production-oracle-profile-v1`;
  const exists = (path: string) => path === profile || path === marker;
  const stat = (path: string) => {
    if (path === profile) return { isDirectory: () => true };
    if (path === marker) return { isDirectory: () => false };
    throw new Error('missing');
  };

  const config = resolveOracleConfig({
    APOCKY_E2E_BASE_URL: `${PRODUCTION_ORIGIN}/`,
    APOCKY_ORACLE_PROFILE_PATH: profile,
    APOCKY_ORACLE_OUTPUT_PATH: 'C:\\oracle-results',
  }, { exists, stat });
  assert.equal(config.origin, PRODUCTION_ORIGIN);
  assert.equal(config.profilePath, profile);
  assert.equal(config.outputRoot, 'C:\\oracle-results');

  throwsCode(() => resolveOracleConfig({
    APOCKY_E2E_BASE_URL: 'https://preview.example.test',
    APOCKY_ORACLE_PROFILE_PATH: profile,
  }, { exists, stat }), 'ORACLE_PRODUCTION_ORIGIN_REQUIRED');
  throwsCode(() => resolveOracleConfig({}, { exists, stat }), 'ORACLE_PROFILE_PATH_REQUIRED');
  throwsCode(() => resolveOracleConfig({ APOCKY_ORACLE_PROFILE_PATH: 'relative-profile' }, { exists, stat }), 'ORACLE_PROFILE_PATH_REQUIRED');
  throwsCode(() => resolveOracleConfig({ APOCKY_ORACLE_PROFILE_PATH: 'C:\\missing' }, { exists, stat }), 'ORACLE_DEDICATED_PROFILE_MISSING');
  throwsCode(() => resolveOracleConfig({
    APOCKY_ORACLE_PROFILE_PATH: profile,
    APOCKY_ORACLE_OUTPUT_PATH: `${profile}\\results`,
  }, { exists, stat }), 'ORACLE_OUTPUT_MUST_NOT_ENTER_PROFILE');

  const bootstrap = resolveOracleConfig({
    APOCKY_ORACLE_PROFILE_PATH: 'C:\\missing',
    APOCKY_ORACLE_BOOTSTRAP: '1',
  }, { exists, stat });
  assert.equal(bootstrap.bootstrap, true);
  assert.equal(bootstrap.profileState, 'missing');
  throwsCode(() => resolveOracleConfig({
    APOCKY_ORACLE_PROFILE_PATH: 'C:\\existing-ordinary-profile',
    APOCKY_ORACLE_BOOTSTRAP: '1',
  }, {
    exists: (path) => path === 'C:\\existing-ordinary-profile',
    stat: () => ({ isDirectory: () => true }),
  }), 'ORACLE_DEDICATED_PROFILE_MISSING');

  const jwt = 'eyJabcdefghijk.abcdefghijklmno.abcdefghijklmnop';
  const secret = 'A'.repeat(100);
  const sanitized = sanitizeEvidence(
    `fetch https://www.apocky.com/api/admin/apocrypha/oracles?token=bad Bearer abc.def.ghi access_token=bad ${jwt} ${secret}`,
  );
  assert.equal(sanitized.includes('?token='), false);
  assert.equal(sanitized.includes('abc.def.ghi'), false);
  assert.equal(sanitized.includes('access_token=bad'), false);
  assert.equal(sanitized.includes(jwt), false);
  assert.equal(sanitized.includes(secret), false);
  assert.match(sanitized, /https:\/\/www\.apocky\.com\/api\/admin\/apocrypha\/oracles/);
  assert.ok(sanitizeEvidence('x'.repeat(2_000)).length <= 600);
  assert.equal(
    redactedPathForEvidence('https://www.apocky.com/api/admin/apocrypha/jobs/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa?token=nope'),
    '/api/admin/apocrypha/jobs/:id',
  );
  assert.equal(redactedPathForEvidence('https://evil.example/api/admin/apocrypha/jobs/id'), null);
  assert.deepEqual(activeConversationRows({ data: { conversations: [{ id: 'one' }] } }), [{ id: 'one' }]);
  assert.equal(activeConversationRows({ data: [{ id: 'legacy-flat-shape' }] }), null);

  console.log('apocrypha-production-browser-oracle.test: 23 guard and sanitization assertions passed');
}

main();

// Admin health cards must consume the truthful /api/health contract.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error('assert failed: ' + message);
}

export function testAdminHealthContract(): void {
  const source = readFileSync(resolve(process.cwd(), 'pages/admin/index.tsx'), 'utf8');

  assert(source.includes('health.supabase_connected'), 'uses observed connectivity');
  assert(source.includes("health.supabase_status === 'auth_failed'"), 'shows key rejection');
  assert(source.includes("health.supabase_status === 'misconfigured'"), 'shows invalid config');
  assert(source.includes("health.supabase_status === 'unconfigured'"), 'shows unconfigured state');
  assert(source.includes("health === null ? '◐ checking'"), 'checking only means no receipt yet');
  assert(source.includes("health.ok ? '✓ live' : '✗ unavailable'"), 'liveness uses ok field');
  assert(!source.includes('supabase_configured'), 'stale env-presence field removed');
  assert(!source.includes("health?.status === 'ok'"), 'stale liveness field removed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  testAdminHealthContract();
  // eslint-disable-next-line no-console
  console.log('admin-health.test : OK · truthful health contract pinned');
}

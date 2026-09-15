// The governed code-effect routes.
//
// This file used to assert mostly against components/apocrypha/PublicChat.tsx — a chat surface that
// had already been retired from every route and has now been deleted. Those assertions described a
// UI nobody could reach; the ones below describe the two BFF routes, which are live and are where
// the authorization actually has to hold.
//
// The contract that matters: a code effect and a rollback are owner-only and explicitly confirmed,
// and the browser is never the thing that decides either. The server re-verifies for itself, before
// it submits anything.

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const codeRoute = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocv4/code.ts'), 'utf8');
const rollbackRoute = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocv4/code/rollback.ts'), 'utf8');

assert(codeRoute.includes('getAdminAuthorization(req)'), 'code BFF independently re-verifies owner authorization');
assert(codeRoute.includes('body.confirm_apply !== true'), 'code BFF rejects absent confirmation');
assert(
  codeRoute.indexOf('getAdminAuthorization(req)') < codeRoute.indexOf('submitRuntimeCode({'),
  'authorization precedes the code effect',
);
assert(rollbackRoute.includes('getAdminAuthorization(req)'), 'rollback BFF independently re-verifies owner authorization');
assert(rollbackRoute.includes('body.confirm_rollback !== true'), 'rollback BFF rejects absent confirmation');
assert(
  rollbackRoute.indexOf('getAdminAuthorization(req)') < rollbackRoute.indexOf('submitRuntimeRollback('),
  'authorization precedes rollback',
);

// No browser surface reaches these routes today. That is a fact worth pinning: if one is added
// later, it must be added deliberately, not inherited from a component nobody remembered was there.
for (const retired of ['PublicChat.tsx', 'WorkspacePanel.tsx']) {
  const path = resolve(process.cwd(), 'components/apocrypha', retired);
  let exists = true;
  try { readFileSync(path); } catch { exists = false; }
  assert(!exists, `${retired} was retired; it must not come back unreferenced`);
}

console.log('public-apocrypha-code-mode.test : OK · 8 governed-effect authorization checks passed');

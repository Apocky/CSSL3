import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const component = readFileSync(
  resolve(process.cwd(), 'components/apocrypha/PublicChat.tsx'),
  'utf8',
);
const style = readFileSync(
  resolve(process.cwd(), 'styles/PublicApocrypha.module.css'),
  'utf8',
);
const codeRoute = readFileSync(
  resolve(process.cwd(), 'pages/api/admin/apocv4/code.ts'),
  'utf8',
);
const rollbackRoute = readFileSync(
  resolve(process.cwd(), 'pages/api/admin/apocv4/code/rollback.ts'),
  'utf8',
);

const ownerGate = "access === 'owner' && mode === 'code'";
const effectBranch = component.indexOf('if (runCodeEffect)');
const responseOnlyBranch = component.indexOf("authFetch('/api/apocrypha/chat'");

assert(component.includes(ownerGate), 'code effects require the verified owner access state');
assert(component.includes("authFetch('/api/admin/apocv4/code'"), 'owner Code mode invokes the governed code BFF');
assert(component.includes('allowed_paths: allowedPaths'), 'exact allowed paths cross the governed boundary');
assert(component.includes('confirm_apply: true'), 'the browser sends explicit one-run confirmation');
assert(component.includes('!codeConfirmed'), 'the run is denied until the owner confirms it');
assert(component.includes('No automatic retry.'), 'the effect surface discloses its no-retry contract');
assert(effectBranch >= 0 && responseOnlyBranch > effectBranch, 'non-effect turns retain the response-only chat path');

assert(component.includes("authFetch('/api/admin/apocv4/code/rollback'"), 'promoted changes expose the governed rollback BFF');
assert(component.includes('confirm_rollback: true'), 'rollback requires an explicit owner action');
assert(component.includes("message.codeEffect.state === 'PROMOTED'"), 'rollback is offered only for a promoted receipt');
assert(component.includes("runtime?.state !== 'ROLLED_BACK'"), 'rollback response is fail-closed on terminal state');

assert(codeRoute.includes('requireApocryphaOwner(req, res)'), 'code BFF independently re-verifies owner authorization');
assert(codeRoute.includes('body.confirm_apply !== true'), 'code BFF rejects absent confirmation');
assert(codeRoute.indexOf('requireApocryphaOwner(req, res)') < codeRoute.indexOf('submitRuntimeCode({'), 'authorization precedes the code effect');
assert(rollbackRoute.includes('requireApocryphaOwner(req, res)'), 'rollback BFF independently re-verifies owner authorization');
assert(rollbackRoute.includes('body.confirm_rollback !== true'), 'rollback BFF rejects absent confirmation');
assert(rollbackRoute.indexOf('requireApocryphaOwner(req, res)') < rollbackRoute.indexOf('submitRuntimeRollback('), 'authorization precedes rollback');

assert(style.includes('.codeScope'), 'owner code scope is visibly composed');
assert(style.includes('.codeReceipt'), 'code effect receipts are visibly composed');
assert(style.includes('.rollbackButton:focus-visible'), 'rollback preserves a visible keyboard focus state');

console.log('public-apocrypha-code-mode.test : OK');

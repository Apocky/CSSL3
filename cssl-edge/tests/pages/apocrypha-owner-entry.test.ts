import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';
import * as owner from '../../lib/brain/owner';
import * as ownerRuntime from '../../lib/mobile/owner-runtime';

// §T execute actual page loader + authorization; omit React rendering only.
function pageLoader(path: string) {
  const output = ts.transpileModule(readFileSync(path, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX },
  }).outputText;
  const exports: Record<string, any> = {};
  runInNewContext(output, { exports, require(name: string) {
    if (name === '@/lib/brain/owner') return owner;
    if (name === '@/lib/mobile/owner-runtime') return ownerRuntime;
    return {};
  } });
  return exports.getServerSideProps;
}

async function main() {
  const keys = ['NODE_ENV', 'LAZARUS_TEST_AUTH_BYPASS', 'APOCKY_ADMIN_EMAILS',
    'APOCV4_RUNTIME_TRANSPORT', 'APOCRYPHA_BRIDGE_OWNER_USER_ID', 'APOCV4_MOBILE_OWNER_BRIDGE'];
  const before = Object.fromEntries(keys.map(key => [key, process.env[key]]));
  const load = pageLoader('pages/apocrypha.tsx');
  const legacy = pageLoader('pages/brain.tsx');
  const res = { setHeader() {} };
  const request = (email?: string) => ({ req: { headers: email ? { 'x-apocky-test-admin-email': email } : {} }, res });
  try {
    Object.assign(process.env, { NODE_ENV: 'test' });
    process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
    process.env.APOCKY_ADMIN_EMAILS = 'owner@example.com,operator@example.com';
    process.env.APOCV4_RUNTIME_TRANSPORT = 'outbound-bridge';
    process.env.APOCV4_MOBILE_OWNER_BRIDGE = '1';
    process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = 'test-admin';
    assert.equal((await load(request('owner@example.com'))).props.ownerConversation, true);
    assert.equal((await legacy(request('owner@example.com'))).props.serverAccess, 'owner');
    assert.equal((await load(request('member@example.com'))).props.ownerConversation, false);
    assert.equal((await load(request())).props.ownerConversation, false);
    assert.equal((await legacy(request())).redirect.destination, '/login?next=%2Fbrain');
    process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = 'different-account';
    assert.equal((await load(request('operator@example.com'))).props.ownerConversation, false,
      'an allowlisted operator cannot open another account encrypted conversation');
    assert.equal((await legacy(request('operator@example.com'))).props.serverAccess, 'forbidden');
    process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = 'test-admin';
    process.env.APOCV4_MOBILE_OWNER_BRIDGE = '0';
    assert.equal((await load(request('owner@example.com'))).props.ownerConversation, false,
      'disabled owner account route must remain disabled');
    console.log('apocrypha-owner-entry: 8 authorization and legacy-route checks passed');
  } finally {
    for (const key of keys) {
      if (before[key] === undefined) delete process.env[key];
      else process.env[key] = before[key];
    }
  }
}
void main().catch(error => { console.error(error); process.exitCode = 1; });

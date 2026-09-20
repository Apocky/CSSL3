// /apocrypha has no server-side entry check any more, and this test now guards that.
//
// WHAT IT USED TO GUARD: eight authorization cases over the page's getServerSideProps -- owner,
// member, anonymous, operator, and the bridge/transport flags -- each asserting which reader got
// `ownerConversation: true`. That machinery decided WHO you were before deciding WHETHER you
// could speak, and it ran on every single request, which is also what made the page uncacheable.
//
// 2026-09-20, Apocky: "The entire flow is too complicated for now just exclude sign-in."
//
// The room is the guest lane, always. So getServerSideProps is gone, the owner resolution is
// gone, and the three-way lane choice is gone. The old assertions were not weakened to make them
// pass -- they described behaviour that was removed on purpose, and a test still asserting a
// retired contract is the loudest kind of stale. They are replaced by assertions that FAIL if any
// of it comes back by accident.
//
// The owner rail itself was not deleted from the product. It lives on /admin/apocrypha, and
// tests/pages/admin-chat.test.ts still holds it to ownerLane(authFetch).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';

const source = readFileSync('pages/apocrypha.tsx', 'utf8');

// Execute the real module, exactly as the old loader did, so this checks the SHIPPED page rather
// than a regex over its text.
function pageExports(path: string): Record<string, unknown> {
  const output = ts.transpileModule(readFileSync(path, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX },
  }).outputText;
  const exports: Record<string, unknown> = {};
  runInNewContext(output, {
    exports,
    require(name: string) {
      // Deliberately a bare stub. If the page calls anything at IMPORT time this throws, which is
      // the point: the first version of the simplified page built its lane at module scope and
      // died here with "guestLane is not a function" before any mock could be installed.
      if (name === 'react') return { useMemo: (fn: () => unknown) => fn() };
      return {};
    },
  });
  return exports;
}

const exported = pageExports('pages/apocrypha.tsx');

assert.equal(exported.getServerSideProps, undefined,
  'the room must have NO server-side entry check; that check is what made it uncacheable');
assert.equal(typeof exported.default, 'function', 'the page still exports a component');

assert.doesNotMatch(source, /requireBrainOwner/,
  'no owner resolution on the public room');
assert.doesNotMatch(source, /ownerLane|memberLane/,
  'no entitlement branch on the public room -- one lane, no account');
assert.doesNotMatch(source, /useSiteSession/,
  'no session hook, so there is nothing left to stall on');
assert.doesNotMatch(source, /\/login\?next|\/register\?next/,
  'the public room offers no sign-in and no registration');
assert.doesNotMatch(source, /DEADLINE|setTimeout/,
  'the 4-second account deadline and its sign-in wall are gone');
assert.match(source, /guestLane\(\)/, 'the room runs on the guest lane');

console.log('apocrypha-owner-entry: 8 checks passed — the room is open, with no entry check');

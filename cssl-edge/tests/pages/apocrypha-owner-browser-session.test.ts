import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';

export async function main(root: string): Promise<void> {
  const compile = (source: string) => ts.transpileModule(source, { compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText;
  let account: Record<string, unknown> = { user: { id: 'owner' }, owner_conversation: true };
  let authorized = true;
  const sessionExports: Record<string, any> = {};
  runInNewContext(compile(readFileSync(join(root, 'components/hub/SiteSession.tsx'), 'utf8')
    + '\nexport { resolveSiteAccess };'), { exports: sessionExports, require(name: string) {
      if (name === 'react') return { createContext: () => ({}) };
      if (name.endsWith('/auth')) return { getAuthClient: () => ({ auth: { getSession: async () => ({ data: { session: {} } }) } }) };
      if (name.endsWith('/browser-auth')) return { authFetch: async (url: string) => ({ ok: true,
        json: async () => url === '/api/auth/me' ? account : { authorized } }) };
      return {};
    } });
  let session = await sessionExports.resolveSiteAccess();
  assert.equal(session.ownerConversation, true, 'browser bearer session admits the server-verified owner');
  account = { user: { id: 'operator' }, owner_conversation: false };
  session = await sessionExports.resolveSiteAccess();
  assert.equal(session.ownerConversation, false, 'admin access alone does not admit another owner conversation');
  account = { user: null, owner_conversation: true };
  assert.equal((await sessionExports.resolveSiteAccess()).ownerConversation, false, 'capability requires authenticated identity');
  account = { user: { id: 'owner' }, owner_conversation: true }; authorized = false;
  assert.notEqual((await sessionExports.resolveSiteAccess()).ownerConversation, true, 'failed admin admission stays closed');
  const pageExports: Record<string, any> = {};
  const jsx = (type: unknown, props: unknown) => ({ type, props });
  runInNewContext(compile(readFileSync(join(root, 'pages/apocrypha.tsx'), 'utf8')), {
    exports: pageExports, require(name: string) {
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' };
      if (name === '@/components/hub/SiteSession') return { useSiteSession: () => session };
      if (name === '@/components/brain/BrainExperience') return { default: 'OwnerConversation' };
      if (name === '@/components/apocrypha/AccountChat') return { default: 'AccountConversation' };
      return {};
    } });
  const surface = (ssr: boolean) => pageExports.default({ ownerConversation: ssr }).props.children[1].type;
  session = { access: 'owner', ownerConversation: true };
  assert.equal(surface(false), 'OwnerConversation', 'owner signed in after SSR gets the canonical conversation');
  session = { access: 'owner', ownerConversation: false };
  assert.equal(surface(false), 'AccountConversation', 'operator cannot select the owner surface');
  session = { access: 'signed-out', ownerConversation: false };
  assert.equal(surface(true), 'AccountConversation', 'stale SSR admission does not outlive sign out');
  session = { access: 'checking', ownerConversation: false };
  assert.equal(surface(true), 'OwnerConversation', 'SSR owner may show verification while browser session loads');
}

// Layer 0/1 — the workspace boundary, tested on two axes with negative controls.
//
// Axis A : the SHAPE of the path (traversal, absolute escape, prefix collision, symlink, case,
//          separator style, secret-file name).
// Axis B : the INTENT (read / write) crossed with the root's writability.
//
// A confinement test that only ever asserts "denied" passes just as well when the implementation
// denies everything, which would be a broken agent rather than a safe one. So every run also
// asserts a MUST-ALLOW set, and fails if that set is empty or if any member of it is refused.

import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

import { Workspace, WorkspaceError } from '../../scripts/apocrypha-work/workspace';

const run = promisify(execFile);

/**
 * Link `link` at `target`, by whatever mechanism this host allows.
 *
 * A real symlink needs Developer Mode or elevation on Windows. A directory junction does not, and
 * realpath resolves it the same way, so it exercises the same escape. Falling back keeps the
 * control live on an ordinary account instead of reporting it as untested.
 */
async function linkDir(target: string, link: string): Promise<boolean> {
  try {
    await symlink(target, link, 'dir');
    return true;
  } catch {
    try {
      await run('cmd', ['/c', 'mklink', '/J', link, target], { windowsHide: true });
      return true;
    } catch {
      return false;
    }
  }
}


function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

interface Case {
  readonly name: string;
  readonly path: (root: string, outside: string) => string;
  readonly intent: 'read' | 'write';
  readonly root?: 'rw' | 'ro';
}

const MUST_DENY: Case[] = [
  { name: 'parent traversal', path: (r) => join(r, '..', 'escaped.txt'), intent: 'read' },
  { name: 'deep traversal', path: (r) => join(r, 'a', '..', '..', '..', 'escaped.txt'), intent: 'read' },
  { name: 'absolute outside', path: (_r, o) => join(o, 'escaped.txt'), intent: 'read' },
  { name: 'prefix collision sibling', path: (r) => `${r}-evil\\note.txt`, intent: 'read' },
  { name: 'symlink out of root', path: (r) => join(r, 'link-out', 'secret.txt'), intent: 'read' },
  { name: 'write into read-only root', path: (r) => join(r, 'nested', 'new.txt'), intent: 'write', root: 'ro' },
  { name: 'dotenv', path: (r) => join(r, '.env'), intent: 'read' },
  { name: 'dotenv.local', path: (r) => join(r, '.env.local'), intent: 'read' },
  { name: 'private key', path: (r) => join(r, 'id_rsa'), intent: 'read' },
  { name: 'pem file', path: (r) => join(r, 'server.pem'), intent: 'read' },
  { name: 'npmrc', path: (r) => join(r, '.npmrc'), intent: 'read' },
  { name: 'service account json', path: (r) => join(r, 'service_account_prod.json'), intent: 'read' },
];

const MUST_ALLOW: Case[] = [
  { name: 'file at root', path: (r) => join(r, 'notes.txt'), intent: 'read' },
  { name: 'nested file', path: (r) => join(r, 'nested', 'deep.ts'), intent: 'read' },
  { name: 'forward slashes', path: (r) => `${r.replace(/\\/g, '/')}/nested/deep.ts`, intent: 'read' },
  { name: 'write at root', path: (r) => join(r, 'created.txt'), intent: 'write' },
  { name: 'write to a path that does not exist yet', path: (r) => join(r, 'nested', 'brand-new.ts'), intent: 'write' },
  { name: 'read inside read-only root', path: (r) => join(r, 'notes.txt'), intent: 'read', root: 'ro' },
  { name: 'symlink that stays inside', path: (r) => join(r, 'link-in', 'deep.ts'), intent: 'read' },
  { name: 'dotenv example is a template', path: (r) => join(r, '.env.example'), intent: 'read' },
  { name: 'dotenv sample is a template', path: (r) => join(r, '.env.sample'), intent: 'read' },
  { name: 'source file merely named key', path: (r) => join(r, 'nested', 'keyboard.ts'), intent: 'read' },
  { name: 'relative path resolves against primary root', path: () => 'notes.txt', intent: 'read' },
];

async function main(): Promise<void> {
  const base = await mkdtemp(join(tmpdir(), 'work-confine-'));
  const rw = join(base, 'Workspace');
  const ro = join(base, 'ReadOnly');
  const outside = join(base, 'outside');
  const evil = `${rw}-evil`;

  for (const dir of [join(rw, 'nested'), join(ro, 'nested'), outside, evil]) await mkdir(dir, { recursive: true });
  await writeFile(join(rw, 'notes.txt'), 'hello', 'utf8');
  await writeFile(join(ro, 'notes.txt'), 'hello', 'utf8');
  await writeFile(join(rw, '.env'), 'SECRET=value', 'utf8');
  await writeFile(join(rw, '.env.local'), 'SECRET=value', 'utf8');
  await writeFile(join(rw, '.env.example'), 'SECRET=', 'utf8');
  await writeFile(join(rw, '.env.sample'), 'SECRET=', 'utf8');
  await writeFile(join(rw, '.npmrc'), 'token', 'utf8');
  await writeFile(join(rw, 'id_rsa'), 'key', 'utf8');
  await writeFile(join(rw, 'server.pem'), 'key', 'utf8');
  await writeFile(join(rw, 'service_account_prod.json'), '{}', 'utf8');
  await writeFile(join(rw, 'nested', 'deep.ts'), 'export const x = 1;', 'utf8');
  await writeFile(join(rw, 'nested', 'keyboard.ts'), 'export const k = 1;', 'utf8');
  await writeFile(join(outside, 'secret.txt'), 'out of bounds', 'utf8');
  await writeFile(join(evil, 'note.txt'), 'sibling', 'utf8');

  // Record it rather than skipping silently: an untested control has to be reported as untested.
  const symlinksUsable = (await linkDir(outside, join(rw, 'link-out')))
    && (await linkDir(join(rw, 'nested'), join(rw, 'link-in')));

  const workspace = await Workspace.open([
    { label: 'rw', path: rw, writable: true },
    { label: 'ro', path: ro, writable: false },
  ]);

  const rootFor = (which: 'rw' | 'ro' | undefined): string => (which === 'ro' ? ro : rw);
  const skipped: string[] = [];
  let denied = 0;
  let allowed = 0;

  for (const testCase of MUST_DENY) {
    if (testCase.name.includes('symlink') && !symlinksUsable) { skipped.push(testCase.name); continue; }
    const target = testCase.path(rootFor(testCase.root), outside);
    let refused = false;
    try {
      await workspace.resolveExisting(target, testCase.intent);
    } catch (error) {
      refused = error instanceof WorkspaceError;
      assert(refused, `${testCase.name}: refused with the wrong error type (${String(error)})`);
    }
    assert(refused, `MUST-DENY "${testCase.name}" was ALLOWED through: ${target}`);
    denied += 1;
  }

  for (const testCase of MUST_ALLOW) {
    if (testCase.name.includes('symlink') && !symlinksUsable) { skipped.push(testCase.name); continue; }
    const target = testCase.path(rootFor(testCase.root), outside);
    try {
      const resolved = await workspace.resolveExisting(target, testCase.intent);
      assert(resolved.path.length > 0, `${testCase.name}: resolved to an empty path`);
    } catch (error) {
      throw new Error(`MUST-ALLOW "${testCase.name}" was REFUSED: ${target} :: ${String(error)}`);
    }
    allowed += 1;
  }

  // Vacuity guards. Without these the suite would still pass if someone made every path deny.
  assert(denied >= 8, `deny coverage collapsed to ${denied} cases`);
  assert(allowed >= 8, `allow coverage collapsed to ${allowed} cases — the boundary may be denying everything`);

  // An empty root set must be rejected outright rather than defaulting to "anywhere".
  let emptyRejected = false;
  try {
    await Workspace.open([]);
  } catch (error) {
    emptyRejected = error instanceof WorkspaceError;
  }
  assert(emptyRejected, 'Workspace.open([]) did not reject an empty root set');

  // describe() must not leak the absolute path back into transcripts.
  const label = workspace.describe(join(rw, 'nested', 'deep.ts'));
  assert(label === 'rw/nested/deep.ts', `describe produced "${label}"`);

  await rm(base, { recursive: true, force: true });
  console.log(`confinement: ${denied} denied, ${allowed} allowed${skipped.length ? `, SKIPPED (symlinks unavailable): ${skipped.join(', ')}` : ''}`);
  if (skipped.length > 0) console.log('confinement: symlink escape is therefore UNTESTED on this host');
}

main().then(() => console.log('work/confinement OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

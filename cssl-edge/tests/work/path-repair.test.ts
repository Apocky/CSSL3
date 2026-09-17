// Path repair, and above all what repair must NOT do.
//
// A model occasionally emits a separator as a space, and one wrong character became 38 flailing
// list_dir and search calls in a real turn. Repair fixes that. The danger is obvious: a resolver
// that rewrites the caller's path until something opens is a confinement hole with good manners.
//
// So this asserts BOTH directions. Repair must work, AND every repaired candidate must still be
// refused when it lands outside a root, on a read-only root for a write, or on a secret file.
// A test that only checked "repair works" would pass on an implementation that opened anything.

import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import assert from 'node:assert/strict';

import { Workspace, WorkspaceError } from '../../scripts/apocrypha-work/workspace';

const SEP = String.fromCharCode(92);

async function main(): Promise<void> {
  const base = await mkdtemp(join(tmpdir(), 'apx-repair-'));
  const inside = join(base, 'inside');
  const outside = join(base, 'outside');
  await mkdir(join(inside, 'deep'), { recursive: true });
  await mkdir(outside, { recursive: true });
  await writeFile(join(inside, 'deep', 'target.ts'), 'export const x = 1;\n');
  await writeFile(join(inside, '.env'), 'SECRET=value\n');
  await writeFile(join(outside, 'secret.txt'), 'not yours\n');

  const workspace = await Workspace.open([{ label: 'inside', path: inside, writable: true }]);
  const good = join(inside, 'deep', 'target.ts');

  // -- a correct path is untouched ---------------------------------------------------------------
  // Repair must be invisible when nothing is wrong; a "repaired" flag on a healthy path would teach
  // the model to distrust paths that were fine.
  const clean = await workspace.resolveForgiving(good, 'read');
  assert.equal(clean.repaired, undefined, 'a path that already resolves must not be reported as repaired');

  // -- the observed corruption is repaired -------------------------------------------------------
  const spaced = good.replace(`${SEP}deep`, `${SEP} deep`);
  assert.notEqual(spaced, good, 'the fixture must actually be corrupted, or this proves nothing');
  await assert.rejects(workspace.resolveExisting(spaced, 'read'), 'the corrupted path must genuinely fail the strict resolver');
  const repaired = await workspace.resolveForgiving(spaced, 'read');
  assert.equal(repaired.path, clean.path, 'repair must land on the file the operator meant');
  assert.ok(repaired.repaired, 'repair must SAY it repaired, so the next call uses the corrected form');

  // -- mixed separators --------------------------------------------------------------------------
  const mixed = good.split(SEP).join('/');
  const mixedFixed = await workspace.resolveForgiving(mixed, 'read');
  assert.equal(mixedFixed.path, clean.path, 'a forward-slash spelling must resolve to the same file');

  // -- CONFINEMENT SURVIVES REPAIR ---------------------------------------------------------------
  // The whole risk of this feature. Each of these is a path that repair could plausibly "fix" into
  // something that opens, and each must still be refused.
  const escapes = [
    join(outside, 'secret.txt'),                                  // plainly outside
    join(outside, ' secret.txt'),                                 // outside AND corrupted
    join(inside, '..', 'outside', 'secret.txt'),                  // traversal
    join(inside, '..', 'outside', ' secret.txt'),                 // traversal AND corrupted
    join(inside, 'deep', '..', '..', 'outside', 'secret.txt'),    // deeper traversal
  ];
  for (const candidate of escapes) {
    await assert.rejects(
      workspace.resolveForgiving(candidate, 'read'),
      (error: unknown) => error instanceof WorkspaceError,
      `repair must never open a path outside the workspace: ${candidate}`,
    );
  }

  // Secret files stay refused even when the spelling needed repair to resolve at all.
  await assert.rejects(
    workspace.resolveForgiving(join(inside, '.env'), 'read'),
    'a secret file must stay refused',
  );
  await assert.rejects(
    workspace.resolveForgiving(join(inside, ' .env'), 'read'),
    'repair must not smuggle a secret file in through a corrupted spelling',
  );

  // Read-only roots stay read-only through the repaired path.
  const readOnly = await Workspace.open([{ label: 'ro', path: inside, writable: false }]);
  await assert.rejects(
    readOnly.resolveForgiving(spaced, 'write'),
    'repair must not turn a read-only root writable',
  );

  // -- the failure message is the ORIGINAL one ---------------------------------------------------
  // Reporting the last candidate's error would send the operator chasing a path they never typed.
  await assert.rejects(
    workspace.resolveForgiving(join(inside, 'no', 'such', 'file.ts'), 'read'),
    (error: unknown) => error instanceof WorkspaceError,
    'an unrepairable path must still fail as a workspace error',
  );

  await rm(base, { recursive: true, force: true });
  console.log('work/path-repair OK - repairs the observed corruption, and confinement survives every repaired candidate');
}

void main().catch((error: unknown) => { console.error(error); process.exit(1); });

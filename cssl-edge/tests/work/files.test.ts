import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rename, rm, stat, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import test, { type TestContext } from 'node:test';
import { promisify } from 'node:util';

import { FILE_TOOLS, runFileTool, type ToolContext } from '../../scripts/apocrypha-work/tools/files';
import { Workspace } from '../../scripts/apocrypha-work/workspace';

async function fixture(context: TestContext): Promise<{ root: string; outside: string; ctx: ToolContext }> {
  const base = await mkdtemp(join(tmpdir(), 'work-files-'));
  context.after(() => rm(base, { recursive: true, force: true }));
  const root = join(base, 'workspace');
  const outside = join(base, 'outside');
  await mkdir(root);
  await mkdir(outside);
  await mkdir(join(root, 'existing'));
  await writeFile(join(root, 'foreign.txt'), 'owner changes stay here\r\n');
  await writeFile(join(outside, 'foreign.txt'), 'outside stays unchanged\n');
  const workspace = await Workspace.open([{ label: 'fixture', path: root, writable: true }]);
  return { root, outside, ctx: { workspace, signal: new AbortController().signal } };
}

test('behavioral/LOCKING C03 large ranges: null=old size guard; floor=one numbered UTF-8 line', async (context) => {
  const { root, ctx } = await fixture(context);
  const lines = Array.from({ length: 14_000 }, (_, index) => `${index + 1}: ${'x'.repeat(48)} caf\u00e9 \ud83d\ude80\r`);
  const text = lines.join('\n');
  await writeFile(join(root, 'large.txt'), text);
  assert.ok(Buffer.byteLength(text) > 512 * 1024);
  const result = await runFileTool('read_file', { path: 'large.txt', start_line: 9000, end_line: 9002 }, ctx);
  assert.equal(result.content, lines.slice(8999, 9002).map((line, index) => `${9000 + index}\t${line}`).join('\n'));
  assert.equal(result.path, join(root, 'large.txt'));
  const prefix = await runFileTool('read_file', { path: 'large.txt', start_line: 1, end_line: 2 }, ctx);
  assert.equal(prefix.content, `1\t${lines[0]}\n2\t${lines[1]}`);
  assert.ok(prefix.bytesRead! > 0 && prefix.bytesRead! <= 64 * 1024, 'a short prefix must not load the entire large file');
  await assert.rejects(runFileTool('read_file', { path: 'large.txt' }, ctx), /range|end_line/i);
  await assert.rejects(runFileTool('read_file', { path: 'large.txt', start_line: 2 }, ctx), /range|end_line/i);
});

test('behavioral/LOCKING C03 range validation: null=coerced or reversed range; floor=one invalid integer', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'short.txt'), 'first\nsecond\nthird');
  for (const value of [0, -1, 1.5, '2', true, null, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
    await assert.rejects(runFileTool('read_file', { path: 'short.txt', start_line: value, end_line: 3 }, ctx), /integer|line|range/i);
    await assert.rejects(runFileTool('read_file', { path: 'short.txt', end_line: value }, ctx), /integer|line|range/i);
  }
  await assert.rejects(runFileTool('read_file', { path: 'short.txt', start_line: 3, end_line: 2 }, ctx), /range|end_line/i);
  await assert.rejects(runFileTool('read_file', { path: 'short.txt', start_line: 9, end_line: 10 }, ctx), /line|range|beyond/i);
  await assert.rejects(runFileTool('read_file', { path: '' }, ctx), /path/i);
  assert.equal((await runFileTool('read_file', { path: 'short.txt', start_line: 2, end_line: 2 }, ctx)).content, '2\tsecond');
});

test('behavioral/LOCKING C03 output cap: null=uncapped numbered response; floor=one excess byte', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'output.txt'), Array.from({ length: 900 }, () => '\u00e9'.repeat(120)).join('\n'));
  const result = await runFileTool('read_file', { path: 'output.txt', start_line: 1, end_line: 900 }, ctx);
  assert.ok(Buffer.byteLength(result.content) <= 64 * 1024, 'numbering and notices count toward the response cap');
  assert.equal(result.truncated, true);
  assert.ok(result.range!.nextLine! > 1 && result.range!.nextLine! < 900);
  assert.ok(!result.content.includes('\ufffd'), 'the byte cap must not split UTF-8');
  await writeFile(join(root, 'invalid.txt'), Buffer.from([0x61, 0xff, 0x62]));
  await assert.rejects(runFileTool('read_file', { path: 'invalid.txt' }, ctx), /UTF-8|encoding/i);
});

test('behavioral/LOCKING C04 create and edit: null=no disk effect or substitution expansion; floor=one byte', async (context) => {
  const { root, ctx } = await fixture(context);
  const text = '\ufeffconst price = "before";\r\n// caf\u00e9 \ud83d\ude80\r\n';
  const created = await runFileTool('write_file', { path: 'existing/new.ts', content: text }, ctx);
  assert.equal(await readFile(join(root, 'existing', 'new.ts'), 'utf8'), text);
  assert.equal(created.path, join(root, 'existing', 'new.ts'));
  assert.ok(created.diff?.patch.includes('before'));
  const edited = await runFileTool('edit_file', { path: 'existing/new.ts', old_text: 'before', new_text: '$& $1 after' }, ctx);
  assert.equal(await readFile(join(root, 'existing', 'new.ts'), 'utf8'), '\ufeffconst price = "$& $1 after";\r\n// caf\u00e9 \ud83d\ude80\r\n');
  assert.ok(edited.diff?.patch.includes('$& $1 after'));
  assert.equal(await readFile(join(root, 'foreign.txt'), 'utf8'), 'owner changes stay here\r\n');
  await assert.rejects(runFileTool('write_file', { path: 'missing/child.ts', content: 'no' }, ctx), /parent|exist/i);
  await assert.rejects(runFileTool('write_file', { path: 'existing/too-big.ts', content: 'x'.repeat(2 * 1024 * 1024 + 1) }, ctx), /exceeds|limit/i);
  await assert.rejects(stat(join(root, 'existing', 'too-big.ts')));
});

test('behavioral/LOCKING C04 mkdir rename delete: null=missing operation; floor=one real file effect', async (context) => {
  const { root, ctx } = await fixture(context);
  for (const name of ['make_dir', 'rename_file', 'delete_file']) assert.equal(FILE_TOOLS.find((tool) => tool.name === name)?.risk, 'write');
  const made = await runFileTool('make_dir', { path: 'generated' }, ctx);
  assert.ok((await stat(join(root, 'generated'))).isDirectory());
  assert.equal(made.path, join(root, 'generated'));
  await runFileTool('write_file', { path: 'generated/original.txt', content: 'exact\r\n\u00e9\n' }, ctx);
  await runFileTool('rename_file', { path: 'generated/original.txt', new_path: 'existing/renamed.txt' }, ctx);
  await assert.rejects(stat(join(root, 'generated', 'original.txt')));
  assert.equal(await readFile(join(root, 'existing', 'renamed.txt'), 'utf8'), 'exact\r\n\u00e9\n');
  await runFileTool('delete_file', { path: 'existing/renamed.txt' }, ctx);
  await assert.rejects(stat(join(root, 'existing', 'renamed.txt')));
  await assert.rejects(runFileTool('delete_file', { path: 'generated' }, ctx), /file|directory/i);
  assert.ok((await stat(join(root, 'generated'))).isDirectory());
  await assert.rejects(runFileTool('make_dir', { path: 'absent/child' }, ctx), /parent|exist/i);
  assert.equal(await readFile(join(root, 'foreign.txt'), 'utf8'), 'owner changes stay here\r\n');
});

test('behavioral/LOCKING C04 rename collision: null=overwritten foreign target; floor=one preserved byte', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'source.txt'), 'source\n');
  await assert.rejects(runFileTool('rename_file', { path: 'source.txt', new_path: 'foreign.txt' }, ctx), /exist|overwrite/i);
  assert.equal(await readFile(join(root, 'source.txt'), 'utf8'), 'source\n');
  assert.equal(await readFile(join(root, 'foreign.txt'), 'utf8'), 'owner changes stay here\r\n');
});

test('behavioral/LOCKING C04 confinement: null=escaped or secret write; floor=one unauthorized effect', async (context) => {
  const { root, outside, ctx } = await fixture(context);
  await writeFile(join(root, 'source.txt'), 'source');
  await writeFile(join(root, '.env'), 'FIXTURE_ONLY=unchanged');
  await symlink(outside, join(root, 'link-out'), process.platform === 'win32' ? 'junction' : 'dir');
  await symlink(join(root, 'existing'), join(root, 'link-in'), process.platform === 'win32' ? 'junction' : 'dir');
  const readOnly = await Workspace.open([{ label: 'ro', path: root, writable: false }]);
  const mutations = [
    { name: 'write_file', args: { content: 'changed' } },
    { name: 'edit_file', args: { old_text: 'owner', new_text: 'changed' } },
    { name: 'make_dir', args: {} },
    { name: 'delete_file', args: {} },
    { name: 'rename_file', args: { new_path: 'renamed.txt' } },
  ];
  for (const mutation of mutations) {
    for (const path of ['../outside/foreign.txt', 'link-out/foreign.txt', '.env']) {
      await assert.rejects(runFileTool(mutation.name, { ...mutation.args, path }, ctx), /workspace|outside|credentials|symlink|junction/i);
    }
    await assert.rejects(runFileTool(mutation.name, { ...mutation.args, path: 'foreign.txt' }, { ...ctx, workspace: readOnly }), /read-only/i);
  }
  await assert.rejects(runFileTool('write_file', { path: 'link-in/new.txt', content: 'no' }, ctx), /symlink|junction/i);
  await assert.rejects(runFileTool('rename_file', { path: 'source.txt', new_path: 'link-in/new.txt' }, ctx), /symlink|junction/i);
  await assert.rejects(runFileTool('rename_file', { path: 'source.txt', new_path: '.env' }, ctx), /credentials/i);
  await assert.rejects(runFileTool('read_file', { path: 'link-out/foreign.txt' }, ctx), /outside|workspace/i);
  assert.equal(await readFile(join(root, '.env'), 'utf8'), 'FIXTURE_ONLY=unchanged');
  assert.equal(await readFile(join(outside, 'foreign.txt'), 'utf8'), 'outside stays unchanged\n');
  await assert.rejects(stat(join(root, 'existing', 'new.txt')));
});

test('behavioral/LOCKING C04 cancellation: null=effect after awaited resolution; floor=one changed byte', async (context) => {
  const { root, ctx } = await fixture(context);
  const mutations = [
    { name: 'write_file', args: { path: 'foreign.txt', content: 'changed' } },
    { name: 'edit_file', args: { path: 'foreign.txt', old_text: 'owner', new_text: 'changed' } },
    { name: 'make_dir', args: { path: 'cancelled-dir' } },
    { name: 'rename_file', args: { path: 'foreign.txt', new_path: 'cancelled.txt' } },
    { name: 'delete_file', args: { path: 'foreign.txt' } },
  ];
  for (const mutation of mutations) {
    for (const afterResolve of [false, true]) {
      const controller = new AbortController();
      const workspace = await Workspace.open([{ label: 'fixture', path: root, writable: true }]);
      const original = workspace.resolveExisting.bind(workspace);
      workspace.resolveExisting = async (...args) => {
        const resolved = await original(...args);
        if (afterResolve) controller.abort(new Error('cancelled after precondition'));
        return resolved;
      };
      if (!afterResolve) controller.abort(new Error('cancelled before effect'));
      await assert.rejects(runFileTool(mutation.name, mutation.args, { ...ctx, workspace, signal: controller.signal }), /cancel|abort/i);
      assert.equal(await readFile(join(root, 'foreign.txt'), 'utf8'), 'owner changes stay here\r\n');
      await assert.rejects(stat(join(root, 'cancelled-dir')));
      await assert.rejects(stat(join(root, 'cancelled.txt')));
    }
  }
});

test('behavioral/LOCKING C03 path repair: null=silent repaired result; floor=one missing path notice', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'existing', 'target.txt'), 'needle');
  for (const name of ['read_file', 'list_dir', 'search']) {
    const path = name === 'read_file' ? ` existing${sep}target.txt` : ' existing';
    const result = await runFileTool(name, { path, pattern: 'needle' }, ctx);
    assert.match(result.content, /path corrected to:/i);
    assert.ok(result.path?.startsWith(root));
  }
});

test('behavioral/LOCKING C03 search and listing pages: null=600-entry or depth loss; floor=one missing match', async (context) => {
  const { root, ctx } = await fixture(context);
  await mkdir(join(root, 'many'));
  for (let index = 0; index < 625; index += 1) await writeFile(join(root, 'many', `${String(index).padStart(4, '0')}  note.txt`), `needle ${index}\n`);
  const deep = join(root, ...Array.from({ length: 10 }, (_, index) => `depth${index}`));
  await mkdir(deep, { recursive: true });
  await writeFile(join(deep, 'deep.txt'), 'needle deep\n');
  await mkdir(join(root, 'node_modules'));
  await writeFile(join(root, 'node_modules', 'ignored.txt'), 'needle ignored\n');
  await writeFile(join(root, '.env'), 'needle secret\n');
  const found = new Set<string>();
  let cursor: string | undefined;
  for (let page = 0; page < 40; page += 1) {
    const result = await runFileTool('search', { path: '.', pattern: 'needle', extensions: 'txt', max_results: 40, cursor }, ctx);
    assert.ok(Array.isArray(result.matches));
    for (const match of result.matches!) {
      assert.ok(!found.has(`${match.path}:${match.line}`), 'cursor must not replay an earlier match');
      found.add(`${match.path}:${match.line}`);
    }
    assert.ok(Buffer.byteLength(result.content) <= 64 * 1024);
    if (result.complete) { assert.equal(result.nextCursor, undefined); break; }
    assert.equal(result.truncated, true);
    assert.ok(result.nextCursor, 'an incomplete page must give a resumable cursor');
    assert.notEqual(result.nextCursor, cursor, 'cursor must make progress');
    cursor = result.nextCursor;
  }
  assert.equal(found.size, 626);
  assert.ok(found.has(`${join(deep, 'deep.txt')}:1`));
  assert.ok(found.has(`${join(root, 'many', '0624  note.txt')}:1`));
  const listed = new Set<string>();
  cursor = undefined;
  for (let page = 0; page < 20; page += 1) {
    const result = await runFileTool('list_dir', { path: 'many', max_entries: 100, cursor }, ctx);
    assert.ok(Array.isArray(result.entries));
    for (const entry of result.entries!) {
      assert.equal(entry.type, 'file');
      assert.equal(entry.size, (await stat(entry.path)).size);
      assert.ok(!listed.has(entry.path));
      listed.add(entry.path);
    }
    if (result.complete) break;
    assert.ok(result.nextCursor);
    cursor = result.nextCursor;
  }
  assert.equal(listed.size, 625);
  const empty = await runFileTool('search', { path: 'existing', pattern: 'needle' }, ctx);
  assert.equal(empty.complete, true);
  assert.deepEqual(empty.matches, []);
  assert.match(empty.content, /no matches/i);
});

test('behavioral/UNLOCKING C03 search deadline: null=catastrophic main-thread regex; floor=one missed deadline', async (context) => {
  const { root } = await fixture(context);
  await writeFile(join(root, 'existing', 'slow.txt'), 'a'.repeat(48_000) + '!\n');
  const source = `
    const { Workspace } = require('./scripts/apocrypha-work/workspace.ts');
    const { runFileTool } = require('./scripts/apocrypha-work/tools/files.ts');
    (async () => {
      const workspace = await Workspace.open([{label: 'fixture', path: ${JSON.stringify(root)}, writable: true}]);
      const result = await runFileTool('search', {path: 'existing', pattern: '(a+)+$', timeout_ms: 100}, {workspace, signal: new AbortController().signal});
      console.log(JSON.stringify(result));
    })().catch(error => { console.error(error); process.exitCode = 1; });
  `;
  const started = performance.now();
  const result = await promisify(execFile)(process.execPath, ['--import', 'tsx', '--eval', source], { timeout: 3000, windowsHide: true, maxBuffer: 128 * 1024 });
  assert.ok(performance.now() - started < 3000);
  const page = JSON.parse(result.stdout.trim());
  assert.equal(page.complete, false);
  assert.equal(page.truncated, true);
  assert.match(page.content, /deadline|time.*limit|timeout/i);
  assert.notEqual(page.content, '(no matches)');
});

test('behavioral/LOCKING C03 cursor validation: null=cursor reused for another query; floor=one omitted file', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'existing', 'first.txt'), 'needle one\nneedle two\n');
  const first = await runFileTool('search', { path: 'existing', pattern: 'needle', max_results: 1 }, ctx);
  assert.ok(first.nextCursor);
  await assert.rejects(runFileTool('search', { path: 'existing', pattern: 'other', cursor: first.nextCursor }, ctx), /cursor|query/i);
  await assert.rejects(runFileTool('search', { path: '.', pattern: 'needle', cursor: 'not-a-cursor' }, ctx), /cursor/i);
  for (const value of [0, -1, 1.5, '2', null, NaN]) {
    await assert.rejects(runFileTool('search', { path: 'existing', pattern: 'needle', max_results: value }, ctx), /integer/i);
  }
  await assert.rejects(runFileTool('search', { path: 'existing', pattern: '[' }, ctx), /regular expression|regex/i);
});

test('behavioral/LOCKING C03 bounded scan: null=full file read; floor=one byte above the scan budget', async (context) => {
  const { root, ctx } = await fixture(context);
  const text = `${'x'.repeat(1023)}\n`.repeat(10_000) + 'needle after the byte limit\n';
  await writeFile(join(root, 'existing', 'scan.txt'), text);
  await assert.rejects(runFileTool('read_file', { path: 'existing/scan.txt', start_line: 10_001, end_line: 10_001 }, ctx), /scan limit/i);
  let cursor: string | undefined;
  let totalBytes = 0;
  let found = false;
  for (let page = 0; page < 30; page += 1) {
    const result = await runFileTool('search', { path: 'existing', pattern: 'needle', cursor }, ctx);
    assert.ok(result.bytesRead! >= 0 && result.bytesRead! <= 8 * 1024 * 1024);
    totalBytes += result.bytesRead!;
    if (result.matches?.some((match) => match.line === 10_001 && match.text === 'needle after the byte limit')) found = true;
    if (result.complete) break;
    assert.notEqual(result.content, '(no matches)');
    assert.ok(result.nextCursor);
    cursor = result.nextCursor;
  }
  assert.equal(found, true);
  assert.equal(totalBytes, Buffer.byteLength(text), 'continuations must resume IO, not reread the prefix');
});

test('behavioral/LOCKING C03 oversized line gap: null=false exhaustive no-match; floor=one unsearched line', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'existing', 'long.txt'), 'x'.repeat(70 * 1024) + '\nneedle after long line\n');
  const result = await runFileTool('search', { path: 'existing', pattern: 'needle' }, ctx);
  assert.equal(result.complete, false);
  assert.equal(result.truncated, true);
  assert.equal(result.skipped?.oversized_lines, 1);
  assert.deepEqual(result.matches, [{ path: join(root, 'existing', 'long.txt'), line: 2, text: 'needle after long line' }]);
  assert.match(result.content, /incomplete|not exhaustive/i);
});

test('behavioral/LOCKING C03 cursor confinement: null=reused traversal handle after a junction swap; floor=one outside entry', async (context) => {
  const { root, outside, ctx } = await fixture(context);
  await writeFile(join(root, 'existing', 'one.txt'), 'needle one\n');
  await writeFile(join(root, 'existing', 'two.txt'), 'needle two\n');
  await writeFile(join(outside, 'one.txt'), 'outside one\n');
  await writeFile(join(outside, 'two.txt'), 'outside two\n');
  const page = await runFileTool('list_dir', { path: 'existing', max_entries: 1 }, ctx);
  assert.ok(page.nextCursor);
  await rename(join(root, 'existing'), join(root, 'moved'));
  await symlink(outside, join(root, 'existing'), process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(runFileTool('list_dir', { path: 'existing', max_entries: 1, cursor: page.nextCursor }, ctx), /outside|workspace|cursor/i);
  await rm(join(root, 'existing'));
  await symlink(join(root, 'moved'), join(root, 'existing'), process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(runFileTool('list_dir', { path: 'existing', max_entries: 1, cursor: page.nextCursor }, ctx), /cursor|changed|symlink|junction/i);
});

test('behavioral/LOCKING C04 nested authority: null=parent grant overrides child read-only root; floor=one write', async (context) => {
  const { root, ctx } = await fixture(context);
  const workspace = await Workspace.open([
    { label: 'parent', path: root, writable: true },
    { label: 'child', path: join(root, 'existing'), writable: false },
  ]);
  await assert.rejects(runFileTool('write_file', { path: 'existing/no.txt', content: 'no' }, { ...ctx, workspace }), /read-only/i);
  await assert.rejects(stat(join(root, 'existing', 'no.txt')));
  await runFileTool('write_file', { path: 'allowed.txt', content: 'allowed' }, { ...ctx, workspace });
  assert.equal(await readFile(join(root, 'allowed.txt'), 'utf8'), 'allowed');
});

test('behavioral/LOCKING C03 regex compatibility: null=reject valid whitespace regex; floor=one missing space match', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'existing', 'spaces.txt'), 'two words\ncompact');
  const spaces = await runFileTool('search', { path: 'existing', pattern: ' ' }, ctx);
  assert.deepEqual(spaces.matches, [{ path: join(root, 'existing', 'spaces.txt'), line: 1, text: 'two words' }]);
  const all = await runFileTool('search', { path: 'existing', pattern: '' }, ctx);
  assert.deepEqual(all.matches?.map((match) => match.line), [1, 2]);
});

test('behavioral/LOCKING C03 cursor ceiling: null=concurrent unbounded handle retention; floor=one cursor above 16', async (context) => {
  const { root, ctx } = await fixture(context);
  await writeFile(join(root, 'existing', 'one.txt'), 'one');
  await writeFile(join(root, 'existing', 'two.txt'), 'two');
  const pages = await Promise.allSettled(Array.from({ length: 24 }, () => runFileTool('list_dir', { path: 'existing', max_entries: 1 }, ctx)));
  let usable = 0;
  for (const page of pages) {
    if (page.status === 'rejected') { assert.match(String(page.reason), /cursor|concurrent|limit|busy/i); continue; }
    assert.ok(page.value.nextCursor);
    try {
      const remaining = await runFileTool('list_dir', { path: 'existing', max_entries: 10, cursor: page.value.nextCursor }, ctx);
      assert.equal(remaining.complete, true);
      assert.equal(remaining.entries?.length, 1);
      usable += 1;
    } catch (error) { assert.match(String(error), /cursor/i); }
  }
  assert.ok(usable >= 1, 'the cursor cap must not deny every caller');
  assert.ok(usable <= 16, `retained ${usable} usable cursors, exceeding the hard resource ceiling`);
});
import { randomUUID } from 'node:crypto';
import type { Dir, Stats } from 'node:fs';
import { link, lstat, mkdir, open, opendir, stat, unlink, type FileHandle } from 'node:fs/promises';
import { join, relative, sep } from 'node:path';
import { TextDecoder } from 'node:util';
import { Worker } from 'node:worker_threads';
import { unifiedDiff } from '../diff';
import { WorkspaceError, type Workspace } from '../workspace';
import type { ToolDefinition } from '../types';

const MAX_READ_BYTES = 512 * 1024;
const MAX_SCAN_BYTES = 8 * 1024 * 1024;
const MAX_OUTPUT_BYTES = 64 * 1024;
const READ_CHUNK_BYTES = 16 * 1024;
const MAX_WRITE_BYTES = 2 * 1024 * 1024;
const MAX_ENTRIES = 600;
const MAX_PAGE_STEPS = 2000;
const MAX_CURSORS = 16;
const CURSOR_TTL_MS = 60_000;
const MAX_SEARCH_LINE_BYTES = 64 * 1024;
const SKIP_DIRS = new Set(['.git', 'node_modules', '.next', 'target', '__pycache__', '.venv', 'dist', '.cache']);

export interface ToolContext {
  readonly workspace: Workspace;
  readonly signal: AbortSignal;
}

export interface ToolResult {
  readonly summary: string;
  readonly content: string;
  readonly diff?: { path: string; added: number; removed: number; patch: string };
  readonly path?: string;
  readonly complete?: boolean;
  readonly truncated?: boolean;
  readonly bytesRead?: number;
  readonly range?: { startLine: number; endLine: number; nextLine?: number };
  readonly nextCursor?: string;
  readonly entries?: readonly { path: string; type: 'file' | 'directory'; size?: number; skipped?: boolean }[];
  readonly matches?: readonly { path: string; line: number; text: string }[];
  readonly skipped?: Readonly<Record<string, number>>;
  readonly previousPath?: string;
  readonly diffTruncated?: boolean;
}

function integer(value: unknown, fallback: number, name: string, maximum = Number.MAX_SAFE_INTEGER): number {
  if (value === undefined) return fallback;
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1 || value > maximum) {
    throw new Error(`${name} must be an integer from 1 to ${maximum}`);
  }
  return value;
}

function stringArg(value: unknown, name: string, fallback?: string): string {
  if (value === undefined && fallback !== undefined) return fallback;
  if (typeof value !== 'string' || value.trim() === '' || value.length > 4096) {
    throw new Error(`${name} must be a non-empty string of at most 4096 characters`);
  }
  return value;
}

export const FILE_TOOLS: ToolDefinition[] = [
  {
    name: 'list_dir',
    risk: 'read',
    description: 'List workspace entries with explicit completeness and stable paths. Continue incomplete pages using cursor. Hidden entries and dependency/build directories are excluded.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Directory path, absolute or relative to the primary root.' },
        depth: { type: 'integer', description: 'Recursion depth, 1 to 4. Default 1.' },
        max_entries: { type: 'integer', minimum: 1, maximum: 600, description: 'Maximum entries per page; default 600.' },
        cursor: { type: 'string', description: 'Single-use continuation from this same query and Workspace; expires after 60 seconds and on host restart.' },
        timeout_ms: { type: 'integer', minimum: 1, maximum: 2000, description: 'Page deadline; default 1000 ms.' },
      },
      required: ['path'],
    },
  },
  {
    name: 'read_file',
    risk: 'read',
    description: 'Read a UTF-8 text file. Returns numbered lines so you can cite and edit them precisely.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string' },
        start_line: { type: 'integer', description: '1-indexed first line. Omit to read from the start.' },
        end_line: { type: 'integer', minimum: 1, description: '1-indexed last line, inclusive. Required above 512 KiB. Scans at most 8 MiB; returns at most 64 KiB.' },
      },
      required: ['path'],
    },
  },
  {
    name: 'write_file',
    risk: 'write',
    description: 'Create a file or replace its entire contents. Prefer edit_file when changing part of an existing file.',
    parameters: {
      type: 'object',
      properties: { path: { type: 'string' }, content: { type: 'string' } },
      required: ['path', 'content'],
    },
  },
  {
    name: 'edit_file',
    risk: 'write',
    description: 'Replace an exact substring in a file. old_text must appear exactly once unless replace_all is true.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string' },
        old_text: { type: 'string' },
        new_text: { type: 'string' },
        replace_all: { type: 'boolean' },
      },
      required: ['path', 'old_text', 'new_text'],
    },
  },
  {
    name: 'make_dir',
    risk: 'write',
    description: 'Create one directory in a writable workspace. Its parent must already exist; never creates parents recursively.',
    parameters: { type: 'object', properties: { path: { type: 'string' } }, required: ['path'] },
  },
  {
    name: 'rename_file',
    risk: 'write',
    description: 'Move one regular file within the same filesystem, without overwriting the destination. Both parents must exist; links and directories are refused.',
    parameters: {
      type: 'object',
      properties: { path: { type: 'string' }, new_path: { type: 'string' } },
      required: ['path', 'new_path'],
    },
  },
  {
    name: 'delete_file',
    risk: 'write',
    description: 'Delete one regular file in a writable workspace. Never deletes a directory, follows a link, or recurses.',
    parameters: { type: 'object', properties: { path: { type: 'string' } }, required: ['path'] },
  },
  {
    name: 'search',
    risk: 'read',
    description: 'Search regular UTF-8 workspace files with a deadline-isolated JavaScript regex. Each page scans at most 8 MiB/2000 entries and returns at most 64 KiB. Continue using cursor; skipped or unfinished content is explicit. Hidden entries, links, and dependency/build directories are excluded.',
    parameters: {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'JavaScript regular expression source.' },
        path: { type: 'string', description: 'Directory to search. Defaults to the primary root.' },
        extensions: { type: 'string', description: 'Comma-separated extension filter, e.g. "ts,tsx".' },
        max_results: { type: 'integer', description: 'Default 60, maximum 300.' },
        cursor: { type: 'string', description: 'Single-use continuation for the same path/pattern/extensions and Workspace. Expires after 60 seconds or restart; enumeration is live, not a filesystem snapshot.' },
        timeout_ms: { type: 'integer', minimum: 1, maximum: 2000, description: 'Page and regex deadline; default 1000 ms. Refine a pattern that repeatedly times out.' },
      },
      required: ['pattern'],
    },
  },
];

/**
 * Tell the model when a path only worked after repair.
 *
 * Prepended to the result rather than logged quietly: the point is that the NEXT call uses the
 * corrected path. Silently fixing it would leave the model repeating the broken one.
 */
function repairNote(repaired: string | undefined): string {
  return repaired === undefined ? '' : `[path corrected to: ${repaired} — use this exact form from now on]
`;
}

async function readRange(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
  const start = integer(args.start_line, 1, 'start_line');
  const end = integer(args.end_line, Number.MAX_SAFE_INTEGER, 'end_line');
  if (end < start) throw new Error('end_line must be at least start_line');
  const { path, repaired } = await ctx.workspace.resolveForgiving(stringArg(args.path, 'path'), 'read');
  ctx.signal.throwIfAborted();
  const info = await stat(path);
  ctx.signal.throwIfAborted();
  if (!info.isFile()) throw new Error('path is not a regular file');
  if (info.size > MAX_READ_BYTES && args.end_line === undefined) {
    throw new Error(`file is ${info.size} bytes; supply start_line and end_line for a bounded range (unbounded limit ${MAX_READ_BYTES} bytes)`);
  }
  const note = repairNote(repaired);
  const budget = MAX_OUTPUT_BYTES - Buffer.byteLength(note) - 512;
  const output: string[] = [];
  let used = 0;
  let lineNumber = 1;
  let pending = '';
  let bytesRead = 0;
  let truncated = false;
  let finished = false;
  const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
  const consume = (text: string, eof = false): void => {
    let offset = 0;
    while (!finished) {
      const newline = text.indexOf('\n', offset);
      const fragment = text.slice(offset, newline === -1 ? undefined : newline);
      if (lineNumber >= start) {
        pending += fragment;
        const cost = Buffer.byteLength(`${lineNumber}\t${pending}`) + (output.length ? 1 : 0);
        if (used + cost > budget) {
          if (output.length === 0) throw new Error(`line ${lineNumber} exceeds the ${MAX_OUTPUT_BYTES}-byte response limit; use search for bounded excerpts or inspect this long line outside read_file`);
          truncated = true;
          finished = true;
          return;
        }
        if (newline !== -1 || eof) {
          output.push(`${lineNumber}\t${pending}`);
          used += cost;
        }
      }
      if (newline === -1 && !eof) return;
      pending = '';
      if (lineNumber === end || (newline === -1 && eof)) { finished = true; return; }
      lineNumber += 1;
      offset = newline + 1;
    }
  };
  ctx.signal.throwIfAborted();
  const handle = await open(path, 'r');
  try {
    ctx.signal.throwIfAborted();
    const buffer = Buffer.allocUnsafe(READ_CHUNK_BYTES);
    while (!finished && bytesRead < MAX_SCAN_BYTES) {
      ctx.signal.throwIfAborted();
      const read = await handle.read(buffer, 0, Math.min(buffer.length, MAX_SCAN_BYTES - bytesRead), bytesRead);
      ctx.signal.throwIfAborted();
      bytesRead += read.bytesRead;
      const chunk = buffer.subarray(0, read.bytesRead);
      if (chunk.includes(0)) throw new Error('file appears to be binary');
      let decoded: string;
      try { decoded = decoder.decode(chunk, { stream: read.bytesRead !== 0 }); }
      catch { throw new Error('file is not valid UTF-8; refusing lossy decoding'); }
      consume(decoded, read.bytesRead === 0);
    }
  } finally {
    await handle.close();
  }
  ctx.signal.throwIfAborted();
  if (!finished) truncated = true;
  if (output.length === 0) {
    throw new Error(truncated
      ? `start_line ${start} was not reached within the ${MAX_SCAN_BYTES}-byte scan limit; choose an earlier range or a smaller file`
      : `start_line ${start} is beyond the end of the file (${lineNumber} lines)`);
  }
  const last = start + output.length - 1;
  const notice = truncated ? `\n[truncated at the output or scan limit; resume with start_line=${last + 1} and an explicit end_line]` : '';
  return {
    path,
    summary: `${ctx.workspace.describe(path)} lines ${start}-${last}${truncated ? ' (truncated)' : ''}`,
    content: note + output.join('\n') + notice,
    bytesRead,
    complete: !truncated,
    truncated,
    range: { startLine: start, endLine: last, ...(truncated ? { nextLine: last + 1 } : {}) },
  };
}

async function fileInfo(path: string, ctx: ToolContext, allowMissing = false): Promise<Stats | null> {
  ctx.signal.throwIfAborted();
  const info = await lstat(path).catch((error: NodeJS.ErrnoException) => {
    if (allowMissing && error.code === 'ENOENT') return null;
    throw error;
  });
  ctx.signal.throwIfAborted();
  if (info && (!info.isFile() || info.isSymbolicLink())) throw new Error('path must be a regular file, not a directory or symlink');
  if (info && info.nlink !== 1) throw new Error('file has multiple hard links; refusing to change an aliased file');
  return info;
}

function sameFile(before: Stats, after: Stats): boolean {
  return before.dev === after.dev && before.ino === after.ino && before.size === after.size
    && before.mtimeMs === after.mtimeMs && before.ctimeMs === after.ctimeMs && before.nlink === after.nlink;
}

async function editableText(path: string, info: Stats | null, ctx: ToolContext): Promise<string> {
  if (!info) return '';
  if (info.size > MAX_WRITE_BYTES) throw new Error(`file exceeds the ${MAX_WRITE_BYTES}-byte edit limit`);
  ctx.signal.throwIfAborted();
  const handle = await open(path, 'r');
  try {
    ctx.signal.throwIfAborted();
    const buffer = Buffer.allocUnsafe(MAX_WRITE_BYTES + 1);
    let length = 0;
    while (length < buffer.length) {
      ctx.signal.throwIfAborted();
      const read = await handle.read(buffer, length, Math.min(READ_CHUNK_BYTES, buffer.length - length), length);
      ctx.signal.throwIfAborted();
      length += read.bytesRead;
      if (read.bytesRead === 0) break;
    }
    const current = await handle.stat();
    ctx.signal.throwIfAborted();
    if (!sameFile(info, current)) throw new Error('file changed while reading; read it again before editing');
    if (length > MAX_WRITE_BYTES) throw new Error(`file exceeds the ${MAX_WRITE_BYTES}-byte edit limit`);
    const bytes = buffer.subarray(0, length);
    if (bytes.includes(0)) throw new Error('file appears to be binary; refusing text replacement');
    try { return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes); }
    catch { throw new Error('file is not valid UTF-8; refusing lossy text replacement'); }
  } finally {
    await handle.close();
  }
}

async function writeText(path: string, content: string, info: Stats | null, ctx: ToolContext): Promise<void> {
  await ctx.workspace.resolveExisting(path, 'write');
  ctx.signal.throwIfAborted();
  const handle = await open(path, info ? 'r+' : 'wx', 0o600);
  try {
    ctx.signal.throwIfAborted();
    const current = await handle.stat();
    ctx.signal.throwIfAborted();
    if (!current.isFile() || current.nlink !== 1 || (info && !sameFile(info, current))) {
      throw new Error('file changed before writing; read it again before editing');
    }
    await ctx.workspace.resolveExisting(path, 'write');
    ctx.signal.throwIfAborted();
    await handle.writeFile(content, { encoding: 'utf8', signal: ctx.signal });
    ctx.signal.throwIfAborted();
    if (info) await handle.truncate(Buffer.byteLength(content));
  } finally {
    await handle.close();
  }
}

function textChange(path: string, before: string, after: string, verb: string, repaired: string | undefined, ctx: ToolContext): ToolResult {
  const label = ctx.workspace.describe(path);
  const difference = unifiedDiff(before, after, label);
  const diffTruncated = Buffer.byteLength(difference.patch) > MAX_OUTPUT_BYTES - 8192;
  return {
    path,
    summary: `${verb} ${label} (+${difference.added} -${difference.removed})`,
    content: repairNote(repaired) + `${verb} ${label}: ${Buffer.byteLength(after)} bytes.${diffTruncated ? ' Diff omitted because it exceeds the response limit; inspect the changed file in bounded ranges.' : ''}`,
    diff: difference.patch && !diffTruncated ? { path: label, ...difference } : undefined,
    diffTruncated,
  };
}

interface DirectoryEntry {
  path: string;
  type: 'file' | 'directory';
  size?: number;
  skipped?: boolean;
}

interface SearchLine { path: string; line: number; text: string }

interface SearchFile {
  path: string;
  handle: FileHandle;
  info: Stats;
  offset: number;
  line: number;
  buffer: Buffer;
  eof: boolean;
  discarding: boolean;
}

interface PageState {
  key: string;
  workspace: Workspace;
  stack: { path: string; depth: number; handle: Dir }[];
  descend?: { path: string; depth: number };
  pendingEntry?: DirectoryEntry;
  file?: SearchFile;
  pendingLine?: SearchLine;
  skipped: Record<string, number>;
  gaps: boolean;
  busy: boolean;
  closed: boolean;
  timer?: ReturnType<typeof setTimeout>;
}

interface PageBudget { deadline: number; bytesRead: number; steps: number; reason?: string }

const cursors = new Map<string, PageState>();
let openPages = 0;

function skip(state: PageState, reason: string, gap = false): void {
  state.skipped[reason] = (state.skipped[reason] ?? 0) + 1;
  if (gap) state.gaps = true;
}

function withinBudget(budget: PageBudget, ctx: ToolContext): boolean {
  ctx.signal.throwIfAborted();
  if (performance.now() >= budget.deadline) budget.reason = 'deadline';
  else if (budget.bytesRead >= MAX_SCAN_BYTES) budget.reason = 'byte scan limit';
  else if (budget.steps >= MAX_PAGE_STEPS) budget.reason = 'entry scan limit';
  return budget.reason === undefined;
}

async function closePage(state: PageState): Promise<void> {
  if (state.closed) return;
  state.closed = true;
  if (state.timer) clearTimeout(state.timer);
  if (state.file) await state.file.handle.close().catch(() => undefined);
  state.file = undefined;
  for (const frame of state.stack.splice(0)) await frame.handle.close().catch(() => undefined);
  openPages -= 1;
}

async function pageState(path: string, key: string, cursor: unknown, ctx: ToolContext): Promise<PageState> {
  ctx.signal.throwIfAborted();
  if (cursor !== undefined) {
    if (typeof cursor !== 'string' || cursor.length > 100) throw new Error('invalid cursor; restart the query');
    const state = cursors.get(cursor);
    if (!state || state.key !== key || state.workspace !== ctx.workspace) throw new Error('cursor expired, was consumed, or belongs to another query or Workspace; restart the query');
    if (state.busy) throw new Error('cursor is already in use');
    state.busy = true;
    clearTimeout(state.timer);
    cursors.delete(cursor);
    return state;
  }
  while (openPages >= MAX_CURSORS) {
    const oldest = cursors.entries().next().value as [string, PageState] | undefined;
    if (!oldest) throw new Error(`concurrent search/list cursor limit is ${MAX_CURSORS}; retry after an active page finishes`);
    cursors.delete(oldest[0]);
    await closePage(oldest[1]);
  }
  ctx.signal.throwIfAborted();
  openPages += 1;
  let handle: Dir;
  try { handle = await opendir(path); }
  catch (error) { openPages -= 1; throw error; }
  const state: PageState = { key, workspace: ctx.workspace, stack: [{ path, depth: 1, handle }], skipped: {}, gaps: false, busy: true, closed: false };
  if (ctx.signal.aborted) { await closePage(state); ctx.signal.throwIfAborted(); }
  return state;
}

function keepPage(state: PageState): string {
  const cursor = randomUUID();
  state.busy = false;
  state.timer = setTimeout(() => {
    cursors.delete(cursor);
    void closePage(state);
  }, CURSOR_TTL_MS);
  state.timer.unref();
  cursors.set(cursor, state);
  return cursor;
}

async function nextEntry(state: PageState, depth: number, budget: PageBudget, ctx: ToolContext): Promise<DirectoryEntry | undefined> {
  while (withinBudget(budget, ctx)) {
    if (state.pendingEntry) { const entry = state.pendingEntry; state.pendingEntry = undefined; return entry; }
    if (state.descend) {
      const child = state.descend;
      state.descend = undefined;
      try {
        const resolved = await ctx.workspace.resolveExisting(child.path, 'read');
        ctx.signal.throwIfAborted();
        const info = await lstat(child.path);
        ctx.signal.throwIfAborted();
        if (info.isSymbolicLink()) { skip(state, 'links'); continue; }
        if (state.stack.length >= 128) { skip(state, 'directory_handle_limit', true); continue; }
        const handle = await opendir(resolved.path);
        state.stack.push({ ...child, path: resolved.path, handle });
        ctx.signal.throwIfAborted();
      } catch (error) {
        ctx.signal.throwIfAborted();
        skip(state, error instanceof WorkspaceError ? 'policy' : 'unreadable_directories', !(error instanceof WorkspaceError));
      }
    }
    const frame = state.stack[state.stack.length - 1];
    if (!frame) return undefined;
    ctx.signal.throwIfAborted();
    const entry = await frame.handle.read().catch((error: NodeJS.ErrnoException) => {
      ctx.signal.throwIfAborted();
      skip(state, `directory_${error.code ?? 'error'}`, true);
      return null;
    });
    ctx.signal.throwIfAborted();
    budget.steps += 1;
    if (!entry) { state.stack.pop(); await frame.handle.close(); continue; }
    if (entry.name.startsWith('.') && entry.name !== '.env.example') { skip(state, 'hidden'); continue; }
    if (entry.isSymbolicLink()) { skip(state, 'links'); continue; }
    const path = join(frame.path, entry.name);
    if (entry.isDirectory()) {
      const excluded = SKIP_DIRS.has(entry.name);
      if (excluded) skip(state, 'excluded_directories');
      else if (frame.depth < depth) state.descend = { path, depth: frame.depth + 1 };
      return { path, type: 'directory', ...(excluded ? { skipped: true } : {}) };
    }
    if (!entry.isFile()) { skip(state, 'non_regular'); continue; }
    try {
      const resolved = await ctx.workspace.resolveExisting(path, 'read');
      ctx.signal.throwIfAborted();
      const info = await lstat(path);
      ctx.signal.throwIfAborted();
      if (!info.isFile() || info.isSymbolicLink()) { skip(state, 'links'); continue; }
      return { path: resolved.path, type: 'file', size: info.size };
    } catch (error) {
      ctx.signal.throwIfAborted();
      skip(state, error instanceof WorkspaceError ? 'policy' : 'unreadable_files', !(error instanceof WorkspaceError));
    }
  }
  return undefined;
}

function pageContent(lines: string[], repaired: string | undefined, complete: boolean, cursor: string | undefined, reason: string | undefined, state: PageState, empty: string): string {
  const body = lines.length ? lines.join('\n') : complete ? empty : '(no results in this page; scan is not exhaustive)';
  const status = complete ? 'complete within the declared depth and file/skip policy' : `incomplete: ${reason ?? 'some content was skipped'}`;
  return repairNote(repaired) + body + `\n[${status}; skipped=${JSON.stringify(state.skipped)}${cursor ? `; cursor=${cursor}` : ''}]`;
}

async function listPage(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
  const depth = integer(args.depth, 1, 'depth', 4);
  const limit = integer(args.max_entries, MAX_ENTRIES, 'max_entries', MAX_ENTRIES);
  const timeout = integer(args.timeout_ms, 1000, 'timeout_ms', 2000);
  const { path, repaired } = await ctx.workspace.resolveForgiving(stringArg(args.path, 'path', '.'), 'read');
  ctx.signal.throwIfAborted();
  const key = JSON.stringify(['list', path, depth]);
  const state = await pageState(path, key, args.cursor, ctx);
  const budget: PageBudget = { deadline: performance.now() + timeout, bytesRead: 0, steps: 0 };
  const entries: DirectoryEntry[] = [];
  const lines: string[] = [];
  let bytes = Buffer.byteLength(repairNote(repaired)) + 4096;
  let done = false;
  try {
    while (entries.length < limit && withinBudget(budget, ctx)) {
      const entry = await nextEntry(state, depth, budget, ctx);
      if (!entry) { done = budget.reason === undefined; break; }
      const shown = relative(path, entry.path).split(sep).join('/');
      const line = entry.type === 'directory' ? `${shown}/${entry.skipped ? '  (skipped)' : ''}` : `${shown}  ${entry.size}b`;
      if (bytes + Buffer.byteLength(line) + 1 > MAX_OUTPUT_BYTES) { state.pendingEntry = entry; budget.reason = 'output limit'; break; }
      bytes += Buffer.byteLength(line) + 1;
      entries.push(entry);
      lines.push(line);
    }
    ctx.signal.throwIfAborted();
    const nextCursor = done ? undefined : keepPage(state);
    const complete = done && !state.gaps;
    if (done) await closePage(state);
    return { path, entries, complete, truncated: !complete, nextCursor, skipped: { ...state.skipped },
      summary: `${ctx.workspace.describe(path)} - ${entries.length} entries${complete ? '' : ' (incomplete page)'}`,
      content: pageContent(lines, repaired, complete, nextCursor, budget.reason ?? (done ? undefined : 'entry limit'), state, '(empty directory)') };
  } catch (error) { await closePage(state); throw error; }
}

class RegexDeadline extends Error {}

function regexMatcher(pattern: string, deadline: number, signal: AbortSignal): { test: (text: string) => Promise<boolean>; close: () => Promise<number> } {
  signal.throwIfAborted();
  const worker = new Worker(`
    const { parentPort, workerData } = require('node:worker_threads');
    const regex = new RegExp(workerData);
    parentPort.on('message', text => parentPort.postMessage(regex.test(text)));
  `, { eval: true, workerData: pattern, execArgv: [], resourceLimits: { maxOldGenerationSizeMb: 16, maxYoungGenerationSizeMb: 4, stackSizeMb: 2 } });
  let failure: Error | undefined;
  worker.on('error', (error: Error) => { failure = error; });
  return {
    test: async (text: string): Promise<boolean> => {
      signal.throwIfAborted();
      if (failure) throw new Error(`invalid regular expression or regex worker failure: ${failure.message}`);
      if (performance.now() >= deadline) throw new RegexDeadline('regex deadline');
      return new Promise<boolean>((resolveMatch, rejectMatch) => {
        const clean = (): void => {
          clearTimeout(timer);
          signal.removeEventListener('abort', aborted);
          worker.off('message', message);
          worker.off('error', failed);
          worker.off('exit', exited);
        };
        const message = (matched: boolean): void => { clean(); resolveMatch(matched); };
        const failed = (error: Error): void => { clean(); rejectMatch(new Error(`invalid regular expression or regex worker failure: ${error.message}`)); };
        const exited = (): void => { clean(); rejectMatch(new Error('regex worker exited before returning a result')); };
        const aborted = (): void => { clean(); rejectMatch(signal.reason ?? new Error('search cancelled')); };
        const timer = setTimeout(() => { clean(); rejectMatch(new RegexDeadline('regex deadline')); }, Math.max(1, deadline - performance.now()));
        worker.once('message', message);
        worker.once('error', failed);
        worker.once('exit', exited);
        signal.addEventListener('abort', aborted, { once: true });
        worker.postMessage(text);
      });
    },
    close: () => worker.terminate(),
  };
}

async function nextLine(state: PageState, extensions: string[], budget: PageBudget, ctx: ToolContext): Promise<SearchLine | undefined> {
  while (withinBudget(budget, ctx)) {
    if (state.pendingLine) return state.pendingLine;
    if (!state.file) {
      const entry = await nextEntry(state, Number.MAX_SAFE_INTEGER, budget, ctx);
      if (!entry) return undefined;
      if (entry.type !== 'file') continue;
      if (extensions.length && !extensions.some((extension) => entry.path.endsWith(`.${extension}`))) { skip(state, 'extensions'); continue; }
      try {
        const resolved = await ctx.workspace.resolveExisting(entry.path, 'read');
        ctx.signal.throwIfAborted();
        const handle = await open(resolved.path, 'r');
        try {
          ctx.signal.throwIfAborted();
          const info = await handle.stat();
          ctx.signal.throwIfAborted();
          if (!info.isFile()) { await handle.close(); skip(state, 'non_regular'); continue; }
          state.file = { path: resolved.path, handle, info, offset: 0, line: 1, buffer: Buffer.alloc(0), eof: false, discarding: false };
        } catch (error) { await handle.close(); throw error; }
      } catch (error) {
        ctx.signal.throwIfAborted();
        skip(state, error instanceof WorkspaceError ? 'policy' : 'unreadable_files', !(error instanceof WorkspaceError));
        continue;
      }
    }
    const file = state.file;
    const newline = file.buffer.indexOf(10);
    if (newline !== -1 || file.eof) {
      const bytes = newline === -1 ? file.buffer : file.buffer.subarray(0, newline);
      file.buffer = newline === -1 ? Buffer.alloc(0) : file.buffer.subarray(newline + 1);
      const line = file.line++;
      let text: string | undefined;
      if (!file.discarding && bytes.length <= MAX_SEARCH_LINE_BYTES) {
        try { text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes); }
        catch { skip(state, 'invalid_utf8', true); }
      } else if (!file.discarding) skip(state, 'oversized_lines', true);
      file.discarding = false;
      if (newline === -1) { await file.handle.close(); state.file = undefined; }
      ctx.signal.throwIfAborted();
      if (text !== undefined) { state.pendingLine = { path: file.path, line, text }; return state.pendingLine; }
      continue;
    }
    if (file.buffer.length > MAX_SEARCH_LINE_BYTES) {
      if (!file.discarding) skip(state, 'oversized_lines', true);
      file.discarding = true;
      file.buffer = Buffer.alloc(0);
    }
    ctx.signal.throwIfAborted();
    const buffer = Buffer.allocUnsafe(Math.min(READ_CHUNK_BYTES, MAX_SCAN_BYTES - budget.bytesRead));
    const read = await file.handle.read(buffer, 0, buffer.length, file.offset);
    ctx.signal.throwIfAborted();
    budget.bytesRead += read.bytesRead;
    file.offset += read.bytesRead;
    if (read.bytesRead === 0) { file.eof = true; continue; }
    const chunk = buffer.subarray(0, read.bytesRead);
    if (chunk.includes(0)) { skip(state, 'binary'); await file.handle.close(); state.file = undefined; continue; }
    file.buffer = file.buffer.length ? Buffer.concat([file.buffer, chunk]) : chunk;
  }
  return undefined;
}

async function searchPage(args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
  if (typeof args.pattern !== 'string' || args.pattern.length > 4096) throw new Error('pattern must be a regular expression string of at most 4096 characters');
  const pattern = args.pattern;
  const limit = integer(args.max_results, 60, 'max_results', 300);
  const timeout = integer(args.timeout_ms, 1000, 'timeout_ms', 2000);
  if (args.extensions !== undefined && (typeof args.extensions !== 'string' || args.extensions.length > 4096)) throw new Error('extensions must be a comma-separated string of at most 4096 characters');
  const extensions = [...new Set(String(args.extensions ?? '').split(',').map((entry) => entry.trim().replace(/^\./, '')).filter(Boolean))].sort();
  const { path, repaired } = await ctx.workspace.resolveForgiving(stringArg(args.path, 'path', '.'), 'read');
  ctx.signal.throwIfAborted();
  const state = await pageState(path, JSON.stringify(['search', path, pattern, extensions]), args.cursor, ctx);
  const budget: PageBudget = { deadline: performance.now() + timeout, bytesRead: 0, steps: 0 };
  let matcher: ReturnType<typeof regexMatcher> | undefined;
  const matches: SearchLine[] = [];
  const lines: string[] = [];
  let bytes = Buffer.byteLength(repairNote(repaired)) + 4096;
  let done = false;
  try {
    matcher = regexMatcher(pattern, budget.deadline, ctx.signal);
    await matcher.test('');
    if (state.file) {
      await ctx.workspace.resolveExisting(state.file.path, 'read');
      ctx.signal.throwIfAborted();
      const current = await state.file.handle.stat();
      ctx.signal.throwIfAborted();
      if (!sameFile(state.file.info, current)) throw new Error('file changed between search pages; restart the query');
    }
    while (matches.length < limit && withinBudget(budget, ctx)) {
      const candidate = await nextLine(state, extensions, budget, ctx);
      if (!candidate) { done = budget.reason === undefined; break; }
      if (await matcher.test(candidate.text)) {
        ctx.signal.throwIfAborted();
        const text = Array.from(candidate.text.trim()).slice(0, 240).join('');
        const line = `${relative(path, candidate.path).split(sep).join('/')}:${candidate.line}: ${text}`;
        if (bytes + Buffer.byteLength(line) + 1 > MAX_OUTPUT_BYTES) { budget.reason = 'output limit'; break; }
        bytes += Buffer.byteLength(line) + 1;
        matches.push({ ...candidate, text });
        lines.push(line);
      }
      state.pendingLine = undefined;
    }
  } catch (error) {
    if (error instanceof RegexDeadline) budget.reason = 'regex deadline; refine the pattern or retry with a larger timeout_ms';
    else { await closePage(state); throw error; }
  } finally { if (matcher) await matcher.close(); }
  if (ctx.signal.aborted) { await closePage(state); ctx.signal.throwIfAborted(); }
  const nextCursor = done ? undefined : keepPage(state);
  const complete = done && !state.gaps;
  if (done) await closePage(state);
  return { path, matches, complete, truncated: !complete, nextCursor, bytesRead: budget.bytesRead, skipped: { ...state.skipped },
    summary: `${matches.length} matches for /${pattern}/${complete ? '' : ' (incomplete page)'}`,
    content: pageContent(lines, repaired, complete, nextCursor, budget.reason ?? (done ? undefined : 'result limit'), state, '(no matches)') };
}

export async function runFileTool(name: string, args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
  const workspace = ctx.workspace;
  ctx.signal.throwIfAborted();

  if (name === 'list_dir') {
    return listPage(args, ctx);
  }

  if (name === 'read_file') {
    return readRange(args, ctx);
  }

  if (name === 'write_file') {
    if (typeof args.content !== 'string') throw new Error('content must be a string');
    const content = args.content;
    if (Buffer.byteLength(content, 'utf8') > MAX_WRITE_BYTES) throw new Error(`content exceeds ${MAX_WRITE_BYTES} bytes`);
    const { path, repaired } = await workspace.resolveForgiving(stringArg(args.path, 'path'), 'write');
    ctx.signal.throwIfAborted();
    const info = await fileInfo(path, ctx, true);
    const before = await editableText(path, info, ctx);
    ctx.signal.throwIfAborted();
    const result = textChange(path, before, content, info ? 'replaced' : 'created', repaired, ctx);
    await writeText(path, content, info, ctx);
    return result;
  }

  if (name === 'edit_file') {
    const { path, repaired } = await workspace.resolveForgiving(stringArg(args.path, 'path'), 'write', true);
    ctx.signal.throwIfAborted();
    if (typeof args.old_text !== 'string' || typeof args.new_text !== 'string') throw new Error('old_text and new_text must be strings');
    if (args.replace_all !== undefined && typeof args.replace_all !== 'boolean') throw new Error('replace_all must be a boolean');
    const oldText = args.old_text;
    const newText = args.new_text;
    if (oldText === '') throw new Error('old_text must not be empty; use write_file to create a file');
    if (Buffer.byteLength(oldText) > MAX_WRITE_BYTES || Buffer.byteLength(newText) > MAX_WRITE_BYTES) throw new Error(`edit text exceeds ${MAX_WRITE_BYTES} bytes`);
    const info = await fileInfo(path, ctx);
    const before = await editableText(path, info, ctx);
    ctx.signal.throwIfAborted();
    let occurrences = 0;
    let offset = 0;
    while ((offset = before.indexOf(oldText, offset)) !== -1) { occurrences += 1; offset += oldText.length; }
    if (occurrences === 0) throw new Error('old_text was not found in the file');
    if (occurrences > 1 && args.replace_all !== true) {
      throw new Error(`old_text occurs ${occurrences} times; pass replace_all or extend the snippet to make it unique`);
    }
    const size = Buffer.byteLength(before) + occurrences * (Buffer.byteLength(newText) - Buffer.byteLength(oldText));
    if (size > MAX_WRITE_BYTES) throw new Error(`edited content exceeds ${MAX_WRITE_BYTES} bytes`);
    const after = args.replace_all === true ? before.split(oldText).join(newText) : before.replace(oldText, () => newText);
    const result = textChange(path, before, after, 'edited', repaired, ctx);
    await writeText(path, after, info, ctx);
    return result;
  }

  if (name === 'make_dir') {
    const { path, repaired } = await workspace.resolveForgiving(stringArg(args.path, 'path'), 'write');
    ctx.signal.throwIfAborted();
    await mkdir(path, { mode: 0o700 });
    return { path, summary: `created directory ${workspace.describe(path)}`, content: repairNote(repaired) + `Created directory ${workspace.describe(path)}.` };
  }

  if (name === 'delete_file') {
    const { path, repaired } = await workspace.resolveForgiving(stringArg(args.path, 'path'), 'write', true);
    ctx.signal.throwIfAborted();
    const before = await fileInfo(path, ctx);
    await workspace.resolveExisting(path, 'write');
    ctx.signal.throwIfAborted();
    const current = await fileInfo(path, ctx);
    if (!before || !current || !sameFile(before, current)) throw new Error('file changed before deletion; inspect it again');
    ctx.signal.throwIfAborted();
    await unlink(path);
    return { path, summary: `deleted ${workspace.describe(path)}`, content: repairNote(repaired) + `Deleted file ${workspace.describe(path)}.` };
  }

  if (name === 'rename_file') {
    const source = await workspace.resolveForgiving(stringArg(args.path, 'path'), 'write', true);
    ctx.signal.throwIfAborted();
    const destination = await workspace.resolveForgiving(stringArg(args.new_path, 'new_path'), 'write');
    ctx.signal.throwIfAborted();
    const before = await fileInfo(source.path, ctx);
    const target = await lstat(destination.path).catch((error: NodeJS.ErrnoException) => {
      if (error.code === 'ENOENT') return null;
      throw error;
    });
    ctx.signal.throwIfAborted();
    if (target) throw new Error('destination already exists; rename never overwrites a file or directory');
    await workspace.resolveExisting(source.path, 'write');
    ctx.signal.throwIfAborted();
    await workspace.resolveExisting(destination.path, 'write');
    ctx.signal.throwIfAborted();
    const current = await fileInfo(source.path, ctx);
    if (!before || !current || !sameFile(before, current)) throw new Error('source changed before rename; inspect it again');
    let linked = false;
    try {
      ctx.signal.throwIfAborted();
      await link(source.path, destination.path);
      linked = true;
      ctx.signal.throwIfAborted();
      await workspace.resolveExisting(source.path, 'write');
      ctx.signal.throwIfAborted();
      const sourceNow = await lstat(source.path);
      ctx.signal.throwIfAborted();
      const destinationNow = await lstat(destination.path);
      ctx.signal.throwIfAborted();
      if (sourceNow.isSymbolicLink() || destinationNow.isSymbolicLink()
        || sourceNow.dev !== before.dev || sourceNow.ino !== before.ino
        || destinationNow.dev !== before.dev || destinationNow.ino !== before.ino) throw new Error('rename paths changed during the move');
      await unlink(source.path);
    } catch (error) {
      if (linked) throw new Error(`rename incomplete: destination ${workspace.describe(destination.path)} was created; source ${workspace.describe(source.path)} may remain. Inspect both paths before retrying. ${error instanceof Error ? error.message : String(error)}`);
      if ((error as NodeJS.ErrnoException).code === 'EXDEV') throw new Error('rename requires the same filesystem; cross-volume moves are not supported');
      throw error;
    }
    return {
      path: destination.path,
      previousPath: source.path,
      summary: `renamed ${workspace.describe(source.path)} to ${workspace.describe(destination.path)}`,
      content: repairNote(source.repaired) + repairNote(destination.repaired) + `Renamed ${workspace.describe(source.path)} to ${workspace.describe(destination.path)} without replacing another file.`,
    };
  }

  if (name === 'search') {
    return searchPage(args, ctx);
  }

  throw new Error(`unknown file tool: ${name}`);
}

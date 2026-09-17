import { readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { join, relative, sep } from 'node:path';
import { unifiedDiff } from '../diff';
import type { Workspace } from '../workspace';
import type { ToolDefinition } from '../types';

const MAX_READ_BYTES = 512 * 1024;
const MAX_WRITE_BYTES = 2 * 1024 * 1024;
const MAX_ENTRIES = 600;
const SKIP_DIRS = new Set(['.git', 'node_modules', '.next', 'target', '__pycache__', '.venv', 'dist', '.cache']);

export interface ToolContext {
  readonly workspace: Workspace;
  readonly signal: AbortSignal;
}

export interface ToolResult {
  readonly summary: string;
  readonly content: string;
  readonly diff?: { path: string; added: number; removed: number; patch: string };
}

function looksBinary(buffer: Buffer): boolean {
  const window = buffer.subarray(0, 4_096);
  for (const byte of window) if (byte === 0) return true;
  return false;
}

export const FILE_TOOLS: ToolDefinition[] = [
  {
    name: 'list_dir',
    risk: 'read',
    description: 'List the entries of a directory inside the workspace. Use this before guessing a path.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Directory path, absolute or relative to the primary root.' },
        depth: { type: 'integer', description: 'Recursion depth, 1 to 4. Default 1.' },
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
        end_line: { type: 'integer', description: '1-indexed last line, inclusive.' },
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
    name: 'search',
    risk: 'read',
    description: 'Search file contents for a regular expression across the workspace.',
    parameters: {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'JavaScript regular expression source.' },
        path: { type: 'string', description: 'Directory to search. Defaults to the primary root.' },
        extensions: { type: 'string', description: 'Comma-separated extension filter, e.g. "ts,tsx".' },
        max_results: { type: 'integer', description: 'Default 60, maximum 300.' },
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

async function walk(root: string, depth: number, out: string[], base: string): Promise<void> {
  if (depth < 0 || out.length >= MAX_ENTRIES) return;
  const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
  for (const entry of entries) {
    if (out.length >= MAX_ENTRIES) return;
    if (entry.name.startsWith('.') && entry.name !== '.env.example') continue;
    const full = join(root, entry.name);
    const shown = relative(base, full).split(sep).join('/');
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) { out.push(`${shown}/  (skipped)`); continue; }
      out.push(`${shown}/`);
      await walk(full, depth - 1, out, base);
    } else if (entry.isFile()) {
      const info = await stat(full).catch(() => null);
      out.push(`${shown}  ${info ? info.size : '?'}b`);
    }
  }
}

export async function runFileTool(name: string, args: Record<string, unknown>, ctx: ToolContext): Promise<ToolResult> {
  const workspace = ctx.workspace;

  if (name === 'list_dir') {
    const { path, repaired } = await workspace.resolveForgiving(String(args.path ?? '.'), 'read');
    const depth = Math.min(4, Math.max(1, Number(args.depth ?? 1)));
    const out: string[] = [];
    await walk(path, depth - 1, out, path);
    const truncated = out.length >= MAX_ENTRIES;
    return {
      summary: `${workspace.describe(path)} — ${out.length}${truncated ? '+' : ''} entries`,
      content: out.length === 0 ? '(empty directory)' : out.join('\n') + (truncated ? `\n… truncated at ${MAX_ENTRIES} entries` : ''),
    };
  }

  if (name === 'read_file') {
    const { path, repaired } = await workspace.resolveForgiving(String(args.path ?? ''), 'read');
    const info = await stat(path);
    if (!info.isFile()) throw new Error('not a file');
    if (info.size > MAX_READ_BYTES) throw new Error(`file is ${info.size} bytes; read a line range instead (limit ${MAX_READ_BYTES})`);
    const buffer = await readFile(path);
    if (looksBinary(buffer)) throw new Error('file appears to be binary');
    const lines = buffer.toString('utf8').split('\n');
    const start = Math.max(1, Number(args.start_line ?? 1));
    const end = Math.min(lines.length, Number(args.end_line ?? lines.length));
    const slice = lines.slice(start - 1, end).map((line, index) => `${start + index}\t${line}`);
    return {
      summary: `${workspace.describe(path)} lines ${start}-${end} of ${lines.length}`,
      content: slice.join('\n'),
    };
  }

  if (name === 'write_file') {
    const content = String(args.content ?? '');
    if (Buffer.byteLength(content, 'utf8') > MAX_WRITE_BYTES) throw new Error(`content exceeds ${MAX_WRITE_BYTES} bytes`);
    const { path } = await workspace.resolveExisting(String(args.path ?? ''), 'write');
    const before = await readFile(path, 'utf8').catch(() => '');
    await writeFile(path, content, 'utf8');
    const label = workspace.describe(path);
    const stat_ = unifiedDiff(before, content, label);
    return {
      summary: `${before === '' ? 'created' : 'replaced'} ${label} (+${stat_.added} −${stat_.removed})`,
      content: `Wrote ${Buffer.byteLength(content, 'utf8')} bytes to ${label}.`,
      diff: stat_.patch ? { path: label, ...stat_ } : undefined,
    };
  }

  if (name === 'edit_file') {
    const { path } = await workspace.resolveExisting(String(args.path ?? ''), 'write');
    const oldText = String(args.old_text ?? '');
    const newText = String(args.new_text ?? '');
    if (oldText === '') throw new Error('old_text must not be empty; use write_file to create a file');
    const before = await readFile(path, 'utf8');
    const occurrences = before.split(oldText).length - 1;
    if (occurrences === 0) throw new Error('old_text was not found in the file');
    if (occurrences > 1 && args.replace_all !== true) {
      throw new Error(`old_text occurs ${occurrences} times; pass replace_all or extend the snippet to make it unique`);
    }
    const after = args.replace_all === true ? before.split(oldText).join(newText) : before.replace(oldText, newText);
    await writeFile(path, after, 'utf8');
    const label = workspace.describe(path);
    const stat_ = unifiedDiff(before, after, label);
    return {
      summary: `edited ${label} (+${stat_.added} −${stat_.removed}${occurrences > 1 ? `, ${occurrences} sites` : ''})`,
      content: `Applied edit to ${label}.`,
      diff: stat_.patch ? { path: label, ...stat_ } : undefined,
    };
  }

  if (name === 'search') {
    const { path, repaired } = await workspace.resolveForgiving(String(args.path ?? '.'), 'read');
    let regex: RegExp;
    try {
      regex = new RegExp(String(args.pattern ?? ''), 'g');
    } catch (error) {
      throw new Error(`invalid regular expression: ${error instanceof Error ? error.message : 'unparseable'}`);
    }
    const limit = Math.min(300, Math.max(1, Number(args.max_results ?? 60)));
    const exts = String(args.extensions ?? '').split(',').map((entry) => entry.trim().replace(/^\./, '')).filter(Boolean);
    const hits: string[] = [];
    const files: string[] = [];
    await walk(path, 6, files, path);
    for (const entry of files) {
      if (ctx.signal.aborted) break;
      if (hits.length >= limit) break;
      if (entry.endsWith('/') || entry.endsWith('(skipped)')) continue;
      const rel = entry.split('  ')[0] ?? '';
      if (rel === '') continue;
      if (exts.length > 0 && !exts.some((ext) => rel.endsWith(`.${ext}`))) continue;
      // Route every candidate back through the workspace so the secret-file rule applies to a
      // grep exactly as it applies to a read. Searching is a read; it just reads more files.
      const resolved = await workspace.resolveExisting(join(path, rel), 'read').catch(() => null);
      if (!resolved) continue;
      const full = resolved.path;
      const info = await stat(full).catch(() => null);
      if (!info || info.size > MAX_READ_BYTES) continue;
      const buffer = await readFile(full).catch(() => null);
      if (!buffer || looksBinary(buffer)) continue;
      buffer.toString('utf8').split('\n').forEach((line, index) => {
        if (hits.length >= limit) return;
        regex.lastIndex = 0;
        if (regex.test(line)) hits.push(`${rel}:${index + 1}: ${line.trim().slice(0, 240)}`);
      });
    }
    return {
      summary: `${hits.length}${hits.length >= limit ? '+' : ''} matches for /${String(args.pattern)}/`,
      content: hits.length === 0 ? '(no matches)' : hits.join('\n'),
    };
  }

  throw new Error(`unknown file tool: ${name}`);
}

/**
 * Load the shared methodology kernel into Apocrypha's system prompt.
 *
 * WHY A LOADER AND NOT A COPY. There are 42 live instruction surfaces in this workspace -- CLAUDE.md
 * files, AGENTS.md files, two runtime system prompts, a Rust const. Pasting a methodology into each
 * one produces 42 documents that agree today and diverge by the end of the month, and nobody can
 * then say which is canonical. One file, read at call time, is the same choice as `require` over
 * copy-paste: the kernel is a dependency, not a duplicate.
 *
 * It re-reads when the file changes, so editing the kernel takes effect on the next turn without a
 * restart. Same reason reflex.json is re-read every five seconds: a rule set you have to restart to
 * change is a rule set that stops being edited.
 *
 * ABSENCE IS DECLARED, NOT SWALLOWED. If the kernel is missing the prompt says so, in the prompt,
 * where the model will see it. An agent running without its methodology and not knowing that is
 * strictly worse than one that knows it is degraded -- this is the same rule the memory layer
 * follows when it reports a degraded region to the model instead of quietly returning less.
 */
import { readFileSync, statSync } from 'node:fs';

/** Where the kernel lives unless APOCRYPHA_METHOD says otherwise. */
export const DEFAULT_METHOD_PATH = 'C:\\Users\\Apocky\\source\\repos\\METHOD.md';

/** A kernel far larger than this is not a kernel any more; it is documentation, and it will not be
 *  held in working memory on turn 200. Truncating loudly beats silently spending the context.
 *
 *  Raised 24k -> 32k on 2026-09-20 when the kernel reached 22.7 KB and was one section away from
 *  being silently clipped mid-sentence. Raising the cap is the lesser evil ONLY because truncation
 *  here is invisible to the model; it is not permission for the kernel to keep growing. The panel
 *  that judged four candidate kernels marked every one of them down on survivability, and length
 *  was the reason. If this constant has to move again, cut the document instead. */
export const MAX_METHOD_BYTES = 32_000;

export interface Method {
  /** The kernel text, or '' when it could not be read. */
  readonly text: string;
  /** Absolute path that was tried. */
  readonly path: string;
  /** Empty when the kernel loaded; otherwise why it did not, in words the model can act on. */
  readonly reason: string;
  readonly bytes: number;
  readonly truncated: boolean;
}

let cache: { path: string; mtimeMs: number; size: number; value: Method } | null = null;

export function methodPath(): string {
  const fromEnv = process.env.APOCRYPHA_METHOD;
  return fromEnv && fromEnv.trim() ? fromEnv.trim() : DEFAULT_METHOD_PATH;
}

/** Read the kernel, re-reading only when the file actually changed. Never throws. */
export function loadMethod(path: string = methodPath()): Method {
  let stat: ReturnType<typeof statSync>;
  try {
    stat = statSync(path);
  } catch (err) {
    cache = null;
    return {
      text: '', path, bytes: 0, truncated: false,
      reason: `the methodology kernel could not be read at ${path}: ${(err as Error).message}`,
    };
  }
  if (cache && cache.path === path && cache.mtimeMs === stat.mtimeMs && cache.size === stat.size) {
    return cache.value;
  }
  let raw: string;
  try {
    raw = readFileSync(path, 'utf8');
  } catch (err) {
    cache = null;
    return {
      text: '', path, bytes: 0, truncated: false,
      reason: `the methodology kernel at ${path} could not be opened: ${(err as Error).message}`,
    };
  }
  const bytes = Buffer.byteLength(raw, 'utf8');
  const truncated = bytes > MAX_METHOD_BYTES;
  const text = truncated ? `${raw.slice(0, MAX_METHOD_BYTES)}\n[kernel truncated at ${MAX_METHOD_BYTES} bytes]` : raw;
  const value: Method = {
    text: text.trim(), path, bytes, truncated,
    reason: text.trim() ? '' : `the methodology kernel at ${path} is empty`,
  };
  cache = { path, mtimeMs: stat.mtimeMs, size: stat.size, value };
  return value;
}

/** The block to splice into a system prompt. Present or absent, it always says which. */
export function methodBlock(method: Method = loadMethod()): string {
  if (!method.text) {
    return [
      'METHODOLOGY KERNEL: NOT LOADED.',
      `  ${method.reason}`,
      '  You are running without the shared method. Say so if the operator asks how you are working,',
      '  and be correspondingly more careful: state what you verified and what you did not.',
    ].join('\n');
  }
  return [
    'METHODOLOGY KERNEL -- this is how you work. It outranks habit and convenience.',
    `  (loaded from ${method.path}${method.truncated ? ', TRUNCATED' : ''})`,
    '',
    method.text,
  ].join('\n');
}

/** Drop the cache. Tests use this; nothing in the running server needs it. */
export function resetMethodCache(): void {
  cache = null;
}

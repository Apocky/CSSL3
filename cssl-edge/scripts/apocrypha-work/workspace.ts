import { lstat, realpath, stat } from 'node:fs/promises';
import { basename, dirname, isAbsolute, relative, resolve, sep } from 'node:path';

export class WorkspaceError extends Error {
  readonly code: string;

  constructor(message: string, code = 'WORKSPACE_DENIED') {
    super(message);
    this.name = 'WorkspaceError';
    this.code = code;
  }
}

// Windows resolves `C:\x` and `c:\X` to the same file, so containment has to be decided on a
// case-folded comparison. Doing it with `startsWith` on the raw strings lets `..\WorkspaceEvil`
// past when the root is `...\Workspace`; `path.relative` is the only form that cannot be fooled
// by a shared prefix.
function contains(root: string, candidate: string): boolean {
  const rel = relative(root, candidate);
  if (rel === '') return true;
  if (isAbsolute(rel)) return false;
  return !rel.split(sep).includes('..');
}

function fold(value: string): string {
  return process.platform === 'win32' ? value.toLowerCase() : value;
}

// Files whose whole purpose is to hold a credential. Nothing in this service can reach the
// network — the model is local and there is no fetch tool — so a read here would not leak
// outward. It would still copy the secret into a session transcript on disk, and a transcript is
// a much easier thing to paste somewhere than a .env file is.
const SECRET_FILE = /(^|[\\/])(\.env(\.[^\\/]*)?|\.npmrc|\.pypirc|\.netrc|id_(rsa|ed25519|ecdsa)|.*\.(pem|pfx|p12|key|keystore|jks)|credentials(\.json)?|service[-_]account.*\.json)$/i;

function isSecretFile(path: string): boolean {
  // .env.example and friends are templates and carry no value.
  if (/\.(example|sample|template)$/i.test(path)) return false;
  return SECRET_FILE.test(path);
}

export interface WorkspaceRoot {
  readonly label: string;
  readonly path: string;
  readonly writable: boolean;
}

export class Workspace {
  private readonly roots: readonly WorkspaceRoot[];

  private constructor(roots: readonly WorkspaceRoot[]) {
    this.roots = roots;
  }

  static async open(specs: readonly { label: string; path: string; writable: boolean }[]): Promise<Workspace> {
    if (specs.length === 0) throw new WorkspaceError('no workspace root is configured', 'WORKSPACE_EMPTY');
    const roots: WorkspaceRoot[] = [];
    for (const spec of specs) {
      const resolved = resolve(spec.path);
      const info = await stat(resolved).catch(() => null);
      if (!info?.isDirectory()) throw new WorkspaceError(`workspace root is not a directory: ${spec.label}`, 'WORKSPACE_ROOT_MISSING');
      // Canonicalise once at open. Every later check compares against the real path, so a
      // symlink planted inside the root cannot widen it after the fact.
      roots.push({ label: spec.label, path: await realpath(resolved), writable: spec.writable });
    }
    return new Workspace(roots);
  }

  list(): readonly WorkspaceRoot[] {
    return this.roots;
  }

  private match(candidate: string): WorkspaceRoot | null {
    const folded = fold(candidate);
    let matched: WorkspaceRoot | null = null;
    for (const root of this.roots) {
      if (contains(fold(root.path), folded)
        && (!matched || root.path.length > matched.path.length || (root.path.length === matched.path.length && !root.writable))) matched = root;
    }
    return matched;
  }

  /**
   * Resolve a caller-supplied path to an absolute path inside a configured root.
   *
   * The returned path is safe to hand to `fs`. Throws rather than clamping, because silently
   * rewriting a path the caller asked for is how a confinement bug becomes invisible.
   */
  /**
   * Deterministic repairs for a path that does not resolve.
   *
   * A model occasionally emits a separator as a space, or mixes slash styles. Observed once in a
   * real turn: one wrong character became 38 flailing list_dir and search calls hunting for a file
   * that was there all along. (It is NOT a general Q2 weakness -- 24 controlled samples across four
   * conditions reproduced the path exactly every time -- but rare is not never, and the recovery is
   * cheap.)
   *
   * Deliberately NOT fuzzy. Each candidate is a fixed rewrite, and each one is re-checked through
   * resolveExisting itself, so confinement, read-only roots and the secret-file rule all still
   * apply. A repair that skipped those would turn a typo into a way out of the workspace.
   */
  private static repairCandidates(input: string): string[] {
    const sep = String.fromCharCode(92);
    // No regex here on purpose. Expressing "a backslash" as a RegExp through two layers of string
    // escaping is exactly how you end up with a pattern that silently matches nothing -- the first
    // version of this compiled cleanly and was wrong. split/join says what it means.
    const trimSegments = (value: string, on: string): string =>
      value.split(on).map((part) => part.trim()).join(on);

    const out = new Set<string>();
    out.add(trimSegments(input, sep));                       // a separator that arrived with a stray space
    out.add(input.split('/').join(sep));                     // forward -> backslash
    out.add(input.split(sep).join('/'));                     // backslash -> forward
    out.add(trimSegments(input.split('/').join(sep), sep));   // both: unify, then de-space
    out.add(trimSegments(input, '/'));
    out.delete(input);
    return [...out].filter((candidate) => candidate.length > 0);
  }

  /**
   * Resolve, and if that fails try a small set of fixed rewrites before giving up.
   *
   * `repaired` names the path that actually worked, so the caller can say so. A silent correction
   * would hide a real operator typo behind a guess.
   */
  async resolveForgiving(input: string, intent: 'read' | 'write', mustExist = intent === 'read'): Promise<{ path: string; root: WorkspaceRoot; repaired?: string }> {
    try {
      const direct = await this.resolveExisting(input, intent);
      // resolveExisting deliberately ACCEPTS a path that does not exist yet, so that write_file can
      // create one. For a READ that is the wrong answer: a mistyped path whose parent happens to
      // exist sails through here and fails later with a bare ENOENT, which is exactly the dead end
      // that sent a real turn into 38 list_dir calls. Reading requires the file to be there.
      if (mustExist && !(await stat(direct.path).then(() => true).catch(() => false))) {
        throw new WorkspaceError(`path does not exist: ${input}`, 'WORKSPACE_NOT_FOUND');
      }
      return direct;
    } catch (error) {
      if (!(error instanceof WorkspaceError) || !['WORKSPACE_NO_PARENT', 'WORKSPACE_NOT_FOUND'].includes(error.code)) throw error;
      for (const candidate of Workspace.repairCandidates(input)) {
        // Through the SAME function: every rule that guards resolveExisting guards this too.
        const hit = await this.resolveExisting(candidate, intent).catch(() => null);
        if (!hit) continue;
        // A candidate only counts if it actually exists; otherwise "repair" would just relocate the
        // same ENOENT to a path the operator never typed.
        if (mustExist && !(await stat(hit.path).then(() => true).catch(() => false))) continue;
        return { ...hit, repaired: candidate };
      }
      throw error; // nothing worked: report the ORIGINAL failure, not the last candidate's
    }
  }

  async resolveExisting(input: string, intent: 'read' | 'write'): Promise<{ path: string; root: WorkspaceRoot }> {
    const primary = this.roots[0];
    if (!primary) throw new WorkspaceError('no workspace root is configured', 'WORKSPACE_EMPTY');
    if (typeof input !== 'string' || input.trim() === '' || input.length > 4096 || input.includes('\0')) {
      throw new WorkspaceError('path must be a non-empty string of at most 4096 characters without NUL', 'WORKSPACE_INVALID_PATH');
    }
    const parts = input.split(/[\\/]/);
    if (parts.includes('..')) throw new WorkspaceError('parent traversal is not allowed in workspace paths', 'WORKSPACE_ESCAPE');
    if (parts.some((part) => isSecretFile(part.trim()))) {
      throw new WorkspaceError(`${input} holds credentials and is not accessible by this agent`, 'WORKSPACE_SECRET_FILE');
    }
    if (process.platform === 'win32' && (
      /^[\\/]{2}[?.][\\/]/.test(input)
      || /[:<>"|?*]/.test(input.replace(/^[a-z]:[\\/]/i, ''))
      || parts.some((part) => (part !== '.' && part.endsWith('.')) || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part.trim()))
    )) throw new WorkspaceError('device paths, alternate data streams, and ambiguous Windows names are not allowed', 'WORKSPACE_INVALID_PATH');
    const absolute = isAbsolute(input) ? resolve(input) : resolve(primary.path, input);
    const requestedRoot = this.match(absolute);
    if (!requestedRoot) throw new WorkspaceError(`path is outside every workspace root: ${input}`, 'WORKSPACE_ESCAPE');
    if (intent === 'write') {
      if (!requestedRoot.writable) throw new WorkspaceError(`workspace root "${requestedRoot.label}" is read-only`, 'WORKSPACE_READ_ONLY');
      const components = relative(requestedRoot.path, absolute).split(sep).filter(Boolean);
      let current = requestedRoot.path;
      for (let index = -1; index < components.length; index += 1) {
        if (index >= 0) current = resolve(current, components[index]!);
        const info = await lstat(current).catch((error: NodeJS.ErrnoException) => {
          if (error.code === 'ENOENT' && index === components.length - 1 && index >= 0) return null;
          throw new WorkspaceError(`path does not exist or its parent is unreachable: ${input}`, 'WORKSPACE_NO_PARENT');
        });
        if (info?.isSymbolicLink()) throw new WorkspaceError('writes through a symlink or junction are not allowed', 'WORKSPACE_LINK');
        if (index < components.length - 1 && !info?.isDirectory()) {
          throw new WorkspaceError(`parent is not a directory: ${input}`, 'WORKSPACE_NO_PARENT');
        }
      }
    }
    // realpath follows symlinks; if the target escapes, the canonical form reveals it. A path
    // that does not exist yet (a file about to be created) has its parent canonicalised instead.
    let canonical: string;
    try {
      canonical = await realpath(absolute);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw new WorkspaceError(`path is not accessible: ${input}`, 'WORKSPACE_NOT_FOUND');
      const parent = dirname(absolute);
      const parentReal = await realpath(parent).catch(() => {
        throw new WorkspaceError(`path does not exist and its parent is unreachable: ${input}`, 'WORKSPACE_NO_PARENT');
      });
      canonical = resolve(parentReal, basename(absolute));
    }
    const root = this.match(canonical);
    if (!root) throw new WorkspaceError(`path is outside every workspace root: ${input}`, 'WORKSPACE_ESCAPE');
    if (intent === 'write' && !root.writable) {
      throw new WorkspaceError(`workspace root "${root.label}" is read-only`, 'WORKSPACE_READ_ONLY');
    }
    // Compare the TRIMMED basename: " .env" is not ".env" to a string equality test, yet it is
    // plainly the same file being asked for. Found by a repair test, and it predates repair.
    if (canonical.split(sep).some((part) => isSecretFile(part.trim()))) {
      throw new WorkspaceError(
        `${input} holds credentials and is not readable by this agent. Ask the operator for the variable NAME you need; never the value.`,
        'WORKSPACE_SECRET_FILE',
      );
    }
    return { path: canonical, root };
  }

  describe(path: string): string {
    const root = this.match(path);
    if (!root) return path;
    const rel = relative(root.path, path);
    return rel === '' ? root.label : `${root.label}/${rel.split(sep).join('/')}`;
  }
}

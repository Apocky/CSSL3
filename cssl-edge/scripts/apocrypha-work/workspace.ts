import { realpath, stat } from 'node:fs/promises';
import { isAbsolute, relative, resolve, sep } from 'node:path';

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
    for (const root of this.roots) {
      if (contains(fold(root.path), folded)) return root;
    }
    return null;
  }

  /**
   * Resolve a caller-supplied path to an absolute path inside a configured root.
   *
   * The returned path is safe to hand to `fs`. Throws rather than clamping, because silently
   * rewriting a path the caller asked for is how a confinement bug becomes invisible.
   */
  async resolveExisting(input: string, intent: 'read' | 'write'): Promise<{ path: string; root: WorkspaceRoot }> {
    const primary = this.roots[0];
    if (!primary) throw new WorkspaceError('no workspace root is configured', 'WORKSPACE_EMPTY');
    const absolute = isAbsolute(input) ? resolve(input) : resolve(primary.path, input);
    // realpath follows symlinks; if the target escapes, the canonical form reveals it. A path
    // that does not exist yet (a file about to be created) has its parent canonicalised instead.
    let canonical: string;
    try {
      canonical = await realpath(absolute);
    } catch {
      const parent = resolve(absolute, '..');
      const parentReal = await realpath(parent).catch(() => {
        throw new WorkspaceError(`path does not exist and its parent is unreachable: ${input}`, 'WORKSPACE_NO_PARENT');
      });
      canonical = resolve(parentReal, absolute.slice(parent.length + 1));
    }
    const root = this.match(canonical);
    if (!root) throw new WorkspaceError(`path is outside every workspace root: ${input}`, 'WORKSPACE_ESCAPE');
    if (intent === 'write' && !root.writable) {
      throw new WorkspaceError(`workspace root "${root.label}" is read-only`, 'WORKSPACE_READ_ONLY');
    }
    if (isSecretFile(canonical)) {
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

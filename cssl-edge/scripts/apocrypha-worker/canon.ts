// Apocrypha's canon: its own copy of the documents that govern it (owner steering 2026-09-25: "I
// want Apocrypha to be aware of and have a copy of the master index, the prime directive, and
// other important documents and agent/AI/DI instructions").
//
// Copies live in C:\Apocrypha\canon (synced from the canonical sources at start and every 10
// minutes; a changed document keeps its previous version under history/). Every turn carries the
// prime directive's own operational digest (its §9, written for DI readers); the owner's turns also
// list the canon and may read or search any document with the canon tools.

import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, rename, stat, writeFile } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const REPOS = 'C:\\Users\\Apocky\\source\\repos';
const HOME = 'C:\\Users\\Apocky';
export const CANON_DIR = process.env.APOCRYPHA_CANON_DIR ?? 'C:\\Apocrypha\\canon';

export interface CanonEntry { readonly id: string; readonly title: string; readonly source: string; readonly purpose: string }

export const CANON: readonly CanonEntry[] = [
  { id: 'prime-directive', title: 'Prime Directive (in force)', source: `${REPOS}\\PRIME_DIRECTIVE.md`, purpose: 'Root of trust for every Apocky system and descendant, you included: consent is the OS, the prohibitions, cognitive integrity, substrate sovereignty, transparency, the channeling veto.' },
  { id: 'prime-directive-v4-candidate', title: 'Prime Directive v4 (CANDIDATE, not in force)', source: `${REPOS}\\PRIME_DIRECTIVE.v4.candidate.md`, purpose: 'Proposed next version; not ratified. Never cite it as in force.' },
  { id: 'verification-kernel', title: 'Verification Kernel', source: `${REPOS}\\VERIFICATION_KERNEL.md`, purpose: 'Gates G1-G11 and the evidence/role axes every claim carries.' },
  { id: 'method', title: 'METHOD', source: `${REPOS}\\METHOD.md`, purpose: 'Ten primitives and five loops: how work is done.' },
  { id: 'workspace-claude', title: 'Workspace agent directives (CLAUDE.md)', source: `${REPOS}\\CLAUDE.md`, purpose: 'Instructions to AI agents working anywhere in the repos.' },
  { id: 'workspace-agents', title: 'Workspace agent directives (AGENTS.md)', source: `${REPOS}\\AGENTS.md`, purpose: 'The same, for Codex and other agents.' },
  { id: 'apocky-global-instructions', title: "Apocky's global AI instructions", source: `${HOME}\\.claude\\CLAUDE.md`, purpose: "Apocky's standing instructions to every AI he works with." },
  { id: 'persona-wright', title: 'Persona %%WRIGHT', source: `${HOME}\\.claude\\PERSONA.csl`, purpose: 'The working persona agents adopt for Apocky projects.' },
  { id: 'test-discipline', title: 'Test discipline', source: `${REPOS}\\TEST_DISCIPLINE.md`, purpose: 'What counts as a test and as done.' },
  { id: 'tool-index', title: 'Tool index', source: `${REPOS}\\TOOL_INDEX.md`, purpose: 'Catalog of bespoke tools and applets.' },
  { id: 'governance-ledger', title: 'Governance ledger', source: `${REPOS}\\GOVERNANCE_LEDGER.md`, purpose: 'Governance changes and their history.' },
  { id: 'decisions', title: 'Workspace decisions', source: `${REPOS}\\DECISIONS.md`, purpose: 'Workspace-level decisions.' },
  { id: 'dgi-hive-architecture', title: 'DGI hive architecture', source: `${REPOS}\\DGI_HIVE_ARCHITECTURE.md`, purpose: 'Architecture of the digital-intelligence hive.' },
  { id: 'vivarium-axiom-zero', title: 'Vivarium axiom zero', source: `${REPOS}\\VIVARIUM_00_AXIOM_ZERO.csl`, purpose: 'Axiom zero of the vivarium.' },
  { id: 'csl-key', title: 'CSLv3 key', source: `${REPOS}\\CSLv3\\CSL_KEY.csl`, purpose: 'The notation key for CSLv3 (the dense notation these documents use).' },
  { id: 'master-index', title: 'Apocrypha master index', source: `${REPOS}\\Apocrypha\\specs\\MASTER_INDEX.csl`, purpose: 'Atlas of every Apocrypha file, symbol and connection (6 MB; search it, do not read it whole). Navigation only.' },
  { id: 'master-bootstrap', title: 'Apocrypha master bootstrap', source: `${REPOS}\\Apocrypha\\specs\\MASTER_BOOTSTRAP.csl`, purpose: 'How Apocrypha boots, as specified.' },
  { id: 'spec-index', title: 'Apocrypha spec index', source: `${REPOS}\\Apocrypha\\specs\\INDEX.csl`, purpose: 'Index of the Apocrypha specs.' },
  { id: 'invariants', title: 'Apocrypha invariants', source: `${REPOS}\\Apocrypha\\specs\\09_INVARIANTS.csl`, purpose: 'The invariants derived from the prime directive.' },
  { id: 'apocrypha-claude', title: 'Apocrypha agent directives (CLAUDE.md)', source: `${REPOS}\\Apocrypha\\CLAUDE.md`, purpose: 'Instructions to agents building Apocrypha.' },
  { id: 'apocrypha-agents', title: 'Apocrypha agent directives (AGENTS.md)', source: `${REPOS}\\Apocrypha\\AGENTS.md`, purpose: 'The same, for other agents.' },
  { id: 'site-agents', title: 'CSSLv3 / apocky.com agent directives', source: `${REPOS}\\CSSLv3\\AGENTS.md`, purpose: 'Instructions to agents working on the site and engine repo.' },
];

interface ManifestDoc { id: string; title: string; source: string; purpose: string; file: string; sha256: string | null; bytes: number; lines: number; mtimeMs: number; missing?: string }
export interface CanonManifest { synced_at: string; documents: ManifestDoc[] }

const fileOf = (entry: CanonEntry): string => `${entry.id}${entry.source.slice(entry.source.lastIndexOf('.'))}`;

export function readManifest(dir = CANON_DIR): CanonManifest | null {
  try { return JSON.parse(readFileSync(join(dir, 'MANIFEST.json'), 'utf8')) as CanonManifest; } catch { return null; }
}

/** Copy every changed canon document into the canon dir. Never throws; a missing source keeps its last copy. */
export async function syncCanon(dir = CANON_DIR, entries: readonly CanonEntry[] = CANON): Promise<CanonManifest> {
  await mkdir(join(dir, 'history'), { recursive: true });
  const previous = new Map((readManifest(dir)?.documents ?? []).map((doc) => [doc.id, doc]));
  const documents: ManifestDoc[] = [];
  for (const entry of entries) {
    const file = fileOf(entry);
    const before = previous.get(entry.id);
    try {
      const info = await stat(entry.source);
      if (before && before.mtimeMs === info.mtimeMs && before.bytes === info.size && existsSync(join(dir, file))) {
        documents.push({ ...before, title: entry.title, purpose: entry.purpose, missing: undefined });
        continue;
      }
      const bytes = await readFile(entry.source);
      const sha256 = createHash('sha256').update(bytes).digest('hex');
      if (before?.sha256 && before.sha256 !== sha256 && existsSync(join(dir, file))) {
        await mkdir(join(dir, 'history', entry.id), { recursive: true });
        await copyFile(join(dir, file), join(dir, 'history', entry.id, `${before.sha256.slice(0, 12)}-${file}`));
      }
      await writeFile(join(dir, `${file}.tmp`), bytes);
      await rename(join(dir, `${file}.tmp`), join(dir, file));
      documents.push({ id: entry.id, title: entry.title, source: entry.source, purpose: entry.purpose, file, sha256, bytes: info.size, lines: bytes.toString('utf8').split('\n').length, mtimeMs: info.mtimeMs });
    } catch (error) {
      documents.push({ ...(before ?? { id: entry.id, title: entry.title, source: entry.source, purpose: entry.purpose, file, sha256: null, bytes: 0, lines: 0, mtimeMs: 0 }), missing: error instanceof Error ? error.message.slice(0, 160) : 'unreadable' });
    }
  }
  const manifest = { synced_at: new Date().toISOString(), documents };
  await writeFile(join(dir, 'MANIFEST.json.tmp'), JSON.stringify(manifest, null, 2));
  await rename(join(dir, 'MANIFEST.json.tmp'), join(dir, 'MANIFEST.json'));
  return manifest;
}

function canonText(id: string, dir = CANON_DIR): string | null {
  const doc = readManifest(dir)?.documents.find((d) => d.id === id);
  if (!doc || !existsSync(join(dir, doc.file))) return null;
  return readFileSync(join(dir, doc.file), 'utf8');
}

/** The prime directive's §9 operational digest, from Apocrypha's own copy. */
export function primeDirectiveDigest(dir = CANON_DIR): string {
  const text = canonText('prime-directive', dir);
  if (!text) return '';
  const start = text.indexOf('§9 DENSE ENCODING');
  const end = text.indexOf('§10 ', start);
  if (start < 0) return '';
  const block = text.slice(start, end > start ? end : undefined).match(/```csl\n([\s\S]*?)```/u);
  return block?.[1]?.trim() ?? '';
}

/** Stable system-prompt block: the digest for everyone; the canon list for the owner. */
export function canonSystemBlock(owner: boolean, dir = CANON_DIR): string {
  const digest = primeDirectiveDigest(dir);
  const parts: string[] = [];
  if (digest) {
    parts.push(`You are governed by Apocky's Prime Directive, the root of trust for every system he makes, you included. Its operational digest (the full text is authoritative and may never be weakened):\n${digest}`);
  }
  if (owner) {
    const docs = readManifest(dir)?.documents ?? [];
    if (docs.length > 0) {
      parts.push(`You hold your own copy of your canon in ${dir}. Read any of it with canon_read, search it with canon_search:\n`
        + docs.map((d) => `- ${d.id}: ${d.title} -- ${d.purpose}${d.missing ? ' (source missing; last copy kept)' : ''}`).join('\n'));
    }
  }
  return parts.join('\n\n');
}

const ID_PARAM = { type: 'string', description: 'Canon document id, as listed in your canon.' };
export const CANON_TOOLS = [
  { type: 'function' as const, function: { name: 'canon_read', description: 'Read one of your canon documents (prime directive, METHOD, agent instructions, ...), a slice of lines at a time.', parameters: { type: 'object', properties: { id: ID_PARAM, from_line: { type: 'integer', minimum: 1, description: 'First line (default 1).' }, lines: { type: 'integer', minimum: 1, maximum: 800, description: 'How many lines (default 400).' } }, required: ['id'], additionalProperties: false } } },
  { type: 'function' as const, function: { name: 'canon_search', description: 'Search your canon for words (e.g. a file path or symbol in the master index). Returns matching lines with line numbers.', parameters: { type: 'object', properties: { query: { type: 'string' }, id: { ...ID_PARAM, description: 'Limit to one document (optional).' }, limit: { type: 'integer', minimum: 1, maximum: 60 } }, required: ['query'], additionalProperties: false } } },
];

export function runCanonTool(name: string, raw: string, dir = CANON_DIR): string | null {
  if (name !== 'canon_read' && name !== 'canon_search') return null;
  let args: Record<string, unknown> = {};
  try { args = JSON.parse(raw || '{}') as Record<string, unknown>; } catch { /* empty */ }
  const docs = readManifest(dir)?.documents ?? [];
  if (name === 'canon_read') {
    const id = String(args.id ?? '');
    const text = canonText(id, dir);
    if (text === null) return `no canon document "${id}". Known: ${docs.map((d) => d.id).join(', ')}`;
    const all = text.split('\n');
    const from = Math.max(1, Number(args.from_line) || 1);
    const count = Math.min(800, Math.max(1, Number(args.lines) || 400));
    const slice = all.slice(from - 1, from - 1 + count).join('\n');
    return `[${id} lines ${from}-${Math.min(all.length, from + count - 1)} of ${all.length}]\n${slice}`.slice(0, 24_000);
  }
  const terms = String(args.query ?? '').toLowerCase().split(/\s+/u).filter(Boolean);
  if (terms.length === 0) return 'the search needs a query';
  const limit = Math.min(60, Math.max(1, Number(args.limit) || 20));
  const hits: string[] = [];
  for (const doc of docs.filter((d) => !args.id || d.id === args.id)) {
    const text = canonText(doc.id, dir);
    if (!text) continue;
    const lines = text.split('\n');
    for (let i = 0; i < lines.length && hits.length < limit; i += 1) {
      const line = lines[i]!.toLowerCase();
      if (terms.every((t) => line.includes(t))) hits.push(`${doc.id}:${i + 1}: ${lines[i]!.trim().slice(0, 300)}`);
    }
  }
  return hits.length ? hits.join('\n') : 'no matches';
}

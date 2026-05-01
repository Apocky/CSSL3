#!/usr/bin/env node
// snapshot-specs.js
// Build-time hermetic snapshot of ../specs/grand-vision/*.csl into
// lib/specs-snapshot.ts. Vercel deploys only cssl-edge/, so getStaticProps
// cannot read parent-directory files at build-time on the deploy host. This
// script copies-by-value into a TS module that ships as part of cssl-edge.
//
// Output : cssl-edge/lib/specs-snapshot.ts
// Re-runs are idempotent · safe to call as `prebuild` + `predev`.

const fs = require('fs');
const path = require('path');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const SPECS_DIR = path.join(REPO_ROOT, 'specs', 'grand-vision');
const OUT_FILE = path.resolve(__dirname, '..', 'lib', 'specs-snapshot.ts');

function readSpecs() {
  if (!fs.existsSync(SPECS_DIR)) {
    console.warn(`[snapshot-specs] missing ${SPECS_DIR} · emitting empty snapshot`);
    return [];
  }
  const entries = fs
    .readdirSync(SPECS_DIR)
    .filter((f) => f.endsWith('.csl'))
    .sort();
  return entries.map((filename) => {
    const slug = filename.replace(/\.csl$/, '');
    const fullPath = path.join(SPECS_DIR, filename);
    const body = fs.readFileSync(fullPath, 'utf8');
    const firstLine = body.split('\n').find((l) => l.trim().length > 0) ?? slug;
    const title = firstLine.replace(/^[#§\s]+/, '').slice(0, 120).trim() || slug;
    return { slug, filename, title, body };
  });
}

function escapeForTemplateLiteral(s) {
  return s.replace(/\\/g, '\\\\').replace(/`/g, '\\`').replace(/\$\{/g, '\\${');
}

function buildOutput(specs) {
  const lines = [];
  lines.push('// AUTO-GENERATED · do NOT edit by hand · regenerate via `npm run snapshot:specs`');
  lines.push('// Source : ../specs/grand-vision/*.csl @ build-time');
  lines.push('// Hermetic snapshot — Vercel-deploy-friendly · no parent-dir fs reads at runtime.');
  lines.push('');
  lines.push('export interface SpecEntry {');
  lines.push('  slug: string;');
  lines.push('  filename: string;');
  lines.push('  title: string;');
  lines.push('  body: string;');
  lines.push('}');
  lines.push('');
  lines.push('export const SPECS: ReadonlyArray<SpecEntry> = [');
  for (const s of specs) {
    lines.push('  {');
    lines.push(`    slug: ${JSON.stringify(s.slug)},`);
    lines.push(`    filename: ${JSON.stringify(s.filename)},`);
    lines.push(`    title: ${JSON.stringify(s.title)},`);
    lines.push('    body: `' + escapeForTemplateLiteral(s.body) + '`,');
    lines.push('  },');
  }
  lines.push('];');
  lines.push('');
  lines.push('export function findSpec(slug: string): SpecEntry | null {');
  lines.push('  return SPECS.find((s) => s.slug === slug) ?? null;');
  lines.push('}');
  lines.push('');
  return lines.join('\n');
}

function main() {
  const specs = readSpecs();
  const out = buildOutput(specs);
  fs.mkdirSync(path.dirname(OUT_FILE), { recursive: true });
  fs.writeFileSync(OUT_FILE, out, 'utf8');
  console.log(`[snapshot-specs] wrote ${specs.length} specs → ${path.relative(REPO_ROOT, OUT_FILE)}`);
}

main();

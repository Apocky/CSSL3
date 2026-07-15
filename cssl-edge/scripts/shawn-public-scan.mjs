import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const repo = resolve(import.meta.dirname, '..');
const sourceTargets = [
  'lib/shawn/atlas.ts',
  'lib/shawn/catalog.ts',
  'pages/shawn.tsx',
  'pages/shawn/reference',
  'components/shawn',
];

const forbidden = [
  ['local Windows path', /[A-Za-z]:\\(?:Users|Documents|Downloads|source)\\/i],
  ['private attachment identifier', /bf3ebc15-e3e5-4be7-aa14-c81479b80b1c/i],
  ['restricted clinical artifact name', /PSYCHIATRIST_SUMMARY_ARTIFACT/i],
  ['private evidence database name', /SHAWN_APOCKY_EVIDENCE\.sqlite/i],
  ['private allowlist address', /apocky13@gmail\.com/i],
  ['service-role credential name', /SUPABASE_SERVICE_ROLE_KEY/i],
  ['probable live secret', /(?:sk_live_|sb_secret_)[A-Za-z0-9_-]{12,}/],
];

function filesUnder(path) {
  if (!existsSync(path)) return [];
  if (!statSync(path).isDirectory()) return [path];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = resolve(path, entry.name);
    return entry.isDirectory() ? filesUnder(child) : [child];
  });
}

const sourceFiles = sourceTargets
  .flatMap((target) => filesUnder(resolve(repo, target)))
  .filter((file) => !file.replaceAll('\\', '/').includes('/clinical'));

const builtPageFiles = [
  ...filesUnder(resolve(repo, '.next/static/chunks/pages')),
  ...filesUnder(resolve(repo, '.next/server/pages')),
];
const isShawnBuildFile = (file) => /\/pages\/shawn(?:[-/.]|$)/i.test(file.replaceAll('\\', '/'));
const shawnBuildFiles = builtPageFiles.filter(isShawnBuildFile);
const publicBuildFiles = shawnBuildFiles.filter((file) => !/\/clinical(?:[-/.]|$)/i.test(file.replaceAll('\\', '/')));
const clinicalClientFiles = shawnBuildFiles.filter((file) => {
  const normalized = file.replaceAll('\\', '/');
  return normalized.includes('/.next/static/') && /\/clinical(?:[-/.]|$)/i.test(normalized);
});
const files = [...sourceFiles, ...publicBuildFiles];

if (files.length === 0) {
  throw new Error('public scan found no atlas source or build files');
}
if (!publicBuildFiles.some((file) => /\/static\/chunks\/pages\/shawn-[^/]+\.js$/i.test(file.replaceAll('\\', '/')))) {
  throw new Error('public scan found no compiled main /shawn client chunk');
}
if (!publicBuildFiles.some((file) => /\/server\/pages\/shawn\.js$/i.test(file.replaceAll('\\', '/')))) {
  throw new Error('public scan found no compiled main /shawn server page');
}
if (clinicalClientFiles.length === 0) {
  throw new Error('public scan found no clinical client chunk to inspect');
}

const findings = [];
for (const file of files) {
  const bytes = readFileSync(file);
  const text = bytes.toString('utf8');
  for (const [label, pattern] of forbidden) {
    if (pattern.test(text)) findings.push(`${label}: ${file}`);
  }
}

const clinicalServerOnly = [
  ['service-role environment name', /SUPABASE_SERVICE_ROLE_KEY/i],
  ['clinical allowlist table', /shawn_clinical_allowlist/i],
  ['clinical object environment name', /SHAWN_CLINICAL_OBJECT/i],
  ['clinical hash environment name', /SHAWN_CLINICAL_SHA256/i],
  ['private storage resolver', /resolveClinicalRoute/i],
  ['configured service client', /configuredServiceClient/i],
];
for (const file of clinicalClientFiles) {
  const text = readFileSync(file, 'utf8');
  for (const [label, pattern] of clinicalServerOnly) {
    if (pattern.test(text)) findings.push(`${label} leaked to clinical client: ${file}`);
  }
}

if (findings.length > 0) {
  throw new Error(`public scan failed\n${findings.join('\n')}`);
}

console.log(`shawn-public-scan : OK · ${files.length} public files + ${clinicalClientFiles.length} clinical client chunks`);

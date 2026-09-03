import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const read = (relative: string) => fs.readFileSync(path.join(root, relative), 'utf8');

const page = read('pages/atlas.tsx');
const component = read('components/atlas/ConstellationAtlas.tsx');
const graph = read('lib/public-surface-graph.ts');
const nextConfig = read('next.config.js');
const vercel = read('vercel.json');

assert.match(page, /<title>Constellation Atlas — Explore Apocky<\/title>/);
assert.match(page, /canonical" href="https:\/\/www\.apocky\.com\/atlas"/);
assert.match(page, /application\/ld\+json/);
assert.match(component, /Constellation\s*<span>Atlas<\/span>/);
assert.match(component, /Map/);
assert.match(component, /Index/);
assert.match(component, /Dictionary/);
assert.match(component, /aria-labelledby="constellation-title constellation-description"/);
assert.match(component, /aria-label="Atlas destinations"/);
assert.match(component, /target="_blank" rel="noopener noreferrer"/);
assert.match(component, /Nothing is sent there unless you choose the handoff/);
assert.match(component, /ATLAS_EMPTY_STATE/);
assert.match(graph, /id: 'membership'/);
assert.match(graph, /id: 'start'/);
assert.match(graph, /id: 'quests'/);
assert.match(graph, /id: 'status'/);
assert.match(graph, /id: 'now'/);
assert.match(graph, /id: 'labs'/);
assert.match(graph, /id: 'documentation'/);
assert.match(graph, /id: 'memory-tools'/);
assert.match(graph, /id: 'divination'/);
assert.match(graph, /id: 'theory-of-everything'/);
assert.match(graph, /summary: 'A public guide to the active Patreon, Ko-fi, and Chaos Tarot paths/);
assert.match(graph, /href: '\/docs\/cssl-language'/);
assert.match(graph, /href: '\/words#symbols'/);
assert.doesNotMatch(`${page}\n${component}\n${graph}`, /(?:from|import\()[^\n]*\/shawn/i);
assert.doesNotMatch(nextConfig, /source:\s*'\/atlas'[^\n]*commons\/atlas\.html/);
assert.doesNotMatch(vercel, /"source":\s*"\/atlas"[^\n]*commons\/atlas\.html/);
assert.ok(fs.existsSync(path.join(root, 'public/commons/atlas.html')), 'static Atlas must remain as the rollback artifact');

console.log('native Atlas exposes accessible map, index, dictionary, relays, and typed recovery without candidate-only imports');

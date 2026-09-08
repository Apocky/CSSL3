import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { PUBLIC_SURFACE_NODES } from '@/lib/public-surface-graph';

const read = (file: string): string => readFileSync(resolve(process.cwd(), file), 'utf8');

const page = read('pages/memory-tools.tsx');
const experience = read('components/memory/MemoryExperience.tsx');
const help = read('components/ui/HelpTip.tsx');
const atlas = read('components/atlas/ConstellationAtlas.tsx');
const shell = read('components/SiteShell.tsx');
const middleware = read('middleware.ts');

assert.match(page, /<MemoryExperience \/>/, 'memory page must render the task-first experience');
assert.match(experience, /aria-label="Breadcrumb"/, 'memory page must provide breadcrumbs');
assert.match(experience, /PUBLIC MEMORY/, 'public memory must have a visible contract');
assert.match(experience, /DEVICE-LOCAL MEMORY/, 'device-local memory must have a visible contract');
assert.match(experience, /SIGNED-IN PRIVATE MNEME/, 'private Mneme must have a visible contract');
assert.match(experience, /readLocalSpellbook\(window\.localStorage\)/, 'local preview must read the existing Spellbook store');
assert.match(experience, /reports the exact current denominator/, 'the conversation route must own its changing public denominator');
assert.match(experience, /href="\/conversations"/, 'memory index must route to the curated conversation surface');
assert.match(experience, /remain local\/private candidates until/, 'unpublished provider exports must stay explicitly private until admitted');
assert.match(experience, /AUTHORED ESSAY · SHAWN APOCKY/, 'authored essays must be distinguished from dialogue');
assert.match(experience, /DIALOGUE · USER AND AI ROLES LABELED/, 'dialogue must retain role attribution');
assert.match(experience, /\/api\/mneme\/me\/(?:health|list|remember|recall|forget|export)/, 'UI must use only the server-bound me profile');
assert.doesNotMatch(experience, /\/api\/mneme\/\$\{|profile_id\s*:/, 'UI must never select or submit a profile id');
assert.match(experience, /PROFILE NOT PROVISIONED/, 'signed-in missing-profile state must remain truthful');
assert.match(experience, /No profile was created automatically/, 'missing profile must not trigger implicit provisioning');
assert.match(experience, /Save correction/, 'topic-bound correction flow must be exposed');
assert.match(experience, /Export my data/, 'private export must be exposed when authorized');
assert.match(experience, /access === 'owner'.*href="\/brain"/, 'owner memory tools must reveal the private Brain contextually');
assert.match(experience, /Yes, delete it/, 'private forgetting must require explicit confirmation');

assert.match(help, /aria-expanded=\{open\}/, 'help trigger must expose its state');
assert.match(help, /aria-controls=\{id\}/, 'help trigger must own the tooltip');
assert.match(help, /event\.key === 'Escape'/, 'help must close from the keyboard');
assert.match(help, /role="tooltip"/, 'help text must have tooltip semantics');

assert.match(atlas, /What are you looking for\?/, 'Atlas must lead with a task-first search');
assert.match(atlas, /filterPublicSurfaceNodes/, 'Atlas must show the shared destination catalogue');
assert.match(atlas, /<DestinationLink node=\{node\}/, 'directory results must offer direct destination links');
assert.ok(PUBLIC_SURFACE_NODES.some(node => node.href === '/memory-tools'), 'Atlas catalogue must route into the memory contract');
assert.ok(PUBLIC_SURFACE_NODES.some(node => node.href === '/conversations'), 'Atlas catalogue must route into curated conversation views');
assert.match(shell, /aria-label="Apocky home"/, 'the shared navigation must provide a named return home');

assert.match(middleware, /BROKERED_MEMBER_MEMORY_PATH/, 'middleware must name the member memory allowlist');
assert.match(middleware, /health\|list\|remember\|recall\|forget\|export/, 'only reviewed member operations may pass');
assert.doesNotMatch(middleware.match(/BROKERED_MEMBER_MEMORY_PATH[^;]+/)?.[0] ?? '', /ingest|smoke/, 'bulk ingest and smoke must remain private');

console.log('memory-tools.test : OK · task-first layers + brokered Mneme + human archive routes');

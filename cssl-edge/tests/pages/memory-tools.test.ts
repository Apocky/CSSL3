import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (file: string): string => readFileSync(resolve(process.cwd(), file), 'utf8');

const page = read('pages/memory-tools.tsx');
const experience = read('components/memory/MemoryExperience.tsx');
const help = read('components/ui/HelpTip.tsx');
const atlas = read('components/atlas/ConstellationAtlas.tsx');
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
assert.match(experience, /Yes, delete it/, 'private forgetting must require explicit confirmation');

assert.match(help, /aria-expanded=\{open\}/, 'help trigger must expose its state');
assert.match(help, /aria-controls=\{id\}/, 'help trigger must own the tooltip');
assert.match(help, /event\.key === 'Escape'/, 'help must close from the keyboard');
assert.match(help, /role="tooltip"/, 'help text must have tooltip semantics');

assert.match(atlas, /What brought you here\?/, 'Atlas must lead with task-first paths');
assert.match(atlas, /href="\/memory-tools"/, 'Atlas must route into the memory contract');
assert.match(atlas, /href="\/conversations"/, 'Atlas must route into curated conversation views');
assert.match(atlas, /aria-label="Breadcrumb"/, 'Atlas must provide breadcrumbs');

assert.match(middleware, /BROKERED_MEMBER_MEMORY_PATH/, 'middleware must name the member memory allowlist');
assert.match(middleware, /health\|list\|remember\|recall\|forget\|export/, 'only reviewed member operations may pass');
assert.doesNotMatch(middleware.match(/BROKERED_MEMBER_MEMORY_PATH[^;]+/)?.[0] ?? '', /ingest|smoke/, 'bulk ingest and smoke must remain private');

console.log('memory-tools.test : OK · task-first layers + brokered Mneme + human archive routes');

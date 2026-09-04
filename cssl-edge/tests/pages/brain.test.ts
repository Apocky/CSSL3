import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string): string => readFileSync(resolve(process.cwd(), path), 'utf8');

const page = read('pages/brain.tsx');
const experience = read('components/brain/BrainExperience.tsx');
const owner = read('lib/brain/owner.ts');
const snapshot = read('pages/api/brain/snapshot.ts');
const runtimeProvider = read('lib/brain/runtime-provider.ts');
const runtimeProxy = read('lib/apocv4/runtime-proxy.ts');
const middleware = read('middleware.ts');
const telemetry = read('lib/akashic-telemetry/client.ts');
const home = read('pages/index.tsx');
const shell = read('components/SiteShell.tsx');

assert.match(page, /getServerSideProps/, 'Brain page must authorize before render');
assert.match(page, /requireBrainOwner/, 'Brain page must use the owner allowlist boundary');
assert.match(page, /private, no-store/, 'Brain document must be private and non-cacheable');
assert.match(page, /noindex,nofollow,noarchive,nosnippet/, 'Brain page must be crawler-dark');
assert.match(owner, /getAdminAuthorization/, 'Brain APIs must derive authority from the server session');
assert.match(owner, /BRAIN_OWNER_REQUIRED/, 'non-owner identities must have a stable denial code');
assert.match(snapshot, /deriveMemberProfileId\(owner\.user\.id\)/, 'snapshot profile must be derived server-side');
assert.doesNotMatch(snapshot, /profile_id:/, 'snapshot must not emit a profile identifier');
assert.match(snapshot, /getMessagesByIds/, 'source references must resolve through the private store');
assert.match(experience, /CONTEXTUAL RECALL · NO MODEL CALL/, 'contextual recall must disclose its deterministic path');
assert.match(experience, /BrainGraph/, 'graph projection missing');
assert.match(experience, /Timeline/, 'timeline projection missing');
assert.match(experience, /Tunnel/, 'source tunnel missing');
assert.match(experience, /sourceMessages\(memory, messages\)/, 'tunnel must bind memory source IDs to message records');
assert.doesNotMatch(experience, /\/api\/mneme\//, 'browser must use only owner-authorized Brain APIs');
assert.doesNotMatch(experience, /ANTHROPIC|VOYAGE|retrievePipeline/, 'new Brain turns must not invoke paid model paths');
assert.match(runtimeProvider, /submitOwnerBrainRuntimeChat/, 'conversation must use the validated local runtime adapter');
assert.match(runtimeProxy, /APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED/, 'local runtime needs an explicit scoped enable gate');
assert.match(runtimeProxy, /owner-brain/, 'runtime bypass must be scoped to the Brain adapter');
assert.match(middleware, /request\.nextUrl\.pathname === '\/brain'/, 'middleware must recognize the private page');
assert.match(telemetry, /pathname === '\/brain'/, 'private page must be a telemetry blackout');
assert.match(home, /access === 'owner'[\s\S]*?href="\/brain"/, 'home must reveal Brain only after owner authorization');
assert.match(shell, /access === 'owner'.*href="\/brain"/, 'shell must reveal Brain only after owner authorization');

console.log('brain-page.test : OK · owner gate + real private projections + local-only provider seam');

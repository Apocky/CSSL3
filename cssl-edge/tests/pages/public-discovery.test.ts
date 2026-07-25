import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';

const root = process.cwd();
const read = (relative: string) => fs.readFileSync(path.join(root, relative), 'utf8');

const manifest = JSON.parse(read('public/.well-known/apocky.json')) as Record<string, unknown>;
const rootManifest = JSON.parse(read('public/apocrypha-manifest.json')) as Record<string, unknown>;
const schema = JSON.parse(read('public/schemas/site-manifest.v1.json')) as Record<string, unknown>;
const pwa = JSON.parse(read('public/manifest.json')) as Record<string, unknown>;
const llms = read('public/llms.txt');
const robots = read('public/robots.txt');
const sitemap = read('public/sitemap.xml');

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const validate = ajv.compile(schema);
assert.equal(validate(manifest), true, JSON.stringify(validate.errors, null, 2));
assert.deepEqual(rootManifest, manifest, 'root manifest alias must remain byte-semantic equivalent to the canonical well-known manifest');

assert.equal(manifest['declared_release_state'], 'private_beta');
assert.equal(pwa['start_url'], '/', 'public PWA must not enter the owner-only admin shell');
assert.match(llms, /account alone\s+does not grant conversation/i);
assert.match(llms, /owner-authorized/i);
assert.match(robots, /Disallow: \/admin\//);
assert.match(robots, /Disallow: \/api\//);
assert.match(sitemap, /https:\/\/www\.apocky\.com\//);
assert.doesNotMatch(sitemap, /\/admin|\/api|\/account|\/login|\/register|\/chat/);

console.log('✓ public discovery manifest, schema, PWA, robots, sitemap, and LLM guidance agree');

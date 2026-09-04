import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string): string => readFileSync(resolve(process.cwd(), path), 'utf8');
const experience = read('components/brain/BrainExperience.tsx');

assert.match(experience, /id="brain-releases"/, 'Brain must expose the owner release shelf anchor');
assert.match(experience, /manifest\.documents\.plan/, 'release shelf must link the public-safe plan');
assert.match(experience, /manifest\.documents\.changelog/, 'release shelf must link the public-safe changelog');
assert.match(experience, /manifest\.documents\.manifest/, 'release shelf must link the build manifest');
assert.match(experience, /publicReleaseDownload\(manifest\)/, 'download visibility must use the release gate');
assert.match(experience, /No promoted public package is attached/, 'candidate state must be explicit');
assert.doesNotMatch(experience, /C:\\Users|127\.0\.0\.1|localhost/, 'release UI cannot expose owner-local coordinates');

console.log('brain-release-page.test : OK · owner shelf + evidence links + gated download seam');

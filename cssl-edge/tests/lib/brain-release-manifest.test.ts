import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';

import {
  apocryphaRelease,
  parseApocryphaReleaseManifest,
  publicReleaseDownload,
} from '@/lib/brain/release-manifest';

const root = process.cwd();
const manifestPath = resolve(root, 'public/releases/apocrypha-living/manifest.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as Record<string, unknown>;
const schema = JSON.parse(readFileSync(resolve(root, 'public/schemas/apocrypha-release-manifest.v1.json'), 'utf8')) as object;
const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
assert.equal(ajv.validate(schema, manifest), true, JSON.stringify(ajv.errors));

assert.equal(apocryphaRelease.status, 'live', 'checked-in manifest must parse');
if (apocryphaRelease.status !== 'live') throw new Error('checked-in release manifest degraded');
assert.equal(apocryphaRelease.manifest.release_state, 'CANDIDATE');
assert.equal(apocryphaRelease.manifest.release_label, 'Candidate — not released');
assert.equal(apocryphaRelease.manifest.version, '1.0.0-rc.1');
assert.equal(apocryphaRelease.manifest.build.state, 'INSTALLABLE_PWA_CANDIDATE');
assert.equal(apocryphaRelease.manifest.build.verification, 'LOCAL_DESKTOP_CHROME_MOBILE_CHROME_IPHONE_WEBKIT_MATRIX_PASSED');
assert.match(apocryphaRelease.manifest.claim_boundary, /Browser installation is distinct from a downloadable native artifact/);
assert.equal(apocryphaRelease.manifest.download, null);
assert.equal(apocryphaRelease.manifest.download_status, 'NO_PROMOTED_ARTIFACT');
assert.equal(publicReleaseDownload(apocryphaRelease.manifest), null);
const missingGates = apocryphaRelease.manifest.build.missing.join(' · ');
assert.match(missingGates, /Physical iPhone installation/);
assert.match(missingGates, /Physical Android installation/);
assert.match(missingGates, /Production promotion approval/);
assert.doesNotMatch(missingGates, /Promoted native installer/, 'native packaging is not a gate for the 1.0 web/PWA lane');

for (const binding of [
  apocryphaRelease.manifest.documents.plan,
  apocryphaRelease.manifest.documents.changelog,
]) {
  const bytes = readFileSync(resolve(root, 'public', binding.href.slice(1)));
  assert.equal(bytes.length, binding.bytes, `${binding.label} byte binding drifted`);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), binding.sha256, `${binding.label} digest drifted`);
}

const serialized = JSON.stringify(manifest);
assert.doesNotMatch(serialized, /[a-z]:[\\/]/i, 'manifest cannot expose an absolute local path');
assert.doesNotMatch(serialized, /localhost|127\.0\.0\.1/i, 'manifest cannot expose owner-local endpoints');
assert.doesNotMatch(serialized, /\b(?:bearer|credential|password|service[_-]?role[_-]?key)\b/i, 'manifest cannot expose secret-bearing fields');
assert.doesNotMatch(serialized, /\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/i, 'manifest cannot expose private turn UUIDs');

const forgedCandidate = structuredClone(manifest);
forgedCandidate.download_status = 'RELEASED_ARTIFACT_PRESENT';
forgedCandidate.download = {
  filename: 'not-a-release.zip',
  href: '/downloads/not-a-release.zip',
  media_type: 'application/zip',
  platform: 'windows-x64',
  bytes: 1,
  sha256: 'a'.repeat(64),
  sha256_href: '/downloads/not-a-release.zip.sha256',
  signature: {
    href: '/downloads/not-a-release.zip.sig',
    sha256: 'b'.repeat(64),
    verifier: 'fixture',
    key_fingerprint: 'fixture',
    receipt_href: '/downloads/not-a-release.signature.json',
  },
};
assert.throws(
  () => parseApocryphaReleaseManifest(forgedCandidate),
  /RELEASE_MANIFEST_INVALID/,
  'candidate metadata cannot grow a public download link',
);

console.log('brain-release-manifest.test : OK · public-safe bindings + candidate gate + no phantom download');

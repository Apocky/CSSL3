import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import { buildReleaseBundle } from '../scripts/snapshot-apocrypha-release.mjs';

const source = JSON.parse(readFileSync(resolve('data/apocrypha-living-release.v1.json'), 'utf8'));
const temporary = mkdtempSync(join(tmpdir(), 'apocky-release-manifest-'));
const downloads = join(temporary, 'downloads');
mkdirSync(downloads, { recursive: true });

try {
  assert.equal(buildReleaseBundle(source, temporary).manifest.download, null);

  const payload = Buffer.from('bounded release fixture', 'utf8');
  const artifactDigest = createHash('sha256').update(payload).digest('hex');
  const signature = Buffer.from('detached signature fixture', 'utf8');
  const signatureDigest = createHash('sha256').update(signature).digest('hex');
  const filename = 'Apocrypha-fixture-windows-x64.zip';
  writeFileSync(join(downloads, filename), payload);
  writeFileSync(join(downloads, `${filename}.sha256`), `${artifactDigest}  ${filename}\n`);
  writeFileSync(join(downloads, `${filename}.sig`), signature);
  writeFileSync(join(downloads, `${filename}.signature.json`), JSON.stringify({
    schema: 'apocrypha.artifact-signature-receipt.v1',
    verified: true,
    artifact_sha256: artifactDigest,
    signature_sha256: signatureDigest,
    verifier: 'fixture-verifier',
    key_fingerprint: 'fixture-key-fingerprint',
  }));

  const declared = {
    ...source,
    artifact: {
      filename,
      media_type: 'application/zip',
      platform: 'windows-x64',
      promoted: true,
      signature_filename: `${filename}.sig`,
      signature_receipt_filename: `${filename}.signature.json`,
    },
  };
  const held = buildReleaseBundle(declared, temporary).manifest;
  assert.equal(held.release_state, 'CANDIDATE');
  assert.equal(held.download_status, 'ARTIFACT_HELD_NOT_RELEASED');
  assert.equal(held.download, null, 'artifact bytes alone cannot create a public download');

  const released = buildReleaseBundle({
    ...declared,
    release_state: 'RELEASED',
    build: { ...declared.build, state: 'RELEASED', release_gate: 'OPEN', missing: [] },
  }, temporary).manifest;
  assert.equal(released.download_status, 'RELEASED_ARTIFACT_PRESENT');
  assert.equal(released.download?.href, `/downloads/${filename}`);
  assert.equal(released.download?.sha256, artifactDigest);
  assert.equal(released.download?.signature.sha256, signatureDigest);

  writeFileSync(join(downloads, `${filename}.signature.json`), JSON.stringify({
    schema: 'apocrypha.artifact-signature-receipt.v1',
    verified: true,
    artifact_sha256: 'f'.repeat(64),
    signature_sha256: signatureDigest,
    verifier: 'fixture-verifier',
    key_fingerprint: 'fixture-key-fingerprint',
  }));
  const tampered = buildReleaseBundle({
    ...declared,
    release_state: 'RELEASED',
    build: { ...declared.build, state: 'RELEASED', release_gate: 'OPEN', missing: [] },
  }, temporary).manifest;
  assert.equal(tampered.download, null, 'a mismatched signature receipt closes the download gate');
} finally {
  rmSync(temporary, { recursive: true, force: true });
}

console.log('release-manifest-generator.test : OK · artifact + digest + signature + promotion + release gates');

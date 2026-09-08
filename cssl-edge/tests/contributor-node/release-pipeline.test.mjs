import { strict as assert } from 'node:assert';
import {
  createHash,
  generateKeyPairSync,
  sign,
} from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  CANDIDATE_FILES,
  PROMOTION_GATES,
  buildPromotionManifest,
  canonicalJson,
  createDeterministicArchive,
  evaluatePromotionManifest,
  generateCandidateRelease,
} from '../../contributor-node/scripts/release-artifacts.mjs';
import {
  publicKeyFingerprint,
  verifyDetachedSignature,
} from '../../contributor-node/scripts/verify-detached-signature.mjs';

function testDeterministicArchiveOrderAndMetadata() {
  const source = [
    { name: 'z.txt', data: Buffer.from('z') },
    { name: 'a.txt', data: Buffer.from('a') },
  ];
  const first = createDeterministicArchive(source);
  const second = createDeterministicArchive([...source].reverse());
  assert.equal(first.sha256, second.sha256);
  assert.deepEqual(first.entries, second.entries);
  assert.equal(first.bytes.readUInt32LE(0), 0x04034b50);
  assert.equal(first.bytes.readUInt16LE(10), 0);
  assert.equal(first.bytes.readUInt16LE(12), 33);
  assert.equal(first.bytes.readUInt32LE(first.bytes.length - 22), 0x06054b50);
}

function testDetachedSignatureInterface() {
  const artifact = Buffer.from('exact archive bytes');
  const pair = generateKeyPairSync('ed25519');
  const signature = sign(null, artifact, pair.privateKey).toString('base64url');
  const pem = pair.publicKey.export({ type: 'spki', format: 'pem' });
  const expected = createHash('sha256').update(artifact).digest('hex');
  const verified = verifyDetachedSignature({
    artifact_bytes: artifact,
    detached_signature: signature,
    public_key_pem: pem,
    expected_sha256: expected,
  });
  assert.equal(verified.ok, true);
  assert.equal(verified.algorithm, 'Ed25519');
  assert.equal(verified.signer_public_key_fingerprint, publicKeyFingerprint(pem));
  const tampered = verifyDetachedSignature({
    artifact_bytes: Buffer.from('tampered archive bytes'),
    detached_signature: signature,
    public_key_pem: pem,
    expected_sha256: expected,
  });
  assert.equal(tampered.ok, false);
  assert.equal(tampered.code, 'ARTIFACT_HASH_MISMATCH');
  const missing = verifyDetachedSignature({
    artifact_bytes: artifact,
    detached_signature: null,
    public_key_pem: pem,
    expected_sha256: expected,
  });
  assert.equal(missing.ok, false);
  assert.equal(missing.code, 'SIGNATURE_ENCODING_INVALID');
}

async function testCandidateOutputsReproduce() {
  const input = await mkdtemp(join(tmpdir(), 'apocrypha-release-input-'));
  const outA = await mkdtemp(join(tmpdir(), 'apocrypha-release-a-'));
  const outB = await mkdtemp(join(tmpdir(), 'apocrypha-release-b-'));
  try {
    for (const file of CANDIDATE_FILES) {
      const target = join(input, file);
      const parent = target.slice(0, target.lastIndexOf('\\') >= 0 ? target.lastIndexOf('\\') : target.lastIndexOf('/'));
      await mkdir(parent, { recursive: true });
      const value = file === 'package.json'
        ? JSON.stringify({ name: '@apocky/test', version: '0.1.0-candidate' })
        : `fixture:${file}\n`;
      await writeFile(target, Buffer.from(value));
    }
    const args = { inputDir: input, sourceDateEpoch: 1_700_000_000, sourceCommit: '0123456789abcdef0123456789abcdef01234567' };
    const first = await generateCandidateRelease({ ...args, outDir: outA });
    const second = await generateCandidateRelease({ ...args, outDir: outB });
    assert.deepEqual(first, second);
    for (const key of ['archive', 'sbom', 'provenance', 'promotion']) {
      const name = first[key].name;
      assert.deepEqual(await readFile(join(outA, name)), await readFile(join(outB, name)));
    }
    assert.equal(first.release_state, 'NOT_DEPLOYABLE');
    assert.equal(first.release_gate, 'CLOSED');
    assert.ok(first.missing_gates.includes('detached_signature'));
    assert.ok(first.missing_gates.includes('sandbox_enforced'));
  } finally {
    await Promise.all([rm(input, { recursive: true, force: true }), rm(outA, { recursive: true, force: true }), rm(outB, { recursive: true, force: true })]);
  }
}

function testPromotionCannotBeForgedOpen() {
  const archive = createDeterministicArchive([{ name: 'runtime.js', data: Buffer.from('runtime') }]);
  const metadata = { name: '@apocky/test', version: '0.1.0-candidate', sourceCommit: 'a'.repeat(40), sourceDateEpoch: 1_700_000_000 };
  const sbomFile = { name: 'test.spdx.json', size: 1, sha256: 'b'.repeat(64) };
  const provenanceFile = { name: 'test.provenance.json', size: 1, sha256: 'c'.repeat(64) };
  const manifest = buildPromotionManifest({
    ...metadata,
    archive: { name: 'runtime.zip', size: archive.size, sha256: archive.sha256 },
    sbomFile,
    provenanceFile,
  });
  assert.deepEqual(Object.keys(manifest.gates).sort(), [...PROMOTION_GATES].sort());
  const forged = structuredClone(manifest);
  forged.release_state = 'READY';
  forged.release_gate = 'OPEN';
  forged.production_eligible = true;
  for (const gate of PROMOTION_GATES) forged.gates[gate] = true;
  forged.artifact.detached_signature = 'A'.repeat(86);
  forged.artifact.signing_key_id = 'forged-key';
  forged.artifact.signer_public_key_fingerprint = 'c'.repeat(64);
  const verdict = evaluatePromotionManifest(forged);
  assert.equal(verdict.release_state, 'NOT_DEPLOYABLE');
  assert.equal(verdict.release_gate, 'CLOSED');
  assert.equal(verdict.production_eligible, false);
  assert.ok(verdict.missing_gates.includes('signature_verification_input'));
}

await (async () => {
  testDeterministicArchiveOrderAndMetadata();
  testDetachedSignatureInterface();
  await testCandidateOutputsReproduce();
  testPromotionCannotBeForgedOpen();
  console.log('contributor-node/release-pipeline.test : OK · 4 tests passed');
})();

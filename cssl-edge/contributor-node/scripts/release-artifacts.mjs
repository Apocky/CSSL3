import {
  execFileSync,
} from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

import {
  DETACHED_SIGNATURE_ALGORITHM,
  publicKeyFingerprint,
  verifyDetachedSignature,
} from './verify-detached-signature.mjs';

export const RELEASE_PIPELINE_SCHEMA = 'apocrypha.contributor.release.v1';
export const PRODUCT = 'Apocrypha Mycelial Contributor Node';
export const REPOSITORY = 'https://github.com/Apocky/CSSL3.git';
export const PROMOTION_GATES = Object.freeze([
  'deterministic_archive',
  'sbom',
  'provenance',
  'artifact_sha256',
  'detached_signature',
  'pinned_signer',
  'controller_transport',
  'authenticated_contribution_oracle',
  'native_install_smoke',
  'sandbox_enforced',
  'malware_scan',
  'rollback_proof',
  'mobile_artifacts',
]);

export const CANDIDATE_FILES = Object.freeze([
  'README.md',
  'RELEASE.csl',
  'dist/cli.d.ts',
  'dist/cli.js',
  'dist/runtime.d.ts',
  'dist/runtime.js',
  'package.json',
]);

const DOS_DATE_1980_01_01 = 33;
const ZIP_LOCAL_HEADER_BYTES = 30;
const ZIP_CENTRAL_HEADER_BYTES = 46;
const ZIP_END_HEADER_BYTES = 22;
const SHA256 = /^[0-9a-f]{64}$/;

function fail(message) {
  throw new Error(message);
}

function assertSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value < 0) fail(`${label} must be a non-negative safe integer.`);
  return value;
}

function asBytes(value, label = 'bytes') {
  if (value instanceof Uint8Array || Buffer.isBuffer(value)) return Buffer.from(value);
  fail(`${label} must be bytes.`);
}

function sha256(value) {
  return createHash('sha256').update(asBytes(value)).digest('hex');
}

function validSourceCommit(value) {
  return typeof value === 'string' && /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(value);
}

function crc32(value) {
  const bytes = asBytes(value);
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function u16(value) {
  const bytes = Buffer.alloc(2);
  bytes.writeUInt16LE(value, 0);
  return bytes;
}

function u32(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value >>> 0, 0);
  return bytes;
}

function canonicalize(value) {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) fail('Canonical JSON does not accept non-finite numbers.');
    return Object.is(value, -0) ? 0 : value;
  }
  if (Array.isArray(value)) return value.map(canonicalize);
  if (typeof value === 'object') {
    const source = value;
    return Object.fromEntries(Object.keys(source).sort().map((key) => [key, canonicalize(source[key])]));
  }
  fail('Canonical JSON does not accept this value type.');
}

export function canonicalJson(value) {
  return JSON.stringify(canonicalize(value));
}

function canonicalBytes(value) {
  return Buffer.from(canonicalJson(value), 'utf8');
}

function documentName(name) {
  return name.replace(/[^A-Za-z0-9.-]+/g, '-').replace(/^-+|-+$/g, '') || 'apocrypha-contributor-node';
}

function zipLocalHeader(name, data) {
  const nameBytes = Buffer.from(name, 'utf8');
  const header = Buffer.alloc(ZIP_LOCAL_HEADER_BYTES);
  header.writeUInt32LE(0x04034b50, 0);
  header.writeUInt16LE(20, 4); // version needed
  header.writeUInt16LE(0, 6); // flags: no data descriptor, no UTF-8 ambiguity in our allowlist
  header.writeUInt16LE(0, 8); // stored, not compressed
  header.writeUInt16LE(0, 10); // fixed DOS time
  header.writeUInt16LE(DOS_DATE_1980_01_01, 12); // fixed DOS date
  header.writeUInt32LE(crc32(data), 14);
  header.writeUInt32LE(data.length, 18);
  header.writeUInt32LE(data.length, 22);
  header.writeUInt16LE(nameBytes.length, 26);
  header.writeUInt16LE(0, 28); // no extra fields
  return Buffer.concat([header, nameBytes, data]);
}

function zipCentralHeader(name, data, offset) {
  const nameBytes = Buffer.from(name, 'utf8');
  const header = Buffer.alloc(ZIP_CENTRAL_HEADER_BYTES);
  header.writeUInt32LE(0x02014b50, 0);
  header.writeUInt16LE(20, 4); // fixed creator version
  header.writeUInt16LE(20, 6); // version needed
  header.writeUInt16LE(0, 8); // flags
  header.writeUInt16LE(0, 10); // stored
  header.writeUInt16LE(0, 12); // fixed DOS time
  header.writeUInt16LE(DOS_DATE_1980_01_01, 14); // fixed DOS date
  header.writeUInt32LE(crc32(data), 16);
  header.writeUInt32LE(data.length, 20);
  header.writeUInt32LE(data.length, 24);
  header.writeUInt16LE(nameBytes.length, 28);
  header.writeUInt16LE(0, 30); // no extra
  header.writeUInt16LE(0, 32); // no comment
  header.writeUInt16LE(0, 34); // disk number
  header.writeUInt16LE(0, 36); // internal attributes
  header.writeUInt32LE(0, 38); // external attributes
  header.writeUInt32LE(offset, 42);
  return Buffer.concat([header, nameBytes]);
}

function zipEnd(entryCount, centralBytes, centralOffset) {
  const header = Buffer.alloc(ZIP_END_HEADER_BYTES);
  header.writeUInt32LE(0x06054b50, 0);
  header.writeUInt16LE(0, 4); // current disk
  header.writeUInt16LE(0, 6); // central directory disk
  header.writeUInt16LE(entryCount, 8);
  header.writeUInt16LE(entryCount, 10);
  header.writeUInt32LE(centralBytes, 12);
  header.writeUInt32LE(centralOffset, 16);
  header.writeUInt16LE(0, 20); // no comment
  return header;
}

function normalizeArchiveEntries(entries) {
  const normalized = entries.map((entry) => {
    if (!entry || typeof entry.name !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._/-]{0,180}$/.test(entry.name)) {
      fail('Archive entry name is invalid.');
    }
    if (entry.name.startsWith('/') || entry.name.includes('..') || entry.name.includes('//')) {
      fail('Archive entry path escapes the package root.');
    }
    const data = asBytes(entry.data, `Archive entry ${entry.name}`);
    if (data.length > 0xffffffff) fail('Archive entry is too large for deterministic ZIP32.');
    return { name: entry.name, data };
  });
  normalized.sort((left, right) => left.name.localeCompare(right.name, 'en'));
  for (let index = 1; index < normalized.length; index += 1) {
    if (normalized[index - 1].name === normalized[index].name) fail(`Duplicate archive entry: ${normalized[index].name}`);
  }
  return normalized;
}

/** Build a ZIP32 archive with fixed timestamps, order, flags, and attributes. */
export function createDeterministicArchive(entries) {
  const normalized = normalizeArchiveEntries(entries);
  if (normalized.length > 0xffff) fail('Archive has too many entries for deterministic ZIP32.');
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const entry of normalized) {
    const local = zipLocalHeader(entry.name, entry.data);
    locals.push(local);
    centrals.push(zipCentralHeader(entry.name, entry.data, offset));
    offset += local.length;
  }
  const central = Buffer.concat(centrals);
  const archive = Buffer.concat([...locals, central, zipEnd(normalized.length, central.length, offset)]);
  if (archive.length > 0xffffffff) fail('Archive is too large for deterministic ZIP32.');
  return {
    bytes: archive,
    size: archive.length,
    sha256: sha256(archive),
    entries: normalized.map((entry) => ({
      name: entry.name,
      bytes: entry.data.length,
      sha256: sha256(entry.data),
    })),
  };
}

function isoEpoch(epoch) {
  assertSafeInteger(epoch, 'source_date_epoch');
  return new Date(epoch * 1000).toISOString();
}

function sbom({ name, version, files, sourceDateEpoch }) {
  const packageId = 'SPDXRef-Package-ApocryphaContributorNode';
  return {
    spdxVersion: 'SPDX-2.3',
    dataLicense: 'CC0-1.0',
    SPDXID: 'SPDXRef-DOCUMENT',
    name,
    documentNamespace: `https://apocky.com/provenance/${documentName(name)}.spdx`,
    creationInfo: {
      created: isoEpoch(sourceDateEpoch),
      creators: ['Tool: apocrypha-contributor-release-pipeline-v1'],
    },
    packages: [{
      SPDXID: packageId,
      name,
      versionInfo: version,
      downloadLocation: 'NOASSERTION',
      filesAnalyzed: true,
      licenseConcluded: 'NOASSERTION',
      licenseDeclared: 'NOASSERTION',
    }],
    files: files.map((file, index) => ({
      SPDXID: `SPDXRef-File-${String(index + 1).padStart(3, '0')}`,
      fileName: file.name,
      checksums: [{ algorithm: 'SHA256', checksumValue: file.sha256 }],
      licenseConcluded: 'NOASSERTION',
      licenseInfoInFiles: ['NOASSERTION'],
    })),
    relationships: [{
      spdxElementId: 'SPDXRef-DOCUMENT',
      relationshipType: 'DESCRIBES',
      relatedSpdxElement: packageId,
    }],
  };
}

function provenance({ name, version, files, archive, sbomFile, sourceCommit, sourceDateEpoch }) {
  const materials = files.map((file) => ({
    uri: `workspace:${file.name}`,
    digest: { sha256: file.sha256 },
  }));
  if (sourceCommit) {
    materials.push({ uri: REPOSITORY, digest: { gitCommit: sourceCommit } });
  }
  return {
    _type: 'https://in-toto.io/Statement/v1',
    subject: [
      { name: archive.name, digest: { sha256: archive.sha256 }, size: archive.size },
      { name: sbomFile.name, digest: { sha256: sbomFile.sha256 }, size: sbomFile.size },
    ],
    predicateType: 'https://slsa.dev/provenance/v1',
    predicate: {
      buildDefinition: {
        buildType: 'https://apocky.com/build/apocrypha-contributor-node/v1',
        externalParameters: {
          package: name,
          version,
          source_date_epoch: sourceDateEpoch,
        },
        internalParameters: {
          archive_format: 'zip32-store-fixed-metadata',
          signature_scope: 'exact_archive_bytes',
          signature_algorithm: DETACHED_SIGNATURE_ALGORITHM,
        },
        resolvedDependencies: materials,
      },
      runDetails: {
        builder: { id: 'https://apocky.com/builders/apocrypha-contributor-release-pipeline-v1' },
        metadata: {
          invocationId: `${name}@${version}+${sourceCommit ?? 'unresolved'}`,
          startedOn: isoEpoch(sourceDateEpoch),
          finishedOn: isoEpoch(sourceDateEpoch),
        },
      },
    },
  };
}

function gateMap({ sourceCommit, sourceDateEpoch }) {
  return {
    deterministic_archive: true,
    sbom: true,
    provenance: validSourceCommit(sourceCommit) && Number.isSafeInteger(sourceDateEpoch),
    artifact_sha256: true,
    detached_signature: false,
    pinned_signer: false,
    controller_transport: false,
    authenticated_contribution_oracle: false,
    native_install_smoke: false,
    sandbox_enforced: false,
    malware_scan: false,
    rollback_proof: false,
    mobile_artifacts: false,
  };
}

/**
 * Produce the internal promotion contract. It is never READY by construction:
 * no publisher key or external gate assertions are accepted by this generator.
 */
export function buildPromotionManifest({ name, version, archive, sbomFile, provenanceFile, sourceCommit, sourceDateEpoch }) {
  const gates = gateMap({ sourceCommit, sourceDateEpoch });
  const missing = PROMOTION_GATES.filter((gate) => gates[gate] !== true);
  return {
    schema_version: RELEASE_PIPELINE_SCHEMA,
    product: PRODUCT,
    package: name,
    version,
    release_state: 'NOT_DEPLOYABLE',
    release_gate: 'CLOSED',
    production_eligible: false,
    artifact: {
      kind: 'deterministic_zip32',
      filename: archive.name,
      bytes: archive.size,
      sha256: archive.sha256,
      signature_algorithm: DETACHED_SIGNATURE_ALGORITHM,
      signature_scope: 'exact_archive_bytes',
      detached_signature: null,
      signing_key_id: null,
      signer_public_key_fingerprint: null,
    },
    sbom: {
      filename: sbomFile.name,
      bytes: sbomFile.size,
      sha256: sbomFile.sha256,
      format: 'SPDX-2.3',
    },
    provenance: {
      filename: provenanceFile.name,
      bytes: provenanceFile.size,
      sha256: provenanceFile.sha256,
      format: 'in-toto.statement.v1',
      source_repository: REPOSITORY,
      source_commit: sourceCommit ?? null,
      source_date_epoch: sourceDateEpoch,
    },
    gates,
    missing_gates: missing,
    unsigned_candidate_download: 'blocked',
    generated_by: 'apocrypha-contributor-release-pipeline-v1',
  };
}

function manifestGateShape(manifest) {
  if (!manifest || typeof manifest !== 'object'
    || manifest.schema_version !== RELEASE_PIPELINE_SCHEMA
    || manifest.product !== PRODUCT
    || typeof manifest.version !== 'string'
    || !/^\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?$/.test(manifest.version)
    || !manifest.gates || typeof manifest.gates !== 'object') return false;
  return PROMOTION_GATES.every((gate) => typeof manifest.gates[gate] === 'boolean');
}

/**
 * Re-evaluate a promotion manifest at the release boundary. The result is
 * fail-closed even when a caller forges `READY`, `OPEN`, or all gate flags.
 */
export function evaluatePromotionManifest(manifest, { artifact_bytes, public_key_pem } = {}) {
  const closed = (missing_gates, signature = null) => ({
    release_state: 'NOT_DEPLOYABLE',
    release_gate: 'CLOSED',
    production_eligible: false,
    missing_gates: [...new Set(missing_gates)],
    signature,
  });
  if (!manifestGateShape(manifest)) return closed(['manifest_shape']);
  const missing = PROMOTION_GATES.filter((gate) => manifest.gates[gate] !== true);
  const artifact = manifest.artifact;
  if (!artifact || typeof artifact !== 'object' || typeof artifact.sha256 !== 'string' || !SHA256.test(artifact.sha256)) {
    missing.push('artifact_sha256');
  }
  if (!artifact || typeof artifact.detached_signature !== 'string') missing.push('detached_signature');
  if (!artifact || typeof artifact.signing_key_id !== 'string' || artifact.signing_key_id.length < 3
    || typeof artifact.signer_public_key_fingerprint !== 'string'
    || !SHA256.test(artifact.signer_public_key_fingerprint)) missing.push('pinned_signer');
  if (missing.length > 0) return closed(missing);
  if (!(artifact_bytes instanceof Uint8Array) || typeof public_key_pem !== 'string') {
    return closed(['signature_verification_input'], { ok: false, code: 'VERIFICATION_INPUT_MISSING' });
  }
  const fingerprint = publicKeyFingerprint(public_key_pem);
  if (fingerprint === null || fingerprint !== artifact.signer_public_key_fingerprint) {
    return closed(['pinned_signer'], { ok: false, code: 'PUBLIC_KEY_FINGERPRINT_MISMATCH' });
  }
  const signature = verifyDetachedSignature({
    artifact_bytes,
    detached_signature: artifact.detached_signature,
    public_key_pem,
    expected_sha256: artifact.sha256,
  });
  if (!signature.ok) return closed(['detached_signature', 'pinned_signer'], signature);
  return {
    release_state: 'READY',
    release_gate: 'OPEN',
    production_eligible: true,
    missing_gates: [],
    signature,
  };
}

async function readPackageFiles(inputDir) {
  return Promise.all(CANDIDATE_FILES.map(async (name) => ({
    name,
    data: await readFile(join(inputDir, name)),
  })));
}

function gitCommit() {
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim() || null;
  } catch {
    return null;
  }
}

function parseEpoch(value) {
  const epoch = Number(value);
  if (!Number.isSafeInteger(epoch) || epoch < 0) fail('SOURCE_DATE_EPOCH must be a non-negative integer.');
  return epoch;
}

export async function generateCandidateRelease({ inputDir, outDir, sourceDateEpoch = 0, sourceCommit = gitCommit() }) {
  const epoch = parseEpoch(sourceDateEpoch);
  const root = resolve(inputDir);
  const output = resolve(outDir);
  const packageJson = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
  const name = String(packageJson.name);
  const version = String(packageJson.version);
  const files = await readPackageFiles(root);
  const archiveBase = `apocrypha-contributor-node-${version}`;
  const archive = createDeterministicArchive(files);
  const archiveFile = { name: `${archiveBase}.zip`, bytes: archive.bytes, size: archive.size, sha256: archive.sha256 };
  const sbomValue = sbom({ name, version, files: archive.entries, sourceDateEpoch: epoch });
  const sbomBytes = canonicalBytes(sbomValue);
  const sbomFile = { name: `${archiveBase}.spdx.json`, bytes: sbomBytes, size: sbomBytes.length, sha256: sha256(sbomBytes) };
  const provenanceValue = provenance({
    name,
    version,
    files: archive.entries,
    archive: archiveFile,
    sbomFile,
    sourceCommit,
    sourceDateEpoch: epoch,
  });
  const provenanceBytes = canonicalBytes(provenanceValue);
  const provenanceFile = {
    name: `${archiveBase}.provenance.json`,
    bytes: provenanceBytes,
    size: provenanceBytes.length,
    sha256: sha256(provenanceBytes),
  };
  const promotion = buildPromotionManifest({
    name,
    version,
    archive: archiveFile,
    sbomFile,
    provenanceFile,
    sourceCommit,
    sourceDateEpoch: epoch,
  });
  const promotionBytes = canonicalBytes(promotion);
  const promotionFile = {
    name: `${archiveBase}.promotion.json`,
    bytes: promotionBytes,
    size: promotionBytes.length,
    sha256: sha256(promotionBytes),
  };
  await mkdir(output, { recursive: true });
  await Promise.all([
    writeFile(join(output, archiveFile.name), archive.bytes),
    writeFile(join(output, sbomFile.name), sbomBytes),
    writeFile(join(output, provenanceFile.name), provenanceBytes),
    writeFile(join(output, promotionFile.name), promotionBytes),
  ]);
  return {
    archive: { name: archiveFile.name, size: archiveFile.size, sha256: archiveFile.sha256 },
    sbom: { name: sbomFile.name, size: sbomFile.size, sha256: sbomFile.sha256 },
    provenance: { name: provenanceFile.name, size: provenanceFile.size, sha256: provenanceFile.sha256 },
    promotion: { name: promotionFile.name, size: promotionFile.size, sha256: promotionFile.sha256 },
    release_state: promotion.release_state,
    release_gate: promotion.release_gate,
    missing_gates: promotion.missing_gates,
  };
}

function argumentMap(argv) {
  const result = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const item = argv[index];
    if (!item.startsWith('--')) throw new Error(`Unexpected argument: ${item}`);
    const key = item.slice(2);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for --${key}`);
    result.set(key, value);
    index += 1;
  }
  return result;
}

async function main() {
  const args = argumentMap(process.argv.slice(2));
  if (args.has('verify')) {
    const artifactBytes = await readFile(args.get('verify'));
    const signature = await readFile(args.get('signature'), 'utf8');
    const publicKeyPem = await readFile(args.get('public-key'), 'utf8');
    const result = verifyDetachedSignature({
      artifact_bytes: artifactBytes,
      detached_signature: signature,
      public_key_pem: publicKeyPem,
      expected_sha256: args.get('expected-sha256'),
    });
    process.stdout.write(`${JSON.stringify(result)}\n`);
    if (!result.ok) process.exitCode = 1;
    return;
  }
  const inputDir = args.get('input') ?? '.';
  const outDir = args.get('out') ?? 'candidate-dist';
  const result = await generateCandidateRelease({
    inputDir,
    outDir,
    sourceDateEpoch: args.get('source-date-epoch') ?? process.env.SOURCE_DATE_EPOCH ?? 0,
    sourceCommit: args.get('source-commit') ?? process.env.SOURCE_COMMIT ?? gitCommit(),
  });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}

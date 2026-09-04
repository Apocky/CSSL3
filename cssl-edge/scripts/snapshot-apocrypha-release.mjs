/** Generate the public-safe Apocrypha release shelf and integrity manifest. */

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), '..');
const DEFAULT_SOURCE = join(REPO_ROOT, 'data', 'apocrypha-living-release.v1.json');
const DEFAULT_PUBLIC_ROOT = join(REPO_ROOT, 'public');
const DEFAULT_OUTPUT = join(DEFAULT_PUBLIC_ROOT, 'releases', 'apocrypha-living');

const RELEASE_STATES = new Set(['CANDIDATE', 'RELEASED', 'RETIRED']);
const FORBIDDEN_PUBLIC_PATTERNS = [
  /[a-z]:[\\/]/i,
  /(?:^|[^a-z])localhost(?:[^a-z]|$)/i,
  /127\.0\.0\.1/,
  /-----BEGIN [^-]*PRIVATE KEY-----/i,
  /\b(?:api[_-]?key|bearer|credential|password|service[_-]?role[_-]?key)\b/i,
  /\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/i,
];

function fail(message) {
  throw new Error(`apocrypha release snapshot refused: ${message}`);
}

function plainObject(value, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${label} must be an object`);
  return value;
}

function text(value, label, maximum = 4096) {
  if (typeof value !== 'string' || value.trim() !== value || value.length < 1 || value.length > maximum) {
    fail(`${label} must be bounded canonical text`);
  }
  return value;
}

function stringArray(value, label, allowEmpty = false) {
  if (!Array.isArray(value) || (!allowEmpty && value.length < 1) || value.some(item => typeof item !== 'string' || item.trim() !== item || !item)) {
    fail(`${label} must be a canonical text array`);
  }
  return value;
}

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]));
  }
  return value;
}

function canonicalBytes(value) {
  return Buffer.from(JSON.stringify(canonical(value)), 'utf8');
}

function documentBytes(value) {
  return Buffer.from(`${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function sha256(payload) {
  return createHash('sha256').update(payload).digest('hex');
}

function publicSafe(value, label) {
  const serialized = JSON.stringify(value);
  for (const pattern of FORBIDDEN_PUBLIC_PATTERNS) {
    if (pattern.test(serialized)) fail(`${label} contains non-public material`);
  }
}

function documentBinding(label, href, bytes) {
  return { label, href, sha256: sha256(bytes), bytes: bytes.length };
}

function readDigestSidecar(path) {
  if (!existsSync(path)) return null;
  const match = readFileSync(path, 'utf8').trim().match(/^([0-9a-f]{64})(?:\s|$)/i);
  return match ? match[1].toLowerCase() : null;
}

function artifactState(source, publicRoot) {
  const build = plainObject(source.build, 'build');
  if (source.artifact === null) {
    return { download: null, downloadStatus: 'NO_PROMOTED_ARTIFACT' };
  }
  const artifact = plainObject(source.artifact, 'artifact');
  const filename = text(artifact.filename, 'artifact filename', 180);
  if (basename(filename) !== filename || !/^[A-Za-z0-9][A-Za-z0-9._-]+$/.test(filename)) {
    fail('artifact filename must be a safe basename');
  }
  const artifactPath = join(publicRoot, 'downloads', filename);
  if (!existsSync(artifactPath)) {
    return { download: null, downloadStatus: 'DECLARED_ARTIFACT_ABSENT' };
  }

  const payload = readFileSync(artifactPath);
  const digest = sha256(payload);
  const sidecarDigest = readDigestSidecar(`${artifactPath}.sha256`);
  const signatureFilename = text(artifact.signature_filename, 'signature filename', 180);
  const receiptFilename = text(
    artifact.signature_receipt_filename,
    'signature receipt filename',
    180,
  );
  if (
    basename(signatureFilename) !== signatureFilename
    || basename(receiptFilename) !== receiptFilename
  ) {
    fail('signature files must be safe basenames');
  }
  const signaturePath = join(publicRoot, 'downloads', signatureFilename);
  const receiptPath = join(publicRoot, 'downloads', receiptFilename);
  let signatureGate = false;
  let signature = null;
  if (existsSync(signaturePath) && existsSync(receiptPath)) {
    const signatureBytes = readFileSync(signaturePath);
    const receipt = plainObject(JSON.parse(readFileSync(receiptPath, 'utf8')), 'signature receipt');
    signatureGate = receipt.schema === 'apocrypha.artifact-signature-receipt.v1'
      && receipt.verified === true
      && receipt.artifact_sha256 === digest
      && receipt.signature_sha256 === sha256(signatureBytes)
      && typeof receipt.verifier === 'string'
      && typeof receipt.key_fingerprint === 'string';
    if (signatureGate) {
      signature = {
        href: `/downloads/${signatureFilename}`,
        sha256: receipt.signature_sha256,
        verifier: text(receipt.verifier, 'signature verifier', 128),
        key_fingerprint: text(receipt.key_fingerprint, 'signature key fingerprint', 256),
        receipt_href: `/downloads/${receiptFilename}`,
      };
    }
  }

  const released = source.release_state === 'RELEASED'
    && build.release_gate === 'OPEN'
    && artifact.promoted === true
    && sidecarDigest === digest
    && signatureGate;
  if (!released) {
    return { download: null, downloadStatus: 'ARTIFACT_HELD_NOT_RELEASED' };
  }
  return {
    downloadStatus: 'RELEASED_ARTIFACT_PRESENT',
    download: {
      filename,
      href: `/downloads/${filename}`,
      media_type: text(artifact.media_type, 'artifact media_type', 128),
      platform: text(artifact.platform, 'artifact platform', 128),
      bytes: payload.length,
      sha256: digest,
      sha256_href: `/downloads/${filename}.sha256`,
      signature,
    },
  };
}

export function buildReleaseBundle(sourceValue, publicRoot = DEFAULT_PUBLIC_ROOT) {
  const source = plainObject(sourceValue, 'source');
  if (source.schema !== 'apocky.apocrypha-release-source.v1') fail('source schema mismatch');
  if (!RELEASE_STATES.has(source.release_state)) fail('release state is invalid');
  const planSource = plainObject(source.plan, 'plan');
  const buildSource = plainObject(source.build, 'build');
  const milestones = source.plan.milestones;
  const changelog = source.changelog;
  if (!Array.isArray(milestones) || !Array.isArray(changelog) || changelog.length < 1) {
    fail('plan milestones and changelog entries are required');
  }
  if (!/^[0-9a-f]{40}$/.test(source.source_revision)) fail('source revision must be a full Git hash');
  stringArray(planSource.principles, 'plan principles');
  stringArray(buildSource.missing, 'build missing gates', source.release_state === 'RELEASED');

  const base = {
    version: text(source.version, 'version', 128),
    release_state: source.release_state,
    generated_at: text(source.generated_at, 'generated_at', 64),
  };
  const plan = {
    schema: 'apocky.apocrypha-public-plan.v1',
    ...base,
    project: text(source.project, 'project', 128),
    track: text(source.track, 'track', 128),
    summary: text(source.summary, 'summary'),
    objective: text(planSource.objective, 'plan objective'),
    principles: planSource.principles,
    milestones,
    release_boundary: text(planSource.release_boundary, 'plan release boundary'),
  };
  const changes = {
    schema: 'apocky.apocrypha-public-changelog.v1',
    ...base,
    project: source.project,
    entries: changelog,
    claim_boundary: 'Public-safe change summaries only; runtime availability and release readiness require fresh direct observation.',
  };
  publicSafe(plan, 'plan');
  publicSafe(changes, 'changelog');
  const planBytes = documentBytes(plan);
  const changelogBytes = documentBytes(changes);
  const artifact = artifactState(source, publicRoot);

  const manifestCore = {
    schema: 'apocky.apocrypha-release-manifest.v1',
    schema_href: '/schemas/apocrypha-release-manifest.v1.json',
    manifest_version: 1,
    ...base,
    release_label: source.release_state === 'RELEASED' ? 'Released' : source.release_state === 'RETIRED' ? 'Retired' : 'Candidate — not released',
    project: source.project,
    track: source.track,
    public_safe: true,
    source_revision: source.source_revision,
    build: {
      state: text(buildSource.state, 'build state', 128),
      verification: text(buildSource.verification, 'build verification', 256),
      release_gate: text(buildSource.release_gate, 'build release gate', 64),
      missing: buildSource.missing,
    },
    documents: {
      plan: documentBinding('Living plan', '/releases/apocrypha-living/plan.json', planBytes),
      changelog: documentBinding('Changelog', '/releases/apocrypha-living/changelog.json', changelogBytes),
      manifest: {
        label: 'Build manifest',
        href: '/releases/apocrypha-living/manifest.json',
      },
    },
    download_status: artifact.downloadStatus,
    download: artifact.download,
    claim_boundary: 'Candidate metadata is not a release. A public download appears only after exact artifact, digest, signature-receipt, promotion, and release gates pass.',
  };
  const manifest = {
    ...manifestCore,
    content_digest: sha256(canonicalBytes(manifestCore)),
  };
  publicSafe(manifest, 'manifest');
  return {
    plan,
    changelog: changes,
    manifest,
    bytes: {
      plan: planBytes,
      changelog: changelogBytes,
      manifest: documentBytes(manifest),
    },
  };
}

export function writeReleaseBundle(sourcePath = DEFAULT_SOURCE, publicRoot = DEFAULT_PUBLIC_ROOT) {
  const source = JSON.parse(readFileSync(sourcePath, 'utf8'));
  const bundle = buildReleaseBundle(source, publicRoot);
  const output = join(publicRoot, 'releases', 'apocrypha-living');
  mkdirSync(output, { recursive: true });
  const targets = {
    plan: join(output, 'plan.json'),
    changelog: join(output, 'changelog.json'),
    manifest: join(output, 'manifest.json'),
  };
  const check = process.argv.includes('--check');
  for (const [name, path] of Object.entries(targets)) {
    const expected = bundle.bytes[name];
    if (check) {
      if (!existsSync(path) || !readFileSync(path).equals(expected)) fail(`${name} output drift`);
    } else {
      writeFileSync(path, expected);
    }
  }
  return bundle;
}

if (process.argv[1] && resolve(process.argv[1]).toLowerCase() === resolve(SCRIPT_PATH).toLowerCase()) {
  const bundle = writeReleaseBundle();
  process.stdout.write(`${JSON.stringify({
    version: bundle.manifest.version,
    release_state: bundle.manifest.release_state,
    content_digest: bundle.manifest.content_digest,
    download_status: bundle.manifest.download_status,
  })}\n`);
}

export const paths = {
  source: DEFAULT_SOURCE,
  output: DEFAULT_OUTPUT,
};

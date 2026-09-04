import releaseManifestJson from '@/public/releases/apocrypha-living/manifest.json';

export type ApocryphaReleaseState = 'CANDIDATE' | 'RELEASED' | 'RETIRED';

export interface ReleaseDocumentLink {
  readonly label: string;
  readonly href: string;
}

export interface ReleaseDocumentBinding extends ReleaseDocumentLink {
  readonly sha256: string;
  readonly bytes: number;
}

export interface ReleaseDownload {
  readonly filename: string;
  readonly href: string;
  readonly media_type: string;
  readonly platform: string;
  readonly bytes: number;
  readonly sha256: string;
  readonly sha256_href: string;
  readonly signature: {
    readonly href: string;
    readonly sha256: string;
    readonly verifier: string;
    readonly key_fingerprint: string;
    readonly receipt_href: string;
  };
}

export interface ApocryphaReleaseManifest {
  readonly schema: 'apocky.apocrypha-release-manifest.v1';
  readonly schema_href: '/schemas/apocrypha-release-manifest.v1.json';
  readonly manifest_version: 1;
  readonly version: string;
  readonly release_state: ApocryphaReleaseState;
  readonly generated_at: string;
  readonly release_label: string;
  readonly project: 'Apocrypha';
  readonly track: 'local-living';
  readonly public_safe: true;
  readonly source_revision: string;
  readonly build: {
    readonly state: string;
    readonly verification: string;
    readonly release_gate: 'CLOSED' | 'OPEN';
    readonly missing: readonly string[];
  };
  readonly documents: {
    readonly plan: ReleaseDocumentBinding;
    readonly changelog: ReleaseDocumentBinding;
    readonly manifest: ReleaseDocumentLink;
  };
  readonly download_status: string;
  readonly download: ReleaseDownload | null;
  readonly claim_boundary: string;
  readonly content_digest: string;
}

export type ReleaseManifestState =
  | { readonly status: 'live'; readonly manifest: ApocryphaReleaseManifest }
  | { readonly status: 'degraded'; readonly code: 'RELEASE_MANIFEST_INVALID' };

const SHA256 = /^[0-9a-f]{64}$/;
const GIT_SHA = /^[0-9a-f]{40}$/;
const FORBIDDEN_PUBLIC_VALUE = /(?:[a-z]:[\\/]|localhost|127\.0\.0\.1|-----BEGIN [^-]*PRIVATE KEY-----|\b(?:api[_-]?key|bearer|credential|password|service[_-]?role[_-]?key)\b|\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b)/i;

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function boundedText(value: unknown, maximum = 4096): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= maximum && value.trim() === value;
}

function publicHref(value: unknown): value is string {
  return boundedText(value, 512)
    && value.startsWith('/')
    && !value.startsWith('//')
    && !value.includes('\\')
    && !value.split('/').includes('..');
}

function documentLink(value: unknown): value is ReleaseDocumentLink {
  const item = record(value);
  return Boolean(item && boundedText(item.label, 128) && publicHref(item.href));
}

function documentBinding(value: unknown): value is ReleaseDocumentBinding {
  const item = record(value);
  return Boolean(
    item
    && documentLink(item)
    && typeof item.sha256 === 'string'
    && SHA256.test(item.sha256)
    && typeof item.bytes === 'number'
    && Number.isSafeInteger(item.bytes)
    && item.bytes > 0,
  );
}

function download(value: unknown): value is ReleaseDownload {
  const item = record(value);
  const signature = record(item?.signature);
  return Boolean(
    item
    && boundedText(item.filename, 180)
    && publicHref(item.href)
    && boundedText(item.media_type, 128)
    && boundedText(item.platform, 128)
    && typeof item.bytes === 'number'
    && Number.isSafeInteger(item.bytes)
    && item.bytes > 0
    && typeof item.sha256 === 'string'
    && SHA256.test(item.sha256)
    && publicHref(item.sha256_href)
    && signature
    && publicHref(signature.href)
    && typeof signature.sha256 === 'string'
    && SHA256.test(signature.sha256)
    && boundedText(signature.verifier, 128)
    && boundedText(signature.key_fingerprint, 256)
    && publicHref(signature.receipt_href),
  );
}

export function parseApocryphaReleaseManifest(value: unknown): ApocryphaReleaseManifest {
  const item = record(value);
  const build = record(item?.build);
  const documents = record(item?.documents);
  const missing = build?.missing;
  const releaseState = item?.release_state;
  const candidateDownload = item?.download;
  const structurallyValid = item?.schema === 'apocky.apocrypha-release-manifest.v1'
    && item.schema_href === '/schemas/apocrypha-release-manifest.v1.json'
    && item.manifest_version === 1
    && boundedText(item.version, 128)
    && (releaseState === 'CANDIDATE' || releaseState === 'RELEASED' || releaseState === 'RETIRED')
    && boundedText(item.generated_at, 64)
    && boundedText(item.release_label, 128)
    && item.project === 'Apocrypha'
    && item.track === 'local-living'
    && item.public_safe === true
    && typeof item.source_revision === 'string'
    && GIT_SHA.test(item.source_revision)
    && build
    && boundedText(build.state, 128)
    && boundedText(build.verification, 256)
    && (build.release_gate === 'CLOSED' || build.release_gate === 'OPEN')
    && Array.isArray(missing)
    && (releaseState === 'RELEASED' || missing.length > 0)
    && missing.every(entry => boundedText(entry, 512))
    && documents
    && documentBinding(documents.plan)
    && documentBinding(documents.changelog)
    && documentLink(documents.manifest)
    && boundedText(item.download_status, 128)
    && (candidateDownload === null || download(candidateDownload))
    && boundedText(item.claim_boundary, 1024)
    && typeof item.content_digest === 'string'
    && SHA256.test(item.content_digest);
  if (!structurallyValid || FORBIDDEN_PUBLIC_VALUE.test(JSON.stringify(value))) {
    throw new Error('RELEASE_MANIFEST_INVALID');
  }
  if (
    candidateDownload !== null
    && (
      releaseState !== 'RELEASED'
      || build.release_gate !== 'OPEN'
      || item.download_status !== 'RELEASED_ARTIFACT_PRESENT'
    )
  ) {
    throw new Error('RELEASE_MANIFEST_INVALID');
  }
  return value as ApocryphaReleaseManifest;
}

export function loadApocryphaReleaseManifest(value: unknown = releaseManifestJson): ReleaseManifestState {
  try {
    return { status: 'live', manifest: parseApocryphaReleaseManifest(value) };
  } catch {
    return { status: 'degraded', code: 'RELEASE_MANIFEST_INVALID' };
  }
}

export function publicReleaseDownload(manifest: ApocryphaReleaseManifest): ReleaseDownload | null {
  return manifest.release_state === 'RELEASED'
    && manifest.build.release_gate === 'OPEN'
    && manifest.download_status === 'RELEASED_ARTIFACT_PRESENT'
    ? manifest.download
    : null;
}

export const apocryphaRelease = loadApocryphaReleaseManifest();

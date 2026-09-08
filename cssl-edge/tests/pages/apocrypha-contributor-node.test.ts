import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import type { NextApiRequest, NextApiResponse } from 'next';
import handler from '@/pages/api/apocrypha/contributor/manifest';
import { canOfferContributorArtifact } from '@/pages/download/apocrypha-node';
import {
  CONTRIBUTOR_NODE_MANIFEST,
  parseContributorNodeManifest,
  type ContributorNodeManifest,
} from '@/lib/apocrypha/contributor-node';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function invoke(method: string): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query: {}, headers: {}, body: undefined } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(key: string, value: string) { out.headers[key.toLowerCase()] = value; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

export function testCandidateManifestIsFailClosed(): void {
  const parsed = parseContributorNodeManifest(CONTRIBUTOR_NODE_MANIFEST);
  assert.ok(parsed);
  assert.equal(parsed.release_state, 'NOT_DEPLOYABLE');
  assert.equal(parsed.release_gate, 'CLOSED');
  assert.equal(parsed.contract.status, 'candidate_only');
  assert.equal(parsed.contract.transport, 'local_only');
  assert.equal(parsed.contract.network, 'disabled');
  assert.equal(parsed.contract.production_eligible, false);
  assert.equal(parsed.execution.default_mode, 'paused');
  assert.equal(parsed.execution.opt_in_required, true);
  assert.equal(parsed.execution.auto_start, false);
  assert.equal(parsed.execution.privilege_escalation, false);
  assert.equal(parsed.privacy.raw_conversation, false);
  assert.equal(parsed.privacy.raw_memory, false);
  assert.equal(parsed.privacy.vault_payloads, false);
  assert.equal(parsed.platforms.length, 5);
  for (const platform of parsed.platforms) {
    assert.equal(platform.state, 'NOT_DEPLOYED');
    assert.equal(platform.artifact, null);
  }
}

export function testReadyWithoutNativeReleaseIsRejected(): void {
  const candidate = structuredClone(CONTRIBUTOR_NODE_MANIFEST) as unknown as {
    release_state: ContributorNodeManifest['release_state'];
    release_gate: ContributorNodeManifest['release_gate'];
    contract: ContributorNodeManifest['contract'];
    platforms: Array<ContributorNodeManifest['platforms'][number]>;
  };
  candidate.release_state = 'READY';
  candidate.release_gate = 'OPEN';
  candidate.platforms = candidate.platforms.map((platform) => ({ ...platform, state: 'READY', artifact: {
    href: `/downloads/apocrypha-node/${platform.target}.zip`,
    filename: `${platform.target}.zip`,
    sha256: 'a'.repeat(64),
    detached_signature: 'candidate-signature',
    signing_key_id: 'apocky-release-v1',
    bytes: 1024,
    provenance: 'candidate-only',
  }}));
  assert.equal(parseContributorNodeManifest(candidate), null);
}

export function testLandingPageCannotOfferCandidateArtifact(): void {
  const page = readFileSync('pages/download/apocrypha-node.tsx', 'utf8');
  assert.match(page, /manifest\.release_state/);
  assert.match(page, /No verified package available/);
  assert.match(page, /Downloads are not enabled yet/);
  assert.match(page, /canOfferContributorArtifact/);
  assert.match(page, /data-release-gate/);
  assert.match(page, /Candidate gates/);
  assert.match(page, /Get-FileHash \.\\PACKAGE\.zip -Algorithm SHA256/);
  assert.match(page, /detached signature/);
  assert.match(page, /href="\/api\/apocrypha\/contributor\/manifest"/);
  assert.doesNotMatch(page, /Mycelium-v0\.1\.0-alpha-windows-x64\.exe\.placeholder/);
}

export function testCandidateArtifactLinkPredicateFailsClosed(): void {
  const candidate = structuredClone(CONTRIBUTOR_NODE_MANIFEST) as ContributorNodeManifest;
  const platform = candidate.platforms[0]!;
  assert.equal(canOfferContributorArtifact(candidate, platform), false);

  const candidateWithUnsignedArtifact = {
    ...candidate,
    platforms: candidate.platforms.map((item, index) => index === 0 ? {
      ...item,
      state: 'READY' as const,
      artifact: {
        href: '/downloads/apocrypha-node/candidate.zip',
        filename: 'candidate.zip',
        sha256: 'a'.repeat(64),
        detached_signature: '',
        signing_key_id: '',
        bytes: 1,
        provenance: 'candidate-only',
      },
    } : item),
  } as unknown as ContributorNodeManifest;
  assert.equal(canOfferContributorArtifact(candidateWithUnsignedArtifact, candidateWithUnsignedArtifact.platforms[0]!), false);
}

export async function testGetReturnsTypedManifest(): Promise<void> {
  const { req, res, out } = invoke('GET');
  await handler(req, res);
  assert.equal(out.statusCode, 200);
  assert.equal(out.headers['cache-control'], 'no-store');
  const body = out.body as { ok: boolean; manifest: ContributorNodeManifest };
  assert.equal(body.ok, true);
  assert.equal(body.manifest.release_state, 'NOT_DEPLOYABLE');
}

export async function testNonGetRefuses(): Promise<void> {
  const { req, res, out } = invoke('POST');
  await handler(req, res);
  assert.equal(out.statusCode, 405);
  assert.equal(out.headers.allow, 'GET');
  assert.deepEqual(out.body, { ok: false, error: 'GET only', served_by: 'cssl-edge', ts: (out.body as { ts: string }).ts });
}

async function runAll(): Promise<void> {
  testCandidateManifestIsFailClosed();
  testReadyWithoutNativeReleaseIsRejected();
  testLandingPageCannotOfferCandidateArtifact();
  testCandidateArtifactLinkPredicateFailsClosed();
  await testGetReturnsTypedManifest();
  await testNonGetRefuses();
  console.log('apocrypha-contributor-node.test : OK · 6 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}

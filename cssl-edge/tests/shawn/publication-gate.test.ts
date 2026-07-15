import { createHash } from 'node:crypto';

import { evaluateAtlasPublicationGate } from '@/lib/shawn/publication-gate';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

function encodedFrame(overrides: Record<string, unknown> = {}): { encoded: string; hash: string } {
  const frame = {
    status: 'ratified',
    atlas: { version: '1.0.0' },
    gates: {
      human_semantic_review: true,
      excerpt_approval: true,
      ratification: true,
    },
    ...overrides,
  };
  const bytes = Buffer.from(JSON.stringify(frame), 'utf8');
  return {
    encoded: bytes.toString('base64'),
    hash: createHash('sha256').update(bytes).digest('hex'),
  };
}

function run(): void {
  const development = evaluateAtlasPublicationGate(
    { NODE_ENV: 'development' },
    { version: '0.1.0-public-candidate', status: 'candidate' },
  );
  assert(development.allowed && development.mode === 'development-preview', 'development exposes review surface');

  const ratified = encodedFrame();
  const candidateProduction = evaluateAtlasPublicationGate(
    {
      NODE_ENV: 'production',
      SHAWN_ATLAS_RATIFIED_FRAME_BASE64: ratified.encoded,
      SHAWN_ATLAS_RATIFIED_FRAME_SHA256: ratified.hash,
    },
    { version: '1.0.0', status: 'candidate' },
  );
  assert(!candidateProduction.allowed, 'candidate compiled atlas is never served in production');

  const missingFrame = evaluateAtlasPublicationGate(
    { NODE_ENV: 'production' },
    { version: '1.0.0', status: 'ratified' },
  );
  assert(!missingFrame.allowed, 'production fails closed without frame');

  const wrongHash = evaluateAtlasPublicationGate(
    {
      NODE_ENV: 'production',
      SHAWN_ATLAS_RATIFIED_FRAME_BASE64: ratified.encoded,
      SHAWN_ATLAS_RATIFIED_FRAME_SHA256: '0'.repeat(64),
    },
    { version: '1.0.0', status: 'ratified' },
  );
  assert(!wrongHash.allowed, 'hash mismatch fails closed');

  const incomplete = encodedFrame({
    gates: {
      human_semantic_review: true,
      excerpt_approval: false,
      ratification: true,
    },
  });
  const incompleteGate = evaluateAtlasPublicationGate(
    {
      NODE_ENV: 'production',
      SHAWN_ATLAS_RATIFIED_FRAME_BASE64: incomplete.encoded,
      SHAWN_ATLAS_RATIFIED_FRAME_SHA256: incomplete.hash,
    },
    { version: '1.0.0', status: 'ratified' },
  );
  assert(!incompleteGate.allowed, 'all human gates are required');

  const versionMismatch = evaluateAtlasPublicationGate(
    {
      NODE_ENV: 'production',
      SHAWN_ATLAS_RATIFIED_FRAME_BASE64: ratified.encoded,
      SHAWN_ATLAS_RATIFIED_FRAME_SHA256: ratified.hash,
    },
    { version: '2.0.0', status: 'ratified' },
  );
  assert(!versionMismatch.allowed, 'frame is bound to exact atlas version');

  const allowed = evaluateAtlasPublicationGate(
    {
      NODE_ENV: 'production',
      SHAWN_ATLAS_RATIFIED_FRAME_BASE64: ratified.encoded,
      SHAWN_ATLAS_RATIFIED_FRAME_SHA256: ratified.hash,
    },
    { version: '1.0.0', status: 'ratified' },
  );
  assert(allowed.allowed && allowed.frameHash === ratified.hash, 'matching ratified frame allows exact version');

  console.log('shawn/publication-gate.test : OK · 7 tests passed');
}

run();

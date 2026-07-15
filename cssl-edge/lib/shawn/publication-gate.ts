import { createHash, timingSafeEqual } from 'node:crypto';

interface AtlasPublicationContract {
  readonly version: string;
  readonly status: 'candidate' | 'ratified';
}

interface RatifiedFrame {
  readonly status?: unknown;
  readonly atlas?: {
    readonly version?: unknown;
  };
  readonly gates?: {
    readonly human_semantic_review?: unknown;
    readonly excerpt_approval?: unknown;
    readonly ratification?: unknown;
  };
}

export interface PublicationGateResult {
  readonly allowed: boolean;
  readonly mode: 'development-preview' | 'ratified-production' | 'blocked-production';
  readonly reason: string;
  readonly frameHash?: string;
}

const SHA256 = /^[a-f0-9]{64}$/;

function hashesEqual(actual: string, expected: string): boolean {
  if (!SHA256.test(actual) || !SHA256.test(expected)) return false;
  return timingSafeEqual(Buffer.from(actual, 'hex'), Buffer.from(expected, 'hex'));
}

export function evaluateAtlasPublicationGate(
  env: NodeJS.ProcessEnv,
  contract: AtlasPublicationContract,
): PublicationGateResult {
  if (env.NODE_ENV !== 'production') {
    return {
      allowed: true,
      mode: 'development-preview',
      reason: 'Non-production review surface; candidate and publication blockers remain visible.',
    };
  }

  if (contract.status !== 'ratified') {
    return {
      allowed: false,
      mode: 'blocked-production',
      reason: 'The compiled atlas is not ratified.',
    };
  }

  const encoded = env.SHAWN_ATLAS_RATIFIED_FRAME_BASE64;
  const expectedHash = env.SHAWN_ATLAS_RATIFIED_FRAME_SHA256?.toLowerCase();
  if (!encoded || !expectedHash || !SHA256.test(expectedHash)) {
    return {
      allowed: false,
      mode: 'blocked-production',
      reason: 'A hash-pinned ratified ContextFrame is not configured.',
    };
  }

  let bytes: Buffer;
  let frame: RatifiedFrame;
  try {
    bytes = Buffer.from(encoded, 'base64');
    if (bytes.length === 0 || bytes.toString('base64').replace(/=+$/u, '') !== encoded.replace(/=+$/u, '')) {
      throw new Error('invalid base64');
    }
    frame = JSON.parse(bytes.toString('utf8')) as RatifiedFrame;
  } catch {
    return {
      allowed: false,
      mode: 'blocked-production',
      reason: 'The configured ContextFrame is not valid canonical JSON payload.',
    };
  }

  const actualHash = createHash('sha256').update(bytes).digest('hex');
  if (!hashesEqual(actualHash, expectedHash)) {
    return {
      allowed: false,
      mode: 'blocked-production',
      reason: 'The configured ContextFrame hash does not match its payload.',
    };
  }

  const gates = frame.gates;
  const gateComplete = gates?.human_semantic_review === true
    && gates.excerpt_approval === true
    && gates.ratification === true;
  if (frame.status !== 'ratified' || frame.atlas?.version !== contract.version || !gateComplete) {
    return {
      allowed: false,
      mode: 'blocked-production',
      reason: 'The ContextFrame does not ratify this atlas version and all required human gates.',
      frameHash: actualHash,
    };
  }

  return {
    allowed: true,
    mode: 'ratified-production',
    reason: 'Hash-matched ContextFrame ratifies this exact atlas version and required human gates.',
    frameHash: actualHash,
  };
}

// Server-only: exact-byte verification precedes JSON projection and all owner/schema checks.
import { createHash } from 'node:crypto';
import { createHistoryCodecInstance, wasmBase64, wasmSha256 } from './generated/history-proof-codec.mjs';

export const HISTORY_PROOF_ACCEPT = 'application/vnd.apocv4.chat-history-proof-bundle.v2+json';
export const HISTORY_PROOF_WIRE_LIMIT = 6 * 1024 * 1024;

export class HistoryProofCodecError extends Error {
  constructor(readonly code: string) {
    super(code);
    this.name = 'HistoryProofCodecError';
  }
}

const verified = new WeakSet<object>();
let compiled: Promise<WebAssembly.Module> | undefined;
let instance: ReturnType<typeof createHistoryCodecInstance> | undefined;

export function isVerifiedHistoryValue(value: unknown): boolean {
  return typeof value === 'object' && value !== null && verified.has(value);
}

function protectAndFreeze(value: unknown, protectedValues: readonly string[]): void {
  if (typeof value === 'string') {
    if (protectedValues.some(secret => secret.length > 0 && value.includes(secret))) {
      throw new HistoryProofCodecError('runtime_reflected_credential');
    }
  } else if (typeof value === 'object' && value !== null) {
    for (const [key, child] of Object.entries(value)) {
      protectAndFreeze(key, protectedValues);
      protectAndFreeze(child, protectedValues);
    }
    Object.freeze(value);
  }
}

function markProofLocations(page: Record<string, unknown>): void {
  verified.add(page);
  for (const turn of page.turns as Record<string, unknown>[]) {
    verified.add(turn);
    if (turn.token_admission !== null) verified.add(turn.token_admission as object);
    if (turn.state === 'COMPLETED') {
      const response = turn.response as Record<string, unknown>;
      verified.add(response);
      const model = response.model_reported as Record<string, unknown>;
      if (model.token_admission !== null) verified.add(model.token_admission as object);
    }
  }
}

async function moduleBytes(): Promise<WebAssembly.Module> {
  if (!compiled) {
    compiled = (async () => {
      const bytes = Buffer.from(wasmBase64, 'base64');
      if (createHash('sha256').update(bytes).digest('hex') !== wasmSha256) throw new Error();
      return WebAssembly.compile(bytes);
    })();
  }
  try {
    return await compiled;
  } catch {
    compiled = undefined;
    instance = undefined;
    throw new HistoryProofCodecError('runtime_history_codec_unavailable');
  }
}

export async function decodeVerifiedHistoryEnvelope(
  input: Uint8Array,
  protectedValues: readonly string[],
): Promise<Record<string, unknown>> {
  if (input.byteLength > HISTORY_PROOF_WIRE_LIMIT) throw new HistoryProofCodecError('history_proof_limit_exceeded');
  const module = await moduleBytes();
  let pageText: string;
  try {
    instance ??= createHistoryCodecInstance(module);
    // § synchronous invocation ; no request can interleave with this instance's heap.
    pageText = instance.verify(input);
  } catch (error) {
    if (typeof error === 'string' && /^history_proof_[a-z_]+$/.test(error)) {
      throw new HistoryProofCodecError(error);
    }
    // § trap discards the complete factory state ; next request gets a fresh instance.
    instance = undefined;
    throw new HistoryProofCodecError('runtime_history_codec_unavailable');
  }
  if (protectedValues.some(secret => secret.length > 0 && pageText.includes(secret))) {
    throw new HistoryProofCodecError('runtime_reflected_credential');
  }
  const page = JSON.parse(pageText) as Record<string, unknown>;
  // § decoded strings scanned too ; escaped capsules must not conceal reflected credentials.
  protectAndFreeze(page, protectedValues);
  markProofLocations(page);
  return { schema_version: 'apocv4.runtime-service.v1', result: page };
}

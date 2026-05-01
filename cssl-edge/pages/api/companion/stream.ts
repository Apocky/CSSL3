// cssl-edge · /api/companion/stream
// Cap-gated SSE-streaming companion endpoint. Emits Anthropic-Messages-API-shaped
// `content_block_delta` chunks at ~100ms cadence so client code can wire up the
// streaming companion UI today and pick up the real upstream when CLAUDE_API_KEY
// lands. NO external network calls — stage-0 ships a deterministic 5-chunk fake.
//
// Auth model :
//   - Cap-bit COMPANION_REMOTE_RELAY = 1 REQUIRED · DEFAULT-DENY when caps=0
//   - Sovereign bypass : sovereign:true + x-loa-sovereign-cap header → allowed
//
// Inputs :
//   - GET ?messages=<base64-json>&cap=<int>&sovereign=<bool>
//   - The base64 payload decodes to Array<{ role:'user'|'assistant', content:string }>
//   - SSE responses are write-streamed · final `data: [DONE]\n\n` sentinel
//
// Audit log :
//   - kind:'companion.stream.begin' on stream-start
//   - kind:'companion.stream.end'   on stream-finish (count + duration_ms)

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';
import { COMPANION_REMOTE_RELAY, checkCap } from '@/lib/cap';
import { SseWriter } from '@/lib/sse';

interface Message {
  role: 'user' | 'assistant';
  content: string;
}

interface StreamError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

function isMessageArray(v: unknown): v is Message[] {
  if (!Array.isArray(v)) return false;
  return v.every(
    (m) =>
      isObject(m) &&
      (m['role'] === 'user' || m['role'] === 'assistant') &&
      typeof m['content'] === 'string'
  );
}

function readQueryParam(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

function readQueryNum(
  q: Record<string, string | string[] | undefined>,
  key: string,
  fallback: number
): number {
  const raw = readQueryParam(q, key);
  if (raw === undefined) return fallback;
  const n = Number(raw);
  if (!Number.isFinite(n)) return fallback;
  return n;
}

function readQueryBool(
  q: Record<string, string | string[] | undefined>,
  key: string
): boolean {
  const raw = readQueryParam(q, key);
  return raw === 'true' || raw === '1';
}

// Decode base64-JSON · returns Message[] OR null on any decode failure.
function decodeMessagesBase64(b64: string): Message[] | null {
  try {
    const decoded = Buffer.from(b64, 'base64').toString('utf-8');
    const parsed = JSON.parse(decoded) as unknown;
    if (isMessageArray(parsed)) return parsed;
    if (isObject(parsed) && isMessageArray(parsed['messages'])) {
      return parsed['messages'];
    }
    return null;
  } catch {
    return null;
  }
}

// Deterministic fake-stream chunker. 5 chunks · text derived from last user msg.
function buildChunks(messages: Message[]): string[] {
  const last = messages[messages.length - 1];
  const lastText = last !== undefined ? last.content.slice(0, 32) : '';
  return [
    `<stream stub · `,
    `${messages.length} msg(s) · `,
    `echoes "${lastText}" · `,
    `chunk 4 of 5 · `,
    `[end of stub stream]`,
  ];
}

// Helper to make stream-cadence injectable for tests (so tests don't sleep).
let _delayMsForTests: number | null = null;
export function _setStreamDelayForTests(ms: number | null): void {
  _delayMsForTests = ms;
}
function chunkDelay(): number {
  return _delayMsForTests ?? 100;
}

function sleep(ms: number): Promise<void> {
  if (ms <= 0) return Promise.resolve();
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<StreamError>
): Promise<void> {
  logHit('companion.stream', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?messages=<base64-json>&cap=&sovereign=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = req.query as Record<string, string | string[] | undefined>;
  const cap = readQueryNum(q, 'cap', 0);
  const sovereignFlag = readQueryBool(q, 'sovereign');
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate. DEFAULT-DENY.
  const decision = checkCap(cap, COMPANION_REMOTE_RELAY, sovereignAllowed);
  if (!decision.ok) {
    const reason = decision.reason ?? 'cap COMPANION_REMOTE_RELAY=0x1 required';
    const d = deny(reason, cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: reason,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Decode messages. Missing or malformed → 400 with envelope.
  const b64 = readQueryParam(q, 'messages');
  if (typeof b64 !== 'string' || b64.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — messages query param required (base64-encoded JSON)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const messages = decodeMessagesBase64(b64);
  if (messages === null) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — messages must base64-decode to Array<{role,content}>',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // SSE headers · keep-alive disable for proxy buffering.
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache, no-transform');
  res.setHeader('Connection', 'keep-alive');
  res.setHeader('X-Accel-Buffering', 'no');
  res.status(200);

  const startTs = Date.now();
  logEvent(
    auditEvent('companion.stream.begin', cap, sovereignAllowed, 'ok', {
      message_count: messages.length,
    })
  );

  // The pages-router NextApiResponse has res.write/res.end on the underlying
  // http.ServerResponse — we narrow via the SseWriter constructor's structural
  // typing requirement.
  const writer = new SseWriter(
    res as unknown as { write(s: string): boolean; end(): void }
  );

  // Emit message-start envelope first (matches Anthropic streaming shape).
  writer.writeData({
    type: 'message_start',
    message: {
      id: `msg_stream_${Date.now().toString(16)}`,
      type: 'message',
      role: 'assistant',
      model: 'claude-opus-4-7',
    },
  });

  const chunks = buildChunks(messages);
  for (let i = 0; i < chunks.length; i += 1) {
    if (i > 0) {
      await sleep(chunkDelay());
    }
    writer.writeData({
      type: 'content_block_delta',
      index: 0,
      delta: { type: 'text_delta', text: chunks[i] ?? '' },
    });
  }

  // Final terminator + close.
  writer.writeDone();
  writer.close();

  const durationMs = Date.now() - startTs;
  logEvent(
    auditEvent('companion.stream.end', cap, sovereignAllowed, 'ok', {
      chunk_count: chunks.length,
      duration_ms: durationMs,
    })
  );
}

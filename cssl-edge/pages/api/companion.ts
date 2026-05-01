// cssl-edge · /api/companion
// Cap-gated remote-Claude relay stub. Mirrors the Anthropic Messages API
// shape so client code can integrate today and pick up real behaviour later.
//
// Auth model :
//   - Cap-bit COMPANION_REMOTE_RELAY = 1 REQUIRED · DEFAULT-DENY when caps=0
//   - Sovereign bypass : sovereign:true + x-loa-sovereign-cap header → allowed
//
// NO external network calls — stage-0 ships a shape-correct mock. Real Claude
// integration lands once CLAUDE_API_KEY is present + Anthropic SDK is wired.

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

// Cap-bit layout. COMPANION_REMOTE_RELAY = bit 0 → mask 0x1.
export const CAP_COMPANION_REMOTE_RELAY = 0x1;

interface Message {
  role: 'user' | 'assistant';
  content: string;
}

interface CompanionRequest {
  messages?: unknown;
  cap?: unknown;
  sovereign?: unknown;
  model?: unknown;
}

// Anthropic-Messages-API-shaped response.
interface CompanionResponse {
  id: string;
  type: 'message';
  role: 'assistant';
  content: Array<{ type: 'text'; text: string }>;
  model: string;
  stop_reason: 'end_turn';
  usage: { input_tokens: number; output_tokens: number };
  served_by: string;
  ts: string;
  stub: true;
}

interface CompanionError {
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

// Approximate token-count : 4 chars/token rule-of-thumb. Not exact — the real
// route will surface the upstream usage block from Anthropic.
function approxTokens(text: string): number {
  return Math.max(1, Math.ceil(text.length / 4));
}

function stubMessageId(): string {
  // 12 hex chars · sufficient for stub-distinct tracing.
  const r = Math.floor(Math.random() * 0xffffffffffff);
  return `msg_stub_${r.toString(16).padStart(12, '0')}`;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<CompanionResponse | CompanionError>
): void {
  logHit('companion', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {messages, cap, sovereign?, model?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON {messages, cap, sovereign?, model?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const reqBody = body as CompanionRequest;

  if (!isMessageArray(reqBody.messages)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — messages must be Array<{role:user|assistant, content:string}>',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);
  const model = typeof reqBody.model === 'string' ? reqBody.model : 'claude-opus-4-7';

  // Cap-gate. DEFAULT-DENY : caps=0 + no sovereign-header → 403.
  const capAllowed = (cap & CAP_COMPANION_REMOTE_RELAY) !== 0;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap COMPANION_REMOTE_RELAY=0x1 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: d.body.extra?.['reason'] as string ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Audit log — 'ok' branch.
  logEvent(
    auditEvent('companion.relay', cap, sovereignAllowed, 'ok', {
      message_count: reqBody.messages.length,
      model,
    })
  );

  // Stub Anthropic-Messages-API shape. NO network call.
  const lastUser = reqBody.messages[reqBody.messages.length - 1];
  const lastUserText = lastUser !== undefined ? lastUser.content : '';
  const inputTokens = reqBody.messages.reduce(
    (acc, m) => acc + approxTokens(m.content),
    0
  );
  const stubText = `<stubbed companion response · ${reqBody.messages.length} message(s) · echoes "${lastUserText.slice(0, 40)}">`;

  const env = envelope();
  res.status(200).json({
    id: stubMessageId(),
    type: 'message',
    role: 'assistant',
    content: [{ type: 'text', text: stubText }],
    model,
    stop_reason: 'end_turn',
    usage: {
      input_tokens: inputTokens,
      output_tokens: approxTokens(stubText),
    },
    served_by: env.served_by,
    ts: env.ts,
    stub: true,
  });
}

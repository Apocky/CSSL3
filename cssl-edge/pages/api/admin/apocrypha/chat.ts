// One authenticated REST turn into the proprietary V2 body.

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  expectedConversationRef,
  fetchApocryphaV2,
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  requireApocryphaOwner,
  scopeConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';

const UPSTREAM_DEADLINE_MS = 25_000;
const MAX_TEXT_BYTES = 16_384;
const TURN_SOURCE_REF = 'public:apocky.com/chat';
const EXPECTED_EXPRESSION_MODE = 'bootstrap_shallow';

interface TurnBody {
  text?: unknown;
  conversation_id?: unknown;
  request_id?: unknown;
}

function boundedText(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!text || Buffer.byteLength(text, 'utf8') > MAX_TEXT_BYTES) return null;
  return text;
}

function stringField(body: Record<string, unknown>, key: string): string | null {
  const value = body[key];
  return typeof value === 'string' && value.length > 0 ? value : null;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    return;
  }

  const owner = await requireApocryphaOwner(req, res);
  if (!owner) return;

  const body = (req.body ?? {}) as TurnBody;
  const text = boundedText(body.text);
  if (!text) {
    res.status(400).json({
      error: `text must contain 1-${MAX_TEXT_BYTES} UTF-8 bytes after trimming`,
      ...envelope(),
    });
    return;
  }
  if (!isOpaqueConversationId(body.conversation_id)) {
    res.status(400).json({
      error: 'conversation_id must be an opaque UUIDv4 minted by this client',
      ...envelope(),
    });
    return;
  }
  if (!isOpaqueClientRequestId(body.request_id)) {
    res.status(400).json({
      error: 'request_id must be an opaque UUIDv4 minted once for this client turn',
      ...envelope(),
    });
    return;
  }

  const clientConversationId = body.conversation_id.toLowerCase();
  const clientRequestId = body.request_id.toLowerCase();
  const scopedConversationId = scopeConversationId(owner.principalRef, clientConversationId);
  const scopedRequestId = scopeRequestId(owner.principalRef, clientRequestId);
  const requiredConversationRef = expectedConversationRef(scopedConversationId, TURN_SOURCE_REF);
  const upstream = await fetchApocryphaV2({
    method: 'POST',
    upstreamPath: '/v2/turn',
    deadlineMs: UPSTREAM_DEADLINE_MS,
    body: {
      text,
      request_id: scopedRequestId,
      idempotency_key: scopedRequestId,
      conversation_id: scopedConversationId,
      source_ref: TURN_SOURCE_REF,
      authority_ref: `authority:authenticated-session:${owner.principalRef}`,
      consent_ref: `consent:authenticated-turn:${owner.principalRef}`,
      privacy_class: 'restricted',
      modality: 'text',
    },
  });

  if (!upstream.ok || !upstream.payload || typeof upstream.payload !== 'object' || Array.isArray(upstream.payload)) {
    res.status(upstream.status).json({
      error: 'Apocrypha could not complete this V2 turn.',
      upstream_status: upstream.status,
      conversation_id: clientConversationId,
      ...envelope(),
    });
    return;
  }

  const payload = upstream.payload as Record<string, unknown>;
  const responseText = stringField(payload, 'text');
  const upstreamRequestId = stringField(payload, 'request_id');
  const conversationRef = stringField(payload, 'conversation_ref');
  const transitionId = stringField(payload, 'transition_id');
  const stateRoot = stringField(payload, 'state_root');
  const expressionMode = stringField(payload, 'expression_mode');
  const outcome = stringField(payload, 'outcome');
  const committedEnvelope = Boolean(
    responseText
      && transitionId
      && stateRoot
      && upstreamRequestId === scopedRequestId
      && conversationRef === requiredConversationRef
      && expressionMode === EXPECTED_EXPRESSION_MODE
      && outcome === 'committed'
      && payload.external_inference === false,
  );
  if (!committedEnvelope) {
    res.status(502).json({
      error: 'The V2 body did not return the exact committed proprietary turn envelope.',
      conversation_id: clientConversationId,
      request_id: clientRequestId,
      duplicate_commit_protection: 'active',
      ...envelope(),
    });
    return;
  }

  res.status(200).json({
    text: responseText,
    conversation_id: clientConversationId,
    conversation_ref: conversationRef,
    request_id: clientRequestId,
    request_ref: scopedRequestId,
    transition_id: transitionId,
    state_root: stateRoot,
    expression_mode: expressionMode,
    external_inference: false,
    outcome,
    duplicate_commit_protection: 'active',
    upstream_status: upstream.status,
    ...envelope(),
  });
}

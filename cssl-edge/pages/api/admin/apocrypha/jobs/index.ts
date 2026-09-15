import { createHash, randomUUID } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  enqueueApocryphaJob,
  readOwnerChatIdempotentJob,
  readOwnerChatConversationHistory,
  type ApocryphaJobKind,
} from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField, requireOwnerIdentity, respondJobError, stringField } from '@/lib/apocrypha/job-http';
import { isOpaqueConversationId } from '@/lib/apocrypha/proxy';

function isSameOwnerChatRequest(
  prior: Record<string, unknown> | undefined,
  expected: {
    prompt: string;
    conversationId: string;
    outputBudget: number;
    responseMode: 'standard' | 'deep';
  },
): boolean {
  return Boolean(prior
    && prior.prompt === expected.prompt
    && prior.conversation_id === expected.conversationId
    && prior.output_budget === expected.outputBudget
    && prior.response_mode === expected.responseMode
    && prior.source === 'apocky.com'
    && prior.privacy_class === 'restricted'
    && prior.memory_scope === 'owner-authorized');
}

function conversationIdForAdmission(
  suppliedConversationId: unknown,
  identity: { tenantId: string; principalId: string },
  idempotencyKey: string,
): string {
  if (typeof suppliedConversationId === 'string') return suppliedConversationId.toLowerCase();
  const bytes = createHash('sha256')
    .update('apocky.owner-chat.conversation.v1\0', 'utf8')
    .update(identity.tenantId, 'utf8')
    .update('\0', 'utf8')
    .update(identity.principalId, 'utf8')
    .update('\0', 'utf8')
    .update(idempotencyKey, 'utf8')
    .digest()
    .subarray(0, 16);
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = bytes.toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'POST') return methodNotAllowed(res, ['POST']);
  try {
    const identity = await requireOwnerIdentity(req, res);
    if (!identity) return;
    const body = objectField(req.body, 'body');
    const prompt = stringField(body.prompt, 'prompt', 32_000);
    const suppliedIdempotencyKey = typeof body.idempotency_key === 'string' && body.idempotency_key.trim()
      ? stringField(body.idempotency_key, 'idempotency_key', 160)
      : null;
    const idempotencyKey = suppliedIdempotencyKey ?? randomUUID();
    if (body.conversation_id != null && !isOpaqueConversationId(body.conversation_id)) {
      return res.status(400).json({
        ok: false,
        code: 'CONVERSATION_ID_INVALID',
        error: 'conversation_id must be null or an opaque UUIDv4.',
      });
    }
    const conversationId = conversationIdForAdmission(body.conversation_id, identity, idempotencyKey);
    const outputBudget = Math.min(4096, Math.max(128, Number(body.output_budget) || 1536));
    const responseMode = body.response_mode === 'deep' ? 'deep' : 'standard';
    const expectedRequest = { prompt, conversationId, outputBudget, responseMode } as const;
    if (suppliedIdempotencyKey) {
      const existing = await readOwnerChatIdempotentJob(identity, conversationId, idempotencyKey);
      if (existing) {
        if (!isSameOwnerChatRequest(existing.request, expectedRequest)) {
          throw new Error('IDEMPOTENCY_CONFLICT');
        }
        return res.status(202).json({
          ok: true,
          accepted: true,
          replayed: true,
          conversation_id: conversationId,
          job: existing.job,
        });
      }
    }
    const conversationHistory = await readOwnerChatConversationHistory(identity, conversationId);
    const request = {
      prompt,
      messages: [
        ...conversationHistory,
        { role: 'user' as const, content: prompt },
      ],
      conversation_id: conversationId,
      conversation_history: conversationHistory,
      retrieval_query: [
        ...conversationHistory.filter((message) => message.role === 'user').slice(-2).map((message) => message.content),
        prompt,
      ].join('\n').slice(0, 4_000),
      output_budget: outputBudget,
      response_mode: responseMode,
      source: 'apocky.com',
      privacy_class: 'restricted',
      memory_scope: 'owner-authorized',
    };
    let replayed = false;
    let job;
    try {
      job = await enqueueApocryphaJob({
        identity,
        kind: 'apocky_chat' as ApocryphaJobKind,
        capability: 'apocky_owner_chat',
        request,
        idempotencyKey,
        idempotencyScope: `owner-chat:${conversationId}`,
        priority: 20,
      });
    } catch (enqueueError) {
      if (!String(enqueueError).includes('IDEMPOTENCY_CONFLICT')) throw enqueueError;
      const existing = await readOwnerChatIdempotentJob(identity, conversationId, idempotencyKey);
      const prior = existing?.request;
      if (!existing || !isSameOwnerChatRequest(prior, expectedRequest)) {
        throw enqueueError;
      }
      job = existing.job;
      replayed = true;
    }
    return res.status(202).json({ ok: true, accepted: true, replayed, conversation_id: conversationId, job });
  } catch (error) {
    return respondJobError(res, error);
  }
}

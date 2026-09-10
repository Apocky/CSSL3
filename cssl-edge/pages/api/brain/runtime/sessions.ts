import type { NextApiRequest, NextApiResponse } from 'next';

import { isOwnerBrainConversationId, isOwnerBrainHistoryCursor, RuntimeProxyError } from '@/lib/apocv4/runtime-proxy';
import { requireBrainOwner, respondBrainOwnerFailure, setBrainPrivateHeaders } from '@/lib/brain/owner';
import {
  getOwnerBrainSession,
  listOwnerBrainSessions,
  ownerBrainRuntimeConfigured,
} from '@/lib/brain/runtime-provider';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
    return;
  }
  const owner = await requireBrainOwner(req);
  if (!owner.ok) {
    respondBrainOwnerFailure(res, owner);
    return;
  }
  if (!ownerBrainRuntimeConfigured()) {
    res.status(503).json({ error: 'The free local Apocv4 provider is not connected.', code: 'BRAIN_LOCAL_PROVIDER_DISABLED', ...envelope() });
    return;
  }
  const queryKeys = Object.keys(req.query);
  const getOne = queryKeys.includes('session_id');
  const sessionId = getOne && queryKeys.length === 1 && isOwnerBrainConversationId(req.query.session_id)
    ? String(req.query.session_id).toLowerCase() : null;
  const cursor = req.query.cursor === undefined ? null : req.query.cursor;
  const rawLimit = req.query.limit === undefined ? '24' : req.query.limit;
  const limit = typeof rawLimit === 'string' && /^(?:[1-9]|[12][0-9]|3[0-2])$/.test(rawLimit) ? Number(rawLimit) : null;
  if (getOne ? sessionId === null : queryKeys.some(key => !['cursor', 'limit'].includes(key))
    || (cursor !== null && !isOwnerBrainHistoryCursor(cursor)) || limit === null) {
    res.status(400).json({ error: 'Request one conversation, or a valid conversation history page.', code: 'BRAIN_SESSION_INVALID', ...envelope() });
    return;
  }

  try {
    const projection = sessionId
      ? await getOwnerBrainSession(owner.user.id, sessionId)
      : await listOwnerBrainSessions(owner.user.id, undefined, { cursor: cursor as string | null, limit: limit ?? 24 });
    res.status(200).json({
      schema_version: sessionId ? 'apocky.owner-brain.session.v1' : 'apocky.owner-brain.sessions.v1',
      status: 'live',
      history_surface: 'g12_chat_history',
      ...(projection.kind === 'owner_brain_history_get'
        ? { session: projection.session }
        : {
          sessions: projection.sessions,
          count: projection.count,
          discovery_scope: projection.discovery_scope,
          next_cursor: projection.next_cursor,
          has_more: projection.has_more,
        }),
      ...envelope(),
    });
  } catch (error) {
    const code = error instanceof RuntimeProxyError ? error.code : 'runtime_unreachable';
    const status = error instanceof RuntimeProxyError && error.publicStatus === 404 ? 404 : 502;
    res.status(status).json({
      error: 'The local Apocv4 conversation history is unavailable.',
      code: `BRAIN_${code.toUpperCase()}`,
      ...envelope(),
    });
  }
}

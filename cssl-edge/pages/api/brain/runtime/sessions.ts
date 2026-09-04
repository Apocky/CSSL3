import type { NextApiRequest, NextApiResponse } from 'next';

import { isOpaqueConversationId } from '@/lib/apocrypha/proxy';
import { RuntimeProxyError } from '@/lib/apocv4/runtime-proxy';
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
  const sessionId = queryKeys.length === 0
    ? null
    : queryKeys.length === 1 && queryKeys[0] === 'session_id' && isOpaqueConversationId(req.query.session_id)
      ? String(req.query.session_id).toLowerCase()
      : undefined;
  if (sessionId === undefined) {
    res.status(400).json({ error: 'Only one opaque session_id may be requested.', code: 'BRAIN_SESSION_INVALID', ...envelope() });
    return;
  }

  try {
    const projection = sessionId
      ? await getOwnerBrainSession(owner.user.id, sessionId)
      : await listOwnerBrainSessions(owner.user.id);
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

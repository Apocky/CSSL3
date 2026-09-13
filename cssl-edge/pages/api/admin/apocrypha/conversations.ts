import type { NextApiRequest, NextApiResponse } from 'next';

import {
  listOwnerChatConversations,
  readOwnerChatConversation,
} from '@/lib/apocrypha/job-control';
import {
  methodNotAllowed,
  requireOwnerIdentity,
  respondJobError,
} from '@/lib/apocrypha/job-http';
import { isOpaqueConversationId, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { ownerChatConversationVisible } from '@/lib/apocrypha/owner-oracle-control';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  if (req.method !== 'GET') return void methodNotAllowed(res, ['GET']);

  try {
    const identity = await requireOwnerIdentity(req, res);
    if (!identity) return;
    const keys = Object.keys(req.query);
    if (Object.prototype.hasOwnProperty.call(req.query, 'id')) {
      const id = req.query.id;
      if (keys.length !== 1 || typeof id !== 'string' || !isOpaqueConversationId(id)) {
        res.status(400).json({ ok: false, code: 'CONVERSATION_ID_INVALID', ...envelope() });
        return;
      }
      if (!await ownerChatConversationVisible(identity, id)) {
        res.status(404).json({ ok: false, code: 'CONVERSATION_NOT_FOUND', ...envelope() });
        return;
      }
      const conversation = await readOwnerChatConversation(identity, id);
      if (!conversation) {
        res.status(404).json({ ok: false, code: 'CONVERSATION_NOT_FOUND', ...envelope() });
        return;
      }
      res.status(200).json({
        ok: true,
        upstream_status: 200,
        data: conversation,
        ...envelope(),
      });
      return;
    }

    const rawScope = req.query.scope;
    const requestedScope = typeof rawScope === 'string' ? rawScope : 'active';
    if (keys.some((key) => key !== 'scope')
      || (rawScope !== undefined && typeof rawScope !== 'string')
      || requestedScope !== 'active') {
      res.status(400).json({ ok: false, code: 'CONVERSATION_SCOPE_INVALID', ...envelope() });
      return;
    }
    const conversations = await listOwnerChatConversations(identity);
    res.status(200).json({
      ok: true,
      upstream_status: 200,
      data: conversations,
      ...envelope(),
    });
  } catch (error) {
    respondJobError(res, error);
  }
}

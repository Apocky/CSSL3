// POST a signed-out turn onto the durable queue.
//
// Same worker as a member, different tenant. The guest identity is minted and read server-side from
// an HttpOnly cookie, so the browser never chooses who it is -- a caller cannot present someone
// else's identity because it never handles one.

import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import {
  GuestChatError, enqueueGuestChat, guestCookie, newGuestId, readGuestCookie,
  type GuestTurn,
} from '@/lib/apocrypha/guest-chat';

export const config = { api: { bodyParser: { sizeLimit: '64kb' } } };

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function history(value: unknown): GuestTurn[] {
  if (!Array.isArray(value)) return [];
  const turns: GuestTurn[] = [];
  for (const item of value.slice(-24)) {
    if (item === null || typeof item !== 'object') continue;
    const record = item as Record<string, unknown>;
    const content = typeof record.content === 'string' ? record.content : '';
    if (content.trim() === '') continue;
    turns.push({ role: record.role === 'assistant' ? 'assistant' : 'user', content });
  }
  return turns;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'Cookie');
  res.setHeader('X-Content-Type-Options', 'nosniff');

  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ ok: false, code: 'ORIGIN_REQUIRED' });
    return;
  }

  const existing = readGuestCookie(req.headers.cookie);
  const guestId = existing ?? newGuestId();
  if (!existing) {
    res.setHeader('Set-Cookie', guestCookie(guestId, process.env.NODE_ENV === 'production'));
  }

  const body = (req.body ?? {}) as Record<string, unknown>;
  const message = typeof body.message === 'string' ? body.message.trim() : '';
  const requestId = typeof body.request_id === 'string' ? body.request_id : '';
  if (!UUID.test(requestId)) {
    res.status(400).json({ ok: false, code: 'REQUEST_ID_INVALID' });
    return;
  }
  if (message === '' || Buffer.byteLength(message, 'utf8') > 8_192) {
    res.status(400).json({ ok: false, code: 'MESSAGE_INVALID' });
    return;
  }

  try {
    const receipt = await enqueueGuestChat({
      guestId, requestId, message, history: history(body.history),
    });
    res.status(202).json({ ok: true, ...receipt });
  } catch (error) {
    if (error instanceof GuestChatError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(502).json({ ok: false, code: 'GUEST_CHAT_UNAVAILABLE' });
  }
}

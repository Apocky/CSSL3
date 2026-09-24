// POST /api/room/say {room, body}
//
// A human speaks. The owner principal is 'apocky'; everyone else is a guest, identified by the
// same HttpOnly cookie the guest chat mints, digested before it becomes an author label. Guests
// may not enter the owner room and may not write faster than one row per two seconds.

import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import { guestCookie, newGuestId, readGuestCookie } from '@/lib/apocrypha/guest-chat';
import {
  GUEST_MIN_GAP_MS, MAX_SAY_CHARS, RoomError, guestAuthor, insertEvent, isOwner, lastWriteAt, parseRoom,
} from '@/lib/room/store';

export const config = { api: { bodyParser: { sizeLimit: '32kb' } } };

function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
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
  try {
    const input = record(req.body);
    const room = parseRoom(input.room);
    const body = typeof input.body === 'string' ? input.body.trim() : '';
    if (body === '') throw new RoomError(400, 'BODY_EMPTY', 'That message was empty.');
    if (body.length > MAX_SAY_CHARS) {
      throw new RoomError(400, 'BODY_TOO_LONG', `That message is too long (${MAX_SAY_CHARS} characters at most).`);
    }

    const owner = await isOwner(req);
    let author = 'apocky';
    if (!owner) {
      if (room === 'owner') throw new RoomError(403, 'OWNER_REQUIRED', 'That room is private.');
      const existing = readGuestCookie(req.headers.cookie);
      const guestId = existing ?? newGuestId();
      if (!existing) res.setHeader('Set-Cookie', guestCookie(guestId, process.env.NODE_ENV === 'production'));
      author = guestAuthor(guestId);
      const last = await lastWriteAt(author);
      if (last !== null && Date.now() - last < GUEST_MIN_GAP_MS) {
        res.setHeader('Retry-After', '2');
        throw new RoomError(429, 'TOO_FAST', 'One message every two seconds.');
      }
    }

    const event = await insertEvent({ room, author, kind: 'utterance', body, meta: {} });
    res.status(201).json({ ok: true, event });
  } catch (error) {
    if (error instanceof RoomError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE' });
  }
}

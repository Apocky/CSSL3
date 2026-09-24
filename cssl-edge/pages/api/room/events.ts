// GET /api/room/events?room=lobby|owner&after=<id>&limit=<=200[&who=1]
//
// The river's poll. Lobby is open to anyone; owner needs the owner principal and answers 403 to
// everyone else, guests included -- not 401, because there is nothing a guest could sign in AS
// that would open it. `who=1` on the first poll tells the page whether it is the owner, so the
// room switch appears without a second endpoint and without resolving the principal every 1.5 s.

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  RoomError, isOwner, listEvents, newestPresence, parseId, parseLimit, parseRoom,
} from '@/lib/room/store';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  try {
    const room = parseRoom(req.query.room);
    const after = parseId(req.query.after);
    const limit = parseLimit(req.query.limit);
    const who = req.query.who === '1';
    let owner: boolean | null = null;
    if (room === 'owner' || who) {
      owner = await isOwner(req);
      if (room === 'owner' && !owner) {
        res.status(403).json({ ok: false, code: 'OWNER_REQUIRED', error: 'That room is private.' });
        return;
      }
    }
    const [events, presence] = await Promise.all([listEvents(room, after, limit), newestPresence(room)]);
    res.status(200).json({
      ok: true,
      events,
      presence,
      now: new Date().toISOString(),
      ...(who ? { viewer: { owner: owner === true } } : {}),
    });
  } catch (error) {
    if (error instanceof RoomError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE' });
  }
}

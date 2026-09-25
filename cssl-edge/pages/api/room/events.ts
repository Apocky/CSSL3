// GET /api/room/events?room=lobby|owner&after=<id>&limit=<=200[&who=1]
//
// The river's poll. Lobby is open to anyone; owner needs the owner principal and answers 403 to
// everyone else. Each poll also carries the room's in-flight turns (jobs still being answered,
// with the text streamed so far) so every reader watches an answer arrive, and presence is derived
// from them before falling back to the loop's last presence row. `who=1` on the first poll tells
// the page who is reading: owner, signed-in member or guest, their author label (so their own
// messages sit on the right), and whether Premium is theirs and connected.

import type { NextApiRequest, NextApiResponse } from 'next';

import { namesFor, resolveRoomKey } from '@/lib/room/rooms';
import {
  RoomError, listEvents, newestPresence, parseId, parseLimit, parseRoom,
} from '@/lib/room/store';
import { directViewer } from '@/lib/direct/mode';
import { flagshipAllowed, flagshipReady, liveTurns, resolveSpeaker } from '@/lib/room/turn';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  try {
    const after = parseId(req.query.after);
    const limit = parseLimit(req.query.limit);
    const who = req.query.who === '1';
    // Signed-in only (owner decision 2026-09-25), and only rooms this account belongs to.
    const reader = await resolveSpeaker(req, { mintGuest: false });
    if (reader.kind === 'guest') {
      res.status(401).json({ ok: false, code: 'SIGN_IN_REQUIRED', error: 'Sign in to read the room.' });
      return;
    }
    const requested = typeof req.query.room === 'string' ? req.query.room : 'me';
    const access = await resolveRoomKey(reader, requested === 'me' ? 'me' : parseRoom(requested));
    const room = access.key;
    const [events, lastPresence, live] = await Promise.all([
      listEvents(room, after, limit), newestPresence(room), liveTurns(room),
    ]);
    const now = new Date().toISOString();
    const presence = live.length > 0
      ? { state: live.some((turn) => turn.text !== '') ? 'speaking' : 'thinking', at: now }
      : lastPresence;
    let viewer: Record<string, unknown> | undefined;
    if (who) {
      const speaker = reader;
      viewer = {
        owner: speaker.kind === 'owner',
        kind: speaker.kind,
        signed_in: speaker.kind !== 'guest',
        author: speaker.author || null,
        premium: await flagshipAllowed(speaker),
        premium_ready: flagshipReady(req),
        direct: directViewer(req).owner,
      };
    }
    const names = await namesFor(events.map((e) => e.author));
    res.status(200).json({ ok: true, room: { key: room, kind: access.kind, role: access.role, quiet: access.quiet }, events, live, presence, names, now, ...(viewer ? { viewer } : {}) });
  } catch (error) {
    if (error instanceof RoomError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE' });
  }
}

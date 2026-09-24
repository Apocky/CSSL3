// GET /api/room/worker/pull?after=<id>              -> the inbox: everyone-but-Apocrypha, both rooms
// GET /api/room/worker/pull?room=<r>&tail=<n>       -> the last n rows of one room, any author
//
// Worker token auth, exactly as the job control plane admits a node. The tail form exists because
// the loop needs owner-room history for the owner-room prompt and the public events route will
// not hand owner rows to anything but the owner principal.

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  MAX_EVENTS_PAGE, RoomError, listEvents, listInbox, parseId, parseLimit, parseRoom, requireRoomWorker,
} from '@/lib/room/store';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  try {
    const nodeId = await requireRoomWorker(req);
    const tail = req.query.tail;
    if (tail !== undefined) {
      const room = parseRoom(req.query.room);
      const limit = parseLimit(tail, 20);
      const events = await listEvents(room, 0, limit);
      res.status(200).json({ ok: true, node_id: nodeId, room, events, now: new Date().toISOString() });
      return;
    }
    const after = parseId(req.query.after);
    const limit = parseLimit(req.query.limit, MAX_EVENTS_PAGE);
    const events = await listInbox(after, limit);
    res.status(200).json({ ok: true, node_id: nodeId, events, now: new Date().toISOString() });
  } catch (error) {
    if (error instanceof RoomError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE' });
  }
}

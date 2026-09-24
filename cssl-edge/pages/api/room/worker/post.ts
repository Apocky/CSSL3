// POST /api/room/worker/post {node_id, room, kind, body, meta}
//
// Apocrypha speaks, or changes state. Only an admitted worker node may write as 'apocrypha';
// the author is fixed here, never taken from the body.

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  MAX_WORKER_BODY_CHARS, RoomError, insertEvent, isKind, parseRoom, requireRoomWorker,
} from '@/lib/room/store';

export const config = { api: { bodyParser: { sizeLimit: '64kb' } } };

function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  try {
    await requireRoomWorker(req);
    const input = record(req.body);
    const room = parseRoom(input.room);
    const kind = input.kind ?? 'utterance';
    if (!isKind(kind)) throw new RoomError(400, 'KIND_INVALID', 'kind must be utterance, thought, recall, presence or system');
    const body = typeof input.body === 'string' ? input.body.trim() : '';
    if (kind !== 'presence' && body === '') throw new RoomError(400, 'BODY_EMPTY', 'body is required');
    if (body.length > MAX_WORKER_BODY_CHARS) throw new RoomError(400, 'BODY_TOO_LONG', 'body is too long');
    const meta = record(input.meta);
    if (JSON.stringify(meta).length > 4_000) throw new RoomError(400, 'META_TOO_LONG', 'meta is too long');
    const event = await insertEvent({ room, author: 'apocrypha', kind, body, meta });
    res.status(201).json({ ok: true, event });
  } catch (error) {
    if (error instanceof RoomError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE' });
  }
}

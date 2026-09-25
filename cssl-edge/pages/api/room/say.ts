// POST /api/room/say {room, body, engine_lane?, attachment_ids?, tools?}
//
// A human speaks, and the message becomes a job (migration 0061): the row and its job are written
// in one transaction, then the PC worker (local lane) or the Vercel runner (flagship lane) answers
// and the answer lands in the river by trigger. The speaker is decided from the session: the owner
// principal is 'apocky', a signed-in member is 'member:<digest>', anyone else is a guest identified
// by the HttpOnly guest cookie, digested before it becomes an author label.

import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import { MAX_SAY_CHARS, RoomError, parseRoom } from '@/lib/room/store';
import { ROOM_TOOLS, flagshipReady, resolveSpeaker, say, type EngineLane, type RoomTool } from '@/lib/room/turn';

export const config = { api: { bodyParser: { sizeLimit: '32kb' } } };

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

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
    const lane: EngineLane = input.engine_lane === 'flagship' ? 'flagship' : 'local';
    const attachmentIds = Array.isArray(input.attachment_ids)
      ? input.attachment_ids.filter((id): id is string => typeof id === 'string' && UUID_RE.test(id)).map((id) => id.toLowerCase())
      : [];
    const tools = Array.isArray(input.tools)
      ? input.tools.filter((tool): tool is RoomTool => (ROOM_TOOLS as readonly unknown[]).includes(tool))
      : [];

    const speaker = await resolveSpeaker(req, { mintGuest: false });
    if (speaker.kind === 'guest') throw new RoomError(401, 'SIGN_IN_REQUIRED', 'Sign in to talk in the room.');
    if (lane === 'flagship' && !flagshipReady(req)) {
      throw new RoomError(503, 'PREMIUM_OFFLINE', 'Apocrypha+ is not connected right now. Switch to Local, or try again soon.');
    }

    const receipt = await say(speaker, { room, body, lane, attachmentIds, tools });
    res.status(201).json({ ok: true, event: receipt.event, job: receipt.job });
  } catch (error) {
    if (error instanceof RoomError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    console.error(JSON.stringify({ at: new Date().toISOString(), level: 'error', event: 'room.say.failed', detail: String(error).slice(0, 300) }));
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE', error: 'The room could not take that message right now.' });
  }
}

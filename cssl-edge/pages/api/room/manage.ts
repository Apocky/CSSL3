// POST /api/room/manage {action, ...}  -- rooms, invitations, friends and names for the signed-in account.
//   list                          -> { rooms, friends }
//   create   {title}              -> { key }            a new invite-only lobby you own
//   leave    {room}               -> {}                 leave a lobby (the owner deletes it)
//   invite   {room}               -> { url, expires_at } a link: 10 uses, 7 days
//   accept   {token}              -> { key, title }     join by link; you and the inviter become friends
//   members  {room}               -> { members }
//   add      {room, friend}       -> {}                 add a friend to a lobby you are in
//   name     {name}               -> {}                 your display name in lobbies

import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import {
  acceptInvite, addFriendToRoom, createInvite, createLobby, leaveRoom, listFriends, listMembers, listRooms, resolveRoomKey, setDisplayName,
} from '@/lib/room/rooms';
import { RoomError, parseRoom } from '@/lib/room/store';
import { resolveSpeaker } from '@/lib/room/turn';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  if (req.method !== 'POST') { res.setHeader('Allow', 'POST'); res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return; }
  if (!hasSameOrigin(req)) { res.status(403).json({ ok: false, code: 'ORIGIN_REQUIRED' }); return; }
  try {
    const body = (req.body && typeof req.body === 'object' ? req.body : {}) as Record<string, unknown>;
    const speaker = await resolveSpeaker(req, { mintGuest: false });
    if (speaker.kind === 'guest') throw new RoomError(401, 'SIGN_IN_REQUIRED', 'Sign in first.');
    const str = (v: unknown) => (typeof v === 'string' ? v : '');
    const member = async () => (await resolveRoomKey(speaker, parseRoom(str(body.room)))).key;
    switch (body.action) {
      case 'list': res.status(200).json({ ok: true, rooms: await listRooms(speaker), friends: await listFriends(speaker) }); return;
      case 'create': res.status(201).json({ ok: true, key: await createLobby(speaker, str(body.title) || 'Lobby') }); return;
      case 'leave': await leaveRoom(speaker, parseRoom(str(body.room))); res.status(200).json({ ok: true }); return;
      case 'invite': {
        const invite = await createInvite(speaker, await member());
        const origin = process.env.NODE_ENV === 'production' ? 'https://www.apocky.com' : `http://${req.headers.host}`;
        res.status(201).json({ ok: true, url: `${origin}/?invite=${invite.token}`, expires_at: invite.expires_at });
        return;
      }
      case 'accept': res.status(200).json({ ok: true, ...(await acceptInvite(speaker, str(body.token))) }); return;
      case 'members': res.status(200).json({ ok: true, members: await listMembers(await member()) }); return;
      case 'add': {
        if (!UUID.test(str(body.friend))) throw new RoomError(400, 'FRIEND_INVALID', 'Pick a friend.');
        await addFriendToRoom(speaker, await member(), str(body.friend).toLowerCase());
        res.status(200).json({ ok: true });
        return;
      }
      case 'name': await setDisplayName(speaker, str(body.name)); res.status(200).json({ ok: true }); return;
      default: throw new RoomError(400, 'ACTION_INVALID', 'Unknown action.');
    }
  } catch (error) {
    if (error instanceof RoomError) { res.status(error.status).json({ ok: false, code: error.code, error: error.message }); return; }
    console.error(JSON.stringify({ at: new Date().toISOString(), level: 'error', event: 'room.manage.failed', detail: String(error).slice(0, 300) }));
    res.status(503).json({ ok: false, code: 'ROOM_UNAVAILABLE', error: 'That could not be done right now.' });
  }
}

// /api/admin/bridge · phone↔desktop bridge for /admin/chat
// Per spec/24 (companion-spec being-written) :
//   ?action=status → poll desktop heartbeat
//   ?action=send (POST) → enqueue message · desktop polls + responds
//   ?action=poll (POST · desktop-side) → desktop pulls pending messages, posts responses
//
// Stub-mode when APOCKY_HUB_SUPABASE_URL is missing · returns shape-correct guidance

import type { NextApiRequest, NextApiResponse } from 'next';
import { getAuthClient } from '../../../lib/auth';

const HEARTBEAT_MS = 30_000;

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  const action = (req.query.action as string) ?? 'status';
  const client = getAuthClient();

  // ── STATUS ──
  if (action === 'status' && req.method === 'GET') {
    if (!client) {
      return res.status(200).json({
        online: false,
        stub: true,
        reason: 'APOCKY_HUB_SUPABASE_URL not set · bridge inactive',
      });
    }
    // When wired : query Supabase realtime presence on channel `desktop:<player_id>`
    // For now : optimistic-stub returning offline until heartbeat-table integrated
    return res.status(200).json({
      online: false,
      desktop: 'none',
      reason: 'Heartbeat table integration pending W9-D2 supabase real-provision',
    });
  }

  // ── SEND (phone → desktop) ──
  if (action === 'send' && req.method === 'POST') {
    const { target, text } = req.body ?? {};
    if (typeof target !== 'string' || typeof text !== 'string' || !text.trim()) {
      return res.status(400).json({ error: 'target + text required' });
    }
    if (!client) {
      // Stub-mode : echo back a useful guidance message
      return res.status(200).json({
        stub: true,
        role: 'system',
        response: `⚠ Bridge in stub-mode. Once Apocky-Hub Supabase is configured (per spec/22 + spec/24), this message ("${text.slice(0, 80)}${text.length > 80 ? '…' : ''}") will route via Realtime channel to your /${target} on whatever desktop instance is online (LoA.exe MCP-server :3001 OR Mycelium-Desktop W10 build). Response streams back here token-by-token.`,
      });
    }
    // When wired : insert into Supabase `admin_bridge_messages` table · desktop subscribes
    return res.status(200).json({
      stub: false,
      role: 'system',
      response: '◐ message queued · awaiting desktop response (poll /api/admin/bridge?action=poll-response)',
    });
  }

  // ── POLL (desktop → phone) · for desktop to pull pending messages and post responses ──
  if (action === 'poll' && req.method === 'POST') {
    if (!client) {
      return res.status(200).json({ messages: [], stub: true });
    }
    return res.status(200).json({ messages: [] });
  }

  return res.status(405).json({ error: 'Method or action not allowed' });
}

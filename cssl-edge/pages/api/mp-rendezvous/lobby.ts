// cssl-edge · /api/mp-rendezvous/lobby
// Sync-coop OR mmo-shard lobby-find. Cap-gated MP_CAP_RENDEZVOUS · 0x100.
// Sovereign-bypass supported via x-loa-sovereign-cap header.
//
// Mode discriminator :
//   - sync-coop : small-group co-op rendezvous (≤8 peers)
//   - mmo-shard : ambient-presence shard (capped peer-count per shard)
//
// Methods :
//   - GET ?cap=&player_id=&mode=&shard_hint= → list lobbies
//   - POST { cap, player_id, mode, shard_hint? } → create-or-join lobby
//
// Gift-economy framing : NO matchmaking-ranking · NO competitive ladders.
// Lobbies are opt-in rendezvous · revocable · no commitment.

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { MP_CAP_RENDEZVOUS } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

export type LobbyMode = 'sync-coop' | 'mmo-shard';

export interface LobbyEntry {
  lobby_id: string;
  mode: LobbyMode;
  shard_hint: string;
  host_player_id: string;
  current_peers: number;
  max_peers: number;
  created_at: string;
}

interface LobbyListOk {
  served_by: string;
  ts: string;
  lobbies: LobbyEntry[];
  total: number;
  mode_filter: string;
  framing: 'gift-economy';
}

interface LobbyCreateOk {
  served_by: string;
  ts: string;
  lobby: LobbyEntry;
  joined: true;
  framing: 'gift-economy';
}

interface LobbyError {
  error: string;
  served_by: string;
  ts: string;
}

const ALLOWED_MODES: ReadonlySet<string> = new Set(['sync-coop', 'mmo-shard']);

// 6 stub lobbies · spans both modes + various peer counts.
const STUB_LOBBIES: ReadonlyArray<LobbyEntry> = [
  { lobby_id: 'lb-001', mode: 'sync-coop', shard_hint: 'glade', host_player_id: 'alice', current_peers: 2, max_peers: 4, created_at: '2026-04-30T10:00:00.000Z' },
  { lobby_id: 'lb-002', mode: 'sync-coop', shard_hint: 'tide', host_player_id: 'bob', current_peers: 3, max_peers: 8, created_at: '2026-04-30T10:30:00.000Z' },
  { lobby_id: 'lb-003', mode: 'mmo-shard', shard_hint: 'mossroot-east', host_player_id: 'carol', current_peers: 14, max_peers: 32, created_at: '2026-04-30T11:00:00.000Z' },
  { lobby_id: 'lb-004', mode: 'mmo-shard', shard_hint: 'glassroot-north', host_player_id: 'dave', current_peers: 22, max_peers: 32, created_at: '2026-04-30T11:30:00.000Z' },
  { lobby_id: 'lb-005', mode: 'sync-coop', shard_hint: 'glade', host_player_id: 'eve', current_peers: 1, max_peers: 4, created_at: '2026-04-30T12:00:00.000Z' },
  { lobby_id: 'lb-006', mode: 'mmo-shard', shard_hint: 'sun-coast', host_player_id: 'frank', current_peers: 8, max_peers: 32, created_at: '2026-04-30T12:30:00.000Z' },
];

function readQuery(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

function genLobbyId(): string {
  const r = Math.floor(Math.random() * 0xffffffffffff);
  return `lb-${r.toString(16).padStart(12, '0')}`;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<LobbyListOk | LobbyCreateOk | LobbyError>
): void {
  logHit('mp-rendezvous.lobby', { method: req.method ?? 'GET' });

  if (req.method !== 'GET' && req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'GET, POST');
    res.status(405).json({
      error: 'Method Not Allowed — GET (list) or POST (create-join)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // GET branch : list lobbies (read-only).
  if (req.method === 'GET') {
    const q = req.query as Record<string, string | string[] | undefined>;
    const capRaw = readQuery(q, 'cap');
    const cap = capRaw !== undefined ? parseInt(capRaw, 10) || 0 : 0;
    const sovereignRaw = readQuery(q, 'sovereign');
    const sovereignFlag = sovereignRaw === 'true';
    const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

    const capAllowed = (cap & MP_CAP_RENDEZVOUS) !== 0;
    if (!capAllowed && !sovereignAllowed) {
      const d = deny('cap MP_CAP_RENDEZVOUS=0x100 required (or sovereign-header)', cap);
      logEvent(d.body);
      const env = envelope();
      res.status(d.status).json({
        error: d.body.extra?.['reason'] as string ?? 'denied',
        served_by: env.served_by,
        ts: env.ts,
      });
      return;
    }

    const modeFilter = (readQuery(q, 'mode') ?? '').toLowerCase();
    let filtered: LobbyEntry[] = STUB_LOBBIES.slice();
    if (modeFilter.length > 0 && ALLOWED_MODES.has(modeFilter)) {
      filtered = filtered.filter((l) => l.mode === modeFilter);
    }

    logEvent(
      auditEvent('mp-rendezvous.list', cap, sovereignAllowed, 'ok', {
        mode_filter: modeFilter,
        returned: filtered.length,
      })
    );

    const env = envelope();
    res.status(200).json({
      served_by: env.served_by,
      ts: env.ts,
      lobbies: filtered,
      total: filtered.length,
      mode_filter: modeFilter,
      framing: 'gift-economy',
    });
    return;
  }

  // POST branch : create-or-join lobby.
  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON {cap, player_id, mode, shard_hint?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const reqBody = body as Record<string, unknown>;
  const cap = typeof reqBody['cap'] === 'number' ? (reqBody['cap'] as number) : 0;
  const sovereignFlag = reqBody['sovereign'] === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  const capAllowed = (cap & MP_CAP_RENDEZVOUS) !== 0;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap MP_CAP_RENDEZVOUS=0x100 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: d.body.extra?.['reason'] as string ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const player_id = typeof reqBody['player_id'] === 'string' ? (reqBody['player_id'] as string) : '';
  const mode = (typeof reqBody['mode'] === 'string' ? (reqBody['mode'] as string) : '').toLowerCase();
  const shard_hint = typeof reqBody['shard_hint'] === 'string' ? (reqBody['shard_hint'] as string) : 'default';

  if (player_id.length === 0 || !ALLOWED_MODES.has(mode)) {
    logEvent(
      auditEvent('mp-rendezvous.create', cap, sovereignAllowed, 'denied', {
        reason: 'player_id required + mode must be sync-coop|mmo-shard',
      })
    );
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — player_id required + mode must be sync-coop|mmo-shard',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const lobby: LobbyEntry = {
    lobby_id: genLobbyId(),
    mode: mode as LobbyMode,
    shard_hint,
    host_player_id: player_id,
    current_peers: 1,
    max_peers: mode === 'sync-coop' ? 8 : 32,
    created_at: new Date().toISOString(),
  };

  logEvent(
    auditEvent('mp-rendezvous.create', cap, sovereignAllowed, 'ok', {
      lobby_id: lobby.lobby_id,
      mode,
      shard_hint,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    lobby,
    joined: true,
    framing: 'gift-economy',
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(
  method: string,
  qOrBody: Record<string, string | string[]> | unknown = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const isGet = method === 'GET';
  const req = {
    method,
    query: isGet ? (qOrBody as Record<string, string | string[]>) : {},
    headers: {},
    body: isGet ? undefined : qOrBody,
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(key: string, val: string) { out.headers[key] = val; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. cap=0 → 403.
export function testCapsZeroDenies(): void {
  const { req, res, out } = mockReqRes('GET', { cap: '0' });
  handler(req, res);
  assert(out.statusCode === 403, `cap=0 must yield 403, got ${out.statusCode}`);
}

// 2. cap-bit set + GET → 200 listing with mode-filter + gift-economy framing.
export function testCapsSetReturnsLobbies(): void {
  const { req, res, out } = mockReqRes('GET', {
    cap: String(MP_CAP_RENDEZVOUS),
    mode: 'sync-coop',
  });
  handler(req, res);
  assert(out.statusCode === 200, `cap-set must yield 200, got ${out.statusCode}`);
  const b = out.body as LobbyListOk;
  assert(Array.isArray(b.lobbies), 'lobbies must be array');
  // Stub catalog has 3 sync-coop entries.
  assert(b.lobbies.length === 3, `expected 3 sync-coop lobbies, got ${b.lobbies.length}`);
  assert(b.framing === 'gift-economy', 'framing must be gift-economy');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCapsZeroDenies();
  testCapsSetReturnsLobbies();
  // eslint-disable-next-line no-console
  console.log('mp-rendezvous/lobby.ts : OK · 2 inline tests passed');
}

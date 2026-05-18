// apocky.com/api/admin/apocrypha/status · backend-reachability probe
//
// Phase-0 stub per Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl.
// Phase-1 will replace this with a real proxy to the cloudflared tunnel
// (env APOCRYPHA_TUNNEL_HOST, e.g. apocrypha.apocky.com) → localhost:8137/api/status.
//
// Until Phase-1 lands, this returns a deterministic stub so the cockpit
// placeholder page can show wiring is in place + announces the next gate.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/require-admin';

interface ApocryphaStatusResponse {
  phase: 'stub' | 'tunnel';
  reachable: boolean;
  tunnel_host: string | null;
  note: string;
  next_gate: string;
  spec: string;
  upstream_status?: number;
  upstream_payload?: unknown;
  upstream_error?: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  if (!(await requireAdmin(req, res))) return;

  const tunnelHost = process.env.APOCRYPHA_TUNNEL_HOST ?? null;

  if (!tunnelHost) {
    const stub: ApocryphaStatusResponse = {
      phase: 'stub',
      reachable: false,
      tunnel_host: null,
      note:
        'Phase-0 stub. Set APOCRYPHA_TUNNEL_HOST env-var (e.g. apocrypha.apocky.com) ' +
        'after Phase-1 wires cloudflared + CF Access. Apocrypha backend lives at ' +
        'localhost:8137 on Apocky-PC.',
      next_gate: 'G1 · Phase-1 · curl https://apocky.com/api/admin/apocrypha/status returns Apocrypha JSON',
      spec: 'Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl',
    };
    return res.status(200).json({ ...stub, ...envelope() });
  }

  // Phase-1 active path · proxy to cloudflared tunnel
  try {
    const upstream = await fetch(`https://${tunnelHost}/api/status`, {
      method: 'GET',
      headers: {
        'CF-Access-Client-Id': process.env.CF_ACCESS_CLIENT_ID ?? '',
        'CF-Access-Client-Secret': process.env.CF_ACCESS_CLIENT_SECRET ?? '',
        Accept: 'application/json',
      },
    });
    let payload: unknown;
    try {
      payload = await upstream.json();
    } catch {
      payload = await upstream.text();
    }
    const body: ApocryphaStatusResponse = {
      phase: 'tunnel',
      reachable: upstream.ok,
      tunnel_host: tunnelHost,
      note: upstream.ok
        ? 'live · proxied via cloudflared tunnel'
        : `upstream returned HTTP ${upstream.status}`,
      next_gate: 'G2 · Phase-1 · CF Access blocks non-Apocky principals',
      spec: 'Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl',
      upstream_status: upstream.status,
      upstream_payload: payload,
    };
    return res.status(200).json({ ...body, ...envelope() });
  } catch (err) {
    const body: ApocryphaStatusResponse = {
      phase: 'tunnel',
      reachable: false,
      tunnel_host: tunnelHost,
      note: 'tunnel proxy failed · cloudflared may be down OR Apocky-PC offline',
      next_gate: 'G1 · Phase-1 · check cloudflared service status',
      spec: 'Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl',
      upstream_error: err instanceof Error ? err.message : String(err),
    };
    return res.status(502).json({ ...body, ...envelope() });
  }
}

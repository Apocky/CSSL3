// apocky.com/api/admin/apocrypha/status · backend-reachability probe
//
// Live backend-reachability probe through the configured cloudflared tunnel.
// Missing configuration is a bounded 503, never a fake/stub success.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/require-admin';

interface ApocryphaStatusResponse {
  phase: 'tunnel';
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
    const unavailable: ApocryphaStatusResponse = {
      phase: 'tunnel',
      reachable: false,
      tunnel_host: null,
      note: 'Apocrypha tunnel is not configured; backend reachability is unknown.',
      next_gate: 'Configure APOCRYPHA_TUNNEL_HOST and verify the live tunnel.',
      spec: 'Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl',
    };
    return res.status(503).json({ ...unavailable, ...envelope() });
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

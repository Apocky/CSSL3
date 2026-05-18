// apocky.com/api/admin/apocrypha/chat_stream · SSE proxy → Apocrypha /api/v1/chat/stream
//
// Pipes the upstream SSE byte-stream through to the browser. Uses the Node.js runtime
// (not Edge) because we need response streaming + CF Access service-token headers.
//
// Per HANDOFF_v10 § TRACK-A polish-pass (modern chat UX w/ streaming).

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/require-admin';

export const config = {
  api: {
    responseLimit: false,    // streaming response ; no length cap
  },
};

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  if (!(await requireAdmin(req, res))) return;

  const tunnel = process.env.APOCRYPHA_TUNNEL_HOST;
  if (!tunnel) {
    return res.status(503).json({
      error: 'APOCRYPHA_TUNNEL_HOST unset ; cockpit cannot reach backend',
      ...envelope(),
    });
  }
  const cfId = process.env.CF_ACCESS_CLIENT_ID;
  const cfSecret = process.env.CF_ACCESS_CLIENT_SECRET;
  if (!cfId || !cfSecret) {
    return res.status(503).json({
      error: 'CF_ACCESS_CLIENT_ID/SECRET not configured',
      ...envelope(),
    });
  }

  try {
    const upstream = await fetch(`https://${tunnel}/api/v1/chat/stream`, {
      method: 'POST',
      headers: {
        'CF-Access-Client-Id': cfId,
        'CF-Access-Client-Secret': cfSecret,
        'Content-Type': 'application/json',
        Accept: 'text/event-stream',
      },
      body: JSON.stringify(req.body),
    });

    if (!upstream.ok || !upstream.body) {
      const text = await upstream.text();
      return res.status(upstream.status).json({
        error: `upstream HTTP ${upstream.status}`,
        detail: text.slice(0, 500),
        ...envelope(),
      });
    }

    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache, no-transform');
    res.setHeader('Connection', 'keep-alive');
    res.setHeader('X-Accel-Buffering', 'no');
    res.flushHeaders();

    const reader = upstream.body.getReader();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(Buffer.from(value));
    }
    res.end();
  } catch (err) {
    if (!res.headersSent) {
      res.status(502).json({
        error: 'apocrypha tunnel stream failed',
        detail: err instanceof Error ? err.message : String(err),
        ...envelope(),
      });
    } else {
      // Already streaming — write an SSE error event then close
      res.write(`event: error\ndata: ${JSON.stringify({ error: err instanceof Error ? err.message : String(err) })}\n\n`);
      res.end();
    }
  }
}

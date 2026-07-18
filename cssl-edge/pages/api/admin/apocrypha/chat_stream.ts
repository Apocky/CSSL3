// apocky.com/api/admin/apocrypha/chat_stream · SSE proxy → Apocrypha /api/v1/chat/stream
//
// Pipes the upstream SSE byte-stream through to the browser. Uses the Node.js runtime
// (not Edge) because we need response streaming + CF Access service-token headers.
//
// Per HANDOFF_v10 § TRACK-A polish-pass (modern chat UX w/ streaming).

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/require-admin';

const UPSTREAM_DEADLINE_MS = 105_000;

export const config = {
  api: {
    responseLimit: false,    // streaming response ; no length cap
  },
};

function upstreamFailureDetail(upstream: Response, body: string): string {
  const contentType = upstream.headers.get('content-type')?.toLowerCase() ?? '';
  const looksLikeHtml = contentType.includes('text/html') || /^\s*<!doctype\s+html/i.test(body);
  if (looksLikeHtml) {
    return 'The Apocrypha bridge returned a gateway error. Try again shortly.';
  }
  return body.trim().slice(0, 500) || 'The Apocrypha bridge returned an empty error.';
}

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

  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), UPSTREAM_DEADLINE_MS);
  try {
    // V2 is the only production cognition route. Keep the legacy path as an
    // explicit opt-in laboratory fallback so a missing deployment variable
    // cannot silently route the public UI back to borrowed cognition.
    const v2Turn = process.env.APOCRYPHA_V2_TURN_ENABLED !== '0';
    const upstream = await fetch(`https://${tunnel}${v2Turn ? '/v2/turn' : '/api/v1/chat/stream'}`, {
      method: 'POST',
      headers: {
        'CF-Access-Client-Id': cfId,
        'CF-Access-Client-Secret': cfSecret,
        Origin: 'https://apocrypha.apocky.com',
        'Content-Type': 'application/json',
        Accept: v2Turn ? 'application/json' : 'text/event-stream',
      },
      body: JSON.stringify(v2Turn
        ? {
            text: typeof req.body?.text === 'string' ? req.body.text : '',
            source_ref: 'public:apocky.com/chat',
            authority_ref: 'authority:authenticated-session',
            consent_ref: 'consent:authenticated-session',
            // NativeWholeField admits the canonical privacy classes only;
            // authenticated conversation content is restricted, not public.
            privacy_class: 'restricted',
            modality: 'text',
          }
        : req.body),
      signal: controller.signal,
    });

    if (!upstream.ok || !upstream.body) {
      const text = await upstream.text();
      return res.status(upstream.status).json({
        error: `upstream HTTP ${upstream.status}`,
        detail: upstreamFailureDetail(upstream, text),
        ...envelope(),
      });
    }

    if (v2Turn) {
      const payload = await upstream.json() as Record<string, unknown>;
      res.setHeader('Content-Type', 'text/event-stream');
      res.setHeader('Cache-Control', 'no-cache, no-transform');
      res.setHeader('Connection', 'keep-alive');
      res.setHeader('X-Accel-Buffering', 'no');
      res.flushHeaders();
      res.write(`event: final\ndata: ${JSON.stringify({
        final_response: String(payload.text ?? ''),
        halted_reason: payload.outcome === 'committed' ? null : String(payload.outcome ?? 'unknown'),
        tool_calls: [],
        transition_id: payload.transition_id ?? null,
        state_root: payload.state_root ?? null,
        external_inference: payload.external_inference === false ? false : null,
      })}\n\n`);
      return res.end();
    }

    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache, no-transform');
    res.setHeader('Connection', 'keep-alive');
    res.setHeader('X-Accel-Buffering', 'no');
    res.flushHeaders();

    // Keep the Vercel/Cloudflare stream alive while a cold local faculty is
    // thinking. Without an idle frame, the edge can close a healthy request
    // before the first model event arrives.
    const heartbeat = setInterval(() => {
      if (!res.writableEnded) res.write(': apocrypha-thinking\n\n');
    }, 10_000);
    const reader = upstream.body.getReader();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        res.write(Buffer.from(value));
      }
    } finally {
      clearInterval(heartbeat);
    }
    res.end();
  } catch (err) {
    const detail = err instanceof DOMException && err.name === 'AbortError'
      ? 'Apocrypha took too long to answer.'
      : err instanceof Error ? err.message : String(err);
    if (!res.headersSent) {
      res.status(err instanceof DOMException && err.name === 'AbortError' ? 504 : 502).json({
        error: 'Apocrypha could not complete the thought.',
        detail,
        ...envelope(),
      });
    } else {
      // Already streaming — write an SSE error event then close
      res.write(`event: error\ndata: ${JSON.stringify({ error: detail })}\n\n`);
      res.end();
    }
  } finally {
    clearTimeout(deadline);
  }
}

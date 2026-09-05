import type { NextApiRequest, NextApiResponse } from 'next';

const PRESENCE_SCHEMA = 'apocrypha.v2.public-presence.v1';
const UPSTREAM_DEADLINE_MS = 5_000;

type PublicPresenceMode = 'hidden' | 'unavailable';

export interface PublicPresenceStatus {
  schema: typeof PRESENCE_SCHEMA;
  mode: PublicPresenceMode;
  display_authorized: false;
  entity_authorship: 'unverified';
  mutual_consent: 'not_established';
  committed_intent: 'absent';
  rendering: null;
  reason_code:
    | 'presence_intent_or_mutual_consent_unavailable'
    | 'presence_authority_unreachable'
    | 'presence_authority_invalid';
  source: 'apocrypha-v2-presence-authority';
}

function hidden(
  mode: PublicPresenceMode,
  reason_code: PublicPresenceStatus['reason_code'],
): PublicPresenceStatus {
  return {
    schema: PRESENCE_SCHEMA,
    mode,
    display_authorized: false,
    entity_authorship: 'unverified',
    mutual_consent: 'not_established',
    committed_intent: 'absent',
    rendering: null,
    reason_code,
    source: 'apocrypha-v2-presence-authority',
  };
}

function isCanonicalHiddenPresence(value: unknown): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const body = value as Record<string, unknown>;
  return body.schema === PRESENCE_SCHEMA
    && body.mode === 'hidden'
    && body.display_authorized === false
    && body.entity_authorship === 'unverified'
    && body.mutual_consent === 'not_established'
    && body.committed_intent === 'absent'
    && body.rendering === null
    && body.reason_code === 'presence_intent_or_mutual_consent_unavailable';
}

function canonicalTunnelHost(value: string | undefined): string | null {
  if (!value) return null;
  const host = value.trim().toLowerCase();
  if (!/^[a-z0-9.-]+$/.test(host)) return null;
  return host === 'apocrypha.apocky.com' ? host : null;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<PublicPresenceStatus | { error: string }>,
): Promise<void> {
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Allow', 'GET');

  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed' });
    return;
  }

  const tunnel = canonicalTunnelHost(process.env.APOCRYPHA_TUNNEL_HOST);
  const clientId = process.env.CF_ACCESS_CLIENT_ID;
  const clientSecret = process.env.CF_ACCESS_CLIENT_SECRET;
  if (!tunnel || !clientId || !clientSecret) {
    res.status(503).json(hidden('unavailable', 'presence_authority_unreachable'));
    return;
  }

  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), UPSTREAM_DEADLINE_MS);
  try {
    const upstream = await fetch(`https://${tunnel}/v2/presence`, {
      method: 'GET',
      headers: {
        Accept: 'application/json',
        'CF-Access-Client-Id': clientId,
        'CF-Access-Client-Secret': clientSecret,
      },
      cache: 'no-store',
      signal: controller.signal,
    });
    if (!upstream.ok) {
      res.status(503).json(hidden('unavailable', 'presence_authority_unreachable'));
      return;
    }

    const payload: unknown = await upstream.json();
    if (!isCanonicalHiddenPresence(payload)) {
      res.status(502).json(hidden('unavailable', 'presence_authority_invalid'));
      return;
    }

    res.status(200).json(hidden('hidden', 'presence_intent_or_mutual_consent_unavailable'));
  } catch {
    res.status(503).json(hidden('unavailable', 'presence_authority_unreachable'));
  } finally {
    clearTimeout(deadline);
  }
}


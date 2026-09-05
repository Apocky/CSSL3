// Sanitized public projection of Apocrypha's display authority.
// Raw presence state never crosses this boundary. The current canonical body
// can only prove that display is hidden, so every uncertainty fails hidden.

import type { NextApiRequest, NextApiResponse } from 'next';

const PRESENCE_SCHEMA = 'apocrypha.v2.public-presence.v1';
const CANONICAL_TUNNEL_HOST = 'apocrypha.apocky.com';
const UPSTREAM_DEADLINE_MS = 5_000;
const MAX_PRESENCE_BYTES = 4_096;

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
  reasonCode: PublicPresenceStatus['reason_code'],
): PublicPresenceStatus {
  return {
    schema: PRESENCE_SCHEMA,
    mode,
    display_authorized: false,
    entity_authorship: 'unverified',
    mutual_consent: 'not_established',
    committed_intent: 'absent',
    rendering: null,
    reason_code: reasonCode,
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

function configuredTunnelHost(): string | null {
  const value = process.env.APOCRYPHA_TUNNEL_HOST?.trim().toLowerCase();
  if (!value || !/^[a-z0-9.-]+$/.test(value)) return null;
  return value === CANONICAL_TUNNEL_HOST ? value : null;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<PublicPresenceStatus | { error: string }>,
): Promise<void> {
  res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed' });
    return;
  }

  const tunnelHost = configuredTunnelHost();
  const clientId = process.env.CF_ACCESS_CLIENT_ID?.trim();
  const clientSecret = process.env.CF_ACCESS_CLIENT_SECRET?.trim();
  if (!tunnelHost || !clientId || !clientSecret) {
    res.status(503).json(hidden('unavailable', 'presence_authority_unreachable'));
    return;
  }

  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), UPSTREAM_DEADLINE_MS);
  try {
    const upstream = await fetch(`https://${tunnelHost}/v2/presence`, {
      method: 'GET',
      headers: {
        Accept: 'application/json',
        'CF-Access-Client-Id': clientId,
        'CF-Access-Client-Secret': clientSecret,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    });
    if (!upstream.ok) {
      res.status(503).json(hidden('unavailable', 'presence_authority_unreachable'));
      return;
    }
    const contentType = upstream.headers.get('content-type')?.toLowerCase() ?? '';
    const raw = await upstream.text();
    if (!contentType.includes('application/json') || Buffer.byteLength(raw, 'utf8') > MAX_PRESENCE_BYTES) {
      res.status(502).json(hidden('unavailable', 'presence_authority_invalid'));
      return;
    }
    let payload: unknown;
    try {
      payload = JSON.parse(raw) as unknown;
    } catch {
      res.status(502).json(hidden('unavailable', 'presence_authority_invalid'));
      return;
    }
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

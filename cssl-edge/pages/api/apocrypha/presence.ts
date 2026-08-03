// Sanitized public projection of Apocrypha's display authority.
// Raw presence state never crosses this boundary. The current canonical body
// can only prove that display is hidden, so every uncertainty fails hidden.

import type { NextApiRequest, NextApiResponse } from 'next';

import { RuntimeProxyError, fetchRuntimeHealth } from '@/lib/apocv4/runtime-proxy';
import { createServerTrace, emitOperationalTelemetry, traceparentFor } from '@/lib/telemetry/server';

const PRESENCE_SCHEMA = 'apocrypha.v2.public-presence.v1';
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

function isInvalidRuntimeEvidence(error: unknown): boolean {
  return error instanceof RuntimeProxyError && [
    'runtime_reflected_credential',
    'runtime_response_invalid',
    'runtime_response_too_large',
  ].includes(error.code);
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<PublicPresenceStatus | { error: string }>,
): Promise<void> {
  const trace = createServerTrace(req);
  const started = performance.now();
  res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed' });
    await emitOperationalTelemetry({
      trace,
      kind: 'api.apocrypha.presence.denied',
      source: 'pages.api.apocrypha.presence',
      plane: 'security',
      severity: 'warn',
      outcome: 'denied',
      status: 405,
      durationMs: Math.round(performance.now() - started),
      message: 'Public presence method denied.',
      authority: 'public-read-only',
    });
    return;
  }

  try {
    const health = await fetchRuntimeHealth(traceparentFor(trace));
    const runtimePayloadBytes = Buffer.byteLength(JSON.stringify(health.observed.runtime), 'utf8');
    if (runtimePayloadBytes > MAX_PRESENCE_BYTES) {
      res.status(502).json(hidden('unavailable', 'presence_authority_invalid'));
      await emitOperationalTelemetry({
        trace,
        kind: 'runtime.apocrypha.presence.rejected',
        source: 'apocv4-runtime-proxy',
        plane: 'runtime',
        severity: 'error',
        outcome: 'failed',
        status: 502,
        durationMs: Math.round(performance.now() - started),
        message: 'Runtime health exceeded the public presence evidence bound.',
        authority: 'public-read-only',
        receiptRef: health.observed.receipt.binding_ref ?? health.observed.receipt.auth_registry_ref,
        attributes: {
          upstream_status: health.observed.receipt.upstream_status,
          runtime_payload_bytes: runtimePayloadBytes,
          maximum_presence_bytes: MAX_PRESENCE_BYTES,
        },
      });
      return;
    }

    res.status(200).json(hidden('hidden', 'presence_intent_or_mutual_consent_unavailable'));
    await emitOperationalTelemetry({
      trace,
      kind: 'runtime.apocrypha.presence.checked',
      source: 'apocv4-runtime-proxy',
      plane: 'runtime',
      severity: 'info',
      outcome: 'succeeded',
      status: 200,
      durationMs: Math.round(performance.now() - started),
      message: 'Runtime reachability observed; public presence remains hidden.',
      authority: 'public-read-only',
      receiptRef: health.observed.receipt.binding_ref ?? health.observed.receipt.auth_registry_ref,
      attributes: {
        upstream_status: health.observed.receipt.upstream_status,
        runtime_payload_bytes: runtimePayloadBytes,
        runtime_status: 'READY',
      },
    });
  } catch (error) {
    const invalid = isInvalidRuntimeEvidence(error);
    const status = invalid ? 502 : 503;
    res.status(status).json(hidden(
      'unavailable',
      invalid ? 'presence_authority_invalid' : 'presence_authority_unreachable',
    ));
    await emitOperationalTelemetry({
      trace,
      kind: invalid ? 'runtime.apocrypha.presence.rejected' : 'runtime.apocrypha.presence.failed',
      source: 'apocv4-runtime-proxy',
      plane: 'runtime',
      severity: 'error',
      outcome: invalid ? 'failed' : 'degraded',
      status,
      durationMs: Math.round(performance.now() - started),
      message: error instanceof RuntimeProxyError ? error.code : 'runtime_proxy_failure',
      authority: 'public-read-only',
      attributes: {
        upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null,
        runtime_public_status: error instanceof RuntimeProxyError ? error.publicStatus : null,
      },
    });
  }
}

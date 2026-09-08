import { randomUUID } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCRYPHA_DESTINATION_ENTITY,
  CHAOS_SOURCE_PRODUCT,
  ChaosIngressError,
  MAX_CHAOS_OBSERVATION_BODY_BYTES,
  TRINITY_OBSERVATION_ROUTE,
  verifyChaosObservationTransport,
  type ChaosIngressCode,
} from '@/lib/apocrypha/chaos-observation';

export const config = {
  api: {
    bodyParser: false,
  },
};

const RESPONSE_SCHEMA = 'apocrypha.chaos.transport-refusal.v1' as const;

interface TransportRefusal {
  schema: typeof RESPONSE_SCHEMA;
  route: typeof TRINITY_OBSERVATION_ROUTE;
  source_product: typeof CHAOS_SOURCE_PRODUCT;
  destination_entity: typeof APOCRYPHA_DESTINATION_ENTITY;
  admitted: false;
  writes_committed: 0;
  effect_authority: 'NONE';
  transport_authenticated: boolean;
  response_authentication: 'UNAVAILABLE';
  code: ChaosIngressCode | 'method_not_allowed';
  retryable: boolean;
  correlation_id: string;
}

function secureHeaders(
  res: NextApiResponse<TransportRefusal>,
  correlationId: string,
  authenticated: boolean,
): void {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'X-Trinity-Key-Id, X-Trinity-Signature');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Apocrypha-Route', TRINITY_OBSERVATION_ROUTE);
  res.setHeader('X-Apocrypha-Schema', RESPONSE_SCHEMA);
  res.setHeader('X-Apocrypha-Transport-Authentication', authenticated ? 'verified' : 'unverified');
  res.setHeader('X-Correlation-ID', correlationId);
}

function refuse(
  res: NextApiResponse<TransportRefusal>,
  status: 400 | 401 | 403 | 405 | 503,
  code: TransportRefusal['code'],
  correlationId: string,
  authenticated = false,
): void {
  secureHeaders(res, correlationId, authenticated);
  const retryable = status === 503;
  if (retryable) res.setHeader('Retry-After', '60');
  res.status(status).json({
    schema: RESPONSE_SCHEMA,
    route: TRINITY_OBSERVATION_ROUTE,
    source_product: CHAOS_SOURCE_PRODUCT,
    destination_entity: APOCRYPHA_DESTINATION_ENTITY,
    admitted: false,
    writes_committed: 0,
    effect_authority: 'NONE',
    transport_authenticated: authenticated,
    response_authentication: 'UNAVAILABLE',
    code,
    retryable,
    correlation_id: correlationId,
  });
}

async function readBoundedBody(req: NextApiRequest): Promise<Buffer> {
  const declaredLength = req.headers['content-length'];
  if (
    typeof declaredLength !== 'string'
    || !/^[1-9]\d*$/.test(declaredLength)
    || !Number.isSafeInteger(Number(declaredLength))
    || Number(declaredLength) > MAX_CHAOS_OBSERVATION_BODY_BYTES
  ) {
    throw new ChaosIngressError('invalid_transport', 400);
  }
  const chunks: Buffer[] = [];
  let bytes = 0;
  for await (const chunk of req) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bytes += buffer.length;
    if (bytes > MAX_CHAOS_OBSERVATION_BODY_BYTES) {
      throw new ChaosIngressError('invalid_transport', 400);
    }
    chunks.push(buffer);
  }
  const body = Buffer.concat(chunks, bytes);
  if (body.length < 2) throw new ChaosIngressError('invalid_transport', 400);
  return body;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<TransportRefusal>,
): Promise<void> {
  const correlationId = randomUUID();
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    refuse(res, 405, 'method_not_allowed', correlationId);
    return;
  }

  try {
    const rawBody = await readBoundedBody(req);
    verifyChaosObservationTransport({
      rawBody,
      headers: req.headers,
      method: req.method,
      url: req.url,
    });
    refuse(res, 503, 'receiver_unavailable', correlationId, true);
    return;
  } catch (error) {
    if (error instanceof ChaosIngressError) {
      refuse(res, error.status, error.code, correlationId, error.transportAuthenticated);
      return;
    }
    refuse(res, 503, 'receiver_unavailable', correlationId);
  }
}

import type { NextApiRequest, NextApiResponse } from 'next';

import type { BrainRuntimeStatus } from '@/lib/brain/contracts';
import { requireBrainOwner, respondBrainOwnerFailure, setBrainPrivateHeaders } from '@/lib/brain/owner';
import { ownerBrainRuntimeConfigured, probeOwnerBrainRuntime } from '@/lib/brain/runtime-provider';
import { RuntimeProxyError } from '@/lib/apocv4/runtime-proxy';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
    return;
  }
  const owner = await requireBrainOwner(req);
  if (!owner.ok) {
    respondBrainOwnerFailure(res, owner);
    return;
  }

  const env = envelope();
  if (!ownerBrainRuntimeConfigured()) {
    const body: BrainRuntimeStatus = {
      schema_version: 'apocky.owner-brain.runtime-status.v1',
      status: 'degraded',
      reason_code: 'BRAIN_LOCAL_PROVIDER_DISABLED',
      observed_at: env.ts,
      latency_ms: null,
      upstream_status: null,
      served_by: env.served_by,
      ts: env.ts,
    };
    res.status(200).json(body);
    return;
  }

  try {
    const projection = await probeOwnerBrainRuntime();
    const receipt = projection.observed.receipt;
    const body: BrainRuntimeStatus = {
      schema_version: 'apocky.owner-brain.runtime-status.v1',
      status: 'live',
      reason_code: null,
      observed_at: receipt.observed_at,
      latency_ms: receipt.latency_ms,
      upstream_status: receipt.upstream_status,
      served_by: env.served_by,
      ts: env.ts,
    };
    res.status(200).json(body);
  } catch (error) {
    const reason = error instanceof RuntimeProxyError ? error.code : 'runtime_unreachable';
    const body: BrainRuntimeStatus = {
      schema_version: 'apocky.owner-brain.runtime-status.v1',
      status: 'degraded',
      reason_code: `BRAIN_${reason.toUpperCase()}`,
      observed_at: error instanceof RuntimeProxyError ? error.observedAt : env.ts,
      latency_ms: null,
      upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null,
      served_by: env.served_by,
      ts: env.ts,
    };
    res.status(200).json(body);
  }
}

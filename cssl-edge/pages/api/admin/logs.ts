import type { NextApiRequest, NextApiResponse } from 'next';

import { setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { requireAdmin } from '@/lib/require-admin';
import { envelope } from '@/lib/response';
import {
  ADMIN_LOG_SCHEMA,
  creationLedgerEntries,
  readAdminTelemetry,
  summarizeTelemetry,
} from '@/lib/telemetry/admin-reader';

export const config = {
  api: {
    responseLimit: '2mb',
  },
};

function singleInteger(
  value: string | string[] | undefined,
  fallback: number | undefined,
  minimum: number,
  maximum: number,
): number | null {
  if (value === undefined) return fallback ?? null;
  if (typeof value !== 'string' || !/^(?:0|[1-9][0-9]*)$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= minimum && parsed <= maximum ? parsed : null;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!(await requireAdmin(req, res))) return;

  const allowedKeys = new Set(['limit', 'before_id']);
  if (Object.keys(req.query).some((key) => !allowedKeys.has(key))) {
    res.status(400).json({ error: 'query may contain only limit and before_id', ...envelope() });
    return;
  }
  const limit = singleInteger(req.query.limit, 240, 1, 500);
  const beforeId = singleInteger(req.query.before_id, undefined, 1, Number.MAX_SAFE_INTEGER);
  if (limit === null || (req.query.before_id !== undefined && beforeId === null)) {
    res.status(400).json({ error: 'invalid telemetry cursor or limit', ...envelope() });
    return;
  }

  const observed = await readAdminTelemetry(limit, beforeId ?? undefined);
  if (observed.source !== 'supabase') {
    res.status(503).json({
      schema_version: ADMIN_LOG_SCHEMA,
      observed_at: new Date().toISOString(),
      error: observed.source === 'unconfigured'
        ? 'telemetry_store_unconfigured'
        : 'telemetry_store_unavailable',
      rows: [],
      summary: {},
      creation_ledger: [],
      ...envelope(),
    });
    return;
  }

  res.status(200).json({
    schema_version: ADMIN_LOG_SCHEMA,
    observed_at: new Date().toISOString(),
    cursor: observed.cursor,
    has_more: observed.hasMore,
    source: observed.source,
    rows: observed.rows,
    summary: summarizeTelemetry(observed.rows),
    creation_ledger: creationLedgerEntries(observed.rows),
  });
}

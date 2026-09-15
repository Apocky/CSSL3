import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import { methodNotAllowed, noStore, requireOwnerIdentity } from '@/lib/apocrypha/job-http';
import {
  cleanupOwnerChatOracle,
  ownerOracleUuid,
  seedOwnerChatOracle,
} from '@/lib/apocrypha/owner-oracle-control';

function exactBody(value: unknown, field: 'nonce' | 'run_id'): string {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`OWNER_ORACLE_${field.toUpperCase()}_INVALID`);
  }
  const body = value as Record<string, unknown>;
  if (Object.keys(body).length !== 1 || !Object.prototype.hasOwnProperty.call(body, field)) {
    throw new Error(`OWNER_ORACLE_${field.toUpperCase()}_INVALID`);
  }
  return ownerOracleUuid(body[field], field);
}

function publicError(res: NextApiResponse, error: unknown) {
  const code = error instanceof Error ? error.message : '';
  if (code.includes('_INVALID')) {
    return res.status(400).json({ ok: false, code: 'ORACLE_INPUT_INVALID' });
  }
  if (code.includes('RUN_NOT_FOUND')) {
    return res.status(404).json({ ok: false, code: 'ORACLE_RUN_NOT_FOUND' });
  }
  if (code.includes('CLEANUP_CONFLICT')) {
    return res.status(409).json({ ok: false, code: 'ORACLE_CLEANUP_CONFLICT' });
  }
  return res.status(503).json({ ok: false, code: 'ORACLE_CONTROL_UNAVAILABLE' });
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  noStore(res);
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Authorization, Cookie, Origin');
  if (req.method !== 'POST' && req.method !== 'DELETE') {
    methodNotAllowed(res, ['POST', 'DELETE']);
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ ok: false, code: 'SAME_ORIGIN_REQUIRED' });
    return;
  }

  try {
    const identity = await requireOwnerIdentity(req, res);
    if (!identity) return;
    if (Object.keys(req.query).length !== 0) throw new Error('OWNER_ORACLE_QUERY_INVALID');

    if (req.method === 'POST') {
      const nonce = exactBody(req.body, 'nonce');
      const run = await seedOwnerChatOracle(identity, nonce);
      res.status(201).json({ ok: true, data: run });
      return;
    }

    const runId = exactBody(req.body, 'run_id');
    const result = await cleanupOwnerChatOracle(identity, runId);
    res.status(200).json({ ok: true, data: result });
  } catch (error) {
    publicError(res, error);
  }
}

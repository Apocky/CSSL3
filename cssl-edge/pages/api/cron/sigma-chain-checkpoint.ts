// § T11-W14-K · /api/cron/sigma-chain-checkpoint
// CADENCE : every 1024 anchored events (or every 6 hours · whichever first)
// PURPOSE : emit a Σ-Chain checkpoint = BLAKE3 hash of (last_checkpoint_root,
//           1024 anchored event-roots) · stored in `sigma_chain_checkpoints`.
//           Provides trust-anchor for offline-replay attestation.
//
// Sovereignty :
//   - checkpoint roots are PUBLIC (full-tree anyone-can-verify)
//   - chain remains tamper-evident · roll-back possible via prev_root pointer
//   - cap_signer = service-account · ¬ user-signing-key

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import {
  isCronAuthorized,
  isCronStubMode,
  reject401,
  emitCronAudit,
  nowDurationMs,
  getServiceRoleClient,
} from '@/lib/cron-auth';

interface OkResp {
  ok: true;
  job: 'sigma-chain-checkpoint';
  emitted: boolean;
  events_in_window: number;
  checkpoint_root: string | null;
  prev_root: string | null;
  stub: boolean;
  notes: string | null;
  served_by: string;
  ts: string;
}

interface ErrResp {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

const CHECKPOINT_WINDOW = 1024;

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('cron.sigma-chain-checkpoint', { method: req.method ?? 'POST' });
  const startMs = Date.now();

  if (req.method !== 'POST' && req.method !== 'GET') {
    res.setHeader('Allow', 'POST, GET');
    const env = envelope();
    res.status(405).json({
      ok: false,
      error: 'POST or GET only',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  if (isCronStubMode()) {
    const env = envelope();
    res.status(200).json({
      ok: true,
      job: 'sigma-chain-checkpoint',
      emitted: false,
      events_in_window: 0,
      checkpoint_root: null,
      prev_root: null,
      stub: true,
      notes: 'stub-mode · no CRON_SECRET',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const auth = isCronAuthorized(req);
  if (!auth.ok) {
    reject401(res, auth.reason ?? 'auth-failed');
    return;
  }

  const sb = getServiceRoleClient();
  let emitted = false;
  let eventsInWindow = 0;
  let checkpointRoot: string | null = null;
  let prevRoot: string | null = null;
  let notes: string | null = null;

  if (sb === null) {
    notes = 'supabase-unconfigured-trace-only';
  } else {
    try {
      // The DB-side helper sigma_chain_emit_checkpoint() does the heavy lift :
      //   1. count events since last checkpoint
      //   2. if ≥ CHECKPOINT_WINDOW : compute BLAKE3 root of those event-hashes
      //      via Postgres digest() functions (or no-op fallback if extension
      //      unavailable · the Rust-side stage-1 reconciler can recompute)
      //   3. INSERT into sigma_chain_checkpoints (root, prev_root, count, ts)
      //   4. RETURN the checkpoint row
      const { data, error } = await sb.rpc('sigma_chain_emit_checkpoint', {
        p_window: CHECKPOINT_WINDOW,
      });
      if (!error && data) {
        const row = Array.isArray(data) ? data[0] : data;
        emitted = Boolean(row?.emitted ?? false);
        eventsInWindow = Number(row?.events_in_window ?? 0);
        checkpointRoot = row?.checkpoint_root ?? null;
        prevRoot = row?.prev_root ?? null;
      } else if (error) {
        notes = `rpc-error:${error.code ?? 'unknown'}`;
      }
    } catch (e) {
      notes = e instanceof Error ? e.message.slice(0, 200) : 'exception';
    }
  }

  const { finished_at, duration_ms } = nowDurationMs(startMs);
  void emitCronAudit({
    job_name: 'sigma-chain-checkpoint',
    started_at: new Date(startMs).toISOString(),
    finished_at,
    duration_ms,
    status: notes === null ? 'ok' : 'partial',
    rows_processed: emitted ? 1 : 0,
    retry_count: 0,
    via: auth.via,
    notes,
  });

  const env = envelope();
  res.status(200).json({
    ok: true,
    job: 'sigma-chain-checkpoint',
    emitted,
    events_in_window: eventsInWindow,
    checkpoint_root: checkpointRoot,
    prev_root: prevRoot,
    stub: false,
    notes,
    served_by: env.served_by,
    ts: env.ts,
  });
}

export { CHECKPOINT_WINDOW };

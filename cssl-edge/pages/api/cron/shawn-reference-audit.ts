import type { NextApiRequest, NextApiResponse } from 'next';
import {
  emitCronAudit,
  isCronAuthorized,
  isCronStubMode,
  nowDurationMs,
  reject401,
} from '@/lib/cron-auth';
import { envelope, logHit } from '@/lib/response';
import { auditReferenceLinks, type LinkAuditReport } from '@/lib/shawn/link-audit';

type AuditRunner = () => Promise<LinkAuditReport>;

interface AuditResponse {
  readonly ok: true;
  readonly job: 'shawn-reference-audit';
  readonly stub: boolean;
  readonly report: LinkAuditReport | null;
  readonly notes: string | null;
  readonly served_by: string;
  readonly ts: string;
}

interface ErrorResponse {
  readonly ok: false;
  readonly error: string;
  readonly served_by: string;
  readonly ts: string;
}

export function createShawnReferenceAuditHandler(
  runAudit: AuditRunner = () => auditReferenceLinks(),
): (req: NextApiRequest, res: NextApiResponse<AuditResponse | ErrorResponse>) => Promise<void> {
  return async function shawnReferenceAuditHandler(req, res): Promise<void> {
    logHit('cron.shawn-reference-audit', { method: req.method ?? 'GET' });
    res.setHeader('Cache-Control', 'private, no-store, max-age=0');
    const startMs = Date.now();

    if (req.method !== 'GET' && req.method !== 'POST') {
      res.setHeader('Allow', 'GET, POST');
      const env = envelope();
      res.status(405).json({
        ok: false,
        error: 'GET or POST only',
        served_by: env.served_by,
        ts: env.ts,
      });
      return;
    }

    if (isCronStubMode()) {
      const env = envelope();
      res.status(200).json({
        ok: true,
        job: 'shawn-reference-audit',
        stub: true,
        report: null,
        notes: 'stub-mode · no CRON_SECRET · no external requests issued',
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

    try {
      const report = await runAudit();
      const { finished_at, duration_ms } = nowDurationMs(startMs);
      void emitCronAudit({
        job_name: 'shawn-reference-audit',
        started_at: new Date(startMs).toISOString(),
        finished_at,
        duration_ms,
        status: report.publicationReady ? 'ok' : 'partial',
        rows_processed: report.audited,
        retry_count: 0,
        via: auth.via,
        notes: report.publicationReady
          ? null
          : `publication-blockers:${report.blocking};warnings:${report.warnings}`,
      });
      const env = envelope();
      res.status(200).json({
        ok: true,
        job: 'shawn-reference-audit',
        stub: false,
        report,
        notes: report.publicationReady ? null : 'audit completed; publication remains blocked',
        served_by: env.served_by,
        ts: env.ts,
      });
    } catch (error) {
      const { finished_at, duration_ms } = nowDurationMs(startMs);
      const message = error instanceof Error ? error.message.slice(0, 200) : 'reference audit failed';
      void emitCronAudit({
        job_name: 'shawn-reference-audit',
        started_at: new Date(startMs).toISOString(),
        finished_at,
        duration_ms,
        status: 'fail',
        rows_processed: 0,
        retry_count: 0,
        via: auth.via,
        notes: message,
      });
      const env = envelope();
      res.status(500).json({
        ok: false,
        error: 'reference audit failed',
        served_by: env.served_by,
        ts: env.ts,
      });
    }
  };
}

export default createShawnReferenceAuditHandler();

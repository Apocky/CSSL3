import type { NextApiRequest, NextApiResponse } from 'next';

import { readExternalJobChunksPage } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, requireOwnerIdentity, respondJobError } from '@/lib/apocrypha/job-http';
import { SseWriter } from '@/lib/sse';

// The owner-side push channel for a running job.
//
// WHAT THIS REPLACES
// ------------------
// The chat surface used to learn that its answer had grown by asking, on a
// timer. Even at a 250ms floor that is a poll: the text exists in Postgres for
// up to an interval before the reader is told, and every tick costs a request
// whether or not anything moved.
//
// This inverts it. The server watches, the reader is told. The reader keeps its
// own poll as a fallback - a stream that dies must not take the answer with it -
// but in the normal case the poll never fires because the push got there first.
//
// WHY IT SENDS DELTAS AND NOT THE SNAPSHOT
// ----------------------------------------
// `/api/admin/apocrypha/jobs/[id]` returns revisions, chunks and events - the
// whole job, growing, re-sent in full on every tick. Here each frame carries
// only what is new, keyed by the same public sequence the snapshot uses, so the
// reader can append rather than replace.
//
// WHY IT IS BOUNDED
// -----------------
// vercel.json caps pages/api at maxDuration 30, so the window closes at 25 and
// the client reopens from its cursor. The reader owns reconnection here (fetch,
// not EventSource - the owner bearer cannot ride on an EventSource) and passes
// its cursor back as ?after=.

const WINDOW_MS = 25_000;
const POLL_FAST_MS = 200;
const POLL_SLOW_MS = 1_500;
const HEARTBEAT_MS = 10_000;
const TERMINAL = new Set(['succeeded', 'failed', 'cancelled']);

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function afterCursor(req: NextApiRequest): number {
  const header = req.headers['last-event-id'];
  const raw = Array.isArray(header) ? header[0] : header
    ?? (Array.isArray(req.query.after) ? req.query.after[0] : req.query.after);
  const parsed = Number(raw ?? 0);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : 0;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET') return methodNotAllowed(res, ['GET']);
  try {
    const identity = await requireOwnerIdentity(req, res);
    if (!identity) return;
    const jobId = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
    if (!jobId) return res.status(400).json({ ok: false, code: 'JOB_ID_REQUIRED' });

    // Resolved before any stream header so an unknown job is a 404, not a 200
    // stream whose first frame says the job does not exist.
    let cursor = afterCursor(req);
    let page = await readExternalJobChunksPage(jobId, identity, cursor);
    if (!page) return res.status(404).json({ ok: false, code: 'JOB_NOT_FOUND' });

    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache, no-transform');
    res.setHeader('Connection', 'keep-alive');
    res.setHeader('X-Accel-Buffering', 'no');
    res.flushHeaders?.();

    const sse = new SseWriter(res);
    let open = true;
    req.on('close', () => { open = false; });

    const deadline = Date.now() + WINDOW_MS;
    let delay = POLL_FAST_MS;
    let lastBeat = Date.now();
    let lastStatus = '';

    while (open) {
      // Status first: queued -> leased -> running is movement the reader is
      // waiting on even before a single token exists.
      if (page.status !== lastStatus) {
        lastStatus = page.status;
        sse.writeEvent('status', { job_id: jobId, status: page.status });
        lastBeat = Date.now();
      }

      for (const chunk of page.chunks) {
        sse.writeIdData(chunk.sequence, { sequence: chunk.sequence, text: chunk.text });
        cursor = chunk.sequence;
      }

      // Terminal AND drained - checking status alone would cut the stream
      // before the last tokens of the answer.
      if (TERMINAL.has(page.status) && page.chunks.length === 0) {
        sse.writeEvent('complete', { job_id: jobId, status: page.status, last_sequence: cursor });
        sse.writeDone();
        break;
      }

      if (page.chunks.length > 0) {
        delay = POLL_FAST_MS;
        lastBeat = Date.now();
      } else {
        delay = Math.min(Math.round(delay * 1.5), POLL_SLOW_MS);
        if (Date.now() - lastBeat >= HEARTBEAT_MS) {
          sse.writeComment('keepalive');
          lastBeat = Date.now();
        }
      }

      if (Date.now() + delay >= deadline) {
        sse.writeEvent('reconnect', { job_id: jobId, last_sequence: cursor });
        break;
      }

      await sleep(delay);
      if (!open) break;
      page = await readExternalJobChunksPage(jobId, identity, cursor);
      if (!page) {
        sse.writeEvent('error', { code: 'JOB_NOT_FOUND' });
        break;
      }
    }

    sse.close();
    return undefined;
  } catch (error) {
    if (res.headersSent) { try { res.end(); } catch { /* socket already gone */ } return undefined; }
    return respondJobError(res, error);
  }
}

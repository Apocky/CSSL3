import type { NextApiRequest, NextApiResponse } from 'next';

import { readExternalJobEventsPage } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, requireChaosIdentity, respondJobError } from '@/lib/apocrypha/job-http';
import { SseWriter } from '@/lib/sse';

// This route is named `events` and, until now, was not one: it returned a
// single cursor-paginated JSON page, so every consumer had to build its own
// polling loop and pay a full request round trip per tick.
//
// It now speaks SSE when asked to, and still returns the JSON page when asked
// for that - existing callers are unchanged.
//
// WHY THE STREAM IS BOUNDED
// -------------------------
// vercel.json caps pages/api at maxDuration 30. A stream that ignores that is
// a stream the platform kills mid-frame. So it closes itself just under the
// cap and lets the browser reconnect, which EventSource does on its own -
// resending the last id it saw as Last-Event-ID. Since our ids ARE the event
// ordinals, a reconnect resumes exactly where the previous window ended. No
// event is replayed and none is skipped; the client does not have to
// participate beyond using EventSource.

const WINDOW_MS = 25_000;
const POLL_FAST_MS = 250;
const POLL_SLOW_MS = 2_000;
const HEARTBEAT_MS = 10_000;
const TERMINAL = new Set(['succeeded', 'failed', 'cancelled']);

function intQuery(value: unknown, fallback: number): number {
  const raw = Array.isArray(value) ? value[0] : value;
  const parsed = Number(raw ?? fallback);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : fallback;
}

// Last-Event-ID wins over ?after=: the browser sets it from what it actually
// received, which is more trustworthy than a cursor the caller remembered.
function startCursor(req: NextApiRequest): number {
  const header = req.headers['last-event-id'];
  const resumed = intQuery(header, -1);
  return resumed >= 0 ? resumed : intQuery(req.query.after, 0);
}

function wantsStream(req: NextApiRequest): boolean {
  const asked = Array.isArray(req.query.stream) ? req.query.stream[0] : req.query.stream;
  if (asked === '0' || asked === 'false') return false;
  if (asked === '1' || asked === 'true') return true;
  return String(req.headers.accept ?? '').includes('text/event-stream');
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET') return methodNotAllowed(res, ['GET']);
  try {
    const identity = await requireChaosIdentity(req);
    const jobId = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
    if (!jobId) return res.status(400).json({ ok: false, code: 'JOB_ID_REQUIRED' });

    if (!wantsStream(req)) {
      const page = await readExternalJobEventsPage(jobId, identity, intQuery(req.query.after, 0));
      return page
        ? res.status(200).json({ ok: true, ...page })
        : res.status(404).json({ ok: false, code: 'JOB_NOT_FOUND' });
    }

    // The first read happens before any SSE header, so a job that does not
    // exist still gets an honest 404 rather than a 200 stream carrying an
    // error frame - which is what a client's error handling expects.
    let cursor = startCursor(req);
    let page = await readExternalJobEventsPage(jobId, identity, cursor);
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

    while (open) {
      for (const event of page.events) {
        sse.writeIdData(event.sequence, event);
        cursor = event.sequence;
      }

      const terminal = TERMINAL.has(page.status);
      // Terminal AND drained. Checking only the status would cut the stream
      // before the events that describe how the job ended.
      if (terminal && page.events.length === 0) {
        sse.writeEvent('complete', { job_id: jobId, status: page.status, last_sequence: cursor });
        sse.writeDone();
        break;
      }

      if (page.events.length > 0) {
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
        // Hand the client back its cursor and let it reconnect. Not an error
        // condition - it is the normal shape of a long stream on a platform
        // with a duration cap.
        sse.writeEvent('reconnect', { job_id: jobId, last_sequence: cursor });
        break;
      }

      await sleep(delay);
      if (!open) break;
      page = await readExternalJobEventsPage(jobId, identity, cursor);
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

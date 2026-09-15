// Poll one signed-out turn.
//
// The job is looked up by guest principal AND id in SQL, so guessing another visitor's job id
// returns a refusal rather than their answer.

import type { NextApiRequest, NextApiResponse } from 'next';

import { GuestChatError, readGuestChatJob, readGuestCookie } from '@/lib/apocrypha/guest-chat';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const TERMINAL = new Set(['succeeded', 'completed', 'failed', 'cancelled', 'dead']);

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'Cookie');
  res.setHeader('X-Content-Type-Options', 'nosniff');

  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }

  // No cookie means no identity to check a job against. Minting one here would hand back a fresh
  // guest who owns nothing, so the honest answer is that this browser has no such job.
  const guestId = readGuestCookie(req.headers.cookie);
  if (!guestId) {
    res.status(404).json({ ok: false, code: 'GUEST_CHAT_NOT_FOUND' });
    return;
  }

  const jobId = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
  if (typeof jobId !== 'string' || !UUID.test(jobId)) {
    res.status(400).json({ ok: false, code: 'JOB_ID_INVALID' });
    return;
  }

  try {
    const state = await readGuestChatJob({ guestId, jobId });
    res.status(200).json({
      ok: true,
      job_id: state.job_id,
      status: state.status,
      terminal: TERMINAL.has(state.status),
      answer: state.status === 'succeeded' || state.status === 'completed' ? state.answer : null,
      error_code: state.error_code,
    });
  } catch (error) {
    if (error instanceof GuestChatError) {
      res.status(error.status).json({ ok: false, code: error.code, error: error.message });
      return;
    }
    res.status(502).json({ ok: false, code: 'GUEST_CHAT_UNAVAILABLE' });
  }
}

// POST { text } -> audio/wav, spoken by Windows on the PC.
import type { NextApiRequest, NextApiResponse } from 'next';

import { directGuard } from '@/lib/direct/guard';
import { speak } from '@/lib/direct/voice';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (!directGuard(req, res, ['POST'])) return;
  const text = typeof req.body?.text === 'string' ? req.body.text.trim() : '';
  if (!text) { res.status(400).json({ ok: false, code: 'TEXT_EMPTY' }); return; }
  try { const wav = await speak(text); res.setHeader('Content-Type', 'audio/wav'); res.status(200).send(wav); }
  catch (error) { res.status(500).json({ ok: false, error: error instanceof Error ? error.message : 'failed' }); }
}

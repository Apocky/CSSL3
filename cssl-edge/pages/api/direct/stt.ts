// POST raw audio (the browser's MediaRecorder blob) -> { text }.
import type { NextApiRequest, NextApiResponse } from 'next';

import { directGuard } from '@/lib/direct/guard';
import { transcribe } from '@/lib/direct/voice';

export const config = { api: { bodyParser: false } };

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (!directGuard(req, res, ['POST'])) return;
  const chunks: Buffer[] = []; let size = 0;
  for await (const chunk of req) { size += (chunk as Buffer).length; if (size > 25 * 1024 * 1024) { res.status(413).json({ ok: false }); return; } chunks.push(chunk as Buffer); }
  try {
    const type = String(req.headers['content-type'] ?? 'audio/webm');
    res.status(200).json({ ok: true, text: await transcribe(Buffer.concat(chunks), type.split('/')[1]?.split(';')[0]) });
  } catch (error) { res.status(500).json({ ok: false, error: error instanceof Error ? error.message : 'failed' }); }
}

import type { NextApiRequest, NextApiResponse } from 'next';
import { mobileConfigFromEnvironment, readPublicMobileEnvironment } from '@/lib/mobile/config';

export default function handler(req: NextApiRequest, res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow');
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed.' });
    return;
  }
  const config = mobileConfigFromEnvironment(readPublicMobileEnvironment());
  if (!config) {
    res.status(503).json({ error: 'Mobile sign-in is not configured.' });
    return;
  }
  res.status(200).json(config);
}

import type { NextApiRequest, NextApiResponse } from 'next';

import { retireLegacyApocryphaAdminRoute } from '@/lib/apocrypha/retired-route';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  await retireLegacyApocryphaAdminRoute(req, res, 'tools');
}

// Fail-closed tombstone for predecessor Apocrypha admin surfaces.

import type { NextApiRequest, NextApiResponse } from 'next';

import { requireApocryphaOwner, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { envelope } from '@/lib/response';

export async function retireLegacyApocryphaAdminRoute(
  req: NextApiRequest,
  res: NextApiResponse,
  surface: string,
): Promise<void> {
  setPrivateNoStore(res);
  if (!(await requireApocryphaOwner(req, res))) return;
  res.status(410).json({
    error: `The predecessor Apocrypha ${surface} surface is retired.`,
    reason_code: 'legacy_apocrypha_admin_surface_retired',
    surface,
    replacement: null,
    ...envelope(),
  });
}

// /api/admin/logs · stub audit log · activates when admin-bridge wires
import type { NextApiRequest, NextApiResponse } from 'next';

export default function handler(_req: NextApiRequest, res: NextApiResponse) {
  return res.status(200).json({
    stub: true,
    rows: [],
  });
}

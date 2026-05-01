// /api/admin/coder/pending · stub · activates when admin-bridge → desktop wires
import type { NextApiRequest, NextApiResponse } from 'next';

export default function handler(_req: NextApiRequest, res: NextApiResponse) {
  return res.status(200).json({
    stub: true,
    edits: [],
  });
}

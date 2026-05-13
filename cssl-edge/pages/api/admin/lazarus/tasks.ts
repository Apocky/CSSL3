import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/lazarus/auth';
import { createTask, listTasks } from '@/lib/lazarus/store';
import type { CreateLazarusTaskInput } from '@/lib/lazarus/types';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  try {
    if (req.method === 'GET') {
      if (!(await requireAdmin(req, res))) return;
      return res.status(200).json({ ...(await listTasks()), ...envelope() });
    }
    if (req.method === 'POST') {
      if (!(await requireAdmin(req, res))) return;
      const body = (req.body ?? {}) as Partial<CreateLazarusTaskInput>;
      if (typeof body.title !== 'string' || typeof body.prompt !== 'string') {
        return res.status(400).json({ error: 'title + prompt required', ...envelope() });
      }
      return res.status(201).json({ ...(await createTask(body as CreateLazarusTaskInput)), ...envelope() });
    }
    res.setHeader('Allow', 'GET, POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}

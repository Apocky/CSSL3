import type { NextApiRequest, NextApiResponse } from 'next';
import { retireApiEndpoint, type ContainmentResponse } from '@/lib/containment';

export default function handler(_req: NextApiRequest, res: NextApiResponse<ContainmentResponse>): void {
  retireApiEndpoint(res, '/api/analytics/event');
}

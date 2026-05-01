// /api/admin/tasks · stub list of scheduled-tasks · activates when admin-bridge wired
import type { NextApiRequest, NextApiResponse } from 'next';

export default function handler(_req: NextApiRequest, res: NextApiResponse) {
  // Reflects the W7/W8/W9/W10 schedule visible at this moment.
  // Live data lands when scheduled-tasks bridge wires (W9-D2 supabase + admin-bridge).
  return res.status(200).json({
    stub: true,
    tasks: [
      {
        taskId: 'loa-w8-dispatch',
        description: 'W8 dispatch · 15 file-disjoint agents · Mycelial + Akashic + Σ-Chain + omni-input + seasonal-hard-perma + automated-Coder + real-Supabase',
        schedule: 'One-time: 5/1/2026, 11:33:00 AM',
        enabled: false,
        fireAt: '2026-05-01T18:33:00.000Z',
        lastRunAt: '2026-05-01T18:33:43.675Z',
      },
      {
        taskId: 'loa-rd-legacy-verify',
        description: 'Verify POD-1 GDDs in main task absorbed legacy design + new monetization · supplement gaps',
        schedule: 'One-time: 5/1/2026, 8:00:00 PM',
        enabled: true,
        nextRunAt: '2026-05-02T03:00:00.000Z',
      },
      {
        taskId: 'loa-w9-polish',
        description: 'W9 · Stripe-checkout wire + /docs auto-built + /devblog + /press + Termly-replace-stub-pages',
        schedule: 'One-time: 5/1/2026, 2:30:00 PM',
        enabled: true,
        nextRunAt: '2026-05-01T21:30:00.000Z',
      },
      {
        taskId: 'loa-w10-mycelium-desktop',
        description: 'W10 · Mycelium-Desktop autonomous-local-agent · Tauri-2.x · 4 NEW crates · 3-mode LLM-bridge · ≤50MB-installer',
        schedule: 'One-time: 5/1/2026, 2:35:00 PM',
        enabled: true,
        nextRunAt: '2026-05-01T21:35:00.000Z',
      },
    ],
  });
}

// /api/admin/bridge · private admin chat relay.
// This route does not return fake chat content. If no server-side model key is configured,
// it returns 503 so the UI can show a real configuration fault.

import type { NextApiRequest, NextApiResponse } from 'next';
import { callAdminChatModel, resolveAdminChatProvider, sanitizeAdminChatMessages } from '@/lib/admin-chat';
import { requireAdmin } from '@/lib/lazarus/auth';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  const action = (req.query.action as string) ?? 'status';
  if (!(await requireAdmin(req, res))) return;

  if (action === 'status' && req.method === 'GET') {
    const provider = resolveAdminChatProvider();
    return res.status(200).json({
      online: provider.ready,
      model_ready: provider.ready,
      provider: provider.provider,
      model: provider.model,
      ...envelope(),
    });
  }

  if (action === 'send' && req.method === 'POST') {
    const body = req.body as { text?: unknown; messages?: unknown } | undefined;
    const text = typeof body?.text === 'string' ? body.text.trim() : '';
    const priorMessages = sanitizeAdminChatMessages(body?.messages);
    if (!text) return res.status(400).json({ error: 'text required', ...envelope() });

    const provider = resolveAdminChatProvider();
    if (!provider.ready) {
      return res.status(503).json({
        error: 'No admin chat model is configured. Set DEEPSEEK_API_KEY or ANTHROPIC_API_KEY on the server.',
        model_ready: false,
        ...envelope(),
      });
    }

    try {
      const result = await callAdminChatModel({
        messages: priorMessages.length > 0 ? priorMessages : [{ role: 'user', content: text }],
      });
      return res.status(200).json({
        response: result.text,
        provider: result.provider,
        model: result.model,
        usage: result.usage,
        ...envelope(),
      });
    } catch (err) {
      return res.status(502).json({
        error: err instanceof Error ? err.message : String(err),
        model_ready: true,
        ...envelope(),
      });
    }
  }

  return res.status(405).json({ error: 'Method or action not allowed' });
}

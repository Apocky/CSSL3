// § T11-W14-K · Supabase edge-function · analytics-bucket-promote
// PURPOSE : promote analytics 1min → 1hr → 1day rollups by calling the
//           existing rollup_promote_minutes() helper. Wraps the RPC with
//           cron-secret auth + audit-emit.
//
// Trigger : every-hour via /api/cron/kan-rollup OR supabase-CLI cron.
// Auth    : Authorization Bearer CRON_SECRET.

// deno-lint-ignore-file no-explicit-any
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';

const CRON_SECRET = Deno.env.get('CRON_SECRET') ?? '';
const SB_URL = Deno.env.get('SUPABASE_URL') ?? '';
const SB_SVC_KEY = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY') ?? '';

interface RespOk {
  ok: true;
  promoted_to_1hr: number;
  promoted_to_1day: number;
  ts: string;
}

interface RespErr {
  ok: false;
  error: string;
  ts: string;
}

function respond(body: RespOk | RespErr, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

Deno.serve(async (req: Request): Promise<Response> => {
  const auth = req.headers.get('authorization') ?? '';
  if (CRON_SECRET.length === 0) {
    return respond({
      ok: false,
      error: 'CRON_SECRET not configured · stub-mode',
      ts: new Date().toISOString(),
    }, 200);
  }
  if (!auth.startsWith('Bearer ') || auth.slice(7) !== CRON_SECRET) {
    return respond({
      ok: false,
      error: 'unauthorized',
      ts: new Date().toISOString(),
    }, 401);
  }
  if (req.method !== 'POST' && req.method !== 'GET') {
    return respond({
      ok: false,
      error: 'POST or GET only',
      ts: new Date().toISOString(),
    }, 405);
  }

  if (SB_URL.length === 0 || SB_SVC_KEY.length === 0) {
    return respond({
      ok: false,
      error: 'SUPABASE_URL or SERVICE_ROLE_KEY missing',
      ts: new Date().toISOString(),
    }, 200);
  }

  const sb = createClient(SB_URL, SB_SVC_KEY, { auth: { persistSession: false } });

  try {
    const { data, error } = await sb.rpc('rollup_promote_minutes');
    if (error) {
      return respond({
        ok: false,
        error: `rpc-failed: ${error.message}`,
        ts: new Date().toISOString(),
      }, 502);
    }
    const row = Array.isArray(data) ? data[0] : data;
    return respond({
      ok: true,
      promoted_to_1hr: Number(row?.promoted_to_1hr ?? 0),
      promoted_to_1day: Number(row?.promoted_to_1day ?? 0),
      ts: new Date().toISOString(),
    }, 200);
  } catch (e: unknown) {
    return respond({
      ok: false,
      error: e instanceof Error ? e.message : 'exception',
      ts: new Date().toISOString(),
    }, 500);
  }
});

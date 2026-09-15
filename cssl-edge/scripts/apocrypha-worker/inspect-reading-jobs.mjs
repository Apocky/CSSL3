// Show a job's request keys, the exact prompt-facing fields, and the tail of the final text. Read-only.
import { readFileSync } from 'node:fs';
import { createClient } from '@supabase/supabase-js';

const envText = readFileSync('C:/Users/Apocky/source/repos/CSSLv3/cssl-edge/.env.local', 'utf8');
const env = {};
for (const line of envText.split(/\r?\n/)) { const m = line.match(/^([A-Z0-9_]+)=(.*)$/); if (m) env[m[1]] = m[2].replace(/^"|"$/g, '').trim(); }
const sb = createClient(env.APOCKY_HUB_SUPABASE_URL, env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY, { auth: { persistSession: false } });

const since = new Date(Date.now() - 24 * 3600 * 1000).toISOString();
const { data: jobs } = await sb.from('apocrypha_job')
  .select('id,kind,status,created_at,request,terminal_revision_id')
  .eq('capability', 'chaos_tarot_reading').gte('created_at', since)
  .order('created_at', { ascending: false }).limit(6);
for (const j of jobs ?? []) {
  const r = j.request ?? {};
  console.log(`\n${j.created_at} ${j.status} ${j.kind} ${j.id}`);
  console.log(`  request keys: ${Object.keys(r).join(', ')}`);
  console.log(`  options: ${JSON.stringify(r.options)}  structured_context keys: ${Object.keys(r.structured_context ?? {}).join(', ')}`);
  console.log(`  service_tier=${r.service_tier} max_output_tokens=${r.model_policy?.max_output_tokens}`);
  if (j.terminal_revision_id) {
    const { data: rev } = await sb.from('apocrypha_job_revision').select('content,usage').eq('id', j.terminal_revision_id).maybeSingle();
    const text = typeof rev?.content === 'string' ? rev.content : JSON.stringify(rev?.content ?? '');
    console.log(`  usage: ${JSON.stringify(rev?.usage ?? {})}`);
    console.log(`  TAIL: …${text.slice(-220).replace(/\n/g, ' ')}`);
  }
}

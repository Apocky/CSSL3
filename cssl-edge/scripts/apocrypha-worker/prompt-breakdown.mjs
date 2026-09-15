// Token breakdown of a real job's request as the worker would render it. Read-only.
import { readFileSync } from 'node:fs';
import { createClient } from '@supabase/supabase-js';

const envText = readFileSync('C:/Users/Apocky/source/repos/CSSLv3/cssl-edge/.env.local', 'utf8');
const env = {};
for (const line of envText.split(/\r?\n/)) { const m = line.match(/^([A-Z0-9_]+)=(.*)$/); if (m) env[m[1]] = m[2].replace(/^"|"$/g, '').trim(); }
const sb = createClient(env.APOCKY_HUB_SUPABASE_URL, env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY, { auth: { persistSession: false } });
const jobId = process.argv[2];
const { data: job, error } = await sb.from('apocrypha_job').select('request').eq('id', jobId).single();
if (error) { console.error(error); process.exit(1); }
const r = job.request;

async function tok(text) {
  const res = await fetch('http://127.0.0.1:19128/tokenize', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ content: text, add_special: false }) });
  return (await res.json()).tokens.length;
}
const pieces = {
  question: r.question ?? '',
  canonical_reading_full: JSON.stringify(r.canonical_reading),
  canonical_reading_meanings_only: JSON.stringify((r.canonical_reading?.items ?? []).map((i) => ({ n: i.name, p: i.position?.name, r: i.is_reversed, u: i.meanings?.upright, rv: i.meanings?.reversed, k: i.meanings?.keywords }))),
  structured_context_full: JSON.stringify(r.structured_context),
  structured_context_sans_policy: JSON.stringify(Object.fromEntries(Object.entries(r.structured_context ?? {}).filter(([k]) => !['apocrypha_policy', 'apocrypha_tier'].includes(k)))),
  options: JSON.stringify(r.options),
  model_policy: JSON.stringify(r.model_policy),
};
for (const [k, v] of Object.entries(pieces)) console.log(`${k.padEnd(34)} chars=${String(v.length).padStart(6)} tokens=${await tok(v)}`);
console.log('\ncanonical_reading top-level keys:', Object.keys(r.canonical_reading ?? {}));
console.log('item keys:', Object.keys(r.canonical_reading?.items?.[0] ?? {}));
console.log('structured_context keys:', Object.keys(r.structured_context ?? {}));

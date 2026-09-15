// Diff two stored job requests field-by-field with llama /tokenize counts. Read-only.
import { readFileSync } from 'node:fs';
import { createClient } from '@supabase/supabase-js';

const envText = readFileSync('C:/Users/Apocky/source/repos/CSSLv3/cssl-edge/.env.local', 'utf8');
const env = {};
for (const line of envText.split(/\r?\n/)) { const m = line.match(/^([A-Z0-9_]+)=(.*)$/); if (m) env[m[1]] = m[2].replace(/^"|"$/g, '').trim(); }
const sb = createClient(env.APOCKY_HUB_SUPABASE_URL, env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY, { auth: { persistSession: false } });
const [a, b] = process.argv.slice(2);
const load = async (id) => (await sb.from('apocrypha_job').select('request').eq('id', id).single()).data.request;
const tok = async (t) => t ? (await (await fetch('http://127.0.0.1:19128/tokenize', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ content: t, add_special: false }) })).json()).tokens.length : 0;
const [ra, rb] = await Promise.all([load(a), load(b)]);
const keys = [...new Set([...Object.keys(ra), ...Object.keys(rb)])].sort();
for (const k of keys) {
  const sa = JSON.stringify(ra[k] ?? null), sbv = JSON.stringify(rb[k] ?? null);
  const same = sa === sbv;
  console.log(`${same ? '  ' : '**'} ${k.padEnd(30)} A=${String(sa.length).padStart(5)}ch/${String(await tok(sa)).padStart(4)}tok  B=${String(sbv.length).padStart(5)}ch/${String(await tok(sbv)).padStart(4)}tok${same ? '' : '  DIFFERENT'}`);
}
for (const k of keys) {
  const sa = JSON.stringify(ra[k] ?? null), sbv = JSON.stringify(rb[k] ?? null);
  if (sa !== sbv && k !== 'canonical_reading') console.log(`\n--- ${k} ---\nA: ${sa.slice(0, 400)}\nB: ${sbv.slice(0, 400)}`);
}
if (JSON.stringify(ra.canonical_reading) !== JSON.stringify(rb.canonical_reading)) {
  const ia = ra.canonical_reading?.items ?? [], ib = rb.canonical_reading?.items ?? [];
  console.log(`\n--- canonical_reading.items: A=${ia.length} B=${ib.length}`);
  for (let i = 0; i < Math.max(ia.length, ib.length); i++) {
    const x = ia[i], y = ib[i];
    console.log(`  [${i}] A=${x?.name}/${x?.position?.name}/rev=${x?.is_reversed}  B=${y?.name}/${y?.position?.name}/rev=${y?.is_reversed}  meaningsEq=${JSON.stringify(x?.meanings) === JSON.stringify(y?.meanings)} posDescEq=${x?.position?.description === y?.position?.description}`);
  }
  console.log(`  spread: A=${JSON.stringify(ra.canonical_reading?.spread)}\n          B=${JSON.stringify(rb.canonical_reading?.spread)}`);
}

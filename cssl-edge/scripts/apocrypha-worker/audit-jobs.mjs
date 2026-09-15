// Read-only audit of apocrypha job control-plane state. Never prints secrets.
import { readFileSync } from 'node:fs';
import { createClient } from '@supabase/supabase-js';

const envText = readFileSync(process.argv[2] ?? 'C:/Users/Apocky/source/repos/CSSLv3/cssl-edge/.env.local', 'utf8');
const env = {};
for (const line of envText.split(/\r?\n/)) {
  const m = line.match(/^([A-Z0-9_]+)=(.*)$/);
  if (m) env[m[1]] = m[2].replace(/^"|"$/g, '').trim();
}
const url = env.APOCKY_HUB_SUPABASE_URL || env.NEXT_PUBLIC_SUPABASE_URL;
const key = env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY || env.SUPABASE_SERVICE_ROLE_KEY;
if (!url || !key) { console.error('missing url/key'); process.exit(1); }
console.log('db host:', new URL(url).host);
const sb = createClient(url, key, { auth: { persistSession: false } });

const since = new Date(Date.now() - 12 * 3600 * 1000).toISOString();
const { data: jobs, error } = await sb
  .from('apocrypha_job')
  .select('id,kind,capability,status,attempt_count,max_attempts,created_at,updated_at,completed_at,available_at,error_code,error_detail,current_attempt_id')
  .gte('created_at', since)
  .order('created_at', { ascending: false })
  .limit(25);
if (error) { console.error('jobs query error', error); process.exit(1); }
console.log(`\n=== apocrypha_job (last 12h): ${jobs.length} ===`);
for (const j of jobs) {
  console.log(`${j.created_at} ${j.status.padEnd(14)} ${j.kind.padEnd(15)} att=${j.attempt_count}/${j.max_attempts} done=${j.completed_at ?? '-'} avail=${j.available_at} err=${j.error_code ?? '-'} ${String(j.error_detail ?? '').slice(0, 120)} id=${j.id}`);
}

const { data: statusCounts } = await sb.from('apocrypha_job').select('status').gte('created_at', since);
const counts = {};
for (const r of statusCounts ?? []) counts[r.status] = (counts[r.status] ?? 0) + 1;
console.log('\nstatus counts (12h):', counts);

const { data: nodes } = await sb.from('apocrypha_worker_node').select('id,status,last_seen_at,model_profiles').limit(10);
console.log('\n=== worker nodes ===');
for (const n of nodes ?? []) {
  const mp = n.model_profiles ?? {};
  console.log(`${n.id} status=${n.status} last_seen=${n.last_seen_at} phase=${mp.phase} qwen_healthy=${mp.qwen_healthy} deadline=${mp.generation_deadline_ms} load=${JSON.stringify(mp.load)}`);
}

if (jobs.length) {
  const latest = jobs[0];
  const { data: attempts } = await sb
    .from('apocrypha_job_attempt')
    .select('id,attempt_no,status,leased_at,started_at,finished_at,lease_expires_at,last_heartbeat_at,failure_code,failure_detail,metrics')
    .eq('job_id', latest.id)
    .order('attempt_no', { ascending: true });
  console.log(`\n=== attempts for latest job ${latest.id} ===`);
  for (const a of attempts ?? []) {
    console.log(`#${a.attempt_no} ${a.status} leased=${a.leased_at} hb=${a.last_heartbeat_at} lease_exp=${a.lease_expires_at} fin=${a.finished_at ?? '-'} err=${a.failure_code ?? '-'} ${String(a.failure_detail ?? '').slice(0, 200)} metrics=${JSON.stringify(a.metrics).slice(0, 200)}`);
  }
  const { data: events } = await sb
    .from('apocrypha_job_event')
    .select('*')
    .eq('job_id', latest.id)
    .order('id', { ascending: true })
    .limit(40);
  console.log(`\n=== events for latest job ===`);
  for (const e of events ?? []) {
    console.log(JSON.stringify(e).slice(0, 320));
  }
  const { count: chunkCount } = await sb.from('apocrypha_job_chunk').select('*', { count: 'exact', head: true }).eq('job_id', latest.id);
  console.log(`\nchunks for latest job: ${chunkCount}`);
}

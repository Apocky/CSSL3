// cssl-edge/lib/mneme/store.ts
// MNEME — Supabase repository layer.
//
// Spec : ../../specs/43_MNEME.csl § OPS + 44_MNEME_PIPELINES.csl § RETRIEVE
//
// USAGE
//   const sb = getMnemeClient();    // service-role; null if env missing
//   await ensureProfile(sb, 'loa-v10', sovereignPk);
//   await insertMessages(sb, msgs);
//   await insertMemory(sb, mem);
//   const hits = await retrieveAll(sb, profile_id, params);
//
// All channel queries assume RLS-bypass via service-role. Caller-side cap
// enforcement happens in the route handlers (lib/cap.ts pattern).

import { createClient, type SupabaseClient } from '@supabase/supabase-js';
import {
    MnemeError,
    type MemoryType,
    type MemoryPublic,
    type Memory,
    type Message,
    type Profile,
    type ChannelHit,
    type ChannelName,
} from './types';
import { maskToHex, maskFromHex, defaultMask, revokeMask } from './sigma';
import { toPgVectorLiteral } from './embed';
import { composeTsQuery } from './csl';

// ── Client construction ────────────────────────────────────────────────

let _client: SupabaseClient | null | undefined;

// Returns service-role client, or null when env missing (caller should
// fall back to mocked behavior in tests/local-dev).
export function getMnemeClient(): SupabaseClient | null {
    if (_client !== undefined) return _client;
    const url     = process.env['NEXT_PUBLIC_SUPABASE_URL'];
    const service = process.env['SUPABASE_SERVICE_ROLE_KEY'];
    if (!url || !service) {
        _client = null;
        return null;
    }
    _client = createClient(url, service, {
        auth: { persistSession: false, autoRefreshToken: false },
        global: { headers: { 'x-mneme-route': 'cssl-edge' } },
    });
    return _client;
}

// Test escape-hatch.
export function _resetMnemeClientForTests(): void {
    _client = undefined;
}

// ── Row shapes (raw from Supabase) ─────────────────────────────────────
// These mirror the SQL DDL in 0040_mneme.sql.

interface ProfileRow {
    profile_id:    string;
    sovereign_pk:  string;       // bytea hex (Supabase returns "\\xAA…")
    sigma_mask:    string;
    created_at:    string;
    memory_count:  number;
    message_count: number;
    meta:          Record<string, unknown>;
}

interface MessageRow {
    id:          string;
    profile_id:  string;
    session_id:  string;
    role:        Message['role'];
    content:     string;
    ts:          string;
    sigma_mask:  string;
}

interface MemoryRow {
    id:              string;
    profile_id:      string;
    type:            MemoryType;
    csl:             string;
    paraphrase:      string;
    topic_key:       string | null;
    search_queries:  string[];
    source_msg_ids:  string[];
    superseded_by:   string | null;
    sigma_mask:      string;
    created_at:      string;
    embedding:       string | null;
}

// ── Bytea conversion ───────────────────────────────────────────────────
// Supabase returns bytea as `"\\x" + hex`. Convert in/out for round-trip.

function pgBytesToBytes(b: string | null | undefined): Uint8Array {
    if (!b) return new Uint8Array(0);
    const s = b.startsWith('\\x') ? b.slice(2) : b;
    if (!/^[0-9a-fA-F]*$/.test(s) || s.length % 2 !== 0) {
        throw new MnemeError('BYTEA_FMT', `unexpected bytea: ${b.slice(0, 16)}…`, 500);
    }
    const out = new Uint8Array(s.length / 2);
    for (let i = 0; i < out.length; i++) {
        out[i] = parseInt(s.substr(i * 2, 2), 16);
    }
    return out;
}

function bytesToPgBytes(b: Uint8Array): string {
    return '\\x' + maskToHex(b).slice(0, b.length * 2);
}

// ── Row → typed conversions ────────────────────────────────────────────

function rowToProfile(r: ProfileRow): Profile {
    return {
        profile_id:    r.profile_id,
        sovereign_pk:  pgBytesToBytes(r.sovereign_pk),
        sigma_mask:    pgBytesToBytes(r.sigma_mask),
        created_at:    r.created_at,
        memory_count:  r.memory_count,
        message_count: r.message_count,
        meta:          r.meta ?? {},
    };
}

function rowToMessage(r: MessageRow): Message {
    return {
        id:         r.id,
        profile_id: r.profile_id,
        session_id: r.session_id,
        role:       r.role,
        content:    r.content,
        ts:         r.ts,
        sigma_mask: pgBytesToBytes(r.sigma_mask),
    };
}

function rowToMemory(r: MemoryRow): Memory {
    let emb: Float32Array | null = null;
    if (r.embedding) {
        // pgvector returns "[a,b,c,...]" string form by default
        const cleaned = r.embedding.replace(/^\[|\]$/g, '');
        const parts = cleaned.split(',');
        const arr = new Float32Array(parts.length);
        for (let i = 0; i < parts.length; i++) {
            arr[i] = parseFloat(parts[i] ?? '0');
        }
        emb = arr;
    }
    return {
        id:             r.id,
        profile_id:     r.profile_id,
        type:           r.type,
        csl:            r.csl,
        paraphrase:     r.paraphrase,
        topic_key:      r.topic_key,
        search_queries: r.search_queries ?? [],
        source_msg_ids: r.source_msg_ids ?? [],
        superseded_by:  r.superseded_by,
        sigma_mask:     pgBytesToBytes(r.sigma_mask),
        created_at:     r.created_at,
        embedding:      emb,
    };
}

export function memoryToPublic(m: Memory): MemoryPublic {
    return {
        id:             m.id,
        profile_id:     m.profile_id,
        type:           m.type,
        csl:            m.csl,
        paraphrase:     m.paraphrase,
        topic_key:      m.topic_key,
        search_queries: m.search_queries,
        source_msg_ids: m.source_msg_ids,
        superseded_by:  m.superseded_by,
        created_at:     m.created_at,
    };
}

// ── Profile CRUD ───────────────────────────────────────────────────────

export async function getProfile(
    sb: SupabaseClient,
    profile_id: string,
): Promise<Profile | null> {
    const { data, error } = await sb
        .from('mneme_profiles')
        .select('*')
        .eq('profile_id', profile_id)
        .maybeSingle();
    if (error) throw new MnemeError('SB_PROFILE_GET', error.message, 502);
    if (!data) return null;
    return rowToProfile(data as ProfileRow);
}

export async function ensureProfile(
    sb: SupabaseClient,
    profile_id: string,
    sovereign_pk: Uint8Array,
    sigma_mask?: Uint8Array,
): Promise<Profile> {
    const existing = await getProfile(sb, profile_id);
    if (existing) return existing;
    const mask = sigma_mask ?? defaultMask();
    const { data, error } = await sb
        .from('mneme_profiles')
        .insert({
            profile_id,
            sovereign_pk: bytesToPgBytes(sovereign_pk),
            sigma_mask:   bytesToPgBytes(mask),
        })
        .select('*')
        .single();
    if (error) throw new MnemeError('SB_PROFILE_INSERT', error.message, 502);
    return rowToProfile(data as ProfileRow);
}

// ── Message insert (idempotent) ────────────────────────────────────────

export async function insertMessages(
    sb: SupabaseClient,
    rows: Array<Omit<Message, 'sigma_mask'> & { sigma_mask: Uint8Array }>,
): Promise<{ stored: number; deduped: number }> {
    if (rows.length === 0) return { stored: 0, deduped: 0 };
    const payload = rows.map(r => ({
        id:         r.id,
        profile_id: r.profile_id,
        session_id: r.session_id,
        role:       r.role,
        content:    r.content,
        ts:         r.ts,
        sigma_mask: bytesToPgBytes(r.sigma_mask),
    }));
    const { data, error } = await sb
        .from('mneme_messages')
        .upsert(payload, { onConflict: 'id', ignoreDuplicates: true })
        .select('id');
    if (error) throw new MnemeError('SB_MSG_INSERT', error.message, 502);
    const stored = (data ?? []).length;
    return { stored, deduped: rows.length - stored };
}

// ── Memory insert ──────────────────────────────────────────────────────

export interface MemoryInsertInput {
    profile_id:     string;
    type:           MemoryType;
    csl:            string;
    paraphrase:     string;
    topic_key:      string | null;
    search_queries: string[];
    source_msg_ids: string[];
    sigma_mask:     Uint8Array;
}

export async function insertMemory(
    sb: SupabaseClient,
    m: MemoryInsertInput,
): Promise<Memory> {
    const { data, error } = await sb
        .from('mneme_memories')
        .insert({
            profile_id:     m.profile_id,
            type:           m.type,
            csl:            m.csl,
            paraphrase:     m.paraphrase,
            topic_key:      m.topic_key,
            search_queries: m.search_queries,
            source_msg_ids: m.source_msg_ids,
            sigma_mask:     bytesToPgBytes(m.sigma_mask),
        })
        .select('*')
        .single();
    if (error) throw new MnemeError('SB_MEM_INSERT', error.message, 502);
    return rowToMemory(data as MemoryRow);
}

export async function updateMemoryEmbedding(
    sb: SupabaseClient,
    memory_id: string,
    embedding: Float32Array,
): Promise<void> {
    const lit = toPgVectorLiteral(embedding);
    const { error } = await sb
        .from('mneme_memories')
        .update({ embedding: lit })
        .eq('id', memory_id);
    if (error) throw new MnemeError('SB_MEM_EMB', error.message, 502);
}

export async function getMemory(
    sb: SupabaseClient,
    profile_id: string,
    memory_id: string,
): Promise<Memory | null> {
    const { data, error } = await sb
        .from('mneme_memories')
        .select('*')
        .eq('profile_id', profile_id)
        .eq('id', memory_id)
        .maybeSingle();
    if (error) throw new MnemeError('SB_MEM_GET', error.message, 502);
    if (!data) return null;
    return rowToMemory(data as MemoryRow);
}

export async function getMemoriesByIds(
    sb: SupabaseClient,
    profile_id: string,
    memory_ids: string[],
): Promise<Memory[]> {
    if (memory_ids.length === 0) return [];
    const { data, error } = await sb
        .from('mneme_memories')
        .select('*')
        .eq('profile_id', profile_id)
        .in('id', memory_ids);
    if (error) throw new MnemeError('SB_MEM_BATCH', error.message, 502);
    return (data ?? []).map(r => rowToMemory(r as MemoryRow));
}

// ── List ───────────────────────────────────────────────────────────────

export async function listMemories(
    sb: SupabaseClient,
    profile_id: string,
    opts: { type?: MemoryType; limit?: number; cursor?: string } = {},
): Promise<{ memories: Memory[]; next_cursor: string | null }> {
    const limit = Math.min(Math.max(opts.limit ?? 50, 1), 200);
    let q = sb
        .from('mneme_memories')
        .select('*')
        .eq('profile_id', profile_id)
        .is('superseded_by', null);
    if (opts.type) q = q.eq('type', opts.type);
    if (opts.cursor) q = q.lt('created_at', opts.cursor);
    const { data, error } = await q
        .order('created_at', { ascending: false })
        .limit(limit + 1);
    if (error) throw new MnemeError('SB_LIST', error.message, 502);
    const rows = (data ?? []).map(r => rowToMemory(r as MemoryRow));
    const hasMore = rows.length > limit;
    const memories = hasMore ? rows.slice(0, limit) : rows;
    const next_cursor = hasMore && memories.length > 0
        ? memories[memories.length - 1]!.created_at
        : null;
    return { memories, next_cursor };
}

// ── Forget (sigma-mask revoke + cascade) ───────────────────────────────

export async function forgetMemory(
    sb: SupabaseClient,
    profile_id: string,
    memory_id: string,
    reason: string,
    caller_pk?: Uint8Array,
): Promise<{ revoked: boolean; cascade: number }> {
    const cur = await getMemory(sb, profile_id, memory_id);
    if (!cur) return { revoked: false, cascade: 0 };
    const ts = Math.floor(Date.now() / 1000);

    // Walk superseded-by chain backwards: any older row that points to this
    // memory (or is pointed to by it) gets revoked too.
    const idsToRevoke = new Set<string>([memory_id]);
    // Collect ancestors (this memory's superseded_by chain back).
    let walking: string | null = cur.superseded_by;
    while (walking && !idsToRevoke.has(walking)) {
        idsToRevoke.add(walking);
        const anc = await getMemory(sb, profile_id, walking);
        if (!anc) break;
        walking = anc.superseded_by;
    }
    // Collect descendants (rows whose superseded_by = current).
    const { data: descendants } = await sb
        .from('mneme_memories')
        .select('id')
        .eq('profile_id', profile_id)
        .eq('superseded_by', memory_id);
    for (const d of (descendants ?? []) as Array<{ id: string }>) {
        idsToRevoke.add(d.id);
    }

    let cascade = 0;
    for (const id of idsToRevoke) {
        const m = await getMemory(sb, profile_id, id);
        if (!m) continue;
        const newMask = revokeMask(m.sigma_mask, ts);
        const { error } = await sb
            .from('mneme_memories')
            .update({ sigma_mask: bytesToPgBytes(newMask) })
            .eq('id', id);
        if (error) throw new MnemeError('SB_FORGET', error.message, 502);
        cascade++;
    }

    await sb
        .from('mneme_audit')
        .insert({
            profile_id,
            kind: 'forget',
            memory_id,
            caller_pk: caller_pk ? bytesToPgBytes(caller_pk) : null,
            details: { reason, cascade },
        });

    return { revoked: true, cascade };
}

// ── Audit ──────────────────────────────────────────────────────────────

export async function emitAudit(
    sb: SupabaseClient,
    profile_id: string,
    kind: 'ingest' | 'remember' | 'recall' | 'forget' | 'supersede' |
          'export' | 'vacuum' | 'csl_invalid' | 'verify_drop',
    details: Record<string, unknown> = {},
    memory_id?: string,
    caller_pk?: Uint8Array,
): Promise<void> {
    const row: Record<string, unknown> = {
        profile_id,
        kind,
        details,
    };
    if (memory_id) row['memory_id'] = memory_id;
    if (caller_pk) row['caller_pk'] = bytesToPgBytes(caller_pk);
    const { error } = await sb.from('mneme_audit').insert(row);
    if (error) {
        // Non-fatal — log via stderr but do not throw (audit-emit best-effort).
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({
            evt: 'mneme.audit.fail', kind, profile_id, error: error.message,
        }));
    }
}

// ── Export bundle ──────────────────────────────────────────────────────

export async function exportProfile(
    sb: SupabaseClient,
    profile_id: string,
): Promise<{ profile: Profile; memories: Memory[]; messages: Message[] }> {
    const profile = await getProfile(sb, profile_id);
    if (!profile) throw new MnemeError('PROFILE_404', `profile ${profile_id} not found`, 404);

    const { data: memRows, error: memErr } = await sb
        .from('mneme_memories')
        .select('*')
        .eq('profile_id', profile_id)
        .order('created_at', { ascending: true });
    if (memErr) throw new MnemeError('SB_EXPORT_MEM', memErr.message, 502);

    const { data: msgRows, error: msgErr } = await sb
        .from('mneme_messages')
        .select('*')
        .eq('profile_id', profile_id)
        .order('ts', { ascending: true });
    if (msgErr) throw new MnemeError('SB_EXPORT_MSG', msgErr.message, 502);

    return {
        profile,
        memories: (memRows ?? []).map(r => rowToMemory(r as MemoryRow)),
        messages: (msgRows ?? []).map(r => rowToMessage(r as MessageRow)),
    };
}

// ══════════════════════════════════════════════════════════════════════
// 6 RETRIEVAL CHANNELS  (spec § 44 RETRIEVE STAGE-3)
// ══════════════════════════════════════════════════════════════════════

export interface RetrieveParams {
    profile_id:  string;
    query:       string;
    fts_terms:   string[];
    topic_keys:  string[];
    vec_q:       Float32Array | null;
    vec_h:       Float32Array | null;
    types?:      MemoryType[];
}

const FTS_LIMIT       = 20;
const VEC_LIMIT       = 20;
const FTS_MSGS_LIMIT  = 10;
const TOPIC_LIMIT     = 5;

// ── Channel 1 : fts_csl (simple tokeniser) ────────────────────────────

export async function channelFtsCsl(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<ChannelHit[]> {
    const tsq = composeTsQuery(p.fts_terms);
    if (tsq.length === 0) return [];
    const sql = `
        SELECT id, ts_rank_cd(csl_tsv, query) AS s
        FROM public.mneme_memories,
             to_tsquery('simple', $tsq$${tsq.replace(/\$/g, '')}$tsq$) AS query
        WHERE profile_id = $1
          AND superseded_by IS NULL
          AND public.mneme_mask_revoked_at(sigma_mask) = 0
          AND csl_tsv @@ query
        ORDER BY s DESC
        LIMIT ${FTS_LIMIT}
    `;
    return rpcRows(sb, sql, [p.profile_id]);
}

// ── Channel 2 : fts_paraphrase (english stemmer) ──────────────────────

export async function channelFtsParaphrase(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<ChannelHit[]> {
    if (p.query.trim().length === 0) return [];
    const safe = p.query.replace(/[^A-Za-z0-9_\s\-]/g, ' ').trim();
    if (safe.length === 0) return [];
    const sql = `
        SELECT id, ts_rank_cd(paraphrase_tsv, query) AS s
        FROM public.mneme_memories,
             plainto_tsquery('english', $tsq$${safe.replace(/\$/g, '')}$tsq$) AS query
        WHERE profile_id = $1
          AND superseded_by IS NULL
          AND public.mneme_mask_revoked_at(sigma_mask) = 0
          AND paraphrase_tsv @@ query
        ORDER BY s DESC
        LIMIT ${FTS_LIMIT}
    `;
    return rpcRows(sb, sql, [p.profile_id]);
}

// ── Channel 3 : fts_messages (raw-net safety) ──────────────────────────

export async function channelFtsMessages(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<ChannelHit[]> {
    if (p.query.trim().length === 0) return [];
    const safe = p.query.replace(/[^A-Za-z0-9_\s\-]/g, ' ').trim();
    if (safe.length === 0) return [];
    const sql = `
        WITH hits AS (
            SELECT id
              FROM public.mneme_messages
             WHERE profile_id = $1
               AND content_tsv @@ plainto_tsquery('english', $tsq$${safe.replace(/\$/g, '')}$tsq$)
             ORDER BY ts DESC
             LIMIT ${FTS_MSGS_LIMIT}
        )
        SELECT m.id, 1.0::float / (1 + row_number() OVER ())::float AS s
          FROM public.mneme_memories m
          JOIN hits h ON h.id = ANY(m.source_msg_ids)
         WHERE m.profile_id = $1
           AND m.superseded_by IS NULL
           AND public.mneme_mask_revoked_at(m.sigma_mask) = 0
         LIMIT ${FTS_LIMIT}
    `;
    return rpcRows(sb, sql, [p.profile_id]);
}

// ── Channel 4 : topic_exact ────────────────────────────────────────────

export async function channelTopicExact(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<ChannelHit[]> {
    if (p.topic_keys.length === 0) return [];
    const { data, error } = await sb
        .from('mneme_memories')
        .select('id, topic_key')
        .eq('profile_id', p.profile_id)
        .is('superseded_by', null)
        .in('topic_key', p.topic_keys)
        .limit(TOPIC_LIMIT);
    if (error) throw new MnemeError('SB_TOPIC', error.message, 502);
    const order = new Map(p.topic_keys.map((k, i) => [k, i]));
    return ((data ?? []) as Array<{ id: string; topic_key: string }>)
        .sort((a, b) => (order.get(a.topic_key) ?? 999) - (order.get(b.topic_key) ?? 999))
        .map(r => ({ memory_id: r.id, score: 1.0 }));
}

// ── Channel 5 : vec_direct ─────────────────────────────────────────────

export async function channelVecDirect(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<ChannelHit[]> {
    if (!p.vec_q) return [];
    const lit = toPgVectorLiteral(p.vec_q);
    const sql = `
        SELECT id, 1 - (embedding <=> $vec$${lit}$vec$::vector) AS s
        FROM public.mneme_memories
        WHERE profile_id = $1
          AND superseded_by IS NULL
          AND public.mneme_mask_revoked_at(sigma_mask) = 0
          AND embedding IS NOT NULL
        ORDER BY embedding <=> $vec$${lit}$vec$::vector
        LIMIT ${VEC_LIMIT}
    `;
    return rpcRows(sb, sql, [p.profile_id]);
}

// ── Channel 6 : vec_hyde ──────────────────────────────────────────────

export async function channelVecHyde(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<ChannelHit[]> {
    if (!p.vec_h) return [];
    const lit = toPgVectorLiteral(p.vec_h);
    const sql = `
        SELECT id, 1 - (embedding <=> $vec$${lit}$vec$::vector) AS s
        FROM public.mneme_memories
        WHERE profile_id = $1
          AND superseded_by IS NULL
          AND public.mneme_mask_revoked_at(sigma_mask) = 0
          AND embedding IS NOT NULL
        ORDER BY embedding <=> $vec$${lit}$vec$::vector
        LIMIT ${VEC_LIMIT}
    `;
    return rpcRows(sb, sql, [p.profile_id]);
}

// ── Aggregate dispatcher ──────────────────────────────────────────────

export async function retrieveAll(
    sb: SupabaseClient,
    p: RetrieveParams,
): Promise<Partial<Record<ChannelName, ChannelHit[]>>> {
    const results = await Promise.allSettled([
        channelFtsCsl(sb, p),
        channelFtsParaphrase(sb, p),
        channelFtsMessages(sb, p),
        channelTopicExact(sb, p),
        channelVecDirect(sb, p),
        channelVecHyde(sb, p),
    ]);
    const out: Partial<Record<ChannelName, ChannelHit[]>> = {};
    const order: ChannelName[] = [
        'fts_csl', 'fts_paraphrase', 'fts_messages',
        'topic_exact', 'vec_direct', 'vec_hyde',
    ];
    for (let i = 0; i < results.length; i++) {
        const name = order[i]!;
        const r = results[i]!;
        if (r.status === 'fulfilled') {
            out[name] = r.value;
        } else {
            // Channel failure is non-fatal — log and continue.
            // eslint-disable-next-line no-console
            console.error(JSON.stringify({
                evt: 'mneme.channel.fail', channel: name,
                err: r.reason instanceof Error ? r.reason.message : String(r.reason),
            }));
            out[name] = [];
        }
    }
    return out;
}

// ── RPC fallback ───────────────────────────────────────────────────────
// Supabase JS lacks a generic SQL escape hatch; we use rpc() to a tiny SQL
// passthrough function (`mneme_exec_select` — see migration 0042 / fallback
// inline) when present, else degrade to an empty result set with a warning.
// All channel SQL above compiles as PARAMETERLESS strings (literals injected
// safely via $..$ dollar-quoted blocks).

interface ExecRow { id: string; s: number }

async function rpcRows(
    sb: SupabaseClient,
    sql: string,
    args: unknown[],
): Promise<ChannelHit[]> {
    // The RPC function is `mneme_exec_select(sql text, args jsonb) RETURNS SETOF mneme_exec_row`.
    // Created by migration 0042 if available. When missing we return [] silently
    // to keep retrieve flows operational against minimal deployments.
    try {
        const { data, error } = await sb.rpc('mneme_exec_select', {
            q_sql:  sql,
            q_args: args,
        });
        if (error) {
            if (/function .* does not exist/i.test(error.message)) {
                return [];
            }
            // eslint-disable-next-line no-console
            console.error(JSON.stringify({
                evt: 'mneme.rpc.fail', err: error.message,
            }));
            return [];
        }
        const rows = (data ?? []) as ExecRow[];
        return rows.map(r => ({
            memory_id: r.id,
            score:     typeof r.s === 'number' ? r.s : Number(r.s ?? 0),
        }));
    } catch (e) {
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({
            evt: 'mneme.rpc.throw',
            err: e instanceof Error ? e.message : String(e),
        }));
        return [];
    }
}

// ── Reciprocal-rank fusion ─────────────────────────────────────────────

const RRF_K = 60;
export const CHANNEL_WEIGHTS: Record<ChannelName, number> = {
    topic_exact:    1.0,
    fts_csl:        0.7,
    vec_hyde:       0.7,
    vec_direct:     0.6,
    fts_paraphrase: 0.5,
    fts_messages:   0.3,
};

export interface RrfResult {
    memory_id: string;
    score:     number;
}

export function reciprocalRankFusion(
    channels: Partial<Record<ChannelName, ChannelHit[]>>,
    k: number,
): RrfResult[] {
    const scores = new Map<string, number>();
    for (const [name, hits] of Object.entries(channels) as Array<[ChannelName, ChannelHit[]]>) {
        const w = CHANNEL_WEIGHTS[name] ?? 0;
        if (w === 0 || !hits) continue;
        // hits already in channel-rank order
        for (let rank = 0; rank < hits.length; rank++) {
            const h = hits[rank]!;
            const inc = w / (RRF_K + rank + 1);
            scores.set(h.memory_id, (scores.get(h.memory_id) ?? 0) + inc);
        }
    }
    const sorted = Array.from(scores.entries())
        .sort((a, b) => b[1] - a[1])
        .slice(0, k)
        .map(([memory_id, score]) => ({ memory_id, score }));
    return sorted;
}

// Convenience : maximum confidence across channels for a memory_id.
export function maxScoreFor(
    memory_id: string,
    channels: Partial<Record<ChannelName, ChannelHit[]>>,
): number {
    let best = 0;
    for (const hits of Object.values(channels)) {
        if (!hits) continue;
        for (const h of hits) {
            if (h.memory_id === memory_id && h.score > best) best = h.score;
        }
    }
    return best;
}

// ── Bytea helper export (for routes that need to pass raw values) ─────

export { maskFromHex, maskToHex };

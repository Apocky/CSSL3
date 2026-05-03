// cssl-edge/lib/mneme/pipeline-retrieve.ts
// MNEME — 7-stage retrieval pipeline.
//
// Spec : ../../specs/44_MNEME_PIPELINES.csl § RETRIEVE
//
// STAGES
//   1. analyze + embed-raw  (parallel)
//   2. embed-HyDE
//   3. 6-channel parallel retrieval
//   4. RRF fusion
//   5. temporal pre-compute (regex date arithmetic)
//   6. synthesis (Sonnet)
//   7. post-process + envelope

import type { SupabaseClient } from '@supabase/supabase-js';
import { embedQuery } from './embed';
import {
    retrieveAll,
    reciprocalRankFusion,
    maxScoreFor,
    getMemoriesByIds,
    emitAudit,
    type RetrieveParams,
} from './store';
import { analyzeQuery, type QueryAnalyzeDeps } from './prompts/query-analyze';
import { synthesize, type SynthesizeDeps } from './prompts/synthesize';
import type {
    ChannelName,
    ChannelHit,
    Memory,
    QueryAnalysis,
    RetrievalDebug,
} from './types';

// ── Pipeline I/O ──────────────────────────────────────────────────────

export interface RetrieveInput {
    profile_id:    string;
    query:         string;
    k?:            number;
    types?:        Array<'fact' | 'event' | 'instruction' | 'task'>;
    audience_bits?: number;
    debug?:        boolean;
}

export interface RetrieveOutput {
    result_nl:  string;
    result_csl: string;
    citations:  string[];
    confidence: number;
    debug?:     RetrievalDebug;
}

export type RetrieveDeps = QueryAnalyzeDeps & SynthesizeDeps & {
    embed?:    (text: string) => Promise<Float32Array>;
    nowMs?:    () => number;
};

// ── Stage 5 : temporal pre-compute ────────────────────────────────────

export interface TemporalFact {
    label:  string;
    iso:    string;
}

const RE_YESTERDAY = /\byesterday\b/i;
const RE_TOMORROW  = /\btomorrow\b/i;
const RE_TODAY     = /\btoday\b/i;
const RE_NDAYS_AGO = /\b(\d+)\s+days?\s+ago\b/i;
const RE_NWEEKS_AGO = /\b(\d+)\s+weeks?\s+ago\b/i;
const RE_NMONTHS_AGO = /\b(\d+)\s+months?\s+ago\b/i;
const RE_LAST_WEEK = /\blast\s+week\b/i;
const RE_LAST_MONTH = /\blast\s+month\b/i;
const RE_ON_DATE   = /\bon\s+(\d{4}-\d{2}-\d{2})\b/i;

function daysAgo(now: Date, n: number): string {
    const d = new Date(now);
    d.setUTCDate(d.getUTCDate() - n);
    return d.toISOString().slice(0, 10);
}

export function computeTemporalFacts(query: string, nowMs: number): TemporalFact[] {
    const out: TemporalFact[] = [];
    const now = new Date(nowMs);
    out.push({ label: 'today', iso: now.toISOString().slice(0, 10) });

    if (RE_YESTERDAY.test(query)) {
        out.push({ label: 'yesterday', iso: daysAgo(now, 1) });
    }
    if (RE_TOMORROW.test(query)) {
        out.push({ label: 'tomorrow', iso: daysAgo(now, -1) });
    }
    if (RE_TODAY.test(query)) {
        out.push({ label: 'today', iso: daysAgo(now, 0) });
    }
    const m1 = query.match(RE_NDAYS_AGO);
    if (m1 && m1[1]) {
        const n = parseInt(m1[1], 10);
        if (Number.isFinite(n) && n >= 0) {
            out.push({ label: `${n} days ago`, iso: daysAgo(now, n) });
        }
    }
    const m2 = query.match(RE_NWEEKS_AGO);
    if (m2 && m2[1]) {
        const n = parseInt(m2[1], 10);
        if (Number.isFinite(n) && n >= 0) {
            out.push({ label: `${n} weeks ago`, iso: daysAgo(now, n * 7) });
        }
    }
    const m3 = query.match(RE_NMONTHS_AGO);
    if (m3 && m3[1]) {
        const n = parseInt(m3[1], 10);
        if (Number.isFinite(n) && n >= 0) {
            out.push({ label: `${n} months ago`, iso: daysAgo(now, n * 30) });
        }
    }
    if (RE_LAST_WEEK.test(query)) {
        out.push({ label: 'last week (start)', iso: daysAgo(now, 7) });
    }
    if (RE_LAST_MONTH.test(query)) {
        out.push({ label: 'last month (start)', iso: daysAgo(now, 30) });
    }
    const m4 = query.match(RE_ON_DATE);
    if (m4 && m4[1]) {
        out.push({ label: `on ${m4[1]}`, iso: m4[1] });
    }
    return out;
}

// ── Pipeline orchestrator ──────────────────────────────────────────────

const DEFAULT_K = 5;

export async function retrievePipeline(
    sb: SupabaseClient | null,
    input: RetrieveInput,
    deps: RetrieveDeps = {},
): Promise<RetrieveOutput> {
    const k = Math.min(Math.max(input.k ?? DEFAULT_K, 1), 20);
    const startMs = Date.now();
    const latency: Record<string, number> = {};
    const debug: RetrievalDebug | undefined = input.debug
        ? { channel_scores: {}, rrf_top_k: [], synth_prompt_tokens: 0, latency_ms: latency }
        : undefined;

    const embedFn = deps.embed ?? embedQuery;

    // Stage 1+2 : analyze + embed-raw + embed-HyDE (mostly parallel)
    const t1 = Date.now();
    const analyzeP = analyzeQuery(input.query, deps);
    const embedQueryP = embedFn(input.query).catch(() => null);
    const analysis: QueryAnalysis = await analyzeP;
    const vec_q = await embedQueryP;
    let vec_h: Float32Array | null = null;
    if (analysis.hyde_csl || analysis.hyde_paraphrase) {
        try {
            vec_h = await embedFn((analysis.hyde_csl + '\n' + analysis.hyde_paraphrase).trim());
        } catch {
            vec_h = null;
        }
    }
    latency['analyze_embed'] = Date.now() - t1;

    // Stage 3 : 6 channels parallel (skipped if no SB)
    const t3 = Date.now();
    const params: RetrieveParams = {
        profile_id: input.profile_id,
        query:      input.query,
        fts_terms:  analysis.fts_terms,
        topic_keys: analysis.topic_keys,
        vec_q,
        vec_h,
        types:      input.types,
    };
    let channels: Partial<Record<ChannelName, ChannelHit[]>> = {};
    if (sb) {
        channels = await retrieveAll(sb, params);
    }
    latency['retrieve'] = Date.now() - t3;
    if (debug) debug.channel_scores = mapChannelScores(channels);

    // Stage 4 : RRF
    const t4 = Date.now();
    const fused = reciprocalRankFusion(channels, k);
    latency['rrf'] = Date.now() - t4;
    if (debug) debug.rrf_top_k = fused.map(f => [f.memory_id, f.score]);

    // Stage 5 : temporal pre-compute
    const t5 = Date.now();
    const nowMs = (deps.nowMs ?? Date.now)();
    const temporal_facts = analysis.is_temporal
        ? computeTemporalFacts(input.query, nowMs)
        : [];
    latency['temporal'] = Date.now() - t5;

    // Fetch full rows for top-k.
    const t6 = Date.now();
    let memories: Memory[] = [];
    if (sb && fused.length > 0) {
        memories = await getMemoriesByIds(sb, input.profile_id, fused.map(f => f.memory_id));
        // re-order to match fused score order
        const order = new Map(fused.map((f, i) => [f.memory_id, i]));
        memories.sort((a, b) => (order.get(a.id) ?? 999) - (order.get(b.id) ?? 999));
    }
    latency['fetch_topk'] = Date.now() - t6;

    // Stage 6 : synthesis
    const t7 = Date.now();
    const pre_computed = temporal_facts.map(f => `${f.label} = ${f.iso}`);
    const synth = await synthesize({
        query: input.query,
        memories,
        pre_computed,
    }, deps);
    latency['synth'] = Date.now() - t7;

    // Stage 7 : post-process
    const finalConfidence = synth.confidence;
    const out: RetrieveOutput = {
        result_nl:  synth.result_nl,
        result_csl: synth.result_csl,
        citations:  synth.citations,
        confidence: finalConfidence,
    };
    if (debug) {
        out.debug = debug;
    }

    if (sb) {
        await emitAudit(sb, input.profile_id, 'recall', {
            query:       input.query,
            citations:   synth.citations,
            confidence:  finalConfidence,
            latency_ms:  Date.now() - startMs,
        });
    }
    // Cap confidence at max-channel-score for citations (defense vs over-confident synth)
    if (out.citations.length > 0) {
        const observed = out.citations
            .map(id => maxScoreFor(id, channels))
            .reduce((a, b) => Math.max(a, b), 0);
        if (observed > 0 && out.confidence > observed + 0.1) {
            out.confidence = Math.min(1, observed + 0.1);
        }
    }
    return out;
}

function mapChannelScores(
    channels: Partial<Record<ChannelName, ChannelHit[]>>,
): RetrievalDebug['channel_scores'] {
    const out: RetrievalDebug['channel_scores'] = {};
    for (const [name, hits] of Object.entries(channels) as Array<[ChannelName, ChannelHit[]]>) {
        out[name] = hits.map(h => [h.memory_id, h.score]);
    }
    return out;
}

// cssl-edge/lib/mneme/pipeline-ingest.ts
// MNEME — 8-stage ingestion pipeline.
//
// Spec : ../../specs/44_MNEME_PIPELINES.csl § INGEST
//
// STAGES
//   1. msg-id assign  (deterministic sha256[:32])
//   2. extract-full   (chunked · concurrent · canonical CSL)
//   3. extract-detail (windowed · concrete-values pass; skipped < 9 msgs)
//   4. verify         (8 checks, drop on fail)
//   5. classify       (type + topic_key + search_queries)
//   6. supersede      (DB trigger handles forward-pointer; we just INSERT)
//   7. write          (insert messages + memories)
//   8. embed          (Voyage; async post-response)

import type { SupabaseClient } from '@supabase/supabase-js';
import { createHash } from 'node:crypto';

import {
    type Memory,
    type Message,
    type ExtractedCandidate,
    type VerifiedCandidate,
    type ClassifiedCandidate,
    type Role,
    MnemeError,
} from './types';
import { validateCsl, composeEmbeddingText } from './csl';
import { embedDocument } from './embed';
import {
    insertMessages,
    insertMemory,
    updateMemoryEmbedding,
    emitAudit,
    type MemoryInsertInput,
} from './store';
import { defaultMask, envDefaultMask } from './sigma';

import { extractFull,   type ExtractFullDeps   } from './prompts/extract-full';
import { extractDetail, type ExtractDetailDeps } from './prompts/extract-detail';
import { verifyCandidate, type VerifyDeps } from './prompts/verify';
import { classifyCandidate, type ClassifyDeps } from './prompts/classify';

// ── Stage 1 : deterministic message ID ─────────────────────────────────

export function deterministicMsgId(session_id: string, role: string, content: string): string {
    return createHash('sha256')
        .update(session_id, 'utf8')
        .update('|', 'utf8')
        .update(role, 'utf8')
        .update('|', 'utf8')
        .update(content, 'utf8')
        .digest('hex')
        .slice(0, 32);
}

// ── Pipeline I/O ──────────────────────────────────────────────────────

export interface IngestInput {
    profile_id:  string;
    session_id:  string;
    messages:    Array<{ role: Role; content: string }>;
    sigma_mask?: Uint8Array;     // override default per-message
}

export interface IngestResult {
    stored:    number;       // memories newly inserted
    deduped:   number;       // memories already present
    extracted: number;       // candidates seen
    dropped:   number;       // verifier or csl-validator rejects
    memory_ids: string[];    // newly inserted memory ids
}

export type IngestDeps = ExtractFullDeps & ExtractDetailDeps & VerifyDeps & ClassifyDeps & {
    embed?:   (text: string) => Promise<Float32Array>;
    nowIso?:  () => string;
};

// ── Levenshtein-based dedupe (cheap, O(N*M)) ──────────────────────────

function lev(a: string, b: string): number {
    if (a === b) return 0;
    const al = a.length, bl = b.length;
    if (al === 0) return bl;
    if (bl === 0) return al;
    let prev = new Array<number>(bl + 1);
    let curr = new Array<number>(bl + 1);
    for (let j = 0; j <= bl; j++) prev[j] = j;
    for (let i = 1; i <= al; i++) {
        curr[0] = i;
        for (let j = 1; j <= bl; j++) {
            const cost = a.charCodeAt(i - 1) === b.charCodeAt(j - 1) ? 0 : 1;
            curr[j] = Math.min(
                (prev[j] ?? 0) + 1,
                (curr[j - 1] ?? 0) + 1,
                (prev[j - 1] ?? 0) + cost,
            );
        }
        [prev, curr] = [curr, prev];
    }
    return prev[bl] ?? 0;
}

function normalisedLev(a: string, b: string): number {
    const m = Math.max(a.length, b.length);
    if (m === 0) return 0;
    return lev(a, b) / m;
}

// Merge full + detail candidates, dedupe by (csl) similarity. Keep the longer.
export function mergeCandidates(
    full: ExtractedCandidate[],
    detail: ExtractedCandidate[],
): ExtractedCandidate[] {
    const all = [...full, ...detail];
    const out: ExtractedCandidate[] = [];
    for (const c of all) {
        let merged = false;
        for (let i = 0; i < out.length; i++) {
            if (normalisedLev(out[i]!.csl, c.csl) < 0.15) {
                if (c.csl.length > out[i]!.csl.length) out[i] = c;
                merged = true;
                break;
            }
        }
        if (!merged) out.push(c);
    }
    return out;
}

// ── Pipeline orchestrator ──────────────────────────────────────────────

export async function ingestPipeline(
    sb: SupabaseClient | null,
    input: IngestInput,
    deps: IngestDeps = {},
): Promise<IngestResult> {
    const result: IngestResult = {
        stored: 0, deduped: 0, extracted: 0, dropped: 0, memory_ids: [],
    };
    const nowIso = (deps.nowIso ?? (() => new Date().toISOString()))();

    // Stage 1 : assign deterministic IDs
    const messages: Message[] = input.messages.map(m => {
        const id = deterministicMsgId(input.session_id, m.role, m.content);
        return {
            id,
            profile_id: input.profile_id,
            session_id: input.session_id,
            role:       m.role,
            content:    m.content,
            ts:         nowIso,
            sigma_mask: input.sigma_mask ?? envDefaultMask(),
        };
    });

    // Stage 2 : extract (full + detail in parallel)
    const [fullCands, detailCands] = await Promise.all([
        extractFull(input.messages, deps),
        extractDetail(input.messages, deps),
    ]);
    const merged = mergeCandidates(fullCands, detailCands);
    result.extracted = merged.length;

    // Build a source transcript snapshot for verify-step grounding.
    const transcript = input.messages
        .map((m, i) => `[${i}] ${m.role}: ${m.content}`)
        .join('\n');

    // Stage 3 : verify
    const verified: VerifiedCandidate[] = [];
    for (const c of merged) {
        const v = await verifyCandidate(c, transcript, deps);
        verified.push(v);
        if (v.verdict === 'dropped') result.dropped++;
    }

    // Stage 4 : classify (skip dropped)
    const classified: ClassifiedCandidate[] = [];
    for (const v of verified) {
        if (v.verdict === 'dropped') continue;
        const c = await classifyCandidate(v, deps);
        if (!c) {
            result.dropped++;
            continue;
        }
        // Stage 6 (CSL gate) — drop if csl is malformed.
        const cslCheck = validateCsl(c.csl, c.type);
        if (!cslCheck.ok) {
            result.dropped++;
            if (sb) {
                await emitAudit(sb, input.profile_id, 'csl_invalid', {
                    csl: c.csl,
                    diags: cslCheck.diags,
                });
            }
            continue;
        }
        classified.push(c);
    }

    // Stage 7 : write (messages + memories)
    if (sb) {
        await insertMessages(sb, messages.map(m => ({
            id:         m.id,
            profile_id: m.profile_id,
            session_id: m.session_id,
            role:       m.role,
            content:    m.content,
            ts:         m.ts,
            sigma_mask: m.sigma_mask,
        })));
    }

    const inserted: Memory[] = [];
    for (const c of classified) {
        const insertInput: MemoryInsertInput = {
            profile_id:     input.profile_id,
            type:           c.type,
            csl:            c.csl,
            paraphrase:     c.paraphrase,
            topic_key:      c.topic_key,
            search_queries: c.search_queries,
            source_msg_ids: messages.map(m => m.id),
            sigma_mask:     input.sigma_mask ?? envDefaultMask(),
        };
        if (sb) {
            try {
                const m = await insertMemory(sb, insertInput);
                inserted.push(m);
                result.memory_ids.push(m.id);
            } catch (e) {
                // Insert failure → audit + drop.
                result.dropped++;
                await emitAudit(sb, input.profile_id, 'verify_drop', {
                    csl: c.csl,
                    error: e instanceof Error ? e.message : String(e),
                });
            }
        } else {
            // Mock-mode : pretend it inserted with synthesized id.
            const fake: Memory = {
                id:             `mock-${result.memory_ids.length}`,
                profile_id:     input.profile_id,
                type:           c.type,
                csl:            c.csl,
                paraphrase:     c.paraphrase,
                topic_key:      c.topic_key,
                search_queries: c.search_queries,
                source_msg_ids: messages.map(m => m.id),
                superseded_by:  null,
                sigma_mask:     defaultMask(),
                created_at:     nowIso,
                embedding:      null,
            };
            inserted.push(fake);
            result.memory_ids.push(fake.id);
        }
    }
    result.stored = inserted.length;

    if (sb) {
        await emitAudit(sb, input.profile_id, 'ingest', {
            session_id: input.session_id,
            stored:     result.stored,
            extracted:  result.extracted,
            dropped:    result.dropped,
        });
    }

    // Stage 8 : embed (background-fire-and-forget when caller doesn't await)
    if (sb) {
        const embedFn = deps.embed ?? embedDocument;
        for (const m of inserted) {
            const text = composeEmbeddingText(m.csl, m.paraphrase, m.search_queries);
            try {
                const vec = await embedFn(text);
                await updateMemoryEmbedding(sb, m.id, vec);
            } catch (e) {
                if (e instanceof MnemeError && e.code === 'EMBED_TIMEOUT') {
                    // leave embedding NULL — cron-vacuum re-embeds @ 03:00 UTC
                    continue;
                }
                // eslint-disable-next-line no-console
                console.error(JSON.stringify({
                    evt: 'mneme.embed.fail',
                    memory_id: m.id,
                    err: e instanceof Error ? e.message : String(e),
                }));
            }
        }
    }

    return result;
}

// ── Single-shot remember (skip extraction, run validate+classify+embed only) ─

export interface RememberInput {
    profile_id:  string;
    csl:         string;
    paraphrase?: string;
    type?:       'fact' | 'event' | 'instruction' | 'task';
    topic_key?:  string;
    sigma_mask?: Uint8Array;
}

export async function rememberPipeline(
    sb: SupabaseClient | null,
    input: RememberInput,
    deps: IngestDeps = {},
): Promise<Memory> {
    // CSL-validate
    const v = validateCsl(input.csl, input.type);
    if (!v.ok) {
        const msg = v.diags.map(d => `[${d.code}] ${d.msg}`).join('; ');
        throw new MnemeError('CSL_INVALID', `csl invalid: ${msg}`, 400);
    }

    let type = input.type;
    let topic_key = input.topic_key ?? v.topic_key;
    let paraphrase = input.paraphrase;
    let search_queries: string[] = [];

    if (!type || !paraphrase || search_queries.length === 0) {
        // Fall through to classifier so we get type / topic_key / queries.
        const fakeCand: VerifiedCandidate = {
            csl:        input.csl,
            paraphrase: paraphrase ?? input.csl,
            span:       [-1, -1],
            pass:       'full',
            verdict:    'pass',
        };
        const c = await classifyCandidate(fakeCand, deps);
        if (c) {
            type = type ?? c.type;
            topic_key = topic_key ?? c.topic_key;
            search_queries = c.search_queries;
        }
    }
    if (!type) type = 'fact';
    if (!paraphrase) paraphrase = input.csl;

    // Type discipline
    if (type === 'event' || type === 'task') {
        topic_key = null;
    }

    const insertInput: MemoryInsertInput = {
        profile_id:     input.profile_id,
        type,
        csl:            input.csl,
        paraphrase,
        topic_key:      topic_key ?? null,
        search_queries,
        source_msg_ids: [],
        sigma_mask:     input.sigma_mask ?? envDefaultMask(),
    };

    if (!sb) {
        // Mock mode
        return {
            id:             'mock-remember',
            profile_id:     input.profile_id,
            type,
            csl:            input.csl,
            paraphrase,
            topic_key:      topic_key ?? null,
            search_queries,
            source_msg_ids: [],
            superseded_by:  null,
            sigma_mask:     defaultMask(),
            created_at:     new Date().toISOString(),
            embedding:      null,
        };
    }
    const m = await insertMemory(sb, insertInput);
    await emitAudit(sb, input.profile_id, 'remember', { memory_id: m.id });

    // Embed (sync — remember is interactive, latency budget = 800ms)
    const embedFn = deps.embed ?? embedDocument;
    try {
        const text = composeEmbeddingText(m.csl, m.paraphrase, m.search_queries);
        const vec = await embedFn(text);
        await updateMemoryEmbedding(sb, m.id, vec);
    } catch {
        // best-effort
    }
    return m;
}

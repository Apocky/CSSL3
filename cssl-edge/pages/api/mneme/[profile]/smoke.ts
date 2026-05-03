// cssl-edge · GET /api/mneme/[profile]/smoke
// MNEME — exercise the in-process pipelines without reaching out to LLM/DB.
//
// Useful for proving the route surface compiles + runs in a fresh deploy.
// All upstream calls are stubbed via dependency injection; no API keys needed.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { ingestPipeline } from '@/lib/mneme/pipeline-ingest';
import { retrievePipeline } from '@/lib/mneme/pipeline-retrieve';
import type {
    IngestDeps,
} from '@/lib/mneme/pipeline-ingest';
import type {
    RetrieveDeps,
} from '@/lib/mneme/pipeline-retrieve';

interface SmokeResponse {
    ok: true;
    profile_id: string;
    ingest:   { stored: number; deduped: number; extracted: number; dropped: number };
    retrieve: { citations: string[]; confidence: number; result_csl: string };
    served_by: string;
    ts: string;
}

interface ErrorResponse {
    error: string;
    served_by: string;
    ts: string;
}

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;

// Stub callTool that fakes Anthropic responses based on tool name.
function makeStubCallTool(): NonNullable<IngestDeps['callTool']> {
    return async <T>(opts: { tool: { name: string }; user: string }): Promise<T> => {
        const name = opts.tool.name;
        switch (name) {
            case 'mneme_extract':
                return { extracted: [{
                    csl: 'user.pref.pkg-mgr ⊗ pnpm',
                    paraphrase: 'User prefers pnpm.',
                    span: [0, 0],
                }] } as unknown as T;
            case 'mneme_extract_detail':
                return { extracted: [] } as unknown as T;
            case 'mneme_verify':
                return { verdict: 'pass' } as unknown as T;
            case 'mneme_classify':
                return {
                    type: 'fact',
                    topic_key: 'user.pref.pkg-mgr',
                    search_queries: [
                        'which package manager?',
                        'pnpm or npm?',
                        'what JS package tool?',
                    ],
                } as unknown as T;
            case 'mneme_query_analyze':
                return {
                    topic_keys:      ['user.pref.pkg-mgr'],
                    fts_terms:       ['package','manager','pnpm','npm'],
                    hyde_csl:        'user.pref.pkg-mgr ⊗ pnpm',
                    hyde_paraphrase: 'The user prefers pnpm.',
                    is_temporal:     false,
                } as unknown as T;
            case 'mneme_synthesize':
                return {
                    result_nl:  'No memories were available in stub mode.',
                    result_csl: 'memory ⊗ ∅ @ query=stub',
                    citations:  [],
                    confidence: 0.0,
                } as unknown as T;
            default:
                return {} as T;
        }
    };
}

const STUB_VEC = (): Float32Array => {
    const v = new Float32Array(1024);
    for (let i = 0; i < v.length; i++) v[i] = (i % 13) / 13;
    return v;
};

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<SmokeResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.smoke', { method: req.method ?? 'GET' });
    const profile_id = String(req.query['profile'] ?? '');
    if (!PROFILE_RE.test(profile_id)) {
        const env = envelope();
        res.status(422).json({
            error: 'Invalid profile_id',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    const stubCall = makeStubCallTool();
    const deps: IngestDeps & RetrieveDeps = {
        callTool: stubCall,
        embed: async () => STUB_VEC(),
        nowIso: () => new Date(0).toISOString(),
        nowMs: () => 0,
    };
    try {
        const ing = await ingestPipeline(null, {
            profile_id,
            session_id: 'smoke',
            messages: [
                { role: 'user', content: 'I prefer pnpm over npm.' },
                { role: 'assistant', content: 'Got it — pnpm noted.' },
            ],
        }, deps);

        const rec = await retrievePipeline(null, {
            profile_id,
            query: 'which package manager?',
        }, deps);

        const env = envelope();
        res.status(200).json({
            ok: true,
            profile_id,
            ingest: {
                stored: ing.stored, deduped: ing.deduped,
                extracted: ing.extracted, dropped: ing.dropped,
            },
            retrieve: {
                citations: rec.citations,
                confidence: rec.confidence,
                result_csl: rec.result_csl,
            },
            served_by: env.served_by, ts: env.ts,
        });
    } catch (e) {
        const env = envelope();
        const msg = e instanceof Error ? e.message : String(e);
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.smoke.fail', err: msg }));
        res.status(502).json({ error: msg, served_by: env.served_by, ts: env.ts });
    }
}

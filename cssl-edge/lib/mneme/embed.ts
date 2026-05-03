// cssl-edge/lib/mneme/embed.ts
// MNEME — Voyage AI embedding client (voyage-3-large, 1024-dim).
//
// Spec : ../../specs/43_MNEME.csl § MODEL-CHOICES.embedding
//
// USAGE
//   const vec = await embedDocument(textJoined);     // input_type=document
//   const vec = await embedQuery(queryString);       // input_type=query
//
// Both return Float32Array(1024). On timeout (>10s) throws MnemeError("EMBED_TIMEOUT").
//
// CONFIG
//   VOYAGE_API_KEY         must be set
//   MNEME_VOYAGE_TIMEOUT   default 10000 (ms)

import { MnemeError } from './types';

const VOYAGE_URL = 'https://api.voyageai.com/v1/embeddings';
const MODEL      = 'voyage-3-large';
const DIM        = 1024;

function timeoutMs(): number {
    const v = parseInt(process.env['MNEME_VOYAGE_TIMEOUT'] ?? '10000', 10);
    return Number.isFinite(v) && v > 0 ? v : 10000;
}

function apiKey(): string {
    const k = process.env['VOYAGE_API_KEY'];
    if (!k) throw new MnemeError('NO_VOYAGE_KEY', 'VOYAGE_API_KEY not set', 500);
    return k;
}

interface VoyageResponse {
    data:  Array<{ embedding: number[]; index: number }>;
    model: string;
    usage: { total_tokens: number };
}

async function embedRaw(input: string, kind: 'query' | 'document'): Promise<Float32Array> {
    const ctl = new AbortController();
    const t   = setTimeout(() => ctl.abort(), timeoutMs());
    try {
        const r = await fetch(VOYAGE_URL, {
            method: 'POST',
            headers: {
                'Authorization': `Bearer ${apiKey()}`,
                'Content-Type':  'application/json',
            },
            body: JSON.stringify({
                model:      MODEL,
                input:      [input],
                input_type: kind,
            }),
            signal: ctl.signal,
        });
        if (!r.ok) {
            const msg = await r.text().catch(() => '');
            throw new MnemeError('VOYAGE_HTTP', `voyage ${r.status}: ${msg.slice(0, 256)}`, 502);
        }
        const j = await r.json() as VoyageResponse;
        const arr = j.data[0]?.embedding;
        if (!arr || arr.length !== DIM) {
            throw new MnemeError('VOYAGE_DIM',
                `voyage returned dim ${arr?.length} (expected ${DIM})`, 502);
        }
        return new Float32Array(arr);
    } catch (e: unknown) {
        if (e instanceof MnemeError) throw e;
        if (e instanceof Error && e.name === 'AbortError') {
            throw new MnemeError('EMBED_TIMEOUT', `voyage timed out (>${timeoutMs()}ms)`, 504);
        }
        throw new MnemeError('VOYAGE_FAIL', String(e), 502);
    } finally {
        clearTimeout(t);
    }
}

export async function embedDocument(text: string): Promise<Float32Array> {
    return embedRaw(text, 'document');
}

export async function embedQuery(text: string): Promise<Float32Array> {
    return embedRaw(text, 'query');
}

// Format a Float32Array as the Postgres pgvector literal `[a,b,c,...]`.
// Used when building parameterised queries with vector args.
export function toPgVectorLiteral(v: Float32Array): string {
    const parts: string[] = new Array(v.length);
    for (let i = 0; i < v.length; i++) {
        // Up to 7 sig figs is plenty for f32 cosine similarity.
        parts[i] = v[i]!.toFixed(7).replace(/0+$/, '').replace(/\.$/, '');
    }
    return '[' + parts.join(',') + ']';
}

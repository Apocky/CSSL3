// cssl-edge/lib/mneme/prompts/extract-detail.ts
// MNEME — extraction PASS-B (detail-pass) over short message windows.
//
// Spec : ../../../specs/44_MNEME_PIPELINES.csl § INGEST § STAGE-2 PASS-B
// v1 · 2026-05-02 · initial
//
// Goal : harvest concrete-values that PASS-A smooths over (version numbers,
//        prices, proper nouns, API routes, file paths, exact timestamps).

import { callTool, MODEL_HAIKU } from '../anthropic';
import type { ExtractedCandidate } from '../types';

export const EXTRACT_DETAIL_VERSION = 'v1.2026-05-02';

const WINDOW_SIZE    = 5;     // 5-msg windows
const WINDOW_OVERLAP = 2;
const MIN_CONVERSATION_MSGS = 9;
const MAX_CONCURRENT = 4;

interface ToolOutput {
    extracted: Array<{
        csl:        string;
        paraphrase: string;
        span:       [number, number];
    }>;
}

const TOOL_DEF = {
    name:        'mneme_extract_detail',
    description: 'Emit memories that capture exact concrete values from this window.',
    input_schema: {
        type: 'object',
        properties: {
            extracted: {
                type: 'array',
                items: {
                    type: 'object',
                    properties: {
                        csl:        { type: 'string', minLength: 1, maxLength: 4096 },
                        paraphrase: { type: 'string', minLength: 1, maxLength: 1024 },
                        span: {
                            type: 'array',
                            minItems: 2,
                            maxItems: 2,
                            items: { type: 'integer', minimum: 0 },
                        },
                    },
                    required: ['csl', 'paraphrase', 'span'],
                    additionalProperties: false,
                },
            },
        },
        required: ['extracted'],
        additionalProperties: false,
    },
};

const SYSTEM = `You are running PASS-B (detail) of a 2-pass memory extractor.
PASS-A already extracted broad facts. Your job is to harvest the EXACT
concrete values that PASS-A would smooth over.

EMPHASIZE
  - Version numbers : "v3.18.2"  →  proj.foo ⊗ ver.3-18-2
  - Prices :          "$12.99"    →  product.bar ⊗ price.usd-12-99
  - Proper nouns :    "Apocky"     →  person.apocky (only if novel)
  - API routes :      "POST /api/x" →  api.x ⊗ method.post
  - File paths :      "src/y.ts"  →  file.src-y-ts ⊗ kind.module
  - Exact dates :     "April 30 2026" → 't2026-04-30
  - Identifiers :     UUIDs, hashes, IDs

DO NOT
  - Repeat broad preferences PASS-A already would have caught.
  - Extract from speculation ("maybe v4.0 next month").
  - Generate prose-y CSL — keep it tight, kebab-case morpheme paths.

Use the mneme_extract_detail tool. Empty extracted: [] is preferred over
hallucinated detail.`;

export function shouldRunDetail(messageCount: number): boolean {
    return messageCount >= MIN_CONVERSATION_MSGS;
}

// Build overlapping windows of WINDOW_SIZE messages each.
export function buildWindows(
    messages: Array<{ role: string; content: string }>,
    size = WINDOW_SIZE,
    overlap = WINDOW_OVERLAP,
): string[] {
    if (messages.length === 0) return [];
    const fmt = (m: { role: string; content: string }, i: number) =>
        `[${i}] ${m.role}: ${m.content}`;
    const out: string[] = [];
    const step = Math.max(1, size - overlap);
    for (let i = 0; i < messages.length; i += step) {
        const slice = messages.slice(i, i + size);
        if (slice.length === 0) break;
        out.push(slice.map((m, j) => fmt(m, i + j)).join('\n'));
        if (i + size >= messages.length) break;
    }
    return out;
}

async function mapWithConcurrency<I, O>(
    items: I[], limit: number, fn: (i: I, n: number) => Promise<O>,
): Promise<O[]> {
    const out: O[] = new Array(items.length);
    let next = 0;
    async function worker(): Promise<void> {
        while (true) {
            const i = next++;
            if (i >= items.length) return;
            out[i] = await fn(items[i]!, i);
        }
    }
    const workers = new Array(Math.min(limit, items.length)).fill(0).map(() => worker());
    await Promise.all(workers);
    return out;
}

export interface ExtractDetailDeps {
    callTool?: typeof callTool;
}

export async function extractDetail(
    messages: Array<{ role: string; content: string }>,
    deps: ExtractDetailDeps = {},
): Promise<ExtractedCandidate[]> {
    if (!shouldRunDetail(messages.length)) return [];
    const windows = buildWindows(messages);
    const tool = deps.callTool ?? callTool;
    const buckets = await mapWithConcurrency(windows, MAX_CONCURRENT, async win => {
        try {
            return await tool<ToolOutput>({
                model:       MODEL_HAIKU,
                system:      SYSTEM,
                user:        win,
                tool:        TOOL_DEF,
                maxTokens:   2048,
                temperature: 0,
            });
        } catch (e) {
            // eslint-disable-next-line no-console
            console.error(JSON.stringify({
                evt: 'mneme.extract-detail.fail',
                err: e instanceof Error ? e.message : String(e),
            }));
            return { extracted: [] };
        }
    });
    const out: ExtractedCandidate[] = [];
    for (const b of buckets) {
        for (const c of b.extracted) {
            out.push({
                csl:        String(c.csl ?? '').trim(),
                paraphrase: String(c.paraphrase ?? '').trim(),
                span:       (c.span ?? [-1, -1]) as [number, number],
                pass:       'detail',
            });
        }
    }
    return out;
}

// cssl-edge/lib/mneme/prompts/extract-full.ts
// MNEME — extraction PASS-A (full-pass) over conversation chunks.
//
// Spec : ../../../specs/44_MNEME_PIPELINES.csl § INGEST § STAGE-2 PASS-A
// v1 · 2026-05-02 · initial

import { callTool, MODEL_HAIKU } from '../anthropic';
import type { ExtractedCandidate } from '../types';

export const EXTRACT_FULL_VERSION = 'v1.2026-05-02';

const CHUNK_CHARS  = 10_000;
const CHUNK_OVERLAP = 2;       // messages of overlap between chunks
const MAX_CONCURRENT = 4;

interface ToolOutput {
    extracted: Array<{
        csl:        string;
        paraphrase: string;
        span:       [number, number];
    }>;
}

const TOOL_DEF = {
    name:        'mneme_extract',
    description: 'Emit candidate memories extracted from the conversation chunk.',
    input_schema: {
        type: 'object',
        properties: {
            extracted: {
                type: 'array',
                description:
                    'Memory candidates. Each item must have a CSLv3 canonical form, ' +
                    'an English paraphrase, and the [line_start, line_end] span ' +
                    '(0-indexed, inclusive) within the chunk.',
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

const SYSTEM = `Extract candidate memories from the conversation chunk below.
A memory is a stable, durable, factually-grounded statement that an agent
should remember beyond this session — a preference, an instruction, an
event with a date, or a piece of project state.

GOOD CANDIDATES
  - User preferences ("I prefer pnpm over npm.")
  - Stable facts about the project ("The repo lives at github.com/Apocky/CSSL3.")
  - Standing instructions ("Run typecheck before every commit.")
  - Concrete events with timestamps ("Deployed feature X to prod 2026-04-30.")

BAD CANDIDATES (skip these)
  - Greetings and chatter ("Hey, how's it going?")
  - Code snippets, error logs, raw outputs
  - Speculation, hedging, or unverified guesses
  - Things that are clearly only relevant to this single message

OUTPUT RULES
  - For every candidate, emit BOTH csl (canonical form) and paraphrase
    (natural-language single sentence). NEVER paraphrase-only.
  - Be conservative. Empty extracted: [] is acceptable when the chunk has
    no durable memories. Quality > quantity.
  - The span [s, e] indicates which lines of the input chunk grounded the
    candidate (0-indexed, inclusive). Use [-1,-1] only for synthesis.

Use the mneme_extract tool. Produce no other output.`;

// Split a long string into roughly CHUNK_CHARS chunks ending on message
// boundaries. We include CHUNK_OVERLAP messages from the prior chunk as
// context so cross-message claims are not lost.
export function chunkConversation(
    messages: Array<{ role: string; content: string }>,
    chunkChars = CHUNK_CHARS,
    overlap   = CHUNK_OVERLAP,
): string[] {
    const fmt = (m: { role: string; content: string }, i: number) =>
        `[${i}] ${m.role}: ${m.content}`;
    const lines = messages.map(fmt);

    const out: string[] = [];
    let i = 0;
    while (i < lines.length) {
        let j = i;
        let charSum = 0;
        while (j < lines.length && charSum + (lines[j]!.length + 1) <= chunkChars) {
            charSum += lines[j]!.length + 1;
            j++;
        }
        if (j === i) {
            // Single message exceeds chunk size — emit alone.
            j = i + 1;
        }
        out.push(lines.slice(i, j).join('\n'));
        if (j >= lines.length) break;
        i = Math.max(j - overlap, i + 1);
    }
    return out;
}

// Run extraction over an array of chunks with bounded concurrency.
async function mapWithConcurrency<I, O>(
    items: I[],
    limit: number,
    fn: (item: I, idx: number) => Promise<O>,
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

export interface ExtractFullDeps {
    callTool?: typeof callTool;     // injectable for tests
}

export async function extractFull(
    messages: Array<{ role: string; content: string }>,
    deps: ExtractFullDeps = {},
): Promise<ExtractedCandidate[]> {
    if (messages.length === 0) return [];
    const chunks = chunkConversation(messages);
    const tool   = deps.callTool ?? callTool;
    const buckets = await mapWithConcurrency(chunks, MAX_CONCURRENT, async chunk => {
        try {
            return await tool<ToolOutput>({
                model:        MODEL_HAIKU,
                system:       SYSTEM,
                user:         chunk,
                tool:         TOOL_DEF,
                maxTokens:    4096,
                temperature:  0,
            });
        } catch (e) {
            // eslint-disable-next-line no-console
            console.error(JSON.stringify({
                evt: 'mneme.extract-full.fail',
                err: e instanceof Error ? e.message : String(e),
            }));
            return { extracted: [] };
        }
    });
    const flat: ExtractedCandidate[] = [];
    for (const out of buckets) {
        for (const c of out.extracted) {
            flat.push({
                csl:        String(c.csl ?? '').trim(),
                paraphrase: String(c.paraphrase ?? '').trim(),
                span:       (c.span ?? [-1, -1]) as [number, number],
                pass:       'full',
            });
        }
    }
    return flat;
}

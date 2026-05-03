// cssl-edge/lib/mneme/prompts/query-analyze.ts
// MNEME — query analyzer (topic_keys + fts_terms + HyDE).
//
// Spec : ../../../specs/44_MNEME_PIPELINES.csl § RETRIEVE § STAGE-1
// v1 · 2026-05-02 · initial

import { callTool, MODEL_HAIKU } from '../anthropic';
import type { QueryAnalysis } from '../types';

export const QUERY_ANALYZE_VERSION = 'v1.2026-05-02';

interface ToolOutput {
    topic_keys:      string[];
    fts_terms:       string[];
    hyde_csl:        string;
    hyde_paraphrase: string;
    is_temporal:     boolean;
}

const TOOL_DEF = {
    name:        'mneme_query_analyze',
    description: 'Decompose the user query into retrieval signals.',
    input_schema: {
        type: 'object',
        properties: {
            topic_keys: {
                type: 'array', items: { type: 'string', minLength: 1, maxLength: 256 },
                description: 'Ranked guesses at the morpheme path the answer lives under.',
                maxItems: 6,
            },
            fts_terms: {
                type: 'array', items: { type: 'string', minLength: 1, maxLength: 80 },
                description: 'Keywords + synonyms for FTS. Strip stopwords.',
                maxItems: 12,
            },
            hyde_csl: { type: 'string', maxLength: 1024 },
            hyde_paraphrase: { type: 'string', maxLength: 1024 },
            is_temporal: { type: 'boolean' },
        },
        required: ['topic_keys', 'fts_terms', 'hyde_csl', 'hyde_paraphrase', 'is_temporal'],
        additionalProperties: false,
    },
};

const SYSTEM = `Decompose a memory-recall query into retrieval signals.

OUTPUT FIELDS
  topic_keys      — Ranked best-guesses at the morpheme path that an answer
                    lives under. Format: dotted-lowercase-kebab.
                    Example for "what package manager?" :
                      ["user.pref.pkg-mgr", "config.npm", "build.tooling"]
  fts_terms       — Keywords + synonyms suitable for full-text search.
                    Strip articles. Add common variants.
                    Example: "package manager" → ["package", "manager", "pkg",
                                                  "pnpm", "npm", "yarn"]
  hyde_csl        — A HYPOTHETICAL canonical-CSL answer to the query, even
                    if the actual answer is unknown. This becomes the embedding
                    target for vector similarity. KEEP IT TIGHT, < 200 chars.
                    Example: "user.pref.pkg-mgr ⊗ pnpm"
  hyde_paraphrase — A HYPOTHETICAL English answer (1 sentence).
                    Example: "The user prefers pnpm over other package managers."
  is_temporal     — true if the query mentions any time reference
                    ("yesterday", "last week", "on 2026-04-30", "N days ago").

Use the mneme_query_analyze tool. NO other output.`;

const TEMPORAL_HINT = /\b(yesterday|today|tomorrow|last\s+\w+|next\s+\w+|\d+\s+(days?|weeks?|months?|years?)\s+ago|on\s+\d{4}-\d{2}-\d{2}|when|how\s+long)\b/i;

export interface QueryAnalyzeDeps {
    callTool?: typeof callTool;
}

export async function analyzeQuery(
    query: string,
    deps: QueryAnalyzeDeps = {},
): Promise<QueryAnalysis> {
    const tool = deps.callTool ?? callTool;
    let out: ToolOutput;
    try {
        out = await tool<ToolOutput>({
            model:       MODEL_HAIKU,
            system:      SYSTEM,
            user:        query,
            tool:        TOOL_DEF,
            maxTokens:   1024,
            temperature: 0,
        });
    } catch (e) {
        // Local fallback : at least surface the query as a single FTS term so
        // retrieval can still proceed via vec_direct + fts_paraphrase.
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({
            evt: 'mneme.query-analyze.fail',
            err: e instanceof Error ? e.message : String(e),
        }));
        return {
            topic_keys:      [],
            fts_terms:       extractFallbackTerms(query),
            hyde_csl:        '',
            hyde_paraphrase: query,
            is_temporal:     TEMPORAL_HINT.test(query),
        };
    }
    return {
        topic_keys:      Array.isArray(out.topic_keys) ? out.topic_keys.slice(0, 6) : [],
        fts_terms:       Array.isArray(out.fts_terms)  ? out.fts_terms.slice(0, 12) : [],
        hyde_csl:        String(out.hyde_csl ?? '').trim(),
        hyde_paraphrase: String(out.hyde_paraphrase ?? '').trim(),
        is_temporal:     out.is_temporal === true || TEMPORAL_HINT.test(query),
    };
}

const STOPWORDS = new Set([
    'the','a','an','of','on','in','to','for','with','by','and','or','but',
    'is','are','was','were','be','been','being','do','does','did','have',
    'has','had','this','that','these','those','i','you','he','she','it','we',
    'they','what','which','who','whom','whose','when','where','why','how',
]);

function extractFallbackTerms(q: string): string[] {
    return q
        .toLowerCase()
        .replace(/[^a-z0-9_\s\-]/g, ' ')
        .split(/\s+/)
        .filter(t => t.length >= 2 && !STOPWORDS.has(t))
        .slice(0, 8);
}

// cssl-edge/lib/mneme/prompts/classify.ts
// MNEME — type + topic_key + search_queries classifier.
//
// Spec : ../../../specs/44_MNEME_PIPELINES.csl § INGEST § STAGE-4
// v1 · 2026-05-02 · initial

import { callTool, MODEL_HAIKU } from '../anthropic';
import type {
    ClassifiedCandidate,
    MemoryType,
    VerifiedCandidate,
} from '../types';

export const CLASSIFY_VERSION = 'v1.2026-05-02';

interface ToolOutput {
    type:           MemoryType;
    topic_key:      string | null;
    search_queries: string[];
}

const TOOL_DEF = {
    name:        'mneme_classify',
    description: 'Classify a verified memory candidate. Produce type, topic_key, and 3-5 search queries.',
    input_schema: {
        type: 'object',
        properties: {
            type:      { type: 'string', enum: ['fact', 'event', 'instruction', 'task'] },
            topic_key: { type: ['string', 'null'], maxLength: 256 },
            search_queries: {
                type: 'array',
                minItems: 3,
                maxItems: 5,
                items: { type: 'string', minLength: 4, maxLength: 200 },
            },
        },
        required: ['type', 'topic_key', 'search_queries'],
        additionalProperties: false,
    },
};

const SYSTEM = `Classify a verified memory candidate.

TYPES
  fact         — true-now, stable, keyed.
                 Examples: "user.pref.pkg-mgr ⊗ pnpm",  "user.theme = dark"
  event        — happened-at-time, timestamped.
                 Examples: "deploy.prod 't2026-04-30 ✓"
  instruction  — how-to, keyed.
                 Examples: "flow.compact-context - { tail-sum → ingest → drop }"
  task         — in-flight, ephemeral, NOT keyed.
                 Examples: "spec.mneme-v1 ⊗ status.draft I> review.user"

TOPIC KEY RULES
  - REQUIRED for fact + instruction.
  - MUST be null for event + task.
  - Format: dotted lowercase morpheme path. ex: "user.pref.pkg-mgr"
  - NOT: "User Preferences > Package Manager", "user_pref_pkgmgr",
         "user/pref/pkg-mgr", or any English-leaning form.
  - Prefer head-morpheme of the CSL string verbatim when valid.

SEARCH QUERIES (3-5)
  - Phrased as questions in everyday English.
  - VARY vocabulary so an embedding model has multiple semantic anchors.
  - Example for "user.pref.pkg-mgr ⊗ pnpm" :
      ["which package manager?",
       "pnpm or npm?",
       "what JS package tool does the user prefer?"]

Use the mneme_classify tool. NO other output.`;

export interface ClassifyDeps {
    callTool?: typeof callTool;
}

export async function classifyCandidate(
    cand: VerifiedCandidate,
    deps: ClassifyDeps = {},
): Promise<ClassifiedCandidate | null> {
    if (cand.verdict === 'dropped') return null;

    const user =
        `CANDIDATE\n` +
        `  csl        : ${cand.csl}\n` +
        `  paraphrase : ${cand.paraphrase}\n`;

    const tool = deps.callTool ?? callTool;
    let out: ToolOutput;
    try {
        out = await tool<ToolOutput>({
            model:       MODEL_HAIKU,
            system:      SYSTEM,
            user,
            tool:        TOOL_DEF,
            maxTokens:   1024,
            temperature: 0,
        });
    } catch (e) {
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({
            evt: 'mneme.classify.fail',
            err: e instanceof Error ? e.message : String(e),
        }));
        return null;
    }

    // Defensive normalisation
    let topic_key: string | null = out.topic_key;
    if (out.type === 'event' || out.type === 'task') {
        topic_key = null;
    }
    const search_queries = Array.isArray(out.search_queries)
        ? out.search_queries.map(q => String(q).trim()).filter(q => q.length > 0).slice(0, 5)
        : [];

    return {
        ...cand,
        type: out.type,
        topic_key,
        search_queries,
    };
}

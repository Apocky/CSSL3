// cssl-edge/lib/mneme/prompts/synthesize.ts
// MNEME — synthesis (Sonnet) over top-k memories.
//
// Spec : ../../../specs/44_MNEME_PIPELINES.csl § RETRIEVE § STAGE-6
// v1 · 2026-05-02 · initial

import { callTool, MODEL_SONNET } from '../anthropic';
import type { Memory } from '../types';
import { validateCsl } from '../csl';

export const SYNTHESIZE_VERSION = 'v1.2026-05-02';

interface ToolOutput {
    result_nl:  string;
    result_csl: string;
    citations:  string[];
    confidence: number;
}

const TOOL_DEF = {
    name:        'mneme_synthesize',
    description: 'Answer the query using ONLY the supplied memories. Cite by id.',
    input_schema: {
        type: 'object',
        properties: {
            result_nl:  { type: 'string', minLength: 1, maxLength: 4096 },
            result_csl: { type: 'string', minLength: 1, maxLength: 4096 },
            citations:  {
                type: 'array',
                items: { type: 'string' },
                description: 'memory_id values from the input list that grounded the answer.',
            },
            confidence: { type: 'number', minimum: 0, maximum: 1 },
        },
        required: ['result_nl', 'result_csl', 'citations', 'confidence'],
        additionalProperties: false,
    },
};

const SYSTEM = `Answer the user's query using ONLY the supplied memories.

RULES
  - NEVER fabricate. If the memories are insufficient, say so plainly in
    result_nl, set result_csl to "memory ⊗ ∅ @ query='<query>'", set
    confidence < 0.3, and citations to [].
  - Cite ONLY by memory_id from the input list. NO external citations.
  - result_nl is plain English, 1-3 sentences. NO bullet lists unless
    the memories themselves are list-shaped.
  - result_csl is canonical CSLv3 — must validate against our subset.
  - confidence is your honest estimate. Bias toward humility.
  - When pre-computed temporal facts are provided, USE them; do not redo
    date arithmetic yourself.

INPUT FORMAT
  The user message contains:
    - The query (line 1).
    - PRE_COMPUTED facts (if any).
    - The memories formatted as: id | type | csl | paraphrase

OUTPUT
  Use the mneme_synthesize tool. NO other output.`;

export interface SynthesizeInput {
    query:           string;
    memories:        Memory[];
    pre_computed?:   string[];     // injected temporal facts, etc.
}

export interface SynthesizeDeps {
    callTool?: typeof callTool;
}

export async function synthesize(
    input: SynthesizeInput,
    deps: SynthesizeDeps = {},
): Promise<ToolOutput> {
    const lines: string[] = [];
    lines.push(`QUERY: ${input.query}`);
    if (input.pre_computed && input.pre_computed.length > 0) {
        lines.push('PRE_COMPUTED:');
        for (const p of input.pre_computed) lines.push(`  - ${p}`);
    }
    lines.push('MEMORIES:');
    if (input.memories.length === 0) {
        lines.push('  (no memories matched)');
    } else {
        for (const m of input.memories) {
            lines.push(
                `  ${m.id} | ${m.type} | ${m.csl.replace(/\s+/g, ' ').slice(0, 400)} | ${m.paraphrase.slice(0, 240)}`,
            );
        }
    }

    const tool = deps.callTool ?? callTool;
    let out: ToolOutput;
    try {
        out = await tool<ToolOutput>({
            model:       MODEL_SONNET,
            system:      SYSTEM,
            user:        lines.join('\n'),
            tool:        TOOL_DEF,
            maxTokens:   2048,
            temperature: 0.3,
        });
    } catch (e) {
        // Conservative fallback : zero-confidence "I don't know" with no citations.
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({
            evt: 'mneme.synth.fail',
            err: e instanceof Error ? e.message : String(e),
        }));
        return {
            result_nl:  "I don't have a memory about that yet.",
            result_csl: `memory ⊗ ∅ @ query='${input.query.replace(/'/g, '')}'`,
            citations:  [],
            confidence: 0,
        };
    }

    // Validate result_csl. If invalid, fall back to a minimal canonical form.
    const v = validateCsl(out.result_csl);
    if (!v.ok) {
        out.result_csl = `synth.fallback ⊗ confidence.${Math.round((out.confidence ?? 0) * 100)}`;
    }

    // Confidence floor : if no citations, cap confidence at 0.3.
    if ((out.citations ?? []).length === 0 && (out.confidence ?? 0) > 0.3) {
        out.confidence = 0.3;
    }
    return {
        result_nl:  String(out.result_nl  ?? '').trim(),
        result_csl: String(out.result_csl ?? '').trim(),
        citations:  Array.isArray(out.citations) ? out.citations : [],
        confidence: Math.max(0, Math.min(1, Number(out.confidence ?? 0))),
    };
}

// cssl-edge/lib/mneme/prompts/verify.ts
// MNEME — 8-check verifier for extracted candidates.
//
// Spec : ../../../specs/44_MNEME_PIPELINES.csl § INGEST § STAGE-3
// v1 · 2026-05-02 · initial

import { callTool, MODEL_HAIKU } from '../anthropic';
import type { ExtractedCandidate, VerifiedCandidate } from '../types';

export const VERIFY_VERSION = 'v1.2026-05-02';

interface ToolOutput {
    verdict:      'pass' | 'corrected' | 'dropped';
    csl_fixed?:   string;
    paraphrase_fixed?: string;
    drop_reason?: string;
    failed_checks?: string[];
}

const TOOL_DEF = {
    name:        'mneme_verify',
    description: 'Verify a candidate memory against its source span. Eight checks.',
    input_schema: {
        type: 'object',
        properties: {
            verdict: {
                type: 'string',
                enum: ['pass', 'corrected', 'dropped'],
            },
            csl_fixed:        { type: 'string', maxLength: 4096 },
            paraphrase_fixed: { type: 'string', maxLength: 1024 },
            drop_reason:      { type: 'string', maxLength: 256 },
            failed_checks: {
                type: 'array',
                items: {
                    type: 'string',
                    enum: [
                        'entity_identity',
                        'object_identity',
                        'location_context',
                        'temporal_accuracy',
                        'organizational_context',
                        'completeness',
                        'relational_context',
                        'support_in_source',
                    ],
                },
            },
        },
        required: ['verdict'],
        additionalProperties: false,
    },
};

const SYSTEM = `Verify a single candidate memory against the source transcript.
Apply these 8 checks. Mark each PASS / FAIL silently then aggregate.

CHECKS
 1. entity_identity        — "user" vs "the user's friend" — distinct entities?
 2. object_identity        — "the project" vs "another project" — same referent?
 3. location_context       — claim bound to right place / scope?
 4. temporal_accuracy      — relative dates resolved to absolute? "yesterday" → 't<YYYY-MM-DD>?
 5. organizational_context — right team, repo, company?
 6. completeness           — claim is whole, not truncated mid-thought?
 7. relational_context     — if claim is "A → B", are A and B distinct + correct?
 8. support_in_source      — claim is literal in source OR is a direct
                             implication. NO speculation.

VERDICT RULES
  pass       — all 8 checks succeed unmodified.
  corrected  — at most one check failed AND a small fix to csl/paraphrase
               makes it pass. Emit csl_fixed + paraphrase_fixed.
  dropped    — any other case. Emit drop_reason (≤ 80 chars).

Use the mneme_verify tool. NO other output.`;

export interface VerifyDeps {
    callTool?: typeof callTool;
}

export async function verifyCandidate(
    cand: ExtractedCandidate,
    sourceTranscript: string,
    deps: VerifyDeps = {},
): Promise<VerifiedCandidate> {
    const user =
        `CANDIDATE\n` +
        `  csl        : ${cand.csl}\n` +
        `  paraphrase : ${cand.paraphrase}\n` +
        `  span       : [${cand.span[0]}, ${cand.span[1]}]\n` +
        `\nSOURCE\n${sourceTranscript}\n`;

    const tool = deps.callTool ?? callTool;
    let out: ToolOutput;
    try {
        out = await tool<ToolOutput>({
            model:       MODEL_HAIKU,
            system:      SYSTEM,
            user,
            tool:        TOOL_DEF,
            maxTokens:   2048,
            temperature: 0,
        });
    } catch (e) {
        // On verifier failure, drop conservatively (do not poison the index).
        return {
            ...cand,
            verdict:    'dropped',
            drop_reason: 'verifier_call_failed: ' +
                (e instanceof Error ? e.message : String(e)).slice(0, 80),
        };
    }

    if (out.verdict === 'pass') {
        return { ...cand, verdict: 'pass' };
    }
    if (out.verdict === 'corrected') {
        return {
            ...cand,
            csl:        (out.csl_fixed ?? cand.csl).trim(),
            paraphrase: (out.paraphrase_fixed ?? cand.paraphrase).trim(),
            verdict:    'corrected',
        };
    }
    return {
        ...cand,
        verdict:     'dropped',
        drop_reason: out.drop_reason ?? 'unspecified',
    };
}

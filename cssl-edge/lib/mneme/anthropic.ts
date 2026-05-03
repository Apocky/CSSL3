// cssl-edge/lib/mneme/anthropic.ts
// MNEME — Anthropic API client (Haiku 4.5 + Sonnet 4.6) with prompt-caching.
//
// Spec : ../../specs/43_MNEME.csl § MODEL-CHOICES + 44_MNEME_PIPELINES.csl § PROMPTS
//
// All structured outputs use tool-use (forced tool call) for schema fidelity.
// CSLv3 onboarding doc is sent as a cached system message on every call.

import { MnemeError } from './types';
import { CSL_SYSTEM_PRELUDE } from './prompts/system-prelude';

const API_URL  = 'https://api.anthropic.com/v1/messages';
const VERSION  = '2023-06-01';

export const MODEL_HAIKU  = 'claude-haiku-4-5';
export const MODEL_SONNET = 'claude-sonnet-4-6';

function apiKey(): string {
    const k = process.env['ANTHROPIC_API_KEY'];
    if (!k) throw new MnemeError('NO_ANTHROPIC_KEY', 'ANTHROPIC_API_KEY not set', 500);
    return k;
}

interface ToolDef {
    name:        string;
    description: string;
    input_schema: object;
}

interface ContentBlock {
    type:  'text' | 'tool_use' | 'tool_result';
    text?: string;
    id?:   string;
    name?: string;
    input?: unknown;
}

interface MessagesResponse {
    id:      string;
    model:   string;
    role:    'assistant';
    content: ContentBlock[];
    stop_reason: string;
    usage:   { input_tokens: number; output_tokens: number; cache_read_input_tokens?: number };
}

export interface CallToolOptions {
    model:        string;
    system:       string;          // call-specific system addendum
    user:         string;          // user-message body
    tool:         ToolDef;
    maxTokens:    number;
    temperature?: number;          // default 0
}

// Call the Messages API forcing a single tool invocation, return parsed input.
// Throws MnemeError on transport / schema / parse failure.
export async function callTool<T>(opts: CallToolOptions): Promise<T> {
    const body = {
        model:       opts.model,
        max_tokens:  opts.maxTokens,
        temperature: opts.temperature ?? 0,
        // prompt-caching: prelude is cacheable; per-call system text trails it.
        system: [
            { type: 'text', text: CSL_SYSTEM_PRELUDE,
              cache_control: { type: 'ephemeral' } },
            { type: 'text', text: opts.system },
        ],
        messages: [
            { role: 'user', content: opts.user },
        ],
        tools: [opts.tool],
        tool_choice: { type: 'tool', name: opts.tool.name },
    };

    const r = await fetch(API_URL, {
        method: 'POST',
        headers: {
            'x-api-key':         apiKey(),
            'anthropic-version': VERSION,
            'Content-Type':      'application/json',
        },
        body: JSON.stringify(body),
    });

    if (!r.ok) {
        const msg = await r.text().catch(() => '');
        throw new MnemeError('ANTHROPIC_HTTP',
            `anthropic ${r.status}: ${msg.slice(0, 256)}`, 502);
    }
    const j = await r.json() as MessagesResponse;
    const block = j.content.find(c => c.type === 'tool_use' && c.name === opts.tool.name);
    if (!block || block.input === undefined) {
        throw new MnemeError('ANTHROPIC_NOTOOL',
            `expected tool_use ${opts.tool.name}, got ${j.stop_reason}`, 502);
    }
    return block.input as T;
}

// Plain text completion (no tool). Used for synthesis when we want both
// natural-language and CSL forms in a single response — we still use a tool
// to enforce shape, so this helper is rarely needed but kept for parity.
export async function callText(
    model: string,
    system: string,
    user: string,
    maxTokens: number,
    temperature = 0,
): Promise<string> {
    const body = {
        model, max_tokens: maxTokens, temperature,
        system: [
            { type: 'text', text: CSL_SYSTEM_PRELUDE,
              cache_control: { type: 'ephemeral' } },
            { type: 'text', text: system },
        ],
        messages: [{ role: 'user', content: user }],
    };
    const r = await fetch(API_URL, {
        method: 'POST',
        headers: {
            'x-api-key':         apiKey(),
            'anthropic-version': VERSION,
            'Content-Type':      'application/json',
        },
        body: JSON.stringify(body),
    });
    if (!r.ok) {
        const msg = await r.text().catch(() => '');
        throw new MnemeError('ANTHROPIC_HTTP',
            `anthropic ${r.status}: ${msg.slice(0, 256)}`, 502);
    }
    const j = await r.json() as MessagesResponse;
    const txt = j.content.filter(c => c.type === 'text').map(c => c.text ?? '').join('');
    return txt;
}

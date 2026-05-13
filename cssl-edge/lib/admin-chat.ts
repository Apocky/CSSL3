export type AdminChatRole = 'user' | 'assistant' | 'system';
export type AdminChatEnv = Record<string, string | undefined>;

export interface AdminChatMessage {
  role: AdminChatRole;
  content: string;
}

export interface AdminChatProviderStatus {
  ready: boolean;
  provider: 'deepseek' | 'anthropic' | 'none';
  model: string | null;
}

export interface AdminChatRequest {
  messages: AdminChatMessage[];
  env?: AdminChatEnv;
  fetchImpl?: typeof fetch;
}

export interface AdminChatResult {
  provider: 'deepseek' | 'anthropic';
  model: string;
  text: string;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
    total_tokens?: number;
  };
}

const DEEPSEEK_API_URL = 'https://api.deepseek.com/chat/completions';
const ANTHROPIC_API_URL = 'https://api.anthropic.com/v1/messages';
const DEFAULT_DEEPSEEK_MODEL = 'deepseek-reasoner';
const DEFAULT_ANTHROPIC_MODEL = 'claude-3-5-sonnet-20241022';

function present(value: string | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

export function resolveAdminChatProvider(env: AdminChatEnv = process.env): AdminChatProviderStatus {
  const deepseekKey = present(env.DEEPSEEK_API_KEY);
  if (deepseekKey) {
    return {
      ready: true,
      provider: 'deepseek',
      model: present(env.ADMIN_CHAT_DEEPSEEK_MODEL) ?? present(env.ADMIN_CHAT_MODEL) ?? DEFAULT_DEEPSEEK_MODEL,
    };
  }

  const anthropicKey = present(env.ANTHROPIC_API_KEY) ?? present(env.CLAUDE_API_KEY);
  if (anthropicKey) {
    return {
      ready: true,
      provider: 'anthropic',
      model: present(env.ADMIN_CHAT_ANTHROPIC_MODEL) ?? present(env.ADMIN_CHAT_MODEL) ?? DEFAULT_ANTHROPIC_MODEL,
    };
  }

  return { ready: false, provider: 'none', model: null };
}

export function sanitizeAdminChatMessages(messages: unknown): AdminChatMessage[] {
  if (!Array.isArray(messages)) return [];
  const clean: AdminChatMessage[] = [];
  for (const message of messages) {
    if (typeof message !== 'object' || message === null) continue;
    const record = message as Record<string, unknown>;
    const role = record.role;
    const content = record.content;
    if ((role !== 'user' && role !== 'assistant' && role !== 'system') || typeof content !== 'string') continue;
    const trimmed = content.trim();
    if (!trimmed) continue;
    clean.push({ role, content: trimmed.slice(0, 12_000) });
  }
  return clean.slice(-30);
}

export function buildAdminChatSystemPrompt(): string {
  return [
    'You are the Apocky private admin chat assistant.',
    'Help with game design, engineering, debugging, deployment, Lazarus operations, Tessera planning, and general reasoning from one unified chat surface.',
    'Answer directly, keep operational context visible, and ask for confirmation before destructive, expensive, or secret-touching actions.',
    'Never reveal credentials, access tokens, hidden environment values, or server-only configuration.',
    'When code or deployment work is requested, include concrete validation steps when useful.',
  ].join('\n');
}

function requireProvider(status: AdminChatProviderStatus): asserts status is AdminChatProviderStatus & { ready: true; model: string } {
  if (!status.ready || !status.model) {
    throw new Error('No admin chat model is configured. Set DEEPSEEK_API_KEY or ANTHROPIC_API_KEY on the server.');
  }
}

async function callDeepSeek(request: AdminChatRequest, status: AdminChatProviderStatus & { ready: true; model: string }): Promise<AdminChatResult> {
  const apiKey = present((request.env ?? process.env).DEEPSEEK_API_KEY);
  if (!apiKey) throw new Error('DEEPSEEK_API_KEY is not configured.');
  const fetcher = request.fetchImpl ?? fetch;
  const messages = sanitizeAdminChatMessages(request.messages);
  const response = await fetcher(DEEPSEEK_API_URL, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: status.model,
      messages: [
        { role: 'system', content: buildAdminChatSystemPrompt() },
        ...messages.map((message) => ({ role: message.role, content: message.content })),
      ],
      stream: false,
      temperature: 0.45,
      max_tokens: 1800,
    }),
  });
  const json = await response.json() as {
    choices?: Array<{ message?: { content?: string } }>;
    usage?: AdminChatResult['usage'];
    error?: { message?: string };
  };
  if (!response.ok) throw new Error(json.error?.message ?? `DeepSeek HTTP ${response.status}`);
  const text = json.choices?.[0]?.message?.content?.trim();
  if (!text) throw new Error('DeepSeek returned an empty response.');
  return { provider: 'deepseek', model: status.model, text, usage: json.usage };
}

async function callAnthropic(request: AdminChatRequest, status: AdminChatProviderStatus & { ready: true; model: string }): Promise<AdminChatResult> {
  const env = request.env ?? process.env;
  const apiKey = present(env.ANTHROPIC_API_KEY) ?? present(env.CLAUDE_API_KEY);
  if (!apiKey) throw new Error('ANTHROPIC_API_KEY is not configured.');
  const fetcher = request.fetchImpl ?? fetch;
  const messages = sanitizeAdminChatMessages(request.messages).filter((message) => message.role !== 'system');
  const response = await fetcher(ANTHROPIC_API_URL, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-api-key': apiKey,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify({
      model: status.model,
      system: buildAdminChatSystemPrompt(),
      max_tokens: 1800,
      temperature: 0.45,
      messages: messages.map((message) => ({ role: message.role, content: message.content })),
    }),
  });
  const json = await response.json() as {
    content?: Array<{ type?: string; text?: string }>;
    usage?: { input_tokens?: number; output_tokens?: number };
    error?: { message?: string };
  };
  if (!response.ok) throw new Error(json.error?.message ?? `Anthropic HTTP ${response.status}`);
  const text = json.content?.map((part) => part.text ?? '').join('').trim();
  if (!text) throw new Error('Anthropic returned an empty response.');
  return { provider: 'anthropic', model: status.model, text, usage: json.usage };
}

export async function callAdminChatModel(request: AdminChatRequest): Promise<AdminChatResult> {
  const status = resolveAdminChatProvider(request.env ?? process.env);
  requireProvider(status);
  if (status.provider === 'deepseek') return callDeepSeek(request, status);
  return callAnthropic(request, status);
}
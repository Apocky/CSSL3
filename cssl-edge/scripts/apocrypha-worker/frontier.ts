import type { QwenGenerationOptions, QwenMessage } from './qwen';
import type { QwenResult, QwenUsage, WorkerConfig } from './types';

type Fetch = typeof fetch;

export interface FrontierStatus {
  provider: 'openai' | 'anthropic' | null;
  model: string | null;
  configured: boolean;
  available: boolean;
  cooling_down_until: string | null;
  last_failure: { code: string; status: number | null; at: string } | null;
}

export class FrontierError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly status: number | null;

  constructor(message: string, code = 'FRONTIER_ERROR', retryable = true, status: number | null = null) {
    super(message);
    this.name = 'FrontierError';
    this.code = code;
    this.retryable = retryable;
    this.status = status;
  }
}

function numeric(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function parseUsage(raw: unknown): QwenUsage {
  if (!raw || typeof raw !== 'object') return {};
  const value = raw as Record<string, unknown>;
  const promptTokens = numeric(value.prompt_tokens ?? value.input_tokens);
  const completionTokens = numeric(value.completion_tokens ?? value.output_tokens);
  const totalTokens = numeric(value.total_tokens)
    ?? (promptTokens !== undefined && completionTokens !== undefined ? promptTokens + completionTokens : undefined);
  return { promptTokens, completionTokens, totalTokens };
}

function boundedErrorText(value: string): string {
  return value.replace(/[\r\n]+/g, ' ').slice(0, 1_000);
}

async function readBounded(response: Response, maxBytes = 2_000_000): Promise<string> {
  const declared = Number(response.headers.get('content-length'));
  if (Number.isFinite(declared) && declared > maxBytes) {
    throw new FrontierError('frontier response exceeded the bounded transport size', 'FRONTIER_TRANSPORT_LIMIT_EXCEEDED', false, response.status);
  }
  if (!response.body) return '';
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let total = 0;
  let text = '';
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel().catch(() => undefined);
      throw new FrontierError('frontier response exceeded the bounded transport size', 'FRONTIER_TRANSPORT_LIMIT_EXCEEDED', false, response.status);
    }
    text += decoder.decode(value, { stream: true });
  }
  return text + decoder.decode();
}

function endpoint(baseUrl: string, path: string): string {
  const base = baseUrl.replace(/\/+$/, '');
  if (base.endsWith('/v1') && path.startsWith('/v1/')) return `${base}${path.slice(3)}`;
  return `${base}${path}`;
}

function textContent(value: unknown): string {
  if (typeof value === 'string') return value;
  if (!Array.isArray(value)) return '';
  return value.flatMap((item): string[] => {
    if (!item || typeof item !== 'object') return [];
    const record = item as Record<string, unknown>;
    return typeof record.text === 'string' ? [record.text] : [];
  }).join('');
}

function openAiContent(payload: Record<string, unknown>): string {
  const choices = Array.isArray(payload.choices) ? payload.choices : [];
  const first = choices[0];
  if (!first || typeof first !== 'object') return '';
  const choice = first as Record<string, unknown>;
  const message = choice.message && typeof choice.message === 'object' ? choice.message as Record<string, unknown> : {};
  return textContent(message.content) || textContent(choice.text);
}

function anthropicContent(payload: Record<string, unknown>): string {
  return textContent(payload.content);
}

export class FrontierClient {
  private readonly config: WorkerConfig;
  private readonly fetchImpl: Fetch;
  private cooldownUntil = 0;
  private lastFailure: FrontierStatus['last_failure'] = null;

  constructor(config: WorkerConfig, fetchImpl: Fetch = fetch) {
    this.config = config;
    this.fetchImpl = fetchImpl;
  }

  status(): FrontierStatus {
    const configured = Boolean(
      this.config.frontierProvider
      && this.config.frontierBaseUrl
      && this.config.frontierApiKey
      && this.config.frontierModel,
    );
    const cooling = this.cooldownUntil > Date.now();
    return {
      provider: this.config.frontierProvider ?? null,
      model: this.config.frontierModel ?? null,
      configured,
      available: configured && !cooling,
      cooling_down_until: cooling ? new Date(this.cooldownUntil).toISOString() : null,
      last_failure: this.lastFailure,
    };
  }

  isAvailable(): boolean {
    return this.status().available;
  }

  async generate(messages: QwenMessage[], options: QwenGenerationOptions, signal?: AbortSignal): Promise<QwenResult> {
    const status = this.status();
    if (!status.configured) throw new FrontierError('frontier rail is not explicitly configured', 'FRONTIER_UNCONFIGURED', false);
    if (!status.available) throw new FrontierError('frontier rail is cooling down after a provider failure', 'FRONTIER_COOLDOWN', true);
    const provider = this.config.frontierProvider as 'openai' | 'anthropic';
    const baseUrl = this.config.frontierBaseUrl as string;
    const model = this.config.frontierModel as string;
    const apiKey = this.config.frontierApiKey as string;
    const started = Date.now();
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new FrontierError('frontier request timed out', 'FRONTIER_TIMEOUT', true)), this.config.frontierTimeoutMs ?? 90_000);
    const abort = () => controller.abort(signal?.reason ?? new FrontierError('frontier request cancelled', 'FRONTIER_CANCELLED', false));
    signal?.addEventListener('abort', abort, { once: true });
    try {
      const system = messages.filter((message) => message.role === 'system').map((message) => message.content).join('\n\n');
      const conversational = messages.filter((message) => message.role !== 'system');
      const body = provider === 'openai'
        ? {
            model,
            messages,
            stream: false,
            max_tokens: Math.max(64, Math.floor(options.maxTokens ?? this.config.maxOutputTokens)),
            ...(options.temperature === undefined ? {} : { temperature: options.temperature }),
            ...(options.topP === undefined ? {} : { top_p: options.topP }),
          }
        : {
            model,
            max_tokens: Math.max(64, Math.floor(options.maxTokens ?? this.config.maxOutputTokens)),
            ...(system ? { system } : {}),
            messages: conversational.length > 0 ? conversational : [{ role: 'user' as const, content: 'Respond to the request.' }],
            ...(options.temperature === undefined ? {} : { temperature: options.temperature }),
            ...(options.topP === undefined ? {} : { top_p: options.topP }),
          };
      const headers: Record<string, string> = { 'content-type': 'application/json', accept: 'application/json' };
      if (provider === 'openai') headers.authorization = `Bearer ${apiKey}`;
      else {
        headers['x-api-key'] = apiKey;
        headers['anthropic-version'] = '2023-06-01';
      }
      const response = await this.fetchImpl(endpoint(baseUrl, provider === 'openai' ? '/chat/completions' : '/v1/messages'), {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      const raw = await readBounded(response);
      if (!response.ok) {
        const retryable = response.status === 408 || response.status === 409 || response.status === 425 || response.status === 429 || response.status >= 500;
        const code = response.status === 429 ? 'FRONTIER_HTTP_429' : `FRONTIER_HTTP_${response.status}`;
        this.noteFailure(code, response.status, retryable);
        throw new FrontierError(`frontier HTTP ${response.status}: ${boundedErrorText(raw)}`, code, retryable, response.status);
      }
      let payload: Record<string, unknown>;
      try { payload = JSON.parse(raw) as Record<string, unknown>; }
      catch { throw new FrontierError('frontier returned malformed JSON', 'FRONTIER_MALFORMED_RESPONSE', true, response.status); }
      const content = provider === 'openai' ? openAiContent(payload) : anthropicContent(payload);
      if (!content.trim()) throw new FrontierError('frontier returned an empty answer', 'FRONTIER_EMPTY_RESPONSE', true, response.status);
      const maxOutputBytes = Math.max(4_096, (options.maxTokens ?? this.config.maxOutputTokens ?? 2_048) * 16);
      if (Buffer.byteLength(content, 'utf8') > maxOutputBytes) {
        throw new FrontierError('frontier exceeded the bounded output size', 'FRONTIER_OUTPUT_LIMIT_EXCEEDED', false, response.status);
      }
      const usage = parseUsage(payload.usage);
      return {
        content,
        usage,
        model: `${provider}:${model}`,
        firstTokenMs: Date.now() - started,
        durationMs: Date.now() - started,
      };
    } catch (error) {
      if (error instanceof FrontierError) {
        if (error.code !== 'FRONTIER_UNCONFIGURED' && error.code !== 'FRONTIER_COOLDOWN' && error.code !== 'FRONTIER_CANCELLED') {
          this.noteFailure(error.code, error.status, error.retryable);
        }
        throw error;
      }
      const detail = error instanceof Error ? error.message : String(error);
      this.noteFailure('FRONTIER_NETWORK_ERROR', null, true);
      throw new FrontierError(`frontier request failed: ${boundedErrorText(detail)}`, 'FRONTIER_NETWORK_ERROR', true);
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
    }
  }

  private noteFailure(code: string, status: number | null, retryable: boolean): void {
    this.lastFailure = { code, status, at: new Date().toISOString() };
    if (retryable) this.cooldownUntil = Date.now() + (this.config.frontierCooldownMs ?? 300_000);
  }
}

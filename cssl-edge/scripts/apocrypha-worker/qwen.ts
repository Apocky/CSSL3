import type { QwenResult, QwenUsage, WorkerConfig } from './types';

type Fetch = typeof fetch;

export interface QwenMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface QwenGenerationOptions {
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  topK?: number;
  minP?: number;
  repeatPenalty?: number;
  seed?: number;
}

export class QwenError extends Error {
  readonly code: string;
  readonly retryable: boolean;

  constructor(message: string, code = 'QWEN_ERROR', retryable = true) {
    super(message);
    this.name = 'QwenError';
    this.code = code;
    this.retryable = retryable;
  }
}

function numeric(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function parseUsage(raw: unknown): QwenUsage {
  if (!raw || typeof raw !== 'object') return {};
  const value = raw as Record<string, unknown>;
  return {
    promptTokens: numeric(value.prompt_tokens),
    completionTokens: numeric(value.completion_tokens),
    totalTokens: numeric(value.total_tokens),
  };
}

function extractContent(payload: unknown): string {
  if (!payload || typeof payload !== 'object') return '';
  const root = payload as Record<string, unknown>;
  const choices = Array.isArray(root.choices) ? root.choices : [];
  const choice = choices[0];
  if (!choice || typeof choice !== 'object') return '';
  const item = choice as Record<string, unknown>;
  const delta = item.delta && typeof item.delta === 'object' ? item.delta as Record<string, unknown> : {};
  const message = item.message && typeof item.message === 'object' ? item.message as Record<string, unknown> : {};
  return typeof delta.content === 'string'
    ? delta.content
    : typeof message.content === 'string'
      ? message.content
      : typeof item.text === 'string'
        ? item.text
        : '';
}

export class QwenClient {
  private readonly config: WorkerConfig;
  private readonly fetchImpl: Fetch;

  constructor(config: WorkerConfig, fetchImpl: Fetch = fetch) {
    this.config = config;
    this.fetchImpl = fetchImpl;
  }

  async probe(signal?: AbortSignal): Promise<{ healthy: boolean; model: string; detail: string }> {
    const base = this.config.qwenBaseUrl.replace(/\/v1$/, '');
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new Error('Qwen probe timeout')), 10_000);
    const abort = () => controller.abort(signal?.reason);
    signal?.addEventListener('abort', abort, { once: true });
    try {
      const [healthResponse, modelsResponse] = await Promise.all([
        this.fetchImpl(`${base}/health`, { signal: controller.signal }),
        this.fetchImpl(`${this.config.qwenBaseUrl}/models`, { signal: controller.signal }),
      ]);
      if (!healthResponse.ok || !modelsResponse.ok) {
        return { healthy: false, model: this.config.modelAlias, detail: `health=${healthResponse.status} models=${modelsResponse.status}` };
      }
      const models = await modelsResponse.json() as { data?: Array<{ id?: string }> };
      const aliases = (models.data ?? []).map((model) => model.id).filter(Boolean);
      const healthy = aliases.includes(this.config.modelAlias);
      return { healthy, model: this.config.modelAlias, detail: healthy ? 'ready' : `alias absent (${aliases.join(', ')})` };
    } catch (error) {
      return { healthy: false, model: this.config.modelAlias, detail: error instanceof Error ? error.message : 'probe failed' };
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
    }
  }

  async generate(
    messages: QwenMessage[],
    options: QwenGenerationOptions,
    onDelta: (delta: string) => Promise<void> | void,
    signal?: AbortSignal,
  ): Promise<QwenResult> {
    const started = Date.now();
    const controller = new AbortController();
    let callbackError: unknown;
    let idleTimer: NodeJS.Timeout | null = null;
    const maxRuntimeTimer = setTimeout(
      () => controller.abort(new QwenError('Qwen generation exceeded maximum runtime', 'QWEN_MAX_RUNTIME', true)),
      this.config.qwenMaxRuntimeMs,
    );
    const resetIdle = (): void => {
      if (idleTimer) clearTimeout(idleTimer);
      idleTimer = setTimeout(
        () => controller.abort(new QwenError('Qwen stream was idle too long', 'QWEN_IDLE_TIMEOUT', true)),
        this.config.qwenIdleTimeoutMs,
      );
    };
    const abort = () => controller.abort(signal?.reason ?? new QwenError('Qwen generation cancelled', 'QWEN_CANCELLED', false));
    signal?.addEventListener('abort', abort, { once: true });
    resetIdle();
    try {
      const body = {
        model: this.config.modelAlias,
        messages,
        stream: true,
        stream_options: { include_usage: true },
        max_tokens: Math.min(options.maxTokens ?? this.config.maxOutputTokens, this.config.maxOutputTokens),
        temperature: options.temperature ?? 0.65,
        top_p: options.topP ?? 0.9,
        top_k: options.topK ?? 40,
        min_p: options.minP ?? 0.05,
        repeat_penalty: options.repeatPenalty ?? 1.08,
        ...(options.seed === undefined ? {} : { seed: options.seed }),
        chat_template_kwargs: { enable_thinking: false },
      };
      const response = await this.fetchImpl(`${this.config.qwenBaseUrl}/chat/completions`, {
        method: 'POST',
        headers: { 'content-type': 'application/json', accept: 'text/event-stream, application/json' },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (!response.ok) {
        const detail = (await response.text()).slice(0, 1_000);
        throw new QwenError(`Qwen HTTP ${response.status}: ${detail}`, `QWEN_HTTP_${response.status}`, response.status >= 500 || response.status === 429);
      }
      resetIdle();
      const contentType = response.headers.get('content-type') ?? '';
      if (!response.body || contentType.includes('application/json')) {
        const payload = await response.json() as Record<string, unknown>;
        const content = extractContent(payload);
        if (content) await onDelta(content);
        return {
          content,
          usage: parseUsage(payload.usage),
          model: typeof payload.model === 'string' ? payload.model : this.config.modelAlias,
          firstTokenMs: content ? Date.now() - started : undefined,
          durationMs: Date.now() - started,
        };
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let content = '';
      let usage: QwenUsage = {};
      let model = this.config.modelAlias;
      let firstTokenMs: number | undefined;
      const consumeLine = async (line: string): Promise<void> => {
        const trimmed = line.trim();
        if (!trimmed.startsWith('data:')) return;
        const data = trimmed.slice(5).trim();
        if (!data || data === '[DONE]') return;
        let payload: Record<string, unknown>;
        try {
          payload = JSON.parse(data) as Record<string, unknown>;
        } catch {
          return;
        }
        resetIdle();
        if (payload.usage) usage = parseUsage(payload.usage);
        if (typeof payload.model === 'string') model = payload.model;
        const delta = extractContent(payload);
        if (delta) {
          if (firstTokenMs === undefined) firstTokenMs = Date.now() - started;
          content += delta;
          try {
            await onDelta(delta);
          } catch (error) {
            callbackError = error;
            throw error;
          }
        }
      };
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        resetIdle();
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split(/\r?\n/);
        buffer = lines.pop() ?? '';
        for (const line of lines) await consumeLine(line);
      }
      buffer += decoder.decode();
      for (const line of buffer.split(/\r?\n/)) await consumeLine(line);
      if (!content.trim()) throw new QwenError('Qwen returned no visible content', 'QWEN_EMPTY_RESPONSE', true);
      return { content, usage, model, firstTokenMs, durationMs: Date.now() - started };
    } catch (error) {
      if (callbackError === error) throw error;
      if (error instanceof QwenError) throw error;
      if (controller.signal.aborted) {
        const reason = controller.signal.reason;
        if (reason instanceof QwenError) throw reason;
        throw new QwenError(reason instanceof Error ? reason.message : 'Qwen generation aborted', 'QWEN_ABORTED', false);
      }
      throw new QwenError(error instanceof Error ? error.message : 'Qwen generation failed');
    } finally {
      if (idleTimer) clearTimeout(idleTimer);
      clearTimeout(maxRuntimeTimer);
      signal?.removeEventListener('abort', abort);
    }
  }
}

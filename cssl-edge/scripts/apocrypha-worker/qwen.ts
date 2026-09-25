import { log } from './log';
import type { QwenResult, QwenUsage, WorkerConfig } from './types';
import { presetById } from '../../lib/apocrypha/sampling';

const BALANCED = presetById('balanced').profile;

// DETECTED FROM THE LIVE TEMPLATE, not configured. 'on'/'off' force it; anything else auto-detects.
//
// This used to be a static env flag defaulting to OFF, and the default was costing every single
// web turn. Measured 2026-09-20 on apocky.com: a turn answering "Reply with exactly: LATENCY
// PROBE" came back as "Thinking Process: 1. Analyze the Request..." -- the model narrating its
// own reasoning into the reply and quoting its system prompt back at the reader, and taking
// ~25 s to do it. Sending enable_thinking:false on the same engine measured 346 completion
// tokens down to 6.
//
// The flag defaulted off for a real reason: the work-lane model's template does NOT declare the
// variable, and with --jinja, handing a template a variable it never declares can 400 the whole
// request. One engine serves both models in turn, so a static "on" would break chat whenever the
// arbiter swapped. Detecting it from /props solves both: ask the template that is loaded RIGHT
// NOW rather than guessing from config that cannot know which model is resident.
const THINKING_OVERRIDE = process.env.APOCRYPHA_QWEN_THINKING_KWARG?.trim().toLowerCase();

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

async function readResponseTextBounded(response: Response, maxBytes: number): Promise<string> {
  if (!response.body) return '';
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let totalBytes = 0;
  let text = '';
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    totalBytes += value.byteLength;
    if (totalBytes > maxBytes) {
      await reader.cancel().catch(() => undefined);
      throw new QwenError('Qwen response exceeded the bounded transport size', 'QWEN_TRANSPORT_LIMIT_EXCEEDED', false);
    }
    text += decoder.decode(value, { stream: true });
  }
  return text + decoder.decode();
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
  
  if (typeof delta.content === 'string' && delta.content.length > 0) return delta.content;
  if (typeof delta.reasoning_content === 'string' && delta.reasoning_content.length > 0) return delta.reasoning_content;
  if (typeof message.content === 'string' && message.content.length > 0) return message.content;
  if (typeof message.reasoning_content === 'string' && message.reasoning_content.length > 0) return message.reasoning_content;
  if (typeof item.text === 'string' && item.text.length > 0) return item.text;

  return typeof delta.content === 'string'
    ? delta.content
    : typeof message.content === 'string'
      ? message.content
      : '';
}

export function isQwenContextOverflow(status: number, detail: string): boolean {
  if (![400, 413, 422].includes(status)) return false;
  return /(?:exceed(?:s|ed)?|maximum|too\s+(?:many|long|large)|limit).{0,80}(?:context|token|prompt)|(?:context|token|prompt).{0,80}(?:exceed(?:s|ed)?|maximum|too\s+(?:many|long|large)|limit)/iu.test(detail);
}

/** The engine-facing slice of the worker config; the hosted lane substitutes its own values. */
export type EngineConfig = Pick<WorkerConfig,
  'qwenBaseUrl' | 'modelAlias' | 'contextWindowTokens' | 'maxOutputTokens' | 'qwenIdleTimeoutMs' | 'qwenMaxRuntimeMs'>;

export function hostedEngineConfig(config: WorkerConfig): EngineConfig | null {
  if (!config.hosted) return null;
  return {
    qwenBaseUrl: config.hosted.baseUrl,
    modelAlias: config.hosted.modelAlias,
    contextWindowTokens: config.hosted.contextWindowTokens,
    maxOutputTokens: config.hosted.maxOutputTokens,
    qwenIdleTimeoutMs: config.qwenIdleTimeoutMs,
    qwenMaxRuntimeMs: config.qwenMaxRuntimeMs,
  };
}

export class QwenClient {
  readonly config: EngineConfig;
  /** True for the hosted flagship lane: OpenAI-dialect gateway, no llama.cpp-only dials. */
  readonly hosted: boolean;
  private readonly fetchImpl: Fetch;
  /** null until a /props probe has read the live chat template. Null means NOT YET MEASURED --
   *  distinct from false, which means the template was read and does not declare the variable. */
  private thinkingKwargSupported: boolean | null = null;

  constructor(config: EngineConfig, fetchImpl: Fetch = fetch, options: { hosted?: boolean } = {}) {
    this.config = config;
    this.fetchImpl = fetchImpl;
    this.hosted = options.hosted === true;
  }

  /** Send enable_thinking:false? Explicit override wins; otherwise what the live template says.
   *  Unknown resolves to NO, which is the safe direction: a template that never sees the kwarg
   *  renders its normal path, while one handed a variable it does not declare can 400 the turn. */
  private sendThinkingKwarg(): boolean {
    if (THINKING_OVERRIDE === 'on') return true;
    if (THINKING_OVERRIDE === 'off') return false;
    return this.thinkingKwargSupported === true;
  }

  /** Exact prompt token count from llama-server /tokenize; null when the endpoint is unavailable. */
  async tokenCount(messages: QwenMessage[], signal?: AbortSignal): Promise<number | null> {
    if (this.hosted) return null;
    const base = this.config.qwenBaseUrl.replace(/\/v1$/, '');
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new Error('Qwen tokenize timeout')), 2_500);
    const abort = () => controller.abort(signal?.reason);
    signal?.addEventListener('abort', abort, { once: true });
    try {
      const content = messages.map((message) => `<|im_start|>${message.role}\n${message.content}<|im_end|>\n`).join('');
      const response = await this.fetchImpl(`${base}/tokenize`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ content, add_special: false, with_pieces: false }),
        signal: controller.signal,
      });
      if (!response.ok) return null;
      const body = await response.json() as { tokens?: unknown[] };
      return Array.isArray(body.tokens) ? body.tokens.length : null;
    } catch {
      return null;
    } finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
    }
  }

  async probe(signal?: AbortSignal): Promise<{ healthy: boolean; model: string; detail: string; contextTokens?: number }> {
    const base = this.config.qwenBaseUrl.replace(/\/v1$/, '');
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new Error('Qwen probe timeout')), 10_000);
    const abort = () => controller.abort(signal?.reason);
    signal?.addEventListener('abort', abort, { once: true });
    try {
      const [healthResponse, modelsResponse, propsResponse] = await Promise.all([
        this.fetchImpl(`${base}/health`, { signal: controller.signal }),
        this.fetchImpl(`${this.config.qwenBaseUrl}/models`, { signal: controller.signal }),
        this.fetchImpl(`${base}/props`, { signal: AbortSignal.any([controller.signal, AbortSignal.timeout(1_500)]) }).catch(() => null),
      ]);
      let contextTokens: number | undefined;
      if (propsResponse?.ok) {
        try {
          const props = await propsResponse.json() as {
            default_generation_settings?: { n_ctx?: unknown };
            chat_template?: unknown;
          };
          const nCtx = props.default_generation_settings?.n_ctx;
          if (typeof nCtx === 'number' && Number.isFinite(nCtx) && nCtx > 0) contextTokens = Math.floor(nCtx);
          // Ask the template that is loaded right now. The GGUF on disk is the wrong thing to
          // grep -- a header scan of both model files found zero hits while the live server
          // reported two, because the template is not where a naive file read looks.
          const template = typeof props.chat_template === 'string' ? props.chat_template : '';
          if (template) this.thinkingKwargSupported = template.includes('enable_thinking');
        } catch {
          // props is advisory only
        }
      }
      if (!healthResponse.ok || !modelsResponse.ok) {
        return { healthy: false, model: this.config.modelAlias, detail: `health=${healthResponse.status} models=${modelsResponse.status}` };
      }
      const models = await modelsResponse.json() as { data?: Array<{ id?: string }> };
      const aliases = (models.data ?? []).map((model) => model.id).filter((id): id is string => Boolean(id));

      // FOLLOW the resident model; do not pin one alias.
      //
      // This check used to require the CONFIGURED alias to appear in /v1/models, which made sense
      // when one engine served one model forever. The arbiter now swaps the model deliberately, so
      // that check failed by design the moment it did -- the worker went degraded, and because it
      // probes before claiming, it stopped claiming at all. Public chat did not get slower; it
      // stopped. llama-server ignores the model field in a completion and serves whatever it has
      // loaded, so the alias was never load-bearing for correctness, only for provenance.
      //
      // Healthy now means an engine is up and serving something. The alias it is ACTUALLY serving
      // is reported back so job provenance records the model that really answered, and a
      // divergence from config is logged rather than silently tolerated.
      const served = aliases[0];
      const healthy = aliases.length > 0;
      const matchesConfig = served !== undefined && aliases.includes(this.config.modelAlias);
      if (healthy && !matchesConfig) {
        log('warn', 'worker.qwen.alias_divergence', {
          configured: this.config.modelAlias,
          serving: served,
          note: 'following the resident model; provenance records the serving alias',
        });
      }
      return {
        healthy,
        model: matchesConfig ? this.config.modelAlias : (served ?? this.config.modelAlias),
        detail: healthy
          ? `ready${contextTokens ? ` n_ctx=${contextTokens}` : ''}${matchesConfig ? '' : ` serving=${served}`}`
          : 'engine served no models',
        ...(contextTokens ? { contextTokens } : {}),
      };
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
    const outputTokenLimit = Math.min(options.maxTokens ?? this.config.maxOutputTokens, this.config.maxOutputTokens);
    const maxAcceptedOutputBytes = Math.max(4_096, outputTokenLimit * 16);
    const maxTransportBytes = Math.max(65_536, outputTokenLimit * 256);
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
        max_tokens: outputTokenLimit,
        // Defaults come from the shared preset table, not from literals sitting here. These five
        // numbers used to be the only thing separating "the chat model" from "the coder", and they
        // were unreachable: no env, no override, no clamp. lib/apocrypha/sampling.ts carries the
        // bands and their provenance (T94_LLM_GENERATION). `balanced` is what conversation wants;
        // a turn that asks for `precise` gets the code band instead.
        temperature: options.temperature ?? BALANCED.temperature,
        top_p: options.topP ?? BALANCED.topP,
        ...(options.seed === undefined ? {} : { seed: options.seed }),
        // llama.cpp-only dials stay on the local lane; the gateway rejects or ignores them.
        ...(this.hosted ? {} : {
          top_k: options.topK ?? BALANCED.topK,
          min_p: options.minP ?? BALANCED.minP,
          repeat_penalty: options.repeatPenalty ?? BALANCED.repeatPenalty,
          // Sent ONLY when the loaded model's template actually declares this variable. Qwen3.5's
          // GGUF does; Qwen3-Coder-Next's does NOT -- grep of the two .gguf files: 1 hit vs 0. With
          // --jinja (which the work-lane launcher passes) handing a template a variable it never
          // declares risks a 400 on every turn, and this line would have been the thing that broke
          // chat the moment one engine started serving both. Off by default for that reason; set
          // APOCRYPHA_QWEN_THINKING_KWARG=on only for a model whose template takes it.
          ...(this.sendThinkingKwarg() ? { chat_template_kwargs: { enable_thinking: false } } : {}),
        }),
      };
      const response = await this.fetchImpl(`${this.config.qwenBaseUrl}/chat/completions`, {
        method: 'POST',
        headers: { 'content-type': 'application/json', accept: 'text/event-stream, application/json' },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (!response.ok) {
        const detail = (await readResponseTextBounded(response, maxTransportBytes)).slice(0, 1_000);
        if (isQwenContextOverflow(response.status, detail)) {
          throw new QwenError(`Qwen rejected the composed context: ${detail}`, 'QWEN_CONTEXT_OVERFLOW', true);
        }
        // A 400 is what a template returns when handed a variable it does not declare. The
        // resident model can change under us -- one engine serves chat and work in turn -- so a
        // rejection retires the detected flag rather than repeating the same rejected request
        // every turn until something probes again. Retryable, so the next attempt drops the kwarg.
        if (response.status === 400 && this.sendThinkingKwarg()) {
          this.thinkingKwargSupported = false;
          throw new QwenError(
            `Qwen rejected the request (HTTP 400): ${detail}. The thinking kwarg has been retired `
            + `for this client; the retry will omit it.`, 'QWEN_HTTP_400', true);
        }
        throw new QwenError(`Qwen HTTP ${response.status}: ${detail}`, `QWEN_HTTP_${response.status}`, response.status >= 500 || response.status === 429);
      }
      resetIdle();
      const contentType = response.headers.get('content-type') ?? '';
      if (!response.body || contentType.includes('application/json')) {
        const payload = JSON.parse(await readResponseTextBounded(response, maxTransportBytes)) as Record<string, unknown>;
        const content = extractContent(payload);
        if (Buffer.byteLength(content, 'utf8') > maxAcceptedOutputBytes) {
          throw new QwenError('Qwen exceeded the bounded output transport size', 'QWEN_OUTPUT_LIMIT_EXCEEDED', false);
        }
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
      let transportBytes = 0;
      let acceptedOutputBytes = 0;
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
          acceptedOutputBytes += Buffer.byteLength(delta, 'utf8');
          if (acceptedOutputBytes > maxAcceptedOutputBytes) {
            throw new QwenError('Qwen exceeded the bounded output transport size', 'QWEN_OUTPUT_LIMIT_EXCEEDED', false);
          }
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
        transportBytes += value.byteLength;
        if (transportBytes > maxTransportBytes) {
          await reader.cancel().catch(() => undefined);
          throw new QwenError('Qwen response exceeded the bounded transport size', 'QWEN_TRANSPORT_LIMIT_EXCEEDED', false);
        }
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

import type { EngineProfile, ToolCallRequest, ToolDefinition } from './types';

export class EngineError extends Error {
  readonly code: string;
  readonly retryable: boolean;

  constructor(message: string, code = 'ENGINE_ERROR', retryable = true) {
    super(message);
    this.name = 'EngineError';
    this.code = code;
    this.retryable = retryable;
  }
}

export interface EngineMessage {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  tool_calls?: { id: string; type: 'function'; function: { name: string; arguments: string } }[];
  tool_call_id?: string;
}

export interface EngineReply {
  readonly content: string;
  readonly toolCalls: ToolCallRequest[];
  readonly finishReason: string;
  readonly usage: { promptTokens?: number; completionTokens?: number; totalTokens?: number };
}

interface ToolCallAccumulator {
  id: string;
  name: string;
  args: string;
}

function parseArguments(raw: string, name: string): Record<string, unknown> {
  const trimmed = raw.trim();
  if (trimmed === '') return {};
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed as Record<string, unknown>;
    throw new Error('arguments were not a JSON object');
  } catch (error) {
    throw new EngineError(
      `the model emitted unparseable arguments for ${name}: ${error instanceof Error ? error.message : 'bad JSON'}`,
      'ENGINE_BAD_TOOL_ARGS',
      true,
    );
  }
}

/** The surface the agent loop depends on, so a scripted engine can stand in for a GPU in tests. */
export interface EngineLike {
  complete(
    messages: readonly EngineMessage[],
    tools: readonly ToolDefinition[],
    onToken: (delta: string) => void,
    signal: AbortSignal,
  ): Promise<EngineReply>;
}

export class EngineClient implements EngineLike {
  private readonly profile: EngineProfile;
  private readonly fetchImpl: typeof fetch;

  constructor(profile: EngineProfile, fetchImpl: typeof fetch = fetch) {
    this.profile = profile;
    this.fetchImpl = fetchImpl;
  }

  async probe(): Promise<{ healthy: boolean; model: string; contextWindow?: number; detail?: string }> {
    try {
      const response = await this.fetchImpl(`${this.profile.baseUrl}/props`, { signal: AbortSignal.timeout(5_000) });
      if (!response.ok) return { healthy: false, model: this.profile.alias, detail: `props returned ${response.status}` };
      const body = await response.json() as { model_alias?: string; default_generation_settings?: { n_ctx?: number } };
      return {
        healthy: true,
        model: body.model_alias ?? this.profile.alias,
        contextWindow: body.default_generation_settings?.n_ctx,
      };
    } catch (error) {
      return { healthy: false, model: this.profile.alias, detail: error instanceof Error ? error.message : 'probe failed' };
    }
  }

  /**
   * One streaming completion.
   *
   * `onToken` is called for visible content only. Tool-call deltas are accumulated silently and
   * surfaced in the returned reply, because a half-built tool call rendered into the transcript
   * reads as the model talking to itself.
   */
  async complete(
    messages: readonly EngineMessage[],
    tools: readonly ToolDefinition[],
    onToken: (delta: string) => void,
    signal: AbortSignal,
  ): Promise<EngineReply> {
    const body = {
      model: this.profile.alias,
      messages,
      tools: tools.map((tool) => ({
        type: 'function',
        function: { name: tool.name, description: tool.description, parameters: tool.parameters },
      })),
      tool_choice: 'auto',
      // Qwen-family templates default to thinking mode, and the server is configured to drop
      // reasoning content rather than stream it. Left on, a turn burns its whole output budget
      // producing tokens nobody ever sees and returns empty. The Chat lane learned this first.
      chat_template_kwargs: { enable_thinking: false },
      stream: true,
      stream_options: { include_usage: true },
      max_tokens: this.profile.maxOutputTokens,
      temperature: this.profile.temperature,
      top_p: this.profile.topP,
      top_k: this.profile.topK,
    };

    const response = await this.fetchImpl(`${this.profile.baseUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
      signal,
    }).catch((error) => {
      if (signal.aborted) throw new EngineError('turn was cancelled', 'ENGINE_CANCELLED', false);
      throw new EngineError(`engine is unreachable at ${this.profile.baseUrl}: ${error instanceof Error ? error.message : 'connect failed'}`, 'ENGINE_UNREACHABLE');
    });

    if (!response.ok || !response.body) {
      const detail = await response.text().catch(() => '');
      throw new EngineError(`engine returned ${response.status}: ${detail.slice(0, 500)}`, 'ENGINE_STATUS', response.status >= 500);
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    const calls = new Map<number, ToolCallAccumulator>();
    let content = '';
    let finishReason = '';
    let usage: EngineReply['usage'] = {};
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';
      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed.startsWith('data:')) continue;
        const payload = trimmed.slice(5).trim();
        if (payload === '[DONE]') continue;
        let event: Record<string, unknown>;
        try {
          event = JSON.parse(payload) as Record<string, unknown>;
        } catch {
          continue;
        }
        const usageBlock = event.usage as Record<string, number> | undefined;
        if (usageBlock) {
          usage = {
            promptTokens: usageBlock.prompt_tokens,
            completionTokens: usageBlock.completion_tokens,
            totalTokens: usageBlock.total_tokens,
          };
        }
        const choice = (event.choices as Array<Record<string, unknown>> | undefined)?.[0];
        if (!choice) continue;
        if (typeof choice.finish_reason === 'string' && choice.finish_reason) finishReason = choice.finish_reason;
        const delta = choice.delta as Record<string, unknown> | undefined;
        if (!delta) continue;
        if (typeof delta.content === 'string' && delta.content) {
          content += delta.content;
          onToken(delta.content);
        }
        const deltaCalls = delta.tool_calls as Array<Record<string, unknown>> | undefined;
        if (!deltaCalls) continue;
        for (const call of deltaCalls) {
          const index = Number(call.index ?? 0);
          const existing = calls.get(index) ?? { id: '', name: '', args: '' };
          if (typeof call.id === 'string' && call.id) existing.id = call.id;
          const fn = call.function as Record<string, unknown> | undefined;
          if (fn) {
            if (typeof fn.name === 'string' && fn.name) existing.name = fn.name;
            if (typeof fn.arguments === 'string') existing.args += fn.arguments;
          }
          calls.set(index, existing);
        }
      }
    }

    const toolCalls: ToolCallRequest[] = [...calls.entries()]
      .sort(([a], [b]) => a - b)
      .filter(([, call]) => call.name !== '')
      .map(([index, call]) => ({
        id: call.id || `call_${index}`,
        name: call.name,
        args: parseArguments(call.args, call.name),
      }));

    return { content, toolCalls, finishReason: finishReason || (toolCalls.length > 0 ? 'tool_calls' : 'stop'), usage };
  }
}

// The mind service: an OpenAI-dialect endpoint that speaks as Apocrypha, with memory.
//
// Sits between a surface (the live room today, anything else tomorrow) and the engine. Callers
// send a plain chat request; this assembles persona + profile + admitted memory, forwards to the
// engine, and streams the answer back. Because it speaks the same dialect as the engine it
// replaces, a surface adopts it by changing one address -- apx-room --hive, and nothing else.
//
// Reasoning is enabled and streamed. The engine emits it as `reasoning_content` deltas (or inline
// <think> tags depending on build); either way it reaches the client as it is produced, so the
// user sees the mind working instead of a blank pane. That is the point: a slow answer that shows
// its work reads as alive, and a fast one that shows nothing reads as frozen.

import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { assemble, type MindMessage } from './mind';

const PORT = Number(process.env.APOCRYPHA_MIND_PORT ?? 19131);
const HOST = process.env.APOCRYPHA_MIND_HOST ?? '127.0.0.1';
const ENGINE = process.env.APOCRYPHA_MIND_ENGINE ?? 'http://127.0.0.1:19128';
const PROFILE_DB = process.env.APOCRYPHA_PROFILE_DB_PATH ?? 'C:/Apocrypha/profile/apocrypha-profile.db';
const ANAMNESIS_DB = process.env.APOCRYPHA_ANAMNESIS_DB_PATH
  ?? 'C:/Users/Apocky/source/repos/anamnesis/anamnesis.db';
const STREAM_REASONING = process.env.APOCRYPHA_MIND_REASONING !== '0';
const MAX_BODY_BYTES = 2_000_000;

let turns = 0;
let failures = 0;
let lastError: string | null = null;

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    request.on('data', (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_BODY_BYTES) {
        reject(new Error('request body too large'));
        request.destroy();
        return;
      }
      chunks.push(chunk);
    });
    request.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    request.on('error', reject);
  });
}

function json(response: ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  response.writeHead(status, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) });
  response.end(payload);
}

function normalizeMessages(value: unknown): MindMessage[] {
  if (!Array.isArray(value)) return [];
  const messages: MindMessage[] = [];
  for (const item of value) {
    if (item === null || typeof item !== 'object') continue;
    const record = item as Record<string, unknown>;
    const role = record.role;
    if (role !== 'system' && role !== 'user' && role !== 'assistant') continue;
    const content = typeof record.content === 'string'
      ? record.content
      : Array.isArray(record.content)
        ? record.content.map((part) => (part as Record<string, unknown>)?.text ?? '').join('')
        : '';
    if (content === '') continue;
    messages.push({ role, content: String(content) });
  }
  return messages;
}

async function handleCompletions(request: IncomingMessage, response: ServerResponse): Promise<void> {
  const raw = await readBody(request);
  let payload: Record<string, unknown>;
  try {
    payload = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    json(response, 400, { error: { message: 'invalid JSON body', type: 'invalid_request_error' } });
    return;
  }

  const incoming = normalizeMessages(payload.messages);
  if (incoming.length === 0) {
    json(response, 400, { error: { message: 'messages must contain at least one turn', type: 'invalid_request_error' } });
    return;
  }

  const built = assemble({ messages: incoming, profileDb: PROFILE_DB, anamnesisDb: ANAMNESIS_DB });
  const wantsStream = payload.stream === true;
  turns += 1;
  console.log(JSON.stringify({
    event: 'mind.turn',
    stream: wantsStream,
    personaBytes: built.personaBytes,
    profileClaims: built.profileClaims,
    memoryRecords: built.memoryRecords,
    turnMessages: incoming.length,
  }));

  const upstream = {
    model: payload.model ?? 'resident',
    messages: built.messages,
    stream: wantsStream,
    temperature: payload.temperature ?? 0.7,
    max_tokens: payload.max_tokens ?? 1_024,
    chat_template_kwargs: { enable_thinking: STREAM_REASONING },
  };

  let engineResponse: Response;
  try {
    engineResponse = await fetch(`${ENGINE}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(upstream),
    });
  } catch (error) {
    failures += 1;
    lastError = (error as Error).message.slice(0, 200);
    json(response, 502, { error: { message: `engine unreachable: ${lastError}`, type: 'upstream_error' } });
    return;
  }

  if (!engineResponse.ok || engineResponse.body === null) {
    failures += 1;
    lastError = `engine HTTP ${engineResponse.status}`;
    json(response, 502, { error: { message: lastError, type: 'upstream_error' } });
    return;
  }

  if (!wantsStream) {
    const body = await engineResponse.text();
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(body);
    return;
  }

  response.writeHead(200, {
    'content-type': 'text/event-stream',
    'cache-control': 'no-cache, no-transform',
    connection: 'keep-alive',
  });

  const reader = engineResponse.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let openedThinking = false;
  let closedThinking = false;

  // Rewrite each SSE frame so reasoning reaches the client as visible content. A surface that
  // only renders `content` would otherwise show nothing at all while the model reasons, which is
  // exactly the frozen-looking pane this is meant to remove.
  const rewrite = (frame: string): string => {
    if (!frame.startsWith('data: ')) return frame;
    const data = frame.slice(6).trim();
    if (data === '[DONE]') return frame;
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(data) as Record<string, unknown>;
    } catch {
      return frame;
    }
    const choices = parsed.choices;
    if (!Array.isArray(choices) || choices.length === 0) return frame;
    const choice = choices[0] as Record<string, unknown>;
    const delta = (choice.delta ?? {}) as Record<string, unknown>;
    const reasoning = typeof delta.reasoning_content === 'string' ? delta.reasoning_content : '';
    const content = typeof delta.content === 'string' ? delta.content : '';

    if (reasoning !== '' && STREAM_REASONING) {
      const prefix = openedThinking ? '' : '<think>';
      openedThinking = true;
      (choice.delta as Record<string, unknown>) = { ...delta, content: `${prefix}${reasoning}`, reasoning_content: undefined };
      return `data: ${JSON.stringify(parsed)}`;
    }
    if (content !== '' && openedThinking && !closedThinking) {
      closedThinking = true;
      (choice.delta as Record<string, unknown>) = { ...delta, content: `</think>\n${content}` };
      return `data: ${JSON.stringify(parsed)}`;
    }
    return frame;
  };

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const frames = buffer.split('\n\n');
      buffer = frames.pop() ?? '';
      for (const frame of frames) {
        response.write(`${rewrite(frame)}\n\n`);
      }
    }
    if (buffer.trim() !== '') response.write(`${rewrite(buffer)}\n\n`);
  } catch (error) {
    failures += 1;
    lastError = (error as Error).message.slice(0, 200);
  } finally {
    response.end();
  }
}

const server = createServer((request, response) => {
  const url = request.url ?? '/';
  if (request.method === 'GET' && (url === '/health' || url === '/')) {
    json(response, 200, {
      service: 'apocrypha-mind',
      status: 'ok',
      engine: ENGINE,
      reasoning: STREAM_REASONING,
      profileDb: PROFILE_DB,
      turns,
      failures,
      lastError,
    });
    return;
  }
  if (request.method === 'POST' && url.startsWith('/v1/chat/completions')) {
    handleCompletions(request, response).catch((error) => {
      failures += 1;
      lastError = (error as Error).message.slice(0, 200);
      if (!response.headersSent) json(response, 500, { error: { message: lastError, type: 'server_error' } });
      else response.end();
    });
    return;
  }
  if (request.method === 'GET' && url.startsWith('/v1/models')) {
    json(response, 200, { object: 'list', data: [{ id: 'apocrypha-mind', object: 'model', owned_by: 'apocrypha' }] });
    return;
  }
  json(response, 404, { error: { message: 'not found', type: 'invalid_request_error' } });
});

server.listen(PORT, HOST, () => {
  console.log(JSON.stringify({
    event: 'apocrypha-mind.listening', host: HOST, port: PORT, engine: ENGINE,
    reasoning: STREAM_REASONING, profileDb: PROFILE_DB, anamnesisDb: ANAMNESIS_DB,
  }));
});

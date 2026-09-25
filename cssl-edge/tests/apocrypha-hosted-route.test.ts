// The flagship's route through the AI Gateway (lib/apocrypha/hosted-route.ts): Opus 5.5 is tried
// once per provider, rotating on refusal, before any other model; the next turn starts at the
// provider that last answered; least-effort thinking; and the two silent failures of 2026-09-25
// (a budget spent entirely on thinking, an error event mid-stream) come back named.
import { QwenClient, QwenError } from '../scripts/apocrypha-worker/qwen';
import { LEAST_EFFORT } from '../lib/apocrypha/hosted-effort';
import { attemptFields, hostedAttempts, noteAttempt, resetHostedRouteForTests } from '../lib/apocrypha/hosted-route';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const FALLBACKS = ['anthropic/claude-opus-5', 'anthropic/claude-sonnet-5'];
const OPUS = 'anthropic/claude-opus-5.5';

function sse(events: unknown[]): Response {
  const body = events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join('') + 'data: [DONE]\n\n';
  return new Response(body, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

type Body = { model: string; models?: string[]; providerOptions?: { gateway?: { only?: string[] } }; reasoning?: { effort?: string } };

function gateway(answer: (body: Body) => Response): { fetchImpl: typeof fetch; seen: Body[] } {
  const seen: Body[] = [];
  const fetchImpl = (async (_url: string, init?: RequestInit) => {
    const body = JSON.parse(String(init?.body)) as Body;
    seen.push(body);
    return answer(body);
  }) as unknown as typeof fetch;
  return { fetchImpl, seen };
}

const refused = (): Response => new Response(JSON.stringify({ error: { message: 'No access to this model at this time.', type: 'rate_limit_exceeded' } }), { status: 429 });
const provider = (body: Body): string | undefined => body.providerOptions?.gateway?.only?.[0];

async function main(): Promise<void> {
  // The plan: every provider for the flagship first, pinned and without gateway model fallbacks,
  // then the other models.
  resetHostedRouteForTests();
  const plan = hostedAttempts(OPUS, FALLBACKS, {});
  assert(plan.length === 6 && plan.slice(0, 4).every((a) => a.model === OPUS && a.provider), `plan: ${JSON.stringify(plan)}`);
  assert(plan.slice(0, 4).map((a) => a.provider).join(',') === 'anthropic,bedrock,vertexAnthropic,claudeaws', 'provider order');
  assert(!('models' in attemptFields(plan[0]!)), 'a pinned flagship attempt must not let the gateway swap the model');
  assert(JSON.stringify(attemptFields(plan[4]!)) === JSON.stringify({ model: FALLBACKS[0], models: [FALLBACKS[1]] }), 'fallback carries the rest of the chain');
  noteAttempt(plan[0]!, false, {});
  assert(hostedAttempts(OPUS, FALLBACKS, {})[0]?.provider === 'bedrock', 'a refusal passes the next turn to the next provider');
  noteAttempt({ model: OPUS, provider: 'claudeaws' }, true, {});
  assert(hostedAttempts(OPUS, FALLBACKS, {})[0]?.provider === 'claudeaws', 'the provider that answered is where the next turn starts');
  assert(hostedAttempts(OPUS, FALLBACKS, { APOCRYPHA_HOSTED_PROVIDERS: 'vertexAnthropic' }).filter((a) => a.provider).length === 1, 'the ring is configurable');

  const config = {
    qwenBaseUrl: 'https://gateway.test/v1', modelAlias: OPUS, contextWindowTokens: 1_000_000, maxOutputTokens: 64_000,
    qwenIdleTimeoutMs: 5_000, qwenMaxRuntimeMs: 10_000, apiKey: 'k',
  };
  const ask = [{ role: 'user' as const, content: 'Capital of France?' }];

  // 1. anthropic refuses, bedrock answers: the answer is Opus 5.5's, from bedrock, at least effort.
  resetHostedRouteForTests();
  const first = gateway((body) => (provider(body) === 'anthropic' ? refused() : sse([{ model: body.model, choices: [{ delta: { content: 'Paris.' } }] }, { choices: [{ finish_reason: 'stop', delta: {} }] }])));
  const one = await new QwenClient(config, first.fetchImpl, { hosted: true }).generate(ask, {}, () => undefined);
  assert(one.content === 'Paris.' && one.model === OPUS, `answer: ${JSON.stringify(one)}`);
  assert(first.seen.map(provider).join(',') === 'anthropic,bedrock', `providers tried: ${first.seen.map(provider).join(',')}`);
  assert(first.seen.every((b) => b.model === OPUS && b.models === undefined && b.reasoning?.effort === LEAST_EFFORT), 'pinned to Opus 5.5 at least effort');
  assert(LEAST_EFFORT === 'low', 'least effort Opus 5.5 accepts is low');

  // 2. the next turn starts at bedrock, which answered.
  const second = gateway((body) => sse([{ model: body.model, choices: [{ delta: { content: 'Paris.' } }] }]));
  await new QwenClient(config, second.fetchImpl, { hosted: true }).generate(ask, {}, () => undefined);
  assert(second.seen.length === 1 && provider(second.seen[0]!) === 'bedrock', 'sticky on the provider that answered');

  // 3. everything refuses: six attempts (four providers, two models), then a visible 429.
  resetHostedRouteForTests();
  const none = gateway(() => refused());
  let code = '';
  try { await new QwenClient(config, none.fetchImpl, { hosted: true }).generate(ask, {}, () => undefined); } catch (error) { code = error instanceof QwenError ? error.code : 'other'; }
  assert(code === 'QWEN_HTTP_429' && none.seen.length === 6, `all refused: ${code} after ${none.seen.length}`);
  assert(none.seen.slice(4).map((b) => b.model).join(',') === FALLBACKS.join(','), 'other models only after every provider');

  // 4. the thesis failure: the whole budget spent thinking, nothing written -> named, not "empty".
  const thinking = gateway(() => sse([{ choices: [{ delta: { reasoning: 'hmm' } }] }, { choices: [{ finish_reason: 'length', delta: {} }] }]));
  code = '';
  try { await new QwenClient(config, thinking.fetchImpl, { hosted: true }).generate(ask, {}, () => undefined); } catch (error) { code = error instanceof QwenError ? error.code : 'other'; }
  assert(code === 'QWEN_THINKING_EXHAUSTED', `budget spent thinking: ${code}`);

  // 5. an error event mid-stream is an error, not silence.
  const broken = gateway(() => sse([{ error: { message: 'upstream overloaded' } }]));
  code = '';
  try { await new QwenClient(config, broken.fetchImpl, { hosted: true }).generate(ask, {}, () => undefined); } catch (error) { code = error instanceof QwenError ? error.code : 'other'; }
  assert(code === 'QWEN_UPSTREAM_ERROR', `stream error: ${code}`);

  console.log('apocrypha-hosted-route.test : OK · provider ring before fallback models, sticky on success, least effort, named thinking/stream failures');
}

void main().catch((error) => { console.error(error); process.exit(1); });

// The engine probe decides whether the worker claims jobs at all, so both directions matter.
//
// It used to require the CONFIGURED alias to appear in /v1/models. The arbiter now swaps the
// resident model on purpose, so that check failed by design the moment it did: the worker reported
// degraded and, because it probes before claiming, stopped serving public chat entirely. This
// pins the replacement -- follow the resident model -- AND pins the cases that must still fail,
// because a probe that can only return healthy is not a probe.

import { QwenClient } from '../scripts/apocrypha-worker/qwen';
import type { WorkerConfig } from '../scripts/apocrypha-worker/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const config = {
  qwenBaseUrl: 'http://127.0.0.1:19128/v1',
  modelAlias: 'qwen35-35b-a3b-q4',
} as WorkerConfig;

function engine(options: {
  health?: number;
  models?: number;
  served?: string[];
  nCtx?: number;
}): typeof fetch {
  const { health = 200, models = 200, served = ['qwen35-35b-a3b-q4'], nCtx = 16_384 } = options;
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith('/health')) return new Response('{}', { status: health });
    if (url.endsWith('/props')) {
      return new Response(JSON.stringify({ default_generation_settings: { n_ctx: nCtx } }), { status: 200 });
    }
    if (url.endsWith('/models')) {
      return new Response(JSON.stringify({ data: served.map((id) => ({ id })) }), { status: models });
    }
    throw new Error(`unexpected probe URL ${url}`);
  }) as typeof fetch;
}

async function main(): Promise<void> {
  // MUST BE HEALTHY -----------------------------------------------------------------------------
  const matching = await new QwenClient(config, engine({})).probe();
  assert(matching.healthy, 'a matching alias was not healthy');
  assert(matching.model === 'qwen35-35b-a3b-q4', `matching probe reported model ${matching.model}`);
  assert(matching.contextTokens === 16_384, 'context tokens were not read from /props');

  // The case that took public chat down: the engine serving a model the worker was not configured
  // for. The direction flipped on 2026-09-16 -- the coder IS the configured model now, so the
  // divergent case is the 35B chat model being loaded underneath it.
  // The fixture's configured alias is the chat model again (see `config` above), so the swapped
  // engine must serve something ELSE -- serving the configured alias here was a matching probe in
  // disguise and could never surface a divergence.
  const swapped = await new QwenClient(config, engine({ served: ['qwen3-coder-next-80b-a3b'] })).probe();
  assert(swapped.healthy, 'a deliberately swapped model was reported unhealthy -- the worker would refuse to claim');
  assert(swapped.model === 'qwen3-coder-next-80b-a3b',
    `provenance must record the model that actually answered, got ${swapped.model}`);
  assert(swapped.detail.includes('serving='), 'the divergence was not surfaced in the probe detail');

  // MUST BE UNHEALTHY ---------------------------------------------------------------------------
  // Without these the change would be "always healthy", which is not a probe (G2/G5).
  const noEngine = await new QwenClient(config, engine({ health: 503 })).probe();
  assert(!noEngine.healthy, 'a 503 from /health was reported healthy');

  const noModels = await new QwenClient(config, engine({ models: 500 })).probe();
  assert(!noModels.healthy, 'a failing /models was reported healthy');

  const emptyList = await new QwenClient(config, engine({ served: [] })).probe();
  assert(!emptyList.healthy, 'an engine serving NO model was reported healthy');
  assert(emptyList.detail.includes('served no models'), 'empty model list produced an unhelpful detail');

  const unreachable = await new QwenClient(config, (async () => { throw new Error('ECONNREFUSED'); }) as typeof fetch).probe();
  assert(!unreachable.healthy, 'an unreachable engine was reported healthy');

  console.log('apocrypha-worker-probe.test: 2 healthy cases, 4 unhealthy cases, alias divergence followed');
}

main().then(() => console.log('apocrypha-worker-probe OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

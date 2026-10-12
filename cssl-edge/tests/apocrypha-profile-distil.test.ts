// The distillation pass's dead-engine trip wire, driven by a fake fetch instead of a GPU.
//
// What it guards against is measured, not imagined: on 2026-10-12 04:15-04:35 the resident engine
// answered every request HTTP 500 "peg-native" (evicted from VRAM, /health still ok), the pass sent
// 25 of 25 batches into it, and the wrapper logged "distil exit code: 0". The same thing had
// happened the day before (23 batches). Two properties fix that, and both are pressed here:
//
//   1. three consecutive ENGINE failures stop the pass after exactly three requests, and the
//      counter resets on any batch that comes back, so a flaky engine still finishes;
//   2. a pass that was stopped, or whose every batch failed, exits 1; partial failure and an
//      empty corpus still exit 0.
//
// Only distilBatches + complete + exitCodeFor are exercised. runPass needs the real databases and
// is covered by the scheduled run itself; the loop it delegates to is what is under test.

import {
  complete, distilBatches, exitCodeFor, parseClaims,
  EngineFailure, MAX_CONSECUTIVE_ENGINE_FAILURES,
  type BatchRun, type FetchLike,
} from '../scripts/apocrypha-profile/distil';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function same<T>(actual: readonly T[], expected: readonly T[], what: string): void {
  assert(JSON.stringify(actual) === JSON.stringify(expected), `${what}: got ${JSON.stringify(actual)} expected ${JSON.stringify(expected)}`);
}

// What llama.cpp b9743 actually sent, copied from the 2026-10-12 server log.
const PEG_MESSAGE = 'The model produced output that does not match the expected peg-native format';

type Step =
  | { readonly status: number; readonly body: string }
  | { readonly reject: Error }
  | { readonly hang: true };

const PEG_500: Step = {
  status: 500,
  body: JSON.stringify({ error: { code: 500, message: PEG_MESSAGE, type: 'server_error' } }),
};
const OK: Step = { status: 200, body: JSON.stringify({ choices: [{ message: { content: '[]' } }] }) };
const BAD_400: Step = {
  status: 400,
  body: JSON.stringify({ error: { code: 400, message: 'the request exceeds the available context size', type: 'invalid_request_error' } }),
};
// A 200 whose body is an error naming the fault: the regex, not the status, must catch this one.
const OK_WITH_PEG: Step = { status: 200, body: JSON.stringify({ error: { message: `unparsed peg-native output` } }) };
const REFUSED: Step = { reject: Object.assign(new TypeError('fetch failed'), { cause: new Error('ECONNREFUSED') }) };
const HANG: Step = { hang: true };

/** Scripted fetch: one step per call, the last step repeating once the script runs out. */
function fakeFetch(steps: readonly Step[]): { readonly fetch: FetchLike; calls: () => number; bodies: () => string[] } {
  let count = 0;
  const bodies: string[] = [];
  const fetch: FetchLike = (_url, init) => {
    const step = steps[Math.min(count, steps.length - 1)];
    count += 1;
    bodies.push(String(init.body));
    if (step === undefined) throw new Error('empty script');
    if ('reject' in step) return Promise.reject(step.reject);
    if ('hang' in step) {
      return new Promise<Response>((_resolve, reject) => {
        init.signal?.addEventListener('abort', () => {
          reject(Object.assign(new Error('This operation was aborted'), { name: 'AbortError' }));
        }, { once: true });
      });
    }
    return Promise.resolve(new Response(step.body, { status: step.status, headers: { 'content-type': 'application/json' } }));
  };
  return { fetch, calls: () => count, bodies: () => bodies };
}

const SLICES: ReadonlyArray<readonly number[]> = Array.from({ length: 10 }, (_, index) => [index]);

interface Drive {
  readonly run: BatchRun;
  readonly calls: number;
  readonly lines: readonly string[];
  readonly accepted: readonly number[];
  readonly firstBody: string | undefined;
}

async function drive(steps: readonly Step[], options: { timeoutMs?: number; max?: number } = {}): Promise<Drive> {
  const fake = fakeFetch(steps);
  const lines: string[] = [];
  const accepted: number[] = [];
  const run = await distilBatches(
    SLICES,
    (slice, signal) => complete(`batch ${slice.join(',')}`, signal, fake.fetch).then(parseClaims),
    (_slice, _claims, at) => { accepted.push(at.index); },
    { timeoutMs: options.timeoutMs ?? 5_000, say: (line) => lines.push(line), maxConsecutiveEngineFailures: options.max },
  );
  return { run, calls: fake.calls(), lines, accepted, firstBody: fake.bodies()[0] };
}

const ABORT_LINE = (last: string): string =>
  `engine unhealthy: stopping pass after ${MAX_CONSECUTIVE_ENGINE_FAILURES} consecutive failures (last: ${last})`;

async function main(): Promise<void> {
  assert(MAX_CONSECUTIVE_ENGINE_FAILURES === 3, 'the trip wire is three, as specified');

  // 1 -- three 500s: exactly three requests, then stop. The log line is the specified one and
  //      carries the engine's own message, which the old "engine HTTP 500" never surfaced.
  {
    const d = await drive([PEG_500]);
    assert(d.calls === 3, `three 500s -> 3 fetch calls, got ${d.calls}`);
    assert(d.run.batchesAttempted === 3 && d.run.batchesFailed === 3, `attempted/failed = 3/3, got ${d.run.batchesAttempted}/${d.run.batchesFailed}`);
    assert(d.accepted.length === 0, 'nothing was accepted');
    const expected = ABORT_LINE(`engine HTTP 500: ${PEG_MESSAGE}`);
    assert(d.run.aborted === expected, `aborted line: got ${JSON.stringify(d.run.aborted)}`);
    assert(d.lines[d.lines.length - 1] === expected, 'the abort line went through the logger last');
    assert(d.lines[0] === `batch 1: FAILED engine HTTP 500: ${PEG_MESSAGE}`, `per-batch line kept its shape, got ${d.lines[0]}`);
    assert(exitCodeFor(d.run) === 1, 'an aborted pass exits 1');
    // The request itself is unchanged: same model, same budget, thinking off.
    const body = JSON.parse(d.firstBody ?? '{}') as { model?: string; max_tokens?: number; chat_template_kwargs?: { enable_thinking?: boolean }; messages?: Array<{ content: string }> };
    assert(body.model === 'resident' && body.max_tokens === 1_400, 'request keeps model=resident and max_tokens=1400');
    assert(body.chat_template_kwargs?.enable_thinking === false, 'request keeps enable_thinking:false');
    assert(body.messages?.[1]?.content === 'batch 0', 'the prompt is the rendered batch');
  }

  // 2 -- 500,200,500,500,500: the 200 resets the counter, so the stop comes on the FIFTH call.
  {
    const d = await drive([PEG_500, OK, PEG_500, PEG_500, PEG_500]);
    assert(d.calls === 5, `500,200,500,500,500 -> 5 fetch calls, got ${d.calls}`);
    assert(d.run.aborted !== null, 'aborted');
    assert(d.run.batchesAttempted === 5 && d.run.batchesFailed === 4, `attempted/failed = 5/4, got ${d.run.batchesAttempted}/${d.run.batchesFailed}`);
    same(d.accepted, [1], 'only the second batch was accepted');
    assert(exitCodeFor(d.run) === 1, 'aborted after partial progress still exits 1');
  }

  // 3 -- the reset, isolated: 2 failures, a success, 2 failures, a success... never trips at 3.
  //      Null for this check: the same script with the wire set to 2 DOES trip, on the second call.
  {
    const script = [PEG_500, PEG_500, OK, PEG_500, PEG_500, OK, PEG_500, PEG_500, OK, OK];
    const d = await drive(script);
    assert(d.run.aborted === null, `flaky engine finishes the pass, got ${d.run.aborted}`);
    assert(d.calls === 10 && d.run.batchesAttempted === 10 && d.run.batchesFailed === 6, `10 calls, 6 failed, got ${d.calls}/${d.run.batchesFailed}`);
    same(d.accepted, [2, 5, 8, 9], 'accepted indices');
    assert(exitCodeFor(d.run) === 0, 'partial failure exits 0');
    const tripped = await drive(script, { max: 2 });
    assert(tripped.calls === 2 && tripped.run.aborted !== null, `with the wire at 2 the same script trips on call 2, got ${tripped.calls}`);
  }

  // 4 -- a 4xx is the batch's fault, not the engine's: ten in a row never trip the wire, but a
  //      pass in which every batch failed still exits 1.
  {
    const d = await drive([BAD_400]);
    assert(d.calls === 10 && d.run.aborted === null, `4xx never trips, got calls=${d.calls} aborted=${d.run.aborted}`);
    assert(d.run.batchesFailed === 10, `all ten failed, got ${d.run.batchesFailed}`);
    assert(d.lines[0] === 'batch 1: FAILED engine HTTP 400: the request exceeds the available context size', `4xx line, got ${d.lines[0]}`);
    assert(exitCodeFor(d.run) === 1, 'all-failed exits 1 even without an abort');
  }

  // 5 -- refused connection: a fetch rejection is an engine failure.
  {
    const d = await drive([REFUSED]);
    assert(d.calls === 3, `three refusals -> 3 calls, got ${d.calls}`);
    assert(d.run.aborted === ABORT_LINE('engine fetch failed: TypeError: fetch failed'), `refusal line, got ${d.run.aborted}`);
  }

  // 6 -- the body regex: a 200 carrying a peg-native error counts, mixed with the other two kinds.
  {
    const d = await drive([OK_WITH_PEG, PEG_500, REFUSED]);
    assert(d.calls === 3 && d.run.aborted !== null, `mixed engine failures trip at 3, got ${d.calls}`);
    assert(d.lines[0] === 'batch 1: FAILED engine HTTP 200: unparsed peg-native output', `200+peg line, got ${d.lines[0]}`);
  }

  // 7 -- the batch timer: a request that never answers is aborted and counts as an engine failure.
  {
    const started = Date.now();
    const d = await drive([HANG], { timeoutMs: 20 });
    const elapsed = Date.now() - started;
    assert(d.calls === 3 && d.run.aborted !== null, `three hangs trip at 3, got ${d.calls}`);
    assert(d.run.aborted.includes('AbortError'), `the abort names the timer, got ${d.run.aborted}`);
    assert(elapsed < 2_000, `three 20 ms timeouts finished in ${elapsed} ms`);
  }

  // 8 -- classification is a type, not a string match, so the loop cannot drift from complete().
  {
    const controller = new AbortController();
    const engine = await complete('x', controller.signal, fakeFetch([PEG_500]).fetch).catch((error: unknown) => error);
    assert(engine instanceof EngineFailure && engine.status === 500, 'a 500 is an EngineFailure with its status');
    const request = await complete('x', controller.signal, fakeFetch([BAD_400]).fetch).catch((error: unknown) => error);
    assert(request instanceof Error && !(request instanceof EngineFailure), 'a 400 is a plain Error');
    const text = await complete('x', controller.signal, fakeFetch([OK]).fetch);
    assert(text === '[]', `success path returns the content, got ${JSON.stringify(text)}`);
  }

  // 9 -- the exit-code table, including the two success shapes that must stay 0.
  {
    const code = (batchesAttempted: number, batchesFailed: number, aborted: string | null): 0 | 1 =>
      exitCodeFor({ batchesAttempted, batchesFailed, aborted });
    assert(code(0, 0, null) === 0, 'empty corpus exits 0');
    assert(code(25, 0, null) === 0, 'clean pass exits 0');
    assert(code(25, 24, null) === 0, 'one good batch out of 25 exits 0');
    assert(code(25, 25, null) === 1, 'all 25 failed exits 1 (the 2026-10-12 shape)');
    assert(code(3, 3, 'stopped') === 1, 'aborted exits 1');
    assert(code(5, 4, 'stopped') === 1, 'aborted after progress exits 1');
  }

  console.log('apocrypha-profile-distil.test: 9 groups, trip wire at 3, reset verified with a null, '
    + '4xx excluded, 200+peg-native included, timer included, exit-code table complete');
}

main().then(() => console.log('apocrypha-profile-distil OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

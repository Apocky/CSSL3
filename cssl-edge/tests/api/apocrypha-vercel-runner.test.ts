// The flagship runner on Vercel: claims through the control-plane RPCs, streams Opus 5.5 from the
// AI Gateway into chunks, completes with a lane/model/cost receipt, fails visibly, and stays off
// without gateway credentials (the PC worker keeps answering).
import { composeMessages, gatewayToken, resetRunnerNodeForTests, runQueuedJobs } from '@/lib/apocrypha/vercel-runner';

function assert(condition: unknown, message: string): asserts condition { if (!condition) throw new Error(`assert failed : ${message}`); }

function sse(events: unknown[]): Response {
  const body = events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join('') + 'data: [DONE]\n\n';
  return new Response(body, { status: 200, headers: { 'content-type': 'text/event-stream' } });
}

async function main(): Promise<void> {
  assert(gatewayToken({ NODE_ENV: 'test' }) === null, 'no credentials -> no token');
  assert(gatewayToken({ NODE_ENV: 'test', VERCEL_OIDC_TOKEN: 'oidc' })?.via === 'oidc', 'OIDC token is accepted');
  assert(gatewayToken({ NODE_ENV: 'test', AI_GATEWAY_API_KEY: 'k', VERCEL_OIDC_TOKEN: 'oidc' })?.via === 'api_key', 'API key wins over OIDC');

  const off = await runQueuedJobs({ env: { NODE_ENV: 'test' }, client: { rpc: async () => { throw new Error('must not touch the database'); } } });
  assert(off.state === 'unconfigured' && off.jobs.length === 0, 'unconfigured runner claims nothing and says so');

  const messages = composeMessages({ prompt: 'What changed?', messages: [{ role: 'user', content: 'Hi' }, { role: 'assistant', content: 'Hello.' }, { role: 'user', content: 'What changed?' }], attachments: [{ name: 'notes.txt', text: 'ATTACHMENT_MARKER' }] });
  assert(messages[0]?.role === 'system' && messages[0].content.includes('ATTACHMENT_MARKER'), 'attachments ride in the system message');
  assert(messages.filter((m) => m.role === 'user').length === 2, 'the prompt is not duplicated when the last message already carries it');

  const calls: Array<{ name: string; args: Record<string, unknown> }> = [];
  let claims = 0;
  const reply = 'The attached note moves the launch to the 14th. '.repeat(12);
  const client = {
    async rpc(name: string, args: Record<string, unknown>) {
      calls.push({ name, args });
      if (name === 'apocrypha_issue_worker_token') return { data: [{ node_id: 'node-1', node_token: 'apn_' + 'a'.repeat(64) }], error: null };
      if (name === 'apocrypha_claim_job') {
        claims += 1;
        if (claims > 2) return { data: [], error: null };
        return { data: [{ job_id: `job-${claims}`, attempt_id: `att-${claims}`, lease_epoch: 1, lease_token: 'apl_x', kind: 'apocky_chat', capability: 'apocky_owner_chat', request: { prompt: claims === 1 ? 'Question' : 'FAIL' } }], error: null };
      }
      return { data: null, error: null };
    },
  };
  const fetchImpl = (async (_url: string, init?: RequestInit) => {
    const body = JSON.parse(String(init?.body)) as { model: string; messages: Array<{ content: string }>; stream: boolean; reasoning: { effort: string } };
    // Owner steering 2026-09-25: the flagship runs at maximum effort with its full output budget,
    // and names gateway fallbacks because Opus is rate-limited per team at the gateway.
    assert(body.model === 'anthropic/claude-opus-5.5' && body.stream === true && body.reasoning.effort === 'max', 'gateway request pins the flagship, streams, and sets effort');
    assert(body.max_tokens === 64_000, 'the flagship gets its full output budget (thinking counts against it)');
    assert(Array.isArray(body.models) && body.models.includes('anthropic/claude-sonnet-5'), 'gateway fallbacks are named');
    if (body.messages.at(-1)?.content === 'FAIL') return new Response('quota', { status: 429 });
    return sse([
      { model: 'anthropic/claude-opus-5.5', choices: [{ delta: { content: '<think>plan</think>' + reply.slice(0, 300) } }] },
      { choices: [{ delta: { content: reply.slice(300) } }] },
      { choices: [], usage: { prompt_tokens: 1000, completion_tokens: 500, total_tokens: 1500 } },
    ]);
  }) as unknown as typeof fetch;
  resetRunnerNodeForTests();
  const outcome = await runQueuedJobs({ env: { NODE_ENV: 'test', AI_GATEWAY_API_KEY: 'k' }, client, fetchImpl, budgetMs: 10_000 });
  assert(outcome.state === 'ran' && outcome.jobs.length === 2, `two jobs ran: ${JSON.stringify(outcome)}`);
  const [ok, failed] = outcome.jobs;
  assert(ok?.status === 'succeeded' && ok.total_cost_usd === 0.014 && ok.model === 'anthropic/claude-opus-5.5', `receipt: ${JSON.stringify(ok)}`);
  assert(failed?.status === 'failed' && failed.error?.startsWith('GATEWAY_HTTP_429'), 'gateway refusal fails the job visibly');
  const chunks = calls.filter((c) => c.name === 'apocrypha_append_chunk');
  assert(chunks.length >= 3 && chunks.every((c, i) => c.args.p_seq === i && (c.args.p_metadata as { engine_lane: string }).engine_lane === 'flagship'), 'chunks stream in order with the lane');
  assert(chunks.map((c) => c.args.p_delta).join('') === '<think>plan</think>' + reply, 'chunks reconstruct the raw stream');
  const complete = calls.find((c) => c.name === 'apocrypha_complete_job');
  assert(complete && complete.args.p_content === reply.trim(), 'the stored answer drops the thought');
  assert((complete.args.p_usage as { engine_lane: string; total_cost_usd: number }).engine_lane === 'flagship', 'usage names the lane');
  const fail = calls.find((c) => c.name === 'apocrypha_fail_job');
  assert(fail && fail.args.p_retryable === true && String(fail.args.p_error_code).startsWith('GATEWAY_HTTP_429'), 'fail_job carries a retryable code');
  assert(calls.filter((c) => c.name === 'apocrypha_issue_worker_token').length === 1, 'one node identity per instance');
  console.log('apocrypha-vercel-runner.test : OK · off without credentials, claim/stream/complete with receipt, visible failure');
}

void main().catch((error) => { console.error(error); process.exit(1); });

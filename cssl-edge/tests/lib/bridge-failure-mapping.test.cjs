const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const webRoot = process.cwd();
const sourcePath = path.resolve(process.argv[2] || 'lib/apocv4/runtime-proxy.ts');
const baseline = process.argv.includes('--baseline-countercase');
const fromWeb = createRequire(path.join(webRoot, 'package.json'));
const ts = fromWeb('typescript');
const source = fs.readFileSync(sourcePath, 'utf8');
const compiled = ts.transpileModule(source, { fileName: sourcePath, compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true,
}, reportDiagnostics: true });
assert.equal((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const { retryableBridgeResult } = fromWeb('./lib/bridge/crypto.ts');
const env = { NODE_ENV: 'test', APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED: '1',
  APOCV4_RUNTIME_TRANSPORT: 'outbound-bridge', APOCV4_RUNTIME_URL: 'https://www.apocky.com',
  APOCRYPHA_BRIDGE_OWNER_USER_ID: '11111111-1111-4111-8111-111111111111',
  APOCV4_API_TOKEN: 'bridge-failure-fixture-token', APOCV4_RUNTIME_DIRECT_IP: '198.51.100.42',
  APOCV4_RUNTIME_DIRECT_PORT: '31234' };
let nextResponse;
const calls = [];
const moduleState = { exports: {} };
const context = { module: moduleState, exports: moduleState.exports, Buffer, Response, Headers,
  URL, URLSearchParams, TextDecoder, TextEncoder, AbortController, setTimeout, clearTimeout,
  process: { env }, fetch: async (url) => { calls.push({ target: String(url), transport: 'test-fetch' }); return nextResponse(); },
  require: spec => {
    if (spec === '../bridge/queue') return { fetchBridge: async input => {
      calls.push({ target: input.target, method: input.method, transport: 'outbound-bridge' }); return nextResponse();
    } };
    if (spec.startsWith('node:')) return fromWeb(spec);
    if (spec.startsWith('.')) return fromWeb(path.resolve(webRoot, 'lib/apocv4', spec));
    throw new Error('Unexpected fixture import: ' + spec);
  },
};
vm.runInNewContext(compiled.outputText, context, { filename: sourcePath, timeout: 5000 });
const api = moduleState.exports;
const conversationId = '22222222-2222-4222-8222-222222222222';
const requestId = '33333333-3333-4333-8333-333333333333';
const sessionPrincipal = 'principal:apocky-member:' + 'a'.repeat(64);
const input = { privacyPartition: 'owner:apocky', credentialProfile: 'owner', sessionPrincipal };
const run = operation => operation === 'chat'
  ? api.submitOwnerBrainRuntimeChat({ ...input, conversationId, requestId, message: 'Fixture request.' })
  : api.getOwnerBrainRuntimeSession({ ...input, sessionId: conversationId });
let assertions = 0;
async function rejected(operation, body, status, expected, options = {}) {
  const wire = typeof body === 'string' ? body : JSON.stringify(body);
  env.APOCV4_RUNTIME_TRANSPORT = options.transport || 'outbound-bridge';
  env.APOCV4_RUNTIME_URL = options.transport === 'test-fetch' ? 'https://198.51.100.42:31234' : 'https://www.apocky.com';
  nextResponse = () => new Response(wire, { status, headers: { 'content-type': options.contentType || 'application/json' } });
  const before = calls.length;
  await assert.rejects(() => run(operation), error => error instanceof api.RuntimeProxyError
    && error.code === expected && error.upstreamStatus === status, operation + ' ' + status + ' ' + expected);
  assert.equal(calls.length, before + 1, 'one upstream response per fixture');
  if (operation === 'history') assert.ok(calls.at(-1).target.includes('/v1/chat/history?'));
  assertions++;
}
async function main() {
  const failure = code => ({ schema_version: 'apocky.bridge.error.v1', code });
  if (baseline) {
    await rejected('chat', failure('BRIDGE_LOCAL_UNAVAILABLE'), 503, 'runtime_response_invalid');
    console.log(JSON.stringify({ state: 'OBSERVED_BASELINE_COUNTERCASE', assertions }));
    return;
  }
  const codes = ['BRIDGE_INDETERMINATE', 'BRIDGE_JOB_EXPIRED', 'BRIDGE_LOCAL_CREDENTIAL_UNAVAILABLE',
    'BRIDGE_LOCAL_UNAVAILABLE', 'BRIDGE_LOCAL_TIMEOUT', 'BRIDGE_HTTP_UNAVAILABLE'];
  for (const code of codes) for (const status of [502, 503]) {
    const body = failure(code);
    assert.equal(retryableBridgeResult({ schema_version: 'apocky.bridge.http-result.v1',
      job_id: '44444444-4444-5444-8444-444444444444', status, headers: { 'content-type': 'application/json' },
      body_base64: Buffer.from(JSON.stringify(body)).toString('base64'), completed_at: '2026-09-05T20:00:00.000Z' }), true);
    for (const operation of ['chat', 'history']) await rejected(operation, body, status, 'runtime_http_error');
  }
  await rejected('chat', failure('BRIDGE_APEX_ADMISSION_PENDING'), 503, 'apex_admission_pending');
  await rejected('history', failure('BRIDGE_APEX_ADMISSION_PENDING'), 503, 'runtime_response_invalid');
  await rejected('chat', failure('BRIDGE_APEX_ADMISSION_PENDING'), 502, 'runtime_response_invalid');
  for (const operation of ['chat', 'history']) {
    for (const status of [200, 201, 400, 404, 500]) await rejected(operation, failure('BRIDGE_LOCAL_UNAVAILABLE'), status, 'runtime_response_invalid');
    for (const body of [
      failure('BRIDGE_UNKNOWN'), { schema_version: 'apocky.bridge.error.v2', code: 'BRIDGE_LOCAL_UNAVAILABLE' },
      { ...failure('BRIDGE_LOCAL_UNAVAILABLE'), result: {} }, { schema_version: 'apocky.bridge.error.v1' },
      { schema_version: 'apocky.bridge.error.v1', code: 503 }, [failure('BRIDGE_LOCAL_UNAVAILABLE')],
      '{"schema_version":"apocky.bridge.error.v1","code":"OTHER","code":"BRIDGE_LOCAL_UNAVAILABLE"}',
      '{"schema_version":"wrong","schema_version":"apocky.bridge.error.v1","code":"BRIDGE_LOCAL_UNAVAILABLE"}',
    ]) await rejected(operation, body, 503, 'runtime_response_invalid');
  }
  await rejected('chat', '{"schema_version":"apocky.bridge.error.v1","code":"OTHER","code":"BRIDGE_APEX_ADMISSION_PENDING"}', 503, 'runtime_response_invalid');
  await rejected('chat', ' { "code" : "BRIDGE_LOCAL_UNAVAILABLE", "schema_version" : "apocky.bridge.error.v1" } ', 503, 'runtime_http_error');
  await rejected('chat', '{"schema_version":"apocky.bridge.error.v1","c\\u006fde":"BRIDGE_LOCAL_UNAVAILABLE"}', 503, 'runtime_http_error');
  await rejected('chat', failure('BRIDGE_LOCAL_UNAVAILABLE'), 503, 'runtime_response_invalid', { contentType: 'text/plain' });
  await rejected('chat', failure('BRIDGE_LOCAL_UNAVAILABLE'), 503, 'runtime_response_invalid', { transport: 'test-fetch' });
  console.log(JSON.stringify({ state: 'REGRESSION_FIXTURES_PASSED', assertions, pendingQueueMutations: 0 }));
}
main().catch(error => { console.error(error); process.exitCode = 1; });

import { createServer } from 'node:http';

import {
  AcceptanceFailure,
  CLIENT_DEADLINE_MS,
  runOwnerAcceptance,
} from '../../scripts/apocv4-owner-acceptance.mjs';

function assert(condition, message) {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function json(response, status, body) {
  response.writeHead(status, {
    'Cache-Control': 'no-store, max-age=0',
    'Content-Type': 'application/json',
  });
  response.end(JSON.stringify(body));
}

function fixtureFetch(origin, authToken, options = {}) {
  return async (input, init = {}) => {
    const requested = new URL(String(input));
    const local = new URL(`${requested.pathname}${requested.search}`, origin);
    const headers = new Headers(init.headers);
    return fetch(local, { ...init, headers });
  };
}

async function withFixture(run) {
  const authToken = 'fixture-admin-secret-token';
  let objectiveRequest = null;
  let reflectCredential = false;
  const server = createServer((request, response) => {
    const authorized = request.headers.authorization === `Bearer ${authToken}`;
    if (request.url === '/admin/apex' && request.method === 'GET') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end('<!doctype html><title>Apex · Apocv4 · Apocrypha · Apocky</title><main>Verifying administrator session…</main>');
      return;
    }
    if (request.url === '/api/admin/apocv4/health' && request.method === 'GET') {
      if (!authorized) {
        json(response, 401, { authorized: false, error: 'Admin authorization required.' });
        return;
      }
      json(response, 200, {
        schema_version: 'apocky.apocv4-runtime-proxy.v1',
        kind: 'health',
        observed: {
          evidence_lane: 'observed_runtime_http',
          receipt: { upstream_status: 200 },
          runtime: {
            schema_version: 'apocv4.runtime-service.v1',
            status: 'READY',
            engine: reflectCredential ? { reflected: authToken } : { status: 'READY' },
            vision: true,
          },
        },
        model_reported: { evidence_lane: 'model_reported', present: false },
      });
      return;
    }
    if (request.url === '/api/admin/apocv4/objective' && request.method === 'POST') {
      const chunks = [];
      request.on('data', (chunk) => chunks.push(chunk));
      request.on('end', () => {
        objectiveRequest = JSON.parse(Buffer.concat(chunks).toString('utf8'));
        if (!authorized) {
          json(response, 401, { authorized: false, error: 'Admin authorization required.' });
          return;
        }
        json(response, 200, {
          schema_version: 'apocky.apocv4-runtime-proxy.v1',
          kind: 'objective',
          observed: {
            evidence_lane: 'observed_runtime_http_and_test_receipts',
            receipt: { upstream_status: 200 },
            runtime: {
              schema_version: 'apocv4.agent-supervisor.v1',
              status: 'ACCEPTED',
              max_iterations: 1,
              iterations_completed: 1,
            },
            attempts: [{ sequence: 1, test_passed: true }],
          },
          model_reported: {
            evidence_lane: 'model_reported_not_observed_fact',
            attempts: [{ sequence: 1, active_model_id: 'fixture/model' }],
          },
        });
      });
      return;
    }
    response.writeHead(404);
    response.end();
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  try {
    const address = server.address();
    assert(address && typeof address === 'object', 'fixture address available');
    const origin = `http://127.0.0.1:${address.port}`;
    await run({
      authToken,
      fetchImpl: fixtureFetch(origin, authToken),
      getObjectiveRequest: () => objectiveRequest,
      setReflectCredential: (value) => { reflectCredential = value; },
    });
  } finally {
    await new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  }
}

async function main() {
  assert(CLIENT_DEADLINE_MS === 94_000, 'client abort bound remains below 95 seconds');

  await withFixture(async ({ fetchImpl, getObjectiveRequest }) => {
    const lines = [];
    const report = await runOwnerAcceptance({
      baseUrl: 'https://candidate.example',
      mode: 'preview',
      fetchImpl,
      log: (line) => lines.push(line),
    });
    assert(report.status === 'PASSED', 'preview-safe acceptance subset passes');
    assert(report.results.filter((row) => row.status === 'PASSED').length === 3, 'three Preview assertions run');
    assert(report.results.filter((row) => row.status === 'PRODUCTION_ENV_ONLY').length === 2, 'auth checks are explicitly classified');
    assert(Object.keys(getObjectiveRequest()).join(',') === 'objective', 'browser-facing request contains one exact objective field');
    assert(lines.length === 1, 'one compact receipt emitted');
  });

  await withFixture(async ({ authToken, fetchImpl, getObjectiveRequest }) => {
    const lines = [];
    const report = await runOwnerAcceptance({
      baseUrl: 'https://apocky.com',
      mode: 'post-promotion',
      bearerToken: authToken,
      fetchImpl,
      log: (line) => lines.push(line),
    });
    assert(report.results.every((row) => row.status === 'PASSED'), 'all five post-promotion assertions pass');
    assert(report.results.at(-1)?.iterations_completed === 1, 'authenticated objective proves one iteration');
    assert(!lines.join('\n').includes(authToken), 'receipt never emits bearer credential');
    assert(!Object.hasOwn(getObjectiveRequest(), 'privacy_partition'), 'client cannot choose runtime partition');
  });

  await withFixture(async ({ authToken, fetchImpl, setReflectCredential }) => {
    setReflectCredential(true);
    let failure = null;
    try {
      await runOwnerAcceptance({
        baseUrl: 'https://apocky.com',
        mode: 'post-promotion',
        bearerToken: authToken,
        fetchImpl,
        log: () => {},
      });
    } catch (error) {
      failure = error;
    }
    assert(failure instanceof AcceptanceFailure, 'reflected credential fails closed');
    assert(failure.code === 'CREDENTIAL_RESPONSE_REFLECTION', 'reflection failure is typed without credential');
  });

  console.log('apocv4-owner-acceptance.test : OK · Preview classification + production auth oracle + secret nonreflection');
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

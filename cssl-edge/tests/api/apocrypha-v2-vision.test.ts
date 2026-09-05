import type { NextApiRequest, NextApiResponse } from 'next';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import visionSessionHandler from '@/pages/api/admin/apocrypha/vision/session';
import visionStateHandler from '@/pages/api/admin/apocrypha/vision/session/[session_ref]';
import visionFrameHandler from '@/pages/api/admin/apocrypha/vision/session/[session_ref]/frame';
import visionControlHandler from '@/pages/api/admin/apocrypha/vision/session/[session_ref]/control';

interface Output { statusCode: number; body: unknown; headers: Record<string, string>; }

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) throw new Error(`assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`);
}

function reqRes(method: string, options: {
  body?: unknown;
  query?: Record<string, string | string[]>;
  owner?: boolean;
  origin?: string;
} = {}): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    origin: options.origin ?? 'https://www.apocky.com',
    'x-forwarded-proto': 'https',
  };
  if (options.owner !== false) headers['x-apocky-test-admin-email'] = 'owner@example.test';
  const req = { method, body: options.body, query: options.query ?? {}, headers } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assertPrivate(out: Output): void {
  assert(out.headers['cache-control']?.includes('private') === true, 'vision response is private');
  assert(out.headers['cache-control']?.includes('no-store') === true, 'vision response is no-store');
  equal(out.headers.vary, 'Authorization, Cookie', 'vision cache varies on auth');
}

const originalEnv = { ...process.env };
const originalFetch = globalThis.fetch;

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  process.env.APOCKY_ADMIN_EMAILS = 'owner@example.test';
  process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com';
  process.env.CF_ACCESS_CLIENT_ID = 'test-client-id';
  process.env.CF_ACCESS_CLIENT_SECRET = 'test-client-secret';

  const sessionRef = '4a5f4e1b-0cf2-4d7e-93bb-c0d8803f7f2a';
  const consentId = '4d7f99cd-e1f6-49b0-93b5-7a2f8ff430cb';
  const frameBody = {
    sequence: 0,
    captured_at_unix_ns: 1_000_000_000,
    recorded_at_unix_ns: 1_000_000_100,
    media_type: 'image/jpeg',
    content_b64: 'aGVsbG8=',
    input_mirrored: false,
    clockwise_rotation_degrees: 0,
  };
  const calls: Array<{ url: string; body: Record<string, unknown> | null }> = [];
  globalThis.fetch = async (input, init) => {
    calls.push({
      url: String(input),
      body: init?.body ? JSON.parse(String(init.body)) as Record<string, unknown> : null,
    });
    return new Response(JSON.stringify({
      schema: 'apocrypha.v2.vision-frame.v1',
      session_ref: sessionRef,
      projection: { projection_ref: 'projection-1', color_space: 'oklab', raw_frame_retention: 'none' },
      projection_ref: 'projection-1',
      observation_hash: 'observation-1',
      transition_id: 'transition-1',
      state_root: 'root-1',
      raw_frame_retention: 'none',
    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  };

  const session = reqRes('POST', { body: { session_ref: sessionRef, consent_id: consentId, purpose: 'webcam_perception' } });
  await visionSessionHandler(session.req, session.res);
  equal(session.out.statusCode, 200, 'session proxy admits explicit consent');
  assertPrivate(session.out);
  equal(calls[0]?.url, 'https://apocrypha.apocky.com/v2/vision/session', 'session uses canonical V2 route');
  equal(calls[0]?.body?.privacy_class, 'restricted', 'vision session is restricted');
  assert(String(calls[0]?.body?.principal_ref).includes('principal:apocky-owner:'), 'principal is server-owned');
  assert(!JSON.stringify(session.out.body).includes('content_b64'), 'session output has no raw frame');

  const frame = reqRes('POST', { query: { session_ref: sessionRef }, body: frameBody });
  await visionFrameHandler(frame.req, frame.res);
  equal(frame.out.statusCode, 200, 'frame proxy admits bounded frame');
  assertPrivate(frame.out);
  equal(calls[1]?.url, `https://apocrypha.apocky.com/v2/vision/session/${sessionRef}/frame`, 'frame uses canonical V2 route');
  assert(!JSON.stringify(frame.out.body).includes('aGVsbG8='), 'frame output never echoes raw bytes');
  assert(!JSON.stringify(frame.out.body).includes('content_b64'), 'frame output never exposes raw field');

  const state = reqRes('GET', { query: { session_ref: sessionRef } });
  await visionStateHandler(state.req, state.res);
  equal(state.out.statusCode, 200, 'state proxy succeeds');
  assertPrivate(state.out);

  const control = reqRes('POST', { query: { session_ref: sessionRef }, body: { event: 'close' } });
  await visionControlHandler(control.req, control.res);
  equal(control.out.statusCode, 200, 'control proxy succeeds');
  assert(Boolean(calls[3]?.url.endsWith(`/v2/vision/session/${sessionRef}/control?event=close`)), 'control forwards event as query contract');

  const invalid = reqRes('POST', { body: { session_ref: 'bad', consent_id: consentId } });
  await visionSessionHandler(invalid.req, invalid.res);
  equal(invalid.out.statusCode, 400, 'invalid session identity denied');
  equal(calls.length, 4, 'invalid session never reaches body');

  const crossOrigin = reqRes('POST', { origin: 'https://attacker.example', body: { session_ref: sessionRef, consent_id: consentId } });
  await visionSessionHandler(crossOrigin.req, crossOrigin.res);
  equal(crossOrigin.out.statusCode, 403, 'cross-origin session denied');
  const unauthenticated = reqRes('GET', { owner: false, query: { session_ref: sessionRef } });
  await visionStateHandler(unauthenticated.req, unauthenticated.res);
  equal(unauthenticated.out.statusCode, 401, 'vision state is owner-only');
  assertPrivate(unauthenticated.out);

  const source = readFileSync(resolve(process.cwd(), 'components/apocrypha/VisionPanel.tsx'), 'utf8');
  assert(source.includes('getUserMedia({ video: true, audio: false })'), 'camera permission is explicit');
  assert(source.includes('Raw frames are held only long enough'), 'UI states ephemeral raw-frame handling');
  assert(source.includes('Start with camera consent'), 'UI requires explicit start');
  assert(source.includes('metadataOnly'), 'UI rejects raw projection payloads');
  console.log('apocrypha-v2-vision.test : OK');
}

main().finally(() => {
  globalThis.fetch = originalFetch;
  for (const key of Object.keys(process.env)) if (!(key in originalEnv)) delete process.env[key];
  Object.assign(process.env, originalEnv);
}).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

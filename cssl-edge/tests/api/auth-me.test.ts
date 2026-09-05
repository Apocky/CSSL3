import type { NextApiRequest, NextApiResponse } from 'next';

import handler from '@/pages/api/auth/me';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function response(): NextApiResponse & {
  readonly headers: Record<string, string | number | readonly string[]>;
  statusCodeValue: number;
  payload: unknown;
} {
  const headers: Record<string, string | number | readonly string[]> = {};
  const res = {
    headers,
    statusCodeValue: 0,
    payload: null as unknown,
    setHeader(name: string, value: string | number | readonly string[]) {
      headers[name.toLowerCase()] = value;
      return res;
    },
    status(code: number) {
      res.statusCodeValue = code;
      return res;
    },
    json(payload: unknown) {
      res.payload = payload;
      return res;
    },
  } as unknown as NextApiResponse & {
    readonly headers: Record<string, string | number | readonly string[]>;
    statusCodeValue: number;
    payload: unknown;
  };
  return res;
}

async function run(method: string): Promise<ReturnType<typeof response>> {
  const req = { method, headers: {}, cookies: {} } as unknown as NextApiRequest;
  const res = response();
  await handler(req, res);
  return res;
}

async function main(): Promise<void> {
  const get = await run('GET');
  assert(get.statusCodeValue === 200, 'GET returns an explicit response');
  assert(String(get.headers['cache-control']).includes('private'), 'identity response is private');
  assert(String(get.headers['cache-control']).includes('no-store'), 'identity response is never cached');
  assert(get.headers.pragma === 'no-cache', 'legacy caches receive a no-cache directive');
  assert(String(get.headers.vary).includes('Cookie'), 'cache key varies on the server session cookie');
  assert(String(get.headers.vary).includes('Authorization'), 'cache key varies on bearer authorization');

  const post = await run('POST');
  assert(post.statusCodeValue === 405, 'non-GET methods are rejected');
  assert(String(post.headers['cache-control']).includes('no-store'), 'method errors remain non-cacheable');
  assert(post.headers.allow === 'GET', 'method contract advertises GET only');

  // eslint-disable-next-line no-console
  console.log('api/auth-me.test : OK · private no-store identity boundary');
}

void main();

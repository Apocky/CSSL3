import { createHash, createHmac, randomUUID } from 'node:crypto';

export const ACCOUNT_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const CONVERSATION_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const ACCOUNT_GRANT_PREFIX = 'apoc-account-v1';

export function accountReference(subject: string): string {
  if (!ACCOUNT_UUID.test(subject)) throw new Error('ACCOUNT_SUBJECT_INVALID');
  return createHash('sha256').update(`apocky.account.v1\0${subject}`, 'utf8').digest('hex');
}

export function validAccountTarget(method: string, target: string): boolean {
  if (method === 'POST') return target === '/v1/account/turn';
  if (method !== 'GET') return false;
  return target === '/v1/account/status' || target === '/v1/account/sessions'
    || (target.startsWith('/v1/account/sessions?session_id=')
      && CONVERSATION_UUID.test(target.slice('/v1/account/sessions?session_id='.length)));
}

export function accountSigningKey(env: NodeJS.ProcessEnv = process.env): { key: Buffer; kid: string } {
  if (typeof window !== 'undefined') throw new Error('ACCOUNT_CONFIGURATION_INVALID');
  const encoded = env.APOCV4_ACCOUNT_GRANT_KEY_B64;
  const kid = env.APOCV4_ACCOUNT_GRANT_KEY_ID;
  if (!encoded || !/^[A-Za-z0-9+/]+={0,2}$/.test(encoded)
    || !kid || !/^[A-Za-z0-9_-]{1,64}$/.test(kid)) throw new Error('ACCOUNT_CONFIGURATION_UNAVAILABLE');
  const key = Buffer.from(encoded, 'base64');
  if (key.toString('base64') !== encoded || key.length < 32 || key.length > 64) {
    throw new Error('ACCOUNT_CONFIGURATION_INVALID');
  }
  return { key, kid };
}

export function signAccountRequest(input: {
  subject: string; method: 'GET' | 'POST'; target: string; body: Uint8Array;
  key: Uint8Array; kid: string; now?: number; nonce?: string;
}): string {
  const now = input.now ?? Math.floor(Date.now() / 1000);
  const nonce = input.nonce ?? randomUUID();
  if (!ACCOUNT_UUID.test(input.subject) || !validAccountTarget(input.method, input.target)
    || !CONVERSATION_UUID.test(nonce) || !Number.isSafeInteger(now) || now < 0
    || !/^[A-Za-z0-9_-]{1,64}$/.test(input.kid) || input.key.byteLength < 32 || input.key.byteLength > 64
    || input.body.byteLength > 100_000 || (input.method === 'GET' && input.body.byteLength !== 0)) {
    throw new Error('ACCOUNT_REQUEST_INVALID');
  }
  const payload = {
    alg: 'HS256', aud: 'apocv4-runtime',
    body_sha256: createHash('sha256').update(input.body).digest('hex'),
    exp: now + 60, iat: now, iss: 'https://www.apocky.com', jti: nonce, kid: input.kid,
    method: input.method, schema_version: 'apocv4.account-request-grant.v1',
    sub: input.subject, target: input.target,
  };
  const encoded = Buffer.from(JSON.stringify(payload), 'utf8').toString('base64url');
  const signed = `${ACCOUNT_GRANT_PREFIX}.${encoded}`;
  return `${signed}.${createHmac('sha256', input.key).update(signed, 'ascii').digest('base64url')}`;
}

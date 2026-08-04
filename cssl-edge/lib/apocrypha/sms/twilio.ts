import { createHmac, timingSafeEqual } from 'node:crypto';

import type { SmsDeliveryPolicy } from './core';
import { formatSmsReply } from './core';

export interface TwilioProviderConfig {
  provider: 'twilio';
  accountSid: string;
  authToken: string;
  apiKeySid: string;
  apiKeySecret: string;
  webhookUrl: string;
  statusCallbackUrl: string;
  smsNumber: string;
}

export interface TwilioSignatureInput {
  authToken: string;
  signature: string;
  url: string;
  params: URLSearchParams;
}

export interface TwilioOutboundMessage {
  to: string;
  text: string;
}

export interface TwilioSendReceipt {
  sid: string;
  status: 'accepted' | 'scheduled' | 'queued';
}

export class TwilioSendError extends Error {
  constructor(
    public readonly code: 'twilio_send_rejected' | 'twilio_send_uncertain',
    public readonly ambiguous: boolean,
    public readonly upstreamStatus: number | null = null,
  ) {
    super(code);
    this.name = 'TwilioSendError';
  }
}

const MESSAGE_SID_RE = /^(?:SM|MM)[0-9a-f]{32}$/i;
const E164_RE = /^\+[1-9][0-9]{7,14}$/;
const SIGNATURE_RE = /^[A-Za-z0-9+/]{27}=$/;
const TWILIO_TIMEOUT_MS = 10_000;
const MAX_PROVIDER_RESPONSE_BYTES = 64 * 1024;

function duplicateParam(params: URLSearchParams): boolean {
  const seen = new Set<string>();
  for (const [key] of params) {
    if (seen.has(key)) return true;
    seen.add(key);
  }
  return false;
}

function signatureBytes(authToken: string, url: string, params: URLSearchParams): Buffer {
  const pairs = [...params.entries()].sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
  const payload = pairs.reduce((value, [key, item]) => `${value}${key}${item}`, url);
  return createHmac('sha1', authToken).update(payload, 'utf8').digest();
}

export function isValidTwilioSignature(input: TwilioSignatureInput): boolean {
  if (
    !input.authToken
    || !SIGNATURE_RE.test(input.signature)
    || !input.url
    || duplicateParam(input.params)
  ) return false;
  const presented = Buffer.from(input.signature, 'base64');
  if (presented.toString('base64') !== input.signature) return false;
  const expected = signatureBytes(input.authToken, input.url, input.params);
  return presented.length === expected.length && timingSafeEqual(presented, expected);
}

function xml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

export function twimlResponse(message?: string): string {
  const child = message ? `<Message>${xml(message)}</Message>` : '';
  return `<?xml version="1.0" encoding="UTF-8"?><Response>${child}</Response>`;
}

async function readBoundedJsonResponse(response: Response): Promise<unknown> {
  const contentType = response.headers.get('content-type')?.toLowerCase() ?? '';
  if (!contentType.includes('application/json') || !response.body) {
    throw new TwilioSendError('twilio_send_uncertain', true, response.status);
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value) {
        total += value.byteLength;
        if (total > MAX_PROVIDER_RESPONSE_BYTES) {
          await reader.cancel();
          throw new TwilioSendError('twilio_send_uncertain', true, response.status);
        }
        chunks.push(value);
      }
    }
    return JSON.parse(Buffer.concat(chunks).toString('utf8')) as unknown;
  } catch (error) {
    if (error instanceof TwilioSendError) throw error;
    throw new TwilioSendError('twilio_send_uncertain', true, response.status);
  }
}

export async function sendTwilioMessage(
  config: TwilioProviderConfig,
  policy: SmsDeliveryPolicy,
  message: TwilioOutboundMessage,
  fetchImpl: typeof fetch = fetch,
): Promise<TwilioSendReceipt> {
  if (!E164_RE.test(message.to)) throw new TypeError('twilio_send_invalid');
  const boundedBody = formatSmsReply(message.text, policy.maxReplyChars, policy.maxSegments);
  const form = new URLSearchParams({
    To: message.to,
    From: config.smsNumber,
    Body: boundedBody,
    StatusCallback: config.statusCallbackUrl,
  });
  const endpoint = `https://api.twilio.com/2010-04-01/Accounts/${config.accountSid}/Messages.json`;
  let response: Response;
  try {
    response = await fetchImpl(endpoint, {
      method: 'POST',
      redirect: 'error',
      headers: {
        authorization: `Basic ${Buffer.from(`${config.apiKeySid}:${config.apiKeySecret}`).toString('base64')}`,
        'content-type': 'application/x-www-form-urlencoded;charset=UTF-8',
      },
      body: form.toString(),
      signal: AbortSignal.timeout(TWILIO_TIMEOUT_MS),
    });
  } catch {
    throw new TwilioSendError('twilio_send_uncertain', true);
  }
  if (response.status !== 201) {
    throw new TwilioSendError(
      response.status >= 500 ? 'twilio_send_uncertain' : 'twilio_send_rejected',
      response.status >= 500,
      response.status,
    );
  }
  const parsed = await readBoundedJsonResponse(response) as { sid?: unknown; status?: unknown };
  if (
    typeof parsed.sid !== 'string'
    || !MESSAGE_SID_RE.test(parsed.sid)
    || (parsed.status !== 'accepted' && parsed.status !== 'scheduled' && parsed.status !== 'queued')
  ) {
    throw new TwilioSendError('twilio_send_uncertain', true, response.status);
  }
  return { sid: parsed.sid, status: parsed.status };
}

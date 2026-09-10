import { randomUUID } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';

import {
  SMS_CONSENT_DISCLOSURE,
  classifySmsCommand,
  consentDisclosureDigest,
  diagnosticTokenHash,
  encryptSmsText,
  estimateSmsSegments,
  formatSmsReply,
  phoneBindingHash,
} from '../../../../lib/apocrypha/sms/core';
import {
  readSmsConfiguration,
  type SmsConfigurationState,
} from '../../../../lib/apocrypha/sms/config';
import {
  createSmsStore,
  smsInboundAad,
  type SmsIngressAction,
  type SmsStore,
} from '../../../../lib/apocrypha/sms/store';
import {
  isValidTwilioSignature,
  twimlResponse,
} from '../../../../lib/apocrypha/sms/twilio';

export const config = { api: { bodyParser: false } };

const MAX_FORM_BYTES = 32 * 1024;
const MAX_MESSAGE_CHARS = 1_600;
const MESSAGE_SID_RE = /^(?:SM|MM)[0-9a-f]{32}$/i;
const TWILIO_SIGNATURE_RE = /^[A-Za-z0-9+/]{27}=$/;
const FORM_CONTENT_TYPE_RE = /^application\/x-www-form-urlencoded(?:\s*;\s*charset=utf-8)?$/i;

class WebhookRequestError extends Error {
  constructor(public readonly status: 400 | 413 | 415) {
    super('sms_webhook_request_invalid');
  }
}

export interface InboundSmsDependencies {
  readConfiguration: () => SmsConfigurationState;
  createStore: () => SmsStore;
  requestId: () => string;
}

function headerValue(req: NextApiRequest, name: string): string | null {
  const value = req.headers[name.toLowerCase()];
  return typeof value === 'string' ? value : null;
}

function hasDuplicateParams(params: URLSearchParams): boolean {
  const seen = new Set<string>();
  for (const [name] of params) {
    if (seen.has(name)) return true;
    seen.add(name);
  }
  return false;
}

function strictUtf8(buffer: Buffer): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(buffer);
  } catch {
    throw new WebhookRequestError(400);
  }
}

async function readBoundedForm(req: NextApiRequest): Promise<URLSearchParams> {
  const contentType = headerValue(req, 'content-type');
  if (!contentType || !FORM_CONTENT_TYPE_RE.test(contentType.trim())) {
    throw new WebhookRequestError(415);
  }
  const contentLength = headerValue(req, 'content-length');
  if (contentLength !== null) {
    if (!/^[0-9]+$/.test(contentLength)) throw new WebhookRequestError(400);
    if (Number(contentLength) > MAX_FORM_BYTES) throw new WebhookRequestError(413);
  }

  const chunks: Buffer[] = [];
  let total = 0;
  for await (const chunk of req) {
    const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk as Uint8Array);
    total += bytes.length;
    if (total > MAX_FORM_BYTES) throw new WebhookRequestError(413);
    chunks.push(bytes);
  }
  const raw = strictUtf8(Buffer.concat(chunks));
  if (contentLength !== null && Number(contentLength) !== total) {
    throw new WebhookRequestError(400);
  }
  if (/%(?![0-9a-f]{2})/i.test(raw)) throw new WebhookRequestError(400);
  const params = new URLSearchParams(raw);
  if (hasDuplicateParams(params)) throw new WebhookRequestError(400);
  return params;
}

function setCommonHeaders(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  res.setHeader('Pragma', 'no-cache');
}

function end(res: NextApiResponse, status: number): void {
  setCommonHeaders(res);
  res.status(status).end();
}

function sendTwiml(res: NextApiResponse, message?: string): void {
  setCommonHeaders(res);
  res.setHeader('Content-Type', 'text/xml; charset=utf-8');
  res.status(200).send(twimlResponse(message));
}

function boundedActionReply(
  action: SmsIngressAction,
  maxReplyChars: number,
  maxSegments: number,
): string | undefined {
  let message: string | undefined;
  switch (action) {
    case 'queued':
    case 'duplicate':
      return undefined;
    case 'stop':
      message = 'Apocrypha SMS consent is revoked. No further replies will be sent.';
      break;
    case 'start':
      message = 'Carrier delivery is unblocked. To reconnect, text CONSENT APOCRYPHA. Text STOP to revoke.';
      break;
    case 'consent':
      message = 'Apocrypha SMS is connected: one dedicated conversation, response-only, no effect authority, excluded from training, with read-only public-safe context. Text STOP to revoke.';
      break;
    case 'help':
      message = 'Apocrypha SMS is response-only, has no effect authority, and may use read-only public-safe context. Text CONSENT APOCRYPHA to connect or STOP to revoke.';
      break;
    case 'consent_required':
      message = `${SMS_CONSENT_DISCLOSURE} Reply CONSENT APOCRYPHA.`;
      if ([...message].length > 320 || estimateSmsSegments(message) > 3) {
        throw new Error('sms_consent_disclosure_budget_invalid');
      }
      return message;
    case 'rate_limited':
      // A rate-limit response must not itself become an unmetered SMS loop.
      return undefined;
    case 'media_unsupported':
      message = 'Media is not supported on this channel yet. Nothing was sent to Apocrypha; send text only.';
      break;
  }
  return formatSmsReply(message, maxReplyChars, maxSegments);
}

export function createInboundSmsHandler(
  overrides: Partial<InboundSmsDependencies> = {},
) {
  const dependencies: InboundSmsDependencies = {
    readConfiguration: () => readSmsConfiguration(),
    createStore: () => createSmsStore(),
    requestId: randomUUID,
    ...overrides,
  };

  return async function inboundSmsHandler(req: NextApiRequest, res: NextApiResponse) {
    if (req.method !== 'POST') {
      res.setHeader('Allow', 'POST');
      end(res, 405);
      return;
    }

    let params: URLSearchParams;
    try {
      params = await readBoundedForm(req);
    } catch (error) {
      end(res, error instanceof WebhookRequestError ? error.status : 400);
      return;
    }

    let state: SmsConfigurationState;
    try {
      state = dependencies.readConfiguration();
    } catch {
      end(res, 503);
      return;
    }
    if (!state.configured) {
      end(res, 503);
      return;
    }
    const system = state.config;
    const signature = headerValue(req, 'x-twilio-signature');
    if (!signature || !TWILIO_SIGNATURE_RE.test(signature) || !isValidTwilioSignature({
      authToken: system.provider.authToken,
      signature,
      url: system.provider.webhookUrl,
      params,
    })) {
      end(res, 403);
      return;
    }

    const accountSid = params.get('AccountSid');
    const to = params.get('To');
    const from = params.get('From');
    if (accountSid !== system.provider.accountSid || to !== system.provider.smsNumber) {
      end(res, 403);
      return;
    }
    // A valid provider event for any unbound sender receives no observable
    // enrollment hint and never reaches persistence or the model.
    if (from !== system.binding.ownerNumber) {
      sendTwiml(res);
      return;
    }

    const providerMessageSid = params.get('MessageSid');
    const body = params.get('Body');
    const mediaRaw = params.get('NumMedia');
    const providerOptOutType = params.get('OptOutType');
    if (
      !providerMessageSid
      || !MESSAGE_SID_RE.test(providerMessageSid)
      || body === null
      || [...body].length > MAX_MESSAGE_CHARS
      || mediaRaw === null
      || !/^(?:0|[1-9]|10)$/.test(mediaRaw)
      || (providerOptOutType !== null && !['STOP', 'START', 'HELP'].includes(providerOptOutType.toUpperCase()))
    ) {
      end(res, 400);
      return;
    }

    // Command classification deliberately precedes media handling so a STOP
    // carrying MMS metadata still revokes immediately.
    const commandKind = classifySmsCommand(body, providerOptOutType);
    const mediaCount = Number(mediaRaw);
    if (commandKind === 'message' && mediaCount === 0 && body.trim().length === 0) {
      end(res, 400);
      return;
    }
    const requestId = dependencies.requestId();
    let result;
    try {
      const aad = smsInboundAad(system.provider.accountSid, providerMessageSid, requestId);
      const bodyCiphertext = encryptSmsText(
        JSON.stringify({ schema: 'apocrypha.sms-inbound.v1', body }),
        system.keyring,
        aad,
      );
      const phoneHash = phoneBindingHash('twilio', from, system.bindingKey);
      const store = dependencies.createStore();
      result = await store.ingest({
        providerAccountSid: system.provider.accountSid,
        providerMessageSid,
        providerRetryTokenHash: diagnosticTokenHash(headerValue(req, 'i-twilio-idempotency-token')),
        phoneHash,
        sessionId: system.binding.sessionId,
        requestId,
        bodyCiphertext,
        commandKind,
        mediaCount,
        consentDisclosureSha256: consentDisclosureDigest(),
      });
    } catch {
      end(res, 503);
      return;
    }

    // Twilio Advanced Opt-Out has already sent its own STOP/START/HELP reply.
    if (providerOptOutType !== null) {
      sendTwiml(res);
      return;
    }
    const replyAction = result.action === 'duplicate'
      && result.messageStatus === 'consent_required'
      && result.channelConsentState !== 'active'
      ? 'consent_required'
      : result.action;
    sendTwiml(
      res,
      boundedActionReply(replyAction, system.policy.maxReplyChars, system.policy.maxSegments),
    );
  };
}

export default createInboundSmsHandler();

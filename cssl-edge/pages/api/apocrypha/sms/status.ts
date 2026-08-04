import type { NextApiRequest, NextApiResponse } from 'next';

import {
  readSmsConfiguration,
  type SmsConfigurationState,
} from '../../../../lib/apocrypha/sms/config';
import {
  createSmsStore,
  type SmsProviderStatus,
  type SmsStore,
} from '../../../../lib/apocrypha/sms/store';
import { isValidTwilioSignature } from '../../../../lib/apocrypha/sms/twilio';

export const config = { api: { bodyParser: false } };

const MAX_FORM_BYTES = 32 * 1024;
const MESSAGE_SID_RE = /^(?:SM|MM)[0-9a-f]{32}$/i;
const TWILIO_SIGNATURE_RE = /^[A-Za-z0-9+/]{27}=$/;
const FORM_CONTENT_TYPE_RE = /^application\/x-www-form-urlencoded(?:\s*;\s*charset=utf-8)?$/i;
const PROVIDER_STATUSES = new Set<SmsProviderStatus>([
  'accepted',
  'scheduled',
  'queued',
  'sending',
  'sent',
  'delivered',
  'undelivered',
  'failed',
  'canceled',
  'read',
]);

class WebhookRequestError extends Error {
  constructor(public readonly status: 400 | 413 | 415) {
    super('sms_webhook_request_invalid');
  }
}

export interface StatusSmsDependencies {
  readConfiguration: () => SmsConfigurationState;
  createStore: () => SmsStore;
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
  let raw: string;
  try {
    raw = new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks));
  } catch {
    throw new WebhookRequestError(400);
  }
  if (contentLength !== null && Number(contentLength) !== total) {
    throw new WebhookRequestError(400);
  }
  if (/%(?![0-9a-f]{2})/i.test(raw)) throw new WebhookRequestError(400);
  const params = new URLSearchParams(raw);
  if (hasDuplicateParams(params)) throw new WebhookRequestError(400);
  return params;
}

function end(res: NextApiResponse, status: number): void {
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.status(status).end();
}

export function createStatusSmsHandler(
  overrides: Partial<StatusSmsDependencies> = {},
) {
  const dependencies: StatusSmsDependencies = {
    readConfiguration: () => readSmsConfiguration(),
    createStore: () => createSmsStore(),
    ...overrides,
  };

  return async function statusSmsHandler(req: NextApiRequest, res: NextApiResponse) {
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
      url: system.provider.statusCallbackUrl,
      params,
    })) {
      end(res, 403);
      return;
    }

    const accountSid = params.get('AccountSid');
    const from = params.get('From');
    const to = params.get('To');
    if (
      accountSid !== system.provider.accountSid
      || from !== system.provider.smsNumber
      || to !== system.binding.ownerNumber
    ) {
      end(res, 403);
      return;
    }

    const outboundMessageSid = params.get('MessageSid');
    const rawStatus = params.get('MessageStatus');
    const rawErrorCode = params.get('ErrorCode');
    const errorCode = rawErrorCode === null || rawErrorCode === '' ? null : rawErrorCode;
    if (
      !outboundMessageSid
      || !MESSAGE_SID_RE.test(outboundMessageSid)
      || !rawStatus
      || !PROVIDER_STATUSES.has(rawStatus as SmsProviderStatus)
      || (errorCode !== null && !/^[0-9]{1,16}$/.test(errorCode))
    ) {
      end(res, 400);
      return;
    }

    try {
      const store = dependencies.createStore();
      await store.recordDelivery(
        outboundMessageSid,
        rawStatus as SmsProviderStatus,
        errorCode,
      );
    } catch {
      end(res, 503);
      return;
    }
    end(res, 204);
  };
}

export default createStatusSmsHandler();

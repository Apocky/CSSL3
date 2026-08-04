import type {
  SmsBinding,
  SmsDeliveryPolicy,
  SmsEnvelopeKeyring,
} from './core';
import type { TwilioProviderConfig } from './twilio';

export interface SmsSystemConfiguration {
  provider: TwilioProviderConfig;
  binding: SmsBinding;
  bindingKey: Buffer;
  keyring: SmsEnvelopeKeyring;
  policy: SmsDeliveryPolicy;
}

export type SmsConfigurationState =
  | { configured: true; config: SmsSystemConfiguration; missing: [] }
  | { configured: false; config: null; missing: string[] };

const ACCOUNT_SID_RE = /^AC[0-9a-f]{32}$/i;
const API_KEY_SID_RE = /^SK[0-9a-f]{32}$/i;
const E164_RE = /^\+[1-9][0-9]{7,14}$/;
const UUID_V4_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const BASE64_RE = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const KEY_ID_RE = /^[A-Za-z0-9_-]{1,32}$/;
const RESERVED_KEY_IDS = new Set(['__proto__', 'prototype', 'constructor']);
const WEBHOOK_HOSTS = new Set(['apocky.com', 'www.apocky.com', 'apocrypha.apocky.com']);

function envText(env: Record<string, string | undefined>, name: string): string | null {
  const value = env[name]?.trim();
  return value ? value : null;
}

function validKeyId(value: string | null): value is string {
  return value !== null && KEY_ID_RE.test(value) && !RESERVED_KEY_IDS.has(value);
}

function validWebhookUrl(
  value: string | null,
  expectedPath: string,
  allowLocal: boolean,
): value is string {
  if (!value) return false;
  try {
    const url = new URL(value);
    const local = url.hostname === 'localhost' || url.hostname === '127.0.0.1';
    return (url.protocol === 'https:' || (allowLocal && local && url.protocol === 'http:'))
      && ((local && allowLocal) || (WEBHOOK_HOSTS.has(url.hostname) && url.port === ''))
      && url.username === ''
      && url.password === ''
      && url.pathname === expectedPath
      && url.hash === ''
      && url.search === '';
  } catch {
    return false;
  }
}

function storageKey(value: string | null): Buffer | null {
  if (!value || !BASE64_RE.test(value)) return null;
  const decoded = Buffer.from(value, 'base64');
  return decoded.length === 32 ? decoded : null;
}

function boundedInteger(value: string | null, fallback: number, minimum: number, maximum: number): number | null {
  if (!value) return fallback;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= minimum && parsed <= maximum ? parsed : null;
}

export function readSmsConfiguration(
  env: Record<string, string | undefined> = process.env,
): SmsConfigurationState {
  const accountSid = envText(env, 'TWILIO_ACCOUNT_SID');
  const authToken = envText(env, 'TWILIO_AUTH_TOKEN');
  const apiKeySid = envText(env, 'TWILIO_API_KEY_SID');
  const apiKeySecret = envText(env, 'TWILIO_API_KEY_SECRET');
  const webhookUrl = envText(env, 'APOCRYPHA_SMS_WEBHOOK_URL');
  const statusCallbackUrl = envText(env, 'APOCRYPHA_SMS_STATUS_CALLBACK_URL');
  const smsNumber = envText(env, 'APOCRYPHA_SMS_NUMBER_E164');
  const ownerNumber = envText(env, 'APOCRYPHA_SMS_OWNER_E164');
  const ownerUserId = envText(env, 'APOCRYPHA_SMS_OWNER_USER_ID');
  const sessionId = envText(env, 'APOCRYPHA_SMS_SESSION_ID');
  const bindingKey = storageKey(envText(env, 'APOCRYPHA_SMS_BINDING_KEY_BASE64'));
  const activeKeyId = envText(env, 'APOCRYPHA_SMS_STORAGE_KEY_ID');
  const key = storageKey(envText(env, 'APOCRYPHA_SMS_STORAGE_KEY_BASE64'));
  const previousKeyId = envText(env, 'APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_ID');
  const previousKeyValue = envText(env, 'APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_BASE64');
  const previousKey = storageKey(previousKeyValue);
  const allowLocalWebhook = env['NODE_ENV'] !== 'production';
  const maxReplyChars = boundedInteger(
    envText(env, 'APOCRYPHA_SMS_MAX_REPLY_CHARS'),
    320,
    160,
    1_200,
  );
  const maxSegments = boundedInteger(envText(env, 'APOCRYPHA_SMS_MAX_SEGMENTS'), 3, 1, 10);
  const dailySegmentBudget = boundedInteger(
    envText(env, 'APOCRYPHA_SMS_DAILY_SEGMENT_BUDGET'),
    30,
    1,
    1_000,
  );

  const invalid: string[] = [];
  if (!accountSid || !ACCOUNT_SID_RE.test(accountSid)) invalid.push('TWILIO_ACCOUNT_SID');
  if (!authToken || authToken.length < 5 || authToken.length > 256) invalid.push('TWILIO_AUTH_TOKEN');
  if (!apiKeySid || !API_KEY_SID_RE.test(apiKeySid)) invalid.push('TWILIO_API_KEY_SID');
  if (!apiKeySecret || apiKeySecret.length < 8 || apiKeySecret.length > 256) invalid.push('TWILIO_API_KEY_SECRET');
  if (!validWebhookUrl(
    webhookUrl,
    '/api/apocrypha/sms/inbound',
    allowLocalWebhook,
  )) invalid.push('APOCRYPHA_SMS_WEBHOOK_URL');
  if (!validWebhookUrl(
    statusCallbackUrl,
    '/api/apocrypha/sms/status',
    allowLocalWebhook,
  ) || statusCallbackUrl === webhookUrl) {
    invalid.push('APOCRYPHA_SMS_STATUS_CALLBACK_URL');
  }
  if (!smsNumber || !E164_RE.test(smsNumber)) invalid.push('APOCRYPHA_SMS_NUMBER_E164');
  if (!ownerNumber || !E164_RE.test(ownerNumber) || ownerNumber === smsNumber) {
    invalid.push('APOCRYPHA_SMS_OWNER_E164');
  }
  if (!ownerUserId || !UUID_V4_RE.test(ownerUserId)) invalid.push('APOCRYPHA_SMS_OWNER_USER_ID');
  if (!sessionId || !UUID_V4_RE.test(sessionId)) invalid.push('APOCRYPHA_SMS_SESSION_ID');
  if (!bindingKey) invalid.push('APOCRYPHA_SMS_BINDING_KEY_BASE64');
  if (!validKeyId(activeKeyId)) invalid.push('APOCRYPHA_SMS_STORAGE_KEY_ID');
  if (!key) invalid.push('APOCRYPHA_SMS_STORAGE_KEY_BASE64');
  if ((previousKeyId === null) !== (previousKeyValue === null)) {
    invalid.push('APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_ID', 'APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_BASE64');
  } else if (previousKeyId !== null && (!validKeyId(previousKeyId) || previousKeyId === activeKeyId)) {
    invalid.push('APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_ID');
  } else if (previousKeyValue !== null && !previousKey) {
    invalid.push('APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_BASE64');
  }
  if (bindingKey && key && bindingKey.equals(key)) {
    invalid.push('APOCRYPHA_SMS_BINDING_KEY_BASE64');
  }
  if (previousKey && key && previousKey.equals(key)) {
    invalid.push('APOCRYPHA_SMS_PREVIOUS_STORAGE_KEY_BASE64');
  }
  if (maxReplyChars === null) invalid.push('APOCRYPHA_SMS_MAX_REPLY_CHARS');
  if (maxSegments === null) invalid.push('APOCRYPHA_SMS_MAX_SEGMENTS');
  if (dailySegmentBudget === null) invalid.push('APOCRYPHA_SMS_DAILY_SEGMENT_BUDGET');
  if (invalid.length > 0) {
    return { configured: false, config: null, missing: [...new Set(invalid)] };
  }
  if (
    !accountSid || !authToken || !apiKeySid || !apiKeySecret || !webhookUrl
    || !statusCallbackUrl || !smsNumber || !ownerNumber || !ownerUserId || !sessionId
    || !bindingKey || !activeKeyId || !key
    || maxReplyChars === null || maxSegments === null || dailySegmentBudget === null
  ) {
    throw new Error('sms_configuration_validation_invariant_failed');
  }
  return {
    configured: true,
    missing: [],
    config: {
      provider: {
        provider: 'twilio',
        accountSid,
        authToken,
        apiKeySid,
        apiKeySecret,
        webhookUrl,
        statusCallbackUrl,
        smsNumber,
      },
      binding: {
        ownerNumber,
        ownerUserId: ownerUserId.toLowerCase(),
        sessionId: sessionId.toLowerCase(),
      },
      bindingKey,
      keyring: {
        activeKeyId,
        storageKey: key,
        decryptionKeys: {
          [activeKeyId]: key,
          ...(previousKeyId && previousKey ? { [previousKeyId]: previousKey } : {}),
        },
      },
      policy: {
        maxReplyChars,
        maxSegments,
        dailySegmentBudget,
      },
    },
  };
}

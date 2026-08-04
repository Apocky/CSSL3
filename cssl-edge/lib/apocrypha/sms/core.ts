import {
  createCipheriv,
  createDecipheriv,
  createHash,
  createHmac,
  randomBytes,
} from 'node:crypto';

export type SmsCommand = 'stop' | 'start' | 'consent' | 'help' | 'message';

export interface SmsBinding {
  ownerNumber: string;
  ownerUserId: string;
  sessionId: string;
}

export interface SmsEnvelopeKeyring {
  activeKeyId: string;
  storageKey: Buffer;
  decryptionKeys: Readonly<Record<string, Buffer>>;
}

export interface SmsDeliveryPolicy {
  maxReplyChars: number;
  maxSegments: number;
  dailySegmentBudget: number;
}

export const SMS_CONSENT_DISCLOSURE = [
  'Connect this number to one dedicated Apocrypha SMS conversation.',
  'Messages are encrypted in the queue, excluded from training, response-only, and cannot take actions; Apocrypha can use only read-only, public-safe context.',
  'Carrier message and data rates may apply. Text STOP at any time to revoke.',
].join(' ');

const LEGACY_CIPHERTEXT_RE = /^v1\.([A-Za-z0-9_-]+)\.([A-Za-z0-9_-]+)\.([A-Za-z0-9_-]+)$/;
const KEYED_CIPHERTEXT_RE = /^v2\.([A-Za-z0-9_-]{1,32})\.([A-Za-z0-9_-]+)\.([A-Za-z0-9_-]+)\.([A-Za-z0-9_-]+)$/;
const KEY_ID_RE = /^[A-Za-z0-9_-]{1,32}$/;
const RESERVED_KEY_IDS = new Set(['__proto__', 'prototype', 'constructor']);
const GSM_7_BASIC = new Set([..."@£$¥èéùìòÇ\nØø\rÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ !\"#¤%&'()*+,-./0123456789:;<=>?¡ABCDEFGHIJKLMNOPQRSTUVWXYZÄÖÑÜ§¿abcdefghijklmnopqrstuvwxyzäöñüà"]);
const GSM_7_EXTENSION = new Set([...'^{}\\[~]|€\f']);

export function consentDisclosureDigest(): string {
  return createHash('sha256').update(SMS_CONSENT_DISCLOSURE, 'utf8').digest('hex');
}

function validKeyId(value: string): boolean {
  return KEY_ID_RE.test(value) && !RESERVED_KEY_IDS.has(value);
}

function normalizedCommand(value: string): string {
  return value
    .trim()
    .toUpperCase()
    .replace(/[‘’]/g, "'")
    .replace(/[.!]+$/g, '')
    .replace(/\s+/g, ' ');
}

function isReasonableRevocation(value: string): boolean {
  // Keep revocation deterministic and fail-closed: every pattern consumes the
  // complete message and names either this contact channel or consent itself.
  // Do not infer revocation from substrings, sentiment, or scoped preferences.
  return /^(?:PLEASE,? )?(?:STOP|UNSUBSCRIBE|CANCEL|QUIT|END|REVOKE|OPT OUT)(?: (?:PLEASE|MESSAGES?|TEXTS?|TEXTING ME|CONTACTING ME|MY CONSENT|CONSENT))?$/.test(value)
    || /^(?:PLEASE,? )?(?:DO NOT|DON'T) (?:TEXT|MESSAGE|CONTACT) ME(?: AGAIN)?$/.test(value)
    || /^(?:PLEASE,? )?(?:DO NOT|DON'T) SEND ME (?:ANY MORE )?(?:TEXTS?|MESSAGES?)$/.test(value)
    || /^(?:PLEASE,? )?NO MORE (?:TEXTS?|MESSAGES?)$/.test(value)
    || /^(?:PLEASE,? )?OPT ME OUT$/.test(value)
    || /^(?:(?:I )?(?:HEREBY )?)(?:WITHDRAW|REVOKE) (?:MY )?CONSENT$/.test(value)
    || /^I (?:DO NOT|DON'T|NO LONGER) WANT (?:TO RECEIVE )?(?:ANY MORE )?(?:TEXTS?|MESSAGES?)$/.test(value);
}

export function classifySmsCommand(body: string, providerOptOutType?: string | null): SmsCommand {
  const provider = providerOptOutType?.trim().toUpperCase();
  if (provider === 'STOP') return 'stop';
  if (provider === 'START') return 'start';
  if (provider === 'HELP') return 'help';
  const normalized = normalizedCommand(body);
  if (
    ['STOP', 'STOPALL', 'UNSUBSCRIBE', 'CANCEL', 'END', 'QUIT', 'REVOKE', 'OPTOUT'].includes(normalized)
    || isReasonableRevocation(normalized)
  ) return 'stop';
  if (['START', 'UNSTOP'].includes(normalized)) return 'start';
  if (normalized === 'CONSENT APOCRYPHA') return 'consent';
  if (['HELP', 'INFO'].includes(normalized)) return 'help';
  return 'message';
}

export function phoneBindingHash(provider: string, e164: string, key: Buffer): string {
  if (!/^[a-z][a-z0-9_-]{1,31}$/.test(provider) || !/^\+[1-9][0-9]{7,14}$/.test(e164) || key.length !== 32) {
    throw new TypeError('sms_binding_invalid');
  }
  return createHmac('sha256', key)
    .update('APOCRYPHA-SMS-PHONE-BINDING-v1\0', 'utf8')
    .update(provider, 'ascii')
    .update('\0', 'utf8')
    .update(e164, 'ascii')
    .digest('hex');
}

export function diagnosticTokenHash(value: string | null | undefined): string | null {
  if (!value) return null;
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

export function encryptSmsText(
  plaintext: string,
  keyOrKeyring: Buffer | SmsEnvelopeKeyring,
  aad: string,
): string {
  const keyring: SmsEnvelopeKeyring | null = Buffer.isBuffer(keyOrKeyring)
    ? null
    : keyOrKeyring;
  const key: Buffer = Buffer.isBuffer(keyOrKeyring)
    ? keyOrKeyring
    : keyOrKeyring.storageKey;
  const configuredActiveKey = keyring?.decryptionKeys[keyring.activeKeyId];
  if (
    key.length !== 32
    || plaintext.length < 1
    || aad.length < 1
    || (keyring !== null && !validKeyId(keyring.activeKeyId))
    || (keyring !== null && (!configuredActiveKey || !key.equals(configuredActiveKey)))
  ) {
    throw new TypeError('sms_ciphertext_invalid');
  }
  const nonce = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', key, nonce);
  cipher.setAAD(Buffer.from(aad, 'utf8'));
  const ciphertext = Buffer.concat([cipher.update(plaintext, 'utf8'), cipher.final()]);
  const tag = cipher.getAuthTag();
  const payload = `${nonce.toString('base64url')}.${ciphertext.toString('base64url')}.${tag.toString('base64url')}`;
  return keyring ? `v2.${keyring.activeKeyId}.${payload}` : `v1.${payload}`;
}

function decryptWithKey(sealedParts: readonly string[], key: Buffer, aad: string): string {
  if (key.length !== 32 || aad.length < 1) throw new Error('invalid');
  const nonce = Buffer.from(sealedParts[0] ?? '', 'base64url');
  const ciphertext = Buffer.from(sealedParts[1] ?? '', 'base64url');
  const tag = Buffer.from(sealedParts[2] ?? '', 'base64url');
  if (nonce.length !== 12 || ciphertext.length < 1 || tag.length !== 16) throw new Error('invalid');
  const decipher = createDecipheriv('aes-256-gcm', key, nonce);
  decipher.setAAD(Buffer.from(aad, 'utf8'));
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString('utf8');
}

export function decryptSmsText(
  sealed: string,
  keyOrKeyring: Buffer | SmsEnvelopeKeyring,
  aad: string,
): string {
  try {
    const keyring: SmsEnvelopeKeyring | null = Buffer.isBuffer(keyOrKeyring)
      ? null
      : keyOrKeyring;
    const keyed = KEYED_CIPHERTEXT_RE.exec(sealed);
    if (keyed) {
      if (!keyring) throw new Error('invalid');
      const keyId = keyed[1] ?? '';
      if (!validKeyId(keyId) || !Object.hasOwn(keyring.decryptionKeys, keyId)) {
        throw new Error('invalid');
      }
      const key = keyring.decryptionKeys[keyId];
      if (!key) throw new Error('invalid');
      return decryptWithKey([keyed[2] ?? '', keyed[3] ?? '', keyed[4] ?? ''], key, aad);
    }
    const legacy = LEGACY_CIPHERTEXT_RE.exec(sealed);
    if (!legacy) throw new Error('invalid');
    const keys = keyring
      ? Object.values(keyring.decryptionKeys)
      : [keyOrKeyring as Buffer];
    for (const key of keys) {
      try {
        return decryptWithKey([legacy[1] ?? '', legacy[2] ?? '', legacy[3] ?? ''], key, aad);
      } catch {
        // Legacy v1 did not carry a key identifier; try the bounded configured keyring.
      }
    }
    throw new Error('invalid');
  } catch {
    throw new Error('sms_ciphertext_invalid');
  }
}

export function estimateSmsSegments(value: string): number {
  let septets = 0;
  let gsm = true;
  for (const char of value) {
    if (GSM_7_BASIC.has(char)) septets += 1;
    else if (GSM_7_EXTENSION.has(char)) septets += 2;
    else {
      gsm = false;
      break;
    }
  }
  if (gsm) return septets <= 160 ? 1 : Math.ceil(septets / 153);
  const units = value.length;
  return units <= 70 ? 1 : Math.ceil(units / 67);
}

export function formatSmsReply(value: string, maxChars: number, maxSegments: number): string {
  const normalized = value.replace(/\r\n?/g, '\n').trim();
  if (
    !normalized
    || !Number.isInteger(maxChars)
    || maxChars < 2
    || !Number.isInteger(maxSegments)
    || maxSegments < 1
  ) {
    throw new TypeError('sms_reply_invalid');
  }
  const points = [...normalized];
  const charBounded = points.length <= maxChars
    ? normalized
    : `${points.slice(0, maxChars - 1).join('')}…`;
  if (estimateSmsSegments(charBounded) <= maxSegments) return charBounded;

  const footer = '\n\nFull reply: apocky.com';
  const footerPoints = [...footer];
  for (let keep = Math.min(points.length, maxChars - footerPoints.length - 1); keep >= 1; keep -= 1) {
    const candidate = `${points.slice(0, keep).join('').trimEnd()}…${footer}`;
    if ([...candidate].length <= maxChars && estimateSmsSegments(candidate) <= maxSegments) {
      return candidate;
    }
  }
  throw new TypeError('sms_reply_budget_too_small');
}

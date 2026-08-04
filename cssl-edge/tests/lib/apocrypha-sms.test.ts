import { strict as assert } from 'node:assert';

import {
  classifySmsCommand,
  decryptSmsText,
  encryptSmsText,
  estimateSmsSegments,
  formatSmsReply,
  isValidTwilioSignature,
  readSmsConfiguration,
  sendTwilioMessage,
  twimlResponse,
} from '@/lib/apocrypha/sms';

const ACCOUNT_SID = 'ACaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const API_KEY_SID = 'SKcccccccccccccccccccccccccccccccc';
const MESSAGE_SID = 'SMbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const AUTH_TOKEN = '12345';

export function testTwilioSignatureVector(): void {
  const params = new URLSearchParams([
    ['CallSid', 'CA1234567890ABCDE'],
    ['Caller', '+14158675310'],
    ['Digits', '1234'],
    ['From', '+14158675310'],
    ['To', '+18005551212'],
  ]);
  assert.equal(
    isValidTwilioSignature({
      authToken: AUTH_TOKEN,
      signature: 'L/OH5YylLD5NRKLltdqwSvS0BnU=',
      url: 'https://example.com/myapp.php?foo=1&bar=2',
      params,
    }),
    true,
    'official Twilio form-signature vector verifies',
  );
  params.set('Digits', '9999');
  assert.equal(
    isValidTwilioSignature({
      authToken: AUTH_TOKEN,
      signature: 'L/OH5YylLD5NRKLltdqwSvS0BnU=',
      url: 'https://example.com/myapp.php?foo=1&bar=2',
      params,
    }),
    false,
    'changed form field fails closed',
  );
  assert.equal(
    isValidTwilioSignature({
      authToken: AUTH_TOKEN,
      signature: 'not-base64',
      url: 'https://example.com/myapp.php?foo=1&bar=2',
      params,
    }),
    false,
    'malformed signatures fail closed before comparison',
  );
}

export function testConsentCommands(): void {
  assert.equal(classifySmsCommand('STOP'), 'stop');
  assert.equal(classifySmsCommand(' stopall '), 'stop');
  assert.equal(classifySmsCommand('START'), 'start');
  assert.equal(classifySmsCommand('UNSTOP'), 'start');
  assert.equal(classifySmsCommand('CONSENT APOCRYPHA'), 'consent');
  assert.equal(classifySmsCommand('HELP'), 'help');
  assert.equal(classifySmsCommand('Please stop texting me'), 'stop');
  assert.equal(classifySmsCommand('revoke consent'), 'stop');
  for (const revocation of [
    "Please don't contact me again",
    'Please don’t contact me again!',
    'Do not send me any more messages.',
    'I withdraw consent',
    'I hereby withdraw my consent.',
    'I revoke my consent',
    "I don't want any more texts",
    'I no longer want to receive messages',
  ]) {
    assert.equal(
      classifySmsCommand(revocation),
      'stop',
      `unequivocal revocation is honored: ${revocation}`,
    );
  }
  for (const nonRevocation of [
    'Please do not stop',
    "I don't want to stop",
    'How do I withdraw consent?',
    'I withdraw consent for training',
    'Please do not contact Alex again',
    "Don't message me about billing",
    'I revoke permission to use tools',
    'I no longer want text-only replies',
    'The phrase I withdraw consent is an example',
  ]) {
    assert.equal(
      classifySmsCommand(nonRevocation),
      'message',
      `ambiguous or scoped text remains a message: ${nonRevocation}`,
    );
  }
  assert.equal(classifySmsCommand('Tell me what changed'), 'message');
  assert.equal(classifySmsCommand('anything', 'STOP'), 'stop');
  assert.equal(classifySmsCommand('anything', 'START'), 'start');
  assert.equal(classifySmsCommand('anything', 'HELP'), 'help');
}

export function testEncryptedQueuePayload(): void {
  const key = Buffer.alloc(32, 7);
  const sealed = encryptSmsText('private message', key, `inbound:${MESSAGE_SID}`);
  assert.notEqual(sealed, 'private message');
  assert.equal(decryptSmsText(sealed, key, `inbound:${MESSAGE_SID}`), 'private message');
  assert.throws(
    () => decryptSmsText(sealed, key, 'inbound:SMwrong'),
    /sms_ciphertext_invalid/,
    'ciphertext is bound to its provider message identity',
  );

  const oldKey = Buffer.alloc(32, 8);
  const currentKey = Buffer.alloc(32, 9);
  const oldKeyring = {
    activeKeyId: '2026-07',
    storageKey: oldKey,
    decryptionKeys: { '2026-07': oldKey },
  };
  const rotatedKeyring = {
    activeKeyId: '2026-08',
    storageKey: currentKey,
    decryptionKeys: { '2026-08': currentKey, '2026-07': oldKey },
  };
  const beforeRotation = encryptSmsText('queued before rotation', oldKeyring, `inbound:${MESSAGE_SID}`);
  assert.match(beforeRotation, /^v2\.2026-07\./);
  assert.equal(
    decryptSmsText(beforeRotation, rotatedKeyring, `inbound:${MESSAGE_SID}`),
    'queued before rotation',
    'previous queue key remains readable during a bounded key rotation',
  );
}

export function testSmsRendering(): void {
  assert.equal(
    twimlResponse('Use <STOP> & stay sovereign.'),
    '<?xml version="1.0" encoding="UTF-8"?><Response><Message>Use &lt;STOP&gt; &amp; stay sovereign.</Message></Response>',
  );
  assert.equal(twimlResponse(), '<?xml version="1.0" encoding="UTF-8"?><Response></Response>');
  const rendered = formatSmsReply('  hello\r\nworld  ', 9, 3);
  assert.equal(rendered, 'hello\nwo…');
  assert.equal([...rendered].length, 9, 'reply cap counts Unicode code points');
  const unicode = formatSmsReply('🙂'.repeat(300), 320, 2);
  assert(estimateSmsSegments(unicode) <= 2, 'UCS-2 reply remains inside its paid segment budget');
}

export function testConfigurationFailsClosed(): void {
  const base = {
    TWILIO_ACCOUNT_SID: ACCOUNT_SID,
    TWILIO_AUTH_TOKEN: AUTH_TOKEN,
    TWILIO_API_KEY_SID: API_KEY_SID,
    TWILIO_API_KEY_SECRET: 'restricted-api-key-secret',
    APOCRYPHA_SMS_WEBHOOK_URL: 'https://www.apocky.com/api/apocrypha/sms/inbound',
    APOCRYPHA_SMS_STATUS_CALLBACK_URL: 'https://www.apocky.com/api/apocrypha/sms/status',
    APOCRYPHA_SMS_NUMBER_E164: '+16025550123',
    APOCRYPHA_SMS_OWNER_E164: '+16025550124',
    APOCRYPHA_SMS_OWNER_USER_ID: '11111111-1111-4111-8111-111111111111',
    APOCRYPHA_SMS_SESSION_ID: '22222222-2222-4222-8222-222222222222',
    APOCRYPHA_SMS_BINDING_KEY_BASE64: Buffer.alloc(32, 2).toString('base64'),
    APOCRYPHA_SMS_STORAGE_KEY_ID: '2026-08',
    APOCRYPHA_SMS_STORAGE_KEY_BASE64: Buffer.alloc(32, 3).toString('base64'),
  };
  const valid = readSmsConfiguration(base);
  assert.equal(valid.configured, true);
  if (!valid.configured) throw new Error('expected configured SMS fixture');
  assert.equal(valid.config.binding.ownerNumber, '+16025550124');
  assert.equal(valid.config.policy.maxReplyChars, 320);
  assert.equal(valid.config.policy.maxSegments, 3);
  assert.equal(valid.config.policy.dailySegmentBudget, 30);

  const invalid = readSmsConfiguration({ ...base, APOCRYPHA_SMS_OWNER_E164: '602-555-0124' });
  assert.equal(invalid.configured, false);
  if (invalid.configured) throw new Error('expected invalid SMS fixture');
  assert(invalid.missing.includes('APOCRYPHA_SMS_OWNER_E164'));

  const unsafeBudget = readSmsConfiguration({ ...base, APOCRYPHA_SMS_DAILY_SEGMENT_BUDGET: '0' });
  assert.equal(unsafeBudget.configured, false, 'an invalid explicit spend limit must fail closed');
  if (unsafeBudget.configured) throw new Error('expected invalid SMS budget fixture');
  assert(unsafeBudget.missing.includes('APOCRYPHA_SMS_DAILY_SEGMENT_BUDGET'));

  const reservedKeyId = readSmsConfiguration({ ...base, APOCRYPHA_SMS_STORAGE_KEY_ID: '__proto__' });
  assert.equal(reservedKeyId.configured, false, 'prototype key identifiers must fail closed');
  if (reservedKeyId.configured) throw new Error('expected reserved key id fixture to fail');
  assert(reservedKeyId.missing.includes('APOCRYPHA_SMS_STORAGE_KEY_ID'));

  const foreignWebhook = readSmsConfiguration({
    ...base,
    APOCRYPHA_SMS_WEBHOOK_URL: 'https://attacker.example/api/apocrypha/sms/inbound',
  });
  assert.equal(foreignWebhook.configured, false, 'webhook origins are pinned to Apocky');
  if (foreignWebhook.configured) throw new Error('expected foreign webhook fixture to fail');
  assert(foreignWebhook.missing.includes('APOCRYPHA_SMS_WEBHOOK_URL'));
}

export async function testTwilioOutboundContract(): Promise<void> {
  const state = readSmsConfiguration({
    TWILIO_ACCOUNT_SID: ACCOUNT_SID,
    TWILIO_AUTH_TOKEN: AUTH_TOKEN,
    TWILIO_API_KEY_SID: API_KEY_SID,
    TWILIO_API_KEY_SECRET: 'restricted-api-key-secret',
    APOCRYPHA_SMS_WEBHOOK_URL: 'https://www.apocky.com/api/apocrypha/sms/inbound',
    APOCRYPHA_SMS_STATUS_CALLBACK_URL: 'https://www.apocky.com/api/apocrypha/sms/status',
    APOCRYPHA_SMS_NUMBER_E164: '+16025550123',
    APOCRYPHA_SMS_OWNER_E164: '+16025550124',
    APOCRYPHA_SMS_OWNER_USER_ID: '11111111-1111-4111-8111-111111111111',
    APOCRYPHA_SMS_SESSION_ID: '22222222-2222-4222-8222-222222222222',
    APOCRYPHA_SMS_BINDING_KEY_BASE64: Buffer.alloc(32, 2).toString('base64'),
    APOCRYPHA_SMS_STORAGE_KEY_ID: '2026-08',
    APOCRYPHA_SMS_STORAGE_KEY_BASE64: Buffer.alloc(32, 3).toString('base64'),
  });
  if (!state.configured) throw new Error('expected configured SMS fixture');

  let observedUrl = '';
  let observedInit: RequestInit | undefined;
  const fakeFetch: typeof fetch = async (input, init) => {
    observedUrl = String(input);
    observedInit = init;
    return new Response(JSON.stringify({ sid: MESSAGE_SID, status: 'queued' }), {
      status: 201,
      headers: { 'content-type': 'application/json' },
    });
  };
  const result = await sendTwilioMessage(
    state.config.provider,
    state.config.policy,
    { to: state.config.binding.ownerNumber, text: 'A bounded reply.' },
    fakeFetch,
  );
  assert.equal(result.sid, MESSAGE_SID);
  assert.equal(
    observedUrl,
    `https://api.twilio.com/2010-04-01/Accounts/${ACCOUNT_SID}/Messages.json`,
  );
  assert.equal(observedInit?.method, 'POST');
  const headers = new Headers(observedInit?.headers);
  assert.equal(
    headers.get('authorization'),
    `Basic ${Buffer.from(`${API_KEY_SID}:restricted-api-key-secret`).toString('base64')}`,
  );
  const body = new URLSearchParams(String(observedInit?.body));
  assert.equal(body.get('To'), '+16025550124');
  assert.equal(body.get('From'), '+16025550123');
  assert.equal(body.get('StatusCallback'), state.config.provider.statusCallbackUrl);
  assert.equal(body.get('Body'), 'A bounded reply.');
  await assert.rejects(
    sendTwilioMessage(
      state.config.provider,
      state.config.policy,
      { to: '602-555-0124', text: 'Never send this.' },
      fakeFetch,
    ),
    /twilio_send_invalid/,
  );
}

async function main(): Promise<void> {
  testTwilioSignatureVector();
  testConsentCommands();
  testEncryptedQueuePayload();
  testSmsRendering();
  testConfigurationFailsClosed();
  await testTwilioOutboundContract();
  // eslint-disable-next-line no-console
  console.log('apocrypha-sms.test : OK · 6 contracts passed');
}

void main();

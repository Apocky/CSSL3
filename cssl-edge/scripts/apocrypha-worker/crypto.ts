import { createCipheriv, createDecipheriv, createHash, randomBytes, scryptSync } from 'node:crypto';

export function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

export function stableJson(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  const entries = Object.entries(value as Record<string, unknown>)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`);
  return `{${entries.join(',')}}`;
}

function deriveJournalKey(nodeToken: string, nodeId: string): Buffer {
  return scryptSync(nodeToken, `apocrypha-worker-journal:${nodeId}`, 32);
}

export interface EncryptedEnvelope {
  version: 1;
  algorithm: 'aes-256-gcm';
  iv: string;
  authTag: string;
  ciphertext: string;
}

export function encryptJournal(plaintext: string, nodeToken: string, nodeId: string): EncryptedEnvelope {
  const iv = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', deriveJournalKey(nodeToken, nodeId), iv);
  cipher.setAAD(Buffer.from('apocrypha-attempt-journal-v1', 'utf8'));
  const ciphertext = Buffer.concat([cipher.update(plaintext, 'utf8'), cipher.final()]);
  return {
    version: 1,
    algorithm: 'aes-256-gcm',
    iv: iv.toString('base64'),
    authTag: cipher.getAuthTag().toString('base64'),
    ciphertext: ciphertext.toString('base64'),
  };
}

export function decryptJournal(envelope: EncryptedEnvelope, nodeToken: string, nodeId: string): string {
  if (envelope.version !== 1 || envelope.algorithm !== 'aes-256-gcm') {
    throw new Error('unsupported journal encryption envelope');
  }
  const decipher = createDecipheriv(
    'aes-256-gcm',
    deriveJournalKey(nodeToken, nodeId),
    Buffer.from(envelope.iv, 'base64'),
  );
  decipher.setAAD(Buffer.from('apocrypha-attempt-journal-v1', 'utf8'));
  decipher.setAuthTag(Buffer.from(envelope.authTag, 'base64'));
  const plaintext = Buffer.concat([
    decipher.update(Buffer.from(envelope.ciphertext, 'base64')),
    decipher.final(),
  ]);
  return plaintext.toString('utf8');
}

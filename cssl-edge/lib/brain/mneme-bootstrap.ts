import {
  createHmac,
  createPrivateKey,
  createPublicKey,
  timingSafeEqual,
} from 'node:crypto';

const ED25519_PKCS8_SEED_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex');
const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');
const HEX_32 = /^[0-9a-f]{64}$/iu;

export type MnemeOwnerKeySource = 'configured_owner_key' | 'server_derived_owner_binding';

export class MnemeBootstrapError extends Error {
  constructor(
    readonly code:
      | 'BRAIN_MNEME_BINDING_UNAVAILABLE'
      | 'BRAIN_MNEME_PROFILE_BINDING_MISMATCH',
    readonly publicStatus: 409 | 503,
  ) {
    super(code);
    this.name = 'MnemeBootstrapError';
  }
}

function validBindingSecret(value: string | undefined): value is string {
  if (!value || value !== value.trim()) return false;
  const bytes = Buffer.from(value, 'utf8');
  return bytes.length >= 32
    && bytes.length <= 8_192
    && [...bytes].every(byte => byte >= 0x21 && byte <= 0x7e);
}

/**
 * Builds a real Ed25519 public key for the verified owner identity. The seed is
 * domain-separated from every other server binding and is never persisted or
 * returned. An explicitly configured owner public key remains authoritative.
 */
export function deriveMnemeOwnerPublicKey(input: {
  readonly userId: string;
  readonly configuredPublicKeyHex?: string;
  readonly bindingSecret?: string;
}): { publicKey: Uint8Array; source: MnemeOwnerKeySource } {
  const configured = input.configuredPublicKeyHex;
  if (configured !== undefined) {
    if (!HEX_32.test(configured)) {
      throw new MnemeBootstrapError('BRAIN_MNEME_BINDING_UNAVAILABLE', 503);
    }
    return {
      publicKey: new Uint8Array(Buffer.from(configured, 'hex')),
      source: 'configured_owner_key',
    };
  }
  if (!input.userId || input.userId.length > 512 || !validBindingSecret(input.bindingSecret)) {
    throw new MnemeBootstrapError('BRAIN_MNEME_BINDING_UNAVAILABLE', 503);
  }
  const seed = createHmac('sha256', Buffer.from(input.bindingSecret, 'utf8'))
    .update('apocky.mneme.owner-ed25519-seed.v1\0', 'utf8')
    .update(input.userId, 'utf8')
    .digest();
  const privateKey = createPrivateKey({
    key: Buffer.concat([ED25519_PKCS8_SEED_PREFIX, seed]),
    format: 'der',
    type: 'pkcs8',
  });
  const encodedPublicKey = createPublicKey(privateKey).export({ format: 'der', type: 'spki' });
  const prefix = encodedPublicKey.subarray(0, ED25519_SPKI_PREFIX.length);
  const rawPublicKey = encodedPublicKey.subarray(ED25519_SPKI_PREFIX.length);
  if (
    rawPublicKey.length !== 32
    || prefix.length !== ED25519_SPKI_PREFIX.length
    || !timingSafeEqual(prefix, ED25519_SPKI_PREFIX)
  ) {
    throw new MnemeBootstrapError('BRAIN_MNEME_BINDING_UNAVAILABLE', 503);
  }
  return {
    publicKey: new Uint8Array(rawPublicKey),
    source: 'server_derived_owner_binding',
  };
}

export function mnemeOwnerPublicKeyMatches(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length
    && left.length === 32
    && timingSafeEqual(Buffer.from(left), Buffer.from(right));
}

import {
  createHash,
  createPublicKey,
  verify as cryptoVerify,
} from 'node:crypto';
import { readFile } from 'node:fs/promises';
import process from 'node:process';

export const DETACHED_SIGNATURE_ALGORITHM = 'Ed25519';
export const DETACHED_SIGNATURE_ENCODING = 'base64url';

const SIGNATURE_BYTES = 64;
const SHA256 = /^[0-9a-f]{64}$/;

function asBytes(value) {
  if (value instanceof Uint8Array) return Buffer.from(value);
  if (Buffer.isBuffer(value)) return value;
  throw new TypeError('artifact_bytes must be a Uint8Array.');
}

function artifactDigest(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/**
 * Decode the detached signature carrier without accepting alternate encodings.
 * The release contract signs the exact archive bytes and stores the 64-byte
 * Ed25519 signature as unpadded base64url text.
 */
export function decodeDetachedSignature(value) {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!/^[A-Za-z0-9_-]{86}$/.test(text)) return null;
  const decoded = Buffer.from(text, 'base64url');
  if (decoded.length !== SIGNATURE_BYTES || decoded.toString('base64url') !== text) return null;
  return decoded;
}

function publicKey(input) {
  if (typeof input !== 'string' || input.length === 0) return null;
  try {
    const key = createPublicKey(input);
    if (key.type !== 'public' || key.asymmetricKeyType !== 'ed25519') return null;
    return key;
  } catch {
    return null;
  }
}

/** Return the SHA-256 fingerprint of a PEM SPKI Ed25519 public key. */
export function publicKeyFingerprint(public_key_pem) {
  const key = publicKey(public_key_pem);
  if (!key) return null;
  try {
    return createHash('sha256').update(key.export({ type: 'spki', format: 'der' })).digest('hex');
  } catch {
    return null;
  }
}

/**
 * Verify an externally-produced detached Ed25519 signature.
 *
 * This function intentionally has no signing path and never accepts a private
 * key. `artifact_bytes` are the exact bytes that the publisher signed. The
 * optional expected digest binds the verification to the promotion manifest.
 */
export function verifyDetachedSignature({
  artifact_bytes,
  detached_signature,
  public_key_pem,
  expected_sha256,
}) {
  let bytes;
  try {
    bytes = asBytes(artifact_bytes);
  } catch {
    return { ok: false, code: 'ARTIFACT_BYTES_INVALID' };
  }

  const artifact_sha256 = artifactDigest(bytes);
  if (expected_sha256 !== undefined && (!SHA256.test(expected_sha256) || expected_sha256 !== artifact_sha256)) {
    return { ok: false, code: 'ARTIFACT_HASH_MISMATCH', artifact_sha256 };
  }

  const signature = decodeDetachedSignature(detached_signature);
  if (!signature) return { ok: false, code: 'SIGNATURE_ENCODING_INVALID', artifact_sha256 };
  const key = publicKey(public_key_pem);
  if (!key) return { ok: false, code: 'PUBLIC_KEY_INVALID', artifact_sha256 };
  const signer_public_key_fingerprint = publicKeyFingerprint(public_key_pem);

  let valid = false;
  try {
    valid = cryptoVerify(null, bytes, key, signature);
  } catch {
    valid = false;
  }
  return valid
    ? { ok: true, algorithm: DETACHED_SIGNATURE_ALGORITHM, artifact_sha256, signer_public_key_fingerprint }
    : { ok: false, code: 'SIGNATURE_INVALID', artifact_sha256 };
}

function argumentMap(argv) {
  const result = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const item = argv[index];
    if (!item.startsWith('--')) throw new Error(`Unexpected argument: ${item}`);
    const key = item.slice(2);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for --${key}`);
    result.set(key, value);
    index += 1;
  }
  return result;
}

async function main() {
  const args = argumentMap(process.argv.slice(2));
  const artifactPath = args.get('artifact');
  const signaturePath = args.get('signature');
  const publicKeyPath = args.get('public-key');
  if (!artifactPath || !signaturePath || !publicKeyPath) {
    throw new Error('Usage: node verify-detached-signature.mjs --artifact FILE --signature FILE --public-key FILE [--expected-sha256 HEX]');
  }
  const [artifactBytes, signature, publicKeyPem] = await Promise.all([
    readFile(artifactPath),
    readFile(signaturePath, 'utf8'),
    readFile(publicKeyPath, 'utf8'),
  ]);
  const result = verifyDetachedSignature({
    artifact_bytes: artifactBytes,
    detached_signature: signature,
    public_key_pem: publicKeyPem,
    expected_sha256: args.get('expected-sha256'),
  });
  process.stdout.write(`${JSON.stringify(result)}\n`);
  if (!result.ok) process.exitCode = 1;
}

if (import.meta.url === new URL(`file://${process.argv[1].replaceAll('\\', '/')}`).href) {
  await main();
}

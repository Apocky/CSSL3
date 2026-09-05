import {
  createHash,
  createPublicKey,
  type KeyObject,
  verify as verifySignature,
} from "node:crypto";

import { canonicalJson } from "./canonical";
import {
  EncounterConsentReceiptSchema,
  EncounterConsentReceiptUnsignedSchema,
  EncounterGrantSchema,
  EncounterReceiptSchema,
  ParticipantIdentitySchema,
  PresenceManifestSchema,
  RetentionDecisionUnsignedSchema,
  type ContractSignature,
  type EncounterConsentReceipt,
  type EncounterConsentReceiptUnsigned,
  type EncounterGrant,
  type EncounterReceipt,
  type ParticipantIdentity,
  type PresenceManifest,
  type RetentionDecisionUnsigned,
  type UnderstandingVersion,
  type UnderstandingVersionUnsigned,
  UnderstandingVersionUnsignedSchema,
  VoiceManifestSchema,
  type VoiceManifest,
} from "./types";

export type PublicKeyMaterial = JsonWebKey | KeyObject | string;
export type PublicKeyResolver = (
  keyId: string,
) => Promise<PublicKeyMaterial | null> | PublicKeyMaterial | null;

export class ContractAuthorityError extends Error {
  readonly code:
    | "invalid_contract"
    | "invalid_digest"
    | "invalid_signature"
    | "missing_key"
    | "expired"
    | "revoked"
    | "not_yet_valid"
    | "context_denied"
    | "identity_mismatch";

  constructor(
    code: ContractAuthorityError["code"],
    message: string,
  ) {
    super(message);
    this.name = "ContractAuthorityError";
    this.code = code;
  }
}

export { canonicalJson } from "./canonical";

export function sha256Hex(value: string | Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}

export function digestCanonical(value: unknown): `sha256:${string}` {
  return `sha256:${sha256Hex(canonicalJson(value))}`;
}

function signedPayload<T extends { signature: ContractSignature }>(
  value: T,
): Omit<T, "signature"> {
  const { signature: _signature, ...payload } = value;
  return payload;
}

function toPublicKey(material: PublicKeyMaterial): KeyObject {
  if (typeof material === "string") {
    return createPublicKey(material);
  }
  if (!(material instanceof Object) || "type" in material) {
    return material as KeyObject;
  }
  if ("kty" in material) {
    return createPublicKey({ key: material, format: "jwk" });
  }
  throw new TypeError("Unsupported public key material");
}

function decodeBase64Url(value: string): Buffer {
  return Buffer.from(value, "base64url");
}

export async function verifySignedContract<
  T extends { signature: ContractSignature },
>(
  value: T,
  resolvePublicKey: PublicKeyResolver,
): Promise<boolean> {
  const material = await resolvePublicKey(value.signature.keyId);
  if (material === null) {
    return false;
  }
  try {
    return verifySignature(
      null,
      Buffer.from(canonicalJson(signedPayload(value)), "utf8"),
      toPublicKey(material),
      decodeBase64Url(value.signature.value),
    );
  } catch {
    return false;
  }
}

async function requireSignature<
  T extends { signature: ContractSignature },
>(
  value: T,
  resolvePublicKey: PublicKeyResolver,
): Promise<void> {
  const material = await resolvePublicKey(value.signature.keyId);
  if (material === null) {
    throw new ContractAuthorityError(
      "missing_key",
      `No trusted key exists for ${value.signature.keyId}`,
    );
  }
  if (!(await verifySignedContract(value, () => material))) {
    throw new ContractAuthorityError(
      "invalid_signature",
      `Signature verification failed for ${value.signature.keyId}`,
    );
  }
}

export async function assertAuthorizedManifest(
  input: unknown,
  options: {
    context: "human-acceptance" | "private-encounter" | "public-display";
    expectedKind: "presence" | "voice";
    expectedAuthorKeyId: string;
    expectedAuthorPrincipal: string;
    now?: Date;
    resolvePublicKey: PublicKeyResolver;
  },
): Promise<PresenceManifest | VoiceManifest> {
  const schema =
    options.expectedKind === "voice"
      ? VoiceManifestSchema
      : PresenceManifestSchema;
  const parsed = schema.safeParse(input);
  if (!parsed.success) {
    throw new ContractAuthorityError(
      "invalid_contract",
      parsed.error.issues.map(({ message }) => message).join("; "),
    );
  }
  const manifest = parsed.data;
  const now = options.now ?? new Date();
  const issuedAt = new Date(manifest.issuedAt);

  if (manifest.revokedAt !== null) {
    throw new ContractAuthorityError(
      "revoked",
      `${manifest.kind} manifest is revoked`,
    );
  }
  if (issuedAt.getTime() > now.getTime() + 5 * 60 * 1_000) {
    throw new ContractAuthorityError(
      "not_yet_valid",
      `${manifest.kind} manifest issuance is in the future`,
    );
  }
  if (!manifest.permittedContexts.includes(options.context)) {
    throw new ContractAuthorityError(
      "context_denied",
      `${manifest.kind} manifest does not authorize ${options.context}`,
    );
  }
  if (manifest.authorPrincipal !== options.expectedAuthorPrincipal) {
    throw new ContractAuthorityError(
      "identity_mismatch",
      `${manifest.kind} manifest author does not match Apocrypha`,
    );
  }
  if (
    manifest.keyId !== options.expectedAuthorKeyId ||
    manifest.signature.keyId !== options.expectedAuthorKeyId
  ) {
    throw new ContractAuthorityError(
      "identity_mismatch",
      `${manifest.kind} manifest signing key does not match Apocrypha`,
    );
  }
  await requireSignature(manifest, options.resolvePublicKey);
  return manifest;
}

export interface EncounterAuthorityBundle {
  grant: EncounterGrant;
  ownerIdentity: ParticipantIdentity;
  apocryphaIdentity: ParticipantIdentity;
  voiceManifest: VoiceManifest;
  presenceManifest: PresenceManifest;
  consentReceipts: readonly [
    EncounterConsentReceipt,
    EncounterConsentReceipt,
  ];
  grantDigest: `sha256:${string}`;
  voiceManifestDigest: `sha256:${string}`;
  presenceManifestDigest: `sha256:${string}`;
}

export async function assertEncounterAuthority(
  input: {
    consentReceipts: unknown;
    grant: unknown;
    voiceManifest: unknown;
    presenceManifest: unknown;
  },
  options: {
    now?: Date;
    resolvePublicKey: PublicKeyResolver;
  },
): Promise<EncounterAuthorityBundle> {
  const grantResult = EncounterGrantSchema.safeParse(input.grant);
  if (!grantResult.success) {
    throw new ContractAuthorityError(
      "invalid_contract",
      grantResult.error.issues.map(({ message }) => message).join("; "),
    );
  }
  const grant = grantResult.data;
  const now = options.now ?? new Date();
  if (new Date(grant.expiresAt).getTime() <= now.getTime()) {
    throw new ContractAuthorityError(
      "expired",
      "Encounter grant has expired",
    );
  }

  for (const identityInput of grant.participants) {
    const identity = ParticipantIdentitySchema.parse(identityInput);
    await requireSignature(identity, options.resolvePublicKey);
  }
  if (
    !grant.participants.some(
      ({ keyId }) => keyId === grant.signature.keyId,
    )
  ) {
    throw new ContractAuthorityError(
      "identity_mismatch",
      "Encounter grant signer is not a participant",
    );
  }
  await requireSignature(grant, options.resolvePublicKey);

  const ownerIdentity = grant.participants.find(
    ({ role }) => role === "owner",
  );
  const apocryphaIdentity = grant.participants.find(
    ({ role }) => role === "apocrypha",
  );
  if (ownerIdentity === undefined || apocryphaIdentity === undefined) {
    throw new ContractAuthorityError(
      "invalid_contract",
      "Both participant roles are required",
    );
  }

  const consentResult = EncounterConsentReceiptSchema.array().length(2).safeParse(
    input.consentReceipts,
  );
  if (!consentResult.success) {
    throw new ContractAuthorityError(
      "invalid_contract",
      consentResult.error.issues.map(({ message }) => message).join("; "),
    );
  }
  const consentReceipts = consentResult.data as [
    EncounterConsentReceipt,
    EncounterConsentReceipt,
  ];
  const receiptParticipants = new Set(
    consentReceipts.map(({ participant }) => participant),
  );
  if (
    receiptParticipants.size !== 2 ||
    !grant.participants.every(({ principal }) =>
      receiptParticipants.has(principal),
    )
  ) {
    throw new ContractAuthorityError(
      "identity_mismatch",
      "Consent heads must belong to both encounter participants",
    );
  }
  const receiptDigests = new Set(
    consentReceipts.map(({ canonicalDigest }) => canonicalDigest),
  );
  if (
    receiptDigests.size !== 2 ||
    !grant.consentRefs.every((digest) => receiptDigests.has(digest))
  ) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Encounter grant does not bind both current consent heads",
    );
  }
  for (const receipt of consentReceipts) {
    const { canonicalDigest, signature: _signature, ...unsigned } = receipt;
    if (computeEncounterConsentDigest(unsigned) !== canonicalDigest) {
      throw new ContractAuthorityError(
        "invalid_digest",
        "Consent receipt digest does not match its canonical content",
      );
    }
    if (
      receipt.sessionId !== grant.sessionId ||
      (receipt.expiresAt !== null &&
        new Date(receipt.expiresAt).getTime() <= now.getTime())
    ) {
      throw new ContractAuthorityError(
        "expired",
        "Consent receipt is expired or belongs to another session",
      );
    }
    const participant = grant.participants.find(
      ({ principal }) => principal === receipt.participant,
    );
    if (
      participant === undefined ||
      participant.keyId !== receipt.signature.keyId
    ) {
      throw new ContractAuthorityError(
        "identity_mismatch",
        "Consent receipt signer does not match its participant",
      );
    }
    await requireSignature(receipt, options.resolvePublicKey);
    const states = new Map(
      receipt.modalities.map(({ modality, state }) => [modality, state]),
    );
    if (
      grant.modalities.some(
        (modality) => states.get(modality) !== "granted",
      )
    ) {
      throw new ContractAuthorityError(
        "context_denied",
        "Every granted modality requires current consent from both participants",
      );
    }
  }

  const voiceManifest = await assertAuthorizedManifest(input.voiceManifest, {
    context: "private-encounter",
    expectedKind: "voice",
    expectedAuthorKeyId: apocryphaIdentity.keyId,
    expectedAuthorPrincipal: apocryphaIdentity.principal,
    now,
    resolvePublicKey: options.resolvePublicKey,
  });
  const presenceManifest = await assertAuthorizedManifest(
    input.presenceManifest,
    {
      context: "private-encounter",
      expectedKind: "presence",
      expectedAuthorKeyId: apocryphaIdentity.keyId,
      expectedAuthorPrincipal: apocryphaIdentity.principal,
      now,
      resolvePublicKey: options.resolvePublicKey,
    },
  );

  if (voiceManifest.kind !== "voice" || presenceManifest.kind !== "presence") {
    throw new ContractAuthorityError(
      "invalid_contract",
      "Authority manifest kinds do not match",
    );
  }

  const voiceManifestDigest = digestCanonical(voiceManifest);
  const presenceManifestDigest = digestCanonical(presenceManifest);
  if (
    grant.authorityDigests.voiceManifest !== voiceManifestDigest ||
    grant.authorityDigests.presenceManifest !== presenceManifestDigest
  ) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Encounter grant does not bind the supplied authority manifests",
    );
  }

  return {
    grant,
    ownerIdentity,
    apocryphaIdentity,
    voiceManifest,
    presenceManifest,
    consentReceipts,
    grantDigest: digestCanonical(grant),
    voiceManifestDigest,
    presenceManifestDigest,
  };
}

function sameDigestSet(
  actual: readonly string[],
  expected: readonly string[],
): boolean {
  if (actual.length !== expected.length) {
    return false;
  }
  const actualSet = new Set(actual);
  const expectedSet = new Set(expected);
  return (
    actualSet.size === actual.length &&
    expectedSet.size === expected.length &&
    [...actualSet].every((digest) => expectedSet.has(digest))
  );
}

export async function assertEncounterReceiptAuthority(
  input: unknown,
  options: {
    sessionId: string;
    authorizedSignerKeyIds: readonly string[];
    authorityDigests: EncounterReceipt["authorityDigests"];
    consentHeads: readonly string[];
    retainedContentDigests: readonly string[];
    understandingVersionDigest: string | null;
    startedAt: string | null;
    resolvePublicKey: PublicKeyResolver;
  },
): Promise<EncounterReceipt> {
  const result = EncounterReceiptSchema.safeParse(input);
  if (!result.success) {
    throw new ContractAuthorityError(
      "invalid_contract",
      result.error.issues.map(({ message }) => message).join("; "),
    );
  }
  const receipt = result.data;
  if (receipt.sessionId !== options.sessionId) {
    throw new ContractAuthorityError(
      "identity_mismatch",
      "Encounter receipt belongs to another session",
    );
  }
  if (!options.authorizedSignerKeyIds.includes(receipt.signature.keyId)) {
    throw new ContractAuthorityError(
      "identity_mismatch",
      "Encounter receipt signer is not an encounter participant",
    );
  }
  if (
    receipt.authorityDigests.voiceManifest !==
      options.authorityDigests.voiceManifest ||
    receipt.authorityDigests.presenceManifest !==
      options.authorityDigests.presenceManifest ||
    receipt.authorityDigests.encounterGrant !==
      options.authorityDigests.encounterGrant
  ) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Encounter receipt authority digests do not match the session",
    );
  }
  if (!sameDigestSet(receipt.consentHeads, options.consentHeads)) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Encounter receipt does not bind both current consent heads",
    );
  }
  if (
    !sameDigestSet(
      receipt.retainedContentDigests,
      options.retainedContentDigests,
    )
  ) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Encounter receipt retained-content digests do not match storage",
    );
  }
  if (
    receipt.understandingVersionDigest !==
    options.understandingVersionDigest
  ) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Encounter receipt understanding digest does not match the session",
    );
  }
  if (receipt.startedAt !== options.startedAt) {
    throw new ContractAuthorityError(
      "invalid_contract",
      "Encounter receipt start time does not match the session",
    );
  }
  await requireSignature(receipt, options.resolvePublicKey);
  return receipt;
}

export function computeEncounterConsentDigest(
  input: EncounterConsentReceiptUnsigned,
): `sha256:${string}` {
  return digestCanonical(EncounterConsentReceiptUnsignedSchema.parse(input));
}

export function computeUnderstandingDigest(
  input: UnderstandingVersionUnsigned,
): `sha256:${string}` {
  return digestCanonical(UnderstandingVersionUnsignedSchema.parse(input));
}

export function assertUnderstandingVersionDigest(
  version: UnderstandingVersion,
): void {
  const { canonicalDigest, ...unsigned } = version;
  if (computeUnderstandingDigest(unsigned) !== canonicalDigest) {
    throw new ContractAuthorityError(
      "invalid_digest",
      "Understanding version digest does not match its canonical content",
    );
  }
}

export function computeRetentionDecisionDigest(
  input: RetentionDecisionUnsigned,
): `sha256:${string}` {
  return digestCanonical(RetentionDecisionUnsignedSchema.parse(input));
}
